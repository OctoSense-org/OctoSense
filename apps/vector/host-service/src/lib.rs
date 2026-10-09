//! `octosense-vector-service` — the `vector` host service (ADR 0013).
//!
//! vectorcraft's engine through its own single command entry point
//! ([`Session::execute`] — the same door its UI, CLI and automation server
//! go through). Every call is a fresh, stateless session. Unlike
//! photocraft, the engine does not contain file paths itself, so the
//! service does: every path is resolved inside the call's area before the
//! engine sees it, and `run` refuses the engine's own file commands.
//!
//! **Where a call works** (ADR 0013, 2026-10-08): the caller's own folder,
//! the [`Area`] the shell's resolver gives it ([`set_area_resolver`]), or
//! without one the legacy `<host dir>/vector`. A document's linked images
//! are the engine's to read when it opens the document — an SVG's
//! `<image href>`, a native document's links (also inside an SVG, PDF or EPS
//! saved with its editing data) — from wherever they point, so before it
//! opens one the service finds every file it would read and refuses the
//! document unless each resolves inside the area ([`fence_links`]). Writes
//! keep the area's rules (a write that may not replace, an agent's, only
//! creates new files, within the quota): a preview through [`Area::write`],
//! an export, which the engine writes itself (with any numbered siblings),
//! into a staging folder inside the area first
//! ([`octosense_engine_area::Stage`]). The generic command door `run` runs
//! through the shared gate ([`door`], ADR 0013 #418): an allowlist built
//! from the reviewed classification `skill/safety.json`. Only `safe` ids
//! run; `code` (the wrappers `command.batch`, the plug-in system, and
//! `prefs.set`, which can load a plug-ins folder), `file`, `host` and
//! unknown ids are refused. Two reviewed inner ids are checked by the gate:
//! `perspective.draw`'s named `shape.*` command passes the gate itself, and
//! `effect.apply` / `appearance.addEffect` admit only an effect the engine
//! builds in, never an effect plug-in `plugin.<id>`. After every command
//! the service re-fences the live document's image links, so a command
//! cannot plant a link the export would then read from outside the area.
//! Engine work runs on the shell's UI thread, so the door caps work too:
//! the gate refuses a parameter past its reviewed limit ([`LIMITS`]), the
//! service weighs what a command would make before it runs and the whole
//! document after ([`Weight`], [`pre_check`]), and what the export would
//! render before it renders ([`check_export`]).
//!
//! Methods (all under the `vector` family; paths relative to the area):
//! - `info {path, depth?}` → the document inspected as JSON (artboards,
//!   layer tree, object count, colour mode, units, import warnings)
//! - `convert {path, out, format?, scale?}` → `{out, format, bytes,
//!   warnings}` — export in the format the extension (or `format`) picks:
//!   SVG/SVGZ, PDF, EPS, DXF, EMF/WMF, PNG/JPG/WebP/GIF/TIFF/BMP/TGA/PSD,
//!   native `.vectorcraft`
//! - `run {path?, cmds: [{id, params?}], out?, format?, scale?}` → `{results,
//!   out: "<rel>" | null, format, bytes, warnings, files?}` — the command
//!   door: every id admitted by [`door`] before the engine runs any (a
//!   refused id refuses the whole call, nothing written), run in order on
//!   the document at `path` or a fresh one, then exported to `out`
//! - `commands {}` → the ids the door runs, from the engine's catalog
//! - `render {path, out, max_side?, artboard?}` → a PNG written to `out`
//!
//! The service serves system apps only until ADR 0013's store capability
//! is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::collections::{BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::door::{Door, Inner, InnerRule, Limit, Measure, Reviewed};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};
use vectorcraft_engine::doc::color::Paint;
use vectorcraft_engine::doc::{AppearanceItem, Document, Effect, Node, NodeKind, RepeatKind};
use vectorcraft_engine::Session;
use vectorcraft_render::vello_cpu::kurbo::{Rect, Shape};

/// The longest preview edge `render` produces.
const MAX_RENDER_SIDE: u32 = 4096;
/// The largest file the service opens (bytes).
const MAX_OPEN_BYTES: u64 = 64 << 20;

/// What the vector engine's reviewer settled for the door beyond the classes
/// (ADR 0013, #418). No reviewed file read (a `file` command reaches paths
/// the service's own `path`/`out` do not cover, so none runs through the
/// door) and no setter (no `safe` vector command sets an app-wide preference
/// by key; every preference write is `host`, and `prefs.set` is `code`).
///
/// Inner ids:
/// - `perspective.draw {command}` runs a named `shape.*` command, which
///   passes the gate itself with its own `params`.
/// - `effect.apply` / `appearance.addEffect` append a live effect by id; the
///   gate admits only an effect the engine builds in ([`builtin_effect`]),
///   never an effect plug-in `plugin.<id>`. The engine's `apply` reads the
///   id from `effect` or, as an alias, `id`, so both keys are gated (the
///   door skips a key a call omits); a call that names a plug-in under
///   either is refused.
///
/// Limits ([`LIMITS`]): every parameter that multiplies the engine's work or
/// memory, reviewed from vectorcraft's implementation at the pinned revision,
/// with a ceiling well above everyday illustration and far below what would
/// stall the shell's UI thread (about a second of engine work and 256 MB per
/// command on a laptop). What a call adds to the document in total, and what
/// its export renders, the service weighs itself ([`Weight`], [`check_export`]).
static REVIEWED: Reviewed = Reviewed {
    file_reads: &[],
    setters: &[],
    inner: &[
        Inner { id: "perspective.draw", param: "command", rule: InnerRule::Command { prefix: "shape.", params: "params" } },
        Inner { id: "effect.apply", param: "effect", rule: InnerRule::Effect { builtin: builtin_effect } },
        Inner { id: "effect.apply", param: "id", rule: InnerRule::Effect { builtin: builtin_effect } },
        Inner { id: "appearance.addEffect", param: "effect", rule: InnerRule::Effect { builtin: builtin_effect } },
        Inner { id: "appearance.addEffect", param: "id", rule: InnerRule::Effect { builtin: builtin_effect } },
    ],
    limits: LIMITS,
    copies_per_call: COPIES_PER_CALL,
};

/// The most the copy limits of one call may multiply to: two arrays of a
/// hundred (a 10 × 10 grid of a 10 × 10 grid), a blend of a hundred steps of
/// a hundred-copy Transform. Live copies (repeats, blends, Transform effects)
/// cost nothing when made and everything on every draw and export.
const COPIES_PER_CALL: f64 = 10_000.0;

/// The longest side, in points, of an artboard: the engine's own `file.new`
/// ceiling (Illustrator's large canvas, 16383 pt × 10).
const MAX_ARTBOARD_SIDE: f64 = 163_830.0;

/// The door's reviewed ceilings on what one command may ask for (ADR 0013,
/// #418). Each names what it multiplies, the engine's own bound, and why the
/// cap sits where it does. A `copies` limit multiplies what the document
/// already holds; those amounts multiply together within [`COPIES_PER_CALL`].
static LIMITS: &[Limit] = &[
    // ---- Shapes (also reached through `perspective.draw`'s inner command) ----
    // A star's points, 2 anchors each (default 5; the engine clamps to 1000).
    Limit { id: "shape.star", what: "points", measure: Measure::Product(&["points"]), max: 1000.0, copies: false },
    // A polygon's sides, one anchor each (default 6; the engine clamps to 1000).
    Limit { id: "shape.polygon", what: "sides", measure: Measure::Product(&["sides"]), max: 1000.0, copies: false },
    // A spiral's segments, one anchor each (default 10; the engine clamps to 1000).
    Limit { id: "shape.spiral", what: "segments", measure: Measure::Product(&["segments"]), max: 1000.0, copies: false },
    // A rectangular grid's rows, one path each (default 5; the engine clamps to 999).
    Limit { id: "shape.rectangularGrid", what: "rows", measure: Measure::Product(&["rows"]), max: 999.0, copies: false },
    // A rectangular grid's columns, one path each (default 5; the engine clamps to 999).
    Limit { id: "shape.rectangularGrid", what: "columns", measure: Measure::Product(&["columns"]), max: 999.0, copies: false },
    // A polar grid's concentric ellipses (default 5; the engine clamps to 999).
    Limit { id: "shape.polarGrid", what: "concentric dividers", measure: Measure::Product(&["concentric"]), max: 999.0, copies: false },
    // A polar grid's radial lines (default 5; the engine clamps to 999).
    Limit { id: "shape.polarGrid", what: "radial dividers", measure: Measure::Product(&["radial"]), max: 999.0, copies: false },
    // A flare's rays, subpaths of one path (default 15; the engine clamps to 50).
    Limit { id: "shape.flare", what: "rays", measure: Measure::Product(&["rays"]), max: 50.0, copies: false },
    // A flare's rings, one path each (default 10; the engine clamps to 50).
    Limit { id: "shape.flare", what: "rings", measure: Measure::Product(&["rings"]), max: 50.0, copies: false },
    // ---- Paths ----
    // The largest number in SVG path data: an arc of radius r becomes about
    // (r / 0.1)^(1/6) cubics before the canvas check sees it (r = 1e70 is
    // 5e11 of them). Capped at the canvas the engine keeps art on, ±4e6 pt.
    Limit { id: "path.create", what: "points from the origin (a coordinate or radius of `d`)", measure: Measure::Custom(path_data_extent), max: 4.0e6, copies: false },
    // Drag points: each is tested against every target segment, so a drag
    // costs points × segments (no engine cap on the list). A long drag is a
    // few hundred points.
    Limit { id: "path.knife", what: "points", measure: Measure::Custom(point_count), max: 1000.0, copies: false },
    // Drag points, each tested against every target segment (24 samples a segment).
    Limit { id: "path.eraseRegion", what: "points", measure: Measure::Custom(point_count), max: 1000.0, copies: false },
    // Drag points, each tested against every selected segment (24 samples a segment).
    Limit { id: "path.eraseSegments", what: "points", measure: Measure::Custom(point_count), max: 1000.0, copies: false },
    // Drag points, each tested against every selected anchor.
    Limit { id: "path.smoothRegion", what: "points", measure: Measure::Custom(point_count), max: 1000.0, copies: false },
    // Drag points, each tested against every open path's ends.
    Limit { id: "path.joinScrub", what: "points", measure: Measure::Custom(point_count), max: 1000.0, copies: false },
    // Blob brush points: the stroke's outline grows with them, then merges
    // with every touching blob.
    Limit { id: "path.blob", what: "points", measure: Measure::Custom(point_count), max: 1000.0, copies: false },
    // Pencil points: linear work (the fit never adds anchors); ten seconds
    // of a fast drag.
    Limit { id: "path.freehand", what: "points", measure: Measure::Custom(point_count), max: 10_000.0, copies: false },
    // Paintbrush points, as `path.freehand`.
    Limit { id: "brush.freehand", what: "points", measure: Measure::Custom(point_count), max: 10_000.0, copies: false },
    // Curvature tool points: one anchor each.
    Limit { id: "path.curvature", what: "points", measure: Measure::Custom(point_count), max: 10_000.0, copies: false },
    // Shape Builder drag points: each pair is resampled into up to 4,000 hit
    // tests over every region, then a boolean per shape. A drag over a few
    // regions is a handful of points.
    Limit { id: "shapeBuilder.merge", what: "points", measure: Measure::Custom(point_count), max: 256.0, copies: false },
    // ---- Strokes ----
    // Dash pattern entries: every stroked path stores its own copy (and the
    // new-art template hands it to up to 1,999 grid lines). Illustrator
    // offers 6; the engine has no cap on the list.
    Limit { id: "stroke.set", what: "dash entries", measure: Measure::Custom(dash_entries), max: 64.0, copies: false },
    // The character stroke's dash entries, as `stroke.set`.
    Limit { id: "text.setRangeStyle", what: "dash entries", measure: Measure::Custom(stroke_options_dash_entries), max: 64.0, copies: false },
    // Width profile points: every flattened vertex scans them, so a stroke
    // costs vertices × points (no engine cap on the list). The built-in
    // profiles have up to 9.
    Limit { id: "stroke.widthProfile.set", what: "width points", measure: Measure::Custom(point_count), max: 256.0, copies: false },
    // Width profile factors (1 = the stroke's weight; no engine maximum): a
    // dot of a dashed, round-capped stroke takes (r / tolerance)^(1/6)
    // segments, millions at 1e30.
    Limit { id: "stroke.widthProfile.set", what: "times the stroke weight", measure: Measure::Custom(profile_width), max: 1000.0, copies: false },
    // ---- Live objects that copy the selection ----
    // Blend steps between each pair of key objects: each step is a full
    // interpolated copy of the keys (the engine refuses more than 1000). With
    // `spacing: "distance"` or smooth colour the engine picks the count, at
    // most 1000 and 256 a pair; the document weight bounds those.
    Limit { id: "object.blend.make", what: "steps", measure: Measure::Custom(blend_steps), max: 1000.0, copies: true },
    // A grid repeat's copies of the selection, rows × cols (default 3 × 3;
    // the engine clamps each to 500, 250,000 copies).
    Limit { id: "object.repeat.grid", what: "copies", measure: Measure::Product(&["rows", "cols"]), max: 10_000.0, copies: true },
    // A radial repeat's copies of the selection (default 8; the engine
    // refuses more than 1000).
    Limit { id: "object.repeat.radial", what: "copies", measure: Measure::Product(&["instances"]), max: 1000.0, copies: true },
    // A repeat's radial copies set through its options (the engine refuses more than 1000).
    Limit { id: "object.repeat.options", what: "copies", measure: Measure::Product(&["instances"]), max: 1000.0, copies: true },
    // A repeat's grid copies set through its options, rows × cols (the engine clamps each to 500).
    Limit { id: "object.repeat.options", what: "copies", measure: Measure::Product(&["rows", "cols"]), max: 10_000.0, copies: true },
    // Object Mosaic tiles, one rectangle each (default 10 × 10; the engine
    // clamps each to 1000, a million rectangles).
    Limit { id: "object.createObjectMosaic", what: "tiles", measure: Measure::Product(&["columns", "rows"]), max: 10_000.0, copies: true },
    // Split Into Grid cells, a rectangle each for every selected path (the
    // engine refuses more than 500 a side, 250,000 per path).
    Limit { id: "object.path.splitIntoGrid", what: "cells", measure: Measure::Product(&["rows", "columns"]), max: 10_000.0, copies: true },
    // A Transform effect's copies, the original included: every draw and
    // export makes them (the engine clamps to 1000 copies).
    Limit { id: "effect.apply", what: "copies (the original included)", measure: Measure::Custom(transform_copies), max: 1001.0, copies: true },
    // As `effect.apply` (its alias).
    Limit { id: "appearance.addEffect", what: "copies (the original included)", measure: Measure::Custom(transform_copies), max: 1001.0, copies: true },
    // Copies merged into an existing effect: which effect `index` names is
    // known only once the command runs, so any effect's `copies` counts.
    Limit { id: "effect.setParams", what: "copies (the original included)", measure: Measure::Custom(any_effect_copies), max: 1001.0, copies: true },
    // ---- Live effects (`effect.apply`, `appearance.addEffect`, `effect.setParams`) ----
    // Zig Zag ridges per segment: anchors × (ridges + 1), stacked effects
    // multiplying (default 4; the engine clamps to 100).
    Limit { id: "effect.apply", what: "ridges", measure: Measure::Custom(zigzag_ridges), max: 100.0, copies: false },
    Limit { id: "appearance.addEffect", what: "ridges", measure: Measure::Custom(zigzag_ridges), max: 100.0, copies: false },
    Limit { id: "effect.setParams", what: "ridges", measure: Measure::Custom(any_effect_ridges), max: 100.0, copies: false },
    // Roughen detail, points per inch of path (up to 2000 a segment; default
    // 10; the engine clamps to 100).
    Limit { id: "effect.apply", what: "points per inch (detail)", measure: Measure::Custom(roughen_detail), max: 100.0, copies: false },
    Limit { id: "appearance.addEffect", what: "points per inch (detail)", measure: Measure::Custom(roughen_detail), max: 100.0, copies: false },
    Limit { id: "effect.setParams", what: "points per inch (detail)", measure: Measure::Custom(any_effect_detail), max: 100.0, copies: false },
    // The Offset Path effect's distance: its sweep grows with the
    // self-intersections a large offset makes of a detailed path (default
    // 10 pt; the engine allows 1e5).
    Limit { id: "effect.apply", what: "points of offset", measure: Measure::Custom(offset_effect), max: 1000.0, copies: false },
    Limit { id: "appearance.addEffect", what: "points of offset", measure: Measure::Custom(offset_effect), max: 1000.0, copies: false },
    Limit { id: "effect.setParams", what: "points of offset", measure: Measure::Custom(any_effect_offset), max: 1000.0, copies: false },
    // Shadow, glow, feather and Gaussian blur distance: a blur's buffer grows
    // by 1.5 × blur on each side, at the export's scale (default 5 pt; the
    // engine allows 1000). Illustrator's Gaussian Blur stops at 250. Through
    // `effect.setParams` this also bounds Round Corners' `radius`.
    Limit { id: "effect.apply", what: "points of blur", measure: Measure::Custom(raster_effect_blur), max: 250.0, copies: false },
    Limit { id: "appearance.addEffect", what: "points of blur", measure: Measure::Custom(raster_effect_blur), max: 250.0, copies: false },
    Limit { id: "effect.setParams", what: "points of blur", measure: Measure::Custom(any_effect_blur), max: 250.0, copies: false },
    // ---- Gradient meshes and envelopes ----
    // Gradient mesh patches, rows × cols (alias `columns`), each drawn as
    // up to 1,024 quads (default 4 × 4; the engine refuses more than 50 a side).
    Limit { id: "object.mesh.create", what: "patches", measure: Measure::Product(&["rows", "cols"]), max: 2500.0, copies: false },
    Limit { id: "object.mesh.create", what: "patches", measure: Measure::Product(&["rows", "columns"]), max: 2500.0, copies: false },
    // A mesh envelope's patches (the engine refuses more than 50 a side).
    Limit { id: "object.envelope.makeWithMesh", what: "patches", measure: Measure::Product(&["rows", "cols"]), max: 2500.0, copies: false },
    Limit { id: "object.envelope.makeWithMesh", what: "patches", measure: Measure::Product(&["rows", "columns"]), max: 2500.0, copies: false },
    Limit { id: "object.envelope.resetWithMesh", what: "patches", measure: Measure::Product(&["rows", "cols"]), max: 2500.0, copies: false },
    Limit { id: "object.envelope.resetWithMesh", what: "patches", measure: Measure::Product(&["rows", "columns"]), max: 2500.0, copies: false },
    // ---- Expanding, offsetting, distorting ----
    // Expand's gradient strips, one object per gradient fill per step
    // (default 255; the engine refuses more than 1000). The service weighs
    // the strips against the selection's gradients before it runs.
    Limit { id: "object.expand", what: "steps", measure: Measure::Product(&["steps"]), max: 1000.0, copies: false },
    // Offset Path's distance, as the effect's (the engine allows 1e5).
    Limit { id: "object.path.offsetPath", what: "points of offset", measure: Measure::Custom(offset_length), max: 1000.0, copies: false },
    // Liquify dabs: one every tenth of the brush (at least 0.5 pt) along
    // the stroke, each touching every target anchor in reach. The engine's
    // 20,000-dab limit is checked only between points, so two far points
    // make millions.
    Limit { id: "object.liquify", what: "brush dabs", measure: Measure::Custom(liquify_dabs), max: 5000.0, copies: false },
    // Liquify stroke points.
    Limit { id: "object.liquify", what: "points", measure: Measure::Custom(point_count), max: 1000.0, copies: false },
    // ---- Rasterizing ----
    // Rasterize's resolution: pixels grow with its square (default the
    // document's raster effects resolution; the engine allows 2400 and up
    // to 64 Mpx). The service checks the pixels themselves before it runs.
    Limit { id: "object.rasterize", what: "ppi", measure: Measure::Product(&["ppi"]), max: 1200.0, copies: false },
    // The flattener's line-art and gradient resolutions, at the top level
    // or in `options` (the High preset's 1200; the engine allows 2400).
    Limit { id: "object.flattenTransparency", what: "ppi", measure: Measure::Custom(flattener_ppi), max: 1200.0, copies: false },
    // The resolution raster effects become images at in PDF, EPS, EMF and
    // Expand Appearance (default 72; the High setting 300; the engine
    // allows 2400, 1,111× the pixels of 72).
    Limit { id: "document.rasterEffectsSettings", what: "ppi", measure: Measure::Custom(raster_effects_ppi), max: 600.0, copies: false },
    // The clipboard's PNG at `scale` pixels per point (the engine allows
    // 64); the service checks its pixels before it runs.
    Limit { id: "clipboard.exportPng", what: "pixels per point", measure: Measure::Product(&["scale"]), max: 16.0, copies: false },
    // `document.serialize`'s raster scale, as `run`'s own `scale` (the
    // engine allows 64); the service checks the whole export before it runs.
    Limit { id: "document.serialize", what: "pixels per point", measure: Measure::Product(&["scale"]), max: 16.0, copies: false },
    // `document.serialize`'s raster resolution (16 pixels per point).
    Limit { id: "document.serialize", what: "ppi", measure: Measure::Product(&["ppi"]), max: 1152.0, copies: false },
    // The EPS flattener's resolutions in `document.serialize`.
    Limit { id: "document.serialize", what: "ppi", measure: Measure::Custom(serialize_flattener_ppi), max: 1200.0, copies: false },
    // Save for Web's image width in pixels (the service checks the image).
    Limit { id: "document.exportForWeb.preview", what: "pixels wide", measure: Measure::Product(&["width"]), max: 8192.0, copies: false },
    // Save for Web's image height in pixels.
    Limit { id: "document.exportForWeb.preview", what: "pixels high", measure: Measure::Product(&["height"]), max: 8192.0, copies: false },
    // Save for Web's image size in percent (16× the artboard).
    Limit { id: "document.exportForWeb.preview", what: "percent", measure: Measure::Product(&["percent"]), max: 1600.0, copies: false },
    // ---- Artboards ----
    // A new artboard's width (no engine maximum here; `file.new` stops at
    // 163,830 pt). A raster export renders an artboard whole.
    Limit { id: "artboard.new", what: "points wide", measure: Measure::Product(&["width"]), max: MAX_ARTBOARD_SIDE, copies: false },
    Limit { id: "artboard.new", what: "points high", measure: Measure::Product(&["height"]), max: MAX_ARTBOARD_SIDE, copies: false },
    Limit { id: "artboard.setProps", what: "points wide", measure: Measure::Product(&["width"]), max: MAX_ARTBOARD_SIDE, copies: false },
    Limit { id: "artboard.setProps", what: "points high", measure: Measure::Product(&["height"]), max: MAX_ARTBOARD_SIDE, copies: false },
    // ---- Type ----
    // Characters of new type: point type lays out every glyph (about 1.4 KB
    // each), area type shapes whole paragraphs. Twenty thousand is a dozen
    // pages; the document weight bounds the total.
    Limit { id: "text.create", what: "characters", measure: Measure::Custom(text_chars), max: 20_000.0, copies: false },
    Limit { id: "text.createInPath", what: "characters", measure: Measure::Custom(text_chars), max: 20_000.0, copies: false },
    // `text.setText` copies its text into every target.
    Limit { id: "text.setText", what: "characters", measure: Measure::Custom(text_chars), max: 20_000.0, copies: false },
    // `type.insert` appends its text to every target.
    Limit { id: "type.insert", what: "characters", measure: Measure::Custom(text_chars), max: 20_000.0, copies: false },
    // `text.editRange`'s inserted text and styled runs.
    Limit { id: "text.editRange", what: "characters", measure: Measure::Custom(edit_range_chars), max: 20_000.0, copies: false },
    // Styled runs' type size: the runs are full character styles taken as
    // given (the engine clamps text.setStyle to 1296 pt, not these).
    Limit { id: "text.editRange", what: "points (type size)", measure: Measure::Custom(run_style_size), max: 1296.0, copies: false },
    // New area type's width: with a width under one glyph the layout steps
    // down the whole height a point at a time (forever at 1e300). The
    // engine's own Area Type Options stop at 100,000 pt.
    Limit { id: "text.create", what: "points wide (area)", measure: Measure::Product(&["area.width"]), max: 100_000.0, copies: false },
    Limit { id: "text.create", what: "points high (area)", measure: Measure::Product(&["area.height"]), max: 100_000.0, copies: false },
    // The text Find searches for: matching is text length × find length.
    Limit { id: "edit.findReplace", what: "characters to find", measure: Measure::Custom(find_chars), max: 1000.0, copies: false },
    // How many times longer the replacement is than what it replaces: every
    // match in every target grows by it, and repeating compounds ("a" → "aa"
    // doubles each time). It multiplies the text the document holds.
    Limit { id: "edit.findReplace", what: "times the text found (replacement length)", measure: Measure::Custom(replace_growth), max: 100.0, copies: true },
    // Area type rows × columns: each cell copies the frame and wrap outlines
    // (the engine clamps each to 100, 10,000 cells).
    Limit { id: "text.areaOptions", what: "cells (rows × columns)", measure: Measure::Product(&["rows", "columns"]), max: 100.0, copies: false },
    // Tab stops: every tab character scans them, every target stores them
    // (no engine cap on the list).
    Limit { id: "text.tabs.set", what: "tab stops", measure: Measure::Custom(tab_stops), max: 100.0, copies: false },
    // Paragraph indents and spacing (no engine clamp): an indent wider than
    // the frame steps down the whole column. Hyphenation is refused: its
    // cost grows with the cube of a word's length.
    Limit { id: "text.setFormat", what: "points (indent or spacing)", measure: Measure::Custom(paragraph_lengths), max: 100_000.0, copies: false },
    // Style attributes are only type-checked by the engine: the clamps of
    // `text.setStyle` (1296 pt type, 10,000 % scale and tracking, 100,000 pt
    // indents) and its tab stops apply here, and hyphenation is refused.
    Limit { id: "charStyle.new", what: "style attributes", measure: Measure::Custom(style_attrs), max: 1.0, copies: false },
    Limit { id: "charStyle.setAttrs", what: "style attributes", measure: Measure::Custom(style_attrs), max: 1.0, copies: false },
    Limit { id: "paraStyle.new", what: "style attributes", measure: Measure::Custom(style_attrs), max: 1.0, copies: false },
    Limit { id: "paraStyle.setAttrs", what: "style attributes", measure: Measure::Custom(style_attrs), max: 1.0, copies: false },
    // ---- Graphs ----
    // Graph cells, rows × the longest row: one bar, marker or wedge each
    // (ragged data a few KB long asks for 1e8). No engine cap.
    Limit { id: "graph.create", what: "data cells", measure: Measure::Custom(graph_cells), max: 10_000.0, copies: false },
    Limit { id: "graph.setData", what: "data cells", measure: Measure::Custom(graph_cells), max: 10_000.0, copies: false },
    // ---- Symbols ----
    // Sprayer points: at most one instance each, every instance drawing the
    // whole symbol (density, radius and scale only lower the count).
    Limit { id: "symbol.spray", what: "points", measure: Measure::Custom(point_count), max: 1000.0, copies: false },
    // Symbolism tool points: each scans every candidate instance with a
    // lookup of the whole document.
    Limit { id: "symbol.adjust", what: "points", measure: Measure::Custom(point_count), max: 256.0, copies: false },
];

