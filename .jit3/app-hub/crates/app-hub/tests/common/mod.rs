#![allow(dead_code)]
use octosense_app_hub::*;
use octosense_app_policy::{AppManifest, HostLimits};
use std::{fs, path::{Path, PathBuf}, sync::atomic::{AtomicU64, Ordering}};

pub struct Fixture {
    pub root: PathBuf,
    pub bundle: PathBuf,
    pub publisher: HubKey,
    pub manifest: AppManifest,
}

impl Fixture {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!("hub-regression-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&root).unwrap();
        let bundle = root.join("bundle");
        fs::create_dir(&bundle).unwrap();
        // A script app: the runnable entry the gate requires.
        fs::write(bundle.join("main.splash"), "Label{text: \"Example\"}").unwrap();
        fs::write(bundle.join("icon.svg"), r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><rect width="64" height="64" fill="#146"/></svg>"##).unwrap();
        // A real, decodable 1x1 PNG; visual quality is tested separately.
        use base64::Engine;
        fs::write(bundle.join("screen.png"), base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=").unwrap()).unwrap();
        fs::write(bundle.join("listing.json"), serde_json::to_vec(&serde_json::json!({
            "schema": 1, "description": "Publisher regression fixture", "category": "utilities",
            "screenshots": ["screen.png"], "icon": "icon.svg", "platforms": ["android"],
            "publisher": {"name": "Example", "support": "https://example.test/support", "privacy_policy_url": "https://example.test/privacy"},
            "age_rating": "all"
        })).unwrap()).unwrap();
        let publisher = HubKey::generate();
        let manifest = AppManifest::parse(r#"{"schema":1,"id":"example-app","version":"1.0.0","name":"Example","integrity":{"bundle_blake3":""}}"#).unwrap();
        let mut fixture = Self { root, bundle, publisher, manifest };
        fixture.sign();
        fixture
    }

    /// The extensible socket ABI requirement: old hosts still read the catalog.
    pub fn socket_api(&mut self) {
        if !self.manifest.requires.iter().any(|f| f == "host-api-v1") {
            self.manifest.requires.push("host-api-v1".into());
        }
        self.manifest.host_api.get_or_insert_with(Default::default).required.insert("wasm.sockets".into(), 1);
        self.sign();
    }

    /// The same app as an L0 card (`page.card`, its data and its kit).
    pub fn card(&mut self) {
        fs::remove_file(self.bundle.join("main.splash")).unwrap();
        fs::create_dir_all(self.bundle.join("kit/native/light")).unwrap();
        fs::write(self.bundle.join("page.card"), include_str!("../fixtures/card/page.card")).unwrap();
        fs::write(self.bundle.join("page.data.json"), include_str!("../fixtures/card/page.data.json")).unwrap();
        fs::write(self.bundle.join("kit/native/light/kit.json"), include_str!("../fixtures/card/kit/native/light/kit.json")).unwrap();
        self.sign();
    }

    pub fn write_manifest(&self) {
        fs::write(self.bundle.join("manifest.json"), serde_json::to_vec_pretty(&self.manifest).unwrap()).unwrap();
    }

    pub fn sign(&mut self) {
        self.manifest.integrity.bundle_blake3 = octosense_app_policy::digest_dir(&self.bundle).unwrap();
        sign_manifest(&self.publisher, &mut self.manifest, "publisher-one").unwrap();
        self.write_manifest();
    }

    pub fn keys(&self) -> PublisherKeys {
        PublisherKeys::new().with("publisher-one", &self.publisher.public_hex())
    }

    pub fn report(&self, previous: Option<&Catalog>) -> GateReport {
        check_bundle(&self.bundle, &HostLimits::default(), &self.keys(), previous).unwrap()
    }

    pub fn entry(&self) -> Entry {
        let report = self.report(None);
        assert!(report.passed(), "{}", report.render());
        entry_for(&self.bundle, &report, "publisher-one", &self.publisher.public_hex(), "", "", "2026-09-25").unwrap()
    }

    /// This app as an unsigned release on record (`--allow-unsigned`).
    pub fn unsigned_entry(&self) -> Entry {
        self.unsigned_entry_at(&self.manifest.version.clone())
    }

    pub fn unsigned_entry_at(&self, version: &str) -> Entry {
        let mut entry = self.entry();
        entry.manifest.version = version.into();
        entry.manifest.integrity.signature = None;
        entry.publisher_key = String::new();
        entry
    }
}

impl Drop for Fixture {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); }
}

pub fn write_catalog(path: &Path, catalog: &Catalog) {
    fs::write(path, serde_json::to_vec_pretty(catalog).unwrap()).unwrap();
}

// ---- shared components (App Hub ADR 0003) -----------------------------------

/// Components built with plain `cargo build --target wasm32-wasip2`, copied
/// byte for byte from OctoSense's `crates/wasm-host/tests/fixtures`.
pub const NOTES: &[u8] = include_bytes!("../fixtures/notes.component.wasm");
pub const FETCH: &[u8] = include_bytes!("../fixtures/fetch.component.wasm");
pub const HOSTCALL: &[u8] = include_bytes!("../fixtures/hostcall.component.wasm");
pub const NETPROBE: &[u8] = include_bytes!("../fixtures/netprobe.component.wasm");

pub fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// What a publisher writes before `hub component-prepare`.
pub fn draft_json(id: &str, version: &str) -> serde_json::Value {
    serde_json::json!({
        "component": {
            "schema": 1, "id": id, "version": version, "name": "Notes",
            "publisher": {"name": "Example", "support": "https://example.test/support", "privacy_policy_url": "https://example.test/privacy"},
            "license": "MIT OR Apache-2.0"
        },
        "listing": {"subtitle": "Markdown notes", "description": "Renders and analyzes Markdown.", "keywords": ["markdown"]}
    })
}

/// An unsigned development release of `wasm` as `id` `version`.
pub fn release(id: &str, version: &str, wasm: &[u8]) -> components::ComponentRelease {
    let draft: components::ComponentDraft = serde_json::from_value(draft_json(id, version)).unwrap();
    components::ComponentRelease::from_draft(draft, wasm).unwrap()
}

/// The catalog entry the gate makes for an unsigned development release.
pub fn component_entry(id: &str, version: &str, wasm: &[u8]) -> ComponentEntry {
    let release = release(id, version, wasm);
    let report = check_component(&release, wasm, false, None).unwrap();
    assert!(report.passed(), "{}", report.render());
    component_entry_for(&release, wasm, &report, "dev:example", "", "", "2026-10-09").unwrap()
}

/// A dependency on `entry` as the app calls it, `alias`.
pub fn dependency(alias: &str, entry: &ComponentEntry) -> octosense_app_policy::ComponentDependency {
    octosense_app_policy::ComponentDependency::new(alias, entry.id(), entry.version(), &entry.component.wasm_blake3)
}

impl Fixture {
    /// The entry for this app, admitted against `previous`.
    pub fn entry_with(&self, previous: Option<&Catalog>) -> Entry {
        let report = self.report(previous);
        assert!(report.passed(), "{}", report.render());
        entry_for(&self.bundle, &report, "publisher-one", &self.publisher.public_hex(), "", "", "2026-09-25").unwrap()
    }

