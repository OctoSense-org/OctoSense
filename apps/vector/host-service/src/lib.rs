//! `octosense-vector-service` — the `vector` host service (ADR 0013).
//!
//! vectorcraft's engine through its own single command entry point
//! ([`Session::execute`] — the same door its UI, CLI and automation server
//! go through). Every call is a fresh, stateless session. Unlike
//! photocraft, the engine does not contain file paths itself, so the
//! service does: every path is resolved inside the call's area before the
//! engine sees it, and `run` refuses the engine's own file commands.
//!
//! **Where a call works** (ADR 0013, 2026-10-08): the caller's own folder,
//! the [`Area`] the shell's resolver gives it ([`set_area_resolver`]), or
//! without one the legacy `<host dir>/vector`. A document's linked images
//! are the engine's to read when it opens the document — an SVG's
//! `<image href>`, a native document's links (also inside an SVG, PDF or EPS
//! saved with its editing data) — from wherever they point, so before it
//! opens one the service finds every file it would read and refuses the
//! document unless each resolves inside the area ([`fence_links`]). Writes
//! keep the area's rules (a write that may not replace, an agent's, only
//! creates new files, within the quota): a preview through [`Area::write`],
//! an export, which the engine writes itself (with any numbered siblings),
//! into a staging folder inside the area first
//! ([`octosense_engine_area::Stage`]). The generic command door `run` is
//! held for its own review: with the shell's resolver installed it is
//! refused outright, because the engine's wrappers (`command.batch`), its
//! preferences (`prefs.set` of a plug-ins folder) and plug-in effects
//! (`effect.apply` of `plugin.<id>`) reach past its list of refused ids.
//!
//! Methods (all under the `vector` family; paths relative to the area):
//! - `info {path, depth?}` → the document inspected as JSON (artboards,
//!   layer tree, object count, colour mode, units, import warnings)
//! - `convert {path, out, format?, scale?}` → `{out, format, bytes,
//!   warnings}` — export in the format the extension (or `format`) picks:
//!   SVG/SVGZ, PDF, EPS, DXF, EMF/WMF, PNG/JPG/WebP/GIF/TIFF/BMP/TGA/PSD,
//!   native `.vectorcraft`
//! - `run {path?, cmds: [{id, params?}], out?, format?}` → command
//!   results, exporting when `out` is given (the engine's drawing and
//!   editing catalog: `vector.commands` lists it; file commands are the
//!   service's and are refused)
//! - `commands {}` → the callable command catalog
//! - `render {path, out, max_side?, artboard?}` → a PNG written to `out`
//!
//! The service serves system apps only until ADR 0013's store capability
//! is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::path::{Component, Path, PathBuf};

use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};
use vectorcraft_engine::Session;

/// The longest preview edge `render` produces.
const MAX_RENDER_SIDE: u32 = 4096;
/// The largest file the service opens (bytes).
const MAX_OPEN_BYTES: u64 = 64 << 20;
/// The most commands one `run` call executes.
const MAX_RUN_CMDS: usize = 64;

/// Which apps may call the service: system apps, as News and Sheets.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct VectorService;

/// Register the `vector` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(VectorService));
}

/// The shell's resolver: where each call works (`None` removes it, and
/// calls work in the legacy `<host dir>/vector` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
}

/// The `vector.*` agent tools (ADR 0013, wave 2), in App Hub's `tools.json`
/// shape: the shell declares them for the virtual owner `os.vector` and grants
/// the system agent its reviewed share (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`).
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for VectorService {
    fn family(&self) -> &'static str {
        "vector"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        reply.send(serve(&AREAS, &call));
    }
}

/// One call, in the area `areas` gives it.
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The vector service serves system apps only.".into());
    }
    if call.method() == "commands" {
        return commands();
    }
    // The command door works only without the shell's resolver (this
    // crate's tests): in the shell it is held for its own review.
    if call.method() == "run" && areas.is_set() {
        return Err("vector.run is held for its own review: the engine's command door is not available in the shell".into());
    }
    let area = areas.area(call, "vector").map_err(|e| format!("vector: {e}"))?;
    dispatch_in(call.method(), &call.args, &area)
}

fn dispatch_in(method: &str, args: &Json, area: &Area) -> Result<Json, String> {
    match method {
        "info" => info(args, area),
        "convert" => convert(args, area),
        "run" => run(args, area),
        "commands" => commands(),
        "render" => render(args, area),
        other => Err(format!("vector.{other} is not a method of the vector service")),
    }
}

/// [`dispatch_in`] in the legacy area `<host_dir>/vector`, as a call
/// without the shell's resolver works.
#[cfg(test)]
fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    if method == "commands" {
        return commands();
    }
    let area = Area::legacy(host_dir, "vector");
    std::fs::create_dir_all(&area.root).map_err(|e| format!("vector: area: {e}"))?;
    dispatch_in(method, args, &area)
}

