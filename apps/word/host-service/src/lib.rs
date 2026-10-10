//! `octosense-word-service` — the `word` host service (ADR 0013).
//!
//! wordcraft's document engine behind typed `word.*` methods. Every call
//! is a fresh, stateless session: the service reads the file itself,
//! hands the engine bytes, and writes the engine's bytes back — the
//! engine never touches the disk. Everything is JSON at the boundary; the
//! engine's types never cross it.
//!
//! Methods (all under the `word` family; paths relative to the call's area,
//! the caller's own folder — see below):
//! - `info {path}` → pages, words, paragraphs, sections, comments and
//!   properties of a document (docx, md, html, rtf, odt, txt, json)
//! - `text {path}` → `{text, words, paragraphs}` — plain-text extraction
//! - `inspect {path, text?}` → the document's structure for agents:
//!   paragraphs with styles and runs, tables with cells
//! - `convert {path, out, format?}` → `{out, format, bytes}` — write the
//!   document as docx, md, html, rtf, odt, txt, json, pdf or png;
//!   `format` overrides `out`'s extension
//! - `new {out, text?, title?}` → `{out, words, paragraphs}` — write a
//!   minimal new document (format by `out`'s extension, usually docx)
//! - `run {path?, cmds: [{id, params?}], out?, format?}` → `{results, out,
//!   format?, bytes?}` — the command door (ADR 0013, #418): run commands of
//!   wordcraft's registry on the document at `path`, or on a new empty one,
//!   then write it to `out` (format by `format`, else `out`'s extension).
//!   Only what the door's allowlist admits runs ([`door`]): commands the
//!   reviewed classification (`skill/safety.json`) classes `safe`, and the
//!   two reviewed picture reads, whose `path` must name a file inside the
//!   area; every other id is refused before any command runs. What one call
//!   may ask for is capped: the parameters that multiply work at the gate
//!   ([`REVIEWED`]), the document after every command and a `.pdf` or `.png`
//!   out in the service ([`RunBudget`], `check_out`).
//!
//! **Where a call works** (ADR 0013, 2026-10-08): in its caller's own
//! folder, the [`Area`] the shell's resolver gives it ([`set_area_resolver`]):
//! the system agent's workspace, an app agent's account folder, or an app's
//! own storage. Without a resolver (tests, App Hub's card-host) a call works
//! in the legacy private folder `<host dir>/word`. Paths never leave the
//! area: `..`, absolute paths and symlink escapes are refused. A write that
//! may not replace (an agent's) only ever creates a new file, and what a
//! call writes stays within the area's quota ([`Area::write`]). wordcraft
//! reads only the bytes the service hands it, and through the door a
//! picture the gate found inside the area, so nothing written inside a
//! document reaches another file. The service serves system apps only until
//! ADR 0013's store capability is designed.

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::collections::HashSet;
use std::ops::RangeInclusive;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};

use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::door::{Door, FileRead, Limit, Measure, Reviewed};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};
use wordcraft_doc::para::InlineObject;
use wordcraft_doc::{Block, Document, StoryRef};
use wordcraft_engine::Session;

/// The largest document the service reads or writes (bytes).
const MAX_DOC_BYTES: u64 = 64 << 20;
/// The most text `new` accepts (bytes).
const MAX_NEW_TEXT_BYTES: usize = 4 << 20;

/// What the word engine's reviewer settled for the door beyond the classes:
/// two `file` commands that only read the picture their `path` names (whole,
/// with `std::fs::read`) and embed its bytes, or take it inline as `data`
/// (`cmd/insert.rs` `picture`, `cmd/objects.rs` `picture.change`). No
/// setter or inner id: no `safe` word command sets an app-wide variable or
/// names another command.
///
/// The limits bound the parameters that multiply work or memory. Every
/// other parameter of the 328 commands is clamped by the engine itself to
/// a size that costs little (font sizes 1–1638, columns 1–12, tab stops ≤ 64,
/// list levels ≤ 8, numbering ≤ 100,000, picture sizes ≤ 4000, line spacing
/// ≤ 132 lines, label sheets ≤ 40 × 10) or has no count at all; what grows by
/// repetition (pasting, typing, replacing the text) is bounded by the
/// service's ceilings on the document after every command ([`RunBudget`]).
static REVIEWED: Reviewed = Reviewed {
    file_reads: &[FileRead { id: "insert.picture", params: &["path"] }, FileRead { id: "picture.change", params: &["path"] }],
    setters: &[],
    inner: &[],
    limits: &[TABLE_CELLS, SPREADSHEET_CELLS, SPLIT_COLUMNS, PAGE_SIDE_LIMIT, REPLACE_COPIES],
    copies_per_call: COPIES_PER_CALL,
    held: &[],
};

/// `insert.table {rows, cols}`: rows × cols cells, each a paragraph the
/// model allocates (~0.8 KB, estimated) and every later layout places. The engine
/// clamps rows to 1000 and cols to 63: 63,000 cells, ~0.25 s to lay out and
/// ~0.7 s to write as PDF at opt-level 1. 10,000 cells (1000 × 10, 160 × 63)
/// lay out in ~35 ms and write in ~0.1 s; an everyday table is under 1,000.
const TABLE_CELLS: Limit = Limit { id: "insert.table", what: "cells", measure: Measure::Product(&["rows", "cols"]), max: 10_000.0, copies: false };

/// `insert.spreadsheet {csv}`: the engine runs `insert.table` itself, out
/// of the gate's sight, with a row per CSV line and a column per field of
/// the widest line (clamped as `insert.table` clamps them, so 5,000 lines
/// would silently become 1,000 rows). The same 10,000 cells, counted from
/// the CSV ([`csv_cells`]); without `csv` it inserts a 3 × 3 sample.
const SPREADSHEET_CELLS: Limit = Limit { id: "insert.spreadsheet", what: "cells", measure: Measure::Custom(csv_cells), max: 10_000.0, copies: false };

/// `table.split {columns}`: splitting a cell widens the table's grid for
/// every row (`Table::split_cell`), so the count multiplies the table's
/// cells. Word's and the engine's own 63 columns: what the widening adds
/// is then bounded by the table's rows and the document ceiling.
const SPLIT_COLUMNS: Limit = Limit { id: "table.split", what: "columns", measure: Measure::Product(&["columns"]), max: 63.0, copies: false };

/// `layout.pageSetup {section}`: replaces the section's page setup with no
/// check, where `layout.size` refuses a page outside 72–1584 points (1–22
/// inches). The `.png` out rasters page 1 at 2 px per point (the renderer
/// allocates up to 16,000 × 16,000 px, 1 GiB, before anything is written)
/// and a tiny page multiplies the pages every layout makes, so the same
/// range ([`page_setup_side`]): the longest side at most 1584 points, the
/// shortest at least 72.
const PAGE_SIDE_LIMIT: Limit =
    Limit { id: "layout.pageSetup", what: "points on a page side", measure: Measure::Custom(page_setup_side), max: 1584.0, copies: false };

/// `edit.replaceAll {text, with}`: a replacement longer than its match
/// multiplies the text it replaces (`a` → `aa`, repeated, doubles the
/// document each time), so its length ratio is a copy factor
/// ([`replace_ratio`]). The engine replaces at most 100,000 matches; a
/// replacement up to 1,000 times its match is well past expanding a
/// placeholder. The service also checks each one's exact growth and work
/// before it runs ([`check_replace_all`]), which sees what the factor
/// cannot (a pattern, or the last search's `text` and `with`).
const REPLACE_COPIES: Limit =
    Limit { id: "edit.replaceAll", what: "times the text it replaces", measure: Measure::Custom(replace_ratio), max: 1000.0, copies: true };

/// The most the replacements of one call may multiply the text by,
/// together: a dozen expansions of 2 to 2.5 times their match fit, a
/// doubling chain stops at its 14th step. The document ceiling bounds
/// what they really add ([`MAX_RUN_CHARS`]).
const COPIES_PER_CALL: f64 = 10_000.0;

/// `insert.spreadsheet`'s cells: lines × the widest line's fields, split
/// on the separator the engine picks from the first line. A quoted line
/// break or separator counts as a real one, so this never counts fewer
/// than the engine makes.
fn csv_cells(params: &Json) -> Result<Option<f64>, String> {
    let Some(csv) = params.get("csv").and_then(Json::as_str) else { return Ok(None) };
    let first = csv.lines().next().unwrap_or("");
    let sep = [',', ';', '\t'].into_iter().max_by_key(|c| first.matches(*c).count()).unwrap_or(',');
    let (mut rows, mut cols) = (0usize, 1usize);
    for line in csv.split(['\n', '\r']).filter(|l| !l.trim().is_empty()) {
        rows += 1;
        cols = cols.max(line.matches(sep).count() + 1);
    }
    Ok(Some(rows as f64 * cols as f64))
}

