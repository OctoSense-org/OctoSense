//! What the first calls of a process cost against the next ones, through App
//! Hub's dispatch as the shell calls them. The deck and word services pay a
//! first-call cost for their engines' process-wide font databases and warm
//! them up at registration; this measures whether pdfcraft has such a cost.
//! The scenario runs in a fresh process: this test binary again, with one
//! ignored test and the scenario's folder in `FIRST_CALL_DIR`. The timings
//! are printed; the checks are about answers, never speed.
//!
//! cargo test --locked -p octosense-pdf-service --test first_call -- --nocapture

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use octosense_appstore::services::{dispatch, take_replies_for, ServiceCall, ServiceHost};
use serde_json::{json, Value};

const DIR: &str = "FIRST_CALL_DIR";
static NEXT: AtomicUsize = AtomicUsize::new(49_000);

struct Host;
impl ServiceHost for Host {
    fn open_sheet(&mut self, _body: String) {}
    fn close_sheet(&mut self) {}
}

fn ms(since: Instant) -> f64 {
    (since.elapsed().as_secs_f64() * 1e4).round() / 10.0
}

/// One call as the shell makes it, and how long it took to answer.
fn call(dir: &Path, method: &str, args: Value) -> (Value, f64) {
    let heap = NEXT.fetch_add(1, Ordering::Relaxed);
    let call = ServiceCall { app_id: "os.pdftools".into(), service: format!("pdf.{method}"), args, from_sheet: false, may_prompt: true, host_dir: dir.to_path_buf() };
    let started = Instant::now();
    dispatch(call, heap, 1, &mut Host);
    let reply = loop {
        if let Some((_, _, reply)) = take_replies_for(&[heap]).pop() {
            break reply;
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let took = ms(started);
    (serde_json::from_str(&reply.unwrap_or_else(|e| panic!("pdf.{method}: {e}"))).unwrap(), took)
}

/// A two-page PDF whose text uses Helvetica without embedding it, so drawing
/// it needs a font from outside the file.
fn two_pages() -> Vec<u8> {
    let content = |text: &str| {
        let s = format!("BT /F1 12 Tf 20 50 Td ({text}) Tj ET");
        format!("<< /Length {} >>\nstream\n{s}\nendstream", s.len())
    };
    let page = |contents: u32| format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Resources << /Font << /F1 7 0 R >> >> /Contents {contents} 0 R >>");
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".to_string(),
        page(4),
        content("Northern Lights"),
        page(6),
        content("The science"),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objs.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n{body}\nendobj\n", i + 1).bytes());
    }
    let xref = pdf.len();
    pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
    for off in &offsets {
        pdf.extend(format!("{off:010} 00000 n \n").bytes());
    }
    pdf.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF", objs.len() + 1).bytes());
    pdf
}

fn scenario(dir: &Path) -> Value {
    octosense_pdf_service::register();
    std::fs::create_dir_all(dir.join("pdf")).unwrap();
    std::fs::write(dir.join("pdf/t.pdf"), two_pages()).unwrap();
    let (info, info_ms) = call(dir, "info", json!({"path": "t.pdf"}));
    assert_eq!(info["document"]["pages"], json!(2), "{info}");
    let (_, info_again_ms) = call(dir, "info", json!({"path": "t.pdf"}));
    let (page, render_ms) = call(dir, "render", json!({"path": "t.pdf", "page": 1, "out": "first.png", "max_side": 480}));
    assert_eq!(page["width"], json!(480), "{page}");
    let (_, render_two_ms) = call(dir, "render", json!({"path": "t.pdf", "page": 2, "out": "second.png", "max_side": 480}));
    let (_, render_again_ms) = call(dir, "render", json!({"path": "t.pdf", "page": 1, "out": "again.png", "max_side": 480}));
    let (text, text_ms) = call(dir, "text", json!({"path": "t.pdf"}));
    assert!(text.to_string().contains("Northern Lights"), "{text}");
    let (_, text_again_ms) = call(dir, "text", json!({"path": "t.pdf"}));
    json!({"kind": "cold", "first_info_ms": info_ms, "info_again_ms": info_again_ms, "first_render_ms": render_ms,
           "second_render_ms": render_two_ms, "render_again_ms": render_again_ms, "first_text_ms": text_ms, "text_again_ms": text_again_ms})
}

#[test]
#[ignore = "run by first_call_against_warm_call, in a process of its own"]
fn scenario_cold() {
    if let Some(dir) = std::env::var_os(DIR) {
        println!("FIRST_CALL {}", scenario(Path::new(&dir)));
    }
}

#[test]
fn first_call_against_warm_call() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "scenario_cold", "--ignored", "--nocapture", "--test-threads=1"])
        .env(DIR, dir.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    // libtest prints the test's own output after its "test … " line.
    let line = stdout.lines().find_map(|l| l.split_once("FIRST_CALL ").map(|(_, timings)| timings)).unwrap_or_else(|| panic!("no timings in {stdout}"));
    eprintln!("pdf {line}");
    assert!(std::fs::read(dir.path().join("pdf/first.png")).unwrap().starts_with(&[0x89, b'P', b'N', b'G']));
}
