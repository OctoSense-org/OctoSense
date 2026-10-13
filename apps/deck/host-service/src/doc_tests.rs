//! The open-document methods (Quick Deck v2, `apps/quickdeck/design/SERVICE.md`),
//! through [`serve_from`] as App Hub hands calls to the service, on decks the
//! engine itself writes and on hostile files built byte by byte
//! ([`crate::guard::fixtures`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use deckcraft_engine::cmd::file as engine_file;
use deckcraft_engine::render;
use deckcraft_engine::Session;
use deckcraft_model::text::RunKind;
use deckcraft_model::{Presentation, ShapeId};
use serde_json::{json, Value as Json};

use super::*;
use crate::guard::fixtures::{self as fx, Part};

const APP: &str = "os.quickdeck";

/// A resolver shaped like the shell's for an app's own requests: every call
/// works in `root`, the foreground may replace a file, within `quota`.
fn resolver(root: &Path, quota: Option<u64>) -> Slot {
    let slot = Slot::new();
    let root = root.to_path_buf();
    slot.set(Some(Arc::new(move |call: &ServiceCall| Ok(Area::new(&root, quota, call.may_prompt)))));
    slot
}

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
        // Tests run on many threads, and the engine's fonts load on one at a
        // time (`crate::warm`): the warm-up first, as the shell does it.
        crate::warm::ready();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("storage");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, quota);
        World { _dir: dir, root, areas }
    }

    /// The same storage, with another quota.
    fn tight(&self, quota: u64) -> Slot {
        resolver(&self.root, Some(quota))
    }

    fn put(&self, rel: &str, bytes: &[u8]) {
        let path = self.root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// A call of `app`, from the foreground (`may_prompt`) or not, from the
    /// isolate whose heap key is `isolate` (0: none).
    fn call_full(&self, app: &str, method: &str, args: Json, may_prompt: bool, isolate: usize) -> Result<Json, String> {
        let call = ServiceCall { app_id: app.into(), service: format!("deck.{method}"), args, from_sheet: false, may_prompt, host_dir: self.root.join(".host") };
        serve_from(&self.areas, &call, isolate)
    }

    fn call(&self, method: &str, args: Json) -> Result<Json, String> {
        self.call_full(APP, method, args, true, 0)
    }

    #[track_caller]
    fn ok(&self, method: &str, args: Json) -> Json {
        self.call(method, args.clone()).unwrap_or_else(|e| panic!("deck.{method} {args}: {e}"))
    }

    #[track_caller]
    fn err(&self, method: &str, args: Json) -> String {
        match self.call(method, args.clone()) {
            Ok(answer) => panic!("deck.{method} {args} answered {answer}"),
            Err(e) => e,
        }
    }

    /// Open `path`: its handle.
    #[track_caller]
    fn open(&self, path: &str) -> String {
        self.ok("open", json!({ "path": path }))["doc"].as_str().unwrap().to_string()
    }

    /// The sample deck (`sample(n)`) saved at `rel` and opened.
    fn sample_open(&self, rel: &str, n: usize) -> String {
        self.put(rel, &native(&sample(n)));
        self.open(rel)
    }

    /// The ids of the open deck's slides, in order.
    fn slide_ids(&self, doc: &str) -> Vec<u64> {
        self.ok("slides", json!({ "doc": doc }))["slides"].as_array().unwrap().iter().map(|s| s["id"].as_u64().unwrap()).collect()
    }

    /// Slide `slide`'s shapes, as `deck.slide` lists them.
    fn shapes(&self, doc: &str, slide: u64) -> Vec<Json> {
        self.ok("slide", json!({ "doc": doc, "slide": slide }))["shapes"].as_array().unwrap().clone()
    }

    /// The id of the first shape on `slide` whose `key` is `value`.
    fn shape_where(&self, doc: &str, slide: u64, key: &str, value: &str) -> u64 {
        let shapes = self.shapes(doc, slide);
        shapes.iter().find(|s| s[key] == value).unwrap_or_else(|| panic!("no shape with {key} {value}: {shapes:?}"))["id"].as_u64().unwrap()
    }

    /// The files under the storage, relative (sorted).
    fn files(&self, under: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut todo = vec![self.root.join(under)];
        while let Some(dir) = todo.pop() {
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    todo.push(p);
                } else {
                    out.push(p.strip_prefix(&self.root).unwrap().to_string_lossy().into_owned());
                }
            }
        }
        out.sort();
        out
    }
}

/// `error` starts with the stable code `code`.
#[track_caller]
fn coded(error: &str, code: &str) {
    assert!(error.starts_with(&format!("{code}: ")), "expected {code}: {error}");
}

/// A 12x8 RGB PNG (two colour bands), as the photo service's tests use.
fn png() -> Vec<u8> {
    const PNG: &str = "89504e470d0a1a0a0000000d494844520000000c000000080802000000428689a60000001d49444154789c6378616383866c725ea021063a2bb279d14310d15911005b9497817c6155610000000049454e44ae426082";
    (0..PNG.len()).step_by(2).map(|i| u8::from_str_radix(&PNG[i..i + 2], 16).unwrap()).collect()
}

/// A deck as the engine makes it: a title slide, then `n - 1` slides with a
/// title and two points. Slide 3 holds a 3 × 2 table, slide 4 a column
/// chart, slide 5 a picture and two shapes that build in on click.
fn sample(n: usize) -> Presentation {
    crate::warm::ready();
    let mut s = Session::new();
    let run = |s: &mut Session, id: &str, p: Json| s.execute(id, &p).unwrap_or_else(|e| panic!("{id}: {e}"));
    run(&mut s, "file.new", json!({ "blank": true }));
    run(&mut s, "slide.new", json!({ "layout": "title", "title": "Northwind Cooperative", "body": "Q3 2026 board update" }));
    for k in 1..n {
        run(&mut s, "slide.new", json!({ "layout": "titleAndContent", "title": format!("Slide {}", k + 1), "body": "First point\nSecond point" }));
        match k {
            2 => {
                run(&mut s, "insert.table", json!({ "rows": 3, "cols": 2, "rect": [100, 300, 400, 120], "data": [["Region", "Members"], ["North", "120"], ["South", "95"]] }));
            }
            3 => {
                run(&mut s, "insert.chart", json!({ "type": "column", "rect": [500, 150, 400, 300], "categories": ["Q1", "Q2", "Q3"], "series": [{ "name": "Revenue", "values": [3.1, 3.6, 4.2] }] }));
            }
            4 => {
                let data = deckcraft_engine::cmd::base64_encode(&png());
                run(&mut s, "insert.picture", json!({ "data": data, "name": "dot.png", "rect": [600, 200, 120, 80] }));
                let a = run(&mut s, "shape.insert", json!({ "preset": "rect", "rect": [50, 400, 100, 60], "text": "First" }))["id"].clone();
                let b = run(&mut s, "shape.insert", json!({ "preset": "ellipse", "rect": [200, 400, 100, 60], "text": "Second" }))["id"].clone();
                run(&mut s, "animation.add", json!({ "effect": "fade", "start": "onClick", "ids": [a] }));
                run(&mut s, "animation.add", json!({ "effect": "fade", "start": "onClick", "ids": [b] }));
            }
            _ => {}
        }
    }
    s.doc().unwrap().doc.as_ref().clone()
}

fn native(p: &Presentation) -> Vec<u8> {
    engine_file::save_bytes(p, "deckcraft").unwrap()
}

fn pptx(p: &Presentation) -> Vec<u8> {
    engine_file::save_bytes(p, "pptx").unwrap()
}

/// The text runs of a shape of the open document, as (text, bold).
fn runs(doc: &str, slide: u64, shape: u64) -> Vec<(String, bool)> {
    docs::with_open(doc, |d| {
        let deck = d.st().unwrap().doc.clone();
        let s = deck.slides.iter().find(|s| u64::from(s.id.0) == slide).unwrap();
        let sh = s.shape(ShapeId(shape as u32)).unwrap();
        sh.text
            .iter()
            .flat_map(|t| &t.paragraphs)
            .enumerate()
            .flat_map(|(i, p)| {
                let mut v: Vec<(String, bool)> = if i > 0 { vec![("\n".to_string(), false)] } else { vec![] };
                v.extend(p.runs.iter().map(|r| (if r.kind == RunKind::Break { "\u{b}".to_string() } else { r.text.clone() }, r.props.bold == Some(true))));
                v
            })
            .collect()
    })
    .unwrap()
}

/// How many undo steps the open document keeps.
fn undo_len(doc: &str) -> usize {
    docs::with_open(doc, |d| d.st().unwrap().history.undo.len()).unwrap()
}

// ------------------------------------------------------------ documents

#[test]
fn opening_a_native_deck_a_pptx_and_an_outline() {
    let w = World::new();
    let deck = sample(6);
    w.put("decks/board.deckcraft", &native(&deck));
    w.put("imports/board.pptx", &pptx(&deck));
    w.put("outline.txt", b"Plan\n\tShip in May\nNext steps\n\tBudget\n");
    let open = w.ok("open", json!({ "path": "decks/board.deckcraft" }));
    let doc = open["doc"].as_str().unwrap();
    assert!(!doc.is_empty() && doc.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "lowercase hex: {open}");
    assert_eq!(open["path"], "decks/board.deckcraft");
    assert_eq!(open["title"], "board", "the file's name when the deck has no title: {open}");
    assert_eq!(open["size"], json!([960.0, 540.0]));
    assert_eq!(open["slides"], 6);
    assert_eq!(open["theme"], "DeckCraft");
    let layouts = open["layouts"].as_array().unwrap();
    assert_eq!(layouts.len(), 11, "the eleven layouts: {open}");
    assert!(layouts.iter().all(|l| l["id"].is_u64() && l["name"].is_string() && l["kind"].is_string()), "{open}");
    assert_eq!(open["sections"], json!([]));
    assert_eq!((open["rev"].clone(), open["edited"].clone(), open["can_undo"].clone(), open["can_redo"].clone()), (json!(0), json!(false), json!(false), json!(false)));
    assert!(open["missing_fonts"].is_array(), "{open}");
    assert!(open.get("changed").is_none());
    // The same deck from a .pptx, and an outline.
    let p = w.ok("open", json!({ "path": "imports/board.pptx" }));
    assert_eq!(p["slides"], 6, "{p}");
    let o = w.ok("open", json!({ "path": "outline.txt" }));
    assert_eq!(o["slides"], 2, "{o}");
    // Nothing is in the open-document table for a closed deck.
    assert_eq!(w.ok("close", json!({ "doc": doc })), json!({ "closed": true }));
    coded(&w.err("state", json!({ "doc": doc })), "unknown_doc");
}

#[test]
fn creating_a_deck_writes_it_as_made() {
    let w = World::new();
    let made = w.ok("create", json!({ "out": "decks/new.deckcraft" }));
    assert_eq!((made["slides"].clone(), made["edited"].clone(), made["can_undo"].clone()), (json!(1), json!(false), json!(false)), "one title slide: {made}");
    assert_eq!(made["path"], "decks/new.deckcraft");
    assert!(std::fs::read(w.root.join("decks/new.deckcraft")).unwrap().starts_with(b"PK"), "written there as made");
    // From slides, with a theme, a size and notes.
    let made = w.ok(
        "create",
        json!({ "out": "decks/harbor.deckcraft", "theme": "harbor", "size": "4:3", "slides": [
            { "layout": "title", "title": "Field trip", "body": "Spring 2027" },
            { "title": "Plan", "body": "Bus at 8\nMuseum at 10", "notes": "Remind them about lunch" },
        ] }),
    );
    assert_eq!((made["theme"].clone(), made["size"].clone(), made["slides"].clone()), (json!("Harbor"), json!([720.0, 540.0]), json!(2)), "{made}");
    let doc = made["doc"].as_str().unwrap();
    let ids = w.slide_ids(doc);
    assert_eq!(w.ok("slide", json!({ "doc": doc, "slide": ids[1] }))["notes"], "Remind them about lunch");
    // From an outline.
    let made = w.ok("create", json!({ "out": "outline.deckcraft", "outline": "One\n\ta\nTwo\nThree" }));
    assert_eq!(made["slides"], 3, "{made}");
    // Refusals before anything is made.
    coded(&w.err("create", json!({ "out": "x.pptx" })), "invalid");
    coded(&w.err("create", json!({ "out": "decks/new.deckcraft" })), "invalid");
    coded(&w.err("create", json!({ "out": "y.deckcraft", "theme": "Neon" })), "invalid");
    coded(&w.err("create", json!({ "out": "y.deckcraft", "slides": [], })), "invalid");
    coded(&w.err("create", json!({ "out": "y.deckcraft", "slides": [{ "title": "a" }], "outline": "b" })), "invalid");
    let many: Vec<Json> = (0..201).map(|i| json!({ "title": format!("S{i}") })).collect();
    coded(&w.err("create", json!({ "out": "y.deckcraft", "slides": many })), "too_large");
    let outline = vec!["Slide"; 201].join("\n");
    coded(&w.err("create", json!({ "out": "y.deckcraft", "outline": outline })), "too_large");
    coded(&w.err("create", json!({ "out": "y.deckcraft", "size": [4000, 4000] })), "too_large");
    assert!(!w.root.join("y.deckcraft").exists(), "nothing written");
}

#[test]
fn state_close_and_unsaved_changes() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 3);
    let footer = json!({ "slide_numbers": false, "footer": false, "footer_text": "", "date": false, "hide_on_title": false });
    assert_eq!(
        w.ok("state", json!({ "doc": doc })),
        json!({ "rev": 0, "edited": false, "can_undo": false, "can_redo": false, "undo": null, "redo": null, "slides": 3,
                "theme": "DeckCraft", "color_scheme": "DeckCraft", "font_scheme": "DeckCraft", "footer": footer })
    );
    let ids = w.slide_ids(&doc);
    w.ok("notes", json!({ "doc": doc, "slide": ids[0], "text": "Welcome everyone" }));
    let state = w.ok("state", json!({ "doc": doc }));
    assert_eq!((state["rev"].clone(), state["edited"].clone(), state["undo"].clone()), (json!(1), json!(true), json!("Edit notes")), "{state}");
    coded(&w.err("close", json!({ "doc": doc })), "unsaved");
    assert_eq!(w.ok("close", json!({ "doc": doc, "discard": true })), json!({ "closed": true }));
}

// ------------------------------------------------------------ handles and the caps on open documents

#[test]
fn an_unknown_or_foreign_handle_is_unknown() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 3);
    let ids = w.slide_ids(&doc);
    let s = ids[1];
    // Another app, in the same storage.
    coded(&w.call_full("os.notes", "state", json!({ "doc": doc }), true, 0).unwrap_err(), "unknown_doc");
    // The same app in another storage scope.
    let other = World::new();
    for bad in [json!("0000"), json!("not a handle"), json!("../x"), json!("ABCDEF"), json!(7), json!(true), json!(["x"])] {
        coded(&w.err("state", json!({ "doc": bad })), "unknown_doc");
    }
    coded(&w.err("state", json!({})), "invalid");
    for (method, args) in [
        ("state", json!({ "doc": doc })),
        ("close", json!({ "doc": doc, "discard": true })),
        ("save", json!({ "doc": doc })),
        ("slides", json!({ "doc": doc })),
        ("slide", json!({ "doc": doc, "slide": s })),
        ("hit", json!({ "doc": doc, "slide": s, "at": [10, 10] })),
        ("view", json!({ "doc": doc, "slide": s, "width": 96 })),
        ("layouts", json!({ "doc": doc })),
        ("text", json!({ "doc": doc })),
        ("format_state", json!({ "doc": doc, "slide": s, "id": 1 })),
        ("slides_op", json!({ "doc": doc, "op": "new" })),
        ("notes", json!({ "doc": doc, "slide": s, "text": "x" })),
        ("transition", json!({ "doc": doc, "all": true, "kind": "fade" })),
        ("insert", json!({ "doc": doc, "slide": s, "kind": "text" })),
        ("set_text", json!({ "doc": doc, "slide": s, "id": 1, "text": "x" })),
        ("format", json!({ "doc": doc, "slide": s, "ids": [1], "bold": true })),
        ("shape", json!({ "doc": doc, "slide": s, "ids": [1], "op": "delete" })),
        ("picture", json!({ "doc": doc, "slide": s, "id": 1, "op": "reset" })),
        ("table", json!({ "doc": doc, "slide": s, "id": 1, "op": "style", "style": "none" })),
        ("chart", json!({ "doc": doc, "slide": s, "id": 1, "title": "x" })),
        ("design", json!({ "doc": doc, "theme": "Slate" })),
        ("undo", json!({ "doc": doc })),
        ("redo", json!({ "doc": doc })),
        ("show", json!({ "doc": doc })),
        ("export", json!({ "doc": doc, "kind": "outline", "out": "x.txt" })),
    ] {
        coded(&other.err(method, args.clone()), "unknown_doc");
        coded(&w.call_full("os.notes", method, args, true, 0).unwrap_err(), "unknown_doc");
    }
    // Still open for its own caller.
    assert_eq!(w.ok("state", json!({ "doc": doc }))["slides"], 3);
}

