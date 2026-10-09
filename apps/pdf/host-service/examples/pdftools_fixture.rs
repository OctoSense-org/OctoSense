//! A developer fixture for PDF Tools (`apps/pdftools`), recorded from this
//! service.
//!
//! App Hub's `card-host` serves no host services, so PDF Tools cannot reach
//! the `pdf` engine there. This example writes four sample PDFs, runs this
//! service on them exactly as the shell would (App Hub's own dispatcher,
//! `octosense_appstore::services::dispatch`), and saves what it answered:
//! `info`, `text` and every page rendered at the app's two sizes, one merge
//! and two splits. It writes a card-host app-data folder:
//!
//! ```text
//! <app-data>/os.pdftools/accounts/device/library/*.pdf   the sample PDFs
//! <app-data>/os.pdftools/dev/engine-replay.json          the recorded answers
//! <app-data>/os.pdftools/dev/replay/**.png               the recorded page renders
//! ```
//!
//! PDF Tools replays those answers only when `dev/engine-replay.json` is in
//! its own storage and the real engine refuses it or is missing; the shells
//! never create `dev/`. See `apps/pdftools/README.md`.
//!
//! ```sh
//! cargo run --locked -p octosense-pdf-service --example pdftools_fixture -- <new app-data dir>
//! card-host --bundle apps/pdftools/bundle --system --app-data <new app-data dir>
//! ```

use std::path::{Path, PathBuf};

use octosense_appstore::services::{dispatch, take_replies_for, ServiceCall, ServiceHost};
use serde_json::{json, Map, Value};

/// The app the calls are made for.
const APP: &str = "os.pdftools";
/// Where PDF Tools keeps its PDFs, relative to its storage (ADR 0004 §11).
const LIBRARY: &str = "accounts/device/library";
/// Where the recorded renders go, relative to its storage.
const REPLAY: &str = "dev/replay";
/// The two render sizes PDF Tools asks for (`THUMB_SIDE`, `VIEW_SIDE` in
/// its main.splash).
const THUMB_SIDE: u64 = 360;
const VIEW_SIDE: u64 = 1400;
/// Any isolate key: the calls are answered inline.
const HEAP: usize = 1;
/// A file in the library that is not a PDF (a failed download).
const DAMAGED: &str = "Damaged scan.pdf";
const DAMAGED_BYTES: &[u8] = b"%PDF-1.7\n% this download stopped after a few bytes\n";

fn main() {
    let Some(app_data) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: pdftools_fixture <new card-host app-data dir>");
        std::process::exit(2);
    };
    let jail = app_data.join(APP);
    if jail.exists() && std::fs::read_dir(&jail).map(|mut d| d.next().is_some()).unwrap_or(true) {
        eprintln!("{} already holds data; give a new or empty app-data folder", jail.display());
        std::process::exit(2);
    }
    if let Err(e) = record(&jail) {
        eprintln!("pdftools_fixture: {e}");
        std::process::exit(1);
    }
    println!("Wrote the PDF Tools fixture to {}", jail.display());
    println!("Run it: card-host --bundle apps/pdftools/bundle --system --app-data {}", app_data.display());
}

/// The sample documents: file name, replay folder, pages.
fn documents() -> Vec<(&'static str, &'static str, Vec<u8>)> {
    vec![
        ("Quarterly report.pdf", "quarterly-report", quarterly_report()),
        ("Board minutes.pdf", "board-minutes", board_minutes()),
        ("Field guide.pdf", "field-guide", field_guide()),
        ("Apartment lease.pdf", "apartment-lease", apartment_lease()),
    ]
}

