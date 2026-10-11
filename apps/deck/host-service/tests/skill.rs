//! The deck engine's skill (ADR 0013): `skill/commands.md` and
//! `skill/safety.json` are what deckcraft's live catalog, at the pinned
//! revision, and `skill/safety-rules.json` generate. Regenerate with
//! `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-deck-service --test skill`.

use std::path::Path;

use octosense_skill_gen::{check_commands, Catalog, Entry};

/// The engine's command catalog (the service drives the engine with bytes
/// and typed methods; no tool runs these ids yet).
fn catalog() -> Catalog {
    let entries = deckcraft_engine::command_specs().iter().map(|c| Entry::new(c.id, c.label, c.params)).collect();
    Catalog { family: "deck", engine: "deckcraft", engine_crate: "deckcraft-engine", package: env!("CARGO_PKG_NAME"), entries }
}

#[test]
fn the_skill_references_match_the_pinned_engine() {
    check_commands(&catalog(), &Path::new(env!("CARGO_MANIFEST_DIR")).join("skill"));
}