#[test]
fn the_ninth_open_of_a_caller_and_the_thirty_third_in_all_are_refused() {
    let w = World::new();
    w.put("a.deckcraft", &native(&sample(1)));
    let docs: Vec<String> = (0..docs::MAX_OPEN).map(|_| w.open("a.deckcraft")).collect();
    let mut unique = docs.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 8, "a handle per open: {docs:?}");
    coded(&w.err("open", json!({ "path": "a.deckcraft" })), "too_many_open");
    coded(&w.err("create", json!({ "out": "b.deckcraft" })), "too_many_open");
    assert!(!w.root.join("b.deckcraft").exists(), "refused before anything is written");
    // A cover is no open document.
    w.ok("cover", json!({ "path": "a.deckcraft" }));
    // The cap is per caller: other apps open theirs, up to 32 in all.
    let apps = ["os.one", "os.two", "os.three"];
    for app in apps {
        for _ in 0..docs::MAX_OPEN {
            w.call_full(app, "open", json!({ "path": "a.deckcraft" }), true, 0).unwrap();
        }
    }
    assert_eq!(docs::open_count(), docs::MAX_OPEN_ALL);
    let e = w.call_full("os.four", "open", json!({ "path": "a.deckcraft" }), true, 0).unwrap_err();
    coded(&e, "too_many_open");
    assert!(e.contains("as many decks as it can"), "{e}");
    w.ok("close", json!({ "doc": docs[3] }));
    w.call_full("os.four", "open", json!({ "path": "a.deckcraft" }), true, 0).unwrap();
}

/// A document opened from an app's isolate lives while that isolate's
/// storage scope does.
#[test]
fn a_document_goes_when_its_apps_storage_scope_does() {
    use makepad_widgets::*;
    let w = World::new();
    w.put("a.deckcraft", &native(&sample(2)));
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
    let opened = w.call_full(APP, "open", json!({ "path": "a.deckcraft" }), true, heap).unwrap();
    let doc = opened["doc"].clone();
    let unbound = w.open("a.deckcraft");
    assert_eq!(w.call_full(APP, "state", json!({ "doc": doc }), true, heap).unwrap()["slides"], 2);
    docs::sweep_at(Instant::now() + docs::IDLE + Duration::from_secs(60));
    assert_eq!(w.call_full(APP, "state", json!({ "doc": doc }), true, heap).unwrap()["slides"], 2, "idle is no reason to close a deck an open app holds");
    card.set_host_tag(&mut cx, None);
    coded(&w.call_full(APP, "state", json!({ "doc": doc }), true, 0).unwrap_err(), "unknown_doc");
    coded(&w.err("state", json!({ "doc": unbound })), "unknown_doc");
    assert_eq!(docs::open_count(), 0);
    // A scope that changes (another account's storage) is gone too.
    card.set_host_tag(&mut cx, Some(APP.into()));
    let opened = w.call_full(APP, "open", json!({ "path": "a.deckcraft" }), true, heap).unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    card.set_sandbox_dir(&mut cx, Some(elsewhere.path().to_path_buf()));
    coded(&w.call_full(APP, "state", json!({ "doc": opened["doc"] }), true, heap).unwrap_err(), "unknown_doc");
}

/// An isolate closing releases its documents at once, with their renders,
/// before a new isolate can get its heap key.
#[test]
fn a_closed_isolates_documents_go_before_its_key_is_reused() {
    use makepad_widgets::*;
    crate::register();
    let w = World::new();
    w.put("a.deckcraft", &native(&sample(2)));
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
    let from = |method: &str, args: Json| w.call_full(APP, method, args, true, heap);
    let renders = |doc: &Json| w.root.join(format!(".cache/slides/{}", doc.as_str().unwrap()));
    let old: Vec<Json> = (0..3).map(|_| from("open", json!({ "path": "a.deckcraft" })).unwrap()["doc"].clone()).collect();
    for doc in &old {
        let s = from("slides", json!({ "doc": doc })).unwrap()["slides"][0]["id"].clone();
        from("view", json!({ "doc": doc, "slide": s, "width": 96 })).unwrap();
        assert!(renders(doc).is_dir());
    }
    octosense_appstore::services::cancel_heap(heap.wrapping_add(64));
    assert_eq!(docs::open_count(), 3);
    octosense_appstore::services::cancel_heap(heap);
    assert_eq!(docs::open_count(), 0, "released by the listener");
    assert!(old.iter().all(|doc| !renders(doc).exists()), "their renders went with them");
    card.set_host_tag(&mut cx, Some(APP.into()));
    card.set_sandbox_dir(&mut cx, Some(w.root.clone()));
    let fresh: Vec<Json> = (0..docs::MAX_OPEN).map(|_| from("open", json!({ "path": "a.deckcraft" })).unwrap()["doc"].clone()).collect();
    coded(&from("open", json!({ "path": "a.deckcraft" })).unwrap_err(), "too_many_open");
    for doc in &old {
        coded(&from("state", json!({ "doc": doc })).unwrap_err(), "unknown_doc");
    }
    docs::while_busy(|| octosense_appstore::services::cancel_heap(heap));
    assert_eq!(docs::open_count(), 8, "queued, not lost");
    coded(&from("state", json!({ "doc": fresh[0] })).unwrap_err(), "unknown_doc");
    assert_eq!(docs::open_count(), 0);
}

#[test]
fn a_document_no_isolate_holds_goes_after_it_is_idle() {
    let w = World::new();
    let doc = w.sample_open("a.deckcraft", 1);
    docs::sweep_at(Instant::now() + docs::IDLE - Duration::from_secs(30));
    assert_eq!(w.ok("state", json!({ "doc": doc }))["slides"], 1, "not idle long enough");
    docs::sweep_at(Instant::now() + docs::IDLE + Duration::from_secs(1));
    coded(&w.err("state", json!({ "doc": doc })), "unknown_doc");
}

// ------------------------------------------------------------ the import guard on every path

/// Each hostile file, and the code it is refused with.
fn hostile() -> Vec<(&'static str, Vec<u8>, &'static str)> {
    let bomb = fx::package(vec![Part::deflated("ppt/slides/slide1.xml", &vec![0u8; 16 << 20])], "");
    let liar = fx::package(vec![Part { declared: Some(100), ..Part::deflated("ppt/slides/slide1.xml", &vec![b' '; 1 << 20]) }], "");
    let many = fx::zip(&(0..=guard::MAX_ENTRIES).map(|i| Part::stored(&format!("f{i}"), b"")).collect::<Vec<_>>());
    let nested = fx::package(vec![Part::stored("ppt/slides/inner.zip", &fx::zip(&[Part::stored("a", b"b")]))], "");
    // A chart's workbook that is a zip bomb: refused by its ratio.
    let workbook_bomb = fx::package(vec![Part::deflated("ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx", &[fx::zip(&[Part::stored("a", b"b")]), vec![0u8; 16 << 20]].concat())], "");
    // A zip whose end record is `end(directory size, directory offset)`.
    let ended = |parts: &[Part], end: &dyn Fn(u32, u32) -> Vec<u8>| {
        let l = fx::layout(parts);
        [l.locals.clone(), l.central.clone(), end(l.central.len() as u32, l.locals.len() as u32)].concat()
    };
    let types = || Part::deflated("[Content_Types].xml", b"<Types/>");
    // The review's proof of concept: one entry in all, two on this disk, the
    // second a movie the guard never saw; then a zip bomb as the second.
    let counted = ended(&[types(), Part::stored("ppt/media/evil.mp4", b"\0\0\0\x18ftypisom")], &|size, at| fx::end_record(2, 1, size, at, b""));
    let counted_bomb = ended(&[types(), Part::deflated("ppt/slides/slide1.xml", &vec![0u8; 16 << 20])], &|size, at| fx::end_record(2, 1, size, at, b""));
    // A directory shorter, or longer, than its end record declares.
    let short = ended(&[types(), Part::stored("ppt/slides/slide1.xml", b"<p:sld/>")], &|size, at| fx::end_record(2, 2, size - 10, at, b""));
    let long = ended(&[types(), Part::stored("ppt/slides/slide1.xml", b"<p:sld/>")], &|size, at| fx::end_record(2, 2, size + 10, at, b""));
    // Two end records; one hidden in the other's comment.
    let two = ended(&[types()], &|size, at| [fx::end_record(1, 1, size, at, b""), fx::end_record(1, 1, size, at, b"")].concat());
    let hidden = ended(&[types()], &|size, at| fx::end_record(1, 1, size, at, &[fx::end_record(1, 1, size, at, b""), b"pad".to_vec()].concat()));
    // Data before the zip: its offsets counting from the zip's start, or
    // from the file's.
    let prepended = [b"PK\x03\x04 prepended".to_vec(), fx::package(vec![], "")].concat();
    let shifted = {
        let junk = b"PK\x03\x04 prepended".to_vec();
        let l = fx::layout_from(&[types()], junk.len() as u32);
        let at = (junk.len() + l.locals.len()) as u32;
        [junk, l.locals.clone(), l.central.clone(), fx::end_record(1, 1, l.central.len() as u32, at, b"")].concat()
    };
    vec![
        ("bomb.pptx", bomb, "held"),
        ("liar.pptx", liar, "damaged"),
        ("many.pptx", many, "held"),
        ("nested.pptx", nested, "held"),
        ("workbook_bomb.pptx", workbook_bomb, "held"),
        ("counted.pptx", counted, "damaged"),
        ("counted_bomb.pptx", counted_bomb, "damaged"),
        ("short.pptx", short, "damaged"),
        ("long.pptx", long, "damaged"),
        ("two_ends.pptx", two, "damaged"),
        ("hidden_end.pptx", hidden, "damaged"),
        ("prepended.pptx", prepended, "damaged"),
        ("shifted.pptx", shifted, "damaged"),
        ("locked.pptx", fx::ole(&["EncryptionInfo", "EncryptedPackage"]), "protected"),
        ("legacy.ppt", fx::ole(&["PowerPoint Document", "Current User"]), "unsupported"),
    ]
}

/// Every refusal of the import guard comes before the engine reads a byte,
/// on every path that opens a file: `deck.open`, `deck.cover`, the v1 file
/// methods and through them the agent's `deck.info` and `deck.run`.
#[test]
fn the_import_guard_refuses_before_the_engine_runs_on_every_path() {
    let w = World::new();
    for (name, bytes, code) in hostile() {
        w.put(name, &bytes);
        let before = engine_reads();
        for (method, args) in [
            ("open", json!({ "path": name })),
            ("cover", json!({ "path": name })),
            ("info", json!({ "path": name })),
            ("text", json!({ "path": name })),
            ("render", json!({ "path": name, "out": "r.png" })),
            ("convert", json!({ "path": name, "out": "c.deckcraft" })),
            ("run", json!({ "path": name, "cmds": [] })),
        ] {
            let e = w.err(method, args);
            coded(&e, code);
            assert!(e.len() > code.len() + 10, "plain words follow: {e}");
        }
        assert_eq!(engine_reads(), before, "{name}: the engine read nothing");
    }
    assert_eq!(docs::open_count(), 0);
    assert!(!w.root.join("r.png").exists() && !w.root.join("c.deckcraft").exists());
    // An honest file passes the same paths, through the engine.
    w.put("ok.deckcraft", &native(&sample(2)));
    let before = engine_reads();
    w.ok("info", json!({ "path": "ok.deckcraft" }));
    w.ok("open", json!({ "path": "ok.deckcraft" }));
    assert_eq!(engine_reads(), before + 2);
    // A .pptx whose chart keeps its workbook as an embedded object (a zip
    // that neither the service nor the engine unzips) opens, on every path.
    let workbook = fx::zip(&[Part::deflated("[Content_Types].xml", b"<Types/>"), Part::deflated("xl/workbook.xml", b"<workbook/>")]);
    w.put("chart.pptx", &fx::with_parts(&pptx(&sample(4)), vec![Part::deflated("ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx", &workbook)]));
    let before = engine_reads();
    let opened = w.ok("open", json!({ "path": "chart.pptx" }));
    assert_eq!(opened["slides"], 4);
    w.ok("cover", json!({ "path": "chart.pptx" }));
    w.ok("info", json!({ "path": "chart.pptx" }));
    assert_eq!(engine_reads(), before + 3);
    // The engine's own refusal of what passed the guard.
    w.put("broken.deckcraft", &fx::native(vec![], "{not json"));
    coded(&w.err("open", json!({ "path": "broken.deckcraft" })), "damaged");
    w.put("notes.rtf", b"{\\rtf1 hello}");
    coded(&w.err("open", json!({ "path": "notes.rtf" })), "unsupported");
    w.put("paper.pdf", b"%PDF-1.7\n");
    coded(&w.err("open", json!({ "path": "paper.pdf" })), "unsupported");
    coded(&w.err("open", json!({ "path": "missing.pptx" })), "not_found");
}

/// The guard's verdict picks the engine's reader (`crate::engine_read`), a
/// step-for-step copy of what the pinned `open_presentation` does after
/// its sniff: on every format both read the very same deck, so a step that
/// function grows on a repin fails here.
#[test]
fn the_guards_reader_reads_as_open_presentation() {
    let deck = sample(5);
    // A package whose slide is wider than the engine keeps: the package
    // reader takes up to 20,000 points, and `repair` narrows it to 4,000,
    // so a reader that skipped `repair` would read another deck.
    let wide = fx::with_rewritten(&pptx(&deck), "ppt/presentation.xml", |xml| {
        let xml = String::from_utf8_lossy(xml);
        assert!(xml.contains("cx=\"12192000\""), "the 960-point slide width is there to widen");
        xml.replacen("cx=\"12192000\"", "cx=\"63500000\"", 1).into_bytes()
    });
    let raw = deckcraft_pptx::import(&wide).unwrap();
    assert_eq!(raw.slide_size.width, 5000.0, "the package reader keeps the width");
    assert_eq!(engine_file::open_presentation("wide.pptx", &wide).unwrap().slide_size.width, 4000.0, "repair narrows it");
    for (name, bytes, format) in [
        ("x.deckcraft", native(&deck), guard::Format::Native),
        ("x.pptx", pptx(&deck), guard::Format::Pptx),
        ("wide.pptx", wide, guard::Format::Pptx),
        ("x.txt", b"Title\n\tPoint\nSecond\n\tAnother point\n".to_vec(), guard::Format::Outline),
        ("x.md", b"".to_vec(), guard::Format::Outline),
    ] {
        assert_eq!(guard::check(name, &bytes), Ok(format), "{name}");
        assert_eq!(engine_read(name, &bytes, format).unwrap(), engine_file::open_presentation(name, &bytes).unwrap(), "{name}");
    }
}

/// A package with its content-type list last, past its first 4 KiB, as
/// LibreOffice writes every one: the engine's own sniff misses it, the
/// guard finds it in the directory, and it opens on every path, still
/// behind every bound of the guard.
#[test]
fn a_package_with_its_content_types_late_opens_on_every_path() {
    let w = World::new();
    let late = fx::with_last(&pptx(&sample(4)), "[Content_Types].xml", vec![]);
    let at = late.windows(19).position(|win| win == b"[Content_Types].xml").unwrap();
    assert!(at > 4096, "the content types sit {at} bytes in");
    assert!(!deckcraft_pptx::sniff(&late) && engine_file::open_presentation("late.pptx", &late).is_err(), "the engine's own entry point misses it");
    w.put("late.pptx", &late);
    let before = engine_reads();
    let opened = w.ok("open", json!({ "path": "late.pptx" }));
    assert_eq!(opened["slides"], 4, "{opened}");
    assert!(w.root.join(w.ok("cover", json!({ "path": "late.pptx" }))["path"].as_str().unwrap()).is_file());
    assert_eq!(w.ok("info", json!({ "path": "late.pptx" }))["slides"].as_array().map(Vec::len), Some(4));
    assert!(w.ok("text", json!({ "path": "late.pptx" }))["outline"].as_str().unwrap().contains("Northwind Cooperative"));
    w.ok("render", json!({ "path": "late.pptx", "out": "late.png" }));
    w.ok("convert", json!({ "path": "late.pptx", "out": "late.deckcraft" }));
    w.ok("run", json!({ "path": "late.pptx", "cmds": [] }));
    assert_eq!(engine_reads(), before + 7, "every path read it through the engine");
    // Still behind every bound: the same layout with a zip inside it, with
    // a zip bomb in it, or with its entry counts disagreeing never reaches
    // the engine.
    let nested = fx::with_last(&pptx(&sample(4)), "[Content_Types].xml", vec![Part::stored("ppt/slides/inner.zip", &fx::zip(&[Part::stored("a", b"b")]))]);
    let bomb = fx::with_last(&pptx(&sample(4)), "[Content_Types].xml", vec![Part::deflated("ppt/slides/slide9.xml", &vec![0u8; 16 << 20])]);
    let mut counted = late.clone();
    let end = counted.len() - 22;
    let total = u16::from_le_bytes([counted[end + 10], counted[end + 11]]);
    counted[end + 8..end + 10].copy_from_slice(&(total + 1).to_le_bytes());
    for (name, bytes, code) in [("late-nested.pptx", nested, "held"), ("late-bomb.pptx", bomb, "held"), ("late-counted.pptx", counted, "damaged")] {
        w.put(name, &bytes);
        let before = engine_reads();
        coded(&w.err("open", json!({ "path": name })), code);
        coded(&w.err("info", json!({ "path": name })), code);
        assert_eq!(engine_reads(), before, "{name}: the engine read nothing");
    }
}