/// A path strictly inside the area: relative, no `..` or absolute
/// component, and the resolved path stays under the area even through
/// symlinks — the same stance the files host tools and the sheet service
/// take.
fn contained_path(area: &Path, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err("vector: `path` is required".into());
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err("vector: paths stay inside this call's folder".into());
    }
    let joined = area.join(rel_path);
    let check_root = area.canonicalize().map_err(|e| format!("vector: folder: {e}"))?;
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
    let resolved = deepest.canonicalize().map_err(|e| format!("vector: {e}"))?;
    if !resolved.starts_with(&check_root) {
        return Err("vector: paths stay inside this call's folder".into());
    }
    Ok(joined)
}

/// A contained output path the call may write, refused before the engine
/// works when the area's rules would refuse it.
fn out_path(area: &Area, rel: &str) -> Result<PathBuf, String> {
    let path = contained_path(&area.root, rel)?;
    area.check(&path, 0).map_err(|e| format!("vector: {e}"))?;
    Ok(path)
}

/// Whether `p`, a path a document names, is a file of the area: absolute,
/// with no `..`, its deepest existing ancestor inside the canonical root
/// (through links). A relative one the engine would resolve against the
/// process's working folder, which is no folder of the caller's.
fn inside(root: &Path, p: &Path) -> bool {
    if !p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)) {
        return false;
    }
    let mut deepest = p.to_path_buf();
    while !deepest.exists() {
        match deepest.parent() {
            Some(parent) => deepest = parent.to_path_buf(),
            None => return false,
        }
    }
    deepest.canonicalize().is_ok_and(|real| real.starts_with(root))
}

/// The files the engine looks for a document's linked images at when it
/// opens it from `dir` (vectorcraft's `links::resolve`): each link's own
/// path, its path relative to the document's folder, and its name there.
fn link_files(doc: &vectorcraft_engine::doc::Document, dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    doc.visit_images(|_, image| {
        if let Some(link) = &image.link {
            out.push(PathBuf::from(&link.path));
            if let Some(rel) = &link.relative {
                out.push(dir.join(rel));
            }
            out.push(dir.join(link.name()));
        }
    });
    out
}

/// Refuse the document at `path` (already contained) unless every file the
/// engine would read for its linked images, opening it, is in the area. The
/// engine reads them with no gate of its own, so they are found first, with
/// the engine's own code and nothing read: an SVG's `<image>` links by its
/// importer, run with a reader that records the paths it is asked for; a
/// native document's links (one an SVG carries as its editing data
/// included, which the engine opens instead of the SVG) from the document
/// itself, as the engine's loader makes it for every other format.
fn fence_links(area: &Area, path: &Path, rel: &str, method: &str) -> Result<(), String> {
    let root = area.root.canonicalize().map_err(|e| format!("vector.{method}: folder: {e}"))?;
    let bytes = std::fs::read(path).map_err(|e| format!("vector.{method}: {rel}: {e}"))?;
    let name = utf8(path)?;
    let dir = path.parent().unwrap_or(&area.root);
    let mut wanted: Vec<PathBuf> = Vec::new();
    let format = vectorcraft_engine::cmd::fileio::detect(name, &bytes).map(|f| f.id);
    let postscript = bytes.starts_with(b"%!PS") || bytes.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6]);
    if matches!(format, Some("svg" | "svgz")) && !postscript {
        let text = vectorcraft_svg::text_of(&bytes).map_err(|e| format!("vector.{method}: {rel}: {e}"))?;
        let asked = std::cell::RefCell::new(Vec::new());
        let record = |p: &str| {
            asked.borrow_mut().push(PathBuf::from(p));
            None
        };
        let folder = dir.to_string_lossy();
        let _ = vectorcraft_svg::import_with(&text, &vectorcraft_svg::ImportOptions { folder: Some(&folder), read: Some(&record) });
        wanted.extend(asked.into_inner());
        if let Some(editing) = vectorcraft_svg::editing(&text).filter(|e| e.intact) {
            if let Some(doc) = vectorcraft_format::base64_decode(&editing.data).and_then(|b| vectorcraft_format::load_file(&b).ok()) {
                wanted.extend(link_files(&doc.doc, dir));
            }
        }
    } else if let Ok(loaded) = vectorcraft_engine::cmd::fileio::load_with(name, &bytes, &Default::default()) {
        wanted.extend(link_files(&loaded.doc, dir));
    }
    if let Some(out) = wanted.iter().find(|p| !inside(&root, p)) {
        return Err(format!(
            "vector.{method}: `{rel}` links `{}`, outside this call's folder: the engine would read it from there; embed the images it links",
            out.display()
        ));
    }
    Ok(())
}