fn record(jail: &Path) -> Result<(), String> {
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    // Without the shell's area resolver the service works in
    // `<host dir>/pdf`; laid out like the app's storage, every path it is
    // given and every path it answers is the one PDF Tools uses.
    let area = work.path().join("pdf");
    let library = area.join(LIBRARY);
    std::fs::create_dir_all(&library).map_err(|e| e.to_string())?;
    let docs = documents();
    for (name, _, bytes) in &docs {
        std::fs::write(library.join(name), bytes).map_err(|e| e.to_string())?;
    }

    // A file that is not a PDF at all (a download that failed): the app's
    // error state, with the engine's own answer.
    std::fs::write(library.join(DAMAGED), DAMAGED_BYTES).map_err(|e| e.to_string())?;

    octosense_pdf_service::register();
    let mut service = Recorder { host_dir: work.path().to_path_buf(), next: 0 };
    let mut recorded = Map::new();
    for (name, slug, _) in &docs {
        let path = format!("{LIBRARY}/{name}");
        recorded.insert(path.clone(), service.document(&path, slug)?);
    }
    let damaged = format!("{LIBRARY}/{DAMAGED}");
    match service.answer("info", json!({ "path": damaged })) {
        Err(error) => recorded.insert(damaged, json!({ "error": error })),
        Ok(_) => return Err(format!("the engine opened {DAMAGED}, which is not a PDF")),
    };

    // One merge, the one the UI test drives: Board minutes, then the report.
    let paths = vec![format!("{LIBRARY}/Board minutes.pdf"), format!("{LIBRARY}/Quarterly report.pdf")];
    let out = format!("{LIBRARY}/Merged.pdf");
    let answer = service.call("merge", json!({ "paths": paths, "out": out }))?;
    let merged = json!({ out.clone(): service.document(&out, "merged")? });
    let merges = vec![json!({ "paths": paths, "out": out, "answer": answer, "docs": merged })];

    // Two splits of the field guide: every 2 pages, and the cover on its own.
    let mut splits = Vec::new();
    for (choice, folder) in [(json!({ "every": 2 }), "split-every-2"), (json!({ "before": [2] }), "split-before-2")] {
        let mut args = json!({ "path": format!("{LIBRARY}/Field guide.pdf"), "out_dir": LIBRARY });
        for (key, value) in choice.as_object().into_iter().flatten() {
            args[key] = value.clone();
        }
        let answer = service.call("split", args.clone())?;
        let mut parts = Map::new();
        for file in answer["files"].as_array().into_iter().flatten() {
            let part = file["path"].as_str().ok_or("split answered a file without a path")?.to_string();
            let stem = Path::new(&part).file_stem().and_then(|s| s.to_str()).unwrap_or("part").to_lowercase().replace(' ', "-");
            parts.insert(part.clone(), service.document(&part, &format!("{folder}/{stem}"))?);
        }
        let mut split = args;
        split["answer"] = answer;
        split["docs"] = Value::Object(parts);
        splits.push(split);
    }

    // The app's storage: the four samples, the renders and the answers.
    let jail_library = jail.join(LIBRARY);
    std::fs::create_dir_all(&jail_library).map_err(|e| e.to_string())?;
    for (name, _, bytes) in &docs {
        std::fs::write(jail_library.join(name), bytes).map_err(|e| e.to_string())?;
    }
    std::fs::write(jail_library.join(DAMAGED), DAMAGED_BYTES).map_err(|e| e.to_string())?;
    copy_tree(&area.join(REPLAY), &jail.join(REPLAY))?;
    let replay = json!({
        "replay": 1,
        "about": "Answers octosense-pdf-service gave for these files, recorded by its pdftools_fixture example. A developer fixture: PDF Tools uses it only when the pdf engine refuses it or is missing.",
        "sides": { "thumb": THUMB_SIDE, "view": VIEW_SIDE },
        "docs": recorded,
        "merges": merges,
        "splits": splits,
    });
    let text = serde_json::to_string_pretty(&replay).map_err(|e| e.to_string())?;
    std::fs::write(jail.join("dev/engine-replay.json"), text).map_err(|e| e.to_string())
}

struct NoSheets;

impl ServiceHost for NoSheets {
    fn open_sheet(&mut self, _body: String) {}
    fn close_sheet(&mut self) {}
}

/// Calls the registered service as App Hub's dispatcher does for an app in
/// the foreground.
struct Recorder {
    host_dir: PathBuf,
    next: u64,
}

impl Recorder {
    fn call(&mut self, method: &str, args: Value) -> Result<Value, String> {
        self.answer(method, args).map_err(|e| format!("{method} failed: {e}"))
    }

    /// The service's answer as the app receives it: its JSON, or its error
    /// text untouched.
    fn answer(&mut self, method: &str, args: Value) -> Result<Value, String> {
        self.next += 1;
        let call = ServiceCall {
            app_id: APP.into(),
            service: format!("pdf.{method}"),
            args,
            from_sheet: false,
            may_prompt: true,
            host_dir: self.host_dir.clone(),
        };
        dispatch(call, HEAP, self.next, &mut NoSheets);
        let (_, _, result) = take_replies_for(&[HEAP])
            .into_iter()
            .find(|(_, id, _)| *id == self.next)
            .ok_or_else(|| format!("pdf.{method} did not answer"))?;
        let json = result?;
        serde_json::from_str(&json).map_err(|e| format!("pdf.{method}: {e}"))
    }

