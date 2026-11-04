//! Publisher key continuity: an update, or a new app from a publisher the
//! hub already knows, is verified against the key in the authenticated
//! catalog, never against a key the submission supplies.
mod common;
use common::*;
use octosense_app_hub::*;
use octosense_app_policy::{HostLimits, SignatureVerifier};
use std::{fs, path::Path, process::Command};

#[test]
fn replacement_key_same_id_is_refused() {
    let mut f = Fixture::new();
    let catalog = Catalog::new(1, "2026-09-25", vec![f.entry()]);
    f.manifest.version = "2.0.0".into();
    f.sign();
    assert!(f.report(Some(&catalog)).passed(), "a same-key update must pass");
    f.publisher = HubKey::generate();
    f.sign();
    let report = f.report(Some(&catalog));
    assert!(!report.passed(), "replacement key passed: {}", report.render());
    assert!(report.findings.iter().any(|f| f.check == "continuity"));
}

#[test]
fn entry_publisher_must_match_signature_owner() {
    let f = Fixture::new();
    let report = f.report(None);
    assert!(entry_for(&f.bundle, &report, "impostor", &f.publisher.public_hex(), "", "", "2026-09-25").is_err());
    assert!(entry_for(&f.bundle, &report, "publisher-one", &HubKey::generate().public_hex(), "", "", "2026-09-25").is_err());
}

#[test]
fn one_publisher_can_publish_two_apps_but_cannot_redefine_its_key() {
    let mut f = Fixture::new();
    let catalog = Catalog::new(1, "2026-09-25", vec![f.entry()]);
    f.manifest.id = "second-app".into();
    f.sign();
    assert!(f.report(Some(&catalog)).passed());
    f.publisher = HubKey::generate();
    f.sign();
    assert!(!f.report(Some(&catalog)).passed(), "a new app must not redefine an existing publisher's key");
}

#[test]
fn unknown_rotation_requires_registry_authorization() {
    let mut f = Fixture::new();
    let first = f.entry();
    f.manifest.version = "2.0.0".into();
    f.publisher = HubKey::generate();
    f.sign();
    let second = f.entry();
    let catalog = Catalog::new(2, "2026-09-25", vec![first, second]);
    f.manifest.version = "3.0.0".into();
    f.sign();
    let report = f.report(Some(&catalog));
    assert!(!report.passed(), "ambiguous history must require reconciliation");
    assert!(report.render().contains("conflicting"), "{}", report.render());
}

#[test]
fn duplicate_key_mappings_do_not_choose_the_first_key() {
    let f = Fixture::new();
    let keys = f.keys().with("publisher-one", &HubKey::generate().public_hex());
    let sig = f.manifest.integrity.signature.as_ref().unwrap();
    assert!(keys.verify(&sig.key_id, &sig.value, &f.manifest.signing_bytes().unwrap()).is_err());
    // Repetition of the same public bytes is harmless, including hex case.
    let same = f.keys().with("publisher-one", &f.publisher.public_hex().to_uppercase());
    same.verify(&sig.key_id, &sig.value, &f.manifest.signing_bytes().unwrap()).unwrap();
}

#[test]
fn unsigned_update_is_refused_even_in_local_check() {
    let mut f = Fixture::new();
    let catalog = Catalog::new(1, "2026-09-25", vec![f.entry()]);
    f.manifest.version = "2.0.0".into();
    f.manifest.integrity.signature = None;
    f.write_manifest();
    let limits = HostLimits::default().with_require_signature(false);
    assert!(!check_bundle(&f.bundle, &limits, &f.keys(), Some(&catalog)).unwrap().passed());
}

#[test]
fn an_unsigned_apps_first_signed_update_records_its_key() {
    // Unsigned first releases are still admitted (`--allow-unsigned`); the
    // first signed update is what puts a key on record for the app.
    let mut f = Fixture::new();
    let unsigned = f.unsigned_entry();
    let catalog = Catalog::new(1, "2026-09-25", vec![unsigned]);
    f.manifest.version = "2.0.0".into();
    f.sign();
    let report = f.report(Some(&catalog));
    assert!(report.passed(), "{}", report.render());
    let signed = entry_for(&f.bundle, &report, "publisher-one", &f.publisher.public_hex(), "", "", "2026-09-25").unwrap();
    // From then on the key is on record: a different one is refused.
    let catalog = Catalog::new(2, "2026-09-25", vec![f.unsigned_entry_at("1.0.0"), signed]);
    f.manifest.version = "3.0.0".into();
    f.publisher = HubKey::generate();
    f.sign();
    assert!(!f.report(Some(&catalog)).passed(), "the recorded key must hold after the first signed update");
}

#[test]
fn entry_cannot_reuse_a_gate_report_after_manifest_changes() {
    let mut f = Fixture::new();
    let report = f.report(None);
    f.manifest.name = "Changed after review".into();
    f.sign();
    assert!(entry_for(&f.bundle, &report, "publisher-one", &f.publisher.public_hex(), "", "", "2026-09-25").is_err());
}

#[test]
fn existing_signed_v1_catalog_still_verifies() {
    let catalog: Catalog = serde_json::from_str(include_str!("../../../catalog.json")).unwrap();
    verify_catalog(&catalog, "6000284a069ba7cada2925094074e8e0baae07e25d1b7fc31f396c993f363e11").unwrap();
}

