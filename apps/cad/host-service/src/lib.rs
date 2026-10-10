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
//!   no name reviewed; every other id is refused before any command runs,
//!   and so is a parameter past its reviewed cap (array and copy counts,
//!   fit points, pattern scales…). Around each command the service bounds
//!   what the call's drawings may grow to and the work that scales with
//!   them ([`caps`]).
//!
//! Paths never leave the area: `..`, absolute paths and symlink escapes are
//! refused. Engine work runs on the shell's UI thread, so a drawing whose
//! extents would take too long to compute is refused when read, and a PNG,
//! SVG or PDF too heavy to draw (or a DXF or DWG too heavy to write) before
//! the engine starts on it, by every method ([`caps`]). The service serves
//! system apps only until ADR 0013's store capability is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use cadcraft_engine::doc::{Drawing, Space};
use cadcraft_engine::Session;
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::door::{Door, Limit, Measure, Reviewed, Setter};
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
///
/// The limits bound every parameter of the 288 commands the door runs that
/// multiplies work or memory, read from their implementations at the pinned
/// cadcraft (`crates/engine/src/cmd/*.rs`, the renderer in `crates/render`):
/// engine work runs on the shell's UI thread, so each cap sits well above
/// everyday drafting and far below what stalls it (about a second of engine
/// work, a few hundred megabytes). What no parameter shows (how large the
/// drawing already is, how fine a hatch is for its boundary, how deep its
/// blocks nest) the service bounds around every command instead ([`caps`]).
static REVIEWED: Reviewed = Reviewed {
    file_reads: &[],
    setters: &[Setter { id: "setvar", keys: &[], keys_of: setvar_keys }],
    inner: &[],
    limits: &[
        ARRAYRECT,
        ARRAYPOLAR,
        ARRAYPATH,
        COPY,
        DIVIDE,
        DIVIDE_BLOCKS,
        POLYGON,
        SPLINE_FIT,
        SPLINE_CONTROL,
        SPLINE_DEGREE,
        SPLINEDIT_FIT,
        TABLE,
        HATCH_POINTS,
        GRADIENT_POINTS,
        BOUNDARY_POINTS,
        HATCH_SCALE,
        HATCHEDIT_SCALE,
        PROPERTIES_SCALE,
        LTSCALE,
        PROPERTIES_LTSCALE,
        ENTITIES_LIMIT,
        INSPECT_LIMIT,
        CAL_EXPR,
        FIND_REPLACE,
        DIMSTYLE_TEMPLATE,
        DIMSTYLE_DIMENSION_TEMPLATE,
        DIMSTYLE_OVERRIDE_TEMPLATE,
        DIMOVERRIDE_TEMPLATE,
        DIMLINEAR_TEXT,
        DIMALIGNED_TEXT,
        TEXTEDIT_TEXT,
        PROPERTIES_TEXT_OVERRIDE,
    ],
    copies_per_call: COPIES_PER_CALL,
    held: &[],
};

/// The variable one `setvar` call sets, as the engine spells it.
fn setvar_keys(params: &Json) -> Vec<String> {
    params["name"].as_str().map(|name| vec![name.trim().to_ascii_uppercase()]).unwrap_or_default()
}

/// The most copies one array or copy command may make of each object it
/// copies: a tenth of AutoCAD's own MAXARRAY default (100,000), well above
/// a drafted grid of columns or a bolt circle, and the most copies one call
/// may make in all ([`COPIES_PER_CALL`]).
const MAX_COPIES: f64 = 10_000.0;

/// The most the copies of one call may multiply to: an array of an array
/// copies the first array's copies again, so the copying commands of a call
/// (arrays, `copy {count}`, divisions into block references) multiply
/// together within this. Independent arrays in one call multiply too, so a
/// call with several large arrays is split into calls; the service's own
/// ceilings on what a drawing may hold ([`caps`]) bound the result either way.
const COPIES_PER_CALL: f64 = 10_000.0;

/// The most fit points a spline is fitted through in one command:
/// `Spline::from_fit_points` solves a dense n × n system it clones once
/// (`geom/src/spline.rs`), 2 · n² · 8 bytes, 64 MB and ~70 ms at 2,000,
/// 3.2 GB at the 20,000 `splinedit` accepts. Hand or survey splines carry
/// tens to hundreds.
const MAX_FIT_POINTS: f64 = 2_000.0;

/// The smallest pattern or linetype scale the door passes (its inverse is
/// what the limits weigh). Everyday scales run from about 0.01 (inch-based
/// patterns in a metre drawing) to 1,000; far below, the engine's own guards
/// already fall back (a stroke past 50,000 dashes is drawn solid, a hatch
/// family past 20,000 lines is skipped), so a smaller scale is never useful.
/// What a moderate scale costs on a large object is bounded by the service
/// before anything is drawn ([`caps::drawable`]).
const MIN_PATTERN_SCALE: f64 = 1e-4;

/// The most `<>` measurement placeholders a dimension text template may
/// hold: the engine replaces each `<>` of DIMPOST with the formatted
/// measurement, and each `<>` of a dimension's own text with all of that, so
/// the placeholders of the two multiply (`render/src/dim.rs` `post`,
/// `apply_override`); a template uses one (`≈<> mm`).
const MAX_PLACEHOLDERS: f64 = 8.0;

/// A count parameter as the gate weighs it: absent or null, the engine's own
/// default; a number or a numeric string by its size; anything else refused.
/// The engine reads counts with `as_u64` and falls back to its default for
/// anything else, so this never weighs less than what the engine does.
fn count_param(p: &Json, key: &str, default: f64) -> Result<f64, String> {
    let n = match p.get(key) {
        None | Some(Json::Null) => return Ok(default),
        Some(Json::Number(n)) => n.as_f64(),
        Some(Json::String(s)) => s.trim().parse::<f64>().ok(),
        Some(_) => None,
    };
    n.filter(|n| n.is_finite()).map(f64::abs).ok_or_else(|| format!("`{key}` is a number the door bounds"))
}

/// How many entries an array parameter holds (`None` when it is no array:
/// the engine then ignores it).
fn array_len(p: &Json, key: &str) -> Option<f64> {
    p.get(key).and_then(Json::as_array).map(|a| a.len() as f64)
}

/// `arrayrect`: `rows` × `cols`, 3 × 4 when absent (`modify.rs` `run_arrayrect`).
fn arrayrect_items(p: &Json) -> Result<Option<f64>, String> {
    Ok(Some(count_param(p, "rows", 3.0)? * count_param(p, "cols", 4.0)?))
}

/// `arraypolar`: `count`, 6 when absent (`modify.rs` `run_arraypolar`).
fn arraypolar_items(p: &Json) -> Result<Option<f64>, String> {
    Ok(Some(count_param(p, "count", 6.0)?))
}

/// Whether `arraypath` spaces its items by `spacing` (a positive number,
/// which the engine prefers to `count`) rather than counting them.
fn spaced_along_path(p: &Json) -> bool {
    p.get("spacing").and_then(Json::as_f64).is_some_and(|sp| sp.is_finite() && sp > 0.0)
}

/// `arraypath`: `count`, 6 when absent (`modify2.rs` `run_arraypath`). With
/// `spacing` the count is the path's length over the spacing, which only
/// the drawing shows: the service weighs that before the command runs.
fn arraypath_items(p: &Json) -> Result<Option<f64>, String> {
    if spaced_along_path(p) {
        return Ok(None);
    }
    Ok(Some(count_param(p, "count", 6.0)?))
}

/// `divide {block}`: one reference to the block per division point.
fn divide_blocks(p: &Json) -> Result<Option<f64>, String> {
    if p.get("block").is_some_and(Json::is_string) {
        return Ok(Some(count_param(p, "segments", 1.0)?));
    }
    Ok(None)
}

/// `spline {fit}`: the fit points it interpolates.
fn spline_fit_points(p: &Json) -> Result<Option<f64>, String> {
    Ok(array_len(p, "fit"))
}

/// `spline {control}`: its control points.
fn spline_control_points(p: &Json) -> Result<Option<f64>, String> {
    Ok(array_len(p, "control"))
}

/// `table`: its cells, the title and header rows included (`table.rs`
/// `run_table`: `rows` defaults to the rows of `cells`, `cols` to its
/// longest row).
fn table_cells(p: &Json) -> Result<Option<f64>, String> {
    let cells = p.get("cells").and_then(Json::as_array);
    let rows_of_cells = cells.map_or(1.0, |c| (c.len() as f64).max(1.0));
    let cols_of_cells = cells.map_or(1.0, |c| c.iter().filter_map(Json::as_array).map(|r| r.len() as f64).fold(1.0, f64::max));
    let rows = count_param(p, "rows", rows_of_cells)?;
    let cols = count_param(p, "cols", cols_of_cells)?;
    let extra = f64::from(u8::from(p.get("title").is_some_and(Json::is_string))) + f64::from(u8::from(p.get("header").is_some_and(Json::is_array)));
    Ok(Some((rows + extra) * cols))
}

/// Pick points (`points`) of `hatch`, `gradient` and `boundary`.
fn pick_points(p: &Json) -> Result<Option<f64>, String> {
    Ok(array_len(p, "points"))
}

/// How much finer than its own scale a `key` scale draws a pattern
/// (1 / scale), for the positive numbers the engine takes.
fn inverse_scale(p: &Json, key: &str) -> Option<f64> {
    p.get(key).and_then(Json::as_f64).filter(|s| s.is_finite() && *s > 0.0).map(|s| 1.0 / s)
}

fn inverse_of_scale(p: &Json) -> Result<Option<f64>, String> {
    Ok(inverse_scale(p, "scale"))
}

fn inverse_of_ltscale(p: &Json) -> Result<Option<f64>, String> {
    Ok(inverse_scale(p, "ltscale"))
}

/// `cal {expr}`: its length in bytes.
fn expr_bytes(p: &Json) -> Result<Option<f64>, String> {
    Ok(p.get("expr").and_then(Json::as_str).map(|e| e.len() as f64))
}

/// `find {find, replace}`: how many times longer the text grows where it
/// matches (`utility.rs` `replace_in` replaces every match).
fn replace_growth(p: &Json) -> Result<Option<f64>, String> {
    let (Some(find), Some(replace)) = (p.get("find").and_then(Json::as_str), p.get("replace").and_then(Json::as_str)) else { return Ok(None) };
    if find.is_empty() {
        return Ok(None);
    }
    Ok(Some(replace.len() as f64 / find.len() as f64))
}

/// The `<>` placeholders of a dimension style's text templates (DIMPOST,
/// DIMAPOST), by any spelling the engine takes (`DimStyle::field_name`).
fn template_placeholders(p: &Json) -> Result<Option<f64>, String> {
    let Some(fields) = p.as_object() else { return Ok(None) };
    let most = fields
        .iter()
        .filter(|(k, _)| matches!(cadcraft_engine::doc::DimStyle::field_name(k), Some("post" | "altPost")))
        .filter_map(|(_, v)| v.as_str())
        .map(|t| t.matches("<>").count() as f64)
        .fold(None, |most: Option<f64>, n| Some(most.unwrap_or(0.0).max(n)));
    Ok(most)
}

/// The `<>` placeholders of a dimension text given as `key`.
fn text_placeholders(p: &Json, key: &str) -> Option<f64> {
    p.get(key).and_then(Json::as_str).map(|t| t.matches("<>").count() as f64)
}

fn text_param_placeholders(p: &Json) -> Result<Option<f64>, String> {
    Ok(text_placeholders(p, "text"))
}

fn text_override_placeholders(p: &Json) -> Result<Option<f64>, String> {
    Ok(text_placeholders(p, "textOverride"))
}

/// `dimoverride`: the placeholders of its `text`, or of the templates it sets.
fn dimoverride_placeholders(p: &Json) -> Result<Option<f64>, String> {
    let text = text_placeholders(p, "text");
    let template = template_placeholders(p)?;
    Ok(match (text, template) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    })
}

/// `arrayrect {rows, cols}` copies every selected object rows × cols − 1
/// times; the engine clamps each to 1..=1000 (a million copies) and takes
/// 3 × 4 when absent. This engine reads `cols` (not `columns`) and has no
/// `levels`. Capped at [`MAX_COPIES`] (100 × 100, 2,500 rows of 4).
const ARRAYRECT: Limit = Limit {
    id: "arrayrect",
    what: "copies (`rows` × `cols`, 3 × 4 when absent)",
    measure: Measure::Custom(arrayrect_items),
    max: MAX_COPIES,
    copies: true,
};

/// `arraypolar {count}` copies the selection `count` − 1 times around the
/// centre; the engine clamps it to 1..=10,000 and takes 6 when absent.
/// Capped at [`MAX_COPIES`], refused rather than clamped.
const ARRAYPOLAR: Limit = Limit {
    id: "arraypolar",
    what: "copies (`count`, 6 when absent)",
    measure: Measure::Custom(arraypolar_items),
    max: MAX_COPIES,
    copies: true,
};

/// `arraypath {count}` copies the selection `count` − 1 times along a path;
/// the engine clamps `count` to 20,000 (`MAX_GEN`) and takes 6 when absent.
/// `{spacing}` makes the count the path's length over the spacing (also up
/// to 20,000), which the service measures before the command runs. Capped
/// at [`MAX_COPIES`] either way.
const ARRAYPATH: Limit = Limit {
    id: "arraypath",
    what: "copies (`count`, 6 when absent)",
    measure: Measure::Custom(arraypath_items),
    max: MAX_COPIES,
    copies: true,
};

/// `copy {count}` places `count` displaced copies of the selection; the
/// engine clamps it to 1..=10,000 (and makes one when absent). Capped at
/// [`MAX_COPIES`], refused rather than clamped.
const COPY: Limit = Limit { id: "copy", what: "copies", measure: Measure::Product(&["count"]), max: MAX_COPIES, copies: true };

/// `divide {segments}` places a point per division: the engine's own range
/// (2..=32,767, AutoCAD's DIVIDE limit) is cheap as points.
const DIVIDE: Limit = Limit { id: "divide", what: "segments", measure: Measure::Product(&["segments"]), max: 32_767.0, copies: false };

/// `divide {segments, block}` places a reference to the block at each
/// division point instead, and each draws the whole block: a copy of the
/// block's objects per division, capped at [`MAX_COPIES`] and counted
/// within the call's copies.
const DIVIDE_BLOCKS: Limit = Limit {
    id: "divide",
    what: "block references (`segments` with `block`)",
    measure: Measure::Custom(divide_blocks),
    max: MAX_COPIES,
    copies: true,
};

/// `polygon {sides}`: the engine's own range is 3..=1024 vertices (it
/// refuses more); the door keeps that bound should a newer engine drop it.
const POLYGON: Limit = Limit { id: "polygon", what: "sides", measure: Measure::Product(&["sides"]), max: 1024.0, copies: false };

/// `spline {fit}`: a dense solve over the fit points ([`MAX_FIT_POINTS`]);
/// the engine has no cap on `spline`.
const SPLINE_FIT: Limit = Limit { id: "spline", what: "fit points (`fit`)", measure: Measure::Custom(spline_fit_points), max: MAX_FIT_POINTS, copies: false };

/// `spline {control}`: the engine caps `spline.cv` (and the curves it
/// generates) at 20,000 points (`MAX_GEN`) but not `spline`; the door holds
/// it to the same.
const SPLINE_CONTROL: Limit =
    Limit { id: "spline", what: "control points (`control`)", measure: Measure::Custom(spline_control_points), max: 20_000.0, copies: false };

/// `spline {degree}`: evaluating a spline costs degree² per point and
/// drawing one evaluates up to 4,000 points; `spline.cv` and the DXF reader
/// clamp the degree to 10, `spline` takes anything below its point count.
const SPLINE_DEGREE: Limit = Limit { id: "spline", what: "degree", measure: Measure::Product(&["degree"]), max: 10.0, copies: false };

/// `splinedit {option: refit, fit}`: the same dense solve as `spline {fit}`
/// ([`MAX_FIT_POINTS`]); the engine accepts 20,000 fit points.
const SPLINEDIT_FIT: Limit =
    Limit { id: "splinedit", what: "fit points (`fit`)", measure: Measure::Custom(spline_fit_points), max: MAX_FIT_POINTS, copies: false };

/// `table {rows, cols}`: every cell is stored, outlined and lettered; the
/// engine refuses more than 2,000 rows or 200 columns (400,000 cells, over
/// a million drawn points). 20,000 cells is a long schedule.
const TABLE: Limit = Limit {
    id: "table",
    what: "cells (rows, the title and header rows included, × `cols`)",
    measure: Measure::Custom(table_cells),
    max: 20_000.0,
    copies: false,
};

/// `hatch {points}`: each pick point re-reads and re-tessellates the whole
/// space, drawing every block reference and multileader in it, to trace the
/// boundary around it (`hatch.rs` `loops_at`); a hatch takes a few.
const HATCH_POINTS: Limit = Limit { id: "hatch", what: "pick points (`points`)", measure: Measure::Custom(pick_points), max: 16.0, copies: false };

/// `gradient {points}`: as `hatch {points}`.
const GRADIENT_POINTS: Limit = Limit { id: "gradient", what: "pick points (`points`)", measure: Measure::Custom(pick_points), max: 16.0, copies: false };

/// `boundary {points}`: as `hatch {points}`.
const BOUNDARY_POINTS: Limit = Limit { id: "boundary", what: "pick points (`points`)", measure: Measure::Custom(pick_points), max: 16.0, copies: false };

