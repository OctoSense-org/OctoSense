//! `octosense-deck-service` — the `deck` host service (ADR 0013).
//!
//! deckcraft's presentation engine behind typed `deck.*` methods. Every
//! call is a fresh, stateless engine session. The service does all file
//! I/O itself and feeds the engine bytes, so the engine never touches a
//! path, and a link written inside a presentation (an external picture or
//! media relationship) is kept as a string and never followed. Everything
//! the service reads or writes lives in the call's area (ADR 0013,
//! 2026-10-08): the caller's own folder from the shell's resolver
//! ([`set_area_resolver`]), or without one the legacy `<host_dir>/deck`. A
//! write that may not replace (an agent's) only creates new files, within
//! the area's quota ([`Area::write`]). Everything is JSON at the boundary;
//! the engine's types never cross it.
//!
//! Methods (all under the `deck` family; paths relative to the call's area):
//! - `info {path}` → the deck inspected: slides with titles, layouts,
//!   shape counts, sections, theme (a `.pptx`, `.deckcraft` or outline text)
//! - `text {path}` → `{outline, slides}` — titles unindented, bullets
//!   tab-indented by level
//! - `render {path, slide?, out, max_side?}` → `{out, slide, width, height,
//!   bytes}` — one slide as a PNG written to `out`
//! - `new {out, slides: [{title, bullets?}]}` → `{out, slides, format}` —
//!   a deck written as `.pptx` or `.deckcraft`
//! - `convert {path, out}` → `{out, format, bytes}` — `.pptx`,
//!   `.deckcraft`, outline `.txt` or `.pdf`, by `out`'s extension
//! - `run {path?, cmds: [{id, params?}], out?, slide?, max_side?}` →
//!   `{results, out, format?, bytes?, slide?, width?, height?}` — the
//!   command door (ADR 0013, #418): run commands of deckcraft's registry on
//!   the deck at `path`, or on a new blank one, then write it to `out` as
//!   `convert` does (`.pptx`, `.deckcraft`, outline `.txt`, `.pdf`) or one
//!   slide as `render` does (`.png`). Only what the door's allowlist admits
//!   runs ([`door`]): commands the reviewed classification
//!   (`skill/safety.json`) classes `safe`, and the four reviewed media reads,
//!   whose `path` must name a file inside the area; every other id is
//!   refused before any command runs. Those reads are the one place the
//!   engine opens a file itself: the door hands it the file's resolved
//!   absolute path, and caps what one call's reads may total. What one call
//!   may ask for is capped: the parameters that multiply work at the gate
//!   ([`REVIEWED`]), the presentation after every command, what it rasters
//!   and what it answers in the service ([`RunBudget`]).
//!
//! The service serves system apps only until ADR 0013's store capability
//! is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};

use deckcraft_engine::cmd::file as engine_file;
use deckcraft_engine::Session;
use deckcraft_model::{Presentation, Shape, ShapeKind, TextBody};
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::door::{Door, FileRead, Held as HeldBack, Limit, Measure, Reviewed};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};

/// The longest preview edge `render` produces.
const MAX_RENDER_SIDE: u64 = 4096;
/// The largest file the service reads or writes (bytes). (What one `run`
/// call's reviewed reads may total is the gate's: the same 64 MiB,
/// `octosense_engine_area::door::MAX_READ_BYTES`.)
const MAX_DECK_BYTES: u64 = 64 << 20;
/// `new` builds at most this many slides per call.
const MAX_NEW_SLIDES: usize = 200;
/// `new` takes at most this many bullets per slide.
const MAX_BULLETS: usize = 64;
/// `new` takes titles and bullets of at most this many characters.
const MAX_LINE_CHARS: usize = 2000;

/// What `convert` writes, by `out`'s extension.
const CONVERT_OUT: &[(&str, &str)] = &[(".pptx", "pptx"), (".deckcraft", "deckcraft"), (".txt", "outline"), (".pdf", "pdf")];
/// What `run` writes: `convert`'s formats, and one slide as `render` draws it.
const RUN_OUT: &[(&str, &str)] = &[(".pptx", "pptx"), (".deckcraft", "deckcraft"), (".txt", "outline"), (".pdf", "pdf"), (".png", "png")];

/// What the deck engine's reviewer settled for the door beyond the classes:
/// two `file` commands that only read the picture their `path` names (whole,
/// with `std::fs::read`) and embed its bytes in the deck, or take it inline
/// as base64 `data`, which wins when both are given. Both go through
/// `insert::media_bytes` (`cmd/insert.rs` 212-224), their only file access:
/// `insert.picture` (`picture`, 295-365: decoded and sized in memory) and
/// `picture.change` (`cmd/shape.rs` 650-661).
///
/// Held back ([`HELD_BACK`], #448): `insert.audio` and `insert.video` read a file
/// the same way, but they and `media.info` and `media.posterFrame` probe and
/// decode video and audio, and `file.openBytes` unzips a presentation from
/// inline bytes. deckcraft sizes frames, sample tables, decoded audio and
/// zip entries from the data's own headers, so hostile data can make it
/// allocate gigabytes and abort the shell process. The door refuses them
/// until deckcraft bounds that.
/// `shape.fill` and `design.background` are deliberately not listed: a
/// non-string `picture` makes them read an undocumented `path`. No setter
/// or inner id: no `safe` deck command sets an app-wide variable by key or
/// names another command (every nested `execute` in the engine runs a fixed
/// id), and animation effects are built-in presets, never plug-ins.
///
/// The limits bound the parameters that multiply work or memory. Every
/// other parameter of the 203 commands is clamped by the engine to a size
/// that costs little (font sizes ≤ 4000, line widths ≤ 1584, columns ≤ 16,
/// SmartArt ≤ 12 items, freeform ≤ 20,000 points, effects' radii ≤ 200,
/// animation and transition times) or has no count: no duplicate, paste or
/// animation command takes one, so nothing here is a copy and the door's
/// copy factor stays 1. What grows by repetition (select all and
/// duplicate, copy and paste) is bounded by the service's ceilings on the
/// session after every command ([`RunBudget`]).
static REVIEWED: Reviewed = Reviewed {
    file_reads: &[FileRead { id: "insert.picture", params: &["path"] }, FileRead { id: "picture.change", params: &["path"] }],
    setters: &[],
    inner: &[],
    limits: &[
        TABLE_CELLS,
        SELECTED_CELLS,
        FILLED_CELLS,
        chart_points("insert.chart"),
        chart_points("chart.data"),
        SLIDE_AREA,
        NEW_SLIDE_AREA,
        OUTLINE_SLIDES,
        FRAGMENTED_SHAPES,
        BULLET_SIZE,
        OUTLINE_WIDTH,
        line_breaks("text.insert"),
        line_breaks("insert.symbol"),
        line_breaks("edit.paste"),
        line_breaks("edit.pasteText"),
        extent("shape.insert"),
        extent("insert.textBox"),
        extent("insert.picture"),
        extent("insert.table"),
        extent("insert.chart"),
        extent("insert.actionButton"),
        extent("master.insertPlaceholder"),
        extent("shape.setBounds"),
        extent("shape.resize"),
        extent("shape.move"),
        extent("arrange.nudge"),
        extent("shape.freeform"),
    ],
    copies_per_call: 1.0,
    held: HELD_BACK,
};

/// Why deckcraft's media and zip commands are held back (#448).
const MEDIA: &str = "deckcraft can abort the shell process on hostile video or audio data (#448), so the door does not run it until the engine bounds that";
const ZIP: &str = "deckcraft can abort the shell process on a hostile zip (#448), so the door does not run it until the engine bounds that";

/// The commands the door holds back although their review would let them
/// run ([`REVIEWED`]).
static HELD_BACK: &[HeldBack] = &[
    HeldBack { id: "insert.audio", why: MEDIA },
    HeldBack { id: "insert.video", why: MEDIA },
    HeldBack { id: "media.info", why: MEDIA },
    HeldBack { id: "media.posterFrame", why: MEDIA },
    HeldBack { id: "file.openBytes", why: ZIP },
];

/// `insert.table {rows, cols}`: rows × cols cells, each a text body every
/// render draws (unclipped). The engine's own clamp, 75 × 75 = 5,625 cells:
/// inserting one takes ~5 ms, rendering it ~2 ms and its PDF page ~0.2 s at
/// opt-level 1; this refuses what the engine would silently shrink.
const TABLE_CELLS: Limit = Limit { id: "insert.table", what: "cells", measure: Measure::Product(&["rows", "cols"]), max: 5625.0, copies: false };

/// `table.selectCells {from, to}`: the engine keeps the rectangle without
/// checking it against the table, and `table.cellFill` then lists every
/// cell in it ([`cell_rect`]); the largest table's 5,625 cells.
const SELECTED_CELLS: Limit = Limit { id: "table.selectCells", what: "cells", measure: Measure::Custom(cell_rect), max: 5625.0, copies: false };

/// `table.cellFill {cells}`: one fill per listed cell; the largest table's
/// 5,625.
const FILLED_CELLS: Limit = Limit { id: "table.cellFill", what: "cells", measure: Measure::Custom(cells_listed), max: 5625.0, copies: false };

/// `insert.chart` / `chart.data {categories, series}`: every category, series
/// and value is a label to lay out or a mark to draw each time the chart's
/// slide renders (and is copied into every target chart); the engine takes
/// any number ([`chart_point_count`]). 10,000 is far past a readable chart.
const fn chart_points(id: &'static str) -> Limit {
    Limit { id, what: "chart data points", measure: Measure::Custom(chart_point_count), max: 10_000.0, copies: false }
}

/// The most square points a slide may cover: 1920 × 1080, four times the
/// default 16:9 slide. A `.pdf` page rasters its slide at 200 dpi (2.78 px a
/// point), so this keeps one page within 16 Mpx (~0.5 s at opt-level 1; the
/// engine's own 4032 × 4032 is 125 Mpx, ~3.8 s and an estimated 1 GB, for
/// each page).
const MAX_SLIDE_AREA: f64 = 1920.0 * 1080.0;

/// `design.slideSize {w, h}`: the slide's area ([`MAX_SLIDE_AREA`]); a
/// `preset` is a built-in size, at most 1008 × 756.
const SLIDE_AREA: Limit = Limit { id: "design.slideSize", what: "square points of slide", measure: Measure::Product(&["w", "h"]), max: MAX_SLIDE_AREA, copies: false };

/// `file.new {size: [w, h]}`: a new presentation's slide area, as
/// `design.slideSize` (the engine takes up to 4999 × 4999).
const NEW_SLIDE_AREA: Limit =
    Limit { id: "file.new", what: "square points of slide", measure: Measure::Product(&["size.0", "size.1"]), max: MAX_SLIDE_AREA, copies: false };

/// `slide.fromOutline {text}`: a slide for every top-level line, each a copy
/// of its layout's placeholders, with no limit in the engine
/// ([`outline_slides`]); `deck.new`'s own 200 slides.
const OUTLINE_SLIDES: Limit = Limit { id: "slide.fromOutline", what: "slides", measure: Measure::Custom(outline_slides), max: 200.0, copies: false };