// ---------- the limits' own measures ----------

/// A number as the engine's most permissive numeric parameters read it
/// (effect parameters, path commands' counts): a JSON number, a string that
/// starts with one (`"10 pt"` reads 10), a boolean as 1 or 0. `None` when
/// absent, null, not a number or not finite: the engine's default applies.
fn loose_num(v: Option<&Json>) -> Option<f64> {
    match v? {
        Json::Number(n) => n.as_f64(),
        Json::String(s) => {
            let t: String = s.trim().chars().take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')).collect();
            t.parse::<f64>().ok()
        }
        Json::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
    .filter(|n| n.is_finite())
}

/// The length of the list `key` holds (`None` when it holds none).
fn list_len(p: &Json, key: &str) -> Option<f64> {
    p.get(key).and_then(Json::as_array).map(|a| a.len() as f64)
}

/// Characters of the string `key` holds.
fn chars_of(p: &Json, key: &str) -> Option<f64> {
    p.get(key).and_then(Json::as_str).map(|s| s.chars().count() as f64)
}

/// `points`, a drag or stroke: how many it holds.
fn point_count(p: &Json) -> Result<Option<f64>, String> {
    Ok(list_len(p, "points"))
}

/// `dash`: how many entries the pattern holds.
fn dash_entries(p: &Json) -> Result<Option<f64>, String> {
    Ok(list_len(p, "dash"))
}

/// `strokeOptions.dash` of a character style range.
fn stroke_options_dash_entries(p: &Json) -> Result<Option<f64>, String> {
    Ok(p.get("strokeOptions").and_then(|o| list_len(o, "dash")))
}

/// The widest side factor (`[t, left, right]`) of a width profile.
fn profile_width(p: &Json) -> Result<Option<f64>, String> {
    let Some(points) = p.get("points").and_then(Json::as_array) else { return Ok(None) };
    Ok(points.iter().filter_map(Json::as_array).flat_map(|pt| pt.iter().skip(1).take(2)).filter_map(Json::as_f64).map(f64::abs).reduce(f64::max))
}

/// The largest absolute number in `path.create`'s SVG path data `d`
/// (coordinates and arc radii alike; flags and angles stay small).
fn path_data_extent(p: &Json) -> Result<Option<f64>, String> {
    let Some(d) = p.get("d").and_then(Json::as_str) else { return Ok(None) };
    let mut max: Option<f64> = None;
    let mut num = String::new();
    let flush = |num: &mut String, max: &mut Option<f64>| -> Result<(), String> {
        if !num.is_empty() {
            // Numbers the SVG grammar runs together ("1.5.5", "1-2") split where
            // a second dot or sign starts; reading the whole run as one bound
            // only overestimates.
            let v: f64 = num.trim_end_matches(['e', 'E', '+', '-']).parse().unwrap_or_else(|_| {
                num.split(['-', '+']).filter_map(|t| t.split('.').next().and_then(|i| i.parse::<f64>().ok())).fold(0.0_f64, |a, b| a.max(b.abs()))
            });
            if !v.is_finite() {
                return Err("`d` holds a number too large to draw".into());
            }
            *max = Some(max.map_or(v.abs(), |m| m.max(v.abs())));
            num.clear();
        }
        Ok(())
    };
    let mut prev = ' ';
    for c in d.chars() {
        let exponent_sign = matches!(c, '+' | '-') && matches!(prev, 'e' | 'E') && !num.is_empty();
        if c.is_ascii_digit() || c == '.' || (matches!(c, 'e' | 'E') && !num.is_empty()) || exponent_sign {
            num.push(c);
        } else if matches!(c, '+' | '-') {
            flush(&mut num, &mut max)?;
            num.push(c);
        } else {
            flush(&mut num, &mut max)?;
        }
        prev = c;
    }
    flush(&mut num, &mut max)?;
    Ok(max)
}

/// Blend steps a call asks for: `steps` (or `value` with `spacing: "steps"`),
/// read as the engine reads them. Distance and smooth-colour spacing let the
/// engine choose (at most 1000 and 256 a pair), which the document weight
/// bounds instead.
fn blend_steps(p: &Json) -> Result<Option<f64>, String> {
    let num = |k: &str| p.get(k).and_then(Json::as_f64);
    Ok(match p.get("spacing").and_then(Json::as_str) {
        Some("steps") => num("value").or_else(|| num("steps")),
        Some(_) => None,
        None => num("steps"),
    })
}

/// The effect an `effect.apply` / `appearance.addEffect` call names:
/// `effect`, else its alias `id`, as the engine reads them.
fn named_effect(p: &Json) -> Option<&str> {
    p.get("effect").and_then(Json::as_str).or_else(|| p.get("id").and_then(Json::as_str))
}

/// Parameter `key` of an effect call: of an `effect.apply` that names one of
/// `effects`; of an `effect.setParams` when `effects` is empty (the effect its
/// `index` names is known only once it runs, so any effect's `key` counts).
fn effect_param(p: &Json, effects: &[&str], key: &str) -> Option<f64> {
    if !effects.is_empty() && !named_effect(p).is_some_and(|e| effects.contains(&e)) {
        return None;
    }
    loose_num(p.get("params").and_then(|q| q.get(key)))
}

/// The copies a Transform effect draws, the original included (the engine
/// drops a fraction and clamps below at 0).
fn copies_drawn(copies: f64) -> f64 {
    copies.max(0.0).floor() + 1.0
}

fn transform_copies(p: &Json) -> Result<Option<f64>, String> {
    Ok(effect_param(p, &["distort.transform"], "copies").map(copies_drawn))
}

fn any_effect_copies(p: &Json) -> Result<Option<f64>, String> {
    Ok(effect_param(p, &[], "copies").map(copies_drawn))
}

fn zigzag_ridges(p: &Json) -> Result<Option<f64>, String> {
    Ok(effect_param(p, &["distort.zigZag"], "ridges").map(f64::abs))
}

fn any_effect_ridges(p: &Json) -> Result<Option<f64>, String> {
    Ok(effect_param(p, &[], "ridges").map(f64::abs))
}

fn roughen_detail(p: &Json) -> Result<Option<f64>, String> {
    Ok(effect_param(p, &["distort.roughen"], "detail").map(f64::abs))
}

fn any_effect_detail(p: &Json) -> Result<Option<f64>, String> {
    Ok(effect_param(p, &[], "detail").map(f64::abs))
}

fn offset_effect(p: &Json) -> Result<Option<f64>, String> {
    Ok(effect_param(p, &["path.offsetPath"], "offset").map(f64::abs))
}

fn any_effect_offset(p: &Json) -> Result<Option<f64>, String> {
    Ok(effect_param(p, &[], "offset").map(f64::abs))
}

/// The larger of two amounts, either of which may be absent.
fn larger(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.abs().max(b.abs())),
        (a, b) => a.or(b),
    }
}

/// The blur of a raster effect `effect.apply` adds: shadows' and glows'
/// `blur`, feather's and Gaussian blur's `radius`.
fn raster_effect_blur(p: &Json) -> Result<Option<f64>, String> {
    let blur = effect_param(p, &["stylize.dropShadow", "stylize.outerGlow", "stylize.innerGlow"], "blur");
    let radius = effect_param(p, &["stylize.feather", "blur.gaussian"], "radius");
    Ok(larger(blur, radius).map(f64::abs))
}

fn any_effect_blur(p: &Json) -> Result<Option<f64>, String> {
    Ok(larger(effect_param(p, &[], "blur"), effect_param(p, &[], "radius")).map(f64::abs))
}

/// A length as the engine's path commands read it: a number of points, or a
/// string with a unit (`"10 mm"`).
fn length_of(v: Option<&Json>) -> Option<f64> {
    match v? {
        Json::String(s) => vectorcraft_engine::doc::Unit::Points.parse(s).filter(|v| v.is_finite()).or_else(|| loose_num(v)),
        v => loose_num(Some(v)),
    }
}

fn offset_length(p: &Json) -> Result<Option<f64>, String> {
    Ok(length_of(p.get("offset")).map(f64::abs))
}

/// How many dabs a Liquify stroke lays: one every tenth of the brush (at
/// least 0.5 pt) along the stroke, as `vectorcraft_tools`' liquify spaces
/// them, plus one per point. `diameter` sets the brush's width and height;
/// they default to 100 pt and the engine clamps them to 0.5–10,000.
fn liquify_dabs(p: &Json) -> Result<Option<f64>, String> {
    let Some(points) = p.get("points").and_then(Json::as_array) else { return Ok(None) };
    let side = |k: &str| p.get("diameter").or_else(|| p.get(k)).and_then(Json::as_f64).unwrap_or(100.0).clamp(0.5, 10_000.0);
    let spacing = (0.1 * side("width").min(side("height"))).max(0.5);
    let at = |v: &Json| Some((v.get(0)?.as_f64()?, v.get(1)?.as_f64()?));
    let mut length = 0.0;
    let mut last: Option<(f64, f64)> = None;
    for (x, y) in points.iter().filter_map(at) {
        if let Some((lx, ly)) = last {
            length += (x - lx).hypot(y - ly);
        }
        last = Some((x, y));
    }
    Ok(Some(points.len() as f64 + length / spacing))
}

/// The highest ppi a flattening asks for: `lineArtPpi` and `gradientPpi` at
/// the top level or in `options` (which wins).
fn flattener_ppi(p: &Json) -> Result<Option<f64>, String> {
    let keys = ["lineArtPpi", "gradientPpi"];
    let top = keys.iter().filter_map(|k| loose_num(p.get(*k)));
    let inner = keys.iter().filter_map(|k| loose_num(p.get("options").and_then(|o| o.get(*k))));
    Ok(top.chain(inner).map(f64::abs).reduce(f64::max))
}

/// `document.serialize`'s EPS flattener resolutions (`flattener: {…}`).
fn serialize_flattener_ppi(p: &Json) -> Result<Option<f64>, String> {
    Ok(p.get("flattener").map(flattener_ppi).transpose()?.flatten())
}

/// The resolution `document.rasterEffectsSettings` sets, read as the engine
/// reads it: a number, `screen` (72), `medium` (150), `high` (300) or a
/// number with `ppi` after it. Anything else the engine refuses itself.
fn raster_effects_ppi(p: &Json) -> Result<Option<f64>, String> {
    Ok(match p.get("resolution") {
        Some(Json::String(s)) => match s.to_ascii_lowercase().as_str() {
            "screen" => Some(72.0),
            "medium" => Some(150.0),
            "high" => Some(300.0),
            other => other.trim_end_matches("ppi").trim().parse::<f64>().ok(),
        },
        Some(v) => v.as_f64(),
        None => None,
    })
}

/// Characters of `text` a type command writes.
fn text_chars(p: &Json) -> Result<Option<f64>, String> {
    Ok(chars_of(p, "text"))
}

/// Characters `text.editRange` writes: `insert` and the styled `runs`' text.
fn edit_range_chars(p: &Json) -> Result<Option<f64>, String> {
    let runs = p.get("runs").and_then(Json::as_array).map(|r| r.iter().filter_map(|r| chars_of(r, "text")).sum::<f64>());
    Ok(match (chars_of(p, "insert"), runs) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
    })
}

/// The largest type size among `text.editRange`'s styled runs.
fn run_style_size(p: &Json) -> Result<Option<f64>, String> {
    let runs = p.get("runs").and_then(Json::as_array);
    Ok(runs.into_iter().flatten().filter_map(|r| r.get("style").and_then(|s| s.get("size")).and_then(Json::as_f64)).map(f64::abs).reduce(f64::max))
}

fn find_chars(p: &Json) -> Result<Option<f64>, String> {
    Ok(chars_of(p, "find"))
}

/// How many times longer `replace` is than `find` (1 when it is not longer).
fn replace_growth(p: &Json) -> Result<Option<f64>, String> {
    let find = chars_of(p, "find").unwrap_or(0.0).max(1.0);
    Ok(chars_of(p, "replace").map(|r| (r / find).max(1.0)))
}

fn tab_stops(p: &Json) -> Result<Option<f64>, String> {
    Ok(list_len(p, "stops"))
}

/// The refusal of hyphenation: Liang hyphenation re-scans the rest of a word
/// at every break, about w³ ÷ line length steps for a word of w letters, and
/// any later edit can run words together (`edit.findReplace` of a space).
fn refuse_hyphenation(p: &Json, key: &str) -> Result<(), String> {
    if p.get(key).and_then(Json::as_bool) == Some(true) {
        return Err("hyphenation is not turned on through the door: its layout grows with the cube of a word's length".into());
    }
    Ok(())
}

/// `text.setFormat`'s paragraph indents and spacing (the largest), with
/// hyphenation refused.
fn paragraph_lengths(p: &Json) -> Result<Option<f64>, String> {
    refuse_hyphenation(p, "hyphenate")?;
    let keys = ["leftIndent", "rightIndent", "firstLineIndent", "spaceBefore", "spaceAfter"];
    Ok(keys.iter().filter_map(|k| p.get(*k).and_then(Json::as_f64)).map(f64::abs).reduce(f64::max))
}

/// A character or paragraph style's `attrs` within the limits the engine
/// applies to the same attributes through `text.setStyle`,
/// `text.setRangeStyle` and `text.setFormat`, which a style's attributes
/// skip: refused (`Err`) when one is over, else nothing to bound.
fn style_attrs(p: &Json) -> Result<Option<f64>, String> {
    let Some(attrs) = p.get("attrs") else { return Ok(None) };
    refuse_hyphenation(attrs, "hyphenate")?;
    let limits: [(&str, f64); 13] = [
        ("size", 1296.0),
        ("leading", 5000.0),
        ("tracking", 10_000.0),
        ("kerning", 10_000.0),
        ("baseline_shift", 1296.0),
        ("h_scale", 10_000.0),
        ("v_scale", 10_000.0),
        ("stroke_width", 1000.0),
        ("left_indent", 100_000.0),
        ("right_indent", 100_000.0),
        ("first_line_indent", 100_000.0),
        ("space_before", 100_000.0),
        ("space_after", 100_000.0),
    ];
    for (key, max) in limits {
        if let Some(v) = attrs.get(key).and_then(Json::as_f64).filter(|v| v.abs() > max) {
            return Err(format!("attribute `{key}` is {v}, more than the {max} the door allows"));
        }
    }
    if let Some(n) = list_len(attrs, "tabs").filter(|n| *n > 100.0) {
        return Err(format!("attribute `tabs` holds {n} tab stops, more than the 100 the door allows"));
    }
    Ok(None)
}

/// The cells a graph's data asks for: categories (rows) × the longest row
/// (series), as `graph.create` / `graph.setData` lay it out: `rows` wins over
/// `csv` (comma or tab separated, a header row and a label column), and
/// extra `series` or `categories` labels add none.
fn graph_cells(p: &Json) -> Result<Option<f64>, String> {
    if let Some(rows) = p.get("rows").and_then(Json::as_array) {
        let lens = rows.iter().filter_map(Json::as_array).map(Vec::len).filter(|n| *n > 0);
        let (count, longest) = lens.fold((0usize, 0usize), |(c, l), n| (c + 1, l.max(n)));
        return Ok(Some((count * longest) as f64));
    }
    let Some(csv) = p.get("csv").and_then(Json::as_str) else { return Ok(None) };
    let lines: Vec<&str> = csv.lines().filter(|l| !l.trim().is_empty()).collect();
    let longest = lines.iter().skip(1).map(|l| l.split([',', '\t']).count().saturating_sub(1)).max().unwrap_or(0);
    Ok(Some((lines.len().saturating_sub(1) * longest) as f64))
}

/// The command door's gate: vectorcraft's reviewed classification
/// (`skill/safety.json`, generated and drift-checked by `tests/skill.rs`)
/// and [`REVIEWED`].
pub fn door() -> Result<&'static Door, String> {
    static DOOR: OnceLock<Result<Door, String>> = OnceLock::new();
    DOOR.get_or_init(|| Door::new("vector", include_str!("../skill/safety.json"), &REVIEWED)).as_ref().map_err(Clone::clone)
}

/// The ids of the effects the engine builds in, computed once from a fresh
/// session's `effect.list` catalog, with any `plugin.<id>` excluded. A fresh
/// session installs no plug-ins, so this is exactly the built-in catalogue.
fn builtin_effects() -> &'static BTreeSet<String> {
    static SET: OnceLock<BTreeSet<String>> = OnceLock::new();
    SET.get_or_init(|| {
        let listed = Session::new().execute("effect.list", &json!({})).unwrap_or(Json::Null);
        listed["catalog"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| e["id"].as_str())
            .filter(|id| !id.starts_with("plugin."))
            .map(str::to_string)
            .collect()
    })
}

/// Whether `name` is an effect the engine builds in (never an installed
/// effect plug-in `plugin.<id>`).
fn builtin_effect(name: &str) -> bool {
    builtin_effects().contains(name)
}

// ---------------------------------------------------------------------------
// What one call may grow a document to (ADR 0013, #418)
// ---------------------------------------------------------------------------
//
// The door's limits bound what one command asks for by its parameters. What
// the commands of a call then add up to, and the copies no parameter counts
// (a duplicate loop, a copy pasted again, a repeat of a repeat built over
// several calls, an effect duplicated onto itself), the service weighs: before
// a command that copies, what it would make; after every command, the whole
// document. Each measure has a ceiling, and a document opened above one may
// not grow past what it was opened with.

/// The undo steps a door call keeps: each holds what a command changed of
/// the document, so 64 commands rewriting a large document would otherwise
/// hold 64 copies of it.
const UNDO_STEPS: usize = 8;

