//! `octosense-sheets-service` — the `sheet` host service (ADR 0013).
//!
//! gridcraft's spreadsheet engine behind typed `sheet.*` methods. A
//! workbook is a host-owned session: created empty or read from an xlsx
//! under the caller's host directory, edited as values and formulas,
//! recalculated on demand, and written back as xlsx into the same
//! directory. Everything is JSON at the boundary; the engine's types never
//! cross it.
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
//! Paths never leave the service's own area, `<host dir>/sheet` ([`area`]):
//! `..`, absolute paths and symlink escapes are refused, the same stance
//! the files host tools take, and the shared host directory's other
//! services (Mail's vaults, Calendar's events) stay out of reach. The
//! service serves system apps, and the native Sheets app, whose agent's
//! `sheets.*` tools the shell routes here
//! (`crates/shell/src/host_tools/engines.rs`).

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
use serde_json::{json, Value as Json};

/// One open workbook session.
struct Book {
    wb: Workbook,
    calc: Calc,
    /// `sheet.fill`'s compiled kernels, by canonical formula text.
    kernels: HashMap<String, std::sync::Arc<makepad_script_compute::kernel::Kernel>>,
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

/// Open workbooks across every caller; a runaway caller is bounded.
const MAX_OPEN_BOOKS: usize = 16;
/// The largest xlsx the service reads or writes (bytes).
const MAX_XLSX_BYTES: u64 = 64 << 20;

/// Which apps may call the service: system apps, as News, and the native
/// Sheets app (the shell routes its agent's `sheets.*` tools here as
/// `"sheets"`).
fn may_call(app_id: &str) -> bool {
    app_id == "sheets" || app_id.starts_with("os.")
}

/// The service's own area under the shared host directory. Every path a
/// caller names is contained here, never in the host directory itself,
/// which other services (Mail, Calendar) keep their data under.
fn area(host_dir: &Path) -> Result<PathBuf, String> {
    let area = host_dir.join("sheet");
    std::fs::create_dir_all(&area).map_err(|e| format!("sheet: service area: {e}"))?;
    Ok(area)
}

pub struct SheetsService;

/// Register the `sheet` service with App Hub's host-service registry.
pub fn register() {
    register_host_service(Box::new(SheetsService));
}

impl HostService for SheetsService {
    fn family(&self) -> &'static str {
        "sheet"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        if !may_call(&call.app_id) {
            reply.send(Err("The sheet service serves system apps only.".into()));
            return;
        }
        let method = call.method().to_string();
        let args = call.args.clone();
        reply.send(area(&call.host_dir).and_then(|area| dispatch(&method, &args, &area)));
    }
}

fn dispatch(method: &str, args: &Json, host_dir: &Path) -> Result<Json, String> {
    match method {
        "new" => book_new(),
        "open" => book_open(args, host_dir),
        "set" => cells_set(args),
        "get" => cells_get(args),
        "eval" => eval_adhoc(args),
        "recalc" => recalc(args),
        "fill" => fill_column(args),
        "export" => export(args, host_dir),
        "close" => close(args),
        other => Err(format!("sheet.{other} is not a method of the sheet service")),
    }
}

fn insert(wb: Workbook) -> Result<Json, String> {
    let names: Vec<String> = wb.sheets.iter().map(|s| s.name.clone()).collect();
    let mut books = books().lock().map_err(|_| "sheet: poisoned")?;
    if books.open.len() >= MAX_OPEN_BOOKS {
        return Err(format!("sheet: {MAX_OPEN_BOOKS} workbooks are already open; close one first"));
    }
    books.next += 1;
    let id = books.next;
    books.open.insert(id, Book { wb, calc: Calc::new(), kernels: HashMap::new() });
    Ok(json!({"book": id, "sheets": names}))
}

fn book_new() -> Result<Json, String> {
    insert(Workbook::new())
}

fn book_open(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let path = contained_path(host_dir, args["path"].as_str().unwrap_or(""))?;
    let meta = std::fs::metadata(&path).map_err(|e| format!("sheet.open: {e}"))?;
    if meta.len() > MAX_XLSX_BYTES {
        return Err("sheet.open: the file is larger than the service reads".into());
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("sheet.open: {e}"))?;
    let (wb, _report) = gridcraft_xlsx::read_xlsx(&bytes).map_err(|e| format!("sheet.open: {e:?}"))?;
    insert(wb)
}

