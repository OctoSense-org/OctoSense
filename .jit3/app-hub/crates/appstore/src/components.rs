//! Shared components for a host (App Hub ADR 0003): the files an app's
//! `wasm` service loads beside the app's own `fns/`.
//!
//! An app's manifest pins each component it uses by id, exact version and
//! BLAKE3 digest (`components`). A system app ships them in its bundle at
//! `components/<blake3>.wasm`; an installed app's live once per digest in
//! the shared, read-only store `<apps root>/.components/<blake3>.wasm`,
//! which the installer fills from the verified catalog ([`install`]).
//! [`resolved`] is what a host calls to load them: it never fetches, and it
//! verifies every file on every call.
use octosense_app_hub::Store;
use std::path::Path;

pub use octosense_app_hub::components::Resolved;

/// The shared components `app_id` pins, as its host's `wasm` service loads
/// them, in the manifest's order:
///
/// - a system app's from its bundle's `components/<blake3>.wasm`
///   ([`crate::system::prepare`] admits the bundle first);
/// - an installed app's from the shared store, resolved against the
///   verified catalog this device cached: the installed release must still
///   run ([`Store::may_run`]), and each component must still be offered
///   there at the pinned version and digest.
///
/// Every file's BLAKE3 digest is checked on every call; nothing is fetched.
/// The error says what to do when a component is missing, changed or
/// withdrawn. An app that pins none gets an empty list. The apps root is
/// the one the host set ([`crate::set_data_root`] or `OCTOSENSE_APP_DATA`),
/// and the catalog channel and anchor are the host's
/// (`OCTOSENSE_HUB_CATALOG`, `OCTOSENSE_HUB_ANCHOR`), as for every launch.
pub fn resolved(app_id: &str) -> Result<Vec<Resolved>, String> {
    let root = crate::data_root_if_set().ok_or("App Hub has no apps root yet")?;
    resolved_in(&root, app_id)
}

/// [`resolved`] under an explicit apps root.
pub fn resolved_in(root: &Path, app_id: &str) -> Result<Vec<Resolved>, String> {
    if let Some(system) = crate::system::system_app(app_id) {
        let (bundle, _) = crate::system::prepare(root, &system)?;
        return octosense_app_hub::components::bundled(&bundle);
    }
    let channel = crate::source::CatalogChannel::from_environment(root)?;
    let anchor = std::env::var("OCTOSENSE_HUB_ANCHOR")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| crate::DEFAULT_ANCHOR.to_string());
    let mut store = channel.configure(
        Store::new(&anchor, root, octosense_app_policy::HostLimits::default())
            .with_host_api_versions(crate::host_api::available_versions()),
    );
    let catalog = channel.read_cache(root).map_err(|_| "No verified App Hub catalog is available on this device".to_string())?;
    store.accept_catalog(&catalog).map_err(|e| format!("App Hub catalog refused: {e}"))?;
    store.resolved_components(app_id)
}

/// Keep every component `manifest` pins in the shared store before the app
/// itself is installed or updated: each resolves from `store`'s verified
/// catalog and is fetched from `origin`, like the app's bundle, only when
/// the store lacks it. [`Store::install_staged`] then refuses an app whose
/// components are not there.
pub fn install(store: &Store, origin: &crate::source::Origin, manifest: &octosense_app_policy::AppManifest) -> Result<Vec<Resolved>, String> {
    store.install_components(manifest, &mut |entry| origin.component(&entry.artifact))
}

#[cfg(test)]
mod tests {
    use super::*;
    use octosense_app_hub::{components::ComponentRelease, Catalog, HubKey};
    use std::path::PathBuf;

    /// OctoSense's notes component, byte for byte (App Hub's fixture).
    const NOTES: &[u8] = include_bytes!("../../app-hub/tests/fixtures/notes.component.wasm");

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("appstore-components-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn markdown() -> octosense_app_hub::ComponentEntry {
        let draft: octosense_app_hub::components::ComponentDraft = serde_json::from_value(serde_json::json!({
            "component": {"schema": 1, "id": "org.example.markdown", "version": "1.0.0", "name": "Markdown",
                "publisher": {"name": "Example", "support": "https://example.test/s", "privacy_policy_url": "https://example.test/p"},
                "license": "MIT"},
            "listing": {"description": "Renders Markdown."}
        }))
        .unwrap();
        let release = ComponentRelease::from_draft(draft, NOTES).unwrap();
        let report = octosense_app_hub::check_component(&release, NOTES, false, None).unwrap();
        octosense_app_hub::component_entry_for(&release, NOTES, &report, "dev:example", "", "", "2026-10-09").unwrap()
    }

