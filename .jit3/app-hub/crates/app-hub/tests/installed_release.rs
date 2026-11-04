//! An installed release runs as the release it is: the exact version the
//! catalog admitted, with its own permissions, from bytes that still hash to
//! what was reviewed. A newer version on offer is a separate fact.
mod common;
use common::*;
use octosense_app_hub::*;
use octosense_app_policy::HostLimits;
use std::fs;

struct Installed {
    fixture: Fixture,
    store: Store,
    catalog: Catalog,
    working: HubKey,
    certificate: String,
}

impl Installed {
    fn new() -> Self {
        let fixture = Fixture::new();
        let anchor = HubKey::generate();
        let working = HubKey::generate();
        let certificate = anchor.certify(&working.public_hex()).unwrap();
        let catalog = Catalog::new(1, "2026-09-25", vec![fixture.entry()]);
        let store = Store::new(&anchor.public_hex(), &fixture.root.join("installed"), HostLimits::default());
        let mut installed = Self { fixture, store, catalog, working, certificate };
        installed.accept();
        installed.store.install_staged("example-app", &installed.fixture.bundle, &installed.fixture.keys(), "2026-09-25").unwrap();
        installed
    }
    fn accept(&mut self) {
        self.catalog.sequence += 1;
        self.working.sign_catalog(&mut self.catalog, &self.certificate).unwrap();
        self.store.accept_catalog(&serde_json::to_string(&self.catalog).unwrap()).unwrap();
    }
    fn offer_update(&mut self) {
        self.fixture.manifest.version = "2.0.0".into();
        self.fixture.manifest.capabilities.push("prompt".into());
        self.fixture.sign();
        self.catalog.entries.push(self.fixture.entry());
        self.accept();
    }
}

#[test]
fn offered_v1_runs_after_v2_publish_with_v1_permissions() {
    let mut i = Installed::new();
    i.offer_update();
    let policy = i.store.may_run("example-app").expect("an offered update must not disable installed v1");
    assert_eq!(policy.version, "1.0.0");
    assert!(!policy.may_prompt, "v2's extra permission is not granted to v1");
}

#[test]
fn withdrawal_targets_the_installed_release_only() {
    let mut i = Installed::new();
    i.offer_update();
    i.catalog.entries[1].status = Status::Withdrawn("bad update".into());
    i.accept();
    assert!(i.store.may_run("example-app").is_ok(), "withdrawn v2 must not block v1");
    i.catalog.entries[0].status = Status::Withdrawn("unsafe v1".into());
    i.catalog.entries[1].status = Status::Offered;
    i.accept();
    let err = i.store.may_run("example-app").unwrap_err();
    assert!(err.contains("unsafe v1"), "{err}");
}

#[test]
fn launch_rejects_modified_installed_bytes() {
    let i = Installed::new();
    fs::write(i.store.install_dir("example-app").join("payload.txt"), "changed").unwrap();
    assert!(i.store.may_run("example-app").unwrap_err().contains("digest"));
}

