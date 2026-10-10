//! `octosense-photo-service` — the `photo` host service (ADR 0013), and
//! the Photos system app's own `photos` service.
//!
//! photocraft's engine through its own headless automation layer. Every
//! call is a fresh, stateless session whose file access is bound to the
//! call's area by photocraft's capability-rooted [`AuthorizedWorkspace`] —
//! paths are relative, and `..`, prefixes and symlink escapes are refused by
//! the engine before any I/O — and whose commands pass photocraft's own
//! policy, which refuses every command that names a path of its own.
//!
//! **Where a call works** (ADR 0013, 2026-10-08): the caller's own folder,
//! the [`Area`] the shell's resolver gives it ([`set_area_resolver`]), or
//! without one the legacy `<host dir>/photo`. A session may read the area
//! and never write it directly: a rendered preview goes through
//! [`Area::write`], and a saved document is written into a staging folder
//! inside the area (the session's only write authority) and moved into place
//! under the area's rules ([`octosense_engine_area::Stage`]), so a write that
//! may not replace (an agent's) only creates new files, within the quota.
//! A document with a smart object linked to a file outside it, or whose Color
//! Lookup smart filter names a table file, is refused once it opens and after
//! every command that could have changed it ([`fence_links`]): the engine
//! reads such a file by its own path, past the workspace (a PSD export embeds
//! a linked file's raw bytes; every re-render of the smart object reads the
//! table).
//!
//! Methods (all under the `photo` family; paths relative to the area):
//! - `info {path}` → the document inspected as JSON
//! - `convert {path, out, format?}` → `{out, warnings}`
//! - `run {path, cmds: [{id, params?}], out?, format?}` → `{results: [{id,
//!   result}], out}` — the command door (ADR 0013, #418): run commands of
//!   photocraft's registry (crops, adjustments, filters) on the document at
//!   `path`, saving it when `out` is given. Only what the door's allowlist
//!   admits runs ([`door`]): the commands the reviewed classification
//!   (`skill/safety.json`) classes `safe`; every other id is refused before
//!   any command runs, and photocraft's own workspace policy then checks each
//!   one again
//! - `commands {}` → the commands `run` runs (the engine's catalog, narrowed
//!   to the door and the engine's workspace policy)
//! - `render {path, out, max_side?}` → a PNG preview written to `out`
//!
//! The service serves system apps only until ADR 0013's store capability
//! is designed.
//!
//! **The `photos` service** ([`register_photos`]) is the Photos system
//! app's own namespace, as `mail` is Mail's: its agent's tools run here
//! (`host_tools::script_apps`). It answers `photos.info` on the same
//! engine and area, and `photos.notify` through the shell's notice hook
//! ([`on_notify`]), so the notice service never stands in for Photos.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::door::{Door, Reviewed};
use octosense_engine_area::{Area, Slot};
use photocraft_automation::headless::Headless;
use photocraft_automation::workspace::{authorize_engine_command, AuthorizedWorkspace};
use photocraft_io::ExportOptions;
use serde_json::{json, Value as Json};

/// The longest preview edge `render` produces.
const MAX_RENDER_SIDE: u32 = 4096;

/// What the photo engine's reviewer settled for the door beyond the classes:
/// nothing, read in the engine at the pinned revision.
///
/// - No reviewed read: photocraft's workspace policy, which every command of
///   a `run` passes after the door, refuses the path parameter of the `file`
///   commands that read one whatever it names (every `file.*` id but a few
///   in-memory ones, a Color Lookup's `file`, Displace's `mapPath`, a brush
///   or gradient import's `path`), so a read the door reviewed would still
///   not run.
/// - No setter: no `safe` command sets an app-wide variable; `prefs.set`,
///   the one that does by key, is `code` (its plug-ins folder and Script
///   Events load and run code).
/// - No inner id: the `safe` commands that name a preset, tool or action
///   reach no plug-in, script or other command. `tools.setBrush {preset}`
///   copies a brush preset's settings from the session's own library, and
///   `tool.presets.select {preset}` applies a tool preset's brush through
///   the fixed `tools.setBrush`; `style.presets.apply {preset}` copies layer
///   effects; `actions.record`/`stop`/`get`/`list`/`delete` only edit or read
///   the session's action list, whose steps only `actions.play` replays and
///   `file.automate.batch` and the droplets take, all `code` (and a headless
///   session has no preset store to save the list to). The other preset
///   commands call fixed ids (`shape.presets.place` → `shape.create`, the
///   pattern presets → `layer.newFillLayer.pattern`, `edit.definePattern`).
///
/// A smart filter names its filter by id, but only the filter command that
/// made it sets that id, after passing this gate; what a filter's parameters
/// could plant (a Color Lookup's table `file`) is [`fence_links`]' to refuse,
/// and `layer.smartFilter.setParams`, which merges parameters into a smart
/// filter, is classed `file` for that reason.
static REVIEWED: Reviewed = Reviewed::NONE;

/// The command door's gate: photocraft's reviewed classification
/// (`skill/safety.json`, generated and drift-checked by `tests/skill.rs`)
/// and [`REVIEWED`].
pub fn door() -> Result<&'static Door, String> {
    static DOOR: OnceLock<Result<Door, String>> = OnceLock::new();
    DOOR.get_or_init(|| Door::new("photo", include_str!("../skill/safety.json"), &REVIEWED)).as_ref().map_err(Clone::clone)
}

/// Which apps may call the service: system apps, as News and Sheets.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct PhotoService;

/// Register the `photo` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(PhotoService));
}

/// The shell's resolver, for both the `photo` service and Photos' own
/// `photos.info`: where each call works (`None` removes it, and calls work
/// in the legacy `<host dir>/photo` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
}

