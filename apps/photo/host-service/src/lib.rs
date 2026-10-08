//! `octosense-photo-service` — the `photo` host service (ADR 0013), and
//! the Photos system app's own `photos` service.
//!
//! photocraft's engine through its own headless automation layer. Every
//! call is a fresh, stateless session whose file access is bound to the
//! service's own area under the caller's host directory, `<host dir>/photo`
//! ([`area`]), by photocraft's capability-rooted [`AuthorizedWorkspace`] —
//! paths are relative, and separators, `..`, prefixes and escapes are
//! refused by the engine before any I/O, so the shared host directory's
//! other services (Mail's vaults, Calendar's events) stay out of reach.
//!
//! Methods (all under the `photo` family; paths relative to the area):
//! - `info {path}` → the document inspected as JSON
//! - `convert {path, out, format?}` → `{out, warnings}`
//! - `run {path, cmds: [{id, params?}], out?, format?}` → command results,
//!   saving when `out` is given (the engine's command catalog: crops,
//!   adjustments, filters — `photo.commands` lists it)
//! - `commands {}` → the engine's command catalog
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

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use photocraft_automation::headless::Headless;
use photocraft_automation::workspace::AuthorizedWorkspace;
use photocraft_io::ExportOptions;
use serde_json::{json, Value as Json};

/// The longest preview edge `render` produces.
const MAX_RENDER_SIDE: u32 = 4096;

/// Which apps may call the service: system apps, as News and Sheets.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

/// The engine's own area under the shared host directory. Every path a
/// caller names is contained here, never in the host directory itself.
fn area(host_dir: &Path) -> Result<PathBuf, String> {
    let area = host_dir.join("photo");
    std::fs::create_dir_all(&area).map_err(|e| format!("photo: service area: {e}"))?;
    Ok(area)
}

pub struct PhotoService;

/// Register the `photo` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(PhotoService));
}

impl HostService for PhotoService {
    fn family(&self) -> &'static str {
        "photo"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        if !may_call(&call.app_id) {
            reply.send(Err("The photo service serves system apps only.".into()));
            return;
        }
        let method = call.method().to_string();
        let args = call.args.clone();
        reply.send(area(&call.host_dir).and_then(|area| dispatch(&method, &args, &area)));
    }
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
            "info" => reply.send(area(&call.host_dir).and_then(|area| info(&call.args, &area))),
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

fn session(host_dir: &Path) -> Result<Headless, String> {
    let ws = AuthorizedWorkspace::new(Some(host_dir), Some(host_dir)).map_err(|e| format!("photo: {e}"))?;
    Ok(Headless::with_workspace(ws))
}

fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    match method {
        "info" => info(args, host_dir),
        "convert" => convert(args, host_dir),
        "run" => run(args, host_dir),
        "commands" => commands(),
        "render" => render(args, host_dir),
        other => Err(format!("photo.{other} is not a method of the photo service")),
    }
}

fn arg_path<'a>(args: &'a Json, key: &str) -> Result<&'a str, String> {
    args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("photo: `{key}` is required"))
}

fn info(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let path = arg_path(args, "path")?;
    let mut h = session(host_dir)?;
    let opened = h.open(Path::new(path)).map_err(|e| format!("photo.info: {e}"))?;
    let mut doc = h.inspect(None).map_err(|e| format!("photo.info: {e}"))?;
    doc["warnings"] = opened["warnings"].clone();
    doc["file"] = json!(path);
    Ok(doc)
}

fn convert(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let path = arg_path(args, "path")?;
    let out = arg_path(args, "out")?;
    let format = args["format"].as_str();
    let mut h = session(host_dir)?;
    let opened = h.open(Path::new(path)).map_err(|e| format!("photo.convert: {e}"))?;
    let saved = h
        .save(None, Some(Path::new(out)), format, &ExportOptions::default())
        .map_err(|e| format!("photo.convert: {e}"))?;
    Ok(json!({"out": out, "warnings": [opened["warnings"], saved["warnings"]]}))
}

fn run(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let path = arg_path(args, "path")?;
    let cmds = args["cmds"].as_array().ok_or("photo.run: `cmds` is a list of {id, params?}")?;
    if cmds.len() > 64 {
        return Err("photo.run: at most 64 commands per call".into());
    }
    let mut h = session(host_dir)?;
    h.open(Path::new(path)).map_err(|e| format!("photo.run: {e}"))?;
    let mut results = Vec::new();
    for c in cmds {
        let id = c["id"].as_str().ok_or("photo.run: each command has an `id`")?;
        let params = if c["params"].is_null() { json!({}) } else { c["params"].clone() };
        let r = h.command_run(id, params).map_err(|e| format!("photo.run {id}: {e}"))?;
        results.push(json!({"id": id, "result": r}));
    }
    let saved = match args["out"].as_str() {
        Some(out) if !out.is_empty() => {
            h.save(None, Some(Path::new(out)), args["format"].as_str(), &ExportOptions::default())
                .map_err(|e| format!("photo.run: {e}"))?;
            json!(out)
        }
        _ => Json::Null,
    };
    Ok(json!({"results": results, "out": saved}))
}

fn commands() -> Result<Json, String> {
    Ok(Headless::new().command_list())
}

fn render(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let path = arg_path(args, "path")?;
    let out = arg_path(args, "out")?;
    let max_side = args["max_side"].as_u64().unwrap_or(1024).min(MAX_RENDER_SIDE as u64) as u32;
    let mut h = session(host_dir)?;
    h.open(Path::new(path)).map_err(|e| format!("photo.render: {e}"))?;
    let png = h.render_png(None, max_side).map_err(|e| format!("photo.render: {e}"))?;
    h.write_render(Path::new(out), &png).map_err(|e| format!("photo.render: {e}"))?;
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

    /// The engine works in its own area under the shared host directory,
    /// so a written path can never land in another service's data.
    #[test]
    fn the_service_area_is_a_subdirectory_of_the_host_dir() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let a = area(host).unwrap();
        assert_eq!(a, host.join("photo"));
        assert!(a.is_dir());
        let input = fixture(&a);
        dispatch("convert", &json!({"path": input, "out": "out.png"}), &a).unwrap();
        assert!(host.join("photo/out.png").is_file());
        assert!(!host.join("out.png").exists());
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
