//! `octosense-effect-service` — the `effect` host service (ADR 0013).
//!
//! effectcraft's motion-graphics engine (an After Effects-class compositor:
//! compositions, layers, keyframes, 300+ effects, expressions, Lottie)
//! through its headless automation backend. Every call is a fresh,
//! stateless session driven by the same command registry every effectcraft
//! frontend dispatches through.
//!
//! Methods (all under the `effect` family; paths relative to the family's
//! area, `<host_dir>/effect`):
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
//! Containment: the shared `.host` directory holds Mail's and Calendar's
//! data, so every effect file lives under the family's own subdirectory,
//! [`area`]. Boundary paths are validated before any I/O, and the engine
//! side is gated too: its file I/O ([`Services`]), media probing
//! ([`Importer`]) and footage decoding (`FootageSource`) each re-check
//! their paths against the area, so a project that references footage
//! outside it renders placeholders instead of reading it. The service
//! serves system apps only until ADR 0013's store capability is designed.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use effectcraft_automation::Backend;
use effectcraft_engine::project::{Footage, ItemId};
use effectcraft_engine::raster::{AuxChannels, Image};
use effectcraft_engine::render::FootageSource;
use effectcraft_engine::time::Tick;
use effectcraft_engine::{Importer, Services, Session};
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
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
        if !may_call(&call.app_id) {
            reply.send(Err("The effect service serves system apps only.".into()));
            return;
        }
        let method = call.method().to_string();
        let args = call.args.clone();
        let host_dir = call.host_dir.clone();
        reply.send(dispatch(&method, &args, &host_dir));
    }
}

fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    let area = area(host_dir)?;
    match method {
        "info" => info(args, &area),
        "render" => render(args, &area),
        "run" => run(args, &area),
        "commands" => commands(args, &area),
        "export_lottie" => export_lottie(args, &area),
        "import_lottie" => import_lottie(args, &area),
        other => Err(format!("effect.{other} is not a method of the effect service")),
    }
}

/// The family's own subdirectory of the caller's host directory, created
/// on first use: the shared `.host` holds Mail's and Calendar's data, so
/// everything the effect service touches lives under `<host_dir>/effect`.
fn area(host_dir: &Path) -> Result<PathBuf, String> {
    let dir = host_dir.join("effect");
    std::fs::create_dir_all(&dir).map_err(|e| format!("effect: {e}"))?;
    Ok(dir)
}

/// A path strictly inside the effect area: relative, normal components
/// only, and resolved (through its deepest existing ancestor, so symlinks
/// cannot escape) under the canonical area.
fn contained(area: &Path, key: &str, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("effect: `{key}` is required"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("effect: `{key}` stays inside the app's effect area"));
    }
    let joined = area.join(rel_path);
    let guard = Guard::new(area).map_err(|e| format!("effect: {e}"))?;
    guard.allowed(&joined.to_string_lossy()).map_err(|e| format!("effect: `{key}`: {e}"))?;
    Ok(joined)
}

/// The engine-side gate (installed as the session's [`Services`]): every
/// read and write the engine performs — `file.open`, `file.saveAs`, the
/// Lottie commands and their extracted assets — re-checks that its path
/// resolves inside the area.
struct Guard {
    /// The canonical effect area.
    root: PathBuf,
}

impl Guard {
    fn new(area: &Path) -> std::io::Result<Self> {
        Ok(Guard { root: area.canonicalize()? })
    }

    /// The path, if it resolves (through its deepest existing ancestor)
    /// inside the area.
    fn allowed(&self, path: &str) -> std::io::Result<PathBuf> {
        let outside = || std::io::Error::new(std::io::ErrorKind::PermissionDenied, "the path is outside the app's effect area");
        let p = Path::new(path);
        let mut deepest = p.to_path_buf();
        while !deepest.exists() {
            match deepest.parent() {
                Some(parent) => deepest = parent.to_path_buf(),
                None => return Err(outside()),
            }
        }
        if !deepest.canonicalize()?.starts_with(&self.root) {
            return Err(outside());
        }
        Ok(p.to_path_buf())
    }
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
        if let Some(d) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(p, data)
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
        effectcraft_media::probe(path).map_err(|e| e.to_string())
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
        self.guard.allowed(&footage.path).is_ok()
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

