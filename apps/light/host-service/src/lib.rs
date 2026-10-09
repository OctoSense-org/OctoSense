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
//! **Where a call works** (ADR 0013, 2026-10-08): the caller's own folder,
//! the [`Area`] the shell's resolver gives it ([`set_area_resolver`]), or
//! without one the legacy `<host dir>/light`. The engine's file access has
//! no fence of its own (`with_fs` reads any path it is handed), so the
//! service hands it only files it has contained: a regular file inside the
//! area (never a folder, which the engine would walk), whose XMP sidecar,
//! if it has one, resolves inside the area too. Outputs are written by the
//! service, all of one export or none, under the area's rules
//! ([`octosense_engine_area::Stage`]): a write that may not replace (an
//! agent's) only creates new files, within the area's quota.
//!
//! Methods (all under the `light` family; paths relative to the area):
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
//! - `run {path, cmds: [{id, params?}], out?, quality?, long_edge?}` →
//!   `{results: [{id, result}], out, width?, height?, sidecars?}` — the
//!   command door (ADR 0013, #418): import the original at `path` as
//!   `develop` does, run commands of lightcraft's registry on it in order,
//!   then write the active photo to `out` as `develop` writes it (`out`'s
//!   extension picks the format; `quality`, `long_edge` as `develop`'s).
//!   Only what the door's allowlist admits runs ([`door`]): the commands the
//!   reviewed classification (`skill/safety.json`) classes `safe`; every
//!   other id is refused before any command runs. `develop.set {values:
//!   {control: number}}` sets develop controls, `develop.auto` runs
//!   auto-tone and `develop.controls` lists the controls with their ranges.
//!   What one call may ask for is capped: the parameters that multiply work
//!   at the gate ([`REVIEWED`]), the photos and their settings after every
//!   command in the service ([`RunBudget`]), the original's and the export's
//!   pixels (`header_pixels`, `check_export_size`).
//!
//! `params` is a `{control: number}` map of the engine's own control ids
//! (`light.exposure`, `color.vibrance`, `wb.temp`…; `light.controls`
//! lists them), applied through `develop.set`; `auto: true` runs the
//! engine's auto-tone first. The service serves system apps only until
//! ADR 0013's store capability is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};

use lightcraft_engine::catalog::PhotoId;
use lightcraft_engine::develop::{BrushStroke, DevelopSettings, MaskShape, Spot};
use lightcraft_engine::export::{export_photo, output_size, ExportFormat, ExportOptions};
use lightcraft_engine::Session;
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::door::{Door, Limit, Measure, Reviewed};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};

/// Originals per `batch` call.
const MAX_BATCH_FILES: usize = 16;
/// The largest original the service reads (RAW files are tens of MB).
const MAX_INPUT_BYTES: u64 = 256 << 20;
/// The longest output edge `long_edge` may ask for (the engine's own cap).
const MAX_LONG_EDGE: u64 = 16_384;

/// What the light engine's reviewer settled for the door beyond the classes:
/// nothing, read in the engine at the pinned revision.
///
/// - No reviewed read: the call's only original is its `path`, which the
///   service contains and imports itself, as `develop` does.
/// - No setter: no `safe` command sets an app-wide variable. `develop.set`
///   takes develop-control ids as keys, the active photo's own settings; the
///   app-wide switches (`app.gpu`, `app.memoryBudget`) and the library
///   preferences that change later disk behaviour (`library.xmpPreferences`
///   and `library.toggleAutoWriteXmp`, which make every catalog change write
///   an XMP sidecar beside its original) are `host`.
/// - No inner id: the `safe` commands that run another command use fixed ids
///   (`crop.autoStraighten` runs `crop.straighten`; `keyword.toggleFromSet`,
///   `metadata.applyPreset` and `photo.pasteMetadata` run `photo.setMeta`);
///   the presets they name (`preset.apply`, `curve.applyPreset`,
///   `filter.applyPreset`) are develop settings, curve points or a library
///   filter held in the session, and a profile id resolves in memory (the
///   LUT registry is filled only by `profile.import`, a `file` command).
///
/// In the in-memory `with_fs` session no `safe` command writes a file: the
/// catalog journal, `prefs.json` and the thumbnail disk cache exist only for
/// a library on disk, and XMP auto-write is off unless a refused `host`
/// command turns it on. What `safe` commands read are the catalogued photos'
/// originals, only the one the service imported (a virtual copy shares it).
///
/// The limits bound the parameters that multiply work or memory. No `safe`
/// command renders the photo at full size (only the service's own export
/// does) or makes it larger, and every slider value, curve, profile and
/// preset is clamped by the engine, except where `develop.merge` and
/// `mask.update` write settings unchecked, which the service checks after
/// every command ([`check_settings`]). No command takes a copy count: a
/// virtual copy is one per photo it names, so nothing here is a copy and the
/// door's copy factor stays 1; the copies one call makes are bounded by the
/// service's photo ceiling ([`MAX_RUN_PHOTOS`]).
static REVIEWED: Reviewed = Reviewed {
    file_reads: &[],
    setters: &[],
    inner: &[],
    limits: &[
        BRUSH_PAINT,
        SPOT_COST,
        COLOR_SAMPLES,
        CROP_SIDE,
        CROP_ASPECT,
        MERGE_SIZE,
        UPDATE_SIZE,
        ids("album.addPhotos"),
        ids("album.removePhotos"),
        ids("album.toggleTarget"),
        ids("develop.matchExposure"),
        ids("develop.paste"),
        ids("develop.pastePrevious"),
        ids("develop.quickAdjust"),
        ids("develop.reset"),
        ids("develop.set"),
        ids("keyword.suggest"),
        ids("keyword.toggleFromSet"),
        ids("library.select"),
        ids("metadata.applyPreset"),
        ids("photo.addToLibrary"),
        ids("photo.analyze"),
        ids("photo.delete"),
        ids("photo.deletePermanently"),
        ids("photo.flag"),
        ids("photo.flipHorizontal"),
        ids("photo.flipVertical"),
        ids("photo.label"),
        ids("photo.pasteMetadata"),
        ids("photo.pick"),
        ids("photo.rate"),
        ids("photo.reject"),
        ids("photo.restore"),
        ids("photo.rotateLeft"),
        ids("photo.rotateRight"),
        ids("photo.setCaptureTime"),
        ids("photo.setMeta"),
        ids("photo.unflag"),
        ids("photo.virtualCopy"),
        ids("preset.apply"),
        ids("stack.auto"),
        ids("stack.group"),
        ids("stack.remove"),
        ids("stack.toggle"),
        ids("stack.ungroup"),
    ],
    copies_per_call: 1.0,
};

/// A command's `ids`: the engine runs it once for every entry, repeats
/// included (`ids: [1, 1, 1, …]` repeats the work, `photo.virtualCopy` the
/// copies), and some of them compare every entry with every other. At most
/// the photos a call may hold ([`MAX_RUN_PHOTOS`]), each named once
/// ([`ids_named`]). The service checks the `ids` of every other command
/// the same way before it runs ([`RunBudget::before`]).
const fn ids(id: &'static str) -> Limit {
    Limit { id, what: "photos in `ids`", measure: Measure::Custom(ids_named), max: MAX_RUN_PHOTOS as f64, copies: false }
}

/// `mask.brushStroke {points, size}`: the renderer places a dab every
/// quarter of its radius (at least every half pixel) along the path, each
/// painting a disc of `size` × the photo's long edge, and the engine clamps
/// neither the points nor their coordinates nor the size: points far
/// outside the photo make billions of dabs and exhaust memory. A stroke
/// paints about 4π × its length × its size image areas ([`brush_paint`]);
/// 16 image areas (a long scribble with a large brush) is a few hundred
/// million pixel writes at full size.
const BRUSH_PAINT: Limit = Limit { id: "mask.brushStroke", what: "image areas of brush", measure: Measure::Custom(brush_paint), max: MAX_STROKE_PAINT, copies: false };

/// `spot.add {points, size}`: every pixel of the spot's bounding box
/// measures its distance to every point, and the engine leaves the points
/// and their coordinates unbounded: the box (clipped to the photo) × the
/// points, in image areas ([`spot_cost`]); 16 is a long healing stroke
/// across the photo.
const SPOT_COST: Limit = Limit { id: "spot.add", what: "image areas of spot", measure: Measure::Custom(spot_cost), max: MAX_SPOT_COST, copies: false };

/// `mask.add {kind: "colorRange", samples}`: every pixel is compared with
/// every sample when the mask renders, and `mask.add` takes any number
/// (`mask.sampleColor` stops at 5); 16.
const COLOR_SAMPLES: Limit =
    Limit { id: "mask.add", what: "colour samples", measure: Measure::Custom(color_samples), max: MAX_COLOR_SAMPLES as f64, copies: false };

/// `crop.set {rect}`: the engine keeps the crop inside the photo but gives it
/// no minimum, and the blur radii of the 384-pixel sample renders
/// (`pointColor.pick`, `develop.targeted`) grow with the output pixels a
/// source pixel spans, so a sliver of a crop makes them run for minutes. A
/// crop side at least 1% of the photo ([`crop_rect`]).
const CROP_SIDE: Limit = Limit { id: "crop.set", what: "crop", measure: Measure::Custom(crop_rect), max: 1.0, copies: false };

/// `crop.aspect {aspect}`: an extreme ratio refits the crop into the same
/// sliver ([`crop_ratio`]); at most 100 : 1 either way, which keeps a side at
/// least 1% of the photo.
const CROP_ASPECT: Limit = Limit { id: "crop.aspect", what: "to 1 crop ratio", measure: Measure::Custom(crop_ratio), max: 100.0, copies: false };

/// `develop.merge {settings}` merges settings JSON into the photo
/// unchecked: masks, strokes, spots, curve points, a crop out of the photo,
/// noise-reduction and defringe strengths past their sliders. The service
/// checks the settings it leaves after every command ([`check_settings`]);
/// this refuses a payload larger than a photo's settings may be at all
/// ([`MAX_SETTINGS_BYTES`]) before anything runs.
const MERGE_SIZE: Limit =
    Limit { id: "develop.merge", what: "bytes of settings", measure: Measure::Custom(settings_param_bytes), max: MAX_SETTINGS_BYTES as f64, copies: false };

/// `mask.update {shape}`: the same for one mask part's shape.
const UPDATE_SIZE: Limit =
    Limit { id: "mask.update", what: "bytes of settings", measure: Measure::Custom(shape_param_bytes), max: MAX_SETTINGS_BYTES as f64, copies: false };