#[test]
fn launch_rejects_modified_manifest_even_when_payload_digest_matches() {
    let i = Installed::new();
    let mut manifest = i.fixture.manifest.clone();
    manifest.capabilities.push("prompt".into());
    fs::write(i.store.install_dir("example-app").join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(i.store.may_run("example-app").unwrap_err().contains("manifest"));
}

#[test]
fn install_rejects_a_different_manifest_with_the_same_id_version_and_digest() {
    let mut i = Installed::new();
    i.fixture.manifest.capabilities.push("prompt".into());
    i.fixture.sign();
    let err = i.store.install_staged("example-app", &i.fixture.bundle, &i.fixture.keys(), "2026-09-25").unwrap_err();
    assert!(err.contains("manifest"), "{err}");
    assert!(i.store.may_run("example-app").is_ok(), "a refused install preserves the previous release");
}

#[test]
fn offline_approved_version_remains_openable_but_missing_release_does_not() {
    let mut i = Installed::new();
    i.offer_update();
    assert!(i.store.installs_allowed("2026-10-25").is_err());
    assert!(i.store.may_run("example-app").is_ok());
    i.catalog.entries.remove(0);
    i.accept();
    let err = i.store.may_run("example-app").unwrap_err();
    assert!(err.contains("1.0.0") && err.contains("catalog"), "{err}");
}

#[test]
fn launch_checks_the_catalog_publisher_signature() {
    let mut i = Installed::new();
    let prepared = i.store.prepare_launch("example-app").unwrap();
    i.catalog.entries[0].publisher_key = HubKey::generate().public_hex();
    i.accept();
    assert!(i.store.may_run("example-app").is_err(), "catalog metadata cannot substitute the app's signing key");
    assert!(i.store.validate_prepared_launch(&prepared).is_err(), "the pending launch must enforce the same signature decision");
}

#[test]
fn install_verifies_against_the_catalog_key_even_if_the_caller_supplies_another() {
    let mut i = Installed::new();
    i.catalog.entries[0].publisher_key = HubKey::generate().public_hex();
    i.accept();
    assert!(i.store.install_staged("example-app", &i.fixture.bundle, &i.fixture.keys(), "2026-09-25").is_err());
}

#[test]
fn lifecycle_keeps_open_and_update_independent_even_after_withdrawal() {
    let mut i = Installed::new();
    i.offer_update();
    let lifecycle = i.store.listings().remove(0).lifecycle;
    assert!(lifecycle.can_open);
    assert_eq!(lifecycle.installed_version.as_deref(), Some("1.0.0"));
    assert_eq!(lifecycle.update_version.as_deref(), Some("2.0.0"));
    i.catalog.entries[0].status = Status::Withdrawn("old release problem".into());
    i.accept();
    let lifecycle = i.store.listings().remove(0).lifecycle;
    assert!(!lifecycle.can_open);
    assert_eq!(lifecycle.update_version.as_deref(), Some("2.0.0"));
    assert!(lifecycle.unavailable_reason.unwrap().contains("old release problem"));
}

#[test]
fn prepared_launch_retains_its_verified_bytes_when_an_update_replaces_the_install() {
    let mut i = Installed::new();
    let prepared = i.store.prepare_launch("example-app").unwrap();
    let expected = fs::read_to_string(prepared.bundle().join("manifest.json")).unwrap();
    i.offer_update();
    i.store.install_staged("example-app", &i.fixture.bundle, &i.fixture.keys(), "2026-09-25").unwrap();
    assert_eq!(fs::read_to_string(prepared.bundle().join("manifest.json")).unwrap(), expected);
    assert_eq!(prepared.policy.version, "1.0.0");
    let snapshot = prepared.bundle().to_path_buf();
    drop(prepared);
    assert!(!snapshot.exists(), "temporary launch bytes are cleaned up when the instance closes");
    assert!(i.store.install_dir("example-app").exists());
}

#[test]
fn withdrawal_between_preparation_and_start_is_refused() {
    let mut i = Installed::new();
    let prepared = i.store.prepare_launch("example-app").unwrap();
    i.offer_update();
    i.store.validate_prepared_launch(&prepared).unwrap();
    i.catalog.entries[0].status = Status::Withdrawn("recalled during startup".into());
    i.accept();
    assert!(i.store.validate_prepared_launch(&prepared).unwrap_err().contains("recalled during startup"));
}

#[test]
fn launch_validation_is_bounded_and_failed_preparation_cleans_up() {
    let i = Installed::new();
    let file = fs::File::create(i.store.install_dir("example-app").join("oversized.txt")).unwrap();
    file.set_len(octosense_app_hub::gate::MAX_BUNDLE_BYTES + 128 * 1024).unwrap();
    assert!(i.store.may_run("example-app").unwrap_err().contains("size limit"));
    assert!(i.store.prepare_launch("example-app").err().unwrap().contains("size limit"));
    assert_eq!(fs::read_dir(i.fixture.root.join("installed/.running")).unwrap().count(), 0);
}

#[test]
fn ambiguous_release_history_cannot_approve_a_launch() {
    let mut i = Installed::new();
    i.catalog.entries.push(i.catalog.entries[0].clone());
    i.accept();
    assert!(i.store.may_run("example-app").unwrap_err().contains("ambiguous"));
}
