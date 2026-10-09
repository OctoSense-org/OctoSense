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
//! ([`octosense_engine_area::Stage`]). The generic command door `run` runs
//! through the shared gate ([`door`], ADR 0013 #418): an allowlist built
//! from the reviewed classification `skill/safety.json`. Only `safe` ids
//! run; `code` (the wrappers `command.batch`, the plug-in system, and
//! `prefs.set`, which can load a plug-ins folder), `file`, `host` and
//! unknown ids are refused. Two reviewed inner ids are checked by the gate:
//! `perspective.draw`'s named `shape.*` command passes the gate itself, and
//! `effect.apply` / `appearance.addEffect` admit only an effect the engine
//! builds in, never an effect plug-in `plugin.<id>`. After every command
//! the service re-fences the live document's image links, so a command
//! cannot plant a link the export would then read from outside the area.
//!
//! Methods (all under the `vector` family; paths relative to the area):
//! - `info {path, depth?}` → the document inspected as JSON (artboards,
//!   layer tree, object count, colour mode, units, import warnings)
//! - `convert {path, out, format?, scale?}` → `{out, format, bytes,
//!   warnings}` — export in the format the extension (or `format`) picks:
//!   SVG/SVGZ, PDF, EPS, DXF, EMF/WMF, PNG/JPG/WebP/GIF/TIFF/BMP/TGA/PSD,
//!   native `.vectorcraft`
//! - `run {path?, cmds: [{id, params?}], out?, format?, scale?}` → `{results,
//!   out: "<rel>" | null, format, bytes, warnings, files?}` — the command
//!   door: every id admitted by [`door`] before the engine runs any (a
//!   refused id refuses the whole call, nothing written), run in order on
//!   the document at `path` or a fresh one, then exported to `out`
//! - `commands {}` → the ids the door runs, from the engine's catalog
//! - `render {path, out, max_side?, artboard?}` → a PNG written to `out`
//!
//! The service serves system apps only until ADR 0013's store capability
//! is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::door::{Door, Inner, InnerRule, Reviewed};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};
use vectorcraft_engine::Session;

/// The longest preview edge `render` produces.
const MAX_RENDER_SIDE: u32 = 4096;
/// The largest file the service opens (bytes).
const MAX_OPEN_BYTES: u64 = 64 << 20;

/// What the vector engine's reviewer settled for the door beyond the classes
/// (ADR 0013, #418). No reviewed file read (a `file` command reaches paths
/// the service's own `path`/`out` do not cover, so none runs through the
/// door) and no setter (no `safe` vector command sets an app-wide preference
/// by key; every preference write is `host`, and `prefs.set` is `code`).
///
/// Inner ids:
/// - `perspective.draw {command}` runs a named `shape.*` command, which
///   passes the gate itself with its own `params`.
/// - `effect.apply` / `appearance.addEffect` append a live effect by id; the
///   gate admits only an effect the engine builds in ([`builtin_effect`]),
///   never an effect plug-in `plugin.<id>`. The engine's `apply` reads the
///   id from `effect` or, as an alias, `id`, so both keys are gated (the
///   door skips a key a call omits); a call that names a plug-in under
///   either is refused.
static REVIEWED: Reviewed = Reviewed {
    file_reads: &[],
    setters: &[],
    inner: &[
        Inner { id: "perspective.draw", param: "command", rule: InnerRule::Command { prefix: "shape.", params: "params" } },
        Inner { id: "effect.apply", param: "effect", rule: InnerRule::Effect { builtin: builtin_effect } },
        Inner { id: "effect.apply", param: "id", rule: InnerRule::Effect { builtin: builtin_effect } },
        Inner { id: "appearance.addEffect", param: "effect", rule: InnerRule::Effect { builtin: builtin_effect } },
        Inner { id: "appearance.addEffect", param: "id", rule: InnerRule::Effect { builtin: builtin_effect } },
    ],
};

/// The command door's gate: vectorcraft's reviewed classification
/// (`skill/safety.json`, generated and drift-checked by `tests/skill.rs`)
/// and [`REVIEWED`].
pub fn door() -> Result<&'static Door, String> {
    static DOOR: OnceLock<Result<Door, String>> = OnceLock::new();
    DOOR.get_or_init(|| Door::new("vector", include_str!("../skill/safety.json"), &REVIEWED)).as_ref().map_err(Clone::clone)
}