    /// `info`, `text` and every page at both sizes.
    fn document(&mut self, path: &str, slug: &str) -> Result<Value, String> {
        let info = self.call("info", json!({ "path": path }))?;
        let text = self.call("text", json!({ "path": path }))?;
        let pages = info["pages"].as_array().map(Vec::len).unwrap_or(0);
        let mut renders = Map::new();
        for page in 1..=pages {
            let mut sizes = Map::new();
            for (kind, side) in [("thumb", THUMB_SIDE), ("view", VIEW_SIDE)] {
                let out = format!("{REPLAY}/{slug}/p{page}-{kind}.png");
                sizes.insert(kind.into(), self.call("render", json!({ "path": path, "page": page, "out": out, "max_side": side }))?);
            }
            renders.insert(page.to_string(), Value::Object(sizes));
        }
        Ok(json!({ "info": info, "text": text, "renders": renders }))
    }
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- the PDFs
//
// Small hand-written PDF 1.4 files: the standard Type 1 fonts (no embedded
// font), filled shapes and plain text, uncompressed, so the samples are
// deterministic and need nothing outside this file.

const W: f64 = 595.0;
const H: f64 = 842.0;
const M: f64 = 56.0;
const NAVY: u32 = 0x1f3a5f;
const INK: u32 = 0x1f2328;
const GREY: u32 = 0x667085;
const RULE: u32 = 0xd0d5dd;
const SOFT: u32 = 0xf2f4f7;

/// Helvetica, Helvetica-Bold, Times-Roman, Times-Italic.
#[derive(Clone, Copy)]
enum Font {
    Sans,
    Bold,
    Serif,
    Italic,
}

impl Font {
    fn tag(self) -> &'static str {
        match self {
            Font::Sans => "F1",
            Font::Bold => "F2",
            Font::Serif => "F3",
            Font::Italic => "F4",
        }
    }
    /// A rough average glyph width, in ems, for line breaking.
    fn em(self) -> f64 {
        match self {
            Font::Sans => 0.52,
            Font::Bold => 0.56,
            Font::Serif | Font::Italic => 0.46,
        }
    }
}

#[derive(Default)]
struct Page {
    ops: String,
}

fn rgb(c: u32) -> String {
    let part = |shift: u32| f64::from((c >> shift) & 0xff) / 255.0;
    format!("{:.3} {:.3} {:.3}", part(16), part(8), part(0))
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)")
}

