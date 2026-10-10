//! The open-document methods (PDF Tools v2, `apps/pdftools/design/SERVICE.md`),
//! through [`serve`] as App Hub hands calls to the service, on the sample PDFs
//! the `pdftools_fixture` example writes and on PDFs built here.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value as Json};

use super::tests::{no_staging_left, resolver, tiny_pdf};
use super::*;

/// The sample PDFs of `examples/pdftools_fixture.rs`.
#[allow(dead_code)]
#[path = "../examples/pdftools_fixture.rs"]
mod fixture;

const APP: &str = "os.pdftools";

/// An app's storage, served as the shell serves an app's own requests.
struct World {
    _dir: tempfile::TempDir,
    root: PathBuf,
    areas: Slot,
}

impl World {
    fn new() -> World {
        World::with_quota(None)
    }

    fn with_quota(quota: Option<u64>) -> World {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("storage");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, quota);
        World { _dir: dir, root, areas }
    }

    /// The sample PDFs, at the top of the storage.
    fn samples(self) -> World {
        for (name, bytes) in fixture::documents() {
            self.put(name, &bytes);
        }
        self.put("Garden plan.pdf", &fixture::garden_plan());
        self
    }

    fn put(&self, rel: &str, bytes: &[u8]) {
        let path = self.root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// A call of `app`, from the foreground (`may_prompt`) or not, from the
    /// isolate whose heap key is `isolate` (0: none).
    fn call_full(&self, app: &str, method: &str, args: Json, may_prompt: bool, isolate: usize) -> Result<Json, String> {
        let call = ServiceCall { app_id: app.into(), service: format!("pdf.{method}"), args, from_sheet: false, may_prompt, host_dir: self.root.join(".host") };
        serve(&self.areas, &call, isolate)
    }

    fn call(&self, method: &str, args: Json) -> Result<Json, String> {
        self.call_full(APP, method, args, true, 0)
    }

    fn ok(&self, method: &str, args: Json) -> Json {
        self.call(method, args.clone()).unwrap_or_else(|e| panic!("pdf.{method} {args}: {e}"))
    }

    fn err(&self, method: &str, args: Json) -> String {
        match self.call(method, args.clone()) {
            Ok(answer) => panic!("pdf.{method} {args} answered {answer}"),
            Err(e) => e,
        }
    }

    /// Open `path`: its handle.
    fn open(&self, path: &str) -> String {
        self.ok("open", json!({ "path": path }))["doc"].as_str().unwrap().to_string()
    }
}

/// `error` starts with the stable code `code`.
#[track_caller]
fn coded(error: &str, code: &str) {
    assert!(error.starts_with(&format!("{code}: ")), "expected {code}: {error}");
}

/// A one-page PDF of `w` × `h` points with a line of text.
fn sized_pdf(w: f64, h: f64) -> Vec<u8> {
    let content = "BT /F1 12 Tf 20 20 Td (A sized page) Tj ET";
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    assemble(&objs)
}

/// Number and cross-reference `objs` (object 1 the catalog).
fn assemble(objs: &[String]) -> Vec<u8> {
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

/// A form whose own scripts would each leave a mark if they ran: field
/// `a` has a keystroke and a format script that write `note`; `total` has
/// a calculate script (and is in the calculation order); `v` has a
/// validate script that refuses every value; a document script writes
/// `note`, and so does the document's open action.
fn scripted_form() -> Vec<u8> {
    let widget = |name: &str, value: &str, y: u32, actions: &str| {
        format!("<< /Type /Annot /Subtype /Widget /FT /Tx /T ({name}) /V ({value}) /Rect [20 {y} 180 {}] /P 3 0 R /F 4 /DA (/Helv 10 Tf 0 g){actions} >>", y + 20)
    };
    let js = |source: &str| format!("<< /S /JavaScript /JS ({source}) >>");
    let page = "BT /Helv 12 Tf 20 280 Td (A scripted form) Tj ET";
    let objs = [
        // 1 catalog, 2 pages, 3 page
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 5 0 R 6 0 R 7 0 R] /CO [5 0 R] /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 8 0 R >> >> >> /OpenAction 9 0 R /Names << /JavaScript 15 0 R >> >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Resources << /Font << /Helv 8 0 R >> >> /Annots [4 0 R 5 0 R 6 0 R 7 0 R] /Contents 16 0 R >>".to_string(),
        // 4–7 the fields
        widget("a", "1", 240, " /AA << /K 10 0 R /F 11 0 R >>"),
        widget("total", "untouched", 200, " /AA << /C 12 0 R >>"),
        widget("v", "", 160, " /AA << /V 13 0 R >>"),
        widget("note", "quiet", 120, ""),
        // 8 the font
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
        // 9 the open action, 10–13 the field scripts, 14 the document script
        js("this.getField(\"note\").value = \"opened\";"),
        js("this.getField(\"note\").value = \"keystroke\";"),
        js("event.value = \"formatted\"; this.getField(\"note\").value = \"format\";"),
        js("event.value = \"calculated\";"),
        js("event.rc = false;"),
        js("this.getField(\"note\").value = \"document script\";"),
        // 15 the document scripts' name tree, 16 the page's content
        "<< /Names [(setup) 14 0 R] >>".to_string(),
        format!("<< /Length {} >>\nstream\n{page}\nendstream", page.len()),
    ];
    assemble(&objs)
}

/// A field's value in `fields` (from pdf.fields).
fn value(fields: &Json, name: &str) -> Json {
    fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == name).map(|f| f["value"].clone()).unwrap_or_else(|| panic!("no field {name}: {fields}"))
}

/// A PDF that opens only with a password, made by the engine itself.
fn locked_pdf() -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("plain.pdf"), tiny_pdf("A secret", "page two")).unwrap();
    let mut a = Automation::new().with_root(dir.path()).unwrap();
    let (doc, _) = open(&mut a, "plain.pdf").unwrap();
    run(&mut a, "doc_protect", &json!({ "doc": doc, "open_password": "correct horse" })).unwrap();
    run(&mut a, "doc_save", &json!({ "doc": doc, "path": "locked.pdf" })).unwrap();
    std::fs::read(dir.path().join("locked.pdf")).unwrap()
}

// ------------------------------------------------------------ documents and reading

