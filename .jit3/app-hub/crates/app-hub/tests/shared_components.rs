//! Shared components (App Hub ADR 0003): the gate's review of a component
//! release, an app's resolution of the components it pins, and the catalog
//! that records them.
mod common;
use common::*;
use octosense_app_hub::*;
use octosense_app_policy::{HostLimits, RefuseAllSignatures};
use std::fs;

fn refusals(findings: &[Finding], check: &str) -> Vec<String> {
    findings.iter().filter(|f| f.check == check && f.severity == Severity::Refusal).map(|f| f.detail.clone()).collect()
}

fn warnings(findings: &[Finding], check: &str) -> Vec<String> {
    findings.iter().filter(|f| f.check == check && f.severity == Severity::Warning).map(|f| f.detail.clone()).collect()
}

// ---- a component release ------------------------------------------------------

#[test]
fn a_release_describes_its_file_and_reviewers_see_what_it_reaches() {
    let notes = release("org.example.markdown", "1.0.0", NOTES);
    let c = &notes.component;
    assert_eq!(c.wasm_blake3, blake3_hex(NOTES));
    assert_eq!(c.bytes, NOTES.len() as u64);
    assert!(c.imports.iter().any(|i| i.starts_with("wasi:filesystem/")), "{:?}", c.imports);
    let names: Vec<&str> = c.exports.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["analyze", "count", "echo-bytes", "grow", "now-ms", "random-u64", "read-file", "save-html", "spin", "to-html"]);
    assert!(c.integrity.github.is_none());

    // A development check: unsigned is a warning, and the reach line names
    // what the component reaches in any app that pins it.
    let report = check_component(&notes, NOTES, false, None).unwrap();
    assert!(report.passed(), "{}", report.render());
    assert_eq!(warnings(&report.findings, "publisher-signature"), ["unsigned: accountability rests on the hub alone"]);
    let reach = report.findings.iter().find(|f| f.check == "functions").unwrap();
    assert_eq!(reach.path.as_deref(), Some("org.example.markdown-1.0.0.wasm"));
    assert_eq!(
        reach.detail,
        "org.example.markdown 1.0.0 is a component that reaches the clock, random numbers and files in its app folder, but no network or other app"
    );
    assert_eq!(report.digest, blake3_hex(NOTES));
    assert!(report.render().starts_with(&format!("org.example.markdown 1.0.0 (component {}) — PASSED\n", blake3_hex(NOTES))));
    assert_eq!(report.json()["kind"], "component");

    // App Hub itself accepts only GitHub-attested releases.
    let report = check_component(&notes, NOTES, true, None).unwrap();
    assert_eq!(refusals(&report.findings, "publisher-signature"), ["App Hub accepts only GitHub-attested component releases"]);

    // HTTP reaches the network; host services need no grant of their own.
    let fetch = check_component(&release("org.example.fetch", "1.0.0", FETCH), FETCH, false, None).unwrap();
    assert!(
        fetch.findings.iter().any(|f| f.detail
            == "org.example.fetch 1.0.0 is a component that reaches the clock and the network, but no files or other app"),
        "{}",
        fetch.render()
    );
    let hostcall = check_component(&release("org.example.hostcall", "1.0.0", HOSTCALL), HOSTCALL, false, None).unwrap();
    assert!(hostcall.render().contains("reaches the clock and its app's host services, but no files, network or other app"), "{}", hostcall.render());
}

#[test]
fn the_gate_refuses_a_file_that_is_not_the_release() {
    let notes = release("org.example.markdown", "1.0.0", NOTES);
    // Another file: its digest and size are not the release's.
    let report = check_component(&notes, FETCH, false, None).unwrap();
    let digest = refusals(&report.findings, "digest");
    assert_eq!(digest.len(), 2, "{}", report.render());
    assert_eq!(digest[0], format!("the file hashes to {}, the release says {}", blake3_hex(FETCH), blake3_hex(NOTES)));
    assert!(digest[1].starts_with("the file is 84774 bytes, the release says 310601"), "{digest:?}");
    // Edited imports or exports.
    let mut edited = notes.clone();
    edited.component.imports.pop();
    let report = check_component(&edited, NOTES, false, None).unwrap();
    assert_eq!(refusals(&report.findings, "component"), ["the release's imports or exports are not the file's; prepare the release again from this file"]);
    // A truncated file.
    let truncated = &NOTES[..NOTES.len() / 2];
    let report = check_component(&notes, truncated, false, None).unwrap();
    assert!(refusals(&report.findings, "contents-invalid")[0].contains("not a valid WebAssembly component"), "{}", report.render());
    // A core module is no component.
    let module: &[u8] = b"\0asm\x01\0\0\0";
    let draft: components::ComponentDraft = serde_json::from_value(draft_json("org.example.m", "1.0.0")).unwrap();
    assert!(components::ComponentRelease::from_draft(draft, module).unwrap_err().contains("not a WebAssembly component"));
}