fn arg_str<'a>(args: &'a Json, key: &str) -> Result<&'a str, String> {
    args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("vector: `{key}` is required"))
}

fn utf8(path: &Path) -> Result<&str, String> {
    path.to_str().ok_or_else(|| "vector: the host directory is not UTF-8".into())
}

/// Open `rel` (contained, size-capped, its links fenced) into the session;
/// the engine's result carries the import warnings.
fn open(session: &mut Session, area: &Area, rel: &str, method: &str) -> Result<Json, String> {
    let path = contained_path(&area.root, rel)?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("vector.{method}: {rel}: {e}"))?;
    if meta.len() > MAX_OPEN_BYTES {
        return Err(format!("vector.{method}: the file is larger than the service opens"));
    }
    fence_links(area, &path, rel, method)?;
    session.execute("document.open", &json!({"path": utf8(&path)?})).map_err(|e| format!("vector.{method}: {e}"))
}

/// `document.export` to `out`, which the engine writes itself (a
/// multi-artboard SVG export also writes `{stem}-{n}.svg` siblings): into a
/// staging folder inside the area, then every file beside `out`, all or
/// none, under the area's rules; the result's own paths made relative
/// again.
fn export(session: &mut Session, area: &Area, args: &Json, out: &Path, method: &str) -> Result<Json, String> {
    let out_rel = arg_str(args, "out")?;
    let stage = area.stage().map_err(|e| format!("vector.{method}: {e}"))?;
    let staged = stage.path(out.file_name().unwrap_or_default());
    let mut params = json!({"path": utf8(&staged)?});
    if let Some(f) = args["format"].as_str() {
        params["format"] = json!(f);
    }
    if let Some(s) = args["scale"].as_f64() {
        params["scale"] = json!(s.clamp(0.01, 16.0));
    }
    let saved = session.execute("document.export", &params).map_err(|e| format!("vector.{method}: {e}"))?;
    let beside = out.parent().unwrap_or(&area.root);
    let moves: Vec<(PathBuf, PathBuf)> = stage.files().into_iter().map(|f| (stage.path(&f), beside.join(f))).collect();
    stage.commit(&moves).map_err(|e| format!("vector.{method}: {e}"))?;
    let rel_dir = Path::new(out_rel).parent().unwrap_or(Path::new(""));
    let rel_files = saved["files"].as_array().map(|files| {
        files
            .iter()
            .filter_map(Json::as_str)
            .map(|f| json!(Path::new(f).file_name().map(|n| rel_dir.join(n).to_string_lossy().into_owned()).unwrap_or_else(|| f.to_string())))
            .collect::<Vec<_>>()
    });
    let mut v = json!({"out": out_rel, "format": saved["format"], "bytes": saved["bytes"], "warnings": saved["warnings"]});
    if let Some(files) = rel_files {
        v["files"] = Json::Array(files);
    }
    Ok(v)
}

fn info(args: &Json, area: &Area) -> Result<Json, String> {
    let rel = arg_str(args, "path")?;
    let mut s = Session::new();
    let opened = open(&mut s, area, rel, "info")?;
    // Depth-limited so a deep document stays a summary; every sliced level
    // reports `childCount`, so truncation is never silent.
    let depth = args["depth"].as_u64().unwrap_or(2).min(8);
    let mut doc = s
        .execute("document.inspect", &json!({"depth": depth, "childLimit": 64}))
        .map_err(|e| format!("vector.info: {e}"))?;
    doc["warnings"] = opened["warnings"].clone();
    // The engine reports the absolute path it opened; the caller's world
    // is the area-relative one.
    doc["path"] = json!(rel);
    doc["file"] = json!(rel);
    Ok(doc)
}

fn convert(args: &Json, area: &Area) -> Result<Json, String> {
    let rel = arg_str(args, "path")?;
    let out = out_path(area, arg_str(args, "out")?)?;
    let mut s = Session::new();
    let opened = open(&mut s, area, rel, "convert")?;
    let mut v = export(&mut s, area, args, &out, "convert")?;
    v["warnings"] = json!([opened["warnings"], v["warnings"].clone()]);
    Ok(v)
}