/// An Office package whose main part is not a presentation (a document or
/// a workbook, written by Office or laid out as LibreOffice writes one,
/// its content types last) is `unsupported:`, not `damaged:`.
#[test]
fn an_office_document_that_is_not_a_presentation_is_unsupported() {
    let w = World::new();
    let rels = |target: &str| format!("<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"{target}\"/></Relationships>");
    let types = |main: &str, ct: &str| Part::deflated("[Content_Types].xml", format!("<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/{main}\" ContentType=\"{ct}\"/></Types>").as_bytes());
    let word = "<?xml version=\"1.0\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:r><w:t>Minutes</w:t></w:r></w:p></w:body></w:document>";
    let docx_type = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
    let book = "<?xml version=\"1.0\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheets/></workbook>";
    let xlsx_type = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
    // As Office writes them: the content types first.
    let office_docx = fx::zip(&[types("word/document.xml", docx_type), Part::deflated("_rels/.rels", rels("word/document.xml").as_bytes()), Part::deflated("word/document.xml", word.as_bytes())]);
    let office_xlsx = fx::zip(&[types("xl/workbook.xml", xlsx_type), Part::deflated("_rels/.rels", rels("xl/workbook.xml").as_bytes()), Part::deflated("xl/workbook.xml", book.as_bytes())]);
    // As LibreOffice lays one out: its content types last, past 4 KiB.
    let styles = format!("<?xml version=\"1.0\"?><w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">{}</w:styles>", "<w:style w:type=\"paragraph\" w:styleId=\"x\"/>".repeat(200));
    let libre_docx = fx::zip(&[
        Part::deflated("_rels/.rels", rels("word/document.xml").as_bytes()),
        Part::stored("word/styles.xml", styles.as_bytes()),
        Part::deflated("word/document.xml", word.as_bytes()),
        types("word/document.xml", docx_type),
    ]);
    assert!(libre_docx.windows(19).position(|win| win == b"[Content_Types].xml").unwrap() > 4096);
    for (name, bytes) in [("minutes.docx", office_docx), ("budget.xlsx", office_xlsx), ("minutes-lo.docx", libre_docx)] {
        w.put(name, &bytes);
        for method in ["open", "cover"] {
            let e = w.err(method, json!({ "path": name }));
            coded(&e, "unsupported");
            assert!(e.contains("not a presentation"), "{name} {method}: {e}");
        }
    }
    assert_eq!(docs::open_count(), 0);
}

/// What the inspector shows, read from the open document's model: the
/// deck's colour and font schemes and its footer (on opening and in its
/// state, so an undo shows too), a box's anchor and autofit, a chart's
/// options and a table's, and the engine's colour schemes with their
/// colours.
#[test]
fn the_inspector_reads_the_decks_design_and_its_objects() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 5);
    let ids = w.slide_ids(&doc);
    // The design: the default theme's own schemes, no footer.
    let opened = w.ok("state", json!({ "doc": doc }));
    assert_eq!((opened["theme"].clone(), opened["color_scheme"].clone(), opened["font_scheme"].clone()), (json!("DeckCraft"), json!("DeckCraft"), json!("DeckCraft")), "{opened}");
    let none = json!({ "slide_numbers": false, "footer": false, "footer_text": "", "date": false, "hide_on_title": false });
    assert_eq!(opened["footer"], none);
    let footer = json!({ "slide_numbers": true, "footer": true, "footer_text": "Board", "date": true, "hide_on_title": true });
    w.ok("design", json!({ "doc": doc, "colors": "Ocean", "fonts": "Classic", "footer": footer }));
    let state = w.ok("state", json!({ "doc": doc }));
    assert_eq!((state["color_scheme"].clone(), state["font_scheme"].clone(), state["footer"].clone()), (json!("Ocean"), json!("Classic"), footer.clone()), "{state}");
    w.ok("save", json!({ "doc": doc }));
    w.ok("close", json!({ "doc": doc }));
    let again = w.open("board.deckcraft");
    let reopened = w.ok("open", json!({ "path": "board.deckcraft" }));
    assert_eq!((reopened["color_scheme"].clone(), reopened["font_scheme"].clone(), reopened["footer"].clone()), (json!("Ocean"), json!("Classic"), footer), "{reopened}");
    w.ok("close", json!({ "doc": reopened["doc"] }));
    w.ok("design", json!({ "doc": again, "colors": "Berry" }));
    w.ok("undo", json!({ "doc": again }));
    assert_eq!(w.ok("state", json!({ "doc": again }))["color_scheme"], "Ocean", "an undo shows");
    // The engine's colour schemes, each with its twelve colours.
    let themes = w.ok("themes", json!({}));
    let ocean = themes["color_schemes"].as_array().unwrap().iter().find(|c| c["name"] == "Ocean").cloned().unwrap();
    let colors = ocean["colors"].as_array().unwrap();
    assert_eq!(colors.len(), 12);
    assert_eq!((colors[2].clone(), colors[4].clone()), (json!("#13293D"), json!("#006494")), "dk2 and accent1, in the engine's order");
    assert_eq!(themes["color_schemes"].as_array().unwrap().len(), deckcraft_model::theme::builtin_color_schemes().len());
    // A box's anchor and autofit as it draws them: a body placeholder
    // shrinks its text as its master says, a text box resizes to its text;
    // then as set.
    let body = w.shape_where(&again, ids[1], "text", "First point\nSecond point");
    let state = w.ok("format_state", json!({ "doc": again, "slide": ids[1], "id": body }));
    assert_eq!((state["anchor"].clone(), state["autofit"].clone()), (json!("top"), json!("shrink")), "{state}");
    let tb = w.ok("insert", json!({ "doc": again, "slide": ids[1], "kind": "text", "text": "Note" }))["id"].clone();
    assert_eq!(w.ok("format_state", json!({ "doc": again, "slide": ids[1], "id": tb }))["autofit"], "resize");
    w.ok("format", json!({ "doc": again, "slide": ids[1], "ids": [body], "anchor": "bottom", "autofit": "none" }));
    let state = w.ok("format_state", json!({ "doc": again, "slide": ids[1], "id": body }));
    assert_eq!((state["anchor"].clone(), state["autofit"].clone()), (json!("bottom"), json!("none")), "{state}");
    // A chart's options and a table's, as deck.chart and deck.table take them.
    let shapes = |slide: u64| w.ok("slide", json!({ "doc": again, "slide": slide }))["shapes"].as_array().unwrap().clone();
    let chart = shapes(ids[3]).into_iter().find(|s| s["chart"].is_object()).unwrap();
    let c = &chart["chart"];
    assert_eq!((c["legend"].clone(), c["data_labels"].clone(), c["gridlines"].clone(), c["palette"].clone()), (json!("b"), json!(false), json!(true), json!("colorful1")), "{c}");
    w.ok("chart", json!({ "doc": again, "slide": ids[3], "id": chart["id"], "legend": "none", "data_labels": true, "gridlines": false, "palette": "monochrome3" }));
    let c = shapes(ids[3]).into_iter().find(|s| s["chart"].is_object()).unwrap()["chart"].clone();
    assert_eq!((c["legend"].clone(), c["data_labels"].clone(), c["gridlines"].clone(), c["palette"].clone()), (json!("none"), json!(true), json!(false), json!("monochrome3")), "{c}");
    let table = shapes(ids[2]).into_iter().find(|s| s["table"].is_object()).unwrap();
    let defaults = json!({ "header_row": true, "total_row": false, "banded_rows": true, "first_column": false, "last_column": false, "banded_columns": false });
    assert_eq!(table["table"]["options"], defaults, "{table}");
    w.ok("table", json!({ "doc": again, "slide": ids[2], "id": table["id"], "op": "options", "header_row": false, "banded_columns": true }));
    let options = shapes(ids[2]).into_iter().find(|s| s["table"].is_object()).unwrap()["table"]["options"].clone();
    assert_eq!((options["header_row"].clone(), options["banded_columns"].clone(), options["banded_rows"].clone()), (json!(false), json!(true), json!(true)), "{options}");
}

// ------------------------------------------------------------ audio and video

/// A solid red picture, `n` pixels square.
fn red_png(n: u32) -> Vec<u8> {
    render::Image { width: n, height: n, pixels: [255u8, 0, 0, 255].repeat((n * n) as usize) }.to_png()
}

/// The straight colour of pixel (`x`, `y`) of `img`.
fn rgba(img: &render::Image, x: u32, y: u32) -> [u8; 4] {
    let px = img.to_straight();
    let i = ((y * img.width + x) * 4) as usize;
    [px[i], px[i + 1], px[i + 2], px[i + 3]]
}

/// Where `with_clips` puts its video on slide 1 (points), and its sound on
/// slide 3.
const VIDEO_BOX: [f64; 4] = [700.0, 420.0, 240.0, 100.0];
const SOUND_BOX: [f64; 4] = [700.0, 380.0, 96.0, 96.0];

/// A deck as deckcraft writes one, with a video on slide 1 (with a red
/// poster, unless `poster` is false) and a sound on slide 3, their bytes
/// `video` and `sound`: carried, never decoded.
fn with_clips(video: &[u8], sound: &[u8], poster: bool) -> Presentation {
    use deckcraft_model::{MediaClip, MediaId, MediaItem, Shape, ShapeKind, Xfrm};
    let mut p = sample(3);
    let clip = |p: &mut Presentation, name: &str, content_type: &str, data: Vec<u8>| {
        let id = MediaId(p.alloc_id());
        p.media.push(MediaItem { id, name: name.into(), content_type: content_type.into(), data: Arc::new(data), link: None });
        id
    };
    let film = clip(&mut p, "media1.mp4", "video/mp4", video.to_vec());
    let still = poster.then(|| clip(&mut p, "image9.png", "image/png", red_png(8)));
    let tone = clip(&mut p, "media2.m4a", "audio/mp4", sound.to_vec());
    let shape = |p: &mut Presentation, name: &str, [x, y, w, h]: [f64; 4], clip: MediaClip| Shape {
        id: ShapeId(p.alloc_id()),
        name: name.into(),
        xfrm: Some(Xfrm::new(x, y, w, h)),
        kind: ShapeKind::Media(clip),
        ..Default::default()
    };
    let video_shape = shape(&mut p, "Video 1", VIDEO_BOX, MediaClip { media: film, video: true, poster: still, volume: 1.0, ..Default::default() });
    let sound_shape = shape(&mut p, "Sound 1", SOUND_BOX, MediaClip { media: tone, video: false, volume: 1.0, ..Default::default() });
    Arc::make_mut(&mut p.slides[0]).shapes.push(video_shape);
    Arc::make_mut(&mut p.slides[2]).shapes.push(sound_shape);
    p
}

/// The deck the open document `doc` holds.
fn held_deck(doc: &str) -> Presentation {
    docs::with_open(doc, |d| d.session.doc().unwrap().doc.as_ref().clone()).unwrap()
}

/// Bytes that would trip a video decoder if one ever saw them: an MP4 whose
/// boxes claim 4 GiB and 16 EiB and a sample table of four billion samples.
fn hostile_movie() -> Vec<u8> {
    let mut b = b"\0\0\0\x18ftypisom\0\0\x02\0isomiso2".to_vec();
    b.extend(0xFFFF_FFFFu32.to_be_bytes());
    b.extend(b"moov");
    b.extend(0x14u32.to_be_bytes());
    b.extend(b"stsz\0\0\0\0");
    b.extend(0xFFFF_FFFFu32.to_be_bytes());
    b.extend(0xFFFF_FFFFu32.to_be_bytes());
    b.extend(1u32.to_be_bytes());
    b.extend(b"mdat");
    b.extend(u64::MAX.to_be_bytes());
    b.extend((0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8));
    b
}

/// The same for a sound: a WAV of 65,535 channels whose data claims 4 GiB.
fn hostile_sound() -> Vec<u8> {
    let mut b = b"RIFF\xFF\xFF\xFF\xFFWAVEfmt \x10\0\0\0\x01\0\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x20\0data".to_vec();
    b.extend(0xFFFF_FFFFu32.to_be_bytes());
    b.extend((0..2048u32).map(|i| (i.wrapping_mul(40_503) >> 8) as u8));
    b
}

/// A deck holding a video and a sound opens from a package and from a
/// native file, draws the video's poster (and a film glyph without one, a
/// speaker glyph for the sound), saves natively and opens again, and
/// exports a package with the clips' bytes unchanged, and a PDF.
#[test]
fn a_deck_with_video_and_sound_opens_draws_saves_and_exports() {
    let w = World::new();
    let video = b"\0\0\0\x18ftypisom\0\0\x02\0isomiso2mp41 a small movie".to_vec();
    let sound = b"\0\0\0\x18ftypM4A \0\0\0\0M4A isom a small sound".to_vec();
    let deck = with_clips(&video, &sound, true);
    w.put("clips.pptx", &pptx(&deck));
    w.put("clips.deckcraft", &native(&deck));
    let carries = |d: &Presentation| d.media.iter().any(|m| *m.data == video) && d.media.iter().any(|m| *m.data == sound);
    let [vx, vy, vw, vh] = VIDEO_BOX;
    let centre = ((vx + vw / 2.0) as u32, (vy + vh / 2.0) as u32);
    for path in ["clips.pptx", "clips.deckcraft"] {
        let doc = w.open(path);
        let held = held_deck(&doc);
        assert!(carries(&held), "{path}: the clips come through as bytes");
        // The video's poster, drawn where the video sits.
        let img = render::render_slide(&held, 0, &render::RenderOpts { scale: 1.0, ..Default::default() });
        let [r, g, b, _] = rgba(&img, centre.0, centre.1);
        assert!(r > 200 && g < 60 && b < 60, "{path}: the poster at the video's centre is red, not {:?}", (r, g, b));
        let ids = w.slide_ids(&doc);
        for step in [None, Some(0)] {
            let mut args = json!({ "doc": doc, "slide": ids[0], "width": 960 });
            if let Some(n) = step {
                args["step"] = json!(n);
            }
            w.ok("view", args);
        }
        w.ok("cover", json!({ "path": path }));
        // Saved natively, it opens again with the same bytes.
        let saved = format!("saved-{}.deckcraft", path.replace('.', "-"));
        w.ok("save", json!({ "doc": doc, "path": saved }));
        let again = w.open(&saved);
        assert!(carries(&held_deck(&again)), "{path}: saved and opened again");
        // A package export carries the clips' bytes unchanged; a PDF draws.
        let out = format!("out-{}.pptx", path.replace('.', "-"));
        w.ok("export", json!({ "doc": doc, "kind": "pptx", "out": out }));
        let exported = std::fs::read(w.root.join(&out)).unwrap();
        assert!(carries(&deckcraft_pptx::import(&exported).unwrap()), "{path}: the package export keeps the bytes");
        assert!(carries(&held_deck(&w.open(&out))), "{path}: and opens again");
        let pdf = w.ok("export", json!({ "doc": doc, "kind": "pdf", "out": format!("out-{}.pdf", path.replace('.', "-")) }));
        assert_eq!(pdf["pages"], 3, "{pdf}");
        for d in [doc, again] {
            w.ok("close", json!({ "doc": d, "discard": true }));
        }
    }
    // Without a poster, the video is a dark frame with a film glyph; the
    // sound, a speaker glyph.
    let bare = with_clips(&video, &sound, false);
    let img = render::render_slide(&bare, 0, &render::RenderOpts { scale: 1.0, ..Default::default() });
    let [r, g, b, _] = rgba(&img, (vx + 4.0) as u32, (vy + 4.0) as u32);
    assert!(r < 60 && g < 60 && b < 60, "the dark frame behind the glyph, not {:?}", (r, g, b));
    let mut silent = bare.clone();
    Arc::make_mut(&mut silent.slides[2]).shapes.pop();
    let opts = render::RenderOpts { scale: 1.0, ..Default::default() };
    assert_ne!(render::render_slide(&bare, 2, &opts).pixels, render::render_slide(&silent, 2, &opts).pixels, "the speaker glyph is drawn");
}

/// Clip bytes that would trip a decoder (`hostile_movie`, `hostile_sound`)
/// ride along on every path: opened, inspected, drawn with and without a
/// build step, presented, saved, exported every way, edited, undone and
/// redone. That proves nothing the service runs decodes them. The four
/// commands that would decode them stay held through the door.
#[test]
fn hostile_clip_bytes_are_carried_on_every_path_and_never_decoded() {
    let w = World::new();
    let (movie, noise) = (hostile_movie(), hostile_sound());
    let deck = with_clips(&movie, &noise, false);
    w.put("hostile.pptx", &pptx(&deck));
    w.put("hostile.deckcraft", &native(&deck));
    for path in ["hostile.pptx", "hostile.deckcraft"] {
        // The file methods, the agent's tools among them.
        w.ok("info", json!({ "path": path }));
        w.ok("text", json!({ "path": path }));
        for out in ["r.png", "c.pptx", "c.deckcraft", "c.txt", "c.pdf"] {
            let (method, args) = if out.ends_with(".png") { ("render", json!({ "path": path, "out": format!("{path}-{out}") })) } else { ("convert", json!({ "path": path, "out": format!("{path}-{out}") })) };
            w.ok(method, args);
        }
        w.ok("run", json!({ "path": path, "cmds": [{ "id": "document.inspect" }, { "id": "slide.inspect", "params": { "index": 0 } }] }));
        for id in ["media.info", "media.posterFrame", "insert.video", "insert.audio"] {
            let e = w.err("run", json!({ "path": path, "cmds": [{ "id": "slide.go", "params": { "index": 0 } }, { "id": id }] }));
            assert!(e.contains(&format!("`{id}` is held back")), "{id}: {e}");
        }
        // The open document, on every method that reads, draws or writes.
        let doc = w.open(path);
        let ids = w.slide_ids(&doc);
        w.ok("slides", json!({ "doc": doc }));
        for (i, s) in ids.iter().enumerate() {
            w.ok("slide", json!({ "doc": doc, "slide": s }));
            w.ok("view", json!({ "doc": doc, "slide": s, "width": 640 }));
            w.ok("view", json!({ "doc": doc, "slide": s, "width": 640, "step": 0 }));
            w.ok("view", json!({ "doc": doc, "slide": s, "width": 320, "edit": true }));
            assert!(w.ok("hit", json!({ "doc": doc, "slide": s, "at": [10, 10] })).is_object(), "slide {i}");
        }
        w.ok("show", json!({ "doc": doc }));
        w.ok("layouts", json!({ "doc": doc }));
        w.ok("cover", json!({ "path": path }));
        w.ok("notes", json!({ "doc": doc, "slide": ids[0], "text": "Hostile bytes ride along" }));
        w.ok("slides_op", json!({ "doc": doc, "op": "duplicate", "slides": [ids[0]] }));
        w.ok("undo", json!({ "doc": doc }));
        w.ok("undo", json!({ "doc": doc }));
        w.ok("redo", json!({ "doc": doc }));
        let stem = path.replace('.', "-");
        w.ok("save", json!({ "doc": doc, "path": format!("{stem}-saved.deckcraft") }));
        w.ok("export", json!({ "doc": doc, "kind": "pptx", "out": format!("{stem}.pptx") }));
        w.ok("export", json!({ "doc": doc, "kind": "pdf", "out": format!("{stem}.pdf") }));
        w.ok("export", json!({ "doc": doc, "kind": "images", "out_dir": format!("{stem}-pictures"), "width": 320 }));
        w.ok("export", json!({ "doc": doc, "kind": "outline", "out": format!("{stem}.txt") }));
        let held = held_deck(&doc);
        assert!(held.media.iter().any(|m| *m.data == movie) && held.media.iter().any(|m| *m.data == noise), "{path}: carried unchanged");
        w.ok("close", json!({ "doc": doc }));
    }
}

