//! What the first call of a process that lays out pages (`info`, `convert` to
//! PNG or PDF) costs against the next ones, through App Hub's dispatch as the
//! shell calls them, and what the warm-up `register()` starts changes. The
//! engine's font database is process-wide, so each scenario runs in a fresh
//! process: this test binary again, with one ignored test and the scenario's
//! folder in `FIRST_CALL_DIR`. The timings are printed; the checks are about
//! answers, never speed.
//!
//! cargo test --locked -p octosense-word-service --test first_call -- --nocapture

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use octosense_appstore::services::{dispatch, register_host_service, take_replies_for, ServiceCall, ServiceHost};
use octosense_word_service::{register, warm, WordService};
use serde_json::{json, Value};

const DIR: &str = "FIRST_CALL_DIR";
static NEXT: AtomicUsize = AtomicUsize::new(48_000);

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
    let call = ServiceCall { app_id: "os.writer".into(), service: format!("word.{method}"), args, from_sheet: false, may_prompt: true, host_dir: dir.to_path_buf() };
    let started = Instant::now();
    dispatch(call, heap, 1, &mut Host);
    let reply = loop {
        if let Some((_, _, reply)) = take_replies_for(&[heap]).pop() {
            break reply;
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let took = ms(started);
    (serde_json::from_str(&reply.unwrap_or_else(|e| panic!("word.{method}: {e}"))).unwrap(), took)
}

/// One scenario, in this process: `cold` registers the service alone, as it
/// was before the warm-up; `warmed` lets the warm-up finish first; `during`
/// calls while it runs; `first` calls before any warm-up, then warms.
fn scenario(kind: &str, dir: &Path) -> Value {
    let mut warm_ms = Value::Null;
    let mut warming = None;
    if kind == "cold" || kind == "first" {
        register_host_service(Box::new(WordService));
    } else {
        let started = Instant::now();
        warming = warm();
        assert!(warming.is_some(), "the first warm() starts the warm-up");
        register();
        assert!(warm().is_none(), "once per process: register() and warm() start no second one");
        if kind == "warmed" {
            warming.take().unwrap().join().unwrap();
            warm_ms = json!(ms(started));
        }
    }
    let text = "Northern Lights\nA short tour of the aurora, where it comes from and how to photograph it.";
    let (made, new_ms) = call(dir, "new", json!({"out": "t.docx", "text": text, "title": "Northern Lights"}));
    assert!(made["words"].as_u64().unwrap() > 0, "{made}");
    let (info, first_ms) = call(dir, "info", json!({"path": "t.docx"}));
    assert_eq!(info["pages"], json!(1), "{info}");
    let (_, again_ms) = call(dir, "info", json!({"path": "t.docx"}));
    let (_, png_ms) = call(dir, "convert", json!({"path": "t.docx", "out": "first.png"}));
    let (_, png_again_ms) = call(dir, "convert", json!({"path": "t.docx", "out": "again.png"}));
    let (_, pdf_ms) = call(dir, "convert", json!({"path": "t.docx", "out": "t.pdf"}));
    let (_, pdf_again_ms) = call(dir, "convert", json!({"path": "t.docx", "out": "again.pdf"}));
    if let Some(thread) = warming {
        thread.join().unwrap();
    }
    if kind == "first" {
        // The warm-up after the calls finds the fonts loaded, and changes no answer.
        let started = Instant::now();
        warm().expect("the first warm() starts the warm-up").join().unwrap();
        warm_ms = json!(ms(started));
        assert!(warm().is_none(), "once per process");
        let (_, after_ms) = call(dir, "convert", json!({"path": "t.docx", "out": "after.png"}));
        assert_eq!(std::fs::read(dir.join("word/after.png")).unwrap(), std::fs::read(dir.join("word/first.png")).unwrap());
        return json!({"kind": kind, "warm_up_ms": warm_ms, "first_info_ms": first_ms, "info_again_ms": again_ms, "png_after_warm_up_ms": after_ms});
    }
    json!({"kind": kind, "warm_up_ms": warm_ms, "new_ms": new_ms, "first_info_ms": first_ms, "info_again_ms": again_ms,
           "first_png_ms": png_ms, "png_again_ms": png_again_ms, "first_pdf_ms": pdf_ms, "pdf_again_ms": pdf_again_ms})
}

fn run(kind: &str) {
    if let Some(dir) = std::env::var_os(DIR) {
        println!("FIRST_CALL {}", scenario(kind, Path::new(&dir)));
    }
}

#[test]
#[ignore = "run by first_call_against_warm_call, in a process of its own"]
fn scenario_cold() {
    run("cold");
}

#[test]
#[ignore = "run by first_call_against_warm_call, in a process of its own"]
fn scenario_warmed() {
    run("warmed");
}

#[test]
#[ignore = "run by first_call_against_warm_call, in a process of its own"]
fn scenario_during() {
    run("during");
}

#[test]
#[ignore = "run by first_call_against_warm_call, in a process of its own"]
fn scenario_first() {
    run("first");
}

#[test]
fn first_call_against_warm_call() {
    let mut runs = Vec::new();
    for kind in ["cold", "warmed", "during", "first"] {
        let dir = tempfile::tempdir().unwrap();
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &format!("scenario_{kind}"), "--ignored", "--nocapture", "--test-threads=1"])
            .env(DIR, dir.path())
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{kind}: {stdout}\n{}", String::from_utf8_lossy(&out.stderr));
        // libtest prints the test's own output after its "test … " line.
        let line = stdout.lines().find_map(|l| l.split_once("FIRST_CALL ").map(|(_, timings)| timings)).unwrap_or_else(|| panic!("{kind}: no timings in {stdout}"));
        eprintln!("word {line}");
        runs.push((serde_json::from_str::<Value>(line).unwrap(), dir));
    }
    // Warmed or not, called while warming or before it, the service answers
    // alike: the page drawn byte for byte the same.
    let png = |dir: &tempfile::TempDir| std::fs::read(dir.path().join("word/first.png")).unwrap();
    let cold = png(&runs[0].1);
    assert!(cold.starts_with(&[0x89, b'P', b'N', b'G']));
    for (timings, dir) in &runs[1..] {
        assert!(png(dir) == cold, "{}: the page differs from the cold run's", timings["kind"]);
    }
}
