//! The device's shared component store (App Hub ADR 0003): an install keeps
//! each component an app pins once per digest, read-only, in
//! `<apps root>/.components`; every launch and every load verifies it; a
//! withdrawal stops the apps that pin it; an uninstall collects what no
//! installed app pins.
mod common;
use common::*;
use octosense_app_hub::*;
use octosense_app_policy::HostLimits;
use std::fs;
use std::path::{Path, PathBuf};

struct Device {
    hub: Hub,
    root: PathBuf,
    store: Store,
}

impl Device {
    fn new(f: &Fixture) -> Self {
        let hub = Hub::new();
        let root = f.root.join("device");
        let store = Store::new(&hub.anchor.public_hex(), &root, HostLimits::default());
        Device { hub, root, store }
    }

    fn offer(&mut self, sequence: u64, apps: Vec<Entry>, components: Vec<ComponentEntry>) {
        self.store.accept_catalog(&self.hub.json(sequence, apps, components)).unwrap();
    }

    fn install(&self, f: &Fixture, entry: &Entry) -> Result<(), String> {
        self.store.install_components(&entry.manifest, &mut |c| fetch(c))?;
        self.store.install_staged(&entry.manifest.id, &f.bundle, &self.store.publisher_keys(), &today()).map(|_| ())
    }
}

/// What the Hub serves at a component's artifact path.
fn fetch(entry: &ComponentEntry) -> Result<Vec<u8>, String> {
    Ok(match entry.id() {
        "org.example.markdown" => NOTES,
        "org.example.hostcall" => HOSTCALL,
        "org.example.fetch" => FETCH,
        other => return Err(format!("{other} is not served")),
    }
    .to_vec())
}

fn stored(root: &Path, wasm: &[u8]) -> PathBuf {
    root.join(".components").join(format!("{}.wasm", blake3_hex(wasm)))
}