#[test]
fn opening_reading_and_closing() {
    let w = World::new().samples();
    let open = w.ok("open", json!({"path": "Quarterly report.pdf"}));
    let doc = open["doc"].as_str().unwrap().to_string();
    assert!(!doc.is_empty() && doc.bytes().all(|b| b.is_ascii_hexdigit()), "{open}");
    assert_eq!(open["path"], "Quarterly report.pdf");
    assert_eq!(open["pages"], 4);
    assert_eq!(open["sizes"], json!([[595.0, 842.0], [595.0, 842.0], [595.0, 842.0], [595.0, 842.0]]));
    assert_eq!(open["title"], "Quarterly report - third quarter 2026");
    assert_eq!(open["outline"], json!([]));
    assert_eq!((open["fields"].clone(), open["comments"].clone(), open["can_edit"].clone()), (json!(0), json!(0), json!(true)), "{open}");

    assert_eq!(w.ok("state", json!({"doc": doc})), json!({"edited": false, "can_undo": false, "can_redo": false, "pages": 4}));
    let info = w.ok("info", json!({"doc": doc}));
    assert_eq!((info["file"].clone(), info["document"]["path"].clone(), info["document"]["pages"].clone()), (json!("Quarterly report.pdf"), json!("Quarterly report.pdf"), json!(4)), "{info}");
    let text = w.ok("text", json!({"doc": doc, "pages": [2]}));
    assert_eq!(text["pages"][0]["page"], 2);
    assert!(text["pages"][0]["text"].as_str().unwrap().contains("Summary"), "{text}");

    // A page at screen resolution, into the render cache; again from it.
    let page = w.ok("page", json!({"doc": doc, "page": 1}));
    let rel = format!(".cache/pages/{doc}/1@96.png");
    assert_eq!(page, json!({"path": rel, "width": 794, "height": 1123, "dpi": 96}));
    let file = w.root.join(&rel);
    assert!(std::fs::read(&file).unwrap().starts_with(b"\x89PNG"));
    let written = std::fs::metadata(&file).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(w.ok("page", json!({"doc": doc, "page": 1})), page);
    assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), written, "served from the cache, not drawn again");
    let small = w.ok("page", json!({"doc": doc, "page": 2, "dpi": 36}));
    assert_eq!(small, json!({"path": format!(".cache/pages/{doc}/2@36.png"), "width": 298, "height": 421, "dpi": 36}));

    let found = w.ok("find", json!({"doc": doc, "query": "revenue"}));
    let total = found["total"].as_u64().unwrap();
    let matches = found["matches"].as_array().unwrap();
    assert!(total >= 4 && matches.len() as u64 == total, "{found}");
    for m in matches {
        let rect = &m["rects"][0];
        assert_eq!(rect.as_array().unwrap().len(), 4, "{m}");
        assert!(rect[2].as_f64().unwrap() > 0.0 && rect[3].as_f64().unwrap() > 0.0, "[x, y, w, h]: {m}");
        let snippet = m["snippet"].as_str().unwrap();
        assert!(snippet.chars().count() <= 120, "{snippet}");
        let marked = snippet.split("[[").nth(1).and_then(|s| s.split("]]").next()).unwrap();
        assert_eq!(marked.to_lowercase(), "revenue", "{snippet}");
    }
    assert_eq!(w.ok("find", json!({"doc": doc, "query": "revenue", "limit": 2}))["matches"].as_array().unwrap().len(), 2);
    assert_eq!(w.ok("find", json!({"doc": doc, "query": "revenue", "limit": 2}))["total"], json!(total), "total counts past the limit");
    assert_eq!(w.ok("find", json!({"doc": doc, "query": "no such words here"})), json!({"total": 0, "matches": []}));

    let lines = w.ok("lines", json!({"doc": doc, "page": 2}));
    let first = &lines["lines"][0];
    assert!(first["n"] == 1 && first["text"].is_string() && first["font"].is_string() && first["size"].is_number(), "{lines}");
    assert_eq!(first["box"].as_array().unwrap().len(), 4);
    assert!(lines["lines"].as_array().unwrap().iter().any(|l| l["text"] == "Summary"), "{lines}");
    let para = &lines["paragraphs"][0];
    assert!(para["n"] == 1 && para["lines"].as_array().is_some_and(|l| !l.is_empty()) && para["box"].as_array().is_some(), "{lines}");

    assert_eq!(w.ok("close", json!({"doc": doc})), json!({"closed": true}));
    coded(&w.err("state", json!({"doc": doc})), "unknown_doc");
    assert!(!w.root.join(format!(".cache/pages/{doc}")).exists(), "a closed document's renders go");
}

#[test]
fn a_damaged_pdf_is_damaged_and_a_missing_one_not_found() {
    let w = World::new().samples();
    let e = w.err("open", json!({"path": "Damaged scan.pdf"}));
    coded(&e, "damaged");
    assert!(e.len() > "damaged: ".len() + 5, "the engine's reason follows: {e}");
    coded(&w.err("open", json!({"path": "Not here.pdf"})), "not_found");
    coded(&w.err("info", json!({"path": "Damaged scan.pdf"})), "damaged");
    coded(&w.err("open", json!({})), "invalid");
    assert_eq!(docs::open_count(), 0);
}

#[test]
fn a_protected_pdf_is_refused_as_protected() {
    let w = World::new().samples();
    w.put("locked.pdf", &locked_pdf());
    for (method, args) in [("open", json!({"path": "locked.pdf"})), ("info", json!({"path": "locked.pdf"})), ("text", json!({"path": "locked.pdf"}))] {
        let e = w.err(method, args);
        coded(&e, "protected");
        assert!(e.contains("password"), "{e}");
    }
    let e = w.err("merge", json!({"paths": ["locked.pdf", "Board minutes.pdf"], "out": "m.pdf"}));
    coded(&e, "protected");
    let doc = w.open("Board minutes.pdf");
    coded(&w.err("pages", json!({"doc": doc, "op": "insert_file", "path": "locked.pdf", "at": 1})), "protected");
    assert_eq!(w.ok("state", json!({"doc": doc}))["pages"], 2, "nothing was inserted");
}

// ------------------------------------------------------------ handles and the cap

#[test]
fn an_unknown_or_foreign_handle_is_unknown() {
    let w = World::new().samples();
    let doc = w.open("Board minutes.pdf");
    // Another app, in the same storage: the handle names nothing of its own.
    let e = w.call_full("os.notes", "state", json!({"doc": doc}), true, 0).unwrap_err();
    coded(&e, "unknown_doc");
    // The same app in another storage scope (another account's folder).
    let other = World::new().samples();
    coded(&other.err("state", json!({"doc": doc})), "unknown_doc");
    coded(&other.err("close", json!({"doc": doc, "discard": true})), "unknown_doc");
    for bad in [json!("0000"), json!("not a handle"), json!("../x"), json!(7), json!(true), json!(["x"])] {
        coded(&w.err("state", json!({"doc": bad})), "unknown_doc");
    }
    coded(&w.err("state", json!({})), "invalid");
    // Every method that takes a document refuses a foreign one alike.
    let foreign = |method: &str, args: Json| {
        let e = other.err(method, args);
        coded(&e, "unknown_doc");
    };
    foreign("info", json!({"doc": doc}));
    foreign("text", json!({"doc": doc}));
    foreign("page", json!({"doc": doc, "page": 1}));
    foreign("find", json!({"doc": doc, "query": "garden"}));
    foreign("lines", json!({"doc": doc, "page": 1}));
    foreign("comments", json!({"doc": doc}));
    foreign("comment", json!({"doc": doc, "op": "add", "page": 1, "type": "note", "at": [10, 10]}));
    foreign("fields", json!({"doc": doc}));
    foreign("fill", json!({"doc": doc, "values": {"x": "y"}}));
    foreign("fill_sign", json!({"doc": doc, "page": 1, "kind": "check", "at": [10, 10]}));
    foreign("pages", json!({"doc": doc, "op": "rotate", "pages": [1], "angle": 90}));
    foreign("edit_text", json!({"doc": doc, "page": 1, "line": 1, "text": "x"}));
    foreign("undo", json!({"doc": doc}));
    foreign("redo", json!({"doc": doc}));
    foreign("save", json!({"doc": doc}));
    foreign("export", json!({"doc": doc, "kind": "text", "out": "x.txt"}));
    // Still open for its own caller; once closed, unknown to it too.
    assert_eq!(w.ok("state", json!({"doc": doc}))["pages"], 2);
    w.ok("close", json!({"doc": doc}));
    coded(&w.err("state", json!({"doc": doc})), "unknown_doc");
}

#[test]
fn the_ninth_open_is_refused() {
    let w = World::new().samples();
    let docs: Vec<String> = (0..docs::MAX_OPEN).map(|_| w.open("Board minutes.pdf")).collect();
    let mut unique = docs.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 8, "a handle per open: {docs:?}");
    let e = w.err("open", json!({"path": "Field guide.pdf"}));
    coded(&e, "too_many_open");
    // The cap is per caller: another app, and another scope, still open theirs.
    assert!(w.call_full("os.notes", "open", json!({"path": "Board minutes.pdf"}), true, 0).is_ok());
    let other = World::new().samples();
    other.open("Board minutes.pdf");
    w.ok("close", json!({"doc": docs[3]}));
    w.open("Field guide.pdf");
    coded(&w.err("open", json!({"path": "Field guide.pdf"})), "too_many_open");
}