#[test]
fn conflicting_catalog_owner_and_missing_recorded_key_require_reconciliation() {
    let mut f = Fixture::new();
    let entry = f.entry();
    f.manifest.version = "2.0.0".into();
    f.sign();
    for bad_entry in [
        // The signature names publisher-one, the entry another owner.
        Entry { publisher: "someone-else".into(), ..entry.clone() },
        // A signed entry with no key on record: nothing to verify against.
        Entry { publisher_key: String::new(), ..entry },
    ] {
        let catalog = Catalog::new(1, "2026-09-25", vec![bad_entry]);
        assert!(!f.report(Some(&catalog)).passed());
    }
}

#[test]
fn refused_report_or_changed_content_cannot_be_published() {
    let f = Fixture::new();
    let report = f.report(None);
    fs::write(f.bundle.join("payload.txt"), "changed after gate").unwrap();
    assert!(entry_for(&f.bundle, &report, "publisher-one", &f.publisher.public_hex(), "", "", "2026-09-25").is_err());
    let refused = f.report(None);
    assert!(!refused.passed());
    assert!(entry_for(&f.bundle, &refused, "publisher-one", &f.publisher.public_hex(), "", "", "2026-09-25").is_err());
}

/// `hub publish` with a throwaway working key, certified by `anchor`.
struct Hub {
    key_path: std::path::PathBuf,
    certificate: String,
    anchor: HubKey,
    catalog_path: std::path::PathBuf,
}

impl Hub {
    fn new(root: &Path) -> Self {
        let working = HubKey::generate();
        let anchor = HubKey::generate();
        let certificate = anchor.certify(&working.public_hex()).unwrap();
        let key_path = root.join("working.key");
        fs::write(&key_path, hex::encode(working.to_bytes())).unwrap();
        Hub { key_path, certificate, anchor, catalog_path: root.join("catalog.json") }
    }

    fn publish(&self, f: &Fixture, extra: &[&str]) -> std::process::Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hub"));
        command.args([
            "publish", f.bundle.to_str().unwrap(), "--publisher", "publisher-one",
            "--catalog", self.catalog_path.to_str().unwrap(), "--key", self.key_path.to_str().unwrap(),
            "--anchor-cert", &self.certificate, "--out", f.root.to_str().unwrap(),
        ]);
        command.args(extra);
        command.output().unwrap()
    }
}

#[test]
fn an_unsigned_first_release_still_publishes_with_allow_unsigned() {
    let mut f = Fixture::new();
    f.manifest.integrity.signature = None;
    f.write_manifest();
    let hub = Hub::new(&f.root);
    let out = hub.publish(&f, &["--allow-unsigned"]);
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let catalog: Catalog = serde_json::from_slice(&fs::read(&hub.catalog_path).unwrap()).unwrap();
    verify_catalog(&catalog, &hub.anchor.public_hex()).unwrap();
    assert_eq!(catalog.entries.len(), 1);
    assert_eq!(catalog.entries[0].publisher_key, "", "an unsigned release records no key");
}

#[test]
fn publish_authenticates_history_and_keeps_the_recorded_key() {
    let mut f = Fixture::new();
    let hub = Hub::new(&f.root);
    let anchor = hub.anchor.public_hex();
    let key_arg = format!("publisher-one={}", f.publisher.public_hex());
    let first = hub.publish(&f, &["--publisher-key", &key_arg]);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    let before = fs::read(&hub.catalog_path).unwrap();

    // History signed by another anchor than the one trusted is not history.
    f.manifest.version = "2.0.0".into();
    f.sign();
    let unauthenticated = hub.publish(&f, &["--publisher-key", &key_arg]);
    assert!(!unauthenticated.status.success());
    assert!(String::from_utf8_lossy(&unauthenticated.stderr).contains("authenticate"), "{}", String::from_utf8_lossy(&unauthenticated.stderr));
    assert_eq!(before, fs::read(&hub.catalog_path).unwrap());

    // A replacement key under the same id is refused, even when supplied.
    let original = f.publisher.to_bytes();
    f.publisher = HubKey::generate();
    f.sign();
    let substitute = format!("publisher-one={}", f.publisher.public_hex());
    let substitution = hub.publish(&f, &["--publisher-key", &substitute, "--anchor", &anchor]);
    assert!(!substitution.status.success());
    assert_eq!(before, fs::read(&hub.catalog_path).unwrap());

    // The recorded key publishes the update, without being supplied again.
    f.publisher = HubKey::from_bytes(&original);
    f.sign();
    let second = hub.publish(&f, &["--anchor", &anchor]);
    assert!(second.status.success(), "{}{}", String::from_utf8_lossy(&second.stdout), String::from_utf8_lossy(&second.stderr));
    let mut catalog: Catalog = serde_json::from_slice(&fs::read(&hub.catalog_path).unwrap()).unwrap();
    verify_catalog(&catalog, &anchor).unwrap();
    assert_eq!(catalog.entries.len(), 2);
    assert_eq!(catalog.entries[1].publisher_key, f.publisher.public_hex(), "the update records the trusted key");

    // Tampered history no longer verifies, so nothing is published over it.
    catalog.entries[0].publisher_key = HubKey::generate().public_hex();
    write_catalog(&hub.catalog_path, &catalog);
    f.manifest.version = "3.0.0".into();
    f.sign();
    let tampered = hub.publish(&f, &["--anchor", &anchor]);
    assert!(!tampered.status.success());
}
