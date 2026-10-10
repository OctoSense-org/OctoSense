//! A local catalog rehearsal through the real Store and shell dispatcher.
//! Ephemeral legacy signatures deliberately exercise installation offline;
//! this is not proof of GitHub attestation or public catalog publication.
use super::*;
use octosense_app_hub::{
    check_bundle, check_component, component_entry_for, components::ComponentRelease, entry_for,
    sign_manifest, Catalog, ComponentEntry, Entry, HubKey, PublisherKeys, Status, Store,
};
use octosense_app_policy::{digest_dir, AppManifest, HostLimits};
use octosense_appstore::{components, host_api, services, source::Origin};

const NOTES: &[u8] = include_bytes!("../../wasm-host/tests/fixtures/notes.component.wasm");
const HOSTCALL: &[u8] = include_bytes!("../../wasm-host/tests/fixtures/hostcall.component.wasm");
const FIRST: &str = "org.example.sharedfirst";
const SECOND: &str = "org.example.sharedsecond";

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct NoSheet;
impl ServiceHost for NoSheet {
    fn open_sheet(&mut self, _: &str) {
        panic!("A component must never open an approval sheet");
    }
    fn close_sheet(&mut self) {}
}

/// Run the normal asynchronous dispatcher and the UI's component host-call
/// pump. A caller-supplied app id never replaces the admitted app's identity.
fn call(app: &str, method: &str, args: Value, root: &Path) -> Result<Value, String> {
    static HEAP: AtomicUsize = AtomicUsize::new(1 << 42);
    let heap = HEAP.fetch_add(1, Ordering::Relaxed);
    services::dispatch(
        ServiceCall {
            app_id: app.into(),
            service: method.into(),
            args,
            from_sheet: false,
            may_prompt: true,
            host_dir: root.join(".host"),
        },
        heap,
        1,
        &mut NoSheet,
    );
    let limit = Instant::now() + Duration::from_secs(15);
    while Instant::now() < limit {
        pump_host_calls();
        if let Some((_, _, answer)) = services::take_replies_for(&[heap]).pop() {
            return answer.and_then(|v| serde_json::from_str(&v).map_err(|e| e.to_string()));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    services::cancel_heap(heap);
    panic!("No answer to {app} {method}");
}

fn component(id: &str, bytes: &[u8]) -> ComponentEntry {
    let release = ComponentRelease::from_draft(
        serde_json::from_value(json!({
            "component": {"schema":1,"id":id,"version":"1.0.0","name":"Acceptance component",
                "license":"MIT","publisher":{"name":"Example","support":"https://example.test/s",
                "privacy_policy_url":"https://example.test/privacy"}},
            "listing":{"description":"Synthetic local shared-component acceptance fixture"}
        }))
        .unwrap(),
        bytes,
    )
    .unwrap();
    let report = check_component(&release, bytes, false, None).unwrap();
    assert!(report.passed(), "{}", report.render());
    // The production publication gate must still refuse this unsigned fixture.
    assert!(!check_component(&release, bytes, true, None)
        .unwrap()
        .passed());
    component_entry_for(
        &release,
        bytes,
        &report,
        "dev:acceptance",
        "",
        "",
        &octosense_app_hub::today(),
    )
    .unwrap()
}

fn bundle(root: &Path, app: &str, pins: &Value, publisher: &HubKey) -> PathBuf {
    let path = root.join("mirror").join(app);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("main.splash"),
        "Label{text: \"Shared component acceptance\"}",
    )
    .unwrap();
    std::fs::write(path.join("icon.svg"), r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8"/></svg>"##).unwrap();
    std::fs::write(path.join("listing.json"), json!({
        "schema":1,"description":"Synthetic fixture","category":"utilities",
        "screenshots":["icon.svg"],"icon":"icon.svg","platforms":["macos","linux","windows","android"],
        "age_rating":"all","publisher":{"name":"Example","support":"https://example.test/s",
        "privacy_policy_url":"https://example.test/privacy"}
    }).to_string()).unwrap();
    let mut manifest = AppManifest::parse(
        &json!({
            "schema":1,"id":app,"version":"1.0.0","name":"Shared component acceptance",
            "integrity":{"bundle_blake3":digest_dir(&path).unwrap()},
            "capabilities":["wasm","storage"],"requires":["wasm-shared-components-v1"],
            "components":pins
        })
        .to_string(),
    )
    .unwrap();
    sign_manifest(publisher, &mut manifest, "acceptance-publisher").unwrap();
    std::fs::write(
        path.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(
        !path.join("fns").exists(),
        "A shared-only app must need no fns directory"
    );
    path
}

#[test]
fn installed_shared_components_keep_app_identity_and_revalidate_live_calls() {
    const CHILD: &str = "OCTOSENSE_TEST_INSTALLED_SHARED_COMPONENTS";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "wasm_service::shared_acceptance::installed_shared_components_keep_app_identity_and_revalidate_live_calls", "--nocapture"])
            .env(CHILD, "1").output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("1 passed"),
            "Child test must actually execute: {stdout}"
        );
        return;
    }
    assert!(
        SHARED_RESOLVER.lock().unwrap().is_none(),
        "Never bypass the installed resolver"
    );
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "octosense-shared-acceptance-{}",
        std::process::id()
    )));
    std::fs::create_dir(&scratch.0).unwrap();
    let root = scratch.0.join("apps");
    std::fs::create_dir(&root).unwrap();
    let anchor = HubKey::generate();
    let working = HubKey::generate();
    let publisher = HubKey::generate();
    let certificate = anchor.certify(&working.public_hex()).unwrap();
    std::env::set_var("OCTOSENSE_APP_DATA", &root);
    std::env::set_var("OCTOSENSE_HUB_CATALOG", "legacy");
    std::env::set_var("OCTOSENSE_HUB_ANCHOR", anchor.public_hex());
    octosense_appstore::set_data_root(root.clone());
    let notes = component("org.example.sharednotes", NOTES);
    let hostcall = component("org.example.sharedhost", HOSTCALL);
    let entries = vec![notes.clone(), hostcall.clone()];
    let pins = json!([
        {"as":"md","id":notes.component.id,"version":"1.0.0","blake3":notes.component.wasm_blake3},
        {"as":"bridge","id":hostcall.component.id,"version":"1.0.0","blake3":hostcall.component.wasm_blake3}
    ]);
    let catalog = |sequence, apps: Vec<Entry>, components: Vec<ComponentEntry>| {
        let mut catalog = Catalog::new(sequence, &octosense_app_hub::today(), apps);
        catalog.components = components;
        working.sign_catalog(&mut catalog, &certificate).unwrap();
        catalog
    };
    let component_catalog = catalog(1, vec![], entries.clone());
    let keys = PublisherKeys::new().with("acceptance-publisher", &publisher.public_hex());
    let bundles: Vec<_> = [FIRST, SECOND]
        .iter()
        .map(|app| bundle(&scratch.0, app, &pins, &publisher))
        .collect();
    let apps: Vec<_> = bundles
        .iter()
        .map(|bundle| {
            let report = check_bundle(
                bundle,
                &HostLimits::default(),
                &keys,
                Some(&component_catalog),
            )
            .unwrap();
            assert!(report.passed(), "{}", report.render());
            entry_for(
                bundle,
                &report,
                "acceptance-publisher",
                &publisher.public_hex(),
                "",
                "",
                &octosense_app_hub::today(),
            )
            .unwrap()
        })
        .collect();
    let encoded = serde_json::to_string(&catalog(1, apps.clone(), entries.clone())).unwrap();
    // A legacy rehearsal must never accidentally become a GitHub acceptance claim.
    assert!(
        Store::new(&anchor.public_hex(), &root, HostLimits::default())
            .with_github_catalog()
            .accept_catalog(&encoded)
            .is_err()
    );
    let mut store = Store::new(&anchor.public_hex(), &root, HostLimits::default())
        .with_host_api_versions(host_api::available_versions());
    store
        .accept_catalog_and_cache(&encoded, &root.join("catalog.json"))
        .unwrap();
    assert!(store
        .install_staged(FIRST, &bundles[0], &keys, &octosense_app_hub::today())
        .is_err());
    let mirror = scratch.0.join("mirror");
    let origin = Origin::Directory(mirror.clone());
    assert!(components::install(&store, &origin, &apps[0].manifest).is_err());
    for (entry, bytes) in [(&notes, NOTES), (&hostcall, HOSTCALL)] {
        let artifact = mirror.join(&entry.artifact);
        std::fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        std::fs::write(artifact, bytes).unwrap();
    }
    for ((app, bundle), entry) in [FIRST, SECOND].iter().zip(&bundles).zip(&apps) {
        components::install(&store, &origin, &entry.manifest).unwrap();
        store
            .install_staged(app, bundle, &keys, &octosense_app_hub::today())
            .unwrap();
        store
            .validate_prepared_launch(&store.prepare_launch(app).unwrap())
            .unwrap();
    }
    let first = components::resolved_in(&root, FIRST).unwrap();
    let second = components::resolved_in(&root, SECOND).unwrap();
    assert_eq!(
        first[0].path, second[0].path,
        "Deduplicate bytes, never instances"
    );
    assert!(std::fs::metadata(&first[0].path)
        .unwrap()
        .permissions()
        .readonly());
    let storage = crate::app_storage::Storage::with_file_secrets(
        crate::app_storage::Layout::new(&scratch.0.join("home")).unwrap(),
    );
    let mut env = areas::FixedEnv {
        storage: Some(storage.clone()),
        ..areas::FixedEnv::default()
    };
    for app in [FIRST, SECOND] {
        env.quotas.insert(
            app.into(),
            areas::JailQuota {
                bytes: Some(4096),
                storage: true,
            },
        );
    }
    *AREA_ENV.lock().unwrap() = Some(Arc::new(env));
    struct Echo;
    impl HostService for Echo {
        fn family(&self) -> &'static str {
            "sharedacceptance"
        }
        fn call(&mut self, call: ServiceCall, reply: Replier, _: &mut dyn ServiceHost) {
            reply.send(Ok(
                json!({"app":call.app_id,"method":call.method(),"args":call.args,
                "may_prompt":call.may_prompt,"from_sheet":call.from_sheet}),
            ));
        }
    }
    services::register_host_service(Box::new(Echo));
    register();
    let invoke = |app, method, args| call(app, method, args, &root);
    assert_eq!(
        invoke(FIRST, "wasm.md.to_html", json!("# Installed")).unwrap(),
        "<h1>Installed</h1>\n"
    );
    assert_eq!(
        invoke(SECOND, "wasm.md.to-html", json!({"markdown":"*Second*"})).unwrap(),
        "<p><em>Second</em></p>\n"
    );
    assert_eq!(invoke(FIRST, "wasm.md.count", Value::Null).unwrap(), 1);
    assert_eq!(invoke(FIRST, "wasm.md.count", json!({})).unwrap(), 2);
    assert_eq!(invoke(SECOND, "wasm.md.count", json!({})).unwrap(), 1);
    assert!(invoke(FIRST, "wasm.count", json!({}))
        .unwrap_err()
        .contains("no function"));
    let described = invoke(FIRST, "wasm.functions", json!({})).unwrap();
    assert!(described["functions"]
        .as_array()
        .unwrap()
        .contains(&json!("md.to_html")));
    assert!(described["functions"]
        .as_array()
        .unwrap()
        .contains(&json!("bridge.call")));
    assert_eq!(described["modules"][0]["shared"]["id"], notes.component.id);
    invoke(
        FIRST,
        "wasm.md.save_html",
        json!({"markdown":"# Private","path":"private.html"}),
    )
    .unwrap();
    assert_eq!(
        invoke(FIRST, "wasm.md.read_file", json!("private.html")).unwrap(),
        "<h1>Private</h1>\n"
    );
    assert!(invoke(SECOND, "wasm.md.read_file", json!("private.html")).is_err());
    assert!(!storage
        .layout()
        .app(SECOND)
        .unwrap()
        .jail
        .join("private.html")
        .exists());
    for app in [FIRST, SECOND] {
        let reply = invoke(
            app,
            "wasm.bridge.call",
            json!([
                "sharedacceptance.echo",
                "{\"app\":\"org.example.impersonated\"}"
            ]),
        )
        .unwrap();
        let reply: Value = serde_json::from_str(reply.as_str().unwrap()).unwrap();
        assert_eq!(
            reply,
            json!({"app":app,"method":"echo","args":{"app":"org.example.impersonated"},"may_prompt":false,"from_sheet":false})
        );
        assert!(
            invoke(app, "wasm.bridge.call", json!(["wasm.md.count", "{}"]))
                .unwrap_err()
                .contains("cannot call wasm.*")
        );
    }
    // A running app must not continue using a cached instance when the shared
    // bytes have changed or its component is withdrawn in the current catalog.
    let component_path = &first[0].path;
    std::fs::remove_file(component_path).unwrap();
    std::fs::write(component_path, b"corrupt").unwrap();
    assert!(invoke(FIRST, "wasm.md.count", json!({})).is_err());
    std::fs::remove_file(component_path).unwrap();
    assert!(invoke(SECOND, "wasm.md.count", json!({})).is_err());
    components::install(&store, &origin, &apps[0].manifest).unwrap();
    // Successful re-admission after repair proves errors do not poison the app.
    assert!(invoke(FIRST, "wasm.md.count", json!({}))
        .unwrap()
        .is_number());
    let mut withdrawn = entries;
    withdrawn[0].status = Status::Withdrawn("Synthetic withdrawal".into());
    let next = serde_json::to_string(&catalog(2, apps, withdrawn)).unwrap();
    store
        .accept_catalog_and_cache(&next, &root.join("catalog.json"))
        .unwrap();
    for app in [FIRST, SECOND] {
        let error = invoke(app, "wasm.md.count", json!({})).unwrap_err();
        assert!(error.contains("withdrawn"), "{error}");
    }
}