/// A document opened from an app's isolate lives while that isolate's
/// storage scope does: when the app closes (App Hub's card shutdown clears
/// the isolate's host tag), the next call of anyone releases it.
#[test]
fn a_document_goes_when_its_apps_storage_scope_does() {
    use makepad_widgets::*;
    let w = World::new().samples();
    let mut cx = Cx::new(Box::new(|_, _| {}));
    let mut card = cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        let value = vm.eval(script! {use mod.widgets.* Splash{}});
        Splash::script_from_value(vm, value)
    });
    card.set_host_tag(&mut cx, Some(APP.into()));
    card.set_sandbox_dir(&mut cx, Some(w.root.clone()));
    card.set_text(&mut cx, "View {}");
    let heap = card.isolate_heap_key(&mut cx).unwrap();
    let opened = w.call_full(APP, "open", json!({"path": "Board minutes.pdf"}), true, heap).unwrap();
    let doc = opened["doc"].clone();
    let unbound = w.open("Field guide.pdf");
    assert_eq!(w.call_full(APP, "state", json!({"doc": doc}), true, heap).unwrap()["pages"], 2);
    // Another surface of the same app and storage reaches it by its handle.
    assert_eq!(w.call_full(APP, "state", json!({"doc": doc}), false, 0).unwrap()["pages"], 2);
    // Long idle is no reason to close a document an open app holds.
    docs::sweep_at(Instant::now() + docs::IDLE + Duration::from_secs(60));
    assert_eq!(w.call_full(APP, "state", json!({"doc": doc}), true, heap).unwrap()["pages"], 2);
    // The app closes.
    card.set_host_tag(&mut cx, None);
    coded(&w.call_full(APP, "state", json!({"doc": doc}), true, 0).unwrap_err(), "unknown_doc");
    // The document no isolate held went with the idle sweep above.
    coded(&w.err("state", json!({"doc": unbound})), "unknown_doc");
    assert_eq!(docs::open_count(), 0);
    // A scope that changes (another account's storage) is gone too.
    card.set_host_tag(&mut cx, Some(APP.into()));
    let opened = w.call_full(APP, "open", json!({"path": "Board minutes.pdf"}), true, heap).unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    card.set_sandbox_dir(&mut cx, Some(elsewhere.path().to_path_buf()));
    coded(&w.call_full(APP, "state", json!({"doc": opened["doc"]}), true, heap).unwrap_err(), "unknown_doc");
}

#[test]
fn a_document_no_isolate_holds_goes_after_it_is_idle() {
    let w = World::new().samples();
    let doc = w.open("Board minutes.pdf");
    docs::sweep_at(Instant::now() + docs::IDLE - Duration::from_secs(30));
    assert_eq!(w.ok("state", json!({"doc": doc}))["pages"], 2, "not idle long enough");
    docs::sweep_at(Instant::now() + docs::IDLE + Duration::from_secs(1));
    coded(&w.err("state", json!({"doc": doc})), "unknown_doc");
}

// ------------------------------------------------------------ comments, fields, Fill & Sign

#[test]
fn comments_threads_and_status() {
    let w = World::new().samples();
    let doc = w.open("Quarterly report.pdf");
    let hit = &w.ok("find", json!({"doc": doc, "query": "closed ahead of plan"}))["matches"][0];
    let highlight = w.ok("comment", json!({"doc": doc, "op": "add", "page": hit["page"], "type": "highlight", "rects": hit["rects"], "color": "yellow", "text": "Good news", "author": "Ana"}));
    let highlight = highlight["id"].as_str().unwrap().to_string();
    let note = w.ok("comment", json!({"doc": doc, "op": "add", "page": 2, "type": "note", "at": [60, 120], "text": "Check the figures", "author": "Ana"}))["id"].clone();
    let note = note.as_str().unwrap().to_string();
    let boxed = w.ok("comment", json!({"doc": doc, "op": "add", "page": 3, "type": "textbox", "rects": [[60, 100, 200, 40]], "text": "Regions"}))["id"].clone();
    for kind in ["underline", "strikeout"] {
        w.ok("comment", json!({"doc": doc, "op": "add", "page": 4, "type": kind, "rects": [[56, 90, 120, 14], [56, 110, 80, 14]]}));
    }
    let reply = w.ok("comment", json!({"doc": doc, "op": "reply", "id": note, "text": "Done", "author": "Tom"}))["id"].clone();
    assert!(reply.is_string() && reply != json!(note), "{reply}");
    assert_eq!(w.ok("comment", json!({"doc": doc, "op": "status", "id": note, "status": "accepted"})), json!({"id": note}));
    assert_eq!(w.ok("comment", json!({"doc": doc, "op": "edit", "id": note, "text": "Check the figures again", "color": "#FF0000"})), json!({"id": note}));
    assert_eq!(w.ok("comment", json!({"doc": doc, "op": "delete", "id": boxed})), json!({"deleted": true}));

    let list = w.ok("comments", json!({"doc": doc}));
    let all = list["comments"].as_array().unwrap();
    assert_eq!(all.len(), 4, "{list}");
    let by_id = |id: &str| all.iter().find(|c| c["id"] == id).unwrap_or_else(|| panic!("no comment {id}: {list}"));
    let h = by_id(&highlight);
    assert_eq!((h["type"].clone(), h["author"].clone(), h["text"].clone(), h["status"].clone()), (json!("highlight"), json!("Ana"), json!("Good news"), json!("none")), "{h}");
    assert_eq!(h["page"], hit["page"]);
    assert_eq!(h["rects"][0].as_array().unwrap().len(), 4, "{h}");
    assert!(h["date"].as_str().is_some_and(|d| d.len() == 16 && d.as_bytes()[4] == b'-' && d.as_bytes()[10] == b'T'), "ISO 8601: {h}");
    let n = by_id(&note);
    assert_eq!((n["type"].clone(), n["text"].clone(), n["color"].clone(), n["status"].clone()), (json!("note"), json!("Check the figures again"), json!("#FF0000"), json!("accepted")), "{n}");
    let replies = n["replies"].as_array().unwrap();
    assert_eq!(replies.len(), 1, "a status is not a reply: {n}");
    assert_eq!((replies[0]["id"].clone(), replies[0]["author"].clone(), replies[0]["text"].clone()), (reply, json!("Tom"), json!("Done")));
    assert!(all.iter().any(|c| c["type"] == "underline") && all.iter().any(|c| c["type"] == "strikeout"), "{list}");

    for bad in [
        json!({"doc": doc, "op": "add", "page": 1, "type": "stamp", "at": [10, 10]}),
        json!({"doc": doc, "op": "add", "page": 1, "type": "highlight"}),
        json!({"doc": doc, "op": "add", "page": 1, "type": "note"}),
        json!({"doc": doc, "op": "add", "page": 1, "type": "highlight", "rects": [[0, 0, 0, 10]]}),
        json!({"doc": doc, "op": "add", "page": 1, "type": "textbox", "rects": [[0, 0, 10, 10], [20, 20, 10, 10]]}),
        json!({"doc": doc, "op": "status", "id": note, "status": "approved"}),
        json!({"doc": doc, "op": "edit", "id": note}),
        json!({"doc": doc, "op": "nudge", "id": note}),
    ] {
        coded(&w.err("comment", bad), "invalid");
    }
    coded(&w.err("comment", json!({"doc": doc, "op": "add", "page": 9, "type": "note", "at": [10, 10]})), "invalid");
    coded(&w.err("comment", json!({"doc": doc, "op": "delete", "id": "no-such-comment"})), "invalid");
    assert_eq!(w.ok("comments", json!({"doc": doc}))["comments"].as_array().unwrap().len(), 4, "refusals changed nothing");
}

