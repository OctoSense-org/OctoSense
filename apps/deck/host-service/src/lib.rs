//! `octosense-deck-service` — the `deck` host service (ADR 0013).
//!
//! deckcraft's presentation engine behind typed `deck.*` methods. Every
//! call is a fresh, stateless engine session. The service does all file
//! I/O itself and feeds the engine bytes, so the engine never touches a
//! path, and a link written inside a presentation (an external picture or
//! media relationship) is kept as a string and never followed. Everything
//! the service reads or writes lives in the call's area (ADR 0013,
//! 2026-10-08): the caller's own folder from the shell's resolver
//! ([`set_area_resolver`]), or without one the legacy `<host_dir>/deck`. A
//! write that may not replace (an agent's) only creates new files, within
//! the area's quota ([`Area::write`]). Everything is JSON at the boundary;
//! the engine's types never cross it.
//!
//! Methods (all under the `deck` family; paths relative to the call's area):
//! - `info {path}` → the deck inspected: slides with titles, layouts,
//!   shape counts, sections, theme (a `.pptx`, `.deckcraft` or outline text)
//! - `text {path}` → `{outline, slides}` — titles unindented, bullets
//!   tab-indented by level
//! - `render {path, slide?, out, max_side?}` → `{out, slide, width, height,
//!   bytes}` — one slide as a PNG written to `out`
//! - `new {out, slides: [{title, bullets?}]}` → `{out, slides, format}` —
//!   a deck written as `.pptx` or `.deckcraft`
//! - `convert {path, out}` → `{out, format, bytes}` — `.pptx`,
//!   `.deckcraft`, outline `.txt` or `.pdf`, by `out`'s extension
//!
//! The service serves system apps only until ADR 0013's store capability
//! is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::path::{Component, Path, PathBuf};

use deckcraft_engine::cmd::file as engine_file;
use deckcraft_engine::Session;
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};

/// The longest preview edge `render` produces.
const MAX_RENDER_SIDE: u64 = 4096;
/// The largest file the service reads or writes (bytes).
const MAX_DECK_BYTES: u64 = 64 << 20;
/// `new` builds at most this many slides per call.
const MAX_NEW_SLIDES: usize = 200;
/// `new` takes at most this many bullets per slide.
const MAX_BULLETS: usize = 64;
/// `new` takes titles and bullets of at most this many characters.
const MAX_LINE_CHARS: usize = 2000;

/// Which apps may call the service: system apps, as News and Sheets.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct DeckService;

mod warm;
pub use warm::warm;

/// Register the `deck` service with App Hub's host-service registry, and
/// start paying the engine's first-call cost on a thread of its own
/// ([`warm`]).
pub fn register() {
    register_host_service(Box::new(DeckService));
    warm();
}

/// The shell's resolver: where each call works (`None` removes it, and
/// calls work in the legacy `<host dir>/deck` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
}

/// The `deck.*` agent tools (ADR 0013, wave 2), in App Hub's `tools.json`
/// shape: the shell declares them for the virtual owner `os.deck` and grants
/// the system agent its reviewed share (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`).
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for DeckService {
    fn family(&self) -> &'static str {
        "deck"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        reply.send(serve(&AREAS, &call));
    }
}

/// One call, in the area `areas` gives it.
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The deck service serves system apps only.".into());
    }
    let area = areas.area(call, "deck").map_err(|e| format!("deck: {e}"))?;
    dispatch_in(call.method(), &call.args, &area)
        .map(|answer| area.relative_json(answer))
        .map_err(|error| area.relative_text(&error))
}

fn dispatch_in(method: &str, args: &Json, area: &Area) -> Result<Json, String> {
    match method {
        "info" => info(args, area),
        "text" => text(args, area),
        "render" => render(args, area),
        "new" => new_deck(args, area),
        "convert" => convert(args, area),
        other => Err(format!("deck.{other} is not a method of the deck service")),
    }
}