/// `shape.merge {op: "fragment", ids}`: fragmenting k shapes makes up to
/// 2^k − 1 pieces, each a boolean of flattened outlines
/// ([`fragment_shapes`]); 8 shapes, 255 pieces. The service checks a merge
/// of the selection, which the gate cannot see, before it runs.
const FRAGMENTED_SHAPES: Limit = Limit { id: "shape.merge", what: "shapes to fragment", measure: Measure::Custom(fragment_shapes), max: 8.0, copies: false };

/// `format.bullets {size}`: the bullet's size in percent of its text, which
/// the engine leaves unbounded; PowerPoint's own 400.
const BULLET_SIZE: Limit = Limit { id: "format.bullets", what: "percent bullets", measure: Measure::Product(&["size"]), max: 400.0, copies: false };

/// `format.textOutline {width}`: an outline stroked around every glyph,
/// which the engine leaves unbounded; the 1584 points it allows a shape's
/// line.
const OUTLINE_WIDTH: Limit = Limit { id: "format.textOutline", what: "points of outline", measure: Measure::Product(&["width"]), max: 1584.0, copies: false };

/// A command's line breaks in its `text`: inserting k of them into a
/// paragraph costs the engine O(k²) (each `\u{b}` walks the runs, each `\n`
/// shifts the paragraphs after it), so 100 KB of them would take minutes
/// ([`line_break_count`]). 1,000 lines is more than a slide shows.
const fn line_breaks(id: &'static str) -> Limit {
    Limit { id, what: "line breaks", measure: Measure::Custom(line_break_count), max: 1000.0, copies: false }
}

/// The furthest a shape may reach from the slide's origin, in points, as
/// a position or a size: 25 times the largest slide side (4032). The
/// renderer flattens a curve into about √(its size in pixels) segments
/// with no ceiling, so a shape 1e20 points across aborts the process
/// when it is drawn; at this size a curve is a few thousand segments.
const MAX_EXTENT: f64 = 100_000.0;

/// A command's geometry (`rect`, `x`, `y`, `w`, `h`, `dx`, `dy`, `points`):
/// its largest coordinate or size, in points ([`extent_of`]), within
/// [`MAX_EXTENT`]. The service also checks every shape after every command,
/// which catches what a parameter does not show (a size kept to an
/// existing extreme aspect, a slide size scaled into the shapes).
const fn extent(id: &'static str) -> Limit {
    Limit { id, what: "points of position or size", measure: Measure::Custom(extent_of), max: MAX_EXTENT, copies: false }
}

/// `table.selectCells`' rectangle: the rows and columns from `from` to `to`.
fn cell_rect(params: &Json) -> Result<Option<f64>, String> {
    let at = |key: &str| {
        let pair = params.get(key)?.as_array()?;
        Some((pair.first()?.as_f64()?, pair.get(1)?.as_f64()?))
    };
    let (Some((r0, c0)), Some((r1, c1))) = (at("from"), at("to")) else { return Ok(None) };
    Ok(Some(((r1 - r0).abs() + 1.0) * ((c1 - c0).abs() + 1.0)))
}

/// `table.cellFill`'s listed `cells`.
fn cells_listed(params: &Json) -> Result<Option<f64>, String> {
    Ok(params.get("cells").and_then(Json::as_array).map(|cells| cells.len() as f64))
}

/// A chart command's data: its categories, and each series with its
/// values.
fn chart_point_count(params: &Json) -> Result<Option<f64>, String> {
    let categories = params.get("categories").and_then(Json::as_array).map(Vec::len);
    let series = params.get("series").and_then(Json::as_array).map(|all| all.iter().map(|s| 1 + s.get("values").and_then(Json::as_array).map_or(0, Vec::len)).sum());
    Ok((categories.is_some() || series.is_some()).then(|| (categories.unwrap_or(0) + series.unwrap_or(0)) as f64))
}

/// `slide.fromOutline`'s slides: its lines that are not indented and not
/// blank.
fn outline_slides(params: &Json) -> Result<Option<f64>, String> {
    Ok(params.get("text").and_then(Json::as_str).map(|text| text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with(char::is_whitespace)).count() as f64))
}

/// `shape.merge`'s named shapes, when it fragments them.
fn fragment_shapes(params: &Json) -> Result<Option<f64>, String> {
    if params.get("op").and_then(Json::as_str) != Some("fragment") {
        return Ok(None);
    }
    Ok(params.get("ids").and_then(Json::as_array).filter(|ids| !ids.is_empty()).map(|ids| ids.len() as f64))
}

/// The line breaks in a command's `text`.
fn line_break_count(params: &Json) -> Result<Option<f64>, String> {
    Ok(params.get("text").and_then(Json::as_str).map(|t| t.matches(['\n', '\r', '\u{b}']).count() as f64))
}

/// The largest absolute number of a command's geometry.
fn extent_of(params: &Json) -> Result<Option<f64>, String> {
    let mut most: Option<f64> = None;
    let mut see = |v: &Json| {
        if let Some(n) = v.as_f64() {
            most = Some(most.map_or(n.abs(), |m| m.max(n.abs())));
        }
    };
    for key in ["x", "y", "w", "h", "dx", "dy"] {
        params.get(key).into_iter().for_each(&mut see);
    }
    params.get("rect").and_then(Json::as_array).into_iter().flatten().for_each(&mut see);
    params.get("points").and_then(Json::as_array).into_iter().flatten().filter_map(Json::as_array).flatten().for_each(&mut see);
    Ok(most)
}

/// The most slides a session may hold while the door works on it (all its
/// presentations): five long decks. `.pdf` writes are bounded apart
/// ([`MAX_RUN_RASTER`]).
const MAX_RUN_SLIDES: usize = 500;
/// The most shapes (on slides, masters and layouts, inside groups too).
const MAX_RUN_SHAPES: usize = 20_000;
/// The most shapes on one slide: with all of them selected, every shape
/// command looks each target up among them, O(n²) a command.
const MAX_SLIDE_SHAPES: usize = 2_000;
/// The most table cells, every table together.
const MAX_RUN_CELLS: usize = 50_000;
/// The most chart data points, every chart together.
const MAX_RUN_POINTS: usize = 50_000;
/// The most characters of text (shapes, cells and speaker notes).
const MAX_RUN_CHARS: usize = 1_000_000;
/// The most media bytes: a deck the service reads is at most 64 MiB, and a
/// call's reads total 64 MiB more (`door::MAX_READ_BYTES`).
const MAX_RUN_MEDIA: u64 = 128 << 20;
/// The most shapes one call's edits may copy or write, together: the engine
/// keeps an undo snapshot of every edit, sharing the slides it left alone
/// but keeping the old copy of every slide it changed, so 64 edits of every
/// slide (a slide size, headers and footers, a transition for all) would
/// hold 64 copies of the deck. Four decks at the ceiling.
const MAX_RUN_COPIED_SHAPES: usize = 4 * MAX_RUN_SHAPES;
/// The largest font size, in points: the engine's own `format.size` clamp
/// (a slide size change scales explicit sizes without one).
const MAX_FONT_SIZE: f64 = 4000.0;
/// The most pixels one call may raster, together: `file.render`, the
/// `.pdf` pages of `file.saveBytes` and the `.pdf` or `.png` out. A `.pdf`
/// rasters every visible slide at 200 dpi, a default slide 4 Mpx in
/// ~73 ms at opt-level 1, so this is ~40 default slides (~2.9 s).
const MAX_RUN_RASTER: u64 = 160_000_000;
/// The most pixels one raster may have: a `file.render` or one `.pdf` page
/// (the `.png` out is at most 4096 px a side already).
const MAX_RASTER: u64 = 4096 * 4096;
/// The most bytes of results one call may return, together: `file.render`
/// answers a base64 PNG and `file.saveBytes` a base64 deck, and every
/// result is kept until the reply; the most the service writes.
const MAX_RUN_RESULT_BYTES: u64 = MAX_DECK_BYTES;

/// The command door's gate: deckcraft's reviewed classification
/// (`skill/safety.json`, generated and drift-checked by `tests/skill.rs`)
/// and [`REVIEWED`].
pub fn door() -> Result<&'static Door, String> {
    static DOOR: OnceLock<Result<Door, String>> = OnceLock::new();
    DOOR.get_or_init(|| Door::new("deck", include_str!("../skill/safety.json"), &REVIEWED)).as_ref().map_err(Clone::clone)
}

/// Which apps may call the service: system apps, as News and Sheets.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct DeckService;

mod warm;
pub use warm::warm;

/// Register the `deck` service with App Hub's host-service registry, and
/// start paying the engine's first-call cost on a thread of its own
/// ([`warm`]).
pub fn register() {
    register_host_service(Box::new(DeckService));
    warm();
}

/// The shell's resolver: where each call works (`None` removes it, and
/// calls work in the legacy `<host dir>/deck` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
}

/// The `deck.*` agent tools (ADR 0013, wave 2), in App Hub's `tools.json`
/// shape: the shell declares them for the virtual owner `os.deck` and grants
/// the system agent its reviewed share (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`).
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for DeckService {
    fn family(&self) -> &'static str {
        "deck"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        reply.send(serve(&AREAS, &call));
    }
}

/// One call, in the area `areas` gives it.
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The deck service serves system apps only.".into());
    }
    let area = areas.area(call, "deck").map_err(|e| format!("deck: {e}"))?;
    dispatch_in(call.method(), &call.args, &area)
        .map(|answer| area.relative_json(answer))
        .map_err(|error| area.relative_text(&error))
}

fn dispatch_in(method: &str, args: &Json, area: &Area) -> Result<Json, String> {
    match method {
        "info" => info(args, area),
        "text" => text(args, area),
        "render" => render(args, area),
        "new" => new_deck(args, area),
        "convert" => convert(args, area),
        "run" => run(args, area),
        other => Err(format!("deck.{other} is not a method of the deck service")),
    }
}

/// [`dispatch_in`] in the legacy area `<host_dir>/deck`, as a call without
/// the shell's resolver works.
#[cfg(test)]
fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    let area = Area::legacy(host_dir, "deck");
    std::fs::create_dir_all(&area.root).map_err(|e| format!("deck: {e}"))?;
    dispatch_in(method, args, &area)
}

/// A path strictly inside the call's area: relative, no `..`, no absolute
/// or prefix component; the resolved path stays under the area even
/// through symlinks.
fn contained(area: &Area, rel: &str, method: &str, key: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("deck.{method}: `{key}` is required"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("deck.{method}: `{key}` stays inside this call's folder"));
    }
    let root = &area.root;
    let joined = root.join(rel_path);
    let check_root = root.canonicalize().map_err(|e| format!("deck.{method}: folder: {e}"))?;
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
    let resolved = deepest.canonicalize().map_err(|e| format!("deck.{method}: {e}"))?;
    if !resolved.starts_with(&check_root) {
        return Err(format!("deck.{method}: `{key}` stays inside this call's folder"));
    }
    Ok(joined)
}