/// Commands `run` executes: the engine's drawing and editing catalog,
/// minus everything that touches files. The service owns all file access
/// (its `path` and `out` arguments), so the engine's own file commands —
/// `file.*` (new-from-template, recovery), `document.*` (open, save,
/// export, place), the `app.*` host group and the swatch libraries — are
/// refused, except the read-only document queries. As a second fence,
/// parameters carrying path-like keys are refused wholesale.
///
/// Every `plugin.*` id is refused too, as `photo.run` refuses photocraft's:
/// vectorcraft's plug-in registry is process-wide and installs WebAssembly
/// from in-band `dataBase64`, so an installed plug-in would outlive the
/// call and serve every later caller of any app, outside the shell's `wasm`
/// service (ADR 0011). This list is not a fence on its own (a wrapper runs
/// other commands past it), which is why the shell holds `run` back
/// entirely ([`serve`]).
fn callable(id: &str, params: &Json) -> Result<(), String> {
    if id == "plugin" || id.starts_with("plugin.") {
        return Err(format!("vector.run: `{id}` is not available through the vector service"));
    }
    const READ_ONLY: &[&str] = &["document.inspect", "document.node", "document.json", "document.find"];
    if !READ_ONLY.contains(&id) && ["file.", "document.", "app.", "swatch.library."].iter().any(|p| id.starts_with(p)) {
        return Err(format!("vector.run: `{id}` is the host's; file access goes through the service's `path` and `out`"));
    }
    if let Some(key) = path_key(params) {
        return Err(format!("vector.run {id}: `{key}` parameters are the host's; file access goes through the service's `path` and `out`"));
    }
    Ok(())
}

/// The first path-like key anywhere in `v`.
fn path_key(v: &Json) -> Option<String> {
    const KEYS: &[&str] = &["path", "paths", "file", "files", "folder", "dir", "url", "href"];
    match v {
        Json::Object(m) => m.iter().find_map(|(k, val)| {
            if KEYS.contains(&k.as_str()) { Some(k.clone()) } else { path_key(val) }
        }),
        Json::Array(a) => a.iter().find_map(path_key),
        _ => None,
    }
}

fn run(args: &Json, area: &Area) -> Result<Json, String> {
    let cmds = args["cmds"].as_array().ok_or("vector.run: `cmds` is a list of {id, params?}")?;
    if cmds.len() > MAX_RUN_CMDS {
        return Err(format!("vector.run: at most {MAX_RUN_CMDS} commands per call"));
    }
    let out = match args["out"].as_str() {
        Some(out) if !out.is_empty() => Some(out_path(area, out)?),
        _ => None,
    };
    let mut s = Session::new();
    match args["path"].as_str() {
        Some(rel) if !rel.is_empty() => {
            open(&mut s, area, rel, "run")?;
        }
        // No input: a fresh default document, ready to draw into.
        _ => {
            s.execute("file.new", &json!({})).map_err(|e| format!("vector.run: {e}"))?;
        }
    }
    let mut results = Vec::new();
    for c in cmds {
        let id = c["id"].as_str().ok_or("vector.run: each command has an `id`")?;
        let params = if c["params"].is_null() { json!({}) } else { c["params"].clone() };
        callable(id, &params)?;
        let r = s.execute(id, &params).map_err(|e| format!("vector.run {id}: {e}"))?;
        results.push(json!({"id": id, "result": r}));
    }
    let saved = match out {
        Some(out) => export(&mut s, area, args, &out, "run")?,
        None => Json::Null,
    };
    Ok(json!({"results": results, "out": saved}))
}

/// The catalog `run` accepts: the engine's own, minus the file commands
/// the service refuses.
fn commands() -> Result<Json, String> {
    let list: Vec<Json> = Session::new()
        .commands()
        .iter()
        .filter(|c| callable(c.id, &Json::Null).is_ok())
        .map(|c| serde_json::to_value(c).unwrap_or_default())
        .collect();
    Ok(Json::Array(list))
}

