//! `octosense-cad-service` — the `cad` host service (ADR 0013).
//!
//! cadcraft's drafting engine behind typed `cad.*` methods. Every call is
//! a fresh, stateless session: the drawing is read from the call's area,
//! inspected, measured, rendered or converted through the engine, and the
//! reply is JSON — the engine's types never cross the boundary. The service
//! itself does all file I/O through `cadcraft_io`'s byte codecs: the
//! engine's own file commands are never run, so nothing in the engine can
//! touch a path this crate did not resolve. A path written inside a drawing
//! (an XREF, an IMAGE's raster file) is dropped by the reader and never
//! followed.
//!
//! **Where a call works** (ADR 0013, 2026-10-08): the caller's own folder,
//! the [`Area`] the shell's resolver gives it ([`set_area_resolver`]), or
//! without one the legacy `<host dir>/cad`. A write that may not replace (an
//! agent's) only creates new files, within the area's quota
//! ([`Area::write`]).
//!
//! Methods (all under the `cad` family; paths relative to the call's area):
//! - `info {path}` → the drawing inspected as JSON (layers, blocks,
//!   layouts, entity counts, extents)
//! - `entities {path, type?, layer?, limit?, offset?}` → `{count, entities}`
//! - `measure {path, dist: {p1, p2} | area: {points | handle}}` → the
//!   engine's DIST or AREA result
//! - `render {path, out, max_side?}` → the model space as PNG or SVG
//!   (by `out`'s extension), fitted to the drawing's extents
//! - `convert {path, out, format?}` → DXF, DWG, SVG, PNG or PDF
//! - `run {path?, cmds: [{id, params?}], out?, format?, max_side?}` →
//!   `{results, out, format?, bytes?, width?, height?}` — the command door
//!   (ADR 0013, #418): run commands of cadcraft's registry on the drawing at
//!   `path`, or on a new one, then write it to `out` as DXF, DWG, SVG or PDF
//!   as `convert` does, or as a PNG as `render` draws it (by `format`, else
//!   `out`'s extension). Only what the door's allowlist admits runs
//!   ([`door`]): commands the reviewed classification (`skill/safety.json`)
//!   classes `safe`, except `setvar`, which sets variables by name and has
//!   no name reviewed; every other id is refused before any command runs.
//!
//! Paths never leave the area: `..`, absolute paths and symlink escapes are
//! refused. The service serves system apps only until ADR 0013's store
//! capability is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use cadcraft_engine::doc::{Drawing, Space};
use cadcraft_engine::Session;
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::door::{Door, Reviewed, Setter};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};

/// The largest drawing file the service reads, and the largest it writes.
const MAX_FILE_BYTES: u64 = 64 << 20;
/// The longest raster edge `render` produces.
const MAX_RENDER_SIDE: u32 = 4096;
/// The most entities one `entities` call returns.
const MAX_ENTITIES: u64 = 2000;
/// What `run` writes, by `format` or `out`'s extension.
const RUN_FORMATS: &[&str] = &["dxf", "dwg", "svg", "png", "pdf"];

/// What the cad engine's reviewer settled for the door beyond the classes.
/// One setter: `setvar {name, value}` (`cmd/settings.rs` `run_setvar`,
/// `sysvars.rs` `set`) sets a session variable by name, or else any drawing
/// header variable the name spells, so no name is reviewed and it is
/// refused. The other `safe` commands that set variables or options are no
/// setters: the drafting toggles (`ortho`, `grid`, `snap`, `polar`,
/// `otrack`, `dynmode`, `lwdisplay`, `isodraft`, `transparencydisplay`,
/// `qpmode`, `selectioncycling`) each flip one fixed field of the call's own
/// `Session.settings`, `osnap` and `dsettings` set fixed fields of it,
/// `units`, `limits`, `ltscale` and the current-property commands write
/// fixed header variables, and `dimoverride` keeps only dimension-style
/// fields; none takes a caller-chosen variable name, and the session ends
/// with the call. No reviewed read (`open {path}` reads before it
/// notices the service installs no file hooks) and no inner id: every
/// nested `execute` runs a fixed id (`explode`, `properties.set`, `dist`,
/// `area`, `layout.set`, the dimension associations), and the command line
/// and script runners (`Session::cmdline`, `Session::script`) are not
/// catalog commands.
static REVIEWED: Reviewed = Reviewed { file_reads: &[], setters: &[Setter { id: "setvar", keys: &[], keys_of: setvar_keys }], inner: &[] };

/// The variable one `setvar` call sets, as the engine spells it.
fn setvar_keys(params: &Json) -> Vec<String> {
    params["name"].as_str().map(|name| vec![name.trim().to_ascii_uppercase()]).unwrap_or_default()
}

/// The command door's gate: cadcraft's reviewed classification
/// (`skill/safety.json`, generated and drift-checked by `tests/skill.rs`)
/// and [`REVIEWED`].
pub fn door() -> Result<&'static Door, String> {
    static DOOR: OnceLock<Result<Door, String>> = OnceLock::new();
    DOOR.get_or_init(|| Door::new("cad", include_str!("../skill/safety.json"), &REVIEWED)).as_ref().map_err(Clone::clone)
}

