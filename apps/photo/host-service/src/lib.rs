//! `octosense-photo-service` — the `photo` host service (ADR 0013).
//!
//! photocraft's engine through its own headless automation layer. Every
//! call is a fresh, stateless session whose file access is bound to the
//! caller's host directory by photocraft's capability-rooted
//! [`AuthorizedWorkspace`] — paths are relative, and separators, `..`,
//! prefixes and escapes are refused by the engine before any I/O.
//!
//! Methods (all under the `photo` family; paths relative to the host dir):
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

use std::path::Path;

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
        let host_dir = call.host_dir.clone();
        reply.send(dispatch(&method, &args, &host_dir));
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
}