fn render(args: &Json, area: &Area) -> Result<Json, String> {
    let rel = arg_str(args, "path")?;
    let out_rel = arg_str(args, "out")?;
    let max_side = args["max_side"].as_u64().unwrap_or(1024).clamp(1, MAX_RENDER_SIDE as u64) as u32;
    let out = out_path(area, out_rel)?;
    let mut s = Session::new();
    open(&mut s, area, rel, "render")?;
    let doc = s.doc().map_err(|e| format!("vector.render: {e}"))?.doc.clone();
    let idx = args["artboard"].as_u64().unwrap_or(0) as usize;
    let rect = doc.artboards.get(idx).map(|a| a.rect).ok_or("vector.render: no such artboard")?;
    let side = rect.width().max(rect.height());
    if side <= 0.0 {
        return Err("vector.render: the artboard is empty".into());
    }
    let scale = (max_side as f64 / side).clamp(0.001, 16.0);
    vectorcraft_render::raster_size(rect, scale).map_err(|e| format!("vector.render: {e}"))?;
    let img = vectorcraft_render::Renderer::new().render_region(&doc, rect, scale, true);
    let png = img.to_png().map_err(|e| format!("vector.render: {e}"))?;
    area.write(&out, &png).map_err(|e| format!("vector.render: {e}"))?;
    Ok(json!({"out": out_rel, "width": img.width, "height": img.height, "bytes": png.len()}))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 64x40 two-shape SVG (written by this test's author, embedded so
    /// the suite needs no fixtures on disk).
    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="40" viewBox="0 0 64 40"><rect x="4" y="4" width="32" height="20" fill="#3366cc"/><circle cx="48" cy="20" r="12" fill="#cc3333"/></svg>"##;

    /// Writes the fixture into the service's area and returns its
    /// area-relative path.
    fn fixture(host: &Path) -> String {
        let area = host.join("vector");
        std::fs::create_dir_all(&area).unwrap();
        std::fs::write(area.join("in.svg"), SVG).unwrap();
        "in.svg".into()
    }

    /// A call as App Hub hands it to the service.
    fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("vector.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
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

    fn no_staging_left(root: &Path) -> bool {
        std::fs::read_dir(root).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with(octosense_engine_area::STAGING_PREFIX))
    }

    /// A 12x8 RGB PNG (two colour bands).
    fn png() -> Vec<u8> {
        const HEX: &str = "89504e470d0a1a0a0000000d494844520000000c000000080802000000428689a60000001d49444154789c6378616383866c725ea021063a2bb279d14310d15911005b9497817c6155610000000049454e44ae426082";
        (0..HEX.len()).step_by(2).map(|i| u8::from_str_radix(&HEX[i..i + 2], 16).unwrap()).collect()
    }

    /// With the shell's resolver every path is relative to the caller's own
    /// folder and stays inside it, through a link too; an export the engine
    /// writes itself lands where it was asked.
    #[test]
    fn the_resolver_root_is_used_and_paths_stay_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("in.svg"), SVG).unwrap();
        let areas = resolver(&root, None);
        let host = dir.path().join(".host");
        let doc = serve(&areas, &service_call("info", json!({"path": "in.svg"}), &host, false)).unwrap();
        assert_eq!(doc["path"], json!("in.svg"), "{doc}");
        serve(&areas, &service_call("convert", json!({"path": "in.svg", "out": "out/again.svg"}), &host, false)).unwrap();
        serve(&areas, &service_call("render", json!({"path": "in.svg", "out": "out/p.png", "max_side": 32}), &host, false)).unwrap();
        assert!(root.join("out/again.svg").is_file() && root.join("out/p.png").is_file());
        assert!(!host.exists() && !root.join("vector").exists() && no_staging_left(&root));
        std::fs::write(dir.path().join("beside.svg"), SVG).unwrap();
        for bad in ["../beside.svg", "/etc/hosts", "out/../../beside.svg"] {
            assert!(serve(&areas, &service_call("info", json!({"path": bad}), &host, false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("convert", json!({"path": "in.svg", "out": bad}), &host, true)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            assert!(serve(&areas, &service_call("info", json!({"path": "up/beside.svg"}), &host, false)).is_err());
            assert!(serve(&areas, &service_call("render", json!({"path": "in.svg", "out": "up/p.png"}), &host, true)).is_err());
            assert!(serve(&areas, &service_call("convert", json!({"path": "in.svg", "out": "up/c.svg"}), &host, true)).is_err());
            assert!(!dir.path().join("p.png").exists() && !dir.path().join("c.svg").exists());
        }
    }

    /// With the shell's resolver installed the command door is held for its
    /// own review: its list of refused ids is no fence, since a wrapper runs
    /// other commands past it (`command.batch` of `plugin.install` passes
    /// the list). Without a resolver (this crate's tests) it still runs.
    #[test]
    fn the_command_door_is_held_in_the_shell() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let held = serve(&areas, &service_call("run", json!({"cmds": []}), dir.path(), true)).unwrap_err();
        assert!(held.contains("held for its own review"), "{held}");
        assert!(callable("plugin.install", &json!({})).is_err());
        let wrapped = json!({"commands": [{"id": "plugin.install", "params": {"dataBase64": "AGFzbQEAAAA="}}]});
        assert!(callable("command.batch", &wrapped).is_ok(), "a wrapper passes the list, which is why the door is held");
        serve(&Slot::new(), &service_call("run", json!({"cmds": []}), dir.path(), true)).unwrap();
        assert!(serve(&areas, &service_call("commands", json!({}), dir.path(), true)).is_ok(), "the catalog stays readable");
    }

    /// An agent's call never replaces a file, before the engine works; an
    /// app's own foreground call may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("in.svg"), SVG).unwrap();
        let areas = resolver(dir.path(), None);
        std::fs::write(dir.path().join("taken.png"), b"keep me").unwrap();
        for (method, args) in [
            ("convert", json!({"path": "in.svg", "out": "taken.png"})),
            ("render", json!({"path": "in.svg", "out": "taken.png"})),
            ("convert", json!({"path": "in.svg", "out": "in.svg"})),
        ] {
            let refused = serve(&areas, &service_call(method, args, dir.path(), false)).unwrap_err();
            assert!(refused.contains("already exists"), "{method}: {refused}");
        }
        // The command door's own export keeps the same rule (past the
        // shell's hold on it).
        let run = json!({"path": "in.svg", "cmds": [], "out": "taken.png"});
        let area = areas.area(&service_call("run", run.clone(), dir.path(), false), "vector").unwrap();
        assert!(dispatch_in("run", &run, &area).unwrap_err().contains("already exists"));
        assert_eq!(std::fs::read(dir.path().join("taken.png")).unwrap(), b"keep me");
        assert!(no_staging_left(dir.path()));
        serve(&areas, &service_call("convert", json!({"path": "in.svg", "out": "taken.png"}), dir.path(), true)).unwrap();
        assert!(std::fs::read(dir.path().join("taken.png")).unwrap().starts_with(b"\x89PNG"));
    }

    /// What a call writes, the engine's own export included, must fit what
    /// is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("in.svg"), SVG).unwrap();
        let tight = resolver(dir.path(), Some(32));
        for (method, args) in [
            ("convert", json!({"path": "in.svg", "out": "c.png"})),
            ("render", json!({"path": "in.svg", "out": "r.png"})),
        ] {
            let refused = serve(&tight, &service_call(method, args, dir.path(), true)).unwrap_err();
            assert!(refused.contains("bytes left"), "{method}: {refused}");
        }
        assert!(!dir.path().join("c.png").exists() && !dir.path().join("r.png").exists() && no_staging_left(dir.path()));
    }

    /// An SVG whose `<image>` links a picture outside the caller's folder
    /// (an absolute path, a `file://` URL, or a relative path that climbs
    /// out) is refused before the engine opens it; one that links a picture
    /// beside it, inside the folder, opens.
    #[test]
    fn an_svg_that_links_a_file_outside_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let outside = dir.path().join("private.png");
        std::fs::write(&outside, png()).unwrap();
        std::fs::write(root.join("pic.png"), png()).unwrap();
        let svg = |href: &str| format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><image href="{href}" width="12" height="8"/><rect x="20" y="2" width="10" height="10" fill="#3366cc"/></svg>"##);
        let areas = resolver(&root, None);
        for (n, href) in [outside.display().to_string(), format!("file://{}", outside.display()), "../private.png".into()].into_iter().enumerate() {
            let name = format!("linked{n}.svg");
            std::fs::write(root.join(&name), svg(&href)).unwrap();
            for (method, args) in [
                ("info", json!({"path": name})),
                ("convert", json!({"path": name, "out": format!("c{n}.png")})),
                ("render", json!({"path": name, "out": format!("r{n}.png")})),
            ] {
                let refused = serve(&areas, &service_call(method, args, &root, false)).unwrap_err();
                assert!(refused.contains("private.png") && refused.contains("outside"), "{href} {method}: {refused}");
            }
        }
        std::fs::write(root.join("local.svg"), svg("pic.png")).unwrap();
        let ok = serve(&areas, &service_call("info", json!({"path": "local.svg"}), &root, false)).unwrap();
        assert_eq!(ok["path"], json!("local.svg"), "{ok}");
        serve(&areas, &service_call("render", json!({"path": "local.svg", "out": "local.png", "max_side": 16}), &root, false)).unwrap();
    }

    /// A native document whose linked image lives outside the caller's
    /// folder is refused before the engine opens it (the engine would read
    /// the link from wherever it points); one whose link is inside opens.
    #[test]
    fn a_native_document_that_links_a_file_outside_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let outside = dir.path().join("private.png");
        std::fs::write(&outside, png()).unwrap();
        std::fs::write(root.join("pic.png"), png()).unwrap();
        for (name, picture) in [("outside.vectorcraft", outside.clone()), ("inside.vectorcraft", root.join("pic.png"))] {
            let mut s = Session::new();
            s.execute("file.new", &json!({})).unwrap();
            s.execute("file.place", &json!({"path": picture.to_string_lossy(), "link": true})).unwrap();
            s.execute("document.export", &json!({"path": root.join(name).to_string_lossy()})).unwrap();
        }
        let areas = resolver(&root, None);
        let refused = serve(&areas, &service_call("info", json!({"path": "outside.vectorcraft"}), &root, false)).unwrap_err();
        assert!(refused.contains("private.png") && refused.contains("outside"), "{refused}");
        assert!(serve(&areas, &service_call("convert", json!({"path": "outside.vectorcraft", "out": "o.svg"}), &root, false)).is_err());
        let ok = serve(&areas, &service_call("info", json!({"path": "inside.vectorcraft"}), &root, false)).unwrap();
        assert_eq!(ok["path"], json!("inside.vectorcraft"), "{ok}");
        // The same document carried as an SVG's (or a PDF's) editing data,
        // which the engine opens in place of the SVG, is refused alike.
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        s.execute("file.place", &json!({"path": outside.to_string_lossy(), "link": true})).unwrap();
        for name in ["editing.svg", "editing.pdf"] {
            s.execute("document.export", &json!({"path": root.join(name).to_string_lossy(), "preserveEditing": true})).unwrap();
            let refused = serve(&areas, &service_call("info", json!({"path": name}), &root, false)).unwrap_err();
            assert!(refused.contains("private.png") && refused.contains("outside"), "{name}: {refused}");
        }
        assert!(std::fs::read_to_string(root.join("editing.svg")).unwrap().contains(vectorcraft_svg::EDITING_NS), "the SVG carries its editing data");
    }

    #[test]
    fn info_reads_a_real_svg() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);
        let doc = dispatch("info", &json!({"path": input}), host).unwrap();
        assert_eq!(doc["artboards"][0]["width"], json!(64.0), "{doc}");
        assert_eq!(doc["artboards"][0]["height"], json!(40.0));
        assert!(doc["objects"].as_u64().unwrap() >= 2, "{doc}");
        assert_eq!(doc["path"], json!("in.svg"), "no absolute paths in results");
    }

    #[test]
    fn convert_roundtrips_svg_and_rasterizes_png() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);

        let svg = dispatch("convert", &json!({"path": input, "out": "out/again.svg"}), host).unwrap();
        assert_eq!(svg["out"], json!("out/again.svg"), "{svg}");
        assert!(host.join("vector/out/again.svg").metadata().unwrap().len() > 0);

        let png = dispatch("convert", &json!({"path": input, "out": "flat.png", "scale": 2.0}), host).unwrap();
        assert_eq!(png["format"], json!("png"), "{png}");
        let bytes = std::fs::read(host.join("vector/flat.png")).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "a real PNG");
    }

    #[test]
    fn run_draws_with_engine_commands_and_refuses_file_commands() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);

        let ran = dispatch(
            "run",
            &json!({"path": input, "cmds": [
                {"id": "shape.rectangle", "params": {"x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0}},
                {"id": "document.inspect", "params": {"depth": 0}},
            ], "out": "drawn.svg"}),
            host,
        )
        .unwrap();
        assert_eq!(ran["results"][0]["id"], json!("shape.rectangle"), "{ran}");
        assert!(ran["results"][1]["result"]["objects"].as_u64().unwrap() >= 3, "{ran}");
        assert!(host.join("vector/drawn.svg").metadata().unwrap().len() > 0);

        // Without an input: a fresh document to draw into.
        let fresh = dispatch("run", &json!({"cmds": [{"id": "document.inspect", "params": {"depth": 0}}]}), host).unwrap();
        assert!(fresh["results"][0]["result"]["artboards"].as_array().is_some_and(|a| !a.is_empty()), "{fresh}");

        // The engine's file commands are the host's.
        for refused in [
            json!({"id": "document.open", "params": {}}),
            json!({"id": "document.export", "params": {}}),
            json!({"id": "file.recovery.list"}),
            json!({"id": "swatch.library.save", "params": {"name": "x"}}),
            // A path-like parameter smuggled into a drawing command.
            json!({"id": "shape.rectangle", "params": {"x": 0.0, "y": 0.0, "width": 5.0, "height": 5.0, "path": "up.svg"}}),
        ] {
            let r = dispatch("run", &json!({"path": input, "cmds": [refused]}), host);
            assert!(r.is_err(), "{refused}");
        }
    }

    /// The plug-in registry is process-wide and installs WebAssembly from
    /// in-band data: `run` refuses every `plugin.*` id before the engine
    /// sees it, and the catalog offer matches.
    #[test]
    fn run_refuses_plugin_commands_and_the_catalog_omits_them() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        for cmd in [
            // A core module's header: refused before anything parses it.
            json!({"id": "plugin.install", "params": {"dataBase64": "AGFzbQEAAAA="}}),
            json!({"id": "plugin.list"}),
            json!({"id": "plugin.remove", "params": {"id": "org.vectorcraft.example.desaturate"}}),
            json!({"id": "plugin.reload"}),
            json!({"id": "plugin"}),
        ] {
            let r = dispatch("run", &json!({"cmds": [cmd.clone()]}), host);
            let e = r.expect_err(&cmd.to_string());
            assert!(e.contains("is not available through the vector service"), "{cmd}: {e}");
        }
        let cat = commands().unwrap();
        let ids: Vec<&str> = cat.as_array().unwrap().iter().filter_map(|c| c["id"].as_str()).collect();
        assert!(ids.iter().all(|id| *id != "plugin" && !id.starts_with("plugin.")), "no plug-in commands offered");
    }

    #[test]
    fn commands_catalog_is_real_and_file_free() {
        let cat = commands().unwrap();
        let cat = cat.as_array().unwrap();
        assert!(cat.len() > 200, "a real catalog, {} commands", cat.len());
        let ids: Vec<&str> = cat.iter().filter_map(|c| c["id"].as_str()).collect();
        assert!(ids.contains(&"document.inspect"));
        assert!(ids.iter().all(|id| !id.starts_with("file.") && !id.starts_with("app.")), "no file commands offered");
        assert!(!ids.contains(&"document.open") && !ids.contains(&"document.export"));
    }

    #[test]
    fn render_writes_a_bounded_png() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);
        let r = dispatch("render", &json!({"path": input, "out": "prev.png", "max_side": 64}), host).unwrap();
        assert_eq!(r["width"], json!(64), "{r}");
        assert_eq!(r["height"], json!(40));
        let bytes = std::fs::read(host.join("vector/prev.png")).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(bytes.len() as u64, r["bytes"].as_u64().unwrap());
    }

    #[test]
    fn paths_stay_inside_the_area() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);
        for bad in ["../up.svg", "/etc/x.svg", "a/../../up.svg"] {
            assert!(dispatch("info", &json!({"path": bad}), host).is_err(), "{bad}");
            assert!(dispatch("convert", &json!({"path": input, "out": bad}), host).is_err(), "{bad}");
            assert!(dispatch("render", &json!({"path": input, "out": bad}), host).is_err(), "{bad}");
            assert!(dispatch("run", &json!({"path": input, "cmds": [], "out": bad}), host).is_err(), "{bad}");
            assert!(dispatch("run", &json!({"path": bad, "cmds": []}), host).is_err(), "{bad}");
        }
        // A required path must be given at all.
        assert!(dispatch("info", &json!({"path": ""}), host).is_err());
        assert!(dispatch("convert", &json!({"path": input, "out": ""}), host).is_err());
        assert!(dispatch("render", &json!({"path": input, "out": ""}), host).is_err());
    }

    #[test]
    fn the_area_is_the_vector_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);
        // Everything lands under `<host>/vector`, nothing in the shared root,
        // and without a resolver a call may replace, as before.
        dispatch("convert", &json!({"path": input, "out": "flat.png"}), host).unwrap();
        serve(&Slot::new(), &service_call("convert", json!({"path": input, "out": "flat.png"}), host, false)).unwrap();
        assert!(host.join("vector/flat.png").exists());
        assert!(!host.join("flat.png").exists());
        // A sibling service's file in the shared host directory is not
        // addressable: the same name resolves inside the area only.
        std::fs::write(host.join("events.json"), "{}").unwrap();
        assert!(dispatch("info", &json!({"path": "events.json"}), host).is_err());
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.sheets"));
        assert!(!may_call("org.example.anything"));
        assert!(!may_call(""));
    }

    /// The agent tools (`tools.json`) pass App Hub's own loader, as the shell
    /// reads them, keep the object schemas octos takes both ways, and name
    /// only methods this service dispatches.
    #[test]
    fn the_agent_tools_pass_app_hubs_loader_and_name_real_methods() {
        use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest};
        let (manifest, _) = ToolManifest::load(TOOLS_JSON, "vector", ToolHost::Contained, false).unwrap();
        assert!(!manifest.tools.is_empty());
        let dir = tempfile::tempdir().unwrap();
        for tool in &manifest.tools {
            assert_eq!(tool.implemented_by, ImplementedBy::HostService, "{}", tool.name);
            assert!(tool.host_method.is_none(), "{}: the shell routes each tool to the method of its own name", tool.name);
            for schema in [&tool.input_schema, &tool.output_schema] {
                assert_eq!(schema["type"], json!("object"), "{}: octos takes object schemas only", tool.name);
            }
            assert!(tool.description.is_ascii(), "{}: descriptions stay within octos's byte limit", tool.name);
            let method = tool.name.strip_prefix("vector.").unwrap();
            if let Err(e) = dispatch(method, &json!({}), dir.path()) {
                assert!(!e.contains("is not a method"), "{}: {e}", tool.name);
            }
        }
    }
}
