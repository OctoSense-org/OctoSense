//! The fixture corpus (ADR 0005 §2): real manifests and packages, each with
//! the digest, signing bytes, entry and resolved [`AppPolicy`] (or the
//! refusal) every `1.x` release must reproduce. See `fixtures/README.md`.
//!
//! `APP_CONTRACT_BLESS=1 cargo test -p octosense-app-contract --test
//! fixtures` fills in the keys a new fixture's `expected.json` lacks. It
//! never changes a key that is already there: the corpus is append-only.
use octosense_app_contract::*;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// Every key an `expected.json` holds once it is complete.
const ACCEPTED_KEYS: &[&str] = &["source", "limits", "digest", "ignored_fields", "signing_blake3", "entry", "policy"];
const REFUSED_KEYS: &[&str] = &["source", "limits", "digest", "refused"];

fn fixtures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("package").is_dir())
        .collect();
    dirs.sort();
    dirs
}

fn limits(name: &str) -> HostLimits {
    match name {
        "system" => HostLimits::system(),
        "default" => HostLimits::default(),
        "unsigned" => HostLimits::default().with_require_signature(false),
        other => panic!("unknown limits {other:?}: use system, default or unsigned"),
    }
}

/// What this build makes of a fixture: the facts `expected.json` pins.
fn observe(package: &Path, limits: &HostLimits) -> Map<String, Value> {
    let mut seen = Map::new();
    let digest = digest_dir(package).expect("a fixture package hashes");
    seen.insert("digest".into(), json!(digest));
    let text = std::fs::read_to_string(package.join(MANIFEST_FILE)).expect("a fixture package has a manifest");
    let outcome = parse(&text).and_then(|manifest| {
        admit_digest(&manifest, &digest, &RefuseAllSignatures)?;
        let policy = resolve(&manifest, limits)?;
        Ok((manifest, policy))
    });
    match outcome {
        Err(error) => {
            seen.insert("refused".into(), json!(error));
        }
        Ok((manifest, policy)) => {
            seen.insert("ignored_fields".into(), json!(manifest.ignored_fields()));
            let signing = manifest.signing_bytes().expect("an admitted manifest has signing bytes");
            seen.insert("signing_blake3".into(), json!(bundle_digest(&signing)));
            let entry = match script_source(package, "http://127.0.0.1:1/") {
                None => "card",
                Some(Ok(_)) => "script",
                Some(Err(e)) => panic!("{}: {e}", package.display()),
            };
            seen.insert("entry".into(), json!(entry));
            seen.insert("policy".into(), serde_json::to_value(&policy).unwrap());
        }
    }
    seen
}

#[test]
fn every_fixture_resolves_as_it_always_has() {
    let bless = std::env::var_os("APP_CONTRACT_BLESS").is_some();
    let fixtures = fixtures();
    assert!(fixtures.len() >= 20, "the corpus only grows: {} fixtures", fixtures.len());
    let mut failures = Vec::new();
    for dir in &fixtures {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let path = dir.join("expected.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: expected.json: {e}"));
        let mut expected: Map<String, Value> =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: expected.json: {e}"));
        let limits_name = expected.get("limits").and_then(Value::as_str).unwrap_or_else(|| panic!("{name}: no limits"));
        let seen = observe(&dir.join("package"), &limits(limits_name));

        if bless {
            let mut grew = false;
            for (key, value) in &seen {
                if !expected.contains_key(key) {
                    expected.insert(key.clone(), value.clone());
                    grew = true;
                }
            }
            if grew {
                std::fs::write(&path, serde_json::to_string_pretty(&expected).unwrap() + "\n").unwrap();
            }
        }

        let keys = if expected.contains_key("refused") { REFUSED_KEYS } else { ACCEPTED_KEYS };
        for key in keys {
            if !expected.contains_key(*key) {
                failures.push(format!("{name}: expected.json has no {key:?} (APP_CONTRACT_BLESS=1 fills in a new fixture)"));
            }
        }
        for (key, want) in &expected {
            match (key.as_str(), seen.get(key)) {
                ("source" | "limits", _) => {}
                // A refusal pins its reason; a later 1.x may say more, never
                // something else.
                ("refused", Some(Value::String(got))) => {
                    if !got.contains(want.as_str().unwrap_or_default()) {
                        failures.push(format!("{name}: refused with {got:?}, expected {want}"));
                    }
                }
                // A later 1.x may add policy fields; every pinned one holds.
                ("policy", Some(Value::Object(got))) => {
                    for (field, want) in want.as_object().into_iter().flatten() {
                        if got.get(field) != Some(want) {
                            failures.push(format!("{name}: policy.{field} is {:?}, expected {want}", got.get(field)));
                        }
                    }
                }
                (_, Some(got)) if got == want => {}
                (_, got) => failures.push(format!(
                    "{name}: {key} is {}, expected {want}",
                    got.map(Value::to_string).unwrap_or_else(|| "absent".into())
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{} fixture mismatches:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn a_shipped_digest_is_the_digest_of_the_package() {
    // Every accepted fixture's manifest names its package's digest, so the
    // corpus also pins `digest_dir` against manifests written by tools.
    for dir in fixtures() {
        let text = std::fs::read_to_string(dir.join("package").join(MANIFEST_FILE)).unwrap();
        let Ok(manifest) = parse(&text) else { continue };
        let digest = digest_dir(&dir.join("package")).unwrap();
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        if name == "refuse-digest-mismatch" {
            assert_ne!(manifest.integrity.bundle_blake3, digest);
        } else {
            assert_eq!(manifest.integrity.bundle_blake3, digest, "{name}");
        }
    }
}

#[test]
fn no_fixture_at_this_minor_has_ignored_fields() {
    // Older manifests (no schema_minor, no requires) read exactly as they
    // did: strictly, with nothing ignored.
    for dir in fixtures() {
        let text = std::fs::read_to_string(dir.join("package").join(MANIFEST_FILE)).unwrap();
        let Ok(manifest) = parse(&text) else { continue };
        if manifest.schema_minor > SCHEMA_MINOR {
            continue;
        }
        assert!(manifest.ignored_fields().is_empty(), "{}", dir.display());
        let strict: Result<AppManifest, _> = serde_json::from_str(&text);
        assert!(strict.is_ok(), "{} reads strictly", dir.display());
    }
}