impl Page {
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, c: u32) {
        self.ops.push_str(&format!("{} rg {x:.1} {y:.1} {w:.1} {h:.1} re f\n", rgb(c)));
    }
    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, width: f64, c: u32) {
        self.ops.push_str(&format!("{} RG {width:.1} w {x1:.1} {y1:.1} m {x2:.1} {y2:.1} l S\n", rgb(c)));
    }
    fn circle(&mut self, cx: f64, cy: f64, r: f64, c: u32) {
        let k = 0.5523 * r;
        self.ops.push_str(&format!(
            "{} rg {:.1} {cy:.1} m {:.1} {:.1} {:.1} {:.1} {cx:.1} {:.1} c {:.1} {:.1} {:.1} {:.1} {:.1} {cy:.1} c \
             {:.1} {:.1} {:.1} {:.1} {cx:.1} {:.1} c {:.1} {:.1} {:.1} {:.1} {:.1} {cy:.1} c f\n",
            rgb(c),
            cx + r,
            cx + r, cy + k, cx + k, cy + r, cy + r,
            cx - k, cy + r, cx - r, cy + k, cx - r,
            cx - r, cy - k, cx - k, cy - r, cy - r,
            cx + k, cy - r, cx + r, cy - k, cx + r,
        ));
    }
    fn triangle(&mut self, a: (f64, f64), b: (f64, f64), c: (f64, f64), color: u32) {
        self.ops.push_str(&format!(
            "{} rg {:.1} {:.1} m {:.1} {:.1} l {:.1} {:.1} l h f\n",
            rgb(color), a.0, a.1, b.0, b.1, c.0, c.1
        ));
    }
    fn text(&mut self, font: Font, size: f64, x: f64, y: f64, c: u32, s: &str) {
        self.ops.push_str(&format!("BT {} rg /{} {size:.1} Tf {x:.1} {y:.1} Td ({}) Tj ET\n", rgb(c), font.tag(), escape(s)));
    }
    /// Text broken into lines that fit `width`; returns the baseline after
    /// the last line.
    fn para(&mut self, font: Font, size: f64, (x, y): (f64, f64), width: f64, c: u32, s: &str) -> f64 {
        let per_line = (width / (size * font.em())).floor().max(8.0) as usize;
        let mut y = y;
        let mut line = String::new();
        for word in s.split_whitespace() {
            if !line.is_empty() && line.len() + 1 + word.len() > per_line {
                self.text(font, size, x, y, c, &line);
                y -= size * 1.38;
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() {
            self.text(font, size, x, y, c, &line);
            y -= size * 1.38;
        }
        y
    }
    fn running(&mut self, title: &str, number: usize) {
        self.text(Font::Sans, 9.0, M, H - 36.0, GREY, title);
        self.line(M, H - 44.0, W - M, H - 44.0, 0.6, RULE);
        self.text(Font::Sans, 9.0, W - M - 10.0, 34.0, GREY, &number.to_string());
    }
}

/// A PDF of `pages` with the document information `info`.
fn pdf(info: &[(&str, &str)], pages: Vec<Page>) -> Vec<u8> {
    let mut objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        String::new(), // the page tree, once the page objects are numbered
        format!(
            "<< {} /CreationDate (D:20261009090000Z) >>",
            info.iter().map(|(k, v)| format!("/{k} ({})", escape(v))).collect::<Vec<_>>().join(" ")
        ),
    ];
    for base in ["Helvetica", "Helvetica-Bold", "Times-Roman", "Times-Italic"] {
        objects.push(format!("<< /Type /Font /Subtype /Type1 /BaseFont /{base} /Encoding /WinAnsiEncoding >>"));
    }
    let mut kids = Vec::new();
    for page in &pages {
        let page_id = objects.len() + 1;
        kids.push(format!("{page_id} 0 R"));
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {W} {H}] \
             /Resources << /Font << /F1 4 0 R /F2 5 0 R /F3 6 0 R /F4 7 0 R >> >> /Contents {} 0 R >>",
            page_id + 1
        ));
        objects.push(format!("<< /Length {} >>\nstream\n{}endstream", page.ops.len(), page.ops));
    }
    objects[1] = format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len());
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{body}\nendobj\n", i + 1).bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).bytes());
    out
}

fn info<'a>(title: &'a str, author: &'a str, subject: &'a str, keywords: &'a str) -> [(&'a str, &'a str); 6] {
    [
        ("Title", title),
        ("Author", author),
        ("Subject", subject),
        ("Keywords", keywords),
        ("Creator", "PDF Tools sample generator"),
        ("Producer", "octosense-pdf-service pdftools_fixture"),
    ]
}

