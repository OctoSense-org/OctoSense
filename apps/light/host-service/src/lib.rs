//! `octosense-light-service` — the `light` host service (ADR 0013).
//!
//! lightcraft's RAW develop and catalog engine behind typed `light.*`
//! methods. Every call is a fresh, stateless in-memory session
//! (`Session::new().with_fs()`): the caller's file is imported, inspected
//! or developed, and the session ends with the call — no persistent
//! library. The ground is what `photo.*` (photocraft, raster document
//! editing) does not cover: RAW decode (DNG, CR2/CR3, NEF, ARW, RAF,
//! ORF, RW2, PEF…), parametric develop controls, and EXIF/XMP metadata.
//!
//! Methods (all under the `light` family; paths relative to the family's
//! own area, `<host dir>/light` — the shared `.host` directory also holds
//! Mail's and Calendar's data, so the service never reads or writes
//! outside its subdirectory):
//! - `info {path}` → `{file, photo, exif, xmp}` — the imported photo's
//!   summary plus every EXIF/TIFF/GPS tag and XMP property of the file
//! - `controls {}` → `{controls: [{id, label, min, max, default, …}]}` —
//!   the engine's develop-control catalog (the valid `params` keys)
//! - `develop {path, out, params?, auto?, long_edge?, quality?}` →
//!   `{out, width, height, sidecars}` — develop one photo and export it
//!   (`out`'s extension picks the format: .jpg .png .tif .webp .avif .dng)
//! - `batch {paths, out_dir, format?, params?, auto?, long_edge?,
//!   quality?}` → `{files: […]}` — the same develop applied to each file,
//!   written as `<out_dir>/<stem>.<format>`
//!
//! `params` is a `{control: number}` map of the engine's own control ids
//! (`light.exposure`, `color.vibrance`, `wb.temp`…; `light.controls`
//! lists them), applied through `develop.set`; `auto: true` runs the
//! engine's auto-tone first. The service serves system apps only until
//! ADR 0013's store capability is designed.

use std::path::{Component, Path, PathBuf};

use lightcraft_engine::catalog::PhotoId;
use lightcraft_engine::export::{export_photo, ExportFormat, ExportOptions};
use lightcraft_engine::Session;
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use serde_json::{json, Value as Json};

/// Originals per `batch` call.
const MAX_BATCH_FILES: usize = 16;
/// The largest original the service reads (RAW files are tens of MB).
const MAX_INPUT_BYTES: u64 = 256 << 20;
/// The longest output edge `long_edge` may ask for (the engine's own cap).
const MAX_LONG_EDGE: u64 = 16_384;

/// Which apps may call the service: system apps, as News and Sheets.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct LightService;

/// Register the `light` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(LightService));
}

/// The `light.*` agent tools (ADR 0013, wave 2), in App Hub's `tools.json`
/// shape: the shell declares them for the virtual owner `os.light` and grants
/// the system agent its reviewed share (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`).
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for LightService {
    fn family(&self) -> &'static str {
        "light"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        if !may_call(&call.app_id) {
            reply.send(Err("The light service serves system apps only.".into()));
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
        "controls" => controls(),
        "develop" => develop(args, host_dir),
        "batch" => batch(args, host_dir),
        other => Err(format!("light.{other} is not a method of the light service")),
    }
}

/// The family's own area inside the caller's host directory. The shared
/// `.host` holds Mail's and Calendar's data too, so everything `light.*`
/// touches stays under `<host_dir>/light`.
fn area(host_dir: &Path) -> Result<PathBuf, String> {
    let dir = host_dir.join("light");
    std::fs::create_dir_all(&dir).map_err(|e| format!("light: {e}"))?;
    Ok(dir)
}

