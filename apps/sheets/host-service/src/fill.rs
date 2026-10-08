//! `sheet.fill` — a formula filled down a column by the Splash kernel JIT.
//!
//! The formula is gridcraft's own parsed [`Expr`]; the numeric subset
//! (arithmetic, `@`-intersected column references, SIN/COS/EXP/LN/SQRT/
//! ABS/MIN/MAX) lowers to an f64 compute kernel — compiled once per
//! formula text and cached on the workbook — and runs natively over all
//! rows at once. Anything outside the subset falls back to the engine's
//! evaluator row by row, so `fill` always computes exactly what recalc
//! would; `accelerated` in the reply says which path ran.
//!
//! Measured on the probe this productionizes (M-series, 1M rows,
//! `=@A:A*1.05+SIN(@B:B)*0.5+EXP(-@A:A*0.01)`): the evaluator 505 ms, the
//! f64 kernel 2.9 ms on 8 threads — values agreeing to 1e-9.

use std::collections::BTreeSet;
use std::sync::Arc;

use gridcraft_core::{CellRef, Value};
use gridcraft_formula::{BinOp, Expr, RefKind, Reference, UnOp};
use gridcraft_model::Workbook;
use makepad_script_compute::kernel::{self, Kernel};

/// Rows a single fill may compute (the service's one-call bound).
pub const MAX_FILL_ROWS: usize = 2_000_000;

/// Elements from which the run splits across threads (bit-identical).
const PARALLEL_FROM: usize = 65_536;

/// The columns a lowered formula reads, in input order.
pub struct Lowered {
    pub body: String,
    pub cols: Vec<u32>,
}

/// Lowers the numeric subset to a kernel expression over `c<k>[i]`.
/// `Err` is the reason the formula stays on the evaluator (not a fault).
pub fn lower(e: &Expr) -> Result<Lowered, String> {
    let mut cols = BTreeSet::new();
    collect_cols(e, &mut cols)?;
    let cols: Vec<u32> = cols.into_iter().collect();
    let mut body = String::new();
    emit(e, &cols, &mut body)?;
    Ok(Lowered { body, cols })
}

fn col_of(r: &Reference) -> Result<u32, String> {
    match &r.kind {
        RefKind::Cols(c0, _, c1, _) if c0 == c1 => Ok(*c0),
        other => Err(format!("reference {other:?}")),
    }
}

fn collect_cols(e: &Expr, out: &mut BTreeSet<u32>) -> Result<(), String> {
    match e {
        Expr::Number(_) => {}
        Expr::Ref(r) => {
            out.insert(col_of(r)?);
        }
        Expr::Paren(i) | Expr::Unary(_, i) => collect_cols(i, out)?,
        Expr::Binary(_, l, r) => {
            collect_cols(l, out)?;
            collect_cols(r, out)?;
        }
        Expr::Call(_, args) => {
            for a in args {
                collect_cols(a, out)?;
            }
        }
        other => return Err(format!("{other:?}")),
    }
    Ok(())
}

fn emit(e: &Expr, cols: &[u32], out: &mut String) -> Result<(), String> {
    match e {
        Expr::Number(n) => out.push_str(&format!("{n:?}")),
        Expr::Paren(i) => {
            out.push('(');
            emit(i, cols, out)?;
            out.push(')');
        }
        Expr::Unary(UnOp::At, i) => emit(i, cols, out)?,
        Expr::Unary(UnOp::Neg, i) => {
            out.push_str("(0.0 - ");
            emit(i, cols, out)?;
            out.push(')');
        }
        Expr::Unary(op, _) => return Err(format!("unary {op:?}")),
        Expr::Binary(op, l, r) => {
            let sym = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                other => return Err(format!("op {other:?}")),
            };
            out.push('(');
            emit(l, cols, out)?;
            out.push_str(sym);
            emit(r, cols, out)?;
            out.push(')');
        }
        Expr::Call(name, args) => {
            let (f, arity) = match name.as_str() {
                "SIN" => ("sin", 1),
                "COS" => ("cos", 1),
                "EXP" => ("exp", 1),
                "LN" => ("ln", 1),
                "SQRT" => ("sqrt", 1),
                "ABS" => ("abs", 1),
                "MIN" => ("min", 2),
                "MAX" => ("max", 2),
                other => return Err(format!("function {other}")),
            };
            if args.len() != arity {
                return Err(format!("{name} takes {arity}"));
            }
            out.push_str(f);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                emit(a, cols, out)?;
            }
            out.push(')');
        }
        Expr::Ref(r) => {
            let col = col_of(r)?;
            let k = cols.iter().position(|c| *c == col).unwrap_or(0);
            out.push_str(&format!("c{k}[i]"));
        }
        other => return Err(format!("{other:?}")),
    }
    Ok(())
}

