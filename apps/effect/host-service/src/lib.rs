//! `octosense-effect-service` — the `effect` host service (ADR 0013).
//!
//! effectcraft's motion-graphics engine (an After Effects-class compositor:
//! compositions, layers, keyframes, 300+ effects, expressions, Lottie)
//! through its headless automation backend. Every call is a fresh,
//! stateless session driven by the same command registry every effectcraft
//! frontend dispatches through.
//!
//! Methods (all under the `effect` family; paths relative to the call's
//! area):
//! - `info {path}` → the project summarised (items, comps, sizes, rates)
//! - `render {path, comp?, time?, out, max_side?, transparent?}` → one comp
//!   frame written to `out` as PNG
//! - `run {path?, cmds: [{id, params?}], out?}` → command results, saving
//!   the project when `out` is given (`effect.commands` lists the catalog)
//! - `commands {filter?}` → the engine's command catalog
//! - `export_lottie {path, comp?, out, include_expressions?}` → a comp as
//!   Lottie `.json`/`.lottie`, with the engine's warnings list
//! - `import_lottie {path, out}` → a Lottie file opened as a composition
//!   and saved as an `.ecproj` project
//!
//! **Where a call works** (ADR 0013, 2026-10-08): the caller's own folder,
//! the [`Area`] the shell's resolver gives it ([`set_area_resolver`]), or
//! without one the legacy `<host dir>/effect`. Boundary paths are validated
//! before any I/O, and the engine side is gated too: its file I/O
//! ([`Services`]), media probing ([`Importer`]) and footage decoding
//! (`FootageSource`) each re-check their paths against the area — an image
//! sequence's every frame, not only its first — so a project that
//! references footage outside it renders placeholders instead of reading
//! it. 3D model footage is refused and renders nothing: the engine reads a
//! model's sibling files (buffers, textures, materials) by the paths inside
//! the model, with no gate of its own. Effect parameters that the engine
//! reads as a file by their own path, past those gates (a LUT, an OCIO
//! file transform or config, a mocha shape file), refuse the project unless
//! they hold the file's text inline ([`fence_effect_files`]). What the
//! engine writes keeps the area's rules ([`Area::write`]: a write that may
//! not replace, an agent's, only creates new files, within the quota). The
//! generic command door `run` is held for its own review: with the shell's
//! resolver installed it is refused outright, because the engine's command
//! wrappers (`engine.batch`, `file.runScript`, whose scripts reach the
//! file system directly) and `prefs.set` slip past any list of refused ids.
//! The service serves system apps only until ADR 0013's store capability is
//! designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use effectcraft_automation::Backend;
use effectcraft_engine::project::{Footage, ItemId};
use effectcraft_engine::raster::{AuxChannels, Image};
use effectcraft_engine::render::FootageSource;
use effectcraft_engine::time::Tick;
use effectcraft_engine::{Importer, Services, Session};
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};

/// The longest frame edge `render` produces.
const MAX_RENDER_SIDE: u32 = 4096;
/// The most commands one `run` call executes.
const MAX_CMDS: usize = 64;
/// The largest file the engine's guarded I/O reads or writes (bytes):
/// projects and Lottie files are JSON, and frames are written by `render`.
const MAX_FILE_BYTES: u64 = 64 << 20;

/// Which apps may call the service: system apps, as Sheets and Photo.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct EffectService;

/// Register the `effect` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(EffectService));
}

/// The shell's resolver: where each call works (`None` removes it, and
/// calls work in the legacy `<host dir>/effect` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
}

/// The `effect.*` agent tools (ADR 0013, wave 2), in App Hub's `tools.json`
/// shape: the shell declares them for the virtual owner `os.effect` and grants
/// the system agent its reviewed share (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`).
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for EffectService {
    fn family(&self) -> &'static str {
        "effect"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        reply.send(serve(&AREAS, &call));
    }
}

/// One call, in the area `areas` gives it. The command door `run` works
/// only without the shell's resolver (this crate's tests): in the shell it
/// is held for its own review ([`callable`]).
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The effect service serves system apps only.".into());
    }
    if call.method() == "run" && areas.is_set() {
        return Err("effect.run is held for its own review: the engine's command door is not available in the shell".into());
    }
    let area = areas.area(call, "effect").map_err(|e| format!("effect: {e}"))?;
    dispatch_in(call.method(), &call.args, &Arc::new(area))
}

fn dispatch_in(method: &str, args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    match method {
        "info" => info(args, area),
        "render" => render(args, area),
        "run" => run(args, area),
        "commands" => commands(args, area),
        "export_lottie" => export_lottie(args, area),
        "import_lottie" => import_lottie(args, area),
        other => Err(format!("effect.{other} is not a method of the effect service")),
    }
}

/// [`dispatch_in`] in the legacy area `<host_dir>/effect`, as a call
/// without the shell's resolver works.
#[cfg(test)]
fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    let area = Area::legacy(host_dir, "effect");
    std::fs::create_dir_all(&area.root).map_err(|e| format!("effect: {e}"))?;
    dispatch_in(method, args, &Arc::new(area))
}

/// A path strictly inside the call's area: relative, normal components
/// only, and resolved (through its deepest existing ancestor, so symlinks
/// cannot escape) under the canonical area.
fn contained(area: &Area, key: &str, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("effect: `{key}` is required"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("effect: `{key}` stays inside this call's effect area"));
    }
    let joined = area.root.join(rel_path);
    let root = area.root.canonicalize().map_err(|e| format!("effect: {e}"))?;
    allowed_in(&root, &joined.to_string_lossy()).map_err(|e| format!("effect: `{key}`: {e}"))?;
    Ok(joined)
}