fn quarterly_report() -> Vec<u8> {
    let title = "Quarterly report - third quarter 2026";
    let mut cover = Page::default();
    cover.rect(0.0, 500.0, W, H - 500.0, NAVY);
    cover.text(Font::Bold, 11.0, M, 770.0, 0xf2a541, "QUARTERLY REPORT");
    cover.text(Font::Bold, 38.0, M, 690.0, 0xffffff, "Third quarter 2026");
    cover.para(Font::Sans, 15.0, (M, 650.0), 420.0, 0xd9e2ec, "Revenue, costs and the outlook for July to September.");
    cover.rect(M, 560.0, 72.0, 6.0, 0xf2a541);
    let cards = [("Revenue", "$4.2M", "+12% on Q2"), ("Gross margin", "61%", "+3 points"), ("Customers", "1,840", "+215 new")];
    for (i, (label, value, note)) in cards.iter().enumerate() {
        let x = M + i as f64 * 166.0;
        cover.rect(x, 330.0, 150.0, 120.0, SOFT);
        cover.text(Font::Sans, 10.0, x + 14.0, 422.0, GREY, label);
        cover.text(Font::Bold, 26.0, x + 14.0, 380.0, NAVY, value);
        cover.text(Font::Sans, 10.0, x + 14.0, 350.0, 0x2e8540, note);
    }
    cover.para(Font::Serif, 12.0, (M, 270.0), W - 2.0 * M, INK,
        "The quarter closed ahead of plan. Subscription revenue grew in every region, the new support tier \
         paid for itself in eight weeks, and costs stayed flat while the team grew by four people.");
    cover.line(M, 80.0, W - M, 80.0, 0.6, RULE);
    cover.text(Font::Sans, 9.0, M, 62.0, GREY, "Prepared by the finance team, 9 October 2026");

    let mut summary = Page::default();
    summary.running(title, 2);
    summary.text(Font::Bold, 24.0, M, 740.0, INK, "Summary");
    let mut y = summary.para(Font::Serif, 12.0, (M, 710.0), W - 2.0 * M, INK,
        "Revenue reached 4.2 million dollars, twelve per cent more than in the second quarter and thirty-five \
         per cent more than a year ago. Most of the growth came from existing customers moving to annual plans.");
    y = summary.para(Font::Serif, 12.0, (M, y - 8.0), W - 2.0 * M, INK,
        "Operating costs were 2.6 million dollars, almost unchanged. Hosting costs fell after the move to the new \
         region, which offset the cost of four new hires in support and engineering.");
    summary.para(Font::Serif, 12.0, (M, y - 8.0), W - 2.0 * M, INK,
        "Cash at the end of the quarter was 9.8 million dollars. No new debt was taken on.");
    let base = 250.0;
    summary.line(M + 20.0, base, W - M, base, 0.8, GREY);
    let bars = [("Q4 2025", 3.1), ("Q1 2026", 3.4), ("Q2 2026", 3.75), ("Q3 2026", 4.2)];
    for (i, (label, value)) in bars.iter().enumerate() {
        let x = M + 50.0 + i as f64 * 110.0;
        let h = value * 52.0;
        summary.rect(x, base, 60.0, h, if i == 3 { 0xf2a541 } else { NAVY });
        summary.text(Font::Bold, 11.0, x + 14.0, base + h + 8.0, INK, &format!("{value:.2}"));
        summary.text(Font::Sans, 10.0, x + 6.0, base - 18.0, GREY, label);
    }
    summary.text(Font::Italic, 11.0, M, 190.0, GREY, "Revenue by quarter, in millions of dollars.");

    let mut regions = Page::default();
    regions.running(title, 3);
    regions.text(Font::Bold, 24.0, M, 740.0, INK, "Regional results");
    regions.para(Font::Serif, 12.0, (M, 710.0), W - 2.0 * M, INK,
        "Every region grew. The South grew fastest after the partnership with two local resellers; the North \
         remains the largest region by revenue.");
    let rows = [
        ["Region", "Revenue", "Change", "Customers"],
        ["North", "$1.32M", "+9%", "520"],
        ["South", "$0.98M", "+15%", "410"],
        ["East", "$1.14M", "+11%", "470"],
        ["West", "$0.76M", "+14%", "440"],
        ["Total", "$4.20M", "+12%", "1,840"],
    ];
    let mut y = 620.0;
    for (r, row) in rows.iter().enumerate() {
        let fill = if r == 0 { NAVY } else if r % 2 == 0 { SOFT } else { 0xffffff };
        regions.rect(M, y - 10.0, W - 2.0 * M, 30.0, fill);
        for (c, cell) in row.iter().enumerate() {
            let font = if r == 0 || r == rows.len() - 1 { Font::Bold } else { Font::Sans };
            let color = if r == 0 { 0xffffff } else { INK };
            regions.text(font, 11.0, M + 12.0 + c as f64 * 120.0, y, color, cell);
        }
        y -= 30.0;
    }
    regions.line(M, y + 20.0, W - M, y + 20.0, 0.8, GREY);
    regions.para(Font::Serif, 12.0, (M, y - 20.0), W - 2.0 * M, INK,
        "Customer numbers count paying organisations at the end of the quarter. Revenue is recognised monthly.");

    let mut outlook = Page::default();
    outlook.running(title, 4);
    outlook.text(Font::Bold, 24.0, M, 740.0, INK, "Outlook");
    let y = outlook.para(Font::Serif, 12.0, (M, 710.0), W - 2.0 * M, INK,
        "We expect fourth-quarter revenue between 4.4 and 4.6 million dollars. Annual plans renew mostly in \
         January, so the first quarter of 2027 should start strongly. Hiring continues at the current pace.");
    outlook.rect(M, y - 150.0, W - 2.0 * M, 120.0, 0xfff4d6);
    outlook.rect(M, y - 150.0, 5.0, 120.0, 0xf2a541);
    outlook.text(Font::Bold, 13.0, M + 20.0, y - 56.0, INK, "Next steps");
    for (i, step) in ["Open the second support office in November.", "Finish the move of the remaining services to the new region.", "Review prices for the small-team plan before January."].iter().enumerate() {
        outlook.text(Font::Sans, 11.0, M + 20.0, y - 80.0 - i as f64 * 18.0, INK, &format!("-  {step}"));
    }
    pdf(&info(title, "Finance team", "Revenue, costs and outlook for July to September 2026", "quarterly, finance, report"),
        vec![cover, summary, regions, outlook])
}

