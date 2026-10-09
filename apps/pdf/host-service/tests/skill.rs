//! The pdf engine's skill (ADR 0013): `skill/commands.md` and
//! `skill/safety.json` are what pdfcraft's live tool table, at the pinned
//! revision, and `skill/safety-rules.json` generate. Regenerate with
//! `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-pdf-service --test skill`.

use std::path::Path;

use octosense_skill_gen::{check_commands, params_from_schema, Catalog, Entry};

/// pdfcraft's automation tools: the table `pdf.*` drives the engine
/// through (`Automation::call`), each with its JSON Schema.
fn catalog() -> Catalog {
    let entries = pdfcraft_automation::tools()
        .iter()
        .map(|t| Entry::new(t.name, format!("{}. {}", t.title, t.description), params_from_schema(&t.input_schema)))
        .collect();
    Catalog { family: "pdf", engine: "pdfcraft", engine_crate: "pdfcraft-automation", package: env!("CARGO_PKG_NAME"), entries }
}

#[test]
fn the_skill_references_match_the_pinned_engine() {
    check_commands(&catalog(), &Path::new(env!("CARGO_MANIFEST_DIR")).join("skill"));
}
