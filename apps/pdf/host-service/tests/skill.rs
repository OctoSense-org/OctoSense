//! The pdf engine's skill (ADR 0013): `skill/commands.md` and
//! `skill/safety.json` are what pdfcraft's live tool table, at the pinned
//! revision, and `skill/safety-rules.json` generate. Regenerate with
//! `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-pdf-service --test skill`.
//! The engine's hardened PDF stack, patched in at the same revision, must be
//! the one built.

use std::path::Path;

use octosense_skill_gen::{check_commands, locked_revision, params_from_schema, workspace_lock, Catalog, Entry};

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

/// The root Cargo.toml patches pdfcraft's bootstrap PDF stack (hayro,
/// hayro-interpret, hayro-jbig2, hayro-syntax, lopdf) with the engine's
/// hardened vendor/ copies, at the engine's own revision. A patch whose
/// version loses to a newer crates.io release goes unused, and the
/// crates.io crate is built instead, without the engine's limits: until the
/// upstream sync, hayro-jbig2's patch (0.3.0) lost to crates.io's 0.3.1, so
/// JBIG2 images were decoded without its bounds on bitmap and dictionary
/// pixels and on decoding steps that never advance. Pin such a release down
/// to the patch's version, e.g. `cargo update -p hayro-jbig2@0.3.1 --precise 0.3.0`.
#[test]
fn the_engines_hardened_pdf_stack_is_the_one_built() {
    let lock = std::fs::read_to_string(workspace_lock()).expect("Cargo.lock");
    let mut sections = lock.split("[[patch.unused]]");
    let packages = sections.next().unwrap_or_default();
    let unused: Vec<&str> =
        sections.filter_map(|entry| entry.lines().find_map(|l| l.strip_prefix("name = \"")?.strip_suffix('"'))).collect();
    let revision = locked_revision(&lock, "pdfcraft-automation").expect("the engine's revision");
    let source = format!("source = \"git+https://github.com/ymote/pdfcraft.git?rev={revision}#{revision}\"");
    for name in ["hayro", "hayro-interpret", "hayro-jbig2", "hayro-syntax", "lopdf"] {
        assert!(!unused.contains(&name), "Cargo.lock lists {name}'s [patch] as unused, so the crates.io crate is built instead");
        let name_line = format!("\nname = \"{name}\"\n");
        let built = packages.split("[[package]]").any(|block| block.contains(&name_line) && block.contains(&source));
        assert!(built, "{name} is not built from the engine's revision {revision}");
    }
}
