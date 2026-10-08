//! `octosense-vector-service` — the `vector` host service (ADR 0013).
//!
//! vectorcraft's engine through its own single command entry point
//! ([`Session::execute`] — the same door its UI, CLI and automation server
//! go through). Every call is a fresh, stateless session. Unlike
//! photocraft, the engine does not contain file paths itself, so the
//! service does: every path is resolved inside the service's own
//! `vector/` area under the caller's host directory before the engine
//! sees it (the shared `.host` directory also holds Mail's and Calendar's
//! data), and `run` refuses the engine's own file commands.
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

use std::path::{Component, Path, PathBuf};

use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
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

impl HostService for VectorService {
    fn family(&self) -> &'static str {
        "vector"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        if !may_call(&call.app_id) {
            reply.send(Err("The vector service serves system apps only.".into()));
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
        "convert" => convert(args, host_dir),
        "run" => run(args, host_dir),
        "commands" => commands(),
        "render" => render(args, host_dir),
        other => Err(format!("vector.{other} is not a method of the vector service")),
    }
}

/// The service's own area under the caller's host directory:
/// `<host_dir>/vector`, created on first use. The shared `.host` directory
/// also holds other services' data (Mail accounts, Calendar events), so
/// every vector path stays inside this subdirectory.
fn area(host_dir: &Path) -> Result<PathBuf, String> {
    let dir = host_dir.join("vector");
    std::fs::create_dir_all(&dir).map_err(|e| format!("vector: area: {e}"))?;
    Ok(dir)
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
        return Err("vector: paths stay inside the service's area of the app's host directory".into());
    }
    let joined = area.join(rel_path);
    let check_root = area.canonicalize().map_err(|e| format!("vector: area: {e}"))?;
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
        return Err("vector: paths stay inside the service's area of the app's host directory".into());
    }
    Ok(joined)
}

/// A contained output path with its parent directories in place.
fn out_path(area: &Path, rel: &str) -> Result<PathBuf, String> {
    let path = contained_path(area, rel)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("vector: {e}"))?;
    }
    Ok(path)
}

fn arg_str<'a>(args: &'a Json, key: &str) -> Result<&'a str, String> {
    args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("vector: `{key}` is required"))
}

fn utf8(path: &Path) -> Result<&str, String> {
    path.to_str().ok_or_else(|| "vector: the host directory is not UTF-8".into())
}

/// Open `rel` (contained, size-capped) into the session; the engine's
/// result carries the import warnings.
fn open(session: &mut Session, area: &Path, rel: &str, method: &str) -> Result<Json, String> {
    let path = contained_path(area, rel)?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("vector.{method}: {rel}: {e}"))?;
    if meta.len() > MAX_OPEN_BYTES {
        return Err(format!("vector.{method}: the file is larger than the service opens"));
    }
    session.execute("document.open", &json!({"path": utf8(&path)?})).map_err(|e| format!("vector.{method}: {e}"))
}

/// `document.export` to a contained path, the result's own paths made
/// relative again (a multi-artboard SVG export writes `{stem}-{n}.svg`
/// siblings; they stay inside the area by construction).
fn export(session: &mut Session, area: &Path, args: &Json, method: &str) -> Result<Json, String> {
    let out_rel = arg_str(args, "out")?;
    let out = out_path(area, out_rel)?;
    let mut params = json!({"path": utf8(&out)?});
    if let Some(f) = args["format"].as_str() {
        params["format"] = json!(f);
    }
    if let Some(s) = args["scale"].as_f64() {
        params["scale"] = json!(s.clamp(0.01, 16.0));
    }
    let saved = session.execute("document.export", &params).map_err(|e| format!("vector.{method}: {e}"))?;
    let rel_files = saved["files"].as_array().map(|files| {
        files
            .iter()
            .filter_map(Json::as_str)
            .map(|f| json!(Path::new(f).strip_prefix(area).ok().and_then(Path::to_str).unwrap_or(f)))
            .collect::<Vec<_>>()
    });
    let mut v = json!({"out": out_rel, "format": saved["format"], "bytes": saved["bytes"], "warnings": saved["warnings"]});
    if let Some(files) = rel_files {
        v["files"] = Json::Array(files);
    }
    Ok(v)
}

fn info(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let rel = arg_str(args, "path")?;
    let area = area(host_dir)?;
    let mut s = Session::new();
    let opened = open(&mut s, &area, rel, "info")?;
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

fn convert(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let rel = arg_str(args, "path")?;
    let area = area(host_dir)?;
    let mut s = Session::new();
    let opened = open(&mut s, &area, rel, "convert")?;
    let mut v = export(&mut s, &area, args, "convert")?;
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
fn callable(id: &str, params: &Json) -> Result<(), String> {
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

fn run(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let cmds = args["cmds"].as_array().ok_or("vector.run: `cmds` is a list of {id, params?}")?;
    if cmds.len() > MAX_RUN_CMDS {
        return Err(format!("vector.run: at most {MAX_RUN_CMDS} commands per call"));
    }
    let area = area(host_dir)?;
    let mut s = Session::new();
    match args["path"].as_str() {
        Some(rel) if !rel.is_empty() => {
            open(&mut s, &area, rel, "run")?;
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
    let saved = match args["out"].as_str() {
        Some(out) if !out.is_empty() => export(&mut s, &area, args, "run")?,
        _ => Json::Null,
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

fn render(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let rel = arg_str(args, "path")?;
    let out_rel = arg_str(args, "out")?;
    let max_side = args["max_side"].as_u64().unwrap_or(1024).clamp(1, MAX_RENDER_SIDE as u64) as u32;
    let area = area(host_dir)?;
    let out = out_path(&area, out_rel)?;
    let mut s = Session::new();
    open(&mut s, &area, rel, "render")?;
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
    std::fs::write(&out, &png).map_err(|e| format!("vector.render: {e}"))?;
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
        let area = area(host).unwrap();
        std::fs::write(area.join("in.svg"), SVG).unwrap();
        "in.svg".into()
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
        // Everything lands under `<host>/vector`, nothing in the shared root.
        dispatch("convert", &json!({"path": input, "out": "flat.png"}), host).unwrap();
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
}