impl HostService for PhotoService {
    fn family(&self) -> &'static str {
        "photo"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        reply.send(serve(&AREAS, &call));
    }
}

/// One call, in the area `areas` gives it.
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The photo service serves system apps only.".into());
    }
    if call.method() == "commands" {
        return commands();
    }
    let area = areas.area(call, "photo").map_err(|e| format!("photo: {e}"))?;
    dispatch_in(call.method(), &call.args, &area)
        .map(|answer| area.relative_json(answer))
        .map_err(|error| area.relative_text(&error))
}

/// The shell's notice hook: `photos.notify`, as Photos, to the glance
/// screen (`glance_notice::notify`).
pub type Notifier = Arc<dyn Fn(&str, &Json) -> Result<Json, String> + Send + Sync>;

static NOTIFIER: OnceLock<Mutex<Option<Notifier>>> = OnceLock::new();

fn notifier() -> &'static Mutex<Option<Notifier>> {
    NOTIFIER.get_or_init(|| Mutex::new(None))
}

/// The shell's notifier for `photos.notify` (None removes it).
pub fn on_notify(notify: Option<Notifier>) {
    *notifier().lock().unwrap_or_else(|e| e.into_inner()) = notify;
}

/// The Photos system app's own service: `photos.info` on the engine,
/// `photos.notify` through the shell's notice hook. Registered by the
/// shell beside [`register`], before the notice service would stand in.
pub fn register_photos() {
    register_host_service(Box::new(PhotosAppService));
}

struct PhotosAppService;

impl HostService for PhotosAppService {
    fn family(&self) -> &'static str {
        "photos"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        if call.app_id != "os.photos" {
            reply.send(Err("The photos service serves os.photos only.".into()));
            return;
        }
        match call.method() {
            "info" => reply.send(AREAS.area(&call, "photo").map_err(|e| format!("photos: {e}")).and_then(|area| {
                info(&call.args, &area)
                    .map(|answer| area.relative_json(answer))
                    .map_err(|error| area.relative_text(&error))
            })),
            "notify" => {
                let notify = notifier().lock().unwrap_or_else(|e| e.into_inner()).clone();
                reply.send(match notify {
                    Some(notify) => notify(&call.app_id, &call.args),
                    None => Err("Photos' notices need the shell".into()),
                });
            }
            other => reply.send(Err(format!("photos.{other}: the photos service has no method of that name"))),
        }
    }
}

/// A session that reads the area and may write only `write` (a staging
/// folder inside the area), or nothing.
fn session(area: &Area, write: Option<&Path>) -> Result<Headless, String> {
    let ws = AuthorizedWorkspace::new(Some(&area.root), write).map_err(|e| format!("photo: {e}"))?;
    Ok(Headless::with_workspace(ws))
}

fn dispatch_in(method: &str, args: &Json, area: &Area) -> Result<Json, String> {
    match method {
        "info" => info(args, area),
        "convert" => convert(args, area),
        "run" => run(args, area),
        "commands" => commands(),
        "render" => render(args, area),
        other => Err(format!("photo.{other} is not a method of the photo service")),
    }
}

/// [`dispatch_in`] in an area at `root` that may replace and has no quota,
/// as the legacy area does.
#[cfg(test)]
fn dispatch(method: &str, args: &Json, root: &Path) -> Result<Json, String> {
    dispatch_in(method, args, &Area::new(root, None, true))
}

/// The deepest smart objects nest that [`check_links`] follows (the PSD
/// exporter refuses deeper nesting itself).
const MAX_LINK_DEPTH: usize = 8;

/// Refuse a document with a smart object linked to a file outside it. The
/// engine reads such a link by its own path with `std::fs`, past the
/// session's workspace: a PSD export embeds the file's raw bytes, and
/// opening the contents reads it. A link whose file the document carries
/// (a PSD's embedded linked file) is the document's own. An embedded source
/// that is itself a layered document (a `.pcraft`, which the PSD exporter
/// converts, a PSD, PSB or TIFF) is checked the same way.
///
/// A smart filter whose parameters name a file is refused too: every
/// re-render of the smart object (a smart filter edit, a transform, a canvas
/// change) reads it by its own path ([`filter_file`]), shown or hidden, so a
/// filter that becomes visible reads it as well.
fn check_links(doc: &photocraft_doc::Document, depth: usize, m: &str) -> Result<(), String> {
    use photocraft_doc::{LayerContent, SmartSource};
    for (_, _, layer) in doc.walk() {
        let LayerContent::Smart(sm) = &layer.content else { continue };
        if let Some(file) = sm.smart_filters.iter().find_map(filter_file) {
            let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
            return Err(format!(
                "{m}: the smart object `{}` has a Color Lookup smart filter that reads a table file by its own path (`{name}`): the engine would read it from wherever it points; embed the table (`data`) or use a built-in `lut` instead",
                layer.name
            ));
        }
        match &sm.source {
            SmartSource::Linked { path } => {
                if photocraft_io::linked::find_linked_file(&doc.metadata, path).is_none() {
                    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
                    return Err(format!(
                        "{m}: the smart object `{}` links a file outside the document (`{name}`): the engine would read it from wherever it points; embed it instead",
                        layer.name
                    ));
                }
            }
            SmartSource::Embedded { file_name, bytes } => {
                let layered = photocraft_format::is_pcraft(bytes) || bytes.starts_with(b"8BPS") || bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*");
                if !layered {
                    continue;
                }
                if depth >= MAX_LINK_DEPTH {
                    return Err(format!("{m}: smart objects nest more than {MAX_LINK_DEPTH} deep"));
                }
                let inner = if photocraft_format::is_pcraft(bytes) {
                    photocraft_format::load_from_bytes(bytes).map_err(|e| e.to_string())
                } else {
                    photocraft_io::import(file_name, bytes).map(|r| r.document).map_err(|e| e.to_string())
                };
                // Contents the engine cannot read as a document, it embeds
                // as they are, reading nothing more.
                if let Ok(inner) = inner {
                    check_links(&inner, depth + 1, m)?;
                }
            }
        }
    }
    Ok(())
}