/// `comment_add` is `file` because of its attachment type: the service
/// gives it the five comment types and the reviewed parameters only, and
/// refuses a file (or any other unreviewed) parameter before the engine
/// sees the call.
#[test]
fn comment_add_takes_no_file_parameter() {
    let w = World::new().samples();
    let doc = w.open("Board minutes.pdf");
    for (extra, value) in [("path", json!("Field guide.pdf")), ("file", json!("Field guide.pdf")), ("image", json!("x.png")), ("stamp", json!("approved")), ("find", json!("Garden")), ("quads", json!([[0, 0, 1, 0, 0, 1, 1, 1]])), ("icon", json!("Paperclip"))] {
        let mut args = json!({"doc": doc, "op": "add", "page": 1, "type": "note", "at": [20, 20], "text": "x"});
        args[extra] = value;
        let e = w.err("comment", args);
        coded(&e, "invalid");
        assert!(e.contains(&format!("`{extra}` is not one of them")), "{e}");
    }
    let e = w.err("comment", json!({"doc": doc, "op": "add", "page": 1, "type": "attachment", "at": [20, 20], "path": "Field guide.pdf"}));
    coded(&e, "invalid");
    let e = w.err("comment", json!({"doc": doc, "op": "add", "page": 1, "type": "attachment", "at": [20, 20]}));
    coded(&e, "invalid");
    assert!(e.contains("highlight, underline, strikeout, note, textbox"), "{e}");
    assert_eq!(w.ok("comments", json!({"doc": doc})), json!({"comments": []}), "nothing reached the document");
    assert_eq!(w.ok("state", json!({"doc": doc}))["edited"], false);
}

#[test]
fn fields_fill_and_fill_and_sign() {
    let w = World::new().samples();
    w.put("form.pdf", &scripted_form());
    let doc = w.open("form.pdf");
    let fields = w.ok("fields", json!({"doc": doc}));
    let a = fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == "a").unwrap().clone();
    assert_eq!((a["type"].clone(), a["value"].clone(), a["required"].clone(), a["page"].clone(), a["options"].clone()), (json!("text"), json!("1"), json!(false), json!(1), json!([])), "{a}");
    assert_eq!(a["rect"], json!([20.0, 40.0, 160.0, 20.0]), "[x, y, w, h] from the page's top-left: {a}");
    assert_eq!(w.ok("fill", json!({"doc": doc, "values": {"a": "7", "note": "hello"}})), json!({"filled": 2}));
    let fields = w.ok("fields", json!({"doc": doc}));
    assert_eq!((value(&fields, "a"), value(&fields, "note")), (json!("7"), json!("hello")));
    let state = w.ok("state", json!({"doc": doc}));
    assert_eq!((state["edited"].clone(), state["can_undo"].clone()), (json!(true), json!(true)), "one undo step: {state}");
    w.ok("undo", json!({"doc": doc}));
    let fields = w.ok("fields", json!({"doc": doc}));
    assert_eq!((value(&fields, "a"), value(&fields, "note")), (json!("1"), json!("quiet")), "both values in one step");
    coded(&w.err("fill", json!({"doc": doc, "values": {"no such field": "x"}})), "invalid");
    coded(&w.err("fill", json!({"doc": doc, "values": {}})), "invalid");
    coded(&w.err("fill", json!({"doc": doc, "values": {"a": {"nested": 1}}})), "invalid");

    // Fill & Sign on a form with no fields.
    let lease = w.open("Apartment lease.pdf");
    let last = w.ok("state", json!({"doc": lease}))["pages"].as_u64().unwrap();
    for (kind, text) in [("text", Some("Sam Rivera")), ("date", None), ("initials", Some("SR")), ("check", None), ("cross", None)] {
        let mut args = json!({"doc": lease, "page": last, "kind": kind, "at": [60, 300]});
        if let Some(text) = text {
            args["text"] = json!(text);
        }
        assert_eq!(w.ok("fill_sign", args), json!({"added": true}), "{kind}");
    }
    assert_eq!(w.ok("comments", json!({"doc": lease}))["comments"].as_array().unwrap().len(), 5, "Fill & Sign marks are annotations");
    coded(&w.err("fill_sign", json!({"doc": lease, "page": 1, "kind": "signature", "at": [60, 300], "text": "Sam"})), "invalid");
    coded(&w.err("fill_sign", json!({"doc": lease, "page": 1, "kind": "text", "at": [60, 300]})), "invalid");
    coded(&w.err("fill_sign", json!({"doc": lease, "page": 1, "kind": "check"})), "invalid");
}

/// `doc_open` and `form_fill` are `code`: with JavaScript on, the engine
/// runs a form's scripts. Every service session has it off, so a form's
/// own scripts never run: the keystroke, format, calculate and refusing
/// validate scripts, the document script and the open action of
/// [`scripted_form`] leave no mark through any method, where the engine's
/// default session (JavaScript on) runs the calculate and validate
/// scripts. An XFA form's initialize and calculate scripts stay off too.
#[test]
fn a_forms_own_scripts_never_run() {
    let w = World::new();
    w.put("form.pdf", &scripted_form());
    // The engine's default session: the scripts are live.
    let mut on = Automation::new().with_root(&w.root).unwrap();
    assert!(on.session().javascript());
    let (d, _) = open(&mut on, "form.pdf").unwrap();
    run(&mut on, "form_fill", &json!({"doc": d, "values": {"a": "5"}})).unwrap();
    let live = run(&mut on, "form_fields", &json!({"doc": d})).unwrap();
    assert_eq!(value(&live, "total"), json!("calculated"), "the calculate script runs with JavaScript on: {live}");
    assert_ne!(value(&live, "note"), json!("quiet"), "a script that writes another field runs with JavaScript on: {live}");
    assert!(run(&mut on, "form_fill", &json!({"doc": d, "values": {"v": "x"}})).is_err(), "the validate script refuses every value with JavaScript on");

    // Through the service: nothing runs, whatever the method.
    let doc = w.open("form.pdf");
    let quiet = |fields: &Json| {
        assert_eq!(value(fields, "note"), json!("quiet"), "no open action, document, keystroke or format script ran: {fields}");
        assert_eq!(value(fields, "total"), json!("untouched"), "no calculate script ran: {fields}");
    };
    quiet(&w.ok("fields", json!({"doc": doc})));
    assert_eq!(w.ok("fill", json!({"doc": doc, "values": {"a": "5", "v": "x"}})), json!({"filled": 2}), "no validate script refused the value");
    let fields = w.ok("fields", json!({"doc": doc}));
    quiet(&fields);
    assert_eq!((value(&fields, "a"), value(&fields, "v")), (json!("5"), json!("x")));
    w.ok("page", json!({"doc": doc, "page": 1}));
    w.ok("find", json!({"doc": doc, "query": "scripted"}));
    w.ok("lines", json!({"doc": doc, "page": 1}));
    w.ok("text", json!({"doc": doc}));
    w.ok("info", json!({"doc": doc}));
    quiet(&w.ok("fields", json!({"doc": doc})));
    // Saved and opened again, still as filled.
    w.ok("save", json!({"doc": doc}));
    w.ok("close", json!({"doc": doc}));
    let again = w.open("form.pdf");
    let fields = w.ok("fields", json!({"doc": again}));
    quiet(&fields);
    assert_eq!(value(&fields, "a"), json!("5"));

    // An XFA form: its initialize script would set qty to 2 on opening, and
    // its calculate script total = qty × price.
    w.put("xfa.pdf", &pdfcraft_xfa::fixtures::shell(&pdfcraft_xfa::fixtures::scripted_template()));
    let xfa = w.open("xfa.pdf");
    assert_ne!(value(&w.ok("fields", json!({"doc": xfa})), "qty"), json!("2"), "no initialize script ran");
    w.ok("fill", json!({"doc": xfa, "values": {"qty": "3"}}));
    let fields = w.ok("fields", json!({"doc": xfa}));
    assert_eq!(value(&fields, "qty"), json!("3"));
    assert_ne!(value(&fields, "total"), json!("15"), "no calculate script ran: {fields}");
}