fn stored_files(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.join(".components"))
        .map(|dir| dir.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// An app pinning `components`, admitted against them. Every app here is
/// signed by the same publisher key.
fn app(id: &str, components: &[&ComponentEntry], capabilities: &[&str], hub: &Hub) -> (Fixture, Entry) {
    let mut f = Fixture::new();
    f.publisher = HubKey::from_bytes(&[7; 32]);
    f.manifest.id = id.into();
    let pins: Vec<_> = components.iter().enumerate().map(|(i, c)| dependency(&format!("c{i}"), c)).collect();
    f.pin(&pins, capabilities, &[]);
    let catalog = hub.catalog(1, vec![], components.iter().map(|c| (*c).clone()).collect());
    let entry = f.entry_with(Some(&catalog));
    (f, entry)
}

#[test]
fn an_install_keeps_each_component_once_read_only_and_verified() {
    let markdown = component_entry("org.example.markdown", "1.0.0", NOTES);
    let probe = Fixture::new();
    let mut device = Device::new(&probe);
    let (f, entry) = app("example-app", &[&markdown], &["storage"], &device.hub);
    device.offer(1, vec![entry.clone()], vec![markdown.clone()]);
    let store = &device.store;

    // The app never lands without its components.
    let err = store.install_staged("example-app", &f.bundle, &store.publisher_keys(), &today()).unwrap_err();
    assert_eq!(err, "component c0 (org.example.markdown 1.0.0) is not in this device's component store; update or reinstall example-app");

    // A download must be the catalog's file: its size, then its digest.
    let err = store.install_components(&entry.manifest, &mut |_| Ok(FETCH.to_vec())).unwrap_err();
    assert_eq!(err, "component org.example.markdown 1.0.0: the download is 84774 bytes, the catalog says 310601");
    let mut tampered = NOTES.to_vec();
    tampered[1000] ^= 1;
    let err = store.install_components(&entry.manifest, &mut |_| Ok(tampered.clone())).unwrap_err();
    assert!(err.starts_with("component org.example.markdown 1.0.0: the downloaded component hashes to"), "{err}");
    assert!(stored_files(&device.root).is_empty(), "nothing unverified is kept: {:?}", stored_files(&device.root));

    // The file is fetched from its artifact path, kept read-only, once.
    let mut fetched = Vec::new();
    let resolved = store
        .install_components(&entry.manifest, &mut |c| {
            fetched.push(c.artifact.clone());
            fetch(c)
        })
        .unwrap();
    assert_eq!(fetched, ["artifacts/org.example.markdown-1.0.0.wasm"]);
    let path = stored(&device.root, NOTES);
    assert_eq!(store.components_dir(), device.root.join(".components"));
    assert_eq!(
        resolved,
        [Resolved {
            alias: "c0".into(),
            id: "org.example.markdown".into(),
            version: "1.0.0".into(),
            blake3: blake3_hex(NOTES),
            path: path.clone()
        }]
    );
    assert_eq!(fs::read(&path).unwrap(), NOTES);
    assert!(fs::metadata(&path).unwrap().permissions().readonly());
    store.install_components(&entry.manifest, &mut |_| panic!("a kept component is not fetched again")).unwrap();

    store.install_staged("example-app", &f.bundle, &store.publisher_keys(), &today()).unwrap();
    store.may_run("example-app").unwrap();
    assert_eq!(store.resolved_components("example-app").unwrap(), resolved);
    let launch = store.prepare_launch("example-app").unwrap();
    assert_eq!(launch.manifest.components.len(), 1);
}

#[test]
fn a_changed_missing_or_withdrawn_component_stops_the_app() {
    let markdown = component_entry("org.example.markdown", "1.0.0", NOTES);
    let probe = Fixture::new();
    let mut device = Device::new(&probe);
    let (f, entry) = app("example-app", &[&markdown], &["storage"], &device.hub);
    device.offer(1, vec![entry.clone()], vec![markdown.clone()]);
    device.install(&f, &entry).unwrap();
    let path = stored(&device.root, NOTES);

    // Changed bytes: refused at launch and at load.
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).unwrap();
    fs::write(&path, HOSTCALL).unwrap();
    let err = device.store.may_run("example-app").unwrap_err();
    assert!(err.starts_with("component c0 (org.example.markdown 1.0.0): ") && err.contains(&format!("hashes to {}", blake3_hex(HOSTCALL))), "{err}");
    assert!(err.ends_with("; reinstall example-app"), "{err}");
    assert_eq!(device.store.resolved_components("example-app").unwrap_err(), err);
    assert!(!device.store.app_availability("example-app").can_open);

    // Missing: the same, until the store keeps it again.
    fs::remove_file(&path).unwrap();
    let err = device.store.may_run("example-app").unwrap_err();
    assert_eq!(err, "component c0 (org.example.markdown 1.0.0) is not in this device's component store; update or reinstall example-app");
    device.store.install_components(&entry.manifest, &mut |c| fetch(c)).unwrap();
    device.store.may_run("example-app").unwrap();

    // Withdrawn: the app that pins it stops at its next catalog, and no new
    // install takes it.
    let mut withdrawn = markdown.clone();
    withdrawn.status = Status::Withdrawn("Renders scripts it should escape".into());
    device.offer(2, vec![entry.clone()], vec![withdrawn]);
    let err = device.store.may_run("example-app").unwrap_err();
    assert_eq!(err, "component c0 (org.example.markdown 1.0.0) was withdrawn: Renders scripts it should escape");
    assert_eq!(device.store.resolved_components("example-app").unwrap_err(), err);
    let availability = device.store.app_availability("example-app");
    assert_eq!((availability.can_open, availability.unavailable_reason.as_deref()), (false, Some(err.as_str())));
    assert_eq!(device.store.install_components(&entry.manifest, &mut |c| fetch(c)).unwrap_err(), err);
    assert_eq!(device.store.resolve_components(&entry.manifest).unwrap_err(), err);
}

