//! `octosense-sheets-service` — the `sheet` host service (ADR 0013).
//!
//! gridcraft's spreadsheet engine behind typed `sheet.*` methods. A
//! workbook is a host-owned session: created empty or read from an xlsx in
//! the call's area, edited as values and formulas, recalculated on demand,
//! and written back as xlsx into the same area. Everything is JSON at the
//! boundary; the engine's types never cross it.
//!
//! Methods (all under the `sheet` family):
//! - `new {}` → `{book, sheets}`
//! - `open {path}` → `{book, sheets}` — `path` relative to the service area
//! - `set {book, sheet?, cells: [{at, value? | formula?}]}` → `{set}`
//! - `get {book, sheet?, range}` → `{values}` (row-major; `A1:C3` or `A1`)
//! - `eval {book?, sheet?, at?, formula}` → `{value}` — ad hoc, not stored;
//!   without `book`, against an empty transient workbook
//! - `recalc {book}` → `{}` — full recalculation, formulas cache results
//! - `export {book, path}` → `{path}` — xlsx under the service area
//! - `close {book}` → `{}`
//!
//! **Where a call works** (ADR 0013, 2026-10-08): in its caller's own
//! folder, the [`Area`] the shell's resolver gives it ([`set_area_resolver`]),
//! or without one the legacy private folder `<host dir>/sheet`. Paths never
//! leave the area: `..`, absolute paths and symlink escapes are refused, the
//! same stance the files host tools take. A write that may not replace (an
//! agent's) only creates new files, within the area's quota
//! ([`Area::write`]). A workbook belongs to the area it was created or
//! opened in: a call from another area cannot see, change, export or close
//! it, whatever its handle. gridcraft does no I/O of its own: a link to
//! another workbook is refused and resolves to `#REF!`. The service serves
//! system apps, and the native Sheets app, whose agent's `sheets.*` tools
//! the shell routes here (`crates/shell/src/host_tools/engines.rs`).

/// The system agent's skill for this engine (ADR 0013).
pub mod skill;

use std::collections::HashMap;

mod fill;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use gridcraft_calc::Calc;
use gridcraft_core::addr::parse_a1_prefix;
use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_model::{Cell, Formula, Sheet, Workbook};
use octosense_appstore::services::{register_host_service, HostService, Replier, ServiceCall, ServiceHost};
use octosense_engine_area::{Area, Slot};
use serde_json::{json, Value as Json};

/// One open workbook session.
struct Book {
    wb: Workbook,
    calc: Calc,
    /// `sheet.fill`'s compiled kernels, by canonical formula text.
    kernels: HashMap<String, std::sync::Arc<makepad_script_compute::kernel::Kernel>>,
    /// The area it belongs to ([`owner`]): only calls in that area reach it.
    owner: PathBuf,
}

#[derive(Default)]
struct Books {
    next: u64,
    open: HashMap<u64, Book>,
}

static BOOKS: OnceLock<Mutex<Books>> = OnceLock::new();

fn books() -> &'static Mutex<Books> {
    BOOKS.get_or_init(|| Mutex::new(Books::default()))
}

/// Open workbooks of one area: a runaway caller is bounded, and cannot
/// crowd out another area's workbooks.
const MAX_OPEN_BOOKS: usize = 16;
/// Open workbooks across every area.
const MAX_OPEN_BOOKS_ALL: usize = 64;
/// The largest xlsx the service reads or writes (bytes).
const MAX_XLSX_BYTES: u64 = 64 << 20;

/// Which apps may call the service: system apps, as News, and the native
/// Sheets app (the shell routes its agent's `sheets.*` tools here as
/// `"sheets"`).
fn may_call(app_id: &str) -> bool {
    app_id == "sheets" || app_id.starts_with("os.")
}

pub struct SheetsService;

/// Register the `sheet` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(SheetsService));
}

/// The shell's resolver: where each call works (`None` removes it, and
/// calls work in the legacy `<host dir>/sheet` again).
static AREAS: Slot = Slot::new();

pub fn set_area_resolver(resolver: Option<octosense_engine_area::Resolver>) {
    AREAS.set(resolver);
}

impl HostService for SheetsService {
    fn family(&self) -> &'static str {
        "sheet"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        reply.send(serve(&AREAS, &call));
    }
}