/// The most photos a call may hold: the original and its virtual copies.
/// Each copy decodes the original again for every render it gets (the
/// decoded source is cached per photo), and the commands that act on every
/// selected photo repeat their work per photo. `batch`'s own 16.
const MAX_RUN_PHOTOS: usize = 16;
/// The most image areas one brush stroke may paint ([`BRUSH_PAINT`]).
const MAX_STROKE_PAINT: f64 = 16.0;
/// The most image areas one spot may cost ([`SPOT_COST`]).
const MAX_SPOT_COST: f64 = 16.0;
/// The most colour samples one colour-range part may hold.
const MAX_COLOR_SAMPLES: usize = 16;
/// The most points a stroke or a spot may have, and the coordinates they
/// stay within (normalized: the photo is 0..1, a stroke may start outside
/// it).
const MAX_POINTS: usize = 10_000;
const POINT_RANGE: std::ops::RangeInclusive<f64> = -1.0..=2.0;
/// The most a stroke's path may run, in long edges of the photo.
const MAX_STROKE_LENGTH: f64 = 100.0;
/// The most a photo's develop settings may hold, as JSON: a photo with
/// dozens of masks and long strokes is ~100 KB; every edit clones them into
/// its history and undo, and every render reads them.
const MAX_SETTINGS_BYTES: usize = 1 << 20;
/// The most masks, mask parts, brush strokes, spots and red-eye corrections
/// a photo may hold: every visible mask keeps a plane of the output's size
/// while it renders (4 bytes a pixel), every part makes a pass over every
/// pixel, every pixel loops over every red-eye correction.
const MAX_MASKS: usize = 16;
const MAX_MASK_PARTS: usize = 32;
const MAX_STROKES: usize = 256;
const MAX_SPOTS: usize = 64;
const MAX_RED_EYES: usize = 16;
/// The most image areas all the strokes and all the spots of a photo may
/// paint and cost, together.
const MAX_PHOTO_PAINT: f64 = 64.0;
/// The most pixels an exported image may have: the export renders at its
/// output size with ~20–30 bytes a pixel beyond the decoded source (a
/// 6000 × 4000 JPEG took 0.9 s and 1.8 GB at opt-level 1, a TIFF 1.9 s and
/// 2.5 GB), so 16 Mpx (4900 × 3266); `long_edge` exports a larger photo
/// smaller. AVIF encodes ~10× slower a pixel (8.2 s for 24 Mpx), so 4 Mpx.
const MAX_OUT_PIXELS: u64 = 16_000_000;
const MAX_AVIF_PIXELS: u64 = 4_000_000;
/// The most pixels an original may have: PNG, TIFF, WebP, GIF, BMP, PSD and
/// JPEG XL are decoded whole when they are imported (~12–16 bytes a pixel;
/// the decoder's own limit is 2^30 pixels, 16 GB), camera raws when they
/// are developed (2 bytes a pixel and their demosaic). 64 Mpx, past every
/// camera but the largest medium formats.
const MAX_INPUT_PIXELS: u64 = 64_000_000;
/// `spot.findDust` decodes the original and renders it at 1600 px every time
/// (it caches neither): at most this many a call.
const MAX_DUST_SEARCHES: usize = 4;
/// The most bytes of results one call may return, together (every result is
/// kept until the reply; `export.savePreset` answers every preset each
/// time): what the service reads at most.
const MAX_RUN_RESULT_BYTES: u64 = 64 << 20;

/// A command's `ids`, refused when one repeats.
fn ids_named(params: &Json) -> Result<Option<f64>, String> {
    let Some(ids) = params.get("ids").and_then(Json::as_array) else { return Ok(None) };
    let mut seen = HashSet::new();
    if let Some(twice) = ids.iter().find(|id| !seen.insert(id.to_string())) {
        return Err(format!("`ids` names photo {twice} twice"));
    }
    Ok(Some(ids.len() as f64))
}

/// The points of a stroke or spot (`points: [[x, y], …]`), refused when
/// there are more than [`MAX_POINTS`] or one is out of [`POINT_RANGE`]
/// (the engine skips a pair that is not two numbers).
fn points_of(params: &Json) -> Result<Vec<(f64, f64)>, String> {
    let points: Vec<(f64, f64)> = params
        .get("points")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?)))
        .collect();
    check_points(points.iter().copied())?;
    Ok(points)
}

fn check_points(points: impl ExactSizeIterator<Item = (f64, f64)>) -> Result<(), String> {
    if points.len() > MAX_POINTS {
        return Err(format!("{} points, more than the {MAX_POINTS} the door allows a stroke or spot", points.len()));
    }
    for (x, y) in points {
        if !(POINT_RANGE.contains(&x) && POINT_RANGE.contains(&y)) {
            return Err(format!("a point at {x}, {y} is outside the {}..{} the door allows (the photo is 0..1)", POINT_RANGE.start(), POINT_RANGE.end()));
        }
    }
    Ok(())
}

/// The image areas a stroke of `points` and `size` paints (4π × length ×
/// size), refused past [`MAX_STROKE_LENGTH`] or with a size outside 0..1.
fn paint_of(points: &[(f64, f64)], size: f64) -> Result<f64, String> {
    if !(0.0..=1.0).contains(&size) {
        return Err(format!("a brush size of {size} is outside the 0..1 (of the long edge) the door allows"));
    }
    let length: f64 = points.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).sum();
    if length > MAX_STROKE_LENGTH {
        return Err(format!("a stroke {length} long edges long, more than the {MAX_STROKE_LENGTH} the door allows"));
    }
    Ok(4.0 * std::f64::consts::PI * length * size)
}

/// `mask.brushStroke`'s paint ([`BRUSH_PAINT`]); the engine's default size
/// is 0.03.
fn brush_paint(params: &Json) -> Result<Option<f64>, String> {
    let points = points_of(params)?;
    let size = params.get("size").and_then(Json::as_f64).unwrap_or(0.03);
    Ok(Some(paint_of(&points, size)?))
}

/// The image areas a spot of `points` and `size` costs: its points'
/// bounding box, grown by the size and clipped to the photo, × the points.
fn spot_area(points: &[(f64, f64)], size: f64) -> f64 {
    let Some(&first) = points.first() else { return 0.0 };
    let (x0, y0, x1, y1) = points.iter().fold((first.0, first.1, first.0, first.1), |(a, b, c, d), &(x, y)| (a.min(x), b.min(y), c.max(x), d.max(y)));
    let clip = |lo: f64, hi: f64| ((hi + size).min(1.0) - (lo - size).max(0.0)).max(0.0);
    clip(x0, x1) * clip(y0, y1) * points.len() as f64
}

/// `spot.add`'s cost ([`SPOT_COST`]): the engine clamps its size to
/// 0.001–0.25, default 0.02.
fn spot_cost(params: &Json) -> Result<Option<f64>, String> {
    let points = points_of(params)?;
    let size = params.get("size").and_then(Json::as_f64).unwrap_or(0.02).clamp(0.001, 0.25);
    Ok(Some(spot_area(&points, size)))
}

/// `mask.add`'s colour samples, when it adds a colour range.
fn color_samples(params: &Json) -> Result<Option<f64>, String> {
    Ok(params.get("samples").and_then(Json::as_array).map(|s| s.len() as f64))
}

/// `crop.set`'s rectangle (normalized `[x0, y0, x1, y1]`), refused when a
/// side is under 1% of the photo or a value is not a number.
fn crop_rect(params: &Json) -> Result<Option<f64>, String> {
    let Some(rect) = params.get("rect").and_then(Json::as_array) else { return Ok(None) };
    let v: Vec<f64> = rect.iter().filter_map(Json::as_f64).collect();
    let [x0, y0, x1, y1] = v[..] else { return Ok(None) };
    if !((x1 - x0).abs() >= 0.01 && (y1 - y0).abs() >= 0.01) {
        return Err("a crop side is at least 1% of the photo".into());
    }
    Ok(None)
}

/// `crop.aspect`'s ratio, the larger side over the smaller: `"WxH"` or
/// `[w, h]` (the other spellings keep the photo's or the crop's own).
fn crop_ratio(params: &Json) -> Result<Option<f64>, String> {
    let (w, h) = match params.get("aspect") {
        Some(Json::String(s)) => match s.split_once(['x', 'X', ':']) {
            Some((w, h)) => (w.trim().parse::<f64>().map_err(|_| "`aspect` is WxH")?, h.trim().parse::<f64>().map_err(|_| "`aspect` is WxH")?),
            None => return Ok(None),
        },
        Some(Json::Array(a)) => (a.first().and_then(Json::as_f64).unwrap_or(1.0), a.get(1).and_then(Json::as_f64).unwrap_or(1.0)),
        _ => return Ok(None),
    };
    let ratio = (w / h).max(h / w);
    if !(ratio.is_finite() && ratio >= 1.0) {
        return Err("`aspect` is two numbers above 0".into());
    }
    Ok(Some(ratio))
}

/// The JSON size of a command's `settings` or `shape`.
fn settings_param_bytes(params: &Json) -> Result<Option<f64>, String> {
    Ok(params.get("settings").map(|v| json_len(v) as f64))
}

fn shape_param_bytes(params: &Json) -> Result<Option<f64>, String> {
    Ok(params.get("shape").map(|v| json_len(v) as f64))
}

/// Bytes written to it, counted and dropped.
struct Count(u64);

impl std::io::Write for Count {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The length of `v` as JSON, without writing it out.
fn json_len(v: &Json) -> u64 {
    let mut count = Count(0);
    let _ = serde_json::to_writer(&mut count, v);
    count.0
}

/// The length of a photo's develop settings as JSON.
fn settings_len(d: &DevelopSettings) -> u64 {
    let mut count = Count(0);
    let _ = serde_json::to_writer(&mut count, d);
    count.0
}

/// The command door's gate: lightcraft's reviewed classification
/// (`skill/safety.json`, generated and drift-checked by `tests/skill.rs`)
/// and [`REVIEWED`].
pub fn door() -> Result<&'static Door, String> {
    static DOOR: OnceLock<Result<Door, String>> = OnceLock::new();
    DOOR.get_or_init(|| Door::new("light", include_str!("../skill/safety.json"), &REVIEWED)).as_ref().map_err(Clone::clone)
}

/// Which apps may call the service: system apps, as News and Sheets.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct LightService;

/// Register the `light` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(LightService));
}

/// The shell's resolver: where each call works (`None` removes it, and
/// calls work in the legacy `<host dir>/light` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
}

/// The `light.*` agent tools (ADR 0013), in App Hub's `tools.json` shape:
/// `light.info` and the command door `light.run`. The shell declares them for
/// the virtual owner `os.light` and grants them to the system agent
/// (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`); the other
/// methods (`controls`, `develop`, `batch`) stay for apps' own requests.
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for LightService {
    fn family(&self) -> &'static str {
        "light"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        reply.send(serve(&AREAS, &call));
    }
}

/// One call, in the area `areas` gives it.
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The light service serves system apps only.".into());
    }
    if call.method() == "controls" {
        return controls();
    }
    let area = areas.area(call, "light").map_err(|e| format!("light: {e}"))?;
    dispatch_in(call.method(), &call.args, &area)
        .map(|answer| area.relative_json(answer))
        .map_err(|error| area.relative_text(&error))
}