#[test]
fn component_ids_versions_publishers_and_listings_follow_their_rules() {
    let mut value = serde_json::to_value(release("org.example.markdown", "1.0.0", NOTES)).unwrap();
    for (pointer, bad, needle) in [
        ("/component/id", serde_json::json!("os.notes"), "under os."),
        ("/component/id", serde_json::json!("org.example.terminal"), "reserved"),
        ("/component/id", serde_json::json!("Org.Example"), "may hold only"),
        ("/component/version", serde_json::json!("1.0"), "one exact semantic version"),
        ("/component/version", serde_json::json!("^1.0.0"), "one exact semantic version"),
        ("/component/license", serde_json::json!(""), "SPDX"),
        ("/component/name", serde_json::json!(" "), "1 to 64 characters"),
        ("/component/publisher/privacy_policy_url", serde_json::json!("http://example.test/p"), "https URL"),
        ("/component/schema", serde_json::json!(2), "schema 2 is not 1"),
        ("/listing/description", serde_json::json!(""), "description is empty"),
        ("/listing/subtitle", serde_json::json!("x".repeat(81)), "subtitle is over 80"),
    ] {
        let original = value.pointer(pointer).unwrap().clone();
        *value.pointer_mut(pointer).unwrap() = bad;
        let edited: ComponentRelease = serde_json::from_value(value.clone()).unwrap();
        let problems = edited.problems();
        assert!(problems.iter().any(|p| p.contains(needle)), "{pointer}: {problems:?}");
        let report = check_component(&edited, NOTES, false, None).unwrap();
        assert!(!report.passed(), "{pointer}");
        *value.pointer_mut(pointer).unwrap() = original;
    }
    // Unknown fields are refused, as in an app manifest.
    value["component"]["range"] = serde_json::json!("^1");
    assert!(serde_json::from_value::<ComponentRelease>(value).unwrap_err().to_string().contains("unknown field `range`"));
}

#[test]
fn a_version_is_published_once_and_a_component_id_is_no_apps() {
    let hub = Hub::new();
    let notes = component_entry("org.example.markdown", "1.0.0", NOTES);
    let catalog = hub.catalog(1, vec![], vec![notes.clone()]);
    let again = check_component(&notes.release(), NOTES, false, Some(&catalog)).unwrap();
    assert_eq!(
        refusals(&again.findings, "version"),
        ["version 1.0.0 of component org.example.markdown is already published; publish a new version"]
    );
    let next = release("org.example.markdown", "1.1.0", NOTES);
    assert!(check_component(&next, NOTES, false, Some(&catalog)).unwrap().passed());
    // An app already holds the id.
    let app = Fixture::new();
    let catalog = hub.catalog(2, vec![app.entry()], vec![]);
    let report = check_component(&release("example-app", "1.0.0", NOTES), NOTES, false, Some(&catalog)).unwrap();
    assert_eq!(refusals(&report.findings, "identity"), ["example-app is an app's id in the catalog; a component needs an id of its own"]);
}

// ---- an app that pins components --------------------------------------------------

fn resolving(hub: &Hub, components: Vec<ComponentEntry>) -> Catalog {
    hub.catalog(1, vec![], components)
}

#[test]
fn an_app_resolves_its_components_from_the_catalog_and_reviewers_see_their_reach() {
    let hub = Hub::new();
    let notes = component_entry("org.example.markdown", "1.0.0", NOTES);
    let catalog = resolving(&hub, vec![notes.clone()]);
    let mut f = Fixture::new();
    f.pin(&[dependency("markdown", &notes)], &["storage"], &[]);
    let report = f.report(Some(&catalog));
    assert!(report.passed(), "{}", report.render());
    assert_eq!(
        warnings(&report.findings, "components"),
        ["component markdown (org.example.markdown 1.0.0) reaches the clock, random numbers and files in its app folder, but no network or other app"]
    );
    // No fns/ is needed when the app's functions are shared components.
    assert!(!report.render().contains("carries no fns/*.wasm"), "{}", report.render());
    // The store says whose code it runs.
    let entry = f.entry_with(Some(&catalog));
    assert!(
        entry.permissions_summary().contains(
            &"Runs shared components App Hub reviewed, with only this app's permissions: org.example.markdown 1.0.0".to_string()
        ),
        "{:?}",
        entry.permissions_summary()
    );
}

