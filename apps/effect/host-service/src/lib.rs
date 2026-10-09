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
//! - `run {path?, cmds: [{id, params?}], out?, comp?, time?, max_side?,
//!   transparent?, include_expressions?}` → `{results, out, format, …}` —
//!   the command door (ADR 0013, #418): run commands of effectcraft's
//!   registry on the project at `path` (an `.ecproj`, or a Lottie `.json` /
//!   `.lottie` opened as a new composition, as `import_lottie` does, with
//!   what it made under `imported`), or on a new empty project, then write
//!   `out` by its extension: `.ecproj` the project; `.json` / `.lottie` a
//!   composition as Lottie (`comp`, `include_expressions`; with its `bytes`
//!   and the engine's `warnings`), as `export_lottie` does; `.png` one frame
//!   (`comp`, `time`, `max_side`, `transparent`; with `bytes`, `width`,
//!   `height`), as `render` does. Only what the door's allowlist admits runs
//!   ([`door`]), and every command is admitted before any runs: one refused
//!   command refuses the call, with nothing written.
//! - `commands {filter?}` → the engine's catalog entries the door runs
//! - `render {path, comp?, time?, out, max_side?, transparent?}` → one comp
//!   frame written to `out` as PNG
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
//! they hold the file's text inline, and so does an Essential Graphics value
//! that sets one inside a precomp, and an effect plug-in
//! ([`fence_effect_files`]). What the engine writes keeps the area's rules
//! ([`Area::write`]: a write that may not replace, an agent's, only creates
//! new files, within the quota).
//!
//! **The command door** `run` runs only the commands the reviewed
//! classification (`skill/safety.json`) classes `safe`: never a `file`,
//! `code`, `network`, `device` or `host` command, nor an id the
//! classification does not know, so the engine's command wrappers
//! (`engine.batch`, `file.runScript`, `learn.step`), its scripts, plug-in
//! loading and preferences stay out whatever list they arrive in; and
//! `effect.apply` only of an effect the engine builds in. What a command
//! plants in the project is fenced after every command, before a later one
//! or the write could draw it. The service serves system apps only until
//! ADR 0013's store capability is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};

use effectcraft_automation::Backend;
use effectcraft_engine::effects::EffectSpec;
use effectcraft_engine::project::{Footage, ItemId};
use effectcraft_engine::raster::{AuxChannels, Image};
use effectcraft_engine::render::FootageSource;
use effectcraft_engine::time::Tick;
use effectcraft_engine::{Importer, Services, Session};
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::door::{Door, Inner, InnerRule, Reviewed};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};

/// The longest frame edge `render` produces.
const MAX_RENDER_SIDE: u32 = 4096;
/// The largest file the engine's guarded I/O reads or writes (bytes):
/// projects and Lottie files are JSON, and frames are written by `render`.
const MAX_FILE_BYTES: u64 = 64 << 20;

/// What the effect engine's reviewer settled for the door beyond the
/// classes. `effect.apply` names its effect by id, display name or alias,
/// which the engine resolves with `lookup` over the built-in effects and
/// every registered plug-in: it must name a built-in no plug-in shadows
/// ([`builtin_effect`]). It is the only `safe` command that names an effect,
/// preset or command by a caller's string and could reach past the
/// built-ins: `effect.applyLast` re-applies the id `effect.apply` recorded;
/// ease presets (`keys.easePreset.apply`), text animation presets
/// (`layer.applyTextPreset`) and brush presets (`paint.brushPreset`) are
/// built-in data (and, for ease presets, the session's own list, empty
/// without a config store); the VR builders run fixed command ids. No `file`
/// command is reviewed to run: the door's files are its own `path` and
/// `out`. No `safe` command sets an app-wide variable by key (`prefs.*` are
/// `host`; `layer.setText`'s `font` only notes a recent font, which a
/// session without a config store never keeps).
static REVIEWED: Reviewed = Reviewed {
    file_reads: &[],
    setters: &[],
    inner: &[Inner { id: "effect.apply", param: "effect", rule: InnerRule::Effect { builtin: builtin_effect } }],
};

/// The command door's gate: effectcraft's reviewed classification
/// (`skill/safety.json`, generated and drift-checked by `tests/skill.rs`)
/// and [`REVIEWED`].
pub fn door() -> Result<&'static Door, String> {
    static DOOR: OnceLock<Result<Door, String>> = OnceLock::new();
    DOOR.get_or_init(|| Door::new("effect", include_str!("../skill/safety.json"), &REVIEWED)).as_ref().map_err(Clone::clone)
}

/// Whether `spec` is one of the effects the engine builds in, not a
/// registered plug-in.
fn is_builtin_spec(spec: &EffectSpec) -> bool {
    effectcraft_engine::effects::registry().iter().any(|s| std::ptr::eq(s, spec))
}

/// Whether `name` (an id, a display name in any case, or an alias, as
/// `effect.apply` takes it) names an effect the engine builds in: it
/// resolves among the built-ins alone, the way the engine's `lookup` does,
/// and `lookup` itself — which also sees every registered plug-in, by id
/// ahead of any built-in's display name, and by display name in its sorted
/// list — lands on that same built-in, so no plug-in shadows it.
fn builtin_effect(name: &str) -> bool {
    use effectcraft_engine::effects as fx;
    let builtins = fx::registry();
    let by_id = |id: &str| builtins.iter().find(|s| s.id == id);
    let own = by_id(name)
        .or_else(|| builtins.iter().find(|s| s.name.eq_ignore_ascii_case(name)))
        .or_else(|| builtins.iter().find(|s| fx::aliases(s.id).iter().any(|a| a.eq_ignore_ascii_case(name))))
        .or_else(|| fx::migrate::EFFECT_NAME_ALIASES.iter().find(|(old, _)| old.eq_ignore_ascii_case(name)).and_then(|(_, id)| by_id(id)));
    match (own, fx::lookup(name)) {
        (Some(own), Some(found)) => std::ptr::eq(own, found),
        _ => false,
    }
}

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

/// One call, in the area `areas` gives it.
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The effect service serves system apps only.".into());
    }
    let area = Arc::new(areas.area(call, "effect").map_err(|e| format!("effect: {e}"))?);
    dispatch_in(call.method(), &call.args, &area)
        .map(|answer| area.relative_json(answer))
        .map_err(|error| area.relative_text(&error))
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
        // `missing/../..` resolves inside the area only until the write
        // creates `missing`: no write path climbs.
        if Path::new(path).components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "the path is outside the call's effect area"));
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
/// start one), no scripting, no plug-in loader, no config store (nothing a
/// command sets in the app's settings persists), no models folder, no
/// media browser of its own.
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