fn board_minutes() -> Vec<u8> {
    let title = "Board meeting minutes - 2 October 2026";
    let mut first = Page::default();
    first.text(Font::Bold, 26.0, M, 760.0, INK, "Board meeting minutes");
    first.text(Font::Sans, 12.0, M, 736.0, GREY, "Riverside Community Garden  -  Thursday 2 October 2026, 7 pm");
    first.line(M, 722.0, W - M, 722.0, 1.0, 0x3a7d44);
    first.text(Font::Bold, 12.0, M, 696.0, INK, "Present");
    first.para(Font::Serif, 12.0, (M + 70.0, 696.0), W - 2.0 * M - 70.0, INK,
        "Ana Lima (chair), Tom Becker (treasurer), Priya Nair (secretary), Joe Okafor, Mei Chen.");
    let items = [
        ("1. Minutes of the last meeting", "The minutes of 4 September were read and approved without changes."),
        ("2. Treasurer's report", "The garden holds 3,420 dollars. Plot fees for the new season are due by 1 November. \
          The water bill was higher than usual because of the dry August; a rain tank would pay for itself in two seasons."),
        ("3. Autumn work day", "The work day is set for Saturday 18 October. Volunteers will mulch the paths, \
          repair the north fence and plant the bulb bed by the entrance. Tools and lunch are provided."),
        ("4. New plot holders", "Six families are on the waiting list. Two plots will be free after the season; \
          they go to the first two families on the list."),
    ];
    let mut y = 650.0;
    for (heading, body) in items {
        first.text(Font::Bold, 13.0, M, y, INK, heading);
        y = first.para(Font::Serif, 12.0, (M, y - 20.0), W - 2.0 * M, INK, body) - 14.0;
    }
    first.text(Font::Sans, 9.0, W - M - 10.0, 34.0, GREY, "1");

    let mut second = Page::default();
    second.running(title, 2);
    second.text(Font::Bold, 13.0, M, 740.0, INK, "5. Rain tank");
    let y = second.para(Font::Serif, 12.0, (M, 720.0), W - 2.0 * M, INK,
        "Tom presented two quotes for a 2,000 litre rain tank. The board agreed the cheaper quote, \
         including installation, for 640 dollars.");
    second.rect(M, y - 110.0, W - 2.0 * M, 90.0, 0xeaf4ec);
    second.text(Font::Bold, 12.0, M + 16.0, y - 44.0, 0x2c5f34, "Decisions");
    second.text(Font::Sans, 11.0, M + 16.0, y - 64.0, INK, "-  Buy and install the rain tank (640 dollars).");
    second.text(Font::Sans, 11.0, M + 16.0, y - 82.0, INK, "-  Hold the autumn work day on 18 October.");
    let mut row = y - 150.0;
    second.text(Font::Bold, 12.0, M, row, INK, "Actions");
    for (who, what, when) in [("Tom", "Order the rain tank", "10 Oct"), ("Priya", "Email the work-day plan", "6 Oct"),
                              ("Mei", "Contact the waiting list", "15 Oct"), ("Joe", "Borrow a trailer for mulch", "17 Oct")] {
        row -= 24.0;
        second.line(M, row - 8.0, W - M, row - 8.0, 0.5, RULE);
        second.text(Font::Bold, 11.0, M, row, INK, who);
        second.text(Font::Sans, 11.0, M + 80.0, row, INK, what);
        second.text(Font::Sans, 11.0, W - M - 60.0, row, GREY, when);
    }
    second.para(Font::Serif, 12.0, (M, row - 50.0), W - 2.0 * M, INK, "The meeting closed at 8.40 pm. Next meeting: 6 November.");
    second.line(M, 140.0, M + 200.0, 140.0, 0.8, INK);
    second.text(Font::Sans, 10.0, M, 124.0, GREY, "Approved by the chair");
    pdf(&info(title, "Priya Nair", "Minutes of the October board meeting", "minutes, board, garden"), vec![first, second])
}