#[test]
fn missing_declarations_do_not_refuse_supported_component_imports() {
    let hub = Hub::new();
    let notes = component_entry("org.example.markdown", "1.0.0", NOTES);
    let fetch = component_entry("org.example.fetch", "1.0.0", FETCH);
    let hostcall = component_entry("org.example.hostcall", "1.0.0", HOSTCALL);
    let catalog = resolving(&hub, vec![notes.clone(), fetch.clone(), hostcall.clone()]);

    let mut f = Fixture::new();
    f.pin(&[dependency("markdown", &notes)], &[], &[]);
    assert!(f.report(Some(&catalog)).passed());
    f.pin(&[dependency("fetch", &fetch)], &[], &[]);
    assert!(f.report(Some(&catalog)).passed());
    // net is the declaration; network.hosts is not required (OctoSense's
    // ruling of 8 October 2026).
    for hosts in [&[][..], &["api.example.com"][..]] {
        f.pin(&[dependency("fetch", &fetch)], &["net"], hosts);
        let report = f.report(Some(&catalog));
        assert!(report.passed(), "{}", report.render());
        assert_eq!(
            warnings(&report.findings, "components"),
            ["component fetch (org.example.fetch 1.0.0) reaches the clock and the network, but no files or other app"]
        );
    }

    let mut f = Fixture::new();
    f.pin(&[dependency("host", &hostcall)], &[], &[]);
    let report = f.report(Some(&catalog));
    assert!(report.passed(), "{}", report.render());
    assert_eq!(
        warnings(&report.findings, "components"),
        ["component host (org.example.hostcall 1.0.0) reaches the clock and its app's host services, but no files, network or other app"]
    );
}

#[test]
fn a_missing_mismatched_or_withdrawn_component_refuses_the_app() {
    let hub = Hub::new();
    let notes = component_entry("org.example.markdown", "1.0.0", NOTES);
    let mut f = Fixture::new();
    // Not in the catalog.
    f.pin(&[dependency("markdown", &notes)], &["storage"], &[]);
    assert_eq!(
        refusals(&f.report(Some(&resolving(&hub, vec![]))).findings, "components"),
        ["component markdown (org.example.markdown 1.0.0) is not in the catalog"]
    );
    // Another digest than the catalog's.
    let mut wrong = dependency("markdown", &notes);
    wrong.blake3 = blake3_hex(FETCH);
    f.pin(&[wrong], &["storage"], &[]);
    let refused = refusals(&f.report(Some(&resolving(&hub, vec![notes.clone()]))).findings, "components");
    assert_eq!(
        refused,
        [format!(
            "component markdown (org.example.markdown 1.0.0) pins blake3 {}, but the catalog's file hashes to {}",
            blake3_hex(FETCH),
            blake3_hex(NOTES)
        )]
    );
    // Withdrawn.
    let mut withdrawn = notes.clone();
    withdrawn.status = Status::Withdrawn("Renders scripts it should escape".into());
    f.pin(&[dependency("markdown", &notes)], &["storage"], &[]);
    assert_eq!(
        refusals(&f.report(Some(&resolving(&hub, vec![withdrawn]))).findings, "components"),
        ["component markdown (org.example.markdown 1.0.0) was withdrawn: Renders scripts it should escape"]
    );
    // Without a catalog the gate cannot tell: a warning says how to check.
    let report = f.report(None);
    assert!(report.passed(), "{}", report.render());
    assert_eq!(
        warnings(&report.findings, "components"),
        ["component markdown (org.example.markdown 1.0.0) is not resolved: check with --catalog to see that it is offered, matches its digest and what it reaches"]
    );
}

#[test]
fn the_feature_without_components_is_a_warning() {
    let mut f = Fixture::new();
    f.pin(&[], &[], &[]);
    let report = f.report(None);
    assert!(report.passed(), "{}", report.render());
    assert_eq!(
        warnings(&report.findings, "components"),
        ["the manifest requires wasm-shared-components-v1 but names no components; hosts without it refuse the app"]
    );
}