/// `layout.pageSetup`'s longest page side (points): the section's `pageW`
/// and `pageH`, or Letter's 612 and 792 for an absent one, as the engine
/// fills them in. A side under 72 points is refused; without `section`
/// the command only reports the setup.
fn page_setup_side(params: &Json) -> Result<Option<f64>, String> {
    let Some(section) = params.get("section") else { return Ok(None) };
    let side = |key: &str, default: f64| match section.get(key) {
        None => Ok(default),
        Some(v) => v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| format!("`section.{key}` is a number of points")),
    };
    let (w, h) = (side("pageW", 612.0)?, side("pageH", 792.0)?);
    if w.min(h) < 72.0 {
        return Err("a page side is at least 72 points (1 inch), as `layout.size` requires".into());
    }
    Ok(Some(w.max(h)))
}

/// `edit.replaceAll`'s copy factor: its `with` ÷ its `text`, in
/// characters. A pattern (`regex: true`) or a missing `text` (the last
/// search's) may match a single character, so `with` alone. Without a
/// `with` (the last replacement's) or with an empty `text` (no match)
/// there is nothing to bound here.
fn replace_ratio(params: &Json) -> Result<Option<f64>, String> {
    let Some(with) = params.get("with").and_then(Json::as_str) else { return Ok(None) };
    let text = match params.get("text").and_then(Json::as_str) {
        Some(t) if params.get("regex").and_then(Json::as_bool) != Some(true) => t.chars().count(),
        _ => 1,
    };
    Ok((text > 0).then(|| with.chars().count() as f64 / text as f64))
}

/// The most characters a document may hold while the door works on it,
/// counting every story (body, headers, footers, notes, comments, text
/// boxes) and the Quick Parts a call saves: about 250 pages of prose, well
/// past an everyday report. Every command's cost grows with the document
/// (at opt-level 1 a full layout of it takes ~0.1 s, its PDF ~0.15 s), so
/// this keeps each of a call's 64 commands bounded.
const MAX_RUN_CHARS: usize = 500_000;
/// The most paragraphs (a table cell holds at least one) a document may
/// hold while the door works on it (~0.8 KB of model each, estimated from
/// its types); a 50,000-cell table lays out in ~0.2 s and writes its PDF in
/// ~0.5 s at opt-level 1.
const MAX_RUN_PARAS: usize = 50_000;
/// The most picture bytes a document may hold while the door works on it:
/// a document the service can read holds at most 64 MiB, and one call's
/// picture reads total at most 64 MiB more (`door::MAX_READ_BYTES`).
const MAX_RUN_MEDIA: u64 = 128 << 20;
/// The most characters one call's edits may copy or write, together, and
/// [`MAX_RUN_COPIED_PARAS`] the most paragraphs: the engine keeps an undo
/// snapshot of every edit, which shares the paragraphs it left alone but
/// keeps the old copy of every one it changed, so 64 whole-document edits
/// would hold 64 copies of the document. Four documents at the ceiling.
const MAX_RUN_COPIED_CHARS: usize = 4 * MAX_RUN_CHARS;
const MAX_RUN_COPIED_PARAS: usize = 4 * MAX_RUN_PARAS;
/// The most pages a `.pdf` out or `document.layout` may lay out: one PDF
/// page costs ~22 µs beyond its text, one `document.layout` entry ~150
/// bytes. A huge font, line spacing or margins put a character or a line
/// on a page of its own (at 1638 points, 100,000 characters made 82,000
/// pages and a 1.8 s PDF at opt-level 1); prose at the ceiling is ~250.
const MAX_RUN_PAGES: usize = 10_000;
/// The page sides (points) a `.pdf` or `.png` out takes: `layout.size`'s
/// own range, 1–22 inches. The `.png` out is page 1 at 2 px per point, so
/// at most 3168 × 3168 px.
const PAGE_SIDES: RangeInclusive<f32> = 72.0..=1584.0;
/// The most `edit.replaceAll` work one command may ask for, in characters
/// shifted: each replacement shifts the rest of its paragraph, so 100,000
/// matches (the engine's own cap) in one 200,000-character paragraph took
/// 27 s at opt-level 1 (~1.3 ns a character); this stays under ~0.7 s.
const MAX_REPLACE_WORK: u64 = 500_000_000;
/// The most picture pixels one call's picture edits may decode, together:
/// each edit decodes the whole picture and encodes it again as PNG,
/// whatever it adds to the document (a huge picture of one colour
/// compresses to almost nothing). An edit of a 12-megapixel photo took
/// 0.1–0.4 s at opt-level 1, so three of them; a shadow counts more
/// ([`picture_edit_pixels`]). The engine refuses a picture over 80
/// megapixels on its own.
const MAX_RUN_PICTURE_PIXELS: u64 = 40_000_000;
/// The picture edits ([`MAX_RUN_PICTURE_PIXELS`]): the commands that go
/// through `cmd/objects.rs` `adjust`.
const PICTURE_EDITS: &[&str] = &[
    "picture.corrections",
    "picture.color",
    "picture.effects",
    "picture.transparency",
    "picture.removeBackground",
    "picture.compress",
    "picture.style",
    "picture.border",
    "arrange.rotate",
];

/// The command door's gate: wordcraft's reviewed classification
/// (`skill/safety.json`, generated and drift-checked by `tests/skill.rs`)
/// and [`REVIEWED`].
pub fn door() -> Result<&'static Door, String> {
    static DOOR: OnceLock<Result<Door, String>> = OnceLock::new();
    DOOR.get_or_init(|| Door::new("word", include_str!("../skill/safety.json"), &REVIEWED)).as_ref().map_err(Clone::clone)
}

/// Which apps may call the service: system apps, as News and Sheets.
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

pub struct WordService;

mod warm;
pub use warm::warm;

/// Register the `word` service with App Hub's host-service registry, and
/// start paying the engine's first-call cost on a thread of its own
/// ([`warm`]).
pub fn register() {
    register_host_service(Box::new(WordService));
    warm();
}

/// The shell's resolver: where each call works (`None` removes it, and
/// calls work in the legacy `<host dir>/word` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
}

/// The `word.*` agent tools (ADR 0013), in App Hub's `tools.json` shape:
/// `word.info` and the command door `word.run`. The shell declares them for
/// the virtual owner `os.word` and grants them to the system agent
/// (`crates/shell/src/host_tools/engines.rs`,
/// `crates/shell/src/system_chat/grants.rs` `ENGINE_TOOLS`); the other
/// methods stay for apps' own requests.
pub const TOOLS_JSON: &str = include_str!("../tools.json");

impl HostService for WordService {
    fn family(&self) -> &'static str {
        "word"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        reply.send(serve(&AREAS, &call));
    }
}

/// One call, in the area `areas` gives it.
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The word service serves system apps only.".into());
    }
    let area = areas.area(call, "word").map_err(|e| format!("word: {e}"))?;
    dispatch_in(call.method(), &call.args, &area)
        .map(|answer| area.relative_json(answer))
        .map_err(|error| area.relative_text(&error))
}

fn dispatch_in(method: &str, args: &Json, area: &Area) -> Result<Json, String> {
    match method {
        "info" => info(args, area),
        "text" => text(args, area),
        "inspect" => inspect(args, area),
        "convert" => convert(args, area),
        "new" => new(args, area),
        "run" => run(args, area),
        other => Err(format!("word.{other} is not a method of the word service")),
    }
}

/// [`dispatch_in`] in the legacy area `<host_dir>/word`, as a call without
/// the shell's resolver works.
#[cfg(test)]
fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    let area = Area::legacy(host_dir, "word");
    std::fs::create_dir_all(&area.root).map_err(|e| format!("word: {e}"))?;
    dispatch_in(method, args, &area)
}

/// A path strictly inside the call's area: relative, no `..`, no absolute
/// component; the resolved parent must stay under the area even through
/// symlinks.
fn contained(area: &Area, key: &str, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err(format!("word: `{key}` is required"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("word: `{key}` stays inside this call's folder"));
    }
    let joined = area.root.join(rel_path);
    let check_root = area.root.canonicalize().map_err(|e| format!("word: folder: {e}"))?;
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
    let resolved = deepest.canonicalize().map_err(|e| format!("word: {e}"))?;
    if !resolved.starts_with(&check_root) {
        return Err(format!("word: `{key}` stays inside this call's folder"));
    }
    Ok(joined)
}