/// A contained output path the call may write, refused before the engine
/// works when the area's rules would refuse it.
fn out_path(area: &Area, rel: &str) -> Result<PathBuf, String> {
    let out = contained(area, "out", rel)?;
    area.check(&out, 0).map_err(|e| format!("effect: {e}"))?;
    Ok(out)
}

/// `path`, if it resolves (through its deepest existing ancestor) inside
/// the canonical `root`.
fn allowed_in(root: &Path, path: &str) -> std::io::Result<PathBuf> {
    let outside = || std::io::Error::new(std::io::ErrorKind::PermissionDenied, "the path is outside the call's effect area");
    let p = Path::new(path);
    let mut deepest = p.to_path_buf();
    while !deepest.exists() {
        match deepest.parent() {
            Some(parent) => deepest = parent.to_path_buf(),
            None => return Err(outside()),
        }
    }
    if !deepest.canonicalize()?.starts_with(root) {
        return Err(outside());
    }
    Ok(p.to_path_buf())
}

/// The engine-side gate (installed as the session's [`Services`]): every
/// read and write the engine performs — `file.open`, `file.saveAs`, the
/// Lottie commands and their extracted assets — re-checks that its path
/// resolves inside the area, and every write keeps the call's rules.
struct Guard {
    /// The canonical area.
    root: PathBuf,
    /// The call's area, whose rules the engine's writes keep.
    rules: Arc<Area>,
}

impl Guard {
    fn new(rules: &Arc<Area>) -> std::io::Result<Self> {
        Ok(Guard { root: rules.root.canonicalize()?, rules: rules.clone() })
    }

    /// The path, if it resolves (through its deepest existing ancestor)
    /// inside the area.
    fn allowed(&self, path: &str) -> std::io::Result<PathBuf> {
        allowed_in(&self.root, path)
    }

    /// The footage's every file inside the area: its path, and each frame
    /// of an image sequence (decoded by its own path).
    fn footage_inside(&self, footage: &Footage) -> bool {
        self.allowed(&footage.path).is_ok() && footage.sequence.iter().all(|frame| self.allowed(frame).is_ok())
    }
}

/// A 3D model's file: the engine reads its sibling resources by the paths
/// written inside it, outside any gate, so the service refuses models.
fn is_model(path: &str) -> bool {
    let ext = Path::new(path).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    effectcraft_media::MODEL_EXTENSIONS.contains(&ext.as_str())
}

impl Services for Guard {
    fn read_file(&self, path: &str) -> std::io::Result<Vec<u8>> {
        let p = self.allowed(path)?;
        if std::fs::metadata(&p)?.len() > MAX_FILE_BYTES {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "the file is larger than the effect service reads"));
        }
        std::fs::read(p)
    }

    fn write_file(&self, path: &str, data: &[u8]) -> std::io::Result<()> {
        if data.len() as u64 > MAX_FILE_BYTES {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "the file is larger than the effect service writes"));
        }
        let p = self.allowed(path)?;
        self.rules.write(&p, data).map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e))
    }

    fn exists(&self, path: &str) -> bool {
        self.allowed(path).map(|p| p.exists()).unwrap_or(false)
    }
}

/// Media probing gated to the area: footage a project references outside
/// it never gets probed into the project.
struct ContainedImporter {
    guard: Arc<Guard>,
}

impl Importer for ContainedImporter {
    fn probe(&self, path: &str) -> Result<Footage, String> {
        self.guard.allowed(path).map_err(|e| e.to_string())?;
        if is_model(path) {
            return Err(format!("{path}: 3D models are not imported through the effect service"));
        }
        // A still with numbered siblings imports as a sequence: every
        // sibling must be inside the area before the engine reads one.
        for frame in effectcraft_media::sequence_files(path) {
            self.guard.allowed(&frame.to_string_lossy()).map_err(|e| e.to_string())?;
        }
        let footage = effectcraft_media::probe(path).map_err(|e| e.to_string())?;
        if !self.guard.footage_inside(&footage) {
            return Err(format!("{path}: the footage reaches outside the call's effect area"));
        }
        Ok(footage)
    }
}

/// Footage decoding gated to the area: a `Footage` whose path escapes it
/// decodes to nothing (the renderer draws its placeholder).
struct ContainedFootage {
    pool: effectcraft_media::MediaPool,
    guard: Arc<Guard>,
}

impl ContainedFootage {
    fn inside(&self, footage: &Footage) -> bool {
        self.guard.footage_inside(footage)
    }
}

impl FootageSource for ContainedFootage {
    fn frame(&self, item: ItemId, footage: &Footage, t: Tick) -> Option<Arc<Image>> {
        if !self.inside(footage) {
            return None;
        }
        FootageSource::frame(&self.pool, item, footage, t)
    }

    fn audio(&self, item: ItemId, footage: &Footage, t: Tick, frames: usize, rate: u32) -> Option<Vec<f32>> {
        if !self.inside(footage) {
            return None;
        }
        FootageSource::audio(&self.pool, item, footage, t, frames, rate)
    }

    fn aux(&self, item: ItemId, footage: &Footage, t: Tick) -> Option<Arc<AuxChannels>> {
        if !self.inside(footage) {
            return None;
        }
        FootageSource::aux(&self.pool, item, footage, t)
    }

    /// No model renders through the service: the engine would read the
    /// model's sibling files by the paths inside it, outside any gate.
    fn model(&self, _item: ItemId, _footage: &Footage) -> Option<Arc<effectcraft_model::Model>> {
        None
    }

    fn vector_frame(&self, item: ItemId, footage: &Footage, scale: f64) -> Option<Arc<Image>> {
        if !self.inside(footage) {
            return None;
        }
        FootageSource::vector_frame(&self.pool, item, footage, scale)
    }

    fn set_cache_budget(&self, bytes: usize) {
        FootageSource::set_cache_budget(&self.pool, bytes);
    }