/// A contained output path the call may write: refused before the engine
/// works when the area's rules would refuse it (an existing file for a call
/// that may not replace).
fn out_path(area: &Area, rel: &str, method: &str) -> Result<PathBuf, String> {
    let out = contained(area, rel, method, "out")?;
    area.check(&out, 0).map_err(|e| format!("deck.{method}: {e}"))?;
    Ok(out)
}

/// Read `args.path` from the call's area and seat it in a fresh engine
/// session (the engine gets bytes, never a path). Reads `.pptx`,
/// `.deckcraft` and outline `.txt`/`.md`, by content.
fn read_deck(args: &Json, area: &Area, method: &str) -> Result<(Session, String), String> {
    let rel = args["path"].as_str().unwrap_or("").to_string();
    let path = contained(area, &rel, method, "path")?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("deck.{method}: {rel}: {e}"))?;
    if meta.len() > MAX_DECK_BYTES {
        return Err(format!("deck.{method}: the file is larger than the service reads"));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("deck.{method}: {rel}: {e}"))?;
    let doc = engine_file::open_presentation(&rel, &bytes).map_err(|e| format!("deck.{method}: {e}"))?;
    let mut s = Session::new();
    engine_file::add_opened(&mut s, &rel, None, doc);
    Ok((s, rel))
}

/// Write engine output into the call's area, capped like reads are, under
/// the area's rules ([`Area::write`]).
fn write_out(area: &Area, path: &Path, bytes: &[u8], method: &str) -> Result<(), String> {
    if bytes.len() as u64 > MAX_DECK_BYTES {
        return Err(format!("deck.{method}: the result is larger than the service writes"));
    }
    area.write(path, bytes).map_err(|e| format!("deck.{method}: {e}"))
}

/// Write `doc` to `out` encoded as `format` (`new`, `convert`, `run`):
/// the bytes written.
fn write_deck(area: &Area, out: &Path, doc: &Presentation, format: &str, method: &str) -> Result<usize, String> {
    let bytes = engine_file::save_bytes(doc, format).map_err(|e| format!("deck.{method}: {e}"))?;
    write_out(area, out, &bytes, method)?;
    Ok(bytes.len())
}

/// Write slide `args.slide` (0-based, default 0) of `doc` to `out` as a
/// PNG whose longest edge is `args.max_side` (default 1024, 16..=4096)
/// (`render`, `run`): `{slide, width, height, bytes}`.
fn write_slide(area: &Area, out: &Path, doc: &Presentation, args: &Json, method: &str) -> Result<Json, String> {
    let slide = args["slide"].as_u64().unwrap_or(0) as usize;
    let max_side = args["max_side"].as_u64().unwrap_or(1024).clamp(16, MAX_RENDER_SIDE) as f64;
    let n = doc.slides.len();
    if slide >= n {
        return Err(format!("deck.{method}: no slide {slide} (the deck has {n})"));
    }
    let longest = doc.slide_size.width.max(doc.slide_size.height).max(1.0);
    let (png, width, height) = engine_file::render_png(doc, slide, max_side / longest, false);
    write_out(area, out, &png, method)?;
    Ok(json!({"slide": slide, "width": width, "height": height, "bytes": png.len()}))
}