/// A deck whose binary parts declare just past the door's 128 MiB of media,
/// each part inside every other bound of the guard, is `too_large:` on all
/// seven paths before the engine reads a byte (the guard's pre-read bound,
/// in the session ceiling's words), whether its media is a native deck's or
/// a package's parts. One just under opens on every path.
#[test]
fn a_deck_past_the_media_ceiling_is_too_large_before_the_engine_reads() {
    use deckcraft_model::{MediaId, MediaItem};
    let w = World::new();
    let item = guard::RATIO_GRACE as usize;
    let filler = |k: usize| {
        let mut data = vec![0u8; item];
        data[..8].copy_from_slice(&(k as u64).to_le_bytes());
        data
    };
    let native_with = |n: usize| {
        let mut p = sample(1);
        for k in 0..n {
            let id = MediaId(p.alloc_id());
            p.media.push(MediaItem { id, name: format!("filler{k}.bin"), content_type: "application/octet-stream".into(), data: Arc::new(filler(k)), link: None });
        }
        native(&p)
    };
    let over = MAX_RUN_MEDIA as usize / item + 1;
    assert!((over * item) as u64 > MAX_RUN_MEDIA && ((over - 1) * item) as u64 <= MAX_RUN_MEDIA, "just over, and just under");
    let package = fx::package((0..over).map(|k| Part::deflated(&format!("ppt/media/filler{k}.bin"), &filler(k))).collect(), "");
    let paths = |name: &str| {
        vec![
            ("open", json!({ "path": name })),
            ("cover", json!({ "path": name })),
            ("info", json!({ "path": name })),
            ("text", json!({ "path": name })),
            ("render", json!({ "path": name, "out": format!("{name}.png") })),
            ("convert", json!({ "path": name, "out": format!("{name}.txt") })),
            ("run", json!({ "path": name, "cmds": [] })),
        ]
    };
    for (name, bytes) in [("over.deckcraft", native_with(over)), ("over.pptx", package)] {
        assert!(bytes.len() < 4 << 20, "{name}: a small file, {} bytes", bytes.len());
        w.put(name, &bytes);
        let before = engine_reads();
        for (method, args) in paths(name) {
            let e = w.err(method, args);
            coded(&e, "too_large");
            assert!(e.contains(&format!("bytes of media, more than the {MAX_RUN_MEDIA} the door allows")), "{name} {method}: {e}");
        }
        assert_eq!(engine_reads(), before, "{name}: refused before the engine read a byte");
    }
    w.put("under.deckcraft", &native_with(over - 1));
    let before = engine_reads();
    for (method, args) in paths("under.deckcraft") {
        w.ok(method, args);
    }
    assert_eq!(engine_reads(), before + 7, "just under: every path read it");
    assert_eq!(docs::open_count(), 1);
    // After the engine reads, every path also holds the deck's own media to
    // the same ceiling, in the same words (`media_ceiling`).
    let deck = |n: usize| {
        let mut p = sample(1);
        for k in 0..n {
            let id = MediaId(p.alloc_id());
            p.media.push(MediaItem { id, name: format!("filler{k}.bin"), content_type: "application/octet-stream".into(), data: Arc::new(filler(k)), link: None });
        }
        p
    };
    let e = media_ceiling(&deck(over)).unwrap_err();
    coded(&e, "too_large");
    assert!(e.contains("bytes of media"), "{e}");
    assert_eq!(media_ceiling(&deck(over - 1)), Ok(()));
}

/// The import guard passes what the engine itself writes, at the sizes the
/// service allows: a native deck's `presentation.json` repeats itself from
/// slide to slide, so its compression ratio is the one to watch.
#[test]
fn the_import_guard_passes_the_engines_own_large_decks() {
    let worst = |bytes: &[u8]| -> f64 {
        // Each entry's declared size over its compressed size, from the
        // central directory the guard reads.
        let end = (0..bytes.len() - 21).rev().find(|i| bytes[*i..*i + 4] == [0x50, 0x4b, 0x05, 0x06]).unwrap();
        let n = u16::from_le_bytes([bytes[end + 10], bytes[end + 11]]) as usize;
        let mut at = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as usize;
        let mut worst: f64 = 0.0;
        for _ in 0..n {
            let c = u32::from_le_bytes(bytes[at + 20..at + 24].try_into().unwrap()) as f64;
            let u = u32::from_le_bytes(bytes[at + 24..at + 28].try_into().unwrap()) as f64;
            if u > guard::RATIO_GRACE as f64 {
                worst = worst.max(u / c.max(1.0));
            }
            let skip = 46 + u16::from_le_bytes([bytes[at + 28], bytes[at + 29]]) as usize + u16::from_le_bytes([bytes[at + 30], bytes[at + 31]]) as usize + u16::from_le_bytes([bytes[at + 32], bytes[at + 33]]) as usize;
            at += skip;
        }
        worst
    };
    // Two hundred slides as the service makes them, and five hundred alike
    // from an outline (the session's ceiling).
    let mut s = Session::new();
    s.execute("file.new", &json!({ "blank": true })).unwrap();
    s.execute("slide.fromOutline", &json!({ "text": vec!["Slide\n\tA point\n\tAnother point"; 200].join("\n") })).unwrap();
    s.execute("slide.fromOutline", &json!({ "text": vec!["Slide\n\tA point\n\tAnother point"; 200].join("\n") })).unwrap();
    s.execute("slide.fromOutline", &json!({ "text": vec!["Slide\n\tA point\n\tAnother point"; 100].join("\n") })).unwrap();
    let alike = s.doc().unwrap().doc.as_ref().clone();
    assert_eq!(alike.slides.len(), 500);
    for (name, deck) in [("sample", sample(200)), ("alike", alike)] {
        for (ext, bytes) in [("deckcraft", native(&deck)), ("pptx", pptx(&deck))] {
            let ratio = worst(&bytes);
            eprintln!("{name}.{ext}: {} bytes, worst ratio past the grace {ratio:.1} to 1", bytes.len());
            assert!(guard::check(&format!("{name}.{ext}"), &bytes).is_ok(), "{name}.{ext}: {:?}", guard::check(&format!("{name}.{ext}"), &bytes));
        }
    }
}

#[test]
fn a_file_or_a_deck_over_the_caps_is_too_large() {
    let w = World::new();
    let big = std::fs::File::create(w.root.join("big.pptx")).unwrap();
    big.set_len(MAX_DECK_BYTES + 1).unwrap();
    coded(&w.err("open", json!({ "path": "big.pptx" })), "too_large");
    coded(&w.err("cover", json!({ "path": "big.pptx" })), "too_large");
    // A deck past the session's 500 slides.
    let mut deck = sample(1);
    let first = deck.slides[0].clone();
    for k in 0..crate::MAX_RUN_SLIDES {
        let mut s = (*first).clone();
        s.id = deckcraft_model::SlideId(100_000 + k as u32);
        deck.slides.push(Arc::new(s));
    }
    w.put("huge.deckcraft", &native(&deck));
    let e = w.err("open", json!({ "path": "huge.deckcraft" }));
    coded(&e, "too_large");
    assert!(e.contains("501 slides"), "{e}");
    coded(&w.err("cover", json!({ "path": "huge.deckcraft" })), "too_large");
}

// ------------------------------------------------------------ reading and drawing

#[test]
fn slides_and_a_slides_shapes() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 6);
    let slides = w.ok("slides", json!({ "doc": doc }));
    let list = slides["slides"].as_array().unwrap();
    assert_eq!(list.len(), 6);
    assert_eq!(list[0]["title"], "Northwind Cooperative");
    assert_eq!((list[1]["index"].clone(), list[1]["hidden"].clone(), list[1]["comments"].clone()), (json!(1), json!(false), json!(0)));
    assert_eq!(list[4]["builds"], 2, "two click builds: {}", list[4]);
    assert_eq!(list[1]["builds"], 0);
    assert!(list.iter().all(|s| s["layout"].is_u64()), "a layout id: {slides}");
    assert_eq!(slides["size"], json!([960.0, 540.0]));
    // One slide's shapes: the title and body placeholders with their
    // paragraphs, a table, a chart, a picture.
    let ids: Vec<u64> = list.iter().map(|s| s["id"].as_u64().unwrap()).collect();
    let s1 = w.ok("slide", json!({ "doc": doc, "slide": ids[1] }));
    assert_eq!((s1["id"].clone(), s1["index"].clone(), s1["hidden"].clone()), (json!(ids[1]), json!(1), json!(false)), "{s1}");
    let body = s1["shapes"].as_array().unwrap().iter().find(|s| s["text"] == "First point\nSecond point").unwrap_or_else(|| panic!("{s1}"));
    assert_eq!(body["paragraphs"], json!([{ "text": "First point", "level": 0 }, { "text": "Second point", "level": 0 }]));
    assert_eq!(body["box"].as_array().unwrap().len(), 4);
    let table = w.shapes(&doc, ids[2]).into_iter().find(|s| s["kind"] == "table").unwrap();
    assert_eq!((table["table"]["rows"].clone(), table["table"]["cols"].clone()), (json!(3), json!(2)), "{table}");
    let chart = w.shapes(&doc, ids[3]).into_iter().find(|s| s["kind"] == "chart").unwrap();
    assert_eq!(chart["chart"]["categories"], json!(["Q1", "Q2", "Q3"]), "{chart}");
    let picture = w.shapes(&doc, ids[4]).into_iter().find(|s| s["kind"] == "picture").unwrap();
    assert_eq!(picture["picture"]["crop"], json!([0.0, 0.0, 0.0, 0.0]), "{picture}");
    assert!(w.shapes(&doc, ids[4]).iter().any(|s| s["line"].is_object()), "an outline for a shape");
    coded(&w.err("slide", json!({ "doc": doc, "slide": 999_999 })), "unknown_id");
    // The outline text.
    let text = w.ok("text", json!({ "doc": doc }));
    assert!(text["outline"].as_str().unwrap().starts_with("Northwind Cooperative\n"), "{text}");
}

#[test]
fn hit_testing_a_shape_its_text_and_its_handles() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 6);
    let ids = w.slide_ids(&doc);
    let rect = w.shape_where(&doc, ids[4], "text", "First");
    // The rectangle at [50, 400, 100, 60]: inside it, and nothing at a
    // corner of the slide.
    let hit = w.ok("hit", json!({ "doc": doc, "slide": ids[4], "at": [100, 430] }));
    assert_eq!(hit["hit"]["id"], json!(rect), "{hit}");
    assert!(hit["hit"]["kind"] == "shape" || hit["hit"]["kind"] == "text", "{hit}");
    assert_eq!(w.ok("hit", json!({ "doc": doc, "slide": ids[4], "at": [955, 535] })), json!({ "hit": null }));
    // With it selected, its bottom-right corner is a resize handle.
    let handle = w.ok("hit", json!({ "doc": doc, "slide": ids[4], "at": [150, 460], "selected": [rect] }));
    assert_eq!((handle["hit"]["kind"].clone(), handle["hit"]["id"].clone()), (json!("resize"), json!(rect)), "{handle}");
    assert!(handle["hit"]["handle"].is_u64());
    coded(&w.err("hit", json!({ "doc": doc, "slide": ids[4], "at": [1, 1], "selected": [999_999] })), "unknown_id");
    // An empty selection is no selection: the corner is the shape again,
    // not a handle, and a tap in it still finds the shape.
    let corner = json!({ "doc": doc, "slide": ids[4], "at": [150, 460] });
    let unselected = w.ok("hit", corner.clone());
    assert_ne!(unselected["hit"]["kind"], "resize", "{unselected}");
    let mut empty = corner;
    empty["selected"] = json!([]);
    assert_eq!(w.ok("hit", empty), unselected);
    let tap = w.ok("hit", json!({ "doc": doc, "slide": ids[4], "at": [100, 430], "selected": [] }));
    assert_eq!(tap["hit"]["id"], json!(rect), "{tap}");
    assert_eq!(w.ok("state", json!({ "doc": doc }))["edited"], false, "a hit changes nothing");
}

#[test]
fn viewing_a_slide_into_the_render_cache() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 6);
    let ids = w.slide_ids(&doc);
    let v = w.ok("view", json!({ "doc": doc, "slide": ids[1], "width": 480 }));
    let path = v["path"].as_str().unwrap().to_string();
    assert!(path.starts_with(&format!(".cache/slides/{doc}/")), "{v}");
    assert_eq!((v["width"].clone(), v["height"].clone(), v["rev"].clone()), (json!(480), json!(270), json!(0)), "{v}");
    let file = w.root.join(&path);
    let first = std::fs::read(&file).unwrap();
    assert!(first.starts_with(b"\x89PNG"));
    let written = std::fs::metadata(&file).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(w.ok("view", json!({ "doc": doc, "slide": ids[1], "width": 480 })), v, "again from the cache");
    assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), written, "not drawn again");
    // The editor view, and the same slide with the body's text left out.
    let body = w.shape_where(&doc, ids[1], "text", "First point\nSecond point");
    let edit = w.ok("view", json!({ "doc": doc, "slide": ids[1], "width": 480, "edit": true }));
    let editing = w.ok("view", json!({ "doc": doc, "slide": ids[1], "width": 480, "edit": true, "editing": body }));
    let png = |v: &Json| std::fs::read(w.root.join(v["path"].as_str().unwrap())).unwrap();
    assert_ne!(edit["path"], editing["path"]);
    assert_ne!(png(&edit), png(&editing), "the text left out");
    // An edit of another slide leaves this render current; an edit of this
    // one draws it again and removes the older revision.
    w.ok("notes", json!({ "doc": doc, "slide": ids[2], "text": "x" }));
    let again = w.ok("view", json!({ "doc": doc, "slide": ids[1], "width": 480 }));
    assert_eq!((again["path"].clone(), again["rev"].clone()), (json!(path), json!(1)), "the slide did not change: {again}");
    let changed = w.ok("set_text", json!({ "doc": doc, "slide": ids[1], "id": body, "text": "One point" }));
    assert_eq!(changed["changed"], json!([ids[1]]), "{changed}");
    let redrawn = w.ok("view", json!({ "doc": doc, "slide": ids[1], "width": 480 }));
    assert_ne!(redrawn["path"], json!(path), "a new revision, a new file");
    assert!(!file.exists(), "the older revision of the same slide, size and mode went");
    assert_ne!(png(&redrawn), first, "shows its unsaved edit");
    // Caps.
    for width in [47, 3841, 0] {
        coded(&w.err("view", json!({ "doc": doc, "slide": ids[1], "width": width })), "invalid");
    }
    coded(&w.err("view", json!({ "doc": doc, "slide": ids[1] })), "invalid");
    coded(&w.err("view", json!({ "doc": doc, "slide": ids[1], "width": 96, "step": 1, "edit": true })), "invalid");
    coded(&w.err("view", json!({ "doc": doc, "slide": ids[1], "width": 96, "editing": 999_999 })), "unknown_id");
    w.ok("view", json!({ "doc": doc, "slide": ids[1], "width": 3840 }));
}

#[test]
fn present_shows_the_order_and_each_builds_end_state() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 6);
    let ids = w.slide_ids(&doc);
    w.ok("slides_op", json!({ "doc": doc, "op": "hide", "slides": [ids[2]], "hidden": true }));
    let show = w.ok("show", json!({ "doc": doc }));
    let order: Vec<u64> = show["order"].as_array().unwrap().iter().map(|e| e["slide"].as_u64().unwrap()).collect();
    assert_eq!(order, vec![ids[0], ids[1], ids[3], ids[4], ids[5]], "the hidden slide is left out: {show}");
    assert_eq!(show["loop"], false);
    assert_eq!(show["order"][3]["steps"], 2, "{show}");
    let from = w.ok("show", json!({ "doc": doc, "from": ids[3] }));
    let order: Vec<u64> = from["order"].as_array().unwrap().iter().map(|e| e["slide"].as_u64().unwrap()).collect();
    assert_eq!(order, vec![ids[3], ids[4], ids[5]]);
    // Each step's end state: before the builds the two shapes are hidden,
    // then one, then both show.
    let step = |k: u64| {
        let v = w.ok("view", json!({ "doc": doc, "slide": ids[4], "width": 240, "step": k }));
        std::fs::read(w.root.join(v["path"].as_str().unwrap())).unwrap()
    };
    let (s0, s1, s2) = (step(0), step(1), step(2));
    assert!(s0 != s1 && s1 != s2 && s0 != s2, "three different end states");
    let plain = w.ok("view", json!({ "doc": doc, "slide": ids[4], "width": 240 }));
    assert_eq!(std::fs::read(w.root.join(plain["path"].as_str().unwrap())).unwrap(), s2, "after every build, the slide as it stands");
    coded(&w.err("view", json!({ "doc": doc, "slide": ids[4], "width": 240, "step": 3 })), "invalid");
    coded(&w.err("show", json!({ "doc": doc, "from": 999_999 })), "unknown_id");
}