fn field_guide() -> Vec<u8> {
    let title = "A field guide to garden birds";
    let mut cover = Page::default();
    cover.rect(0.0, 0.0, W, H, 0xf4efe1);
    cover.rect(0.0, 430.0, W, H - 430.0, 0x2f5d3a);
    cover.circle(440.0, 690.0, 70.0, 0xf4c95d);
    cover.text(Font::Bold, 12.0, M, 780.0, 0xcfe3c9, "A POCKET GUIDE");
    cover.text(Font::Bold, 34.0, M, 600.0, 0xffffff, "A field guide to");
    cover.text(Font::Bold, 34.0, M, 556.0, 0xffffff, "garden birds");
    cover.para(Font::Sans, 14.0, (M, 510.0), 360.0, 0xe4efe0, "Six common visitors, how to recognise them and where to look.");
    for (i, c) in [0xd9572b, 0x2f6db5, 0xe8b730, 0x2b2b2b, 0x8a5a3c, 0x4a3f6b].iter().enumerate() {
        cover.circle(M + 40.0 + i as f64 * 82.0, 300.0, 30.0, *c);
    }
    cover.text(Font::Italic, 12.0, M, 220.0, 0x5b5b4c, "Robin, blue tit, goldfinch, blackbird, wren and starling.");
    cover.text(Font::Sans, 9.0, M, 60.0, 0x5b5b4c, "Greenway Nature Club");

    let birds = [
        ("Robin", 0xd9572bu32, "14 cm", "Lawns, hedges, the edge of the vegetable bed",
         "A round bird with an orange-red face and breast and a brown back. Robins defend a territory all year and \
          sing in winter too. They follow gardeners to pick up worms from freshly turned soil.",
         "Both males and females sing; the winter song is thinner and sadder than the spring song."),
        ("Blue tit", 0x2f6db5, "12 cm", "Feeders, trees, nest boxes",
         "Small and busy, with a blue cap, a white face and a yellow belly. Blue tits hang upside down to reach \
          insects at the tips of branches and come to seed feeders in groups.",
         "A pair can raise ten or more chicks, and the parents bring caterpillars hundreds of times a day."),
        ("Goldfinch", 0xe8b730, "12 cm", "Thistles, teasels, nyjer feeders",
         "A red face, black and white head and a broad gold bar on each wing. Goldfinches travel in chattering \
          flocks and pick seeds from seed heads with their fine beaks.",
         "Leave some seed heads standing over winter and goldfinches will visit until spring."),
        ("Blackbird", 0x2b2b2b, "25 cm", "Lawns, shrubs, under hedges",
         "Males are black with a yellow beak and eye ring; females are dark brown. Blackbirds toss leaves aside \
          looking for worms and eat fallen fruit in autumn.",
         "The rich, fluting song is often heard at dusk from a rooftop or a high branch."),
        ("Wren", 0x8a5a3c, "10 cm", "Low cover, log piles, ivy",
         "A tiny brown bird with a short tail often held upright. Wrens creep through low cover like mice, \
          but their song is astonishingly loud for their size.",
         "In cold weather many wrens may roost together in one nest box to keep warm."),
        ("Starling", 0x4a3f6b, "21 cm", "Lawns, rooftops, aerials",
         "From a distance starlings look black; close up they shine purple and green, spotted with white in winter. \
          They walk rather than hop and probe lawns for grubs.",
         "Starlings are fine mimics and copy other birds, phones and car alarms."),
    ];
    let mut pages = vec![cover];
    for (i, (name, color, size, where_, about, fact)) in birds.iter().enumerate() {
        let mut page = Page::default();
        page.running(title, i + 2);
        page.text(Font::Bold, 30.0, M, 740.0, *color, name);
        page.text(Font::Sans, 11.0, M, 714.0, GREY, &format!("Length {size}   -   Where to look: {where_}"));
        page.rect(M, 440.0, W - 2.0 * M, 240.0, 0xf4efe1);
        page.circle(300.0, 540.0, 70.0, *color);
        page.circle(372.0, 600.0, 38.0, *color);
        page.triangle((404.0, 608.0), (440.0, 598.0), (404.0, 590.0), 0xe0a030);
        page.circle(382.0, 612.0, 6.0, 0xffffff);
        page.circle(383.0, 612.0, 3.0, 0x111111);
        page.triangle((236.0, 520.0), (170.0, 470.0), (250.0, 500.0), *color);
        let y = page.para(Font::Serif, 13.0, (M, 400.0), W - 2.0 * M, INK, about);
        page.rect(M, y - 90.0, W - 2.0 * M, 70.0, 0xeaf4ec);
        page.text(Font::Bold, 11.0, M + 14.0, y - 42.0, 0x2c5f34, "Did you know?");
        page.para(Font::Sans, 11.0, (M + 14.0, y - 60.0), W - 2.0 * M - 28.0, INK, fact);
        pages.push(page);
    }
    pdf(&info(title, "Greenway Nature Club", "Six common garden birds and how to recognise them", "birds, nature, guide"), pages)
}