/// The output format `rel`'s extension names, confined to `allowed`
/// `(extension, engine format)` pairs — never the engine's silent
/// default.
fn out_format(rel: &str, method: &str, allowed: &[(&str, &'static str)]) -> Result<&'static str, String> {
    let lower = rel.to_ascii_lowercase();
    for (ext, format) in allowed {
        if lower.ends_with(ext) {
            return Ok(format);
        }
    }
    let exts: Vec<&str> = allowed.iter().map(|(e, _)| *e).collect();
    Err(format!("deck.{method}: `out` ends in one of {}", exts.join(", ")))
}

/// `info {path}` — the engine's `document.inspect` as JSON.
fn info(args: &Json, area: &Area) -> Result<Json, String> {
    let (mut s, rel) = read_deck(args, area, "info")?;
    let mut v = s.execute("document.inspect", &json!({})).map_err(|e| format!("deck.info: {e}"))?;
    v["file"] = json!(rel);
    Ok(v)
}

/// `text {path}` — the deck as outline text (titles unindented, body
/// paragraphs tab-indented by level).
fn text(args: &Json, area: &Area) -> Result<Json, String> {
    let (s, _rel) = read_deck(args, area, "text")?;
    let st = s.doc().map_err(|e| format!("deck.text: {e}"))?;
    Ok(json!({"outline": deckcraft_format::slides_to_outline(&st.doc), "slides": st.doc.slides.len()}))
}

/// `render {path, slide?, out, max_side?}` — one slide as a PNG whose
/// longest edge is `max_side` (default 1024, at most 4096).
fn render(args: &Json, area: &Area) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    if !out_rel.to_ascii_lowercase().ends_with(".png") {
        return Err("deck.render: `out` is a .png path".into());
    }
    let out = out_path(area, out_rel, "render")?;
    let (s, _rel) = read_deck(args, area, "render")?;
    let st = s.doc().map_err(|e| format!("deck.render: {e}"))?;
    let mut answer = write_slide(area, &out, &st.doc, args, "render")?;
    answer["out"] = json!(out_rel);
    Ok(answer)
}

/// `new {out, slides: [{title, bullets?}]}` — a deck built from titles
/// and flat bullet lists, written as `.pptx` or `.deckcraft`.
fn new_deck(args: &Json, area: &Area) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    let format = out_format(out_rel, "new", &[(".pptx", "pptx"), (".deckcraft", "deckcraft")])?;
    let out = out_path(area, out_rel, "new")?;
    let slides = args["slides"].as_array().ok_or("deck.new: `slides` is a list of {title, bullets?}")?;
    if slides.is_empty() || slides.len() > MAX_NEW_SLIDES {
        return Err(format!("deck.new: `slides` is 1..={MAX_NEW_SLIDES} slides"));
    }
    // One outline line per title and bullet (the engine's own outline
    // import); line breaks and tabs inside a line become spaces.
    let clean = |s: &str, what: &str| -> Result<String, String> {
        if s.chars().count() > MAX_LINE_CHARS {
            return Err(format!("deck.new: a {what} is at most {MAX_LINE_CHARS} characters"));
        }
        Ok(s.replace(['\n', '\r', '\t'], " ").trim().to_string())
    };
    let mut outline = String::new();
    for slide in slides {
        let title = clean(slide["title"].as_str().unwrap_or(""), "title")?;
        if title.is_empty() {
            return Err("deck.new: each slide has a non-empty `title`".into());
        }
        outline.push_str(&title);
        outline.push('\n');
        let bullets: &[Json] = match &slide["bullets"] {
            Json::Null => &[],
            Json::Array(b) => b,
            other => return Err(format!("deck.new: `bullets` is a list of strings, not {other}")),
        };
        if bullets.len() > MAX_BULLETS {
            return Err(format!("deck.new: at most {MAX_BULLETS} bullets per slide"));
        }
        for bullet in bullets {
            let line = clean(bullet.as_str().ok_or("deck.new: `bullets` is a list of strings")?, "bullet")?;
            if line.is_empty() {
                continue;
            }
            outline.push('\t');
            outline.push_str(&line);
            outline.push('\n');
        }
    }
    let mut p = deckcraft_model::defaults::blank_presentation(deckcraft_model::defaults::WIDE, Default::default(), false);
    let made = deckcraft_format::outline_to_slides(&mut p, &outline);
    write_deck(area, &out, &p, format, "new")?;
    Ok(json!({"out": out_rel, "slides": made, "format": format}))
}

/// `convert {path, out}` — the deck re-encoded by `out`'s extension:
/// `.pptx`, `.deckcraft`, outline `.txt` or `.pdf`.
fn convert(args: &Json, area: &Area) -> Result<Json, String> {
    let out_rel = args["out"].as_str().unwrap_or("");
    let format = out_format(out_rel, "convert", CONVERT_OUT)?;
    let out = out_path(area, out_rel, "convert")?;
    let (s, _rel) = read_deck(args, area, "convert")?;
    let st = s.doc().map_err(|e| format!("deck.convert: {e}"))?;
    let bytes = write_deck(area, &out, &st.doc, format, "convert")?;
    Ok(json!({"out": out_rel, "format": format, "bytes": bytes}))
}

/// The command door: every command admitted by [`door`] before the engine
/// runs any (a refused one refuses the whole call, with nothing written),
/// run in order on one session over the deck at `path` or a new blank one
/// (the engine's own `file.new {blank: true}`: 16:9, the default theme, no
/// slides), whose active deck is then written to `out` under the area's
/// rules: by `out`'s extension as `convert` writes it, or one slide as
/// `render` draws it (`.png`, with `slide` and `max_side`).
fn run(args: &Json, area: &Area) -> Result<Json, String> {
    // Admit every command first: one refused id refuses the whole call, with
    // nothing opened and nothing written.
    let admitted = door()?.admit_all(&args["cmds"], area)?;
    let out = match args["out"].as_str().filter(|o| !o.is_empty()) {
        Some(rel) => {
            let format = out_format(rel, "run", RUN_OUT)?;
            Some((rel, out_path(area, rel, "run")?, format))
        }
        None => None,
    };
    let mut s = match args["path"].as_str().filter(|p| !p.is_empty()) {
        Some(_) => read_deck(args, area, "run")?.0,
        None => {
            let mut s = Session::new();
            s.execute("file.new", &json!({"blank": true})).map_err(|e| format!("deck.run: {e}"))?;
            s
        }
    };
    let mut budget = RunBudget::start(&s, !admitted.is_empty())?;
    let mut results = Vec::with_capacity(admitted.len());
    for (id, params) in admitted {
        budget.before(&s, &id, &params)?;
        let r = s.execute(&id, &params).map_err(|e| format!("deck.run {id}: {e}"))?;
        budget.after(&s, &id, &r)?;
        results.push(json!({"id": id, "result": r}));
    }
    let Some((out_rel, out, format)) = out else { return Ok(json!({"results": results, "out": Json::Null})) };
    let st = s.doc().map_err(|e| format!("deck.run: {e}"))?;
    match format {
        "png" => budget.raster(png_pixels(&st.doc, args), "the .png out")?,
        "pdf" => budget.pdf(&st.doc, "the .pdf out")?,
        _ => {}
    }
    let mut answer = match format {
        "png" => write_slide(area, &out, &st.doc, args, "run")?,
        _ => json!({"bytes": write_deck(area, &out, &st.doc, format, "run")?}),
    };
    answer["results"] = json!(results);
    answer["out"] = json!(out_rel);
    answer["format"] = json!(format);
    Ok(answer)
}

/// What a session holds, as the door's ceilings count it, over all its
/// presentations (`file.new` and `file.openBytes` each add one): slides;
/// shapes on slides, masters and layouts, inside groups too; table cells;
/// chart data; text in shapes, cells and notes; media bytes; and the first
/// shape or text whose geometry the renderer could not afford.
#[derive(Clone, Debug, Default)]
struct Held {
    slides: usize,
    shapes: usize,
    most_on_a_slide: usize,
    cells: usize,
    points: usize,
    chars: usize,
    media: u64,
    problem: Option<String>,
}

impl Held {
    fn of(s: &Session) -> Held {
        let mut held = Held::default();
        for st in s.documents() {
            let doc = &st.doc;
            held.slides += doc.slides.len();
            held.media += doc.media.iter().map(|m| m.data.len() as u64).sum::<u64>();
            for slide in &doc.slides {
                let before = held.shapes;
                held.add_shapes(&slide.shapes);
                held.most_on_a_slide = held.most_on_a_slide.max(held.shapes - before);
                held.add_text(&slide.notes);
            }
            for master in doc.masters.iter().chain(&doc.notes_master).chain(&doc.handout_master) {
                held.add_master(master);
            }
        }
        held
    }

    fn add_master(&mut self, master: &deckcraft_model::Master) {
        self.add_shapes(&master.shapes);
        for layout in &master.layouts {
            self.add_shapes(&layout.shapes);
        }
    }

    /// Count `shapes` and what they hold, groups to the depth the model's
    /// own walkers go (64), and note the first box out of
    /// [`MAX_EXTENT`].
    fn add_shapes(&mut self, shapes: &[Shape]) {
        let mut stack: Vec<(&Shape, usize)> = shapes.iter().map(|s| (s, 0)).collect();
        while let Some((shape, depth)) = stack.pop() {
            self.shapes += 1;
            if let Some(x) = &shape.xfrm {
                self.check_box(x.x, x.y, x.w, x.h);
            }
            if let Some(w) = shape.line.as_ref().and_then(|l| l.width) {
                self.check_box(0.0, 0.0, w, 0.0);
            }
            if let Some(text) = &shape.text {
                self.add_text(text);
            }
            match &shape.kind {
                ShapeKind::Group { children, child } => {
                    self.check_box(child.x, child.y, child.w, child.h);
                    if depth < 64 {
                        stack.extend(children.iter().map(|c| (c, depth + 1)));
                    }
                }
                ShapeKind::Table(t) => {
                    for cell in t.rows.iter().flat_map(|r| &r.cells) {
                        self.cells += 1;
                        self.add_text(&cell.text);
                    }
                }
                ShapeKind::Chart(c) => self.points += c.categories.len() + c.series.iter().map(|s| 1 + s.values.len()).sum::<usize>(),
                _ => {}
            }
        }
    }

    /// Count `text`'s characters, and note the first font or bullet size
    /// over the engine's own clamps (a slide size change scales them with
    /// none).
    fn add_text(&mut self, text: &TextBody) {
        for p in &text.paragraphs {
            if p.props.bullet_size.is_some_and(|k| k.is_nan() || k > BULLET_SIZE.max / 100.0) && self.problem.is_none() {
                self.problem = Some(format!("a bullet {} times its text, more than the {} the door allows", p.props.bullet_size.unwrap_or_default(), BULLET_SIZE.max / 100.0));
            }
            for (chars, size) in p.runs.iter().map(|r| (r.text.chars().count(), r.props.size)).chain([(0, p.end_props.size)]) {
                self.chars += chars;
                if size.is_some_and(|s| s.is_nan() || s > MAX_FONT_SIZE) && self.problem.is_none() {
                    self.problem = Some(format!("text of {} points, more than the {MAX_FONT_SIZE} the door allows", size.unwrap_or_default()));
                }
            }
        }
    }

    fn check_box(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if self.problem.is_none() && ![x, y, w, h].iter().all(|v| v.abs() <= MAX_EXTENT) {
            self.problem = Some(format!("a shape at {x}, {y} of {w} × {h} points, beyond the {MAX_EXTENT} points the door allows a position or size"));
        }
    }

    /// Why `self` is over the door's ceilings, if it is.
    fn over(&self) -> Option<String> {
        let counts = [
            (self.slides, MAX_RUN_SLIDES, "slides"),
            (self.shapes, MAX_RUN_SHAPES, "shapes"),
            (self.most_on_a_slide, MAX_SLIDE_SHAPES, "shapes on one slide"),
            (self.cells, MAX_RUN_CELLS, "table cells"),
            (self.points, MAX_RUN_POINTS, "chart data points"),
            (self.chars, MAX_RUN_CHARS, "characters of text"),
        ];
        if let Some(problem) = &self.problem {
            return Some(problem.clone());
        }
        if let Some((n, max, what)) = counts.into_iter().find(|(n, max, _)| n > max) {
            return Some(format!("{n} {what}, more than the {max} the door allows"));
        }
        (self.media > MAX_RUN_MEDIA).then(|| format!("{} bytes of media, more than the {MAX_RUN_MEDIA} the door allows", self.media))
    }
}

/// Every slide and master of the session, by address: one an edit changed
/// is a new allocation (the engine shares them copy-on-write).
fn deck_addresses(s: &Session) -> HashSet<usize> {
    let mut seen = HashSet::new();
    for st in s.documents() {
        seen.extend(st.doc.slides.iter().map(|x| Arc::as_ptr(x) as usize));
        seen.extend(st.doc.masters.iter().map(|x| Arc::as_ptr(x) as usize));
    }
    seen
}

/// What one `run` call has done so far, against the door's ceilings: the
/// session within [`Held`]'s ceilings after every command (which stops a
/// duplicate or paste loop, whose doubling has no count for the gate to
/// see), what its edits copy within [`MAX_RUN_COPIED_SHAPES`], what it
/// rasters within [`MAX_RUN_RASTER`] and what it answers within
/// [`MAX_RUN_RESULT_BYTES`].
struct RunBudget {
    decks: Vec<usize>,
    seen: HashSet<usize>,
    copied: Held,
    raster: u64,
    result_bytes: u64,
}

impl RunBudget {
    /// The budget of a call on `s` as opened: with commands to run, refused
    /// if the deck is already over a ceiling.
    fn start(s: &Session, runs_commands: bool) -> Result<RunBudget, String> {
        if runs_commands {
            if let Some(why) = Held::of(s).over() {
                return Err(format!("deck.run: the presentation holds {why}, so the door does not run commands on it"));
            }
        }
        Ok(RunBudget { decks: decks(s), seen: deck_addresses(s), copied: Held::default(), raster: 0, result_bytes: 0 })
    }

    /// Before command `id`: what it will raster or fragment, where the gate
    /// cannot see it.
    fn before(&mut self, s: &Session, id: &str, params: &Json) -> Result<(), String> {
        let Some(st) = s.active() else { return Ok(()) };
        match id {
            "file.render" => self.raster(render_pixels(&st.doc, params), "`file.render`"),
            "file.saveBytes" if params.get("format").and_then(Json::as_str) == Some("pdf") => self.pdf(&st.doc, "`file.saveBytes`"),
            "shape.merge" if params.get("op").and_then(Json::as_str) == Some("fragment") => {
                let named = params.get("ids").and_then(Json::as_array).map_or(0, Vec::len);
                let n = if named > 0 { named } else if params.get("id").is_some() { 1 } else { st.selection.shapes.len() };
                if n as f64 > FRAGMENTED_SHAPES.max {
                    return Err(format!("deck.run: `shape.merge` would fragment {n} shapes, more than the {} the door allows in one command", FRAGMENTED_SHAPES.max));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// After command `id`: the session within the ceilings, its edits'
    /// copies within theirs, and its answer within the results budget.
    fn after(&mut self, s: &Session, id: &str, result: &Json) -> Result<(), String> {
        if let Some(why) = Held::of(s).over() {
            return Err(format!("deck.run: `{id}` leaves the presentation with {why}"));
        }
        let now = decks(s);
        if now != self.decks {
            self.decks = now;
            let seen = deck_addresses(s);
            for st in s.documents() {
                for slide in st.doc.slides.iter().filter(|x| !self.seen.contains(&(Arc::as_ptr(x) as usize))) {
                    self.copied.add_shapes(&slide.shapes);
                    self.copied.add_text(&slide.notes);
                }
                for master in st.doc.masters.iter().filter(|x| !self.seen.contains(&(Arc::as_ptr(x) as usize))) {
                    self.copied.add_master(master);
                }
            }
            self.seen = seen;
            if self.copied.shapes > MAX_RUN_COPIED_SHAPES || self.copied.chars > 4 * MAX_RUN_CHARS {
                return Err(format!(
                    "deck.run: `{id}`: the edits of this call have written or changed {} shapes and {} characters, more than the {MAX_RUN_COPIED_SHAPES} shapes and {} characters the door allows one call (the engine keeps an undo copy of every slide an edit changes)",
                    self.copied.shapes,
                    self.copied.chars,
                    4 * MAX_RUN_CHARS
                ));
            }
        }
        self.result_bytes += json_len(result);
        if self.result_bytes > MAX_RUN_RESULT_BYTES {
            return Err(format!("deck.run: `{id}`: the results of this call total {} bytes, more than the {MAX_RUN_RESULT_BYTES} the door returns", self.result_bytes));
        }
        Ok(())
    }

    /// One raster of `pixels`: within [`MAX_RASTER`], and added to what
    /// the call rasters ([`RunBudget::add_raster`]).
    fn raster(&mut self, pixels: u64, what: &str) -> Result<(), String> {
        if pixels > MAX_RASTER {
            return Err(format!("deck.run: {what} would raster {pixels} pixels, more than the {MAX_RASTER} the door allows one raster"));
        }
        self.add_raster(pixels, what)
    }

    /// Add `pixels` to what the call rasters, within [`MAX_RUN_RASTER`].
    fn add_raster(&mut self, pixels: u64, what: &str) -> Result<(), String> {
        self.raster += pixels;
        if self.raster > MAX_RUN_RASTER {
            return Err(format!("deck.run: {what}: the rasters of this call would total {} pixels, more than the {MAX_RUN_RASTER} the door allows one call", self.raster));
        }
        Ok(())
    }

    /// A `.pdf` of `doc`: a raster of every visible slide at 200 dpi.
    fn pdf(&mut self, doc: &Presentation, what: &str) -> Result<(), String> {
        let page = pdf_page_pixels(doc);
        if page > MAX_RASTER {
            let (w, h) = (doc.slide_size.width, doc.slide_size.height);
            return Err(format!("deck.run: {what}: a {w} × {h} point slide is a {page}-pixel .pdf page, more than the {MAX_RASTER} the door allows one raster"));
        }
        self.add_raster(page * doc.slides.iter().filter(|s| !s.hidden).count() as u64, what)
    }
}

/// The address of every presentation of the session: an edit replaces its
/// presentation (`Session::edit`), so a change here means a new one.
fn decks(s: &Session) -> Vec<usize> {
    s.documents().iter().map(|st| Arc::as_ptr(&st.doc) as usize).collect()
}

/// The pixels of one raster `w` × `h` points at `scale` px a point, each
/// side clamped as the renderer clamps it (1..=16000).
fn raster_pixels(w: f64, h: f64, scale: f64) -> u64 {
    let side = |v: f64| (v * scale).ceil().clamp(1.0, 16_000.0) as u64;
    side(w) * side(h)
}

/// What `file.render` rasters: the slide at its `scale` (default 1, the
/// engine clamps it to 0.01–16).
fn render_pixels(doc: &Presentation, params: &Json) -> u64 {
    let scale = params.get("scale").and_then(Json::as_f64).filter(|v| v.is_finite()).unwrap_or(1.0).clamp(0.01, 16.0);
    raster_pixels(doc.slide_size.width, doc.slide_size.height, scale)
}

/// What one `.pdf` page rasters: its slide at 200 dpi (`save_bytes` uses
/// the PDF defaults).
fn pdf_page_pixels(doc: &Presentation) -> u64 {
    raster_pixels(doc.slide_size.width.max(1.0), doc.slide_size.height.max(1.0), 200.0 / 72.0)
}

/// What the `.png` out rasters: one slide with its longest edge at
/// `max_side` ([`write_slide`]).
fn png_pixels(doc: &Presentation, args: &Json) -> u64 {
    let max_side = args["max_side"].as_u64().unwrap_or(1024).clamp(16, MAX_RENDER_SIDE) as f64;
    let longest = doc.slide_size.width.max(doc.slide_size.height).max(1.0);
    raster_pixels(doc.slide_size.width, doc.slide_size.height, (max_side / longest).clamp(0.01, 16.0))
}

/// The length of `v` as JSON, without writing it out.
fn json_len(v: &Json) -> u64 {
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
    let mut count = Count(0);
    let _ = serde_json::to_writer(&mut count, v);
    count.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real two-slide deck written through the engine's own pptx
    /// export: the fixture every test reads back.
    fn make(host: &Path) -> &'static str {
        let made = dispatch(
            "new",
            &json!({"out": "talk.pptx", "slides": [
                {"title": "Why decks", "bullets": ["One engine", "No UI"]},
                {"title": "How it ports"}
            ]}),
            host,
        )
        .unwrap();
        assert_eq!(made["slides"], json!(2), "{made}");
        assert_eq!(made["format"], json!("pptx"));
        "talk.pptx"
    }

    #[test]
    fn new_writes_a_real_pptx_into_the_family_area() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let rel = make(host);
        let on_disk = host.join("deck").join(rel);
        let bytes = std::fs::read(&on_disk).unwrap();
        assert!(bytes.starts_with(b"PK"), "a real zip-based pptx under <host_dir>/deck");

        assert!(dispatch("new", &json!({"out": "x.exe", "slides": [{"title": "t"}]}), host).is_err(), "only .pptx/.deckcraft");
        assert!(dispatch("new", &json!({"out": "x.pptx", "slides": []}), host).is_err(), "no empty deck");
        assert!(dispatch("new", &json!({"out": "x.pptx", "slides": [{"title": "  "}]}), host).is_err(), "no blank title");
    }

    #[test]
    fn info_reads_slides_and_titles() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let rel = make(host);
        let v = dispatch("info", &json!({"path": rel}), host).unwrap();
        assert_eq!(v["file"], json!(rel));
        let slides = v["slides"].as_array().unwrap();
        assert_eq!(slides.len(), 2, "{v}");
        assert_eq!(slides[0]["title"], json!("Why decks"));
        assert_eq!(slides[1]["title"], json!("How it ports"));
    }

    #[test]
    fn text_extracts_the_outline() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let rel = make(host);
        let v = dispatch("text", &json!({"path": rel}), host).unwrap();
        assert_eq!(v["slides"], json!(2));
        let outline = v["outline"].as_str().unwrap();
        assert!(outline.contains("Why decks\n\tOne engine\n\tNo UI\nHow it ports"), "{outline:?}");
    }

    #[test]
    fn render_writes_a_png_preview() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let rel = make(host);
        let v = dispatch("render", &json!({"path": rel, "slide": 1, "out": "prev/s2.png", "max_side": 256}), host).unwrap();
        assert_eq!(v["slide"], json!(1));
        let (w, h) = (v["width"].as_u64().unwrap(), v["height"].as_u64().unwrap());
        assert_eq!(w.max(h), 256, "{v}");
        assert!(w.min(h) > 0);
        let png = std::fs::read(host.join("deck").join("prev/s2.png")).unwrap();
        assert_eq!(v["bytes"].as_u64().unwrap(), png.len() as u64);
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']), "a real PNG");

        assert!(dispatch("render", &json!({"path": rel, "slide": 9, "out": "x.png"}), host).is_err(), "no slide 9");
        assert!(dispatch("render", &json!({"path": rel, "out": "x.jpg"}), host).is_err(), "PNG only");
    }

    #[test]
    fn convert_roundtrips_native_outline_and_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let rel = make(host);

        dispatch("convert", &json!({"path": rel, "out": "talk.deckcraft"}), host).unwrap();
        let back = dispatch("info", &json!({"path": "talk.deckcraft"}), host).unwrap();
        assert_eq!(back["slides"].as_array().unwrap().len(), 2, "pptx → native roundtrip");

        dispatch("convert", &json!({"path": rel, "out": "talk.txt"}), host).unwrap();
        let txt = std::fs::read_to_string(host.join("deck/talk.txt")).unwrap();
        assert!(txt.contains("Why decks"), "{txt:?}");

        dispatch("convert", &json!({"path": rel, "out": "talk.pdf"}), host).unwrap();
        assert!(std::fs::read(host.join("deck/talk.pdf")).unwrap().starts_with(b"%PDF"), "a real PDF");

        assert!(dispatch("convert", &json!({"path": rel, "out": "talk.bin"}), host).is_err(), "no silent native default");
    }

    #[test]
    fn paths_stay_inside_the_deck_area() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        // A neighbouring family's data in the shared host dir must stay
        // out of reach.
        std::fs::create_dir_all(host.join("calendar")).unwrap();
        std::fs::write(host.join("calendar/events.json"), b"[]").unwrap();
        make(host);
        for bad in ["../up.pptx", "/etc/x.pptx", "a/../../up.pptx", "../calendar/events.json", ""] {
            assert!(dispatch("info", &json!({"path": bad}), host).is_err(), "read {bad}");
            assert!(dispatch("new", &json!({"out": bad, "slides": [{"title": "t"}]}), host).is_err(), "write {bad}");
            assert!(dispatch("render", &json!({"path": "talk.pptx", "out": bad}), host).is_err(), "render to {bad}");
            assert!(dispatch("convert", &json!({"path": "talk.pptx", "out": bad}), host).is_err(), "convert to {bad}");
        }
        assert_eq!(std::fs::read(host.join("calendar/events.json")).unwrap(), b"[]", "untouched");
    }

    /// A call as App Hub hands it to the service.
    fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("deck.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
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

    fn slides() -> Json {
        json!([{"title": "Why decks", "bullets": ["One engine"]}, {"title": "How it ports"}])
    }

    /// Without the shell's resolver a call works in `<host dir>/deck`, as
    /// before, and may replace.
    #[test]
    fn without_a_resolver_the_area_is_the_deck_subdir() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        serve(&Slot::new(), &service_call("new", json!({"out": "talk.pptx", "slides": slides()}), host, false)).unwrap();
        assert!(host.join("deck/talk.pptx").is_file(), "writes land inside the area");
        assert!(!host.join("talk.pptx").exists(), "never beside it");
        serve(&Slot::new(), &service_call("new", json!({"out": "talk.pptx", "slides": slides()}), host, false)).unwrap();
    }

    /// With the shell's resolver every path is relative to the caller's own
    /// folder and stays inside it, through a link too.
    #[test]
    fn the_resolver_root_is_used_and_paths_stay_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        let host = dir.path().join(".host");
        serve(&areas, &service_call("new", json!({"out": "talks/t.pptx", "slides": slides()}), &host, false)).unwrap();
        assert!(root.join("talks/t.pptx").is_file() && !host.exists() && !root.join("deck").exists());
        let v = serve(&areas, &service_call("info", json!({"path": "talks/t.pptx"}), &host, false)).unwrap();
        assert_eq!(v["slides"].as_array().unwrap().len(), 2, "{v}");
        std::fs::write(dir.path().join("beside.pptx"), b"x").unwrap();
        for bad in ["../beside.pptx", "/etc/hosts", "talks/../../beside.pptx"] {
            assert!(serve(&areas, &service_call("info", json!({"path": bad}), &host, false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("convert", json!({"path": "talks/t.pptx", "out": bad}), &host, true)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("out")).unwrap();
            assert!(serve(&areas, &service_call("render", json!({"path": "talks/t.pptx", "out": "out/s.png"}), &host, true)).is_err());
            assert!(!dir.path().join("s.png").exists());
        }
    }

    /// An agent's call never replaces a file, before the engine runs; an
    /// app's own foreground call may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        serve(&areas, &service_call("new", json!({"out": "t.pptx", "slides": slides()}), dir.path(), false)).unwrap();
        std::fs::write(dir.path().join("t.txt"), b"keep me").unwrap();
        for (method, args) in [
            ("convert", json!({"path": "t.pptx", "out": "t.txt"})),
            ("new", json!({"out": "t.pptx", "slides": slides()})),
        ] {
            let refused = serve(&areas, &service_call(method, args, dir.path(), false)).unwrap_err();
            assert!(refused.contains("already exists"), "{method}: {refused}");
        }
        std::fs::write(dir.path().join("s.png"), b"keep").unwrap();
        assert!(serve(&areas, &service_call("render", json!({"path": "t.pptx", "out": "s.png"}), dir.path(), false)).is_err());
        assert_eq!(std::fs::read(dir.path().join("t.txt")).unwrap(), b"keep me");
        serve(&areas, &service_call("convert", json!({"path": "t.pptx", "out": "t.txt"}), dir.path(), true)).unwrap();
        assert!(std::fs::read_to_string(dir.path().join("t.txt")).unwrap().contains("Why decks"));
    }

    /// What a call writes must fit what is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let refused = serve(&resolver(dir.path(), Some(100)), &service_call("new", json!({"out": "t.pptx", "slides": slides()}), dir.path(), true)).unwrap_err();
        assert!(refused.contains("bytes left"), "{refused}");
        assert!(!dir.path().join("t.pptx").exists());
        serve(&resolver(dir.path(), Some(1 << 22)), &service_call("new", json!({"out": "t.pptx", "slides": slides()}), dir.path(), true)).unwrap();
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.slides"));
        assert!(!may_call("org.example.anything"));
        assert!(!may_call(""));
    }

    /// The agent tools (`tools.json`) pass App Hub's own loader, as the shell
    /// reads them, keep the object schemas octos takes both ways, and name
    /// only methods this service dispatches.
    #[test]
    fn the_agent_tools_pass_app_hubs_loader_and_name_real_methods() {
        use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest};
        let (manifest, _) = ToolManifest::load(TOOLS_JSON, "deck", ToolHost::Contained, false).unwrap();
        assert!(!manifest.tools.is_empty());
        let dir = tempfile::tempdir().unwrap();
        for tool in &manifest.tools {
            assert_eq!(tool.implemented_by, ImplementedBy::HostService, "{}", tool.name);
            assert!(tool.host_method.is_none(), "{}: the shell routes each tool to the method of its own name", tool.name);
            for schema in [&tool.input_schema, &tool.output_schema] {
                assert_eq!(schema["type"], json!("object"), "{}: octos takes object schemas only", tool.name);
            }
            assert!(tool.description.is_ascii(), "{}: descriptions stay within octos's byte limit", tool.name);
            let method = tool.name.strip_prefix("deck.").unwrap();
            if let Err(e) = dispatch(method, &json!({}), dir.path()) {
                assert!(!e.contains("is not a method"), "{}: {e}", tool.name);
            }
        }
    }

    /// A 12x8 RGB PNG (two colour bands), as the photo service's tests use.
    fn png() -> Vec<u8> {
        const PNG: &str = "89504e470d0a1a0a0000000d494844520000000c000000080802000000428689a60000001d49444154789c6378616383866c725ea021063a2bb279d14310d15911005b9497817c6155610000000049454e44ae426082";
        (0..PNG.len()).step_by(2).map(|i| u8::from_str_radix(&PNG[i..i + 2], 16).unwrap()).collect()
    }

    /// Half a second of 8 kHz mono PCM as a WAV, as the engine's media tests build it.
    fn wav() -> Vec<u8> {
        let rate = 8000u32;
        let data: Vec<u8> = (0..rate / 2).flat_map(|i| (((i as f32 * 0.3).sin() * 3000.0) as i16).to_le_bytes()).collect();
        let mut b = b"RIFF".to_vec();
        b.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 2).to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(&data);
        b
    }

    /// The door runs allowlisted commands in a temporary area and writes a
    /// new deck: `safe` commands build it from a blank one, and nothing
    /// outside the area is touched.
    #[test]
    fn the_door_runs_allowlisted_commands_in_its_area() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let made = serve(
            &areas,
            &service_call(
                "run",
                json!({"cmds": [
                    {"id": "slide.new", "params": {"layout": "title", "title": "Quarterly review"}},
                    {"id": "slide.new", "params": {"title": "Revenue", "body": "Grew twelve percent"}},
                    {"id": "document.inspect"}
                ], "out": "review.pptx"}),
                dir.path(),
                false,
            ),
        )
        .unwrap();
        assert_eq!(made["out"], json!("review.pptx"), "{made}");
        assert_eq!(made["format"], json!("pptx"));
        assert_eq!(made["results"].as_array().unwrap().len(), 3);
        assert_eq!(made["results"][2]["result"]["slides"].as_array().unwrap().len(), 2, "a blank deck gained two slides: {made}");
        let back = serve(&areas, &service_call("text", json!({"path": "review.pptx"}), dir.path(), false)).unwrap();
        assert_eq!(back["outline"], json!("Quarterly review\nRevenue\n\tGrew twelve percent\n"), "{back}");
        // An existing deck, edited and written beside itself as outline
        // text; a query without `out` writes nothing.
        let edited = serve(
            &areas,
            &service_call("run", json!({"path": "review.pptx", "cmds": [{"id": "slide.last"}, {"id": "slide.new", "params": {"title": "Costs"}}], "out": "review-2.txt"}), dir.path(), false),
        )
        .unwrap();
        assert_eq!(edited["format"], json!("outline"), "{edited}");
        assert!(std::fs::read_to_string(dir.path().join("review-2.txt")).unwrap().ends_with("Costs\n"));
        let query = serve(&areas, &service_call("run", json!({"path": "review.pptx", "cmds": [{"id": "slide.inspect", "params": {"index": 1}}]}), dir.path(), false)).unwrap();
        assert!(query["out"].is_null() && query["results"][0]["result"]["title"] == json!("Revenue"), "{query}");
        let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names.len(), 2, "only the two outputs: {names:?}");
    }

    /// Every kind of `out` the door writes: the deck as `convert` writes it
    /// (`.pptx`, `.deckcraft`, outline `.txt`, `.pdf`), one slide as `render`
    /// draws it (`.png`), each from the session after the commands.
    #[test]
    fn the_door_writes_every_out_kind() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        serve(&areas, &service_call("new", json!({"out": "talk.pptx", "slides": slides()}), dir.path(), false)).unwrap();
        let run = |out: &str, extra: Json| {
            let mut args = json!({"path": "talk.pptx", "cmds": [{"id": "slide.last"}, {"id": "slide.new", "params": {"title": "Questions"}}], "out": out});
            for (k, v) in extra.as_object().unwrap() {
                args[k] = v.clone();
            }
            serve(&areas, &service_call("run", args, dir.path(), false))
        };
        for (out, format) in [("k.pptx", "pptx"), ("k.deckcraft", "deckcraft")] {
            let v = run(out, json!({})).unwrap();
            assert_eq!((v["out"].as_str(), v["format"].as_str()), (Some(out), Some(format)), "{v}");
            assert_eq!(v["bytes"].as_u64().unwrap(), std::fs::metadata(dir.path().join(out)).unwrap().len());
            let info = serve(&areas, &service_call("info", json!({"path": out}), dir.path(), false)).unwrap();
            assert_eq!(info["slides"].as_array().unwrap().len(), 3, "{out}: {info}");
        }
        let txt = run("k.txt", json!({})).unwrap();
        assert_eq!(txt["format"], json!("outline"), "{txt}");
        assert_eq!(std::fs::read_to_string(dir.path().join("k.txt")).unwrap(), "Why decks\n\tOne engine\nHow it ports\nQuestions\n");
        let pdf = run("k.pdf", json!({})).unwrap();
        assert_eq!(pdf["format"], json!("pdf"), "{pdf}");
        assert!(std::fs::read(dir.path().join("k.pdf")).unwrap().starts_with(b"%PDF"));
        let png = run("k.png", json!({"slide": 2, "max_side": 256})).unwrap();
        assert_eq!((png["format"].as_str(), png["slide"].as_u64()), (Some("png"), Some(2)), "{png}");
        let (w, h) = (png["width"].as_u64().unwrap(), png["height"].as_u64().unwrap());
        assert_eq!(w.max(h), 256, "{png}");
        let bytes = std::fs::read(dir.path().join("k.png")).unwrap();
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']) && png["bytes"].as_u64().unwrap() == bytes.len() as u64);
        let default = run("first.png", json!({})).unwrap();
        assert_eq!((default["slide"].as_u64(), default["width"].as_u64().unwrap().max(default["height"].as_u64().unwrap())), (Some(0), 1024), "{default}");
        // A slide the deck does not have, or an extension the door does not
        // write, writes nothing.
        assert!(run("none.png", json!({"slide": 9})).unwrap_err().contains("no slide 9 (the deck has 3)"));
        assert!(run("k.jpg", json!({})).unwrap_err().contains("`out` ends in one of"));
        assert!(!dir.path().join("none.png").exists() && !dir.path().join("k.jpg").exists());
    }

    /// Every class but `safe` (and the reviewed reads) is refused, and so is
    /// an id the classification does not know, before any command runs: a
    /// refused id anywhere in the list writes nothing. (deckcraft has no
    /// `code` or `network` command.)
    #[test]
    fn the_door_refuses_every_other_class_and_unknown_ids() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let refused = |id: &str, params: Json| {
            let cmds = json!([{"id": "slide.new", "params": {"title": "x"}}, {"id": id, "params": params}]);
            serve(&areas, &service_call("run", json!({"cmds": cmds, "out": "x.pptx"}), dir.path(), false)).unwrap_err()
        };
        for (id, class) in [
            ("media.play", "device"),
            ("media.toggle", "device"),
            ("media.seek", "device"),
            ("show.fromStart", "device"),
            ("show.fromCurrent", "device"),
            ("file.save", "host"),
            ("file.saveAs", "host"),
            ("file.saveTemplate", "host"),
        ] {
            let e = refused(id, json!({"path": "x.pptx"}));
            assert!(e.contains(&format!("`{id}` is classed {class}")), "{id}: {e}");
        }
        for id in ["file.open", "file.export", "file.close", "file.recovery.save", "file.recovery.list", "file.recovery.open", "file.recovery.discard", "shape.fill", "design.background"] {
            let e = refused(id, json!({"path": "elsewhere.pptx"}));
            assert!(e.contains(&format!("`{id}` reads or writes files")) && e.contains("not reviewed to run through it"), "{id}: {e}");
        }
        assert!(refused("deck.secret", json!({})).contains("not a reviewed deck command"));
        assert!(refused("Slide.New", json!({})).contains("not a reviewed deck command"), "ids match exactly");
        assert!(!dir.path().join("x.pptx").exists(), "nothing written");
        let too_many: Vec<Json> = (0..65).map(|_| json!({"id": "slide.new"})).collect();
        assert!(serve(&areas, &service_call("run", json!({"cmds": too_many}), dir.path(), false)).unwrap_err().contains("at most 64"));
        assert!(serve(&areas, &service_call("run", json!({"cmds": [{"params": {}}]}), dir.path(), false)).unwrap_err().contains("each command has an `id`"));
        assert!(serve(&areas, &service_call("run", json!({"out": "y.pptx"}), dir.path(), false)).unwrap_err().contains("`cmds` is a list"));
    }

    /// The reviewed picture reads take a file inside the area only and embed
    /// it; the media and zip commands are held back (#448), from a file or
    /// inline data alike; the `file` commands with an undocumented read, and
    /// `file.close`, stay refused; the door never writes over an existing
    /// `out`, and keeps to the quota and to the area.
    #[test]
    fn the_doors_file_reads_and_writes_keep_the_areas_rules() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        std::fs::create_dir(dir.path().join("pics")).unwrap();
        std::fs::write(dir.path().join("pics/dot.png"), png()).unwrap();
        std::fs::create_dir(dir.path().join("sound")).unwrap();
        std::fs::write(dir.path().join("sound/tone.wav"), wav()).unwrap();
        let made = serve(
            &areas,
            &service_call(
                "run",
                json!({"cmds": [
                    {"id": "slide.new", "params": {"layout": "blank"}},
                    {"id": "insert.picture", "params": {"path": "pics/dot.png"}},
                    {"id": "picture.change", "params": {"path": "pics/dot.png"}}
                ], "out": "media.pptx"}),
                dir.path(),
                false,
            ),
        )
        .unwrap();
        assert_eq!(made["out"], json!("media.pptx"), "{made}");
        let info = serve(&areas, &service_call("info", json!({"path": "media.pptx"}), dir.path(), false)).unwrap();
        let types: Vec<&str> = info["media"].as_array().unwrap().iter().filter_map(|m| m["type"].as_str()).collect();
        assert!(types.contains(&"image/png"), "the picture was embedded: {info}");
        // Hostile media and zips: deckcraft sizes what it allocates from the
        // data's own headers (#448), so the door refuses every route to its
        // media and zip parsers, from a file in the area or inline data,
        // before the engine sees a byte.
        // The refusal comes before any byte is decoded, so a stub will do.
        let clip = "UklGRiQAAABXQVZF";
        for (id, params) in [
            ("insert.audio", json!({"path": "sound/tone.wav"})),
            ("insert.audio", json!({"data": clip, "name": "tone.wav"})),
            ("insert.video", json!({"path": "sound/tone.wav"})),
            ("insert.video", json!({"data": clip, "name": "clip.mp4"})),
            ("media.info", json!({})),
            ("media.posterFrame", json!({"ms": 0})),
            ("file.openBytes", json!({"name": "bomb.deckcraft", "data": clip})),
        ] {
            let cmds = json!([{"id": "slide.new"}, {"id": id, "params": params}]);
            let e = serve(&areas, &service_call("run", json!({"cmds": cmds, "out": "held.pptx"}), dir.path(), false)).unwrap_err();
            assert!(e.starts_with(&format!("deck.run: `{id}` is held back from the door: deckcraft can abort the shell process")) && e.contains("(#448)"), "{id}: {e}");
        }
        assert!(!dir.path().join("held.pptx").exists(), "nothing written");
        // Only a file inside the area, for every reviewed read.
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.png"), png()).unwrap();
        let secret = outside.path().join("secret.png").to_string_lossy().into_owned();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path().join("secret.png"), dir.path().join("link.png")).unwrap();
        for id in ["insert.picture", "picture.change"] {
            for bad in [secret.as_str(), "../secret.png", "pics/../../secret.png", "link.png", "pics", "missing.png"] {
                let cmds = json!([{"id": "slide.new"}, {"id": id, "params": {"path": bad}}]);
                let e = serve(&areas, &service_call("run", json!({"cmds": cmds, "out": "leak.pptx"}), dir.path(), false)).unwrap_err();
                assert!(e.contains(&format!("deck.run: `{id}`: `path`: ")), "{id} {bad}: the reviewed read's own check: {e}");
                assert!(!e.contains(outside.path().to_string_lossy().as_ref()), "{id} {bad}: an error never spells a host path: {e}");
            }
        }
        // A non-string `picture` makes these read `path`: refused whatever
        // it names, inside the area or out; and so is `file.close`.
        for (id, params) in [
            ("shape.fill", json!({"picture": null, "path": secret})),
            ("shape.fill", json!({"picture": null, "path": "pics/dot.png"})),
            ("design.background", json!({"picture": null, "path": secret})),
            ("design.background", json!({"picture": 0, "path": "pics/dot.png"})),
            ("file.close", json!({})),
        ] {
            let cmds = json!([{"id": "slide.new"}, {"id": "shape.insert", "params": {"preset": "rect"}}, {"id": id, "params": params}]);
            let e = serve(&areas, &service_call("run", json!({"cmds": cmds, "out": "leak.pptx"}), dir.path(), false)).unwrap_err();
            assert!(e.contains(&format!("`{id}` reads or writes files")), "{id}: {e}");
        }
        assert!(!dir.path().join("leak.pptx").exists(), "nothing written");
        // What one call's reads may total is capped before the engine reads.
        let big = std::fs::File::create(dir.path().join("pics/big.png")).unwrap();
        big.set_len(MAX_DECK_BYTES + 1).unwrap();
        let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "slide.new"}, {"id": "insert.picture", "params": {"path": "pics/big.png"}}]}), dir.path(), false)).unwrap_err();
        assert!(e.contains("the files this call reads total more than 67108864 bytes"), "{e}");
        // Never over an existing file, within the quota, inside the area.
        std::fs::write(dir.path().join("taken.pptx"), b"keep").unwrap();
        let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "slide.new"}], "out": "taken.pptx"}), dir.path(), false)).unwrap_err();
        assert!(e.contains("`taken.pptx` already exists"), "{e}");
        assert_eq!(std::fs::read(dir.path().join("taken.pptx")).unwrap(), b"keep");
        let e = serve(&resolver(dir.path(), Some(64)), &service_call("run", json!({"cmds": [{"id": "slide.new"}], "out": "big.pptx"}), dir.path(), false)).unwrap_err();
        assert!(e.contains("bytes left"), "{e}");
        assert!(!dir.path().join("big.pptx").exists());
        for bad in ["../up.pptx", "/etc/x.pptx", "pics/../../up.pptx"] {
            assert!(serve(&areas, &service_call("run", json!({"cmds": [], "out": bad}), dir.path(), false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("run", json!({"cmds": [], "path": bad}), dir.path(), false)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), dir.path().join("up")).unwrap();
            assert!(serve(&areas, &service_call("run", json!({"cmds": [{"id": "slide.new"}], "out": "up/made.pptx"}), dir.path(), false)).is_err());
            assert!(!outside.path().join("made.pptx").exists());
        }
    }

    /// The door's gate is built from the generated classification and the
    /// reviewed reads, which must be `file` commands of the catalog: every
    /// `safe` id and the two picture reads run, but for the media and zip
    /// commands held back (#448), and nothing else.
    #[test]
    fn the_door_is_built_from_the_reviewed_classification() {
        let door = door().unwrap();
        for id in ["slide.new", "text.set", "document.inspect", "slide.inspect", "file.new", "file.saveBytes", "insert.picture", "picture.change"] {
            assert!(door.runs(id), "{id}");
        }
        for id in ["file.open", "file.export", "file.close", "file.save", "file.recovery.open", "shape.fill", "design.background", "media.play", "show.fromStart"] {
            assert!(!door.runs(id), "{id}");
        }
        for held in REVIEWED.held {
            assert!(!door.runs(held.id), "{} is held back", held.id);
        }
        let safety: Json = serde_json::from_str(include_str!("../skill/safety.json")).unwrap();
        let classes = safety["commands"].as_object().unwrap();
        let safe = classes.values().filter(|c| *c == "safe").count();
        let held_safe = REVIEWED.held.iter().filter(|h| classes[h.id] == "safe").count();
        assert_eq!(door.runnable().len(), safe - held_safe + REVIEWED.file_reads.len());
    }

    /// Every `deck.run` call the skill's examples show runs, in order,
    /// in one area as the system agent's (with the files they name placed
    /// there first), and writes its `out`: the skill teaches commands that
    /// work.
    #[test]
    fn the_skill_examples_run() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        std::fs::write(dir.path().join("chart.png"), png()).unwrap();
        let body = include_str!("../skill/SKILL.md");
        let examples = body.split("\n## Examples").nth(1).and_then(|rest| rest.split("\n## ").next()).unwrap();
        let mut ran = 0;
        for span in examples.split('`').skip(1).step_by(2) {
            let Some(args) = span.strip_prefix("deck.run ") else { continue };
            let args: Json = serde_json::from_str(args).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            let got = serve(&areas, &service_call("run", args.clone(), dir.path(), false)).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            assert_eq!(got["results"].as_array().map(Vec::len), args["cmds"].as_array().map(Vec::len), "{got}");
            if let Some(out) = args["out"].as_str() {
                assert!(dir.path().join(out).is_file(), "`{span}` wrote no {out}");
            }
            ran += 1;
        }
        assert!(ran >= 6, "{ran} examples");
        let outline = std::fs::read_to_string(dir.path().join("launch.txt")).unwrap();
        assert_eq!(outline, "Launch plan\nGoals\n\tShip in May\n\tTwo pilots\nNext steps\n\tBudget\n\tHiring\n");
        let info = serve(&areas, &service_call("info", json!({"path": "launch-2.pptx"}), dir.path(), false)).unwrap();
        assert_eq!(info["slides"].as_array().unwrap().len(), 4, "{info}");
    }

    /// One `deck.run` call working in `dir`, as the system agent makes it.
    fn run_in(dir: &Path, args: Json) -> Result<Json, String> {
        serve(&resolver(dir, None), &service_call("run", args, dir, false))
    }

    /// Every limit of the door: a command at the cap passes the gate, one
    /// over is refused before anything runs, naming the parameter and the
    /// cap.
    #[test]
    fn the_doors_limits_pass_at_their_cap_and_refuse_one_over() {
        let dir = tempfile::tempdir().unwrap();
        let area = Area::new(dir.path(), None, false);
        let gate = |id: &str, params: Json| door().unwrap().admit_all(&json!([{"id": id, "params": params}]), &area).map(|_| ());
        let cells = |n: usize| json!({"cells": vec![json!([0, 0]); n]});
        let chart = |n: usize| json!({"series": [{"name": "s", "values": vec![1.0; n - 1]}]});
        let outline = |n: usize| json!({"text": vec!["Slide\n\tbullet"; n].join("\n")});
        let breaks = |n: usize| json!({"text": "a\n".repeat(n)});
        let ids = |n: u32| json!({"op": "fragment", "ids": (1..=n).collect::<Vec<_>>()});
        let mut cases: Vec<(&str, Json, Json, String)> = vec![
            ("insert.table", json!({"rows": 75, "cols": 75}), json!({"rows": 76, "cols": 75}), "`rows` × `cols` is 5700, more than the 5625 cells".into()),
            ("table.selectCells", json!({"from": [0, 0], "to": [74, 74]}), json!({"from": [0, 0], "to": [75, 74]}), "asks for 5700 cells, more than the 5625".into()),
            ("table.cellFill", cells(5625), cells(5626), "asks for 5626 cells, more than the 5625".into()),
            ("insert.chart", chart(10_000), chart(10_001), "asks for 10001 chart data points, more than the 10000".into()),
            ("chart.data", chart(10_000), chart(10_001), "asks for 10001 chart data points, more than the 10000".into()),
            ("design.slideSize", json!({"w": 1920, "h": 1080}), json!({"w": 1921, "h": 1080}), "`w` × `h` is 2074680, more than the 2073600 square points of slide".into()),
            ("file.new", json!({"size": [1920, 1080]}), json!({"size": [1921, 1080]}), "`size.0` × `size.1` is 2074680, more than the 2073600 square points of slide".into()),
            ("slide.fromOutline", outline(200), outline(201), "asks for 201 slides, more than the 200".into()),
            ("shape.merge", ids(8), ids(9), "asks for 9 shapes to fragment, more than the 8".into()),
            ("format.bullets", json!({"size": 400}), json!({"size": 401}), "`size` is 401, more than the 400 percent bullets".into()),
            ("format.textOutline", json!({"width": 1584}), json!({"width": 1585}), "`width` is 1585, more than the 1584 points of outline".into()),
        ];
        for id in ["text.insert", "insert.symbol", "edit.paste", "edit.pasteText"] {
            cases.push((id, breaks(1000), breaks(1001), "asks for 1001 line breaks, more than the 1000".into()));
        }
        let rect = |n: f64| json!({"rect": [0.0, 0.0, n, 10.0]});
        for id in ["shape.insert", "insert.textBox", "insert.picture", "insert.table", "insert.chart", "insert.actionButton", "master.insertPlaceholder"] {
            cases.push((id, rect(100_000.0), rect(100_001.0), "asks for 100001 points of position or size, more than the 100000".into()));
        }
        for (id, at, over) in [
            ("shape.setBounds", json!({"x": -100_000, "y": 0, "w": 10, "h": 10}), json!({"x": -100_001, "y": 0, "w": 10, "h": 10})),
            ("shape.resize", json!({"w": 100_000, "lockAspect": true}), json!({"w": 100_001, "lockAspect": true})),
            ("shape.move", json!({"dx": 100_000}), json!({"dx": 100_001})),
            ("arrange.nudge", json!({"dx": 0, "dy": -100_000}), json!({"dx": 0, "dy": -100_001})),
            ("shape.freeform", json!({"points": [[0, 0], [100_000, 5]]}), json!({"points": [[0, 0], [5, 100_001]]})),
        ] {
            cases.push((id, at, over, "asks for 100001 points of position or size, more than the 100000".into()));
        }
        for (id, at, over, refusal) in cases {
            gate(id, at).unwrap_or_else(|e| panic!("{id} at the cap: {e}"));
            let e = gate(id, over).unwrap_err();
            assert!(e.contains(&format!("`{id}`")) && e.contains(&refusal), "{id}: {e}");
        }
        // At their caps the commands also run.
        let at_caps = json!([
            {"id": "slide.new", "params": {"layout": "blank"}},
            {"id": "insert.table", "params": {"rows": 75, "cols": 75}},
            {"id": "table.selectCells", "params": {"from": [0, 0], "to": [74, 74]}},
            {"id": "table.cellFill", "params": {"color": "FF0000"}},
            {"id": "design.slideSize", "params": {"w": 1920, "h": 1080, "scale": "none"}},
            {"id": "slide.fromOutline", "params": outline(200)},
        ]);
        run_in(dir.path(), json!({"cmds": at_caps})).unwrap();
    }

    /// A 1,000,000 × 1,000,000 table is refused before any command runs,
    /// and nothing is written.
    #[test]
    fn a_huge_table_is_refused_before_anything_runs() {
        let dir = tempfile::tempdir().unwrap();
        let cmds = json!([{"id": "slide.new", "params": {"layout": "blank"}}, {"id": "insert.table", "params": {"rows": 1e6, "cols": 1e6}}]);
        let e = run_in(dir.path(), json!({"cmds": cmds, "out": "t.pptx"})).unwrap_err();
        assert!(e.contains("`insert.table`: `rows` × `cols` is 1000000000000, more than the 5625 cells"), "{e}");
        assert!(!dir.path().join("t.pptx").exists());
    }

    /// A huge slide size is refused at the gate; a deck read from a file
    /// with one (the engine allows 4032 × 4032 points) is refused before its
    /// `.pdf` is rastered, by `file.render` over a raster and by
    /// `file.saveBytes` as PDF, while its `.png` out stays bounded.
    #[test]
    fn a_huge_slide_size_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let e = run_in(dir.path(), json!({"cmds": [{"id": "design.slideSize", "params": {"w": 4032, "h": 4032}}], "out": "big.pptx"})).unwrap_err();
        assert!(e.contains("`design.slideSize`: `w` × `h` is 16257024, more than the 2073600 square points of slide"), "{e}");
        let e = run_in(dir.path(), json!({"cmds": [{"id": "file.new", "params": {"size": [4999, 4999]}}]})).unwrap_err();
        assert!(e.contains("`file.new`: `size.0` × `size.1` is 24990001"), "{e}");
        let mut s = Session::new();
        s.execute("file.new", &json!({"blank": true})).unwrap();
        s.execute("slide.new", &json!({"title": "Poster"})).unwrap();
        s.execute("design.slideSize", &json!({"w": 4032, "h": 4032})).unwrap();
        std::fs::write(dir.path().join("poster.pptx"), engine_file::save_bytes(&s.doc().unwrap().doc, "pptx").unwrap()).unwrap();
        let e = run_in(dir.path(), json!({"path": "poster.pptx", "cmds": [], "out": "poster.pdf"})).unwrap_err();
        // The engine repairs a read deck's slide to at most 4000 points a side.
        assert!(e.contains("the .pdf out: a 4000 × 4000 point slide is a 123476544-pixel .pdf page, more than the 16777216"), "{e}");
        assert!(!dir.path().join("poster.pdf").exists());
        let e = run_in(dir.path(), json!({"path": "poster.pptx", "cmds": [{"id": "file.saveBytes", "params": {"format": "pdf"}}]})).unwrap_err();
        assert!(e.contains("`file.saveBytes`: a 4000 × 4000 point slide"), "{e}");
        run_in(dir.path(), json!({"path": "poster.pptx", "cmds": [{"id": "file.render", "params": {"scale": 1}}]})).unwrap();
        let e = run_in(dir.path(), json!({"path": "poster.pptx", "cmds": [{"id": "file.render", "params": {"scale": 2}}]})).unwrap_err();
        assert!(e.contains("`file.render` would raster 64000000 pixels, more than the 16777216"), "{e}");
        let png = run_in(dir.path(), json!({"path": "poster.pptx", "cmds": [], "out": "poster.png", "max_side": 4096})).unwrap();
        assert_eq!(png["width"], json!(4096), "{png}");
    }

    /// Select all and duplicate doubles a slide's shapes each round, with no
    /// count for the gate to see: the ceiling refuses the round that crosses
    /// it, and nothing is written.
    #[test]
    fn a_duplicate_loop_is_stopped_by_the_ceiling() {
        let dir = tempfile::tempdir().unwrap();
        let mut cmds = vec![json!({"id": "slide.new", "params": {"layout": "blank"}}), json!({"id": "shape.insert", "params": {"preset": "rect", "rect": [10, 10, 20, 20]}})];
        for _ in 0..12 {
            cmds.extend([json!({"id": "edit.selectAll"}), json!({"id": "edit.duplicate"})]);
        }
        let e = run_in(dir.path(), json!({"cmds": cmds, "out": "loop.pptx"})).unwrap_err();
        assert!(e.contains("`edit.duplicate` leaves the presentation with 2048 shapes on one slide, more than the 2000 the door allows"), "{e}");
        assert!(!dir.path().join("loop.pptx").exists());
        // Slides double the same way, against their own ceiling.
        let mut cmds = vec![json!({"id": "slide.new", "params": {"layout": "blank"}})];
        for round in 0..10 {
            cmds.push(json!({"id": "slide.selectSlides", "params": {"indices": (0..1u64 << round).collect::<Vec<_>>()}}));
            cmds.push(json!({"id": "slide.duplicate"}));
        }
        let e = run_in(dir.path(), json!({"cmds": cmds})).unwrap_err();
        assert!(e.contains("`slide.duplicate` leaves the presentation with 512 slides, more than the 500 the door allows"), "{e}");
    }

    /// A shape kept to an extreme aspect ratio, or a slide size scaled into
    /// the shapes (`maximize` multiplies them by up to 56 a command), grows
    /// past what the renderer can draw without any parameter showing it:
    /// the check of every shape after every command refuses it.
    #[test]
    fn geometry_grown_past_the_renderer_is_refused_after_the_command() {
        let dir = tempfile::tempdir().unwrap();
        let cmds = json!([
            {"id": "slide.new", "params": {"layout": "blank"}},
            {"id": "shape.insert", "params": {"preset": "ellipse", "rect": [0, 0, 0.001, 100_000]}},
            {"id": "shape.resize", "params": {"w": 1000, "lockAspect": true}}
        ]);
        let e = run_in(dir.path(), json!({"cmds": cmds, "out": "x.png"})).unwrap_err();
        assert!(e.contains("`shape.resize` leaves the presentation with a shape at") && e.contains("beyond the 100000 points"), "{e}");
        let maximize = |w: u32, h: u32| json!({"id": "design.slideSize", "params": {"w": w, "h": h, "scale": "maximize"}});
        let cmds = json!([
            {"id": "slide.new", "params": {"layout": "blank"}},
            {"id": "shape.insert", "params": {"preset": "rect", "rect": [100, 100, 200, 100]}},
            maximize(72, 4032), maximize(4032, 72), maximize(72, 4032)
        ]);
        let e = run_in(dir.path(), json!({"cmds": cmds, "out": "y.png"})).unwrap_err();
        assert!(e.contains("`design.slideSize` leaves the presentation with a shape at"), "{e}");
        assert!(!dir.path().join("x.png").exists() && !dir.path().join("y.png").exists());
    }

    /// Fragmenting the selection makes up to 2^k − 1 pieces: checked before
    /// the merge runs, where the gate cannot see the selection.
    #[test]
    fn fragmenting_a_large_selection_is_refused_before_it_runs() {
        let dir = tempfile::tempdir().unwrap();
        let mut cmds = vec![json!({"id": "slide.new", "params": {"layout": "blank"}})];
        cmds.extend((0..9).map(|i| json!({"id": "shape.insert", "params": {"preset": "ellipse", "rect": [i * 10, 0, 40, 40]}})));
        cmds.extend([json!({"id": "edit.selectAll"}), json!({"id": "shape.merge", "params": {"op": "fragment"}})]);
        let e = run_in(dir.path(), json!({"cmds": cmds})).unwrap_err();
        assert!(e.contains("`shape.merge` would fragment 9 shapes, more than the 8"), "{e}");
    }

    /// A `.pdf` rasters every visible slide: the rasters of one call share a
    /// budget, checked before any page is drawn.
    #[test]
    fn the_rasters_of_a_call_share_a_budget() {
        let dir = tempfile::tempdir().unwrap();
        let slides: Vec<Json> = (0..40).map(|i| json!({"id": "slide.new", "params": {"title": format!("Slide {i}")}})).collect();
        let e = run_in(dir.path(), json!({"cmds": slides, "out": "deck.pdf"})).unwrap_err();
        assert!(e.contains("the .pdf out: the rasters of this call would total 160020000 pixels, more than the 160000000"), "{e}");
        assert!(!dir.path().join("deck.pdf").exists());
        // Nine renders of 4032 × 2268 pixels and the 30 pages of the `.pdf`
        // out share the same budget.
        let mut cmds: Vec<Json> = (0..30).map(|i| json!({"id": "slide.new", "params": {"title": format!("Slide {i}")}})).collect();
        cmds.extend((0..9).map(|_| json!({"id": "file.render", "params": {"scale": 4.2}})));
        let e = run_in(dir.path(), json!({"cmds": cmds, "out": "deck.pdf"})).unwrap_err();
        assert!(e.contains("the .pdf out: the rasters of this call would total 202316184 pixels, more than the 160000000"), "{e}");
    }

    /// Every result is kept until the reply: `file.saveBytes` answers the
    /// whole deck each time, so the results of a call share a budget.
    #[test]
    fn the_results_of_a_call_share_a_budget() {
        let dir = tempfile::tempdir().unwrap();
        // 12 MB of noise as a picture's bytes: data no zip compresses.
        let mut noise = png();
        let mut x = 7u32;
        noise.extend((0..12_000_000).map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 24) as u8
        }));
        std::fs::write(dir.path().join("noise.png"), noise).unwrap();
        let mut cmds = vec![json!({"id": "slide.new", "params": {"layout": "blank"}}), json!({"id": "insert.picture", "params": {"path": "noise.png"}})];
        cmds.extend((0..5).map(|_| json!({"id": "file.saveBytes", "params": {"format": "pptx"}})));
        let e = run_in(dir.path(), json!({"cmds": cmds})).unwrap_err();
        assert!(e.contains("`file.saveBytes`: the results of this call total") && e.contains("more than the 67108864 the door returns"), "{e}");
    }
}
