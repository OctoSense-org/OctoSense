//! Drift check: `registry::all()` against the octos sources it mirrors
//! (`octos-llm/src/registry/*.rs` + `model_catalog.json`) of an octos
//! checkout named by `OCTOS_SRC` (at the rev apps/appcard/app/Cargo.toml
//! pins). Skips (with a note) when it is not set.

use octosense_llm_config::registry;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn octos_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("OCTOS_SRC")?);
    dir.join("crates/octos-llm/src/registry").is_dir().then_some(dir)
}

/// The text after `field:` up to the next top-level `,` in the ENTRY block.
fn field<'a>(entry: &'a str, name: &str) -> &'a str {
    let key = format!("\n    {name}:");
    let start = entry.find(&key).unwrap_or_else(|| panic!("no `{name}` in entry")) + key.len();
    let rest = &entry[start..];
    let mut depth = 0i32;
    for (i, c) in rest.char_indices() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            ',' if depth == 0 => return rest[..i].trim(),
            _ => {}
        }
    }
    rest.trim()
}

fn strings(s: &str) -> Vec<String> {
    s.split('"').skip(1).step_by(2).map(str::to_string).collect()
}

/// `Some("x")` → Some(x); `Some(CONST)` → the const's value; `None` → None.
fn opt_str(s: &str, consts: &BTreeMap<String, String>) -> Option<String> {
    if s == "None" {
        return None;
    }
    let inner = s.strip_prefix("Some(").and_then(|r| r.strip_suffix(')')).expect("Some(..)");
    match strings(inner).into_iter().next() {
        Some(lit) => Some(lit),
        None => Some(consts.get(inner.trim()).unwrap_or_else(|| panic!("const {inner}")).clone()),
    }
}

fn str_consts(src: &str, out: &mut BTreeMap<String, String>) {
    for line in src.lines() {
        let l = line.trim_start_matches("pub ").trim();
        if let Some(rest) = l.strip_prefix("const ") {
            if let Some((name, value)) = rest.split_once(": &str =") {
                if let Some(v) = strings(value).into_iter().next() {
                    out.insert(name.trim().to_string(), v);
                }
            }
        }
    }
}

#[test]
fn registry_matches_octos_sources() {
    let Some(octos) = octos_dir() else {
        eprintln!("OCTOS_SRC not set — registry drift check skipped");
        return;
    };
    let reg = octos.join("crates/octos-llm/src/registry");
    let mut consts = BTreeMap::new();
    str_consts(&std::fs::read_to_string(octos.join("crates/octos-llm/src/local_discovery.rs")).unwrap(), &mut consts);

    let catalog: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(octos.join("model_catalog.json")).unwrap()).unwrap();
    let defaults: BTreeMap<String, String> = catalog["models"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["default"].as_bool() == Some(true))
        .filter_map(|m| {
            let (f, model) = m["provider"].as_str()?.split_once('/')?;
            Some((f.to_string(), model.to_string()))
        })
        .collect();

    let mut seen = Vec::new();
    for entry in std::fs::read_dir(&reg).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().unwrap() == "mod.rs" || path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        let mut local = consts.clone();
        str_consts(&src, &mut local);
        let Some(at) = src.find("ENTRY: ProviderEntry = ProviderEntry {") else { continue };
        let block = &src[at..at + src[at..].find("\n};").unwrap()];
        let id = strings(field(block, "name")).remove(0);
        let ours = registry::lookup(&id).unwrap_or_else(|| panic!("octos family `{id}` missing here ({})", rel(&path)));
        assert_eq!(ours.id, id);
        assert_eq!(ours.aliases, strings(field(block, "aliases")), "{id} aliases");
        assert_eq!(ours.key_env.map(str::to_string), opt_str(field(block, "api_key_env"), &local), "{id} key env");
        assert_eq!(ours.key_env_aliases, strings(field(block, "key_env_aliases")), "{id} key env aliases");
        assert_eq!(
            ours.default_base_url.map(str::to_string),
            opt_str(field(block, "default_base_url"), &local),
            "{id} base url"
        );
        assert_eq!(ours.key_required, field(block, "requires_api_key") == "true", "{id} key required");
        assert_eq!(ours.default_model.map(str::to_string), defaults.get(&id).cloned(), "{id} default model");
        seen.push(id);
    }
    assert_eq!(seen.len(), registry::all().len(), "families here that octos lacks: {seen:?}");
}

fn rel(p: &Path) -> String {
    p.file_name().unwrap().to_string_lossy().into_owned()
}