/// The file a smart filter reads by its own path when the smart object
/// re-renders: a Color Lookup's table `file` (photocraft's
/// `smart_cmds::apply_smart_filters` → `filters::apply_filter_to_surface` →
/// `adjust_cmds::adjust_as_filter` → `lookup_from_params`, `std::fs::read`).
/// No other smart filter reads a file when it re-renders: a Displace's
/// `mapPath` is resolved only when the filter command itself runs.
fn filter_file(f: &photocraft_doc::SmartFilter) -> Option<&str> {
    if f.command != "image.adjustments.colorLookup" {
        return None;
    }
    f.params.get("file").and_then(Json::as_str).filter(|file| !file.is_empty())
}

/// [`check_links`] on every document the session holds: once a document
/// opens, after every command `run` runs, and before a save.
fn fence_links(h: &Headless, m: &str) -> Result<(), String> {
    for d in h.session.documents() {
        check_links(&d.doc, 0, m)?;
    }
    Ok(())
}

/// An output path the call may write: relative and plain (the engine's own
/// rules: no `..`, `\`, `:` or absolute path), its deepest existing
/// ancestor inside the area through symlinks, and admitted by the area's
/// rules before the engine works.
fn out_path(area: &Area, rel: &str, m: &str) -> Result<PathBuf, String> {
    let outside = || format!("{m}: `out` stays inside this call's folder");
    let p = Path::new(rel);
    if rel.contains(['\\', ':']) || p.is_absolute() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(outside());
    }
    let joined = area.root.join(p);
    let root = area.root.canonicalize().map_err(|e| format!("{m}: folder: {e}"))?;
    let mut deepest = joined.clone();
    while !deepest.exists() {
        match deepest.parent() {
            Some(parent) => deepest = parent.to_path_buf(),
            None => break,
        }
    }
    if !deepest.canonicalize().map_err(|e| format!("{m}: {e}"))?.starts_with(&root) {
        return Err(outside());
    }
    area.check(&joined, 0).map_err(|e| format!("{m}: {e}"))?;
    Ok(joined)
}

/// The saved document's name in the staging folder: `out`'s own leaf, so
/// the engine picks the format by its extension as it would at `out`.
fn leaf(out: &Path) -> String {
    out.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn arg_path<'a>(args: &'a Json, key: &str) -> Result<&'a str, String> {
    args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("photo: `{key}` is required"))
}

fn info(args: &Json, area: &Area) -> Result<Json, String> {
    let path = arg_path(args, "path")?;
    let mut h = session(area, None)?;
    let opened = h.open(Path::new(path)).map_err(|e| format!("photo.info: {e}"))?;
    fence_links(&h, "photo.info")?;
    let mut doc = h.inspect(None).map_err(|e| format!("photo.info: {e}"))?;
    doc["warnings"] = opened["warnings"].clone();
    doc["file"] = json!(path);
    Ok(doc)
}

fn convert(args: &Json, area: &Area) -> Result<Json, String> {
    let path = arg_path(args, "path")?;
    let out = arg_path(args, "out")?;
    let out_abs = out_path(area, out, "photo.convert")?;
    let format = args["format"].as_str();
    let stage = area.stage().map_err(|e| format!("photo.convert: {e}"))?;
    let mut h = session(area, Some(stage.dir()))?;
    let opened = h.open(Path::new(path)).map_err(|e| format!("photo.convert: {e}"))?;
    fence_links(&h, "photo.convert")?;
    let saved = h
        .save(None, Some(Path::new(&leaf(&out_abs))), format, &ExportOptions::default())
        .map_err(|e| format!("photo.convert: {e}"))?;
    stage.commit(&[(stage.path(leaf(&out_abs)), out_abs)]).map_err(|e| format!("photo.convert: {e}"))?;
    Ok(json!({"out": out, "warnings": [opened["warnings"], saved["warnings"]]}))
}

