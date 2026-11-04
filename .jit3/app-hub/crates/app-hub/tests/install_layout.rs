//! An installed bundle lives outside its app's storage: the app writes in
//! `<root>/<id>/`, its reviewed bytes sit in `<root>/.bundles/<id>/bundle`,
//! where nothing the app can reach is. Installs made before this layout move
//! out once, and the app's data stays where it was.
mod common;
use common::*;
use octosense_app_hub::*;
use octosense_app_policy::HostLimits;
use std::fs;
use std::path::Path;

struct Device {
    fixture: Fixture,
    store: Store,
    root: std::path::PathBuf,
}

impl Device {
    fn new() -> Self {
        let fixture = Fixture::new();
        let anchor = HubKey::generate();
        let working = HubKey::generate();
        let certificate = anchor.certify(&working.public_hex()).unwrap();
        let mut catalog = Catalog::new(1, "2026-09-25", vec![fixture.entry()]);
        working.sign_catalog(&mut catalog, &certificate).unwrap();
        let root = fixture.root.join("apps");
        let mut store = Store::new(&anchor.public_hex(), &root, HostLimits::default());
        store.accept_catalog(&serde_json::to_string(&catalog).unwrap()).unwrap();
        Device { fixture, store, root }
    }

    fn install(&self) {
        self.store.install_staged("example-app", &self.fixture.bundle, &self.fixture.keys(), "2026-09-25").unwrap();
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn an_installed_bundle_is_outside_the_apps_storage() {
    let d = Device::new();
    d.install();
    let policy = d.store.may_run("example-app").unwrap();
    let jail = policy.jail_root(&d.root);
    let bundle = d.store.install_dir("example-app");
    assert_eq!(bundle, installed_bundle_dir(&d.root, "example-app"));
    assert!(!bundle.starts_with(&jail), "{} is inside the app's storage {}", bundle.display(), jail.display());
    assert!(bundle.join("manifest.json").is_file());
}

#[test]
fn a_legacy_install_moves_out_once_and_keeps_its_data() {
    let d = Device::new();
    let legacy = d.root.join("example-app");
    copy_dir(&d.fixture.bundle, &legacy.join("bundle"));
    fs::create_dir_all(legacy.join("data")).unwrap();
    fs::write(legacy.join("data/notes"), "keep my notes").unwrap();

    adopt_legacy_installs(&d.root).unwrap();
    assert!(!legacy.join("bundle").exists(), "the old copy left the app's storage");
    assert!(d.store.install_dir("example-app").join("manifest.json").is_file());
    assert_eq!(fs::read_to_string(legacy.join("data/notes")).unwrap(), "keep my notes");
    assert!(d.store.may_run("example-app").is_ok(), "the moved install still opens");
    // Again: nothing left to move, nothing changes.
    adopt_legacy_installs(&d.root).unwrap();
    assert!(d.store.may_run("example-app").is_ok());
}

#[test]
fn a_legacy_interrupted_update_is_settled_as_it_moves() {
    // Interrupted between the two renames: only the previous bundle is there.
    let d = Device::new();
    let legacy = d.root.join("example-app");
    copy_dir(&d.fixture.bundle, &legacy.join(".bundle-previous"));
    fs::create_dir_all(legacy.join(".bundle-next")).unwrap();
    fs::write(legacy.join(".bundle-next/partial.txt"), "half a download").unwrap();
    adopt_legacy_installs(&d.root).unwrap();
    assert!(d.store.may_run("example-app").is_ok(), "the last complete bundle is restored");
    assert!(!legacy.join(".bundle-previous").exists() && !legacy.join(".bundle-next").exists());
}

#[test]
fn a_moved_install_does_not_replace_a_newer_one() {
    // Both layouts present (an old copy left behind): the current one wins.
    let d = Device::new();
    d.install();
    let legacy = d.root.join("example-app/bundle");
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join("manifest.json"), "{}").unwrap();
    adopt_legacy_installs(&d.root).unwrap();
    assert!(d.store.may_run("example-app").is_ok());
    assert!(!legacy.exists(), "the stale copy does not stay in the app's storage");
}

#[test]
fn removing_an_app_removes_its_bundle_and_its_data() {
    let d = Device::new();
    d.install();
    fs::create_dir_all(d.root.join("example-app/data")).unwrap();
    d.store.remove("example-app").unwrap();
    assert!(!d.root.join("example-app").exists());
    assert!(!installed_bundle_dir(&d.root, "example-app").exists());
    assert!(!install_root(&d.root, "example-app").exists());
}
