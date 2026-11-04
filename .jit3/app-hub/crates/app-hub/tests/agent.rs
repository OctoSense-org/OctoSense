//! The gate and the pack over a bundle that ships its own agent: tools.json,
//! AGENT.md and a skill (ADR 0002 §3, §4). The bundle is the News example
//! from `crates/app-policy/tests/fixtures/news-agent`.
use octosense_app_hub::pack::{pack_system_app, Pack};
use octosense_app_hub::{check_bundle, entry_for, unpack, Severity};
use octosense_app_policy::{AgentBundle, AppManifest, HostLimits, RefuseAllSignatures, MANIFEST_FILE, TOOLS_FILE};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../app-policy/tests/fixtures/news-agent")
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("app-hub-agent-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy(&fixture(), &dir);
    dir
}

fn stamp(dir: &Path) {
    let path = dir.join(MANIFEST_FILE);
    let mut manifest: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    manifest["integrity"]["bundle_blake3"] = Value::String(octosense_app_policy::digest_dir(dir).unwrap());
    std::fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
}

fn limits() -> HostLimits {
    HostLimits::default().with_require_signature(false)
}

/// Findings of the checks this change adds, plus the policy and assets
/// checks that now look at agent files. The listing and signature findings
/// of a fixture without a listing are not the point here.
fn agent_findings(dir: &Path) -> Vec<(Severity, String)> {
    check_bundle(dir, &limits(), &RefuseAllSignatures, None)
        .unwrap()
        .findings
        .into_iter()
        .filter(|f| matches!(f.check, "tools" | "agent" | "skills" | "policy" | "assets" | "contents" | "digest"))
        .map(|f| (f.severity, format!("{}: {}", f.check, f.detail)))
        .collect()
}

#[test]
fn the_gate_admits_the_news_agent_and_records_destructive_tools() {
    let dir = scratch("admit");
    stamp(&dir);
    let found = agent_findings(&dir);
    assert!(found.iter().all(|(s, _)| *s == Severity::Warning), "{found:?}");

    // Add a destructive tool: still admitted, and the report records that
    // every call waits for approval.
    let path = dir.join(TOOLS_FILE);
    let mut tools: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    tools["tools"].as_array_mut().unwrap().push(json!({
        "name": "news.topics.clear", "description": "Forget every followed topic.",
        "input_schema": {"type":"object","properties":{}}, "output_schema": {"type":"object"},
        "risk": "destructive", "implemented_by": "host-service"
    }));
    std::fs::write(&path, tools.to_string()).unwrap();
    stamp(&dir);
    let found = agent_findings(&dir);
    assert!(found.iter().all(|(s, _)| *s == Severity::Warning), "{found:?}");
    assert!(found.iter().any(|(_, d)| d == "tools: news.topics.clear is destructive: every call waits for the host's approval"), "{found:?}");

    // A tool the app implements may confirm on its own sheet; the report
    // records whose confirmation it is.
    let mut tools: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    tools["tools"].as_array_mut().unwrap().push(json!({
        "name": "news.share", "description": "Share a story.",
        "input_schema": {"type":"object","properties":{}}, "output_schema": {"type":"object"},
        "risk": "destructive", "implemented_by": "app", "confirm": "app"
    }));
    std::fs::write(&path, tools.to_string()).unwrap();
    stamp(&dir);
    let found = agent_findings(&dir);
    assert!(found.iter().all(|(s, _)| *s == Severity::Warning), "{found:?}");
    assert!(found.iter().any(|(_, d)| d.starts_with("tools: news.share is destructive: every call waits for the app's own confirmation sheet")), "{found:?}");
}