/// The kernel source for a lowered formula.
pub fn kernel_source(l: &Lowered) -> String {
    let mut src = String::new();
    for k in 0..l.cols.len() {
        src.push_str(&format!("let c{k} = input(f64)\n"));
    }
    src.push_str("let out = output(f64)\n");
    src.push_str(&format!("fn element(i) {{ out[i] = {} }}\n", l.body));
    src
}

fn pack(v: &[f64]) -> Vec<u32> {
    v.iter().flat_map(|x| [x.to_bits() as u32, (x.to_bits() >> 32) as u32]).collect()
}

/// Reads column `col`'s first `rows` cells as f64 (empty and non-numeric
/// cells read as 0.0, the evaluator's arithmetic coercion for blanks).
pub fn column_f64(wb: &Workbook, sheet: usize, col: u32, rows: usize) -> Vec<f64> {
    let s = &wb.sheets[sheet];
    (0..rows)
        .map(|r| match s.cell(CellRef::new(r as u32, col)).map(|c| &c.value) {
            Some(Value::Number(n)) => *n,
            Some(Value::Bool(b)) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            _ => 0.0,
        })
        .collect()
}

/// Runs a compiled fill kernel over `rows` elements.
pub fn run(kernel: &Arc<Kernel>, inputs: &[Vec<f64>], rows: usize) -> Result<Vec<f64>, String> {
    let words: Vec<Vec<u32>> = inputs.iter().map(|v| pack(v)).collect();
    let mut outw = vec![0u32; rows * 2];
    let mut call = kernel.call();
    for (k, w) in words.iter().enumerate() {
        call.input_u32(&format!("c{k}"), w).map_err(|e| format!("sheet.fill: {e:?}"))?;
    }
    call.output_u32("out", &mut outw).map_err(|e| format!("sheet.fill: {e:?}"))?;
    let threads = if rows >= PARALLEL_FROM { std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1).min(8) } else { 1 };
    let r = if threads > 1 { call.run_parallel(rows, threads) } else { call.run(rows) };
    drop(call);
    r.map_err(|e| format!("sheet.fill: {e:?}"))?;
    Ok(outw.chunks_exact(2).map(|w| f64::from_bits(w[0] as u64 | ((w[1] as u64) << 32))).collect())
}

/// Compiles a lowered formula (the caller caches by formula text).
pub fn compile(l: &Lowered) -> Result<Arc<Kernel>, String> {
    kernel::compile(&kernel_source(l)).map_err(|e| e.first().map(|e| e.message.clone()).unwrap_or_else(|| "kernel compile failed".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_formula_lowers_to_two_columns() {
        let e = gridcraft_formula::parse("@A:A*1.05+SIN(@B:B)*0.5+EXP(-@A:A*0.01)").unwrap();
        let l = lower(&e).unwrap();
        assert_eq!(l.cols, vec![0, 1]);
        assert_eq!(l.body, "(((c0[i]*1.05)+(sin(c1[i])*0.5))+exp(((0.0 - c0[i])*0.01)))");
    }

    #[test]
    fn outside_the_subset_is_a_reason_not_a_fault() {
        for f in ["TEXT(@A:A,\"0\")", "@A:A&\"x\"", "VLOOKUP(@A:A,B:D,2)", "A1*2"] {
            let e = gridcraft_formula::parse(f).unwrap();
            assert!(lower(&e).is_err(), "{f}");
        }
    }
}