#[test]
fn layouts_themes_fonts_and_shapes() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 2);
    let layouts = w.ok("layouts", json!({ "doc": doc, "width": 120 }));
    let list = layouts["layouts"].as_array().unwrap();
    assert_eq!(list.len(), 11, "{layouts}");
    for l in list {
        assert!(std::fs::read(w.root.join(l["path"].as_str().unwrap())).unwrap().starts_with(b"\x89PNG"), "{l}");
    }
    // A theme change draws them again; the older ones go.
    let before = list[0]["path"].as_str().unwrap().to_string();
    w.ok("design", json!({ "doc": doc, "theme": "Nocturne" }));
    let after = w.ok("layouts", json!({ "doc": doc, "width": 120 }));
    assert_ne!(after["layouts"][0]["path"], json!(before));
    assert!(!w.root.join(&before).exists());
    let themes = w.ok("themes", json!({}));
    let names: Vec<&str> = themes["themes"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["DeckCraft", "Harbor", "Ember", "Meadow", "Nocturne", "Paper", "Coral Reef", "Slate"]);
    assert_eq!(themes["themes"][0]["label"], "Default");
    assert!(themes["color_schemes"].as_array().unwrap().iter().any(|c| c["name"] == "Ocean" && c["colors"].as_array().is_some_and(|k| k.len() == 12)), "{themes}");
    assert!(themes["font_schemes"].as_array().unwrap().iter().any(|f| f["name"] == "Office Compatible"), "{themes}");
    let fonts = w.ok("fonts", json!({}));
    assert!(fonts["families"].as_array().is_some_and(|f| !f.is_empty()), "at least the last-resort face: {fonts}");
    let shapes = w.ok("shapes", json!({}));
    assert!(shapes["presets"].as_array().unwrap().iter().any(|p| p["name"] == "rect" && p["label"].is_string() && p["category"].is_string()), "{shapes}");
}

#[test]
fn format_state_of_a_box_and_of_a_paragraph() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 2);
    let ids = w.slide_ids(&doc);
    let body = w.shape_where(&doc, ids[1], "text", "First point\nSecond point");
    w.ok("format", json!({ "doc": doc, "slide": ids[1], "ids": [body], "paragraphs": [1, 1], "bold": true, "color": "#C0392B", "align": "center" }));
    let box_state = w.ok("format_state", json!({ "doc": doc, "slide": ids[1], "id": body }));
    assert_eq!(box_state["bold"], false, "the box's first run: {box_state}");
    let p1 = w.ok("format_state", json!({ "doc": doc, "slide": ids[1], "id": body, "paragraph": 1 }));
    assert_eq!((p1["bold"].clone(), p1["color"].clone(), p1["align"].clone()), (json!(true), json!("#C0392B"), json!("center")), "{p1}");
    assert!(p1["font"].is_string() && p1["size"].is_number(), "{p1}");
    coded(&w.err("format_state", json!({ "doc": doc, "slide": ids[1], "id": body, "paragraph": 5 })), "invalid");
    assert_eq!(w.ok("state", json!({ "doc": doc }))["rev"], 1, "reading the format changed nothing");
}

// ------------------------------------------------------------ slides

#[test]
fn slide_operations() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 4);
    let ids = w.slide_ids(&doc);
    // New, at the end by default, and at a position.
    let made = w.ok("slides_op", json!({ "doc": doc, "op": "new", "layout": "titleOnly", "title": "Questions" }));
    let q = made["slide"].as_u64().unwrap();
    assert_eq!(made["changed"], json!([q]), "{made}");
    assert_eq!(*w.slide_ids(&doc).last().unwrap(), q);
    let at1 = w.ok("slides_op", json!({ "doc": doc, "op": "new", "at": 1 }))["slide"].as_u64().unwrap();
    assert_eq!(w.slide_ids(&doc)[1], at1);
    // Duplicate: each copy follows its original.
    let dup = w.ok("slides_op", json!({ "doc": doc, "op": "duplicate", "slides": [ids[2], ids[0]] }));
    let copies: Vec<u64> = dup["slides"].as_array().unwrap().iter().map(|c| c.as_u64().unwrap()).collect();
    let now = w.slide_ids(&doc);
    assert_eq!(now.len(), 8);
    assert_eq!(now.iter().position(|x| *x == copies[0]).unwrap(), now.iter().position(|x| *x == ids[0]).unwrap() + 1, "{now:?} {copies:?}");
    assert_eq!(now.iter().position(|x| *x == copies[1]).unwrap(), now.iter().position(|x| *x == ids[2]).unwrap() + 1, "{now:?} {copies:?}");
    // Delete, but never every slide.
    w.ok("slides_op", json!({ "doc": doc, "op": "delete", "slides": copies }));
    let now = w.slide_ids(&doc);
    assert_eq!(now.len(), 6);
    coded(&w.err("slides_op", json!({ "doc": doc, "op": "delete", "slides": now })), "invalid");
    // Move: the slides named, in their order, at the position named.
    let order = w.slide_ids(&doc);
    w.ok("slides_op", json!({ "doc": doc, "op": "move", "slides": [order[1], order[0]], "to": 3 }));
    let moved = w.slide_ids(&doc);
    assert_eq!(moved, vec![order[2], order[3], order[4], order[1], order[0], order[5]], "{order:?} → {moved:?}");
    w.ok("slides_op", json!({ "doc": doc, "op": "move", "slides": [order[0]], "to": 0 }));
    assert_eq!(w.slide_ids(&doc)[0], order[0]);
    // Hide, layout, reset.
    let s = w.slide_ids(&doc)[2];
    w.ok("slides_op", json!({ "doc": doc, "op": "hide", "slides": [s], "hidden": true }));
    assert_eq!(w.ok("slide", json!({ "doc": doc, "slide": s }))["hidden"], true);
    let blank = w.ok("open", json!({ "path": "board.deckcraft" }))["layouts"].as_array().unwrap().iter().find(|l| l["kind"] == "blank").unwrap()["id"].clone();
    let laid = w.ok("slides_op", json!({ "doc": doc, "op": "layout", "slides": [s], "layout": blank }));
    assert_eq!(laid["changed"], json!([s]));
    assert_eq!(w.ok("slide", json!({ "doc": doc, "slide": s }))["layout"], blank);
    w.ok("slides_op", json!({ "doc": doc, "op": "reset", "slides": [s] }));
    // Sections.
    let secs = w.ok("slides_op", json!({ "doc": doc, "op": "section_add", "at": 2, "name": "Results" }));
    assert_eq!(secs["sections"], json!([{ "name": "Default Section", "first": 0, "count": 2 }, { "name": "Results", "first": 2, "count": 4 }]), "{secs}");
    let secs = w.ok("slides_op", json!({ "doc": doc, "op": "section_rename", "section": 1, "name": "Numbers" }));
    assert_eq!(secs["sections"][1]["name"], "Numbers");
    let n = w.slide_ids(&doc).len();
    let secs = w.ok("slides_op", json!({ "doc": doc, "op": "section_move", "section": 1, "to": 0 }));
    assert_eq!(secs["sections"][0]["name"], "Numbers", "{secs}");
    let secs = w.ok("slides_op", json!({ "doc": doc, "op": "section_remove", "section": 0 }));
    assert_eq!(secs["sections"].as_array().unwrap().len(), 1, "{secs}");
    assert_eq!(w.slide_ids(&doc).len(), n, "removing a section keeps its slides");
    // Refusals.
    coded(&w.err("slides_op", json!({ "doc": doc, "op": "shuffle" })), "invalid");
    coded(&w.err("slides_op", json!({ "doc": doc, "op": "hide", "slides": [999_999], "hidden": true })), "unknown_id");
    coded(&w.err("slides_op", json!({ "doc": doc, "op": "section_rename", "section": 9, "name": "x" })), "invalid");
    let many: Vec<u64> = (0..501).collect();
    coded(&w.err("slides_op", json!({ "doc": doc, "op": "hide", "slides": many, "hidden": true })), "too_large");
}

#[test]
fn notes_and_transitions() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 3);
    let ids = w.slide_ids(&doc);
    let n = w.ok("notes", json!({ "doc": doc, "slide": ids[1], "text": "Pause here\nThen the chart" }));
    assert_eq!(n["changed"], json!([ids[1]]));
    assert_eq!(w.ok("slide", json!({ "doc": doc, "slide": ids[1] }))["notes"], "Pause here\nThen the chart");
    coded(&w.err("notes", json!({ "doc": doc, "slide": ids[1], "text": "x".repeat(20_001) })), "too_large");
    let t = w.ok("transition", json!({ "doc": doc, "slides": [ids[1]], "kind": "push", "option": "l", "duration": 800 }));
    assert_eq!(t["changed"], json!([ids[1]]));
    let s = w.ok("slide", json!({ "doc": doc, "slide": ids[1] }));
    assert_eq!((s["transition"]["kind"].clone(), s["transition"]["option"].clone(), s["transition"]["duration"].clone()), (json!("push"), json!("l"), json!(800)), "{s}");
    let all = w.ok("transition", json!({ "doc": doc, "all": true, "kind": "fade" }));
    assert_eq!(all["changed"].as_array().unwrap().len(), 3, "{all}");
    assert!(w.ok("slides", json!({ "doc": doc }))["slides"].as_array().unwrap().iter().all(|s| s["transition"] == "fade"));
    coded(&w.err("transition", json!({ "doc": doc, "all": true, "kind": "teleport" })), "invalid");
    coded(&w.err("transition", json!({ "doc": doc, "all": true, "kind": "push", "option": "sideways" })), "invalid");
    coded(&w.err("transition", json!({ "doc": doc, "kind": "fade" })), "invalid");
    coded(&w.err("transition", json!({ "doc": doc, "all": true, "kind": "fade", "duration": 60_000 })), "invalid");
}

// ------------------------------------------------------------ objects

#[test]
fn inserting_text_shapes_pictures_tables_and_charts() {
    let w = World::new();
    w.put("images/dot.png", &png());
    let doc = w.sample_open("board.deckcraft", 2);
    let s = w.slide_ids(&doc)[1];
    let t = w.ok("insert", json!({ "doc": doc, "slide": s, "kind": "text", "rect": [100, 100, 300, 50], "text": "A note" }));
    assert_eq!((t["changed"].clone(), t["can_undo"].clone()), (json!([s]), json!(true)), "{t}");
    let shapes = w.shapes(&doc, s);
    let note = shapes.iter().find(|x| x["id"] == t["id"]).unwrap();
    assert_eq!(note["text"], "A note");
    let sh = w.ok("insert", json!({ "doc": doc, "slide": s, "kind": "shape", "preset": "roundRect", "text": "Box" }));
    let boxed = w.shapes(&doc, s).into_iter().find(|x| x["id"] == sh["id"]).unwrap();
    assert_eq!(boxed["box"], json!([408.0, 198.0, 144.0, 144.0]), "the service centres a default box: {boxed}");
    let p = w.ok("insert", json!({ "doc": doc, "slide": s, "kind": "picture", "path": "images/dot.png" }));
    assert_eq!(p["size"].as_array().unwrap().len(), 2, "{p}");
    let tb = w.ok("insert", json!({ "doc": doc, "slide": s, "kind": "table", "rows": 2, "cols": 3, "cells": [["a", "b", "c"], ["1", "2"]], "style": "medium2-accent1" }));
    let table = w.shapes(&doc, s).into_iter().find(|x| x["id"] == tb["id"]).unwrap();
    assert_eq!(table["table"]["cells"], json!([["a", "b", "c"], ["1", "2", ""]]), "{table}");
    assert_eq!(table["table"]["style"], "medium2-accent1");
    let ch = w.ok("insert", json!({ "doc": doc, "slide": s, "kind": "chart", "type": "bar", "categories": ["A", "B"], "series": [{ "name": "S", "values": [1, 2] }], "title": "Sales" }));
    let chart = w.shapes(&doc, s).into_iter().find(|x| x["id"] == ch["id"]).unwrap();
    assert_eq!((chart["chart"]["title"].clone(), chart["chart"]["categories"].clone()), (json!("Sales"), json!(["A", "B"])), "{chart}");
    // Each insert is one undo step: the table and its style go together.
    w.ok("undo", json!({ "doc": doc }));
    w.ok("undo", json!({ "doc": doc }));
    assert!(w.shapes(&doc, s).iter().all(|x| x["kind"] != "table" && x["kind"] != "chart"));
    // Refusals.
    coded(&w.err("insert", json!({ "doc": doc, "slide": s, "kind": "shape", "preset": "dragon" })), "invalid");
    coded(&w.err("insert", json!({ "doc": doc, "slide": s, "kind": "video", "path": "x.mp4" })), "invalid");
    coded(&w.err("insert", json!({ "doc": doc, "slide": s, "kind": "table", "rows": 76, "cols": 2 })), "invalid");
    coded(&w.err("insert", json!({ "doc": doc, "slide": s, "kind": "chart", "type": "pie3d", "categories": [], "series": [] })), "invalid");
    coded(&w.err("insert", json!({ "doc": doc, "slide": 999_999, "kind": "text" })), "unknown_id");
}

/// The caps on objects: SERVICE.md's and the door's, each refused before
/// anything changes.
#[test]
fn the_caps_on_objects() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 2);
    let s = w.slide_ids(&doc)[1];
    let before = w.ok("state", json!({ "doc": doc }));
    // A fill of 1,025 cells (33 × 32 = 1,056 > 1,024).
    let row: Vec<&str> = vec!["x"; 32];
    let cells: Vec<Vec<&str>> = vec![row.clone(); 33];
    let e = w.err("insert", json!({ "doc": doc, "slide": s, "kind": "table", "rows": 33, "cols": 32, "cells": cells }));
    coded(&e, "too_large");
    assert!(e.contains("1024"), "{e}");
    let cells: Vec<Vec<&str>> = vec![row; 32];
    w.ok("insert", json!({ "doc": doc, "slide": s, "kind": "table", "rows": 32, "cols": 32, "cells": cells }));
    w.ok("undo", json!({ "doc": doc }));
    // A chart of 10,001 points (the door's).
    let values: Vec<f64> = vec![1.0; 10_000];
    let e = w.err("insert", json!({ "doc": doc, "slide": s, "kind": "chart", "type": "line", "categories": [], "series": [{ "name": "s", "values": values }] }));
    coded(&e, "too_large");
    assert!(e.contains("10001 chart data points"), "{e}");
    // 1,001 line breaks.
    coded(&w.err("insert", json!({ "doc": doc, "slide": s, "kind": "text", "text": "a\n".repeat(1_001) })), "too_large");
    // A position past 100,000 points (the door's).
    let e = w.err("insert", json!({ "doc": doc, "slide": s, "kind": "text", "rect": [100_001, 0, 10, 10] }));
    coded(&e, "too_large");
    assert!(e.contains("100001 points"), "{e}");
    // 501 shapes named.
    let many: Vec<u64> = (1..=501).collect();
    coded(&w.err("shape", json!({ "doc": doc, "slide": s, "ids": many, "op": "delete" })), "too_large");
    // A slide past 1,920 × 1,080 points (the door's).
    coded(&w.err("design", json!({ "doc": doc, "size": [2000, 1100] })), "too_large");
    let after = w.ok("state", json!({ "doc": doc }));
    assert_eq!(
        (after["edited"].clone(), after["can_undo"].clone(), after["can_redo"].clone()),
        (before["edited"].clone(), json!(false), json!(true)),
        "only the one fill that fit, undone: {after}"
    );
}

/// The door's `RunBudget` ceilings hold after every command of an app's
/// method, as after `deck.run`'s: duplicating 201 slides of a 300-slide deck
/// crosses the session's 500 slides partway, and the whole call goes back.
/// A drawing keeps to 4,096 pixels a side and 16 megapixels.
#[test]
fn the_sessions_ceilings_hold_after_every_command() {
    let w = World::new();
    w.put("long.deckcraft", &native(&sample(300)));
    let doc = w.open("long.deckcraft");
    let ids = w.slide_ids(&doc);
    let e = w.err("slides_op", json!({ "doc": doc, "op": "duplicate", "slides": ids[..201].to_vec() }));
    coded(&e, "too_large");
    assert!(e.contains("`slide.duplicate` leaves the presentation with 501 slides, more than the 500"), "{e}");
    assert!(e.starts_with("too_large: deck.slides_op:"), "named by the method: {e}");
    let state = w.ok("state", json!({ "doc": doc }));
    assert_eq!((state["slides"].clone(), state["rev"].clone(), state["can_undo"].clone()), (json!(300), json!(0), json!(false)), "all of it went back: {state}");
    w.ok("slides_op", json!({ "doc": doc, "op": "duplicate", "slides": ids[..200].to_vec() }));
    assert_eq!(w.ok("state", json!({ "doc": doc }))["slides"], 500);
    // A portrait slide drawn 3,840 pixels wide would be 6,827 high.
    let small = w.sample_open("small.deckcraft", 1);
    w.ok("design", json!({ "doc": small, "size": [540, 960] }));
    let s = w.slide_ids(&small)[0];
    let e = w.err("view", json!({ "doc": small, "slide": s, "width": 3840 }));
    coded(&e, "too_large");
    assert!(e.contains("3840 × 6827"), "{e}");
    w.ok("view", json!({ "doc": small, "slide": s, "width": 2304 }));
}

