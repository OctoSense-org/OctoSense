//! The photo engine's skill (ADR 0013): `skill/commands.md` and
//! `skill/safety.json` are what photocraft's live catalog, at the pinned
//! revision, and `skill/safety-rules.json` generate. Regenerate with
//! `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-photo-service --test skill`.

use std::path::Path;

use octosense_skill_gen::{check_commands, Catalog, Entry};
use photocraft_automation::headless::Headless;

/// The engine's whole catalog, before `photo.commands` narrows it to what
/// the command door runs, so every id gets a class.
fn catalog() -> Catalog {
    let list = Headless::new().command_list();
    let entries = list
        .as_array()
        .expect("photocraft lists its commands")
        .iter()
        .map(|c| Entry::new(c["id"].as_str().expect("an id"), c["label"].as_str().unwrap_or(""), c["params"].as_str().unwrap_or("")))
        .collect();
    Catalog { family: "photo", engine: "photocraft", engine_crate: "photocraft-automation", package: env!("CARGO_PKG_NAME"), entries }
}

#[test]
fn the_skill_references_match_the_pinned_engine() {
    check_commands(&catalog(), &Path::new(env!("CARGO_MANIFEST_DIR")).join("skill"));
}