/// `hatch {scale}`: a smaller pattern scale draws proportionally more lines
/// and dashes ([`MIN_PATTERN_SCALE`]); the engine floors it at 1e-9.
const HATCH_SCALE: Limit = Limit {
    id: "hatch",
    what: "times the pattern's density (1 / `scale`)",
    measure: Measure::Custom(inverse_of_scale),
    max: 1.0 / MIN_PATTERN_SCALE,
    copies: false,
};

/// `hatchedit {scale}`: as `hatch {scale}`.
const HATCHEDIT_SCALE: Limit = Limit {
    id: "hatchedit",
    what: "times the pattern's density (1 / `scale`)",
    measure: Measure::Custom(inverse_of_scale),
    max: 1.0 / MIN_PATTERN_SCALE,
    copies: false,
};

/// `properties.set {scale}` rescales a hatch pattern (and a block
/// reference, which this floor leaves ample room for): as `hatch {scale}`.
const PROPERTIES_SCALE: Limit = Limit {
    id: "properties.set",
    what: "times the pattern's density (1 / `scale`)",
    measure: Measure::Custom(inverse_of_scale),
    max: 1.0 / MIN_PATTERN_SCALE,
    copies: false,
};

/// `ltscale {scale}` (LTSCALE) multiplies the dashes of every linetyped
/// object in the drawing as it shrinks ([`MIN_PATTERN_SCALE`]).
const LTSCALE: Limit = Limit {
    id: "ltscale",
    what: "times the linetype's dash density (1 / `scale`)",
    measure: Measure::Custom(inverse_of_scale),
    max: 1.0 / MIN_PATTERN_SCALE,
    copies: false,
};

/// `properties.set {ltscale}`: an object's own linetype scale, as LTSCALE.
/// (CELTSCALE, the scale new objects get, has no command of its own: only
/// `setvar` sets it, which the door refuses.)
const PROPERTIES_LTSCALE: Limit = Limit {
    id: "properties.set",
    what: "times the linetype's dash density (1 / `ltscale`)",
    measure: Measure::Custom(inverse_of_ltscale),
    max: 1.0 / MIN_PATTERN_SCALE,
    copies: false,
};

/// `entities {limit}`: every matching object is serialised whole into the
/// reply; the engine has no cap (`cad.entities` holds it to the same 2,000).
const ENTITIES_LIMIT: Limit =
    Limit { id: "entities", what: "objects per answer", measure: Measure::Product(&["limit"]), max: MAX_ENTITIES as f64, copies: false };

/// `drawing.inspect {limit}`: as `entities {limit}` (200 when absent).
const INSPECT_LIMIT: Limit =
    Limit { id: "drawing.inspect", what: "objects per answer", measure: Measure::Product(&["limit"]), max: MAX_ENTITIES as f64, copies: false };

/// `cal {expr}`: the calculator recurses once per unary sign and per `^`
/// without counting (only parentheses count, to 200), so a long run of
/// either deepens the UI thread's stack; the engine accepts 10,000 bytes.
/// 1,024 is the constraint expressions' own limit.
const CAL_EXPR: Limit = Limit { id: "cal", what: "bytes of `expr`", measure: Measure::Custom(expr_bytes), max: 1024.0, copies: false };

/// `find {find, replace}` replaces every match in every text of the space,
/// so a long replacement for a short match multiplies the drawing's text
/// (and a series of them compounds); up to 16 times keeps a three-letter
/// abbreviation's spelled-out form.
const FIND_REPLACE: Limit = Limit {
    id: "find",
    what: "times the text it matches (`replace` over `find`)",
    measure: Measure::Custom(replace_growth),
    max: 16.0,
    copies: false,
};

/// `dimstyle` DIMPOST / DIMAPOST templates ([`MAX_PLACEHOLDERS`]).
const DIMSTYLE_TEMPLATE: Limit = Limit {
    id: "dimstyle",
    what: "measurement placeholders (`<>` in DIMPOST or DIMAPOST)",
    measure: Measure::Custom(template_placeholders),
    max: MAX_PLACEHOLDERS,
    copies: false,
};

/// `dimstyle.dimension`: as `dimstyle`.
const DIMSTYLE_DIMENSION_TEMPLATE: Limit = Limit {
    id: "dimstyle.dimension",
    what: "measurement placeholders (`<>` in DIMPOST or DIMAPOST)",
    measure: Measure::Custom(template_placeholders),
    max: MAX_PLACEHOLDERS,
    copies: false,
};

/// `dimstyle.override`: per-dimension templates, as `dimstyle`.
const DIMSTYLE_OVERRIDE_TEMPLATE: Limit = Limit {
    id: "dimstyle.override",
    what: "measurement placeholders (`<>` in DIMPOST or DIMAPOST)",
    measure: Measure::Custom(template_placeholders),
    max: MAX_PLACEHOLDERS,
    copies: false,
};

/// `dimoverride`: its `text`, or the templates it sets.
const DIMOVERRIDE_TEMPLATE: Limit = Limit {
    id: "dimoverride",
    what: "measurement placeholders (`<>`)",
    measure: Measure::Custom(dimoverride_placeholders),
    max: MAX_PLACEHOLDERS,
    copies: false,
};

/// `dimlinear {text}`: a dimension's own text ([`MAX_PLACEHOLDERS`]).
const DIMLINEAR_TEXT: Limit = Limit {
    id: "dimlinear",
    what: "measurement placeholders (`<>` in `text`)",
    measure: Measure::Custom(text_param_placeholders),
    max: MAX_PLACEHOLDERS,
    copies: false,
};

/// `dimaligned {text}`: as `dimlinear {text}`.
const DIMALIGNED_TEXT: Limit = Limit {
    id: "dimaligned",
    what: "measurement placeholders (`<>` in `text`)",
    measure: Measure::Custom(text_param_placeholders),
    max: MAX_PLACEHOLDERS,
    copies: false,
};

/// `textedit {text}` sets a dimension's text too (a plain text with this
/// many `<>` is rare).
const TEXTEDIT_TEXT: Limit = Limit {
    id: "textedit",
    what: "measurement placeholders (`<>` in `text`)",
    measure: Measure::Custom(text_param_placeholders),
    max: MAX_PLACEHOLDERS,
    copies: false,
};