#[test]
fn set_text_replaces_only_the_span_that_changed() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 2);
    let s = w.slide_ids(&doc)[1];
    let id = w.ok("insert", json!({ "doc": doc, "slide": s, "kind": "text", "rect": [100, 100, 400, 60], "text": "Hello world today" }))["id"].as_u64().unwrap();
    // Make "world" bold, by its paragraph's characters 6 to 11.
    docs::with_doc(&json!({ "doc": doc }), &Ctx { area: &Area::new(&w.root, None, true), app: APP, isolate: 0 }, "test", |d| {
        let i = d.slide_index(s as u32)?;
        for (cmd, p) in [
            ("slide.go", json!({ "index": i })),
            ("text.edit", json!({ "id": id, "at": [0, 6] })),
            ("text.select", json!({ "anchor": [0, 6], "caret": [0, 11] })),
            ("format.bold", json!({ "on": true })),
            ("text.exit", json!({})),
        ] {
            d.session.execute(cmd, &p).unwrap();
        }
        Ok(())
    })
    .unwrap();
    let bold = |t: &str| (t.to_string(), true);
    let plain = |t: &str| (t.to_string(), false);
    assert_eq!(runs(&doc, s, id), [plain("Hello "), bold("world"), plain(" today")]);
    let steps = undo_len(&doc);
    // An insertion before the bold word takes the format at the caret.
    let r = w.ok("set_text", json!({ "doc": doc, "slide": s, "id": id, "text": "Hello brave world today" }));
    assert_eq!((r["id"].clone(), r["changed"].clone()), (json!(id), json!([s])), "{r}");
    assert_eq!(runs(&doc, s, id), [plain("Hello brave "), bold("world"), plain(" today")]);
    // Replacing the bold word keeps it bold: the new text takes the format
    // of the span's first character; the rest of the box keeps its runs.
    w.ok("set_text", json!({ "doc": doc, "slide": s, "id": id, "text": "Hello brave there today" }));
    assert_eq!(runs(&doc, s, id), [plain("Hello brave "), bold("there"), plain(" today")]);
    // A deletion, and a new paragraph with a line break.
    w.ok("set_text", json!({ "doc": doc, "slide": s, "id": id, "text": "Hello there today" }));
    assert_eq!(runs(&doc, s, id), [plain("Hello "), bold("there"), plain(" today")]);
    w.ok("set_text", json!({ "doc": doc, "slide": s, "id": id, "text": "Hello there today\nNext\u{b}line" }));
    let text = w.shapes(&doc, s).into_iter().find(|x| x["id"] == json!(id)).unwrap();
    assert_eq!(text["text"], "Hello there today\nNext\u{b}line", "{text}");
    assert_eq!(runs(&doc, s, id)[..3], [plain("Hello "), bold("there"), plain(" today")]);
    // Each call is one undo step: back through each of them, exactly.
    assert_eq!(undo_len(&doc), steps + 4);
    w.ok("undo", json!({ "doc": doc }));
    w.ok("undo", json!({ "doc": doc }));
    assert_eq!(runs(&doc, s, id), [plain("Hello brave "), bold("there"), plain(" today")]);
    w.ok("undo", json!({ "doc": doc }));
    w.ok("undo", json!({ "doc": doc }));
    assert_eq!(runs(&doc, s, id), [plain("Hello "), bold("world"), plain(" today")]);
    // The same text changes nothing.
    let same = w.ok("set_text", json!({ "doc": doc, "slide": s, "id": id, "text": "Hello world today" }));
    assert_eq!(same["changed"], json!([]));
    // Text as typed: AutoCorrect and smart quotes are off.
    let typed = "\"teh\" isn't -> (c) wich";
    w.ok("set_text", json!({ "doc": doc, "slide": s, "id": id, "text": typed }));
    let got = w.shapes(&doc, s).into_iter().find(|x| x["id"] == json!(id)).unwrap();
    assert_eq!(got["text"], typed);
    // An empty box takes its text whole; a table cell by its cell.
    let empty = w.ok("insert", json!({ "doc": doc, "slide": s, "kind": "text", "rect": [100, 300, 200, 40] }))["id"].as_u64().unwrap();
    w.ok("set_text", json!({ "doc": doc, "slide": s, "id": empty, "text": "Fresh\n\tindented" }));
    let fresh = w.shapes(&doc, s).into_iter().find(|x| x["id"] == json!(empty)).unwrap();
    assert_eq!(fresh["text"], "Fresh\n\tindented", "a leading tab stays a tab: {fresh}");
    let table = w.ok("insert", json!({ "doc": doc, "slide": s, "kind": "table", "rows": 2, "cols": 2 }))["id"].as_u64().unwrap();
    w.ok("set_text", json!({ "doc": doc, "slide": s, "id": table, "cell": [1, 0], "text": "Cell" }));
    let cells = w.shapes(&doc, s).into_iter().find(|x| x["id"] == json!(table)).unwrap()["table"]["cells"].clone();
    assert_eq!(cells, json!([["", ""], ["Cell", ""]]));
    coded(&w.err("set_text", json!({ "doc": doc, "slide": s, "id": table, "text": "x" })), "invalid");
    coded(&w.err("set_text", json!({ "doc": doc, "slide": s, "id": table, "cell": [5, 0], "text": "x" })), "invalid");
    coded(&w.err("set_text", json!({ "doc": doc, "slide": s, "id": 999_999, "text": "x" })), "unknown_id");
    coded(&w.err("set_text", json!({ "doc": doc, "slide": s, "id": id, "text": "a\n".repeat(1_001) })), "too_large");
}

#[test]
fn formatting_boxes_and_paragraphs_is_one_undo_step() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 2);
    let s = w.slide_ids(&doc)[1];
    let body = w.shape_where(&doc, s, "text", "First point\nSecond point");
    let steps = undo_len(&doc);
    let r = w.ok(
        "format",
        json!({ "doc": doc, "slide": s, "ids": [body], "font": "Inter", "size": 28, "bold": true, "italic": true, "underline": true, "color": "accent2", "align": "right", "line_spacing": 1.5, "space_before": 6, "anchor": "middle", "autofit": "none", "columns": 2, "margins": { "left": 10 } }),
    );
    assert_eq!((r["changed"].clone(), r["can_undo"].clone()), (json!([s]), json!(true)), "{r}");
    assert_eq!(undo_len(&doc), steps + 1, "thirteen format commands, one undo step");
    assert_eq!(w.ok("state", json!({ "doc": doc }))["undo"], "Format text");
    let state = w.ok("format_state", json!({ "doc": doc, "slide": s, "id": body }));
    assert_eq!((state["bold"].clone(), state["italic"].clone(), state["underline"].clone(), state["size"].clone(), state["align"].clone()), (json!(true), json!(true), json!(true), json!(28.0), json!("right")), "{state}");
    w.ok("undo", json!({ "doc": doc }));
    let state = w.ok("format_state", json!({ "doc": doc, "slide": s, "id": body }));
    assert_eq!((state["bold"].clone(), state["italic"].clone()), (json!(false), json!(false)), "all of it undone at once: {state}");
    w.ok("redo", json!({ "doc": doc }));
    assert_eq!(w.ok("format_state", json!({ "doc": doc, "slide": s, "id": body }))["bold"], true);
    // A paragraph's level, in and out.
    w.ok("format", json!({ "doc": doc, "slide": s, "ids": [body], "paragraphs": [1, 1], "level": 2 }));
    let paras = w.shapes(&doc, s).into_iter().find(|x| x["id"] == json!(body)).unwrap()["paragraphs"].clone();
    assert_eq!(paras, json!([{ "text": "First point", "level": 0 }, { "text": "Second point", "level": 2 }]));
    w.ok("format", json!({ "doc": doc, "slide": s, "ids": [body], "paragraphs": [1, 1], "level": -1 }));
    assert_eq!(w.shapes(&doc, s).into_iter().find(|x| x["id"] == json!(body)).unwrap()["paragraphs"][1]["level"], 1);
    // Refusals.
    coded(&w.err("format", json!({ "doc": doc, "slide": s, "ids": [body] })), "invalid");
    coded(&w.err("format", json!({ "doc": doc, "slide": s, "ids": [body], "paragraphs": [0, 9], "bold": true })), "invalid");
    coded(&w.err("format", json!({ "doc": doc, "slide": s, "ids": [body, body + 1], "paragraphs": [0, 0], "bold": true })), "invalid");
    coded(&w.err("format", json!({ "doc": doc, "slide": s, "ids": [body], "color": "red" })), "invalid");
    coded(&w.err("format", json!({ "doc": doc, "slide": s, "ids": [body], "size": 4001 })), "invalid");
}

#[test]
fn arranging_and_styling_shapes() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 6);
    let s = w.slide_ids(&doc)[4];
    let a = w.shape_where(&doc, s, "text", "First");
    let b = w.shape_where(&doc, s, "text", "Second");
    let op = |args: Json| {
        let mut args = args;
        args["doc"] = json!(doc);
        args["slide"] = json!(s);
        w.ok("shape", args)
    };
    let box_of = |id: u64| w.shapes(&doc, s).into_iter().find(|x| x["id"] == json!(id)).unwrap()["box"].clone();
    op(json!({ "ids": [a], "op": "bounds", "x": 10, "y": 20, "w": 30, "h": 40 }));
    assert_eq!(box_of(a), json!([10.0, 20.0, 30.0, 40.0]));
    op(json!({ "ids": [a], "op": "move", "dx": 5, "dy": -5 }));
    assert_eq!(box_of(a), json!([15.0, 15.0, 30.0, 40.0]));
    op(json!({ "ids": [a], "op": "rotate", "deg": 30 }));
    assert_eq!(w.shapes(&doc, s).into_iter().find(|x| x["id"] == json!(a)).unwrap()["rotation"], 30.0);
    op(json!({ "ids": [a], "op": "line", "color": "#000000", "width": 2, "dash": "dash" }));
    let line = w.shapes(&doc, s).into_iter().find(|x| x["id"] == json!(a)).unwrap()["line"].clone();
    assert_eq!((line["color"].clone(), line["width"].clone(), line["dash"].clone()), (json!("#000000"), json!(2.0), json!("dash")), "{line}");
    // The same outline again changes nothing: no undo step, no new revision.
    let (rev, steps) = (w.ok("state", json!({ "doc": doc }))["rev"].clone(), undo_len(&doc));
    let again = op(json!({ "ids": [a], "op": "line", "color": "#000000", "width": 2, "dash": "dash" }));
    assert_eq!((again["changed"].clone(), again["rev"].clone()), (json!([]), rev), "{again}");
    assert_eq!(undo_len(&doc), steps);
    for args in [
        json!({ "ids": [a], "op": "flip", "axis": "vertical" }),
        json!({ "ids": [a], "op": "order", "to": "front" }),
        json!({ "ids": [a, b], "op": "align", "edge": "top", "to": "selection" }),
        json!({ "ids": [a, b], "op": "distribute", "dir": "horizontal" }),
        json!({ "ids": [a], "op": "fill", "color": "#2E6FD8", "transparency": 0.25 }),
        json!({ "ids": [a], "op": "fill", "gradient": { "stops": [[0, "#FFFFFF"], [1, "accent1"]], "angle": 45 } }),
        json!({ "ids": [a], "op": "fill", "none": true }),
        json!({ "ids": [a], "op": "effects", "shadow": "outer", "glow": 6, "soft_edges": 2, "reflection": "tight" }),
        json!({ "ids": [a], "op": "style", "index": 9 }),
        json!({ "ids": [a], "op": "lock", "locked": true }),
        json!({ "ids": [a], "op": "lock", "locked": false }),
        json!({ "ids": [a], "op": "hide", "hidden": true }),
        json!({ "ids": [a], "op": "hide", "hidden": false }),
    ] {
        let r = op(args.clone());
        assert_eq!((r["changed"].clone(), r["ids"].clone()), (json!([s]), args["ids"].clone()), "{args}: {r}");
    }
    let group = op(json!({ "ids": [a, b], "op": "group" }))["ids"].clone();
    assert_eq!(group.as_array().unwrap().len(), 1, "{group}");
    let members = op(json!({ "ids": group, "op": "ungroup" }))["ids"].clone();
    assert_eq!(members.as_array().unwrap().len(), 2, "{members}");
    let copies = op(json!({ "ids": [a], "op": "duplicate" }))["ids"].clone();
    assert_eq!(copies.as_array().unwrap().len(), 1);
    assert_eq!(op(json!({ "ids": copies, "op": "delete" }))["ids"], json!([]));
    // A fill never takes a picture, a pattern or a path.
    for (key, value) in [("picture", json!(0)), ("path", json!("images/dot.png")), ("pattern", json!({ "preset": "pct50" }))] {
        let mut args = json!({ "doc": doc, "slide": s, "ids": [a], "op": "fill", "color": "#000000" });
        args[key] = value;
        let e = w.err("shape", args);
        coded(&e, "invalid");
        assert!(e.contains(&format!("`{key}` is not one of them")), "{e}");
    }
    coded(&w.err("shape", json!({ "doc": doc, "slide": s, "ids": [a, b], "op": "bounds", "x": 0, "y": 0, "w": 1, "h": 1 })), "invalid");
    coded(&w.err("shape", json!({ "doc": doc, "slide": s, "ids": [a], "op": "group" })), "invalid");
    coded(&w.err("shape", json!({ "doc": doc, "slide": s, "ids": [a], "op": "style", "index": 42 })), "invalid");
    coded(&w.err("shape", json!({ "doc": doc, "slide": s, "ids": [a], "op": "spin" })), "invalid");
    coded(&w.err("shape", json!({ "doc": doc, "slide": s, "ids": [999_999], "op": "delete" })), "unknown_id");
}

#[test]
fn pictures_tables_and_charts() {
    let w = World::new();
    w.put("images/dot.png", &png());
    let doc = w.sample_open("board.deckcraft", 6);
    let ids = w.slide_ids(&doc);
    let pic = w.shape_where(&doc, ids[4], "kind", "picture");
    let pop = |args: Json| {
        let mut args = args;
        args["doc"] = json!(doc);
        args["slide"] = json!(ids[4]);
        args["id"] = json!(pic);
        w.ok("picture", args)
    };
    pop(json!({ "op": "crop", "left": 0.1, "right": 0.2 }));
    pop(json!({ "op": "adjust", "brightness": 0.2, "contrast": -0.1, "saturation": 1.5, "grayscale": true, "transparency": 0.3 }));
    let p = w.shapes(&doc, ids[4]).into_iter().find(|x| x["id"] == json!(pic)).unwrap()["picture"].clone();
    assert_eq!((p["crop"][0].clone(), p["grayscale"].clone()), (json!(0.1), json!(true)), "{p}");
    pop(json!({ "op": "reset" }));
    pop(json!({ "op": "replace", "path": "images/dot.png" }));
    pop(json!({ "op": "alt_text", "text": "A dot", "decorative": false }));
    assert_eq!(w.shapes(&doc, ids[4]).into_iter().find(|x| x["id"] == json!(pic)).unwrap()["alt_text"], "A dot");
    coded(&w.err("picture", json!({ "doc": doc, "slide": ids[4], "id": w.shape_where(&doc, ids[4], "text", "First"), "op": "reset" })), "invalid");
    // A table.
    let table = w.shape_where(&doc, ids[2], "kind", "table");
    let top = |args: Json| {
        let mut args = args;
        args["doc"] = json!(doc);
        args["slide"] = json!(ids[2]);
        args["id"] = json!(table);
        w.ok("table", args)
    };
    assert_eq!((top(json!({ "op": "insert_row", "row": 0, "below": true }))["rows"].clone()), json!(4));
    assert_eq!(top(json!({ "op": "insert_col", "col": 1, "right": false }))["cols"], 3);
    assert_eq!(top(json!({ "op": "delete_row", "row": 3 }))["rows"], 3);
    assert_eq!(top(json!({ "op": "delete_col", "col": 0 }))["cols"], 2);
    top(json!({ "op": "merge", "from": [0, 0], "to": [0, 1] }));
    top(json!({ "op": "split", "cell": [0, 0] }));
    top(json!({ "op": "style", "style": "light1-accent2" }));
    top(json!({ "op": "options", "header_row": true, "banded_rows": false }));
    top(json!({ "op": "fill", "cells": [[1, 1], [2, 1]], "color": "#FFEEDD" }));
    top(json!({ "op": "fill", "cells": [[1, 1]], "color": "none" }));
    top(json!({ "op": "col_width", "col": 0, "width": 150 }));
    let r = top(json!({ "op": "row_height", "row": 0, "height": 30 }));
    assert_eq!((r["rows"].clone(), r["cols"].clone(), r["changed"].clone()), (json!(3), json!(2), json!([ids[2]])), "{r}");
    coded(&w.err("table", json!({ "doc": doc, "slide": ids[2], "id": table, "op": "delete_row", "row": 9 })), "invalid");
    coded(&w.err("table", json!({ "doc": doc, "slide": ids[2], "id": table, "op": "merge", "from": [0, 0], "to": [5, 5] })), "invalid");
    // A chart.
    let chart = w.shape_where(&doc, ids[3], "kind", "chart");
    let r = w.ok("chart", json!({ "doc": doc, "slide": ids[3], "id": chart, "type": "line", "categories": ["A", "B"], "series": [{ "name": "X", "values": [1, null] }], "title": "Trend", "legend": "b", "data_labels": true, "gridlines": false, "palette": "colorful" }));
    assert_eq!(r["changed"], json!([ids[3]]));
    let c = w.shapes(&doc, ids[3]).into_iter().find(|x| x["id"] == json!(chart)).unwrap()["chart"].clone();
    assert_eq!((c["title"].clone(), c["categories"].clone()), (json!("Trend"), json!(["A", "B"])), "{c}");
    w.ok("chart", json!({ "doc": doc, "slide": ids[3], "id": chart, "title": "" }));
    assert!(w.shapes(&doc, ids[3]).into_iter().find(|x| x["id"] == json!(chart)).unwrap()["chart"]["title"].is_null(), "an empty title removes it");
    coded(&w.err("chart", json!({ "doc": doc, "slide": ids[3], "id": table, "title": "x" })), "unknown_id");
    coded(&w.err("chart", json!({ "doc": doc, "slide": ids[3], "id": chart })), "invalid");
}

