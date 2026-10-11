//! The design engine's skill (ADR 0013): `skill/commands.md` and
//! `skill/safety.json` are what designcraft's live catalog, at the pinned
//! revision, and `skill/safety-rules.json` generate. Regenerate with
//! `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-design-service --test skill`.

use std::path::Path;

use octosense_skill_gen::{check_commands, Catalog, Entry};

/// The catalog `design.commands` serves.
fn catalog() -> Catalog {
    let entries = designcraft_engine::command_specs().iter().map(|c| Entry::new(c.id, c.label, c.params)).collect();
    Catalog { family: "design", engine: "designcraft", engine_crate: "designcraft-engine", package: env!("CARGO_PKG_NAME"), entries }
}

#[test]
fn the_skill_references_match_the_pinned_engine() {
    check_commands(&catalog(), &Path::new(env!("CARGO_MANIFEST_DIR")).join("skill"));
}