fn with_book<T>(args: &Json, f: impl FnOnce(&mut Book) -> Result<T, String>) -> Result<T, String> {
    let id = args["book"].as_u64().ok_or("sheet: `book` is required")?;
    let mut books = books().lock().map_err(|_| "sheet: poisoned")?;
    let book = books.open.get_mut(&id).ok_or("sheet: no such workbook (opened and not closed?)")?;
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

fn cells_set(args: &Json) -> Result<Json, String> {
    with_book(args, |book| {
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

fn cells_get(args: &Json) -> Result<Json, String> {
    with_book(args, |book| {
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

fn eval_adhoc(args: &Json) -> Result<Json, String> {
    let eval = |wb: &Workbook, si: usize| -> Result<Json, String> {
        let at = match args["at"].as_str() {
            Some(s) => parse_a1(s)?,
            None => CellRef::new(0, 0),
        };
        let formula = args["formula"].as_str().ok_or("sheet.eval: `formula` is required")?;
        let v = gridcraft_calc::evaluate(wb, si, at, formula);
        Ok(json!({"value": value_to_json(&v)}))
    };
    if args["book"].is_null() {
        // A plain calculation: no workbook to open, nothing read or kept.
        let wb = Workbook::new();
        return eval(&wb, sheet_index(&wb, args)?);
    }
    with_book(args, |book| {
        let si = sheet_index(&book.wb, args)?;
        eval(&book.wb, si)
    })
}

/// `fill {book, sheet?, column, rows, formula}` — the formula filled down
/// `column` for `rows` rows. The numeric subset runs as one f64 compute
/// kernel (cached per formula text on the workbook); anything else falls
/// back to the engine's evaluator per row. Results land as values.
fn fill_column(args: &Json) -> Result<Json, String> {
    let col_s = args["column"].as_str().ok_or("sheet.fill: `column` is a column like C")?;
    let col = gridcraft_core::letters_to_col(col_s).ok_or_else(|| format!("sheet.fill: `{col_s}` is not a column"))?;
    let rows = args["rows"].as_u64().ok_or("sheet.fill: `rows` is required")? as usize;
    if rows == 0 || rows > fill::MAX_FILL_ROWS {
        return Err(format!("sheet.fill: `rows` is 1..={}", fill::MAX_FILL_ROWS));
    }
    let formula = args["formula"].as_str().ok_or("sheet.fill: `formula` is required")?;
    let body = formula.strip_prefix('=').unwrap_or(formula);
    let expr = gridcraft_formula::parse(body).map_err(|e| format!("sheet.fill: {e:?}"))?;
    with_book(args, |book| {
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

fn recalc(args: &Json) -> Result<Json, String> {
    with_book(args, |book| {
        let Book { wb, calc, .. } = book;
        calc.recalc_all(wb);
        Ok(json!({}))
    })
}

fn export(args: &Json, host_dir: &Path) -> Result<Json, String> {
    let rel = args["path"].as_str().unwrap_or("").to_string();
    let path = contained_path(host_dir, &rel)?;
    with_book(args, |book| {
        let bytes = gridcraft_xlsx::write_xlsx(&book.wb).map_err(|e| format!("sheet.export: {e:?}"))?;
        if bytes.len() as u64 > MAX_XLSX_BYTES {
            return Err("sheet.export: the workbook is larger than the service writes".into());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("sheet.export: {e}"))?;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("sheet.export: {e}"))?;
        Ok(json!({"path": rel}))
    })
}

fn close(args: &Json) -> Result<Json, String> {
    let id = args["book"].as_u64().ok_or("sheet: `book` is required")?;
    let mut books = books().lock().map_err(|_| "sheet: poisoned")?;
    books.open.remove(&id).ok_or("sheet: no such workbook")?;
    Ok(json!({}))
}

/// A path strictly inside `host_dir`: relative, no `..`, no absolute
/// component; the resolved parent must stay under the host dir even
/// through symlinks.
fn contained_path(host_dir: &Path, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err("sheet: `path` is required".into());
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err("sheet: `path` stays inside the app's host directory".into());
    }
    let joined = host_dir.join(rel_path);
    let check_root = host_dir.canonicalize().map_err(|e| format!("sheet: host dir: {e}"))?;
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
        return Err("sheet: `path` stays inside the app's host directory".into());
    }
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(book: u64, cells: Json) -> Json {
        dispatch("set", &json!({"book": book, "cells": cells}), Path::new("/")).unwrap()
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
        let host = dir.path();

        let opened = book_new().unwrap();
        let book = opened["book"].as_u64().unwrap();
        assert_eq!(opened["sheets"], json!(["Sheet1"]));

        set(book, json!([
            {"at": "A1", "value": 2.0},
            {"at": "A2", "value": 3.0},
            {"at": "B1", "formula": "A1*A2+1"},
            {"at": "C1", "value": "label"},
        ]));
        dispatch("recalc", &json!({"book": book}), host).unwrap();
        let got = dispatch("get", &json!({"book": book, "range": "A1:C1"}), host).unwrap();
        assert_eq!(got["values"], json!([[2.0, 7.0, "label"]]));

        let ev = dispatch("eval", &json!({"book": book, "formula": "SUM(A1:A2)*10"}), host).unwrap();
        assert_eq!(ev["value"], json!(50.0));

        let exported = dispatch("export", &json!({"book": book, "path": "out/test.xlsx"}), host).unwrap();
        assert_eq!(exported["path"], json!("out/test.xlsx"));

        let reopened = dispatch("open", &json!({"path": "out/test.xlsx"}), host).unwrap();
        let book2 = reopened["book"].as_u64().unwrap();
        dispatch("recalc", &json!({"book": book2}), host).unwrap();
        let got2 = dispatch("get", &json!({"book": book2, "range": "B1"}), host).unwrap();
        assert_eq!(got2["values"], json!([[7.0]]), "the formula survives the xlsx roundtrip");

        dispatch("close", &json!({"book": book}), host).unwrap();
        dispatch("close", &json!({"book": book2}), host).unwrap();
        assert!(dispatch("get", &json!({"book": book, "range": "A1"}), host).is_err());
    }

    #[test]
    fn fill_matches_the_evaluator_and_falls_back_outside_the_subset() {
        let host = Path::new("/");
        let book = book_new().unwrap()["book"].as_u64().unwrap();
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
        let done = dispatch("fill", &json!({"book": book, "column": "C", "rows": 10000, "formula": formula}), host).unwrap();
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
        let again = dispatch("fill", &json!({"book": book, "column": "D", "rows": 10000, "formula": formula}), host).unwrap();
        assert_eq!(again["accelerated"], json!(true));
        // Outside the subset: computed anyway, honestly unaccelerated.
        let fb = dispatch("fill", &json!({"book": book, "column": "E", "rows": 16, "formula": "CONCAT(\"r\",@A:A)"}), host);
        let fb = fb.unwrap();
        assert_eq!(fb["accelerated"], json!(false), "{fb}");
        dispatch("close", &json!({"book": book}), host).unwrap();
    }

    #[test]
    fn paths_stay_inside_the_host_dir() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        for bad in ["../up.xlsx", "/etc/x.xlsx", "a/../../up.xlsx", ""] {
            assert!(dispatch("open", &json!({"path": bad}), host).is_err(), "{bad}");
            let book = book_new().unwrap()["book"].as_u64().unwrap();
            assert!(dispatch("export", &json!({"book": book, "path": bad}), host).is_err(), "{bad}");
            dispatch("close", &json!({"book": book}), host).unwrap();
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

    /// The service works in its own area under the shared host directory,
    /// so an exported path can never land in another service's data.
    #[test]
    fn the_service_area_is_a_subdirectory_of_the_host_dir() {
        let dir = tempfile::tempdir().unwrap();
        let host = dir.path();
        let a = area(host).unwrap();
        assert_eq!(a, host.join("sheet"));
        assert!(a.is_dir());
        let book = book_new().unwrap()["book"].as_u64().unwrap();
        dispatch("export", &json!({"book": book, "path": "out.xlsx"}), &a).unwrap();
        dispatch("close", &json!({"book": book}), &a).unwrap();
        assert!(host.join("sheet/out.xlsx").is_file());
        assert!(!host.join("out.xlsx").exists());
    }

    /// `eval` without `book` computes against an empty transient workbook:
    /// the one read tool an agent can call with no session first.
    #[test]
    fn eval_without_a_book_is_a_plain_calculation() {
        let v = eval_adhoc(&json!({"formula": "=1+2*3"})).unwrap();
        assert_eq!(v["value"], json!(7.0), "{v}");
        let v = eval_adhoc(&json!({"formula": "=CONCAT(\"a\",\"b\")"})).unwrap();
        assert_eq!(v["value"], json!("ab"), "{v}");
    }
}