fn dispatch_in(method: &str, args: &Json, area: &Area) -> Result<Json, String> {
    match method {
        "info" => info(args, area),
        "controls" => controls(),
        "develop" => develop(args, area),
        "batch" => batch(args, area),
        "run" => run(args, area),
        other => Err(format!("light.{other} is not a method of the light service")),
    }
}

/// [`dispatch_in`] in the legacy area `<host_dir>/light`, as a call without
/// the shell's resolver works.
#[cfg(test)]
fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    if method == "controls" {
        return controls();
    }
    let area = Area::legacy(host_dir, "light");
    std::fs::create_dir_all(&area.root).map_err(|e| format!("light: {e}"))?;
    dispatch_in(method, args, &area)
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
        return Err(format!("light.{method}: `{rel}` leaves this call's folder"));
    }
    let joined = dir.join(rel_path);
    let check_root = dir.canonicalize().map_err(|e| format!("light.{method}: folder: {e}"))?;
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
        return Err(format!("light.{method}: `{rel}` leaves this call's folder"));
    }
    Ok(joined)
}

/// An original the engine may import: a regular file contained in the area
/// (the engine walks a folder it is handed, into whatever links it holds),
/// whose XMP sidecar, when there is one, resolves inside the area too: the
/// engine reads `<stem>.xmp`, `<name>.xmp` and their `.XMP` spellings beside
/// the original, following links.
fn original(dir: &Path, rel: &str, method: &str) -> Result<PathBuf, String> {
    let abs = contained_path(dir, rel, method)?;
    if !std::fs::metadata(&abs).is_ok_and(|m| m.is_file()) {
        return Err(format!("light.{method}: `{rel}` is not a file"));
    }
    // The engine decodes most formats whole when it imports them: their size
    // is read from the header first.
    if let Some(pixels) = header_pixels(&abs).map_err(|e| format!("light.{method}: `{rel}`: {e}"))? {
        if pixels > MAX_INPUT_PIXELS {
            return Err(format!("light.{method}: `{rel}` is {pixels} pixels, more than the {MAX_INPUT_PIXELS} the service decodes"));
        }
    }
    let root = dir.canonicalize().map_err(|e| format!("light.{method}: folder: {e}"))?;
    let full = abs.to_string_lossy();
    let stem = abs.with_extension("xmp");
    let named = PathBuf::from(format!("{full}.xmp"));
    for sidecar in [stem.with_extension("XMP"), named.with_extension("XMP"), stem, named] {
        if std::fs::symlink_metadata(&sidecar).is_err() {
            continue;
        }
        let inside = sidecar.canonicalize().is_ok_and(|real| real.starts_with(&root));
        if !inside {
            return Err(format!("light.{method}: `{rel}`'s XMP sidecar leads outside this call's folder"));
        }
    }
    Ok(abs)
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

fn info(args: &Json, area: &Area) -> Result<Json, String> {
    let dir = &area.root;
    let rel = arg_str(args, "path", "info")?;
    let path = original(dir, rel, "info")?;
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
/// and write the bytes, its sidecars with it, all or none under the area's
/// rules — never over an imported original.
fn export_one(s: &mut Session, area: &Area, id: PhotoId, out_abs: &Path, out_rel: &str, args: &Json, method: &str) -> Result<Json, String> {
    let o = export_options(args, out_abs, out_rel, method)?;
    let guard = s.original_guard();
    guard.check(out_abs).map_err(|e| format!("light.{method}: {e}"))?;
    area.check(out_abs, 0).map_err(|e| format!("light.{method}: {e}"))?;
    check_export_size(s, id, &o, method)?;
    // A panic in the export (it runs outside the engine's command guard)
    // is the call's error, not the caller's thread's.
    let e = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| export_photo(s, id, &o, 1)))
        .map_err(|_| format!("light.{method}: the engine failed to export the photo"))?
        .map_err(|e| format!("light.{method}: {e}"))?;
    // The output and its sidecars, staged, then moved into place together.
    let stage = area.stage().map_err(|e| format!("light.{method}: {e}"))?;
    let main = stage.path("out");
    std::fs::write(&main, &e.bytes).map_err(|e| format!("light.{method}: {e}"))?;
    let mut moves = vec![(main, out_abs.to_path_buf())];
    let mut sidecars = Vec::new();
    for (n, (sc_ext, bytes)) in e.sidecars.iter().enumerate() {
        let sc = out_abs.with_extension(sc_ext);
        guard.check(&sc).map_err(|e| format!("light.{method}: {e}"))?;
        let staged = stage.path(format!("sidecar-{n}"));
        std::fs::write(&staged, bytes).map_err(|e| format!("light.{method}: {e}"))?;
        moves.push((staged, sc));
        sidecars.push(Path::new(out_rel).with_extension(sc_ext).to_string_lossy().to_string());
    }
    stage.commit(&moves).map_err(|e| format!("light.{method}: {e}"))?;
    Ok(json!({"out": out_rel, "width": e.width, "height": e.height, "sidecars": sidecars}))
}

/// The export options of a call: `quality` (1–100, default 92), `long_edge`
/// (16–16384, a whole number however it is written; none exports the
/// photo's own size) and the format `out`'s extension names.
fn export_options(args: &Json, out_abs: &Path, out_rel: &str, method: &str) -> Result<ExportOptions, String> {
    let quality = args["quality"].as_u64().unwrap_or(92).clamp(1, 100);
    let mut p = json!({"quality": quality});
    if let Some(n) = args["long_edge"].as_f64().filter(|n| n.is_finite() && *n > 0.0) {
        p["longEdge"] = json!((n.round() as u64).clamp(16, MAX_LONG_EDGE));
    }
    let ext = out_abs.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
    let format = ExportFormat::parse(&ext)
        .ok_or_else(|| format!("light.{method}: `{out_rel}`: unknown extension (use .jpg, .png, .tif, .webp, .avif or .dng)"))?;
    let mut o = ExportOptions::from_json(&p);
    o.format = format;
    Ok(o)
}

/// Before an export: a rendered image within [`MAX_OUT_PIXELS`] (AVIF
/// [`MAX_AVIF_PIXELS`]), the original a DNG re-encodes within
/// [`MAX_INPUT_PIXELS`]. The engine renders at the output size, which a crop
/// (or a `long_edge`) sets.
fn check_export_size(s: &Session, id: PhotoId, o: &ExportOptions, method: &str) -> Result<(), String> {
    let photo = s.catalog.photo(id).ok_or_else(|| format!("light.{method}: no photo to export"))?;
    if !o.format.is_rendered() {
        let pixels = u64::from(photo.width) * u64::from(photo.height);
        if pixels > MAX_INPUT_PIXELS {
            return Err(format!("light.{method}: the original is {pixels} pixels, more than the {MAX_INPUT_PIXELS} the service re-encodes"));
        }
        return Ok(());
    }
    let (w, h) = output_size(photo, o);
    let pixels = w as u64 * h as u64;
    let max = if o.format == ExportFormat::Avif { MAX_AVIF_PIXELS } else { MAX_OUT_PIXELS };
    if pixels > max {
        return Err(format!("light.{method}: the export would be {w} × {h} = {pixels} pixels, more than the {max} the service writes (give a smaller `long_edge`)"));
    }
    Ok(())
}

fn develop(args: &Json, area: &Area) -> Result<Json, String> {
    let dir = &area.root;
    let rel = arg_str(args, "path", "develop")?;
    let out_rel = arg_str(args, "out", "develop")?;
    let input = original(dir, rel, "develop")?;
    let out = contained_path(dir, out_rel, "develop")?;
    area.check(&out, 0).map_err(|e| format!("light.develop: {e}"))?;
    let mut s = Session::new().with_fs();
    let id = import(&mut s, &input, rel, "develop")?;
    apply_params(&mut s, args, "develop")?;
    export_one(&mut s, area, id, &out, out_rel, args, "develop")
}

fn batch(args: &Json, area: &Area) -> Result<Json, String> {
    let dir = &area.root;
    let paths = args["paths"].as_array().ok_or("light.batch: `paths` is a list of files")?;
    if paths.is_empty() || paths.len() > MAX_BATCH_FILES {
        return Err(format!("light.batch: `paths` is 1..={MAX_BATCH_FILES} files"));
    }
    let out_dir = arg_str(args, "out_dir", "batch")?;
    let format = args["format"].as_str().unwrap_or("jpg");
    ExportFormat::parse(format)
        .ok_or_else(|| format!("light.batch: `{format}` is not an export format (jpg, png, tif, webp, avif or dng)"))?;
    // Every input and output named first: a name the call may not write
    // is refused before anything is developed.
    let mut named = Vec::new();
    let mut stems: Vec<String> = Vec::new();
    for p in paths {
        let rel = p.as_str().filter(|s| !s.is_empty()).ok_or("light.batch: each of `paths` is a file path")?;
        let stem = Path::new(rel)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("light.batch: `{rel}` has no file name"))?;
        if stems.contains(&stem) {
            return Err(format!("light.batch: two of `paths` would both write `{out_dir}/{stem}.{format}`"));
        }
        stems.push(stem.clone());
        named.push((rel, stem));
    }
    let mut jobs = Vec::new();
    for (rel, stem) in named {
        let input = original(dir, rel, "batch")?;
        let out_rel = format!("{out_dir}/{stem}.{format}");
        let out = contained_path(dir, &out_rel, "batch")?;
        area.check(&out, 0).map_err(|e| format!("light.batch: {e}"))?;
        jobs.push((rel, input, out_rel, out));
    }
    let mut s = Session::new().with_fs();
    let mut files = Vec::new();
    for (rel, input, out_rel, out) in jobs {
        let id = import(&mut s, &input, rel, "batch")?;
        apply_params(&mut s, args, "batch")?;
        let mut one = export_one(&mut s, area, id, &out, &out_rel, args, "batch")?;
        one["path"] = json!(rel);
        files.push(one);
    }
    Ok(json!({"files": files}))
}