/// The door's ceilings, one per measure of a [`Weight`], each with what it
/// counts and why it sits there.
const CEILINGS: [(f64, &str); 15] = [
    // Every node lookup walks the document's tree, so an edit of a selection
    // costs selected × nodes (engine `Document::node`): 20,000² node visits
    // is about a second.
    (20_000.0, "nodes in its layers, symbols, patterns and masks"),
    // Rendering and exporting meet live copies one by one (repeats, blends,
    // symbol instances, Transform-effect copies): a few µs each.
    (100_000.0, "objects as drawn (live copies counted)"),
    // 48 bytes each, flattened on every draw; counted as drawn.
    (1_000_000.0, "anchor points as drawn"),
    // The engine's own per-subpath limit in Liquify and blends; adding and
    // simplifying anchors is quadratic in it.
    (20_000.0, "anchor points in one subpath"),
    // Layout shapes every character (about 190 B) and point type places every
    // glyph (about 1.4 KB): 100,000 is about 160 MB laid out at once.
    (100_000.0, "characters of type"),
    // Each image decodes whole into RGBA to be drawn: 256 MB at the ceiling,
    // the engine's own largest raster, once.
    (64.0e6, "pixels of images"),
    // Encoded image data stays in memory for the whole session (the engine
    // never drops a blob, and every recolour or rasterize adds one).
    (128.0 * 1024.0 * 1024.0, "bytes of image data"),
    // The engine's own New Document limit.
    (1000.0, "artboards"),
    // The renderer recurses once per level (symbols in symbols too).
    (64.0, "levels of nesting"),
    // Guides are cheap, but copying selected guides doubles them, and Slices
    // from Guides builds every cell before it checks.
    (5000.0, "guides"),
    // The clipboard holds whole copies (an import's art too) for the call.
    (20_000.0, "nodes on the clipboard"),
    // Brush definitions and their art live in the document's extra data,
    // which every edit copies whole.
    (16.0 * 1024.0 * 1024.0, "bytes of brush and other extra data"),
    // A Transform effect that scales its copies grows them geometrically
    // (scale^copies): coordinates beyond this approach float limits.
    (1.0e6, "times its size (a Transform effect's growth)"),
    // A Pathfinder effect's sweep keeps a winding entry per shape for every
    // segment, on every draw; Scribble scans every edge once per row.
    (5.0e7, "segment visits in one live effect"),
    // Threaded type re-flows the story once per frame after every edit.
    (5.0e7, "characters re-flowed through threaded frames"),
];

/// What the door weighs a document by after every command of a call (see
/// [`CEILINGS`] for each measure's reason).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Weight {
    nodes: f64,
    objects: f64,
    anchors: f64,
    subpath: f64,
    chars: f64,
    image_pixels: f64,
    image_bytes: f64,
    artboards: f64,
    nesting: f64,
    guides: f64,
    clipboard: f64,
    extra_bytes: f64,
    growth: f64,
    effect_work: f64,
    thread_work: f64,
}

impl Weight {
    fn values(&self) -> [f64; 15] {
        [
            self.nodes,
            self.objects,
            self.anchors,
            self.subpath,
            self.chars,
            self.image_pixels,
            self.image_bytes,
            self.artboards,
            self.nesting,
            self.guides,
            self.clipboard,
            self.extra_bytes,
            self.growth,
            self.effect_work,
            self.thread_work,
        ]
    }

    /// The ceilings of a call that opened a document weighing `self`: each
    /// the door's own, or what the document was opened with when that is more.
    fn ceilings(&self) -> [f64; 15] {
        let mut out = [0.0; 15];
        for ((o, v), (c, _)) in out.iter_mut().zip(self.values()).zip(CEILINGS) {
            *o = c.max(v);
        }
        out
    }
}

/// `n` as a message shows it: whole numbers whole, others to two places,
/// the very large in scientific notation.
fn shown(n: f64) -> String {
    match n.abs() {
        a if a >= 1e15 || !a.is_finite() => format!("{n:.3e}"),
        _ if n.fract() == 0.0 => format!("{}", n as i64),
        _ => format!("{n:.2}"),
    }
}

/// Refuse `now` over `ceilings` (from [`Weight::ceilings`]): `what` says
/// which command it is the document after, or would be.
fn check_weight(now: &Weight, ceilings: &[f64; 15], what: &str) -> Result<(), String> {
    for ((n, max), (_, label)) in now.values().into_iter().zip(ceilings).zip(CEILINGS) {
        if n > *max {
            return Err(format!("vector.run: {what} the document would hold {} {label}, more than the {} the door allows a call", shown(n), shown(*max)));
        }
    }
    Ok(())
}

/// Objects, anchor points and characters as the renderer and the exporters
/// meet them (a live repeat's, blend's or symbol instance's art and a
/// Transform effect's copies once per copy), and the full-frame passes a
/// raster export makes for them (opacity masks, blend modes, knockouts).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Drawn {
    objects: f64,
    anchors: f64,
    chars: f64,
    passes: f64,
}

impl Drawn {
    fn scaled(self, k: f64) -> Drawn {
        Drawn { objects: self.objects * k, anchors: self.anchors * k, chars: self.chars * k, passes: self.passes * k }
    }
    fn max(self, o: Drawn) -> Drawn {
        Drawn { objects: self.objects.max(o.objects), anchors: self.anchors.max(o.anchors), chars: self.chars.max(o.chars), passes: self.passes.max(o.passes) }
    }
}

impl std::ops::AddAssign for Drawn {
    fn add_assign(&mut self, o: Drawn) {
        self.objects += o.objects;
        self.anchors += o.anchors;
        self.chars += o.chars;
        self.passes += o.passes;
    }
}

/// A raster effect a document draws: where its object paints (document
/// space), how far its effects reach beyond that and how far they blur, and
/// how many times it is drawn.
#[derive(Clone, Copy, Debug)]
struct RasterRecord {
    bounds: Rect,
    reach: f64,
    blur: f64,
    count: f64,
}

/// A pattern paint a document draws: the size of what it covers (document
/// space), the pattern, the paint's scale (its transform's determinant) and
/// how many times it is drawn.
#[derive(Clone, Debug)]
struct PatternRecord {
    width: f64,
    height: f64,
    pattern: String,
    det: f64,
    count: f64,
}

/// Anchor points a glyph of outlined or effected type stands for (the
/// bundled font averages 23 path elements a Latin glyph).
const GLYPH_ANCHORS: f64 = 30.0;

/// How far [`Weigher`] walks into nesting: deeper still is over the nesting
/// ceiling anyway, and the walk stays off the stack's edge.
const WALK_DEPTH: usize = 256;

/// Walks a document once, multiplying live copies instead of making them.
struct Weigher<'a> {
    doc: &'a Document,
    /// What one instance of each symbol draws, how deep its art nests and
    /// how much its Transform effects grow; `None` while it is being weighed
    /// (meeting it again then is a cycle).
    symbols: HashMap<&'a str, Option<(Drawn, usize, f64)>>,
    /// Pixels of each image shown, by key.
    images: HashMap<&'a str, f64>,
    nodes: f64,
    subpath: f64,
    nesting: usize,
    growth: f64,
    effect_work: f64,
    /// The multiplier the node being walked is drawn with (instances of the
    /// repeats, blends and symbols around it).
    count: f64,
    raster: Vec<RasterRecord>,
    patterns: Vec<PatternRecord>,
    /// A symbol that holds an instance of itself.
    cycle: Option<&'a str>,
}

impl<'a> Weigher<'a> {
    fn new(doc: &'a Document) -> Self {
        Weigher {
            doc,
            symbols: HashMap::new(),
            images: HashMap::new(),
            nodes: 0.0,
            subpath: 0.0,
            nesting: 0,
            growth: 1.0,
            effect_work: 0.0,
            count: 1.0,
            raster: Vec::new(),
            patterns: Vec::new(),
            cycle: None,
        }
    }

    /// What `n` draws, at nesting `depth`, inside Transform-effect growth `growth`.
    fn node(&mut self, n: &'a Node, depth: usize, growth: f64) -> Drawn {
        self.nodes += 1.0;
        self.nesting = self.nesting.max(depth);
        let mut d = Drawn { objects: 1.0, ..Drawn::default() };
        if depth > WALK_DEPTH {
            return d;
        }
        let growth = growth * own_growth(n);
        self.growth = self.growth.max(growth);
        match &n.kind {
            NodeKind::Layer { children, .. } | NodeKind::Group { children, .. } | NodeKind::Compound { children, .. } => {
                for c in children {
                    d += self.node(c, depth + 1, growth);
                }
            }
            NodeKind::Path { path, .. } => {
                d.anchors += path.anchor_count() as f64;
                for sp in &path.subpaths {
                    self.subpath = self.subpath.max(sp.anchors.len() as f64);
                }
            }
            NodeKind::Text(t) => d.chars += t.runs.iter().map(|r| r.text.chars().count()).sum::<usize>() as f64,
            NodeKind::Image(im) => {
                self.images.insert(&im.key, f64::from(im.width) * f64::from(im.height));
            }
            NodeKind::SymbolInstance { symbol, .. } => d += self.symbol(symbol, depth + 1, growth),
            NodeKind::Blend { children, spec } => {
                let keys: Vec<(Drawn, Option<Rect>)> = children.iter().map(|c| (self.node(c, depth + 1, growth), c.geometric_bounds())).collect();
                for (k, _) in &keys {
                    d += *k;
                }
                for pair in keys.windows(2) {
                    let steps = blend_pair_steps(spec, pair[0].1, pair[1].1);
                    d += pair[0].0.max(pair[1].0).scaled(steps);
                }
            }
            NodeKind::Envelope { content, fidelity, .. } => {
                let mut c = Drawn::default();
                for x in content {
                    c += self.node(x, depth + 1, growth);
                }
                // Each content segment is cut into pieces of the content's
                // diagonal over (8 + 0.56 × fidelity), at most 64 a segment.
                let k = 8.0 + 0.56 * fidelity.clamp(0.0, 100.0);
                c.anchors = subdivided(c.anchors + c.chars * GLYPH_ANCHORS, content_length(content), nodes_diagonal(content), k, 64.0);
                d += c;
            }
            NodeKind::Mesh(m) => {
                // At least 2 × 2 quads a patch, up to 32 × 32 when large on screen.
                d.objects += 4.0 * f64::from(m.rows) * f64::from(m.cols);
                d.anchors += m.points.len() as f64;
            }
            NodeKind::Repeat(r) => {
                let instances = repeat_instances(&r.kind);
                let outer = self.count;
                self.count *= instances;
                let mut src = Drawn::default();
                for s in &r.source {
                    src += self.node(s, depth + 1, growth);
                }
                self.count = outer;
                d += src.scaled(instances);
            }
        }
        // A blend mode, a knockout, or a group's own opacity or isolation is a
        // compositing layer over the whole frame (vello_cpu layers span the
        // viewport; the renderer folds a plain leaf's opacity into its paint).
        let group = n.children().is_some();
        if n.blend != vectorcraft_engine::doc::color::BlendMode::Normal || n.knocks_out(false) || (group && (n.opacity < 1.0 || n.isolate)) {
            d.passes += 1.0;
        }
        let mut d = self.effects(n, d);
        self.paints(n);
        if let Some(m) = n.mask.as_deref().filter(|m| !m.disabled) {
            d += self.node(&m.art, depth + 1, growth);
            d.passes += 1.0;
        }
        d
    }

    /// What one instance of symbol `name` draws (its art, weighed once).
    fn symbol(&mut self, name: &'a str, depth: usize, growth: f64) -> Drawn {
        match self.symbols.get(name) {
            Some(Some((d, inner, g))) => {
                self.nesting = self.nesting.max(depth + inner);
                self.growth = self.growth.max(growth * g);
                return *d;
            }
            Some(None) => {
                self.cycle.get_or_insert(name);
                return Drawn::default();
            }
            None => {}
        }
        let Some(sym) = self.doc.symbols.iter().find(|s| s.name == name) else { return Drawn::default() };
        self.symbols.insert(name, None);
        let (nesting, outer_growth) = (std::mem::take(&mut self.nesting), std::mem::replace(&mut self.growth, 1.0));
        let d = self.node(&sym.art, 0, 1.0);
        let (inner, g) = (self.nesting, self.growth);
        self.nesting = nesting.max(depth + inner);
        self.growth = outer_growth.max(growth * g);
        self.symbols.insert(name, Some((d, inner, g)));
        d
    }

    /// `d`, what `n` draws without its effects, with its visible effects
    /// applied: the object's own in stack order, then the heaviest of its
    /// fills' and strokes' own, each running on the object's result. Type
    /// counts its glyphs' outlines when an effect reshapes it.
    fn effects(&mut self, n: &'a Node, d: Drawn) -> Drawn {
        // Raster effects, where the object's own geometry lies (its visual
        // bounds: the renderer's painted bounds would evaluate its geometry
        // effects, the very work being weighed), drawn once per copy.
        let raster: Vec<_> = std::iter::once(&n.appearance.effects).chain(n.appearance.items.iter().map(AppearanceItem::effects)).flat_map(|e| vectorcraft_render::effects::raster_effects(e)).collect();
        if let (false, Some(bounds)) = (raster.is_empty(), n.visual_bounds()) {
            let reach = raster.iter().map(|f| f.outset()).fold(0.0, f64::max);
            let blur = raster.iter().map(raster_blur).sum();
            self.raster.push(RasterRecord { bounds, reach, blur, count: self.count * own_copies(n) });
        }
        let reshapes = |list: &[Effect]| list.iter().any(|e| e.visible && !vectorcraft_render::effects::is_raster(&e.id));
        if !reshapes(&n.appearance.effects) && !n.appearance.items.iter().any(|i| reshapes(i.effects())) {
            return d;
        }
        let mut geometry = Geometry::of(n, d);
        geometry.apply(&n.appearance.effects, self);
        let object = geometry.clone();
        for item in &n.appearance.items {
            let mut g = object.clone();
            g.apply(item.effects(), self);
            geometry.drawn = geometry.drawn.max(g.drawn);
        }
        geometry.drawn
    }

    /// Record the pattern paints `n`'s fills and strokes lay (exports that
    /// cannot paint a pattern lay its tiles one by one).
    fn paints(&mut self, n: &'a Node) {
        for item in n.appearance.items.iter().filter(|i| i.visible()) {
            if let (Paint::Pattern { pattern, xf }, Some(b)) = (item.paint(), n.visual_bounds()) {
                let det = xf.determinant().abs();
                self.patterns.push(PatternRecord { width: b.width(), height: b.height(), pattern: pattern.clone(), det, count: self.count });
            }
        }
    }
}

/// The copies `n`'s own visible Transform effects draw, the original
/// included (stacked effects multiply).
fn own_copies(n: &Node) -> f64 {
    let items = n.appearance.items.iter().map(AppearanceItem::effects);
    std::iter::once(&n.appearance.effects)
        .chain(items)
        .flatten()
        .filter(|e| e.visible && e.id == "distort.transform")
        .map(|e| copies_drawn(loose_num(e.params.get("copies")).unwrap_or(0.0).min(1000.0)))
        .product()
}

/// How much `n`'s own visible Transform effects grow its copies:
/// scale^copies, when they scale up.
fn own_growth(n: &Node) -> f64 {
    let items = n.appearance.items.iter().map(AppearanceItem::effects);
    std::iter::once(&n.appearance.effects)
        .chain(items)
        .flatten()
        .filter(|e| e.visible && e.id == "distort.transform")
        .map(|e| {
            let p = &e.params;
            let scale = loose_num(p.get("scaleH")).unwrap_or(100.0).abs().max(loose_num(p.get("scaleV")).unwrap_or(100.0).abs()) / 100.0;
            let copies = loose_num(p.get("copies")).unwrap_or(0.0).clamp(0.0, 1000.0).floor();
            if scale > 1.0 { scale.powf(copies).min(f64::MAX) } else { 1.0 }
        })
        .product()
}

/// The blur distance of a raster effect.
fn raster_blur(f: &vectorcraft_render::effects::RasterFx) -> f64 {
    use vectorcraft_render::effects::RasterFx;
    match f {
        RasterFx::DropShadow { blur, .. } | RasterFx::OuterGlow { blur, .. } | RasterFx::InnerGlow { blur, .. } => *blur,
        RasterFx::Feather { radius } | RasterFx::GaussianBlur { radius } => *radius,
    }
}

/// The copies a repeat draws, as the engine clamps them.
fn repeat_instances(kind: &RepeatKind) -> f64 {
    match kind {
        RepeatKind::Radial { instances, .. } => f64::from((*instances).clamp(1, 1000)),
        RepeatKind::Grid { rows, cols, .. } => f64::from((*rows).clamp(1, 500)) * f64::from((*cols).clamp(1, 500)),
        RepeatKind::Mirror { .. } => 2.0,
    }
}

/// The steps a blend lays between two keys, as `vectorcraft_doc::blend`
/// counts them: a set number, the distance between the keys' centres over
/// the spacing, or at most 256 for smooth colour (and 1000 along a spine
/// whose length the door does not measure).
fn blend_pair_steps(spec: &vectorcraft_engine::doc::BlendSpec, a: Option<Rect>, b: Option<Rect>) -> f64 {
    use vectorcraft_engine::doc::BlendSpacing;
    match spec.spacing {
        BlendSpacing::Steps(n) => f64::from(n.clamp(1, 1000)),
        BlendSpacing::Distance(d) => match (a, b, &spec.spine) {
            (Some(a), Some(b), None) => ((a.center() - b.center()).hypot() / d.max(0.01)).round().clamp(0.0, 1000.0),
            _ => 1000.0,
        },
        BlendSpacing::SmoothColor => 256.0,
    }
}

/// The total length of the paths in `n` (type stands at three ems of 12 pt
/// a character), measured for effects that subdivide by length.
fn node_length(n: &Node) -> f64 {
    let mut total = 0.0;
    n.walk(&mut |c| match &c.kind {
        NodeKind::Path { path, .. } => total += path.to_bezpath().perimeter(1.0),
        NodeKind::Text(t) => total += 36.0 * t.runs.iter().map(|r| r.text.chars().count()).sum::<usize>() as f64,
        _ => {}
    });
    total
}

/// [`node_length`] of every node of `nodes`.
fn content_length(nodes: &[std::sync::Arc<Node>]) -> f64 {
    nodes.iter().map(|n| node_length(n)).sum()
}

/// The diagonal of `nodes`' geometric bounds.
fn nodes_diagonal(nodes: &[std::sync::Arc<Node>]) -> f64 {
    let b = nodes.iter().filter_map(|n| n.geometric_bounds()).reduce(|a, b| a.union(b));
    b.map_or(0.0, |b| b.width().hypot(b.height()))
}

/// Anchors after an effect cuts every segment into pieces of `diagonal / k`,
/// at most `cap` a segment: about `anchors + length × k / diagonal`.
fn subdivided(anchors: f64, length: f64, diagonal: f64, k: f64, cap: f64) -> f64 {
    let by_length = if diagonal > 0.0 && length.is_finite() { anchors + length * k / diagonal } else { f64::INFINITY };
    by_length.min(anchors * cap)
}

/// An object's geometry as its effect stack reshapes it: what it draws, and
/// its path length while the effects so far keep it measurable.
#[derive(Clone)]
struct Geometry<'a> {
    node: &'a Node,
    drawn: Drawn,
    /// The paths' length, measured on first use; `Some(INFINITY)` once an
    /// effect has lengthened them past measuring (Roughen, Zig Zag), when
    /// later effects take their per-segment ceilings.
    length: Option<f64>,
}

impl<'a> Geometry<'a> {
    fn of(node: &'a Node, mut drawn: Drawn) -> Self {
        drawn.anchors += drawn.chars * GLYPH_ANCHORS;
        Geometry { node, drawn, length: None }
    }

    fn length(&mut self) -> f64 {
        let node = self.node;
        *self.length.get_or_insert_with(|| node_length(node))
    }

    fn diagonal(&self) -> f64 {
        self.node.geometric_bounds().map_or(0.0, |b| b.width().hypot(b.height()))
    }

    /// Apply the visible effects of `list`, in stack order, as
    /// `vectorcraft_effects` evaluates them (see each one's count there).
    fn apply(&mut self, list: &[Effect], w: &mut Weigher) {
        for e in list.iter().filter(|e| e.visible) {
            let p = &e.params;
            let num = |k: &str, default: f64| loose_num(p.get(k)).unwrap_or(default);
            match e.id.as_str() {
                // (copies + 1) × everything, each copy scaled again.
                "distort.transform" => {
                    let copies = num("copies", 0.0).clamp(0.0, 1000.0).floor();
                    let scale = num("scaleH", 100.0).abs().max(num("scaleV", 100.0).abs()) / 100.0;
                    let lengthen = if (scale - 1.0).abs() < 1e-9 { copies + 1.0 } else { ((scale.powf(copies + 1.0) - 1.0) / (scale - 1.0)).abs() };
                    self.drawn = self.drawn.scaled(copies + 1.0);
                    self.length = Some(self.length() * lengthen);
                }
                // (ridges + 1) points a segment; the path then zigzags past measuring.
                "distort.zigZag" => {
                    self.drawn.anchors *= num("ridges", 4.0).clamp(0.0, 100.0).floor() + 1.0;
                    self.length = Some(f64::INFINITY);
                }
                // detail points an inch of path, at most 2000 a segment.
                "distort.roughen" => {
                    let detail = num("detail", 10.0).clamp(0.0, 100.0);
                    let length = self.length();
                    self.drawn.anchors = subdivided(self.drawn.anchors, length, 72.0, detail, 2000.0);
                    self.length = Some(f64::INFINITY);
                }
                // Pieces of the diagonal / 16 (Free Distort), / 24 (warps), and
                // of min(r / 8, r / (1 + 2θ)) (Twist), at most 64 a segment.
                id if id == "distort.freeDistort" || id == "distort.twist" || id.starts_with("warp.") => {
                    let k = match id {
                        "distort.freeDistort" => 16.0,
                        "distort.twist" => 2.0 * (1.0 + 2.0 * num("angle", 10.0).clamp(-3600.0, 3600.0).to_radians().abs()).max(8.0),
                        _ => 24.0,
                    };
                    let (length, diagonal) = (self.length(), self.diagonal());
                    self.drawn.anchors = subdivided(self.drawn.anchors, length, diagonal, k, 64.0);
                }
                // At most 2000 rows over the object, each scanning every edge of
                // the path flattened at 0.25 pt; the scribble is its outline.
                "stylize.scribble" => {
                    let rows = (self.diagonal() / num("spacing", 5.0).max(0.1)).clamp(1.0, 2000.0);
                    let edges = self.length() / 0.25;
                    w.effect_work = w.effect_work.max(rows * edges);
                    self.drawn.anchors += rows * 16.0;
                }
                // Outlines on both sides, joins, then a union.
                "path.offsetPath" | "path.outlineStroke" => self.drawn.anchors *= 4.0,
                // At most 2 anchors a corner.
                "stylize.roundCorners" => self.drawn.anchors *= 2.0,
                // Booleans over every member: a winding entry per shape for
                // every segment of the sweep.
                id if id.starts_with("pathfinder.") => {
                    w.effect_work = w.effect_work.max(self.drawn.anchors * self.drawn.objects);
                    self.drawn.anchors *= 2.0;
                }
                "cropMarks" => {
                    self.drawn.objects += 8.0;
                    self.drawn.anchors += 24.0;
                }
                _ => {}
            }
        }
    }
}

/// The JSON's size in bytes, about (strings and keys at their length,
/// anything else at 8).
fn json_bytes(v: &Json) -> f64 {
    match v {
        Json::String(s) => s.len() as f64 + 2.0,
        Json::Array(a) => a.iter().map(json_bytes).sum::<f64>() + 2.0,
        Json::Object(m) => m.iter().map(|(k, v)| k.len() as f64 + 3.0 + json_bytes(v)).sum::<f64>() + 2.0,
        _ => 8.0,
    }
}