/// One call, in the area `areas` gives it. A plain calculation (`eval`
/// without a workbook) reads and keeps nothing, so it needs no area.
fn serve(areas: &Slot, call: &ServiceCall) -> Result<Json, String> {
    if !may_call(&call.app_id) {
        return Err("The sheet service serves system apps only.".into());
    }
    if call.method() == "eval" && call.args["book"].is_null() {
        return eval_adhoc(&call.args, None);
    }
    let area = areas.area(call, "sheet").map_err(|e| format!("sheet: {e}"))?;
    dispatch(call.method(), &call.args, &area)
}

fn dispatch(method: &str, args: &Json, area: &Area) -> Result<Json, String> {
    match method {
        "new" => book_new(area),
        "open" => book_open(args, area),
        "set" => cells_set(args, area),
        "get" => cells_get(args, area),
        "eval" => eval_adhoc(args, Some(area)),
        "recalc" => recalc(args, area),
        "fill" => fill_column(args, area),
        "export" => export(args, area),
        "close" => close(args, area),
        other => Err(format!("sheet.{other} is not a method of the sheet service")),
    }
}

/// Whose workbooks a call reaches: its area's root, as the file system
/// spells it (two spellings of one folder are one owner).
fn owner(area: &Area) -> PathBuf {
    area.root.canonicalize().unwrap_or_else(|_| area.root.clone())
}

fn insert(wb: Workbook, area: &Area) -> Result<Json, String> {
    let names: Vec<String> = wb.sheets.iter().map(|s| s.name.clone()).collect();
    let owner = owner(area);
    let mut books = books().lock().map_err(|_| "sheet: poisoned")?;
    if books.open.values().filter(|b| b.owner == owner).count() >= MAX_OPEN_BOOKS {
        return Err(format!("sheet: {MAX_OPEN_BOOKS} workbooks are already open; close one first"));
    }
    if books.open.len() >= MAX_OPEN_BOOKS_ALL {
        return Err("sheet: the engine holds as many workbooks as it can; close one first".into());
    }
    books.next += 1;
    let id = books.next;
    books.open.insert(id, Book { wb, calc: Calc::new(), kernels: HashMap::new(), owner });
    Ok(json!({"book": id, "sheets": names}))
}

fn book_new(area: &Area) -> Result<Json, String> {
    insert(Workbook::new(), area)
}

fn book_open(args: &Json, area: &Area) -> Result<Json, String> {
    let path = contained_path(area, args["path"].as_str().unwrap_or(""))?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("sheet.open: {e}"))?;
    if meta.len() > MAX_XLSX_BYTES {
        return Err("sheet.open: the file is larger than the service reads".into());
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("sheet.open: {e}"))?;
    let (wb, _report) = gridcraft_xlsx::read_xlsx(&bytes).map_err(|e| format!("sheet.open: {e:?}"))?;
    insert(wb, area)
}

/// The workbook `args.book` names, when it belongs to `area`: another
/// area's handle is answered exactly as one that does not exist.
fn with_book<T>(args: &Json, area: &Area, f: impl FnOnce(&mut Book) -> Result<T, String>) -> Result<T, String> {
    let id = args["book"].as_u64().ok_or("sheet: `book` is required")?;
    let owner = owner(area);
    let mut books = books().lock().map_err(|_| "sheet: poisoned")?;
    let book = books.open.get_mut(&id).filter(|b| b.owner == owner).ok_or("sheet: no such workbook (opened and not closed?)")?;
    f(book)
}

fn sheet_index(wb: &Workbook, args: &Json) -> Result<usize, String> {
    match &args["sheet"] {
        Json::Null => Ok(0),
        Json::Number(n) => {
            let i = n.as_u64().unwrap_or(u64::MAX) as usize;
            if i < wb.sheets.len() { Ok(i) } else { Err(format!("sheet: no sheet {i}")) }
        }
        Json::String(name) => wb
            .sheets
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("sheet: no sheet named {name}")),
        other => Err(format!("sheet: `sheet` is an index or a name, not {other}")),
    }
}

/// `A1` (absolute anchors allowed, sheet names not) to a cell.
fn parse_a1(s: &str) -> Result<CellRef, String> {
    match parse_a1_prefix(s.trim()) {
        Some((col, row, _, _, rest)) if rest.is_empty() => Ok(CellRef::new(row, col)),
        _ => Err(format!("sheet: `{s}` is not a cell like A1")),
    }
}