    fn pin(entry: &octosense_app_hub::ComponentEntry) -> serde_json::Value {
        serde_json::json!([{"as": "markdown", "id": entry.id(), "version": entry.version(), "blake3": entry.component.wasm_blake3}])
    }

    /// A system app's pack: the bundle in `dir` with its digest stamped in.
    fn system_pack(dir: &Path, id: &str, components: serde_json::Value) -> &'static str {
        std::fs::write(dir.join("main.splash"), "Label{text: \"System\"}").unwrap();
        let digest = octosense_app_policy::digest_dir(dir).unwrap();
        let manifest = serde_json::json!({"schema":1,"id":id,"version":"1","name":"Demo",
            "integrity":{"bundle_blake3":digest},"capabilities":["wasm","storage"],
            "requires":["wasm-shared-components-v1"],"components":components});
        std::fs::write(dir.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
        Box::leak(serde_json::to_string(&octosense_app_hub::pack::pack_dir(dir).unwrap()).unwrap().into_boxed_str())
    }

    #[test]
    fn a_system_app_resolves_the_components_its_bundle_ships() {
        let entry = markdown();
        let source = scratch("system-source");
        std::fs::create_dir_all(source.join("components")).unwrap();
        std::fs::write(source.join(format!("components/{}.wasm", entry.component.wasm_blake3)), NOTES).unwrap();
        let pack = system_pack(&source, "os.componentdemo", pin(&entry));
        crate::system::register_system_app(crate::system::SystemApp { id: "os.componentdemo", name: "Demo", pack, assets: &[] });
        let root = scratch("system-root");
        let resolved = resolved_in(&root, "os.componentdemo").unwrap();
        assert_eq!(resolved.len(), 1);
        let one = &resolved[0];
        assert_eq!((one.alias.as_str(), one.id.as_str(), one.version.as_str()), ("markdown", "org.example.markdown", "1.0.0"));
        assert_eq!(one.blake3, entry.component.wasm_blake3);
        assert!(one.path.starts_with(root.join(".system").join("os.componentdemo")), "{}", one.path.display());
        assert_eq!(std::fs::read(&one.path).unwrap(), NOTES);

        // A system app whose bundle lacks a component it pins never opens.
        let lacking = scratch("system-lacking");
        let pack = system_pack(&lacking, "os.componentlack", pin(&entry));
        let app = crate::system::SystemApp { id: "os.componentlack", name: "Lacking", pack, assets: &[] };
        let err = crate::system::prepare(&scratch("system-lacking-root"), &app).unwrap_err();
        assert!(err.contains("is missing from os.componentlack's bundle"), "{err}");
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn unhex(text: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }

    /// The host API for an installed app reads the host's channel and anchor
    /// from the environment, so this runs in a child process that sets them.
    #[test]
    fn an_installed_app_resolves_from_the_verified_cached_catalog() {
        const CHILD: &str = "OCTOSENSE_TEST_COMPONENTS_ANCHOR";
        let Some(anchor_hex) = std::env::var_os(CHILD) else {
            let anchor = HubKey::generate();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "components::tests::an_installed_app_resolves_from_the_verified_cached_catalog", "--nocapture"])
                .env(CHILD, hex(&anchor.to_bytes()))
                .env("OCTOSENSE_HUB_CATALOG", "legacy")
                .env("OCTOSENSE_HUB_ANCHOR", anchor.public_hex())
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(output.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
            assert!(stdout.contains("1 passed"), "{stdout}");
            return;
        };
        let anchor = HubKey::from_bytes(&unhex(anchor_hex.to_str().unwrap()));
        let working = HubKey::generate();
        let certificate = anchor.certify(&working.public_hex()).unwrap();
        let root = scratch("installed-root");
        let entry = markdown();

        // The app, signed by its publisher and admitted against the catalog.
        let bundle = scratch("installed-bundle");
        std::fs::write(bundle.join("main.splash"), "Label{text: \"Installed\"}").unwrap();
        std::fs::write(bundle.join("icon.svg"), r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8"/></svg>"##).unwrap();
        std::fs::write(bundle.join("listing.json"), serde_json::to_vec(&serde_json::json!({
            "schema": 1, "description": "Fixture", "category": "utilities", "screenshots": ["icon.svg"], "icon": "icon.svg",
            "platforms": ["macos"], "age_rating": "all",
            "publisher": {"name": "Example", "support": "https://example.test/s", "privacy_policy_url": "https://example.test/p"}
        })).unwrap()).unwrap();
        let publisher = HubKey::generate();
        let mut manifest = octosense_app_policy::AppManifest::parse(&serde_json::json!({
            "schema":1,"id":"org.example.writer","version":"1.0.0","name":"Writer",
            "integrity":{"bundle_blake3":octosense_app_policy::digest_dir(&bundle).unwrap()},
            "capabilities":["wasm","storage"],"requires":["wasm-shared-components-v1"],"components":pin(&entry)
        }).to_string()).unwrap();
        octosense_app_hub::sign_manifest(&publisher, &mut manifest, "publisher-one").unwrap();
        std::fs::write(bundle.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
        let catalog_with = |sequence: u64, apps: Vec<octosense_app_hub::Entry>, component: octosense_app_hub::ComponentEntry| {
            let mut catalog = Catalog::new(sequence, &octosense_app_hub::today(), apps);
            catalog.components = vec![component];
            working.sign_catalog(&mut catalog, &certificate).unwrap();
            catalog
        };
        let keys = octosense_app_hub::PublisherKeys::new().with("publisher-one", &publisher.public_hex());
        let report = octosense_app_hub::check_bundle(&bundle, &octosense_app_policy::HostLimits::default(), &keys, Some(&catalog_with(1, vec![], entry.clone()))).unwrap();
        assert!(report.passed(), "{}", report.render());
        let app = octosense_app_hub::entry_for(&bundle, &report, "publisher-one", &publisher.public_hex(), "", "", "2026-10-09").unwrap();
        let catalog = serde_json::to_string(&catalog_with(1, vec![app.clone()], entry.clone())).unwrap();

        // No verified catalog yet: nothing resolves.
        assert_eq!(resolved_in(&root, "org.example.writer").unwrap_err(), "No verified App Hub catalog is available on this device");
        std::fs::write(root.join("catalog.json"), &catalog).unwrap();
        assert!(resolved_in(&root, "org.example.writer").unwrap_err().contains("is not installed"));

        // Installed as the store installs it, the component comes from the store.
        let mut store = Store::new(&anchor.public_hex(), &root, octosense_app_policy::HostLimits::default());
        store.accept_catalog(&catalog).unwrap();
        let origin = crate::source::Origin::Directory(scratch("installed-mirror"));
        assert!(install(&store, &origin, &app.manifest).unwrap_err().contains("is not in this hub mirror"));
        let crate::source::Origin::Directory(mirror) = &origin else { unreachable!() };
        std::fs::create_dir_all(mirror.join("artifacts")).unwrap();
        std::fs::write(mirror.join(&entry.artifact), NOTES).unwrap();
        install(&store, &origin, &app.manifest).unwrap();
        store.install_staged("org.example.writer", &bundle, &store.publisher_keys(), &octosense_app_hub::today()).unwrap();
        let resolved = resolved_in(&root, "org.example.writer").unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].path, root.join(".components").join(format!("{}.wasm", entry.component.wasm_blake3)));
        assert_eq!(resolved[0].alias, "markdown");

        // Withdrawn in a newer verified catalog: it no longer resolves.
        let mut withdrawn = entry.clone();
        withdrawn.status = octosense_app_hub::Status::Withdrawn("Renders scripts it should escape".into());
        std::fs::write(root.join("catalog.json"), serde_json::to_string(&catalog_with(2, vec![app], withdrawn)).unwrap()).unwrap();
        assert_eq!(
            resolved_in(&root, "org.example.writer").unwrap_err(),
            "component markdown (org.example.markdown 1.0.0) was withdrawn: Renders scripts it should escape"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