/// A path strictly inside the family area: relative, no `..`, no absolute
/// component; the resolved parent must stay under the area even through
/// symlinks.
fn contained_path(dir: &Path, rel: &str, method: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("light.{method}: the path is required"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("light.{method}: `{rel}` leaves the app's light area"));
    }
    let joined = dir.join(rel_path);
    let check_root = dir.canonicalize().map_err(|e| format!("light.{method}: light area: {e}"))?;
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
    let resolved = deepest.canonicalize().map_err(|e| format!("light.{method}: {e}"))?;
    if !resolved.starts_with(&check_root) {
        return Err(format!("light.{method}: `{rel}` leaves the app's light area"));
    }
    Ok(joined)
}

fn arg_str<'a>(args: &'a Json, key: &str, method: &str) -> Result<&'a str, String> {
    args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("light.{method}: `{key}` is required"))
}

/// Import one original into the session and make it the active photo.
fn import(s: &mut Session, abs: &Path, rel: &str, method: &str) -> Result<PhotoId, String> {
    let meta = std::fs::metadata(abs).map_err(|e| format!("light.{method}: `{rel}`: {e}"))?;
    if meta.len() > MAX_INPUT_BYTES {
        return Err(format!("light.{method}: `{rel}` is larger than the service reads"));
    }
    let r = s
        .execute("library.import", &json!({"paths": [abs.to_string_lossy()]}))
        .map_err(|e| format!("light.{method}: {e}"))?;
    // The same bytes twice in one call: the engine dedupes and names the
    // photo it already has.
    let id = match r["imported"][0].as_u64().or_else(|| r["duplicates"][0]["existing"].as_u64()) {
        Some(id) => id,
        None => {
            let why = r["failed"][0][1].as_str().unwrap_or("not a readable photo");
            return Err(format!("light.{method}: `{rel}`: {why}"));
        }
    };
    s.execute("library.select", &json!({"ids": [id], "active": id})).map_err(|e| format!("light.{method}: {e}"))?;
    Ok(PhotoId(id))
}

fn info(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let dir = area(host_dir)?;
    let rel = arg_str(args, "path", "info")?;
    let path = contained_path(&dir, rel, "info")?;
    let mut s = Session::new().with_fs();
    let id = import(&mut s, &path, rel, "info")?;
    let mut photo = s.execute("photo.inspect", &json!({"id": id.0})).map_err(|e| format!("light.info: {e}"))?;
    if let Some(o) = photo.as_object_mut() {
        // the photo's place on disk is the caller's `path`, not the host's
        o.remove("source");
    }
    let meta = s.execute("photo.allMetadata", &json!({"id": id.0})).map_err(|e| format!("light.info: {e}"))?;
    Ok(json!({"file": rel, "photo": photo, "exif": meta["exif"], "xmp": meta["xmp"]}))
}

fn controls() -> Result<Json, String> {
    let mut s = Session::new();
    let v = s.execute("develop.controls", &json!({})).map_err(|e| format!("light.controls: {e}"))?;
    Ok(json!({"controls": v}))
}

/// Apply the call's develop parameters to the active photo: `auto` first,
/// then `params` through `develop.set` (the engine validates control ids).
fn apply_params(s: &mut Session, args: &Json, method: &str) -> Result<(), String> {
    if args["auto"].as_bool().unwrap_or(false) {
        s.execute("develop.auto", &json!({})).map_err(|e| format!("light.{method}: {e}"))?;
    }
    match &args["params"] {
        Json::Null => Ok(()),
        Json::Object(map) if map.is_empty() => Ok(()),
        Json::Object(_) => s
            .execute("develop.set", &json!({"values": args["params"]}))
            .map(|_| ())
            .map_err(|e| format!("light.{method}: {e}")),
        _ => Err(format!("light.{method}: `params` is a {{control: number}} map — see light.controls")),
    }
}