/// `A1:C9` or a single `A1`.
fn parse_range(s: &str) -> Result<RangeRef, String> {
    let (a, b) = match s.split_once(':') {
        Some((a, b)) => (parse_a1(a)?, parse_a1(b)?),
        None => {
            let c = parse_a1(s)?;
            (c, c)
        }
    };
    Ok(RangeRef::new(a, b))
}

fn json_to_value(v: &Json) -> Result<Value, String> {
    Ok(match v {
        Json::Null => Value::Empty,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => Value::Number(n.as_f64().ok_or("sheet: a finite number")?),
        Json::String(s) => Value::Text(Arc::from(s.as_str())),
        other => return Err(format!("sheet: a cell takes null, bool, number or text, not {other}")),
    })
}

fn value_to_json(v: &Value) -> Json {
    match v {
        Value::Empty => Json::Null,
        Value::Number(n) => json!(n),
        Value::Text(t) => json!(t.as_ref()),
        Value::Bool(b) => json!(b),
        Value::Error(e) => json!({"error": format!("{e:?}")}),
        Value::Array(a) => json!({"rows": a.rows, "cols": a.cols}),
    }
}

fn cells_set(args: &Json, area: &Area) -> Result<Json, String> {
    with_book(args, area, |book| {
        let si = sheet_index(&book.wb, args)?;
        let cells = args["cells"].as_array().ok_or("sheet.set: `cells` is a list")?;
        let sheet = Arc::make_mut(&mut book.wb.sheets[si]);
        let mut set = 0usize;
        for item in cells {
            let at = parse_a1(item["at"].as_str().unwrap_or(""))?;
            if let Some(f) = item["formula"].as_str() {
                sheet.cells.set(
                    at,
                    Cell { value: Value::Empty, formula: Some(Arc::new(Formula::new(f))), style: Default::default() },
                );
            } else {
                sheet.set_value(at, json_to_value(&item["value"])?);
            }
            set += 1;
        }
        Ok(json!({"set": set}))
    })
}

fn cells_get(args: &Json, area: &Area) -> Result<Json, String> {
    with_book(args, area, |book| {
        let si = sheet_index(&book.wb, args)?;
        let range = parse_range(args["range"].as_str().unwrap_or(""))?;
        let sheet: &Sheet = &book.wb.sheets[si];
        let mut rows = Vec::new();
        for r in range.start.row..=range.end.row {
            let mut row = Vec::new();
            for c in range.start.col..=range.end.col {
                let v = sheet.cell(CellRef::new(r, c)).map(|cell| cell.value.clone()).unwrap_or(Value::Empty);
                row.push(value_to_json(&v));
            }
            rows.push(Json::Array(row));
        }
        Ok(json!({"values": rows}))
    })
}

fn eval_adhoc(args: &Json, area: Option<&Area>) -> Result<Json, String> {
    let eval = |wb: &Workbook, si: usize| -> Result<Json, String> {
        let at = match args["at"].as_str() {
            Some(s) => parse_a1(s)?,
            None => CellRef::new(0, 0),
        };
        let formula = args["formula"].as_str().ok_or("sheet.eval: `formula` is required")?;
        let v = gridcraft_calc::evaluate(wb, si, at, formula);
        Ok(json!({"value": value_to_json(&v)}))
    };
    let Some(area) = area.filter(|_| !args["book"].is_null()) else {
        // A plain calculation: no workbook to open, nothing read or kept.
        let wb = Workbook::new();
        return eval(&wb, sheet_index(&wb, args)?);
    };
    with_book(args, area, |book| {
        let si = sheet_index(&book.wb, args)?;
        eval(&book.wb, si)
    })
}