// ---- components in a bundle ---------------------------------------------------------

fn bundled_file(f: &Fixture, wasm: &[u8]) -> String {
    let name = format!("components/{}.wasm", blake3_hex(wasm));
    fs::create_dir_all(f.bundle.join("components")).unwrap();
    fs::write(f.bundle.join(&name), wasm).unwrap();
    name
}

#[test]
fn a_store_app_never_ships_components_and_a_system_app_ships_each_it_pins() {
    let notes = component_entry("org.example.markdown", "1.0.0", NOTES);
    // A store app's components come from the catalog.
    let mut f = Fixture::new();
    let name = bundled_file(&f, NOTES);
    f.pin(&[dependency("markdown", &notes)], &["storage"], &[]);
    let report = f.report(None);
    let refused: Vec<&Finding> = report.findings.iter().filter(|f| f.check == "components" && f.severity == Severity::Refusal).collect();
    assert_eq!(refused.len(), 1, "{}", report.render());
    assert_eq!(refused[0].path.as_deref(), Some(name.as_str()));
    assert_eq!(refused[0].detail, "components/ ships only in a system app's bundle; a store app's components come from the catalog");

    // A system app ships what it pins, checked like a catalog component.
    let mut f = Fixture::new();
    f.manifest.id = "os.notesdemo".into();
    f.manifest.integrity.signature = None;
    bundled_file(&f, NOTES);
    f.manifest.components = vec![dependency("markdown", &notes)];
    f.manifest.requires = vec!["wasm-shared-components-v1".into()];
    f.manifest.capabilities = vec!["wasm".into(), "storage".into()];
    let system = |f: &mut Fixture| {
        f.manifest.integrity.signature = None;
        f.manifest.integrity.bundle_blake3 = octosense_app_policy::digest_dir(&f.bundle).unwrap();
        f.write_manifest();
        gate::check_system_bundle(&f.bundle, &RefuseAllSignatures).unwrap()
    };
    let report = system(&mut f);
    assert!(report.passed(), "{}", report.render());
    assert_eq!(
        warnings(&report.findings, "components"),
        ["component markdown (org.example.markdown 1.0.0) reaches the clock, random numbers and files in its app folder, but no network or other app"]
    );
    // A file it does not name, and one that is missing.
    let extra = bundled_file(&f, HOSTCALL);
    let report = system(&mut f);
    let refused: Vec<&Finding> = report.findings.iter().filter(|x| x.check == "components" && x.severity == Severity::Refusal).collect();
    assert_eq!(refused.len(), 1, "{}", report.render());
    assert_eq!((refused[0].path.as_deref(), refused[0].detail.as_str()), (Some(extra.as_str()), "this file is not one of the manifest's components"));
    fs::remove_file(f.bundle.join(&extra)).unwrap();
    fs::remove_dir_all(f.bundle.join("components")).unwrap();
    let report = system(&mut f);
    assert_eq!(
        refusals(&report.findings, "components"),
        [format!("component markdown (org.example.markdown 1.0.0) is not in the bundle: a system app ships it as components/{}.wasm", blake3_hex(NOTES))]
    );
    // A bundled file must be named by its own digest.
    fs::create_dir_all(f.bundle.join("components")).unwrap();
    fs::write(f.bundle.join(format!("components/{}.wasm", blake3_hex(NOTES))), FETCH).unwrap();
    let report = system(&mut f);
    assert!(
        refusals(&report.findings, "contents-invalid").iter().any(|d| d.contains("not the digest its name gives")),
        "{}",
        report.render()
    );
}

// ---- the catalog --------------------------------------------------------------------

#[test]
fn a_catalog_without_components_reads_signs_and_attests_as_before() {
    // The legacy catalog: same signing bytes, still verified by its anchor.
    let legacy: Catalog = serde_json::from_str(include_str!("../../../catalog.json")).unwrap();
    assert!(legacy.components.is_empty());
    assert!(!serde_json::to_string(&legacy).unwrap().contains("\"components\""));
    verify_catalog(&legacy, DEFAULT_ANCHOR).unwrap();
    // The attested v2 payload: re-serialised, byte for byte the attested bytes.
    use base64::Engine;
    let envelope: serde_json::Value = serde_json::from_str(include_str!("../../../catalog-v2.json")).unwrap();
    let payload = base64::engine::general_purpose::STANDARD.decode(envelope["catalog"].as_str().unwrap()).unwrap();
    let parsed: Catalog = serde_json::from_slice(&payload).unwrap();
    assert!(parsed.components.is_empty());
    assert_eq!(serde_json::to_vec(&parsed).unwrap(), payload);
}