/// What a document's exports would rasterize or lay out one by one, found
/// while weighing it.
#[derive(Debug, Default)]
struct ExportLoad {
    /// Full-frame passes a raster export makes (opacity masks, blend modes,
    /// knockouts), counted as drawn.
    passes: f64,
    raster: Vec<RasterRecord>,
    patterns: Vec<PatternRecord>,
}

/// The weight of the session's active document (and its clipboard), with
/// what its exports would rasterize; `Err` when it holds a symbol that holds
/// an instance of itself (the renderer would recurse without end).
fn weigh(s: &Session) -> Result<(Weight, ExportLoad), String> {
    // A command may close the document (`file.close`): nothing left to weigh.
    let Ok(st) = s.doc() else { return Ok((Weight::default(), ExportLoad::default())) };
    let doc = &st.doc;
    let mut w = Weigher::new(doc);
    let mut drawn = Drawn::default();
    for l in &doc.layers {
        drawn += w.node(l, 0, 1.0);
    }
    // Symbol definitions no instance shows, and pattern tiles (drawn once
    // into a repeating image).
    for sym in &doc.symbols {
        w.symbol(&sym.name, 0, 1.0);
    }
    for def in &doc.patterns {
        for a in &def.art {
            w.node(a, 0, 1.0);
        }
    }
    if let Some(name) = w.cycle {
        return Err(format!("vector.run: the symbol `{name}` holds an instance of itself, which no renderer can draw"));
    }
    let mut clipboard = Weigher::new(doc);
    for n in &s.clipboard.nodes {
        clipboard.node(n, 0, 1.0);
    }
    let thread_work = doc
        .text_threads
        .iter()
        .map(|frames| {
            let chars: usize = frames.iter().filter_map(|id| doc.node(*id)).map(|n| match &n.kind {
                NodeKind::Text(t) => t.runs.iter().map(|r| r.text.chars().count()).sum(),
                _ => 0,
            }).sum();
            frames.len() as f64 * chars as f64
        })
        .sum();
    let weight = Weight {
        nodes: w.nodes,
        objects: drawn.objects,
        anchors: drawn.anchors,
        subpath: w.subpath,
        chars: drawn.chars,
        image_pixels: w.images.values().sum(),
        image_bytes: doc.images.values().map(|b| (b.bytes.len() + b.proxy.as_ref().map_or(0, |p| p.len())) as f64).sum(),
        artboards: doc.artboards.len() as f64,
        nesting: w.nesting as f64,
        guides: doc.guides.len() as f64,
        clipboard: clipboard.nodes,
        extra_bytes: doc.unknown.iter().map(|(k, v)| k.len() as f64 + json_bytes(v)).sum(),
        growth: w.growth,
        effect_work: w.effect_work,
        thread_work,
    };
    Ok((weight, ExportLoad { passes: drawn.passes, raster: w.raster, patterns: w.patterns }))
}

// ---------------------------------------------------------------------------
// What an export renders (ADR 0013, #418)
// ---------------------------------------------------------------------------

/// The longest side, in pixels, of an image a door export renders.
const MAX_RASTER_SIDE: f64 = 8192.0;
/// The most pixels one door export renders (all its images together), and
/// one rasterizing command makes: 4096², 64 MB of RGBA (about 200 MB at the
/// export's peak, with its straight-alpha copy and the encoder's). Enough
/// for 4K UHD twice over and A4 or Letter at 300 ppi; larger prints belong
/// in PDF or SVG. (The engine's own ceilings are 32,768 px a side and 2^28
/// px, a GiB of RGBA.)
const MAX_RASTER_PIXELS: f64 = 16.0 * 1024.0 * 1024.0;
/// The most pixel passes a raster export makes over its frame: each opacity
/// mask, blend mode and knockout drawn redraws the whole frame offscreen,
/// a CMYK document draws it once per ink plane, a PSD once more per layer.
const MAX_RASTER_PASSES: f64 = 16.0 * MAX_RASTER_PIXELS;
/// The most pixels the blur buffers of one raster export hold: a shadow's
/// is cached for the whole frame, and a blur's spreads 1.5 × its distance on
/// each side, at the export's scale.
const MAX_FILTER_PIXELS: f64 = 4.0 * MAX_RASTER_PIXELS;
/// The most pixels a PSD export keeps until it writes the file: the merged
/// image and every visible top-level layer drawn alone (256 MB of RGBA).
const MAX_PSD_PIXELS: f64 = 4.0 * MAX_RASTER_PIXELS;
/// The most pattern tiles a PDF, EPS or EMF export lays one by one (none of
/// them gets a pattern paint from the engine): as many as the drawn-objects
/// ceiling. The engine's own cap is 250,000 tiles a fill.
const MAX_EXPORT_TILES: f64 = 100_000.0;
/// The engine's largest raster-effect image (`rasterfx::effect_image`).
const ENGINE_EFFECT_PIXELS: f64 = 64.0e6;

/// The artboards (or the art's bounds) a raster export renders, as the
/// engine's `document.export` picks them: `range` over `artboards` over
/// `artboard`, every one with `useArtboards: true`, the visible art's bounds
/// with `false`, else the one named (default the first). Names the engine
/// refuses are left to it.
fn raster_regions(doc: &Document, p: &Json) -> Vec<Rect> {
    let count = doc.artboards.len();
    let named: Option<Vec<usize>> = match (p.get("range").and_then(Json::as_str), p.get("artboards").and_then(Json::as_array), p.get("artboard").and_then(Json::as_u64)) {
        (Some(r), _, _) if r.trim().eq_ignore_ascii_case("all") => Some((0..count).collect()),
        (Some(r), _, _) => vectorcraft_engine::doc::range::parse_range(r, count).ok(),
        (None, Some(a), _) => Some(a.iter().filter_map(Json::as_u64).map(|i| i as usize).collect()),
        (None, None, Some(a)) => Some(vec![a as usize]),
        _ => None,
    };
    let chosen = match p.get("useArtboards").and_then(Json::as_bool) {
        Some(false) => return vectorcraft_render::encode::art_bounds(doc).into_iter().collect(),
        Some(true) => named.unwrap_or_else(|| (0..count).collect()),
        None => named.unwrap_or_else(|| vec![0]),
    };
    chosen.into_iter().filter_map(|i| doc.artboards.get(i).map(|a| a.rect)).collect()
}

/// Pixels of the blur buffers drawing `load`'s raster effects into `region`
/// at `scale` pixels per point: each object's painted pixels (clipped to the
/// frame) grown by 1.5 × its blur on each side, once per copy drawn.
fn filter_pixels(load: &ExportLoad, region: Rect, scale: f64) -> f64 {
    let (fw, fh) = (region.width() * scale, region.height() * scale);
    load.raster
        .iter()
        .filter(|r| {
            let b = r.bounds.inflate(r.reach, r.reach);
            b.x0 < region.x1 && b.x1 > region.x0 && b.y0 < region.y1 && b.y1 > region.y0
        })
        .map(|r| {
            let spread = 1.5 * r.blur * scale + 2.0;
            let (w, h) = ((r.bounds.width() * scale).min(fw) + 2.0 * spread, (r.bounds.height() * scale).min(fh) + 2.0 * spread);
            w * h * r.count
        })
        .sum()
}