/// Which apps may call the service: system apps, as News and Sheets.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct CadService;

/// Register the `cad` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(CadService));
}

/// The shell's resolver: where each call works (`None` removes it, and
/// calls work in the legacy `<host dir>/cad` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
}

/// The `cad.*` agent tools (ADR 0013, wave 2), in App Hub's `tools.json`
/// shape: the shell declares them for the virtual owner `os.cad` and grants
/// the system agent its reviewed share (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`).
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for CadService {
    fn family(&self) -> &'static str {
        "cad"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        reply.send(serve(&AREAS, &call));
    }
}

/// One call, in the area `areas` gives it.
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The cad service serves system apps only.".into());
    }
    let area = areas.area(call, "cad").map_err(|e| format!("cad: {e}"))?;
    dispatch_in(call.method(), &call.args, &area)
        .map(|answer| area.relative_json(answer))
        .map_err(|error| area.relative_text(&error))
}

fn dispatch_in(method: &str, args: &Json, area: &Area) -> Result<Json, String> {
    match method {
        "info" => info(args, area),
        "entities" => entities(args, area),
        "measure" => measure(args, area),
        "render" => render(args, area),
        "convert" => convert(args, area),
        "run" => run(args, area),
        other => Err(format!("cad.{other} is not a method of the cad service")),
    }
}

/// [`dispatch_in`] in the legacy area `<host_dir>/cad`, as a call without
/// the shell's resolver works.
#[cfg(test)]
fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    let area = Area::legacy(host_dir, "cad");
    std::fs::create_dir_all(&area.root).map_err(|e| format!("cad: {e}"))?;
    dispatch_in(method, args, &area)
}

fn arg_str<'a>(ctx: &str, args: &'a Json, key: &str) -> Result<&'a str, String> {
    args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("{ctx}: `{key}` is required"))
}

/// A path strictly inside the call's area: relative, no `..`, no absolute
/// component; the resolved parent must stay under the area even through
/// symlinks.
fn contained(ctx: &str, area: &Area, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("{ctx}: a path is required"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("{ctx}: `{rel}` stays inside this call's folder"));
    }
    let joined = area.root.join(rel_path);
    let check_root = area.root.canonicalize().map_err(|e| format!("{ctx}: folder: {e}"))?;
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
    let resolved = deepest.canonicalize().map_err(|e| format!("{ctx}: {e}"))?;
    if !resolved.starts_with(&check_root) {
        return Err(format!("{ctx}: `{rel}` stays inside this call's folder"));
    }
    Ok(joined)
}

/// A contained output path the call may write, refused before the engine
/// works when the area's rules would refuse it.
fn out_path(ctx: &str, area: &Area, rel: &str) -> Result<PathBuf, String> {
    let out = contained(ctx, area, rel)?;
    area.check(&out, 0).map_err(|e| format!("{ctx}: {e}"))?;
    Ok(out)
}

/// Read a drawing from the area through the engine's codecs (DXF ASCII or
/// binary, DWG), with the file size capped.
fn read_drawing(ctx: &str, area: &Area, rel: &str) -> Result<Drawing, String> {
    let path = contained(ctx, area, rel)?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("{ctx}: {rel}: {e}"))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!("{ctx}: the file is larger than the service reads"));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("{ctx}: {rel}: {e}"))?;
    cadcraft_io::read(&bytes, rel).map_err(|e| format!("{ctx}: {e}"))
}

/// A fresh session with the drawing open; the engine sees no file system.
fn load(ctx: &str, area: &Area, rel: &str) -> Result<Session, String> {
    let d = read_drawing(ctx, area, rel)?;
    let title = Path::new(rel).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| rel.to_string());
    let mut s = Session::empty();
    s.open_drawing(d, &title, None);
    Ok(s)
}

/// Write produced bytes into the area, capped, under the area's rules
/// ([`Area::write`]: no replacement unless allowed, within the quota).
fn write_out(ctx: &str, area: &Area, rel: &str, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(format!("{ctx}: the result is larger than the service writes"));
    }
    let path = contained(ctx, area, rel)?;
    area.write(&path, bytes).map_err(|e| format!("{ctx}: {e}"))
}

fn info(args: &Json, area: &Area) -> Result<Json, String> {
    let path = arg_str("cad.info", args, "path")?;
    let mut s = load("cad.info", area, path)?;
    let mut doc = s.execute("drawing.inspect", &json!({"entities": false})).map_err(|e| format!("cad.info: {e}"))?;
    doc["file"] = json!(path);
    Ok(doc)
}

fn entities(args: &Json, area: &Area) -> Result<Json, String> {
    let path = arg_str("cad.entities", args, "path")?;
    let mut s = load("cad.entities", area, path)?;
    let params = json!({
        "type": args["type"],
        "layer": args["layer"],
        "limit": args["limit"].as_u64().unwrap_or(500).min(MAX_ENTITIES),
        "offset": args["offset"].as_u64().unwrap_or(0),
    });
    s.execute("entities", &params).map_err(|e| format!("cad.entities: {e}"))
}