/// `fill {book, sheet?, column, rows, formula}` — the formula filled down
/// `column` for `rows` rows. The numeric subset runs as one f64 compute
/// kernel (cached per formula text on the workbook); anything else falls
/// back to the engine's evaluator per row. Results land as values.
fn fill_column(args: &Json, area: &Area) -> Result<Json, String> {
    let col_s = args["column"].as_str().ok_or("sheet.fill: `column` is a column like C")?;
    let col = gridcraft_core::letters_to_col(col_s).ok_or_else(|| format!("sheet.fill: `{col_s}` is not a column"))?;
    let rows = args["rows"].as_u64().ok_or("sheet.fill: `rows` is required")? as usize;
    if rows == 0 || rows > fill::MAX_FILL_ROWS {
        return Err(format!("sheet.fill: `rows` is 1..={}", fill::MAX_FILL_ROWS));
    }
    let formula = args["formula"].as_str().ok_or("sheet.fill: `formula` is required")?;
    let body = formula.strip_prefix('=').unwrap_or(formula);
    let expr = gridcraft_formula::parse(body).map_err(|e| format!("sheet.fill: {e:?}"))?;
    with_book(args, area, |book| {
        let si = sheet_index(&book.wb, args)?;
        match fill::lower(&expr) {
            Ok(lowered) => {
                let kernel = match book.kernels.get(body) {
                    Some(k) => k.clone(),
                    None => {
                        let k = fill::compile(&lowered)?;
                        if book.kernels.len() >= 32 {
                            book.kernels.clear();
                        }
                        book.kernels.insert(body.to_string(), k.clone());
                        k
                    }
                };
                let inputs: Vec<Vec<f64>> =
                    lowered.cols.iter().map(|c| fill::column_f64(&book.wb, si, *c, rows)).collect();
                let out = fill::run(&kernel, &inputs, rows)?;
                let sheet = Arc::make_mut(&mut book.wb.sheets[si]);
                for (r, v) in out.iter().enumerate() {
                    sheet.set_value(CellRef::new(r as u32, col), Value::Number(*v));
                }
                Ok(json!({"rows": rows, "accelerated": true}))
            }
            Err(reason) => {
                for r in 0..rows {
                    let at = CellRef::new(r as u32, col);
                    let v = gridcraft_calc::recalc::evaluate_expr(&book.wb, si, at, &expr);
                    let sheet = Arc::make_mut(&mut book.wb.sheets[si]);
                    sheet.set_value(at, v);
                }
                Ok(json!({"rows": rows, "accelerated": false, "reason": reason}))
            }
        }
    })
}

fn recalc(args: &Json, area: &Area) -> Result<Json, String> {
    with_book(args, area, |book| {
        let Book { wb, calc, .. } = book;
        calc.recalc_all(wb);
        Ok(json!({}))
    })
}

fn export(args: &Json, area: &Area) -> Result<Json, String> {
    let rel = args["path"].as_str().unwrap_or("").to_string();
    let path = contained_path(area, &rel)?;
    area.check(&path, 0).map_err(|e| format!("sheet.export: {e}"))?;
    let bytes = with_book(args, area, |book| gridcraft_xlsx::write_xlsx(&book.wb).map_err(|e| format!("sheet.export: {e:?}")))?;
    if bytes.len() as u64 > MAX_XLSX_BYTES {
        return Err("sheet.export: the workbook is larger than the service writes".into());
    }
    area.write(&path, &bytes).map_err(|e| format!("sheet.export: {e}"))?;
    Ok(json!({"path": rel}))
}

fn close(args: &Json, area: &Area) -> Result<Json, String> {
    let id = args["book"].as_u64().ok_or("sheet: `book` is required")?;
    let owner = owner(area);
    let mut books = books().lock().map_err(|_| "sheet: poisoned")?;
    if !books.open.get(&id).is_some_and(|b| b.owner == owner) {
        return Err("sheet: no such workbook".into());
    }
    books.open.remove(&id);
    Ok(json!({}))
}