    fn purge(&self) {
        FootageSource::purge(&self.pool);
    }

    // `set_conform_folder` is deliberately not forwarded: the default keeps
    // the pool's decoded-audio cache in memory instead of letting a command
    // point it at a folder.

    fn cache_budget(&self) -> Option<usize> {
        FootageSource::cache_budget(&self.pool)
    }
}

/// A fresh headless session whose file I/O, media probing and footage
/// decoding are all bound to the area. No exporter (the render queue's
/// encoders write wherever their output modules point, so `run` cannot
/// start one), no scripting, no plug-ins, no model downloads.
fn session(area: &Arc<Area>) -> Result<Backend, String> {
    let guard = Arc::new(Guard::new(area).map_err(|e| format!("effect: {e}"))?);
    let s = Session {
        services: guard.clone(),
        footage: Arc::new(ContainedFootage { pool: effectcraft_media::MediaPool::new(), guard: guard.clone() }),
        importer: Some(Arc::new(ContainedImporter { guard })),
        expr: Some(Arc::new(effectcraft_expr::Expressions)),
        expr_check: Some(effectcraft_expr::check_syntax),
        ..Default::default()
    };
    Ok(Backend::headless(s))
}

/// Open `args.path` (relative to the area) in the session, its effects'
/// file parameters fenced ([`fence_effect_files`]).
fn open(b: &mut Backend, area: &Area, args: &Json, method: &str) -> Result<String, String> {
    let rel = args["path"].as_str().unwrap_or("").to_string();
    let abs = contained(area, "path", &rel)?;
    b.exec("file.open", json!({"path": abs.to_string_lossy()})).map_err(|e| format!("effect.{method}: {e}"))?;
    fence_effect_files(b, method)?;
    Ok(rel)
}

/// How the engine reads an effect parameter that may name a file: each
/// loader takes the file's text inline, and reads anything else as a path,
/// with `std::fs`, outside the session's gated services.
#[derive(Clone, Copy)]
enum FileParam {
    /// A `.cube` LUT (`load_lut`): inline when it has a line break.
    Lut,
    /// An OCIO file transform (`ocio::load_file`): inline when it has a
    /// line break.
    OcioFile,
    /// An OCIO config (`load_config`): even inline, its search paths and
    /// file transforms name files, so any config is refused.
    OcioConfig,
    /// mocha shape data (`mocha_shape::load`): inline when it is JSON.
    MochaShapes,
}

impl FileParam {
    /// The parameter `key` (its match path inside the effect), when the
    /// engine may read it as a file: Apply Color LUT's `lut`, Lumetri's
    /// input LUT and look, OCIO's `file` and `configFile`, mocha's
    /// `shapeData`.
    fn of(key: &str) -> Option<FileParam> {
        let leaf = key.rsplit('/').next().unwrap_or(key);
        match (key, leaf) {
            (_, "lut") | ("basicCorrection/inputLutFile", _) | ("creative/lookFile", _) | (_, "inputLutFile") | (_, "lookFile") => Some(FileParam::Lut),
            (_, "file") => Some(FileParam::OcioFile),
            (_, "configFile") => Some(FileParam::OcioConfig),
            (_, "shapeData") => Some(FileParam::MochaShapes),
            _ => None,
        }
    }

    /// Whether the engine would take `value` inline (or ignore it) rather
    /// than read it as a path.
    fn inline(self, value: &str) -> bool {
        let t = value.trim();
        t.is_empty()
            || match self {
                FileParam::Lut | FileParam::OcioFile => value.contains('\n'),
                FileParam::OcioConfig => false,
                FileParam::MochaShapes => t.starts_with('{') || t.starts_with('['),
            }
    }
}