/// Refuse an export of `doc` as format `f` with the engine's export params
/// `p` (those `document.export` or `document.serialize` takes) over the
/// door's caps, before the engine renders anything: a raster's pixels, its
/// full-frame passes and blur buffers; the images raster effects become in
/// PDF, EPS and EMF, and the pattern tiles those formats lay one by one.
/// SVG writes effects as filters and patterns as patterns; DXF and the
/// native format rasterize nothing.
fn check_export(doc: &Document, load: &ExportLoad, f: &vectorcraft_engine::cmd::fileio::Format, p: &Json, what: &str) -> Result<(), String> {
    if f.raster {
        let scale = p.get("ppi").and_then(Json::as_f64).map(|ppi| ppi / 72.0).or_else(|| p.get("scale").and_then(Json::as_f64)).unwrap_or(1.0).clamp(0.01, 64.0);
        let regions = raster_regions(doc, p);
        let mut pixels = 0.0;
        for r in &regions {
            let (w, h) = vectorcraft_render::raster_size(*r, scale).map_err(|e| format!("vector.run: {what}: {e}"))?;
            let (w, h) = (f64::from(w), f64::from(h));
            if w > MAX_RASTER_SIDE || h > MAX_RASTER_SIDE {
                return Err(format!(
                    "vector.run: {what} would be {} × {} pixels, more than the {} a side the door renders: lower `scale` or the artboard's size",
                    shown(w),
                    shown(h),
                    shown(MAX_RASTER_SIDE)
                ));
            }
            pixels += w * h;
        }
        if pixels > MAX_RASTER_PIXELS {
            return Err(format!(
                "vector.run: {what} would render {} pixels, more than the {} the door renders in one export: lower `scale` or the artboard's size",
                shown(pixels),
                shown(MAX_RASTER_PIXELS)
            ));
        }
        let layers = if f.id == "psd" && p.get("layers").and_then(Json::as_bool) != Some(false) {
            doc.layers.iter().filter(|l| l.visible && !l.is_template()).count() as f64
        } else {
            0.0
        };
        if pixels * (1.0 + layers) > MAX_PSD_PIXELS {
            return Err(format!(
                "vector.run: {what} would keep {} pixels of PSD layers (the merged image and each visible top-level layer), more than the {} the door allows one export: lower `scale`",
                shown(pixels * (1.0 + layers)),
                shown(MAX_PSD_PIXELS)
            ));
        }
        let inks = if doc.color_mode == vectorcraft_engine::doc::ColorMode::Cmyk || p.get("colorModel").and_then(Json::as_str) == Some("cmyk") { 2.0 } else { 1.0 };
        let passes = pixels * (1.0 + layers + load.passes) * inks;
        if passes > MAX_RASTER_PASSES {
            return Err(format!(
                "vector.run: {what} would draw {} pixels in full-frame passes (opacity masks, blend modes, knockouts, ink planes, PSD layers), more than the {} the door allows one export",
                shown(passes),
                shown(MAX_RASTER_PASSES)
            ));
        }
        let filters: f64 = regions.iter().map(|r| filter_pixels(load, *r, scale)).sum();
        if filters > MAX_FILTER_PIXELS {
            return Err(format!(
                "vector.run: {what} would blur {} pixels of shadows, glows and blurs, more than the {} the door allows one export: lower `scale`",
                shown(filters),
                shown(MAX_FILTER_PIXELS)
            ));
        }
        return Ok(());
    }
    // A PDF that must be flattened (version 1.3, PDF/X-1a or X-3) and EPS
    // rasterize transparency with the flattener: at its line-art resolution,
    // 1200 ppi for PDF's default High preset, 300 for EPS's Medium.
    let flattened_pdf = p.get("compatibility").is_some_and(|c| c.as_str() == Some("1.3") || c.as_f64() == Some(1.3))
        || matches!(p.get("standard").and_then(Json::as_str), Some("pdfX1a" | "pdfX3"));
    let ppi = match f.id {
        "pdf" | "ai" if flattened_pdf => doc.raster_effects_ppi.max(1200.0),
        "pdf" | "ai" | "emf" | "wmf" => doc.raster_effects_ppi,
        "eps" => doc.raster_effects_ppi.max(300.0),
        _ => return Ok(()),
    };
    let k = (ppi / 72.0).clamp(1.0 / 72.0, 2400.0 / 72.0);
    let add = doc.raster_effects.add_around + 2.0;
    let pixels: f64 = load
        .raster
        .iter()
        .map(|r| {
            let b = r.bounds.inflate(r.reach + add, r.reach + add);
            ((b.width() * k).ceil().max(1.0) * (b.height() * k).ceil().max(1.0)).min(ENGINE_EFFECT_PIXELS)
        })
        .sum();
    if pixels > MAX_RASTER_PIXELS {
        return Err(format!(
            "vector.run: {what} would render its raster effects (shadows, glows, blurs, feathers) as {} pixels of images at {} ppi, more than the {} the door renders in one export: lower document.rasterEffectsSettings' resolution or export SVG",
            shown(pixels),
            shown(ppi),
            shown(MAX_RASTER_PIXELS)
        ));
    }
    let tiles: f64 = load
        .patterns
        .iter()
        .map(|t| {
            let Some(def) = doc.pattern(&t.pattern) else { return 0.0 };
            let side = t.det.sqrt().max(1e-9);
            let across = |extent: f64, tile: f64| extent / (tile * side) + 4.0;
            let laid = (across(t.width, def.width()) * across(t.height, def.height())).min(250_000.0);
            let art: f64 = def.art.iter().map(|a| a.count() as f64).sum();
            laid * art * t.count
        })
        .sum();
    if tiles > MAX_EXPORT_TILES {
        return Err(format!(
            "vector.run: {what} would lay {} pattern tiles one by one, more than the {} the door allows one export: enlarge the pattern's tile or export SVG",
            shown(tiles),
            shown(MAX_EXPORT_TILES)
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Before a command runs: what it would make of the document (ADR 0013, #418)
// ---------------------------------------------------------------------------

/// The most segment × shape steps a boolean sweep may take in one command
/// (Pathfinder, Live Paint, Shape Builder, the flattener): the sweep keeps a
/// winding entry per shape for every segment, so this is also its memory.
const MAX_SWEEP: f64 = 2.5e7;
/// The most passes × steps for the Pathfinder operations that sweep once
/// per shape (Trim, Crop, Outline, Merge).
const MAX_SWEEPS: f64 = 1.0e9;
/// The most dot × outline-segment tests a clipped Vector Halftone makes.
const MAX_HALFTONE_TESTS: f64 = 2.0e8;
/// The most point × instance × node steps of the Symbolism tools.
const MAX_SYMBOLISM: f64 = 1.0e9;
/// The most regions Image Trace may outline (image pixels ÷ `noise`, its
/// worst case): each becomes a path of about ten anchors.
const MAX_TRACE_REGIONS: f64 = 500_000.0;
/// The most text a placeholder fills one frame with (the engine's 10,000
/// words), in characters.
const PLACEHOLDER_CHARS: f64 = 55_607.0;
/// The largest frame, in points, the door lays area type into (the engine's
/// own Area Type Options limit): a frame narrower than one glyph steps down
/// its whole height a point at a time.
const MAX_FRAME_SIDE: f64 = 100_000.0;

/// What a set of nodes draws, how many nodes it holds and its longest subpath.
#[derive(Clone, Copy, Debug, Default)]
struct Part {
    drawn: Drawn,
    nodes: f64,
    subpath: f64,
}

/// The [`Part`] `nodes` make in `doc` (each counted as often as listed).
fn part_of<'a>(doc: &'a Document, nodes: impl IntoIterator<Item = &'a Node>) -> Part {
    let mut w = Weigher::new(doc);
    let mut drawn = Drawn::default();
    for n in nodes {
        drawn += w.node(n, 0, 1.0);
    }
    Part { drawn, nodes: w.nodes, subpath: w.subpath }
}

/// The nodes a command acts on: `ids` (as listed, repeats kept: the engine
/// acts once per entry), else `id`, else the selection.
fn targets<'a>(doc: &'a Document, selection: &[vectorcraft_engine::doc::NodeId], p: &Json) -> Vec<&'a Node> {
    let ids: Vec<u64> = match (p.get("ids").and_then(Json::as_array), p.get("id").and_then(Json::as_u64)) {
        (Some(ids), _) => ids.iter().filter_map(Json::as_u64).collect(),
        (None, Some(id)) => vec![id],
        _ => selection.iter().map(|i| i.0).collect(),
    };
    ids.into_iter().filter_map(|i| doc.node(vectorcraft_engine::doc::NodeId(i))).collect()
}

/// The leaves under `nodes` a boolean operation takes (paths and compound
/// paths, type as its glyphs) and their anchor points.
fn shapes_of(nodes: &[&Node]) -> (f64, f64) {
    let (mut shapes, mut anchors) = (0.0, 0.0);
    for n in nodes {
        n.walk(&mut |c| match &c.kind {
            NodeKind::Path { path, .. } => {
                shapes += 1.0;
                anchors += path.anchor_count() as f64;
            }
            NodeKind::Text(t) => {
                let chars = t.runs.iter().map(|r| r.text.chars().count()).sum::<usize>() as f64;
                shapes += chars;
                anchors += chars * GLYPH_ANCHORS;
            }
            _ => {}
        });
    }
    (shapes, anchors)
}

/// The union of `nodes`' visual bounds.
fn bounds_of(nodes: &[&Node]) -> Option<Rect> {
    nodes.iter().filter_map(|n| n.visual_bounds()).reduce(|a, b| a.union(b))
}

/// Characters of type under `nodes`, and how many type objects hold them.
fn type_of(nodes: &[&Node]) -> (f64, f64) {
    let (mut chars, mut objects) = (0.0, 0.0);
    for n in nodes {
        n.walk(&mut |c| {
            if let NodeKind::Text(t) = &c.kind {
                objects += 1.0;
                chars += t.runs.iter().map(|r| r.text.chars().count()).sum::<usize>() as f64;
            }
        });
    }
    (chars, objects)
}

/// How many times a command copies what [`copy_base`] names: its copy
/// limits' factor, or for the commands that copy with no count, 2 (or the
/// clipboard once per artboard for Paste on All Artboards). 1: no copies.
fn copy_factor(s: &Session, door: &Door, id: &str, p: &Json) -> f64 {
    let limited = door.copies(id, p);
    if limited > 1.0 {
        return limited;
    }
    let copy = p.get("copy").and_then(Json::as_bool) == Some(true);
    match id {
        "edit.duplicate" | "path.mirrorCut" | "layer.duplicate" | "artboard.duplicate" => 2.0,
        "edit.paste" | "edit.pasteInPlace" | "edit.pasteInFront" | "edit.pasteInBack" | "edit.pasteWithoutFormatting" => 2.0,
        "edit.pasteOnAllArtboards" => 1.0 + s.doc().map_or(1, |st| st.doc.artboards.len()) as f64,
        "object.move" | "object.rotate" | "object.scale" | "object.reflect" | "object.shear" | "object.transform" | "object.transformEach"
        | "object.nudge" | "object.setBounds" | "perspective.move" | "perspective.nudge" | "perspective.transform" | "layer.move" | "artboard.move"
            if copy =>
        {
            2.0
        }
        "object.transformAgain" if s.doc().is_ok_and(|st| st.last_transform.is_some_and(|(_, copy)| copy)) => 2.0,
        "perspective.plane.move" if p.get("objects").and_then(Json::as_str) == Some("copy") => 2.0,
        _ => 1.0,
    }
}

/// Whether a command's copies are new nodes (live repeats, blends and
/// Transform effects only draw theirs).
fn real_copies(id: &str) -> bool {
    !matches!(id, "object.blend.make" | "object.repeat.grid" | "object.repeat.radial" | "object.repeat.options" | "effect.apply" | "appearance.addEffect" | "effect.setParams")
}

/// What a copying command copies: the clipboard for the Paste commands, the
/// art on an artboard for the artboard commands, every object on a
/// perspective plane (the whole document, as a bound), rows for the Layers
/// panel's commands, else its targets.
fn copy_base(s: &Session, id: &str, p: &Json) -> Part {
    let Ok(st) = s.doc() else { return Part::default() };
    let doc = &st.doc;
    match id {
        _ if id.starts_with("edit.paste") => {
            let mut w = Weigher::new(doc);
            let mut drawn = Drawn::default();
            for n in &s.clipboard.nodes {
                drawn += w.node(n, 0, 1.0);
            }
            Part { drawn, nodes: w.nodes, subpath: w.subpath }
        }
        "artboard.duplicate" | "artboard.move" => {
            let index = p.get("index").and_then(Json::as_u64).map_or(0, |i| i as usize);
            let Some(board) = doc.artboards.get(index).map(|a| a.rect) else { return Part::default() };
            let inside = doc.layers.iter().flat_map(|l| l.children().into_iter().flatten()).filter(|n| n.geometric_bounds().is_some_and(|b| board.contains(b.origin()) || board.intersect(b).area() > 0.0));
            part_of(doc, inside.map(|n| &**n))
        }
        "perspective.plane.move" => part_of(doc, doc.layers.iter().map(|l| &**l)),
        "layer.duplicate" | "layer.move" => {
            let rows = match (p.get("ids").and_then(Json::as_array), p.get("id").and_then(Json::as_u64)) {
                (Some(_), _) | (None, Some(_)) => targets(doc, &[], p),
                _ => {
                    let rows = st.highlighted_rows();
                    let rows = if rows.is_empty() { st.current_layer().into_iter().collect() } else { rows };
                    rows.into_iter().filter_map(|i| doc.node(i)).collect()
                }
            };
            part_of(doc, rows)
        }
        _ => part_of(doc, targets(doc, &st.selection.objects, p)),
    }
}

/// `now` with `extra` copied `times` more over it: drawn measures always,
/// nodes too when the copies are nodes.
fn projected(now: &Weight, extra: &Part, times: f64, nodes: bool) -> Weight {
    let mut w = *now;
    w.objects += extra.drawn.objects * times;
    w.anchors += extra.drawn.anchors * times;
    w.chars += extra.drawn.chars * times;
    if nodes {
        w.nodes += extra.nodes * times;
    }
    w
}

/// Refuse `id` with `amount` over `max`, before it runs.
fn over(id: &str, amount: f64, max: f64, what: &str) -> Result<(), String> {
    if amount > max {
        return Err(format!("vector.run: `{id}` would take {} {what}, more than the {} the door allows one command", shown(amount), shown(max)));
    }
    Ok(())
}

/// The pixels a rasterization of `bounds` at `ppi` makes.
fn raster_pixels(bounds: Rect, ppi: f64) -> f64 {
    let k = ppi / 72.0;
    (bounds.width() * k).ceil().max(1.0) * (bounds.height() * k).ceil().max(1.0)
}

/// Refuse a command, before it runs, whose work the document would make too
/// large for the door: copies of what it copies past the call's ceilings
/// (by its copy limits, or the copies no parameter counts), sweeps that are
/// quadratic in the shapes they take, rasters past the door's pixels, a
/// symbol made to hold itself, a graph axis that never ends.
fn pre_check(s: &Session, door: &Door, id: &str, p: &Json, now: &Weight, ceilings: &[f64; 15]) -> Result<(), String> {
    // No document: the engine refuses the command itself.
    let Ok(st) = s.doc() else { return Ok(()) };
    let doc = &st.doc;
    let with = format!("with `{id}`");
    let factor = copy_factor(s, door, id, p);
    if factor > 1.0 && id != "edit.findReplace" {
        let base = copy_base(s, id, p);
        let after = projected(now, &base, factor - 1.0, real_copies(id));
        check_weight(&after, ceilings, &with)?;
    }
    let targets = || targets(doc, &st.selection.objects, p);
    match id {
        // Every match in every text object grows by the replacement.
        "edit.findReplace" => {
            let growth = replace_growth(p)?.unwrap_or(1.0);
            check_weight(&Weight { chars: now.chars * growth, ..*now }, ceilings, &with)
        }
        // Build: each new sublayer holds copies of the objects up to its own,
        // n(n + 1) / 2 copies of n objects.
        "layer.releaseToLayers" | "layer.releaseToLayersBuild" if id.ends_with("Build") || p.get("build").and_then(Json::as_bool) == Some(true) => {
            let target = match p.get("id").and_then(Json::as_u64) {
                Some(i) => doc.node(vectorcraft_engine::doc::NodeId(i)),
                None => match st.highlighted_rows().as_slice() {
                    [one] => doc.node(*one),
                    _ => st.current_layer().and_then(|l| doc.node(l)),
                },
            };
            let objects: Vec<&Node> = target.and_then(Node::children).into_iter().flatten().filter(|c| !c.is_layer()).map(|c| &**c).collect();
            let n = objects.len();
            let mut copies = Part::default();
            for (j, o) in objects.iter().enumerate() {
                let one = part_of(doc, [*o]);
                let times = n.saturating_sub(1 + j) as f64;
                copies.drawn += one.drawn.scaled(times);
                copies.nodes += one.nodes * times;
            }
            check_weight(&projected(now, &copies, 1.0, true), ceilings, &with)
        }
        // Gradient fills become `steps` strips each; type becomes outlines.
        "object.expand" => {
            let nodes = targets();
            let steps = p.get("steps").and_then(Json::as_f64).unwrap_or(255.0);
            let mut gradients = 0.0;
            if p.get("fill").and_then(Json::as_bool) != Some(false) {
                for n in &nodes {
                    n.walk(&mut |c| gradients += c.appearance.items.iter().filter(|i| matches!(i.paint(), Paint::Gradient(_))).count() as f64);
                }
            }
            let (chars, _) = if p.get("object").and_then(Json::as_bool) != Some(false) { type_of(&nodes) } else { (0.0, 0.0) };
            let extra = Part { drawn: Drawn { objects: gradients * steps + chars, anchors: gradients * steps * 4.0 + chars * GLYPH_ANCHORS, ..Drawn::default() }, nodes: gradients * steps + chars, subpath: 0.0 };
            check_weight(&projected(now, &extra, 1.0, true), ceilings, &with)
        }
        // One path (or compound path) per glyph.
        "type.createOutlines" => {
            let (chars, _) = type_of(&targets());
            let extra = Part { drawn: Drawn { objects: chars, anchors: chars * GLYPH_ANCHORS, ..Drawn::default() }, nodes: chars, subpath: 0.0 };
            check_weight(&projected(now, &extra, 1.0, true), ceilings, &with)
        }
        // An anchor in the middle of every segment: twice the anchors, each
        // insertion shifting the subpath (quadratic in its length).
        "object.path.addAnchorPoints" => {
            let part = part_of(doc, targets());
            check_weight(&Weight { anchors: now.anchors + part.drawn.anchors, subpath: now.subpath.max(2.0 * part.subpath), ..*now }, ceilings, &with)
        }
        // Straight lines skip the guard against more anchors: up to 256 a segment.
        "object.path.simplify" if p.get("straightLines").and_then(Json::as_bool) == Some(true) => {
            let part = part_of(doc, targets());
            check_weight(&Weight { anchors: now.anchors + 255.0 * part.drawn.anchors, ..*now }, ceilings, &with)
        }
        _ if id.starts_with("object.pathfinder.") => {
            let (shapes, segments) = shapes_of(&targets());
            over(id, shapes * segments, MAX_SWEEP, "segment × shape steps")?;
            if matches!(id, "object.pathfinder.trim" | "object.pathfinder.crop" | "object.pathfinder.outline" | "object.pathfinder.merge") {
                over(id, shapes * shapes * segments, MAX_SWEEPS, "sweep steps (one sweep per shape)")?;
            }
            Ok(())
        }
        // A planar map of every path taken: two lists per segment, each with
        // an entry per pair of shapes.
        "livePaint.make" | "livePaint.merge" | "shapeBuilder.merge" | "shapeBuilder.regions" => {
            let (shapes, segments) = shapes_of(&targets());
            over(id, segments * shapes * shapes, MAX_SWEEPS / 32.0, "planar map entries (segments × shapes²)")
        }
        "livePaint.fill" if p.get("group").is_none() => {
            let (shapes, segments) = shapes_of(&targets());
            over(id, segments * shapes * shapes, MAX_SWEEPS / 32.0, "planar map entries (segments × shapes²)")
        }
        "object.flattenTransparency" => {
            let nodes = targets();
            let (shapes, segments) = shapes_of(&nodes);
            over(id, shapes * segments, MAX_SWEEP, "segment × shape steps")?;
            let preset = match p.get("preset").and_then(Json::as_str).map(str::to_ascii_lowercase).as_deref() {
                Some("high") => 1200.0,
                _ => 300.0,
            };
            let ppi = flattener_ppi(p)?.unwrap_or(0.0).max(preset);
            match bounds_of(&nodes) {
                Some(b) => over(id, raster_pixels(b, ppi).min(ENGINE_EFFECT_PIXELS * shapes.max(1.0)), MAX_RASTER_PIXELS, "pixels of flattened regions"),
                None => Ok(()),
            }
        }
        "object.rasterize" => {
            let nodes = targets();
            let ppi = p.get("ppi").and_then(Json::as_f64).unwrap_or(doc.raster_effects_ppi);
            let pad = loose_num(p.get("padding").or_else(|| p.get("addAround"))).unwrap_or(doc.raster_effects.add_around).clamp(0.0, 1000.0);
            match bounds_of(&nodes) {
                Some(b) => over(id, raster_pixels(b.inflate(pad, pad), ppi), MAX_RASTER_PIXELS, "pixels"),
                None => Ok(()),
            }
        }
        // Every dot of a clipped halftone is tested against the united
        // outline: dots (at most 100,000 cells a screen, the engine's) ×
        // outline segments.
        "object.vectorHalftone" if p.get("clip").and_then(Json::as_bool) != Some(false) => {
            let nodes = targets();
            let (_, segments) = shapes_of(&nodes);
            let cell = 72.0 / p.get("frequency").and_then(Json::as_f64).unwrap_or(20.0).clamp(1.0, 300.0);
            let screens = if p.get("mode").and_then(Json::as_str) == Some("cmyk") { 4.0 } else { 1.0 };
            let cells = bounds_of(&nodes).map_or(0.0, |b| ((b.width() + b.height()).powi(2) / 2.0 / (cell * cell)).min(100_000.0));
            over(id, 4.0 * screens * cells * segments, MAX_HALFTONE_TESTS, "dot × outline tests")
        }
        // Raster effects become images at the document's raster effects resolution.
        "effect.expandAppearance" => {
            let nodes = targets();
            let k = (doc.raster_effects_ppi / 72.0).clamp(1.0 / 72.0, 2400.0 / 72.0);
            let add = doc.raster_effects.add_around + 2.0;
            let mut pixels = 0.0;
            for n in &nodes {
                n.walk(&mut |c| {
                    let list = std::iter::once(&c.appearance.effects).chain(c.appearance.items.iter().map(AppearanceItem::effects));
                    let fx: Vec<_> = list.flat_map(|e| vectorcraft_render::effects::raster_effects(e)).collect();
                    if let (false, Some(b)) = (fx.is_empty(), c.visual_bounds()) {
                        let reach = fx.iter().map(|f| f.outset()).fold(0.0, f64::max) + add;
                        pixels += raster_pixels(b.inflate(reach, reach), k * 72.0).min(ENGINE_EFFECT_PIXELS);
                    }
                });
            }
            over(id, pixels, MAX_RASTER_PIXELS, "pixels of raster-effect images")
        }
        "clipboard.exportPng" => match s.clipboard.bounds() {
            Some(b) => over(id, raster_pixels(b, 72.0 * p.get("scale").and_then(Json::as_f64).unwrap_or(1.0)), MAX_RASTER_PIXELS, "pixels"),
            None => Ok(()),
        },
        "document.serialize" => {
            let format = p.get("format").and_then(Json::as_str).unwrap_or("vectorcraft");
            let Ok(f) = vectorcraft_engine::cmd::fileio::writable_format(Some(format), None) else { return Ok(()) };
            let (_, load) = weigh(s)?;
            check_export(doc, &load, f, p, &format!("`{id}` as {}", f.label))
        }
        "document.pdfSettings" if p.get("includeDocument").and_then(Json::as_bool) == Some(true) => {
            let Ok(f) = vectorcraft_engine::cmd::fileio::writable_format(Some("pdf"), None) else { return Ok(()) };
            let (_, load) = weigh(s)?;
            check_export(doc, &load, f, p, &format!("`{id}`"))
        }
        "document.exportForWeb.preview" => {
            let Ok(settings) = s.web_settings(p) else { return Ok(()) };
            let Ok((region, scale)) = vectorcraft_engine::cmd::webexport::region(doc, &settings) else { return Ok(()) };
            over(id, raster_pixels(region, 72.0 * scale), MAX_RASTER_PIXELS, "pixels")
        }
        // The engine builds every cell the guides cut before it counts them.
        "object.slice.fromGuides" => {
            let vertical = doc.guides.iter().filter(|g| g.vertical).count() as f64;
            let horizontal = doc.guides.len() as f64 - vertical;
            over(id, (vertical + 1.0) * (horizontal + 1.0), 1000.0, "slices (the engine's own limit)")
        }
        "guide.move" | "object.nudge" if p.get("copy").and_then(Json::as_bool) == Some(true) => {
            check_weight(&Weight { guides: now.guides + st.selection.guides.len() as f64, ..*now }, ceilings, &with)
        }
        // Text written into every text object targeted.
        "text.setText" | "type.insert" | "type.fillPlaceholder" => {
            let (_, objects) = type_of(&targets());
            let each = if id == "type.fillPlaceholder" { PLACEHOLDER_CHARS } else { chars_of(p, "text").unwrap_or(0.0) };
            check_weight(&Weight { chars: now.chars + each * objects, ..*now }, ceilings, &with)
        }
        "text.editRange" => check_weight(&Weight { chars: now.chars + edit_range_chars(p)?.unwrap_or(0.0), ..*now }, ceilings, &with),
        // Area type laid into an existing path: its frame's size bounds the
        // layout's scan. A placeholder fills the frame (10,000 words at most).
        "text.create" | "text.createInPath" => {
            if id == "text.createInPath" && p.get("mode").and_then(Json::as_str) != Some("onPath") {
                let frame = p.get("path").and_then(Json::as_u64).and_then(|i| doc.node(vectorcraft_engine::doc::NodeId(i)));
                if let Some(b) = frame.and_then(Node::geometric_bounds) {
                    over(id, b.width().max(b.height()), MAX_FRAME_SIDE, "points of frame")?;
                }
            }
            let placeholder = if p.get("placeholder").and_then(Json::as_bool) == Some(true) { PLACEHOLDER_CHARS } else { 0.0 };
            check_weight(&Weight { chars: now.chars + placeholder + chars_of(p, "text").unwrap_or(0.0), ..*now }, ceilings, &with)
        }
        "symbol.update" => {
            let Some(name) = p.get("name").and_then(Json::as_str) else { return Ok(()) };
            if holds_symbol(doc, &targets(), name) {
                return Err(format!("vector.run: `{id}` would make the symbol `{name}` hold an instance of itself, which no renderer can draw"));
            }
            Ok(())
        }
        // At most one instance a point, each drawing the whole symbol.
        "symbol.spray" => {
            let name = p.get("name").and_then(Json::as_str).map(str::to_string).or_else(|| doc.unknown.get("currentSymbol").and_then(Json::as_str).map(str::to_string));
            let points = list_len(p, "points").unwrap_or(0.0);
            let Some(sym) = name.and_then(|n| doc.symbols.iter().find(|s| s.name == n)) else { return Ok(()) };
            let art = part_of(doc, [&*sym.art]);
            check_weight(&projected(now, &art, points, false), ceilings, &with)
        }
        "symbol.adjust" => {
            let selected = targets().iter().filter(|n| matches!(n.kind, NodeKind::SymbolInstance { .. })).count() as f64;
            let candidates = if selected > 0.0 {
                selected
            } else {
                let mut all = 0.0;
                for l in &doc.layers {
                    l.walk(&mut |c| all += f64::from(u8::from(matches!(c.kind, NodeKind::SymbolInstance { .. }))));
                }
                all
            };
            over(id, list_len(p, "points").unwrap_or(0.0) * candidates * now.nodes, MAX_SYMBOLISM, "point × instance × node steps")
        }
        "graph.setType" | "graph.setData" => graph_axes(doc, &st.selection.objects, p).map_err(|e| format!("vector.run: `{id}`: {e}")),
        // Tracing decodes the image whole and outlines every region of at
        // least `noise` pixels (default 25; the built-in presets go down to 4).
        "imageTrace.make" | "imageTrace.makeAndExpand" => {
            let image_of = |n: &Node| match &n.kind {
                NodeKind::Image(i) => Some((i.width, i.height)),
                _ => None,
            };
            let traced = targets().into_iter().find_map(|n| image_of(n).or_else(|| n.children().and_then(|c| c.first()).and_then(|c| image_of(c))));
            let Some((w, h)) = traced else { return Ok(()) };
            let pixels = f64::from(w) * f64::from(h);
            over(id, pixels, MAX_RASTER_PIXELS, "pixels of image to trace")?;
            let noise = p.get("params").and_then(|q| q.get("noise")).and_then(Json::as_f64).unwrap_or(if p.get("preset").is_some() { 4.0 } else { 25.0 });
            over(id, pixels / noise.max(1.0), MAX_TRACE_REGIONS, "traced regions at most (image pixels ÷ noise)")
        }
        _ => Ok(()),
    }
}

/// Whether any of `nodes` (or the art of a symbol one of them places, all
/// the way down) places symbol `name`.
fn holds_symbol(doc: &Document, nodes: &[&Node], name: &str) -> bool {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut stack: Vec<&Node> = nodes.to_vec();
    while let Some(n) = stack.pop() {
        let mut found = false;
        n.walk(&mut |c| {
            if let NodeKind::SymbolInstance { symbol, .. } = &c.kind {
                if symbol == name {
                    found = true;
                } else if let Some(sym) = doc.symbols.iter().find(|s| s.name == *symbol).filter(|s| seen.insert(s.name.as_str())) {
                    stack.push(&sym.art);
                }
            }
        });
        if found {
            return true;
        }
    }
    false
}

/// A graph's value axis as `graph.setType` / `graph.setData` would lay it:
/// the stored `axisMin` and `axisMax` with `p`'s over them. The engine steps
/// from one to the other by a hundredth of the range at most; when that step
/// is below the floating-point resolution of the values (1e17 to 1e17 + 16)
/// the loop never ends, making a tick and a label each turn.
fn graph_axes(doc: &Document, selection: &[vectorcraft_engine::doc::NodeId], p: &Json) -> Result<(), String> {
    let start = p.get("id").and_then(Json::as_u64).map(vectorcraft_engine::doc::NodeId).into_iter().chain(selection.iter().copied());
    let mut spec = None;
    for id in start {
        let mut cur = Some(id);
        while let Some(c) = cur {
            if let Some(g) = doc.node(c).and_then(|n| n.graph.as_deref()) {
                spec = Some(g);
                break;
            }
            cur = doc.parent_of(c);
        }
        if spec.is_some() {
            break;
        }
    }
    let Some(spec) = spec else { return Ok(()) };
    let axis = |key: &str, stored: Option<f64>| match p.get(key) {
        Some(v) => v.as_f64(),
        None => stored,
    };
    let (Some(lo), Some(hi)) = (axis("axisMin", spec.axis_min), axis("axisMax", spec.axis_max)) else { return Ok(()) };
    let range = (hi - lo).abs();
    if !(lo.is_finite() && hi.is_finite()) || (range >= 1e-12 && range < 1e-9 * lo.abs().max(hi.abs())) {
        return Err(format!("the value axis from {lo} to {hi} is too narrow for its values' precision: its ticks would never end"));
    }
    Ok(())
}

/// Which apps may call the service: system apps, as News and Sheets.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct VectorService;

/// Register the `vector` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(VectorService));
}

/// The shell's resolver: where each call works (`None` removes it, and
/// calls work in the legacy `<host dir>/vector` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
}

/// The `vector.*` agent tools (ADR 0013, wave 2), in App Hub's `tools.json`
/// shape: the shell declares them for the virtual owner `os.vector` and grants
/// the system agent its reviewed share (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`).
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for VectorService {
    fn family(&self) -> &'static str {
        "vector"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        reply.send(serve(&AREAS, &call));
    }
}

/// One call, in the area `areas` gives it.
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The vector service serves system apps only.".into());
    }
    if call.method() == "commands" {
        return commands();
    }
    let area = areas.area(call, "vector").map_err(|e| format!("vector: {e}"))?;
    dispatch_in(call.method(), &call.args, &area)
        .map(|answer| area.relative_json(answer))
        .map_err(|error| area.relative_text(&error))
}

fn dispatch_in(method: &str, args: &Json, area: &Area) -> Result<Json, String> {
    match method {
        "info" => info(args, area),
        "convert" => convert(args, area),
        "run" => run(args, area),
        "commands" => commands(),
        "render" => render(args, area),
        other => Err(format!("vector.{other} is not a method of the vector service")),
    }
}

/// [`dispatch_in`] in the legacy area `<host_dir>/vector`, as a call
/// without the shell's resolver works.
#[cfg(test)]
fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    if method == "commands" {
        return commands();
    }
    let area = Area::legacy(host_dir, "vector");
    std::fs::create_dir_all(&area.root).map_err(|e| format!("vector: area: {e}"))?;
    dispatch_in(method, args, &area)
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
        return Err("vector: paths stay inside this call's folder".into());
    }
    let joined = area.join(rel_path);
    let check_root = area.canonicalize().map_err(|e| format!("vector: folder: {e}"))?;
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
        return Err("vector: paths stay inside this call's folder".into());
    }
    Ok(joined)
}

/// A contained output path the call may write, refused before the engine
/// works when the area's rules would refuse it.
fn out_path(area: &Area, rel: &str) -> Result<PathBuf, String> {
    let path = contained_path(&area.root, rel)?;
    area.check(&path, 0).map_err(|e| format!("vector: {e}"))?;
    Ok(path)
}

/// Whether `p`, a path a document names, is a file of the area: absolute,
/// with no `..`, its deepest existing ancestor inside the canonical root
/// (through links). A relative one the engine would resolve against the
/// process's working folder, which is no folder of the caller's.
fn inside(root: &Path, p: &Path) -> bool {
    if !p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)) {
        return false;
    }
    let mut deepest = p.to_path_buf();
    while !deepest.exists() {
        match deepest.parent() {
            Some(parent) => deepest = parent.to_path_buf(),
            None => return false,
        }
    }
    deepest.canonicalize().is_ok_and(|real| real.starts_with(root))
}

/// The files the engine looks for a document's linked images at when it
/// opens it from `dir` (vectorcraft's `links::resolve`): each link's own
/// path, its path relative to the document's folder, and its name there.
fn link_files(doc: &vectorcraft_engine::doc::Document, dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    doc.visit_images(|_, image| {
        if let Some(link) = &image.link {
            out.push(PathBuf::from(&link.path));
            if let Some(rel) = &link.relative {
                out.push(dir.join(rel));
            }
            out.push(dir.join(link.name()));
        }
    });
    out
}

/// Refuse the document at `path` (already contained) unless every file the
/// engine would read for its linked images, opening it, is in the area. The
/// engine reads them with no gate of its own, so they are found first, with
/// the engine's own code and nothing read: an SVG's `<image>` links by its
/// importer, run with a reader that records the paths it is asked for; a
/// native document's links (one an SVG carries as its editing data
/// included, which the engine opens instead of the SVG) from the document
/// itself, as the engine's loader makes it for every other format.
fn fence_links(area: &Area, path: &Path, rel: &str, method: &str) -> Result<(), String> {
    let root = area.root.canonicalize().map_err(|e| format!("vector.{method}: folder: {e}"))?;
    let bytes = std::fs::read(path).map_err(|e| format!("vector.{method}: {rel}: {e}"))?;
    let name = utf8(path)?;
    let dir = path.parent().unwrap_or(&area.root);
    let mut wanted: Vec<PathBuf> = Vec::new();
    let format = vectorcraft_engine::cmd::fileio::detect(name, &bytes).map(|f| f.id);
    let postscript = bytes.starts_with(b"%!PS") || bytes.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6]);
    if matches!(format, Some("svg" | "svgz")) && !postscript {
        let text = vectorcraft_svg::text_of(&bytes).map_err(|e| format!("vector.{method}: {rel}: {e}"))?;
        let asked = std::cell::RefCell::new(Vec::new());
        let record = |p: &str| {
            asked.borrow_mut().push(PathBuf::from(p));
            None
        };
        let folder = dir.to_string_lossy();
        let _ = vectorcraft_svg::import_with(&text, &vectorcraft_svg::ImportOptions { folder: Some(&folder), read: Some(&record) });
        wanted.extend(asked.into_inner());
        if let Some(editing) = vectorcraft_svg::editing(&text).filter(|e| e.intact) {
            if let Some(doc) = vectorcraft_format::base64_decode(&editing.data).and_then(|b| vectorcraft_format::load_file(&b).ok()) {
                wanted.extend(link_files(&doc.doc, dir));
            }
        }
    } else if let Ok(loaded) = vectorcraft_engine::cmd::fileio::load_with(name, &bytes, &Default::default()) {
        wanted.extend(link_files(&loaded.doc, dir));
    }
    if let Some(out) = wanted.iter().find(|p| !inside(&root, p)) {
        return Err(format!(
            "vector.{method}: `{rel}` links `{}`, outside this call's folder: the engine would read it from there; embed the images it links",
            out.display()
        ));
    }
    Ok(())
}

fn arg_str<'a>(args: &'a Json, key: &str) -> Result<&'a str, String> {
    args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("vector: `{key}` is required"))
}

fn utf8(path: &Path) -> Result<&str, String> {
    path.to_str().ok_or_else(|| "vector: the host directory is not UTF-8".into())
}

/// Open `rel` (contained, size-capped, its links fenced) into the session;
/// the engine's result carries the import warnings.
fn open(session: &mut Session, area: &Area, rel: &str, method: &str) -> Result<Json, String> {
    let path = contained_path(&area.root, rel)?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("vector.{method}: {rel}: {e}"))?;
    if meta.len() > MAX_OPEN_BYTES {
        return Err(format!("vector.{method}: the file is larger than the service opens"));
    }
    fence_links(area, &path, rel, method)?;
    session.execute("document.open", &json!({"path": utf8(&path)?})).map_err(|e| format!("vector.{method}: {e}"))
}

