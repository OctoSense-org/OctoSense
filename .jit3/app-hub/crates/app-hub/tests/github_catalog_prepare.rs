mod common;
use common::Fixture;
use octosense_app_hub::{github_catalog, Catalog, Status};
use std::fs;

fn candidate(f: &mut Fixture) -> Catalog {
    let mut entry = f.entry();
    entry.source.repository = "https://github.com/example/example-app".into();
    entry.source.commit = "1".repeat(40);
    entry.admitted = octosense_app_hub::today();
    let path = f.root.join(&entry.artifact);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::rename(&f.bundle, &path).unwrap();
    f.bundle = path;
    fs::write(
        f.root.join(format!("{}.pack.json", entry.artifact)),
        serde_json::to_vec(&octosense_app_hub::pack_dir(&f.bundle).unwrap()).unwrap(),
    )
    .unwrap();
    fs::create_dir(f.root.join("index")).unwrap();
    fs::write(
        f.root
            .join(format!("index/{}-{}.json", entry.app_id(), entry.version())),
        serde_json::to_vec(&entry).unwrap(),
    )
    .unwrap();
    Catalog::new(1, &octosense_app_hub::today(), vec![entry])
}
fn prepare(f: &Fixture, base: &Catalog, c: &Catalog) -> Result<Vec<u8>, String> {
    github_catalog::prepare(base, &serde_json::to_vec(c).unwrap(), &f.root)
}

#[test]
fn actual_signed_bundle_pack_and_index_are_admitted_without_execution() {
    let mut f = Fixture::new();
    let c = candidate(&mut f);
    let base = Catalog::new(0, &octosense_app_hub::today(), vec![]);
    let bytes = prepare(&f, &base, &c).unwrap();
    assert_eq!(bytes, prepare(&f, &base, &c).unwrap());
    assert_eq!(
        serde_json::from_slice::<Catalog>(&bytes).unwrap().sequence,
        1
    );
    fs::write(f.bundle.join("main.splash"), "Label{text: \"unreviewed\"}").unwrap();
    assert!(prepare(&f, &base, &c).is_err());
}

#[test]
fn pack_index_source_and_signer_substitution_are_refused() {
    let mut f = Fixture::new();
    let c = candidate(&mut f);
    let base = Catalog::new(0, &octosense_app_hub::today(), vec![]);
    for field in ["commit", "repository", "publisher_key", "artifact"] {
        let mut changed = c.clone();
        let entry = &mut changed.entries[0];
        match field {
            "commit" => entry.source.commit = "main".into(),
            "repository" => entry.source.repository = "https://attacker.example/repo".into(),
            "publisher_key" => entry.publisher_key = "0".repeat(64),
            _ => entry.artifact = "../escape".into(),
        }
        assert!(prepare(&f, &base, &changed).is_err(), "{field}");
    }
    for relative in [
        format!("{}.pack.json", c.entries[0].artifact),
        "index/example-app-1.0.0.json".into(),
    ] {
        let path = f.root.join(relative);
        let original = fs::read(&path).unwrap();
        fs::write(&path, "{}").unwrap();
        assert!(prepare(&f, &base, &c).is_err());
        fs::write(path, original).unwrap();
    }
}

#[test]
fn only_monotonic_withdrawal_can_change_existing_history() {
    let mut f = Fixture::new();
    let base = candidate(&mut f);
    let mut next = base.clone();
    next.sequence = 2;
    next.entries[0].status = Status::Withdrawn("Publisher withdrew this version".into());
    assert!(prepare(&f, &base, &next).is_ok());
    let mut modified = next.clone();
    modified.entries[0].source.commit = "2".repeat(40);
    assert!(prepare(&f, &base, &modified).is_err());
    let mut deleted = next.clone();
    deleted.entries.clear();
    assert!(prepare(&f, &base, &deleted).is_err());
    let mut rollback = next.clone();
    rollback.sequence = 1;
    assert!(prepare(&f, &base, &rollback).is_err());
    let mut revived = next.clone();
    revived.sequence = 3;
    revived.entries[0].status = Status::Offered;
    assert!(prepare(&f, &next, &revived).is_err());
}

#[test]
fn cli_authenticates_current_legacy_catalog_and_emits_exact_review_receipt() {
    use sha2::{Digest, Sha256};
    let f = Fixture::new();
    let original = include_bytes!("../../../catalog.json");
    let mut next = github_catalog::authenticated_base(original).unwrap();
    let prior = next.sequence;
    next.sequence += 1;
    next.published = octosense_app_hub::today();
    next.key = None;
    next.signature = None;
    next.entries
        .iter_mut()
        .find(|e| e.status.is_offered())
        .unwrap()
        .status = Status::Withdrawn("Synthetic candidate test; never published".into());
    let candidate = serde_json::to_vec(&next).unwrap();
    fs::write(f.root.join("base.json"), original).unwrap();
    fs::write(f.root.join("candidate.json"), &candidate).unwrap();
    let output_path = f.root.join(github_catalog::CATALOG_SUBJECT);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_hub"))
        .args(["catalog-prepare", "--base"])
        .arg(f.root.join("base.json"))
        .arg("--candidate")
        .arg(f.root.join("candidate.json"))
        .arg("--artifact-root")
        .arg(&f.root)
        .arg("--out")
        .arg(&output_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["base_sequence"], prior);
    assert_eq!(
        receipt["base_sha256"],
        hex::encode(Sha256::digest(original))
    );
    assert_eq!(
        receipt["candidate_sha256"],
        hex::encode(Sha256::digest(&candidate))
    );
    assert_eq!(
        receipt["payload_sha256"],
        hex::encode(Sha256::digest(fs::read(&output_path).unwrap()))
    );
    assert_eq!(receipt["subject"], github_catalog::CATALOG_SUBJECT);
    assert_eq!(receipt["added_artifacts"], serde_json::json!([]));
    let proof = f.root.join("proof.json");
    fs::write(&proof, "{}").unwrap();
    let envelope = f.root.join("catalog-v2.json");
    let refused = std::process::Command::new(env!("CARGO_BIN_EXE_hub"))
        .args(["catalog-envelope", "--catalog"])
        .arg(output_path)
        .arg("--attestation")
        .arg(proof)
        .arg("--out")
        .arg(&envelope)
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(!envelope.exists());
}