    fn model(&self, item: ItemId, footage: &Footage) -> Option<Arc<effectcraft_model::Model>> {
        if !self.inside(footage) {
            return None;
        }
        FootageSource::model(&self.pool, item, footage)
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
fn session(area: &Path) -> Result<Backend, String> {
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

/// Open `args.path` (relative to the area) in the session.
fn open(b: &mut Backend, area: &Path, args: &Json, method: &str) -> Result<String, String> {
    let rel = args["path"].as_str().unwrap_or("").to_string();
    let abs = contained(area, "path", &rel)?;
    b.exec("file.open", json!({"path": abs.to_string_lossy()})).map_err(|e| format!("effect.{method}: {e}"))?;
    Ok(rel)
}

fn info(args: &Json, area: &Path) -> Result<Json, String> {
    let mut b = session(area)?;
    let rel = open(&mut b, area, args, "info")?;
    let mut sum = b.exec("project.summary", json!({})).map_err(|e| format!("effect.info: {e}"))?;
    sum["path"] = json!(rel);
    Ok(sum)
}

fn render(args: &Json, area: &Path) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    let out = contained(area, "out", out_rel)?;
    let requested = args["max_side"].as_u64().unwrap_or(1024);
    let max_side = if requested == 0 { MAX_RENDER_SIDE } else { requested.min(MAX_RENDER_SIDE as u64) as u32 };
    let mut b = session(area)?;
    open(&mut b, area, args, "render")?;
    let comp = args.get("comp").filter(|c| !c.is_null());
    let transparent = args["transparent"].as_bool().unwrap_or(false);
    let frame = b.render_with(comp, args["time"].as_f64(), max_side, transparent).map_err(|e| format!("effect.render: {e}"))?;
    if let Some(d) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(d).map_err(|e| format!("effect.render: {e}"))?;
    }
    std::fs::write(&out, &frame.png).map_err(|e| format!("effect.render: {e}"))?;
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
/// `plugin.*`. Listing installed plug-ins stays readable.
fn callable(id: &str) -> Result<(), String> {
    if id.starts_with("effect.plugins.") && id != "effect.plugins.list" {
        return Err(format!("effect.run: `{id}` is not available through the effect service"));
    }
    Ok(())
}

fn run(args: &Json, area: &Path) -> Result<Json, String> {
    let cmds = args["cmds"].as_array().ok_or("effect.run: `cmds` is a list of {id, params?}")?;
    if cmds.len() > MAX_CMDS {
        return Err(format!("effect.run: at most {MAX_CMDS} commands per call"));
    }
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
        results.push(json!({"id": id, "result": r}));
    }
    let saved = match args["out"].as_str() {
        Some(out) if !out.is_empty() => {
            let abs = contained(area, "out", out)?;
            b.exec("file.saveAs", json!({"path": abs.to_string_lossy()})).map_err(|e| format!("effect.run: {e}"))?;
            json!(out)
        }
        _ => Json::Null,
    };
    Ok(json!({"results": results, "out": saved}))
}

fn commands(args: &Json, area: &Path) -> Result<Json, String> {
    let mut b = session(area)?;
    b.exec("command.list", json!({"filter": args["filter"]})).map_err(|e| format!("effect.commands: {e}"))
}

fn export_lottie(args: &Json, area: &Path) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    let out = contained(area, "out", out_rel)?;
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

fn import_lottie(args: &Json, area: &Path) -> Result<Json, String> {
    let rel = args["path"].as_str().unwrap_or("");
    let lottie = contained(area, "path", rel)?;
    let out_rel = args["out"].as_str().unwrap_or("");
    let out = contained(area, "out", out_rel)?;
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
        let a = area(host).unwrap();
        assert_eq!(a, host.join("effect"));
        assert!(a.is_dir());
        assert_eq!(area(host).unwrap(), a, "idempotent");
        fixture(host);
        assert!(host.join("effect/main.ecproj").is_file(), "files land inside the area");
        assert!(!host.join("main.ecproj").exists(), "and not beside Mail's and Calendar's data");
    }

    #[test]
    /// Plug-in loading mutates a process-wide registry with WebAssembly:
    /// `run` refuses every `effect.plugins.*` mutator before the engine
    /// sees it; only the read-only list stays.
    #[test]
    fn run_refuses_plugin_mutators() {
        let dir = tempfile::tempdir().unwrap();
        let area = dir.path();
        for id in ["effect.plugins.load", "effect.plugins.unload", "effect.plugins.reload"] {
            let r = run(&json!({"cmds": [{"id": id, "params": {"path": "x.wasm"}}]}), area);
            let e = r.unwrap_err();
            assert!(e.contains("not available"), "{id}: {e}");
        }
        assert!(callable("effect.plugins.list").is_ok());
        assert!(callable("comp.new").is_ok());
    }

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