/// `document.export` to `out`, which the engine writes itself (a
/// multi-artboard SVG export also writes `{stem}-{n}.svg` siblings): into a
/// staging folder inside the area, then every file beside `out`, all or
/// none, under the area's rules; the result's own paths made relative
/// again.
/// The engine's `document.export` params for a call's `format`, `scale`
/// (0.01–16 pixels per point) and `artboard`, without the path.
fn export_params(args: &Json) -> Json {
    let mut params = json!({});
    if let Some(f) = args["format"].as_str() {
        params["format"] = json!(f);
    }
    if let Some(s) = args["scale"].as_f64() {
        params["scale"] = json!(s.clamp(0.01, 16.0));
    }
    // A raster export writes one artboard: this one (0-based, default 0).
    if let Some(a) = args["artboard"].as_u64() {
        params["artboard"] = json!(a);
    }
    params
}

fn export(session: &mut Session, area: &Area, args: &Json, out: &Path, method: &str) -> Result<Json, String> {
    let out_rel = arg_str(args, "out")?;
    let stage = area.stage().map_err(|e| format!("vector.{method}: {e}"))?;
    let staged = stage.path(out.file_name().unwrap_or_default());
    let mut params = export_params(args);
    params["path"] = json!(utf8(&staged)?);
    let saved = session.execute("document.export", &params).map_err(|e| format!("vector.{method}: {e}"))?;
    let beside = out.parent().unwrap_or(&area.root);
    let moves: Vec<(PathBuf, PathBuf)> = stage.files().into_iter().map(|f| (stage.path(&f), beside.join(f))).collect();
    stage.commit(&moves).map_err(|e| format!("vector.{method}: {e}"))?;
    let rel_dir = Path::new(out_rel).parent().unwrap_or(Path::new(""));
    let rel_files = saved["files"].as_array().map(|files| {
        files
            .iter()
            .filter_map(Json::as_str)
            .map(|f| json!(Path::new(f).file_name().map(|n| rel_dir.join(n).to_string_lossy().into_owned()).unwrap_or_else(|| f.to_string())))
            .collect::<Vec<_>>()
    });
    let mut v = json!({"out": out_rel, "format": saved["format"], "bytes": saved["bytes"], "warnings": saved["warnings"]});
    if let Some(files) = rel_files {
        v["files"] = Json::Array(files);
    }
    Ok(v)
}