/// Open the Lottie file at `lottie` (already contained in the area) as a
/// new composition of the session's project, made the active one: the
/// engine reads it through the [`Guard`] (inside the area, size-capped) and
/// writes its embedded images beside it under the area's rules. Its effects
/// are fenced like an opened project's.
fn import(b: &mut Backend, lottie: &Path, method: &str) -> Result<Json, String> {
    let r = b.exec("file.importLottie", json!({"path": lottie.to_string_lossy()})).map_err(|e| format!("effect.{method}: {e}"))?;
    fence_effect_files(b, method)?;
    Ok(json!({"comp": r["comp"], "items": r["items"], "warnings": r["warnings"]}))
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

/// Refuse a project the engine would draw by reading a file by its own
/// path, or with an effect plug-in:
///
/// - a LUT, OCIO file or config, or mocha shape parameter holding a path
///   (in its value or any keyframe), or driven by an expression, which
///   could produce one when the frame renders;
/// - an Essential Properties value of a precomp layer whose control sets
///   such a parameter inside the precomp — directly, through a mirror or a
///   link, or through the Essential Properties of precomps nested deeper:
///   the renderer puts the instance's value into the parameter
///   (`essential::with_overrides`), so it is fenced as the parameter is;
/// - an effect instance whose id resolves to a registered plug-in rather
///   than a built-in effect (the renderer finds effects by id among both).
///
/// The engine reads those files with `std::fs`, outside the session's gated
/// services, so no area check could stop the read; the file's text inline
/// is drawn as before. It walks the session's project as it is now, so
/// `run` calls it after every command and before it writes.
fn fence_effect_files(b: &mut Backend, method: &str) -> Result<(), String> {
    use effectcraft_engine::project::{essential, GroupKind, ItemKind, LayerId, LayerSource, Node, PropGroup, Property, Uid, Value};

    /// Why the engine would read `p` as a file by its own path, if it would.
    fn reads_a_file(p: &Property, kind: FileParam) -> Option<&'static str> {
        if p.has_expression() {
            return Some("has an expression");
        }
        let named = std::iter::once(&p.value).chain(p.keys.iter().map(|k| &k.value)).any(|v| matches!(v, Value::Str(text) if !kind.inline(text)));
        named.then_some("names a file")
    }
    /// The file parameters of one effect instance: (match path, property, how it is read).
    fn file_params<'a>(g: &'a PropGroup, prefix: &str, out: &mut Vec<(String, &'a Property, FileParam)>) {
        for node in &g.children {
            match node {
                Node::Group(sub) => file_params(sub, &format!("{prefix}{}/", sub.match_id), out),
                Node::Prop(p) => {
                    let key = format!("{prefix}{}", p.match_id);
                    if let Some(kind) = FileParam::of(&key).or_else(|| FileParam::of(&p.match_id)) {
                        out.push((key, p, kind));
                    }
                }
            }
        }
    }
    /// An Essential Properties value of a precomp layer, and the properties
    /// inside the precomp its control sets when the layer renders.
    struct Instance<'a> {
        at: (ItemId, LayerId, Uid),
        layer: &'a str,
        prop: &'a Property,
        inner: ItemId,
        targets: Vec<(LayerId, Uid)>,
    }
    const READS: &str = "the engine would read a file by its own path, outside this call's folder; put the file's text in the parameter instead";

    let Some(session) = b.session() else { return Ok(()) };
    let project = &session.project;
    let comps = || {
        project.items.iter().filter_map(|(id, item)| match &item.kind {
            ItemKind::Comp(comp) => Some((*id, comp.as_ref())),
            _ => None,
        })
    };
    // Every property whose value reaches a file loader when a frame renders,
    // with what it feeds: (how the loader reads it, its key, the effect).
    let mut feeds: BTreeMap<(ItemId, LayerId, Uid), (FileParam, String, String)> = BTreeMap::new();
    for (cid, comp) in comps() {
        for layer in &comp.layers {
            let Some(effects) = layer.effects() else { continue };
            for effect in effects.groups() {
                if let GroupKind::Effect { effect: id } = &effect.kind {
                    if effectcraft_engine::effects::find(id).is_some_and(|spec| !is_builtin_spec(spec)) {
                        return Err(format!(
                            "effect.{method}: the effect `{id}` on layer `{}` is an effect plug-in, and no plug-in runs through the effect service",
                            layer.name
                        ));
                    }
                }
                let mut params = vec![];
                file_params(effect, "", &mut params);
                for (key, p, kind) in params {
                    if let Some(why) = reads_a_file(p, kind) {
                        return Err(format!("effect.{method}: the {} effect on layer `{}` {why} for `{key}`: {READS}", effect.match_id, layer.name));
                    }
                    feeds.insert((cid, layer.id, p.uid), (kind, key, effect.match_id.clone()));
                }
            }
        }
    }
    let mut instances = vec![];
    for (cid, comp) in comps() {
        for layer in &comp.layers {
            let LayerSource::Comp { item: inner } = layer.source else { continue };
            let Some(group) = essential::group(layer) else { continue };
            let Some(eg) = project.comp(inner).and_then(|c| c.essential.as_ref()) else { continue };
            group.walk("", &mut |_, p| {
                if let Some(control) = essential::control_of(&p.match_id).and_then(|c| eg.resolve(c)) {
                    instances.push(Instance { at: (cid, layer.id, p.uid), layer: &layer.name, prop: p, inner, targets: control.kind.targets() });
                }
            });
        }
    }
    // An instance value feeds what its control's targets feed; a target may
    // itself be an instance value one precomp further in.
    loop {
        let mut grew = false;
        for i in &instances {
            if feeds.contains_key(&i.at) {
                continue;
            }
            let Some(fed) = i.targets.iter().find_map(|(l, u)| feeds.get(&(i.inner, *l, *u))).cloned() else { continue };
            if let Some(why) = reads_a_file(i.prop, fed.0) {
                return Err(format!(
                    "effect.{method}: the Essential Property `{}` on layer `{}` {why} for `{}` of the {} effect it sets inside its composition: {READS}",
                    i.prop.name, i.layer, fed.1, fed.2
                ));
            }
            feeds.insert(i.at, fed);
            grew = true;
        }
        if !grew {
            break;
        }
    }
    Ok(())
}

/// What a write to `out` makes, by `out`'s extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutKind {
    /// `.ecproj`: the project, as `file.saveAs` writes it.
    Project,
    /// `.json` / `.lottie`: a composition as Lottie, as `export_lottie` writes it.
    Lottie,
    /// `.png`: one frame of a composition, as `render` writes it.
    Frame,
}

impl OutKind {
    fn of(rel: &str) -> Option<OutKind> {
        match extension(rel).as_str() {
            "ecproj" => Some(OutKind::Project),
            "json" | "lottie" => Some(OutKind::Lottie),
            "png" => Some(OutKind::Frame),
            _ => None,
        }
    }
}