#[test]
fn an_uninstall_collects_what_no_installed_app_pins() {
    let markdown = component_entry("org.example.markdown", "1.0.0", NOTES);
    let hostcall = component_entry("org.example.hostcall", "1.0.0", HOSTCALL);
    let fetch_component = component_entry("org.example.fetch", "1.0.0", FETCH);
    let probe = Fixture::new();
    let mut device = Device::new(&probe);
    let (one, first) = app("example-one", &[&markdown], &["storage"], &device.hub);
    let (two, second) = app("example-two", &[&markdown, &hostcall], &["storage"], &device.hub);
    device.offer(1, vec![first.clone(), second.clone()], vec![markdown.clone(), hostcall.clone(), fetch_component.clone()]);
    device.install(&one, &first).unwrap();
    device.install(&two, &second).unwrap();
    // Two apps pin the same component: one file.
    let both = {
        let mut names = vec![format!("{}.wasm", blake3_hex(NOTES)), format!("{}.wasm", blake3_hex(HOSTCALL))];
        names.sort();
        names
    };
    assert_eq!(stored_files(&device.root), both);

    // A component no app pins (an update's old version, say) is collected.
    components::keep(&device.root, &blake3_hex(FETCH), FETCH).unwrap();
    assert_eq!(device.store.collect_components().unwrap(), [blake3_hex(FETCH)]);
    assert_eq!(stored_files(&device.root), both);

    device.store.remove("example-one").unwrap();
    assert_eq!(stored_files(&device.root), both, "example-two still pins both");
    device.store.remove("example-two").unwrap();
    assert!(stored_files(&device.root).is_empty(), "{:?}", stored_files(&device.root));
    assert!(device.store.collect_components().unwrap().is_empty());
}