/// Export the active photo to `out_abs` (the format from its extension)
/// and write the bytes — never over an imported original.
fn export_one(s: &mut Session, id: PhotoId, out_abs: &Path, out_rel: &str, args: &Json, method: &str) -> Result<Json, String> {
    let quality = args["quality"].as_u64().unwrap_or(92).clamp(1, 100);
    let mut p = json!({"quality": quality});
    if let Some(n) = args["long_edge"].as_u64() {
        p["longEdge"] = json!(n.clamp(16, MAX_LONG_EDGE));
    }
    let ext = out_abs.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
    let format = ExportFormat::parse(&ext)
        .ok_or_else(|| format!("light.{method}: `{out_rel}`: unknown extension (use .jpg, .png, .tif, .webp, .avif or .dng)"))?;
    let mut o = ExportOptions::from_json(&p);
    o.format = format;
    let guard = s.original_guard();
    guard.check(out_abs).map_err(|e| format!("light.{method}: {e}"))?;
    let e = export_photo(s, id, &o, 1).map_err(|e| format!("light.{method}: {e}"))?;
    if let Some(parent) = out_abs.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("light.{method}: {e}"))?;
    }
    std::fs::write(out_abs, &e.bytes).map_err(|e| format!("light.{method}: {e}"))?;
    let mut sidecars = Vec::new();
    for (sc_ext, bytes) in &e.sidecars {
        let sc = out_abs.with_extension(sc_ext);
        guard.check(&sc).map_err(|e| format!("light.{method}: {e}"))?;
        std::fs::write(&sc, bytes).map_err(|e| format!("light.{method}: {e}"))?;
        sidecars.push(Path::new(out_rel).with_extension(sc_ext).to_string_lossy().to_string());
    }
    Ok(json!({"out": out_rel, "width": e.width, "height": e.height, "sidecars": sidecars}))
}

fn develop(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let dir = area(host_dir)?;
    let rel = arg_str(args, "path", "develop")?;
    let out_rel = arg_str(args, "out", "develop")?;
    let input = contained_path(&dir, rel, "develop")?;
    let out = contained_path(&dir, out_rel, "develop")?;
    let mut s = Session::new().with_fs();
    let id = import(&mut s, &input, rel, "develop")?;
    apply_params(&mut s, args, "develop")?;
    export_one(&mut s, id, &out, out_rel, args, "develop")
}

