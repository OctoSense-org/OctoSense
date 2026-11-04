//! Catalog display metadata stays readable by stores before host_method.
//! Dispatch still comes from the exact publisher-signed bundle.
mod common;
use common::Fixture;
use octosense_app_hub::*;
use octosense_app_policy::{AgentBundle, HostLimits};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;

// Strict wire shape from app-policy at 966e711 (before connected services).
// Defaults/omissions match that reader so roundtrip equality checks the
// canonical bytes it verifies, not merely permissive JSON parsing.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyTool {
    name: String,
    description: String,
    input_schema: Value,
    output_schema: Value,
    risk: String,
    #[serde(default)]
    background: bool,
    #[serde(default)]
    shareable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    private_data: Option<bool>,
    implemented_by: String,
    #[serde(default = "host", skip_serializing_if = "is_host")]
    confirm: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    outward: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    auto_approvable: bool,
}
fn host() -> String {
    "host".into()
}
fn is_host(value: &String) -> bool {
    value == "host"
}
fn yes() -> bool {
    true
}
fn is_true(value: &bool) -> bool {
    *value
}

fn connected() -> Fixture {
    let mut f = Fixture::new();
    f.manifest.id = "org.example.calendarpreview".into();
    f.manifest.capabilities = vec!["gcalendar".into()];
    fs::write(
        f.bundle.join("tools.json"),
        json!({"schema":1,"tools":[{
            "name":"calendarpreview.cached", "description":"Read its cached agenda.",
            "input_schema":{"type":"object"}, "output_schema":{"type":"object"},
            "risk":"read", "background":false, "shareable":false, "private_data":true,
            "implemented_by":"host-service", "host_method":"gcalendar.cached"
        }]})
        .to_string(),
    )
    .unwrap();
    f.sign();
    f
}

#[test]
fn legacy_catalog_tools_roundtrip_without_changing_signed_bundle_dispatch() {
    let f = connected();
    let original_bytes = fs::read(f.bundle.join("tools.json")).unwrap();
    let original = AgentBundle::load(&f.bundle, &f.manifest).unwrap().unwrap();
    // Demonstrate the actual pre-fix failure: the strict legacy reader
    // refuses this new field, even though the entry is only for display.
    let err =
        serde_json::from_value::<LegacyTool>(serde_json::to_value(&original.tools[0]).unwrap())
            .unwrap_err();
    assert!(
        err.to_string().contains("unknown field `host_method`"),
        "{err}"
    );

    let entry = f.entry();
    let mut expected = original.tools.clone();
    for tool in &mut expected {
        tool.host_method = None;
    }
    assert_eq!(
        entry.tools, expected,
        "all permission and schema fields must survive"
    );
    assert_eq!(
        fs::read(f.bundle.join("tools.json")).unwrap(),
        original_bytes
    );

    let anchor = HubKey::generate();
    let working = HubKey::generate();
    let mut catalog = Catalog::new(1, "2026-10-06", vec![entry]);
    working
        .sign_catalog(
            &mut catalog,
            &anchor.certify(&working.public_hex()).unwrap(),
        )
        .unwrap();
    let wire = serde_json::to_value(&catalog).unwrap();
    let legacy_tools: Vec<LegacyTool> =
        serde_json::from_value(wire["entries"][0]["tools"].clone()).unwrap();
    let mut legacy_wire = wire.clone();
    legacy_wire["entries"][0]["tools"] = serde_json::to_value(legacy_tools).unwrap();
    assert_eq!(
        legacy_wire, wire,
        "legacy defaults must preserve signed JSON"
    );
    let legacy_roundtrip: Catalog = serde_json::from_value(legacy_wire).unwrap();
    assert_eq!(
        legacy_roundtrip.signing_bytes().unwrap(),
        catalog.signing_bytes().unwrap()
    );
    verify_catalog(&legacy_roundtrip, &anchor.public_hex()).unwrap();

    let mut store = Store::new(
        &anchor.public_hex(),
        &f.root.join("installed"),
        HostLimits::default(),
    );
    store
        .accept_catalog(&serde_json::to_string(&catalog).unwrap())
        .unwrap();
    assert!(f.manifest.agent.is_none());
    assert!(!original.tools.is_empty());
    let listing = store.listings().pop().unwrap();
    assert!(listing.privacy.iter().any(|line| line.contains("Ask assistant") && line.contains("consent")));
    assert!(listing.privacy.iter().any(|line| line.contains("configured AI provider")));
    assert!(listing.privacy.iter().any(|line| line.contains("No app-declared background assistant")));
    assert!(!listing.privacy.iter().any(|line| line == "Runs no assistant."));
    store
        .install_staged(&f.manifest.id, &f.bundle, &f.keys(), "2026-10-06")
        .unwrap();
    store.may_run(&f.manifest.id).unwrap();
    let installed = store.install_dir(&f.manifest.id);
    let loaded = AgentBundle::load(&installed, &f.manifest).unwrap().unwrap();
    assert_eq!(
        loaded.tools, original.tools,
        "dispatch must load the original signed bindings"
    );
    assert_eq!(
        loaded.tools[0].host_method.as_deref(),
        Some("gcalendar.cached")
    );

    // Removing or redirecting that binding after install still invalidates
    // the signed package. Display projection must never relax integrity.
    fs::write(installed.join("tools.json"), original_bytes).unwrap();
    let mut tampered: Value =
        serde_json::from_slice(&fs::read(installed.join("tools.json")).unwrap()).unwrap();
    tampered["tools"][0]["host_method"] = json!("gcalendar.get");
    fs::write(installed.join("tools.json"), tampered.to_string()).unwrap();
    assert!(store.may_run(&f.manifest.id).is_err());
    assert!(AgentBundle::load(&installed, &f.manifest).is_err());
}

#[test]
fn display_projection_never_masks_a_forbidden_binding_from_admission() {
    let mut f = connected();
    let path = f.bundle.join("tools.json");
    let mut tools: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    tools["tools"][0]["host_method"] = json!("gcalendar.review_save");
    fs::write(path, tools.to_string()).unwrap();
    f.sign();
    let report = f.report(None);
    assert!(!report.passed());
    assert!(entry_for(
        &f.bundle,
        &report,
        "publisher-one",
        &f.publisher.public_hex(),
        "",
        "",
        "2026-10-06"
    )
    .is_err());
}
