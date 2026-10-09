//! The sheet engine's skill (ADR 0013): `skill/functions.md` is gridcraft's
//! worksheet-function catalog at the pinned revision (the engine works by
//! formula, not by command ids, so it has no `commands.md` or
//! `safety.json`). Regenerate with
//! `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-sheets-service --test skill`.

use std::fmt::Write as _;
use std::path::Path;

use octosense_skill_gen::{check_generated, locked_revision, regen_command, workspace_lock};

/// `functions.md`: every worksheet function by category, one per line as
/// its signature and description, then the special forms the calculator
/// evaluates itself.
fn functions_md() -> String {
    let mut functions: Vec<&gridcraft_functions::FnSpec> = gridcraft_functions::all().iter().collect();
    functions.sort_by(|a, b| (format!("{:?}", a.category), a.name).cmp(&(format!("{:?}", b.category), b.name)));
    let lock = std::fs::read_to_string(workspace_lock()).expect("Cargo.lock");
    let revision = locked_revision(&lock, "gridcraft-functions").expect("gridcraft's locked revision");
    let mut out = String::new();
    let _ = writeln!(out, "# sheet engine functions");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "The gridcraft calculator's worksheet functions at revision {}: {} functions by category, one per line as \
         `SIGNATURE` description; (volatile) marks one that changes on every recalculation. Generated from the engine \
         by `{}`; do not edit.",
        revision.get(..12).unwrap_or(&revision),
        functions.len(),
        regen_command(env!("CARGO_PKG_NAME"))
    );
    let _ = writeln!(out);
    let mut special: Vec<&str> = gridcraft_calc::SPECIAL_FUNCTIONS.to_vec();
    special.sort_unstable();
    special.dedup();
    let _ = writeln!(out, "The calculator also evaluates these forms itself (references, laziness, hidden rows): {}.", special.join(", "));
    let mut category = String::new();
    for f in functions {
        let c = format!("{:?}", f.category);
        if c != category {
            let _ = writeln!(out);
            let _ = writeln!(out, "## {c}");
            let _ = writeln!(out);
            category = c;
        }
        let volatile = if f.volatile { " (volatile)" } else { "" };
        let description = f.description.split_whitespace().collect::<Vec<_>>().join(" ");
        let _ = writeln!(out, "- `{}`{volatile} {description}", f.signature);
    }
    out
}

#[test]
fn the_function_reference_matches_the_pinned_engine() {
    check_generated(env!("CARGO_PKG_NAME"), &Path::new(env!("CARGO_MANIFEST_DIR")).join("skill"), "functions.md", &functions_md());
}

/// Every `` `sheets.eval {...}` gives N `` in `SKILL.md` is what the engine
/// computes (`sheet.eval` without a book: an empty workbook), so the
/// examples the system agent learns from stay true across engine pins.
#[test]
fn the_skill_examples_compute_what_they_say() {
    let skill = include_str!("../skill/SKILL.md");
    let mut checked = 0;
    for line in skill.lines() {
        let Some(start) = line.find("`sheets.eval ") else { continue };
        let call = &line[start + "`sheets.eval ".len()..];
        // A call with arguments, not the tool list's signature.
        let Some(end) = call.find("}`").filter(|_| call.starts_with("{\"")) else { continue };
        let args: serde_json::Value = serde_json::from_str(&call[..=end]).expect("an example's arguments are JSON");
        let Some(said) = call[end + 2..].trim_start().strip_prefix("gives ") else { continue };
        let said = said.trim_end_matches('.');
        let (about, number) = match said.strip_prefix("about ") {
            Some(n) => (true, n),
            None => (false, said),
        };
        let want: f64 = number.parse().unwrap_or_else(|_| panic!("`{said}` is not a number"));
        let formula = args["formula"].as_str().expect("a formula");
        let got = match gridcraft_calc::evaluate(&gridcraft_model::Workbook::new(), 0, gridcraft_core::CellRef::new(0, 0), formula) {
            gridcraft_core::Value::Number(n) => n,
            other => panic!("{formula} gives {other:?}"),
        };
        let close = if about { (got - want).abs() < 0.005 } else { got == want };
        assert!(close, "{formula}: the skill says {said}, the engine gives {got}");
        checked += 1;
    }
    assert!(checked >= 3, "the skill's worked calculations were found ({checked})");
}