// ---- shared components (App Hub ADR 0003) -----------------------------------

use common::{component_entry, dependency, NOTES};
use octosense_app_hub::ComponentEntry;

fn base_with(components: Vec<ComponentEntry>) -> Catalog {
    let mut base = Catalog::new(0, "2026-10-08", vec![]);
    base.components = components;
    base
}

fn next_of(base: &Catalog) -> Catalog {
    let mut next = base.clone();
    next.sequence += 1;
    next.published = octosense_app_hub::today();
    next
}

#[test]
fn component_history_may_only_be_withdrawn() {
    let f = Fixture::new();
    let base = base_with(vec![component_entry("org.example.markdown", "1.0.0", NOTES)]);
    let mut withdrawn = next_of(&base);
    withdrawn.components[0].status = Status::Withdrawn("Renders scripts it should escape".into());
    let bytes = prepare(&f, &base, &withdrawn).unwrap();
    let prepared: Catalog = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(prepared.components[0].status, withdrawn.components[0].status);
    assert_eq!(github_catalog::added_artifacts(&base, &prepared), Vec::<serde_json::Value>::new());

    let mut changed = withdrawn.clone();
    changed.components[0].admitted = "2026-10-01".into();
    assert_eq!(
        prepare(&f, &base, &changed).unwrap_err(),
        "candidate changes existing component history or re-offers a withdrawn version"
    );
    let mut removed = withdrawn.clone();
    removed.components.clear();
    assert_eq!(prepare(&f, &base, &removed).unwrap_err(), "candidate reorders or drops component history; withdraw instead");
    let mut revived = next_of(&withdrawn);
    revived.components[0].status = Status::Offered;
    assert!(prepare(&f, &withdrawn, &revived).is_err());
    let mut empty_reason = next_of(&base);
    empty_reason.components[0].status = Status::Withdrawn(" ".into());
    assert!(prepare(&f, &base, &empty_reason).is_err());
}

#[test]
fn a_new_component_needs_github_provenance_and_its_reviewed_files() {
    let f = Fixture::new();
    let base = base_with(vec![]);
    let mut entry = component_entry("org.example.markdown", "1.0.0", NOTES);
    let mut next = next_of(&base);
    next.components.push(entry.clone());
    // Its source first, as for an app.
    assert_eq!(prepare(&f, &base, &next).unwrap_err(), "new component source must name an immutable commit");
    entry.source.repository = "https://github.com/example/markdown".into();
    entry.source.commit = "1".repeat(40);
    next.components[0] = entry.clone();
    assert_eq!(prepare(&f, &base, &next).unwrap_err(), "candidate component artifact is missing");
    fs::create_dir_all(f.root.join("artifacts")).unwrap();
    fs::write(f.root.join(&entry.artifact), NOTES).unwrap();
    // Then the gate: App Hub publishes only GitHub-attested components.
    let err = prepare(&f, &base, &next).unwrap_err();
    assert!(err.starts_with("candidate component was refused by the gate:"), "{err}");
    assert!(err.contains("[refused] publisher-signature: App Hub accepts only GitHub-attested component releases"), "{err}");
    // An app's id is never a component's.
    let mut clash = next_of(&base);
    let app = f.entry();
    clash.entries.push(app.clone());
    let mut same_id = entry.clone();
    same_id.component.id = app.manifest.id.clone();
    same_id.artifact = format!("artifacts/{}-1.0.0.wasm", app.manifest.id);
    clash.components.push(same_id);
    assert_eq!(prepare(&f, &base, &clash).unwrap_err(), "example-app is both an app and a component in the catalog");
}

#[test]
fn a_new_app_may_pin_a_component_its_catalog_offers() {
    let mut f = Fixture::new();
    let markdown = component_entry("org.example.markdown", "1.0.0", NOTES);
    f.pin(&[dependency("markdown", &markdown)], &["storage"], &[]);
    let base = base_with(vec![markdown.clone()]);
    let mut c = candidate(&mut f);
    c.components = base.components.clone();
    let bytes = prepare(&f, &base, &c).unwrap();
    let prepared: Catalog = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(prepared.components, std::slice::from_ref(&markdown));
    assert_eq!(prepared.entries[0].manifest.components, [dependency("markdown", &markdown)]);

    // The same app against a base without the component, or with it
    // withdrawn, is refused by the gate.
    let err = prepare(&f, &base_with(vec![]), &Catalog { components: vec![], ..c.clone() }).unwrap_err();
    assert!(err.contains("component markdown (org.example.markdown 1.0.0) is not in the catalog"), "{err}");
    let mut withdrawn = markdown.clone();
    withdrawn.status = Status::Withdrawn("Renders scripts it should escape".into());
    let base = base_with(vec![withdrawn.clone()]);
    let err = prepare(&f, &base, &Catalog { components: vec![withdrawn], ..c.clone() }).unwrap_err();
    assert!(err.contains("was withdrawn: Renders scripts it should escape"), "{err}");
}