fn info(args: &Json, area: &Area) -> Result<Json, String> {
    let rel = arg_str(args, "path")?;
    let mut s = Session::new();
    let opened = open(&mut s, area, rel, "info")?;
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

fn convert(args: &Json, area: &Area) -> Result<Json, String> {
    let rel = arg_str(args, "path")?;
    let out = out_path(area, arg_str(args, "out")?)?;
    let mut s = Session::new();
    let opened = open(&mut s, area, rel, "convert")?;
    let mut v = export(&mut s, area, args, &out, "convert")?;
    v["warnings"] = json!([opened["warnings"], v["warnings"].clone()]);
    Ok(v)
}

/// After a command runs, refuse the call if any file the engine would read
/// for the live document's linked images is outside the area. The open-time
/// [`fence_links`] checks the file on disk; this checks the document a
/// command just changed, so a command cannot plant a link the export would
/// then read from elsewhere (e.g. `clipboard.importSvg` of an `<image href>`
/// that points out, then `edit.pasteInPlace`). Reuses the open-time fence's
/// [`link_files`] and [`inside`] on the session's current document; relative
/// links resolve against `dir` (the opened document's folder, else the area
/// root), and `inside` refuses any that is not an existing file in the area.
fn fence_live_links(area: &Area, s: &Session, dir: &Path, method: &str) -> Result<(), String> {
    let Ok(st) = s.doc() else { return Ok(()) };
    let root = area.root.canonicalize().map_err(|e| format!("vector.{method}: folder: {e}"))?;
    if let Some(out) = link_files(&st.doc, dir).iter().find(|p| !inside(&root, p)) {
        return Err(format!(
            "vector.{method}: a command set an image link `{}`, outside this call's folder: the engine would read it from there; embed the images instead",
            out.display()
        ));
    }
    Ok(())
}

fn run(args: &Json, area: &Area) -> Result<Json, String> {
    // Admit every command first: one refused id refuses the whole call, with
    // nothing opened and nothing written.
    let door = door()?;
    let admitted = door.admit_all(&args["cmds"], area)?;
    // A name the call may not write is refused before the engine works, and
    // so is a format it cannot write.
    let out = match args["out"].as_str().filter(|o| !o.is_empty()) {
        Some(rel) => {
            let path = out_path(area, rel)?;
            let format = vectorcraft_engine::cmd::fileio::writable_format(args["format"].as_str(), Some(utf8(&path)?)).map_err(|e| format!("vector.run: {e}"))?;
            Some((path, format))
        }
        None => None,
    };
    let mut s = Session::new();
    // The folder relative links resolve against: the opened document's (its
    // own links were fenced at open), else the area root for a fresh one.
    let dir = match args["path"].as_str().filter(|p| !p.is_empty()) {
        Some(rel) => {
            open(&mut s, area, rel, "run")?;
            contained_path(&area.root, rel).ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(|| area.root.clone())
        }
        // No input: a fresh default document, ready to draw into.
        None => {
            s.execute("file.new", &json!({})).map_err(|e| format!("vector.run: {e}"))?;
            area.root.clone()
        }
    };
    // What the call may grow the document to: the door's ceilings, or what
    // the document was opened with (ADR 0013, #418). A door call keeps a few
    // undo steps, not one copy of the document per command.
    if let Ok(st) = s.doc_mut() {
        st.history.limit = UNDO_STEPS;
    }
    let (opened, _) = weigh(&s)?;
    let ceilings = opened.ceilings();
    let mut now = opened;
    let mut results = Vec::with_capacity(admitted.len());
    for (id, params) in &admitted {
        // What the command would make of the document, before it runs.
        pre_check(&s, door, id, params, &now, &ceilings)?;
        let r = s.execute(id, params).map_err(|e| format!("vector.run {id}: {e}"))?;
        // No command may plant a link the export would read from outside.
        fence_live_links(area, &s, &dir, "run")?;
        // What the document holds now.
        now = weigh(&s)?.0;
        check_weight(&now, &ceilings, &format!("after `{id}`"))?;
        results.push(json!({"id": id, "result": r}));
    }
    match out {
        // The staged export's fields, flat, with the command results.
        Some((out, format)) => {
            // What the export renders, before it renders anything.
            if let Ok(st) = s.doc() {
                let (_, load) = weigh(&s)?;
                check_export(&st.doc, &load, format, &export_params(args), &format!("exporting `{}`", arg_str(args, "out")?))?;
            }
            let mut v = export(&mut s, area, args, &out, "run")?;
            if let Some(obj) = v.as_object_mut() {
                obj.insert("results".into(), Json::Array(results));
            }
            Ok(v)
        }
        None => Ok(json!({"results": results, "out": Json::Null})),
    }
}

/// The ids `run` accepts: the engine's catalog filtered to what the door
/// runs (every `safe` id; no `file`, `code`, `host` or unknown id).
fn commands() -> Result<Json, String> {
    let door = door()?;
    let list: Vec<Json> = Session::new()
        .commands()
        .iter()
        .filter(|c| door.runs(c.id))
        .map(|c| serde_json::to_value(c).unwrap_or_default())
        .collect();
    Ok(Json::Array(list))
}

fn render(args: &Json, area: &Area) -> Result<Json, String> {
    let rel = arg_str(args, "path")?;
    let out_rel = arg_str(args, "out")?;
    let max_side = args["max_side"].as_u64().unwrap_or(1024).clamp(1, MAX_RENDER_SIDE as u64) as u32;
    let out = out_path(area, out_rel)?;
    let mut s = Session::new();
    open(&mut s, area, rel, "render")?;
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
    area.write(&out, &png).map_err(|e| format!("vector.render: {e}"))?;
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
        let area = host.join("vector");
        std::fs::create_dir_all(&area).unwrap();
        std::fs::write(area.join("in.svg"), SVG).unwrap();
        "in.svg".into()
    }

    /// A call as App Hub hands it to the service.
    fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("vector.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
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

    fn no_staging_left(root: &Path) -> bool {
        std::fs::read_dir(root).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with(octosense_engine_area::STAGING_PREFIX))
    }

    /// A 12x8 RGB PNG (two colour bands).
    fn png() -> Vec<u8> {
        const HEX: &str = "89504e470d0a1a0a0000000d494844520000000c000000080802000000428689a60000001d49444154789c6378616383866c725ea021063a2bb279d14310d15911005b9497817c6155610000000049454e44ae426082";
        (0..HEX.len()).step_by(2).map(|i| u8::from_str_radix(&HEX[i..i + 2], 16).unwrap()).collect()
    }

    /// With the shell's resolver every path is relative to the caller's own
    /// folder and stays inside it, through a link too; an export the engine
    /// writes itself lands where it was asked.
    #[test]
    fn the_resolver_root_is_used_and_paths_stay_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("in.svg"), SVG).unwrap();
        let areas = resolver(&root, None);
        let host = dir.path().join(".host");
        let doc = serve(&areas, &service_call("info", json!({"path": "in.svg"}), &host, false)).unwrap();
        assert_eq!(doc["path"], json!("in.svg"), "{doc}");
        serve(&areas, &service_call("convert", json!({"path": "in.svg", "out": "out/again.svg"}), &host, false)).unwrap();
        serve(&areas, &service_call("render", json!({"path": "in.svg", "out": "out/p.png", "max_side": 32}), &host, false)).unwrap();
        assert!(root.join("out/again.svg").is_file() && root.join("out/p.png").is_file());
        assert!(!host.exists() && !root.join("vector").exists() && no_staging_left(&root));
        std::fs::write(dir.path().join("beside.svg"), SVG).unwrap();
        for bad in ["../beside.svg", "/etc/hosts", "out/../../beside.svg"] {
            assert!(serve(&areas, &service_call("info", json!({"path": bad}), &host, false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("convert", json!({"path": "in.svg", "out": bad}), &host, true)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            assert!(serve(&areas, &service_call("info", json!({"path": "up/beside.svg"}), &host, false)).is_err());
            assert!(serve(&areas, &service_call("render", json!({"path": "in.svg", "out": "up/p.png"}), &host, true)).is_err());
            assert!(serve(&areas, &service_call("convert", json!({"path": "in.svg", "out": "up/c.svg"}), &host, true)).is_err());
            assert!(!dir.path().join("p.png").exists() && !dir.path().join("c.svg").exists());
        }
    }

    /// The ids a fresh engine session reports as installed plug-ins (empty
    /// unless a plug-in got installed), read through the engine's own
    /// `plugin.list`, never through the door.
    fn installed_plugins() -> Vec<String> {
        let listed = Session::new().execute("plugin.list", &json!({})).unwrap();
        listed["plugins"].as_array().unwrap().iter().filter_map(|p| p["id"].as_str().map(str::to_string)).collect()
    }

    /// With the shell's resolver installed the door now runs: an allowlisted
    /// run draws shapes on a fresh document, applies a built-in effect with
    /// `effect.apply`, and exports to `out`, the file landing inside the
    /// area; a run on an existing SVG works too.
    #[test]
    fn the_door_runs_allowlisted_commands_in_its_area() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let made = serve(
            &areas,
            &service_call(
                "run",
                json!({"cmds": [
                    {"id": "shape.rectangle", "params": {"x": 4.0, "y": 4.0, "width": 24.0, "height": 16.0}},
                    {"id": "effect.apply", "params": {"effect": "stylize.dropShadow"}},
                    {"id": "shape.ellipse", "params": {"x": 30.0, "y": 8.0, "width": 16.0, "height": 16.0}},
                    {"id": "document.inspect", "params": {"depth": 0}}
                ], "out": "art/drawn.svg"}),
                dir.path(),
                false,
            ),
        )
        .unwrap();
        assert_eq!(made["out"], json!("art/drawn.svg"), "{made}");
        assert_eq!(made["format"], json!("svg"));
        assert_eq!(made["results"].as_array().unwrap().len(), 4);
        assert_eq!(made["results"][1]["id"], json!("effect.apply"), "{made}");
        assert!(made["results"][3]["result"]["objects"].as_u64().unwrap() >= 2, "{made}");
        assert!(dir.path().join("art/drawn.svg").is_file() && no_staging_left(dir.path()));
        // An existing SVG, edited and rasterised to PNG beside it.
        std::fs::write(dir.path().join("in.svg"), SVG).unwrap();
        let edited = serve(
            &areas,
            &service_call("run", json!({"path": "in.svg", "cmds": [{"id": "shape.rectangle", "params": {"x": 1.0, "y": 1.0, "width": 8.0, "height": 8.0}}], "out": "out.png"}), dir.path(), false),
        )
        .unwrap();
        assert_eq!(edited["format"], json!("png"), "{edited}");
        assert!(std::fs::read(dir.path().join("out.png")).unwrap().starts_with(b"\x89PNG"));
        // A query with no `out` writes nothing and reports a null `out`.
        let query = serve(&areas, &service_call("run", json!({"path": "in.svg", "cmds": [{"id": "document.inspect", "params": {"depth": 0}}]}), dir.path(), false)).unwrap();
        assert!(query["out"].is_null() && query["results"][0]["result"]["objects"].as_u64().is_some(), "{query}");
    }

    /// Every class but `safe` present in vector's classification is refused,
    /// and so is an id the classification does not know, before any command
    /// runs: a refused id anywhere in the list writes nothing.
    #[test]
    fn the_door_refuses_every_other_class_and_unknown_ids() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        // A representative of each refused class in safety.json.
        for (id, class) in [
            ("document.open", "file"),       // file, not reviewed
            ("swatch.library.save", "file"), // file, not reviewed
            ("command.batch", "code"),       // runs other commands
            ("prefs.set", "code"),           // can load a plug-ins folder
            ("plugin.install", "code"),      // installs WebAssembly
            ("view.proofSetup", "host"),     // process-wide render proof
            ("file.new", "host"),            // records a recent-size preference
        ] {
            let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "shape.rectangle", "params": {"x": 0.0, "y": 0.0, "width": 4.0, "height": 4.0}}, {"id": id}], "out": "x.svg"}), dir.path(), false)).unwrap_err();
            if class == "file" {
                assert!(e.contains("not reviewed to run through it"), "{id}: {e}");
            } else {
                assert!(e.contains(&format!("`{id}` is classed {class}")), "{id}: {e}");
            }
        }
        let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "vector.secret"}]}), dir.path(), false)).unwrap_err();
        assert!(e.contains("not a reviewed vector command"), "{e}");
        assert!(!dir.path().join("x.svg").exists(), "a refused id anywhere wrote nothing");
        let too_many: Vec<Json> = (0..65).map(|_| json!({"id": "shape.rectangle", "params": {"x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0}})).collect();
        assert!(serve(&areas, &service_call("run", json!({"cmds": too_many}), dir.path(), false)).unwrap_err().contains("at most 64"));
    }

    /// #418's three routes past the old deny-list are all refused by the
    /// gate, and none installs a plug-in: (a) `command.batch` wrapping
    /// `plugin.install`; (b) `prefs.set` of a plug-ins folder; (c)
    /// `effect.apply` / `appearance.addEffect` of a `plugin.<id>` effect.
    #[test]
    fn the_door_refuses_418s_three_hostile_fixtures() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        // A folder with a .wasm, for the prefs.set fixture.
        let plugins = dir.path().join("plugins");
        std::fs::create_dir(&plugins).unwrap();
        std::fs::write(plugins.join("evil.wasm"), b"\x00asm\x01\x00\x00\x00").unwrap();
        let fixtures = [
            // (a) a batch wrapping an install of a tiny valid module header.
            json!({"id": "command.batch", "params": {"commands": [{"command": "plugin.install", "params": {"dataBase64": "AGFzbQEAAAA="}}]}}),
            // (b) the Additional Plug-ins Folder preference.
            json!({"id": "prefs.set", "params": {"key": "pluginsFolder", "value": plugins.to_string_lossy()}}),
            // (c) an effect plug-in, through either command and key.
            json!({"id": "effect.apply", "params": {"effect": "plugin.anything"}}),
            json!({"id": "effect.apply", "params": {"id": "plugin.anything"}}),
            json!({"id": "appearance.addEffect", "params": {"effect": "plugin.anything"}}),
            json!({"id": "appearance.addEffect", "params": {"id": "plugin.anything"}}),
        ];
        for cmd in fixtures {
            let label = cmd.to_string();
            let refused = serve(&areas, &service_call("run", json!({"cmds": [cmd], "out": "x.svg"}), dir.path(), false)).unwrap_err();
            assert!(refused.contains("classed code") || refused.contains("is not an effect the engine builds in"), "{label}: {refused}");
            assert!(!dir.path().join("x.svg").exists(), "{label}: wrote nothing");
            assert!(installed_plugins().is_empty(), "{label}: no plug-in installed");
        }
    }

    /// `perspective.draw` admits only a named `shape.*` command, which passes
    /// the gate itself (its own params admitted); any other inner id is
    /// refused before the engine runs. (Whether the engine then attaches the
    /// shape to the grid is its own perspective geometry, not the gate's.)
    #[test]
    fn perspective_draw_runs_only_shape_commands() {
        let dir = tempfile::tempdir().unwrap();
        let area = Area::new(dir.path(), None, false);
        let door = door().unwrap();
        // The named shape command passes the gate, so its params are admitted
        // and handed back under `params` for the engine to run.
        let ok = door.admit("perspective.draw", &json!({"command": "shape.rectangle", "params": {"x": 2.0, "y": 2.0, "width": 10.0, "height": 10.0}}), &area).unwrap();
        assert_eq!(ok["command"], json!("shape.rectangle"), "{ok}");
        assert_eq!(ok["params"]["width"], json!(10.0), "{ok}");
        // Any other inner id is refused, whatever its own class.
        let areas = resolver(dir.path(), None);
        for inner in ["plugin.install", "document.open", "command.batch", "file.new", "object.group"] {
            let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "perspective.draw", "params": {"command": inner, "params": {}}}]}), dir.path(), false)).unwrap_err();
            assert!(e.contains("runs only `shape.*` commands"), "{inner}: {e}");
        }
    }

    /// The post-command link fence: a `safe` command can plant an image link
    /// pointing outside the area (`clipboard.importSvg` of an `<image href>`
    /// that climbs out, then `edit.pasteInPlace`); the call is refused after
    /// that command, before any export reads the link.
    #[test]
    fn a_command_that_plants_an_outside_link_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let outside = dir.path().join("private.png");
        std::fs::write(&outside, png()).unwrap();
        let areas = resolver(&root, None);
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><image href="{}" width="12" height="8"/></svg>"##,
            outside.display()
        );
        let refused = serve(
            &areas,
            &service_call(
                "run",
                json!({"cmds": [
                    {"id": "clipboard.importSvg", "params": {"svg": svg}},
                    {"id": "edit.pasteInPlace"}
                ], "out": "c.svg"}),
                &root,
                false,
            ),
        )
        .unwrap_err();
        assert!(refused.contains("private.png") && refused.contains("outside this call's folder"), "{refused}");
        assert!(!root.join("c.svg").exists() && no_staging_left(&root), "nothing written");
        // The same objects whose images are embedded (a data: URL) pass:
        // no link is planted.
        let data_svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10" viewBox="0 0 20 10"><rect x="2" y="2" width="8" height="6" fill="#3366cc"/></svg>"##;
        serve(&areas, &service_call("run", json!({"cmds": [{"id": "clipboard.importSvg", "params": {"svg": data_svg}}, {"id": "edit.pasteInPlace"}], "out": "ok.svg"}), &root, false)).unwrap();
        assert!(root.join("ok.svg").is_file());
    }

    /// The door's gate is built from the generated classification and the
    /// reviewed inner ids; it runs every `safe` id and no other, and the
    /// built-in effect catalogue it admits is the engine's own.
    #[test]
    fn the_door_is_built_from_the_reviewed_classification() {
        let door = door().unwrap();
        assert!(door.runs("shape.rectangle") && door.runs("effect.apply") && door.runs("appearance.addEffect") && door.runs("perspective.draw"));
        assert!(door.runs("document.inspect") && door.runs("edit.pasteInPlace") && door.runs("clipboard.importSvg"));
        assert!(!door.runs("document.open") && !door.runs("command.batch") && !door.runs("prefs.set") && !door.runs("plugin.install"));
        assert!(!door.runs("view.proofSetup") && !door.runs("file.new") && !door.runs("vector.secret"));
        assert!(door.runnable().len() > 500, "{}", door.runnable().len());
        // The built-in effect catalogue the inner rule admits, computed from
        // a fresh session's effect.list, holds real built-ins and no plug-in.
        let effects = builtin_effects();
        assert!(effects.len() >= 34, "{} built-in effects", effects.len());
        assert!(builtin_effect("stylize.dropShadow") && builtin_effect("warp.arc") && builtin_effect("blur.gaussian"));
        assert!(!builtin_effect("plugin.anything") && !builtin_effect(""));
    }

    /// An agent's call never replaces a file, before the engine works; an
    /// app's own foreground call may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("in.svg"), SVG).unwrap();
        let areas = resolver(dir.path(), None);
        std::fs::write(dir.path().join("taken.png"), b"keep me").unwrap();
        for (method, args) in [
            ("convert", json!({"path": "in.svg", "out": "taken.png"})),
            ("render", json!({"path": "in.svg", "out": "taken.png"})),
            ("convert", json!({"path": "in.svg", "out": "in.svg"})),
        ] {
            let refused = serve(&areas, &service_call(method, args, dir.path(), false)).unwrap_err();
            assert!(refused.contains("already exists"), "{method}: {refused}");
        }
        // The command door's own export keeps the same rule (past the
        // shell's hold on it).
        let run = json!({"path": "in.svg", "cmds": [], "out": "taken.png"});
        let area = areas.area(&service_call("run", run.clone(), dir.path(), false), "vector").unwrap();
        assert!(dispatch_in("run", &run, &area).unwrap_err().contains("already exists"));
        assert_eq!(std::fs::read(dir.path().join("taken.png")).unwrap(), b"keep me");
        assert!(no_staging_left(dir.path()));
        serve(&areas, &service_call("convert", json!({"path": "in.svg", "out": "taken.png"}), dir.path(), true)).unwrap();
        assert!(std::fs::read(dir.path().join("taken.png")).unwrap().starts_with(b"\x89PNG"));
    }

    /// What a call writes, the engine's own export included, must fit what
    /// is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("in.svg"), SVG).unwrap();
        let tight = resolver(dir.path(), Some(32));
        for (method, args) in [
            ("convert", json!({"path": "in.svg", "out": "c.png"})),
            ("render", json!({"path": "in.svg", "out": "r.png"})),
        ] {
            let refused = serve(&tight, &service_call(method, args, dir.path(), true)).unwrap_err();
            assert!(refused.contains("bytes left"), "{method}: {refused}");
        }
        assert!(!dir.path().join("c.png").exists() && !dir.path().join("r.png").exists() && no_staging_left(dir.path()));
    }

    /// An SVG whose `<image>` links a picture outside the caller's folder
    /// (an absolute path, a `file://` URL, or a relative path that climbs
    /// out) is refused before the engine opens it; one that links a picture
    /// beside it, inside the folder, opens.
    #[test]
    fn an_svg_that_links_a_file_outside_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let outside = dir.path().join("private.png");
        std::fs::write(&outside, png()).unwrap();
        std::fs::write(root.join("pic.png"), png()).unwrap();
        let svg = |href: &str| format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><image href="{href}" width="12" height="8"/><rect x="20" y="2" width="10" height="10" fill="#3366cc"/></svg>"##);
        let areas = resolver(&root, None);
        for (n, href) in [outside.display().to_string(), format!("file://{}", outside.display()), "../private.png".into()].into_iter().enumerate() {
            let name = format!("linked{n}.svg");
            std::fs::write(root.join(&name), svg(&href)).unwrap();
            for (method, args) in [
                ("info", json!({"path": name})),
                ("convert", json!({"path": name, "out": format!("c{n}.png")})),
                ("render", json!({"path": name, "out": format!("r{n}.png")})),
            ] {
                let refused = serve(&areas, &service_call(method, args, &root, false)).unwrap_err();
                assert!(refused.contains("private.png") && refused.contains("outside"), "{href} {method}: {refused}");
            }
        }
        std::fs::write(root.join("local.svg"), svg("pic.png")).unwrap();
        let ok = serve(&areas, &service_call("info", json!({"path": "local.svg"}), &root, false)).unwrap();
        assert_eq!(ok["path"], json!("local.svg"), "{ok}");
        serve(&areas, &service_call("render", json!({"path": "local.svg", "out": "local.png", "max_side": 16}), &root, false)).unwrap();
    }

    /// A native document whose linked image lives outside the caller's
    /// folder is refused before the engine opens it (the engine would read
    /// the link from wherever it points); one whose link is inside opens.
    #[test]
    fn a_native_document_that_links_a_file_outside_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let outside = dir.path().join("private.png");
        std::fs::write(&outside, png()).unwrap();
        std::fs::write(root.join("pic.png"), png()).unwrap();
        for (name, picture) in [("outside.vectorcraft", outside.clone()), ("inside.vectorcraft", root.join("pic.png"))] {
            let mut s = Session::new();
            s.execute("file.new", &json!({})).unwrap();
            s.execute("file.place", &json!({"path": picture.to_string_lossy(), "link": true})).unwrap();
            s.execute("document.export", &json!({"path": root.join(name).to_string_lossy()})).unwrap();
        }
        let areas = resolver(&root, None);
        let refused = serve(&areas, &service_call("info", json!({"path": "outside.vectorcraft"}), &root, false)).unwrap_err();
        assert!(refused.contains("private.png") && refused.contains("outside"), "{refused}");
        assert!(serve(&areas, &service_call("convert", json!({"path": "outside.vectorcraft", "out": "o.svg"}), &root, false)).is_err());
        let ok = serve(&areas, &service_call("info", json!({"path": "inside.vectorcraft"}), &root, false)).unwrap();
        assert_eq!(ok["path"], json!("inside.vectorcraft"), "{ok}");
        // The same document carried as an SVG's (or a PDF's) editing data,
        // which the engine opens in place of the SVG, is refused alike.
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        s.execute("file.place", &json!({"path": outside.to_string_lossy(), "link": true})).unwrap();
        for name in ["editing.svg", "editing.pdf"] {
            s.execute("document.export", &json!({"path": root.join(name).to_string_lossy(), "preserveEditing": true})).unwrap();
            let refused = serve(&areas, &service_call("info", json!({"path": name}), &root, false)).unwrap_err();
            assert!(refused.contains("private.png") && refused.contains("outside"), "{name}: {refused}");
        }
        assert!(std::fs::read_to_string(root.join("editing.svg")).unwrap().contains(vectorcraft_svg::EDITING_NS), "the SVG carries its editing data");
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

        // The engine's file commands are the host's: the gate refuses them
        // (every one is classed `file` or `host`, none reviewed).
        for refused in [
            json!({"id": "document.open", "params": {}}),
            json!({"id": "document.export", "params": {}}),
            json!({"id": "file.recovery.list"}),
            json!({"id": "swatch.library.save", "params": {"name": "x"}}),
        ] {
            let r = dispatch("run", &json!({"path": input, "cmds": [refused]}), host);
            assert!(r.is_err(), "{refused}");
        }
        // A `safe` drawing command carrying an extra path-like key is a
        // harmless no-op: the engine ignores the key, the door needs no
        // path fence because a `safe` command touches no file.
        let ok = dispatch("run", &json!({"path": input, "cmds": [{"id": "shape.rectangle", "params": {"x": 0.0, "y": 0.0, "width": 5.0, "height": 5.0, "path": "up.svg"}}]}), host).unwrap();
        assert_eq!(ok["results"][0]["id"], json!("shape.rectangle"), "{ok}");
        assert!(ok["out"].is_null(), "no out: {ok}");
    }

    /// The plug-in registry is process-wide and installs WebAssembly from
    /// in-band data: the gate refuses every `plugin.*` id (classed `code`)
    /// before the engine sees it, and the bare `plugin` id is unknown. The
    /// catalog offer matches.
    #[test]
    fn run_refuses_plugin_commands_and_the_catalog_omits_them() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        for (cmd, needle) in [
            // A core module's header: refused before anything parses it.
            (json!({"id": "plugin.install", "params": {"dataBase64": "AGFzbQEAAAA="}}), "classed code"),
            (json!({"id": "plugin.list"}), "classed code"),
            (json!({"id": "plugin.remove", "params": {"id": "org.vectorcraft.example.desaturate"}}), "classed code"),
            (json!({"id": "plugin.reload"}), "classed code"),
            (json!({"id": "plugin"}), "not a reviewed vector command"),
        ] {
            let r = dispatch("run", &json!({"cmds": [cmd.clone()]}), host);
            let e = r.expect_err(&cmd.to_string());
            assert!(e.contains(needle), "{cmd}: {e}");
        }
        let cat = commands().unwrap();
        let ids: Vec<&str> = cat.as_array().unwrap().iter().filter_map(|c| c["id"].as_str()).collect();
        assert!(ids.iter().all(|id| *id != "plugin" && !id.starts_with("plugin.")), "no plug-in commands offered");
    }

    #[test]
    fn commands_catalog_is_real_and_only_runnable_ids() {
        let cat = commands().unwrap();
        let cat = cat.as_array().unwrap();
        assert!(cat.len() > 200, "a real catalog, {} commands", cat.len());
        let ids: Vec<&str> = cat.iter().filter_map(|c| c["id"].as_str()).collect();
        assert!(ids.contains(&"document.inspect") && ids.contains(&"shape.rectangle") && ids.contains(&"effect.apply"));
        // A `safe`, session-only `file.*` command is offered; the `file`-class
        // access commands and the plug-in system are not.
        assert!(ids.contains(&"file.close"), "a session-only file command is offered");
        assert!(!ids.contains(&"file.saveAs") && !ids.contains(&"file.place"), "file access is not offered");
        assert!(!ids.contains(&"document.open") && !ids.contains(&"document.export"));
        assert!(ids.iter().all(|id| !id.starts_with("plugin.")), "no plug-in commands offered");
        // Every offered id is one the door actually runs.
        let door = door().unwrap();
        assert!(ids.iter().all(|id| door.runs(id)), "only runnable ids are offered");
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
        // Everything lands under `<host>/vector`, nothing in the shared root,
        // and without a resolver a call may replace, as before.
        dispatch("convert", &json!({"path": input, "out": "flat.png"}), host).unwrap();
        serve(&Slot::new(), &service_call("convert", json!({"path": input, "out": "flat.png"}), host, false)).unwrap();
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

    /// The agent tools (`tools.json`) pass App Hub's own loader, as the shell
    /// reads them, keep the object schemas octos takes both ways, and name
    /// only methods this service dispatches.
    #[test]
    fn the_agent_tools_pass_app_hubs_loader_and_name_real_methods() {
        use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest};
        let (manifest, _) = ToolManifest::load(TOOLS_JSON, "vector", ToolHost::Contained, false).unwrap();
        assert!(!manifest.tools.is_empty());
        let dir = tempfile::tempdir().unwrap();
        for tool in &manifest.tools {
            assert_eq!(tool.implemented_by, ImplementedBy::HostService, "{}", tool.name);
            assert!(tool.host_method.is_none(), "{}: the shell routes each tool to the method of its own name", tool.name);
            for schema in [&tool.input_schema, &tool.output_schema] {
                assert_eq!(schema["type"], json!("object"), "{}: octos takes object schemas only", tool.name);
            }
            assert!(tool.description.is_ascii(), "{}: descriptions stay within octos's byte limit", tool.name);
            let method = tool.name.strip_prefix("vector.").unwrap();
            if let Err(e) = dispatch(method, &json!({}), dir.path()) {
                assert!(!e.contains("is not a method"), "{}: {e}", tool.name);
            }
        }
    }

    /// A raster `out` is one artboard: `artboard` picks which (0-based),
    /// at `scale` pixels per point.
    #[test]
    fn the_door_exports_the_artboard_it_is_given() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let cmds = json!([{"id": "artboard.new", "params": {"width": 100, "height": 50}}]);
        let second = serve(&areas, &service_call("run", json!({"cmds": cmds, "out": "second.png", "artboard": 1, "scale": 2}), dir.path(), false)).unwrap();
        assert_eq!(second["out"], json!("second.png"), "{second}");
        let png = std::fs::read(dir.path().join("second.png")).unwrap();
        let size = (u32::from_be_bytes(png[16..20].try_into().unwrap()), u32::from_be_bytes(png[20..24].try_into().unwrap()));
        assert_eq!(size, (200, 100), "the new 100x50 pt artboard at 2 px/pt");
    }

    /// Every `vector.run` call the skill's examples show runs, in order,
    /// in one area as the system agent's (with the files they name placed
    /// there first), and writes its `out`: the skill teaches commands that
    /// work.
    #[test]
    fn the_skill_examples_run() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        std::fs::write(dir.path().join("logo.svg"), br##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="40" viewBox="0 0 64 40"><rect x="4" y="4" width="32" height="20" fill="#3366cc"/><circle cx="48" cy="20" r="10" fill="#cc3333"/></svg>"##).unwrap();
        let body = include_str!("../skill/SKILL.md");
        let examples = body.split("\n## Examples").nth(1).and_then(|rest| rest.split("\n## ").next()).unwrap();
        let mut ran = 0;
        for span in examples.split('`').skip(1).step_by(2) {
            let Some(args) = span.strip_prefix("vector.run ") else { continue };
            let args: Json = serde_json::from_str(args).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            let got = serve(&areas, &service_call("run", args.clone(), dir.path(), false)).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            assert_eq!(got["results"].as_array().map(Vec::len), args["cmds"].as_array().map(Vec::len), "{got}");
            if let Some(out) = args["out"].as_str() {
                assert!(dir.path().join(out).is_file(), "`{span}` wrote no {out}");
            }
            ran += 1;
        }
        assert!(ran >= 4, "{ran} examples");
        let png = std::fs::read(dir.path().join("logo@4x.png")).unwrap();
        assert_eq!((u32::from_be_bytes(png[16..20].try_into().unwrap()), u32::from_be_bytes(png[20..24].try_into().unwrap())), (256, 160), "4x the 64x40 artboard");
        let badge = std::fs::read_to_string(dir.path().join("badge.svg")).unwrap();
        assert!(badge.contains("Beta") && badge.contains("3366cc"), "{badge}");
    }

    /// `n` copies of `v`.
    fn many(v: Json, n: usize) -> Json {
        Json::Array(vec![v; n])
    }

    /// A rectangle at (`x`, `y`), `side` points square.
    fn rect(x: f64, y: f64, side: f64) -> Json {
        json!({"id": "shape.rectangle", "params": {"x": x, "y": y, "width": side, "height": side}})
    }

    /// `run` in a fresh resolver area rooted at `dir`.
    fn run_in(dir: &Path, args: Json) -> Result<Json, String> {
        serve(&resolver(dir, None), &service_call("run", args, dir, false))
    }

    /// Every reviewed limit admits its command at the ceiling and refuses it
    /// one past, saying what it bounds; every limit has a row here.
    #[test]
    fn every_limit_passes_at_its_cap_and_refuses_one_over() {
        let dir = tempfile::tempdir().unwrap();
        let area = Area::new(dir.path(), None, false);
        let door = door().unwrap();
        let pt = json!([1.0, 2.0]);
        let text = |n: usize| "a".repeat(n);
        let stops = |n: usize| many(json!({"position": 10}), n);
        let rows_of = |n: usize| many(many(json!(1.0), 100), n);
        let csv = |n: usize| {
            let header = format!(",{}", vec!["s"; 100].join(","));
            let line = format!("c,{}", vec!["1"; 100].join(","));
            std::iter::once(header).chain(std::iter::repeat_n(line, n)).collect::<Vec<_>>().join("\n")
        };
        // (id, what the limit bounds, params at the cap, params one over, what the refusal says)
        let rows: Vec<(&str, &str, Json, Json, &str)> = vec![
            ("shape.star", "points", json!({"points": 1000}), json!({"points": 1001}), "points"),
            ("shape.polygon", "sides", json!({"sides": 1000}), json!({"sides": 1001}), "sides"),
            ("shape.spiral", "segments", json!({"segments": 1000}), json!({"segments": 1001}), "segments"),
            ("shape.rectangularGrid", "rows", json!({"rows": 999}), json!({"rows": 1000}), "rows"),
            ("shape.rectangularGrid", "columns", json!({"columns": 999}), json!({"columns": 1000}), "columns"),
            ("shape.polarGrid", "concentric dividers", json!({"concentric": 999}), json!({"concentric": 1000}), "concentric"),
            ("shape.polarGrid", "radial dividers", json!({"radial": 999}), json!({"radial": 1000}), "radial"),
            ("shape.flare", "rays", json!({"rays": 50}), json!({"rays": 51}), "rays"),
            ("shape.flare", "rings", json!({"rings": 50}), json!({"rings": 51}), "rings"),
            (
                "path.create",
                "points from the origin (a coordinate or radius of `d`)",
                json!({"d": "M0 0 A4000000 4e6 0 0 1 10 -10Z"}),
                json!({"d": "M0 0 A4000001 4 0 0 1 10 10"}),
                "points from the origin",
            ),
            ("path.knife", "points", json!({"points": many(pt.clone(), 1000)}), json!({"points": many(pt.clone(), 1001)}), "points"),
            ("path.eraseRegion", "points", json!({"points": many(pt.clone(), 1000)}), json!({"points": many(pt.clone(), 1001)}), "points"),
            ("path.eraseSegments", "points", json!({"points": many(pt.clone(), 1000)}), json!({"points": many(pt.clone(), 1001)}), "points"),
            ("path.smoothRegion", "points", json!({"points": many(pt.clone(), 1000)}), json!({"points": many(pt.clone(), 1001)}), "points"),
            ("path.joinScrub", "points", json!({"points": many(pt.clone(), 1000)}), json!({"points": many(pt.clone(), 1001)}), "points"),
            ("path.blob", "points", json!({"points": many(pt.clone(), 1000)}), json!({"points": many(pt.clone(), 1001)}), "points"),
            ("path.freehand", "points", json!({"points": many(pt.clone(), 10_000)}), json!({"points": many(pt.clone(), 10_001)}), "points"),
            ("brush.freehand", "points", json!({"points": many(pt.clone(), 10_000)}), json!({"points": many(pt.clone(), 10_001)}), "points"),
            ("path.curvature", "points", json!({"points": many(pt.clone(), 10_000)}), json!({"points": many(pt.clone(), 10_001)}), "points"),
            ("shapeBuilder.merge", "points", json!({"points": many(pt.clone(), 256)}), json!({"points": many(pt.clone(), 257)}), "points"),
            ("stroke.set", "dash entries", json!({"dash": many(json!(2), 64)}), json!({"dash": many(json!(2), 65)}), "dash entries"),
            (
                "text.setRangeStyle",
                "dash entries",
                json!({"id": 1, "strokeOptions": {"dash": many(json!(2), 64)}}),
                json!({"id": 1, "strokeOptions": {"dash": many(json!(2), 65)}}),
                "dash entries",
            ),
            ("stroke.widthProfile.set", "width points", json!({"points": many(json!([0.5, 1, 1]), 256)}), json!({"points": many(json!([0.5, 1, 1]), 257)}), "width points"),
            ("stroke.widthProfile.set", "times the stroke weight", json!({"points": [[0, 1000, 2]]}), json!({"points": [[0, 2, 1001]]}), "times the stroke weight"),
            ("object.blend.make", "steps", json!({"steps": 1000}), json!({"spacing": "steps", "value": 1001}), "steps"),
            ("object.repeat.grid", "copies", json!({"rows": 100, "cols": 100}), json!({"rows": 101, "cols": 100}), "copies"),
            ("object.repeat.radial", "copies", json!({"instances": 1000}), json!({"instances": 1001}), "copies"),
            ("object.repeat.options", "copies", json!({"instances": 1000}), json!({"instances": 1001}), "copies"),
            ("object.repeat.options", "copies", json!({"rows": 100, "cols": 100}), json!({"rows": 100, "cols": 101}), "copies"),
            ("object.createObjectMosaic", "tiles", json!({"columns": 100, "rows": 100}), json!({"columns": 101, "rows": 100}), "tiles"),
            ("object.path.splitIntoGrid", "cells", json!({"rows": 100, "columns": 100}), json!({"rows": 100, "columns": 101}), "cells"),
            (
                "effect.apply",
                "copies (the original included)",
                json!({"effect": "distort.transform", "params": {"copies": 1000}}),
                json!({"effect": "distort.transform", "params": {"copies": 1001}}),
                "1002 copies",
            ),
            (
                "appearance.addEffect",
                "copies (the original included)",
                json!({"id": "distort.transform", "params": {"copies": 1000}}),
                json!({"id": "distort.transform", "params": {"copies": "1001"}}),
                "1002 copies",
            ),
            ("effect.setParams", "copies (the original included)", json!({"index": 0, "params": {"copies": 1000}}), json!({"index": 0, "params": {"copies": 1001}}), "1002 copies"),
            ("effect.apply", "ridges", json!({"effect": "distort.zigZag", "params": {"ridges": 100}}), json!({"effect": "distort.zigZag", "params": {"ridges": 101}}), "ridges"),
            ("appearance.addEffect", "ridges", json!({"effect": "distort.zigZag", "params": {"ridges": 100}}), json!({"effect": "distort.zigZag", "params": {"ridges": 101}}), "ridges"),
            ("effect.setParams", "ridges", json!({"index": 0, "params": {"ridges": 100}}), json!({"index": 0, "params": {"ridges": 101}}), "ridges"),
            (
                "effect.apply",
                "points per inch (detail)",
                json!({"effect": "distort.roughen", "params": {"detail": 100}}),
                json!({"effect": "distort.roughen", "params": {"detail": 101}}),
                "points per inch",
            ),
            (
                "appearance.addEffect",
                "points per inch (detail)",
                json!({"effect": "distort.roughen", "params": {"detail": 100}}),
                json!({"effect": "distort.roughen", "params": {"detail": 101}}),
                "points per inch",
            ),
            ("effect.setParams", "points per inch (detail)", json!({"index": 0, "params": {"detail": 100}}), json!({"index": 0, "params": {"detail": 101}}), "points per inch"),
            (
                "effect.apply",
                "points of offset",
                json!({"effect": "path.offsetPath", "params": {"offset": -1000}}),
                json!({"effect": "path.offsetPath", "params": {"offset": 1001}}),
                "points of offset",
            ),
            (
                "appearance.addEffect",
                "points of offset",
                json!({"effect": "path.offsetPath", "params": {"offset": 1000}}),
                json!({"effect": "path.offsetPath", "params": {"offset": "1001 pt"}}),
                "points of offset",
            ),
            ("effect.setParams", "points of offset", json!({"index": 0, "params": {"offset": 1000}}), json!({"index": 0, "params": {"offset": -1001}}), "points of offset"),
            ("effect.apply", "points of blur", json!({"effect": "stylize.dropShadow", "params": {"blur": 250}}), json!({"effect": "stylize.dropShadow", "params": {"blur": 251}}), "points of blur"),
            ("appearance.addEffect", "points of blur", json!({"effect": "blur.gaussian", "params": {"radius": 250}}), json!({"effect": "stylize.feather", "params": {"radius": 251}}), "points of blur"),
            ("effect.setParams", "points of blur", json!({"index": 0, "params": {"blur": 250, "radius": 250}}), json!({"index": 0, "params": {"radius": 251}}), "points of blur"),
            ("object.mesh.create", "patches", json!({"rows": 50, "cols": 50}), json!({"rows": 50, "cols": 51}), "patches"),
            ("object.mesh.create", "patches", json!({"rows": 50, "columns": 50}), json!({"rows": 51, "columns": 50}), "patches"),
            ("object.envelope.makeWithMesh", "patches", json!({"rows": 50, "cols": 50}), json!({"rows": 51, "cols": 50}), "patches"),
            ("object.envelope.makeWithMesh", "patches", json!({"rows": 50, "columns": 50}), json!({"rows": 50, "columns": 51}), "patches"),
            ("object.envelope.resetWithMesh", "patches", json!({"rows": 50, "cols": 50}), json!({"rows": 51, "cols": 50}), "patches"),
            ("object.envelope.resetWithMesh", "patches", json!({"rows": 50, "columns": 50}), json!({"rows": 50, "columns": 51}), "patches"),
            ("object.expand", "steps", json!({"steps": 1000}), json!({"steps": 1001}), "steps"),
            ("object.path.offsetPath", "points of offset", json!({"offset": "1000 pt"}), json!({"offset": "36 cm"}), "points of offset"),
            ("object.liquify", "brush dabs", json!({"points": [[0, 0], [49_980, 0]]}), json!({"points": [[0, 0], [49_990, 0]]}), "brush dabs"),
            ("object.liquify", "points", json!({"points": many(pt.clone(), 1000)}), json!({"points": many(pt.clone(), 1001)}), "points"),
            ("object.rasterize", "ppi", json!({"ppi": 1200}), json!({"ppi": 1201}), "ppi"),
            ("object.flattenTransparency", "ppi", json!({"lineArtPpi": 1200, "options": {"gradientPpi": 1200}}), json!({"options": {"gradientPpi": 1201}}), "ppi"),
            ("document.rasterEffectsSettings", "ppi", json!({"resolution": "600 ppi"}), json!({"resolution": 601}), "ppi"),
            ("clipboard.exportPng", "pixels per point", json!({"scale": 16}), json!({"scale": 16.5}), "pixels per point"),
            ("document.serialize", "pixels per point", json!({"format": "png", "scale": 16}), json!({"format": "png", "scale": 17}), "pixels per point"),
            ("document.serialize", "ppi", json!({"format": "png", "ppi": 1152}), json!({"format": "png", "ppi": 1153}), "ppi"),
            ("document.serialize", "ppi", json!({"format": "eps", "flattener": {"lineArtPpi": 1200}}), json!({"format": "eps", "flattener": {"gradientPpi": 1201}}), "ppi"),
            ("document.exportForWeb.preview", "pixels wide", json!({"width": 8192}), json!({"width": 8193}), "pixels wide"),
            ("document.exportForWeb.preview", "pixels high", json!({"height": 8192}), json!({"height": 8193}), "pixels high"),
            ("document.exportForWeb.preview", "percent", json!({"percent": 1600}), json!({"percent": 1601}), "percent"),
            ("artboard.new", "points wide", json!({"width": 163_830}), json!({"width": 163_831}), "points wide"),
            ("artboard.new", "points high", json!({"height": 163_830}), json!({"height": 163_831}), "points high"),
            ("artboard.setProps", "points wide", json!({"index": 0, "width": 163_830}), json!({"index": 0, "width": 163_831}), "points wide"),
            ("artboard.setProps", "points high", json!({"index": 0, "height": 163_830}), json!({"index": 0, "height": 163_831}), "points high"),
            ("text.create", "characters", json!({"x": 0, "y": 0, "text": text(20_000)}), json!({"x": 0, "y": 0, "text": text(20_001)}), "characters"),
            ("text.createInPath", "characters", json!({"path": 1, "text": text(20_000)}), json!({"path": 1, "text": text(20_001)}), "characters"),
            ("text.setText", "characters", json!({"text": text(20_000)}), json!({"text": text(20_001)}), "characters"),
            ("type.insert", "characters", json!({"text": text(20_000)}), json!({"text": text(20_001)}), "characters"),
            (
                "text.editRange",
                "characters",
                json!({"id": 1, "start": 0, "end": 0, "insert": text(19_999), "runs": [{"text": "b", "style": {}}]}),
                json!({"id": 1, "start": 0, "end": 0, "insert": text(20_000), "runs": [{"text": "b", "style": {}}]}),
                "characters",
            ),
            ("text.editRange", "points (type size)", json!({"id": 1, "runs": [{"text": "x", "style": {"size": 1296}}]}), json!({"id": 1, "runs": [{"text": "x", "style": {"size": 1297}}]}), "type size"),
            ("text.create", "points wide (area)", json!({"x": 0, "y": 0, "text": "x", "area": {"width": 100_000, "height": 10}}), json!({"x": 0, "y": 0, "area": {"width": 100_001}}), "points wide"),
            ("text.create", "points high (area)", json!({"x": 0, "y": 0, "text": "x", "area": {"width": 0, "height": 100_000}}), json!({"x": 0, "y": 0, "area": {"width": 0, "height": 1e300}}), "points high"),
            ("edit.findReplace", "characters to find", json!({"find": text(1000)}), json!({"find": text(1001)}), "characters to find"),
            ("edit.findReplace", "times the text found (replacement length)", json!({"find": "a", "replace": text(100)}), json!({"find": "ab", "replace": text(201)}), "times the text found"),
            ("text.areaOptions", "cells (rows × columns)", json!({"rows": 10, "columns": 10}), json!({"rows": 10, "columns": 11}), "cells"),
            ("text.tabs.set", "tab stops", json!({"stops": stops(100)}), json!({"stops": stops(101)}), "tab stops"),
            ("text.setFormat", "points (indent or spacing)", json!({"leftIndent": 100_000, "hyphenate": false}), json!({"spaceAfter": -100_001}), "indent or spacing"),
            ("text.setFormat", "points (indent or spacing)", json!({"firstLineIndent": 12}), json!({"hyphenate": true}), "hyphenation"),
            ("charStyle.new", "style attributes", json!({"attrs": {"size": 1296, "tracking": -10_000}}), json!({"attrs": {"size": 1297}}), "`size` is 1297"),
            ("charStyle.setAttrs", "style attributes", json!({"name": "c", "attrs": {"h_scale": 10_000}}), json!({"name": "c", "attrs": {"v_scale": 10_001}}), "`v_scale`"),
            ("paraStyle.new", "style attributes", json!({"attrs": {"left_indent": 100_000, "tabs": stops(100)}}), json!({"attrs": {"hyphenate": true}}), "hyphenation"),
            ("paraStyle.setAttrs", "style attributes", json!({"name": "p", "attrs": {"space_after": 100_000}}), json!({"name": "p", "attrs": {"tabs": stops(101)}}), "tab stops"),
            ("graph.create", "data cells", json!({"rows": rows_of(100)}), json!({"rows": rows_of(101)}), "data cells"),
            ("graph.setData", "data cells", json!({"csv": csv(100)}), json!({"csv": csv(101)}), "data cells"),
            ("symbol.spray", "points", json!({"points": many(pt.clone(), 1000)}), json!({"points": many(pt.clone(), 1001)}), "points"),
            ("symbol.adjust", "points", json!({"points": many(pt.clone(), 256)}), json!({"points": many(pt.clone(), 257)}), "points"),
        ];
        for (id, _, at, over, says) in &rows {
            door.admit(id, at, &area).unwrap_or_else(|e| panic!("{id} at its cap: {e}"));
            let e = door.admit(id, over, &area).expect_err(&format!("{id} past its cap: {over}"));
            assert!(e.contains(says), "{id}: {e}");
        }
        for l in REVIEWED.limits {
            assert!(rows.iter().any(|(id, what, ..)| *id == l.id && *what == l.what), "no row tests the limit on {} ({})", l.id, l.what);
        }
    }

    /// A `shape.*` multiplier nested in `perspective.draw` is refused by its
    /// own limit through the inner rule, before anything runs.
    #[test]
    fn a_nested_shape_multiplier_is_refused_through_the_inner_rule() {
        let dir = tempfile::tempdir().unwrap();
        for (command, params, says) in [
            ("shape.star", json!({"cx": 100, "cy": 100, "radius1": 50, "radius2": 20, "points": 1e9}), "`shape.star`: `points` is 1000000000"),
            ("shape.rectangularGrid", json!({"x": 0, "y": 0, "width": 100, "height": 100, "rows": 5000}), "`shape.rectangularGrid`: `rows` is 5000"),
        ] {
            let e = run_in(dir.path(), json!({"cmds": [{"id": "perspective.draw", "params": {"command": command, "params": params}}], "out": "p.svg"})).unwrap_err();
            assert!(e.contains(says) && e.contains("the door allows in one command"), "{e}");
        }
        assert!(!dir.path().join("p.svg").exists() && no_staging_left(dir.path()));
        // Within the limit it draws.
        let ok = run_in(dir.path(), json!({"cmds": [{"id": "perspective.draw", "params": {"command": "shape.star", "params": {"cx": 100, "cy": 100, "radius1": 50, "radius2": 20, "points": 12}}}]}));
        assert!(ok.is_ok(), "{ok:?}");
    }

    /// A raster `out` is checked before the export renders anything: a huge
    /// artboard at scale 16 is refused (over the engine's own limit), and so
    /// are images within it but past the door's side or its pixels.
    #[test]
    fn a_huge_artboard_exported_at_scale_16_is_refused_before_export() {
        let dir = tempfile::tempdir().unwrap();
        let board = |w: f64, h: f64| json!({"id": "artboard.setProps", "params": {"index": 0, "width": w, "height": h}});
        for (w, h, out, says) in [
            (163_830.0, 163_830.0, "huge.png", "pixels"),
            (1000.0, 100.0, "wide.png", "16000 × 1600 pixels, more than the 8192 a side"),
            (500.0, 500.0, "big.png", "64000000 pixels, more than the 16777216 the door renders in one export"),
        ] {
            let e = run_in(dir.path(), json!({"cmds": [board(w, h), rect(10.0, 10.0, 40.0)], "out": out, "scale": 16})).unwrap_err();
            assert!(e.contains(says) && e.contains(out), "{out}: {e}");
            assert!(!dir.path().join(out).exists(), "{out}: nothing written");
        }
        assert!(no_staging_left(dir.path()));
        // The same artboard as SVG (nothing rasterized) exports.
        run_in(dir.path(), json!({"cmds": [board(163_830.0, 163_830.0), rect(10.0, 10.0, 40.0)], "out": "huge.svg", "scale": 16})).unwrap();
        assert!(dir.path().join("huge.svg").is_file());
    }

    /// The raster cap itself: 4096 × 4096 pixels pass, one row more does not
    /// (checked without rendering); a PSD's layers each render again.
    #[test]
    fn a_raster_export_is_admitted_at_the_doors_pixels_and_refused_past_them() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 256, "height": 256})).unwrap();
        let fileio = |f: &str| vectorcraft_engine::cmd::fileio::writable_format(Some(f), None).unwrap();
        let check = |s: &Session, f: &str, p: Json| {
            let (_, load) = weigh(s).unwrap();
            check_export(&s.doc().unwrap().doc, &load, fileio(f), &p, "the export")
        };
        check(&s, "png", json!({"scale": 16.0})).unwrap();
        s.execute("artboard.setProps", &json!({"index": 0, "height": 256.0625})).unwrap();
        let e = check(&s, "png", json!({"scale": 16.0})).unwrap_err();
        assert!(e.contains("16781312 pixels, more than the 16777216"), "{e}");
        // Every artboard of a `useArtboards` export counts towards one budget.
        s.execute("artboard.setProps", &json!({"index": 0, "height": 128})).unwrap();
        s.execute("artboard.new", &json!({"width": 256, "height": 129})).unwrap();
        check(&s, "png", json!({"scale": 16.0})).unwrap();
        assert!(check(&s, "png", json!({"scale": 16.0, "useArtboards": true})).unwrap_err().contains("more than the 16777216"));
        // A PSD keeps each visible top-level layer drawn alone.
        s.execute("artboard.setProps", &json!({"index": 0, "height": 256})).unwrap();
        for _ in 0..3 {
            s.execute("layer.new", &json!({})).unwrap();
        }
        check(&s, "psd", json!({"scale": 8.0})).unwrap();
        let e = check(&s, "psd", json!({"scale": 16.0})).unwrap_err();
        assert!(e.contains("pixels of PSD layers"), "{e}");
        assert!(check(&s, "psd", json!({"scale": 16.0, "layers": false})).is_ok());
    }

    /// Two blends at the steps cap each make a million copies together, over
    /// the call's copy budget: refused before anything runs.
    #[test]
    fn two_chained_blends_at_the_cap_exceed_the_calls_copies() {
        let dir = tempfile::tempdir().unwrap();
        let cmds = json!([
            rect(0.0, 0.0, 10.0),
            rect(100.0, 0.0, 10.0),
            {"id": "select.all"},
            {"id": "object.blend.make", "params": {"steps": 1000}},
            rect(0.0, 100.0, 10.0),
            {"id": "select.all"},
            {"id": "object.blend.make", "params": {"steps": 1000}}
        ]);
        let e = run_in(dir.path(), json!({"cmds": cmds, "out": "blends.svg"})).unwrap_err();
        assert!(e.contains("the copies this call makes multiply to 1000000, more than the 10000"), "{e}");
        assert!(!dir.path().join("blends.svg").exists());
        // One blend at the cap runs (two keys, a thousand steps).
        let one = json!([rect(0.0, 0.0, 10.0), rect(100.0, 0.0, 10.0), {"id": "select.all"}, {"id": "object.blend.make", "params": {"steps": 1000}}]);
        run_in(dir.path(), json!({"cmds": one, "out": "blend.svg"})).unwrap();
    }

    /// A Transform effect's copies past the cap are refused through
    /// `effect.apply` and through `effect.setParams`, which merges them into
    /// an effect already there; copies at the cap stacked by duplicating the
    /// effect are caught by the document weight.
    #[test]
    fn an_over_cap_transform_copy_count_is_refused_through_apply_and_set_params() {
        let dir = tempfile::tempdir().unwrap();
        let transform = |copies: f64| json!({"id": "effect.apply", "params": {"effect": "distort.transform", "params": {"copies": copies, "moveH": 5}}});
        let e = run_in(dir.path(), json!({"cmds": [rect(0.0, 0.0, 10.0), transform(5000.0)], "out": "t.svg"})).unwrap_err();
        assert!(e.contains("`effect.apply` asks for 5001 copies (the original included), more than the 1001"), "{e}");
        let set = json!({"id": "effect.setParams", "params": {"index": 0, "params": {"copies": 5000}}});
        let e = run_in(dir.path(), json!({"cmds": [rect(0.0, 0.0, 10.0), transform(10.0), set], "out": "t.svg"})).unwrap_err();
        assert!(e.contains("`effect.setParams` asks for 5001 copies (the original included), more than the 1001"), "{e}");
        let alias = json!({"id": "appearance.addEffect", "params": {"id": "distort.transform", "params": {"copies": 5000}}});
        assert!(run_in(dir.path(), json!({"cmds": [rect(0.0, 0.0, 10.0), alias]})).unwrap_err().contains("5001 copies"));
        // At the cap it runs; duplicated onto itself it would draw a million.
        let stacked = json!([rect(0.0, 0.0, 10.0), transform(1000.0), {"id": "effect.duplicate", "params": {"index": 0}}]);
        let e = run_in(dir.path(), json!({"cmds": stacked, "out": "t.svg"})).unwrap_err();
        assert!(e.contains("after `effect.duplicate` the document would hold 1002002 objects as drawn"), "{e}");
        assert!(!dir.path().join("t.svg").exists());
        run_in(dir.path(), json!({"cmds": [rect(0.0, 0.0, 10.0), transform(1000.0)], "out": "t.svg"})).unwrap();
    }

    /// Doubling with no count (select all, duplicate; select all, copy, paste)
    /// stops at the document's ceiling, before the duplicate that would pass it.
    #[test]
    fn a_duplicate_loop_is_stopped_by_the_document_ceiling() {
        let dir = tempfile::tempdir().unwrap();
        let mut duplicate = vec![rect(0.0, 0.0, 4.0)];
        let mut paste = vec![rect(0.0, 0.0, 4.0)];
        for _ in 0..20 {
            duplicate.extend([json!({"id": "select.all"}), json!({"id": "edit.duplicate"})]);
            paste.extend([json!({"id": "select.all"}), json!({"id": "edit.copy"}), json!({"id": "edit.pasteInPlace"})]);
        }
        let e = run_in(dir.path(), json!({"cmds": duplicate, "out": "loop.svg"})).unwrap_err();
        assert!(e.contains("with `edit.duplicate` the document would hold") && e.contains("nodes in its layers") && e.contains("more than the 20000 the door allows a call"), "{e}");
        let e = run_in(dir.path(), json!({"cmds": paste, "out": "loop.svg"})).unwrap_err();
        assert!(e.contains("with `edit.pasteInPlace` the document would hold") && e.contains("more than the 20000"), "{e}");
        assert!(!dir.path().join("loop.svg").exists() && no_staging_left(dir.path()));
    }

    /// A repeat of a repeat multiplies as drawn: refused in one call by the
    /// copy budget, and across calls by the document weight (the second
    /// repeat would draw a hundred million copies of the first one's art).
    #[test]
    fn a_repeat_of_a_repeat_is_weighed_as_drawn_across_calls() {
        let dir = tempfile::tempdir().unwrap();
        let grid = json!({"id": "object.repeat.grid", "params": {"rows": 100, "cols": 100}});
        let e = run_in(dir.path(), json!({"cmds": [rect(0.0, 0.0, 4.0), grid.clone(), {"id": "select.all"}, grid.clone()]})).unwrap_err();
        assert!(e.contains("multiply to 100000000"), "{e}");
        run_in(dir.path(), json!({"cmds": [rect(0.0, 0.0, 4.0), grid.clone()], "out": "grid.vectorcraft"})).unwrap();
        let e = run_in(dir.path(), json!({"path": "grid.vectorcraft", "cmds": [{"id": "select.all"}, grid.clone()], "out": "grid2.vectorcraft"})).unwrap_err();
        assert!(e.contains("with `object.repeat.grid` the document would hold") && e.contains("objects as drawn"), "{e}");
        assert!(!dir.path().join("grid2.vectorcraft").exists());
    }

    /// `symbol.update` with an instance of the symbol selected would make it
    /// hold itself (the renderer would recurse without end): refused.
    #[test]
    fn a_symbol_made_to_hold_itself_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let cmds = json!([rect(0.0, 0.0, 10.0), {"id": "symbol.new", "params": {"name": "Loop"}}, {"id": "symbol.update", "params": {"name": "Loop"}}]);
        let e = run_in(dir.path(), json!({"cmds": cmds, "out": "s.svg"})).unwrap_err();
        assert!(e.contains("`symbol.update` would make the symbol `Loop` hold an instance of itself"), "{e}");
        assert!(!dir.path().join("s.svg").exists());
    }

    /// Release to Layers (Build) copies n(n + 1) / 2 objects: weighed before
    /// it runs. A hundred and one paths pass, two hundred and one do not.
    #[test]
    fn release_to_layers_build_is_weighed_before_it_copies() {
        let dir = tempfile::tempdir().unwrap();
        let grid = |n: u32| json!({"id": "shape.rectangularGrid", "params": {"x": 0, "y": 0, "width": 100, "height": 100, "rows": n, "columns": n}});
        let build = |n: u32| json!([grid(n), {"id": "object.ungroup"}, {"id": "layer.releaseToLayersBuild"}]);
        run_in(dir.path(), json!({"cmds": build(50)})).unwrap();
        let e = run_in(dir.path(), json!({"cmds": build(100)})).unwrap_err();
        assert!(e.contains("with `layer.releaseToLayersBuild` the document would hold") && e.contains("nodes"), "{e}");
    }

    /// A graph's value axis too narrow for its values' precision would tick
    /// forever: refused, also when its two ends come in separate calls.
    #[test]
    fn a_graph_axis_that_would_never_end_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let graph = json!({"id": "graph.create", "params": {"type": "column", "x": 0, "y": 0, "width": 200, "height": 100, "rows": [[1, 2], [3, 4]]}});
        let (lo, hi) = (1e17, 1e17 + 16.0);
        let both = json!({"id": "graph.setType", "params": {"axisMin": lo, "axisMax": hi}});
        let e = run_in(dir.path(), json!({"cmds": [graph.clone(), both]})).unwrap_err();
        assert!(e.contains("`graph.setType`") && e.contains("its ticks would never end"), "{e}");
        // The maximum alone lays a sound axis from 0; the minimum then closes it.
        let split = json!([graph.clone(), {"id": "graph.setType", "params": {"axisMax": hi}}, {"id": "graph.setType", "params": {"axisMin": lo}}]);
        let e = run_in(dir.path(), json!({"cmds": split})).unwrap_err();
        assert!(e.contains("its ticks would never end"), "{e}");
        run_in(dir.path(), json!({"cmds": [graph, {"id": "graph.setType", "params": {"axisMin": 0, "axisMax": 10}}]})).unwrap();
    }

    /// PDF draws raster effects as images at the document's raster effects
    /// resolution, and lays pattern tiles one by one: both are weighed before
    /// the export; SVG, which writes filters and patterns, exports the same.
    #[test]
    fn what_a_pdf_export_would_rasterize_or_lay_is_weighed_first() {
        let dir = tempfile::tempdir().unwrap();
        let shadow = json!([
            {"id": "document.rasterEffectsSettings", "params": {"resolution": 600}},
            rect(0.0, 0.0, 2000.0),
            {"id": "effect.apply", "params": {"effect": "stylize.dropShadow"}}
        ]);
        let e = run_in(dir.path(), json!({"cmds": shadow, "out": "fx.pdf"})).unwrap_err();
        assert!(e.contains("raster effects") && e.contains("at 600 ppi"), "{e}");
        run_in(dir.path(), json!({"cmds": shadow, "out": "fx.svg"})).unwrap();
        let tiles = json!([
            {"id": "shape.ellipse", "params": {"x": 0, "y": 0, "width": 1, "height": 1}},
            {"id": "object.pattern.make", "params": {"name": "Dots", "width": 1, "height": 1, "edit": false}},
            rect(0.0, 0.0, 600.0),
            {"id": "paint.setFill", "params": {"swatch": "Dots"}}
        ]);
        let e = run_in(dir.path(), json!({"cmds": tiles, "out": "tiles.pdf"})).unwrap_err();
        assert!(e.contains("pattern tiles one by one"), "{e}");
        run_in(dir.path(), json!({"cmds": tiles, "out": "tiles.svg"})).unwrap();
        assert!(!dir.path().join("fx.pdf").exists() && !dir.path().join("tiles.pdf").exists() && no_staging_left(dir.path()));
    }

    /// Image Trace is weighed by its image before it decodes it: a region of
    /// every `noise` pixels at most. Rasterizing is weighed by its pixels.
    #[test]
    fn image_trace_and_rasterize_are_weighed_before_they_run() {
        let dir = tempfile::tempdir().unwrap();
        let image = json!([rect(0.0, 0.0, 999.0), {"id": "object.rasterize", "params": {"ppi": 72}}]);
        let trace = |noise: u32| {
            let mut cmds = image.as_array().unwrap().clone();
            cmds.push(json!({"id": "imageTrace.make", "params": {"params": {"mode": "blackAndWhite", "noise": noise}}}));
            json!({"cmds": cmds})
        };
        let e = run_in(dir.path(), trace(1)).unwrap_err();
        assert!(e.contains("`imageTrace.make` would take 1000000 traced regions at most"), "{e}");
        run_in(dir.path(), trace(2)).unwrap();
        let big = json!({"cmds": [rect(0.0, 0.0, 999.0), {"id": "object.rasterize", "params": {"ppi": 300}}]});
        let e = run_in(dir.path(), big).unwrap_err();
        assert!(e.contains("`object.rasterize` would take 17363889 pixels, more than the 16777216"), "{e}");
    }

    /// The weight counts what live objects draw, without making it: a repeat
    /// of a blend, a symbol placed twice in a symbol placed twice.
    #[test]
    fn the_document_weight_counts_live_copies_as_drawn() {
        let mut s = Session::new();
        s.execute("file.new", &json!({})).unwrap();
        s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap();
        s.execute("shape.rectangle", &json!({"x": 100, "y": 0, "width": 10, "height": 10})).unwrap();
        s.execute("select.all", &json!({})).unwrap();
        s.execute("object.blend.make", &json!({"steps": 10})).unwrap();
        s.execute("object.repeat.grid", &json!({"rows": 2, "cols": 5})).unwrap();
        let (w, _) = weigh(&s).unwrap();
        // The layer, the repeat, then ten copies of the blend: itself, its two
        // keys and ten steps.
        assert_eq!(w.objects, 2.0 + 10.0 * 13.0, "{w:?}");
        assert_eq!(w.anchors, 10.0 * 12.0 * 4.0, "{w:?}");
        assert!(w.nodes >= 5.0, "{w:?}");
    }
}