// ------------------------------------------------------------ pages, edits, history

#[test]
fn page_operations_and_text_edits() {
    let w = World::new().samples();
    let doc = w.open("Quarterly report.pdf");
    let pages = |args: Json| {
        let mut args = args;
        args["doc"] = json!(doc);
        w.ok("pages", args)["pages"].as_u64().unwrap()
    };
    assert_eq!(pages(json!({"op": "rotate", "pages": [1], "angle": 90})), 4);
    assert_eq!(w.ok("page", json!({"doc": doc, "page": 1, "dpi": 24}))["width"], 281, "page 1 is landscape now");
    assert_eq!(w.ok("info", json!({"doc": doc}))["pages"][0]["width"], json!(842.0));
    assert_eq!(pages(json!({"op": "rotate", "pages": [1], "angle": -90})), 4);
    assert_eq!(pages(json!({"op": "duplicate", "pages": [2]})), 5);
    assert_eq!(pages(json!({"op": "move", "pages": [3], "to": 1})), 5);
    assert!(w.ok("text", json!({"doc": doc, "pages": [1]}))["pages"][0]["text"].as_str().unwrap().contains("Summary"), "the copy moved to the front");
    assert_eq!(pages(json!({"op": "delete", "pages": [1]})), 4);
    assert_eq!(pages(json!({"op": "insert_file", "path": "Garden plan.pdf", "at": 5})), 7);
    assert_eq!(pages(json!({"op": "insert_file", "path": "Board minutes.pdf", "at": 1, "pages": [2]})), 8);
    assert_eq!(w.ok("pages", json!({"doc": doc, "op": "extract", "pages": [6, 7, 8], "out": "garden again.pdf"})), json!({"path": "garden again.pdf"}));
    assert_eq!(w.ok("info", json!({"path": "garden again.pdf"}))["document"]["pages"], 3);
    coded(&w.err("pages", json!({"doc": doc, "op": "delete", "pages": [1, 2, 3, 4, 5, 6, 7, 8]})), "invalid");
    coded(&w.err("pages", json!({"doc": doc, "op": "rotate", "pages": [1], "angle": 45})), "invalid");
    coded(&w.err("pages", json!({"doc": doc, "op": "move", "pages": [99], "to": 1})), "invalid");
    coded(&w.err("pages", json!({"doc": doc, "op": "shuffle", "pages": [1]})), "invalid");
    let many: Vec<u64> = (1..=513).collect();
    coded(&w.err("pages", json!({"doc": doc, "op": "duplicate", "pages": many})), "too_large");
    assert_eq!(w.ok("state", json!({"doc": doc}))["pages"], 8);

    // A line, then a paragraph, edited in place.
    let lines = w.ok("lines", json!({"doc": doc, "page": 3}));
    let heading = lines["lines"].as_array().unwrap().iter().find(|l| l["text"] == "Summary").unwrap_or_else(|| panic!("{lines}"))["n"].clone();
    assert_eq!(w.ok("edit_text", json!({"doc": doc, "page": 3, "line": heading, "text": "Overview"})), json!({"edited": true}));
    let lines = w.ok("lines", json!({"doc": doc, "page": 3}));
    assert!(lines["lines"].as_array().unwrap().iter().any(|l| l["text"] == "Overview"), "{lines}");
    let para = lines["paragraphs"].as_array().unwrap().iter().find(|p| p["text"].as_str().unwrap().starts_with("Revenue reached")).unwrap()["n"].clone();
    w.ok("edit_text", json!({"doc": doc, "page": 3, "paragraph": para, "text": "Revenue reached 4.3 million dollars."}));
    assert!(w.ok("text", json!({"doc": doc, "pages": [3]}))["pages"][0]["text"].as_str().unwrap().contains("4.3 million"));
    coded(&w.err("edit_text", json!({"doc": doc, "page": 3, "line": 1, "paragraph": 1, "text": "x"})), "invalid");
    coded(&w.err("edit_text", json!({"doc": doc, "page": 3, "text": "x"})), "invalid");
    coded(&w.err("edit_text", json!({"doc": doc, "page": 3, "line": 999, "text": "x"})), "invalid");
}

#[test]
fn undo_and_redo_across_calls() {
    let w = World::new().samples();
    let doc = w.open("Board minutes.pdf");
    coded(&w.err("undo", json!({"doc": doc})), "invalid");
    w.ok("comment", json!({"doc": doc, "op": "add", "page": 1, "type": "note", "at": [40, 40], "text": "one"}));
    w.ok("pages", json!({"doc": doc, "op": "rotate", "pages": [2], "angle": 180}));
    assert_eq!(w.ok("state", json!({"doc": doc})), json!({"edited": true, "can_undo": true, "can_redo": false, "pages": 2}));
    assert_eq!(w.ok("undo", json!({"doc": doc})), json!({"edited": true, "can_undo": true, "can_redo": true}));
    assert_eq!(w.ok("info", json!({"doc": doc}))["pages"][1]["rotation"], 0, "the rotation was undone");
    assert_eq!(w.ok("undo", json!({"doc": doc})), json!({"edited": true, "can_undo": false, "can_redo": true}));
    assert_eq!(w.ok("comments", json!({"doc": doc}))["comments"], json!([]));
    assert_eq!(w.ok("redo", json!({"doc": doc}))["can_redo"], true);
    assert_eq!(w.ok("comments", json!({"doc": doc}))["comments"].as_array().unwrap().len(), 1);
    assert_eq!(w.ok("redo", json!({"doc": doc})), json!({"edited": true, "can_undo": true, "can_redo": false}));
    assert_eq!(w.ok("info", json!({"doc": doc}))["pages"][1]["rotation"], 180);
    coded(&w.err("redo", json!({"doc": doc})), "invalid");
    // A render after an undo shows the document as it is now.
    let before = w.ok("page", json!({"doc": doc, "page": 2, "dpi": 24}));
    let file = w.root.join(before["path"].as_str().unwrap());
    let upside_down = std::fs::read(&file).unwrap();
    w.ok("undo", json!({"doc": doc}));
    let after = w.ok("page", json!({"doc": doc, "page": 2, "dpi": 24}));
    assert_eq!(before["path"], after["path"]);
    assert_ne!(std::fs::read(&file).unwrap(), upside_down, "drawn again without the rotation");
    // Closing with edits asks first.
    coded(&w.err("close", json!({"doc": doc})), "unsaved");
    assert_eq!(w.ok("close", json!({"doc": doc, "discard": true})), json!({"closed": true}));
}

// ------------------------------------------------------------ saving and writing

#[test]
fn saving_incrementally_and_as_a_new_file() {
    let w = World::new().samples();
    let original = std::fs::read(w.root.join("Board minutes.pdf")).unwrap();
    let doc = w.open("Board minutes.pdf");
    w.ok("comment", json!({"doc": doc, "op": "add", "page": 1, "type": "note", "at": [40, 40], "text": "kept"}));
    let saved = w.ok("save", json!({"doc": doc}));
    assert_eq!(saved["path"], "Board minutes.pdf");
    let now = std::fs::read(w.root.join("Board minutes.pdf")).unwrap();
    assert_eq!(saved["bytes"], json!(now.len()));
    assert!(now.len() > original.len() && now.starts_with(&original), "an incremental update of the file");
    assert_eq!(w.ok("state", json!({"doc": doc}))["edited"], false);
    w.ok("close", json!({"doc": doc}));
    // The comment is in the file.
    let doc = w.open("Board minutes.pdf");
    assert_eq!(w.ok("comments", json!({"doc": doc}))["comments"][0]["text"], "kept");
    // Save as: a new file, which the document is from then on.
    w.ok("pages", json!({"doc": doc, "op": "rotate", "pages": [1], "angle": 90}));
    assert_eq!(w.ok("save", json!({"doc": doc, "path": "copies/minutes rotated.pdf"}))["path"], "copies/minutes rotated.pdf");
    assert_eq!(std::fs::read(w.root.join("Board minutes.pdf")).unwrap(), now, "the original is untouched");
    assert_eq!(w.ok("info", json!({"doc": doc}))["file"], "copies/minutes rotated.pdf");
    w.ok("pages", json!({"doc": doc, "op": "delete", "pages": [2]}));
    w.ok("save", json!({"doc": doc}));
    assert_eq!(w.ok("info", json!({"path": "copies/minutes rotated.pdf"}))["document"]["pages"], 1, "the next save went to the new file");
    assert_eq!(std::fs::read(w.root.join("Board minutes.pdf")).unwrap(), now);
    assert!(no_staging_left(&w.root));
}