/// `properties.set {textOverride}`: a dimension's text, as `dimlinear {text}`.
const PROPERTIES_TEXT_OVERRIDE: Limit = Limit {
    id: "properties.set",
    what: "measurement placeholders (`<>` in `textOverride`)",
    measure: Measure::Custom(text_override_placeholders),
    max: MAX_PLACEHOLDERS,
    copies: false,
};

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
/// binary, DWG), with the file size capped, and refused when one pass over
/// its extents (which opening, rendering and most commands make) would not
/// end in reasonable time ([`caps::openable`]).
fn read_drawing(ctx: &str, area: &Area, rel: &str) -> Result<Drawing, String> {
    let path = contained(ctx, area, rel)?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("{ctx}: {rel}: {e}"))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!("{ctx}: the file is larger than the service reads"));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("{ctx}: {rel}: {e}"))?;
    let d = cadcraft_io::read(&bytes, rel).map_err(|e| format!("{ctx}: {e}"))?;
    caps::openable(ctx, &d)?;
    Ok(d)
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
    caps::work("cad.entities", &s, "entities", &params)?;
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
            caps::work("cad.measure", &s, "area", &args["area"])?;
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
/// Refused before anything is drawn when the drawing would take more than
/// the service draws at once ([`caps::drawable`]).
fn fitted_png(ctx: &str, d: &Drawing, side: u32) -> Result<(Vec<u8>, u32, u32), String> {
    caps::drawable(ctx, d, "png")?;
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
/// (`render`'s SVG too), within what the service draws ([`caps::drawable`])
/// or writes ([`caps::writable`]) at once.
fn encoded(ctx: &str, d: &Drawing, format: &str) -> Result<Vec<u8>, String> {
    if matches!(format, "svg" | "png" | "pdf") {
        caps::drawable(ctx, d, format)?;
    } else {
        caps::writable(ctx, d)?;
    }
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
///
/// Around every command the service bounds what no parameter shows
/// ([`caps::Watch`]): what a copying command would add to the drawing before
/// it runs, the work of the commands that scale with the drawing, and what
/// the call's drawings hold after it.
fn run(args: &Json, area: &Area) -> Result<Json, String> {
    // Admit every command first: one refused id refuses the whole call, with
    // nothing opened and nothing written.
    let admitted = door()?.admit_all(&args["cmds"], area)?;
    caps::named_once(&admitted)?;
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
    let mut watch = caps::Watch::new(&s);
    let mut results = Vec::with_capacity(admitted.len());
    for (id, params) in admitted {
        watch.before(&s, &id, &params)?;
        let r = s.execute(&id, &params).map_err(|e| format!("cad.run {id}: {e}"))?;
        watch.after(&mut s, &id, &r)?;
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

/// What the service bounds around the door's commands beyond what the
/// gate's limits see in their parameters (ADR 0013, #418): engine work runs
/// on the shell's UI thread, and much of it scales with the drawing rather
/// than with any parameter.
///
/// - **Growth.** Before a command that copies (`copy`, the arrays, `mirror`,
///   `rotate` and `scale` with `copy`, `pasteclip`, `explode`, `ncopy`,
///   `insert {explode}`, `divide`, `measure`, `block`, `layout.copy`…) the
///   service works out what it would add, objects and weight, from the
///   objects it copies (`handles`, else the selection; a handle named twice
///   is refused, [`named_once`]), and refuses it when the call's drawings
///   would pass their ceilings. After every command that changed them it
///   weighs them again, which also catches growth by repetition with no
///   count (`selectall` and `mirror` in turn, `copyclip`/`pasteclip`
///   loops, `pedit {option: fit}` doubling a polyline). A drawing opened
///   above a ceiling may be edited, but a call never takes it higher.
/// - **Work that scales with the drawing.** `trim`/`extend`, `overkill`,
///   `join`, the associative dimensions of `qdim`, `dimcontinue` and
///   `dimbaseline`, the pick points of `hatch`, crossing selections over
///   block references, path divisions, spline refits, grip edits of long
///   polylines, fine area tessellation, `qselect` property sheets and
///   whole-object answers are each weighed before they run ([`work`]).
/// - **Drawing.** A PNG, SVG or PDF is drawn only when the service's estimate
///   of its display list (every block reference expanded, every dash and
///   hatch line counted, as `cadcraft_render` makes them) fits
///   [`RENDER_CEILING`] ([`drawable`]): a hatch pattern or linetype that is
///   fine for its object, planted by a command or already in the file, would
///   otherwise draw hundreds of millions of lines.
/// - **Opening.** A drawing whose extents take too long to compute (blocks of
///   blocks, a block that holds itself, hundreds of thousands of layers) is
///   refused before it opens ([`openable`]).
mod caps {
    use std::collections::HashMap;

    use cadcraft_engine::cmd::curves::Chain;
    use cadcraft_engine::doc::{library, Attrib, Block, Dimension, Drawing, Entity, EntityKind, Handle, Hatch, Layer, Linetype, LwPolyline, Space, MAX_BLOCK_DEPTH};
    use cadcraft_engine::geom::{arc_segments, ccw_sweep, Polyline, Segment, Spline, TAU};
    use cadcraft_engine::Session;
    use serde_json::Value as Json;

    use super::{count_param, door, spaced_along_path, COPIES_PER_CALL, MAX_COPIES, MAX_FIT_POINTS};

    /// The most objects (model and paper space of every open drawing, and
    /// the clipboard) a call may leave: a large plan holds tens of
    /// thousands; at about 460 bytes each (`size_of::<Entity>()` is 416),
    /// 200,000 stay under 100 MB, and a pass over all of them (a move of
    /// everything, a PNG of plain lines) under ~0.3 s.
    pub(super) const ENTITY_CEILING: f64 = 200_000.0;

    /// The most the call's drawings may weigh ([`weight`]: one per object,
    /// vertex, point and table cell, one per two characters of text, block
    /// definitions included): about 80 MB of vertices on top of the objects.
    /// It catches growth inside objects, which adds none (a polyline whose
    /// vertices double, text a `find` grows, a block that copies its objects).
    pub(super) const WEIGHT_CEILING: f64 = 2_000_000.0;

    /// The most work one pass over the call's drawings' extents may take
    /// ([`visits`]): every object, every object of every block it
    /// references, nested to the engine's 16 levels (`doc/src/extents.rs`
    /// recurses without remembering), and the layer scans the engine makes
    /// per object. Opening a drawing computes its extents first, and most
    /// commands do again: 200,000 plain objects take 9 ms.
    pub(super) const VISIT_CEILING: f64 = 5_000_000.0;

    /// The most drawing work one PNG, SVG or PDF may take ([`Draw`]: display
    /// list points, triangle corners and primitives): about a second of the
    /// CPU rasteriser or the PDF writer on a laptop (measured: a million
    /// dashes, 2.7 million units, take 0.65 s as PNG and 0.93 s as PDF).
    pub(super) const RENDER_CEILING: f64 = 3_000_000.0;

    /// Undo steps kept per drawing within a call. Every step keeps the
    /// objects its command changed, so a run of whole-drawing edits (moving
    /// every object, 64 times) would keep 64 copies of the drawing; the
    /// engine keeps 2,000. One keeps `undo` of the last command working.
    pub(super) const UNDO_KEPT: usize = 1;

    /// The most the replies of one call may hold, all commands together
    /// (about 4 MB of JSON, far more than an agent reads).
    pub(super) const ANSWER_BYTES: f64 = 4_000_000.0;

    /// The most weight one query may copy into its reply whole (`list`,
    /// `properties`, `entities`, `drawing.inspect` serialise every vertex):
    /// about 4 MB of JSON.
    pub(super) const ANSWER_WEIGHT: f64 = 50_000.0;

    /// The most segment intersections one `trim` or `extend` tests: each
    /// segment of the object (an ellipse is sampled at 720, a spline at 512)
    /// against every segment of every cutting edge, which is every visible
    /// object when `edges` is absent (measured: 12 ms for a line against
    /// 200,000 edges).
    pub(super) const TRIM_TESTS: f64 = 10_000_000.0;

    /// `overkill` compares the serialised form of each object with every one
    /// before it (`modify.rs` `run_overkill`): objects × their weight within
    /// this (20,000 lines, weight 5 each, take about 0.3 s).
    pub(super) const OVERKILL_WORK: f64 = 2_000_000_000.0;

    /// `join` and `pedit {option: join}` chain pieces by scanning the rest
    /// for each piece they attach: pieces² / 2 tests (20,000 take ~0.3 s).
    pub(super) const JOIN_PIECES: f64 = 20_000.0;

    /// New dimensions attach to the objects under their definition points
    /// by scanning every object's snap points per point (`assoc.rs`
    /// `find_snap`): dimensions × points × snap points within this
    /// (measured: 500 lines' `qdim` in a 100,000-line drawing, 6e8, 5 s).
    pub(super) const SNAP_TESTS: f64 = 60_000_000.0;

    /// `divide`, `measure` and `arraypath` find each point along the path
    /// by walking its segments from the start (`curves.rs`
    /// `Chain::at_length`): points × path segments within this.
    pub(super) const PATH_STEPS: f64 = 20_000_000.0;

    /// The most points the fine tessellation of a measured object may take
    /// (`area`, `list` and `properties` tessellate polylines at 1e-4,
    /// `massprop` at 1e-5: up to 2,222 points per bulged segment).
    pub(super) const FINE_POINTS: f64 = 4_000_000.0;

    /// Grip edits list a polyline's grips by rebuilding all its segments
    /// once per vertex (`EntityKind::grips`): vertices² within this
    /// (measured: 6,000 vertices, 0.19 s).
    pub(super) const GRIP_WORK: f64 = 40_000_000.0;

    /// The most objects (a polyline of many vertices counting more) a
    /// `qselect` property filter or `qselect.info` serialises: each candidate
    /// becomes a JSON property sheet (`qselect.rs` `properties`; measured:
    /// 200,000 lines, 6.6 s).
    pub(super) const QSELECT_OBJECTS: f64 = 30_000.0;

    /// The most steps the service's own estimates take before they give up
    /// (and refuse): each is a cheap visit of one object.
    const ESTIMATE_STEPS: u64 = 20_000_000;

    /// A number for a message: whole, without a fraction.
    fn shown(n: f64) -> String {
        if n.is_finite() && n.abs() < 1e15 {
            format!("{}", n.round() as i64)
        } else {
            "too many to count".into()
        }
    }

    /// A handle as the engine reads one (`cmd/mod.rs` `targets`).
    fn handle_value(v: &Json) -> Option<Handle> {
        v.as_str().and_then(Handle::parse_hex).or_else(|| v.as_u64().map(Handle))
    }

    /// The objects a command acts on, as the engine picks them: `handles`,
    /// else `handle`, else the selection.
    fn targets(s: &Session, p: &Json) -> Vec<Handle> {
        if let Some(a) = p.get("handles").and_then(Json::as_array) {
            return a.iter().filter_map(handle_value).collect();
        }
        if let Some(h) = p.get("handle").and_then(handle_value) {
            return vec![h];
        }
        s.selection()
    }

    /// Of `hs`, the objects that exist in `d`.
    fn existing<'d>(d: &'d Drawing, hs: &[Handle]) -> Vec<&'d Entity> {
        hs.iter().filter_map(|h| d.entity(*h).map(|e| &**e)).collect()
    }

    /// Every command of a call names each object once in its lists of
    /// handles: the engine acts on a handle as often as it is named (`copy`
    /// copies it again, `block` stores it again, `list` lists it again), so
    /// a repeated handle multiplies work as a count would.
    pub(super) fn named_once(cmds: &[(String, Json)]) -> Result<(), String> {
        for (id, p) in cmds {
            for key in ["handles", "targets", "handles2", "join", "edges"] {
                let Some(list) = p.get(key).and_then(Json::as_array) else { continue };
                let mut seen = std::collections::HashSet::new();
                if let Some(h) = list.iter().filter_map(handle_value).find(|h| !seen.insert(*h)) {
                    return Err(format!("cad.run: `{id}`: `{key}` names the object {} more than once; name each object once", h.hex()));
                }
            }
        }
        Ok(())
    }

    // ---------------- weight: what the drawings hold ----------------

    fn text_weight(s: &str) -> f64 {
        s.len() as f64 / 2.0
    }

    fn attrib_weight(a: &Attrib) -> f64 {
        1.0 + text_weight(&a.tag) + text_weight(&a.text.value) + text_weight(&a.prompt) + text_weight(&a.text.style)
    }

    /// What one object holds in memory, in the units of [`WEIGHT_CEILING`]:
    /// one for the object, one per vertex, point, knot, tag and table cell,
    /// one per two characters of its text.
    pub(super) fn weight(e: &Entity) -> f64 {
        let t = text_weight;
        let own = match &e.kind {
            EntityKind::LwPolyline(p) => p.vertices.len() as f64,
            EntityKind::Polyline3d(p) => p.points.len() as f64,
            EntityKind::Spline(s) => (s.control.len() + s.fit.len() + s.knots.len() + s.weights.len()) as f64,
            EntityKind::Hatch(h) => t(&h.pattern) + h.loops.iter().map(|l| 1.0 + l.vertices.len() as f64).sum::<f64>(),
            EntityKind::Text(x) => t(&x.value) + t(&x.style),
            EntityKind::MText(x) => t(&x.contents) + t(&x.style),
            EntityKind::AttDef(a) => attrib_weight(a),
            EntityKind::Insert(i) => t(&i.block) + i.attribs.iter().map(attrib_weight).sum::<f64>(),
            EntityKind::Dimension(d) => {
                t(&d.text)
                    + t(&d.style)
                    + d.block.as_deref().map_or(0.0, t)
                    + d.assoc.len() as f64
                    + d.overrides.iter().map(|(k, v)| 1.0 + t(k) + answer_bytes(v) / 2.0).sum::<f64>()
            }
            EntityKind::Leader(l) => l.vertices.len() as f64,
            EntityKind::MLeader(m) => m.leaders.iter().map(|l| 1.0 + l.len() as f64).sum::<f64>() + m.text.as_ref().map_or(0.0, |x| t(&x.contents)),
            EntityKind::Wipeout(w) => w.boundary.len() as f64,
            EntityKind::Table(x) => {
                (x.row_heights.len() + x.col_widths.len()) as f64 + x.cells.iter().map(|r| r.iter().map(|c| 1.0 + t(&c.text)).sum::<f64>()).sum::<f64>()
            }
            EntityKind::Image(i) => t(&i.path),
            EntityKind::Viewport(v) => v.frozen_layers.iter().map(|l| 1.0 + t(l)).sum::<f64>() + v.layer_colors.len() as f64,
            EntityKind::Unknown(u) => u.tags.iter().map(|g| 1.0 + t(&g.value)).sum::<f64>(),
            _ => 0.0,
        };
        1.0 + t(&e.common.layer) + t(&e.common.linetype) + own
    }

    /// What a drawing holds: its objects, its block definitions' objects and
    /// its tables.
    fn drawing_weight(d: &Drawing) -> f64 {
        let t = text_weight;
        let objects: f64 = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).flat_map(|s| s.iter()).map(|e| weight(e)).sum();
        let blocks: f64 = d.blocks.values().map(|b| 1.0 + t(&b.name) + b.entities.iter().map(|e| weight(e)).sum::<f64>()).sum();
        let tables = d.layers.iter().map(|l| 1.0 + t(&l.name) + t(&l.linetype) + t(&l.description)).sum::<f64>()
            + d.linetypes.iter().map(|l| 1.0 + t(&l.name) + t(&l.description) + l.pattern.len() as f64).sum::<f64>()
            + d.dim_styles.iter().map(|s| 1.0 + t(&s.name) + t(&s.post) + t(&s.alt_post) + t(&s.decimal_separator)).sum::<f64>()
            + d.layer_states.iter().map(|s| 1.0 + s.layers.len() as f64).sum::<f64>()
            + d.groups.iter().map(|g| 1.0 + g.members.len() as f64).sum::<f64>()
            + d.constraints.iter().map(|c| 1.0 + c.refs.len() as f64 + t(&c.name) + t(&c.expr)).sum::<f64>()
            + d.parametric.parameters.iter().map(|q| 1.0 + t(&q.name) + t(&q.expr) + t(&q.description)).sum::<f64>()
            + (d.text_styles.len() + d.mleader_styles.len() + d.table_styles.len() + d.views.len() + d.ucss.len() + d.layouts.len() + d.header.vars.len()) as f64;
        objects + blocks + tables
    }

    // ---------------- visits: what one pass over the extents takes ----------------

    /// Positions of a table's names as the engine finds them (the first
    /// case-insensitive match, by a linear scan).
    fn positions<'a>(names: impl Iterator<Item = &'a str>) -> HashMap<String, usize> {
        let mut out = HashMap::new();
        for (i, n) in names.enumerate() {
            out.entry(n.to_ascii_lowercase()).or_insert(i);
        }
        out
    }

    /// Samples the engine tessellates a spline into at `tol`
    /// (`geom/src/spline.rs` `Spline::tessellate`).
    fn spline_samples(s: &Spline, tol: f64) -> f64 {
        if !s.is_valid() {
            return s.control.len() as f64;
        }
        let poly: f64 = s.control.windows(2).map(|w| w[0].dist(w[1])).sum();
        let n = ((poly / tol.max(1e-9)).sqrt() * 2.0).clamp(8.0, 2000.0).floor() * s.degree.max(1) as f64;
        n.min(4000.0) + 1.0
    }

    struct Visits<'d> {
        d: &'d Drawing,
        layers: HashMap<String, usize>,
        layer_cost: HashMap<&'d str, f64>,
        memo: HashMap<(String, usize), f64>,
        steps: u64,
    }

    impl<'d> Visits<'d> {
        fn new(d: &'d Drawing) -> Self {
            Visits { d, layers: positions(d.layers.iter().map(|l| l.name.as_str())), layer_cost: HashMap::new(), memo: HashMap::new(), steps: 0 }
        }

        /// The engine scans the layer table for an object's layer
        /// (`Drawing::is_visible`): one step per eight names passed.
        fn layer(&mut self, name: &'d str) -> f64 {
            if let Some(c) = self.layer_cost.get(name) {
                return *c;
            }
            let pos = self.layers.get(&name.to_ascii_lowercase()).map_or(self.d.layers.len(), |i| i + 1);
            let c = pos as f64 / 8.0;
            self.layer_cost.insert(name, c);
            c
        }

        fn entity(&mut self, e: &'d Entity, depth: usize) -> f64 {
            self.steps += 1;
            if self.steps > ESTIMATE_STEPS {
                return f64::INFINITY;
            }
            let own = match &e.kind {
                EntityKind::Insert(i) => self.nested(&i.block, depth) + i.attribs.len() as f64,
                EntityKind::Dimension(dm) => 4.0 + dm.block.as_deref().map_or(0.0, |b| self.nested(b, depth)),
                EntityKind::LwPolyline(p) => p.vertices.len() as f64,
                EntityKind::Polyline3d(p) => p.points.len() as f64,
                EntityKind::Leader(l) => l.vertices.len() as f64,
                EntityKind::MLeader(m) => m.leaders.iter().map(|l| l.len() as f64).sum(),
                EntityKind::Wipeout(w) => w.boundary.len() as f64,
                EntityKind::Hatch(h) => h.loops.iter().map(|l| l.vertices.len() as f64).sum(),
                // `Spline::bounds` tessellates at 1e-3, degree² per sample.
                EntityKind::Spline(s) => spline_samples(s, 1e-3) * ((s.degree + 1) as f64).powi(2) / 8.0,
                EntityKind::Ellipse(_) => 32.0,
                EntityKind::Text(t) => t.value.len() as f64 / 64.0,
                EntityKind::MText(t) => t.contents.len() as f64 / 64.0,
                EntityKind::Table(t) => (t.row_heights.len() + t.col_widths.len()) as f64,
                _ => 0.0,
            };
            1.0 + self.layer(&e.common.layer) + own
        }

        /// One pass over a referenced block's objects at `depth + 1`; past
        /// the engine's depth, nothing.
        fn nested(&mut self, name: &str, depth: usize) -> f64 {
            if depth >= MAX_BLOCK_DEPTH {
                return 0.0;
            }
            // A name the block table does not hold exactly is looked for in
            // every block, case-insensitively (`Drawing::block`).
            let scan = if self.d.blocks.contains_key(name) { 0.0 } else { self.d.blocks.len() as f64 / 8.0 };
            let Some(b) = self.d.block(name) else { return scan };
            let key = (b.name.clone(), depth + 1);
            if let Some(n) = self.memo.get(&key) {
                return scan + n;
            }
            let mut n = 0.0;
            for be in b.entities.iter() {
                n += self.entity(be, depth + 1);
            }
            self.memo.insert(key, n);
            scan + n
        }
    }

    /// One pass over the extents of every space of `d`, in the units of
    /// [`VISIT_CEILING`].
    pub(super) fn visits(d: &Drawing) -> f64 {
        let mut v = Visits::new(d);
        let mut n = 0.0;
        for e in std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).flat_map(|s| s.iter()) {
            n += v.entity(e, 0);
        }
        n
    }

    /// Refuse a drawing whose extents (which opening it computes first)
    /// would take more than [`VISIT_CEILING`].
    pub(super) fn openable(ctx: &str, d: &Drawing) -> Result<(), String> {
        let n = visits(d);
        if n > VISIT_CEILING {
            return Err(format!(
                "{ctx}: one pass over this drawing would take {} steps, more than the {} the service takes (its blocks nest too deeply, a block holds itself, or its layer table is too large)",
                shown(n),
                shown(VISIT_CEILING)
            ));
        }
        Ok(())
    }

    /// What a session holds, as the service weighs it.
    #[derive(Clone, Copy, Debug, Default)]
    pub(super) struct Heft {
        pub entities: f64,
        pub weight: f64,
        pub visits: f64,
    }

    /// Every open drawing of the session, and its clipboard.
    pub(super) fn heft(s: &Session) -> Heft {
        let mut h = Heft::default();
        for st in &s.docs {
            let d: &Drawing = &st.doc;
            h.entities += d.entity_count() as f64;
            h.weight += drawing_weight(d);
            h.visits += visits(d);
        }
        h.entities += s.clipboard.len() as f64;
        h.weight += s.clipboard.iter().map(weight).sum::<f64>();
        h
    }

    // ---------------- drawing: the display list an output would take ----------------

    /// Display-list units per character of a stroke-font text (measured: 6
    /// points and 1.3 primitives).
    const STROKE_CHAR: f64 = 10.0;
    /// Per character of a TrueType text, filled (measured with Arial: about
    /// 600 triangle corners per Latin character).
    const OUTLINE_CHAR: f64 = 800.0;
    /// Per non-Latin character of a TrueType text (an ideograph has ten
    /// times the contours of a Latin letter).
    const OUTLINE_WIDE_CHAR: f64 = 8000.0;
    /// A dimension's own lines, arrows and fills (measured: about 30).
    const DIMENSION_LINES: f64 = 64.0;

    /// Whether a style's font may be a TrueType font the engine finds on the
    /// system (`fonts/src/ttf.rs` `find`): everything but its stroke fonts.
    fn outline_font(name: &str) -> bool {
        let k = name.trim().to_ascii_lowercase();
        let k = k.trim_end_matches(".ttf").trim_end_matches(".otf").trim_end_matches(".ttc");
        !(k.is_empty() || k.ends_with(".shx") || matches!(k, "cadcraft stroke" | "txt" | "simplex" | "romans"))
    }

    /// The context an object is drawn in (`render/src/lib.rs` `Ctx`).
    #[derive(Clone)]
    struct Frame {
        depth: usize,
        /// The scale block references multiply objects by (finer chords).
        scale: f64,
        /// The layer of the reference, for objects on layer 0.
        layer: Option<String>,
        /// The linetype of the reference, for BYBLOCK objects.
        ltype: String,
    }

    /// The display list an output of model space would take, as
    /// `cadcraft_render::build` makes it: every object's tessellated points
    /// (chords within `tol`), every dash of its linetype, every line and dash
    /// of its hatch pattern, every glyph of its text, every cell of its
    /// table, and every block reference expanded, `rows` × `cols` times,
    /// nested to the engine's 16 levels. Counted in points, triangle corners
    /// and primitives; an upper estimate where the engine's own work cannot
    /// be counted without doing it.
    pub(super) struct Draw<'d> {
        d: &'d Drawing,
        tol: f64,
        /// Text drawn (outputs) or not (hit tests).
        text: bool,
        /// Linetypes dashed (drawing) or not (top-level outlines of hit tests).
        dashes: bool,
        /// PDF: layers marked not to plot are left out.
        plotting: bool,
        ltscale: f64,
        fill: bool,
        textfill: bool,
        layers: HashMap<String, usize>,
        linetypes: HashMap<String, usize>,
        patterns: HashMap<String, Vec<library::PatternLine>>,
        memo: HashMap<(String, usize, i32, String, String), f64>,
        steps: u64,
    }

    impl<'d> Draw<'d> {
        pub(super) fn new(d: &'d Drawing, tol: f64, text: bool, plotting: bool) -> Self {
            Draw {
                d,
                tol: if tol.is_finite() && tol > 0.0 { tol } else { 0.001 },
                text,
                dashes: true,
                plotting,
                ltscale: d.header.f64("LTSCALE", 1.0),
                fill: d.header.i64("FILLMODE", 1) != 0,
                textfill: d.header.i64("TEXTFILL", 1) != 0,
                layers: positions(d.layers.iter().map(|l| l.name.as_str())),
                linetypes: positions(d.linetypes.iter().map(|l| l.name.as_str())),
                patterns: HashMap::new(),
                memo: HashMap::new(),
                steps: 0,
            }
        }

        /// A hatch pattern's line families (unknown names draw as ANSI31).
        fn pattern(&mut self, name: &str) -> Vec<library::PatternLine> {
            let key = name.to_ascii_uppercase();
            self.patterns
                .entry(key)
                .or_insert_with(|| library::pattern(name).or_else(|| library::pattern("ANSI31")).map(|p| p.lines).unwrap_or_default())
                .clone()
        }

        fn top() -> Frame {
            Frame { depth: 0, scale: 1.0, layer: None, ltype: "Continuous".into() }
        }

        /// Every object of a space.
        pub(super) fn space(&mut self, space: &Space) -> f64 {
            let Some(store) = self.d.space(space) else { return 0.0 };
            let top = Self::top();
            store.iter().map(|e| self.entity(e, &top)).sum()
        }

        fn layer(&self, name: &str) -> Option<&'d Layer> {
            self.layers.get(&name.to_ascii_lowercase()).and_then(|i| self.d.layers.get(*i))
        }

        fn linetype(&self, name: &str) -> Option<&'d Linetype> {
            self.linetypes.get(&name.to_ascii_lowercase()).and_then(|i| self.d.linetypes.get(*i)).filter(|l| !l.pattern.is_empty())
        }

        fn font_of(&self, style: &str) -> String {
            self.d.text_style(style).map_or_else(|| "CADCraft Stroke".to_string(), |s| s.font.clone())
        }

        /// The glyphs of `s` in `style`'s font (`inline`: MTEXT codes may
        /// switch to any font).
        fn chars(&self, style: &str, s: &str, inline: bool) -> f64 {
            if !(inline || outline_font(&self.font_of(style))) {
                return s.chars().count() as f64 * STROKE_CHAR;
            }
            let (latin, wide) = s.chars().fold((0.0, 0.0), |(l, w), c| if c.is_ascii() { (l + 1.0, w) } else { (l, w + 1.0) });
            let filled = if self.textfill { 1.0 } else { 0.125 };
            (latin * OUTLINE_CHAR + wide * OUTLINE_WIDE_CHAR) * filled
        }

        /// One stroke of `pts` points along `len` world units, dashed by `lt`
        /// at `scale` as `render/src/linetype.rs` `apply` does: past 50,000
        /// patterns (or two million pattern steps) it is drawn solid.
        fn stroke(len: f64, pts: f64, lt: Option<&Linetype>, scale: f64) -> f64 {
            let solid = pts + 1.0;
            let Some(lt) = lt else { return solid };
            let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
            let total = lt.pattern_length() * scale;
            if pts < 2.0 || total.is_nan() || total <= 1e-12 || !len.is_finite() {
                return solid;
            }
            let patterns = len / total;
            if patterns > 50_000.0 {
                return solid;
            }
            let elements = lt.pattern.len() as f64;
            if patterns * elements > 2_000_000.0 {
                // Stepped through, then thrown away.
                return solid + 2_000_000.0 / 4.0;
            }
            let on = lt.pattern.iter().filter(|e| e.length >= 0.0).count() as f64;
            let runs = (patterns + 1.0) * on.max(1.0) + 1.0;
            runs * 3.0 + pts
        }

        fn seg_points(s: &Segment, tol: f64) -> f64 {
            match s {
                Segment::Line(_) => 2.0,
                Segment::Arc { arc, .. } => (arc_segments(arc.radius, arc.sweep(), tol) + 1) as f64,
            }
        }

        fn entity(&mut self, e: &'d Entity, f: &Frame) -> f64 {
            self.steps += 1;
            if self.steps > ESTIMATE_STEPS {
                return f64::INFINITY;
            }
            let layer_name = if e.common.layer == "0" { f.layer.as_deref().unwrap_or("0") } else { e.common.layer.as_str() };
            let layer = self.layer(layer_name);
            if !(e.common.visible && layer.is_none_or(|l| l.visible() && (!self.plotting || l.plot))) {
                return 0.0;
            }
            let lt_name = match e.common.linetype.to_ascii_lowercase().as_str() {
                "bylayer" => layer.map_or_else(|| "Continuous".to_string(), |l| l.linetype.clone()),
                "byblock" => f.ltype.clone(),
                _ => e.common.linetype.clone(),
            };
            let lt = if self.dashes { self.linetype(&lt_name) } else { None };
            let scale = self.ltscale * e.common.ltscale;
            let tol = self.tol / f.scale.max(1e-12);
            match &e.kind {
                EntityKind::Line(l) => Self::stroke(l.a.xy().dist(l.b.xy()), 2.0, lt, scale),
                EntityKind::Circle(c) => Self::stroke(c.radius.abs() * TAU, (arc_segments(c.radius, TAU, tol) + 1) as f64, lt, scale),
                EntityKind::Arc(a) => {
                    let sweep = ccw_sweep(a.start, a.end);
                    Self::stroke(a.radius.abs() * sweep, (arc_segments(a.radius, sweep, tol) + 1) as f64, lt, scale)
                }
                EntityKind::Ellipse(el) => {
                    let (major, sweep) = (el.major.xy().len(), ccw_sweep(el.start, el.end));
                    Self::stroke(major * sweep, (arc_segments(major, sweep, tol).max(8) + 1) as f64, lt, scale)
                }
                EntityKind::Spline(sp) => {
                    let poly: f64 = sp.control.windows(2).map(|w| w[0].dist(w[1])).sum();
                    let n = spline_samples(sp, tol);
                    Self::stroke(poly, n, lt, scale) + n * ((sp.degree + 1) as f64).powi(2) / 16.0
                }
                EntityKind::LwPolyline(p) => self.polyline(p, lt, scale, tol),
                EntityKind::Polyline3d(p) => {
                    let mut n = 0.0;
                    let closing = p.closed.then(|| (p.points.last(), p.points.first()));
                    for (a, b) in p.points.windows(2).map(|w| (Some(&w[0]), Some(&w[1]))).chain(closing) {
                        if let (Some(a), Some(b)) = (a, b) {
                            n += Self::stroke(a.xy().dist(b.xy()), 2.0, lt, scale);
                        }
                    }
                    n
                }
                EntityKind::Face3d(x) => (0..4).map(|i| Self::stroke(x.corners[i].xy().dist(x.corners[(i + 1) % 4].xy()), 2.0, lt, scale)).sum(),
                EntityKind::Viewport(v) => [v.width, v.height, v.width, v.height].iter().map(|side| Self::stroke(side.abs(), 2.0, lt, scale)).sum(),
                EntityKind::Solid(_) | EntityKind::Trace(_) => 7.0,
                EntityKind::Point(_) => 2.0,
                EntityKind::Ray(_) | EntityKind::XLine(_) => 3.0,
                EntityKind::Wipeout(w) => w.boundary.len() as f64 + 2.0,
                EntityKind::Image(_) => 12.0,
                EntityKind::Leader(l) => {
                    let len: f64 = l.vertices.windows(2).map(|w| w[0].xy().dist(w[1].xy())).sum();
                    Self::stroke(len, l.vertices.len() as f64, lt, scale) + if l.arrow { 16.0 } else { 0.0 }
                }
                EntityKind::MLeader(m) => {
                    let lines: f64 = m.leaders.iter().map(|l| l.len() as f64 + 7.0).sum();
                    lines + 3.0 + m.text.as_ref().map_or(0.0, |t| self.chars(&t.style, &t.contents, has_font_codes(&t.contents)))
                }
                EntityKind::Text(t) => {
                    if self.text {
                        self.chars(&t.style, &t.value, false)
                    } else {
                        0.0
                    }
                }
                // Attribute definitions draw even without text (`render` draws their tag).
                EntityKind::AttDef(a) => self.chars(&a.text.style, &a.tag, false),
                EntityKind::MText(t) => {
                    if self.text {
                        self.chars(&t.style, &t.contents, has_font_codes(&t.contents))
                    } else {
                        0.0
                    }
                }
                EntityKind::Table(t) => {
                    let (rows, cols) = (t.row_heights.len().min(10_000), t.col_widths.len().min(10_000));
                    let mut n = (rows * cols) as f64 * 12.0;
                    if self.text {
                        for row in t.cells.iter().take(rows) {
                            for c in row.iter().take(cols) {
                                n += self.chars("Standard", &c.text, false);
                            }
                        }
                    }
                    n
                }
                EntityKind::Hatch(h) => self.hatch(h, tol),
                EntityKind::Insert(ins) => {
                    let mut n = 0.0;
                    if f.depth < MAX_BLOCK_DEPTH {
                        if let Some(b) = self.d.block(&ins.block) {
                            let rows = f64::from(ins.rows.clamp(1, 10_000));
                            let cols = f64::from(ins.cols.clamp(1, 10_000));
                            let s = ins.scale.x.abs().max(ins.scale.y.abs());
                            let s = if s.is_finite() && s > 0.0 { s } else { 1.0 };
                            let sub = self.sub_frame(e, f, f.scale * s);
                            n += rows * cols * (1.0 + self.block(b, &sub));
                        }
                        if self.text {
                            n += ins.attribs.iter().filter(|a| !a.invisible).map(|a| self.chars(&a.text.style, &a.text.value, false)).sum::<f64>();
                        }
                    }
                    n
                }
                EntityKind::Dimension(dm) => match dm.block.as_ref().and_then(|n| self.d.block(n)) {
                    Some(b) if f.depth < MAX_BLOCK_DEPTH => {
                        let sub = self.sub_frame(e, f, f.scale);
                        self.block(b, &sub)
                    }
                    // Its text is laid out whether or not it is drawn.
                    _ => DIMENSION_LINES + self.dim_text(dm),
                },
                EntityKind::Unknown(_) => 0.0,
            }
        }

        /// The glyphs of a dimension's generated text.
        fn dim_text(&self, dm: &Dimension) -> f64 {
            let st = dimension_style(self.d, dm);
            let n = dimension_text_chars(dm, &st);
            if outline_font(&self.font_of(&st.text_style)) {
                n * if self.textfill { OUTLINE_CHAR } else { OUTLINE_CHAR / 8.0 }
            } else {
                n * STROKE_CHAR
            }
        }

        fn polyline(&mut self, p: &LwPolyline, lt: Option<&Linetype>, scale: f64, tol: f64) -> f64 {
            let segs = Polyline { vertices: p.vertices.clone(), closed: p.closed }.segments();
            let wide = p.const_width > 0.0 || p.vertices.iter().any(|v| v.start_width > 0.0 || v.end_width > 0.0);
            if wide && self.fill {
                return segs.iter().map(|s| (Self::seg_points(s, tol) - 1.0) * 6.0).sum::<f64>() + 1.0;
            }
            if wide || p.plinegen || lt.is_none() {
                let len: f64 = segs.iter().map(Segment::len).sum();
                let pts: f64 = segs.iter().map(|s| Self::seg_points(s, tol)).sum::<f64>().max(1.0);
                return Self::stroke(len, pts, lt, scale);
            }
            segs.iter().map(|s| Self::stroke(s.len(), Self::seg_points(s, tol), lt, scale)).sum()
        }

        /// The context of a block reference's objects (`render` `sub_ctx`).
        fn sub_frame(&self, e: &Entity, f: &Frame, scale: f64) -> Frame {
            let layer = if e.common.layer == "0" { f.layer.clone() } else { Some(e.common.layer.clone()) };
            let ltype = match e.common.linetype.to_ascii_lowercase().as_str() {
                "byblock" => f.ltype.clone(),
                "bylayer" => self.layer(layer.as_deref().unwrap_or("0")).map_or_else(|| "Continuous".to_string(), |l| l.linetype.clone()),
                _ => e.common.linetype.clone(),
            };
            Frame { depth: f.depth + 1, scale, layer, ltype }
        }

        /// One copy of a block's objects in frame `f`, remembered per block,
        /// depth, scale (to a power of two, rounded up) and inherited layer
        /// and linetype.
        fn block(&mut self, b: &'d Block, f: &Frame) -> f64 {
            let bucket = f.scale.log2().ceil().clamp(-60.0, 60.0) as i32;
            let key = (b.name.clone(), f.depth, bucket, f.layer.clone().unwrap_or_default().to_ascii_lowercase(), f.ltype.to_ascii_lowercase());
            if let Some(n) = self.memo.get(&key) {
                return *n;
            }
            let frame = Frame { scale: 2f64.powi(bucket), ..f.clone() };
            let mut n = 0.0;
            for be in b.entities.iter() {
                n += self.entity(be, &frame);
            }
            self.memo.insert(key, n);
            n
        }

        /// A hatch as `render/src/hatch.rs` draws it: each pattern family
        /// swept across the boundary (every boundary edge crossed per line,
        /// at most 20,000 lines a family), then dashed (at most 100,000
        /// dashes a span); a solid fill by slabs (`fill.rs`, at most 6
        /// million corners).
        fn hatch(&mut self, h: &Hatch, tol: f64) -> f64 {
            let loops: Vec<Vec<Segment>> = h.loops.iter().map(|l| Polyline { vertices: l.vertices.clone(), closed: true }.segments()).collect();
            let edges: f64 = loops.iter().flatten().map(|s| Self::seg_points(s, tol) - 1.0).sum::<f64>().max(1.0);
            if h.solid || h.pattern.eq_ignore_ascii_case("SOLID") || h.gradient.is_some() {
                return if self.fill { solid_fill(h, tol, edges) } else { 0.0 };
            }
            let families = self.pattern(&h.pattern);
            let scale = if h.scale.is_finite() && h.scale > 1e-9 { h.scale } else { 1.0 };
            let mut n = edges;
            for fam in &families {
                let spacing = (fam.delta.1 * scale).abs();
                if spacing < 1e-9 {
                    continue;
                }
                let ang = fam.angle.to_radians() + h.angle;
                // The boundary in the family's frame, its arcs bounded by
                // their bulge.
                let (mut lo, mut hi) = (cadcraft_engine::geom::Vec2::new(f64::INFINITY, f64::INFINITY), cadcraft_engine::geom::Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
                let mut crossings = 0.0;
                for (l, segs) in h.loops.iter().zip(&loops) {
                    for v in &l.vertices {
                        let q = v.p.rotate(-ang);
                        lo = lo.min(q);
                        hi = hi.max(q);
                    }
                    for s in segs {
                        let (a, b) = (s.start().rotate(-ang), s.end().rotate(-ang));
                        let (dy, twice) = match s {
                            Segment::Line(_) => ((a.y - b.y).abs(), 1.0),
                            Segment::Arc { arc, .. } => {
                                let r = arc.radius.abs();
                                lo = lo.min(arc.center.rotate(-ang) - cadcraft_engine::geom::Vec2::new(r, r));
                                hi = hi.max(arc.center.rotate(-ang) + cadcraft_engine::geom::Vec2::new(r, r));
                                ((a.y - b.y).abs() + 2.0 * r, 2.0)
                            }
                        };
                        crossings += twice * (dy / spacing + 1.0);
                    }
                }
                if hi.y.is_nan() || lo.y.is_nan() || hi.y < lo.y {
                    continue;
                }
                // Past 20,000 lines the engine skips the family, but this
                // frame bounds the boundary loosely: count the most it draws.
                let lines = ((hi.y - lo.y) / spacing + 2.0).min(20_001.0);
                let spans = crossings.min(lines * edges * 2.0) / 2.0;
                let width = (hi.x - lo.x).max(0.0);
                let dashes: Vec<f64> = fam.dashes.iter().map(|d| d * scale).collect();
                let pat_len: f64 = dashes.iter().map(|d| d.abs()).sum();
                let runs = if dashes.is_empty() || pat_len < 1e-12 {
                    spans
                } else {
                    let elements = dashes.len() as f64;
                    let on = dashes.iter().filter(|d| **d >= 0.0).count() as f64;
                    let steps = ((lines * width / pat_len + spans) * elements).min(spans * 100_000.0);
                    steps / elements * on
                };
                // Two points and a primitive per run, and every boundary edge
                // tested per line (a test is a small fraction of a drawn point).
                n += runs * 3.0 + lines * edges / 64.0;
            }
            n
        }
    }

    /// A solid hatch's slab triangulation (`render/src/fill.rs`
    /// `triangulate_evenodd`): three corners per active edge per slab, cut
    /// off past six million; nothing past 200,000 edges.
    fn solid_fill(h: &Hatch, tol: f64, edges: f64) -> f64 {
        if edges > 200_000.0 {
            return edges;
        }
        let mut spans: Vec<(f64, f64)> = Vec::new();
        for l in &h.loops {
            let pts = Polyline { vertices: l.vertices.clone(), closed: true }.tessellate(tol);
            let n = pts.len();
            if n < 3 {
                continue;
            }
            for i in 0..n {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                if (a.y - b.y).abs() > 1e-15 && a.is_finite() && b.is_finite() {
                    spans.push((a.y.min(b.y), a.y.max(b.y)));
                }
            }
        }
        if spans.is_empty() || spans.len() > 200_000 {
            return edges;
        }
        let mut ys: Vec<f64> = spans.iter().flat_map(|(a, b)| [*a, *b]).collect();
        ys.sort_by(f64::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
        let at = |y: f64| ys.partition_point(|v| *v < y - 1e-12);
        let active: f64 = spans.iter().map(|(a, b)| at(*b).saturating_sub(at(*a)) as f64).sum();
        (active * 3.0).min(6_600_000.0) + edges
    }

    /// Whether MTEXT contents switch fonts inline (`\f…;`), which may name a
    /// TrueType font whatever the style says.
    fn has_font_codes(contents: &str) -> bool {
        contents.contains("\\f") || contents.contains("\\F")
    }

    /// A dimension's style with its own overrides, for the fields its text
    /// depends on (the engine's `DimStyle::with_overrides` round-trips the
    /// whole style through JSON per dimension).
    fn dimension_style(d: &Drawing, dm: &Dimension) -> cadcraft_engine::doc::DimStyle {
        let mut st = d.dim_style(&dm.style).cloned().unwrap_or_default();
        for (k, v) in &dm.overrides {
            let text = || v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());
            let small = || v.as_f64().map_or(0, |x| x.clamp(0.0, 255.0).round() as u8);
            let on = || v.as_bool().unwrap_or_else(|| v.as_f64().is_some_and(|x| x != 0.0));
            match cadcraft_engine::doc::DimStyle::field_name(k) {
                Some("post") => st.post = text(),
                Some("altPost") => st.alt_post = text(),
                Some("decimalSeparator") => st.decimal_separator = text(),
                Some("textStyle") => st.text_style = text(),
                Some("decimals") => st.decimals = small(),
                Some("tolDecimals") => st.tol_decimals = small(),
                Some("altDecimals") => st.alt_decimals = small(),
                Some("angularDecimals") => st.angular_decimals = small(),
                Some("linearFactor") => st.linear_factor = v.as_f64().unwrap_or(st.linear_factor),
                Some("limits") => st.limits = on(),
                Some("tolerance") => st.tolerance = on(),
                Some("alt") => st.alt = on(),
                _ => {}
            }
        }
        st
    }

    /// The characters of a dimension's generated text, from above
    /// (`render/src/dim.rs` `linear_text`): every number at its decimals
    /// and separator, DIMPOST's `<>` each replaced by the measurement, the
    /// dimension's own text's `<>` each replaced by all of that.
    pub(super) fn dimension_text_chars(dm: &Dimension, st: &cadcraft_engine::doc::DimStyle) -> f64 {
        let reach = [dm.defpt, dm.text_mid, dm.p13, dm.p14, dm.p15, dm.p16].iter().map(|p| p.x.abs().max(p.y.abs())).fold(0.0, f64::max);
        let digits = (reach * 2.0 * st.linear_factor.abs()).max(1.0).log10().min(400.0) + 2.0;
        let decimals = f64::from(st.decimals.max(st.tol_decimals).max(st.alt_decimals).max(st.angular_decimals));
        let num = digits + decimals + st.decimal_separator.len() as f64 + 2.0;
        let main = if st.limits { 2.0 * num + 24.0 } else { num + 2.0 };
        let template = |t: &str, inner: f64| {
            let k = t.matches("<>").count() as f64;
            if k > 0.0 {
                k * inner + t.len() as f64
            } else {
                inner + t.len() as f64
            }
        };
        let mut base = template(&st.post, main);
        if st.tolerance {
            base += 2.0 * num + 24.0;
        }
        if st.alt {
            base += template(&st.alt_post, num) + 4.0;
        }
        if dm.text.is_empty() {
            base
        } else if dm.text.trim().is_empty() {
            0.0
        } else {
            dm.text.matches("<>").count() as f64 * base + dm.text.len() as f64
        }
    }

    /// The chord tolerance `cadcraft_io::pdf` draws model space with: about
    /// 0.05 mm on the fitted sheet.
    fn pdf_tolerance(d: &Drawing) -> f64 {
        let o = cadcraft_io::pdf::PdfOptions { compress: true, ..Default::default() };
        let Ok(page) = cadcraft_io::pdf::page_for(d, &Space::Model, &o) else { return 0.001 };
        let unit_mm = cadcraft_engine::render::paper::paper_unit_mm(d);
        let sheet = cadcraft_engine::render::Sheet::from_page(&page, unit_mm);
        let b = d.extents(&Space::Model);
        let fit = if b.is_empty() { 1.0 } else { (sheet.printable.width() / b.width().max(1e-12)).min(sheet.printable.height() / b.height().max(1e-12)) };
        let fit = if fit.is_finite() && fit > 0.0 { fit } else { 1.0 };
        let tol = 0.05 / unit_mm / fit;
        if tol.is_finite() && tol > 0.0 { tol } else { 0.001 }
    }

    /// What drawing `d`'s model space as `format` (png, svg or pdf) would
    /// take.
    pub(super) fn drawing_cost(d: &Drawing, format: &str) -> f64 {
        // PNG and SVG draw with the renderer's default 0.001 chords.
        let (tol, plotting) = if format == "pdf" { (pdf_tolerance(d), true) } else { (0.001, false) };
        Draw::new(d, tol, true, plotting).space(&Space::Model)
    }

    /// Refuse to draw `d` as `format` past [`RENDER_CEILING`].
    pub(super) fn drawable(ctx: &str, d: &Drawing, format: &str) -> Result<(), String> {
        let n = drawing_cost(d, format);
        if n > RENDER_CEILING {
            return Err(format!(
                "{ctx}: drawing this as {} would take {} display points, more than the {} the service draws at once (a hatch pattern or linetype too fine for its objects, a large array of a block, a very large table or text); coarsen the pattern or linetype scale, or write DXF or DWG",
                format.to_ascii_uppercase(),
                shown(n),
                shown(RENDER_CEILING)
            ));
        }
        Ok(())
    }

    /// What writing `d` as DXF (or DWG, through DXF) lays out beyond its
    /// objects: `dxf_write.rs` draws every dimension twice (its text
    /// position, and the anonymous block AutoCAD expects), text included.
    pub(super) fn writing_cost(d: &Drawing) -> f64 {
        let draw = Draw::new(d, 0.001, true, false);
        let stores = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities));
        stores.flat_map(|st| st.iter()).filter_map(|e| if let EntityKind::Dimension(dm) = &e.kind { Some(2.0 * (DIMENSION_LINES + draw.dim_text(dm))) } else { None }).sum()
    }

    /// Refuse to write `d` as DXF or DWG past [`RENDER_CEILING`].
    pub(super) fn writable(ctx: &str, d: &Drawing) -> Result<(), String> {
        let n = writing_cost(d);
        if n > RENDER_CEILING {
            return Err(format!(
                "{ctx}: writing this drawing would lay out {} display points of dimensions, more than the {} the service lays out at once (dimension text templates whose `<>` multiply)",
                shown(n),
                shown(RENDER_CEILING)
            ));
        }
        Ok(())
    }

    // ---------------- before a command: what it would add, what it would take ----------------

    /// What `hs` hold: how many objects and their weight.
    fn held(d: &Drawing, hs: &[Handle]) -> (f64, f64) {
        let es = existing(d, hs);
        (es.len() as f64, es.iter().map(|e| weight(e)).sum())
    }

    /// The path a `measure`, `divide` or `arraypath` walks.
    fn path_of(s: &Session, id: &str, p: &Json) -> Option<Chain> {
        let d = s.doc().ok()?;
        let h = if id == "arraypath" { p.get("path").and_then(handle_value)? } else { targets(s, p).first().copied()? };
        Chain::of(&d.entity(h)?.kind)
    }

    /// How many items a `arraypath {spacing}` or `measure` places along its
    /// path (the engine stops at 20,000, `MAX_GEN`).
    fn items_along(s: &Session, id: &str, p: &Json) -> f64 {
        let Some(c) = path_of(s, id, p) else { return 1.0 };
        let l = c.len();
        let step = if id == "arraypath" { p.get("spacing").and_then(Json::as_f64) } else { p.get("length").and_then(Json::as_f64) };
        match step {
            Some(sp) if sp.is_finite() && sp > 0.0 && l.is_finite() => ((l / sp).floor() + 1.0).clamp(1.0, 20_000.0),
            _ => 1.0,
        }
    }

    /// Objects and weight one `explode` of `e` makes (`modify.rs`
    /// `explode_kind`), the source removed.
    fn explode_parts(d: &Drawing, e: &Entity) -> Option<(f64, f64)> {
        let per = 1.0 + text_weight(&e.common.layer) + text_weight(&e.common.linetype);
        Some(match &e.kind {
            EntityKind::LwPolyline(p) => {
                let n = p.vertices.len() as f64;
                (n, n * (per + 1.0))
            }
            EntityKind::Polyline3d(p) => {
                let n = p.points.len() as f64;
                (n, n * (per + 1.0))
            }
            EntityKind::Insert(i) => {
                let b = d.block(&i.block)?;
                (b.entities.len() as f64, b.entities.iter().map(|e| weight(e)).sum())
            }
            EntityKind::MText(m) => {
                let lines = (m.contents.matches("\\P").count() + m.contents.matches('\n').count() + 1) as f64;
                (lines, lines * per + text_weight(&m.contents))
            }
            EntityKind::Dimension(dm) => {
                // Its lines and arrows, and one MTEXT holding its whole text.
                let text = dimension_text_chars(dm, &dimension_style(d, dm));
                (16.0, 16.0 * per + text / 2.0)
            }
            _ => return None,
        })
    }

    /// The leaves of a block reference `ncopy` copies (`modify2.rs`
    /// `nested_leaves`): every object of the block and of the blocks it
    /// references, nested to 16 levels, the engine stopping a level once it
    /// holds 20,000.
    fn leaves(d: &Drawing, name: &str, depth: usize, memo: &mut HashMap<(String, usize), (f64, f64)>, steps: &mut u64) -> (f64, f64) {
        if depth >= MAX_BLOCK_DEPTH {
            return (0.0, 0.0);
        }
        let Some(b) = d.block(name) else { return (0.0, 0.0) };
        let key = (b.name.clone(), depth);
        if let Some(v) = memo.get(&key) {
            return *v;
        }
        let mut out = (0.0, 0.0);
        for e in b.entities.iter() {
            *steps += 1;
            if *steps > ESTIMATE_STEPS {
                return (f64::INFINITY, f64::INFINITY);
            }
            match &e.kind {
                EntityKind::Insert(i) => {
                    let (n, w) = leaves(d, &i.block, depth + 1, memo, steps);
                    out = (out.0 + n, out.1 + w);
                }
                EntityKind::AttDef(_) => {}
                _ => out = (out.0 + 1.0, out.1 + weight(e)),
            }
        }
        memo.insert(key, out);
        out
    }

    /// What `id` would add to the call's drawings, objects and weight, when
    /// it copies; `factor` is its copy factor (the gate's, or the service's
    /// for what the gate cannot see).
    fn growth(s: &Session, id: &str, p: &Json, factor: f64) -> Option<(f64, f64)> {
        let d = s.doc().ok()?;
        let copying = |copies: f64| {
            let (n, w) = held(d, &targets(s, p));
            Some((n * copies, w * copies))
        };
        let flag = |k: &str| p.get(k).and_then(Json::as_bool).unwrap_or(false);
        match id {
            "copy" => copying(count_param(p, "count", 1.0).ok()?.clamp(1.0, MAX_COPIES)),
            "arrayrect" | "arraypolar" => copying(factor - 1.0),
            "arraypath" => {
                let path = p.get("path").and_then(handle_value);
                let hs: Vec<Handle> = targets(s, p).into_iter().filter(|h| Some(*h) != path).collect();
                let (n, w) = held(d, &hs);
                Some((n * (factor - 1.0), w * (factor - 1.0)))
            }
            "rotate" | "scale" if flag("copy") => copying(1.0),
            "mirror" if !flag("erase") => copying(1.0),
            "grip.rotate" | "grip.scale" | "grip.mirror" if flag("copy") => copying(1.0),
            "grip.move" if flag("copy") && grip_moves(p) => copying(1.0),
            "copyclip" | "copybase" | "cutclip" => copying(1.0),
            // Two centre lines per circle or arc.
            "centermark" => copying(2.0),
            "pasteclip" | "pasteorig" => Some((s.clipboard.len() as f64, s.clipboard.iter().map(weight).sum())),
            "explode" => {
                let (mut n, mut w) = (0.0, 0.0);
                for e in existing(d, &targets(s, p)) {
                    if let Some((pn, pw)) = explode_parts(d, e) {
                        n += pn;
                        w += pw;
                    }
                }
                Some((n, w))
            }
            "ncopy" => {
                let e = existing(d, &targets(s, p)).into_iter().next()?;
                let EntityKind::Insert(i) = &e.kind else { return None };
                if p.get("pick").is_some() {
                    return Some((1.0, 1.0));
                }
                let (n, w) = leaves(d, &i.block, 0, &mut HashMap::new(), &mut 0);
                let largest = d.blocks.values().map(|b| b.entities.len()).max().unwrap_or(0) as f64;
                let kept = n.min(20_000.0 + largest);
                Some((kept, if n > 0.0 { w * kept / n } else { 0.0 }))
            }
            "insert" => {
                let b = d.block(p.get("name").and_then(Json::as_str)?)?;
                let attribs: f64 = b.entities.iter().filter(|e| matches!(e.kind, EntityKind::AttDef(_))).map(|e| weight(e)).sum();
                if flag("explode") {
                    Some((b.entities.len() as f64, b.entities.iter().map(|e| weight(e)).sum::<f64>() + attribs))
                } else {
                    Some((1.0, 1.0 + attribs))
                }
            }
            "divide" | "measure" => {
                let n = if id == "divide" { count_param(p, "segments", 1.0).ok()? } else { factor.max(items_along(s, id, p)) };
                let each = match p.get("block").and_then(Json::as_str).and_then(|b| d.block(b)) {
                    Some(b) => 1.0 + b.entities.iter().filter(|e| matches!(e.kind, EntityKind::AttDef(_))).map(|e| weight(e)).sum::<f64>(),
                    None => 2.0,
                };
                Some((n, n * each))
            }
            "block" => {
                let (_, w) = held(d, &targets(s, p));
                Some((0.0, w))
            }
            "layout.copy" | "layout" => {
                if id == "layout" && !matches!(p.get("option").and_then(Json::as_str).map(str::to_ascii_lowercase).as_deref(), Some("copy" | "c")) {
                    return None;
                }
                let from = p.get("from").or_else(|| p.get("name")).and_then(Json::as_str)?;
                let l = d.layout(from)?;
                Some((l.entities.len() as f64, l.entities.iter().map(|e| weight(e)).sum()))
            }
            "line" | "point" | "point.multiple" | "dimcontinue" | "dimbaseline" | "qdim" => {
                // One object per point (a line per pair), with the current
                // layer and linetype; a dimension holds more.
                let per = 1.0 + text_weight(&d.header.str("CLAYER", "0")) + text_weight(&d.header.str("CELTYPE", "ByLayer"));
                let points = p.get("points").and_then(Json::as_array).map_or(1.0, |a| a.len() as f64);
                let n = match id {
                    "qdim" => qdim_points(d, &targets(s, p)),
                    // `line` draws a line between each pair, and closes.
                    "line" => (points - 1.0).max(0.0) + f64::from(u8::from(points > 2.0 && p.get("closed").and_then(Json::as_bool).unwrap_or(false))),
                    _ => points,
                };
                let per = if matches!(id, "line" | "point" | "point.multiple") { per } else { per + 16.0 };
                Some((n, n * per))
            }
            "pedit" => {
                let opt = p.get("option").and_then(Json::as_str).unwrap_or("").to_ascii_lowercase();
                let vertices: f64 = existing(d, &targets(s, p))
                    .iter()
                    .map(|e| match &e.kind {
                        EntityKind::LwPolyline(pl) => pl.vertices.len() as f64,
                        _ => 2.0,
                    })
                    .sum();
                match opt.as_str() {
                    // A fit curve doubles the vertices; a spline frame
                    // multiplies them by SPLINESEGS (at most 20,000 each).
                    "fit" | "f" => Some((0.0, vertices)),
                    "spline" | "s" => Some((0.0, (vertices * d.header.i64("SPLINESEGS", 8).clamp(1, 64) as f64).min(20_000.0 * existing(d, &targets(s, p)).len() as f64))),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Whether `grip.move` moves objects about a grip (rather than stretching one).
    fn grip_moves(p: &Json) -> bool {
        p.get("mode").and_then(Json::as_str).is_some_and(|m| m.eq_ignore_ascii_case("move")) || (p.get("index").is_none() && p.get("base").is_some())
    }

    /// The points `qdim` dimensions between: every segment end, circle centre
    /// and point of its objects (`annotate.rs` `run_qdim`).
    fn qdim_points(d: &Drawing, hs: &[Handle]) -> f64 {
        existing(d, hs)
            .iter()
            .map(|e| match &e.kind {
                EntityKind::LwPolyline(p) => 2.0 * p.vertices.len() as f64,
                EntityKind::Polyline3d(p) => 2.0 * p.points.len() as f64,
                EntityKind::Hatch(h) => 2.0 * h.loops.iter().map(|l| l.vertices.len()).sum::<usize>() as f64,
                EntityKind::Leader(l) => 2.0 * l.vertices.len() as f64,
                EntityKind::MLeader(m) => 2.0 * m.leaders.iter().map(Vec::len).sum::<usize>() as f64,
                EntityKind::Wipeout(w) => 2.0 * w.boundary.len() as f64,
                _ => 8.0,
            })
            .sum()
    }

    /// The segments an object contributes as a cutting edge of `trim` and
    /// `extend` (`modify.rs` `edge_segments`, `prim_to_segments`).
    fn edge_segments(e: &Entity) -> f64 {
        match &e.kind {
            EntityKind::LwPolyline(p) => p.vertices.len() as f64,
            EntityKind::Polyline3d(p) => p.points.len() as f64,
            EntityKind::Hatch(h) => h.loops.iter().map(|l| l.vertices.len() as f64).sum(),
            EntityKind::Leader(l) => l.vertices.len() as f64,
            EntityKind::MLeader(m) => m.leaders.iter().map(|l| l.len() as f64).sum(),
            EntityKind::Wipeout(w) => w.boundary.len() as f64 + 1.0,
            EntityKind::Ellipse(el) => {
                let (major, sweep) = (el.major.xy().len(), ccw_sweep(el.start, el.end));
                (arc_segments(major, sweep, major * 1e-4).max(8) + 1) as f64
            }
            EntityKind::Spline(sp) => spline_samples(sp, 1e-4),
            EntityKind::Solid(_) | EntityKind::Trace(_) | EntityKind::Face3d(_) | EntityKind::Viewport(_) => 4.0,
            EntityKind::Line(_) | EntityKind::Arc(_) | EntityKind::Circle(_) | EntityKind::Ray(_) | EntityKind::XLine(_) => 1.0,
            _ => 0.0,
        }
    }

    /// The points the fine tessellation of a polyline takes at `tol`.
    fn fine_points(vertices: &[cadcraft_engine::geom::PolyVertex], tol: f64) -> f64 {
        Polyline { vertices: vertices.to_vec(), closed: true }
            .segments()
            .iter()
            .map(|s| match s {
                Segment::Line(_) => 1.0,
                Segment::Arc { arc, .. } => arc_segments(arc.radius, arc.sweep(), tol) as f64,
            })
            .sum()
    }

    /// The fine tessellation `area`, `list`, `properties` and `massprop`
    /// make of an object.
    fn fine_cost(e: &Entity, id: &str) -> f64 {
        let tol = if id == "massprop" { 1e-5 } else { 1e-4 };
        match &e.kind {
            EntityKind::LwPolyline(p) => fine_points(&p.vertices, tol),
            EntityKind::Hatch(h) => h.loops.iter().map(|l| fine_points(&l.vertices, tol)).sum(),
            EntityKind::Circle(c) if id == "massprop" => arc_segments(c.radius, TAU, c.radius * 1e-5) as f64,
            _ => 1.0,
        }
    }

    /// The candidates `assoc::find_snap` scans per definition point: every
    /// object's snap points in the space.
    fn snap_points(d: &Drawing, space: &Space) -> f64 {
        d.space(space)
            .map(|st| {
                st.iter()
                    .map(|e| match &e.kind {
                        EntityKind::Line(_) => 3.0,
                        EntityKind::Arc(_) => 4.0,
                        EntityKind::Circle(_) | EntityKind::Point(_) => 1.0,
                        EntityKind::LwPolyline(p) => p.vertices.len().min(10_000) as f64,
                        _ => 0.0,
                    })
                    .sum()
            })
            .unwrap_or(0.0)
    }

    /// What drawing the block references, dimensions and multileaders of a
    /// space takes for hit tests (`select.rs` `hit_polylines` draws them,
    /// text off), of those whose bounds `tested` keeps; with `outlines`,
    /// every other object too, tessellated only, as `hatch.rs` `outlines`
    /// traces the space around a pick point.
    fn hit_cost(d: &Drawing, space: &Space, tol: f64, tested: &dyn Fn(&cadcraft_engine::geom::Bounds2) -> bool, outlines: bool) -> f64 {
        let Some(store) = d.space(space) else { return 0.0 };
        let mut draw = Draw::new(d, tol, false, false);
        let top = Draw::top();
        let mut n = 0.0;
        for e in store.iter() {
            draw.dashes = true;
            let drawn = matches!(e.kind, EntityKind::Insert(_) | EntityKind::MLeader(_)) || (!outlines && matches!(e.kind, EntityKind::Dimension(_)));
            if drawn && !tested(&cadcraft_engine::doc::entity_bounds(d, e, 0)) {
                continue;
            }
            n += match &e.kind {
                _ if drawn => draw.entity(e, &top),
                EntityKind::Dimension(_) | EntityKind::Hatch(_) | EntityKind::Text(_) | EntityKind::MText(_) => 0.0,
                EntityKind::AttDef(_) | EntityKind::Table(_) | EntityKind::Image(_) if outlines => 5.0,
                // Tessellated, not dashed.
                _ if outlines => {
                    draw.dashes = false;
                    draw.entity(e, &top)
                }
                _ => 0.0,
            };
        }
        n
    }

    /// Refuse `id` when what it would take, beyond its own parameters,
    /// scales past the service's bounds with the drawing (`ctx` names the
    /// caller in the message).
    pub(super) fn work(ctx: &str, s: &Session, id: &str, p: &Json) -> Result<(), String> {
        let Ok(d) = s.doc() else { return Ok(()) };
        // `what` reads after "would", with `{n}` for the amount.
        let refuse = |what: &str, n: f64, max: f64, hint: &str| -> Result<(), String> {
            if n > max {
                return Err(format!("{ctx}: `{id}` would {}, more than the {} the door allows in one command{hint}", what.replace("{n}", &shown(n)), shown(max)));
            }
            Ok(())
        };
        let space = s.space();
        let option = || p.get("option").and_then(Json::as_str).map(str::to_ascii_lowercase);
        match id {
            "trim" | "extend" => {
                let Some(target) = existing(d, &targets(s, p)).into_iter().next() else { return Ok(()) };
                let samples = match &target.kind {
                    EntityKind::Ellipse(_) => 720.0,
                    EntityKind::Spline(_) => 512.0,
                    EntityKind::LwPolyline(pl) => pl.vertices.len().max(1) as f64,
                    _ => 1.0,
                };
                let named: Option<Vec<Handle>> = p.get("edges").and_then(Json::as_array).map(|a| a.iter().filter_map(handle_value).collect());
                let edges: f64 = match &named {
                    Some(hs) => existing(d, hs).iter().filter(|e| e.handle != target.handle).map(|e| edge_segments(e)).sum(),
                    None => d.space(&space).map_or(0.0, |st| st.iter().filter(|e| e.handle != target.handle && d.is_visible(e)).map(|e| edge_segments(e)).sum()),
                };
                refuse("test {n} segment intersections", samples * edges + edges, TRIM_TESTS, " (name its cutting `edges`)")
            }
            "overkill" => {
                let hs = if p.get("handles").is_some() { targets(s, p) } else { d.space(&space).map(|st| st.handles()).unwrap_or_default() };
                let (n, w) = held(d, &hs);
                refuse("compare objects by their weight, {n} steps", n * w, OVERKILL_WORK, " (give `handles`)")
            }
            "join" => {
                let pieces: f64 = existing(d, &targets(s, p))
                    .iter()
                    .map(|e| match &e.kind {
                        EntityKind::LwPolyline(pl) if !pl.closed => pl.vertices.len() as f64,
                        EntityKind::Line(_) | EntityKind::Arc(_) => 1.0,
                        _ => 0.0,
                    })
                    .sum();
                refuse("chain {n} pieces", pieces, JOIN_PIECES, "")
            }
            "pedit" if matches!(option().as_deref(), Some("join" | "j")) => {
                let pool = p.get("handles2").or_else(|| p.get("join")).and_then(Json::as_array).map_or(0.0, |a| a.len() as f64);
                refuse("chain {n} objects", pool, JOIN_PIECES, "")
            }
            "qdim" | "dimcontinue" | "dimbaseline" | "dimreassociate" if d.header.i64("DIMASSOC", 2) != 0 => {
                let dims = match id {
                    "qdim" => qdim_points(d, &targets(s, p)),
                    "dimreassociate" => existing(d, &targets(s, p)).iter().filter(|e| matches!(e.kind, EntityKind::Dimension(_))).count() as f64 * 2.0,
                    _ => p.get("points").and_then(Json::as_array).map_or(0.0, |a| a.len() as f64),
                };
                refuse("test {n} snap points to attach its dimensions", dims * 2.0 * snap_points(d, &space), SNAP_TESTS, "")
            }
            "hatch" | "gradient" | "boundary" => {
                let picks = p.get("points").and_then(Json::as_array).map_or(0.0, |a| a.len() as f64);
                if picks == 0.0 {
                    return Ok(());
                }
                let ext = d.extents(&space);
                let tol = (ext.width() + ext.height()).max(1e-9) / 20_000.0;
                let per_pick = hit_cost(d, &space, tol, &|_| true, true) + visits(d);
                refuse("trace {n} display points of outlines around its pick points", picks * per_pick, RENDER_CEILING, " (select the boundary objects as `handles`)")
            }
            "select" | "stretch" => {
                // A crossing window draws the objects that straddle its edge,
                // a fence those near it, a pick those under the aperture
                // (`select.rs`); a plain window and handles draw none.
                let points = |key: &str| p.get(key).and_then(Json::as_array).map(|w| w.iter().filter_map(cadcraft_engine::cmd::point_value).collect::<Vec<_>>());
                let bounds = |pts: &[cadcraft_engine::geom::Vec2]| {
                    pts.iter().fold(cadcraft_engine::geom::Bounds2::EMPTY, |mut b, q| {
                        b.add(*q);
                        b
                    })
                };
                let crossing = id == "stretch" || p.get("crossing").and_then(Json::as_bool).unwrap_or(false);
                let cost = if let Some(w) = points("window").filter(|w| w.len() >= 2) {
                    if !crossing {
                        return Ok(());
                    }
                    let bx = cadcraft_engine::geom::Bounds2::new(w[0], w[1]);
                    let tol = (bx.width() + bx.height()).max(1e-9) / 2000.0;
                    hit_cost(d, &space, tol, &|eb| eb.intersects(&bx) && !bx.contains_box(eb), false)
                } else if let Some(f) = points("fence").filter(|f| !f.is_empty()) {
                    let fb = bounds(&f);
                    let tol = (fb.width() + fb.height()).max(1e-9) / 2000.0;
                    let probe = fb.expand(tol);
                    hit_cost(d, &space, tol, &|eb| eb.intersects(&probe), false)
                } else if let Some(at) = p.get("at").and_then(cadcraft_engine::cmd::point_value) {
                    let ap = p.get("aperture").and_then(Json::as_f64).filter(|a| a.is_finite()).unwrap_or_else(|| s.pixel_size() * s.settings.pickbox * 1.5);
                    let probe = cadcraft_engine::geom::Bounds2::new(at, at).expand(ap.abs() * 2.0);
                    hit_cost(d, &space, ap.abs() / 4.0, &|eb| eb.intersects(&probe), false)
                } else {
                    return Ok(());
                };
                refuse("draw {n} display points of block references to test them", cost, RENDER_CEILING, " (select by `handles`)")
            }
            "divide" | "measure" | "arraypath" => {
                let Some(path) = path_of(s, id, p) else { return Ok(()) };
                let points = match id {
                    "divide" => count_param(p, "segments", 1.0).unwrap_or(1.0),
                    "arraypath" if !spaced_along_path(p) => count_param(p, "count", 6.0).unwrap_or(6.0).min(20_000.0),
                    _ => items_along(s, id, p),
                };
                refuse("take {n} steps along its path", points * path.segs.len() as f64, PATH_STEPS, "")
            }
            "splinedit" | "grip.stretch" | "grip.move" if id != "grip.move" || !grip_moves(p) => {
                let refits = id != "splinedit" || matches!(option().as_deref(), Some("close" | "open" | "move" | "refit"));
                let Some(e) = p.get("handle").and_then(handle_value).or_else(|| targets(s, p).first().copied()).and_then(|h| d.entity(h)) else { return Ok(()) };
                // Closing a fitted spline refits it through one more point.
                let closing = id == "splinedit" && option().as_deref() == Some("close");
                match &e.kind {
                    EntityKind::Spline(sp) if refits && sp.fit.len() >= 2 => {
                        refuse("refit a spline through {n} fit points", sp.fit.len() as f64 + f64::from(u8::from(closing)), MAX_FIT_POINTS, "")
                    }
                    EntityKind::LwPolyline(pl) if id != "splinedit" => refuse("take {n} steps to list a polyline's grips", (pl.vertices.len() as f64).powi(2), GRIP_WORK, ""),
                    _ => Ok(()),
                }
            }
            "grip.rotate" | "grip.scale" | "grip.mirror" | "grip.move" if p.get("base").is_none() => {
                let h = p.get("baseHandle").and_then(handle_value).or_else(|| targets(s, p).first().copied());
                match h.and_then(|h| d.entity(h)).map(|e| &e.kind) {
                    Some(EntityKind::LwPolyline(pl)) => refuse("take {n} steps to list a polyline's grips", (pl.vertices.len() as f64).powi(2), GRIP_WORK, ""),
                    _ => Ok(()),
                }
            }
            "area" | "measuregeom" | "massprop" | "list" | "properties" => {
                if id == "measuregeom" && !matches!(p.get("mode").and_then(Json::as_str).map(str::to_ascii_lowercase).as_deref(), Some("area" | "ar")) {
                    return Ok(());
                }
                let single = matches!(id, "area" | "measuregeom");
                if single && p.get("points").is_some() {
                    return Ok(());
                }
                let hs = targets(s, p);
                let es = existing(d, if single { &hs[..hs.len().min(1)] } else { &hs });
                refuse("tessellate {n} points", es.iter().map(|e| fine_cost(e, id)).sum(), FINE_POINTS, "")?;
                if matches!(id, "list" | "properties") {
                    refuse("copy objects of weight {n} into its answer", es.iter().map(|e| weight(e)).sum(), ANSWER_WEIGHT, " (ask about fewer objects)")?;
                }
                Ok(())
            }
            // A property filter (and the property list) serialise every
            // candidate whole and measure its area (`qselect.rs` `properties`).
            "qselect" | "qselect.info" if id == "qselect.info" || p.get("property").is_some() => {
                let apply = p.get("applyTo").and_then(Json::as_str).unwrap_or("drawing").to_ascii_lowercase();
                let hs = if apply.starts_with("sel") || apply == "current" {
                    s.selection()
                } else {
                    d.space(&space).map(|st| st.handles()).unwrap_or_default()
                };
                let es = existing(d, &hs);
                let objects: f64 = es.iter().map(|e| 1.0 + weight(e) / 16.0 + fine_cost(e, "properties") / 1000.0).sum();
                refuse("serialise the properties of {n} objects (by weight)", objects, QSELECT_OBJECTS, " (narrow it with `applyTo: selection`)")
            }
            "entities" | "drawing.inspect" => {
                let st = s.state().map_err(|e| format!("{ctx}: {e}"))?;
                let Some(store) = d.space(&st.space) else { return Ok(()) };
                let reported: f64 = if id == "entities" {
                    let lower = |k: &str| p.get(k).and_then(Json::as_str).map(str::to_ascii_lowercase);
                    let (ty, layer) = (lower("type"), lower("layer"));
                    let window = p.get("window").and_then(Json::as_array).and_then(|w| {
                        Some(cadcraft_engine::geom::Bounds2::new(cadcraft_engine::cmd::point_value(w.first()?)?, cadcraft_engine::cmd::point_value(w.get(1)?)?))
                    });
                    let limit = p.get("limit").and_then(Json::as_u64).unwrap_or(500) as usize;
                    let offset = p.get("offset").and_then(Json::as_u64).unwrap_or(0) as usize;
                    store
                        .iter()
                        .filter(|e| ty.as_ref().is_none_or(|t| e.kind.type_name().eq_ignore_ascii_case(t) || e.kind.dxf_name().eq_ignore_ascii_case(t)))
                        .filter(|e| layer.as_ref().is_none_or(|l| e.common.layer.eq_ignore_ascii_case(l)))
                        .filter(|e| window.is_none_or(|w| w.intersects(&cadcraft_engine::doc::entity_bounds(d, e, 0))))
                        .skip(offset)
                        .take(limit)
                        .map(|e| weight(e))
                        .sum()
                } else if p.get("entities").and_then(Json::as_bool).unwrap_or(true) {
                    store.iter().take(p.get("limit").and_then(Json::as_u64).unwrap_or(200) as usize).map(|e| weight(e)).sum()
                } else {
                    0.0
                };
                refuse("copy objects of weight {n} into its answer", reported, ANSWER_WEIGHT, " (lower `limit`)")
            }
            _ => Ok(()),
        }
    }

    /// The size of a JSON value as text, near enough.
    pub(super) fn answer_bytes(v: &Json) -> f64 {
        match v {
            Json::Null | Json::Bool(_) => 5.0,
            Json::Number(_) => 12.0,
            Json::String(s) => s.len() as f64 + 2.0,
            Json::Array(a) => 2.0 + a.iter().map(|x| answer_bytes(x) + 1.0).sum::<f64>(),
            Json::Object(o) => 2.0 + o.iter().map(|(k, x)| k.len() as f64 + 4.0 + answer_bytes(x)).sum::<f64>(),
        }
    }

    /// One call's bounds, run around each of its commands (`run`).
    pub(super) struct Watch {
        /// What the call's drawings may reach: the ceilings, or what they
        /// held when the call opened them, whichever is more.
        limit: Heft,
        /// What they held after the last command that changed them.
        now: Heft,
        /// Each open drawing's identity and version, to see what a command
        /// changed.
        seen: Vec<(u64, usize)>,
        clipboard: usize,
        /// The product of the copies the call has made so far.
        copies: f64,
        answered: f64,
    }

    fn versions(s: &Session) -> Vec<(u64, usize)> {
        s.docs.iter().map(|st| (st.uid, std::sync::Arc::as_ptr(&st.doc) as usize)).collect()
    }

    impl Watch {
        pub(super) fn new(s: &Session) -> Watch {
            let now = heft(s);
            let limit = Heft { entities: now.entities.max(ENTITY_CEILING), weight: now.weight.max(WEIGHT_CEILING), visits: now.visits.max(VISIT_CEILING) };
            Watch { limit, now, seen: versions(s), clipboard: s.clipboard.len(), copies: 1.0, answered: 0.0 }
        }

        /// Before `id` runs: what it would add, its copies within the call's,
        /// and its work.
        pub(super) fn before(&mut self, s: &Session, id: &str, p: &Json) -> Result<(), String> {
            // The gate's copy factor, and the service's for what it cannot
            // see (an `arraypath` by spacing, a `measure` into blocks).
            let own = match id {
                "arraypath" if spaced_along_path(p) => items_along(s, id, p),
                "measure" if p.get("block").is_some_and(Json::is_string) => items_along(s, id, p),
                _ => 1.0,
            };
            if own > MAX_COPIES {
                return Err(format!(
                    "cad.run: `{id}` would place {} copies along its path, more than the {} the door allows in one command",
                    shown(own),
                    shown(MAX_COPIES)
                ));
            }
            let factor = door()?.copies(id, p) * own;
            self.copies *= factor.max(1.0);
            if self.copies > COPIES_PER_CALL {
                return Err(format!(
                    "cad.run: `{id}`: the copies this call makes multiply to {}, more than the {} the door allows in one call (a copy multiplies what the commands before it made)",
                    shown(self.copies),
                    shown(COPIES_PER_CALL)
                ));
            }
            if let Some((n, w)) = growth(s, id, p, factor) {
                if self.now.entities + n > self.limit.entities {
                    return Err(format!(
                        "cad.run: `{id}` would bring the call's drawings to {} objects, more than the {} a call may reach",
                        shown(self.now.entities + n),
                        shown(self.limit.entities)
                    ));
                }
                if self.now.weight + w > self.limit.weight {
                    return Err(format!(
                        "cad.run: `{id}` would bring the call's drawings to a weight of {} (objects, vertices, cells and text), more than the {} a call may reach",
                        shown(self.now.weight + w),
                        shown(self.limit.weight)
                    ));
                }
            }
            work("cad.run", s, id, p)
        }

        /// After `id` ran: keep one undo step, weigh what changed against the
        /// call's ceilings, and the replies so far against theirs.
        pub(super) fn after(&mut self, s: &mut Session, id: &str, result: &Json) -> Result<(), String> {
            for st in &mut s.docs {
                let extra = st.undo.len().saturating_sub(UNDO_KEPT);
                st.undo.drain(..extra);
                let extra = st.redo.len().saturating_sub(UNDO_KEPT);
                st.redo.drain(..extra);
            }
            let seen = versions(s);
            if seen != self.seen || s.clipboard.len() != self.clipboard || matches!(id, "copyclip" | "copybase" | "cutclip") {
                self.seen = seen;
                self.clipboard = s.clipboard.len();
                self.now = heft(s);
                for (what, n, max) in [
                    ("objects", self.now.entities, self.limit.entities),
                    ("weight (objects, vertices, cells and text)", self.now.weight, self.limit.weight),
                    ("steps for one pass over their extents", self.now.visits, self.limit.visits),
                ] {
                    if n > max {
                        return Err(format!("cad.run: after `{id}` the call's drawings hold {} {what}, more than the {} a call may reach", shown(n), shown(max)));
                    }
                }
            }
            self.answered += answer_bytes(result);
            if self.answered > ANSWER_BYTES {
                return Err(format!(
                    "cad.run: after `{id}` the call's answers hold {} bytes, more than the {} a call returns (ask for less: `limit`, fewer objects)",
                    shown(self.answered),
                    shown(ANSWER_BYTES)
                ));
            }
            Ok(())
        }
    }
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

    /// `n` points zig-zagging across a 1,000-unit strip, for `line` to draw
    /// n − 1 lines of about 1,000 units each in one command.
    fn zigzag(n: usize) -> Json {
        Json::Array((0..n).map(|i| json!([if i % 2 == 0 { 0.0 } else { 1000.0 }, i as f64 * 0.01])).collect())
    }

    /// A call in a fresh area, as the system agent's: the answer or the refusal.
    fn run_in(dir: &Path, args: Json) -> Result<Json, String> {
        serve(&resolver(dir, None), &service_call("run", args, dir, false))
    }

    /// Every limit of the door, at its cap and one past it: the gate admits
    /// the first and refuses the second (naming what it counts) before any
    /// command runs. Absent counts weigh the engine's own defaults.
    #[test]
    fn the_door_caps_every_parameter_that_multiplies_work() {
        let dir = tempfile::tempdir().unwrap();
        let area = Area::new(dir.path(), None, false);
        let pts = |n: usize| Json::Array((0..n).map(|i| json!([i, (i * 7) % 13])).collect());
        let marks = |n: usize| "<>".repeat(n);
        let text = |n: usize| "x".repeat(n);
        let rows: Vec<(&str, &str, Json, Json)> = vec![
            ("arrayrect", "copies (`rows` × `cols`", json!({"rows": 100, "cols": 100}), json!({"rows": 101, "cols": 100})),
            ("arrayrect", "copies (`rows` × `cols`", json!({"rows": 2500}), json!({"rows": 2501})),
            ("arraypolar", "copies (`count`, 6 when absent)", json!({"count": 10_000, "center": [0, 0]}), json!({"count": 10_001, "center": [0, 0]})),
            ("arraypath", "copies (`count`, 6 when absent)", json!({"count": 10_000, "path": "1F"}), json!({"count": "10001", "path": "1F"})),
            ("copy", "copies", json!({"count": 10_000, "delta": [1, 0]}), json!({"count": 10_001, "delta": [1, 0]})),
            ("divide", "segments", json!({"segments": 32_767, "handle": "1F"}), json!({"segments": 32_768, "handle": "1F"})),
            ("divide", "block references", json!({"segments": 10_000, "handle": "1F", "block": "B"}), json!({"segments": 10_001, "handle": "1F", "block": "B"})),
            ("polygon", "sides", json!({"sides": 1024, "center": [0, 0], "radius": 1}), json!({"sides": 1025, "center": [0, 0], "radius": 1})),
            ("spline", "fit points (`fit`)", json!({"fit": pts(2000)}), json!({"fit": pts(2001)})),
            ("spline", "control points (`control`)", json!({"control": pts(20_000)}), json!({"control": pts(20_001)})),
            ("spline", "degree", json!({"control": pts(12), "degree": 10}), json!({"control": pts(12), "degree": 11})),
            ("splinedit", "fit points (`fit`)", json!({"option": "refit", "fit": pts(2000)}), json!({"option": "refit", "fit": pts(2001)})),
            ("table", "cells", json!({"at": [0, 0], "rows": 100, "cols": 200}), json!({"at": [0, 0], "rows": 101, "cols": 200})),
            ("table", "cells", json!({"at": [0, 0], "rows": 98, "cols": 200, "title": "T", "header": ["a"]}), json!({"at": [0, 0], "rows": 99, "cols": 200, "title": "T", "header": ["a"]})),
            ("hatch", "pick points (`points`)", json!({"points": pts(16)}), json!({"points": pts(17)})),
            ("gradient", "pick points (`points`)", json!({"points": pts(16)}), json!({"points": pts(17)})),
            ("boundary", "pick points (`points`)", json!({"points": pts(16)}), json!({"points": pts(17)})),
            ("hatch", "times the pattern's density", json!({"handles": ["1F"], "scale": 1e-4}), json!({"handles": ["1F"], "scale": 0.99e-4})),
            ("hatchedit", "times the pattern's density", json!({"scale": 1e-4}), json!({"scale": 0.99e-4})),
            ("properties.set", "times the pattern's density", json!({"scale": 1e-4}), json!({"scale": 0.99e-4})),
            ("ltscale", "times the linetype's dash density", json!({"scale": 1e-4}), json!({"scale": 0.99e-4})),
            ("properties.set", "times the linetype's dash density", json!({"ltscale": 1e-4}), json!({"ltscale": 0.99e-4})),
            ("entities", "objects per answer", json!({"limit": 2000}), json!({"limit": 2001})),
            ("drawing.inspect", "objects per answer", json!({"limit": 2000}), json!({"limit": 2001})),
            ("cal", "bytes of `expr`", json!({"expr": format!("1{}", "+1".repeat(511)) + " "}), json!({"expr": format!("1{}", "+1".repeat(512))})),
            ("find", "times the text it matches", json!({"find": "ab", "replace": text(32)}), json!({"find": "ab", "replace": text(33)})),
            ("dimstyle", "measurement placeholders", json!({"name": "S", "DIMPOST": marks(8)}), json!({"name": "S", "post": marks(9)})),
            ("dimstyle.dimension", "measurement placeholders", json!({"name": "S", "dimapost": marks(8)}), json!({"name": "S", "altPost": marks(9)})),
            ("dimstyle.override", "measurement placeholders", json!({"DIMPOST": marks(8)}), json!({"DIMPOST": marks(9)})),
            ("dimoverride", "measurement placeholders", json!({"text": marks(8)}), json!({"text": marks(9)})),
            ("dimlinear", "measurement placeholders", json!({"p1": [0, 0], "p2": [1, 0], "at": [0, 1], "text": marks(8)}), json!({"p1": [0, 0], "p2": [1, 0], "at": [0, 1], "text": marks(9)})),
            ("dimaligned", "measurement placeholders", json!({"p1": [0, 0], "p2": [1, 0], "at": [0, 1], "text": marks(8)}), json!({"p1": [0, 0], "p2": [1, 0], "at": [0, 1], "text": marks(9)})),
            ("textedit", "measurement placeholders", json!({"handle": "1F", "text": marks(8)}), json!({"handle": "1F", "text": marks(9)})),
            ("properties.set", "measurement placeholders", json!({"textOverride": marks(8)}), json!({"textOverride": marks(9)})),
        ];
        let door = door().unwrap();
        for (id, what, at, over) in &rows {
            let one = |p: &Json| door.admit_all(&json!([{"id": id, "params": p}]), &area);
            assert!(one(at).is_ok(), "{id} at its cap: {:?}", one(at));
            let e = one(over).unwrap_err();
            assert!(e.starts_with(&format!("cad.run: `{id}`")) && e.contains(what) && e.contains("the door allows in one command"), "{id} past its cap: {e}");
        }
        // The messages, word for word, for a product and a reviewer's measure.
        assert_eq!(
            door.admit_all(&json!([{"id": "copy", "params": {"count": 20_000, "delta": [1, 0]}}]), &area).unwrap_err(),
            "cad.run: `copy`: `count` is 20000, more than the 10000 copies the door allows in one command"
        );
        assert_eq!(
            door.admit_all(&json!([{"id": "arrayrect", "params": {"rows": 1000, "cols": 1000}}]), &area).unwrap_err(),
            "cad.run: `arrayrect` asks for 1000000 copies (`rows` × `cols`, 3 × 4 when absent), more than the 10000 the door allows in one command"
        );
        // A count the gate cannot weigh is refused; `columns` and `levels`
        // are not this engine's, so they bound nothing.
        assert!(door.admit_all(&json!([{"id": "arrayrect", "params": {"rows": [5]}}]), &area).unwrap_err().contains("`rows` is a number the door bounds"));
        assert!(door.admit_all(&json!([{"id": "arrayrect", "params": {"rows": 10, "columns": 1e9, "levels": 1e9}}]), &area).is_ok());
        // Every limit of the door is in the table above.
        for l in REVIEWED.limits {
            assert!(rows.iter().any(|(id, what, ..)| *id == l.id && l.what.contains(what)), "untested limit {} ({})", l.id, l.what);
        }
    }

    /// Two arrays of a million copies each: the first is refused on its own,
    /// before anything runs, and nothing is written.
    #[test]
    fn the_door_refuses_two_chained_arrays_of_a_million_copies() {
        let dir = tempfile::tempdir().unwrap();
        let array = json!({"id": "arrayrect", "params": {"rows": 1000, "cols": 1000, "rowSpacing": 1, "colSpacing": 1}});
        let cmds = json!([{"id": "line", "params": {"points": [[0, 0], [1, 1]]}}, {"id": "selectall"}, array, {"id": "selectall"}, array]);
        let e = run_in(dir.path(), json!({"cmds": cmds, "out": "grid.dxf"})).unwrap_err();
        assert!(e.contains("`arrayrect` asks for 1000000 copies") && e.contains("more than the 10000 the door allows in one command"), "{e}");
        assert!(!dir.path().join("grid.dxf").exists());
    }

    /// Two arrays at the cap each are refused together, as their copies
    /// multiply past the call's; two that multiply to the call's copies run.
    #[test]
    fn the_door_refuses_arrays_whose_copies_multiply_past_the_call() {
        let dir = tempfile::tempdir().unwrap();
        let array = |n: u32| json!({"id": "arrayrect", "params": {"rows": n, "cols": n, "rowSpacing": 2, "colSpacing": 2}});
        let start = json!({"id": "line", "params": {"points": [[0, 0], [1, 1]]}});
        let e = run_in(dir.path(), json!({"cmds": [start, {"id": "selectall"}, array(100), {"id": "selectall"}, array(100)], "out": "a.dxf"})).unwrap_err();
        assert!(e.contains("the copies this call makes multiply to 100000000, more than the 10000 the door allows in one call"), "{e}");
        assert!(!dir.path().join("a.dxf").exists());
        let made = run_in(dir.path(), json!({"cmds": [start, {"id": "selectall"}, array(10), {"id": "selectall"}, array(10)], "out": "b.dxf"})).unwrap();
        assert_eq!(made["results"][4]["result"]["created"], json!(9_900), "{}", made["results"]);
        let info = run_in(dir.path(), json!({"path": "b.dxf", "cmds": [{"id": "drawing.inspect", "params": {"entities": false}}]})).unwrap();
        assert_eq!(info["results"][0]["result"]["entityCount"], json!(10_000));
    }

    /// The service refuses a command that would take the call's drawings
    /// past their object ceiling before it runs: an array at the gate's cap
    /// of a drawing near the ceiling, a doubling `mirror` of everything, a
    /// paste that a copy repeats.
    #[test]
    fn the_service_refuses_growth_past_the_entity_ceiling() {
        let dir = tempfile::tempdir().unwrap();
        let big = [json!({"id": "line", "params": {"points": zigzag(100_000)}}), json!({"id": "line", "params": {"points": zigzag(99_998)}})];
        // 199,996 lines: four more fit (a line of five points), five do not.
        let mut cmds = big.to_vec();
        cmds.push(json!({"id": "line", "params": {"points": zigzag(5)}}));
        let near = run_in(dir.path(), json!({"cmds": cmds})).unwrap();
        assert_eq!(near["results"].as_array().unwrap().len(), 3);
        let mut cmds = big.to_vec();
        cmds.extend([json!({"id": "selectall"}), json!({"id": "arrayrect", "params": {"rows": 100, "cols": 100, "rowSpacing": 1, "colSpacing": 1}})]);
        let e = run_in(dir.path(), json!({"cmds": cmds})).unwrap_err();
        assert!(e.contains("`arrayrect` would bring the call's drawings to") && e.contains("objects, more than the 200000 a call may reach"), "{e}");
        let mut cmds = big.to_vec();
        cmds.push(json!({"id": "line", "params": {"points": zigzag(6)}}));
        let e = run_in(dir.path(), json!({"cmds": cmds})).unwrap_err();
        assert!(e.contains("`line` would bring the call's drawings to 200001 objects"), "{e}");
        // Growth by repetition with no count.
        let mirror = [json!({"id": "selectall"}), json!({"id": "mirror", "params": {"p1": [0, -1], "p2": [1, -1]}})];
        let mut cmds = vec![json!({"id": "line", "params": {"points": zigzag(25_001)}})];
        for _ in 0..4 {
            cmds.extend(mirror.iter().cloned());
        }
        let e = run_in(dir.path(), json!({"cmds": cmds})).unwrap_err();
        assert!(e.contains("`mirror` would bring the call's drawings to 400000 objects"), "{e}");
        let mut cmds = vec![json!({"id": "line", "params": {"points": zigzag(25_001)}}), json!({"id": "selectall"}), json!({"id": "copyclip"})];
        cmds.extend((0..8).map(|_| json!({"id": "pasteclip"})));
        let e = run_in(dir.path(), json!({"cmds": cmds})).unwrap_err();
        assert!(e.contains("`pasteclip` would bring the call's drawings to 225000 objects"), "{e}");
    }

    /// A hatch pattern or a linetype fine for its objects is refused before
    /// anything is drawn, as PNG, SVG or PDF, by `run`, `render` and
    /// `convert` alike; the drawing itself (a hatch definition) still writes
    /// as DXF.
    #[test]
    fn the_service_refuses_to_draw_a_hatch_or_linetype_too_fine_for_its_objects() {
        let dir = tempfile::tempdir().unwrap();
        // ANSI33 at 0.05 on a 100-unit square: 85 million dashes, 47 s as PNG.
        let hatch = json!([
            {"id": "rectang", "params": {"p1": [0, 0], "p2": [100, 100]}},
            {"id": "selectall"},
            {"id": "hatch", "params": {"pattern": "ANSI33", "scale": 0.05}}
        ]);
        for out in ["h.png", "h.svg", "h.pdf"] {
            let started = std::time::Instant::now();
            let e = run_in(dir.path(), json!({"cmds": hatch, "out": out})).unwrap_err();
            assert!(e.contains("drawing this as") && e.contains("display points, more than the 3000000 the service draws at once"), "{out}: {e}");
            assert!(started.elapsed().as_secs_f64() < 5.0, "{out}: refused without drawing");
            assert!(!dir.path().join(out).exists());
        }
        run_in(dir.path(), json!({"cmds": hatch, "out": "h.dxf"})).unwrap();
        let areas = resolver(dir.path(), None);
        for (method, out) in [("render", "r.png"), ("render", "r.svg"), ("convert", "r.pdf")] {
            let e = serve(&areas, &service_call(method, json!({"path": "h.dxf", "out": out}), dir.path(), false)).unwrap_err();
            assert!(e.starts_with(&format!("cad.{method}: drawing this as")), "{method} {out}: {e}");
        }
        // A coarser scale on the same square draws.
        let fine = json!([{"id": "rectang", "params": {"p1": [0, 0], "p2": [100, 100]}}, {"id": "selectall"}, {"id": "hatch", "params": {"pattern": "ANSI33", "scale": 4}}]);
        run_in(dir.path(), json!({"cmds": fine, "out": "ok.png", "max_side": 64})).unwrap();
        // Dashes: 1,999 lines of 1,000 units, DASHED at 0.03, by the object's
        // linetype scale or the drawing's LTSCALE.
        for scale in [json!({"id": "properties.set", "params": {"linetype": "DASHED", "ltscale": 0.03}}), json!({"id": "ltscale", "params": {"scale": 0.03}})] {
            let mut cmds = vec![json!({"id": "linetype", "params": {"load": "DASHED"}}), json!({"id": "line", "params": {"points": zigzag(2000)}}), json!({"id": "selectall"})];
            cmds.push(json!({"id": "properties.set", "params": {"linetype": "DASHED"}}));
            cmds.push(scale);
            let e = run_in(dir.path(), json!({"cmds": cmds, "out": "l.png"})).unwrap_err();
            assert!(e.contains("drawing this as PNG would take"), "{e}");
        }
    }

    /// A drawing whose blocks multiply one pass over its extents (blocks of
    /// blocks ten deep, written by the engine itself) is refused before it
    /// opens, by every method; a call that makes a block hold itself is
    /// refused after that command.
    #[test]
    fn the_service_refuses_a_drawing_whose_blocks_multiply_its_extents() {
        use cadcraft_engine::doc::{Block, Entity, Handle, Insert};
        let dir = tempfile::tempdir().unwrap();
        let reference = |name: &str, x: f64| {
            EntityKind::Insert(Insert { block: name.into(), insert: Vec3::new(x, 0.0, 0.0), scale: Vec3::new(1.0, 1.0, 1.0), rotation: 0.0, attribs: vec![], cols: 1, rows: 1, col_spacing: 0.0, row_spacing: 0.0 })
        };
        let mut d = Drawing::new_imperial();
        let mut h = 0x1000;
        for level in 0..7 {
            let mut b = Block::new(&format!("B{level}"));
            for i in 0..10 {
                h += 1;
                let kind = if level == 6 { EntityKind::Line(doc::Line { a: Vec3::new(f64::from(i), 0.0, 0.0), b: Vec3::new(f64::from(i), 1.0, 0.0) }) } else { reference(&format!("B{}", level + 1), f64::from(i) * 20.0) };
                b.entities.push(Entity::new(Handle(h), kind));
            }
            d.blocks.insert(format!("B{level}"), std::sync::Arc::new(b));
        }
        d.handseed = h + 1;
        d.add(&Space::Model, Common::default(), reference("B0", 0.0)).unwrap();
        std::fs::write(dir.path().join("deep.dxf"), cadcraft_io::write(&d, "deep.dxf").unwrap()).unwrap();
        let areas = resolver(dir.path(), None);
        for (method, args) in [("info", json!({"path": "deep.dxf"})), ("render", json!({"path": "deep.dxf", "out": "d.png"})), ("run", json!({"path": "deep.dxf", "cmds": []}))] {
            let e = serve(&areas, &service_call(method, args, dir.path(), false)).unwrap_err();
            assert!(e.contains("one pass over this drawing would take") && e.contains("more than the 5000000 the service takes"), "{method}: {e}");
        }
        // A block redefined to hold ten references to itself.
        let cmds = json!([
            {"id": "line", "params": {"points": [[0, 0], [1, 0]]}},
            {"id": "selectall"},
            {"id": "block", "params": {"name": "LOOP", "base": [0, 0]}},
            {"id": "selectall"},
            {"id": "arrayrect", "params": {"rows": 1, "cols": 10, "colSpacing": 2}},
            {"id": "selectall"},
            {"id": "block", "params": {"name": "LOOP", "base": [0, 0]}},
            {"id": "entities"}
        ]);
        let e = run_in(dir.path(), json!({"cmds": cmds, "out": "loop.dxf"})).unwrap_err();
        assert!(e.contains("after `block` the call's drawings hold") && e.contains("steps for one pass over their extents"), "{e}");
        assert!(!dir.path().join("loop.dxf").exists());
    }

    /// What a hostile drawing plants without a parameter is weighed where it
    /// would be paid: a block reference repeated ten thousand by ten thousand
    /// times, and dimension text templates whose `<>` multiply, are refused
    /// before they are drawn, written, exploded or hit-tested.
    #[test]
    fn the_service_weighs_what_a_hostile_drawing_would_draw_and_write() {
        use cadcraft_engine::doc::{Block, DimKind, Dimension, Entity, Handle, Insert};
        let v = |x: f64, y: f64| Vec3::new(x, y, 0.0);
        // A block of 100 lines, referenced once as a 10,000 × 10,000 grid.
        let mut d = Drawing::new_imperial();
        let mut b = Block::new("B");
        for i in 0..100u64 {
            b.entities.push(Entity::new(Handle(0x1000 + i), EntityKind::Line(doc::Line { a: v(0.0, i as f64 * 0.01), b: v(1.0, i as f64 * 0.01) })));
        }
        d.blocks.insert("B".into(), std::sync::Arc::new(b));
        d.handseed = 0x2000;
        let grid = d
            .add(&Space::Model, Common::default(), EntityKind::Insert(Insert { block: "B".into(), insert: v(0.0, 0.0), scale: v(1.0, 1.0), rotation: 0.0, attribs: vec![], cols: 10_000, rows: 10_000, col_spacing: 2.0, row_spacing: 2.0 }))
            .unwrap();
        let mut s = Session::empty();
        s.open_drawing(d, "grid", None);
        let dr = s.doc().unwrap();
        assert!(caps::drawable("cad.run", dr, "png").unwrap_err().contains("drawing this as PNG"));
        assert!(caps::writable("cad.run", dr).is_ok(), "a block reference writes as one");
        let crossing = json!({"window": [[-1, -1], [5, 5]], "crossing": true});
        assert!(caps::work("cad.run", &s, "select", &crossing).unwrap_err().contains("would draw"), "a crossing window draws the reference to test it");
        assert!(caps::work("cad.run", &s, "select", &json!({"window": [[-1, -1], [5, 5]]})).is_ok(), "a plain window draws nothing");
        assert!(caps::Watch::new(&s).before(&s, "explode", &json!({"handles": [grid.hex()]})).is_ok(), "exploding it makes one copy of the block");
        // DIMPOST with 1,000 `<>`, and a dimension whose own text holds 1,000.
        let mut d = Drawing::new_imperial();
        d.dim_styles[0].post = "<>".repeat(1000);
        let dim = Dimension {
            kind: DimKind::Linear { rotation: 0.0 },
            defpt: v(0.0, 1.0),
            text_mid: v(0.0, 0.0),
            p13: v(0.0, 0.0),
            p14: v(10.0, 0.0),
            p15: v(0.0, 0.0),
            p16: v(0.0, 0.0),
            text: "<>".repeat(1000),
            style: "Standard".into(),
            measurement: 0.0,
            text_rotation: 0.0,
            user_text_pos: false,
            block: None,
            overrides: Default::default(),
            assoc: vec![],
        };
        let h = d.add(&Space::Model, Common::default(), EntityKind::Dimension(dim)).unwrap();
        let mut s = Session::empty();
        s.open_drawing(d, "dims", None);
        let dr = s.doc().unwrap();
        assert!(caps::drawable("cad.run", dr, "svg").unwrap_err().contains("drawing this as SVG"));
        assert!(caps::writable("cad.run", dr).unwrap_err().contains("writing this drawing would lay out"));
        assert!(caps::Watch::new(&s).before(&s, "explode", &json!({"handles": [h.hex()]})).unwrap_err().contains("`explode` would bring the call's drawings to a weight of"));
        let crossing = json!({"window": [[-1, -1], [5, 5]], "crossing": true});
        assert!(caps::work("cad.run", &s, "select", &crossing).unwrap_err().contains("would draw"));
    }

    /// The commands whose work scales with the drawing rather than with a
    /// parameter are weighed before they run; a repeated handle and a reply
    /// past what a call returns are refused, and only one undo step is kept.
    #[test]
    fn the_service_bounds_work_that_scales_with_the_drawing() {
        let dir = tempfile::tempdir().unwrap();
        let lines = json!({"id": "line", "params": {"points": zigzag(25_001)}});
        let refused = |more: Vec<Json>| {
            let mut cmds = vec![lines.clone()];
            cmds.extend(more);
            run_in(dir.path(), json!({"cmds": cmds})).unwrap_err()
        };
        assert!(refused(vec![json!({"id": "overkill"})]).contains("`overkill` would compare objects by their weight"));
        assert!(refused(vec![json!({"id": "selectall"}), json!({"id": "join"})]).contains("`join` would chain 25000 pieces, more than the 20000"));
        let ellipse = json!({"id": "ellipse", "params": {"center": [500, 100], "major": [100, 0], "ratio": 0.5}});
        let e = refused(vec![ellipse.clone(), json!({"id": "select", "params": {"window": [[350, 40], [650, 160]]}}), json!({"id": "trim", "params": {"pick": [600, 100]}})]);
        assert!(e.contains("`trim` would test") && e.contains("segment intersections") && e.contains("(name its cutting `edges`)"), "{e}");
        // Named edges keep it small: the engine itself then answers.
        let named = run_in(dir.path(), json!({"cmds": [ellipse, {"id": "line", "params": {"points": [[0, 0], [1, 1]]}}, {"id": "trim", "params": {"handle": "100", "edges": ["101"], "pick": [600, 100]}}]})).unwrap_err();
        assert!(!named.contains("the door allows"), "{named}");
        let first: Vec<Json> = (0x100..0x164).map(|h: u64| json!(format!("{h:X}"))).collect();
        assert!(refused(vec![json!({"id": "qdim", "params": {"handles": first, "at": [0, -10]}})]).contains("`qdim` would test"));
        let e = run_in(dir.path(), json!({"cmds": [{"id": "erase", "params": {"handles": ["1F", "20", "1F"]}}]})).unwrap_err();
        assert_eq!(e, "cad.run: `erase`: `handles` names the object 1F more than once; name each object once");
        // Whole objects in an answer: 2,000 polylines of 100 vertices each.
        let poly: Vec<Json> = (0..100).map(|i| json!([i, i % 2])).collect();
        let cmds = json!([{"id": "pline", "params": {"vertices": poly}}, {"id": "selectall"}, {"id": "copy", "params": {"delta": [0, 2], "count": 1999}}, {"id": "entities", "params": {"limit": 2000}}]);
        let e = run_in(dir.path(), json!({"cmds": cmds})).unwrap_err();
        assert!(e.contains("`entities` would copy objects of weight") && e.contains("(lower `limit`)"), "{e}");
        // Replies together: a dozen answers of 2,000 lines each.
        let mut cmds = vec![lines.clone()];
        cmds.extend((0..12).map(|_| json!({"id": "entities", "params": {"limit": 2000}})));
        let e = run_in(dir.path(), json!({"cmds": cmds})).unwrap_err();
        assert!(e.contains("the call's answers hold") && e.contains("more than the 4000000 a call returns"), "{e}");
        assert!(refused(vec![json!({"id": "qselect.info"})]).contains("`qselect.info` would serialise the properties of"));
        // A polyline of 7,000 vertices: its grips take vertices² steps; a
        // path of 10,000 segments walked 32,767 times; a spline refitted
        // through more fit points than the door fits; bulges of near-full
        // circles tessellated finely.
        let long: Vec<Json> = (0..7000).map(|i| json!([i, i % 2])).collect();
        let e = run_in(dir.path(), json!({"cmds": [{"id": "pline", "params": {"vertices": long}}, {"id": "grip.stretch", "params": {"handle": "100", "index": 0, "to": [-1, -1]}}]})).unwrap_err();
        assert!(e.contains("`grip.stretch` would take 49000000 steps to list a polyline's grips"), "{e}");
        let path: Vec<Json> = (0..10_001).map(|i| json!([i, i % 2])).collect();
        let e = run_in(dir.path(), json!({"cmds": [{"id": "pline", "params": {"vertices": path}}, {"id": "divide", "params": {"handle": "100", "segments": 32_767}}]})).unwrap_err();
        assert!(e.contains("`divide` would take") && e.contains("steps along its path"), "{e}");
        let fit: Vec<Json> = (0..2000).map(|i| json!([i, (i * 7) % 13])).collect();
        let fitted = json!({"id": "spline", "params": {"fit": fit}});
        run_in(dir.path(), json!({"cmds": [fitted, {"id": "splinedit", "params": {"handle": "100", "option": "move", "index": 0, "to": [0, 1]}}]})).unwrap();
        let e = run_in(dir.path(), json!({"cmds": [fitted, {"id": "splinedit", "params": {"handle": "100", "option": "close"}}]})).unwrap_err();
        assert!(e.contains("`splinedit` would refit a spline through 2001 fit points, more than the 2000"), "{e}");
        let bulged: Vec<Json> = (0..2000).map(|i| json!({"p": [i * 10, 0], "bulge": 1e6})).collect();
        let e = run_in(dir.path(), json!({"cmds": [{"id": "pline", "params": {"vertices": bulged, "closed": true}}, {"id": "area", "params": {"handle": "100"}}]})).unwrap_err();
        assert!(e.contains("`area` would tessellate") && e.contains("more than the 4000000"), "{e}");
        // One undo step: the second undo finds nothing to undo.
        let line = |y: i32| json!({"id": "line", "params": {"points": [[0, y], [1, y]]}});
        let done = run_in(dir.path(), json!({"cmds": [line(0), line(1), line(2), {"id": "undo"}, {"id": "undo"}, {"id": "entities"}]})).unwrap();
        assert_eq!(done["results"][4]["result"]["message"], json!("Everything has been undone"), "{}", done["results"]);
        assert_eq!(done["results"][5]["result"]["count"], json!(2));
    }

    /// What the gate cannot see, the service measures before the command
    /// runs: an `arraypath` spaced along a long path, a `measure` into block
    /// references, an `explode` of references to a large block.
    #[test]
    fn the_service_measures_the_copies_the_gate_cannot_see() {
        let dir = tempfile::tempdir().unwrap();
        let path_and_dot = json!([{"id": "line", "params": {"points": [[0, 0], [1000, 0]]}}, {"id": "circle", "params": {"center": [0, 5], "radius": 1}}]);
        let along = |spacing: f64| {
            let mut cmds = path_and_dot.as_array().unwrap().clone();
            cmds.push(json!({"id": "arraypath", "params": {"handles": ["101"], "path": "100", "spacing": spacing}}));
            run_in(dir.path(), json!({"cmds": cmds}))
        };
        let e = along(0.01).unwrap_err();
        assert!(e.contains("`arraypath` would place 20000 copies along its path, more than the 10000 the door allows in one command"), "{e}");
        assert_eq!(along(1.0).unwrap()["results"][2]["result"]["handles"].as_array().map(Vec::len), Some(1000));
        let into_blocks = |length: f64| {
            let mut cmds = path_and_dot.as_array().unwrap().clone();
            cmds.push(json!({"id": "block", "params": {"name": "DOT", "base": [0, 5], "handles": ["101"]}}));
            cmds.push(json!({"id": "measure", "params": {"handle": "100", "length": length, "block": "DOT"}}));
            run_in(dir.path(), json!({"cmds": cmds}))
        };
        assert!(into_blocks(0.06).unwrap_err().contains("`measure` would place 16667 copies along its path"));
        assert_eq!(into_blocks(1.0).unwrap()["results"][3]["result"]["handles"].as_array().map(Vec::len), Some(999));
        // Ten references to a block of 25,000 lines explode into 250,000.
        let cmds = json!([
            {"id": "line", "params": {"points": zigzag(25_001)}},
            {"id": "selectall"},
            {"id": "block", "params": {"name": "BIG", "base": [0, 0]}},
            {"id": "selectall"},
            {"id": "arrayrect", "params": {"rows": 1, "cols": 10, "colSpacing": 2000}},
            {"id": "selectall"},
            {"id": "explode"}
        ]);
        let e = run_in(dir.path(), json!({"cmds": cmds})).unwrap_err();
        assert!(e.contains("`explode` would bring the call's drawings to 250010 objects"), "{e}");
    }

    /// The service's drawing estimate never counts less than the renderer
    /// draws (points, triangle corners and primitives of
    /// `cadcraft_render::build`), and stays within a small factor of it, for
    /// every kind of object the door can make: a pin bump that draws more
    /// fails here.
    #[test]
    fn the_drawing_estimate_bounds_the_renderer() {
        let fresh = || {
            let mut s = Session::empty();
            s.new_drawing(false);
            s.execute("linetype", &json!({"load": "*"})).unwrap();
            s
        };
        let run = |s: &mut Session, cmds: Json| {
            for c in cmds.as_array().unwrap() {
                s.execute(c["id"].as_str().unwrap(), &c["params"]).unwrap();
            }
        };
        let cases: Vec<(&str, Json)> = vec![
            ("lines", json!([{"id": "line", "params": {"points": zigzag(200)}}])),
            ("circles and arcs", json!([{"id": "circle", "params": {"center": [0, 0], "radius": 0.01}}, {"id": "circle", "params": {"center": [0, 0], "radius": 50}}, {"id": "circle", "params": {"center": [0, 0], "radius": 1e5}}, {"id": "arc", "params": {"center": [0, 0], "radius": 10, "start": 0, "end": 135}}])),
            ("ellipse, spline, polylines", json!([{"id": "ellipse", "params": {"center": [0, 0], "major": [10, 0], "ratio": 0.4}}, {"id": "spline", "params": {"fit": [[0, 0], [5, 8], [10, -3], [15, 6]]}}, {"id": "pline", "params": {"vertices": [[0, 0], {"p": [10, 0], "bulge": 0.5}, [10, 10]], "width": 0.2}}, {"id": "revcloud", "params": {"p1": [0, 0], "p2": [40, 30]}}])),
            ("text, mtext, dimensions, table", json!([{"id": "text", "params": {"at": [0, 0], "text": "The quick brown fox jumps over the lazy dog"}}, {"id": "mtext", "params": {"at": [0, 5], "text": "one\ntwo three", "width": 10}}, {"id": "dimlinear", "params": {"p1": [0, 0], "p2": [12.3456, 0], "at": [5, 2]}}, {"id": "dimradius", "params": {"center": [0, 0], "point": [3, 4]}}, {"id": "table", "params": {"at": [0, -10], "rows": 5, "cols": 3, "cells": [["a", "b"], ["c"]]}}])),
            ("hatches", json!([{"id": "rectang", "params": {"p1": [0, 0], "p2": [100, 60]}}, {"id": "selectall"}, {"id": "hatch", "params": {"pattern": "ANSI31"}}, {"id": "circle", "params": {"center": [150, 30], "radius": 30}}, {"id": "select", "params": {"handles": ["102"]}}, {"id": "hatch", "params": {"pattern": "ANSI33", "scale": 0.5}}, {"id": "rectang", "params": {"p1": [200, 0], "p2": [260, 60]}}, {"id": "select", "params": {"handles": ["104"]}}, {"id": "hatch", "params": {"pattern": "SOLID"}}])),
            ("linetypes", json!([{"id": "line", "params": {"points": zigzag(50)}}, {"id": "circle", "params": {"center": [0, 0], "radius": 50}}, {"id": "selectall"}, {"id": "properties.set", "params": {"linetype": "BORDER", "ltscale": 0.1}}])),
            ("block references", json!([{"id": "circle", "params": {"center": [0, 0], "radius": 1}}, {"id": "line", "params": {"points": [[-1, -1], [1, 1]]}}, {"id": "selectall"}, {"id": "block", "params": {"name": "B", "base": [0, 0]}}, {"id": "selectall"}, {"id": "arrayrect", "params": {"rows": 5, "cols": 5, "rowSpacing": 3, "colSpacing": 3}}, {"id": "selectall"}, {"id": "block", "params": {"name": "C", "base": [0, 0]}}, {"id": "selectall"}, {"id": "arraypolar", "params": {"center": [-50, 0], "count": 4}}, {"id": "insert", "params": {"name": "B", "at": [100, 100], "scale": 1000}}])),
        ];
        for (label, cmds) in cases {
            let mut s = fresh();
            run(&mut s, cmds);
            let d = s.doc().unwrap();
            let list = cadcraft_engine::render::build(d, &Space::Model, &Default::default());
            let drawn = (list.verts.len() + list.tris.len() + list.prims.len()) as f64;
            let est = caps::drawing_cost(d, "png");
            assert!(est >= drawn && est <= drawn * 12.0 + 100.0, "{label}: estimate {est}, drawn {drawn}");
        }
    }
}
