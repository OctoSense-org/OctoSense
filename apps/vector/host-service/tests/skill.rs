//! The vector engine's skill (ADR 0013): `skill/commands.md` and
//! `skill/safety.json` are what vectorcraft's live catalog, at the pinned
//! revision, and `skill/safety-rules.json` generate. Regenerate with
//! `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-vector-service --test skill`.

use std::path::Path;

use octosense_skill_gen::{check_commands, Catalog, Entry};

/// The engine's whole catalog (`vector.commands` serves it minus the ids
/// `vector.run` refuses), so every id gets a class.
fn catalog() -> Catalog {
    let entries = vectorcraft_engine::command_specs().iter().map(|c| Entry::new(c.id, c.label, c.params)).collect();
    Catalog { family: "vector", engine: "vectorcraft", engine_crate: "vectorcraft-engine", package: env!("CARGO_PKG_NAME"), entries }
}

#[test]
fn the_skill_references_match_the_pinned_engine() {
    check_commands(&catalog(), &Path::new(env!("CARGO_MANIFEST_DIR")).join("skill"));
}