/// A full save goes to a new file and never over an existing one, from the
/// foreground too; an incremental save over the document's own file needs
/// the foreground; extract and export write new files only.
#[test]
fn a_full_save_never_replaces_a_file() {
    let w = World::new().samples();
    w.put("taken.pdf", b"keep me");
    let doc = w.open("Board minutes.pdf");
    w.ok("comment", json!({"doc": doc, "op": "add", "page": 1, "type": "note", "at": [40, 40], "text": "x"}));
    for out in ["taken.pdf", "Board minutes.pdf", "Field guide.pdf"] {
        let e = w.err("save", json!({"doc": doc, "path": out}));
        coded(&e, "invalid");
        assert!(e.contains("already exists"), "{e}");
    }
    assert_eq!(std::fs::read(w.root.join("taken.pdf")).unwrap(), b"keep me");
    assert_eq!(w.ok("state", json!({"doc": doc}))["edited"], true, "still unsaved");
    coded(&w.err("pages", json!({"doc": doc, "op": "extract", "pages": [1], "out": "taken.pdf"})), "invalid");
    w.put("pages/Board minutes_page_2.png", b"theirs");
    coded(&w.err("export", json!({"doc": doc, "kind": "images", "out_dir": "pages", "dpi": 24})), "invalid");
    assert!(!w.root.join("pages/Board minutes_page_1.png").exists(), "all or nothing");
    assert_eq!(std::fs::read(w.root.join("pages/Board minutes_page_2.png")).unwrap(), b"theirs");
    coded(&w.err("export", json!({"doc": doc, "kind": "text", "out": "taken.pdf"})), "invalid");
    assert_eq!(std::fs::read(w.root.join("taken.pdf")).unwrap(), b"keep me");
    // A surface in the background may not replace the document's own file.
    let before = std::fs::read(w.root.join("Board minutes.pdf")).unwrap();
    let e = w.call_full(APP, "save", json!({"doc": doc}), false, 0).unwrap_err();
    coded(&e, "invalid");
    assert_eq!(std::fs::read(w.root.join("Board minutes.pdf")).unwrap(), before);
    assert!(w.call_full(APP, "save", json!({"doc": doc, "path": "from the background.pdf"}), false, 0).is_ok(), "a new file it may write");
    assert!(no_staging_left(&w.root));
}

#[test]
fn exports_and_merges_with_ranges() {
    let w = World::new().samples();
    let doc = w.open("Quarterly report.pdf");
    let images = w.ok("export", json!({"doc": doc, "kind": "images", "out_dir": "exports", "dpi": 24, "pages": [1, 3]}));
    assert_eq!(images, json!({"paths": ["exports/Quarterly report_page_1.png", "exports/Quarterly report_page_3.png"]}));
    for p in images["paths"].as_array().unwrap() {
        assert!(std::fs::read(w.root.join(p.as_str().unwrap())).unwrap().starts_with(b"\x89PNG"), "{p}");
    }
    let all = w.ok("export", json!({"doc": doc, "kind": "images", "out_dir": "every page", "dpi": 24}));
    assert_eq!(all["paths"].as_array().unwrap().len(), 4);
    assert_eq!(w.ok("export", json!({"doc": doc, "kind": "text", "out": "exports/report.txt"})), json!({"paths": ["exports/report.txt"]}));
    assert!(std::fs::read_to_string(w.root.join("exports/report.txt")).unwrap().contains("Regional results"));
    coded(&w.err("export", json!({"doc": doc, "kind": "images", "out_dir": "x", "dpi": 301})), "invalid");
    coded(&w.err("export", json!({"doc": doc, "kind": "images", "out_dir": "x", "format": "jpeg"})), "invalid");
    coded(&w.err("export", json!({"doc": doc, "kind": "office", "out": "x.docx"})), "invalid");

    // Combine with page ranges, a person's dash included.
    let merged = w.ok("merge", json!({"paths": [{"path": "Quarterly report.pdf", "pages": "1–2"}, "Board minutes.pdf", {"path": "Garden plan.pdf", "pages": "3"}], "out": "mix.pdf"}));
    assert_eq!(merged["out"], "mix.pdf");
    assert_eq!(w.ok("info", json!({"path": "mix.pdf"}))["document"]["pages"], 5);
    w.ok("merge", json!({"paths": [{"path": "Field guide.pdf", "pages": "all"}, {"path": "Board minutes.pdf"}], "out": "mix2.pdf"}));
    assert_eq!(w.ok("info", json!({"path": "mix2.pdf"}))["document"]["pages"], 9);
    coded(&w.err("merge", json!({"paths": [{"path": "Field guide.pdf", "pages": "1; rm"}, "Board minutes.pdf"], "out": "m3.pdf"})), "invalid");
    coded(&w.err("merge", json!({"paths": [{"path": "Field guide.pdf", "range": "1"}, "Board minutes.pdf"], "out": "m3.pdf"})), "invalid");
    assert!(no_staging_left(&w.root));
}

#[test]
fn paths_never_leave_the_storage() {
    let w = World::new().samples();
    std::fs::write(w.root.parent().unwrap().join("beside.pdf"), tiny_pdf("outside", "the storage")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(w.root.parent().unwrap(), w.root.join("up")).unwrap();
    let doc = w.open("Board minutes.pdf");
    for bad in ["../beside.pdf", "/etc/hosts", "a/../../beside.pdf", "up/beside.pdf", "./Board minutes.pdf"] {
        if bad.starts_with("up/") && !cfg!(unix) {
            continue;
        }
        coded(&w.err("open", json!({"path": bad})), "invalid");
        coded(&w.err("pages", json!({"doc": doc, "op": "insert_file", "path": bad, "at": 1})), "invalid");
        coded(&w.err("pages", json!({"doc": doc, "op": "extract", "pages": [1], "out": bad})), "invalid");
        coded(&w.err("save", json!({"doc": doc, "path": bad})), "invalid");
        coded(&w.err("export", json!({"doc": doc, "kind": "text", "out": bad})), "invalid");
        coded(&w.err("export", json!({"doc": doc, "kind": "images", "out_dir": bad, "dpi": 24})), "invalid");
        coded(&w.err("merge", json!({"paths": [{"path": bad}, "Board minutes.pdf"], "out": "m.pdf"})), "invalid");
    }
    let parent = w.root.parent().unwrap();
    let names: Vec<String> = std::fs::read_dir(parent).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names.len(), 2, "nothing landed beside the storage: {names:?}");
    assert_eq!(w.ok("state", json!({"doc": doc}))["pages"], 2);
}

// ------------------------------------------------------------ caps and the render cache

