//! `octosense-deck-service` — the `deck` host service (ADR 0013).
//!
//! deckcraft's presentation engine behind typed `deck.*` methods. Every
//! call is a fresh, stateless engine session. The service does all file
//! I/O itself and feeds the engine bytes, so the engine never touches a
//! path; everything the service reads or writes lives in the family's
//! own corner of the caller's host directory (`<host_dir>/deck`) — the
//! shared `.host` also holds Mail's and Calendar's data, which this
//! service must never reach. Everything is JSON at the boundary; the
//! engine's types never cross it.
//!
//! Methods (all under the `deck` family; paths relative to `<host_dir>/deck`):
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

/// Register the `deck` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(DeckService));
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
        if !may_call(&call.app_id) {
            reply.send(Err("The deck service serves system apps only.".into()));
            return;
        }
        let method = call.method().to_string();
        let args = call.args.clone();
        let host_dir = call.host_dir.clone();
        reply.send(dispatch(&method, &args, &host_dir));
    }
}

fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    match method {
        "info" => info(args, host_dir),
        "text" => text(args, host_dir),
        "render" => render(args, host_dir),
        "new" => new_deck(args, host_dir),
        "convert" => convert(args, host_dir),
        other => Err(format!("deck.{other} is not a method of the deck service")),
    }
}

/// The family's own corner of the host directory: `<host_dir>/deck`,
/// created on first use. The shared `.host` also holds other services'
/// data (Mail's, Calendar's), so nothing the deck service reads or
/// writes may leave this subdirectory.
fn area(host_dir: &Path) -> Result<PathBuf, String> {
    let dir = host_dir.join("deck");
    std::fs::create_dir_all(&dir).map_err(|e| format!("deck: {e}"))?;
    Ok(dir)
}

/// A path strictly inside the family area: relative, no `..`, no absolute
/// or prefix component; the resolved path stays under `<host_dir>/deck`
/// even through symlinks.
fn contained(host_dir: &Path, rel: &str, method: &str, key: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("deck.{method}: `{key}` is required"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("deck.{method}: `{key}` stays inside the app's deck directory"));
    }
    let root = area(host_dir)?;
    let joined = root.join(rel_path);
    let check_root = root.canonicalize().map_err(|e| format!("deck.{method}: deck dir: {e}"))?;
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
        return Err(format!("deck.{method}: `{key}` stays inside the app's deck directory"));
    }
    Ok(joined)
}

/// Read `args.path` from the family area and seat it in a fresh engine
/// session (the engine gets bytes, never a path). Reads `.pptx`,
/// `.deckcraft` and outline `.txt`/`.md`, by content.
fn read_deck(args: &Json, host_dir: &Path, method: &str) -> Result<(Session, String), String> {
    let rel = args["path"].as_str().unwrap_or("").to_string();
    let path = contained(host_dir, &rel, method, "path")?;
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

/// Write engine output into the family area, capped like reads are.
fn write_out(path: &Path, bytes: &[u8], method: &str) -> Result<(), String> {
    if bytes.len() as u64 > MAX_DECK_BYTES {
        return Err(format!("deck.{method}: the result is larger than the service writes"));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("deck.{method}: {e}"))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("deck.{method}: {e}"))
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
fn info(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let (mut s, rel) = read_deck(args, host_dir, "info")?;
    let mut v = s.execute("document.inspect", &json!({})).map_err(|e| format!("deck.info: {e}"))?;
    v["file"] = json!(rel);
    Ok(v)
}

/// `text {path}` — the deck as outline text (titles unindented, body
/// paragraphs tab-indented by level).
fn text(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let (s, _rel) = read_deck(args, host_dir, "text")?;
    let st = s.doc().map_err(|e| format!("deck.text: {e}"))?;
    Ok(json!({"outline": deckcraft_format::slides_to_outline(&st.doc), "slides": st.doc.slides.len()}))
}

/// `render {path, slide?, out, max_side?}` — one slide as a PNG whose
/// longest edge is `max_side` (default 1024, at most 4096).
fn render(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    if !out_rel.to_ascii_lowercase().ends_with(".png") {
        return Err("deck.render: `out` is a .png path".into());
    }
    let out = contained(host_dir, out_rel, "render", "out")?;
    let slide = args["slide"].as_u64().unwrap_or(0) as usize;
    let max_side = args["max_side"].as_u64().unwrap_or(1024).clamp(16, MAX_RENDER_SIDE) as f64;
    let (s, _rel) = read_deck(args, host_dir, "render")?;
    let st = s.doc().map_err(|e| format!("deck.render: {e}"))?;
    let n = st.doc.slides.len();
    if slide >= n {
        return Err(format!("deck.render: no slide {slide} (the deck has {n})"));
    }
    let longest = st.doc.slide_size.width.max(st.doc.slide_size.height).max(1.0);
    let (png, width, height) = engine_file::render_png(&st.doc, slide, max_side / longest, false);
    write_out(&out, &png, "render")?;
    Ok(json!({"out": out_rel, "slide": slide, "width": width, "height": height, "bytes": png.len()}))
}

/// `new {out, slides: [{title, bullets?}]}` — a deck built from titles
/// and flat bullet lists, written as `.pptx` or `.deckcraft`.
fn new_deck(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    let format = out_format(out_rel, "new", &[(".pptx", "pptx"), (".deckcraft", "deckcraft")])?;
    let out = contained(host_dir, out_rel, "new", "out")?;
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
    write_out(&out, &bytes, "new")?;
    Ok(json!({"out": out_rel, "slides": made, "format": format}))
}

/// `convert {path, out}` — the deck re-encoded by `out`'s extension:
/// `.pptx`, `.deckcraft`, outline `.txt` or `.pdf`.
fn convert(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    let format = out_format(
        out_rel,
        "convert",
        &[(".pptx", "pptx"), (".deckcraft", "deckcraft"), (".txt", "outline"), (".pdf", "pdf")],
    )?;
    let out = contained(host_dir, out_rel, "convert", "out")?;
    let (s, _rel) = read_deck(args, host_dir, "convert")?;
    let st = s.doc().map_err(|e| format!("deck.convert: {e}"))?;
    let bytes = engine_file::save_bytes(&st.doc, format).map_err(|e| format!("deck.convert: {e}"))?;
    write_out(&out, &bytes, "convert")?;
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

    #[test]
    fn the_family_area_is_its_own_subdir() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let a = area(host).unwrap();
        assert_eq!(a, host.join("deck"));
        assert!(a.is_dir(), "created on first use");
        make(host);
        assert!(host.join("deck/talk.pptx").is_file(), "writes land inside the area");
        assert!(!host.join("talk.pptx").exists(), "never beside it");
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