/// The command door: every command admitted by [`door`] before the engine
/// runs any (a refused one refuses the whole call, with nothing read or
/// written), then the original at `path` imported into a fresh session as
/// `develop` imports it, each command run on it in order, and the active
/// photo written to `out` as `develop` writes it ([`export_one`]: `quality`,
/// `long_edge`, never over an original, under the area's rules).
fn run(args: &Json, area: &Area) -> Result<Json, String> {
    let dir = &area.root;
    let rel = arg_str(args, "path", "run")?;
    // Admit every command first: one refused id refuses the whole call, with
    // nothing imported and nothing written.
    let admitted = door()?.admit_all(&args["cmds"], area)?;
    let input = original(dir, rel, "run")?;
    // A name the call may not write, or a format the door does not write,
    // is refused before the engine works. The door writes rendered files and
    // DNGs, never the engine's `original` export, whose XMP sidecar is named
    // after `out` and could land beside an original as its sidecar.
    let out = match args["out"].as_str().filter(|o| !o.is_empty()) {
        Some(out_rel) => {
            let out = contained_path(dir, out_rel, "run")?;
            let ext = out.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
            ExportFormat::parse(&ext)
                .filter(|f| *f != ExportFormat::Original)
                .ok_or_else(|| format!("light.run: `{out_rel}`: unknown extension (use .jpg, .png, .tif, .webp, .avif or .dng)"))?;
            area.check(&out, 0).map_err(|e| format!("light.run: {e}"))?;
            Some((out_rel, out))
        }
        None => None,
    };
    let mut s = Session::new().with_fs();
    let photo = import(&mut s, &input, rel, "run")?;
    // A camera raw is sized from its header when it is imported, and decoded
    // whole when it is developed.
    if let Some(p) = s.catalog.photo(photo) {
        let pixels = u64::from(p.width) * u64::from(p.height);
        if pixels > MAX_INPUT_PIXELS {
            return Err(format!("light.run: `{rel}` is {pixels} pixels, more than the {MAX_INPUT_PIXELS} the service decodes"));
        }
    }
    let mut budget = RunBudget::start(&s, !admitted.is_empty())?;
    let mut results = Vec::with_capacity(admitted.len());
    for (id, params) in admitted {
        budget.before(&id, &params)?;
        let r = s.execute(&id, &params).map_err(|e| format!("light.run {id}: {e}"))?;
        budget.after(&s, &id, &r)?;
        results.push(json!({"id": id, "result": r}));
    }
    let Some((out_rel, out)) = out else { return Ok(json!({"results": results, "out": Json::Null})) };
    let active = s.active().ok_or("light.run: no photo is active to write to `out`")?;
    let mut written = export_one(&mut s, area, active, &out, out_rel, args, "run")?;
    written["results"] = Json::Array(results);
    Ok(written)
}

/// What one `run` call has done so far, against the door's ceilings: at
/// most [`MAX_RUN_PHOTOS`] photos, each photo's settings within
/// [`check_settings`] after every command (whatever wrote them: a slider,
/// `develop.merge`, a preset, a paste), at most [`MAX_DUST_SEARCHES`] dust
/// searches, and its answers within [`MAX_RUN_RESULT_BYTES`].
struct RunBudget {
    /// Each photo's settings as last checked. Holding them makes the
    /// catalog copy them for an edit (it edits in place what nothing else
    /// holds), so the same pointer means unchanged.
    checked: HashMap<u64, Arc<DevelopSettings>>,
    dust_searches: usize,
    result_bytes: u64,
}

impl RunBudget {
    /// The budget of a call on `s` as imported (its settings may come from an
    /// XMP sidecar): with commands to run, refused if it is already over.
    fn start(s: &Session, runs_commands: bool) -> Result<RunBudget, String> {
        if runs_commands {
            for p in s.catalog.photos() {
                check_settings(&p.develop).map_err(|e| format!("light.run: the photo's settings hold {e}, so the door does not run commands on it"))?;
            }
        }
        let checked = s.catalog.photos().map(|p| (p.id.0, p.develop.clone())).collect();
        Ok(RunBudget { checked, dust_searches: 0, result_bytes: 0 })
    }

    /// Before command `id`: its `ids` (every command's, those the gate does
    /// not know too) and the dust searches of the call.
    fn before(&mut self, id: &str, params: &Json) -> Result<(), String> {
        if let Some(n) = ids_named(params).map_err(|e| format!("light.run: `{id}`: {e}"))? {
            if n > MAX_RUN_PHOTOS as f64 {
                return Err(format!("light.run: `{id}`: `ids` names {n} photos, more than the {MAX_RUN_PHOTOS} the door allows"));
            }
        }
        if id == "spot.findDust" {
            self.dust_searches += 1;
            if self.dust_searches > MAX_DUST_SEARCHES {
                return Err(format!("light.run: `spot.findDust` decodes and renders the photo every time: at most {MAX_DUST_SEARCHES} in one call"));
            }
        }
        Ok(())
    }

    /// After command `id`: the photos within the ceiling, the settings of
    /// every photo it changed within [`check_settings`], and its answer
    /// within the results budget.
    fn after(&mut self, s: &Session, id: &str, result: &Json) -> Result<(), String> {
        let n = s.catalog.len();
        if n > MAX_RUN_PHOTOS {
            return Err(format!("light.run: `{id}` leaves {n} photos, more than the {MAX_RUN_PHOTOS} the door allows (the original and its virtual copies)"));
        }
        for p in s.catalog.photos() {
            if !self.checked.get(&p.id.0).is_some_and(|c| Arc::ptr_eq(c, &p.develop) || **c == *p.develop) {
                check_settings(&p.develop).map_err(|e| format!("light.run: `{id}` leaves photo {} with {e}", p.id.0))?;
                self.checked.insert(p.id.0, p.develop.clone());
            }
        }
        self.result_bytes += json_len(result);
        if self.result_bytes > MAX_RUN_RESULT_BYTES {
            return Err(format!("light.run: `{id}`: the results of this call total {} bytes, more than the {MAX_RUN_RESULT_BYTES} the door returns", self.result_bytes));
        }
        Ok(())
    }
}

/// Whether a photo's develop settings are within what the door lets a call
/// render: every slider within its range, the crop inside the photo and at
/// least 1% of it, and the masks, strokes, spots and red-eye corrections
/// within their counts and what they paint ([`MAX_PHOTO_PAINT`]), the
/// whole within [`MAX_SETTINGS_BYTES`]. The engine clamps what its sliders
/// and tools write; `develop.merge`, `mask.update` and a sidecar write
/// anything.
fn check_settings(d: &DevelopSettings) -> Result<(), String> {
    for spec in lightcraft_engine::develop::CONTROLS {
        if let Some(v) = lightcraft_engine::develop::controls::get(d, spec.id) {
            if !(spec.min..=spec.max).contains(&v) {
                return Err(format!("`{}` at {v}, outside its {}..{}", spec.id, spec.min, spec.max));
            }
        }
    }
    let crop = &d.crop.geometry;
    let r = &crop.rect;
    let inside = [r.x0, r.y0, r.x1, r.y1].iter().all(|v| (-1.0..=2.0).contains(v));
    if !(inside && (r.x1 - r.x0).abs() >= 0.01 && (r.y1 - r.y0).abs() >= 0.01 && (-45.0..=45.0).contains(&crop.angle)) {
        return Err(format!("a crop of {}, {} to {}, {} at {}°, not one the door renders (inside the photo, each side at least 1% of it)", r.x0, r.y0, r.x1, r.y1, crop.angle));
    }
    let mut paint = 0.0;
    let parts: usize = d.masks.iter().map(|m| m.components.len()).sum();
    if d.masks.len() > MAX_MASKS || parts > MAX_MASK_PARTS {
        return Err(format!("{} masks of {parts} parts, more than the {MAX_MASKS} masks and {MAX_MASK_PARTS} parts the door allows", d.masks.len()));
    }
    let strokes: Vec<&BrushStroke> = d.masks.iter().flat_map(|m| &m.components).flat_map(|c| match &c.shape {
        MaskShape::Brush { strokes } => strokes.as_slice(),
        _ => &[],
    }).collect();
    if strokes.len() > MAX_STROKES {
        return Err(format!("{} brush strokes, more than the {MAX_STROKES} the door allows", strokes.len()));
    }
    for stroke in strokes {
        check_points(stroke.points.iter().map(|p| (p.x, p.y)))?;
        let points: Vec<(f64, f64)> = stroke.points.iter().map(|p| (p.x, p.y)).collect();
        let painted = paint_of(&points, stroke.size)?;
        if painted > MAX_STROKE_PAINT {
            return Err(format!("a brush stroke painting {painted} image areas, more than the {MAX_STROKE_PAINT} the door allows"));
        }
        paint += painted;
    }
    for c in d.masks.iter().flat_map(|m| &m.components) {
        if let MaskShape::ColorRange { samples, .. } = &c.shape {
            if samples.len() > MAX_COLOR_SAMPLES {
                return Err(format!("{} colour samples, more than the {MAX_COLOR_SAMPLES} the door allows", samples.len()));
            }
        }
    }
    if d.spots.len() > MAX_SPOTS || d.red_eye.len() > MAX_RED_EYES {
        return Err(format!("{} spots and {} red-eye corrections, more than the {MAX_SPOTS} and {MAX_RED_EYES} the door allows", d.spots.len(), d.red_eye.len()));
    }
    for spot in &d.spots {
        paint += check_spot(spot)?;
    }
    if paint > MAX_PHOTO_PAINT {
        return Err(format!("strokes and spots painting {paint} image areas, more than the {MAX_PHOTO_PAINT} the door allows a photo"));
    }
    let bytes = settings_len(d);
    if bytes > MAX_SETTINGS_BYTES as u64 {
        return Err(format!("{bytes} bytes of settings, more than the {MAX_SETTINGS_BYTES} the door allows"));
    }
    Ok(())
}

/// What a spot costs ([`spot_area`]), refused out of the size the engine
/// gives it (0.001–0.25) or past [`MAX_SPOT_COST`].
fn check_spot(spot: &Spot) -> Result<f64, String> {
    check_points(spot.points.iter().map(|p| (p.x, p.y)))?;
    if !(0.0..=0.25).contains(&spot.size) {
        return Err(format!("a spot of size {}, outside the 0..0.25 the door allows", spot.size));
    }
    let points: Vec<(f64, f64)> = spot.points.iter().map(|p| (p.x, p.y)).collect();
    let cost = spot_area(&points, spot.size);
    if cost > MAX_SPOT_COST {
        return Err(format!("a spot costing {cost} image areas, more than the {MAX_SPOT_COST} the door allows"));
    }
    Ok(cost)
}