fn measure(args: &Json, area: &Area) -> Result<Json, String> {
    let path = arg_str("cad.measure", args, "path")?;
    let mut s = load("cad.measure", area, path)?;
    match (args["dist"].is_object(), args["area"].is_object()) {
        (true, false) => {
            let r = s.execute("dist", &args["dist"]).map_err(|e| format!("cad.measure: {e}"))?;
            Ok(json!({"dist": r}))
        }
        (false, true) => {
            let r = s.execute("area", &args["area"]).map_err(|e| format!("cad.measure: {e}"))?;
            Ok(json!({"area": r}))
        }
        _ => Err("cad.measure: give `dist` {p1, p2} or `area` {points | handle}".into()),
    }
}

/// `max_side` as `render` takes it: 16..=4096 pixels, 1024 when absent.
fn max_side(args: &Json) -> u32 {
    args["max_side"].as_u64().unwrap_or(1024).clamp(16, MAX_RENDER_SIDE as u64) as u32
}

/// The model space of `d` as a PNG fitted to the drawing's extents, its
/// longest edge `side` pixels (`render`, `run`): the bytes, width and height.
fn fitted_png(ctx: &str, d: &Drawing, side: u32) -> Result<(Vec<u8>, u32, u32), String> {
    let b = d.extents(&Space::Model);
    let (w, h) = if b.is_empty() {
        (side, side)
    } else {
        let (bw, bh) = (b.width().max(1e-9), b.height().max(1e-9));
        let scale = f64::from(side) / bw.max(bh);
        (((bw * scale).round() as u32).clamp(1, side), ((bh * scale).round() as u32).clamp(1, side))
    };
    let png = cadcraft_io::png(d, &Space::Model, w, h).map_err(|e| format!("{ctx}: {e}"))?;
    Ok((png, w, h))
}

/// `d` encoded as `format` by the engine's codecs, as `convert` writes it
/// (`render`'s SVG too).
fn encoded(ctx: &str, d: &Drawing, format: &str) -> Result<Vec<u8>, String> {
    cadcraft_io::write(d, &format!("out.{format}")).map_err(|e| format!("{ctx}: {e}"))
}

/// The output format: `format`, else `out`'s extension, in lower case
/// (empty when neither names one).
fn out_format(args: &Json, out: &str) -> String {
    match args["format"].as_str() {
        Some(f) if !f.is_empty() => f.to_ascii_lowercase(),
        _ => Path::new(out).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default(),
    }
}

fn render(args: &Json, area: &Area) -> Result<Json, String> {
    let path = arg_str("cad.render", args, "path")?;
    let out = arg_str("cad.render", args, "out")?;
    out_path("cad.render", area, out)?;
    let d = read_drawing("cad.render", area, path)?;
    let ext = Path::new(out).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let bytes = match ext.as_str() {
        "svg" => encoded("cad.render", &d, "svg")?,
        "png" => fitted_png("cad.render", &d, max_side(args))?.0,
        _ => return Err("cad.render: `out` ends in .png or .svg".into()),
    };
    write_out("cad.render", area, out, &bytes)?;
    Ok(json!({"out": out, "bytes": bytes.len(), "format": ext}))
}

fn convert(args: &Json, area: &Area) -> Result<Json, String> {
    let path = arg_str("cad.convert", args, "path")?;
    let out = arg_str("cad.convert", args, "out")?;
    out_path("cad.convert", area, out)?;
    let format = out_format(args, out);
    if format.is_empty() {
        return Err("cad.convert: give `format` or an extension on `out`".into());
    }
    let d = read_drawing("cad.convert", area, path)?;
    let bytes = encoded("cad.convert", &d, &format)?;
    write_out("cad.convert", area, out, &bytes)?;
    Ok(json!({"out": out, "bytes": bytes.len(), "format": format}))
}