fn batch(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let dir = area(host_dir)?;
    let paths = args["paths"].as_array().ok_or("light.batch: `paths` is a list of files")?;
    if paths.is_empty() || paths.len() > MAX_BATCH_FILES {
        return Err(format!("light.batch: `paths` is 1..={MAX_BATCH_FILES} files"));
    }
    let out_dir = arg_str(args, "out_dir", "batch")?;
    let format = args["format"].as_str().unwrap_or("jpg");
    ExportFormat::parse(format)
        .ok_or_else(|| format!("light.batch: `{format}` is not an export format (jpg, png, tif, webp, avif or dng)"))?;
    let mut s = Session::new().with_fs();
    let mut files = Vec::new();
    let mut stems: Vec<String> = Vec::new();
    for p in paths {
        let rel = p.as_str().filter(|s| !s.is_empty()).ok_or("light.batch: each of `paths` is a file path")?;
        let input = contained_path(&dir, rel, "batch")?;
        let stem = input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("light.batch: `{rel}` has no file name"))?;
        if stems.contains(&stem) {
            return Err(format!("light.batch: two of `paths` would both write `{out_dir}/{stem}.{format}`"));
        }
        stems.push(stem.clone());
        let out_rel = format!("{out_dir}/{stem}.{format}");
        let out = contained_path(&dir, &out_rel, "batch")?;
        let id = import(&mut s, &input, rel, "batch")?;
        apply_params(&mut s, args, "batch")?;
        let mut one = export_one(&mut s, id, &out, &out_rel, args, "batch")?;
        one["path"] = json!(rel);
        files.push(one);
    }
    Ok(json!({"files": files}))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 12x8 RGB PNG, two colour bands (written by this test's author
    /// once, embedded so the suite needs no fixtures on disk).
    const PNG: &str = "89504e470d0a1a0a0000000d494844520000000c000000080802000000428689a60000001d49444154789c6378616383866c725ea021063a2bb279d14310d15911005b9497817c6155610000000049454e44ae426082";

    fn png_bytes() -> Vec<u8> {
        (0..PNG.len()).step_by(2).map(|i| u8::from_str_radix(&PNG[i..i + 2], 16).unwrap()).collect()
    }

    /// Write a fixture into the family area, as a caller's earlier export
    /// or the files host tools would have.
    fn place(host: &Path, name: &str, bytes: &[u8]) -> String {
        let dir = host.join("light");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), bytes).unwrap();
        name.into()
    }

    /// A tiny real linear DNG from the engine's own synthetic-scene
    /// generator: the RAW path with no media file checked in.
    fn dng_bytes() -> Vec<u8> {
        lightcraft_merge::synth::bracket_dngs(96, 64, &[0.0]).unwrap().remove(0)
    }

    #[test]
    fn area_is_the_family_subdir_and_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let a = area(host).unwrap();
        assert_eq!(a, host.join("light"));
        assert!(a.is_dir(), "created on first use");
    }

    #[test]
    fn info_reads_a_png_and_its_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = place(host, "in.png", &png_bytes());

        let got = dispatch("info", &json!({"path": input}), host).unwrap();
        assert_eq!(got["file"], json!("in.png"));
        assert_eq!(got["photo"]["width"], json!(12), "{got}");
        assert_eq!(got["photo"]["height"], json!(8));
        assert!(got["photo"].get("source").is_none(), "host paths stay the host's");
        assert!(got["exif"].is_array() && got["xmp"].is_array(), "{got}");
    }

    #[test]
    fn controls_lists_the_develop_catalog() {
        let got = dispatch("controls", &json!({}), Path::new("/")).unwrap();
        let list = got["controls"].as_array().unwrap();
        assert!(list.len() > 20, "a real catalog, not a stub: {}", list.len());
        let exposure = list.iter().find(|c| c["id"] == json!("light.exposure")).expect("light.exposure");
        assert!(exposure["min"].is_number() && exposure["max"].is_number() && exposure["default"].is_number());
    }

    #[test]
    fn develop_applies_params_and_exports() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = place(host, "in.png", &png_bytes());

        let plain = dispatch("develop", &json!({"path": input, "out": "plain.jpg"}), host).unwrap();
        assert_eq!(plain["out"], json!("plain.jpg"));
        assert_eq!(plain["width"], json!(12), "{plain}");
        assert_eq!(plain["height"], json!(8));
        let brightened = dispatch(
            "develop",
            &json!({"path": input, "out": "bright.jpg", "params": {"light.exposure": 1.5}}),
            host,
        )
        .unwrap();
        let a = std::fs::read(host.join("light/plain.jpg")).unwrap();
        let b = std::fs::read(host.join("light/bright.jpg")).unwrap();
        assert!(!a.is_empty() && !b.is_empty(), "{brightened}");
        assert_ne!(a, b, "the exposure parameter reached the render");

        let bad = dispatch("develop", &json!({"path": input, "out": "x.jpg", "params": {"nonsense": 1.0}}), host);
        assert!(bad.unwrap_err().contains("nonsense"), "the engine validates control ids");
    }

    #[test]
    fn develop_decodes_a_real_raw_dng() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = place(host, "shot.dng", &dng_bytes());

        let got = dispatch("info", &json!({"path": input}), host).unwrap();
        assert_eq!(got["photo"]["width"], json!(96), "{got}");
        assert_eq!(got["photo"]["height"], json!(64));
        assert!(!got["exif"].as_array().unwrap().is_empty(), "a DNG carries EXIF: {got}");

        let dev = dispatch(
            "develop",
            &json!({"path": input, "out": "shot.jpg", "params": {"light.exposure": 0.5}, "long_edge": 48}),
            host,
        )
        .unwrap();
        assert_eq!(dev["out"], json!("shot.jpg"));
        assert!(dev["width"].as_u64().unwrap() <= 48, "long_edge caps the output: {dev}");
        assert!(host.join("light/shot.jpg").metadata().unwrap().len() > 0);
    }

    #[test]
    fn batch_develops_each_file_into_the_out_dir() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let a = place(host, "a.png", &png_bytes());
        let b = place(host, "b.dng", &dng_bytes());

        let got = dispatch(
            "batch",
            &json!({"paths": [a, b], "out_dir": "developed", "params": {"light.contrast": 30.0}}),
            host,
        )
        .unwrap();
        let files = got["files"].as_array().unwrap();
        assert_eq!(files.len(), 2, "{got}");
        assert_eq!(files[0]["out"], json!("developed/a.jpg"));
        assert!(host.join("light/developed/a.jpg").metadata().unwrap().len() > 0);
        assert!(host.join("light/developed/b.jpg").metadata().unwrap().len() > 0);

        // The same bytes under another name: the engine dedupes the import
        // and the service still develops both outputs.
        let c = place(host, "c.png", &png_bytes());
        let deduped = dispatch("batch", &json!({"paths": [a, c], "out_dir": "dedup"}), host).unwrap();
        assert_eq!(deduped["files"].as_array().unwrap().len(), 2, "{deduped}");
        assert!(host.join("light/dedup/c.jpg").metadata().unwrap().len() > 0);

        let dup = dispatch("batch", &json!({"paths": ["a.png", "sub/a.png"], "out_dir": "d2"}), host);
        assert!(dup.unwrap_err().contains("both write"), "no two inputs may write the same output");
        let too_many: Vec<String> = (0..MAX_BATCH_FILES + 1).map(|i| format!("f{i}.png")).collect();
        let cap = dispatch("batch", &json!({"paths": too_many, "out_dir": "d3"}), host);
        assert!(cap.unwrap_err().contains("1..="), "the batch cap holds");
    }

    #[test]
    fn paths_stay_inside_the_light_area() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        // a sibling of the area: Mail's data in the shared `.host`
        std::fs::write(host.join("accounts.json"), b"{}").unwrap();
        let input = place(host, "in.png", &png_bytes());

        for bad in ["../accounts.json", "/etc/x.png", "a/../../up.png", ""] {
            assert!(dispatch("info", &json!({"path": bad}), host).is_err(), "{bad}");
            assert!(dispatch("develop", &json!({"path": input, "out": bad}), host).is_err(), "{bad}");
            assert!(dispatch("batch", &json!({"paths": [bad], "out_dir": "d"}), host).is_err(), "{bad}");
            assert!(dispatch("batch", &json!({"paths": [&input], "out_dir": bad}), host).is_err(), "{bad}");
        }
        // nothing leaked beside the area and Mail's file
        let mut names: Vec<String> =
            std::fs::read_dir(host).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        names.sort();
        assert_eq!(names, vec!["accounts.json".to_string(), "light".to_string()]);
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.photos"));
        assert!(may_call("os.sheets"));
        assert!(!may_call("org.example.app"));
        assert!(!may_call(""));
    }

    /// The agent tools (`tools.json`) pass App Hub's own loader, as the shell
    /// reads them, keep the object schemas octos takes both ways, and name
    /// only methods this service dispatches.
    #[test]
    fn the_agent_tools_pass_app_hubs_loader_and_name_real_methods() {
        use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest};
        let (manifest, _) = ToolManifest::load(TOOLS_JSON, "light", ToolHost::Contained, false).unwrap();
        assert!(!manifest.tools.is_empty());
        let dir = tempfile::tempdir().unwrap();
        for tool in &manifest.tools {
            assert_eq!(tool.implemented_by, ImplementedBy::HostService, "{}", tool.name);
            assert!(tool.host_method.is_none(), "{}: the shell routes each tool to the method of its own name", tool.name);
            for schema in [&tool.input_schema, &tool.output_schema] {
                assert_eq!(schema["type"], json!("object"), "{}: octos takes object schemas only", tool.name);
            }
            assert!(tool.description.is_ascii(), "{}: descriptions stay within octos's byte limit", tool.name);
            let method = tool.name.strip_prefix("light.").unwrap();
            if let Err(e) = dispatch(method, &json!({}), dir.path()) {
                assert!(!e.contains("is not a method"), "{}: {e}", tool.name);
            }
        }
    }
}
