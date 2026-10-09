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
//! - `run {path?, cmds: [{id, params?}], out?, slide?, max_side?}` →
//!   `{results, out, format?, bytes?, slide?, width?, height?}` — the
//!   command door (ADR 0013, #418): run commands of deckcraft's registry on
//!   the deck at `path`, or on a new blank one, then write it to `out` as
//!   `convert` does (`.pptx`, `.deckcraft`, outline `.txt`, `.pdf`) or one
//!   slide as `render` does (`.png`). Only what the door's allowlist admits
//!   runs ([`door`]): commands the reviewed classification
//!   (`skill/safety.json`) classes `safe`, and the four reviewed media reads,
//!   whose `path` must name a file inside the area; every other id is
//!   refused before any command runs. Those reads are the one place the
//!   engine opens a file itself: the door hands it the file's resolved
//!   absolute path, and caps what one call's reads may total.
//!
//! The service serves system apps only until ADR 0013's store capability
//! is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use deckcraft_engine::cmd::file as engine_file;
use deckcraft_engine::Session;
use deckcraft_model::Presentation;
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::door::{Door, FileRead, Reviewed};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};

/// The longest preview edge `render` produces.
const MAX_RENDER_SIDE: u64 = 4096;
/// The largest file the service reads or writes (bytes). (What one `run`
/// call's reviewed reads may total is the gate's: the same 64 MiB,
/// `octosense_engine_area::door::MAX_READ_BYTES`.)
const MAX_DECK_BYTES: u64 = 64 << 20;
/// `new` builds at most this many slides per call.
const MAX_NEW_SLIDES: usize = 200;
/// `new` takes at most this many bullets per slide.
const MAX_BULLETS: usize = 64;
/// `new` takes titles and bullets of at most this many characters.
const MAX_LINE_CHARS: usize = 2000;

/// What `convert` writes, by `out`'s extension.
const CONVERT_OUT: &[(&str, &str)] = &[(".pptx", "pptx"), (".deckcraft", "deckcraft"), (".txt", "outline"), (".pdf", "pdf")];
/// What `run` writes: `convert`'s formats, and one slide as `render` draws it.
const RUN_OUT: &[(&str, &str)] = &[(".pptx", "pptx"), (".deckcraft", "deckcraft"), (".txt", "outline"), (".pdf", "pdf"), (".png", "png")];

/// What the deck engine's reviewer settled for the door beyond the classes:
/// four `file` commands that only read the file their `path` names (whole,
/// with `std::fs::read`) and embed its bytes in the deck, or take it inline
/// as base64 `data`, which wins when both are given. All four go through
/// `insert::media_bytes` (`cmd/insert.rs` 212-224), their only file access:
/// `insert.picture` (`picture`, 295-365: decoded and sized in memory),
/// `insert.audio` and `insert.video` (`media`, 445-508: probed and given a
/// poster frame in memory; the unplayable-codec status line they queue is
/// never drained headless) and `picture.change` (`cmd/shape.rs` 650-661).
/// `shape.fill` and `design.background` are deliberately not listed: a
/// non-string `picture` makes them read an undocumented `path`. No setter
/// or inner id: no `safe` deck command sets an app-wide variable by key or
/// names another command (every nested `execute` in the engine runs a fixed
/// id), and animation effects are built-in presets, never plug-ins.
static REVIEWED: Reviewed = Reviewed {
    file_reads: &[
        FileRead { id: "insert.picture", params: &["path"] },
        FileRead { id: "insert.audio", params: &["path"] },
        FileRead { id: "insert.video", params: &["path"] },
        FileRead { id: "picture.change", params: &["path"] },
    ],
    setters: &[],
    inner: &[],
};

/// The command door's gate: deckcraft's reviewed classification
/// (`skill/safety.json`, generated and drift-checked by `tests/skill.rs`)
/// and [`REVIEWED`].
pub fn door() -> Result<&'static Door, String> {
    static DOOR: OnceLock<Result<Door, String>> = OnceLock::new();
    DOOR.get_or_init(|| Door::new("deck", include_str!("../skill/safety.json"), &REVIEWED)).as_ref().map_err(Clone::clone)
}

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
        "run" => run(args, area),
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