fn arg_str<'a>(args: &'a Json, key: &str) -> Result<&'a str, String> {
    args[key].as_str().filter(|s| !s.is_empty()).ok_or_else(|| format!("word: `{key}` is required"))
}

/// Read and parse the document at `path` (relative to the area); the
/// engine parses bytes, the service does the I/O.
fn open(args: &Json, area: &Area) -> Result<(Document, String), String> {
    let rel = arg_str(args, "path")?;
    let path = contained(area, "path", rel)?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("word: {rel}: {e}"))?;
    if meta.len() > MAX_DOC_BYTES {
        return Err(format!("word: {rel} is larger than the service reads"));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("word: {rel}: {e}"))?;
    let doc = wordcraft_engine::io::open_bytes(rel, &bytes)?;
    Ok((doc, rel.to_string()))
}

/// Serialise `doc` in `name`'s format (by extension) and write it to the
/// contained `out` path under the area's rules ([`Area::write`]: no
/// replacement unless allowed, within the quota), creating parents.
fn write(doc: &Document, name: &str, out: &Path, area: &Area) -> Result<usize, String> {
    let bytes = wordcraft_engine::io::save_bytes(name, doc)?;
    if bytes.len() as u64 > MAX_DOC_BYTES {
        return Err("word: the document is larger than the service writes".into());
    }
    area.write(out, &bytes)?;
    Ok(bytes.len())
}

fn info(args: &Json, area: &Area) -> Result<Json, String> {
    let (doc, rel) = open(args, area).map_err(|e| format!("word.info: {e}"))?;
    let mut s = Session::new(doc);
    s.path = Some(rel.clone().into());
    let mut r = s.run("file.info", &json!({})).map_err(|e| format!("word.info: {e}"))?;
    r["file"] = json!(rel);
    Ok(r)
}

fn text(args: &Json, area: &Area) -> Result<Json, String> {
    let (doc, _) = open(args, area).map_err(|e| format!("word.text: {e}"))?;
    Ok(json!({
        "text": doc.plain_text(StoryRef::Body),
        "words": doc.word_count(),
        "paragraphs": doc.paragraph_count(),
    }))
}

fn inspect(args: &Json, area: &Area) -> Result<Json, String> {
    let (doc, rel) = open(args, area).map_err(|e| format!("word.inspect: {e}"))?;
    let mut s = Session::new(doc);
    s.path = Some(rel.into());
    let with_text = args["text"].as_bool().unwrap_or(true);
    s.run("document.inspect", &json!({"text": with_text})).map_err(|e| format!("word.inspect: {e}"))
}

fn convert(args: &Json, area: &Area) -> Result<Json, String> {
    let out_rel = arg_str(args, "out").map_err(|e| format!("word.convert: {e}"))?;
    let out = contained(area, "out", out_rel).map_err(|e| format!("word.convert: {e}"))?;
    // A name the call may not write is refused before the engine works.
    area.check(&out, 0).map_err(|e| format!("word.convert: {e}"))?;
    let (doc, _) = open(args, area).map_err(|e| format!("word.convert: {e}"))?;
    // `format` overrides the extension of `out`; the engine's own format
    // dispatch decides what it can save.
    let name = match args["format"].as_str().map(|f| f.trim_start_matches('.')).filter(|f| !f.is_empty()) {
        Some(fmt) => format!("out.{fmt}"),
        None => out_rel.to_string(),
    };
    let bytes = write(&doc, &name, &out, area).map_err(|e| format!("word.convert: {e}"))?;
    let format = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    Ok(json!({"out": out_rel, "format": format, "bytes": bytes}))
}

fn new(args: &Json, area: &Area) -> Result<Json, String> {
    let out_rel = arg_str(args, "out").map_err(|e| format!("word.new: {e}"))?;
    let out = contained(area, "out", out_rel).map_err(|e| format!("word.new: {e}"))?;
    let text = args["text"].as_str().unwrap_or("");
    if text.len() > MAX_NEW_TEXT_BYTES {
        return Err("word.new: `text` is larger than the service accepts".into());
    }
    let mut doc = Document::from_text(text);
    if let Some(title) = args["title"].as_str() {
        doc.core.title = title.to_string();
    }
    write(&doc, out_rel, &out, area).map_err(|e| format!("word.new: {e}"))?;
    Ok(json!({"out": out_rel, "words": doc.word_count(), "paragraphs": doc.paragraph_count()}))
}

/// The command door: every command admitted by [`door`] before the engine
/// runs any (a refused one refuses the whole call, with nothing written),
/// run in order on one session over the document at `path` or a new empty
/// one, which is then written to `out` under the area's rules. The
/// document stays within the door's ceilings before and after every
/// command ([`RunBudget`]), and a `.pdf` or `.png` out within its page
/// size and count ([`check_out`]).
fn run(args: &Json, area: &Area) -> Result<Json, String> {
    // Admit every command first: one refused id refuses the whole call, with
    // nothing opened and nothing written.
    let admitted = door()?.admit_all(&args["cmds"], area)?;
    let out = match args["out"].as_str().filter(|o| !o.is_empty()) {
        Some(rel) => {
            let path = contained(area, "out", rel).map_err(|e| format!("word.run: {e}"))?;
            area.check(&path, 0).map_err(|e| format!("word.run: {e}"))?;
            Some((rel, path))
        }
        None => None,
    };
    let doc = match args["path"].as_str().filter(|p| !p.is_empty()) {
        Some(_) => open(args, area).map_err(|e| format!("word.run: {e}"))?.0,
        None => Document::new(),
    };
    let mut s = Session::new(doc);
    let mut budget = if admitted.is_empty() { None } else { Some(RunBudget::start(&s)?) };
    let mut results = Vec::with_capacity(admitted.len());
    for (id, params) in admitted {
        if let Some(b) = budget.as_mut() {
            b.before(&mut s, &id, &params)?;
        }
        let r = s.run(&id, &params).map_err(|e| format!("word.run {id}: {e}"))?;
        if let Some(b) = budget.as_mut() {
            b.after(&s, &id)?;
        }
        results.push(json!({"id": id, "result": r}));
    }
    let Some((out_rel, out)) = out else { return Ok(json!({"results": results, "out": Json::Null})) };
    let name = match args["format"].as_str().map(|f| f.trim_start_matches('.')).filter(|f| !f.is_empty()) {
        Some(fmt) => format!("out.{fmt}"),
        None => out_rel.to_string(),
    };
    let format = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    check_out(&s, &format)?;
    let bytes = write(&s.doc, &name, &out, area).map_err(|e| format!("word.run: {e}"))?;
    Ok(json!({"results": results, "out": out_rel, "format": format, "bytes": bytes}))
}

/// What a session holds, as the door's ceilings count it: the characters
/// and paragraphs of every story (and of the Quick Parts a call saves, deep
/// copies the session keeps), and the picture bytes.
#[derive(Clone, Copy, Debug, Default)]
struct Held {
    chars: usize,
    paras: usize,
    media: u64,
}

impl Held {
    fn of(s: &Session) -> Held {
        let mut held = Held { media: s.doc.media.values().map(|m| m.len() as u64).sum(), ..Held::default() };
        for story in stories(&s.doc) {
            held.add(story.iter().map(|b| &**b));
        }
        for part in s.building_blocks.values() {
            held.add(part.blocks.iter());
        }
        held
    }

    /// Count `blocks`, the paragraphs of tables' cells included, however deep.
    fn add<'a>(&mut self, blocks: impl Iterator<Item = &'a Block>) {
        let mut stack: Vec<&Block> = blocks.collect();
        while let Some(block) = stack.pop() {
            match block {
                Block::Para(p) => {
                    self.paras += 1;
                    self.chars += p.text.chars().count();
                }
                Block::Table(t) => stack.extend(t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| c.blocks.iter().map(|b| &**b))),
            }
        }
    }

    /// Why `self` is over the door's ceilings, if it is.
    fn over(&self) -> Option<String> {
        if self.chars > MAX_RUN_CHARS {
            Some(format!("{} characters, more than the {MAX_RUN_CHARS} the door allows a document", self.chars))
        } else if self.paras > MAX_RUN_PARAS {
            Some(format!("{} paragraphs (a table cell holds one), more than the {MAX_RUN_PARAS} the door allows a document", self.paras))
        } else if self.media > MAX_RUN_MEDIA {
            Some(format!("{} bytes of pictures, more than the {MAX_RUN_MEDIA} the door allows a document", self.media))
        } else {
            None
        }
    }
}

