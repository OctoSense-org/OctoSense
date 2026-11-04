//! The Store consent text must include own tools even with no generic grants.
mod common;
use common::Fixture;
use octosense_app_hub::*;
use octosense_app_policy::{AgentBundle, AppManifest, HostLimits};
use serde_json::{json, Value};
use std::fs;

fn connected(app: &str, family: &str, methods: &[(&str, &str)]) -> Fixture {
    let mut fixture = Fixture::new();
    let mut manifest = serde_json::to_value(&fixture.manifest).unwrap();
    manifest["id"] = json!(format!("org.example.{app}"));
    manifest["capabilities"] = json!([family]);
    manifest["agent"] = json!({"profile":"read-only", "tools":[], "model":{"needs":["tool_calling"]}});
    fixture.manifest = AppManifest::parse(&manifest.to_string()).unwrap();
    let tools: Vec<Value> = methods.iter().map(|(name, method)| json!({
        "name":format!("{app}.{name}"), "description":"Read this app's connected data.",
        "input_schema":{"type":"object"}, "output_schema":{"type":"object"},
        "risk":"read", "background":false, "shareable":false, "private_data":true,
        "implemented_by":"host-service", "host_method":method
    })).collect();
    fs::write(fixture.bundle.join("tools.json"), json!({"schema":1,"tools":tools}).to_string()).unwrap();
    fixture.sign();
    fixture
}

#[test]
fn store_consent_discloses_signed_own_read_tools_with_no_generic_grants() {
    for fixture in [
        connected("notespreview", "github", &[("repositories", "github.repositories"), ("files", "github.files"), ("read", "github.read")]),
        connected("agendapreview", "gcalendar", &[("calendars", "gcalendar.calendars"), ("cached", "gcalendar.cached"), ("refresh", "gcalendar.refresh"), ("event", "gcalendar.get")]),
    ] {
        let original_tools = fs::read(fixture.bundle.join("tools.json")).unwrap();
        let loaded = AgentBundle::load(&fixture.bundle, &fixture.manifest).unwrap().unwrap();
        assert!(loaded.generic_tools.is_empty());
        assert!(!loaded.tools.is_empty());
        let entry = fixture.entry();
        // The same display projection used by real catalogs has no dispatch
        // bindings, but retains the own-tool names needed by the Store.
        assert!(entry.tools.iter().all(|tool| tool.host_method.is_none()));
        let anchor = HubKey::generate();
        let working = HubKey::generate();
        let mut catalog = Catalog::new(1, "2026-10-07", vec![entry]);
        working.sign_catalog(&mut catalog, &anchor.certify(&working.public_hex()).unwrap()).unwrap();
        let mut store = Store::new(&anchor.public_hex(), &fixture.root.join("store"), HostLimits::default());
        store.accept_catalog(&serde_json::to_string(&catalog).unwrap()).unwrap();
        let listing = store.listings().pop().unwrap();
        assert!(listing.permissions.iter().any(|line| line.contains("only after you allow it")));
        let own = listing.permissions.iter().find(|line| line.starts_with("Its assistant can use these app tools:")).unwrap();
        for tool in &loaded.tools {
            assert!(own.contains(&tool.name), "missing {}: {own}", tool.name);
        }
        assert!(!listing.permissions.iter().any(|line| line.contains("no tools") || line.contains("additional tools")));
        assert_eq!(fs::read(fixture.bundle.join("tools.json")).unwrap(), original_tools);
    }
}

#[test]
fn own_tools_and_additional_requests_stay_distinct_in_consent() {
    let fixture = connected("notespreview", "github", &[("read", "github.read")]);
    let mut entry = fixture.entry();
    // Display already-reviewed host and cross-app requests; this test neither
    // offers that tool at admission nor grants or executes it.
    let mut manifest = serde_json::to_value(&entry.manifest).unwrap();
    manifest["agent"]["tools"] = json!(["ask_user_question", "net.fetch", "calendar.list"]);
    entry.manifest = AppManifest::parse(&manifest.to_string()).unwrap();
    let lines = entry.permissions_summary();
    assert!(lines.contains(&"Its assistant can use these app tools: notespreview.read".into()));
    assert!(lines.contains(&"Its assistant requests these additional tools: net.fetch, calendar.list".into()));
    assert_eq!(lines.iter().filter(|line| *line == "Ask you questions").count(), 1);
    assert!(!lines.iter().any(|line| line.contains("ask_user_question") || line.contains("no tools") || line.contains("other apps' tools")));
}