#[test]
fn dpi_and_megapixel_caps() {
    let w = World::new().samples();
    w.put("poster.pdf", &sized_pdf(5000.0, 5000.0));
    w.put("tiny.pdf", &tiny_pdf("a", "b"));
    let doc = w.open("tiny.pdf");
    for dpi in [23, 301, 600, 0] {
        coded(&w.err("page", json!({"doc": doc, "page": 1, "dpi": dpi})), "invalid");
    }
    coded(&w.err("page", json!({"doc": doc, "page": 1, "dpi": "high"})), "invalid");
    assert_eq!(w.ok("page", json!({"doc": doc, "page": 1}))["dpi"], 96, "96 by default");
    assert_eq!(w.ok("page", json!({"doc": doc, "page": 1, "dpi": 24}))["width"], 67);
    assert_eq!(w.ok("page", json!({"doc": doc, "page": 1, "dpi": 300}))["width"], 834);
    assert_eq!(w.ok("page", json!({"doc": doc, "page": 1, "dpi": 99.6}))["dpi"], 100, "rounded to a whole dpi");
    coded(&w.err("page", json!({"doc": doc, "page": 3})), "invalid");
    coded(&w.err("page", json!({"doc": doc, "page": 0})), "invalid");
    // 5000 pt a side: 16 MP is about 57 dpi at that size.
    let poster = w.open("poster.pdf");
    let e = w.err("page", json!({"doc": poster, "page": 1, "dpi": 96}));
    coded(&e, "too_large");
    assert!(e.contains("megapixels"), "{e}");
    assert_eq!(w.ok("page", json!({"doc": poster, "page": 1, "dpi": 57}))["width"], 3959);
    coded(&w.err("page", json!({"doc": poster, "page": 1, "dpi": 58})), "too_large");
    coded(&w.err("export", json!({"doc": poster, "kind": "images", "out_dir": "big", "dpi": 96})), "too_large");
    assert!(!w.root.join("big").exists());
    // find's limit.
    coded(&w.err("find", json!({"doc": doc, "query": "a", "limit": 501})), "invalid");
    coded(&w.err("find", json!({"doc": doc, "query": "a", "limit": 0})), "invalid");
    coded(&w.err("find", json!({"doc": doc, "query": "   "})), "invalid");
    w.ok("find", json!({"doc": doc, "query": "a", "limit": 500}));
    // 512 pages per call.
    let many: Vec<u64> = (1..=513).collect();
    coded(&w.err("text", json!({"doc": doc, "pages": many})), "too_large");
    coded(&w.err("export", json!({"doc": doc, "kind": "text", "out": "t.txt", "pages": many})), "too_large");
}

/// The cache keeps at most 64 renders: the oldest go first.
#[test]
fn the_render_cache_keeps_to_its_file_cap() {
    let w = World::new();
    w.put("tiny.pdf", &tiny_pdf("a", "b"));
    let doc = w.open("tiny.pdf");
    let mut written = Vec::new();
    for dpi in (24..=300).step_by(8) {
        for page in [1, 2] {
            written.push(w.ok("page", json!({"doc": doc, "page": page, "dpi": dpi}))["path"].as_str().unwrap().to_string());
        }
    }
    assert!(written.len() > cache::MAX_FILES, "{} renders", written.len());
    let kept: Vec<&String> = written.iter().filter(|p| w.root.join(p).exists()).collect();
    assert_eq!(kept.len(), cache::MAX_FILES);
    let gone = written.len() - cache::MAX_FILES;
    assert!(written[..gone].iter().all(|p| !w.root.join(p).exists()), "the oldest went first");
    assert!(written[gone..].iter().all(|p| w.root.join(p).exists()));
    // An evicted render is drawn again when asked for.
    let again = w.ok("page", json!({"doc": doc, "page": 1, "dpi": 24}));
    assert!(w.root.join(again["path"].as_str().unwrap()).exists());
}

/// The cache keeps at most 16 MiB of renders: the oldest go first.
#[test]
fn the_render_cache_keeps_to_its_byte_cap() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(".cache/pages");
    let old = root.join("aaaa");
    std::fs::create_dir_all(&old).unwrap();
    let mib = vec![0u8; 1 << 20];
    let base = std::time::SystemTime::now() - Duration::from_secs(3600);
    for i in 0..17u64 {
        let path = old.join(format!("{i}@96.png"));
        std::fs::write(&path, &mib).unwrap();
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(base + Duration::from_secs(i)).unwrap();
    }
    let keep = root.join("bbbb/1@96.png");
    cache::evict(&root, &keep, 1 << 20, cache::LIMITS);
    let left: Vec<u64> = (0..17).filter(|i| old.join(format!("{i}@96.png")).exists()).collect();
    assert_eq!(left, (2..17).collect::<Vec<_>>(), "15 MiB stay with the new 1 MiB render; the two oldest went");
    // The file cap counts too, under a test's small limits.
    cache::evict(&root, &keep, 0, cache::Limits { bytes: u64::MAX, files: 4 });
    let left: Vec<u64> = (0..17).filter(|i| old.join(format!("{i}@96.png")).exists()).collect();
    assert_eq!(left, vec![14, 15, 16]);
}

/// Before a write would fail for room, the render cache is cleared; when
/// that is not enough either, the write fails with `storage_full:`.
#[test]
fn the_render_cache_is_cleared_before_storage_full() {
    let roomy = World::new();
    roomy.put("Board minutes.pdf", &fixture::documents().into_iter().find(|(n, _)| *n == "Board minutes.pdf").unwrap().1);
    let doc = roomy.open("Board minutes.pdf");
    let mut cached = 0;
    for page in [1, 2] {
        let r = roomy.ok("page", json!({"doc": doc, "page": page, "dpi": 150}));
        cached += std::fs::metadata(roomy.root.join(r["path"].as_str().unwrap())).unwrap().len();
    }
    // The same storage, nearly full: 200 bytes left, besides the cache.
    let tight = World { _dir: tempfile::tempdir().unwrap(), root: roomy.root.clone(), areas: resolver(&roomy.root, Some(200)) };
    let out = tight.ok("export", json!({"doc": doc, "kind": "text", "out": "minutes.txt"}));
    assert_eq!(out["paths"][0], "minutes.txt");
    let text = std::fs::metadata(tight.root.join("minutes.txt")).unwrap().len();
    assert!(text > 200 && text <= 200 + cached, "the export needed the cache's room: {text} bytes");
    for page in [1, 2] {
        assert!(!tight.root.join(format!(".cache/pages/{doc}/{page}@150.png")).exists(), "the cache was cleared");
    }
    // Nothing left to clear: the write fails as storage_full.
    let e = tight.err("export", json!({"doc": doc, "kind": "text", "out": "again.txt"}));
    coded(&e, "storage_full");
    assert!(!tight.root.join("again.txt").exists());
    // A render that does not fit what is left fails the same way.
    let e = tight.err("page", json!({"doc": doc, "page": 1, "dpi": 150}));
    coded(&e, "storage_full");
}

/// A cache folder that is a link is refused: the cache never counts, trims
/// or writes files elsewhere in the storage.
#[cfg(unix)]
#[test]
fn the_render_cache_never_follows_a_link() {
    let w = World::new().samples();
    std::fs::create_dir_all(w.root.join("accounts/device/library")).unwrap();
    std::fs::copy(w.root.join("Board minutes.pdf"), w.root.join("accounts/device/library/Kept.pdf")).unwrap();
    std::os::unix::fs::symlink(w.root.join("accounts/device/library"), w.root.join(".cache")).unwrap();
    let doc = w.open("Board minutes.pdf");
    coded(&w.err("page", json!({"doc": doc, "page": 1})), "invalid");
    assert!(w.root.join("accounts/device/library/Kept.pdf").is_file(), "nothing in the library was trimmed");
    assert_eq!(std::fs::read_dir(w.root.join("accounts/device/library")).unwrap().count(), 1);
    // A document's own folder that is a link, likewise.
    std::fs::remove_file(w.root.join(".cache")).unwrap();
    std::fs::create_dir_all(w.root.join(".cache/pages")).unwrap();
    std::os::unix::fs::symlink(w.root.join("accounts/device/library"), w.root.join(format!(".cache/pages/{doc}"))).unwrap();
    coded(&w.err("page", json!({"doc": doc, "page": 1})), "invalid");
    assert_eq!(std::fs::read_dir(w.root.join("accounts/device/library")).unwrap().count(), 1);
}