    /// Make this app pin `components`, with the feature and these extra
    /// capabilities and hosts, and sign it again.
    pub fn pin(&mut self, components: &[octosense_app_policy::ComponentDependency], capabilities: &[&str], hosts: &[&str]) {
        self.manifest.components = components.to_vec();
        if !self.manifest.requires.iter().any(|f| f == "wasm-shared-components-v1") {
            self.manifest.requires.push("wasm-shared-components-v1".into());
        }
        for capability in ["wasm"].iter().chain(capabilities) {
            if !self.manifest.capabilities.iter().any(|c| c == capability) {
                self.manifest.capabilities.push(capability.to_string());
            }
        }
        self.manifest.network.hosts = hosts.iter().map(|h| h.to_string()).collect();
        self.sign();
    }
}

/// A legacy anchor-signed catalog, for a development hub's tests.
pub struct Hub {
    pub anchor: HubKey,
    pub working: HubKey,
}

impl Hub {
    pub fn new() -> Self {
        Hub { anchor: HubKey::generate(), working: HubKey::generate() }
    }

    pub fn sign(&self, catalog: &mut Catalog) {
        let certificate = self.anchor.certify(&self.working.public_hex()).unwrap();
        self.working.sign_catalog(catalog, &certificate).unwrap();
    }

    pub fn catalog(&self, sequence: u64, entries: Vec<Entry>, components: Vec<ComponentEntry>) -> Catalog {
        let mut catalog = Catalog::new(sequence, &today(), entries);
        catalog.components = components;
        self.sign(&mut catalog);
        catalog
    }

    pub fn json(&self, sequence: u64, entries: Vec<Entry>, components: Vec<ComponentEntry>) -> String {
        serde_json::to_string(&self.catalog(sequence, entries, components)).unwrap()
    }
}