/// The command door: every command admitted by [`door`], and by photocraft's
/// own workspace policy, before the engine runs any (a refused one refuses
/// the whole call, with nothing opened or written), then run in order on the
/// document at `path` in a session that
/// reads the area and writes only a staging folder, the documents fenced
/// ([`fence_links`]) once it opens and after every command, and the result
/// saved to `out` under the area's rules.
///
/// photocraft's plug-ins (`plugin.*`, `code`) never run: the registry is
/// process-wide and takes WebAssembly from in-band `data`, which the
/// engine's workspace policy does not cover, so an installed plug-in would
/// outlive the call, serve every later caller of any app, and run under
/// photocraft's own budgets (60 s, 512 MiB), far above the shell's `wasm`
/// service (ADR 0011). Agents install WebAssembly only through that service.
fn run(args: &Json, area: &Area) -> Result<Json, String> {
    let path = arg_path(args, "path")?;
    // Admit every command first: one refused id refuses the whole call, with
    // nothing opened and nothing written. photocraft's own workspace policy,
    // which the session applies to every command again, refuses some `safe`
    // ids too: here, before any command runs.
    let admitted = door()?.admit_all(&args["cmds"], area)?;
    for (id, params) in &admitted {
        authorize_engine_command(id, params).map_err(|e| format!("photo.run: {e}"))?;
    }
    let out_abs = match args["out"].as_str() {
        Some(out) if !out.is_empty() => Some(out_path(area, out, "photo.run")?),
        _ => None,
    };
    // Whatever the commands might write lands in the staging folder, and
    // only the saved document leaves it.
    let stage = area.stage().map_err(|e| format!("photo.run: {e}"))?;
    let mut h = session(area, Some(stage.dir()))?;
    h.open(Path::new(path)).map_err(|e| format!("photo.run: {e}"))?;
    fence_links(&h, "photo.run")?;
    let mut results = Vec::with_capacity(admitted.len());
    for (id, params) in admitted {
        let r = h.command_run(&id, params).map_err(|e| format!("photo.run {id}: {e}"))?;
        // What the command did to the documents, before the next one runs.
        fence_links(&h, &format!("photo.run {id}"))?;
        results.push(json!({"id": id, "result": r}));
    }
    let saved = match (args["out"].as_str(), out_abs) {
        (Some(out), Some(out_abs)) => {
            fence_links(&h, "photo.run")?;
            h.save(None, Some(Path::new(&leaf(&out_abs))), args["format"].as_str(), &ExportOptions::default())
                .map_err(|e| format!("photo.run: {e}"))?;
            stage.commit(&[(stage.path(leaf(&out_abs)), out_abs)]).map_err(|e| format!("photo.run: {e}"))?;
            json!(out)
        }
        _ => Json::Null,
    };
    Ok(json!({"results": results, "out": saved}))
}

/// The catalog `run` accepts: the engine's own, narrowed to the ids the door
/// runs ([`door`]) and photocraft's workspace policy lets through (it refuses
/// some `safe` ids whatever their parameters, `layer.smartObjects.warp` for
/// one), so the offer matches the gate.
fn commands() -> Result<Json, String> {
    let door = door()?;
    let runs = |id: &str| door.runs(id) && authorize_engine_command(id, &json!({})).is_ok();
    let keep = |items: Vec<Json>| -> Vec<Json> { items.into_iter().filter(|c| c["id"].as_str().is_some_and(runs)).collect() };
    Ok(match Headless::new().command_list() {
        Json::Array(items) => Json::Array(keep(items)),
        Json::Object(mut map) => {
            if let Some(Json::Array(items)) = map.remove("commands") {
                map.insert("commands".into(), Json::Array(keep(items)));
            }
            Json::Object(map)
        }
        other => other,
    })
}