#[test]
fn a_collection_never_removes_what_an_unreadable_app_might_pin() {
    let markdown = component_entry("org.example.markdown", "1.0.0", NOTES);
    let probe = Fixture::new();
    let mut device = Device::new(&probe);
    let (f, entry) = app("example-app", &[&markdown], &["storage"], &device.hub);
    device.offer(1, vec![entry.clone()], vec![markdown]);
    device.install(&f, &entry).unwrap();
    components::keep(&device.root, &blake3_hex(FETCH), FETCH).unwrap();
    let broken = installed_bundle_dir(&device.root, "example-broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("manifest.json"), "{not json").unwrap();
    assert!(device.store.collect_components().is_err());
    assert_eq!(stored_files(&device.root).len(), 2);
}

#[test]
fn a_staged_install_keeps_components_in_the_hosts_store() {
    let markdown = component_entry("org.example.markdown", "1.0.0", NOTES);
    let probe = Fixture::new();
    let mut device = Device::new(&probe);
    let (f, entry) = app("example-app", &[&markdown], &["storage"], &device.hub);
    device.offer(1, vec![entry.clone()], vec![markdown]);
    let staging = device.root.join(".app-hub-install").join("verified");
    let staged = device.store.for_install_root(&staging);
    assert_eq!(staged.components_dir(), device.store.components_dir());
    staged.install_components(&entry.manifest, &mut |c| fetch(c)).unwrap();
    staged.install_staged("example-app", &f.bundle, &staged.publisher_keys(), &today()).unwrap();
    assert!(stored(&device.root, NOTES).is_file());
    assert!(!staging.join(".components").exists());
}

#[test]
fn a_system_app_carries_its_components_in_its_bundle() {
    let markdown = component_entry("org.example.markdown", "1.0.0", NOTES);
    let f = Fixture::new();
    let bundle = f.root.join("system-bundle");
    fs::create_dir_all(bundle.join("components")).unwrap();
    let file = bundle.join(format!("components/{}.wasm", blake3_hex(NOTES)));
    fs::write(&file, NOTES).unwrap();
    fs::write(bundle.join("main.splash"), "Label{text: \"System\"}").unwrap();
    let digest = octosense_app_policy::digest_dir(&bundle).unwrap();
    let manifest = serde_json::json!({"schema":1,"id":"os.notesdemo","version":"1","name":"Notes demo",
        "integrity":{"bundle_blake3":digest},"capabilities":["wasm","storage"],
        "requires":["wasm-shared-components-v1"],"components":[dependency("markdown", &markdown)]});
    fs::write(bundle.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
    let resolved = components::bundled(&bundle).unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!((resolved[0].alias.as_str(), resolved[0].path.as_path()), ("markdown", file.as_path()));
    // Changed, then missing.
    fs::write(&file, HOSTCALL).unwrap();
    let err = components::bundled(&bundle).unwrap_err();
    assert!(err.starts_with("component markdown (org.example.markdown 1.0.0): ") && err.contains("hashes to"), "{err}");
    fs::remove_file(&file).unwrap();
    assert_eq!(
        components::bundled(&bundle).unwrap_err(),
        format!("component markdown (org.example.markdown 1.0.0) is missing from os.notesdemo's bundle (components/{}.wasm)", blake3_hex(NOTES))
    );
}

#[test]
fn socket_apps_keep_catalog_readable_but_need_a_compatible_host_before_download() {
    let mut f = Fixture::new();
    let sockets = component_entry("org.example.netprobe", "1.0.0", NETPROBE);
    let mut device = Device::new(&f);
    let component_catalog = device.hub.catalog(1, vec![], vec![sockets.clone()]);
    f.pin(&[dependency("network", &sockets)], &[], &[]);
    f.socket_api();
    let entry = f.entry_with(Some(&component_catalog));
    // These are only RC5-known manifest features. Host API names are an
    // extensible map, so a missing implementation refuses this app, not the
    // signed catalog and its unrelated apps.
    assert!(entry.manifest.requires.iter().all(|feature| ["host-api-v1", "wasm-shared-components-v1"].contains(&feature.as_str())));
    device.offer(1, vec![entry.clone()], vec![sockets.clone()]);
    assert!(device.store.resolve_components(&entry.manifest).is_ok());
    let err = device.store.install_components(&entry.manifest, &mut |_| panic!("unsupported hosts must not download")).unwrap_err();
    assert_eq!(err, "this host does not implement required APIs: wasm.sockets@1");
    assert_eq!(device.store.install_staged("example-app", &f.bundle, &device.store.publisher_keys(), &today()).unwrap_err(), err);
    assert!(stored_files(&device.root).is_empty());

    device.store = device.store.with_host_api_versions(std::collections::BTreeMap::from([
        ("app_policy.device_consent".into(), 1), ("wasm.sockets".into(), 1),
    ]));
    device.store.install_components(&entry.manifest, &mut |_| Ok(NETPROBE.to_vec())).unwrap();
    device.store.install_staged("example-app", &f.bundle, &device.store.publisher_keys(), &today()).unwrap();
    assert_eq!(device.store.resolved_components("example-app").unwrap().len(), 1);
    device.store.may_run("example-app").unwrap();
    // Availability is checked again at launch, independently of installation.
    device.store = device.store.with_host_api_versions(std::collections::BTreeMap::new());
    assert_eq!(device.store.may_run("example-app").unwrap_err(), err);
}

#[test]
fn shared_socket_imports_cannot_skip_the_app_requirement_even_if_already_cached() {
    let mut f = Fixture::new();
    let sockets = component_entry("org.example.netprobe", "1.0.0", NETPROBE);
    let mut device = Device::new(&f);
    device.offer(1, vec![], vec![sockets.clone()]);
    f.pin(&[dependency("network", &sockets)], &[], &[]);
    components::keep(&device.root, &blake3_hex(NETPROBE), NETPROBE).unwrap();
    let err = device.store.resolve_components(&f.manifest).unwrap_err();
    assert!(err.contains("requires host_api.required wasm.sockets@1"), "{err}");
    assert_eq!(device.store.install_components(&f.manifest, &mut |_| panic!("no download on missing ABI")).unwrap_err(), err);
}