/// [`dispatch_in`] in the legacy area `<host_dir>/deck`, as a call without
/// the shell's resolver works.
#[cfg(test)]
fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    let area = Area::legacy(host_dir, "deck");
    std::fs::create_dir_all(&area.root).map_err(|e| format!("deck: {e}"))?;
    dispatch_in(method, args, &area)
}

/// A path strictly inside the call's area: relative, no `..`, no absolute
/// or prefix component; the resolved path stays under the area even
/// through symlinks.
fn contained(area: &Area, rel: &str, method: &str, key: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("deck.{method}: `{key}` is required"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("deck.{method}: `{key}` stays inside this call's folder"));
    }
    let root = &area.root;
    let joined = root.join(rel_path);
    let check_root = root.canonicalize().map_err(|e| format!("deck.{method}: folder: {e}"))?;
    let deepest = {
        let mut p = joined.clone();
        while !p.exists() {
            match p.parent() {
                Some(parent) => p = parent.to_path_buf(),
                None => break,
            }
        }
        p
    };
    let resolved = deepest.canonicalize().map_err(|e| format!("deck.{method}: {e}"))?;
    if !resolved.starts_with(&check_root) {
        return Err(format!("deck.{method}: `{key}` stays inside this call's folder"));
    }
    Ok(joined)
}

/// A contained output path the call may write: refused before the engine
/// works when the area's rules would refuse it (an existing file for a call
/// that may not replace).
fn out_path(area: &Area, rel: &str, method: &str) -> Result<PathBuf, String> {
    let out = contained(area, rel, method, "out")?;
    area.check(&out, 0).map_err(|e| format!("deck.{method}: {e}"))?;
    Ok(out)
}

/// Read `args.path` from the call's area and seat it in a fresh engine
/// session (the engine gets bytes, never a path). Reads `.pptx`,
/// `.deckcraft` and outline `.txt`/`.md`, by content.
fn read_deck(args: &Json, area: &Area, method: &str) -> Result<(Session, String), String> {
    let rel = args["path"].as_str().unwrap_or("").to_string();
    let path = contained(area, &rel, method, "path")?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("deck.{method}: {rel}: {e}"))?;
    if meta.len() > MAX_DECK_BYTES {
        return Err(format!("deck.{method}: the file is larger than the service reads"));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("deck.{method}: {rel}: {e}"))?;
    let doc = engine_file::open_presentation(&rel, &bytes).map_err(|e| format!("deck.{method}: {e}"))?;
    let mut s = Session::new();
    engine_file::add_opened(&mut s, &rel, None, doc);
    Ok((s, rel))
}

/// Write engine output into the call's area, capped like reads are, under
/// the area's rules ([`Area::write`]).
fn write_out(area: &Area, path: &Path, bytes: &[u8], method: &str) -> Result<(), String> {
    if bytes.len() as u64 > MAX_DECK_BYTES {
        return Err(format!("deck.{method}: the result is larger than the service writes"));
    }
    area.write(path, bytes).map_err(|e| format!("deck.{method}: {e}"))
}