/// The command door: every command admitted by [`door`] before the engine
/// runs any (a refused one refuses the whole call, with nothing written),
/// run in order on one session over the drawing at `path` or a new one (what
/// the engine's own `new {}` makes: an empty imperial drawing; `new
/// {metric: true}` among the commands opens a metric one, which later
/// commands and `out` then use), whose active drawing is then written to
/// `out` under the area's rules: DXF, DWG, SVG or PDF as `convert` writes
/// them, a PNG as `render` draws it (`max_side`), by `format` or else
/// `out`'s extension.
fn run(args: &Json, area: &Area) -> Result<Json, String> {
    // Admit every command first: one refused id refuses the whole call, with
    // nothing opened and nothing written.
    let admitted = door()?.admit_all(&args["cmds"], area)?;
    let out = match args["out"].as_str().filter(|o| !o.is_empty()) {
        Some(rel) => {
            let format = out_format(args, rel);
            if !RUN_FORMATS.contains(&format.as_str()) {
                return Err(format!("cad.run: `format`, or else the extension of `out`, is one of {}", RUN_FORMATS.join(", ")));
            }
            out_path("cad.run", area, rel)?;
            Some((rel, format))
        }
        None => None,
    };
    let mut s = match args["path"].as_str().filter(|p| !p.is_empty()) {
        Some(path) => load("cad.run", area, path)?,
        None => {
            let mut s = Session::empty();
            s.new_drawing(false);
            s
        }
    };
    let mut results = Vec::with_capacity(admitted.len());
    for (id, params) in admitted {
        let r = s.execute(&id, &params).map_err(|e| format!("cad.run {id}: {e}"))?;
        results.push(json!({"id": id, "result": r}));
    }
    let Some((out_rel, format)) = out else { return Ok(json!({"results": results, "out": Json::Null})) };
    let d = s.doc().map_err(|e| format!("cad.run: {e}"))?;
    let mut answer = json!({"results": results, "out": out_rel, "format": format});
    let bytes = if format == "png" {
        let (png, width, height) = fitted_png("cad.run", d, max_side(args))?;
        answer["width"] = json!(width);
        answer["height"] = json!(height);
        png
    } else {
        encoded("cad.run", d, &format)?
    };
    write_out("cad.run", area, out_rel, &bytes)?;
    answer["bytes"] = json!(bytes.len());
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_engine::doc::{self as doc, Common, EntityKind};
    use cadcraft_engine::geom::Vec3;

    /// A two-entity drawing written as DXF through the engine itself: a
    /// 10-unit line from the origin and a circle of radius 2 at (5, 5).
    fn fixture(host: &Path) -> String {
        let v = |x: f64, y: f64| Vec3::new(x, y, 0.0);
        let mut d = Drawing::new_metric();
        d.add(&Space::Model, Common::default(), EntityKind::Line(doc::Line { a: v(0.0, 0.0), b: v(10.0, 0.0) })).unwrap();
        d.add(&Space::Model, Common::default(), EntityKind::Circle(doc::Circle { center: v(5.0, 5.0), radius: 2.0 })).unwrap();
        let bytes = cadcraft_io::write(&d, "in.dxf").unwrap();
        std::fs::create_dir_all(host.join("cad")).unwrap();
        std::fs::write(host.join("cad").join("in.dxf"), bytes).unwrap();
        "in.dxf".into()
    }

    #[test]
    fn info_reads_a_real_dxf() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);

        let doc = dispatch("info", &json!({"path": input}), host).unwrap();
        assert_eq!(doc["file"], json!("in.dxf"), "{doc}");
        assert_eq!(doc["entityCount"], json!(2));
        assert_eq!(doc["counts"]["Line"], json!(1));
        assert_eq!(doc["counts"]["Circle"], json!(1));
        assert!(doc["layers"].as_array().is_some_and(|l| !l.is_empty()), "{doc}");
        assert!(!doc["extents"].is_null());
    }

    #[test]
    fn entities_filters_by_type() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);

        let all = dispatch("entities", &json!({"path": input}), host).unwrap();
        assert_eq!(all["count"], json!(2), "{all}");
        let circles = dispatch("entities", &json!({"path": input, "type": "circle"}), host).unwrap();
        assert_eq!(circles["count"], json!(1), "{circles}");
        assert_eq!(circles["entities"][0]["type"], json!("Circle"));
        assert_eq!(circles["entities"][0]["layer"], json!("0"));
    }

    #[test]
    fn measure_dist_area_and_handle() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);

        let d = dispatch("measure", &json!({"path": input, "dist": {"p1": [0, 0], "p2": [3, 4]}}), host).unwrap();
        assert_eq!(d["dist"]["distance"], json!(5.0), "{d}");

        let a = dispatch(
            "measure",
            &json!({"path": input, "area": {"points": [[0, 0], [4, 0], [4, 3], [0, 3]]}}),
            host,
        )
        .unwrap();
        assert_eq!(a["area"]["area"], json!(12.0), "{a}");
        assert_eq!(a["area"]["perimeter"], json!(14.0));

        // AREA on the circle, found by its real handle.
        let circles = dispatch("entities", &json!({"path": input, "type": "circle"}), host).unwrap();
        let handle = circles["entities"][0]["handle"].as_str().unwrap().to_string();
        let byh = dispatch("measure", &json!({"path": input, "area": {"handle": handle}}), host).unwrap();
        let area = byh["area"]["area"].as_f64().unwrap();
        assert!((area - 4.0 * std::f64::consts::PI).abs() < 1e-9, "{byh}");

        assert!(dispatch("measure", &json!({"path": input}), host).is_err(), "dist or area is required");
    }

    #[test]
    fn render_writes_png_and_svg() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);

        let png = dispatch("render", &json!({"path": input, "out": "prev/out.png", "max_side": 64}), host).unwrap();
        assert!(png["bytes"].as_u64().unwrap() > 0, "{png}");
        let written = std::fs::read(host.join("cad/prev/out.png")).unwrap();
        assert_eq!(&written[..4], b"\x89PNG", "a real PNG, inside the cad area");
        assert!(written.len() as u64 == png["bytes"].as_u64().unwrap());

        let svg = dispatch("render", &json!({"path": input, "out": "out.svg"}), host).unwrap();
        assert_eq!(svg["format"], json!("svg"));
        let text = std::fs::read_to_string(host.join("cad/out.svg")).unwrap();
        assert!(text.contains("<svg"), "{text}");

        assert!(dispatch("render", &json!({"path": input, "out": "out.gif"}), host).is_err(), "png or svg only");
    }

    #[test]
    fn convert_roundtrips_dxf_and_honours_format() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);

        let copy = dispatch("convert", &json!({"path": input, "out": "copy.dxf"}), host).unwrap();
        assert_eq!(copy["format"], json!("dxf"), "{copy}");
        let doc = dispatch("info", &json!({"path": "copy.dxf"}), host).unwrap();
        assert_eq!(doc["entityCount"], json!(2), "the drawing survives the DXF roundtrip");

        let svg = dispatch("convert", &json!({"path": input, "out": "plot.vec", "format": "svg"}), host).unwrap();
        assert_eq!(svg["format"], json!("svg"));
        assert!(std::fs::read_to_string(host.join("cad/plot.vec")).unwrap().contains("<svg"));

        assert!(dispatch("convert", &json!({"path": input, "out": "x.nope"}), host).is_err(), "unsupported format");
    }

    #[test]
    fn paths_stay_inside_the_cad_area() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);
        // A sibling of the cad area, as Mail's and Calendar's data are.
        std::fs::write(host.join("calendar.json"), b"{}").unwrap();

        for bad in ["../calendar.json", "../up.dxf", "/etc/x.dxf", "a/../../up.dxf", ""] {
            assert!(dispatch("info", &json!({"path": bad}), host).is_err(), "info {bad}");
            assert!(dispatch("render", &json!({"path": input, "out": bad}), host).is_err(), "render {bad}");
            assert!(dispatch("convert", &json!({"path": input, "out": bad, "format": "dxf"}), host).is_err(), "convert {bad}");
        }
        assert_eq!(std::fs::read_to_string(host.join("calendar.json")).unwrap(), "{}", "siblings untouched");
    }

    #[test]
    fn the_area_is_the_cad_subdir() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);
        serve(&Slot::new(), &service_call("render", json!({"path": input, "out": "p.png"}), host, false)).unwrap();
        let mut names: Vec<String> =
            std::fs::read_dir(host).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, vec!["cad"], "without a resolver everything the service touches lands under cad/");
        // and may replace, as it always could
        serve(&Slot::new(), &service_call("render", json!({"path": input, "out": "p.png"}), host, false)).unwrap();
    }

    /// A call as App Hub hands it to the service.
    fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("cad.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
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

    /// The fixture drawing, written straight into `root` (a caller's folder).
    fn fixture_in(root: &Path) -> String {
        let made = tempfile::tempdir().unwrap();
        let name = fixture(made.path());
        std::fs::copy(made.path().join("cad").join(&name), root.join(&name)).unwrap();
        name
    }

    /// With the shell's resolver every path is relative to the caller's own
    /// folder and stays inside it, through a link too.
    #[test]
    fn the_resolver_root_is_used_and_paths_stay_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let input = fixture_in(&root);
        let areas = resolver(&root, None);
        let host = dir.path().join(".host");
        let doc = serve(&areas, &service_call("info", json!({"path": input}), &host, false)).unwrap();
        assert_eq!(doc["entityCount"], json!(2), "{doc}");
        serve(&areas, &service_call("convert", json!({"path": input, "out": "out/copy.dxf"}), &host, false)).unwrap();
        assert!(root.join("out/copy.dxf").is_file() && !host.exists() && !root.join("cad").exists());
        std::fs::write(dir.path().join("beside.dxf"), b"x").unwrap();
        for bad in ["../beside.dxf", "/etc/hosts", "out/../../beside.dxf"] {
            assert!(serve(&areas, &service_call("info", json!({"path": bad}), &host, false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("render", json!({"path": input, "out": bad}), &host, true)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            assert!(serve(&areas, &service_call("info", json!({"path": "up/beside.dxf"}), &host, false)).is_err());
            assert!(serve(&areas, &service_call("render", json!({"path": input, "out": "up/p.svg"}), &host, true)).is_err());
            assert!(!dir.path().join("p.svg").exists());
        }
    }

    /// An agent's call never replaces a file, before the engine runs; an
    /// app's own foreground call may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        let input = fixture_in(dir.path());
        let areas = resolver(dir.path(), None);
        std::fs::write(dir.path().join("p.svg"), b"keep me").unwrap();
        for method in ["render", "convert"] {
            let refused = serve(&areas, &service_call(method, json!({"path": input, "out": "p.svg"}), dir.path(), false)).unwrap_err();
            assert!(refused.contains("`p.svg` already exists"), "{method}: {refused}");
        }
        assert_eq!(std::fs::read(dir.path().join("p.svg")).unwrap(), b"keep me");
        serve(&areas, &service_call("render", json!({"path": input, "out": "p.svg"}), dir.path(), true)).unwrap();
        assert!(std::fs::read_to_string(dir.path().join("p.svg")).unwrap().contains("<svg"));
    }

    /// What a call writes must fit what is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let input = fixture_in(dir.path());
        let refused = serve(&resolver(dir.path(), Some(16)), &service_call("convert", json!({"path": input, "out": "c.dxf"}), dir.path(), true)).unwrap_err();
        assert!(refused.contains("bytes left"), "{refused}");
        assert!(!dir.path().join("c.dxf").exists());
        serve(&resolver(dir.path(), Some(1 << 22)), &service_call("convert", json!({"path": input, "out": "c.dxf"}), dir.path(), true)).unwrap();
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.sheets"));
        assert!(may_call("os.news"));
        assert!(!may_call("org.example.app"));
        assert!(!may_call(""));
    }

    /// The agent tools (`tools.json`) pass App Hub's own loader, as the shell
    /// reads them, keep the object schemas octos takes both ways, and name
    /// only methods this service dispatches.
    #[test]
    fn the_agent_tools_pass_app_hubs_loader_and_name_real_methods() {
        use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest};
        let (manifest, _) = ToolManifest::load(TOOLS_JSON, "cad", ToolHost::Contained, false).unwrap();
        assert!(!manifest.tools.is_empty());
        let dir = tempfile::tempdir().unwrap();
        for tool in &manifest.tools {
            assert_eq!(tool.implemented_by, ImplementedBy::HostService, "{}", tool.name);
            assert!(tool.host_method.is_none(), "{}: the shell routes each tool to the method of its own name", tool.name);
            for schema in [&tool.input_schema, &tool.output_schema] {
                assert_eq!(schema["type"], json!("object"), "{}: octos takes object schemas only", tool.name);
            }
            assert!(tool.description.is_ascii(), "{}: descriptions stay within octos's byte limit", tool.name);
            let method = tool.name.strip_prefix("cad.").unwrap();
            if let Err(e) = dispatch(method, &json!({}), dir.path()) {
                assert!(!e.contains("is not a method"), "{}: {e}", tool.name);
            }
        }
    }

    /// The door runs allowlisted commands in a temporary area: `safe`
    /// commands draw on a new drawing, which is written as DXF and read back;
    /// an existing drawing is edited; queries answer without writing.
    #[test]
    fn the_door_runs_allowlisted_commands_in_its_area() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let made = serve(
            &areas,
            &service_call(
                "run",
                json!({"cmds": [
                    {"id": "line", "params": {"points": [[0, 0], [10, 0]]}},
                    {"id": "circle", "params": {"center": [5, 5], "radius": 2}},
                    {"id": "entities"}
                ], "out": "plan.dxf"}),
                dir.path(),
                false,
            ),
        )
        .unwrap();
        assert_eq!(made["out"], json!("plan.dxf"), "{made}");
        assert_eq!(made["format"], json!("dxf"));
        assert_eq!(made["results"][2]["result"]["count"], json!(2), "{made}");
        let back = serve(&areas, &service_call("entities", json!({"path": "plan.dxf"}), dir.path(), false)).unwrap();
        let types: Vec<&str> = back["entities"].as_array().unwrap().iter().filter_map(|e| e["type"].as_str()).collect();
        assert_eq!(types, ["Line", "Circle"], "{back}");
        // An existing drawing, edited and written beside itself.
        let input = fixture_in(dir.path());
        let edited = serve(&areas, &service_call("run", json!({"path": input, "cmds": [{"id": "circle", "params": {"center": [20, 0], "radius": 1}}], "out": "more.dxf"}), dir.path(), false)).unwrap();
        assert_eq!(edited["out"], json!("more.dxf"), "{edited}");
        let info = serve(&areas, &service_call("info", json!({"path": "more.dxf"}), dir.path(), false)).unwrap();
        assert_eq!(info["entityCount"], json!(3), "{info}");
        // What `cad.entities` and `cad.measure` answered, without `out`.
        let query = serve(
            &areas,
            &service_call(
                "run",
                json!({"path": input, "cmds": [
                    {"id": "entities", "params": {"type": "circle"}},
                    {"id": "dist", "params": {"p1": [0, 0], "p2": [3, 4]}},
                    {"id": "area", "params": {"points": [[0, 0], [4, 0], [4, 3], [0, 3]]}}
                ]}),
                dir.path(),
                false,
            ),
        )
        .unwrap();
        assert!(query["out"].is_null(), "{query}");
        assert_eq!(query["results"][0]["result"]["count"], json!(1));
        assert_eq!(query["results"][1]["result"]["distance"], json!(5.0));
        assert_eq!(query["results"][2]["result"]["area"], json!(12.0));
        let mut names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["in.dxf", "more.dxf", "plan.dxf"], "only the outputs");
    }

    /// Every kind of `out` the door writes, from the session after the
    /// commands: DXF, DWG, SVG and PDF as `convert` writes them, a PNG as
    /// `render` draws it; by `format`, else the extension.
    #[test]
    fn the_door_writes_every_out_kind() {
        let dir = tempfile::tempdir().unwrap();
        let input = fixture_in(dir.path());
        let areas = resolver(dir.path(), None);
        let run = |out: &str, extra: Json| {
            let mut args = json!({"path": input, "cmds": [{"id": "line", "params": {"points": [[0, 10], [10, 10]]}}], "out": out});
            for (k, v) in extra.as_object().unwrap() {
                args[k] = v.clone();
            }
            serve(&areas, &service_call("run", args, dir.path(), false))
        };
        for out in ["k.dxf", "k.dwg"] {
            let v = run(out, json!({})).unwrap();
            assert_eq!(v["format"].as_str(), Path::new(out).extension().and_then(|e| e.to_str()), "{v}");
            assert_eq!(v["bytes"].as_u64().unwrap(), std::fs::metadata(dir.path().join(out)).unwrap().len());
            let info = serve(&areas, &service_call("info", json!({"path": out}), dir.path(), false)).unwrap();
            assert_eq!(info["entityCount"], json!(3), "{out} reads back with the new line: {info}");
        }
        assert!(std::fs::read(dir.path().join("k.dwg")).unwrap().starts_with(b"AC10"), "a real DWG");
        let svg = run("k.svg", json!({})).unwrap();
        assert_eq!(svg["format"], json!("svg"), "{svg}");
        assert!(std::fs::read_to_string(dir.path().join("k.svg")).unwrap().contains("<svg"));
        let pdf = run("k.pdf", json!({})).unwrap();
        assert_eq!(pdf["format"], json!("pdf"), "{pdf}");
        assert!(std::fs::read(dir.path().join("k.pdf")).unwrap().starts_with(b"%PDF"));
        let png = run("k.png", json!({"max_side": 64})).unwrap();
        assert_eq!(png["format"], json!("png"), "{png}");
        assert_eq!(png["width"].as_u64().unwrap().max(png["height"].as_u64().unwrap()), 64, "{png}");
        let bytes = std::fs::read(dir.path().join("k.png")).unwrap();
        assert!(bytes.starts_with(b"\x89PNG") && png["bytes"].as_u64().unwrap() == bytes.len() as u64);
        let default = run("big.png", json!({})).unwrap();
        assert_eq!(default["width"].as_u64().unwrap().max(default["height"].as_u64().unwrap()), 1024, "as render: {default}");
        // `format` names the kind whatever `out` ends in.
        let named = run("plot.vec", json!({"format": "svg"})).unwrap();
        assert_eq!(named["format"], json!("svg"), "{named}");
        assert!(std::fs::read_to_string(dir.path().join("plot.vec")).unwrap().contains("<svg"));
        for (out, extra) in [("x.bmp", json!({})), ("x.dxf", json!({"format": "exe"})), ("noext", json!({}))] {
            let e = run(out, extra).unwrap_err();
            assert!(e.contains("is one of dxf, dwg, svg, png, pdf"), "{out}: {e}");
            assert!(!dir.path().join(out).exists());
        }
    }

    /// Every class but `safe` is refused (cadcraft has only `safe` and
    /// `file` commands), and so are `setvar`, which sets variables by name
    /// with none reviewed, and any id the classification does not spell
    /// exactly, before any command runs: a refused id anywhere in the list
    /// writes nothing.
    #[test]
    fn the_door_refuses_every_other_class_and_unknown_ids() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let refused = |id: &str, params: Json| {
            let cmds = json!([{"id": "line", "params": {"points": [[0, 0], [1, 1]]}}, {"id": id, "params": params}]);
            serve(&areas, &service_call("run", json!({"cmds": cmds, "out": "x.dxf"}), dir.path(), false)).unwrap_err()
        };
        for id in ["open", "qsave", "saveas", "wblock", "plot", "exportpdf"] {
            let e = refused(id, json!({"path": "elsewhere.dxf"}));
            assert!(e.contains(&format!("`{id}` reads or writes files")) && e.contains("not reviewed to run through it"), "{id}: {e}");
        }
        for name in ["ORTHOMODE", "osmode", "HYPERLINKBASE", "CADCRAFT_LAYISO"] {
            let e = refused("setvar", json!({"name": name, "value": 1}));
            assert!(e.contains("`setvar` sets app-wide variables, and no key of it is reviewed"), "{name}: {e}");
        }
        // The engine folds an id's case; the door matches the reviewed
        // spelling exactly, so a variant is an unknown id.
        for id in ["cad.secret", "OPEN", "Line", "save"] {
            let e = refused(id, json!({}));
            assert!(e.contains("not a reviewed cad command"), "{id}: {e}");
        }
        assert!(!dir.path().join("x.dxf").exists(), "nothing written");
        let too_many: Vec<Json> = (0..65).map(|_| json!({"id": "regen"})).collect();
        assert!(serve(&areas, &service_call("run", json!({"cmds": too_many}), dir.path(), false)).unwrap_err().contains("at most 64"));
        assert!(serve(&areas, &service_call("run", json!({"cmds": [{"id": ""}]}), dir.path(), false)).unwrap_err().contains("each command has an `id`"));
        assert!(serve(&areas, &service_call("run", json!({"out": "y.dxf"}), dir.path(), false)).unwrap_err().contains("`cmds` is a list"));
    }

    /// `open` reads no file through the door, inside the area or out;
    /// `qsave` and `saveas` write none; `out` never replaces, keeps to the
    /// quota, and `path` and `out` stay inside the area.
    #[test]
    fn the_doors_writes_keep_the_areas_rules() {
        let dir = tempfile::tempdir().unwrap();
        let input = fixture_in(dir.path());
        let areas = resolver(dir.path(), None);
        let outside = tempfile::tempdir().unwrap();
        let secret = fixture_in(outside.path());
        let secret = outside.path().join(secret).to_string_lossy().into_owned();
        for path in [secret.as_str(), "in.dxf", "../in.dxf"] {
            let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "open", "params": {"path": path}}, {"id": "entities"}]}), dir.path(), false)).unwrap_err();
            assert!(e.contains("`open` reads or writes files"), "{path}: {e}");
            assert!(!e.contains(outside.path().to_string_lossy().as_ref()), "{path}: {e}");
        }
        for id in ["qsave", "saveas"] {
            for path in ["saved.dxf", secret.as_str()] {
                let e = serve(&areas, &service_call("run", json!({"path": input, "cmds": [{"id": id, "params": {"path": path}}]}), dir.path(), false)).unwrap_err();
                assert!(e.contains(&format!("`{id}` reads or writes files")), "{id} {path}: {e}");
            }
        }
        let e = serve(&areas, &service_call("run", json!({"path": input, "cmds": [{"id": "setvar", "params": {"name": "ORTHOMODE", "value": 1}}]}), dir.path(), false)).unwrap_err();
        assert!(e.contains("sets app-wide variables"), "{e}");
        assert!(!dir.path().join("saved.dxf").exists());
        let mut left: Vec<_> = std::fs::read_dir(outside.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, ["in.dxf"], "the folder outside is untouched");
        // Never over an existing file, within the quota, inside the area.
        std::fs::write(dir.path().join("taken.dxf"), b"keep").unwrap();
        let e = serve(&areas, &service_call("run", json!({"path": input, "cmds": [], "out": "taken.dxf"}), dir.path(), false)).unwrap_err();
        assert!(e.contains("`taken.dxf` already exists"), "{e}");
        assert_eq!(std::fs::read(dir.path().join("taken.dxf")).unwrap(), b"keep");
        let e = serve(&resolver(dir.path(), Some(16)), &service_call("run", json!({"cmds": [{"id": "line", "params": {"points": [[0, 0], [1, 1]]}}], "out": "c.dxf"}), dir.path(), false)).unwrap_err();
        assert!(e.contains("bytes left"), "{e}");
        assert!(!dir.path().join("c.dxf").exists());
        for bad in ["../up.dxf", "/etc/x.dxf", "a/../../up.dxf"] {
            assert!(serve(&areas, &service_call("run", json!({"cmds": [], "out": bad}), dir.path(), false)).is_err(), "out {bad}");
            assert!(serve(&areas, &service_call("run", json!({"cmds": [], "path": bad}), dir.path(), false)).is_err(), "path {bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), dir.path().join("up")).unwrap();
            assert!(serve(&areas, &service_call("run", json!({"cmds": [], "path": "up/in.dxf"}), dir.path(), false)).is_err());
            assert!(serve(&areas, &service_call("run", json!({"cmds": [], "out": "up/made.svg"}), dir.path(), false)).is_err());
            assert!(!outside.path().join("made.svg").exists());
        }
    }

    /// The door's gate is built from the generated classification and the
    /// reviewed setter, which must be a `safe` command of the catalog: every
    /// `safe` id but `setvar` runs, and no `file` one.
    #[test]
    fn the_door_is_built_from_the_reviewed_classification() {
        let door = door().unwrap();
        for id in ["line", "circle", "entities", "dist", "area", "drawing.inspect", "new", "close", "getvar", "sysvars", "ortho", "osnap", "dsettings", "document.bytes"] {
            assert!(door.runs(id), "{id}");
        }
        for id in ["setvar", "open", "qsave", "saveas", "wblock", "plot", "exportpdf"] {
            assert!(!door.runs(id), "{id}");
        }
        let safety: Json = serde_json::from_str(include_str!("../skill/safety.json")).unwrap();
        let safe = safety["commands"].as_object().unwrap().values().filter(|c| *c == "safe").count();
        assert_eq!(door.runnable().len(), safe - 1);
    }

    /// Every `cad.run` call the skill's examples show runs, in order,
    /// in one area as the system agent's (with the files they name placed
    /// there first), and writes its `out`: the skill teaches commands that
    /// work.
    #[test]
    fn the_skill_examples_run() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);

        let body = include_str!("../skill/SKILL.md");
        let examples = body.split("\n## Examples").nth(1).and_then(|rest| rest.split("\n## ").next()).unwrap();
        let mut ran = 0;
        for span in examples.split('`').skip(1).step_by(2) {
            let Some(args) = span.strip_prefix("cad.run ") else { continue };
            let args: Json = serde_json::from_str(args).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            let got = serve(&areas, &service_call("run", args.clone(), dir.path(), false)).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            assert_eq!(got["results"].as_array().map(Vec::len), args["cmds"].as_array().map(Vec::len), "{got}");
            if let Some(out) = args["out"].as_str() {
                assert!(dir.path().join(out).is_file(), "`{span}` wrote no {out}");
            }
            ran += 1;
        }
        assert!(ran >= 5, "{ran} examples");
        let walls = serve(&areas, &service_call("run", json!({"path": "room.dxf", "cmds": [{"id": "entities", "params": {"layer": "WALLS"}}]}), dir.path(), false)).unwrap();
        assert_eq!(walls["results"][0]["result"]["count"], json!(1), "{walls}");
        let measured = serve(&areas, &service_call("run", json!({"path": "room.dxf", "cmds": [{"id": "area", "params": {"points": [[0, 0], [4000, 0], [4000, 3000], [0, 3000]]}}]}), dir.path(), false)).unwrap();
        assert_eq!(measured["results"][0]["result"]["area"], json!(12_000_000.0), "{measured}");
    }
}