#[test]
fn a_signed_catalog_records_components_beside_apps_and_refuses_contradictions() {
    let hub = Hub::new();
    let root = std::env::temp_dir().join(format!("hub-components-catalog-{}", std::process::id()));
    let notes = component_entry("org.example.markdown", "1.0.0", NOTES);
    let mut store = Store::new(&hub.anchor.public_hex(), &root, HostLimits::default());
    store.accept_catalog(&hub.json(1, vec![], vec![notes.clone()])).unwrap();
    assert_eq!(store.component("org.example.markdown", "1.0.0"), Some(&notes));
    assert_eq!(store.catalog().unwrap().component("org.example.markdown", "1.0.0"), Some(&notes));
    assert!(store.component("org.example.markdown", "1.0.1").is_none());
    // The same version twice, an id that is also an app's, and a
    // noncanonical artifact are refused even under a valid signature.
    let mut fresh = Store::new(&hub.anchor.public_hex(), &root, HostLimits::default());
    let err = fresh.accept_catalog(&hub.json(2, vec![], vec![notes.clone(), notes.clone()])).unwrap_err();
    assert_eq!(err, "catalog repeats component org.example.markdown 1.0.0");
    let app = Fixture::new();
    let clash = component_entry(app.manifest.id.as_str(), "1.0.0", NOTES);
    let err = fresh.accept_catalog(&hub.json(2, vec![app.entry()], vec![clash])).unwrap_err();
    assert_eq!(err, "example-app is both an app and a component in the catalog");
    let mut moved = notes.clone();
    moved.artifact = "artifacts/elsewhere.wasm".into();
    let err = fresh.accept_catalog(&hub.json(2, vec![], vec![moved])).unwrap_err();
    assert_eq!(err, "component org.example.markdown 1.0.0 has a noncanonical artifact path");
    // A changed component breaks the catalog signature like any entry.
    let mut catalog = hub.catalog(3, vec![], vec![notes]);
    catalog.components[0].component.wasm_blake3 = blake3_hex(FETCH);
    assert!(verify_catalog(&catalog, &hub.anchor.public_hex()).is_err());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_shared_socket_component_propagates_its_abi_to_the_app() {
    let hub = Hub::new();
    let sockets = component_entry("org.example.netprobe", "1.0.0", NETPROBE);
    let catalog = resolving(&hub, vec![sockets.clone()]);
    let mut f = Fixture::new();
    f.pin(&[dependency("network", &sockets)], &[], &[]);
    let report = f.report(Some(&catalog));
    assert!(refusals(&report.findings, "components").iter().any(|detail| detail.contains("requires host_api.required wasm.sockets@1")), "{}", report.render());
    f.socket_api();
    let report = f.report(Some(&catalog));
    assert!(report.passed(), "{}", report.render());
    assert!(warnings(&report.findings, "components").iter().any(|detail| detail.contains("imports wasi:sockets; disclose net usage")));
    assert!(warnings(&report.findings, "components").iter().any(|detail| detail.contains("reaches") && detail.contains("the network")));
}

#[test]
fn a_system_bundle_socket_component_requires_the_same_host_abi() {
    let sockets = component_entry("org.example.netprobe", "1.0.0", NETPROBE);
    let mut f = Fixture::new();
    f.manifest.id = "os.networkdemo".into();
    bundled_file(&f, NETPROBE);
    f.pin(&[dependency("network", &sockets)], &[], &[]);
    let system = |f: &mut Fixture| {
        f.manifest.integrity.signature = None;
        f.manifest.integrity.bundle_blake3 = octosense_app_policy::digest_dir(&f.bundle).unwrap();
        f.write_manifest();
        gate::check_system_bundle(&f.bundle, &RefuseAllSignatures).unwrap()
    };
    let report = system(&mut f);
    assert!(refusals(&report.findings, "components").iter().any(|detail| detail.contains("requires host_api.required wasm.sockets@1")), "{}", report.render());
    f.socket_api();
    let report = system(&mut f);
    assert!(report.passed(), "{}", report.render());
}