/// The ids of the effects the engine builds in, computed once from a fresh
/// session's `effect.list` catalog, with any `plugin.<id>` excluded. A fresh
/// session installs no plug-ins, so this is exactly the built-in catalogue.
fn builtin_effects() -> &'static BTreeSet<String> {
    static SET: OnceLock<BTreeSet<String>> = OnceLock::new();
    SET.get_or_init(|| {
        let listed = Session::new().execute("effect.list", &json!({})).unwrap_or(Json::Null);
        listed["catalog"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| e["id"].as_str())
            .filter(|id| !id.starts_with("plugin."))
            .map(str::to_string)
            .collect()
    })
}

/// Whether `name` is an effect the engine builds in (never an installed
/// effect plug-in `plugin.<id>`).
fn builtin_effect(name: &str) -> bool {
    builtin_effects().contains(name)
}

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
    let area = areas.area(call, "vector").map_err(|e| format!("vector: {e}"))?;
    dispatch_in(call.method(), &call.args, &area)
        .map(|answer| area.relative_json(answer))
        .map_err(|error| area.relative_text(&error))
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
    // A raster export writes one artboard: this one (0-based, default 0).
    if let Some(a) = args["artboard"].as_u64() {
        params["artboard"] = json!(a);
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

/// After a command runs, refuse the call if any file the engine would read
/// for the live document's linked images is outside the area. The open-time
/// [`fence_links`] checks the file on disk; this checks the document a
/// command just changed, so a command cannot plant a link the export would
/// then read from elsewhere (e.g. `clipboard.importSvg` of an `<image href>`
/// that points out, then `edit.pasteInPlace`). Reuses the open-time fence's
/// [`link_files`] and [`inside`] on the session's current document; relative
/// links resolve against `dir` (the opened document's folder, else the area
/// root), and `inside` refuses any that is not an existing file in the area.
fn fence_live_links(area: &Area, s: &Session, dir: &Path, method: &str) -> Result<(), String> {
    let Ok(st) = s.doc() else { return Ok(()) };
    let root = area.root.canonicalize().map_err(|e| format!("vector.{method}: folder: {e}"))?;
    if let Some(out) = link_files(&st.doc, dir).iter().find(|p| !inside(&root, p)) {
        return Err(format!(
            "vector.{method}: a command set an image link `{}`, outside this call's folder: the engine would read it from there; embed the images instead",
            out.display()
        ));
    }
    Ok(())
}

fn run(args: &Json, area: &Area) -> Result<Json, String> {
    // Admit every command first: one refused id refuses the whole call, with
    // nothing opened and nothing written.
    let admitted = door()?.admit_all(&args["cmds"], area)?;
    // A name the call may not write is refused before the engine works.
    let out = match args["out"].as_str().filter(|o| !o.is_empty()) {
        Some(rel) => Some(out_path(area, rel)?),
        None => None,
    };
    let mut s = Session::new();
    // The folder relative links resolve against: the opened document's (its
    // own links were fenced at open), else the area root for a fresh one.
    let dir = match args["path"].as_str().filter(|p| !p.is_empty()) {
        Some(rel) => {
            open(&mut s, area, rel, "run")?;
            contained_path(&area.root, rel).ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(|| area.root.clone())
        }
        // No input: a fresh default document, ready to draw into.
        None => {
            s.execute("file.new", &json!({})).map_err(|e| format!("vector.run: {e}"))?;
            area.root.clone()
        }
    };
    let mut results = Vec::with_capacity(admitted.len());
    for (id, params) in &admitted {
        let r = s.execute(id, params).map_err(|e| format!("vector.run {id}: {e}"))?;
        // No command may plant a link the export would read from outside.
        fence_live_links(area, &s, &dir, "run")?;
        results.push(json!({"id": id, "result": r}));
    }
    match out {
        // The staged export's fields, flat, with the command results.
        Some(out) => {
            let mut v = export(&mut s, area, args, &out, "run")?;
            if let Some(obj) = v.as_object_mut() {
                obj.insert("results".into(), Json::Array(results));
            }
            Ok(v)
        }
        None => Ok(json!({"results": results, "out": Json::Null})),
    }
}

/// The ids `run` accepts: the engine's catalog filtered to what the door
/// runs (every `safe` id; no `file`, `code`, `host` or unknown id).
fn commands() -> Result<Json, String> {
    let door = door()?;
    let list: Vec<Json> = Session::new()
        .commands()
        .iter()
        .filter(|c| door.runs(c.id))
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

    /// The ids a fresh engine session reports as installed plug-ins (empty
    /// unless a plug-in got installed), read through the engine's own
    /// `plugin.list`, never through the door.
    fn installed_plugins() -> Vec<String> {
        let listed = Session::new().execute("plugin.list", &json!({})).unwrap();
        listed["plugins"].as_array().unwrap().iter().filter_map(|p| p["id"].as_str().map(str::to_string)).collect()
    }

    /// With the shell's resolver installed the door now runs: an allowlisted
    /// run draws shapes on a fresh document, applies a built-in effect with
    /// `effect.apply`, and exports to `out`, the file landing inside the
    /// area; a run on an existing SVG works too.
    #[test]
    fn the_door_runs_allowlisted_commands_in_its_area() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let made = serve(
            &areas,
            &service_call(
                "run",
                json!({"cmds": [
                    {"id": "shape.rectangle", "params": {"x": 4.0, "y": 4.0, "width": 24.0, "height": 16.0}},
                    {"id": "effect.apply", "params": {"effect": "stylize.dropShadow"}},
                    {"id": "shape.ellipse", "params": {"x": 30.0, "y": 8.0, "width": 16.0, "height": 16.0}},
                    {"id": "document.inspect", "params": {"depth": 0}}
                ], "out": "art/drawn.svg"}),
                dir.path(),
                false,
            ),
        )
        .unwrap();
        assert_eq!(made["out"], json!("art/drawn.svg"), "{made}");
        assert_eq!(made["format"], json!("svg"));
        assert_eq!(made["results"].as_array().unwrap().len(), 4);
        assert_eq!(made["results"][1]["id"], json!("effect.apply"), "{made}");
        assert!(made["results"][3]["result"]["objects"].as_u64().unwrap() >= 2, "{made}");
        assert!(dir.path().join("art/drawn.svg").is_file() && no_staging_left(dir.path()));
        // An existing SVG, edited and rasterised to PNG beside it.
        std::fs::write(dir.path().join("in.svg"), SVG).unwrap();
        let edited = serve(
            &areas,
            &service_call("run", json!({"path": "in.svg", "cmds": [{"id": "shape.rectangle", "params": {"x": 1.0, "y": 1.0, "width": 8.0, "height": 8.0}}], "out": "out.png"}), dir.path(), false),
        )
        .unwrap();
        assert_eq!(edited["format"], json!("png"), "{edited}");
        assert!(std::fs::read(dir.path().join("out.png")).unwrap().starts_with(b"\x89PNG"));
        // A query with no `out` writes nothing and reports a null `out`.
        let query = serve(&areas, &service_call("run", json!({"path": "in.svg", "cmds": [{"id": "document.inspect", "params": {"depth": 0}}]}), dir.path(), false)).unwrap();
        assert!(query["out"].is_null() && query["results"][0]["result"]["objects"].as_u64().is_some(), "{query}");
    }

    /// Every class but `safe` present in vector's classification is refused,
    /// and so is an id the classification does not know, before any command
    /// runs: a refused id anywhere in the list writes nothing.
    #[test]
    fn the_door_refuses_every_other_class_and_unknown_ids() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        // A representative of each refused class in safety.json.
        for (id, class) in [
            ("document.open", "file"),       // file, not reviewed
            ("swatch.library.save", "file"), // file, not reviewed
            ("command.batch", "code"),       // runs other commands
            ("prefs.set", "code"),           // can load a plug-ins folder
            ("plugin.install", "code"),      // installs WebAssembly
            ("view.proofSetup", "host"),     // process-wide render proof
            ("file.new", "host"),            // records a recent-size preference
        ] {
            let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "shape.rectangle", "params": {"x": 0.0, "y": 0.0, "width": 4.0, "height": 4.0}}, {"id": id}], "out": "x.svg"}), dir.path(), false)).unwrap_err();
            if class == "file" {
                assert!(e.contains("not reviewed to run through it"), "{id}: {e}");
            } else {
                assert!(e.contains(&format!("`{id}` is classed {class}")), "{id}: {e}");
            }
        }
        let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "vector.secret"}]}), dir.path(), false)).unwrap_err();
        assert!(e.contains("not a reviewed vector command"), "{e}");
        assert!(!dir.path().join("x.svg").exists(), "a refused id anywhere wrote nothing");
        let too_many: Vec<Json> = (0..65).map(|_| json!({"id": "shape.rectangle", "params": {"x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0}})).collect();
        assert!(serve(&areas, &service_call("run", json!({"cmds": too_many}), dir.path(), false)).unwrap_err().contains("at most 64"));
    }

    /// #418's three routes past the old deny-list are all refused by the
    /// gate, and none installs a plug-in: (a) `command.batch` wrapping
    /// `plugin.install`; (b) `prefs.set` of a plug-ins folder; (c)
    /// `effect.apply` / `appearance.addEffect` of a `plugin.<id>` effect.
    #[test]
    fn the_door_refuses_418s_three_hostile_fixtures() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        // A folder with a .wasm, for the prefs.set fixture.
        let plugins = dir.path().join("plugins");
        std::fs::create_dir(&plugins).unwrap();
        std::fs::write(plugins.join("evil.wasm"), b"\x00asm\x01\x00\x00\x00").unwrap();
        let fixtures = [
            // (a) a batch wrapping an install of a tiny valid module header.
            json!({"id": "command.batch", "params": {"commands": [{"command": "plugin.install", "params": {"dataBase64": "AGFzbQEAAAA="}}]}}),
            // (b) the Additional Plug-ins Folder preference.
            json!({"id": "prefs.set", "params": {"key": "pluginsFolder", "value": plugins.to_string_lossy()}}),
            // (c) an effect plug-in, through either command and key.
            json!({"id": "effect.apply", "params": {"effect": "plugin.anything"}}),
            json!({"id": "effect.apply", "params": {"id": "plugin.anything"}}),
            json!({"id": "appearance.addEffect", "params": {"effect": "plugin.anything"}}),
            json!({"id": "appearance.addEffect", "params": {"id": "plugin.anything"}}),
        ];
        for cmd in fixtures {
            let label = cmd.to_string();
            let refused = serve(&areas, &service_call("run", json!({"cmds": [cmd], "out": "x.svg"}), dir.path(), false)).unwrap_err();
            assert!(refused.contains("classed code") || refused.contains("is not an effect the engine builds in"), "{label}: {refused}");
            assert!(!dir.path().join("x.svg").exists(), "{label}: wrote nothing");
            assert!(installed_plugins().is_empty(), "{label}: no plug-in installed");
        }
    }

    /// `perspective.draw` admits only a named `shape.*` command, which passes
    /// the gate itself (its own params admitted); any other inner id is
    /// refused before the engine runs. (Whether the engine then attaches the
    /// shape to the grid is its own perspective geometry, not the gate's.)
    #[test]
    fn perspective_draw_runs_only_shape_commands() {
        let dir = tempfile::tempdir().unwrap();
        let area = Area::new(dir.path(), None, false);
        let door = door().unwrap();
        // The named shape command passes the gate, so its params are admitted
        // and handed back under `params` for the engine to run.
        let ok = door.admit("perspective.draw", &json!({"command": "shape.rectangle", "params": {"x": 2.0, "y": 2.0, "width": 10.0, "height": 10.0}}), &area).unwrap();
        assert_eq!(ok["command"], json!("shape.rectangle"), "{ok}");
        assert_eq!(ok["params"]["width"], json!(10.0), "{ok}");
        // Any other inner id is refused, whatever its own class.
        let areas = resolver(dir.path(), None);
        for inner in ["plugin.install", "document.open", "command.batch", "file.new", "object.group"] {
            let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "perspective.draw", "params": {"command": inner, "params": {}}}]}), dir.path(), false)).unwrap_err();
            assert!(e.contains("runs only `shape.*` commands"), "{inner}: {e}");
        }
    }

    /// The post-command link fence: a `safe` command can plant an image link
    /// pointing outside the area (`clipboard.importSvg` of an `<image href>`
    /// that climbs out, then `edit.pasteInPlace`); the call is refused after
    /// that command, before any export reads the link.
    #[test]
    fn a_command_that_plants_an_outside_link_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let outside = dir.path().join("private.png");
        std::fs::write(&outside, png()).unwrap();
        let areas = resolver(&root, None);
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><image href="{}" width="12" height="8"/></svg>"##,
            outside.display()
        );
        let refused = serve(
            &areas,
            &service_call(
                "run",
                json!({"cmds": [
                    {"id": "clipboard.importSvg", "params": {"svg": svg}},
                    {"id": "edit.pasteInPlace"}
                ], "out": "c.svg"}),
                &root,
                false,
            ),
        )
        .unwrap_err();
        assert!(refused.contains("private.png") && refused.contains("outside this call's folder"), "{refused}");
        assert!(!root.join("c.svg").exists() && no_staging_left(&root), "nothing written");
        // The same objects whose images are embedded (a data: URL) pass:
        // no link is planted.
        let data_svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10" viewBox="0 0 20 10"><rect x="2" y="2" width="8" height="6" fill="#3366cc"/></svg>"##;
        serve(&areas, &service_call("run", json!({"cmds": [{"id": "clipboard.importSvg", "params": {"svg": data_svg}}, {"id": "edit.pasteInPlace"}], "out": "ok.svg"}), &root, false)).unwrap();
        assert!(root.join("ok.svg").is_file());
    }

    /// The door's gate is built from the generated classification and the
    /// reviewed inner ids; it runs every `safe` id and no other, and the
    /// built-in effect catalogue it admits is the engine's own.
    #[test]
    fn the_door_is_built_from_the_reviewed_classification() {
        let door = door().unwrap();
        assert!(door.runs("shape.rectangle") && door.runs("effect.apply") && door.runs("appearance.addEffect") && door.runs("perspective.draw"));
        assert!(door.runs("document.inspect") && door.runs("edit.pasteInPlace") && door.runs("clipboard.importSvg"));
        assert!(!door.runs("document.open") && !door.runs("command.batch") && !door.runs("prefs.set") && !door.runs("plugin.install"));
        assert!(!door.runs("view.proofSetup") && !door.runs("file.new") && !door.runs("vector.secret"));
        assert!(door.runnable().len() > 500, "{}", door.runnable().len());
        // The built-in effect catalogue the inner rule admits, computed from
        // a fresh session's effect.list, holds real built-ins and no plug-in.
        let effects = builtin_effects();
        assert!(effects.len() >= 34, "{} built-in effects", effects.len());
        assert!(builtin_effect("stylize.dropShadow") && builtin_effect("warp.arc") && builtin_effect("blur.gaussian"));
        assert!(!builtin_effect("plugin.anything") && !builtin_effect(""));
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

        // The engine's file commands are the host's: the gate refuses them
        // (every one is classed `file` or `host`, none reviewed).
        for refused in [
            json!({"id": "document.open", "params": {}}),
            json!({"id": "document.export", "params": {}}),
            json!({"id": "file.recovery.list"}),
            json!({"id": "swatch.library.save", "params": {"name": "x"}}),
        ] {
            let r = dispatch("run", &json!({"path": input, "cmds": [refused]}), host);
            assert!(r.is_err(), "{refused}");
        }
        // A `safe` drawing command carrying an extra path-like key is a
        // harmless no-op: the engine ignores the key, the door needs no
        // path fence because a `safe` command touches no file.
        let ok = dispatch("run", &json!({"path": input, "cmds": [{"id": "shape.rectangle", "params": {"x": 0.0, "y": 0.0, "width": 5.0, "height": 5.0, "path": "up.svg"}}]}), host).unwrap();
        assert_eq!(ok["results"][0]["id"], json!("shape.rectangle"), "{ok}");
        assert!(ok["out"].is_null(), "no out: {ok}");
    }

    /// The plug-in registry is process-wide and installs WebAssembly from
    /// in-band data: the gate refuses every `plugin.*` id (classed `code`)
    /// before the engine sees it, and the bare `plugin` id is unknown. The
    /// catalog offer matches.
    #[test]
    fn run_refuses_plugin_commands_and_the_catalog_omits_them() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        for (cmd, needle) in [
            // A core module's header: refused before anything parses it.
            (json!({"id": "plugin.install", "params": {"dataBase64": "AGFzbQEAAAA="}}), "classed code"),
            (json!({"id": "plugin.list"}), "classed code"),
            (json!({"id": "plugin.remove", "params": {"id": "org.vectorcraft.example.desaturate"}}), "classed code"),
            (json!({"id": "plugin.reload"}), "classed code"),
            (json!({"id": "plugin"}), "not a reviewed vector command"),
        ] {
            let r = dispatch("run", &json!({"cmds": [cmd.clone()]}), host);
            let e = r.expect_err(&cmd.to_string());
            assert!(e.contains(needle), "{cmd}: {e}");
        }
        let cat = commands().unwrap();
        let ids: Vec<&str> = cat.as_array().unwrap().iter().filter_map(|c| c["id"].as_str()).collect();
        assert!(ids.iter().all(|id| *id != "plugin" && !id.starts_with("plugin.")), "no plug-in commands offered");
    }

    #[test]
    fn commands_catalog_is_real_and_only_runnable_ids() {
        let cat = commands().unwrap();
        let cat = cat.as_array().unwrap();
        assert!(cat.len() > 200, "a real catalog, {} commands", cat.len());
        let ids: Vec<&str> = cat.iter().filter_map(|c| c["id"].as_str()).collect();
        assert!(ids.contains(&"document.inspect") && ids.contains(&"shape.rectangle") && ids.contains(&"effect.apply"));
        // A `safe`, session-only `file.*` command is offered; the `file`-class
        // access commands and the plug-in system are not.
        assert!(ids.contains(&"file.close"), "a session-only file command is offered");
        assert!(!ids.contains(&"file.saveAs") && !ids.contains(&"file.place"), "file access is not offered");
        assert!(!ids.contains(&"document.open") && !ids.contains(&"document.export"));
        assert!(ids.iter().all(|id| !id.starts_with("plugin.")), "no plug-in commands offered");
        // Every offered id is one the door actually runs.
        let door = door().unwrap();
        assert!(ids.iter().all(|id| door.runs(id)), "only runnable ids are offered");
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

    /// A raster `out` is one artboard: `artboard` picks which (0-based),
    /// at `scale` pixels per point.
    #[test]
    fn the_door_exports_the_artboard_it_is_given() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let cmds = json!([{"id": "artboard.new", "params": {"width": 100, "height": 50}}]);
        let second = serve(&areas, &service_call("run", json!({"cmds": cmds, "out": "second.png", "artboard": 1, "scale": 2}), dir.path(), false)).unwrap();
        assert_eq!(second["out"], json!("second.png"), "{second}");
        let png = std::fs::read(dir.path().join("second.png")).unwrap();
        let size = (u32::from_be_bytes(png[16..20].try_into().unwrap()), u32::from_be_bytes(png[20..24].try_into().unwrap()));
        assert_eq!(size, (200, 100), "the new 100x50 pt artboard at 2 px/pt");
    }

    /// Every `vector.run` call the skill's examples show runs, in order,
    /// in one area as the system agent's (with the files they name placed
    /// there first), and writes its `out`: the skill teaches commands that
    /// work.
    #[test]
    fn the_skill_examples_run() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        std::fs::write(dir.path().join("logo.svg"), br##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="40" viewBox="0 0 64 40"><rect x="4" y="4" width="32" height="20" fill="#3366cc"/><circle cx="48" cy="20" r="10" fill="#cc3333"/></svg>"##).unwrap();
        let body = include_str!("../skill/SKILL.md");
        let examples = body.split("\n## Examples").nth(1).and_then(|rest| rest.split("\n## ").next()).unwrap();
        let mut ran = 0;
        for span in examples.split('`').skip(1).step_by(2) {
            let Some(args) = span.strip_prefix("vector.run ") else { continue };
            let args: Json = serde_json::from_str(args).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            let got = serve(&areas, &service_call("run", args.clone(), dir.path(), false)).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            assert_eq!(got["results"].as_array().map(Vec::len), args["cmds"].as_array().map(Vec::len), "{got}");
            if let Some(out) = args["out"].as_str() {
                assert!(dir.path().join(out).is_file(), "`{span}` wrote no {out}");
            }
            ran += 1;
        }
        assert!(ran >= 4, "{ran} examples");
        let png = std::fs::read(dir.path().join("logo@4x.png")).unwrap();
        assert_eq!((u32::from_be_bytes(png[16..20].try_into().unwrap()), u32::from_be_bytes(png[20..24].try_into().unwrap())), (256, 160), "4x the 64x40 artboard");
        let badge = std::fs::read_to_string(dir.path().join("badge.svg")).unwrap();
        assert!(badge.contains("Beta") && badge.contains("3366cc"), "{badge}");
    }
}