/// A path strictly inside the call's area: relative, no `..`, no absolute
/// component; the resolved parent must stay under the area even through
/// symlinks.
fn contained_path(area: &Area, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err("sheet: `path` is required".into());
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err("sheet: `path` stays inside this call's folder".into());
    }
    let joined = area.root.join(rel_path);
    let check_root = area.root.canonicalize().map_err(|e| format!("sheet: folder: {e}"))?;
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
    let resolved = deepest.canonicalize().map_err(|e| format!("sheet: {e}"))?;
    if !resolved.starts_with(&check_root) {
        return Err("sheet: `path` stays inside this call's folder".into());
    }
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A call's area at `root`, as the legacy one or a resolver gives it.
    fn at(root: &Path) -> Area {
        Area::new(root, None, true)
    }

    fn set(area: &Area, book: u64, cells: Json) -> Json {
        dispatch("set", &json!({"book": book, "cells": cells}), area).unwrap()
    }

    /// A call as App Hub hands it to the service.
    fn service_call(method: &str, args: Json, host_dir: &Path, may_prompt: bool) -> ServiceCall {
        ServiceCall { app_id: "os.fixture".into(), service: format!("sheet.{method}"), args, from_sheet: false, may_prompt, host_dir: host_dir.to_path_buf() }
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

    #[test]
    fn a1_roundtrips_with_the_engine() {
        for name in ["A1", "B2", "AA10", "C900"] {
            let c = parse_a1(name).unwrap();
            assert_eq!(c.a1(), name, "roundtrip {name}");
        }
        assert!(parse_a1("1A").is_err());
        assert!(parse_a1("A1:B2").is_err());
    }

    #[test]
    fn a_workbook_computes_and_exports_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let area = at(dir.path());

        let opened = book_new(&area).unwrap();
        let book = opened["book"].as_u64().unwrap();
        assert_eq!(opened["sheets"], json!(["Sheet1"]));

        set(&area, book, json!([
            {"at": "A1", "value": 2.0},
            {"at": "A2", "value": 3.0},
            {"at": "B1", "formula": "A1*A2+1"},
            {"at": "C1", "value": "label"},
        ]));
        dispatch("recalc", &json!({"book": book}), &area).unwrap();
        let got = dispatch("get", &json!({"book": book, "range": "A1:C1"}), &area).unwrap();
        assert_eq!(got["values"], json!([[2.0, 7.0, "label"]]));

        let ev = dispatch("eval", &json!({"book": book, "formula": "SUM(A1:A2)*10"}), &area).unwrap();
        assert_eq!(ev["value"], json!(50.0));

        let exported = dispatch("export", &json!({"book": book, "path": "out/test.xlsx"}), &area).unwrap();
        assert_eq!(exported["path"], json!("out/test.xlsx"));

        let reopened = dispatch("open", &json!({"path": "out/test.xlsx"}), &area).unwrap();
        let book2 = reopened["book"].as_u64().unwrap();
        dispatch("recalc", &json!({"book": book2}), &area).unwrap();
        let got2 = dispatch("get", &json!({"book": book2, "range": "B1"}), &area).unwrap();
        assert_eq!(got2["values"], json!([[7.0]]), "the formula survives the xlsx roundtrip");

        dispatch("close", &json!({"book": book}), &area).unwrap();
        dispatch("close", &json!({"book": book2}), &area).unwrap();
        assert!(dispatch("get", &json!({"book": book, "range": "A1"}), &area).is_err());
    }

    #[test]
    fn fill_matches_the_evaluator_and_falls_back_outside_the_subset() {
        let dir = tempfile::tempdir().unwrap();
        let area = at(dir.path());
        let book = book_new(&area).unwrap()["book"].as_u64().unwrap();
        // Columns A and B: 10k rows of inputs.
        {
            let mut books = books().lock().unwrap();
            let b = books.open.get_mut(&book).unwrap();
            let sheet = Arc::make_mut(&mut b.wb.sheets[0]);
            for r in 0..10_000u32 {
                sheet.set_value(CellRef::new(r, 0), Value::Number(r as f64 * 0.001));
                sheet.set_value(CellRef::new(r, 1), Value::Number(r as f64 * 0.013));
            }
        }
        let formula = "@A:A*1.05+SIN(@B:B)*0.5+EXP(-@A:A*0.01)";
        let done = dispatch("fill", &json!({"book": book, "column": "C", "rows": 10000, "formula": formula}), &area).unwrap();
        assert_eq!(done["accelerated"], json!(true), "{done}");
        // The kernel's column agrees with the engine's evaluator.
        let expr = gridcraft_formula::parse(formula).unwrap();
        {
            let mut books = books().lock().unwrap();
            let b = books.open.get_mut(&book).unwrap();
            for r in [0u32, 1234, 9999] {
                let want = match gridcraft_calc::recalc::evaluate_expr(&b.wb, 0, CellRef::new(r, 2), &expr) {
                    Value::Number(n) => n,
                    other => panic!("{other:?}"),
                };
                let got = match b.wb.sheets[0].cell(CellRef::new(r, 2)).map(|c| c.value.clone()) {
                    Some(Value::Number(n)) => n,
                    other => panic!("{other:?}"),
                };
                assert!((want - got).abs() <= 1e-9 * want.abs().max(1.0), "row {r}: {want} vs {got}");
            }
        }
        // The second fill with the same text reuses the cached kernel.
        let again = dispatch("fill", &json!({"book": book, "column": "D", "rows": 10000, "formula": formula}), &area).unwrap();
        assert_eq!(again["accelerated"], json!(true));
        // Outside the subset: computed anyway, honestly unaccelerated.
        let fb = dispatch("fill", &json!({"book": book, "column": "E", "rows": 16, "formula": "CONCAT(\"r\",@A:A)"}), &area);
        let fb = fb.unwrap();
        assert_eq!(fb["accelerated"], json!(false), "{fb}");
        dispatch("close", &json!({"book": book}), &area).unwrap();
    }

    #[test]
    fn paths_stay_inside_the_area() {
        let dir = tempfile::tempdir().unwrap();
        let area = at(dir.path());
        for bad in ["../up.xlsx", "/etc/x.xlsx", "a/../../up.xlsx", ""] {
            assert!(dispatch("open", &json!({"path": bad}), &area).is_err(), "{bad}");
            let book = book_new(&area).unwrap()["book"].as_u64().unwrap();
            assert!(dispatch("export", &json!({"book": book, "path": bad}), &area).is_err(), "{bad}");
            dispatch("close", &json!({"book": book}), &area).unwrap();
        }
    }

    #[test]
    fn only_system_apps_and_the_native_sheets_app_may_call() {
        assert!(may_call("os.sheets"));
        assert!(may_call("sheets"));
        assert!(!may_call("org.example.anything"));
        assert!(!may_call("sheetsy"));
        assert!(!may_call(""));
    }

    /// Without the shell's resolver the service works in its own area
    /// under the shared host directory, so an exported path can never land
    /// in another service's data, and an export may replace as before.
    #[test]
    fn without_a_resolver_the_area_is_a_subdirectory_of_the_host_dir() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let legacy = Slot::new();
        let book = serve(&legacy, &service_call("new", json!({}), host, false)).unwrap()["book"].as_u64().unwrap();
        for _ in 0..2 {
            serve(&legacy, &service_call("export", json!({"book": book, "path": "out.xlsx"}), host, false)).unwrap();
        }
        serve(&legacy, &service_call("close", json!({"book": book}), host, false)).unwrap();
        assert!(host.join("sheet/out.xlsx").is_file());
        assert!(!host.join("out.xlsx").exists());
    }

    /// `eval` without `book` computes against an empty transient workbook:
    /// the one read tool an agent can call with no session first, and with
    /// no folder either.
    #[test]
    fn eval_without_a_book_is_a_plain_calculation() {
        let v = eval_adhoc(&json!({"formula": "=1+2*3"}), None).unwrap();
        assert_eq!(v["value"], json!(7.0), "{v}");
        let v = eval_adhoc(&json!({"formula": "=CONCAT(\"a\",\"b\")"}), None).unwrap();
        assert_eq!(v["value"], json!("ab"), "{v}");
        let nowhere = Slot::new();
        nowhere.set(Some(std::sync::Arc::new(|_: &ServiceCall| Err("no folder".to_string()))));
        let v = serve(&nowhere, &service_call("eval", json!({"formula": "=2+2"}), Path::new("/nonexistent"), false)).unwrap();
        assert_eq!(v["value"], json!(4.0));
        assert!(serve(&nowhere, &service_call("new", json!({}), Path::new("/nonexistent"), false)).unwrap_err().contains("no folder"));
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
        let book = serve(&areas, &service_call("new", json!({}), &host, false)).unwrap()["book"].as_u64().unwrap();
        serve(&areas, &service_call("export", json!({"book": book, "path": "books/b.xlsx"}), &host, false)).unwrap();
        assert!(root.join("books/b.xlsx").is_file() && !host.exists() && !root.join("sheet").exists());
        let again = serve(&areas, &service_call("open", json!({"path": "books/b.xlsx"}), &host, false)).unwrap()["book"].as_u64().unwrap();
        std::fs::write(dir.path().join("beside.xlsx"), b"x").unwrap();
        for bad in ["../beside.xlsx", "/etc/hosts", "books/../../beside.xlsx"] {
            assert!(serve(&areas, &service_call("open", json!({"path": bad}), &host, false)).is_err(), "{bad}");
            assert!(serve(&areas, &service_call("export", json!({"book": book, "path": bad}), &host, true)).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), root.join("up")).unwrap();
            assert!(serve(&areas, &service_call("open", json!({"path": "up/beside.xlsx"}), &host, false)).is_err());
            assert!(serve(&areas, &service_call("export", json!({"book": book, "path": "up/made.xlsx"}), &host, true)).is_err());
            assert!(!dir.path().join("made.xlsx").exists());
        }
        for b in [book, again] {
            serve(&areas, &service_call("close", json!({"book": b}), &host, false)).unwrap();
        }
    }

    /// A workbook belongs to the area it was made in: another caller's
    /// folder sees no such workbook, even holding its handle.
    #[test]
    fn a_workbook_is_its_own_areas() {
        let dir = tempfile::tempdir().unwrap();
        let (mine, theirs) = (dir.path().join("mine"), dir.path().join("theirs"));
        std::fs::create_dir(&mine).unwrap();
        std::fs::create_dir(&theirs).unwrap();
        let (me, them) = (resolver(&mine, None), resolver(&theirs, None));
        let book = serve(&me, &service_call("new", json!({}), dir.path(), false)).unwrap()["book"].as_u64().unwrap();
        serve(&me, &service_call("set", json!({"book": book, "cells": [{"at": "A1", "value": "private"}]}), dir.path(), false)).unwrap();
        for (method, args) in [
            ("get", json!({"book": book, "range": "A1"})),
            ("set", json!({"book": book, "cells": [{"at": "A1", "value": 1}]})),
            ("eval", json!({"book": book, "formula": "=A1"})),
            ("recalc", json!({"book": book})),
            ("export", json!({"book": book, "path": "stolen.xlsx"})),
            ("close", json!({"book": book})),
        ] {
            let refused = serve(&them, &service_call(method, args, dir.path(), true)).unwrap_err();
            assert!(refused.contains("no such workbook"), "{method}: {refused}");
        }
        assert!(!theirs.join("stolen.xlsx").exists());
        let got = serve(&me, &service_call("get", json!({"book": book, "range": "A1"}), dir.path(), false)).unwrap();
        assert_eq!(got["values"], json!([["private"]]), "untouched");
        serve(&me, &service_call("close", json!({"book": book}), dir.path(), false)).unwrap();
    }

    /// An agent's export never replaces a file; an app's own foreground
    /// export may.
    #[test]
    fn an_agent_never_replaces_a_file_and_an_app_may() {
        let dir = tempfile::tempdir().unwrap();
        let areas = resolver(dir.path(), None);
        let book = serve(&areas, &service_call("new", json!({}), dir.path(), false)).unwrap()["book"].as_u64().unwrap();
        std::fs::write(dir.path().join("taken.xlsx"), b"keep me").unwrap();
        let refused = serve(&areas, &service_call("export", json!({"book": book, "path": "taken.xlsx"}), dir.path(), false)).unwrap_err();
        assert!(refused.contains("`taken.xlsx` already exists"), "{refused}");
        assert_eq!(std::fs::read(dir.path().join("taken.xlsx")).unwrap(), b"keep me");
        serve(&areas, &service_call("export", json!({"book": book, "path": "taken.xlsx"}), dir.path(), true)).unwrap();
        assert!(std::fs::read(dir.path().join("taken.xlsx")).unwrap().starts_with(b"PK"));
        serve(&areas, &service_call("close", json!({"book": book}), dir.path(), true)).unwrap();
    }

    /// What a call writes must fit what is left of the area's quota.
    #[test]
    fn output_over_the_quota_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let tight = resolver(dir.path(), Some(100));
        let book = serve(&tight, &service_call("new", json!({}), dir.path(), true)).unwrap()["book"].as_u64().unwrap();
        let refused = serve(&tight, &service_call("export", json!({"book": book, "path": "b.xlsx"}), dir.path(), true)).unwrap_err();
        assert!(refused.contains("bytes left"), "{refused}");
        assert!(!dir.path().join("b.xlsx").exists());
        serve(&tight, &service_call("close", json!({"book": book}), dir.path(), true)).unwrap();
    }
}