/// Write `doc` to `out` encoded as `format` (`new`, `convert`, `run`):
/// the bytes written.
fn write_deck(area: &Area, out: &Path, doc: &Presentation, format: &str, method: &str) -> Result<usize, String> {
    let bytes = engine_file::save_bytes(doc, format).map_err(|e| format!("deck.{method}: {e}"))?;
    write_out(area, out, &bytes, method)?;
    Ok(bytes.len())
}

/// Write slide `args.slide` (0-based, default 0) of `doc` to `out` as a
/// PNG whose longest edge is `args.max_side` (default 1024, 16..=4096)
/// (`render`, `run`): `{slide, width, height, bytes}`.
fn write_slide(area: &Area, out: &Path, doc: &Presentation, args: &Json, method: &str) -> Result<Json, String> {
    let slide = args["slide"].as_u64().unwrap_or(0) as usize;
    let max_side = args["max_side"].as_u64().unwrap_or(1024).clamp(16, MAX_RENDER_SIDE) as f64;
    let n = doc.slides.len();
    if slide >= n {
        return Err(format!("deck.{method}: no slide {slide} (the deck has {n})"));
    }
    let longest = doc.slide_size.width.max(doc.slide_size.height).max(1.0);
    let (png, width, height) = engine_file::render_png(doc, slide, max_side / longest, false);
    write_out(area, out, &png, method)?;
    Ok(json!({"slide": slide, "width": width, "height": height, "bytes": png.len()}))
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
    let (s, _rel) = read_deck(args, area, "render")?;
    let st = s.doc().map_err(|e| format!("deck.render: {e}"))?;
    let mut answer = write_slide(area, &out, &st.doc, args, "render")?;
    answer["out"] = json!(out_rel);
    Ok(answer)
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
    write_deck(area, &out, &p, format, "new")?;
    Ok(json!({"out": out_rel, "slides": made, "format": format}))
}

/// `convert {path, out}` — the deck re-encoded by `out`'s extension:
/// `.pptx`, `.deckcraft`, outline `.txt` or `.pdf`.
fn convert(args: &Json, area: &Area) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    let format = out_format(out_rel, "convert", CONVERT_OUT)?;
    let out = out_path(area, out_rel, "convert")?;
    let (s, _rel) = read_deck(args, area, "convert")?;
    let st = s.doc().map_err(|e| format!("deck.convert: {e}"))?;
    let bytes = write_deck(area, &out, &st.doc, format, "convert")?;
    Ok(json!({"out": out_rel, "format": format, "bytes": bytes}))
}