/// The output format `rel`'s extension names, confined to `allowed`
/// `(extension, engine format)` pairs — never the engine's silent
/// default.
fn out_format(rel: &str, method: &str, allowed: &[(&str, &'static str)]) -> Result<&'static str, String> {
    let lower = rel.to_ascii_lowercase();
    for (ext, format) in allowed {
        if lower.ends_with(ext) {
            return Ok(format);
        }
    }
    let exts: Vec<&str> = allowed.iter().map(|(e, _)| *e).collect();
    Err(format!("deck.{method}: `out` ends in one of {}", exts.join(", ")))
}

/// `info {path}` — the engine's `document.inspect` as JSON.
fn info(args: &Json, area: &Area) -> Result<Json, String> {
    let (mut s, rel) = read_deck(args, area, "info")?;
    let mut v = s.execute("document.inspect", &json!({})).map_err(|e| format!("deck.info: {e}"))?;
    v["file"] = json!(rel);
    Ok(v)
}

/// `text {path}` — the deck as outline text (titles unindented, body
/// paragraphs tab-indented by level).
fn text(args: &Json, area: &Area) -> Result<Json, String> {
    let (s, _rel) = read_deck(args, area, "text")?;
    let st = s.doc().map_err(|e| format!("deck.text: {e}"))?;
    Ok(json!({"outline": deckcraft_format::slides_to_outline(&st.doc), "slides": st.doc.slides.len()}))
}

/// `render {path, slide?, out, max_side?}` — one slide as a PNG whose
/// longest edge is `max_side` (default 1024, at most 4096).
fn render(args: &Json, area: &Area) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    if !out_rel.to_ascii_lowercase().ends_with(".png") {
        return Err("deck.render: `out` is a .png path".into());
    }
    let out = out_path(area, out_rel, "render")?;
    let slide = args["slide"].as_u64().unwrap_or(0) as usize;
    let max_side = args["max_side"].as_u64().unwrap_or(1024).clamp(16, MAX_RENDER_SIDE) as f64;
    let (s, _rel) = read_deck(args, area, "render")?;
    let st = s.doc().map_err(|e| format!("deck.render: {e}"))?;
    let n = st.doc.slides.len();
    if slide >= n {
        return Err(format!("deck.render: no slide {slide} (the deck has {n})"));
    }
    let longest = st.doc.slide_size.width.max(st.doc.slide_size.height).max(1.0);
    let (png, width, height) = engine_file::render_png(&st.doc, slide, max_side / longest, false);
    write_out(area, &out, &png, "render")?;
    Ok(json!({"out": out_rel, "slide": slide, "width": width, "height": height, "bytes": png.len()}))
}

/// `new {out, slides: [{title, bullets?}]}` — a deck built from titles
/// and flat bullet lists, written as `.pptx` or `.deckcraft`.
fn new_deck(args: &Json, area: &Area) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    let format = out_format(out_rel, "new", &[(".pptx", "pptx"), (".deckcraft", "deckcraft")])?;
    let out = out_path(area, out_rel, "new")?;
    let slides = args["slides"].as_array().ok_or("deck.new: `slides` is a list of {title, bullets?}")?;
    if slides.is_empty() || slides.len() > MAX_NEW_SLIDES {
        return Err(format!("deck.new: `slides` is 1..={MAX_NEW_SLIDES} slides"));
    }
    // One outline line per title and bullet (the engine's own outline
    // import); line breaks and tabs inside a line become spaces.
    let clean = |s: &str, what: &str| -> Result<String, String> {
        if s.chars().count() > MAX_LINE_CHARS {
            return Err(format!("deck.new: a {what} is at most {MAX_LINE_CHARS} characters"));
        }
        Ok(s.replace(['\n', '\r', '\t'], " ").trim().to_string())
    };
    let mut outline = String::new();
    for slide in slides {
        let title = clean(slide["title"].as_str().unwrap_or(""), "title")?;
        if title.is_empty() {
            return Err("deck.new: each slide has a non-empty `title`".into());
        }
        outline.push_str(&title);
        outline.push('\n');
        let bullets: &[Json] = match &slide["bullets"] {
            Json::Null => &[],
            Json::Array(b) => b,
            other => return Err(format!("deck.new: `bullets` is a list of strings, not {other}")),
        };
        if bullets.len() > MAX_BULLETS {
            return Err(format!("deck.new: at most {MAX_BULLETS} bullets per slide"));
        }
        for bullet in bullets {
            let line = clean(bullet.as_str().ok_or("deck.new: `bullets` is a list of strings")?, "bullet")?;
            if line.is_empty() {
                continue;
            }
            outline.push('\t');
            outline.push_str(&line);
            outline.push('\n');
        }
    }
    let mut p = deckcraft_model::defaults::blank_presentation(deckcraft_model::defaults::WIDE, Default::default(), false);
    let made = deckcraft_format::outline_to_slides(&mut p, &outline);
    let bytes = engine_file::save_bytes(&p, format).map_err(|e| format!("deck.new: {e}"))?;
    write_out(area, &out, &bytes, "new")?;
    Ok(json!({"out": out_rel, "slides": made, "format": format}))
}

/// `convert {path, out}` — the deck re-encoded by `out`'s extension:
/// `.pptx`, `.deckcraft`, outline `.txt` or `.pdf`.
fn convert(args: &Json, area: &Area) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    let format = out_format(
        out_rel,
        "convert",
        &[(".pptx", "pptx"), (".deckcraft", "deckcraft"), (".txt", "outline"), (".pdf", "pdf")],
    )?;
    let out = out_path(area, out_rel, "convert")?;
    let (s, _rel) = read_deck(args, area, "convert")?;
    let st = s.doc().map_err(|e| format!("deck.convert: {e}"))?;
    let bytes = engine_file::save_bytes(&st.doc, format).map_err(|e| format!("deck.convert: {e}"))?;
    write_out(area, &out, &bytes, "convert")?;
    Ok(json!({"out": out_rel, "format": format, "bytes": bytes.len()}))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real two-slide deck written through the engine's own pptx
    /// export: the fixture every test reads back.
    fn make(host: &Path) -> &'static str {
        let made = dispatch(
            "new",
            &json!({"out": "talk.pptx", "slides": [
                {"title": "Why decks", "bullets": ["One engine", "No UI"]},
                {"title": "How it ports"}
            ]}),
            host,
        )
        .unwrap();
        assert_eq!(made["slides"], json!(2), "{made}");
        assert_eq!(made["format"], json!("pptx"));
        "talk.pptx"
    }

    #[test]
    fn new_writes_a_real_pptx_into_the_family_area() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let rel = make(host);
        let on_disk = host.join("deck").join(rel);
        let bytes = std::fs::read(&on_disk).unwrap();
        assert!(bytes.starts_with(b"PK"), "a real zip-based pptx under <host_dir>/deck");

        assert!(dispatch("new", &json!({"out": "x.exe", "slides": [{"title": "t"}]}), host).is_err(), "only .pptx/.deckcraft");
        assert!(dispatch("new", &json!({"out": "x.pptx", "slides": []}), host).is_err(), "no empty deck");
        assert!(dispatch("new", &json!({"out": "x.pptx", "slides": [{"title": "  "}]}), host).is_err(), "no blank title");
    }

    #[test]
    fn info_reads_slides_and_titles() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let rel = make(host);
        let v = dispatch("info", &json!({"path": rel}), host).unwrap();
        assert_eq!(v["file"], json!(rel));
        let slides = v["slides"].as_array().unwrap();
        assert_eq!(slides.len(), 2, "{v}");
        assert_eq!(slides[0]["title"], json!("Why decks"));
        assert_eq!(slides[1]["title"], json!("How it ports"));
    }

    #[test]
    fn text_extracts_the_outline() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let rel = make(host);
        let v = dispatch("text", &json!({"path": rel}), host).unwrap();
        assert_eq!(v["slides"], json!(2));
        let outline = v["outline"].as_str().unwrap();
        assert!(outline.contains("Why decks\n\tOne engine\n\tNo UI\nHow it ports"), "{outline:?}");
    }

    #[test]
    fn render_writes_a_png_preview() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let rel = make(host);
        let v = dispatch("render", &json!({"path": rel, "slide": 1, "out": "prev/s2.png", "max_side": 256}), host).unwrap();
        assert_eq!(v["slide"], json!(1));
        let (w, h) = (v["width"].as_u64().unwrap(), v["height"].as_u64().unwrap());
        assert_eq!(w.max(h), 256, "{v}");
        assert!(w.min(h) > 0);
        let png = std::fs::read(host.join("deck").join("prev/s2.png")).unwrap();
        assert_eq!(v["bytes"].as_u64().unwrap(), png.len() as u64);
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']), "a real PNG");

        assert!(dispatch("render", &json!({"path": rel, "slide": 9, "out": "x.png"}), host).is_err(), "no slide 9");
        assert!(dispatch("render", &json!({"path": rel, "out": "x.jpg"}), host).is_err(), "PNG only");
    }

    #[test]
    fn convert_roundtrips_native_outline_and_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let rel = make(host);

        dispatch("convert", &json!({"path": rel, "out": "talk.deckcraft"}), host).unwrap();
        let back = dispatch("info", &json!({"path": "talk.deckcraft"}), host).unwrap();
        assert_eq!(back["slides"].as_array().unwrap().len(), 2, "pptx → native roundtrip");

        dispatch("convert", &json!({"path": rel, "out": "talk.txt"}), host).unwrap();
        let txt = std::fs::read_to_string(host.join("deck/talk.txt")).unwrap();
        assert!(txt.contains("Why decks"), "{txt:?}");

        dispatch("convert", &json!({"path": rel, "out": "talk.pdf"}), host).unwrap();
        assert!(std::fs::read(host.join("deck/talk.pdf")).unwrap().starts_with(b"%PDF"), "a real PDF");

        assert!(dispatch("convert", &json!({"path": rel, "out": "talk.bin"}), host).is_err(), "no silent native default");
    }

    #[test]
    fn paths_stay_inside_the_deck_area() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        // A neighbouring family's data in the shared host dir must stay
        // out of reach.
        std::fs::create_dir_all(host.join("calendar")).unwrap();
        std::fs::write(host.join("calendar/events.json"), b"[]").unwrap();
        make(host);
        for bad in ["../up.pptx", "/etc/x.pptx", "a/../../up.pptx", "../calendar/events.json", ""] {
            assert!(dispatch("info", &json!({"path": bad}), host).is_err(), "read {bad}");
            assert!(dispatch("new", &json!({"out": bad, "slides": [{"title": "t"}]}), host).is_err(), "write {bad}");
            assert!(dispatch("render", &json!({"path": "talk.pptx", "out": bad}), host).is_err(), "render to {bad}");
            assert!(dispatch("convert", &json!({"path": "talk.pptx", "out": bad}), host).is_err(), "convert to {bad}");
        }
        assert_eq!(std::fs::read(host.join("calendar/events.json")).unwrap(), b"[]", "untouched");
    }

    /// A call as App Hub hands it to the service.
    fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("deck.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
    }

    /// A resolver shaped like the shell's: every call works in `root`, an
    /// app's own foreground call may replace a file and an agent's may not,
    /// within `quota`.
    fn resolver(root: &Path, quota: Option<u64>) -> Slot {
        let slot = Slot::new();
        let root = root.to_path_buf();
        slot.set(Some(std::sync::Arc::new(move |call: &ServiceCall| Ok(Area::new(&root, quota, call.may_prompt)))));
        slot
    }

    fn slides() -> Json {
        json!([{"title": "Why decks", "bullets": ["One engine"]}, {"title": "How it ports"}])
    }

    /// Without the shell's resolver a call works in `<host dir>/deck`, as
    /// before, and may replace.
    #[test]
    fn without_a_resolver_the_area_is_the_deck_subdir() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        serve(&Slot::new(), &service_call("new", json!({"out": "talk.pptx", "slides": slides()}), host, false)).unwrap();
        assert!(host.join("deck/talk.pptx").is_file(), "writes land inside the area");
        assert!(!host.join("talk.pptx").exists(), "never beside it");
        serve(&Slot::new(), &service_call("new", json!({"out": "talk.pptx", "slides": slides()}), host, false)).unwrap();
    }

    /// With the shell's resolver every path is relative to the caller's own
    /// folder and stays inside it, through a link too.
    #[test]
    fn the_resolver_root_is_used_and_paths_stay_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        let host = dir.path().join(".host");
        serve(&areas, &service_call("new", json!({"out": "talks/t.pptx", "slides": slides()}), &host, false)).unwrap();
        assert!(root.join("talks/t.pptx").is_file() && !host.exists() && !root.join("deck").exists());
        let v = serve(&areas, &service_call("info", json!({"path": "talks/t.pptx"}), &host, false)).unwrap();
        assert_eq!(v["slides"].as_array().unwrap().len(), 2, "{v}");
        std::fs::write(dir.path().join("beside.pptx"), b"x").unwrap();
        for bad in ["../beside.pptx", "/etc/hosts", "talks/../../beside.pptx"] {
            assert!(serve(&areas, &service_call("info", json!({"path": bad}), &host, false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("convert", json!({"path": "talks/t.pptx", "out": bad}), &host, true)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("out")).unwrap();
            assert!(serve(&areas, &service_call("render", json!({"path": "talks/t.pptx", "out": "out/s.png"}), &host, true)).is_err());
            assert!(!dir.path().join("s.png").exists());
        }
    }

    /// An agent's call never replaces a file, before the engine runs; an
    /// app's own foreground call may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        serve(&areas, &service_call("new", json!({"out": "t.pptx", "slides": slides()}), dir.path(), false)).unwrap();
        std::fs::write(dir.path().join("t.txt"), b"keep me").unwrap();
        for (method, args) in [
            ("convert", json!({"path": "t.pptx", "out": "t.txt"})),
            ("new", json!({"out": "t.pptx", "slides": slides()})),
        ] {
            let refused = serve(&areas, &service_call(method, args, dir.path(), false)).unwrap_err();
            assert!(refused.contains("already exists"), "{method}: {refused}");
        }
        std::fs::write(dir.path().join("s.png"), b"keep").unwrap();
        assert!(serve(&areas, &service_call("render", json!({"path": "t.pptx", "out": "s.png"}), dir.path(), false)).is_err());
        assert_eq!(std::fs::read(dir.path().join("t.txt")).unwrap(), b"keep me");
        serve(&areas, &service_call("convert", json!({"path": "t.pptx", "out": "t.txt"}), dir.path(), true)).unwrap();
        assert!(std::fs::read_to_string(dir.path().join("t.txt")).unwrap().contains("Why decks"));
    }

    /// What a call writes must fit what is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let refused = serve(&resolver(dir.path(), Some(100)), &service_call("new", json!({"out": "t.pptx", "slides": slides()}), dir.path(), true)).unwrap_err();
        assert!(refused.contains("bytes left"), "{refused}");
        assert!(!dir.path().join("t.pptx").exists());
        serve(&resolver(dir.path(), Some(1 << 22)), &service_call("new", json!({"out": "t.pptx", "slides": slides()}), dir.path(), true)).unwrap();
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.slides"));
        assert!(!may_call("org.example.anything"));
        assert!(!may_call(""));
    }

    /// The agent tools (`tools.json`) pass App Hub's own loader, as the shell
    /// reads them, keep the object schemas octos takes both ways, and name
    /// only methods this service dispatches.
    #[test]
    fn the_agent_tools_pass_app_hubs_loader_and_name_real_methods() {
        use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest};
        let (manifest, _) = ToolManifest::load(TOOLS_JSON, "deck", ToolHost::Contained, false).unwrap();
        assert!(!manifest.tools.is_empty());
        let dir = tempfile::tempdir().unwrap();
        for tool in &manifest.tools {
            assert_eq!(tool.implemented_by, ImplementedBy::HostService, "{}", tool.name);
            assert!(tool.host_method.is_none(), "{}: the shell routes each tool to the method of its own name", tool.name);
            for schema in [&tool.input_schema, &tool.output_schema] {
                assert_eq!(schema["type"], json!("object"), "{}: octos takes object schemas only", tool.name);
            }
            assert!(tool.description.is_ascii(), "{}: descriptions stay within octos's byte limit", tool.name);
            let method = tool.name.strip_prefix("deck.").unwrap();
            if let Err(e) = dispatch(method, &json!({}), dir.path()) {
                assert!(!e.contains("is not a method"), "{}: {e}", tool.name);
            }
        }
    }
}