/// The stories of `doc`: its body and its parts (headers, footers, notes,
/// comments, text boxes).
fn stories(doc: &Document) -> impl Iterator<Item = &wordcraft_doc::Blocks> {
    std::iter::once(&doc.body).chain(doc.parts.values().map(|p| &p.blocks))
}

/// The top-level blocks of every story, by address. Blocks are shared
/// copy-on-write and the engine snapshots the document before every edit,
/// so a block an edit changed or wrote is a new allocation.
fn block_addresses(doc: &Document) -> HashSet<usize> {
    stories(doc).flat_map(|story| story.iter().map(|b| Arc::as_ptr(b) as usize)).collect()
}

/// What one `run` call has done so far, against the door's ceilings: the
/// document must stay within [`MAX_RUN_CHARS`], [`MAX_RUN_PARAS`] and
/// [`MAX_RUN_MEDIA`] after every command (which stops growth by repetition
/// with no count, such as select all, copy and paste), what the call's
/// edits copy within [`MAX_RUN_COPIED_CHARS`] and [`MAX_RUN_COPIED_PARAS`],
/// and its picture edits within [`MAX_RUN_PICTURE_PIXELS`]. Where a
/// command's growth or work can be known before it runs, it is checked
/// then ([`RunBudget::before`]).
struct RunBudget {
    held: Held,
    rev: u64,
    blocks: HashSet<usize>,
    copied: Held,
    picture_pixels: u64,
}

impl RunBudget {
    /// The budget of a call on `s`'s document as opened: refused if it is
    /// already over a ceiling.
    fn start(s: &Session) -> Result<RunBudget, String> {
        let held = Held::of(s);
        if let Some(why) = held.over() {
            return Err(format!("word.run: the document holds {why}, so the door does not run commands on it"));
        }
        Ok(RunBudget { held, rev: s.rev(), blocks: block_addresses(&s.doc), copied: Held::default(), picture_pixels: 0 })
    }

    /// Before command `id`: what it will add or do, where that can be known.
    fn before(&mut self, s: &mut Session, id: &str, params: &Json) -> Result<(), String> {
        match id {
            "edit.replaceAll" => check_replace_all(s, params, &self.held),
            "edit.paste" | "edit.pasteText" | "edit.pasteMerge" => {
                let mut adds = Held::default();
                match params.get("text").and_then(Json::as_str) {
                    Some(text) => {
                        adds.chars = text.chars().count();
                        adds.paras = text.matches(['\n', '\r']).count() + 1;
                    }
                    None => adds.add(s.clipboard.iter().flat_map(|f| f.blocks.iter())),
                }
                // What is pasted replaces the selection.
                let selected = s.selected_text();
                let after = Held {
                    chars: (self.held.chars + adds.chars).saturating_sub(selected.chars().count()),
                    paras: (self.held.paras + adds.paras).saturating_sub(selected.matches('\n').count()),
                    media: self.held.media,
                };
                match after.over() {
                    Some(why) => Err(format!("word.run: `{id}` would leave the document with {why}")),
                    None => Ok(()),
                }
            }
            "document.layout" => {
                let pages = s.layout().pages.len();
                if pages > MAX_RUN_PAGES {
                    return Err(format!("word.run: `document.layout`: the document lays out to {pages} pages, more than the {MAX_RUN_PAGES} the door lists"));
                }
                Ok(())
            }
            _ if PICTURE_EDITS.contains(&id) => {
                self.picture_pixels += picture_edit_pixels(s, id, params);
                if self.picture_pixels > MAX_RUN_PICTURE_PIXELS {
                    return Err(format!(
                        "word.run: `{id}`: the picture edits of this call would decode {} pixels, more than the {MAX_RUN_PICTURE_PIXELS} the door allows one call",
                        self.picture_pixels
                    ));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// After command `id`: the document within the ceilings, and what the
    /// call's edits have copied within theirs.
    fn after(&mut self, s: &Session, id: &str) -> Result<(), String> {
        self.held = Held::of(s);
        if let Some(why) = self.held.over() {
            return Err(format!("word.run: `{id}` leaves the document with {why}"));
        }
        if s.rev() != self.rev {
            self.rev = s.rev();
            let now = block_addresses(&s.doc);
            for story in stories(&s.doc) {
                self.copied.add(story.iter().filter(|b| !self.blocks.contains(&(Arc::as_ptr(b) as usize))).map(|b| &**b));
            }
            self.blocks = now;
            if self.copied.chars > MAX_RUN_COPIED_CHARS || self.copied.paras > MAX_RUN_COPIED_PARAS {
                return Err(format!(
                    "word.run: `{id}`: the edits of this call have written or changed {} paragraphs and {} characters, more than the {MAX_RUN_COPIED_PARAS} paragraphs and {MAX_RUN_COPIED_CHARS} characters the door allows one call (the engine keeps an undo copy of everything an edit changes)",
                    self.copied.paras, self.copied.chars
                ));
            }
        }
        Ok(())
    }
}

/// Before `edit.replaceAll`: the matches it will replace, found by the
/// engine's own search (run as `edit.find` with the command's options, the
/// selection and find state put back after, so a pattern and the last
/// search's `text` and `with` count as they will), must leave the document
/// within [`MAX_RUN_CHARS`] and cost at most [`MAX_REPLACE_WORK`].
fn check_replace_all(s: &mut Session, params: &Json, held: &Held) -> Result<(), String> {
    let (sel, find, status) = (s.sel.clone(), s.find.clone(), s.status.clone());
    let mut probe: serde_json::Map<String, Json> =
        ["text", "matchCase", "wholeWord", "regex"].iter().filter_map(|k| params.get(*k).map(|v| (k.to_string(), v.clone()))).collect();
    if params.get("text").and_then(Json::as_str).is_none() {
        probe.insert("text".into(), json!(find.query));
    }
    let found = s.run("edit.find", &Json::Object(probe));
    let matches = std::mem::take(&mut s.find.results);
    (s.sel, s.find, s.status) = (sel, find, status);
    if found.is_err() {
        // A bad pattern: the command reports it itself.
        return Ok(());
    }
    let with = params.get("with").and_then(Json::as_str).unwrap_or(s.find.replace.as_str()).chars().count();
    let (mut chars, mut work) = (held.chars, 0u64);
    for (a, b) in &matches {
        let Some(p) = s.doc.para(a.story, &a.path) else { continue };
        chars = (chars + with).saturating_sub(p.text.get(a.off..b.off).map_or(0, |m| m.chars().count()));
        work += p.text.len() as u64;
    }
    let n = matches.len();
    if chars > MAX_RUN_CHARS {
        return Err(format!(
            "word.run: `edit.replaceAll`: {n} replacements would leave the document with {chars} characters, more than the {MAX_RUN_CHARS} the door allows a document"
        ));
    }
    if work > MAX_REPLACE_WORK {
        return Err(format!(
            "word.run: `edit.replaceAll`: {n} replacements in their paragraphs would shift {work} characters, more than the {MAX_REPLACE_WORK} the door allows one command (each replacement shifts the rest of its paragraph: split a long paragraph first)"
        ));
    }
    Ok(())
}

/// The pixels picture edit `id` decodes: those of the picture it acts on
/// (the first one in the selection), from its header, 0 when there is
/// none. A shadow (`picture.style {style: "shadow"}`) blurs with a radius of
/// the picture's longest side ÷ 80, so it costs that much more a pixel
/// (7.5 s for a 4000 × 3000 picture at opt-level 1): it counts 1 + the
/// longest side ÷ 128 times.
fn picture_edit_pixels(s: &Session, id: &str, params: &Json) -> u64 {
    let Some((_, InlineObject::Image { media, .. })) = wordcraft_engine::cmd::objects::selected(s) else { return 0 };
    let Some((w, h)) = s.doc.media.get(&media).and_then(|bytes| wordcraft_engine::render::image_size(bytes)) else { return 0 };
    let pixels = u64::from(w) * u64::from(h);
    if id == "picture.style" && params.get("style").and_then(Json::as_str) == Some("shadow") {
        return pixels * (1 + u64::from(w.max(h)) / 128);
    }
    pixels
}

/// Before a `.pdf` or `.png` out: every page within [`PAGE_SIDES`] (the
/// `.png` is page 1 at 2 px per point), and a `.pdf` within
/// [`MAX_RUN_PAGES`]. A document read from a file carries its own page
/// setup, so this looks at the document, whatever the commands did.
fn check_out(s: &Session, format: &str) -> Result<(), String> {
    if !matches!(format, "pdf" | "png") {
        return Ok(());
    }
    for (_, section) in s.doc.sections() {
        let (w, h) = (section.page_w, section.page_h);
        if !(PAGE_SIDES.contains(&w) && PAGE_SIDES.contains(&h)) {
            return Err(format!(
                "word.run: a page of {w} × {h} points is outside the {}–{} points (1–22 inches) a side the door writes as .{format}",
                PAGE_SIDES.start(),
                PAGE_SIDES.end()
            ));
        }
    }
    if format == "pdf" {
        let pages = s.export_layout().pages.len();
        if pages > MAX_RUN_PAGES {
            return Err(format!("word.run: the document lays out to {pages} pages, more than the {MAX_RUN_PAGES} the door writes as .pdf"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A docx written by the engine itself: the suite needs no fixtures
    /// on disk.
    fn fixture(host: &Path) -> &'static str {
        let made = dispatch(
            "new",
            &json!({"out": "in.docx", "text": "Hello wordcraft\nA second paragraph for the fixture.", "title": "Fixture"}),
            host,
        )
        .unwrap();
        assert_eq!(made["out"], json!("in.docx"), "{made}");
        "in.docx"
    }

    #[test]
    fn new_writes_a_docx_the_engine_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let made = dispatch("new", &json!({"out": "a/fresh.docx", "text": "One\nTwo"}), host).unwrap();
        assert_eq!(made["paragraphs"], json!(2), "{made}");
        assert_eq!(made["words"], json!(2));
        // The file is real and lands inside the area, not beside it.
        assert!(host.join("word/a/fresh.docx").metadata().unwrap().len() > 0);
        assert!(!host.join("a/fresh.docx").exists());
        let back = dispatch("text", &json!({"path": "a/fresh.docx"}), host).unwrap();
        assert_eq!(back["text"].as_str().unwrap().trim(), "One\nTwo");
    }

    #[test]
    fn info_reports_pages_words_and_properties() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);
        let doc = dispatch("info", &json!({"path": input}), host).unwrap();
        assert_eq!(doc["file"], json!(input), "{doc}");
        assert_eq!(doc["paragraphs"], json!(2));
        assert_eq!(doc["words"], json!(8));
        assert!(doc["pages"].as_u64().unwrap() >= 1);
        assert_eq!(doc["properties"]["title"], json!("Fixture"));
    }

    #[test]
    fn text_extracts_the_plain_text() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);
        let got = dispatch("text", &json!({"path": input}), host).unwrap();
        let text = got["text"].as_str().unwrap();
        assert!(text.contains("Hello wordcraft"), "{text}");
        assert!(text.contains("A second paragraph"), "{text}");
        assert_eq!(got["paragraphs"], json!(2));
    }

    #[test]
    fn inspect_lists_the_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);
        let got = dispatch("inspect", &json!({"path": input}), host).unwrap();
        let blocks = got["blocks"].as_array().expect("blocks");
        assert_eq!(blocks.len(), 2, "{got}");
        assert_eq!(blocks[0]["type"], json!("paragraph"));
        assert_eq!(blocks[0]["text"], json!("Hello wordcraft"));
        // Without text, the structure stays and the text goes.
        let bare = dispatch("inspect", &json!({"path": input, "text": false}), host).unwrap();
        assert!(bare["blocks"][0]["text"].is_null(), "{bare}");
    }