/// Refuse a project whose effects would make the engine read a file by its
/// own path: a LUT, OCIO file or config, or mocha shape parameter holding
/// a path (in its value or any keyframe), or driven by an expression,
/// which could produce one when the frame renders. The engine reads those
/// with `std::fs`, outside the session's gated services, so no area check
/// could stop the read; the file's text inline is drawn as before.
fn fence_effect_files(b: &mut Backend, method: &str) -> Result<(), String> {
    use effectcraft_engine::project::{ItemKind, Node, PropGroup, Value};
    fn walk(g: &PropGroup, prefix: &str, effect: &str, layer: &str, method: &str) -> Result<(), String> {
        for node in &g.children {
            match node {
                Node::Group(sub) => walk(sub, &format!("{prefix}{}/", sub.match_id), effect, layer, method)?,
                Node::Prop(p) => {
                    let key = format!("{prefix}{}", p.match_id);
                    let Some(kind) = FileParam::of(&key).or_else(|| FileParam::of(&p.match_id)) else { continue };
                    let refused = |why: &str| {
                        Err(format!(
                            "effect.{method}: the {effect} effect on layer `{layer}` {why} for `{key}`: the engine would read a file by its own path, outside this call's folder; put the file's text in the parameter instead"
                        ))
                    };
                    if p.has_expression() {
                        return refused("has an expression");
                    }
                    for value in std::iter::once(&p.value).chain(p.keys.iter().map(|k| &k.value)) {
                        if let Value::Str(text) = value {
                            if !kind.inline(text) {
                                return refused("names a file");
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    let Some(session) = b.session() else { return Ok(()) };
    for item in session.project.items.values() {
        let ItemKind::Comp(comp) = &item.kind else { continue };
        for layer in &comp.layers {
            let Some(effects) = layer.effects() else { continue };
            for node in &effects.children {
                if let Node::Group(effect) = node {
                    walk(effect, "", &effect.match_id, &layer.name, method)?;
                }
            }
        }
    }
    Ok(())
}

fn info(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let mut b = session(area)?;
    let rel = open(&mut b, area, args, "info")?;
    let mut sum = b.exec("project.summary", json!({})).map_err(|e| format!("effect.info: {e}"))?;
    sum["path"] = json!(rel);
    Ok(sum)
}

fn render(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    let out = out_path(area, out_rel)?;
    let requested = args["max_side"].as_u64().unwrap_or(1024);
    let max_side = if requested == 0 { MAX_RENDER_SIDE } else { requested.min(MAX_RENDER_SIDE as u64) as u32 };
    let mut b = session(area)?;
    open(&mut b, area, args, "render")?;
    let comp = args.get("comp").filter(|c| !c.is_null());
    let transparent = args["transparent"].as_bool().unwrap_or(false);
    let frame = b.render_with(comp, args["time"].as_f64(), max_side, transparent).map_err(|e| format!("effect.render: {e}"))?;
    area.write(&out, &frame.png).map_err(|e| format!("effect.render: {e}"))?;
    Ok(json!({
        "out": out_rel, "bytes": frame.png.len(),
        "width": frame.width, "height": frame.height,
        "comp": frame.comp, "time": frame.time,
    }))
}

/// Engine command ids `run` refuses. Plug-in loading changes a
/// process-wide registry from caller-named input and runs WebAssembly
/// under the engine's own budgets, outside the shell's `wasm` service
/// (ADR 0011) — the same boundary `photo.run` draws against photocraft's
/// `plugin.*`. Listing installed plug-ins stays readable. The media
/// browser, watch folders, Collect Files and logging reach the file system
/// directly, outside the session's gated services, so they are refused too.
/// This list is not a fence on its own: the engine's command wrappers run
/// other commands past it, which is why the shell holds `run` back
/// entirely ([`serve`]).
fn callable(id: &str) -> Result<(), String> {
    let ambient = id.starts_with("mediaBrowser.")
        || id.starts_with("file.watchFolder")
        || id == "file.collectFiles"
        || id.starts_with("help.enableLogging");
    if (id.starts_with("effect.plugins.") && id != "effect.plugins.list") || ambient {
        return Err(format!("effect.run: `{id}` is not available through the effect service"));
    }
    Ok(())
}

fn run(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let cmds = args["cmds"].as_array().ok_or("effect.run: `cmds` is a list of {id, params?}")?;
    if cmds.len() > MAX_CMDS {
        return Err(format!("effect.run: at most {MAX_CMDS} commands per call"));
    }
    let out_abs = match args["out"].as_str() {
        Some(out) if !out.is_empty() => Some(out_path(area, out)?),
        _ => None,
    };
    let mut b = session(area)?;
    // Without `path`, commands build on a fresh empty project (comp.new …).
    if args["path"].as_str().is_some_and(|p| !p.is_empty()) {
        open(&mut b, area, args, "run")?;
    }
    let mut results = Vec::new();
    for c in cmds {
        let id = c["id"].as_str().ok_or("effect.run: each command has an `id`")?;
        callable(id)?;
        let params = if c["params"].is_null() { json!({}) } else { c["params"].clone() };
        let r = b.exec(id, params).map_err(|e| format!("effect.run {id}: {e}"))?;
        // Before a later command could draw it.
        fence_effect_files(&mut b, "run")?;
        results.push(json!({"id": id, "result": r}));
    }
    let saved = match (args["out"].as_str(), out_abs) {
        (Some(out), Some(abs)) => {
            b.exec("file.saveAs", json!({"path": abs.to_string_lossy()})).map_err(|e| format!("effect.run: {e}"))?;
            json!(out)
        }
        _ => Json::Null,
    };
    Ok(json!({"results": results, "out": saved}))
}

fn commands(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let mut b = session(area)?;
    b.exec("command.list", json!({"filter": args["filter"]})).map_err(|e| format!("effect.commands: {e}"))
}

fn export_lottie(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    let out = out_path(area, out_rel)?;
    let mut b = session(area)?;
    open(&mut b, area, args, "export_lottie")?;
    let include = args["include_expressions"].as_bool().unwrap_or(false);
    let r = b
        .exec(
            "file.exportLottie",
            json!({"comp": args["comp"], "path": out.to_string_lossy(), "includeExpressions": include}),
        )
        .map_err(|e| format!("effect.export_lottie: {e}"))?;
    Ok(json!({"out": out_rel, "bytes": r["bytes"], "warnings": r["warnings"]}))
}

fn import_lottie(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let rel = args["path"].as_str().unwrap_or("");
    let lottie = contained(area, "path", rel)?;
    let out_rel = args["out"].as_str().unwrap_or("");
    let out = out_path(area, out_rel)?;
    let mut b = session(area)?;
    let r = b
        .exec("file.importLottie", json!({"path": lottie.to_string_lossy()}))
        .map_err(|e| format!("effect.import_lottie: {e}"))?;
    b.exec("file.saveAs", json!({"path": out.to_string_lossy()})).map_err(|e| format!("effect.import_lottie: {e}"))?;
    Ok(json!({"out": out_rel, "comp": r["comp"], "items": r["items"], "warnings": r["warnings"]}))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a real project in the area — a 64x36 24 fps comp with an
    /// opaque solid — through the engine's own commands, saved as
    /// `main.ecproj`.
    fn fixture(host: &Path) -> Json {
        dispatch(
            "run",
            &json!({"cmds": [
                {"id": "comp.new", "params": {"name": "Main", "width": 64, "height": 36, "frameRate": 24, "duration": 1.0}},
                {"id": "layer.newSolid", "params": {"name": "Red", "color": "#cc3344"}},
            ], "out": "main.ecproj"}),
            host,
        )
        .unwrap()
    }

    #[test]
    fn the_area_is_the_familys_subdir() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let a = Slot::new().area(&service_call("info", json!({}), host, false), "effect").unwrap();
        assert_eq!(a.root, host.join("effect"));
        assert!(a.root.is_dir() && a.may_replace && a.quota_left.is_none(), "without a resolver, as before");
        fixture(host);
        fixture(host);
        assert!(host.join("effect/main.ecproj").is_file(), "files land inside the area, replacing as before");
        assert!(!host.join("main.ecproj").exists(), "and not beside Mail's and Calendar's data");
    }

    /// A call as App Hub hands it to the service.
    fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("effect.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
    }

    /// A resolver shaped like the shell's: every call works in `root`, an
    /// app's own foreground call may replace a file and an agent's may not,
    /// within `quota`.
    fn resolver(root: &Path, quota: Option<u64>) -> Slot {
        let slot = Slot::new();
        let root = root.to_path_buf();
        slot.set(Some(Arc::new(move |call: &ServiceCall| Ok(Area::new(&root, quota, call.may_prompt)))));
        slot
    }

    /// The command door in the area `areas` gives the call, past
    /// [`serve`]'s hold on it: how these tests build their fixtures, and
    /// check the door's own write rules.
    fn run_in(areas: &Slot, args: Json, host_dir: &Path, may_prompt: bool) -> Result<Json, String> {
        let area = areas.area(&service_call("run", args.clone(), host_dir, may_prompt), "effect")?;
        dispatch_in("run", &args, &Arc::new(area))
    }

    /// The fixture project, built straight into a caller's folder `root`.
    fn fixture_in(areas: &Slot, root: &Path) -> Json {
        run_in(
            areas,
            json!({"cmds": [
                {"id": "comp.new", "params": {"name": "Main", "width": 64, "height": 36, "frameRate": 24, "duration": 1.0}},
                {"id": "layer.newSolid", "params": {"name": "Red", "color": "#cc3344"}},
            ], "out": "main.ecproj"}),
            root,
            true,
        )
        .unwrap()
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
        fixture_in(&areas, &host);
        let sum = serve(&areas, &service_call("info", json!({"path": "main.ecproj"}), &host, false)).unwrap();
        assert_eq!(sum["path"], json!("main.ecproj"), "{sum}");
        serve(&areas, &service_call("render", json!({"path": "main.ecproj", "out": "frames/f.png", "max_side": 16}), &host, false)).unwrap();
        serve(&areas, &service_call("export_lottie", json!({"path": "main.ecproj", "comp": "Main", "out": "main.json"}), &host, false)).unwrap();
        serve(&areas, &service_call("import_lottie", json!({"path": "main.json", "out": "again.ecproj"}), &host, false)).unwrap();
        for made in ["main.ecproj", "frames/f.png", "main.json", "again.ecproj"] {
            assert!(root.join(made).is_file(), "{made}");
        }
        assert!(!host.exists() && !root.join("effect").exists());
        std::fs::write(dir.path().join("beside.ecproj"), b"{}").unwrap();
        for bad in ["../beside.ecproj", "/etc/hosts", "frames/../../beside.ecproj"] {
            assert!(serve(&areas, &service_call("info", json!({"path": bad}), &host, false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("render", json!({"path": "main.ecproj", "out": bad}), &host, true)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            assert!(serve(&areas, &service_call("info", json!({"path": "up/beside.ecproj"}), &host, false)).is_err());
            assert!(serve(&areas, &service_call("render", json!({"path": "main.ecproj", "out": "up/f.png"}), &host, true)).is_err());
            assert!(!dir.path().join("f.png").exists());
        }
    }

    /// An agent's call never replaces a file — the engine's own saves
    /// included — before the engine works; an app's own foreground call may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        fixture_in(&areas, dir.path());
        for name in ["taken.png", "taken.json", "taken.ecproj"] {
            std::fs::write(dir.path().join(name), b"keep me").unwrap();
        }
        for (method, args) in [
            ("render", json!({"path": "main.ecproj", "out": "taken.png"})),
            ("export_lottie", json!({"path": "main.ecproj", "comp": "Main", "out": "taken.json"})),
        ] {
            let refused = serve(&areas, &service_call(method, args, dir.path(), false)).unwrap_err();
            assert!(refused.contains("already exists"), "{method}: {refused}");
        }
        for out in ["taken.ecproj", "main.ecproj"] {
            let refused = run_in(&areas, json!({"path": "main.ecproj", "cmds": [], "out": out}), dir.path(), false).unwrap_err();
            assert!(refused.contains("already exists"), "run: {refused}");
        }
        for name in ["taken.png", "taken.json", "taken.ecproj"] {
            assert_eq!(std::fs::read(dir.path().join(name)).unwrap(), b"keep me", "{name}");
        }
        // Not even a command that saves on its own may replace one.
        let saves = run_in(&areas, json!({"path": "main.ecproj", "cmds": [{"id": "file.saveAs", "params": {"path": dir.path().join("taken.ecproj").to_string_lossy()}}]}), dir.path(), false);
        assert!(saves.is_err(), "{saves:?}");
        assert_eq!(std::fs::read(dir.path().join("taken.ecproj")).unwrap(), b"keep me");
        serve(&areas, &service_call("render", json!({"path": "main.ecproj", "out": "taken.png", "max_side": 8}), dir.path(), true)).unwrap();
        assert!(std::fs::read(dir.path().join("taken.png")).unwrap().starts_with(&[0x89, b'P', b'N', b'G']));
    }

    /// What a call writes, the engine's own saves included, must fit what
    /// is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        fixture_in(&resolver(dir.path(), None), dir.path());
        let tight = resolver(dir.path(), Some(8));
        for (method, args) in [
            ("render", json!({"path": "main.ecproj", "out": "f.png"})),
            ("export_lottie", json!({"path": "main.ecproj", "comp": "Main", "out": "m.json"})),
        ] {
            let refused = serve(&tight, &service_call(method, args, dir.path(), true)).unwrap_err();
            assert!(refused.contains("bytes left"), "{method}: {refused}");
        }
        let refused = run_in(&tight, json!({"path": "main.ecproj", "cmds": [], "out": "copy.ecproj"}), dir.path(), true).unwrap_err();
        assert!(refused.contains("bytes left"), "run: {refused}");
        assert!(!dir.path().join("f.png").exists() && !dir.path().join("copy.ecproj").exists() && !dir.path().join("m.json").exists());
    }

    /// Footage outside the folder is never decoded: an image sequence
    /// whose frames lead outside it (through a link on import, or listed by
    /// a project) decodes nothing, while the same sequence inside decodes;
    /// a 3D model (whose sibling files the engine would read unguarded) is
    /// refused and renders nothing.
    #[test]
    fn footage_outside_the_folder_is_never_read() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let rules = Arc::new(Area::new(&root, None, true));
        let guard = Arc::new(Guard::new(&rules).unwrap());
        let png = |rgb: [u8; 3]| {
            let mut out = std::io::Cursor::new(Vec::new());
            image::RgbaImage::from_pixel(4, 4, image::Rgba([rgb[0], rgb[1], rgb[2], 255])).write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        };
        std::fs::write(root.join("shot_0001.png"), png([0, 200, 0])).unwrap();
        std::fs::write(root.join("shot_0002.png"), png([0, 0, 200])).unwrap();
        let secret = dir.path().join("secret.png");
        std::fs::write(&secret, png([200, 0, 0])).unwrap();
        let importer = ContainedImporter { guard: guard.clone() };
        let source = ContainedFootage { pool: effectcraft_media::MediaPool::new(), guard: guard.clone() };
        // Inside the folder: a two-frame sequence that decodes.
        let sequence = importer.probe(&root.join("shot_0001.png").to_string_lossy()).unwrap();
        assert_eq!(sequence.sequence.len(), 2, "{sequence:?}");
        assert!(source.frame(ItemId(1), &sequence, Tick::ZERO).is_some(), "a sequence inside the folder decodes");
        // A project listing a frame outside: nothing of it decodes.
        let mut listed = sequence.clone();
        listed.sequence[1] = secret.to_string_lossy().into();
        for t in [Tick::ZERO, Tick::from_seconds_f64(1.0 / 30.0)] {
            assert!(source.frame(ItemId(2), &listed, t).is_none(), "a frame outside is never decoded");
        }
        #[cfg(unix)]
        {
            // A sibling that is a link out of the folder: refused on import.
            std::fs::remove_file(root.join("shot_0002.png")).unwrap();
            std::os::unix::fs::symlink(&secret, root.join("shot_0002.png")).unwrap();
            let refused = importer.probe(&root.join("shot_0001.png").to_string_lossy()).unwrap_err();
            assert!(refused.contains("outside"), "{refused}");
        }
        // Models: refused on import, and never loaded.
        let gltf = root.join("scene.gltf");
        std::fs::write(&gltf, br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"../secret.png","byteLength":4}]}"#).unwrap();
        assert!(importer.probe(&gltf.to_string_lossy()).unwrap_err().contains("3D models"));
        let model = Footage { path: gltf.to_string_lossy().into(), kind: effectcraft_engine::project::FootageKind::Model, ..Default::default() };
        assert!(source.model(ItemId(3), &model).is_none());
    }

    /// With the shell's resolver installed the command door is held for its
    /// own review: its list of refused ids is no fence, since the engine's
    /// wrappers run other commands past it (`engine.batch` here runs the
    /// refused media browser). Without a resolver (this crate's tests) it
    /// still runs.
    #[test]
    fn the_command_door_is_held_in_the_shell() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let held = serve(&areas, &service_call("run", json!({"cmds": [{"id": "comp.new"}]}), dir.path(), true)).unwrap_err();
        assert!(held.contains("held for its own review"), "{held}");
        assert!(callable("mediaBrowser.list").is_err());
        assert!(callable("engine.batch").is_ok(), "a wrapper's own id passes the list, which is why the door is held");
        let legacy = Slot::new();
        serve(&legacy, &service_call("run", json!({"cmds": [{"id": "comp.new"}]}), dir.path(), true)).unwrap();
        assert!(serve(&areas, &service_call("commands", json!({}), dir.path(), true)).is_ok(), "the catalog stays readable");
    }

    /// A 2x2x2 `.cube` LUT that turns every colour pure green.
    const GREEN_LUT: &str = "LUT_3D_SIZE 2\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n";

    /// Effect parameters that the engine reads as a file by their own path,
    /// with `std::fs` past every gate, refuse the project. The hostile
    /// fixture is a real project whose Apply Color LUT names a `.cube`
    /// outside the caller's folder: the engine's own session reads it (its
    /// render turns green), the service refuses it for every method that
    /// opens it. The LUT's text inline is drawn; a command door call that
    /// sets a path, or an expression that could make one, is refused before
    /// a later command could draw it; so are OCIO configs and mocha shape
    /// files.
    #[test]
    fn effect_parameters_that_name_files_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let secret = dir.path().join("secret.cube");
        std::fs::write(&secret, GREEN_LUT).unwrap();
        let areas = resolver(&root, None);
        let solid = || vec![
            json!({"id": "comp.new", "params": {"name": "Main", "width": 16, "height": 16, "frameRate": 24, "duration": 1.0}}),
            json!({"id": "layer.newSolid", "params": {"name": "Red", "color": "#cc3344"}}),
        ];
        let with = |extra: Vec<Json>| solid().into_iter().chain(extra).collect::<Vec<Json>>();
        let lut = |value: &str| vec![
            json!({"id": "effect.apply", "params": {"effect": "ec.utility.applylut"}}),
            json!({"id": "prop.set", "params": {"path": "effects/#1/lut", "value": value}}),
        ];
        run_in(&areas, json!({"cmds": solid(), "out": "plain.ecproj"}), &root, true).unwrap();
        run_in(&areas, json!({"cmds": with(lut(GREEN_LUT)), "out": "inline.ecproj"}), &root, true).unwrap();
        // The LUT inline is drawn through the service.
        serve(&areas, &service_call("render", json!({"path": "inline.ecproj", "out": "inline.png", "max_side": 16}), &root, false)).unwrap();
        // The same project, its LUT naming the file outside instead.
        let text = std::fs::read_to_string(root.join("inline.ecproj")).unwrap();
        let quoted = |v: &str| serde_json::to_string(v).unwrap();
        let hostile = text.replace(&quoted(GREEN_LUT), &quoted(&secret.to_string_lossy()));
        assert_ne!(hostile, text, "the fixture names the outside file");
        std::fs::write(root.join("hostile.ecproj"), hostile).unwrap();
        // The engine's own session reads it: its render differs from the
        // plain solid's.
        let engine_render = |name: &str| {
            let mut b = Backend::headless(Session::default());
            b.exec("file.open", json!({"path": root.join(name).to_string_lossy()})).unwrap();
            b.render_with(None, Some(0.0), 16, false).unwrap().png
        };
        assert_ne!(engine_render("hostile.ecproj"), engine_render("plain.ecproj"), "the fixture is live: the engine reads the outside LUT");
        for (method, args) in [
            ("info", json!({"path": "hostile.ecproj"})),
            ("render", json!({"path": "hostile.ecproj", "out": "hostile.png", "max_side": 16})),
            ("export_lottie", json!({"path": "hostile.ecproj", "comp": "Main", "out": "hostile.json"})),
        ] {
            let refused = serve(&areas, &service_call(method, args, &root, false)).unwrap_err();
            assert!(refused.contains("names a file") && refused.contains("`lut`"), "{method}: {refused}");
        }
        assert!(!root.join("hostile.png").exists() && !root.join("hostile.json").exists());
        // The command door refuses a path, or an expression, as it is set.
        let refused = run_in(&areas, json!({"cmds": with(lut(&secret.to_string_lossy()))}), &root, true).unwrap_err();
        assert!(refused.contains("names a file"), "{refused}");
        let expression = with(vec![
            json!({"id": "effect.apply", "params": {"effect": "ec.utility.applylut"}}),
            json!({"id": "prop.setExpression", "params": {"path": "effects/#1/lut", "expression": "'/etc/x.cube'"}}),
        ]);
        let refused = run_in(&areas, json!({"cmds": expression}), &root, true).unwrap_err();
        assert!(refused.contains("has an expression"), "{refused}");
        for (effect, key, value) in [
            ("ec.color.ociocolorspace", "configFile", "studio.ocio"),
            ("ec.color.ociofile", "file", "/etc/look.cube"),
            ("ec.obsolete.mochashape", "shapeData", "shapes.json"),
            ("ec.color.lumetri", "basicCorrection/inputLutFile", "/etc/in.cube"),
        ] {
            let cmds = with(vec![
                json!({"id": "effect.apply", "params": {"effect": effect}}),
                json!({"id": "prop.set", "params": {"path": format!("effects/#1/{key}"), "value": value}}),
            ]);
            let refused = run_in(&areas, json!({"cmds": cmds}), &root, true).unwrap_err();
            assert!(refused.contains("names a file") && refused.contains(key), "{effect}: {refused}");
        }
    }

    /// `effect.run` refuses the commands that reach the file system outside
    /// the session's gated services.
    #[test]
    fn run_refuses_ambient_file_commands() {
        let dir = tempfile::tempdir().unwrap();
        for id in ["mediaBrowser.list", "mediaBrowser.go", "mediaBrowser.fileInfo", "file.watchFolder", "file.watchFolder.poll", "file.collectFiles", "help.enableLogging"] {
            let e = dispatch("run", &json!({"cmds": [{"id": id, "params": {"path": "/"}}]}), dir.path()).unwrap_err();
            assert!(e.contains("not available"), "{id}: {e}");
        }
    }

    /// Plug-in loading mutates a process-wide registry with WebAssembly:
    /// `run` refuses every `effect.plugins.*` mutator before the engine
    /// sees it; only the read-only list stays.
    #[test]
    fn run_refuses_plugin_mutators() {
        let dir = tempfile::tempdir().unwrap();
        let area = Arc::new(Area::new(dir.path(), None, true));
        for id in ["effect.plugins.load", "effect.plugins.unload", "effect.plugins.reload"] {
            let r = run(&json!({"cmds": [{"id": id, "params": {"path": "x.wasm"}}]}), &area);
            let e = r.unwrap_err();
            assert!(e.contains("not available"), "{id}: {e}");
        }
        assert!(callable("effect.plugins.list").is_ok());
        assert!(callable("comp.new").is_ok());
    }

    #[test]
    fn run_builds_a_project_with_engine_commands() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let ran = fixture(host);
        let results = ran["results"].as_array().unwrap();
        assert_eq!(results.len(), 2, "{ran}");
        assert_eq!(results[0]["id"], json!("comp.new"));
        assert_eq!(ran["out"], json!("main.ecproj"));
        assert!(host.join("effect/main.ecproj").metadata().unwrap().len() > 0);
    }

    #[test]
    fn info_reads_the_composition_back() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        fixture(host);
        let sum = dispatch("info", &json!({"path": "main.ecproj"}), host).unwrap();
        assert_eq!(sum["path"], json!("main.ecproj"), "{sum}");
        let items = sum["items"].as_array().unwrap();
        let comp = items.iter().find(|i| i["name"] == json!("Main")).unwrap();
        assert_eq!(comp["size"], json!([64, 36]), "{comp}");
        assert_eq!(comp["frameRate"], json!(24.0));
        assert_eq!(comp["layers"], json!(1));
    }

    #[test]
    fn render_writes_the_comps_frame_as_png() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        fixture(host);
        let r = dispatch("render", &json!({"path": "main.ecproj", "out": "frames/first.png", "time": 0.0, "max_side": 32}), host)
            .unwrap();
        assert_eq!(r["out"], json!("frames/first.png"), "{r}");
        assert!(r["bytes"].as_u64().unwrap() > 0);
        assert_eq!((r["width"].as_u64().unwrap(), r["height"].as_u64().unwrap()), (32, 18));
        let png = std::fs::read(host.join("effect/frames/first.png")).unwrap();
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!((img.width(), img.height()), (32, 18));
        let p = img.get_pixel(16, 9);
        let want = [0xcc_i32, 0x33, 0x44];
        for (got, want) in p.0.iter().take(3).zip(want) {
            assert!((*got as i32 - want).abs() <= 4, "the solid's colour comes back ({p:?})");
        }
    }

    #[test]
    fn commands_lists_the_engine_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let cat = dispatch("commands", &json!({}), host).unwrap();
        let list = cat.as_array().unwrap();
        assert!(list.len() > 300, "a real catalog, not a stub ({})", list.len());
        assert!(list.iter().any(|c| c["id"] == json!("comp.new")));
        let filtered = dispatch("commands", &json!({"filter": "lottie"}), host).unwrap();
        assert!(filtered.as_array().unwrap().iter().any(|c| c["id"] == json!("file.exportLottie")));
    }

    #[test]
    fn export_lottie_writes_the_comp_as_lottie_json() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        fixture(host);
        let r = dispatch("export_lottie", &json!({"path": "main.ecproj", "comp": "Main", "out": "main.json"}), host).unwrap();
        assert_eq!(r["out"], json!("main.json"), "{r}");
        assert!(r["bytes"].as_u64().unwrap() > 0);
        assert!(r["warnings"].is_array());
        let lottie: Json = serde_json::from_slice(&std::fs::read(host.join("effect/main.json")).unwrap()).unwrap();
        assert!(lottie["layers"].is_array(), "{lottie}");
    }

    #[test]
    fn import_lottie_opens_it_as_a_project() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        fixture(host);
        dispatch("export_lottie", &json!({"path": "main.ecproj", "comp": "Main", "out": "main.json"}), host).unwrap();
        let r = dispatch("import_lottie", &json!({"path": "main.json", "out": "roundtrip.ecproj"}), host).unwrap();
        assert!(r["comp"].is_number(), "{r}");
        assert!(host.join("effect/roundtrip.ecproj").metadata().unwrap().len() > 0);
        let sum = dispatch("info", &json!({"path": "roundtrip.ecproj"}), host).unwrap();
        let items = sum["items"].as_array().unwrap();
        assert!(items.iter().any(|i| i["size"] == json!([64, 36])), "the comp survives the roundtrip: {sum}");
    }

    #[test]
    fn paths_stay_inside_the_effect_area() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        for bad in ["../up.ecproj", "/etc/x.ecproj", "a/../../up.ecproj", ""] {
            assert!(dispatch("info", &json!({"path": bad}), host).is_err(), "{bad}");
            assert!(dispatch("render", &json!({"path": "main.ecproj", "out": bad}), host).is_err(), "{bad}");
            assert!(dispatch("import_lottie", &json!({"path": bad, "out": "a.ecproj"}), host).is_err(), "{bad}");
        }
        // The engine side is gated too: commands that name a path outside
        // the area are refused by the session's guarded file services.
        let escape = dispatch(
            "run",
            &json!({"cmds": [
                {"id": "comp.new", "params": {"name": "C"}},
                {"id": "file.saveAs", "params": {"path": "/tmp/effect-escape.ecproj"}},
            ]}),
            host,
        );
        assert!(escape.is_err(), "{escape:?}");
        assert!(!Path::new("/tmp/effect-escape.ecproj").exists());
        let read = dispatch("run", &json!({"cmds": [{"id": "file.open", "params": {"path": "/etc/hosts"}}]}), host);
        assert!(read.is_err(), "{read:?}");
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.photos"));
        assert!(may_call("os.app-studio"));
        assert!(!may_call("org.example.app"));
        assert!(!may_call(""));
    }

    /// The agent tools (`tools.json`) pass App Hub's own loader, as the shell
    /// reads them, keep the object schemas octos takes both ways, and name
    /// only methods this service dispatches.
    #[test]
    fn the_agent_tools_pass_app_hubs_loader_and_name_real_methods() {
        use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest};
        let (manifest, _) = ToolManifest::load(TOOLS_JSON, "effect", ToolHost::Contained, false).unwrap();
        assert!(!manifest.tools.is_empty());
        let dir = tempfile::tempdir().unwrap();
        for tool in &manifest.tools {
            assert_eq!(tool.implemented_by, ImplementedBy::HostService, "{}", tool.name);
            assert!(tool.host_method.is_none(), "{}: the shell routes each tool to the method of its own name", tool.name);
            for schema in [&tool.input_schema, &tool.output_schema] {
                assert_eq!(schema["type"], json!("object"), "{}: octos takes object schemas only", tool.name);
            }
            assert!(tool.description.is_ascii(), "{}: descriptions stay within octos's byte limit", tool.name);
            let method = tool.name.strip_prefix("effect.").unwrap();
            if let Err(e) = dispatch(method, &json!({}), dir.path()) {
                assert!(!e.contains("is not a method"), "{}: {e}", tool.name);
            }
        }
    }
}