/// `rel`'s extension, lowercase.
fn extension(rel: &str) -> String {
    Path::new(rel).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

/// Write the session's project to `out` (contained, and admitted by the
/// area's rules) as `kind`, fenced once more first: the project itself
/// through the [`Guard`], a composition as Lottie (`comp`,
/// `include_expressions`) through the [`Guard`], or one frame (`comp`,
/// `time`, `max_side`, `transparent`) as PNG under the area's rules. Returns
/// the writer's own answer fields.
fn write_out(b: &mut Backend, area: &Area, out: &Path, kind: OutKind, args: &Json, method: &str) -> Result<Json, String> {
    fence_effect_files(b, method)?;
    let comp = args.get("comp").filter(|c| !c.is_null());
    match kind {
        OutKind::Project => {
            let r = b.exec("file.saveAs", json!({"path": out.to_string_lossy()})).map_err(|e| format!("effect.{method}: {e}"))?;
            Ok(json!({"bytes": r["bytes"]}))
        }
        OutKind::Lottie => {
            let mut p = json!({"path": out.to_string_lossy(), "includeExpressions": args["include_expressions"].as_bool().unwrap_or(false)});
            if let Some(comp) = comp {
                p["comp"] = comp.clone();
            }
            let r = b.exec("file.exportLottie", p).map_err(|e| format!("effect.{method}: {e}"))?;
            Ok(json!({"bytes": r["bytes"], "warnings": r["warnings"]}))
        }
        OutKind::Frame => {
            let requested = args["max_side"].as_u64().unwrap_or(1024);
            let max_side = if requested == 0 { MAX_RENDER_SIDE } else { requested.min(MAX_RENDER_SIDE as u64) as u32 };
            let transparent = args["transparent"].as_bool().unwrap_or(false);
            let frame = b.render_with(comp, args["time"].as_f64(), max_side, transparent).map_err(|e| format!("effect.{method}: {e}"))?;
            area.write(out, &frame.png).map_err(|e| format!("effect.{method}: {e}"))?;
            Ok(json!({
                "bytes": frame.png.len(),
                "width": frame.width, "height": frame.height,
                "comp": frame.comp, "time": frame.time,
            }))
        }
    }
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
    let mut b = session(area)?;
    open(&mut b, area, args, "render")?;
    let mut r = write_out(&mut b, area, &out, OutKind::Frame, args, "render")?;
    r["out"] = json!(out_rel);
    Ok(r)
}

/// The command door: every command admitted by [`door`] before the engine
/// runs any (a refused one refuses the whole call, with nothing written),
/// run in order on one session over the project or Lottie file at `path`, or
/// a new empty project, the project fenced after each; then `out` written by
/// its extension under the area's rules ([`write_out`]).
fn run(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let admitted = door()?.admit_all(&args["cmds"], area)?;
    let out = match args["out"].as_str().filter(|o| !o.is_empty()) {
        Some(rel) => {
            let kind = OutKind::of(rel)
                .ok_or_else(|| format!("effect.run: `out` is a project (.ecproj), a Lottie file (.json or .lottie) or a frame (.png), not `{rel}`"))?;
            Some((rel, out_path(area, rel)?, kind))
        }
        None => None,
    };
    let mut b = session(area)?;
    // Without `path`, commands build on a fresh empty project (comp.new …).
    let imported = match args["path"].as_str().filter(|p| !p.is_empty()) {
        Some(rel) if matches!(extension(rel).as_str(), "json" | "lottie") => {
            let lottie = contained(area, "path", rel)?;
            Some(import(&mut b, &lottie, "run")?)
        }
        Some(_) => {
            open(&mut b, area, args, "run")?;
            None
        }
        None => None,
    };
    let mut results = Vec::with_capacity(admitted.len());
    for (id, params) in admitted {
        let r = b.exec(&id, params).map_err(|e| format!("effect.run {id}: {e}"))?;
        // What the command planted, before a later command or the write
        // could draw it.
        fence_effect_files(&mut b, "run")?;
        results.push(json!({"id": id, "result": r}));
    }
    let mut answer = json!({"results": results, "out": Json::Null, "format": Json::Null});
    if let Some(imported) = imported {
        answer["imported"] = imported;
    }
    if let Some((rel, abs, kind)) = out {
        if let Json::Object(fields) = write_out(&mut b, area, &abs, kind, args, "run")? {
            for (key, value) in fields {
                answer[key.as_str()] = value;
            }
        }
        answer["out"] = json!(rel);
        answer["format"] = json!(extension(rel));
    }
    Ok(answer)
}

/// The engine's catalog entries of the commands the door runs.
fn commands(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let door = door()?;
    let mut b = session(area)?;
    let list = b.exec("command.list", json!({"filter": args["filter"]})).map_err(|e| format!("effect.commands: {e}"))?;
    let runs = |c: &&Json| c["id"].as_str().is_some_and(|id| door.runs(id));
    Ok(Json::Array(list.as_array().map(|all| all.iter().filter(runs).cloned().collect()).unwrap_or_default()))
}

fn export_lottie(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    let out = out_path(area, out_rel)?;
    let mut b = session(area)?;
    open(&mut b, area, args, "export_lottie")?;
    let mut r = write_out(&mut b, area, &out, OutKind::Lottie, args, "export_lottie")?;
    r["out"] = json!(out_rel);
    Ok(r)
}

fn import_lottie(args: &Json, area: &Arc<Area>) -> Result<Json, String> {
    let rel = args["path"].as_str().unwrap_or("");
    let lottie = contained(area, "path", rel)?;
    let out_rel = args["out"].as_str().unwrap_or("");
    let out = out_path(area, out_rel)?;
    let mut b = session(area)?;
    let mut r = import(&mut b, &lottie, "import_lottie")?;
    write_out(&mut b, area, &out, OutKind::Project, args, "import_lottie")?;
    r["out"] = json!(out_rel);
    Ok(r)
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

    /// The command door as the shell calls it, in the area `areas` gives
    /// the call.
    fn run_in(areas: &Slot, args: Json, host_dir: &Path, may_prompt: bool) -> Result<Json, String> {
        serve(areas, &service_call("run", args, host_dir, may_prompt))
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

    /// The centre pixel of a PNG.
    fn centre(png: &[u8]) -> [u8; 4] {
        let img = image::load_from_memory(png).unwrap().to_rgba8();
        img.get_pixel(img.width() / 2, img.height() / 2).0
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
        // Nor a command that saves on its own: the door never runs one.
        let saves = run_in(&areas, json!({"path": "main.ecproj", "cmds": [{"id": "file.saveAs", "params": {"path": dir.path().join("taken.ecproj").to_string_lossy()}}]}), dir.path(), false);
        assert!(saves.as_ref().is_err_and(|e| e.contains("not reviewed to run through it")), "{saves:?}");
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

    /// The engine's writes never climb out of the area through a folder the
    /// write itself would create: `missing/../../x` resolves inside until
    /// `missing` exists, so the Guard refuses any `..` in a write path.
    #[test]
    fn the_guard_never_writes_through_a_parent_step() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let guard = Guard::new(&Arc::new(Area::new(&root, None, true))).unwrap();
        let climb = root.join("missing/../../escape.txt");
        assert!(guard.write_file(&climb.to_string_lossy(), b"x").is_err());
        assert!(!dir.path().join("escape.txt").exists(), "nothing lands beside the area");
        guard.write_file(&root.join("made/ok.txt").to_string_lossy(), b"x").unwrap();
        assert_eq!(std::fs::read(root.join("made/ok.txt")).unwrap(), b"x");
    }

    /// A 2x2x2 `.cube` LUT that turns every colour pure green.
    const GREEN_LUT: &str = "LUT_3D_SIZE 2\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n";
    /// The same, pure blue.
    const BLUE_LUT: &str = "LUT_3D_SIZE 2\n0 0 1\n0 0 1\n0 0 1\n0 0 1\n0 0 1\n0 0 1\n0 0 1\n0 0 1\n";

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

    /// The commands that reach the file system outside the session's gated
    /// services (the media browser, watch folders, Collect Files, logging)
    /// are classed `file` or `host`, so the door refuses them.
    #[test]
    fn run_refuses_ambient_file_commands() {
        let dir = tempfile::tempdir().unwrap();
        for id in ["mediaBrowser.list", "mediaBrowser.go", "mediaBrowser.fileInfo", "file.watchFolder", "file.watchFolder.poll", "file.collectFiles", "help.enableLogging"] {
            let e = dispatch("run", &json!({"cmds": [{"id": id, "params": {"path": "/"}}]}), dir.path()).unwrap_err();
            assert!(e.contains("not reviewed to run through it") || e.contains("is classed host"), "{id}: {e}");
        }
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

    /// `effect.commands` lists the catalog entries of exactly the commands
    /// the door runs.
    #[test]
    fn commands_lists_what_the_door_runs() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let cat = dispatch("commands", &json!({}), host).unwrap();
        let list = cat.as_array().unwrap();
        assert_eq!(list.len(), door().unwrap().runnable().len(), "every command the door runs, and only those");
        assert!(list.len() > 300, "a real catalog, not a stub ({})", list.len());
        let has = |list: &[Json], id: &str| list.iter().any(|c| c["id"] == json!(id));
        assert!(has(list, "comp.new") && has(list, "effect.apply") && has(list, "effect.plugins.list"));
        for refused in ["engine.batch", "file.exportLottie", "file.saveAs", "prefs.set", "effect.plugins.load", "playback.toggle", "help.website"] {
            assert!(!has(list, refused), "{refused}");
        }
        let filtered = dispatch("commands", &json!({"filter": "effect.p"}), host).unwrap();
        let filtered = filtered.as_array().unwrap();
        assert!(has(filtered, "effect.plugins.list") && has(filtered, "effect.pickColor") && !has(filtered, "effect.plugins.load"), "{filtered:?}");
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
        // Commands that name a path of their own never run through the
        // door (and the session's guarded file services would refuse one
        // outside the area anyway).
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

    /// The door runs allowlisted commands in a temporary area with the
    /// shell's resolver installed, as an agent's call: a comp, a solid and a
    /// built-in effect make a new project inside the area, which
    /// `effect.info` and a query read back.
    #[test]
    fn the_door_runs_allowlisted_commands_in_its_area() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        let made = run_in(
            &areas,
            json!({"cmds": [
                {"id": "comp.new", "params": {"name": "Main", "width": 64, "height": 36, "frameRate": 24, "duration": 1.0}},
                {"id": "layer.newSolid", "params": {"name": "Red", "color": "#cc3344"}},
                {"id": "effect.apply", "params": {"effect": "Gaussian Blur"}},
                {"id": "prop.set", "params": {"path": "effects/#1/blurriness", "value": 12.0}},
            ], "out": "blurred.ecproj"}),
            &root,
            false,
        )
        .unwrap();
        assert_eq!(made["out"], json!("blurred.ecproj"), "{made}");
        assert_eq!(made["format"], json!("ecproj"));
        assert!(made["bytes"].as_u64().is_some_and(|n| n > 0), "{made}");
        assert_eq!(made["results"].as_array().unwrap().len(), 4);
        assert_eq!(made["results"][2]["result"]["effect"], json!("ec.blur.gaussian"));
        assert!(root.join("blurred.ecproj").is_file() && !dir.path().join("blurred.ecproj").exists());
        let sum = serve(&areas, &service_call("info", json!({"path": "blurred.ecproj"}), &root, false)).unwrap();
        let comp = sum["items"].as_array().unwrap().iter().find(|i| i["name"] == json!("Main")).cloned().unwrap();
        assert_eq!((comp["size"].clone(), comp["layers"].clone()), (json!([64, 36]), json!(1)), "{sum}");
        // A query writes nothing, and reads the effect back.
        let query = run_in(
            &areas,
            json!({"path": "blurred.ecproj", "cmds": [{"id": "prop.get", "params": {"comp": "Main", "layer": "Red", "path": "effects/#1/blurriness"}}]}),
            &root,
            false,
        )
        .unwrap();
        assert!(query["out"].is_null() && query["format"].is_null(), "{query}");
        assert_eq!(query["results"][0]["result"]["value"], json!(12.0), "{query}");
    }

    /// Every class but `safe` is refused, and so is an id the
    /// classification does not know, before any command runs: a refused id
    /// anywhere in the list writes nothing. Checked over the whole
    /// classification, then through the service for each class.
    #[test]
    fn the_door_refuses_every_other_class_and_unknown_ids() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        let door = door().unwrap();
        let safety: Json = serde_json::from_str(include_str!("../skill/safety.json")).unwrap();
        let area = Area::new(&root, None, false);
        let mut refused_classes = std::collections::BTreeSet::new();
        for (id, class) in safety["commands"].as_object().unwrap() {
            let admitted = door.admit(id, &json!({}), &area);
            match class.as_str().unwrap() {
                "safe" => assert!(door.runs(id) && admitted.is_ok(), "{id}: {admitted:?}"),
                other => {
                    assert!(!door.runs(id) && admitted.is_err(), "{id} is classed {other}");
                    refused_classes.insert(other.to_string());
                }
            }
        }
        assert_eq!(refused_classes.into_iter().collect::<Vec<_>>(), ["code", "device", "file", "host", "network"]);
        let start = json!({"id": "comp.new", "params": {"name": "Main", "width": 16, "height": 16}});
        for (id, params, class) in [
            ("engine.batch", json!({"steps": []}), "code"),
            ("file.runScript", json!({"path": "evil.jsx"}), "code"),
            ("script.run", json!({"code": "1"}), "code"),
            ("scriptui.click", json!({}), "code"),
            ("file.executeFile", json!({"path": "/bin/sh"}), "code"),
            ("help.website", json!({}), "network"),
            ("roto.model.download", json!({}), "network"),
            ("playback.toggle", json!({}), "device"),
            ("prefs.set", json!({"key": "pluginsFolder", "value": "/tmp/evil"}), "host"),
            ("view.zoomIn", json!({}), "host"),
            ("edit.purge", json!({}), "host"),
            ("roto.model.select", json!({"id": "x"}), "host"),
            ("mediaBrowser.addFavorite", json!({}), "host"),
        ] {
            let e = run_in(&areas, json!({"cmds": [start, {"id": id, "params": params}], "out": "x.ecproj"}), &root, false).unwrap_err();
            assert!(e.contains(&format!("`{id}` is classed {class}")) && e.contains("never runs it"), "{id}: {e}");
        }
        for (id, params) in [
            ("file.saveAs", json!({"path": "elsewhere.ecproj"})),
            ("file.open", json!({"path": "/etc/hosts"})),
            ("file.import", json!({"path": "/etc/hosts"})),
            ("mediaBrowser.list", json!({"path": "/"})),
            ("renderQueue.render", json!({})),
            ("comp.saveFrameAs", json!({"path": "f.png"})),
            ("templates.create", json!({"id": "lower-third"})),
            ("roto.model.install", json!({"path": "/etc/hosts"})),
        ] {
            let e = run_in(&areas, json!({"cmds": [start, {"id": id, "params": params}], "out": "x.ecproj"}), &root, false).unwrap_err();
            assert!(e.contains(&format!("`{id}` reads or writes files")) && e.contains("not reviewed to run through it"), "{id}: {e}");
        }
        let e = run_in(&areas, json!({"cmds": [start, {"id": "effect.secret"}], "out": "x.ecproj"}), &root, false).unwrap_err();
        assert!(e.contains("`effect.secret` is not a reviewed effect command"), "{e}");
        assert!(run_in(&areas, json!({"cmds": [start, {"id": ""}]}), &root, false).unwrap_err().contains("has an `id`"));
        assert!(run_in(&areas, json!({"cmds": [{"id": "comp.new", "params": [1]}]}), &root, false).unwrap_err().contains("`params` is an object"));
        let too_many: Vec<Json> = (0..65).map(|_| json!({"id": "time.start"})).collect();
        assert!(run_in(&areas, json!({"cmds": too_many}), &root, false).unwrap_err().contains("at most 64"));
        assert!(!root.join("x.ecproj").exists(), "nothing written");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0, "nothing at all");
    }

    /// Composite commands run other commands that no check on their own id
    /// sees: a batch is refused even when it wraps only an allowed command,
    /// and when it wraps a plug-in install; so are scripts and the tutorial
    /// step that runs its own command list.
    #[test]
    fn the_door_refuses_composites() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        for c in [
            json!({"id": "engine.batch", "params": {"steps": [{"command": "comp.new", "params": {"name": "C"}}]}}),
            json!({"id": "engine.batch", "params": {"steps": [{"command": "effect.plugins.load", "params": {"path": "evil.wasm"}}]}}),
            json!({"id": "file.runScript", "params": {"path": "evil.jsx"}}),
            json!({"id": "learn.step", "params": {"action": "showMe"}}),
        ] {
            let id = c["id"].as_str().unwrap().to_string();
            let cmds = json!([{"id": "learn.start", "params": {"id": "nope"}}, {"id": "comp.new", "params": {"name": "C"}}, c]);
            let e = run_in(&areas, json!({"cmds": cmds, "out": "c.ecproj"}), dir.path(), false).unwrap_err();
            assert!(e.contains(&format!("`{id}` is classed code")), "{id}: {e}");
            assert!(!e.contains("unknown tutorial"), "refused before the first command ran: {e}");
        }
        assert!(!dir.path().join("c.ecproj").exists());
    }

    /// Plug-in loading changes a process-wide registry and runs
    /// WebAssembly: `effect.plugins.load` is classed code and the other
    /// would-be mutators are ids the classification does not know, so the
    /// door refuses them all; listing the registry is `safe` and runs.
    #[test]
    fn the_door_refuses_plugin_mutators_and_lists_plugins() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let e = run_in(&areas, json!({"cmds": [{"id": "effect.plugins.load", "params": {"path": "x.wasm"}}]}), dir.path(), false).unwrap_err();
        assert!(e.contains("`effect.plugins.load` is classed code"), "{e}");
        for id in ["effect.plugins.install", "effect.plugins.reload", "effect.plugins.remove", "effect.plugins.unload"] {
            let e = run_in(&areas, json!({"cmds": [{"id": id, "params": {"path": "x.wasm"}}]}), dir.path(), false).unwrap_err();
            assert!(e.contains(&format!("`{id}` is not a reviewed effect command")), "{id}: {e}");
        }
        let listed = run_in(&areas, json!({"cmds": [{"id": "effect.plugins.list"}]}), dir.path(), false).unwrap();
        let r = &listed["results"][0]["result"];
        assert!(r["plugins"].is_array() && r["wasm"] == json!(false), "no plug-in loader in the service's session: {listed}");
        assert!(listed["out"].is_null());
    }

    /// A plug-in for the tests: it shadows a built-in in the engine's
    /// `lookup` and never draws.
    struct Shadow(effectcraft_engine::effects::plugin::PluginManifest);

    impl effectcraft_engine::effects::plugin::EffectPlugin for Shadow {
        fn manifest(&self) -> &effectcraft_engine::effects::plugin::PluginManifest {
            &self.0
        }

        fn render(
            &self,
            _: &mut effectcraft_engine::effects::plugin::PluginFrame,
            _: &effectcraft_engine::effects::plugin::PluginParams,
            _: f64,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    /// Register, once per test process, two plug-ins that shadow built-ins
    /// in the engine's `lookup`: one under Mosaic's display name in a
    /// category sorted before Stylize, one whose id is Emboss's display
    /// name. No other test names Mosaic or Emboss.
    fn shadow_builtins() {
        use effectcraft_engine::effects::plugin::{register_plugin, PLUGIN_API_VERSION};
        static DONE: OnceLock<()> = OnceLock::new();
        DONE.get_or_init(|| {
            for (id, name) in [("octosense.test.mosaic", "Mosaic"), ("Emboss", "OctoSense Test Emboss")] {
                let manifest = serde_json::from_value(json!({"api": PLUGIN_API_VERSION, "id": id, "name": name, "category": "AAA OctoSense Test"})).unwrap();
                register_plugin(Arc::new(Shadow(manifest))).unwrap();
            }
        });
    }

    /// `effect.apply` runs only an effect the engine builds in, by id,
    /// display name (any case) or alias; a name that is no built-in, a
    /// plug-in's id, or a built-in's name a registered plug-in shadows in
    /// the engine's own lookup is refused before any command runs. A project
    /// whose effect instance names a plug-in is refused by the fence.
    #[test]
    fn the_door_applies_only_built_in_effects() {
        use effectcraft_engine::effects::lookup;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        for name in ["Gaussian Blur", "gaussian BLUR", "ec.blur.gaussian", "Apply Color LUT", "Keylight (1.2)", "mocha shape"] {
            assert!(builtin_effect(name), "{name}");
        }
        for name in ["", "Totally Not An Effect", "org.example.evil", "plugin.evil", "ec.blur"] {
            assert!(!builtin_effect(name), "{name}");
        }
        let apply = |effect: Json| {
            json!({"cmds": [
                {"id": "comp.new", "params": {"name": "Main", "width": 16, "height": 16}},
                {"id": "layer.newSolid", "params": {"name": "Red", "color": "#cc3344"}},
                {"id": "effect.apply", "params": {"effect": effect}},
            ], "out": "fx.ecproj"})
        };
        for name in ["Totally Not An Effect", "plugin.evil", ""] {
            let e = run_in(&areas, apply(json!(name)), &root, false).unwrap_err();
            assert!(e.contains(&format!("`{name}` is not an effect the engine builds in")), "{name}: {e}");
        }
        assert!(run_in(&areas, apply(json!(7)), &root, false).unwrap_err().contains("is not an effect the engine builds in"));
        assert!(run_in(&areas, apply(json!(["Gaussian Blur", "plugin.evil"])), &root, false).unwrap_err().contains("`plugin.evil`"));
        assert!(!root.join("fx.ecproj").exists());
        // Registered plug-ins that shadow built-ins: the engine's lookup
        // lands on them, so the door refuses those names.
        shadow_builtins();
        assert_eq!(lookup("Mosaic").map(|s| s.id), Some("octosense.test.mosaic"), "the shadow is live");
        assert_eq!(lookup("Emboss").map(|s| s.id), Some("Emboss"), "the shadow is live");
        for name in ["Mosaic", "MOSAIC", "Emboss", "octosense.test.mosaic", "OctoSense Test Emboss"] {
            assert!(!builtin_effect(name), "{name}");
            let e = run_in(&areas, apply(json!(name)), &root, false).unwrap_err();
            assert!(e.contains("is not an effect the engine builds in"), "{name}: {e}");
        }
        assert!(!root.join("fx.ecproj").exists());
        // The built-ins themselves, by id, still apply.
        let mut cmds = apply(json!("ec.stylize.mosaic"));
        cmds["cmds"].as_array_mut().unwrap().push(json!({"id": "effect.apply", "params": {"effect": "ec.stylize.emboss"}}));
        cmds["cmds"].as_array_mut().unwrap().push(json!({"id": "effect.plugins.list"}));
        let made = run_in(&areas, cmds, &root, false).unwrap();
        assert_eq!((made["results"][2]["result"]["effect"].clone(), made["results"][3]["result"]["effect"].clone()), (json!("ec.stylize.mosaic"), json!("ec.stylize.emboss")));
        let ids: Vec<&str> = made["results"][4]["result"]["plugins"].as_array().unwrap().iter().filter_map(|p| p["id"].as_str()).collect();
        assert!(ids.contains(&"octosense.test.mosaic") && ids.contains(&"Emboss"), "{ids:?}");
        // A project whose effect instance names the plug-in: refused by every
        // method that opens it.
        let text = std::fs::read_to_string(root.join("fx.ecproj")).unwrap();
        let hostile = text.replace("\"ec.stylize.mosaic\"", "\"octosense.test.mosaic\"");
        assert_ne!(hostile, text);
        std::fs::write(root.join("plugin.ecproj"), hostile).unwrap();
        for (method, args) in [
            ("info", json!({"path": "plugin.ecproj"})),
            ("run", json!({"path": "plugin.ecproj", "cmds": [], "out": "plugin.png"})),
            ("render", json!({"path": "plugin.ecproj", "out": "plugin2.png"})),
        ] {
            let e = serve(&areas, &service_call(method, args, &root, false)).unwrap_err();
            assert!(e.contains("`octosense.test.mosaic`") && e.contains("is an effect plug-in"), "{method}: {e}");
        }
        assert!(!root.join("plugin.png").exists() && !root.join("plugin2.png").exists());
    }

    /// #419's LUT fence stays closed through the door, as the shell calls
    /// it: Apply Color LUT's `lut` set to a `.cube` outside the area is
    /// refused right after the command that sets it, for every kind of
    /// `out`, with nothing written; the LUT's text inline is drawn. The same
    /// holds when the path arrives through Essential Graphics: an OCIO File
    /// Transform's `file` exposed as a control and given the path as a
    /// precomp layer's value (the renderer puts it into the parameter), in
    /// one precomp or through two. Each hostile fixture is live: the engine's
    /// own session reads the outside file.
    #[test]
    fn the_door_keeps_the_lut_fence_closed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let secret = dir.path().join("secret.cube");
        std::fs::write(&secret, GREEN_LUT).unwrap();
        let secret = secret.to_string_lossy().into_owned();
        let areas = resolver(&root, None);
        let lut = |value: &str| {
            json!([
                {"id": "comp.new", "params": {"name": "Main", "width": 16, "height": 16, "frameRate": 24, "duration": 1.0}},
                {"id": "layer.newSolid", "params": {"name": "Red", "color": "#cc3344"}},
                {"id": "effect.apply", "params": {"effect": "Apply Color LUT"}},
                {"id": "prop.set", "params": {"path": "effects/#1/lut", "value": value}},
                {"id": "effect.pickColor", "params": {"param": "missing", "x": 1, "y": 1}},
            ])
        };
        for out in ["lut.ecproj", "lut.png", "lut.json"] {
            let e = run_in(&areas, json!({"cmds": lut(&secret), "out": out}), &root, false).unwrap_err();
            assert!(e.contains("names a file") && e.contains("`lut`") && !e.contains("pickColor"), "{out}: {e}");
            assert!(!root.join(out).exists(), "{out}");
        }
        let mut inline = lut(GREEN_LUT);
        inline.as_array_mut().unwrap().pop();
        let drawn = run_in(&areas, json!({"cmds": inline, "out": "inline.png", "max_side": 16}), &root, false).unwrap();
        assert_eq!(drawn["format"], json!("png"), "{drawn}");
        assert_eq!(centre(&std::fs::read(root.join("inline.png")).unwrap())[..3], [0, 255, 0], "the inline LUT is drawn");

        // Through Essential Graphics: Inner's OCIO `file` is a control, and
        // Main's layer of Inner gives it a value.
        let exposed = |value: &str| {
            vec![
                json!({"id": "comp.new", "params": {"name": "Inner", "width": 16, "height": 16, "frameRate": 24, "duration": 1.0}}),
                json!({"id": "layer.newSolid", "params": {"name": "Lit", "color": "#cc3344"}}),
                json!({"id": "effect.apply", "params": {"effect": "OCIO File Transform"}}),
                json!({"id": "essential.addProperty", "params": {"comp": "Inner", "layer": "Lit", "path": "effects/#1/file"}}),
                json!({"id": "comp.new", "params": {"name": "Main", "width": 16, "height": 16, "frameRate": 24, "duration": 1.0}}),
                json!({"id": "layer.addItem", "params": {"comp": "Main", "item": "Inner"}}),
                json!({"id": "essential.set", "params": {"comp": "Main", "layer": "Inner", "control": "File", "value": value}}),
            ]
        };
        // Then Top's layer of Main gives Main's control of that value
        // another one (without one, Main's own value shows).
        let nested = |control: u64, value: Option<&str>| {
            let mut cmds = vec![
                json!({"id": "essential.addProperty", "params": {"comp": "Main", "layer": "Inner", "path": format!("essential/eg{control}"), "name": "Look"}}),
                json!({"id": "comp.new", "params": {"name": "Top", "width": 16, "height": 16, "frameRate": 24, "duration": 1.0}}),
                json!({"id": "layer.addItem", "params": {"comp": "Top", "item": "Main"}}),
            ];
            if let Some(value) = value {
                cmds.push(json!({"id": "essential.set", "params": {"comp": "Top", "layer": "Main", "control": "Look", "value": value}}));
            }
            cmds
        };
        // The engine's own session, unfenced, reads the outside file both
        // ways.
        let engine = |cmds: Vec<Json>, comp: &str| {
            let mut b = Backend::headless(Session::default());
            let mut control = 0;
            for c in cmds {
                let r = b.exec(c["id"].as_str().unwrap(), c["params"].clone()).unwrap();
                control = r["controls"][0].as_u64().unwrap_or(control);
            }
            (centre(&b.render_with(Some(&json!(comp)), Some(0.0), 16, false).unwrap().png), control)
        };
        assert_eq!(engine(exposed(""), "Main").0[..3], [0xcc, 0x33, 0x44], "no value: the solid as it is");
        assert_eq!(engine(exposed(&secret), "Main").0[..3], [0, 255, 0], "the fixture is live: the engine reads the outside LUT");
        let (blue, control) = engine(exposed(BLUE_LUT), "Main");
        assert_eq!(blue[..3], [0, 0, 255], "a value inline is drawn");
        let chain = |value: Option<&str>| exposed(BLUE_LUT).into_iter().chain(nested(control, value)).collect::<Vec<Json>>();
        assert_eq!(engine(chain(None), "Top").0[..3], [0, 0, 255], "Main's own value");
        assert_eq!(engine(chain(Some(&secret)), "Top").0[..3], [0, 255, 0], "the nested fixture is live too");
        // The door refuses both, right after the value is set.
        let e = run_in(&areas, json!({"cmds": exposed(&secret), "out": "exposed.png", "max_side": 16}), &root, false).unwrap_err();
        assert!(e.contains("Essential Property `File`") && e.contains("names a file") && e.contains("`file`"), "{e}");
        let first = run_in(&areas, json!({"cmds": exposed(BLUE_LUT), "out": "chain.ecproj"}), &root, false).unwrap();
        assert_eq!(first["results"][3]["result"]["controls"][0].as_u64(), Some(control), "{first}");
        let e = run_in(&areas, json!({"path": "chain.ecproj", "cmds": nested(control, Some(&secret)), "out": "chain.png", "max_side": 16}), &root, false).unwrap_err();
        assert!(e.contains("Essential Property `Look`") && e.contains("names a file") && e.contains("`file`"), "{e}");
        assert!(!root.join("exposed.png").exists() && !root.join("chain.png").exists());
        // The file's text inline, as a value, is drawn.
        run_in(&areas, json!({"path": "chain.ecproj", "cmds": nested(control, Some(GREEN_LUT)), "out": "chain-inline.png", "comp": "Top", "max_side": 16}), &root, false).unwrap();
        assert_eq!(centre(&std::fs::read(root.join("chain-inline.png")).unwrap())[..3], [0, 255, 0]);
        // A project file carrying such a value is refused when opened.
        let text = std::fs::read_to_string(root.join("chain.ecproj")).unwrap();
        let quoted = |v: &str| serde_json::to_string(v).unwrap();
        let hostile = text.replace(&quoted(BLUE_LUT), &quoted(&secret));
        assert_ne!(hostile, text);
        std::fs::write(root.join("hostile.ecproj"), hostile).unwrap();
        let e = serve(&areas, &service_call("info", json!({"path": "hostile.ecproj"}), &root, false)).unwrap_err();
        assert!(e.contains("Essential Property `File`") && e.contains("names a file"), "{e}");
    }

    /// `run` writes each kind of `out` by its extension: the project
    /// (.ecproj), a composition as Lottie (.json, .lottie) with its
    /// warnings, one frame (.png) with its size; a Lottie `path` opens as a
    /// composition the commands then edit. Another extension is refused
    /// before the engine works.
    #[test]
    fn the_door_writes_each_kind_of_out() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        let project = fixture_in(&areas, &root);
        assert_eq!((project["format"].clone(), project["out"].clone()), (json!("ecproj"), json!("main.ecproj")), "{project}");
        assert!(project["bytes"].as_u64().is_some_and(|n| n > 0), "{project}");
        let lottie = run_in(&areas, json!({"path": "main.ecproj", "cmds": [], "out": "main.json", "comp": "Main"}), &root, false).unwrap();
        assert_eq!(lottie["format"], json!("json"), "{lottie}");
        assert!(lottie["warnings"].is_array() && lottie["bytes"].as_u64().is_some_and(|n| n > 0), "{lottie}");
        let parsed: Json = serde_json::from_slice(&std::fs::read(root.join("main.json")).unwrap()).unwrap();
        assert!(parsed["layers"].is_array(), "{parsed}");
        let dot = run_in(&areas, json!({"path": "main.ecproj", "cmds": [], "out": "main.lottie"}), &root, false).unwrap();
        assert_eq!(dot["format"], json!("lottie"), "{dot}");
        assert!(std::fs::read(root.join("main.lottie")).unwrap().starts_with(b"PK"), "a dotLottie archive");
        let frame = run_in(
            &areas,
            json!({"path": "main.ecproj", "cmds": [{"id": "layer.newSolid", "params": {"name": "Blue", "color": "#2244cc"}}], "out": "frame.png", "time": 0.0, "max_side": 32}),
            &root,
            false,
        )
        .unwrap();
        assert_eq!((frame["format"].clone(), frame["width"].clone(), frame["height"].clone()), (json!("png"), json!(32), json!(18)), "{frame}");
        let png = std::fs::read(root.join("frame.png")).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert_eq!(centre(&png)[..3], [0x22, 0x44, 0xcc], "the command ran before the frame was drawn");
        let back = run_in(
            &areas,
            json!({"path": "main.json", "cmds": [{"id": "layer.newSolid", "params": {"name": "Blue", "color": "#2244cc"}}], "out": "from-lottie.ecproj"}),
            &root,
            false,
        )
        .unwrap();
        assert!(back["imported"]["comp"].is_number() && back["imported"]["warnings"].is_array(), "{back}");
        let sum = serve(&areas, &service_call("info", json!({"path": "from-lottie.ecproj"}), &root, false)).unwrap();
        let comp = sum["items"].as_array().unwrap().iter().find(|i| i["type"] == json!("Composition") && i["size"] == json!([64, 36])).cloned();
        assert_eq!(comp.map(|c| c["layers"].clone()), Some(json!(2)), "the Lottie comp, with the new solid: {sum}");
        let e = run_in(&areas, json!({"cmds": [{"id": "comp.new"}], "out": "movie.mp4"}), &root, false).unwrap_err();
        assert!(e.contains("`out` is a project (.ecproj)"), "{e}");
        assert!(!root.join("movie.mp4").exists());
    }

    /// Whatever `run` writes keeps the area's rules: an agent's `out` never
    /// replaces a file, for any kind; the result fits the quota; `out` and
    /// `path` stay inside the area; an app's own call may replace.
    #[test]
    fn the_doors_writes_keep_the_areas_rules() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        fixture_in(&areas, &root);
        for name in ["taken.ecproj", "taken.json", "taken.png"] {
            std::fs::write(root.join(name), b"keep me").unwrap();
            let e = run_in(&areas, json!({"path": "main.ecproj", "cmds": [], "out": name}), &root, false).unwrap_err();
            assert!(e.contains("already exists"), "{name}: {e}");
            assert_eq!(std::fs::read(root.join(name)).unwrap(), b"keep me", "{name}");
        }
        let tight = resolver(&root, Some(8));
        for name in ["q.ecproj", "q.json", "q.png"] {
            let e = run_in(&tight, json!({"path": "main.ecproj", "cmds": [], "out": name}), &root, true).unwrap_err();
            assert!(e.contains("bytes left"), "{name}: {e}");
            assert!(!root.join(name).exists(), "{name}");
        }
        std::fs::write(dir.path().join("up.json"), b"{}").unwrap();
        for bad in ["../up.ecproj", "/etc/x.png", "a/../../up.json"] {
            assert!(run_in(&areas, json!({"cmds": [], "out": bad}), &root, true).is_err(), "out {bad}");
            assert!(run_in(&areas, json!({"cmds": [], "path": bad}), &root, true).is_err(), "path {bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            assert!(run_in(&areas, json!({"cmds": [], "path": "up/up.json"}), &root, true).is_err());
            assert!(run_in(&areas, json!({"path": "main.ecproj", "cmds": [], "out": "up/f.png"}), &root, true).is_err());
            assert!(!dir.path().join("f.png").exists());
        }
        run_in(&areas, json!({"path": "main.ecproj", "cmds": [], "out": "taken.png", "max_side": 8}), &root, true).unwrap();
        assert!(std::fs::read(root.join("taken.png")).unwrap().starts_with(&[0x89, b'P', b'N', b'G']), "an app's own call may replace");
    }

    /// The door's gate is built from the generated classification: it runs
    /// exactly the `safe` ids.
    #[test]
    fn the_door_is_built_from_the_reviewed_classification() {
        let door = door().unwrap();
        let safety: Json = serde_json::from_str(include_str!("../skill/safety.json")).unwrap();
        let safe = safety["commands"].as_object().unwrap().values().filter(|c| *c == "safe").count();
        assert_eq!(door.runnable().len(), safe);
        assert!(door.runs("comp.new") && door.runs("effect.apply") && door.runs("prop.set") && door.runs("effect.plugins.list") && door.runs("essential.set"));
        for refused in ["engine.batch", "file.saveAs", "prefs.set", "effect.plugins.load", "mediaBrowser.addFavorite", "learn.step", "nope"] {
            assert!(!door.runs(refused), "{refused}");
        }
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

    /// Every `effect.run` call the skill's examples show runs, in order,
    /// in one area as the system agent's (with the files they name placed
    /// there first), and writes its `out`: the skill teaches commands that
    /// work.
    #[test]
    fn the_skill_examples_run() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        // The Lottie animation the examples open: one red solid layer.
        std::fs::write(dir.path().join("intro.json"), br##"{"v":"5.7.0","fr":24,"ip":0,"op":24,"w":32,"h":18,"nm":"Main","ddd":0,"assets":[],"layers":[{"ddd":0,"ind":1,"ty":1,"nm":"Red","sr":1,"ks":{"o":{"a":0,"k":100},"r":{"a":0,"k":0},"p":{"a":0,"k":[16,9,0]},"a":{"a":0,"k":[16,9,0]},"s":{"a":0,"k":[100,100,100]}},"ao":0,"sw":32,"sh":18,"sc":"#cc3344","ip":0,"op":24,"st":0,"bm":0}]}"##).unwrap();
        let body = include_str!("../skill/SKILL.md");
        let examples = body.split("\n## Examples").nth(1).and_then(|rest| rest.split("\n## ").next()).unwrap();
        let mut ran = 0;
        for span in examples.split('`').skip(1).step_by(2) {
            let Some(args) = span.strip_prefix("effect.run ") else { continue };
            let args: Json = serde_json::from_str(args).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            let got = serve(&areas, &service_call("run", args.clone(), dir.path(), false)).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            assert_eq!(got["results"].as_array().map(Vec::len), args["cmds"].as_array().map(Vec::len), "{got}");
            if let Some(out) = args["out"].as_str() {
                assert!(dir.path().join(out).is_file(), "`{span}` wrote no {out}");
            }
            ran += 1;
        }
        assert!(ran >= 4, "{ran} examples");
        let info = serve(&areas, &service_call("info", json!({"path": "title.ecproj"}), dir.path(), false)).unwrap();
        assert!(info.to_string().contains("Title"), "{info}");
        let blurred = String::from_utf8_lossy(&std::fs::read(dir.path().join("intro.ecproj")).unwrap()).to_lowercase();
        assert!(blurred.contains("gaussian"), "the blur is in the saved project");
    }
}