// ------------------------------------------------------------ design, history, revisions

#[test]
fn design_changes_reach_every_slide() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 3);
    let ids = w.slide_ids(&doc);
    let r = w.ok("design", json!({ "doc": doc, "theme": "Harbor", "colors": "Ocean", "fonts": "Classic" }));
    assert_eq!(r["changed"], json!(ids), "a theme change lists every slide: {r}");
    assert_eq!(w.ok("state", json!({ "doc": doc }))["undo"], "Change theme");
    let sized = w.ok("design", json!({ "doc": doc, "size": "4:3", "scale": "fit" }));
    assert_eq!(sized["size"], json!([720.0, 540.0]));
    let bg = w.ok("design", json!({ "doc": doc, "background": { "color": "#F2F4F8", "slides": [ids[1]] } }));
    assert_eq!(bg["changed"], json!([ids[1]]), "{bg}");
    let all = w.ok("design", json!({ "doc": doc, "background": { "gradient": { "stops": [[0, "#FFFFFF"], [1, "#000000"]] }, "all": true } }));
    assert_eq!(all["changed"], json!(ids));
    w.ok("design", json!({ "doc": doc, "background": { "reset": true, "all": true } }));
    let footer = w.ok("design", json!({ "doc": doc, "footer": { "slide_numbers": true, "footer": true, "footer_text": "Northwind", "hide_on_title": true } }));
    assert_eq!(footer["changed"], json!(ids), "{footer}");
    // A background never takes a picture.
    for (key, value) in [("picture", json!("aGVsbG8=")), ("path", json!("images/dot.png")), ("style", json!(3))] {
        let mut bg = json!({ "color": "#000000", "all": true });
        bg[key] = value;
        let e = w.err("design", json!({ "doc": doc, "background": bg }));
        coded(&e, "invalid");
        assert!(e.contains(&format!("`{key}` is not one of them")), "{e}");
    }
    coded(&w.err("design", json!({ "doc": doc })), "invalid");
    coded(&w.err("design", json!({ "doc": doc, "theme": "Vapor" })), "invalid");
    coded(&w.err("design", json!({ "doc": doc, "background": { "color": "#000000" } })), "invalid");
    coded(&w.err("design", json!({ "doc": doc, "scale": "fit" })), "invalid");
}

#[test]
fn revisions_and_changed_slides() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 4);
    let ids = w.slide_ids(&doc);
    let r = w.ok("notes", json!({ "doc": doc, "slide": ids[2], "text": "x" }));
    assert_eq!((r["rev"].clone(), r["changed"].clone()), (json!(1), json!([ids[2]])));
    // A move changes no slide's content, until slides show their numbers.
    let m = w.ok("slides_op", json!({ "doc": doc, "op": "move", "slides": [ids[3]], "to": 1 }));
    assert_eq!((m["rev"].clone(), m["changed"].clone()), (json!(2), json!([])), "{m}");
    w.ok("design", json!({ "doc": doc, "footer": { "slide_numbers": true } }));
    let m = w.ok("slides_op", json!({ "doc": doc, "op": "move", "slides": [ids[3]], "to": 3 }));
    let changed: Vec<u64> = m["changed"].as_array().unwrap().iter().map(|c| c.as_u64().unwrap()).collect();
    assert!(changed.contains(&ids[1]) && changed.contains(&ids[2]) && changed.contains(&ids[3]) && !changed.contains(&ids[0]), "the slides whose number changed: {m}");
    // Undo and redo bump the revision and list what they changed.
    let u = w.ok("undo", json!({ "doc": doc }));
    assert_eq!((u["rev"].clone(), u["redo"].clone()), (json!(5), json!("Move slides")), "{u}");
    let rd = w.ok("redo", json!({ "doc": doc }));
    assert_eq!(rd["rev"], 6);
    // A save changes no revision.
    let saved = w.ok("save", json!({ "doc": doc }));
    assert_eq!(saved["rev"], 6);
    assert_eq!(w.ok("state", json!({ "doc": doc }))["edited"], false);
}

#[test]
fn undo_and_redo_across_calls_with_a_limit_of_a_hundred() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 2);
    let s = w.slide_ids(&doc)[1];
    coded(&w.err("undo", json!({ "doc": doc })), "invalid");
    coded(&w.err("redo", json!({ "doc": doc })), "invalid");
    w.ok("notes", json!({ "doc": doc, "slide": s, "text": "one" }));
    w.ok("notes", json!({ "doc": doc, "slide": s, "text": "two" }));
    let notes = || w.ok("slide", json!({ "doc": doc, "slide": s }))["notes"].clone();
    let u = w.ok("undo", json!({ "doc": doc }));
    assert_eq!((u["can_undo"].clone(), u["can_redo"].clone(), u["undo"].clone(), u["redo"].clone()), (json!(true), json!(true), json!("Edit notes"), json!("Edit notes")), "{u}");
    assert_eq!(notes(), "one");
    w.ok("undo", json!({ "doc": doc }));
    assert_eq!(notes(), "");
    assert_eq!(w.ok("state", json!({ "doc": doc }))["edited"], false, "back where it opened");
    w.ok("redo", json!({ "doc": doc }));
    assert_eq!(notes(), "one");
    // A new change drops what could be redone.
    w.ok("notes", json!({ "doc": doc, "slide": s, "text": "three" }));
    coded(&w.err("redo", json!({ "doc": doc })), "invalid");
    // At most 100 steps.
    for k in 0..105 {
        w.ok("notes", json!({ "doc": doc, "slide": s, "text": format!("n{k}") }));
    }
    assert_eq!(undo_len(&doc), docs::MAX_UNDO);
    for _ in 0..docs::MAX_UNDO {
        w.ok("undo", json!({ "doc": doc }));
    }
    coded(&w.err("undo", json!({ "doc": doc })), "invalid");
    assert_eq!(notes(), "n4", "the oldest hundred steps were kept");
}

/// A call whose engine command fails leaves the document as it was: no
/// half-made change, no undo step.
#[test]
fn a_failing_call_changes_nothing() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 3);
    let ids = w.slide_ids(&doc);
    let before = w.ok("state", json!({ "doc": doc }));
    let table = w.shape_where(&doc, ids[2], "kind", "table");
    // The engine refuses to delete a table's last row.
    for _ in 0..2 {
        w.ok("table", json!({ "doc": doc, "slide": ids[2], "id": table, "op": "delete_row", "row": 0 }));
    }
    let e = w.err("table", json!({ "doc": doc, "slide": ids[2], "id": table, "op": "delete_row", "row": 0 }));
    coded(&e, "invalid");
    let after = w.ok("state", json!({ "doc": doc }));
    assert_eq!(after["rev"], json!(before["rev"].as_u64().unwrap() + 2), "only the two deletes that worked");
    // A plan the door refuses runs nothing (a held command, through the gate).
    docs::with_doc(&json!({ "doc": doc }), &Ctx { area: &Area::new(&w.root, None, true), app: APP, isolate: 0 }, "test", |d| {
        let plan = gate::Plan { method: "deck.test", most: 3, cmds: vec![("slide.go", json!({ "index": 0 })), ("insert.video", json!({ "data": "AAAA" }))] };
        let e = gate::change(d, &Area::new(&w.root, None, true), plan, "x").err().unwrap();
        assert!(e.starts_with("held: deck.test: `insert.video` is held back"), "{e}");
        let plan = gate::Plan { method: "deck.test", most: 1, cmds: vec![("slide.go", json!({ "index": 0 })), ("slide.go", json!({ "index": 1 }))] };
        let e = gate::change(d, &Area::new(&w.root, None, true), plan, "x").err().unwrap();
        assert!(e.starts_with("too_large: deck.test: 2 engine commands, more than the 1"), "{e}");
        let plan = gate::Plan { method: "deck.test", most: 1, cmds: vec![("shape.fill", json!({ "picture": 0, "path": "x.png" }))] };
        let e = gate::change(d, &Area::new(&w.root, None, true), plan, "x").err().unwrap();
        assert!(e.starts_with("invalid: deck.test: `shape.fill`: `picture` would have it read a file"), "{e}");
        let plan = gate::Plan { method: "deck.test", most: 1, cmds: vec![("file.save", json!({}))] };
        assert!(gate::change(d, &Area::new(&w.root, None, true), plan, "x").err().unwrap().contains("classed host"));
        Ok(())
    })
    .unwrap();
    assert_eq!(w.ok("state", json!({ "doc": doc }))["rev"], after["rev"]);
}

/// The per-slide cache of what a session holds counts what `Held::of`
/// counts, after every kind of change.
#[test]
fn the_held_cache_counts_what_the_ceilings_count() {
    let mut s = Session::new();
    engine_file::add_opened(&mut s, "x.deckcraft", None, sample(6));
    let mut cache = gate::HeldCache::default();
    assert_eq!(cache.held(&s), Held::of(&s));
    for (id, p) in [
        ("slide.go", json!({ "index": 2 })),
        ("insert.table", json!({ "rows": 4, "cols": 4 })),
        ("slide.duplicate", json!({ "index": 2 })),
        ("design.theme", json!({ "name": "Slate" })),
        ("slide.delete", json!({ "index": 0 })),
        ("design.slideSize", json!({ "w": 720, "h": 540 })),
    ] {
        s.execute(id, &p).unwrap();
        assert_eq!(cache.held(&s), Held::of(&s), "after {id}");
    }
}

// ------------------------------------------------------------ saving and exporting

#[test]
fn saving_and_a_render_after_a_save_and_a_reopen() {
    let w = World::new();
    let doc = w.sample_open("decks/board.deckcraft", 3);
    let ids = w.slide_ids(&doc);
    let body = w.shape_where(&doc, ids[1], "text", "First point\nSecond point");
    w.ok("set_text", json!({ "doc": doc, "slide": ids[1], "id": body, "text": "Saved point" }));
    let saved = w.ok("save", json!({ "doc": doc }));
    assert_eq!(saved["path"], "decks/board.deckcraft");
    assert_eq!(saved["bytes"].as_u64().unwrap(), std::fs::metadata(w.root.join("decks/board.deckcraft")).unwrap().len());
    let view = w.ok("view", json!({ "doc": doc, "slide": ids[1], "width": 320 }));
    let drawn = std::fs::read(w.root.join(view["path"].as_str().unwrap())).unwrap();
    w.ok("close", json!({ "doc": doc }));
    let again = w.open("decks/board.deckcraft");
    assert_eq!(w.slide_ids(&again), ids, "ids stay through a save and a reopen");
    assert!(w.shapes(&again, ids[1]).iter().any(|s| s["text"] == "Saved point"));
    let reopened = w.ok("view", json!({ "doc": again, "slide": ids[1], "width": 320 }));
    assert_eq!(std::fs::read(w.root.join(reopened["path"].as_str().unwrap())).unwrap(), drawn, "the same slide, drawn again after the reopen");
    // A deck from a .pptx has no native file: its first save names one.
    w.put("imports/board.pptx", &pptx(&sample(2)));
    let p = w.open("imports/board.pptx");
    coded(&w.err("save", json!({ "doc": p })), "invalid");
    assert_eq!(w.ok("save", json!({ "doc": p, "path": "decks/imported.deckcraft" }))["path"], "decks/imported.deckcraft");
    w.ok("notes", json!({ "doc": p, "slide": w.slide_ids(&p)[0], "text": "x" }));
    w.ok("save", json!({ "doc": p }));
    assert!(w.root.join("decks/imported.deckcraft").is_file() && w.root.join("imports/board.pptx").is_file());
}

/// A full save goes to a new file and never over an existing one; a save
/// over the deck's own file needs the foreground.
#[test]
fn a_full_save_never_replaces_another_file() {
    let w = World::new();
    w.put("taken.deckcraft", b"keep me");
    w.put("other.deckcraft", &native(&sample(1)));
    let doc = w.sample_open("board.deckcraft", 2);
    let original = std::fs::read(w.root.join("board.deckcraft")).unwrap();
    w.ok("notes", json!({ "doc": doc, "slide": w.slide_ids(&doc)[0], "text": "x" }));
    for out in ["taken.deckcraft", "other.deckcraft", "board.deckcraft"] {
        let e = w.err("save", json!({ "doc": doc, "path": out }));
        coded(&e, "invalid");
        assert!(e.contains("already exists"), "{e}");
    }
    assert_eq!(std::fs::read(w.root.join("taken.deckcraft")).unwrap(), b"keep me");
    coded(&w.err("save", json!({ "doc": doc, "path": "board.pptx" })), "invalid");
    // From the background, never over its own file; a new file it may.
    let e = w.call_full(APP, "save", json!({ "doc": doc }), false, 0).unwrap_err();
    coded(&e, "invalid");
    assert_eq!(std::fs::read(w.root.join("board.deckcraft")).unwrap(), original);
    assert!(w.call_full(APP, "save", json!({ "doc": doc, "path": "copy.deckcraft" }), false, 0).is_ok());
    // The deck is the new file from then on; the original stays as it was.
    w.ok("notes", json!({ "doc": doc, "slide": w.slide_ids(&doc)[0], "text": "y" }));
    w.ok("save", json!({ "doc": doc }));
    assert_eq!(std::fs::read(w.root.join("board.deckcraft")).unwrap(), original);
    let staging: Vec<String> = w.files("").into_iter().filter(|f| f.contains(octosense_engine_area::STAGING_PREFIX)).collect();
    assert!(staging.is_empty(), "{staging:?}");
}

#[test]
fn exporting_pptx_pdf_pictures_and_outline() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 6);
    let ids = w.slide_ids(&doc);
    let p = w.ok("export", json!({ "doc": doc, "kind": "pptx", "out": "exports/board.pptx" }));
    assert_eq!(p["path"], "exports/board.pptx");
    // An exported .pptx opens back with the same slides.
    let back = w.ok("open", json!({ "path": "exports/board.pptx" }));
    assert_eq!(back["slides"], 6, "{back}");
    let pdf = w.ok("export", json!({ "doc": doc, "kind": "pdf", "out": "exports/board.pdf", "dpi": 72 }));
    assert_eq!(pdf["pages"], 6, "{pdf}");
    assert!(std::fs::read(w.root.join("exports/board.pdf")).unwrap().starts_with(b"%PDF"));
    let handouts = w.ok("export", json!({ "doc": doc, "kind": "pdf", "out": "exports/handouts.pdf", "layout": "handouts", "per_page": 4, "dpi": 72 }));
    assert_eq!(handouts["pages"], 2, "{handouts}");
    w.ok("export", json!({ "doc": doc, "kind": "pdf", "out": "exports/notes.pdf", "layout": "notes", "slides": [ids[1], ids[2]], "dpi": 72 }));
    let images = w.ok("export", json!({ "doc": doc, "kind": "images", "out_dir": "exports/pictures", "width": 320, "slides": [ids[0], ids[2]] }));
    assert_eq!(images["paths"], json!(["exports/pictures/Slide 1.png", "exports/pictures/Slide 3.png"]));
    for p in images["paths"].as_array().unwrap() {
        assert!(std::fs::read(w.root.join(p.as_str().unwrap())).unwrap().starts_with(b"\x89PNG"));
    }
    let outline = w.ok("export", json!({ "doc": doc, "kind": "outline", "out": "exports/board.txt" }));
    assert!(std::fs::read_to_string(w.root.join("exports/board.txt")).unwrap().starts_with("Northwind Cooperative"), "{outline}");
    // New files only, whole or not at all.
    coded(&w.err("export", json!({ "doc": doc, "kind": "pptx", "out": "exports/board.pptx" })), "invalid");
    w.put("exports/more/Slide 2.png", b"theirs");
    coded(&w.err("export", json!({ "doc": doc, "kind": "images", "out_dir": "exports/more", "width": 64 })), "invalid");
    assert!(!w.root.join("exports/more/Slide 1.png").exists(), "nothing written");
    // Caps, before anything is written.
    coded(&w.err("export", json!({ "doc": doc, "kind": "pdf", "out": "x.pdf", "dpi": 201 })), "invalid");
    coded(&w.err("export", json!({ "doc": doc, "kind": "pdf", "out": "x.pdf", "per_page": 5, "layout": "handouts" })), "invalid");
    coded(&w.err("export", json!({ "doc": doc, "kind": "images", "out_dir": "big", "width": 4097 })), "invalid");
    coded(&w.err("export", json!({ "doc": doc, "kind": "word", "out": "x.docx" })), "invalid");
    assert!(!w.root.join("x.pdf").exists() && !w.root.join("big").exists());
}

#[test]
fn export_caps_on_rasters_and_pictures() {
    let w = World::new();
    w.put("long.deckcraft", &native(&sample(80)));
    let doc = w.open("long.deckcraft");
    // 80 slides at 150 dpi are 180 megapixels: over the 160 one export
    // rasters; at 72 dpi they fit.
    let e = w.err("export", json!({ "doc": doc, "kind": "pdf", "out": "all.pdf" }));
    coded(&e, "too_large");
    assert!(e.contains("megapixels"), "{e}");
    let e = w.err("export", json!({ "doc": doc, "kind": "images", "out_dir": "pics", "width": 64 }));
    coded(&e, "too_large");
    assert!(e.contains("80 slides"), "{e}");
    let ids = w.slide_ids(&doc);
    let e = w.err("export", json!({ "doc": doc, "kind": "images", "out_dir": "pics", "width": 4096, "slides": ids[..32].to_vec() }));
    coded(&e, "too_large");
    assert!(e.contains("256"), "{e}");
    assert!(!w.root.join("all.pdf").exists() && !w.root.join("pics").exists());
}