/// The pixels an original decodes to, from its header, for the formats the
/// engine decodes whole when it imports them: PNG, TIFF (BigTIFF too),
/// WebP, GIF, BMP, PSD and JPEG XL. `None` for the others: a camera raw is
/// sized from its own header (and checked after import), a JPEG decodes at
/// an eighth of its size. A JPEG XL container whose codestream does not
/// start near the top of the file is refused: the door cannot size it.
fn header_pixels(path: &Path) -> Result<Option<u64>, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut head = Vec::new();
    (&mut file).take(64 << 10).read_to_end(&mut head).map_err(|e| e.to_string())?;
    let b = head.as_slice();
    let get = |o: usize, n: usize| b.get(o..o + n);
    let le16 = |o: usize| get(o, 2).map(|x| u64::from(u16::from_le_bytes([x[0], x[1]])));
    let be32 = |o: usize| get(o, 4).map(|x| u64::from(u32::from_be_bytes([x[0], x[1], x[2], x[3]])));
    let le32 = |o: usize| get(o, 4).map(|x| u64::from(u32::from_le_bytes([x[0], x[1], x[2], x[3]])));
    let le24 = |o: usize| get(o, 3).map(|x| u64::from(x[0]) | u64::from(x[1]) << 8 | u64::from(x[2]) << 16);
    let area = |w: Option<u64>, h: Option<u64>| w.zip(h).map(|(w, h)| w * h);
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok(area(be32(16), be32(20)));
    }
    if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        return Ok(area(le16(6), le16(8)));
    }
    if b.starts_with(b"BM") {
        return Ok(match le32(14) {
            Some(12) => area(le16(18), le16(20)),
            Some(_) => area(le32(18).map(|w| u64::from((w as u32 as i32).unsigned_abs())), le32(22).map(|h| u64::from((h as u32 as i32).unsigned_abs()))),
            None => None,
        });
    }
    if b.starts_with(b"8BPS") {
        return Ok(area(be32(18), be32(14)));
    }
    if b.len() >= 30 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        return Ok(match &b[12..16] {
            b"VP8 " => area(le16(26).map(|w| w & 0x3fff), le16(28).map(|h| h & 0x3fff)),
            b"VP8L" => le32(21).map(|v| ((v & 0x3fff) + 1) * (((v >> 14) & 0x3fff) + 1)),
            b"VP8X" => area(le24(24).map(|w| w + 1), le24(27).map(|h| h + 1)),
            _ => None,
        });
    }
    let tiff = match b.get(0..4) {
        Some(b"II*\0") => Some((false, false)),
        Some(b"MM\0*") => Some((true, false)),
        Some(b"II+\0") => Some((false, true)),
        Some(b"MM\0+") => Some((true, true)),
        _ => None,
    };
    if let Some((big_endian, big)) = tiff {
        return tiff_pixels(&mut file, b, big_endian, big);
    }
    if b.starts_with(&[0xFF, 0x0A]) {
        return Ok(jxl_pixels(&b[2..]));
    }
    if b.starts_with(&[0, 0, 0, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A]) {
        let mut at = 0usize;
        while let (Some(size), Some(kind)) = (be32(at), get(at + 4, 4)) {
            let (header, size) = match size {
                1 => (16, get(at + 8, 8).map_or(0, |x| u64::from_be_bytes(x.try_into().unwrap_or_default()) as usize)),
                0 => (8, b.len() - at),
                n => (8, n as usize),
            };
            let start = at + header + if kind == b"jxlp" { 4 } else { 0 };
            if matches!(kind, b"jxlc" | b"jxlp") {
                if let Some(code) = b.get(start..).filter(|c| c.starts_with(&[0xFF, 0x0A])) {
                    return Ok(jxl_pixels(&code[2..]));
                }
                break;
            }
            if size < header {
                break;
            }
            at += size;
        }
        return Err("a JPEG XL file whose size the door cannot read".into());
    }
    Ok(None)
}

/// A TIFF's first image size (tags 256 and 257 of its first IFD).
fn tiff_pixels(file: &mut std::fs::File, head: &[u8], big_endian: bool, big: bool) -> Result<Option<u64>, String> {
    let num = |x: &[u8]| -> u64 {
        let mut v = 0u64;
        for (i, byte) in x.iter().enumerate() {
            v |= u64::from(*byte) << (8 * if big_endian { x.len() - 1 - i } else { i });
        }
        v
    };
    let ifd = if big { head.get(8..16).map(num) } else { head.get(4..8).map(num) };
    let Some(ifd) = ifd else { return Ok(None) };
    let (count_len, entry_len) = if big { (8, 20) } else { (2, 12) };
    let mut read = |offset: u64, len: usize| -> Option<Vec<u8>> {
        let mut buf = vec![0u8; len];
        file.seek(SeekFrom::Start(offset)).ok()?;
        file.read_exact(&mut buf).ok()?;
        Some(buf)
    };
    let Some(count) = read(ifd, count_len).map(|c| num(&c).min(4096) as usize) else { return Ok(None) };
    let Some(entries) = read(ifd + count_len as u64, count * entry_len) else { return Ok(None) };
    let (mut w, mut h) = (None, None);
    for e in entries.chunks(entry_len) {
        let (tag, kind) = (num(&e[0..2]), num(&e[2..4]));
        let value = if big { &e[12..20] } else { &e[8..12] };
        let v = match kind {
            3 => num(&value[0..2]),
            4 => num(&value[0..4]),
            16 => num(&value[0..8]),
            _ => continue,
        };
        match tag {
            256 => w = Some(v),
            257 => h = Some(v),
            _ => {}
        }
    }
    Ok(w.zip(h).map(|(w, h)| w * h))
}