// ------------------------------------------------------------ snippets

#[test]
fn snippets_mark_the_match_within_120_characters() {
    let text = "The quarter closed ahead of plan.\nSubscription revenue grew in every region, and revenue from support grew too.";
    let (chars, hits) = reading::occurrences(text, "REVENUE");
    assert_eq!(hits.len(), 2);
    let first = reading::snippet(&chars, hits[0].0, hits[0].1);
    assert!(first.contains("Subscription [[revenue]] grew"), "{first}");
    assert!(first.chars().count() <= reading::SNIPPET);
    let (chars, hits) = reading::occurrences(text, "plan. subscription");
    assert_eq!(hits.len(), 1, "a match across a line break, as the engine finds it");
    assert!(reading::snippet(&chars, hits[0].0, hits[0].1).contains("[[plan. Subscription]]"));
    let long: String = (0..400).map(|i| if i % 7 == 0 { ' ' } else { 'x' }).collect();
    let text = format!("{long} needle {long}");
    let (chars, hits) = reading::occurrences(&text, "needle");
    let s = reading::snippet(&chars, hits[0].0, hits[0].1);
    assert_eq!(s.chars().count(), reading::SNIPPET, "{s}");
    assert!(s.starts_with('…') && s.ends_with('…') && s.contains("[[needle]]"), "{s}");
    let huge: String = "word ".repeat(60);
    let (chars, hits) = reading::occurrences(&huge, &huge);
    let s = reading::snippet(&chars, hits[0].0, hits[0].1);
    assert!(s.chars().count() <= reading::SNIPPET && s.starts_with("[[word") && s.ends_with("…]]"), "{s}");
}

#[test]
fn pdf_dates_read_as_iso_8601() {
    assert_eq!(review::date(&json!("2026-10-10 14:30")), json!("2026-10-10T14:30"), "the engine's listing: the file's wall clock");
    assert_eq!(review::date(&json!("D:20261010143000+02'00'")), json!("2026-10-10T14:30:00+02:00"));
    assert_eq!(review::date(&json!("D:20261010143000Z")), json!("2026-10-10T14:30:00Z"));
    assert_eq!(review::date(&json!("D:20261010")), json!("2026-10-10T00:00:00Z"));
    assert_eq!(review::date(&json!("yesterday")), json!("yesterday"));
    assert_eq!(review::date(&Json::Null), Json::Null);
}

#[test]
fn every_v2_method_refuses_arguments_it_does_not_take() {
    let w = World::new().samples();
    let doc = w.open("Board minutes.pdf");
    for (method, args) in [
        ("open", json!({"path": "Board minutes.pdf", "password": "x"})),
        ("close", json!({"doc": doc, "force": true})),
        ("state", json!({"doc": doc, "x": 1})),
        ("page", json!({"doc": doc, "page": 1, "out": "elsewhere.png"})),
        ("find", json!({"doc": doc, "query": "x", "case_sensitive": true})),
        ("lines", json!({"doc": doc, "page": 1, "all": true})),
        ("fields", json!({"doc": doc, "page": 1})),
        ("fill", json!({"doc": doc, "values": {"a": "b"}, "script": "x"})),
        ("fill_sign", json!({"doc": doc, "page": 1, "kind": "check", "at": [1, 1], "author": "x"})),
        ("pages", json!({"doc": doc, "op": "extract", "pages": [1], "out": "x.pdf", "delete": true})),
        ("pages", json!({"doc": doc, "op": "rotate", "pages": [1], "angle": 90, "subset": "odd"})),
        ("edit_text", json!({"doc": doc, "page": 1, "line": 1, "text": "x", "font": "times"})),
        ("undo", json!({"doc": doc, "steps": 2})),
        ("save", json!({"doc": doc, "full": true})),
        ("export", json!({"doc": doc, "kind": "images", "out_dir": "x", "quality": 50})),
    ] {
        let e = w.err(method, args.clone());
        coded(&e, "invalid");
        assert!(e.contains("is not one of them"), "{method} {args}: {e}");
    }
    assert_eq!(w.ok("state", json!({"doc": doc}))["edited"], false, "nothing reached the document");
    assert!(!Path::new(&w.root.join("x.pdf")).exists());
}

/// What each method costs on a typical page, and a 100-page document's
/// first page, printed: engine work runs on the shell's UI thread (#399),
/// and SERVICE.md asks for well under 100 ms a typical page. Measure in
/// release:
///
/// cargo test --release --locked -p octosense-pdf-service --lib -- --ignored --nocapture timings
#[test]
#[ignore = "prints timings; run by hand, in release"]
fn timings_on_a_typical_page() {
    let w = World::new().samples();
    w.put("form.pdf", &scripted_form());
    let mut report = Vec::new();
    let mut time = |label: &str, method: &str, args: Json| -> Json {
        let started = Instant::now();
        let out = w.ok(method, args);
        report.push(format!("{label:<34} {:>8.1} ms", started.elapsed().as_secs_f64() * 1000.0));
        out
    };
    let doc = time("open (4 pages)", "open", json!({"path": "Quarterly report.pdf"}))["doc"].clone();
    time("page 1 at 96 dpi", "page", json!({"doc": doc, "page": 1}));
    time("page 1 at 96 dpi, cached", "page", json!({"doc": doc, "page": 1}));
    time("page 2 at 96 dpi", "page", json!({"doc": doc, "page": 2}));
    time("page 3 at 150 dpi", "page", json!({"doc": doc, "page": 3, "dpi": 150}));
    time("page 4 at 36 dpi (a thumbnail)", "page", json!({"doc": doc, "page": 4, "dpi": 36}));
    time("find, first", "find", json!({"doc": doc, "query": "revenue"}));
    time("find, again", "find", json!({"doc": doc, "query": "quarter"}));
    time("lines", "lines", json!({"doc": doc, "page": 2}));
    time("comment add (note)", "comment", json!({"doc": doc, "op": "add", "page": 2, "type": "note", "at": [60, 120], "text": "x"}));
    time("comments", "comments", json!({"doc": doc}));
    time("fill_sign (date)", "fill_sign", json!({"doc": doc, "page": 4, "kind": "date", "at": [60, 300]}));
    time("pages rotate", "pages", json!({"doc": doc, "op": "rotate", "pages": [1], "angle": 90}));
    time("page 1 at 96 dpi, after an edit", "page", json!({"doc": doc, "page": 1}));
    time("edit_text (a line)", "edit_text", json!({"doc": doc, "page": 2, "line": 1, "text": "Overview"}));
    time("undo", "undo", json!({"doc": doc}));
    time("redo", "redo", json!({"doc": doc}));
    time("state", "state", json!({"doc": doc}));
    time("save (incremental)", "save", json!({"doc": doc}));
    time("export text", "export", json!({"doc": doc, "kind": "text", "out": "report.txt"}));
    time("close", "close", json!({"doc": doc}));
    let form = time("open (a form)", "open", json!({"path": "form.pdf"}))["doc"].clone();
    time("fields", "fields", json!({"doc": form}));
    time("fill (2 fields)", "fill", json!({"doc": form, "values": {"a": "5", "v": "x"}}));
    // A 112-page document: 16 field guides combined.
    let guides: Vec<&str> = vec!["Field guide.pdf"; 16];
    w.ok("merge", json!({"paths": guides, "out": "guides.pdf"}));
    let big = time("open (112 pages)", "open", json!({"path": "guides.pdf"}))["doc"].clone();
    time("its page 1 at 96 dpi", "page", json!({"doc": big, "page": 1}));
    time("find in 112 pages, first", "find", json!({"doc": big, "query": "garden"}));
    eprintln!("pdf service timings:\n{}", report.join("\n"));
}
