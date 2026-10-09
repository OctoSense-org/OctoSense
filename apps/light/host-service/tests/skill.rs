//! The light engine's skill (ADR 0013): `skill/commands.md` and
//! `skill/safety.json` are what lightcraft's live catalog, at the pinned
//! revision, and `skill/safety-rules.json` generate, and
//! `skill/controls.md` is its develop-control catalog (the keys of
//! `light.develop`'s `params`). Regenerate with
//! `OCTOSENSE_SKILL_REGEN=1 cargo test --locked -p octosense-light-service --test skill`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use octosense_skill_gen::{check_commands, check_generated, locked_revision, regen_command, workspace_lock, Catalog, Entry};
use serde_json::{json, Value};

fn skill_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("skill")
}

/// The engine's command catalog (the service drives a few of these ids
/// itself; no tool runs them directly yet).
fn catalog() -> Catalog {
    let entries = lightcraft_engine::command_specs().iter().map(|c| Entry::new(c.id, c.label, c.params)).collect();
    Catalog { family: "light", engine: "lightcraft", engine_crate: "lightcraft-engine", package: env!("CARGO_PKG_NAME"), entries }
}

#[test]
fn the_skill_references_match_the_pinned_engine() {
    check_commands(&catalog(), &skill_dir());
}

fn number(v: &Value) -> String {
    match v.as_f64() {
        Some(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", n as i64),
        Some(n) => format!("{n}"),
        None => v.to_string(),
    }
}

/// `controls.md`: what `light.controls` answers, one control per line by
/// section, so the model greps a key instead of listing them all.
fn controls_md() -> String {
    let mut session = lightcraft_engine::Session::new();
    let list = session.execute("develop.controls", &json!({})).expect("the develop-control catalog");
    let mut controls: Vec<&Value> = list.as_array().expect("a list").iter().collect();
    controls.sort_by(|a, b| {
        let key = |c: &Value| (c["section"].to_string(), c["id"].as_str().unwrap_or("").to_owned());
        key(a).cmp(&key(b))
    });
    let lock = std::fs::read_to_string(workspace_lock()).expect("Cargo.lock");
    let revision = locked_revision(&lock, "lightcraft-engine").expect("lightcraft's locked revision");
    let mut out = String::new();
    let _ = writeln!(out, "# light engine develop controls");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "The lightcraft engine's develop controls at revision {}: {} controls, the keys of `params` for light.develop \
         and light.batch, one per line as `id` label: min to max, default. Generated from the engine by `{}`; do not edit.",
        revision.get(..12).unwrap_or(&revision),
        controls.len(),
        regen_command(env!("CARGO_PKG_NAME"))
    );
    let mut section = String::new();
    for c in controls {
        let s = c["section"].as_str().map(str::to_owned).unwrap_or_else(|| c["section"].to_string());
        if s != section {
            let _ = writeln!(out);
            let _ = writeln!(out, "## {s}");
            let _ = writeln!(out);
            section = s;
        }
        let label = c["label"].as_str().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ");
        let _ = writeln!(
            out,
            "- `{}` {label}: {} to {}, default {}",
            c["id"].as_str().unwrap_or(""),
            number(&c["min"]),
            number(&c["max"]),
            number(&c["default"])
        );
    }
    out
}

#[test]
fn the_develop_control_reference_matches_the_pinned_engine() {
    check_generated(env!("CARGO_PKG_NAME"), &skill_dir(), "controls.md", &controls_md());
}