fn render(args: &Json, area: &Area) -> Result<Json, String> {
    let path = arg_path(args, "path")?;
    let out = arg_path(args, "out")?;
    let out_abs = out_path(area, out, "photo.render")?;
    let max_side = args["max_side"].as_u64().unwrap_or(1024).min(MAX_RENDER_SIDE as u64) as u32;
    let mut h = session(area, None)?;
    h.open(Path::new(path)).map_err(|e| format!("photo.render: {e}"))?;
    fence_links(&h, "photo.render")?;
    let png = h.render_png(None, max_side).map_err(|e| format!("photo.render: {e}"))?;
    area.write(&out_abs, &png).map_err(|e| format!("photo.render: {e}"))?;
    Ok(json!({"out": out, "bytes": png.len()}))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 12x8 RGB PNG, two colour bands (written by this test's author
    /// once, embedded so the suite needs no fixtures on disk).
    const PNG: &str = "89504e470d0a1a0a0000000d494844520000000c000000080802000000428689a60000001d49444154789c6378616383866c725ea021063a2bb279d14310d15911005b9497817c6155610000000049454e44ae426082";

    fn fixture(dir: &Path) -> String {
        let bytes: Vec<u8> = (0..PNG.len()).step_by(2).map(|i| u8::from_str_radix(&PNG[i..i + 2], 16).unwrap()).collect();
        std::fs::write(dir.join("in.png"), bytes).unwrap();
        "in.png".into()
    }

    #[test]
    fn info_convert_run_render_on_a_real_png() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);

        let doc = dispatch("info", &json!({"path": input}), host).unwrap();
        assert_eq!(doc["width"], json!(12), "{doc}");
        assert_eq!(doc["height"], json!(8));

        let conv = dispatch("convert", &json!({"path": input, "out": "out.webp"}), host).unwrap();
        assert_eq!(conv["out"], json!("out.webp"));
        assert!(host.join("out.webp").metadata().unwrap().len() > 0);

        let ran = dispatch(
            "run",
            &json!({"path": input, "cmds": [{"id": "filter.blur.gaussianBlur", "params": {"radius": 2.0}}], "out": "blurred.png"}),
            host,
        )
        .unwrap();
        assert_eq!(ran["results"][0]["id"], json!("filter.blur.gaussianBlur"), "{ran}");
        assert!(host.join("blurred.png").metadata().unwrap().len() > 0);

        let rend = dispatch("render", &json!({"path": input, "out": "prev.png", "max_side": 64}), host).unwrap();
        assert!(rend["bytes"].as_u64().unwrap() > 0);

        let cat = commands().unwrap();
        assert!(cat.as_array().map(|a| a.len() > 500).unwrap_or(false) || cat["commands"].as_array().map(|a| a.len() > 500).unwrap_or(false), "a real catalog");
    }

    #[test]
    fn paths_outside_the_workspace_are_refused_by_the_engine() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        for bad in ["../up.png", "/etc/x.png", "a/../../up.png"] {
            assert!(dispatch("info", &json!({"path": bad}), host).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.photos"));
        assert!(!may_call("org.example.app"));
    }

    /// The plug-in registry is process-wide and installs WebAssembly from
    /// in-band data: the door refuses every `plugin.*` id before the engine
    /// sees it (`code`, and `plugin.list` `host`), and the catalog offer
    /// matches the gate.
    #[test]
    fn run_refuses_plugin_commands_and_the_catalog_omits_them() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);
        for (id, class) in [("plugin.install", "code"), ("plugin.run", "code"), ("plugin.list", "host"), ("plugin.remove", "code"), ("plugin.reload", "code")] {
            let r = dispatch("run", &json!({"path": input, "cmds": [{"id": id, "params": {"data": "AGFzbQEAAAA="}}]}), host);
            let e = r.unwrap_err();
            assert!(e.contains(&format!("`{id}` is classed {class}")) && e.contains("never runs it"), "{id}: {e}");
        }
        // A refused id refuses the whole call, even behind an allowed one.
        let r = dispatch("run", &json!({"path": input, "cmds": [{"id": "filter.blur.gaussianBlur", "params": {"radius": 1.0}}, {"id": "plugin.install", "params": {"data": "AGFzbQEAAAA="}}], "out": "never.png"}), host);
        assert!(r.unwrap_err().contains("`plugin.install` is classed code"));
        assert!(!host.join("never.png").exists());
        let cat = commands().unwrap();
        let items = cat.as_array().cloned().or_else(|| cat["commands"].as_array().cloned()).unwrap();
        assert!(items.iter().all(|c| c["id"].as_str().is_some_and(|id| !id.starts_with("plugin"))), "no plugin ids offered");
        assert!(items.len() > 500, "a real catalog remains");
    }

    /// The door runs an allowlisted command in a temporary area and saves
    /// the document to `out`; a query without `out` writes nothing.
    #[test]
    fn the_door_runs_allowlisted_commands_in_its_area() {
        let dir = tempfile::tempdir().unwrap();
        let input = fixture(dir.path());
        let areas = resolver(dir.path(), None);
        let ran = serve(
            &areas,
            &service_call(
                "run",
                json!({"path": input, "cmds": [
                    {"id": "filter.blur.gaussianBlur", "params": {"radius": 2.0}},
                    {"id": "document.inspect"}
                ], "out": "out/blurred.png"}),
                dir.path(),
                false,
            ),
        )
        .unwrap();
        assert_eq!(ran["out"], json!("out/blurred.png"), "{ran}");
        assert_eq!(ran["results"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect::<Vec<_>>(), ["filter.blur.gaussianBlur", "document.inspect"]);
        assert_eq!(ran["results"][1]["result"]["width"], json!(12), "{ran}");
        let blurred = std::fs::read(dir.path().join("out/blurred.png")).unwrap();
        assert!(blurred.starts_with(&[0x89, b'P', b'N', b'G']));
        let plain = serve(&areas, &service_call("convert", json!({"path": input, "out": "out/plain.png"}), dir.path(), false)).unwrap();
        assert_eq!(plain["out"], json!("out/plain.png"));
        assert_ne!(std::fs::read(dir.path().join("out/plain.png")).unwrap(), blurred, "the filter reached the saved document");
        let query = serve(&areas, &service_call("run", json!({"path": input, "cmds": [{"id": "document.inspect"}]}), dir.path(), false)).unwrap();
        assert!(query["out"].is_null() && query["results"][0]["result"]["height"] == json!(8), "{query}");
        assert!(no_staging_left(dir.path()));
        let mut names: Vec<String> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["in.png", "out"]);
    }

    /// Every class but `safe` is refused, and so is an id the classification
    /// does not know, before any command runs: a refused id anywhere in the
    /// list opens and writes nothing. A batch, a script or an action that
    /// wraps allowed commands is still the batch, the script or the action.
    #[test]
    fn the_door_refuses_every_other_class_and_unknown_ids() {
        let dir = tempfile::tempdir().unwrap();
        let input = fixture(dir.path());
        let areas = resolver(dir.path(), None);
        let run = |cmds: Json| serve(&areas, &service_call("run", json!({"path": input, "cmds": cmds, "out": "x.png"}), dir.path(), true));
        let blur = json!({"id": "filter.blur.gaussianBlur", "params": {"radius": 1.0}});
        for (cmd, class) in [
            (json!({"id": "actions.play", "params": {"action": 0}}), "code"),
            (json!({"id": "file.scripts.browse", "params": {"script": "filter.blur.gaussianBlur {\"radius\": 1}"}}), "code"),
            (json!({"id": "file.automate.batch", "params": {"steps": [["filter.blur.gaussianBlur", {"radius": 1.0}]], "input": input, "output": "batch"}}), "code"),
            (json!({"id": "file.automate.runDroplet", "params": {"path": "x.pcdroplet"}}), "code"),
            (json!({"id": "filter.lastFilter"}), "code"),
            (json!({"id": "prefs.set", "params": {"path": "plugIns.useAdditionalPluginsFolder", "value": true}}), "code"),
            (json!({"id": "plugin.install", "params": {"data": "AGFzbQEAAAA="}}), "code"),
            (json!({"id": "prefs.get"}), "host"),
            (json!({"id": "edit.colorSettings", "params": {"workingRgb": "srgb"}}), "host"),
            (json!({"id": "view.proofColors"}), "host"),
            (json!({"id": "file.print", "params": {"dryRun": true}}), "device"),
        ] {
            let id = cmd["id"].as_str().unwrap().to_string();
            let e = run(json!([blur, cmd])).unwrap_err();
            assert!(e.contains(&format!("`{id}` is classed {class}")) && e.contains("never runs it"), "{id}: {e}");
        }
        for (id, params) in [
            ("file.saveACopy", json!({"path": "copy.png"})),
            ("file.placeEmbedded", json!({"path": "in.png"})),
            ("layer.exportAs", json!({"path": "layer.png"})),
            ("image.adjustments.colorLookup", json!({"lut": "none"})),
            ("layer.smartFilter.setParams", json!({"params": {"file": "lut.cube"}})),
        ] {
            let e = run(json!([blur, {"id": id, "params": params}])).unwrap_err();
            assert!(e.contains(&format!("`{id}` reads or writes files")) && e.contains("not reviewed to run through it"), "{id}: {e}");
        }
        let e = run(json!([blur, {"id": "photo.secret"}])).unwrap_err();
        assert!(e.contains("`photo.secret` is not a reviewed photo command"), "{e}");
        // A `safe` id photocraft's own workspace policy refuses is refused
        // before any command runs too.
        let e = run(json!([blur, {"id": "layer.smartObjects.warp", "params": {"style": "arc"}}])).unwrap_err();
        assert!(e.contains("`layer.smartObjects.warp` uses ambient filesystem paths"), "{e}");
        assert!(run(json!([{"id": ""}])).unwrap_err().contains("each command has an `id`"));
        assert!(run(json!([{"id": "filter.blur.gaussianBlur", "params": "radius"}])).unwrap_err().contains("`params` is an object"));
        let too_many: Vec<Json> = (0..65).map(|_| json!({"id": "document.inspect"})).collect();
        assert!(run(Json::Array(too_many)).unwrap_err().contains("at most 64"));
        let mut names: Vec<String> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["in.png"], "nothing opened into a staging folder, nothing written");
    }

    /// The door's gate is built from the generated classification, and the
    /// catalog `photo.commands` offers is what the door and the engine's own
    /// workspace policy both run.
    #[test]
    fn the_door_is_built_from_the_reviewed_classification() {
        let door = door().unwrap();
        for id in ["filter.blur.gaussianBlur", "image.adjustments.levels", "layer.new.layer", "tools.setBrush", "tool.presets.select", "style.presets.apply", "actions.record", "actions.get"] {
            assert!(door.runs(id), "{id}");
        }
        for id in ["actions.play", "file.automate.batch", "file.scripts.browse", "prefs.set", "plugin.install", "layer.smartFilter.setParams", "file.saveACopy", "photo.secret"] {
            assert!(!door.runs(id), "{id}");
        }
        let cat = commands().unwrap();
        let items = cat.as_array().cloned().or_else(|| cat["commands"].as_array().cloned()).unwrap();
        let offered: Vec<&str> = items.iter().filter_map(|c| c["id"].as_str()).collect();
        assert!(offered.iter().all(|id| door.runs(id)), "only ids the door runs are offered");
        assert!(offered.contains(&"filter.blur.gaussianBlur") && offered.contains(&"layer.smartObjects.convertToSmartObject"));
        assert!(!offered.contains(&"layer.smartObjects.warp"), "the engine's workspace policy refuses it whatever its parameters");
        assert!(offered.len() > 600 && offered.len() <= door.runnable().len(), "{} of {}", offered.len(), door.runnable().len());
    }

    /// Without the shell's resolver the engine works in its own area under
    /// the shared host directory, so a written path can never land in
    /// another service's data; it may replace, as before.
    #[test]
    fn the_service_area_is_a_subdirectory_of_the_host_dir() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        std::fs::create_dir_all(host.join("photo")).unwrap();
        let input = fixture(&host.join("photo"));
        for _ in 0..2 {
            serve(&Slot::new(), &service_call("convert", json!({"path": input, "out": "out.png"}), host, false)).unwrap();
        }
        assert!(host.join("photo/out.png").is_file());
        assert!(!host.join("out.png").exists());
    }

    /// A call as App Hub hands it to the service.
    fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("photo.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
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

    fn no_staging_left(root: &Path) -> bool {
        std::fs::read_dir(root).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with(octosense_engine_area::STAGING_PREFIX))
    }

    /// With the shell's resolver every path is relative to the caller's own
    /// folder and stays inside it, through a link too.
    #[test]
    fn the_resolver_root_is_used_and_paths_stay_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let input = fixture(&root);
        let areas = resolver(&root, None);
        let host = dir.path().join(".host");
        let doc = serve(&areas, &service_call("info", json!({"path": input}), &host, false)).unwrap();
        assert_eq!(doc["width"], json!(12), "{doc}");
        std::fs::create_dir(root.join("out")).unwrap();
        serve(&areas, &service_call("convert", json!({"path": input, "out": "out/a.webp"}), &host, false)).unwrap();
        serve(&areas, &service_call("render", json!({"path": input, "out": "out/p.png", "max_side": 8}), &host, false)).unwrap();
        serve(&areas, &service_call("run", json!({"path": input, "cmds": [{"id": "filter.blur.gaussianBlur", "params": {"radius": 1.0}}], "out": "out/b.png"}), &host, false)).unwrap();
        for made in ["out/a.webp", "out/p.png", "out/b.png"] {
            assert!(root.join(made).is_file(), "{made}");
        }
        assert!(!host.exists() && !root.join("photo").exists() && no_staging_left(&root));
        fixture(dir.path());
        for bad in ["../in.png", "/etc/hosts", "out/../../in.png"] {
            assert!(serve(&areas, &service_call("info", json!({"path": bad}), &host, false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("convert", json!({"path": input, "out": bad}), &host, true)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            assert!(serve(&areas, &service_call("info", json!({"path": "up/in.png"}), &host, false)).is_err());
            assert!(serve(&areas, &service_call("render", json!({"path": input, "out": "up/p.png"}), &host, true)).is_err());
            assert!(serve(&areas, &service_call("convert", json!({"path": input, "out": "up/c.png"}), &host, true)).is_err());
            assert!(!dir.path().join("p.png").exists() && !dir.path().join("c.png").exists());
        }
    }

    /// An agent's call never replaces a file; an app's own foreground call
    /// may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        let input = fixture(dir.path());
        let areas = resolver(dir.path(), None);
        std::fs::write(dir.path().join("taken.png"), b"keep me").unwrap();
        for (method, args) in [
            ("convert", json!({"path": input, "out": "taken.png"})),
            ("render", json!({"path": input, "out": "taken.png"})),
            ("run", json!({"path": input, "cmds": [], "out": "taken.png"})),
        ] {
            let refused = serve(&areas, &service_call(method, args, dir.path(), false)).unwrap_err();
            assert!(refused.contains("`taken.png` already exists"), "{method}: {refused}");
        }
        assert_eq!(std::fs::read(dir.path().join("taken.png")).unwrap(), b"keep me");
        assert!(no_staging_left(dir.path()));
        serve(&areas, &service_call("convert", json!({"path": input, "out": "taken.png"}), dir.path(), true)).unwrap();
        assert!(std::fs::read(dir.path().join("taken.png")).unwrap().starts_with(&[0x89, b'P', b'N', b'G']));
    }

    /// What a call writes, the engine's saved document included, must fit
    /// what is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let input = fixture(dir.path());
        let tight = resolver(dir.path(), Some(16));
        for (method, args) in [
            ("convert", json!({"path": input, "out": "c.png"})),
            ("render", json!({"path": input, "out": "r.png"})),
        ] {
            let refused = serve(&tight, &service_call(method, args, dir.path(), true)).unwrap_err();
            assert!(refused.contains("bytes left"), "{method}: {refused}");
        }
        assert!(!dir.path().join("c.png").exists() && !dir.path().join("r.png").exists() && no_staging_left(dir.path()));
    }

    /// What the hostile fixtures' outside file holds: not an image, so the
    /// engine embeds it byte for byte.
    const SECRET: &[u8] = b"TOP-SECRET-MARKER-0123456789";

    /// A `.pcraft` document at `out`: the fixture photo turned into a smart
    /// object whose source is `source`, made and saved by the engine's own
    /// trusted session.
    fn smart_document(dir: &Path, out: &Path, source: photocraft_doc::SmartSource) {
        use photocraft_doc::LayerContent;
        let input = fixture(dir);
        let mut h = Headless::trusted_local();
        h.open(&dir.join(input)).unwrap();
        h.command_run("layer.smartObjects.convertToSmartObject", json!({})).unwrap();
        let d = h.session.active_mut().unwrap();
        let id = d.active_layer.unwrap();
        let doc = Arc::make_mut(&mut d.doc);
        let LayerContent::Smart(sm) = &mut doc.layer_mut(id).unwrap().content else { panic!("not a smart object") };
        sm.source = source;
        h.save(None, Some(out), None, &ExportOptions::default()).unwrap();
    }

    /// A document with a smart object linked to a file outside it is
    /// refused by every method, before the engine could read the link. The
    /// hostile fixture is a real `.pcraft` whose smart object links a file
    /// beside the caller's folder: the engine's own session embeds that
    /// file's raw bytes when it saves the document as a PSD. The same
    /// document nested as another one's embedded smart object is refused
    /// too.
    #[test]
    fn a_smart_object_linked_outside_is_refused() {
        use photocraft_doc::SmartSource;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let secret = dir.path().join("secret.bin");
        std::fs::write(&secret, SECRET).unwrap();
        smart_document(&root, &root.join("hostile.pcraft"), SmartSource::Linked { path: secret.to_string_lossy().into_owned() });
        // The engine's own session reads the link: its PSD carries the
        // outside file's bytes.
        let leak = dir.path().join("leak.psd");
        let mut engine = Headless::trusted_local();
        engine.open(&root.join("hostile.pcraft")).unwrap();
        engine.save(None, Some(&leak), None, &ExportOptions::default()).unwrap();
        let leaked = std::fs::read(&leak).unwrap();
        assert!(leaked.windows(SECRET.len()).any(|w| w == SECRET), "the fixture is live: the engine embeds the linked file");
        // The service refuses it, whatever the method.
        let areas = resolver(&root, None);
        let hostile = std::fs::read(root.join("hostile.pcraft")).unwrap();
        smart_document(&root, &root.join("outer.pcraft"), SmartSource::Embedded { file_name: "inner.pcraft".into(), bytes: Arc::new(hostile) });
        for path in ["hostile.pcraft", "outer.pcraft"] {
            for (method, args) in [
                ("info", json!({"path": path})),
                ("convert", json!({"path": path, "out": "out.psd"})),
                ("render", json!({"path": path, "out": "out.png"})),
                ("run", json!({"path": path, "cmds": [], "out": "run.psd"})),
            ] {
                let refused = serve(&areas, &service_call(method, args, &root, true)).unwrap_err();
                assert!(refused.contains("links a file outside the document") && refused.contains("secret.bin"), "{path} {method}: {refused}");
            }
        }
        for out in ["out.psd", "out.png", "run.psd"] {
            assert!(!root.join(out).exists(), "{out}");
        }
        assert!(no_staging_left(&root));
        // An embedded smart object is the document's own, and opens.
        let png = std::fs::read(root.join("in.png")).unwrap();
        smart_document(&root, &root.join("embedded.pcraft"), SmartSource::Embedded { file_name: "in.png".into(), bytes: Arc::new(png) });
        serve(&areas, &service_call("convert", json!({"path": "embedded.pcraft", "out": "embedded.psd"}), &root, true)).unwrap();
    }

    /// A 2×2×2 `.cube` table that maps every colour to pure green.
    const GREEN_CUBE: &str = "LUT_3D_SIZE 2\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n0 1 0\n";

    /// A `.pcraft` document at `out`: the fixture photo turned into a smart
    /// object with a Color Lookup smart filter of `params` that has not
    /// re-rendered yet, made and saved by the engine's own trusted session.
    fn lookup_document(dir: &Path, out: &Path, params: Json) {
        use photocraft_doc::{BlendMode, LayerContent, SmartFilter};
        let input = fixture(dir);
        let mut h = Headless::trusted_local();
        h.open(&dir.join(input)).unwrap();
        h.command_run("layer.smartObjects.convertToSmartObject", json!({})).unwrap();
        let d = h.session.active_mut().unwrap();
        let id = d.active_layer.unwrap();
        let doc = Arc::make_mut(&mut d.doc);
        let LayerContent::Smart(sm) = &mut doc.layer_mut(id).unwrap().content else { panic!("not a smart object") };
        sm.smart_filters.push(SmartFilter { command: "image.adjustments.colorLookup".into(), params, blend: BlendMode::Normal, opacity: 1.0, visible: true });
        h.save(None, Some(out), None, &ExportOptions::default()).unwrap();
    }

    /// A document whose Color Lookup smart filter names a table file is
    /// refused by every method, before the engine could read it: every
    /// re-render of the smart object reads the file by its own path, wherever
    /// it points. The hostile fixture is live (the engine's own session reads
    /// the outside table on the first re-render), and a table the document
    /// embeds (`data`) opens and runs. The door never lets a command plant
    /// such a file either: `layer.smartFilter.setParams` is classed `file`.
    #[test]
    fn a_color_lookup_smart_filter_naming_a_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let cube = dir.path().join("secret.cube");
        std::fs::write(&cube, GREEN_CUBE).unwrap();
        lookup_document(&root, &root.join("hostile.pcraft"), json!({"file": cube.to_string_lossy()}));
        // The engine's own session reads the outside table when the smart
        // object re-renders; without the file the filter changes nothing.
        let rerendered = || {
            let mut engine = Headless::trusted_local();
            engine.open(&root.join("hostile.pcraft")).unwrap();
            let before = engine.render_png(None, 16).unwrap();
            engine.command_run("layer.smartFilter.setVisible", json!({"visible": true})).unwrap();
            (before, engine.render_png(None, 16).unwrap())
        };
        let (before, after) = rerendered();
        assert_ne!(after, before, "the fixture is live: the re-render reads the outside table");
        std::fs::remove_file(&cube).unwrap();
        let (before, after) = rerendered();
        assert_eq!(after, before, "the change came from the outside file");
        std::fs::write(&cube, GREEN_CUBE).unwrap();
        // The service refuses it, whatever the method, before a command runs.
        let areas = resolver(&root, None);
        for (method, args) in [
            ("info", json!({"path": "hostile.pcraft"})),
            ("convert", json!({"path": "hostile.pcraft", "out": "out.png"})),
            ("render", json!({"path": "hostile.pcraft", "out": "prev.png"})),
            ("run", json!({"path": "hostile.pcraft", "cmds": [{"id": "layer.smartFilter.setVisible", "params": {"visible": true}}], "out": "run.png"})),
        ] {
            let refused = serve(&areas, &service_call(method, args, &root, true)).unwrap_err();
            assert!(refused.contains("Color Lookup smart filter") && refused.contains("secret.cube"), "{method}: {refused}");
        }
        for out in ["out.png", "prev.png", "run.png"] {
            assert!(!root.join(out).exists(), "{out}");
        }
        assert!(no_staging_left(&root));
        // An embedded table is the document's own: it opens, re-renders and saves.
        lookup_document(&root, &root.join("embedded.pcraft"), json!({"data": GREEN_CUBE, "fileName": "green.cube"}));
        let ran = serve(
            &areas,
            &service_call("run", json!({"path": "embedded.pcraft", "cmds": [{"id": "layer.smartFilter.setVisible", "params": {"visible": true}}], "out": "embedded.png"}), &root, false),
        )
        .unwrap();
        assert_eq!(ran["out"], json!("embedded.png"), "{ran}");
        // Planting a file into its filter is refused before anything runs.
        let planted = json!({"id": "layer.smartFilter.setParams", "params": {"params": {"file": cube.to_string_lossy()}}});
        let e = serve(&areas, &service_call("run", json!({"path": "embedded.pcraft", "cmds": [planted], "out": "planted.png"}), &root, false)).unwrap_err();
        assert!(e.contains("`layer.smartFilter.setParams` reads or writes files"), "{e}");
        assert!(!root.join("planted.png").exists() && no_staging_left(&root));
    }

    /// `photos.notify` without the shell's hook refuses rather than
    /// pretending a notice went out; the hook's answer passes through.
    #[test]
    fn the_notify_hook_is_the_shells() {
        on_notify(None);
        let hookless = notifier().lock().unwrap().clone();
        assert!(hookless.is_none());
        on_notify(Some(Arc::new(|app: &str, args: &Json| Ok(json!({"card_id": "n1", "app": app, "title": args["title"]})))));
        let hook = notifier().lock().unwrap().clone().unwrap();
        let out = hook("os.photos", &json!({"title": "Hi", "body": "There"})).unwrap();
        assert_eq!(out["card_id"], json!("n1"));
        assert_eq!(out["app"], json!("os.photos"));
        on_notify(None);
    }
}