fn apartment_lease() -> Vec<u8> {
    let title = "Residential lease agreement";
    let clauses = [
        ("1. Parties", "This agreement is made between Harbour Lane Homes (the landlord) and Sam Rivera (the tenant) for \
          the apartment at 14 Harbour Lane, flat 3B (the premises)."),
        ("2. Term", "The lease begins on 1 November 2026 and ends on 31 October 2027. It continues month to month after that \
          unless either party gives written notice at least thirty days before the end of the term."),
        ("3. Rent", "The rent is 1,450 dollars a month, due on the first day of each month by bank transfer. Rent paid more \
          than five days late carries a fee of 40 dollars."),
        ("4. Deposit", "The tenant pays a deposit of 1,450 dollars before moving in. The landlord returns it within \
          twenty-one days after the tenant moves out, less the cost of any damage beyond normal wear."),
        ("5. Utilities", "The tenant pays for electricity, internet and gas. The landlord pays for water, rubbish \
          collection and the upkeep of shared areas."),
        ("6. Use of the premises", "The premises are a private home for the tenant and the people named in this agreement. \
          No business that brings customers to the building may be run from the premises."),
        ("7. Repairs", "The tenant reports repairs promptly in writing. The landlord makes urgent repairs within \
          forty-eight hours and other repairs within fourteen days."),
        ("8. Access", "The landlord may enter the premises for inspections or repairs with at least twenty-four hours' notice, \
          or at any time in an emergency."),
        ("9. Pets", "One cat or one small dog is allowed with the landlord's written consent. The tenant is responsible \
          for any damage the animal causes."),
        ("10. Alterations", "The tenant may hang pictures and shelves. Painting, new locks or other changes need the \
          landlord's written consent."),
        ("11. Ending the lease early", "The tenant may end the lease early with sixty days' written notice and a fee equal to \
          one month's rent."),
        ("12. Whole agreement", "This document is the whole agreement between the parties. Changes are valid only in writing \
          and signed by both parties."),
    ];
    let mut pages = Vec::new();
    let mut page = Page::default();
    page.text(Font::Bold, 22.0, M, 760.0, INK, title);
    page.text(Font::Sans, 11.0, M, 738.0, GREY, "14 Harbour Lane, flat 3B  -  1 November 2026 to 31 October 2027");
    page.line(M, 724.0, W - M, 724.0, 1.0, INK);
    let mut y = 696.0;
    for (heading, body) in clauses {
        if y < 150.0 {
            pages.push(std::mem::take(&mut page));
            page.running(title, pages.len() + 1);
            y = 740.0;
        }
        page.text(Font::Bold, 12.0, M, y, INK, heading);
        y = page.para(Font::Serif, 11.0, (M, y - 18.0), W - 2.0 * M, INK, body) - 12.0;
    }
    if y < 260.0 {
        pages.push(std::mem::take(&mut page));
        page.running(title, pages.len() + 1);
        y = 740.0;
    }
    page.text(Font::Bold, 12.0, M, y - 20.0, INK, "Signatures");
    for (i, who) in ["Landlord: Harbour Lane Homes", "Tenant: Sam Rivera"].iter().enumerate() {
        let x = M + i as f64 * 250.0;
        page.line(x, y - 100.0, x + 210.0, y - 100.0, 0.8, INK);
        page.text(Font::Sans, 10.0, x, y - 116.0, GREY, who);
        page.text(Font::Sans, 10.0, x, y - 132.0, GREY, "Date:");
    }
    pages.push(page);
    pdf(&info(title, "Harbour Lane Homes", "Lease for 14 Harbour Lane, flat 3B", "lease, rental, agreement"), pages)
}