/// The command door: every command admitted by [`door`] before the engine
/// runs any (a refused one refuses the whole call, with nothing written),
/// run in order on one session over the deck at `path` or a new blank one
/// (the engine's own `file.new {blank: true}`: 16:9, the default theme, no
/// slides), whose active deck is then written to `out` under the area's
/// rules: by `out`'s extension as `convert` writes it, or one slide as
/// `render` draws it (`.png`, with `slide` and `max_side`).
fn run(args: &Json, area: &Area) -> Result<Json, String> {
    // Admit every command first: one refused id refuses the whole call, with
    // nothing opened and nothing written.
    let admitted = door()?.admit_all(&args["cmds"], area)?;
    let out = match args["out"].as_str().filter(|o| !o.is_empty()) {
        Some(rel) => {
            let format = out_format(rel, "run", RUN_OUT)?;
            Some((rel, out_path(area, rel, "run")?, format))
        }
        None => None,
    };
    let mut s = match args["path"].as_str().filter(|p| !p.is_empty()) {
        Some(_) => read_deck(args, area, "run")?.0,
        None => {
            let mut s = Session::new();
            s.execute("file.new", &json!({"blank": true})).map_err(|e| format!("deck.run: {e}"))?;
            s
        }
    };
    let mut results = Vec::with_capacity(admitted.len());
    for (id, params) in admitted {
        let r = s.execute(&id, &params).map_err(|e| format!("deck.run {id}: {e}"))?;
        results.push(json!({"id": id, "result": r}));
    }
    let Some((out_rel, out, format)) = out else { return Ok(json!({"results": results, "out": Json::Null})) };
    let st = s.doc().map_err(|e| format!("deck.run: {e}"))?;
    let mut answer = match format {
        "png" => write_slide(area, &out, &st.doc, args, "run")?,
        _ => json!({"bytes": write_deck(area, &out, &st.doc, format, "run")?}),
    };
    answer["results"] = json!(results);
    answer["out"] = json!(out_rel);
    answer["format"] = json!(format);
    Ok(answer)
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

    /// A 12x8 RGB PNG (two colour bands), as the photo service's tests use.
    fn png() -> Vec<u8> {
        const PNG: &str = "89504e470d0a1a0a0000000d494844520000000c000000080802000000428689a60000001d49444154789c6378616383866c725ea021063a2bb279d14310d15911005b9497817c6155610000000049454e44ae426082";
        (0..PNG.len()).step_by(2).map(|i| u8::from_str_radix(&PNG[i..i + 2], 16).unwrap()).collect()
    }

    /// Half a second of 8 kHz mono PCM as a WAV, as the engine's media tests build it.
    fn wav() -> Vec<u8> {
        let rate = 8000u32;
        let data: Vec<u8> = (0..rate / 2).flat_map(|i| (((i as f32 * 0.3).sin() * 3000.0) as i16).to_le_bytes()).collect();
        let mut b = b"RIFF".to_vec();
        b.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 2).to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(&data);
        b
    }

    /// The door runs allowlisted commands in a temporary area and writes a
    /// new deck: `safe` commands build it from a blank one, and nothing
    /// outside the area is touched.
    #[test]
    fn the_door_runs_allowlisted_commands_in_its_area() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let made = serve(
            &areas,
            &service_call(
                "run",
                json!({"cmds": [
                    {"id": "slide.new", "params": {"layout": "title", "title": "Quarterly review"}},
                    {"id": "slide.new", "params": {"title": "Revenue", "body": "Grew twelve percent"}},
                    {"id": "document.inspect"}
                ], "out": "review.pptx"}),
                dir.path(),
                false,
            ),
        )
        .unwrap();
        assert_eq!(made["out"], json!("review.pptx"), "{made}");
        assert_eq!(made["format"], json!("pptx"));
        assert_eq!(made["results"].as_array().unwrap().len(), 3);
        assert_eq!(made["results"][2]["result"]["slides"].as_array().unwrap().len(), 2, "a blank deck gained two slides: {made}");
        let back = serve(&areas, &service_call("text", json!({"path": "review.pptx"}), dir.path(), false)).unwrap();
        assert_eq!(back["outline"], json!("Quarterly review\nRevenue\n\tGrew twelve percent\n"), "{back}");
        // An existing deck, edited and written beside itself as outline
        // text; a query without `out` writes nothing.
        let edited = serve(
            &areas,
            &service_call("run", json!({"path": "review.pptx", "cmds": [{"id": "slide.last"}, {"id": "slide.new", "params": {"title": "Costs"}}], "out": "review-2.txt"}), dir.path(), false),
        )
        .unwrap();
        assert_eq!(edited["format"], json!("outline"), "{edited}");
        assert!(std::fs::read_to_string(dir.path().join("review-2.txt")).unwrap().ends_with("Costs\n"));
        let query = serve(&areas, &service_call("run", json!({"path": "review.pptx", "cmds": [{"id": "slide.inspect", "params": {"index": 1}}]}), dir.path(), false)).unwrap();
        assert!(query["out"].is_null() && query["results"][0]["result"]["title"] == json!("Revenue"), "{query}");
        let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names.len(), 2, "only the two outputs: {names:?}");
    }

    /// Every kind of `out` the door writes: the deck as `convert` writes it
    /// (`.pptx`, `.deckcraft`, outline `.txt`, `.pdf`), one slide as `render`
    /// draws it (`.png`), each from the session after the commands.
    #[test]
    fn the_door_writes_every_out_kind() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        serve(&areas, &service_call("new", json!({"out": "talk.pptx", "slides": slides()}), dir.path(), false)).unwrap();
        let run = |out: &str, extra: Json| {
            let mut args = json!({"path": "talk.pptx", "cmds": [{"id": "slide.last"}, {"id": "slide.new", "params": {"title": "Questions"}}], "out": out});
            for (k, v) in extra.as_object().unwrap() {
                args[k] = v.clone();
            }
            serve(&areas, &service_call("run", args, dir.path(), false))
        };
        for (out, format) in [("k.pptx", "pptx"), ("k.deckcraft", "deckcraft")] {
            let v = run(out, json!({})).unwrap();
            assert_eq!((v["out"].as_str(), v["format"].as_str()), (Some(out), Some(format)), "{v}");
            assert_eq!(v["bytes"].as_u64().unwrap(), std::fs::metadata(dir.path().join(out)).unwrap().len());
            let info = serve(&areas, &service_call("info", json!({"path": out}), dir.path(), false)).unwrap();
            assert_eq!(info["slides"].as_array().unwrap().len(), 3, "{out}: {info}");
        }
        let txt = run("k.txt", json!({})).unwrap();
        assert_eq!(txt["format"], json!("outline"), "{txt}");
        assert_eq!(std::fs::read_to_string(dir.path().join("k.txt")).unwrap(), "Why decks\n\tOne engine\nHow it ports\nQuestions\n");
        let pdf = run("k.pdf", json!({})).unwrap();
        assert_eq!(pdf["format"], json!("pdf"), "{pdf}");
        assert!(std::fs::read(dir.path().join("k.pdf")).unwrap().starts_with(b"%PDF"));
        let png = run("k.png", json!({"slide": 2, "max_side": 256})).unwrap();
        assert_eq!((png["format"].as_str(), png["slide"].as_u64()), (Some("png"), Some(2)), "{png}");
        let (w, h) = (png["width"].as_u64().unwrap(), png["height"].as_u64().unwrap());
        assert_eq!(w.max(h), 256, "{png}");
        let bytes = std::fs::read(dir.path().join("k.png")).unwrap();
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']) && png["bytes"].as_u64().unwrap() == bytes.len() as u64);
        let default = run("first.png", json!({})).unwrap();
        assert_eq!((default["slide"].as_u64(), default["width"].as_u64().unwrap().max(default["height"].as_u64().unwrap())), (Some(0), 1024), "{default}");
        // A slide the deck does not have, or an extension the door does not
        // write, writes nothing.
        assert!(run("none.png", json!({"slide": 9})).unwrap_err().contains("no slide 9 (the deck has 3)"));
        assert!(run("k.jpg", json!({})).unwrap_err().contains("`out` ends in one of"));
        assert!(!dir.path().join("none.png").exists() && !dir.path().join("k.jpg").exists());
    }

    /// Every class but `safe` (and the reviewed reads) is refused, and so is
    /// an id the classification does not know, before any command runs: a
    /// refused id anywhere in the list writes nothing. (deckcraft has no
    /// `code` or `network` command.)
    #[test]
    fn the_door_refuses_every_other_class_and_unknown_ids() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let refused = |id: &str, params: Json| {
            let cmds = json!([{"id": "slide.new", "params": {"title": "x"}}, {"id": id, "params": params}]);
            serve(&areas, &service_call("run", json!({"cmds": cmds, "out": "x.pptx"}), dir.path(), false)).unwrap_err()
        };
        for (id, class) in [
            ("media.play", "device"),
            ("media.toggle", "device"),
            ("media.seek", "device"),
            ("show.fromStart", "device"),
            ("show.fromCurrent", "device"),
            ("file.save", "host"),
            ("file.saveAs", "host"),
            ("file.saveTemplate", "host"),
        ] {
            let e = refused(id, json!({"path": "x.pptx"}));
            assert!(e.contains(&format!("`{id}` is classed {class}")), "{id}: {e}");
        }
        for id in ["file.open", "file.export", "file.close", "file.recovery.save", "file.recovery.list", "file.recovery.open", "file.recovery.discard", "shape.fill", "design.background"] {
            let e = refused(id, json!({"path": "elsewhere.pptx"}));
            assert!(e.contains(&format!("`{id}` reads or writes files")) && e.contains("not reviewed to run through it"), "{id}: {e}");
        }
        assert!(refused("deck.secret", json!({})).contains("not a reviewed deck command"));
        assert!(refused("Slide.New", json!({})).contains("not a reviewed deck command"), "ids match exactly");
        assert!(!dir.path().join("x.pptx").exists(), "nothing written");
        let too_many: Vec<Json> = (0..65).map(|_| json!({"id": "slide.new"})).collect();
        assert!(serve(&areas, &service_call("run", json!({"cmds": too_many}), dir.path(), false)).unwrap_err().contains("at most 64"));
        assert!(serve(&areas, &service_call("run", json!({"cmds": [{"params": {}}]}), dir.path(), false)).unwrap_err().contains("each command has an `id`"));
        assert!(serve(&areas, &service_call("run", json!({"out": "y.pptx"}), dir.path(), false)).unwrap_err().contains("`cmds` is a list"));
    }

    /// The reviewed media reads take a file inside the area only and embed
    /// it; the `file` commands with an undocumented read, and `file.close`,
    /// stay refused; the door never writes over an existing `out`, and keeps
    /// to the quota and to the area.
    #[test]
    fn the_doors_file_reads_and_writes_keep_the_areas_rules() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        std::fs::create_dir(dir.path().join("pics")).unwrap();
        std::fs::write(dir.path().join("pics/dot.png"), png()).unwrap();
        std::fs::create_dir(dir.path().join("sound")).unwrap();
        std::fs::write(dir.path().join("sound/tone.wav"), wav()).unwrap();
        let made = serve(
            &areas,
            &service_call(
                "run",
                json!({"cmds": [
                    {"id": "slide.new", "params": {"layout": "blank"}},
                    {"id": "insert.picture", "params": {"path": "pics/dot.png"}},
                    {"id": "picture.change", "params": {"path": "pics/dot.png"}},
                    {"id": "insert.audio", "params": {"path": "sound/tone.wav"}}
                ], "out": "media.pptx"}),
                dir.path(),
                false,
            ),
        )
        .unwrap();
        assert_eq!(made["out"], json!("media.pptx"), "{made}");
        let info = serve(&areas, &service_call("info", json!({"path": "media.pptx"}), dir.path(), false)).unwrap();
        let types: Vec<&str> = info["media"].as_array().unwrap().iter().filter_map(|m| m["type"].as_str()).collect();
        assert!(types.contains(&"image/png") && types.contains(&"audio/wav"), "the files were embedded: {info}");
        // Only a file inside the area, for every reviewed read.
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.png"), png()).unwrap();
        let secret = outside.path().join("secret.png").to_string_lossy().into_owned();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path().join("secret.png"), dir.path().join("link.png")).unwrap();
        for id in ["insert.picture", "insert.audio", "insert.video", "picture.change"] {
            for bad in [secret.as_str(), "../secret.png", "pics/../../secret.png", "link.png", "pics", "missing.png"] {
                let cmds = json!([{"id": "slide.new"}, {"id": id, "params": {"path": bad}}]);
                let e = serve(&areas, &service_call("run", json!({"cmds": cmds, "out": "leak.pptx"}), dir.path(), false)).unwrap_err();
                assert!(e.contains(&format!("deck.run: `{id}`: `path`: ")), "{id} {bad}: the reviewed read's own check: {e}");
                assert!(!e.contains(outside.path().to_string_lossy().as_ref()), "{id} {bad}: an error never spells a host path: {e}");
            }
        }
        // A non-string `picture` makes these read `path`: refused whatever
        // it names, inside the area or out; and so is `file.close`.
        for (id, params) in [
            ("shape.fill", json!({"picture": null, "path": secret})),
            ("shape.fill", json!({"picture": null, "path": "pics/dot.png"})),
            ("design.background", json!({"picture": null, "path": secret})),
            ("design.background", json!({"picture": 0, "path": "pics/dot.png"})),
            ("file.close", json!({})),
        ] {
            let cmds = json!([{"id": "slide.new"}, {"id": "shape.insert", "params": {"preset": "rect"}}, {"id": id, "params": params}]);
            let e = serve(&areas, &service_call("run", json!({"cmds": cmds, "out": "leak.pptx"}), dir.path(), false)).unwrap_err();
            assert!(e.contains(&format!("`{id}` reads or writes files")), "{id}: {e}");
        }
        assert!(!dir.path().join("leak.pptx").exists(), "nothing written");
        // What one call's reads may total is capped before the engine reads.
        let big = std::fs::File::create(dir.path().join("pics/big.png")).unwrap();
        big.set_len(MAX_DECK_BYTES + 1).unwrap();
        let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "slide.new"}, {"id": "insert.picture", "params": {"path": "pics/big.png"}}]}), dir.path(), false)).unwrap_err();
        assert!(e.contains("the files this call reads total more than 67108864 bytes"), "{e}");
        // Never over an existing file, within the quota, inside the area.
        std::fs::write(dir.path().join("taken.pptx"), b"keep").unwrap();
        let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "slide.new"}], "out": "taken.pptx"}), dir.path(), false)).unwrap_err();
        assert!(e.contains("`taken.pptx` already exists"), "{e}");
        assert_eq!(std::fs::read(dir.path().join("taken.pptx")).unwrap(), b"keep");
        let e = serve(&resolver(dir.path(), Some(64)), &service_call("run", json!({"cmds": [{"id": "slide.new"}], "out": "big.pptx"}), dir.path(), false)).unwrap_err();
        assert!(e.contains("bytes left"), "{e}");
        assert!(!dir.path().join("big.pptx").exists());
        for bad in ["../up.pptx", "/etc/x.pptx", "pics/../../up.pptx"] {
            assert!(serve(&areas, &service_call("run", json!({"cmds": [], "out": bad}), dir.path(), false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("run", json!({"cmds": [], "path": bad}), dir.path(), false)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), dir.path().join("up")).unwrap();
            assert!(serve(&areas, &service_call("run", json!({"cmds": [{"id": "slide.new"}], "out": "up/made.pptx"}), dir.path(), false)).is_err());
            assert!(!outside.path().join("made.pptx").exists());
        }
    }

    /// The door's gate is built from the generated classification and the
    /// reviewed reads, which must be `file` commands of the catalog: every
    /// `safe` id and the four reads run, nothing else.
    #[test]
    fn the_door_is_built_from_the_reviewed_classification() {
        let door = door().unwrap();
        for id in ["slide.new", "text.set", "document.inspect", "slide.inspect", "file.new", "file.saveBytes", "insert.picture", "insert.audio", "insert.video", "picture.change"] {
            assert!(door.runs(id), "{id}");
        }
        for id in ["file.open", "file.export", "file.close", "file.save", "file.recovery.open", "shape.fill", "design.background", "media.play", "show.fromStart"] {
            assert!(!door.runs(id), "{id}");
        }
        let safety: Json = serde_json::from_str(include_str!("../skill/safety.json")).unwrap();
        let safe = safety["commands"].as_object().unwrap().values().filter(|c| *c == "safe").count();
        assert_eq!(door.runnable().len(), safe + REVIEWED.file_reads.len());
    }

    /// Every `deck.run` call the skill's examples show runs, in order,
    /// in one area as the system agent's (with the files they name placed
    /// there first), and writes its `out`: the skill teaches commands that
    /// work.
    #[test]
    fn the_skill_examples_run() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        std::fs::write(dir.path().join("chart.png"), png()).unwrap();
        let body = include_str!("../skill/SKILL.md");
        let examples = body.split("\n## Examples").nth(1).and_then(|rest| rest.split("\n## ").next()).unwrap();
        let mut ran = 0;
        for span in examples.split('`').skip(1).step_by(2) {
            let Some(args) = span.strip_prefix("deck.run ") else { continue };
            let args: Json = serde_json::from_str(args).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            let got = serve(&areas, &service_call("run", args.clone(), dir.path(), false)).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            assert_eq!(got["results"].as_array().map(Vec::len), args["cmds"].as_array().map(Vec::len), "{got}");
            if let Some(out) = args["out"].as_str() {
                assert!(dir.path().join(out).is_file(), "`{span}` wrote no {out}");
            }
            ran += 1;
        }
        assert!(ran >= 6, "{ran} examples");
        let outline = std::fs::read_to_string(dir.path().join("launch.txt")).unwrap();
        assert_eq!(outline, "Launch plan\nGoals\n\tShip in May\n\tTwo pilots\nNext steps\n\tBudget\n\tHiring\n");
        let info = serve(&areas, &service_call("info", json!({"path": "launch-2.pptx"}), dir.path(), false)).unwrap();
        assert_eq!(info["slides"].as_array().unwrap().len(), 4, "{info}");
    }
}