#[test]
fn paths_never_leave_the_storage() {
    let w = World::new();
    w.put("images/dot.png", &png());
    let doc = w.sample_open("board.deckcraft", 2);
    let s = w.slide_ids(&doc)[1];
    let pic = w.ok("insert", json!({ "doc": doc, "slide": s, "kind": "picture", "path": "images/dot.png" }))["id"].clone();
    let parent = w.root.parent().unwrap().to_path_buf();
    std::fs::write(parent.join("beside.png"), png()).unwrap();
    std::fs::write(parent.join("beside.deckcraft"), native(&sample(1))).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&parent, w.root.join("up")).unwrap();
        std::os::unix::fs::symlink(parent.join("beside.png"), w.root.join("link.png")).unwrap();
    }
    for bad in ["../beside.deckcraft", "/etc/hosts", "a/../../beside.deckcraft", "up/beside.deckcraft", "./board.deckcraft"] {
        coded(&w.err("open", json!({ "path": bad })), "invalid");
        coded(&w.err("cover", json!({ "path": bad })), "invalid");
    }
    for bad in ["../x.deckcraft", "/tmp/x.deckcraft", "up/x.deckcraft"] {
        coded(&w.err("create", json!({ "out": bad })), "invalid");
        coded(&w.err("save", json!({ "doc": doc, "path": bad })), "invalid");
    }
    for bad in ["../x.pptx", "up/x.pptx"] {
        coded(&w.err("export", json!({ "doc": doc, "kind": "pptx", "out": bad })), "invalid");
    }
    coded(&w.err("export", json!({ "doc": doc, "kind": "images", "out_dir": "up", "width": 64 })), "invalid");
    for bad in ["../beside.png", "/etc/hosts", "up/beside.png", "link.png"] {
        let e = w.err("insert", json!({ "doc": doc, "slide": s, "kind": "picture", "path": bad }));
        coded(&e, "invalid");
        assert!(!e.contains(parent.to_string_lossy().as_ref()), "an error never spells a host path: {e}");
        coded(&w.err("picture", json!({ "doc": doc, "slide": s, "id": pic, "op": "replace", "path": bad })), "invalid");
    }
    let names: Vec<String> = std::fs::read_dir(&parent).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names.len(), 3, "nothing landed beside the storage: {names:?}");
}

/// The reviewed picture reads take a picture inside storage only: a missing
/// file, one past 64 MiB, a sound or a movie, an SVG and a picture past 50
/// megapixels are each refused before the engine reads it.
#[test]
fn the_picture_reads_take_pictures_only() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 2);
    let s = w.slide_ids(&doc)[1];
    coded(&w.err("insert", json!({ "doc": doc, "slide": s, "kind": "picture", "path": "missing.png" })), "not_found");
    let big = std::fs::File::create(w.root.join("big.png")).unwrap();
    big.set_len(octosense_engine_area::door::MAX_READ_BYTES + 1).unwrap();
    coded(&w.err("insert", json!({ "doc": doc, "slide": s, "kind": "picture", "path": "big.png" })), "too_large");
    w.put("tone.wav", b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0");
    w.put("clip.png", b"\0\0\0\x18ftypisom");
    w.put("logo.svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>");
    for bad in ["tone.wav", "clip.png", "logo.svg"] {
        let e = w.err("insert", json!({ "doc": doc, "slide": s, "kind": "picture", "path": bad }));
        coded(&e, "invalid");
        assert!(e.contains("not a picture"), "{e}");
    }
    // A PNG header naming 20,000 × 20,000 pixels.
    let mut bomb = png();
    bomb[16..20].copy_from_slice(&20_000u32.to_be_bytes());
    bomb[20..24].copy_from_slice(&20_000u32.to_be_bytes());
    w.put("bomb.png", &bomb);
    let e = w.err("insert", json!({ "doc": doc, "slide": s, "kind": "picture", "path": "bomb.png" }));
    coded(&e, "too_large");
    assert!(e.contains("20000 × 20000"), "{e}");
    // A picture whose header doesn't say its size fails closed: a PNG cut
    // inside its header, a JPEG with no frame.
    w.put("cut.png", &png()[..20]);
    w.put("frameless.jpg", b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00\xFF\xD9");
    for odd in ["cut.png", "frameless.jpg"] {
        let e = w.err("insert", json!({ "doc": doc, "slide": s, "kind": "picture", "path": odd }));
        coded(&e, "invalid");
        assert!(e.contains("can't read the picture's size"), "{e}");
    }
    assert_eq!(w.ok("state", json!({ "doc": doc }))["rev"], 0, "nothing reached the deck");
    assert_eq!(pictures::dimensions(&png()), Some((12, 8)));
}

// ------------------------------------------------------------ the cache and covers

#[test]
fn the_render_cache_keeps_to_its_file_cap() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 2);
    let s = w.slide_ids(&doc)[0];
    let mut written = Vec::new();
    for width in (48..48 + 70).map(|k| k * 2) {
        written.push(w.ok("view", json!({ "doc": doc, "slide": s, "width": width }))["path"].as_str().unwrap().to_string());
    }
    let kept: Vec<&String> = written.iter().filter(|p| w.root.join(p).exists()).collect();
    assert_eq!(kept.len(), cache::MAX_FILES);
    let gone = written.len() - cache::MAX_FILES;
    assert!(written[..gone].iter().all(|p| !w.root.join(p).exists()), "the oldest went first");
    let again = w.ok("view", json!({ "doc": doc, "slide": s, "width": 96 }));
    assert!(w.root.join(again["path"].as_str().unwrap()).exists(), "an evicted render is drawn again");
}

#[test]
fn the_render_cache_keeps_to_its_byte_cap_and_covers_to_theirs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(".cache/slides");
    let old = root.join("aaaa");
    std::fs::create_dir_all(&old).unwrap();
    let mib = vec![0u8; 1 << 20];
    let base = std::time::SystemTime::now() - Duration::from_secs(3600);
    for i in 0..17u64 {
        let path = old.join(format!("{i}-s@960-r0.png"));
        std::fs::write(&path, &mib).unwrap();
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(base + Duration::from_secs(i)).unwrap();
    }
    let keep = root.join("bbbb/1-s@960-r0.png");
    let dot = dir.path().join(".cache");
    cache::evict(&dot, &keep, 1 << 20, cache::LIMITS);
    let left: Vec<u64> = (0..17).filter(|i| old.join(format!("{i}-s@960-r0.png")).exists()).collect();
    assert_eq!(left, (2..17).collect::<Vec<_>>(), "15 MiB stay with the new 1 MiB render; the two oldest went");
    // Covers keep to their 32, and slide renders go before covers.
    let w = World::new();
    let mut covers = Vec::new();
    for i in 0..=cache::MAX_COVERS {
        let name = format!("deck {i:02}.deckcraft");
        w.put(&name, &native(&sample(1)));
        let path = w.ok("cover", json!({ "path": name, "width": 48 }))["path"].as_str().unwrap().to_string();
        std::fs::File::options().write(true).open(w.root.join(&path)).unwrap().set_modified(base + Duration::from_secs(i as u64)).unwrap();
        covers.push(path);
    }
    let kept: Vec<bool> = covers.iter().map(|p| w.root.join(p).exists()).collect();
    assert!(!kept[0] && kept[1..].iter().all(|k| *k), "the 33rd cover took the oldest's place: {kept:?}");
}

#[test]
fn a_cover_is_kept_across_closes_and_changes_with_its_deck() {
    let w = World::new();
    w.put("decks/board.deckcraft", &native(&sample(3)));
    let cover = w.ok("cover", json!({ "path": "decks/board.deckcraft" }));
    let path = cover["path"].as_str().unwrap().to_string();
    assert!(path.starts_with(".cache/covers/") && path.ends_with("@320.png"), "{cover}");
    assert_eq!((cover["width"].clone(), cover["height"].clone()), (json!(320), json!(180)));
    assert_eq!(docs::open_count(), 0, "no open document");
    let before = engine_reads();
    assert_eq!(w.ok("cover", json!({ "path": "decks/board.deckcraft" })), cover, "kept: the deck was not read again");
    assert_eq!(engine_reads(), before);
    // Opened, edited, saved: a new version, a new cover; the old one goes.
    let doc = w.open("decks/board.deckcraft");
    w.ok("design", json!({ "doc": doc, "theme": "Nocturne" }));
    std::thread::sleep(Duration::from_millis(20));
    w.ok("save", json!({ "doc": doc }));
    w.ok("close", json!({ "doc": doc }));
    let after = w.ok("cover", json!({ "path": "decks/board.deckcraft" }));
    assert_ne!(after["path"], cover["path"]);
    assert!(!w.root.join(&path).exists(), "the older cover went");
    assert!(w.root.join(after["path"].as_str().unwrap()).exists(), "kept across the close");
    for bad in [json!({ "path": "decks/board.deckcraft", "width": 47 }), json!({ "path": "decks/board.deckcraft", "page": 2 }), json!({})] {
        coded(&w.err("cover", bad), "invalid");
    }
}

/// The whole cache, covers too, is cleared before a write would fail with
/// `storage_full:`; with nothing left to clear, the write fails so.
#[test]
fn the_cache_is_cleared_before_storage_full() {
    let roomy = World::new();
    roomy.put("board.deckcraft", &native(&sample(3)));
    roomy.ok("cover", json!({ "path": "board.deckcraft", "width": 960 }));
    let doc = roomy.open("board.deckcraft");
    let s = roomy.slide_ids(&doc)[0];
    roomy.ok("view", json!({ "doc": doc, "slide": s, "width": 1920 }));
    let cached: u64 = roomy.files(".cache").iter().map(|f| std::fs::metadata(roomy.root.join(f)).unwrap().len()).sum();
    assert!(cached > 0);
    let outline_len = 120;
    let tight = World { _dir: tempfile::tempdir().unwrap(), root: roomy.root.clone(), areas: roomy.tight(outline_len) };
    let out = tight.ok("export", json!({ "doc": doc, "kind": "pptx", "out": "board.pptx" }));
    let len = out["bytes"].as_u64().unwrap();
    assert!(len > outline_len && len <= outline_len + cached, "the export needed the cache's room: {len} bytes");
    assert!(tight.files(".cache").is_empty(), "the cache was cleared: {:?}", tight.files(".cache"));
    let e = tight.err("export", json!({ "doc": doc, "kind": "pptx", "out": "again.pptx" }));
    coded(&e, "storage_full");
    coded(&tight.err("save", json!({ "doc": doc, "path": "copy.deckcraft" })), "storage_full");
    coded(&tight.err("view", json!({ "doc": doc, "slide": s, "width": 1920 })), "storage_full");
    assert!(!tight.root.join("again.pptx").exists());
}

// ------------------------------------------------------------ the arguments every method takes

#[test]
fn every_v2_method_refuses_arguments_it_does_not_take() {
    let w = World::new();
    let doc = w.sample_open("board.deckcraft", 3);
    let s = w.slide_ids(&doc)[1];
    for (method, args) in [
        ("open", json!({ "path": "board.deckcraft", "password": "x" })),
        ("create", json!({ "out": "n.deckcraft", "template": "x" })),
        ("close", json!({ "doc": doc, "force": true })),
        ("state", json!({ "doc": doc, "x": 1 })),
        ("save", json!({ "doc": doc, "format": "pptx" })),
        ("slides", json!({ "doc": doc, "all": true })),
        ("slide", json!({ "doc": doc, "slide": s, "index": 1 })),
        ("hit", json!({ "doc": doc, "slide": s, "at": [1, 1], "mods": "shift" })),
        ("view", json!({ "doc": doc, "slide": s, "width": 96, "out": "x.png" })),
        ("layouts", json!({ "doc": doc, "master": 1 })),
        ("themes", json!({ "x": 1 })),
        ("fonts", json!({ "scan": true })),
        ("shapes", json!({ "filter": "x" })),
        ("text", json!({ "doc": doc, "level": 1 })),
        ("format_state", json!({ "doc": doc, "slide": s, "id": 1, "run": 0 })),
        ("cover", json!({ "path": "board.deckcraft", "slide": 2 })),
        ("slides_op", json!({ "doc": doc, "op": "new", "data": "x" })),
        ("notes", json!({ "doc": doc, "slide": s, "text": "x", "format": "html" })),
        ("transition", json!({ "doc": doc, "all": true, "kind": "fade", "sound": "x.wav" })),
        ("insert", json!({ "doc": doc, "slide": s, "kind": "text", "data": "aGk=" })),
        ("set_text", json!({ "doc": doc, "slide": s, "id": 1, "text": "x", "at": [0, 0] })),
        ("format", json!({ "doc": doc, "slide": s, "ids": [1], "bold": true, "outline": 3 })),
        ("shape", json!({ "doc": doc, "slide": s, "ids": [1], "op": "move", "dx": 1, "x": 3 })),
        ("picture", json!({ "doc": doc, "slide": s, "id": 1, "op": "reset", "data": "x" })),
        ("table", json!({ "doc": doc, "slide": s, "id": 1, "op": "style", "style": "none", "rows": 3 })),
        ("chart", json!({ "doc": doc, "slide": s, "id": 1, "title": "x", "raw": "<c/>" })),
        ("design", json!({ "doc": doc, "theme": "Slate", "picture": "x" })),
        ("undo", json!({ "doc": doc, "steps": 2 })),
        ("redo", json!({ "doc": doc, "steps": 2 })),
        ("show", json!({ "doc": doc, "presenter": true })),
        ("export", json!({ "doc": doc, "kind": "pdf", "out": "x.pdf", "password": "x" })),
    ] {
        let e = w.err(method, args.clone());
        coded(&e, "invalid");
        assert!(e.contains("is not one of them"), "{method} {args}: {e}");
    }
    assert_eq!(w.ok("state", json!({ "doc": doc }))["rev"], 0, "nothing reached the deck");
}

/// What a typical call costs, printed and checked: engine work runs on the
/// shell's UI thread (#399), so SERVICE.md asks a 30-slide deck to open and
/// draw its first slide in under a second, and a typical edit to answer in
/// well under 100 ms. Measure in release:
///
/// cargo test --release --locked -p octosense-deck-service --lib -- --ignored --nocapture timings
#[test]
#[ignore = "prints timings; run by hand, in release"]
fn timings_on_a_thirty_slide_deck() {
    let w = World::new();
    w.put("images/dot.png", &png());
    w.put("board.deckcraft", &native(&sample(30)));
    // The font database's first catalog is the process's, not the call's.
    deckcraft_fonts::FontDb::global().families();
    let mut report = Vec::new();
    let mut time = |label: &str, method: &str, args: Json| -> (Json, f64) {
        let started = Instant::now();
        let out = w.ok(method, args);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        report.push(format!("{label:<40} {ms:>8.1} ms"));
        (out, ms)
    };
    let started = Instant::now();
    let (open, open_ms) = time("open (30 slides)", "open", json!({ "path": "board.deckcraft" }));
    let doc = open["doc"].as_str().unwrap().to_string();
    let ids: Vec<u64> = w.slide_ids(&doc);
    let (_, first_ms) = time("its first slide at 1280 px", "view", json!({ "doc": doc, "slide": ids[0], "width": 1280 }));
    let open_and_first = started.elapsed().as_secs_f64() * 1000.0;
    let body = w.shape_where(&doc, ids[1], "text", "First point\nSecond point");
    let mut edits = Vec::new();
    edits.push(time("set_text (one word)", "set_text", json!({ "doc": doc, "slide": ids[1], "id": body, "text": "First point\nSecond item" })).1);
    edits.push(time("format (bold, size, colour)", "format", json!({ "doc": doc, "slide": ids[1], "ids": [body], "bold": true, "size": 24, "color": "#2E6FD8" })).1);
    edits.push(time("slide (inspect)", "slide", json!({ "doc": doc, "slide": ids[1] })).1);
    edits.push(time("slides (inspect)", "slides", json!({ "doc": doc })).1);
    edits.push(time("shape move", "shape", json!({ "doc": doc, "slide": ids[1], "ids": [body], "op": "move", "dx": 4, "dy": 4 })).1);
    edits.push(time("notes", "notes", json!({ "doc": doc, "slide": ids[1], "text": "Speak slowly" })).1);
    edits.push(time("hit", "hit", json!({ "doc": doc, "slide": ids[1], "at": [200, 200] })).1);
    edits.push(time("undo", "undo", json!({ "doc": doc })).1);
    edits.push(time("redo", "redo", json!({ "doc": doc })).1);
    time("view after an edit, 1280 px", "view", json!({ "doc": doc, "slide": ids[1], "width": 1280 }));
    time("thumbnail, 240 px", "view", json!({ "doc": doc, "slide": ids[2], "width": 240 }));
    time("insert picture", "insert", json!({ "doc": doc, "slide": ids[2], "kind": "picture", "path": "images/dot.png" }));
    time("insert table 4 × 4 with cells", "insert", json!({ "doc": doc, "slide": ids[3], "kind": "table", "rows": 4, "cols": 4, "cells": [["a", "b"], ["c", "d"]] }));
    time("design theme", "design", json!({ "doc": doc, "theme": "Harbor" }));
    time("layouts at 240 px", "layouts", json!({ "doc": doc }));
    time("save", "save", json!({ "doc": doc }));
    time("export pptx", "export", json!({ "doc": doc, "kind": "pptx", "out": "board.pptx" }));
    time("export pdf at 72 dpi", "export", json!({ "doc": doc, "kind": "pdf", "out": "board.pdf", "dpi": 72 }));
    time("cover", "cover", json!({ "path": "board.deckcraft" }));
    time("close", "close", json!({ "doc": doc }));
    report.push(format!("{:<40} {open_and_first:>8.1} ms", "open and first slide, together"));
    eprintln!("deck service timings:\n{}", report.join("\n"));
    assert!(open_and_first < 1000.0, "open {open_ms:.1} ms + first slide {first_ms:.1} ms");
    let slowest = edits.iter().cloned().fold(0.0, f64::max);
    assert!(slowest < 100.0, "a typical edit took {slowest:.1} ms");
}