/// A JPEG XL codestream's size (its `SizeHeader`, read after the 0xFF0A
/// signature; bits least significant first).
fn jxl_pixels(code: &[u8]) -> Option<u64> {
    let mut pos = 0usize;
    let mut bits = |n: usize| -> Option<u64> {
        let mut v = 0u64;
        for i in 0..n {
            let bit = (code.get(pos / 8)? >> (pos % 8)) & 1;
            v |= u64::from(bit) << i;
            pos += 1;
        }
        Some(v)
    };
    let small = bits(1)? == 1;
    let dim = |bits: &mut dyn FnMut(usize) -> Option<u64>| -> Option<u64> {
        if small {
            return Some((bits(5)? + 1) * 8);
        }
        let n = [9, 13, 18, 30][bits(2)? as usize];
        Some(bits(n)? + 1)
    };
    let h = dim(&mut bits)?;
    let w = match bits(3)? {
        0 => dim(&mut bits)?,
        r => {
            let (num, den) = [(1, 1), (12, 10), (4, 3), (3, 2), (16, 9), (5, 4), (2, 1)][r as usize - 1];
            h * num / den
        }
    };
    Some(w * h)
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
        let input = place(host, "in.png", &png_bytes());
        for _ in 0..2 {
            serve(&Slot::new(), &service_call("develop", json!({"path": input, "out": "o.jpg"}), host, false)).unwrap();
        }
        assert!(host.join("light/o.jpg").is_file(), "without a resolver the legacy area, replacing as before");
    }

    /// A call as App Hub hands it to the service.
    fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("light.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
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

    /// With the shell's resolver every path is relative to the caller's own
    /// folder and stays inside it, through a link too.
    #[test]
    fn the_resolver_root_is_used_and_paths_stay_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("in.png"), png_bytes()).unwrap();
        let areas = resolver(&root, None);
        let host = dir.path().join(".host");
        let got = serve(&areas, &service_call("info", json!({"path": "in.png"}), &host, false)).unwrap();
        assert_eq!(got["photo"]["width"], json!(12), "{got}");
        serve(&areas, &service_call("develop", json!({"path": "in.png", "out": "out/in.jpg"}), &host, false)).unwrap();
        assert!(root.join("out/in.jpg").is_file() && !host.exists() && !root.join("light").exists());
        std::fs::write(dir.path().join("beside.png"), png_bytes()).unwrap();
        for bad in ["../beside.png", "/etc/hosts", "out/../../beside.png"] {
            assert!(serve(&areas, &service_call("info", json!({"path": bad}), &host, false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("develop", json!({"path": "in.png", "out": bad}), &host, true)).is_err(), "{bad}");
        }
        // A folder is not an original: the engine would walk it.
        std::fs::create_dir(root.join("album")).unwrap();
        assert!(serve(&areas, &service_call("info", json!({"path": "album"}), &host, false)).unwrap_err().contains("not a file"));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            std::os::unix::fs::symlink(dir.path().join("beside.png"), root.join("linked.png")).unwrap();
            assert!(serve(&areas, &service_call("info", json!({"path": "up/beside.png"}), &host, false)).is_err());
            assert!(serve(&areas, &service_call("info", json!({"path": "linked.png"}), &host, false)).is_err());
            assert!(serve(&areas, &service_call("develop", json!({"path": "in.png", "out": "up/o.jpg"}), &host, true)).is_err());
            assert!(!dir.path().join("o.jpg").exists());
        }
    }

    /// The engine reads an original's XMP sidecar beside it, following
    /// links: a sidecar that leads outside the folder refuses the original,
    /// for every method and spelling, before the engine reads anything.
    #[cfg(unix)]
    #[test]
    fn a_sidecar_link_out_of_the_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let secret = dir.path().join("secret.xmp");
        std::fs::write(&secret, r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title><rdf:Alt><rdf:li xml:lang="x-default">outside secret</rdf:li></rdf:Alt></dc:title></rdf:Description></rdf:RDF></x:xmpmeta>"#).unwrap();
        let areas = resolver(&root, None);
        for (n, sidecar) in ["doc{n}.xmp", "doc{n}.XMP", "doc{n}.png.xmp", "doc{n}.png.XMP"].iter().enumerate() {
            let name = format!("doc{n}.png");
            std::fs::write(root.join(&name), png_bytes()).unwrap();
            std::os::unix::fs::symlink(&secret, root.join(sidecar.replace("{n}", &n.to_string()))).unwrap();
            for (method, args) in [
                ("info", json!({"path": name})),
                ("develop", json!({"path": name, "out": format!("o{n}.jpg")})),
                ("batch", json!({"paths": [name], "out_dir": format!("b{n}")})),
            ] {
                let refused = serve(&areas, &service_call(method, args, &root, false)).unwrap_err();
                assert!(refused.contains("sidecar leads outside"), "{sidecar} {method}: {refused}");
                assert!(!refused.contains("outside secret"));
            }
        }
        // A sidecar inside the folder is the engine's to read, as before.
        std::fs::write(root.join("ok.png"), png_bytes()).unwrap();
        std::fs::copy(&secret, root.join("ok.xmp")).unwrap();
        serve(&areas, &service_call("info", json!({"path": "ok.png"}), &root, false)).unwrap();
    }

    /// An agent's call never replaces a file (a batch refuses before it
    /// develops anything); an app's own foreground call may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.png"), png_bytes()).unwrap();
        std::fs::write(dir.path().join("b.png"), png_bytes()).unwrap();
        std::fs::write(dir.path().join("taken.jpg"), b"keep me").unwrap();
        std::fs::create_dir(dir.path().join("out")).unwrap();
        std::fs::write(dir.path().join("out/b.jpg"), b"theirs").unwrap();
        let areas = resolver(dir.path(), None);
        let refused = serve(&areas, &service_call("develop", json!({"path": "a.png", "out": "taken.jpg"}), dir.path(), false)).unwrap_err();
        assert!(refused.contains("`taken.jpg` already exists"), "{refused}");
        let refused = serve(&areas, &service_call("batch", json!({"paths": ["a.png", "b.png"], "out_dir": "out"}), dir.path(), false)).unwrap_err();
        assert!(refused.contains("`out/b.jpg` already exists"), "{refused}");
        assert!(!dir.path().join("out/a.jpg").exists(), "nothing developed first");
        assert_eq!(std::fs::read(dir.path().join("taken.jpg")).unwrap(), b"keep me");
        serve(&areas, &service_call("develop", json!({"path": "a.png", "out": "taken.jpg"}), dir.path(), true)).unwrap();
        assert_ne!(std::fs::read(dir.path().join("taken.jpg")).unwrap(), b"keep me");
    }

    /// What a call writes must fit what is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.png"), png_bytes()).unwrap();
        let refused = serve(&resolver(dir.path(), Some(32)), &service_call("develop", json!({"path": "a.png", "out": "o.jpg"}), dir.path(), true)).unwrap_err();
        assert!(refused.contains("bytes left"), "{refused}");
        assert!(!dir.path().join("o.jpg").exists());
        serve(&resolver(dir.path(), Some(1 << 22)), &service_call("develop", json!({"path": "a.png", "out": "o.jpg"}), dir.path(), true)).unwrap();
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

    /// The names in `dir`, sorted.
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        names.sort();
        names
    }

    /// The door imports the original and runs allowlisted commands on it:
    /// `develop.controls` lists the controls with their ranges,
    /// `develop.auto` runs auto-tone and `develop.set` sets controls, and
    /// the active photo is written to `out` with `quality` and `long_edge`,
    /// byte for byte what `develop` writes for the same edit. A query
    /// without `out` writes nothing, and nothing lands beside the original.
    #[test]
    fn the_door_runs_allowlisted_commands_and_writes_out() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("shot.dng"), dng_bytes()).unwrap();
        let areas = resolver(dir.path(), None);
        let ran = serve(
            &areas,
            &service_call(
                "run",
                json!({"path": "shot.dng", "cmds": [
                    {"id": "develop.controls", "params": {"section": "light"}},
                    {"id": "develop.auto"},
                    {"id": "develop.set", "params": {"values": {"light.exposure": 0.5}}},
                    {"id": "develop.get"}
                ], "out": "out/shot.jpg", "quality": 80, "long_edge": 48}),
                dir.path(),
                false,
            ),
        )
        .unwrap();
        assert_eq!(ran["out"], json!("out/shot.jpg"), "{ran}");
        let results = ran["results"].as_array().unwrap();
        assert_eq!(results.iter().map(|r| r["id"].as_str().unwrap()).collect::<Vec<_>>(), ["develop.controls", "develop.auto", "develop.set", "develop.get"]);
        let exposure = results[0]["result"].as_array().unwrap().iter().find(|c| c["id"] == json!("light.exposure")).expect("light.exposure");
        assert!(exposure["min"].is_number() && exposure["max"].is_number() && exposure["default"].is_number(), "{exposure}");
        assert!(results[1]["result"]["exposure"].is_number(), "auto-tone answers what it set: {}", results[1]["result"]);
        assert_eq!(results[3]["result"]["light"]["exposure"], json!(0.5), "{}", results[3]["result"]);
        assert!(ran["width"].as_u64().unwrap() <= 48 && ran["height"].as_u64().unwrap() <= 48, "long_edge caps the output: {ran}");
        assert!(ran["sidecars"].as_array().is_some_and(|s| s.is_empty()), "{ran}");
        let by_run = std::fs::read(dir.path().join("out/shot.jpg")).unwrap();
        assert!(by_run.starts_with(&[0xFF, 0xD8]), "a JPEG");
        let developed = serve(
            &areas,
            &service_call(
                "develop",
                json!({"path": "shot.dng", "out": "out/developed.jpg", "auto": true, "params": {"light.exposure": 0.5}, "quality": 80, "long_edge": 48}),
                dir.path(),
                false,
            ),
        )
        .unwrap();
        assert_eq!((&developed["width"], &developed["height"]), (&ran["width"], &ran["height"]));
        // The embedded ICC profile records the second it was made, so the
        // two files are compared with that stamp blanked.
        let unstamped = |mut jpeg: Vec<u8>| {
            if let Some(at) = jpeg.windows(12).position(|w| w == b"ICC_PROFILE\0") {
                let header = at + 14;
                if jpeg.len() >= header + 36 {
                    jpeg[header + 24..header + 36].fill(0);
                }
            }
            jpeg
        };
        let developed_bytes = std::fs::read(dir.path().join("out/developed.jpg")).unwrap();
        assert_eq!(unstamped(developed_bytes), unstamped(by_run.clone()), "run covers develop: the same edit, the same bytes");
        let query = serve(&areas, &service_call("run", json!({"path": "shot.dng", "cmds": [{"id": "photo.inspect"}]}), dir.path(), false)).unwrap();
        assert!(query["out"].is_null() && query["results"][0]["result"]["width"] == json!(96), "{query}");
        assert_eq!(query["results"][0]["result"]["source"]["path"], json!("shot.dng"), "host paths read relative: {query}");
        assert_eq!(names(dir.path()), ["out", "shot.dng"], "nothing beside the original");
        assert_eq!(names(&dir.path().join("out")), ["developed.jpg", "shot.jpg"]);
    }

    /// Every class but `safe` is refused (lightcraft has no `code` id), and
    /// so is an id the classification does not know, before any command
    /// runs: a refused id anywhere in the list writes nothing.
    #[test]
    fn the_door_refuses_every_other_class_and_unknown_ids() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("in.png"), png_bytes()).unwrap();
        let areas = resolver(dir.path(), None);
        let run = |cmds: Json| serve(&areas, &service_call("run", json!({"path": "in.png", "cmds": cmds, "out": "x.jpg"}), dir.path(), true));
        for (id, class) in [
            ("segment.model.download", "network"),
            ("library.devices", "device"),
            ("app.gpu", "host"),
            ("app.memoryBudget", "host"),
            ("library.preferences", "host"),
            ("library.autoImport", "host"),
            ("library.smartPreviewsLocation", "host"),
        ] {
            let e = run(json!([{"id": "develop.auto"}, {"id": id}])).unwrap_err();
            assert!(e.contains(&format!("`{id}` is classed {class}")) && e.contains("never runs it"), "{id}: {e}");
        }
        for id in ["edit.undo", "edit.redo", "library.import", "photo.saveMetadataToFile", "photo.convertToDng", "photo.rename", "profile.import", "export.checkTarget"] {
            let e = run(json!([{"id": "develop.auto"}, {"id": id}])).unwrap_err();
            assert!(e.contains(&format!("`{id}` reads or writes files")) && e.contains("not reviewed to run through it"), "{id}: {e}");
        }
        let e = run(json!([{"id": "light.secret"}])).unwrap_err();
        assert!(e.contains("`light.secret` is not a reviewed light command"), "{e}");
        assert!(run(json!([{"id": ""}])).unwrap_err().contains("each command has an `id`"));
        assert!(run(json!([{"id": "develop.set", "params": [1]}])).unwrap_err().contains("`params` is an object"));
        let too_many: Vec<Json> = (0..65).map(|_| json!({"id": "develop.get"})).collect();
        assert!(run(Json::Array(too_many)).unwrap_err().contains("at most 64"));
        assert!(serve(&areas, &service_call("run", json!({"cmds": []}), dir.path(), true)).unwrap_err().contains("`path` is required"));
        assert!(serve(&areas, &service_call("run", json!({"path": "in.png"}), dir.path(), true)).unwrap_err().contains("`cmds` is a list"));
        assert_eq!(names(dir.path()), ["in.png"], "nothing written");
    }

    /// `library.xmpPreferences` turns on XMP auto-write, after which the
    /// engine writes a sidecar beside the original on every catalog change.
    /// The fixture is live on the engine itself; through the door both
    /// switches are refused before anything runs, so no `.xmp` appears
    /// beside the caller's original.
    #[test]
    fn xmp_auto_write_is_refused_and_nothing_lands_beside_the_original() {
        let live = tempfile::tempdir().unwrap();
        let original = live.path().join("in.png");
        std::fs::write(&original, png_bytes()).unwrap();
        let mut engine = Session::new().with_fs();
        import(&mut engine, &original, "in.png", "test").unwrap();
        engine.execute("library.xmpPreferences", &json!({"autoWrite": true})).unwrap();
        engine.execute("develop.set", &json!({"values": {"light.exposure": 1.0}})).unwrap();
        assert!(live.path().join("in.xmp").is_file(), "the fixture is live: the engine writes a sidecar beside the original");

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("in.png"), png_bytes()).unwrap();
        let areas = resolver(dir.path(), None);
        for switch in [json!({"id": "library.xmpPreferences", "params": {"autoWrite": true}}), json!({"id": "library.toggleAutoWriteXmp"})] {
            let cmds = json!([switch, {"id": "develop.set", "params": {"values": {"light.exposure": 1.0}}}, {"id": "photo.rate", "params": {"rating": 5}}]);
            for out in [json!("o.jpg"), Json::Null] {
                let e = serve(&areas, &service_call("run", json!({"path": "in.png", "cmds": cmds, "out": out}), dir.path(), true)).unwrap_err();
                assert!(e.contains("is classed host") && e.contains("never runs it"), "{e}");
            }
        }
        // The same edit without the switch runs, and still writes no sidecar.
        serve(
            &areas,
            &service_call(
                "run",
                json!({"path": "in.png", "cmds": [{"id": "develop.set", "params": {"values": {"light.exposure": 1.0}}}, {"id": "photo.rate", "params": {"rating": 5}}], "out": "o.jpg"}),
                dir.path(),
                true,
            ),
        )
        .unwrap();
        assert_eq!(names(dir.path()), ["in.png", "o.jpg"], "no `.xmp` beside the original");
    }

    /// The door's paths keep the area's rules: commands that read or write
    /// paths of their own are refused (`library.import` of an outside file,
    /// `edit.undo`, which moves files on disk); `path` and `out` stay inside
    /// the folder, through links too; `out` never replaces an existing file
    /// in an agent's call, never an original in any call, and keeps to the
    /// quota; an unknown format is refused before anything runs.
    #[test]
    fn the_doors_paths_keep_the_areas_rules() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("in.png"), png_bytes()).unwrap();
        std::fs::write(dir.path().join("outside.png"), png_bytes()).unwrap();
        let areas = resolver(&root, None);
        let call = |args: Json, may_prompt: bool| serve(&areas, &service_call("run", args, &root, may_prompt));
        let outside = dir.path().join("outside.png").to_string_lossy().into_owned();
        for paths in [json!([outside.clone()]), json!(["../outside.png"]), json!([dir.path().to_string_lossy()])] {
            let e = call(json!({"path": "in.png", "cmds": [{"id": "library.import", "params": {"paths": paths}}], "out": "o.jpg"}), true).unwrap_err();
            assert!(e.contains("`library.import` reads or writes files") && e.contains("not reviewed"), "{e}");
        }
        let e = call(json!({"path": "in.png", "cmds": [{"id": "develop.set", "params": {"values": {"light.exposure": 1.0}}}, {"id": "edit.undo"}]}), true).unwrap_err();
        assert!(e.contains("`edit.undo` reads or writes files"), "{e}");
        // An agent's `out` never replaces a file, before the engine works.
        std::fs::write(root.join("taken.jpg"), b"keep me").unwrap();
        let e = call(json!({"path": "in.png", "cmds": [{"id": "develop.auto"}], "out": "taken.jpg"}), false).unwrap_err();
        assert!(e.contains("`taken.jpg` already exists"), "{e}");
        assert_eq!(std::fs::read(root.join("taken.jpg")).unwrap(), b"keep me");
        // An app's own call may replace, but never the original itself.
        let e = call(json!({"path": "in.png", "cmds": [{"id": "develop.auto"}], "out": "in.png"}), true).unwrap_err();
        assert!(e.contains("never writes over an original"), "{e}");
        assert_eq!(std::fs::read(root.join("in.png")).unwrap(), png_bytes());
        call(json!({"path": "in.png", "cmds": [{"id": "develop.auto"}], "out": "taken.jpg"}), true).unwrap();
        assert_ne!(std::fs::read(root.join("taken.jpg")).unwrap(), b"keep me");
        for out in ["o.gif", "in.original"] {
            let e = call(json!({"path": "in.png", "cmds": [], "out": out}), true).unwrap_err();
            assert!(e.contains("unknown extension"), "{out}: {e}");
        }
        let tight = resolver(&root, Some(32));
        let e = serve(&tight, &service_call("run", json!({"path": "in.png", "cmds": [], "out": "big.jpg"}), &root, true)).unwrap_err();
        assert!(e.contains("bytes left"), "{e}");
        for bad in ["../outside.png", "/etc/hosts", "a/../../outside.png", ""] {
            assert!(call(json!({"path": bad, "cmds": [], "out": "o.jpg"}), true).is_err(), "{bad}");
        }
        for bad in ["../o.jpg", "/tmp/o.jpg", "a/../../o.jpg"] {
            assert!(call(json!({"path": "in.png", "cmds": [], "out": bad}), true).is_err(), "{bad}");
        }
        let mut left = vec!["in.png", "taken.jpg"];
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            std::os::unix::fs::symlink(dir.path().join("outside.png"), root.join("linked.png")).unwrap();
            assert!(call(json!({"path": "up/outside.png", "cmds": [], "out": "o.jpg"}), true).is_err());
            assert!(call(json!({"path": "linked.png", "cmds": [], "out": "o.jpg"}), true).is_err());
            assert!(call(json!({"path": "in.png", "cmds": [], "out": "up/o.jpg"}), true).is_err());
            left.extend(["linked.png", "up"]);
        }
        left.sort();
        assert_eq!(names(&root), left, "nothing else written in the folder");
        assert_eq!(names(dir.path()), ["outside.png", "workspace"], "nothing written beside the folder");
    }

    /// The door's gate is built from the generated classification: it runs
    /// every `safe` id, the develop commands among them, and nothing else.
    #[test]
    fn the_door_is_built_from_the_reviewed_classification() {
        let door = door().unwrap();
        for id in ["develop.set", "develop.auto", "develop.controls", "develop.get", "library.select", "photo.inspect", "preset.apply"] {
            assert!(door.runs(id), "{id}");
        }
        for id in ["edit.undo", "library.import", "library.xmpPreferences", "library.toggleAutoWriteXmp", "segment.model.download", "library.devices", "light.secret"] {
            assert!(!door.runs(id), "{id}");
        }
        let safety: Json = serde_json::from_str(include_str!("../skill/safety.json")).unwrap();
        let safe = safety["commands"].as_object().unwrap().values().filter(|c| *c == "safe").count();
        assert_eq!(door.runnable().len(), safe);
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

    /// Every `light.run` call the skill's examples show runs, in order,
    /// in one area as the system agent's (with the files they name placed
    /// there first), and writes its `out`: the skill teaches commands that
    /// work.
    #[test]
    fn the_skill_examples_run() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        std::fs::write(dir.path().join("IMG_0042.dng"), dng_bytes()).unwrap();
        let body = include_str!("../skill/SKILL.md");
        let examples = body.split("\n## Examples").nth(1).and_then(|rest| rest.split("\n## ").next()).unwrap();
        let mut ran = 0;
        for span in examples.split('`').skip(1).step_by(2) {
            let Some(args) = span.strip_prefix("light.run ") else { continue };
            let args: Json = serde_json::from_str(args).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            let got = serve(&areas, &service_call("run", args.clone(), dir.path(), false)).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            assert_eq!(got["results"].as_array().map(Vec::len), args["cmds"].as_array().map(Vec::len), "{got}");
            if let Some(out) = args["out"].as_str() {
                assert!(dir.path().join(out).is_file(), "`{span}` wrote no {out}");
            }
            ran += 1;
        }
        assert!(ran >= 3, "{ran} examples");
        let controls = serve(&areas, &service_call("run", json!({"path": "IMG_0042.dng", "cmds": [{"id": "develop.controls", "params": {"section": "color"}}]}), dir.path(), false)).unwrap();
        assert!(controls["results"][0]["result"].to_string().contains("color.vibrance"), "{controls}");
        assert!(!dir.path().join("IMG_0042.xmp").exists(), "nothing beside the original");
    }

    /// One `light.run` call on `path` in `dir`, as the system agent makes it.
    fn run_on(dir: &Path, path: &str, cmds: Json, extra: Json) -> Result<Json, String> {
        let mut args = json!({"path": path, "cmds": cmds});
        for (k, v) in extra.as_object().into_iter().flatten() {
            args[k] = v.clone();
        }
        serve(&resolver(dir, None), &service_call("run", args, dir, true))
    }

    /// Every limit of the door: a command at (or just within) the cap passes
    /// the gate, one over is refused before anything runs.
    #[test]
    fn the_doors_limits_pass_at_their_cap_and_refuse_one_over() {
        let dir = tempfile::tempdir().unwrap();
        let area = Area::new(dir.path(), None, false);
        let gate = |id: &str, params: Json| door().unwrap().admit_all(&json!([{"id": id, "params": params}]), &area).map(|_| ());
        let stroke = |size: f64| json!({"points": [[-1.0, 0.5], [2.0, 0.5]], "size": size});
        let spot = |n: usize| json!({"points": (0..n).map(|i| if i % 2 == 0 { [0.0, 0.0] } else { [1.0, 1.0] }).collect::<Vec<_>>()});
        let samples = |n: usize| json!({"kind": "colorRange", "samples": vec![[0.5, 0.0, 0.0]; n]});
        let ids = |n: u64| json!({"ids": (1..=n).collect::<Vec<_>>(), "rating": 3});
        let big = |key: &str, n: usize| json!({key: {"name": "x".repeat(n)}});
        let cases = [
            // 4π × 3 long edges × 0.42 = 15.8 image areas; × 0.43 = 16.2.
            ("mask.brushStroke", stroke(0.42), stroke(0.43), "asks for 16.2"),
            ("spot.add", spot(16), spot(17), "asks for 17 image areas of spot, more than the 16"),
            ("mask.add", samples(16), samples(17), "asks for 17 colour samples, more than the 16"),
            ("crop.aspect", json!({"aspect": "100x1"}), json!({"aspect": "101x1"}), "asks for 101 to 1 crop ratio, more than the 100"),
            ("crop.aspect", json!({"aspect": [1, 100]}), json!({"aspect": [1, 101]}), "asks for 101 to 1 crop ratio, more than the 100"),
            ("photo.rate", ids(16), ids(17), "asks for 17 photos in `ids`, more than the 16"),
            ("photo.virtualCopy", ids(16), ids(17), "asks for 17 photos in `ids`, more than the 16"),
            ("develop.merge", big("settings", 1000), big("settings", MAX_SETTINGS_BYTES), "bytes of settings, more than the 1048576"),
            ("mask.update", big("shape", 1000), big("shape", MAX_SETTINGS_BYTES), "bytes of settings, more than the 1048576"),
        ];
        for (id, at, over, refusal) in cases {
            gate(id, at).unwrap_or_else(|e| panic!("{id} at the cap: {e}"));
            let e = gate(id, over).unwrap_err();
            assert!(e.contains(&format!("`{id}`")) && e.contains(refusal), "{id}: {e}");
        }
        // The other ways a stroke, a spot or a crop asks for too much.
        let refused = [
            ("mask.brushStroke", json!({"points": [[0.0, 0.0], [1e9, 0.0]]}), "outside the -1..2"),
            ("mask.brushStroke", json!({"points": [[0.0, 0.0], [1.0, 0.0]], "size": 1.5}), "brush size of 1.5"),
            ("mask.brushStroke", json!({"points": (0..40).map(|i| [if i % 2 == 0 { -1.0 } else { 2.0 }, 0.5]).collect::<Vec<_>>(), "size": 0.001}), "long edges long, more than the 100"),
            ("mask.brushStroke", json!({"points": vec![[0.5, 0.5]; MAX_POINTS + 1]}), "points, more than the 10000"),
            ("spot.add", json!({"points": [[5.0, 5.0]]}), "outside the -1..2"),
            ("crop.set", json!({"rect": [0.0, 0.0, 0.0099, 1.0]}), "a crop side is at least 1% of the photo"),
            ("photo.rate", json!({"ids": [1, 1], "rating": 2}), "`ids` names photo 1 twice"),
        ];
        for (id, params, why) in refused {
            let e = gate(id, params).unwrap_err();
            assert!(e.contains(why), "{id}: {e}");
        }
        gate("crop.set", json!({"rect": [0.0, 0.0, 0.01, 0.01]})).unwrap();
    }

    /// The `ids` limits cover every command of the door whose engine spec
    /// takes `ids`: one added upstream fails here until it is reviewed.
    #[test]
    fn every_command_that_takes_ids_has_its_limit() {
        let safety: Json = serde_json::from_str(include_str!("../skill/safety.json")).unwrap();
        let mut documented: Vec<&str> = include_str!("../skill/commands.md")
            .lines()
            .filter_map(|l| l.strip_prefix("- `")?.split_once('`'))
            .filter(|(id, rest)| safety["commands"][*id] == json!("safe") && rest.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| w == "ids"))
            .map(|(id, _)| id)
            .collect();
        documented.sort();
        let mut limited: Vec<&str> = REVIEWED.limits.iter().filter(|l| l.what == "photos in `ids`").map(|l| l.id).collect();
        limited.sort();
        assert_eq!(limited, documented);
    }

    /// Select all and make virtual copies doubles the catalog every round,
    /// with no count for the gate to see: the photo ceiling stops it.
    #[test]
    fn a_virtual_copy_loop_is_stopped_by_the_photo_ceiling() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("in.png"), png_bytes()).unwrap();
        let cmds: Vec<Json> = (0..5).flat_map(|_| [json!({"id": "library.selectAll"}), json!({"id": "photo.virtualCopy"})]).collect();
        let e = run_on(dir.path(), "in.png", Json::Array(cmds), json!({"out": "o.jpg"})).unwrap_err();
        assert!(e.contains("`photo.virtualCopy` leaves 32 photos, more than the 16 the door allows"), "{e}");
        assert!(!dir.path().join("o.jpg").exists());
    }

    /// `develop.merge` writes settings unchecked: a crop out of the photo
    /// (an export larger than the original), dozens of masks, a noise
    /// reduction past its slider are refused after the command, before any
    /// render; within the bounds it runs.
    #[test]
    fn unchecked_settings_are_refused_after_the_command() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("in.png"), png_bytes()).unwrap();
        let merge = |settings: Json| json!([{"id": "develop.merge", "params": {"settings": settings}}]);
        for (settings, why) in [
            (json!({"crop": {"geometry": {"rect": {"x0": 0.0, "y0": 0.0, "x1": 50.0, "y1": 50.0}}}}), "a crop of 0, 0 to 50, 50"),
            (json!({"crop": {"geometry": {"rect": {"x0": 0.5, "y0": 0.5, "x1": 0.501, "y1": 0.9}}}}), "each side at least 1% of it"),
            (json!({"masks": vec![json!({}); 17]}), "17 masks"),
            (json!({"detail": {"nr_luminance": 1e6}}), "`detail.nrLuminance` at 1000000, outside its 0..100"),
            (json!({"spots": vec![json!({"points": [[0.5, 0.5]]}); 65]}), "65 spots"),
            (json!({"masks": [{"components": [{"shape": {"kind": "brush", "strokes": [{"points": [[0.0, 0.0], [1e12, 0.0]]}]}}]}]}), "outside the -1..2"),
        ] {
            let e = run_on(dir.path(), "in.png", merge(settings.clone()), json!({"out": "o.jpg"})).unwrap_err();
            assert!(e.contains("`develop.merge` leaves photo") && e.contains(why), "{settings}: {e}");
        }
        assert!(!dir.path().join("o.jpg").exists());
        let ok = merge(json!({"detail": {"nr_luminance": 40.0}, "masks": [{"components": [{"shape": {"kind": "radial", "center": {"x": 0.5, "y": 0.5}, "rx": 0.2, "ry": 0.2, "angle": 0.0, "feather": 50.0, "invert": false}}]}]}));
        run_on(dir.path(), "in.png", ok, json!({"out": "ok.jpg"})).unwrap();
    }

    /// A PNG of `w` × `h` grey pixels, stored (uncompressed), as a quick
    /// large original.
    fn grey_png(w: u32, h: u32) -> Vec<u8> {
        fn crc32(data: &[u8]) -> u32 {
            let mut c = 0xFFFF_FFFFu32;
            for &b in data {
                c ^= u32::from(b);
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                }
            }
            !c
        }
        fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
            out.extend((data.len() as u32).to_be_bytes());
            let body: Vec<u8> = kind.iter().chain(data).copied().collect();
            out.extend(&body);
            out.extend(crc32(&body).to_be_bytes());
        }
        let row: Vec<u8> = std::iter::once(0).chain(std::iter::repeat_n(128, w as usize * 3)).collect();
        let raw: Vec<u8> = std::iter::repeat_n(row, h as usize).flatten().collect();
        let (mut a, mut b) = (1u32, 0u32);
        for &v in &raw {
            a = (a + u32::from(v)) % 65521;
            b = (b + a) % 65521;
        }
        let mut z = vec![0x78, 0x01];
        let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
        for (i, block) in blocks.iter().enumerate() {
            z.push(u8::from(i + 1 == blocks.len()));
            z.extend((block.len() as u16).to_le_bytes());
            z.extend((!(block.len() as u16)).to_le_bytes());
            z.extend(*block);
        }
        z.extend(((b << 16) | a).to_be_bytes());
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let ihdr: Vec<u8> = w.to_be_bytes().into_iter().chain(h.to_be_bytes()).chain([8, 2, 0, 0, 0]).collect();
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &z);
        chunk(&mut out, b"IEND", &[]);
        out
    }

    /// The export's size: a rendered image within its pixel cap, refused
    /// before it renders (AVIF lower), such as a crop `develop.merge` set
    /// three times the photo across; `long_edge` read whatever way a whole
    /// number is written.
    #[test]
    fn the_export_stays_within_its_pixels() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("shot.dng"), dng_bytes()).unwrap();
        let ran = run_on(dir.path(), "shot.dng", json!([]), json!({"out": "small.jpg", "long_edge": 48.0})).unwrap();
        assert!(ran["width"].as_u64().unwrap() <= 48, "a long edge written 48.0 still caps the export: {ran}");
        std::fs::write(dir.path().join("grey.png"), grey_png(1400, 1400)).unwrap();
        let crop = |lo: f64, hi: f64| json!([{"id": "develop.merge", "params": {"settings": {"crop": {"geometry": {"rect": {"x0": lo, "y0": lo, "x1": hi, "y1": hi}}}}}}]);
        let e = run_on(dir.path(), "grey.png", crop(-1.0, 2.0), json!({"out": "big.jpg"})).unwrap_err();
        assert!(e.contains("the export would be 4200 × 4200 = 17640000 pixels, more than the 16000000"), "{e}");
        let e = run_on(dir.path(), "grey.png", crop(-0.5, 1.5), json!({"out": "big.avif"})).unwrap_err();
        assert!(e.contains("the export would be 2800 × 2800 = 7840000 pixels, more than the 4000000"), "{e}");
        assert!(!dir.path().join("big.jpg").exists() && !dir.path().join("big.avif").exists());
        let ran = run_on(dir.path(), "grey.png", crop(-1.0, 2.0), json!({"out": "big-small.jpg", "long_edge": 256})).unwrap();
        assert_eq!(ran["width"], json!(256), "{ran}");
    }

    /// A small file whose header claims an enormous image (the engine would
    /// decode it whole when it imports it) is refused before it is read,
    /// for every format the engine decodes whole, through every method.
    #[test]
    fn an_original_too_large_to_decode_is_refused_from_its_header() {
        let dir = tempfile::tempdir().unwrap();
        let be = |n: u32| n.to_be_bytes().to_vec();
        let le = |n: u32| n.to_le_bytes().to_vec();
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend(be(40_000).into_iter().chain(be(40_000)).chain([8, 2, 0, 0, 0]));
        let mut gif = b"GIF89a".to_vec();
        gif.extend([0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0]);
        let mut bmp = b"BM".to_vec();
        bmp.extend([0u8; 12].into_iter().chain(le(40)).chain(le(20_000)).chain(le((-20_000i32) as u32)));
        let mut psd = b"8BPS\0\x01\0\0\0\0\0\0\0\x03".to_vec();
        psd.extend(be(30_000).into_iter().chain(be(30_000)));
        let mut webp = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0\0\0\0\0".to_vec();
        webp.extend([0xFF, 0x3F, 0x00, 0xFF, 0x3F, 0x00, 0, 0]);
        let mut tiff = b"II*\0".to_vec();
        tiff.extend(le(8).into_iter().chain([2, 0]).chain([0, 1, 4, 0]).chain(le(1)).chain(le(20_000)).chain([1, 1, 4, 0]).chain(le(1)).chain(le(20_000)).chain(le(0)));
        let mut tiff_be = b"MM\0*".to_vec();
        tiff_be.extend(be(8).into_iter().chain([0, 2]).chain([1, 0, 0, 3]).chain(be(1)).chain([0x4E, 0x20, 0, 0]).chain([1, 1, 0, 3]).chain(be(1)).chain([0x4E, 0x20, 0, 0]));
        // JPEG XL: SizeHeader with a 30-bit height of 20,000 and a 1:1 ratio.
        let mut bits: Vec<bool> = vec![false, true, true];
        bits.extend((0..30).map(|i| (19_999u32 >> i) & 1 == 1));
        bits.extend([true, false, false]);
        let mut jxl = vec![0xFF, 0x0A];
        jxl.extend(bits.chunks(8).map(|c| c.iter().enumerate().fold(0u8, |b, (i, on)| b | (u8::from(*on) << i))));
        for (name, bytes, pixels) in [
            ("bomb.png", png, 1_600_000_000u64),
            ("bomb.gif", gif, 65_535 * 65_535),
            ("bomb.bmp", bmp, 400_000_000),
            ("bomb.psd", psd, 900_000_000),
            ("bomb.webp", webp, 16_384 * 16_384),
            ("bomb.tif", tiff, 400_000_000),
            ("bomb-be.tif", tiff_be, 400_000_000),
            ("bomb.jxl", jxl, 400_000_000),
        ] {
            std::fs::write(dir.path().join(name), bytes).unwrap();
            for method in ["run", "info"] {
                let args = json!({"path": name, "cmds": [], "out": "o.jpg"});
                let e = serve(&resolver(dir.path(), None), &service_call(method, args, dir.path(), true)).unwrap_err();
                assert!(e.contains(&format!("`{name}` is {pixels} pixels, more than the 64000000 the service decodes")), "{method} {name}: {e}");
            }
        }
        // Ordinary photos still pass.
        assert_eq!(header_pixels(&{
            std::fs::write(dir.path().join("ok.png"), png_bytes()).unwrap();
            dir.path().join("ok.png")
        })
        .unwrap(), Some(96));
    }

    /// `spot.findDust` decodes and renders the photo each time, caching
    /// neither: a few a call.
    #[test]
    fn dust_searches_are_bounded_per_call() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("in.png"), png_bytes()).unwrap();
        let dust = |n: usize| Json::Array((0..n).map(|_| json!({"id": "spot.findDust", "params": {"add": false}})).collect());
        run_on(dir.path(), "in.png", dust(4), json!({})).unwrap();
        let e = run_on(dir.path(), "in.png", dust(5), json!({})).unwrap_err();
        assert!(e.contains("at most 4 in one call"), "{e}");
    }

    /// Every result is kept until the reply: `export.savePreset` answers every
    /// preset each time, so the results of a call share a budget.
    #[test]
    fn the_results_of_a_call_share_a_budget() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("in.png"), png_bytes()).unwrap();
        let cmds: Vec<Json> = (0..12).map(|i| json!({"id": "export.savePreset", "params": {"name": format!("p{i}"), "params": {"note": "x".repeat(1_000_000)}}})).collect();
        let e = run_on(dir.path(), "in.png", Json::Array(cmds), json!({})).unwrap_err();
        assert!(e.contains("`export.savePreset`: the results of this call total") && e.contains("more than the 67108864 the door returns"), "{e}");
    }

    /// A command the gate's `ids` limits do not list still has its `ids`
    /// checked before it runs.
    #[test]
    fn every_commands_ids_are_checked() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("in.png"), png_bytes()).unwrap();
        let e = run_on(dir.path(), "in.png", json!([{"id": "stack.setTop", "params": {"ids": [1, 1]}}]), json!({})).unwrap_err();
        assert!(e.contains("`stack.setTop`: `ids` names photo 1 twice"), "{e}");
    }
}