#[test]
fn the_gate_refuses_what_the_agent_review_refuses_and_the_digest_pins_agent_files() {
    let dir = scratch("refuse");
    stamp(&dir);
    std::fs::write(dir.join("skills/news-digest/manifest.json"), r#"{"name":"news-digest","version":"1","uses":["mail.send"]}"#).unwrap();
    let found = agent_findings(&dir);
    assert!(found.iter().any(|(s, d)| *s == Severity::Refusal && d.starts_with("digest:")), "the skill is pinned: {found:?}");
    assert!(found.iter().any(|(s, d)| *s == Severity::Refusal && d.contains("uses mail.send")), "{found:?}");

    // Background with no triggers is a policy refusal.
    let path = dir.join(MANIFEST_FILE);
    let mut manifest: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    manifest["agent"]["triggers"] = json!({});
    std::fs::write(&path, manifest.to_string()).unwrap();
    stamp(&dir);
    let found = agent_findings(&dir);
    assert!(found.iter().any(|(s, d)| *s == Severity::Refusal && d.contains("declares no triggers")), "{found:?}");
}

#[test]
fn agent_destinations_are_disclosures_not_host_allowlists() {
    let dir = scratch("hosts");
    std::fs::write(dir.join("AGENT.md"), "# News\n\nPrefer stories from https://feeds.example.org/top when fresh.\n").unwrap();
    stamp(&dir);
    let found = agent_findings(&dir);
    assert!(!found.iter().any(|(s, _)| *s == Severity::Refusal), "{found:?}");

    let path = dir.join(MANIFEST_FILE);
    let mut manifest: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    manifest["capabilities"] = json!(["storage", "net"]);
    manifest["network"] = json!({"hosts": ["feeds.example.org"]});
    std::fs::write(&path, manifest.to_string()).unwrap();
    stamp(&dir);
    let found = agent_findings(&dir);
    assert!(found.iter().all(|(s, _)| *s == Severity::Warning), "a declared host is fine: {found:?}");
}

/// What an entry needs beyond the agent: an app to run, and the store
/// listing the gate admits nothing without.
fn list(dir: &Path) {
    use base64::Engine;
    std::fs::write(dir.join("main.splash"), "Label{text: \"News\"}").unwrap();
    std::fs::write(dir.join("icon.svg"), r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><rect width="64" height="64" fill="#146"/></svg>"##).unwrap();
    let png = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=")
        .unwrap();
    std::fs::write(dir.join("screen.png"), png).unwrap();
    std::fs::write(dir.join("listing.json"), json!({
        "schema": 1, "description": "News with its own agent", "category": "news",
        "screenshots": ["screen.png"], "icon": "icon.svg", "platforms": ["android"],
        "publisher": {"name": "Example", "support": "https://example.test/support", "privacy_policy_url": "https://example.test/privacy"},
        "age_rating": "all"
    }).to_string()).unwrap();
}

#[test]
fn the_catalog_entry_carries_the_tools_and_the_store_lines() {
    let dir = scratch("entry");
    list(&dir);
    stamp(&dir);
    let report = check_bundle(&dir, &limits(), &RefuseAllSignatures, None).unwrap();
    assert!(report.passed(), "{}", report.render());
    let entry = entry_for(&dir, &report, "p", "", "https://example.invalid/r", "c", "2026-09-27").unwrap();
    assert_eq!(entry.tools.len(), 5);
    let lines = entry.permissions_summary();
    assert!(lines.iter().any(|l| l.contains("may work while the app is closed")), "{lines:?}");
    assert!(lines.iter().any(|l| l == "Offers news.list to other assistants you allow."), "{lines:?}");
    // An entry without tools serialises without the field, so its signing
    // bytes are what they were before the field existed.
    let mut bare = entry.clone();
    bare.tools.clear();
    assert!(serde_json::to_value(&bare).unwrap().get("tools").is_none());
    let round: octosense_app_hub::Entry = serde_json::from_str(&serde_json::to_string(&entry).unwrap()).unwrap();
    assert_eq!(round.tools, entry.tools);
}

#[test]
fn a_packed_system_app_pins_its_agent_files_and_loads_after_unpacking() {
    let dir = scratch("pack");
    let path = dir.join(MANIFEST_FILE);
    let mut manifest: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    manifest["id"] = json!("os.news");
    std::fs::write(&path, manifest.to_string()).unwrap();
    let packed = pack_system_app(&dir).unwrap();
    assert_eq!(packed.id, "os.news");
    let pack: Pack = serde_json::from_str(&packed.pack_json).unwrap();
    for file in ["tools.json", "AGENT.md", "skills/news-digest/SKILL.md", "skills/news-digest/manifest.json"] {
        assert!(pack.files.contains_key(file), "{file} is packed");
    }
    let out = dir.with_extension("unpacked");
    let _ = std::fs::remove_dir_all(&out);
    unpack(&pack, &out).unwrap();
    let manifest = AppManifest::parse(&std::fs::read_to_string(out.join(MANIFEST_FILE)).unwrap()).unwrap();
    let agent = AgentBundle::load(&out, &manifest).unwrap().unwrap();
    assert_eq!(agent.namespace, "news");
    assert!(agent.tool("news.list").is_some() && agent.tool("mail.send").is_none());
    // The stamped digest covers the agent files: editing one after packing
    // is refused on load.
    std::fs::write(out.join(TOOLS_FILE), "{\"schema\":1,\"tools\":[]}").unwrap();
    assert!(AgentBundle::load(&out, &manifest).unwrap_err().contains("does not match"));
}

#[test]
fn the_review_packet_shows_the_agent_files_and_asks_about_them() {
    let dir = scratch("scan");
    std::fs::write(dir.join("main.splash"), "Label{text: \"News\"}").unwrap();
    stamp(&dir);
    let report = check_bundle(&dir, &limits(), &RefuseAllSignatures, None).unwrap();
    let packet = octosense_app_hub::scan::packet(&dir, &report).unwrap();
    let files: Vec<&str> = packet.agent_files.keys().map(String::as_str).collect();
    assert_eq!(files, ["AGENT.md", "skills/news-digest/SKILL.md", "skills/news-digest/manifest.json", "tools.json"]);
    assert!(packet.questions.iter().any(|q| q.contains("agent_files") && q.contains("confirm \"app\"")));
    assert!(packet.grants.iter().any(|g| g.contains("may work while the app is closed")), "{:?}", packet.grants);
}

#[test]
fn the_gate_refuses_a_native_apps_id() {
    let dir = scratch("native-id");
    let path = dir.join(MANIFEST_FILE);
    let mut manifest: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    manifest["id"] = json!("terminal");
    std::fs::write(&path, manifest.to_string()).unwrap();
    stamp(&dir);
    let report = check_bundle(&dir, &limits(), &RefuseAllSignatures, None).unwrap();
    let identity: Vec<_> = report.findings.iter().filter(|f| f.check == "identity").collect();
    assert!(
        identity.iter().any(|f| f.severity == Severity::Refusal && f.detail.contains("reserved")),
        "{:?}",
        report.findings
    );
}
