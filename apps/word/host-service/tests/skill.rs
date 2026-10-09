//! The word engine's skill (ADR 0013): `skill/commands.md` and
//! `skill/safety.json` are what wordcraft's live catalog, at the pinned
//! revision, and `skill/safety-rules.json` generate. Regenerate with
//! `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-word-service --test skill`.

use std::path::Path;

use octosense_skill_gen::{check_commands, Catalog, Entry};

/// The engine's command registry (the service feeds the engine bytes; no
/// tool runs these ids directly yet).
fn catalog() -> Catalog {
    let registry = wordcraft_engine::cmd::registry();
    let entries = registry.all().iter().map(|c| Entry::new(c.id, c.label, c.params)).collect();
    Catalog { family: "word", engine: "wordcraft", engine_crate: "wordcraft-engine", package: env!("CARGO_PKG_NAME"), entries }
}

#[test]
fn the_skill_references_match_the_pinned_engine() {
    check_commands(&catalog(), &Path::new(env!("CARGO_MANIFEST_DIR")).join("skill"));
}