    #[test]
    fn convert_round_trips_docx_md_and_txt() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let input = fixture(host);

        let md = dispatch("convert", &json!({"path": input, "out": "out.md"}), host).unwrap();
        assert_eq!(md["format"], json!("md"), "{md}");
        let md_text = std::fs::read_to_string(host.join("word/out.md")).unwrap();
        assert!(md_text.contains("Hello wordcraft"), "{md_text}");

        // `format` overrides the extension: plain text into a .log name.
        let txt = dispatch("convert", &json!({"path": input, "out": "notes.log", "format": "txt"}), host).unwrap();
        assert_eq!(txt["format"], json!("txt"));
        let log = std::fs::read_to_string(host.join("word/notes.log")).unwrap();
        assert!(log.contains("A second paragraph"), "{log}");

        // Markdown reads back in as a document: a real docx comes out.
        let back = dispatch("convert", &json!({"path": "out.md", "out": "back.docx"}), host).unwrap();
        assert!(back["bytes"].as_u64().unwrap() > 0);
        let info = dispatch("info", &json!({"path": "back.docx"}), host).unwrap();
        assert_eq!(info["paragraphs"], json!(2), "{info}");

        // A format the engine cannot save is the engine's error, prefixed.
        let bad = dispatch("convert", &json!({"path": input, "out": "x.xyz"}), host).unwrap_err();
        assert!(bad.starts_with("word.convert: "), "{bad}");
    }

    /// A call as App Hub hands it to the service.
    fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("word.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
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

    /// Without the shell's resolver a call works in `<host dir>/word` and
    /// may replace, as before; a sibling of the area (another service's
    /// file in the shared host dir) is out of reach by name, and a store
    /// app is refused before any folder is made.
    #[test]
    fn without_a_resolver_the_area_is_the_word_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let made = serve(&Slot::new(), &service_call("new", json!({"out": "a.docx", "text": "x"}), host, false)).unwrap();
        assert_eq!(made["out"], json!("a.docx"));
        assert!(host.join("word/a.docx").is_file(), "created on first use");
        serve(&Slot::new(), &service_call("new", json!({"out": "a.docx", "text": "y"}), host, false)).unwrap();
        std::fs::write(host.join("events.json"), b"calendar data").unwrap();
        let miss = dispatch("text", &json!({"path": "events.json"}), host).unwrap_err();
        assert!(miss.starts_with("word.text: "), "{miss}");
        let mut store = service_call("info", json!({"path": "a.docx"}), &host.join("other"), true);
        store.app_id = "org.example.app".into();
        assert!(serve(&Slot::new(), &store).unwrap_err().contains("system apps only"));
        assert!(!host.join("other").exists());
    }

    /// With the shell's resolver every path is relative to the caller's own
    /// folder and stays inside it, through a link too; no private folder is
    /// made.
    #[test]
    fn the_resolver_root_is_used_and_paths_stay_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        let areas = resolver(&root, None);
        let host = dir.path().join(".host");
        serve(&areas, &service_call("new", json!({"out": "notes/a.docx", "text": "One\nTwo"}), &host, false)).unwrap();
        assert!(root.join("notes/a.docx").is_file());
        assert!(!host.exists() && !root.join("word").exists(), "no private folder");
        let info = serve(&areas, &service_call("info", json!({"path": "notes/a.docx"}), &host, false)).unwrap();
        assert_eq!(info["paragraphs"], json!(2), "{info}");
        std::fs::write(dir.path().join("beside.docx"), b"x").unwrap();
        for bad in ["../beside.docx", "/etc/hosts", "notes/../../beside.docx"] {
            assert!(serve(&areas, &service_call("info", json!({"path": bad}), &host, false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("new", json!({"out": bad, "text": "x"}), &host, true)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("out")).unwrap();
            assert!(serve(&areas, &service_call("info", json!({"path": "out/beside.docx"}), &host, false)).is_err());
            assert!(serve(&areas, &service_call("new", json!({"out": "out/made.docx", "text": "x"}), &host, true)).is_err());
            assert!(!dir.path().join("made.docx").exists());
        }
    }

    /// An agent's call (it may not prompt) never replaces a file, before
    /// the engine even runs; an app's own foreground call may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        serve(&areas, &service_call("new", json!({"out": "in.docx", "text": "Hello"}), dir.path(), false)).unwrap();
        std::fs::write(dir.path().join("out.md"), b"keep me").unwrap();
        let agent = serve(&areas, &service_call("convert", json!({"path": "in.docx", "out": "out.md"}), dir.path(), false)).unwrap_err();
        assert!(agent.starts_with("word.convert: ") && agent.contains("`out.md` already exists"), "{agent}");
        assert_eq!(std::fs::read(dir.path().join("out.md")).unwrap(), b"keep me");
        assert!(serve(&areas, &service_call("new", json!({"out": "in.docx", "text": "Other"}), dir.path(), false)).is_err());
        serve(&areas, &service_call("convert", json!({"path": "in.docx", "out": "out.md"}), dir.path(), true)).unwrap();
        assert!(std::fs::read_to_string(dir.path().join("out.md")).unwrap().contains("Hello"));
    }

    /// What a call writes must fit what is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let refused = serve(&resolver(dir.path(), Some(64)), &service_call("new", json!({"out": "big.docx", "text": "x"}), dir.path(), true)).unwrap_err();
        assert!(refused.contains("bytes left"), "{refused}");
        assert!(!dir.path().join("big.docx").exists(), "nothing written");
        serve(&resolver(dir.path(), Some(1 << 20)), &service_call("new", json!({"out": "big.docx", "text": "x"}), dir.path(), true)).unwrap();
    }

    #[test]
    fn paths_stay_inside_the_area() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        std::fs::write(host.join("up.docx"), b"outside").unwrap();
        for bad in ["../up.docx", "/etc/x.docx", "a/../../up.docx", ""] {
            assert!(dispatch("info", &json!({"path": bad}), host).is_err(), "{bad}");
            assert!(dispatch("new", &json!({"out": bad, "text": "x"}), host).is_err(), "{bad}");
            assert!(dispatch("convert", &json!({"path": "in.docx", "out": bad}), host).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_system_apps_may_call() {
        assert!(may_call("os.notes"));
        assert!(!may_call("org.example.anything"));
        assert!(!may_call(""));
    }

    /// The door runs an allowlisted command in a temporary area and writes
    /// a new document: `safe` commands build it, and nothing outside the
    /// area is touched.
    #[test]
    fn the_door_runs_allowlisted_commands_in_its_area() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let made = serve(
            &areas,
            &service_call(
                "run",
                json!({"cmds": [
                    {"id": "text.insert", "params": {"text": "Quarterly report"}},
                    {"id": "para.style", "params": {"style": "Heading 1"}},
                    {"id": "text.newParagraph"},
                    {"id": "text.insert", "params": {"text": "Revenue grew."}},
                    {"id": "document.text"}
                ], "out": "report.docx"}),
                dir.path(),
                false,
            ),
        )
        .unwrap();
        assert_eq!(made["out"], json!("report.docx"), "{made}");
        assert_eq!(made["format"], json!("docx"));
        assert_eq!(made["results"].as_array().unwrap().len(), 5);
        let back = serve(&areas, &service_call("text", json!({"path": "report.docx"}), dir.path(), false)).unwrap();
        assert!(back["text"].as_str().unwrap().contains("Quarterly report\nRevenue grew."), "{back}");
        // An existing document, edited and written beside itself; a query
        // without `out` writes nothing.
        let edited = serve(
            &areas,
            &service_call("run", json!({"path": "report.docx", "cmds": [{"id": "caret.docEnd"}, {"id": "text.insert", "params": {"text": " Costs fell."}}], "out": "report-2.md"}), dir.path(), false),
        )
        .unwrap();
        assert_eq!(edited["format"], json!("md"), "{edited}");
        assert!(std::fs::read_to_string(dir.path().join("report-2.md")).unwrap().contains("Costs fell."));
        let query = serve(&areas, &service_call("run", json!({"path": "report.docx", "cmds": [{"id": "document.inspect", "params": {"text": false}}]}), dir.path(), false)).unwrap();
        assert!(query["out"].is_null() && query["results"][0]["result"]["blocks"].is_array(), "{query}");
    }

    /// Every class but `safe` (and the reviewed reads) is refused, and so is
    /// an id the classification does not know, before any command runs: a
    /// refused id anywhere in the list writes nothing.
    #[test]
    fn the_door_refuses_every_other_class_and_unknown_ids() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        for (id, class) in [
            ("tools.macros", "code"),
            ("tools.recordMacro", "code"),
            ("edit.repeat", "code"),
            ("references.researcher", "network"),
            ("review.readAloud", "device"),
            ("file.print", "host"),
            ("view.zoom", "host"),
        ] {
            let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "text.insert", "params": {"text": "x"}}, {"id": id}], "out": "x.docx"}), dir.path(), false)).unwrap_err();
            assert!(e.contains(&format!("`{id}` is classed {class}")), "{id}: {e}");
        }
        let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "file.saveAs", "params": {"path": "elsewhere.docx"}}]}), dir.path(), false)).unwrap_err();
        assert!(e.contains("not reviewed to run through it"), "{e}");
        let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "word.secret"}]}), dir.path(), false)).unwrap_err();
        assert!(e.contains("not a reviewed word command"), "{e}");
        // A macro that wraps an allowed command is still the macro: refused.
        let e = serve(
            &areas,
            &service_call("run", json!({"cmds": [{"id": "tools.macros", "params": {"define": {"name": "m", "steps": [{"command": "text.insert", "params": {"text": "x"}}]}}}]}), dir.path(), false),
        )
        .unwrap_err();
        assert!(e.contains("classed code"), "{e}");
        assert!(!dir.path().join("x.docx").exists(), "nothing written");
        let too_many: Vec<Json> = (0..65).map(|_| json!({"id": "text.newParagraph"})).collect();
        assert!(serve(&areas, &service_call("run", json!({"cmds": too_many}), dir.path(), false)).unwrap_err().contains("at most 64"));
    }

    /// The reviewed picture reads take a file inside the area only; the door
    /// never writes over an existing `out`, and keeps to the quota.
    #[test]
    fn the_doors_file_reads_and_writes_keep_the_areas_rules() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let png = png();
        std::fs::create_dir(dir.path().join("pics")).unwrap();
        std::fs::write(dir.path().join("pics/dot.png"), &png).unwrap();
        let made = serve(&areas, &service_call("run", json!({"cmds": [{"id": "insert.picture", "params": {"path": "pics/dot.png"}}], "out": "with-picture.docx"}), dir.path(), false)).unwrap();
        assert_eq!(made["out"], json!("with-picture.docx"), "{made}");
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.png"), &png).unwrap();
        let secret = outside.path().join("secret.png").to_string_lossy().into_owned();
        for bad in [secret.as_str(), "../secret.png", "pics/../../secret.png"] {
            let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "insert.picture", "params": {"path": bad}}]}), dir.path(), false)).unwrap_err();
            assert!(e.starts_with("word.run: `insert.picture`: `path`: "), "{bad}: {e}");
        }
        std::fs::write(dir.path().join("taken.docx"), b"keep").unwrap();
        let e = serve(&areas, &service_call("run", json!({"cmds": [{"id": "text.insert", "params": {"text": "x"}}], "out": "taken.docx"}), dir.path(), false)).unwrap_err();
        assert!(e.contains("`taken.docx` already exists"), "{e}");
        assert_eq!(std::fs::read(dir.path().join("taken.docx")).unwrap(), b"keep");
        let e = serve(&resolver(dir.path(), Some(16)), &service_call("run", json!({"cmds": [{"id": "text.insert", "params": {"text": "x"}}], "out": "big.docx"}), dir.path(), false)).unwrap_err();
        assert!(e.contains("bytes left"), "{e}");
        for bad in ["../up.docx", "/etc/x.docx"] {
            assert!(serve(&areas, &service_call("run", json!({"cmds": [], "out": bad}), dir.path(), false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("run", json!({"cmds": [], "path": bad}), dir.path(), false)).is_err(), "{bad}");
        }
    }

    /// A 12x8 RGB PNG (two colour bands), as the photo service's tests use.
    fn png() -> Vec<u8> {
        const PNG: &str = "89504e470d0a1a0a0000000d494844520000000c000000080802000000428689a60000001d49444154789c6378616383866c725ea021063a2bb279d14310d15911005b9497817c6155610000000049454e44ae426082";
        (0..PNG.len()).step_by(2).map(|i| u8::from_str_radix(&PNG[i..i + 2], 16).unwrap()).collect()
    }

    /// Every `word.run` call the skill's examples show runs, in order, in
    /// one area as the system agent's (with the picture it names placed
    /// there), and writes its `out`: the skill teaches commands that work.
    #[test]
    fn the_skill_examples_run() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        std::fs::write(dir.path().join("chart.png"), png()).unwrap();
        let body = include_str!("../skill/SKILL.md");
        let examples = body.split("\n## Examples").nth(1).and_then(|rest| rest.split("\n## ").next()).unwrap();
        let mut ran = 0;
        for span in examples.split('`').skip(1).step_by(2) {
            let Some(args) = span.strip_prefix("word.run ") else { continue };
            let args: Json = serde_json::from_str(args).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            let got = serve(&areas, &service_call("run", args.clone(), dir.path(), false)).unwrap_or_else(|e| panic!("`{span}`: {e}"));
            assert_eq!(got["results"].as_array().map(Vec::len), args["cmds"].as_array().map(Vec::len), "{got}");
            if let Some(out) = args["out"].as_str() {
                assert!(dir.path().join(out).is_file(), "`{span}` wrote no {out}");
            }
            ran += 1;
        }
        assert!(ran >= 5, "{ran} examples");
        let memo = serve(&areas, &service_call("inspect", json!({"path": "memo.docx"}), dir.path(), false)).unwrap();
        assert_eq!((memo["blocks"][0]["style"].as_str(), memo["blocks"][1]["style"].as_str()), (Some("Heading1"), Some("Normal")), "{memo}");
        assert!(std::fs::read_to_string(dir.path().join("memo-v2.md")).unwrap().contains("Turnover grew"));
    }

    /// The door's gate is built from the generated classification and the
    /// reviewed reads, which must be `file` commands of the catalog.
    #[test]
    fn the_door_is_built_from_the_reviewed_classification() {
        let door = door().unwrap();
        assert!(door.runs("text.insert") && door.runs("insert.picture") && door.runs("picture.change"));
        assert!(!door.runs("file.save") && !door.runs("tools.macros") && !door.runs("review.readAloud"));
        assert!(door.runnable().len() > 300, "{}", door.runnable().len());
    }

    /// The agent tools (`tools.json`) pass App Hub's own loader, as the shell
    /// reads them, keep the object schemas octos takes both ways, and name
    /// only methods this service dispatches.
    #[test]
    fn the_agent_tools_pass_app_hubs_loader_and_name_real_methods() {
        use octosense_app_policy::{ImplementedBy, ToolHost, ToolManifest};
        let (manifest, _) = ToolManifest::load(TOOLS_JSON, "word", ToolHost::Contained, false).unwrap();
        assert!(!manifest.tools.is_empty());
        let dir = tempfile::tempdir().unwrap();
        for tool in &manifest.tools {
            assert_eq!(tool.implemented_by, ImplementedBy::HostService, "{}", tool.name);
            assert!(tool.host_method.is_none(), "{}: the shell routes each tool to the method of its own name", tool.name);
            for schema in [&tool.input_schema, &tool.output_schema] {
                assert_eq!(schema["type"], json!("object"), "{}: octos takes object schemas only", tool.name);
            }
            assert!(tool.description.is_ascii(), "{}: descriptions stay within octos's byte limit", tool.name);
            let method = tool.name.strip_prefix("word.").unwrap();
            if let Err(e) = dispatch(method, &json!({}), dir.path()) {
                assert!(!e.contains("is not a method"), "{}: {e}", tool.name);
            }
        }
    }

    /// One `word.run` call working in `dir`, as the system agent makes it.
    fn run_in(dir: &Path, args: Json) -> Result<Json, String> {
        serve(&resolver(dir, None), &service_call("run", args, dir, false))
    }

    /// Every limit of the door: a call at the cap passes the gate (and
    /// runs), one over is refused, naming the parameter and the cap.
    #[test]
    fn the_doors_limits_pass_at_their_cap_and_refuse_one_over() {
        let dir = tempfile::tempdir().unwrap();
        let csv = |rows: usize, cols: usize| vec![vec!["x"; cols].join(","); rows].join("\n");
        let replace = |n: usize| json!([{"id": "text.insert", "params": {"text": "a b"}}, {"id": "edit.replaceAll", "params": {"text": "a", "with": "a".repeat(n)}}]);
        let split = |n: usize| json!([{"id": "insert.table", "params": {"rows": 1, "cols": 1}}, {"id": "table.split", "params": {"columns": n}}]);
        let page = |w: u32| json!([{"id": "layout.pageSetup", "params": {"section": {"pageW": w, "pageH": 1584}}}]);
        let cases = [
            (
                json!([{"id": "insert.table", "params": {"rows": 1000, "cols": 10}}]),
                json!([{"id": "insert.table", "params": {"rows": 1001, "cols": 10}}]),
                "`insert.table`: `rows` × `cols` is 10010, more than the 10000 cells the door allows in one command",
            ),
            (
                json!([{"id": "insert.spreadsheet", "params": {"csv": csv(1000, 10)}}]),
                json!([{"id": "insert.spreadsheet", "params": {"csv": csv(1001, 10)}}]),
                "`insert.spreadsheet` asks for 10010 cells, more than the 10000 the door allows in one command",
            ),
            (split(63), split(64), "`table.split`: `columns` is 64, more than the 63 columns the door allows in one command"),
            (page(1584), page(1585), "`layout.pageSetup` asks for 1585 points on a page side, more than the 1584 the door allows in one command"),
            (replace(1000), replace(1001), "`edit.replaceAll` asks for 1001 times the text it replaces, more than the 1000 the door allows in one command"),
        ];
        for (at, over, refusal) in cases {
            run_in(dir.path(), json!({"cmds": at})).unwrap_or_else(|e| panic!("{at}: {e}"));
            let e = run_in(dir.path(), json!({"cmds": over})).unwrap_err();
            assert!(e.contains(refusal), "{e}");
        }
        // A page side under an inch is refused too, whatever the other side.
        let e = run_in(dir.path(), json!({"cmds": [{"id": "layout.pageSetup", "params": {"section": {"pageW": 71, "pageH": 792}}}]})).unwrap_err();
        assert!(e.contains("`layout.pageSetup`: a page side is at least 72 points"), "{e}");
        // The ceilings themselves hold at their caps: a document of exactly
        // 500,000 characters runs, one more character is refused.
        let text = vec!["x".repeat(99); 5000].join("\n");
        run_in(dir.path(), json!({"cmds": [{"id": "document.setText", "params": {"text": text}}, {"id": "caret.docEnd"}, {"id": "text.insert", "params": {"text": "y".repeat(5000)}}]})).unwrap();
        let e = run_in(dir.path(), json!({"cmds": [{"id": "document.setText", "params": {"text": text}}, {"id": "caret.docEnd"}, {"id": "text.insert", "params": {"text": "y".repeat(5001)}}]}))
            .unwrap_err();
        assert!(e.contains("`text.insert` leaves the document with 500001 characters, more than the 500000 the door allows a document"), "{e}");
    }

    /// A 1,000,000 × 1,000,000 table is refused before any command runs,
    /// and nothing is written.
    #[test]
    fn a_huge_table_is_refused_before_anything_runs() {
        let dir = tempfile::tempdir().unwrap();
        let cmds = json!([{"id": "text.insert", "params": {"text": "x"}}, {"id": "insert.table", "params": {"rows": 1e6, "cols": 1e6}}]);
        let e = run_in(dir.path(), json!({"cmds": cmds, "out": "t.docx"})).unwrap_err();
        assert!(e.contains("`insert.table`: `rows` × `cols` is 1000000000000, more than the 10000 cells"), "{e}");
        assert!(!dir.path().join("t.docx").exists());
    }

    /// `edit.replaceAll`s that each double the text (`a` → `aa`) multiply
    /// together: a chain of 14 is refused by the copy budget before any
    /// command runs, one of 13 runs.
    #[test]
    fn a_replace_all_doubling_chain_is_refused_by_the_copy_budget() {
        let dir = tempfile::tempdir().unwrap();
        let chain = |n: usize| {
            let mut cmds = vec![json!({"id": "text.insert", "params": {"text": "a"}})];
            cmds.extend((0..n).map(|_| json!({"id": "edit.replaceAll", "params": {"text": "a", "with": "aa"}})));
            json!({"cmds": cmds, "out": format!("chain-{n}.txt")})
        };
        let e = run_in(dir.path(), chain(14)).unwrap_err();
        assert!(e.contains("the copies this call makes multiply to 16384, more than the 10000 the door allows in one call"), "{e}");
        assert!(!dir.path().join("chain-14.txt").exists(), "nothing written");
        run_in(dir.path(), chain(13)).unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("chain-13.txt")).unwrap().trim(), "a".repeat(8192));
    }

    /// Select all, copy and paste doubles the document each round, with no
    /// count for the gate to see: the ceiling refuses the paste that would
    /// cross it, before it runs, and nothing is written.
    #[test]
    fn a_select_all_copy_paste_loop_is_stopped_by_the_size_ceiling() {
        let dir = tempfile::tempdir().unwrap();
        let mut cmds = vec![json!({"id": "document.setText", "params": {"text": vec!["x".repeat(99); 1000].join("\n")}})];
        for _ in 0..4 {
            cmds.extend([json!({"id": "select.all"}), json!({"id": "edit.copy"}), json!({"id": "caret.docEnd"}), json!({"id": "edit.paste"})]);
        }
        let e = run_in(dir.path(), json!({"cmds": cmds, "out": "loop.docx"})).unwrap_err();
        assert!(e.contains("`edit.paste` would leave the document with 792000 characters, more than the 500000 the door allows a document"), "{e}");
        assert!(!dir.path().join("loop.docx").exists());
        // Pasting over a selection replaces it: select all and paste the
        // same document again stays within the ceiling.
        let swap = json!([{"id": "document.setText", "params": {"text": vec!["x".repeat(99); 3000].join("\n")}}, {"id": "select.all"}, {"id": "edit.copy"}, {"id": "edit.paste"}]);
        run_in(dir.path(), json!({"cmds": swap})).unwrap();
    }

    /// A page over 22 inches read from a file (the gate refuses one through
    /// `layout.pageSetup`) is refused before the `.pdf` or `.png` out is
    /// made; another format still writes.
    #[test]
    fn a_huge_page_is_refused_before_the_pdf_or_png_out() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Session::new(Document::from_text("A poster"));
        s.run("layout.pageSetup", &json!({"section": {"pageW": 5000, "pageH": 5000}})).unwrap();
        std::fs::write(dir.path().join("poster.docx"), wordcraft_engine::io::save_bytes("poster.docx", &s.doc).unwrap()).unwrap();
        for out in ["poster.pdf", "poster.png"] {
            let e = run_in(dir.path(), json!({"path": "poster.docx", "cmds": [], "out": out})).unwrap_err();
            assert!(e.contains("a page of 5000 × 5000 points is outside the 72–1584 points (1–22 inches) a side"), "{out}: {e}");
            assert!(!dir.path().join(out).exists());
        }
        run_in(dir.path(), json!({"path": "poster.docx", "cmds": [], "out": "poster.md"})).unwrap();
        let e = run_in(dir.path(), json!({"cmds": [{"id": "layout.pageSetup", "params": {"section": {"pageW": 5000, "pageH": 5000}}}], "out": "x.docx"})).unwrap_err();
        assert!(e.contains("asks for 5000 points on a page side, more than the 1584"), "{e}");
    }

    /// A huge font puts every character on a page of its own: a `.pdf` out
    /// or `document.layout` of more pages than the door lays out is
    /// refused before it is made.
    #[test]
    fn too_many_pages_are_refused_before_the_pdf_out_and_the_layout_list() {
        let dir = tempfile::tempdir().unwrap();
        let big = |then: Json| {
            json!({"cmds": [{"id": "document.setText", "params": {"text": "x".repeat(15_000)}}, {"id": "select.all"}, {"id": "format.size", "params": {"size": 1638}}, then], "out": "big.pdf"})
        };
        let e = run_in(dir.path(), big(json!({"id": "document.text"}))).unwrap_err();
        assert!(e.contains("pages, more than the 10000 the door writes as .pdf"), "{e}");
        let e = run_in(dir.path(), big(json!({"id": "document.layout"}))).unwrap_err();
        assert!(e.contains("`document.layout`: the document lays out to") && e.contains("more than the 10000 the door lists"), "{e}");
        assert!(!dir.path().join("big.pdf").exists());
    }

    /// `edit.replaceAll` in one long paragraph shifts the rest of the
    /// paragraph for every match: refused by its work estimate before it
    /// runs, while the same matches over short paragraphs run.
    #[test]
    fn a_quadratic_replace_all_is_refused_by_its_work_estimate() {
        let dir = tempfile::tempdir().unwrap();
        let replace = |text: String| json!({"cmds": [{"id": "document.setText", "params": {"text": text}}, {"id": "edit.replaceAll", "params": {"text": "a", "with": "c"}}]});
        let e = run_in(dir.path(), replace("ab".repeat(100_000))).unwrap_err();
        assert!(e.contains("100000 replacements in their paragraphs would shift 20000000000 characters, more than the 500000000"), "{e}");
        let ran = run_in(dir.path(), replace(vec!["ab".repeat(50); 2000].join("\n"))).unwrap();
        assert_eq!(ran["results"][1]["result"]["replaced"], json!(100_000), "{ran}");
        // A pattern counts its real matches, and its replacement's growth.
        let e = run_in(
            dir.path(),
            json!({"cmds": [{"id": "document.setText", "params": {"text": vec!["ab".repeat(50); 2000].join("\n")}}, {"id": "edit.replaceAll", "params": {"text": "[ab]", "regex": true, "with": "vwxyz"}}]}),
        )
        .unwrap_err();
        assert!(e.contains("100000 replacements would leave the document with 600000 characters, more than the 500000"), "{e}");
    }

    /// Every edit keeps an undo copy of what it changed: whole-document
    /// edits of a large document are bounded by the call's copy budget,
    /// small edits are not.
    #[test]
    fn whole_document_edits_are_bounded_by_the_undo_copy_budget() {
        let dir = tempfile::tempdir().unwrap();
        let text = vec!["word word "; 40_000].join("\n");
        let mut cmds = vec![json!({"id": "document.setText", "params": {"text": text}}), json!({"id": "select.all"})];
        cmds.extend((0..6).map(|_| json!({"id": "format.bold"})));
        let e = run_in(dir.path(), json!({"cmds": cmds})).unwrap_err();
        assert!(e.contains("`format.bold`: the edits of this call have written or changed 240000 paragraphs"), "{e}");
        let mut cmds = vec![json!({"id": "document.setText", "params": {"text": text}})];
        cmds.extend((0..31).flat_map(|_| [json!({"id": "caret.docEnd"}), json!({"id": "text.insert", "params": {"text": "more"}})]));
        run_in(dir.path(), json!({"cmds": cmds})).unwrap();
    }

    /// Every picture edit decodes the whole picture and encodes it again:
    /// one call's edits share a pixel budget, read from the picture's
    /// header before the engine decodes it.
    #[test]
    fn picture_edits_share_a_pixel_budget() {
        let dir = tempfile::tempdir().unwrap();
        // The engine's own render of a 22-inch page: 3168 × 3168 pixels.
        let mut page = Session::new(Document::from_text("A picture"));
        page.run("layout.size", &json!({"width": 1584, "height": 1584})).unwrap();
        std::fs::write(dir.path().join("page.png"), wordcraft_engine::io::save_bytes("page.png", &page.doc).unwrap()).unwrap();
        let edits = |n: usize| {
            let mut cmds = vec![json!({"id": "insert.picture", "params": {"path": "page.png"}})];
            cmds.extend((0..n).map(|_| json!({"id": "picture.compress", "params": {"maxPixels": 20000}})));
            json!({"cmds": cmds})
        };
        run_in(dir.path(), edits(3)).unwrap();
        let e = run_in(dir.path(), edits(4)).unwrap_err();
        assert!(e.contains("`picture.compress`: the picture edits of this call would decode 40144896 pixels, more than the 40000000"), "{e}");
        // A shadow blurs with a radius of the picture's longest side ÷ 80:
        // one on this picture counts 25 times its pixels.
        let shadow = json!({"cmds": [{"id": "insert.picture", "params": {"path": "page.png"}}, {"id": "picture.style", "params": {"style": "shadow"}}]});
        let e = run_in(dir.path(), shadow).unwrap_err();
        assert!(e.contains("`picture.style`: the picture edits of this call would decode 250905600 pixels"), "{e}");
        // Small pictures edit freely.
        std::fs::write(dir.path().join("dot.png"), png()).unwrap();
        let mut cmds = vec![json!({"id": "insert.picture", "params": {"path": "dot.png"}})];
        cmds.extend((0..60).map(|_| json!({"id": "picture.corrections", "params": {"brightness": 5}})));
        run_in(dir.path(), json!({"cmds": cmds})).unwrap();
    }

    /// A document read from a file that is already over a ceiling is
    /// refused before any command runs; with no commands it still converts.
    #[test]
    fn a_document_over_the_ceiling_is_refused_before_commands_run() {
        let dir = tempfile::tempdir().unwrap();
        dispatch("new", &json!({"out": "big.txt", "text": vec!["x".repeat(99); 6000].join("\n")}), dir.path()).unwrap();
        std::fs::rename(dir.path().join("word/big.txt"), dir.path().join("big.txt")).unwrap();
        let e = run_in(dir.path(), json!({"path": "big.txt", "cmds": [{"id": "document.text"}]})).unwrap_err();
        assert!(e.contains("the document holds 594000 characters, more than the 500000 the door allows a document, so the door does not run commands on it"), "{e}");
        run_in(dir.path(), json!({"path": "big.txt", "cmds": [], "out": "big.docx"})).unwrap();
    }
}
