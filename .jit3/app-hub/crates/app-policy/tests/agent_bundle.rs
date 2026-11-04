//! An app's own agent in its bundle (ADR 0002 §3, §4, §12): the tool
//! manifest, AGENT.md, skills, model requirements and triggers. The fixture
//! under `tests/fixtures/news-agent` is the News example the docs show.
use octosense_app_policy::agent::{check_instruction_text, check_schema, review, Issue};
use octosense_app_policy::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/news-agent")
}

/// A scratch copy of the News fixture, to mutate.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("app-policy-agent-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy(&fixture(), &dir);
    dir
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

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn write_json(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

/// Change the manifest, stamp the digest of the directory as it is now, and
/// parse it.
fn stamp(dir: &Path, edit: impl FnOnce(&mut Value)) -> AppManifest {
    let path = dir.join(MANIFEST_FILE);
    let mut manifest = read_json(&path);
    edit(&mut manifest);
    manifest["integrity"]["bundle_blake3"] = Value::String(digest_dir(dir).unwrap());
    write_json(&path, &manifest);
    AppManifest::parse(&manifest.to_string()).unwrap()
}

fn edit_tools(dir: &Path, edit: impl FnOnce(&mut Value)) {
    let path = dir.join(TOOLS_FILE);
    let mut tools = read_json(&path);
    edit(&mut tools);
    write_json(&path, &tools);
}

fn refusals(dir: &Path, manifest: &AppManifest) -> Vec<Issue> {
    review(dir, manifest).issues.into_iter().filter(|i| i.refusal).collect()
}

fn refused_with(dir: &Path, manifest: &AppManifest, needle: &str) {
    let found = refusals(dir, manifest);
    assert!(found.iter().any(|i| i.detail.contains(needle)), "expected a refusal containing {needle:?}, got {found:?}");
}

fn open_limits() -> HostLimits {
    HostLimits::default().with_require_signature(false)
}

// ------------------------------------------------------------------- loading

#[test]
fn the_news_example_loads_as_an_agent_bundle() {
    let dir = scratch("news");
    let manifest = stamp(&dir, |_| {});
    let review = review(&dir, &manifest);
    assert_eq!(review.refusals().count(), 0, "{:?}", review.issues);
    let bundle = AgentBundle::load(&dir, &manifest).unwrap().expect("an agent");
    assert_eq!(bundle.namespace, "news");
    assert_eq!(
        bundle.tool_names(),
        ["news.list", "news.read", "news.topics.get", "news.topics.set", "news.digest.write"]
    );
    assert!(bundle.agent_md.as_deref().unwrap().starts_with("# News agent"));
    assert_eq!(bundle.skills.len(), 1);
    let skill = &bundle.skills[0];
    assert_eq!(skill.name, "news-digest");
    assert!(skill.files.contains_key("SKILL.md") && skill.files.contains_key("manifest.json"));
    assert_eq!(skill.manifest.uses, ["news.list", "news.read", "news.digest.write"]);
    assert!(bundle.background);
    assert_eq!(bundle.triggers.schedule, ["0 7 * * *", "0 19 * * *"]);
    assert_eq!(bundle.triggers.events, ["news.items.new"]);
    assert_eq!(bundle.model.per_task["triage"].tier, ModelTier::Fast);
    assert_eq!(bundle.model.per_task["synthesis"].tier, ModelTier::Strong);

    let get = bundle.tool("news.topics.get").unwrap();
    assert_eq!((get.local_name(), get.broker_name().as_str()), ("topics.get", "topics_get"));
    assert_eq!(get.risk.broker_name(), "Read");
    assert_eq!(get.implemented_by, ImplementedBy::HostService);
    assert!(get.runs_unattended_in_background());
}

/// The gate's review (and so `AgentBundle::load`) keeps `ask_user_question`
/// as the agent's one kernel tool and refuses any other.
#[test]
fn the_agent_keeps_ask_user_question_and_no_other_kernel_tool() {
    let dir = scratch("kernel-tools");
    let manifest = stamp(&dir, |m| m["agent"]["tools"] = json!(["ask_user_question"]));
    let bundle = AgentBundle::load(&dir, &manifest).unwrap().expect("an agent");
    assert_eq!(bundle.kernel_tools(), ["ask_user_question"]);
    assert!(bundle.tool_names().contains(&"ask_user_question".to_string()));
    for tool in ["shell", "web_search", "peer_send_input"] {
        let manifest = stamp(&dir, |m| m["agent"]["tools"] = json!(["ask_user_question", tool]));
        refused_with(&dir, &manifest, &format!("octos kernel tool {tool:?}"));
        assert!(AgentBundle::load(&dir, &manifest).is_err(), "{tool}");
    }
}

#[test]
fn the_policy_carries_the_model_background_and_triggers() {
    let dir = scratch("policy");
    let manifest = stamp(&dir, |m| {
        m["agent"]["model"]["needs"] = json!(["reasoning", "tool_calling", "reasoning"]);
    });
    let policy = policy::resolve(&manifest, &open_limits()).unwrap();
    let agent = policy.agent.unwrap();
    assert!(agent.background_requested);
    assert_eq!(agent.model.needs, [ModelNeed::ToolCalling, ModelNeed::Reasoning], "sorted and deduplicated");
    assert_eq!(agent.triggers.events, ["news.items.new"]);
    assert_eq!(agent.instructions.as_deref(), Some("AGENT.md"));
    assert!(agent.skills.contains("news-digest"));
}

#[test]
fn load_refuses_a_bundle_changed_after_stamping() {
    let dir = scratch("tamper");
    let manifest = stamp(&dir, |_| {});
    // Every agent file is pinned: touching any of them breaks the digest.
    for file in [TOOLS_FILE, AGENT_FILE, "skills/news-digest/SKILL.md", "skills/news-digest/manifest.json"] {
        let path = dir.join(file);
        let original = std::fs::read(&path).unwrap();
        let mut changed = original.clone();
        changed.extend_from_slice(b"\n");
        std::fs::write(&path, &changed).unwrap();
        let err = AgentBundle::load(&dir, &manifest).unwrap_err();
        assert!(err.contains("does not match"), "{file}: {err}");
        std::fs::write(&path, original).unwrap();
    }
    assert!(AgentBundle::load(&dir, &manifest).is_ok());
}

#[test]
fn an_app_without_agent_or_tools_has_no_agent_bundle() {
    let dir = std::env::temp_dir().join(format!("app-policy-agent-none-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("page.card"), "card").unwrap();
    write_json(&dir.join(MANIFEST_FILE), &json!({"schema":1,"id":"quiet","version":"1","name":"Q","integrity":{"bundle_blake3":""}}));
    let manifest = stamp(&dir, |_| {});
    assert_eq!(AgentBundle::load(&dir, &manifest).unwrap(), None);
}

// --------------------------------------------------------- manifest schema 1

#[test]
fn a_manifest_written_before_the_agent_additions_signs_the_same_way() {
    // The new fields are skipped at their defaults, so the canonical bytes a
    // publisher signed before they existed are the bytes computed now.
    let legacy = r#"{"schema":1,"id":"forecast","version":"1.0.0","name":"Forecast","integrity":{"bundle_blake3":"00"},
        "agent":{"profile":"read-only","tools":["ledger.read"],"max_iterations":2}}"#;
    let bytes = String::from_utf8(AppManifest::parse(legacy).unwrap().signing_bytes().unwrap()).unwrap();
    assert_eq!(
        bytes,
        r#"{"agent":{"max_iterations":2,"profile":"read-only","token_budget":null,"tools":["ledger.read"]},"capabilities":[],"compute":{"instruction_budget":null,"memory_bytes":null},"id":"forecast","integrity":{"bundle_blake3":"00","signature":null},"name":"Forecast","network":{"hosts":[]},"schema":1,"storage":{"max_bytes":null},"version":"1.0.0"}"#
    );
}

#[test]
fn model_needs_and_tiers_are_closed_vocabularies() {
    let base = |model: Value| {
        json!({"schema":1,"id":"news","version":"1","name":"N","integrity":{"bundle_blake3":""},
               "agent":{"profile":"read-only","model":model}})
        .to_string()
    };
    assert!(AppManifest::parse(&base(json!({"needs":["telepathy"]}))).unwrap_err().contains("not valid"));
    assert!(AppManifest::parse(&base(json!({"tier":"huge"}))).unwrap_err().contains("not valid"));
    assert!(AppManifest::parse(&base(json!({"provider":"openai"}))).unwrap_err().contains("not valid"), "no model names");
    // local_only is app-wide: a task cannot relax it.
    assert!(AppManifest::parse(&base(json!({"per_task":{"triage":{"local_only":false}}}))).unwrap_err().contains("not valid"));
    for need in KNOWN_MODEL_NEEDS {
        assert!(AppManifest::parse(&base(json!({"needs":[need]}))).is_ok(), "{need}");
    }
    let bad_task = AppManifest::parse(&base(json!({"per_task":{"Triage!":{}}}))).unwrap();
    assert!(policy::resolve(&bad_task, &open_limits()).unwrap_err().contains("model task"));
}

#[test]
fn background_requires_triggers_and_triggers_are_well_formed() {
    let resolve = |agent: Value| {
        let m = AppManifest::parse(
            &json!({"schema":1,"id":"os.news","version":"1","name":"N","integrity":{"bundle_blake3":""},"agent":agent}).to_string(),
        )
        .unwrap();
        policy::resolve(&m, &open_limits())
    };
    let err = resolve(json!({"profile":"read-only","background":true})).unwrap_err();
    assert!(err.contains("declares no triggers"), "{err}");
    assert!(resolve(json!({"profile":"read-only","background":true,"triggers":{"schedule":["*/30 6-22 * * 1-5"]}})).is_ok());
    let err = resolve(json!({"profile":"read-only","triggers":{"schedule":["every morning"]}})).unwrap_err();
    assert!(err.contains("five cron fields"), "{err}");
    let err = resolve(json!({"profile":"read-only","triggers":{"events":["mail.new"]}})).unwrap_err();
    assert!(err.contains("news.<name>"), "an app wakes on its own events only: {err}");
    // Triggers without background are fine: they fire while the app is open.
    assert!(resolve(json!({"profile":"read-only","triggers":{"events":["news.items.new"]}})).is_ok());
    let err = resolve(json!({"profile":"read-only","instructions":"../AGENT.md"})).unwrap_err();
    assert!(err.contains("bundle-relative"), "{err}");
}

// ------------------------------------------------------------------- tools.json

#[test]
fn every_tool_is_in_the_apps_namespace() {
    let dir = scratch("namespace");
    edit_tools(&dir, |t| t["tools"][0]["name"] = json!("mail.list"));
    let manifest = stamp(&dir, |_| {});
    refused_with(&dir, &manifest, "outside the app's namespace");
    edit_tools(&dir, |t| t["tools"][0]["name"] = json!("news"));
    refused_with(&dir, &manifest, "must be named news.<tool>");
    edit_tools(&dir, |t| t["tools"][0]["name"] = json!("news.List"));
    refused_with(&dir, &manifest, "segments of [a-z0-9_]");
}

#[test]
fn shared_host_method_alias_is_pinned_without_a_family_declaration() {
    let dir = scratch("shared-host-method");
    edit_tools(&dir, |tools| {
        tools["tools"][0]["host_method"] = json!("gcalendar.cached");
        tools["tools"][0]["private_data"] = json!(true);
        tools["tools"][0]["shareable"] = json!(false);
    });
    let manifest = stamp(&dir, |_| {});
    assert!(manifest.capabilities.iter().all(|c| c != "gcalendar"));
    let loaded = AgentBundle::load(&dir, &manifest).unwrap().unwrap();
    let tool = loaded.tool("news.list").unwrap();
    assert_eq!(tool.service_method(), "gcalendar.cached");
    assert_eq!(
        tool.name, "news.list",
        "alias never changes owner namespace"
    );
    edit_tools(&dir, |tools| {
        tools["tools"][0]["host_method"] = json!("gmail.messages")
    });
    assert!(AgentBundle::load(&dir, &manifest)
        .unwrap_err()
        .contains("digest"));
}

#[test]
fn host_api_read_aliases_require_privacy_but_not_family_declarations() {
    for method in [
        "runtime.list",
        "runtime.describe",
        "model.capabilities",
        "model.video.status",
        "auth.backend.me",
        "auth.backend.request",
        "camera.permission.status",
        "microphone.permission.status",
        "location.permission.status",
        "location.get",
        "device_calendar.permission.status",
        "device_calendar.calendars.list",
        "device_calendar.events.list",
        "device_calendar.events.get",
        "mail.compose_status",
    ] {
        let dir = scratch(&format!("host-api-read-{method}"));
        edit_tools(&dir, |tools| {
            tools["tools"][0]["host_method"] = json!(method);
            tools["tools"][0]["private_data"] = json!(true);
            tools["tools"][0]["shareable"] = json!(false);
        });
        let manifest = stamp(&dir, |_| {});
        let loaded = AgentBundle::load(&dir, &manifest).unwrap().unwrap();
        let tool = loaded.tool("news.list").unwrap();
        assert_eq!(tool.service_method(), method);
        assert_eq!(tool.risk, Risk::Read);
        assert!(!tool.shareable, "admission must not expand sharing");
        edit_tools(&dir, |tools| tools["tools"][0]["private_data"] = json!(false));
        let manifest = stamp(&dir, |_| {});
        assert!(AgentBundle::load(&dir, &manifest).unwrap_err().contains("private_data: true"), "{method}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn host_aliases_cannot_target_script_implementations_sheets_or_unreviewed_writes() {
    let base = || {
        json!({"schema":1,"tools":[{
            "name":"inbox.messages","description":"Read the connected inbox.",
            "input_schema":{"type":"object"},"output_schema":{"type":"object"},
            "risk":"read","private_data":true,"implemented_by":"host-service","host_method":"gmail.messages"
        }]})
    };
    let valid =
        ToolManifest::load(&base().to_string(), "inbox", ToolHost::Contained, false).unwrap();
    assert_eq!(valid.0.tools[0].service_method(), "gmail.messages");
    let mut app = base();
    app["tools"][0]["implemented_by"] = json!("app");
    assert!(
        ToolManifest::load(&app.to_string(), "inbox", ToolHost::Contained, false)
            .unwrap_err()
            .contains("only valid")
    );
    for method in [
        "gmail.sheet.send",
        "gmail.sheet",
        "auth.connect",
        "auth.disconnect",
        "auth.accounts",
        "auth.active",
        "auth.select",
        "auth.backend.sheet.approve",
        "camera.permission.request",
        "audio.play",
        "audio.status",
        "audio.stop",
        "microphone.record_start",
        "microphone.record_status",
        "microphone.record_stop",
        "microphone.record_cancel",
        "files.pick_photo",
        "files.share",
        "microphone.permission.request",
        "location.permission.request",
        "camera.permission.revoke",
        "microphone.permission.revoke",
        "location.permission.revoke",
        "device_calendar.permission.request",
        "device_calendar.permission.revoke",
        "device_calendar.calendars.select",
        "device_calendar.events.create",
        "device_calendar.events.update",
        "device_calendar.events.delete",
        "app_tools.dispatch",
        "app_policy.device_consent",
        "gmail.send",
        "mail.send",
        "mail.review_send",
        "gmail.draft.review",
        "gcalendar.review_save",
        "github.review_save",
        "github.save",
        "unknown.read",
        "gmail..message",
        "Gmail.message",
        "gmail/message",
        "gmail.message?scope=all",
        "gmail.message\n",
        "gmail.message.extra",
    ] {
        let mut tool = base();
        tool["tools"][0]["host_method"] = json!(method);
        assert!(
            ToolManifest::load(&tool.to_string(), "inbox", ToolHost::Contained, false).is_err(),
            "{method}"
        );
    }
}

#[test]
fn shared_local_mutations_keep_risk_and_private_data_floors() {
    let make = |method: &str, risk: &str, private: bool| {
        json!({"schema":1,"tools":[{
            "name":"inbox.action","description":"Run a scoped inbox action.",
            "input_schema":{"type":"object"},"output_schema":{"type":"object"},
            "risk":risk,"private_data":private,"implemented_by":"host-service","host_method":method
        }]})
        .to_string()
    };
    for method in [
        "gmail.draft.open",
        "mail.compose",
        "model.image",
        "model.audio",
        "model.embeddings",
        "model.video",
        "model.video.cancel",
        "gmail.draft.edit",
        "gmail.event.decide",
        "glance.publish",
        "glance.withdraw",
    ] {
        assert!(ToolManifest::load(
            &make(method, "read", true),
            "inbox",
            ToolHost::Contained,
            false
        )
        .unwrap_err()
        .contains("at least act"));
        assert!(ToolManifest::load(
            &make(method, "act", true),
            "inbox",
            ToolHost::Contained,
            false
        )
        .is_ok());
        assert!(ToolManifest::load(
            &make(method, "act", false),
            "inbox",
            ToolHost::Contained,
            false
        )
        .unwrap_err()
        .contains("private_data: true"));
    }
    assert!(ToolManifest::load(
        &make("gmail.event.status", "read", true),
        "inbox",
        ToolHost::Contained,
        false
    )
    .is_ok());
    assert!(ToolManifest::load(
        &make("gmail.message", "read", false),
        "inbox",
        ToolHost::Contained,
        false
    )
    .unwrap_err()
    .contains("private_data: true"));
}

#[test]
fn an_apps_own_wasm_function_is_a_reviewed_host_method() {
    // `wasm.<function>` runs the bundle's own module, so it is in no shared
    // list and has no risk or private-data floor.
    let make = |method: &str| {
        json!({"schema":1,"tools":[{
            "name":"inbox.slots","description":"Find free slots in a day.",
            "input_schema":{"type":"object"},"output_schema":{"type":"object"},
            "risk":"read","implemented_by":"host-service","host_method":method
        }]})
        .to_string()
    };
    let (tools, _) =
        ToolManifest::load(&make("wasm.find_slots"), "inbox", ToolHost::Contained, false).unwrap();
    assert_eq!(tools.tools[0].service_method(), "wasm.find_slots");
    for method in ["wasm.sheet", "wasm.find_slots.extra", "wasm.Find", "wasm."] {
        assert!(
            ToolManifest::load(&make(method), "inbox", ToolHost::Contained, false).is_err(),
            "{method}"
        );
    }
    let dir = scratch("wasm-host-method");
    edit_tools(&dir, |tools| {
        tools["tools"][0]["host_method"] = json!("wasm.find_slots");
    });
    let manifest = stamp(&dir, |_| {});
    assert!(AgentBundle::load(&dir, &manifest).is_ok());
    let manifest = stamp(&dir, |m| {
        m["capabilities"].as_array_mut().unwrap().push(json!("wasm"))
    });
    let loaded = AgentBundle::load(&dir, &manifest).unwrap().unwrap();
    assert_eq!(loaded.tool("news.list").unwrap().service_method(), "wasm.find_slots");
}

#[test]
fn omitted_host_method_keeps_legacy_serialization_and_dispatch() {
    let original = read_json(&fixture().join(TOOLS_FILE));
    let tools = ToolManifest::parse(&original.to_string()).unwrap();
    let serialized = serde_json::to_value(&tools).unwrap();
    for (tool, value) in tools
        .tools
        .iter()
        .zip(serialized["tools"].as_array().unwrap())
    {
        assert!(tool.host_method.is_none());
        assert_eq!(tool.service_method(), tool.name);
        assert!(!value.as_object().unwrap().contains_key("host_method"));
    }
}

#[test]
fn an_app_whose_short_id_the_broker_cannot_name_may_not_declare_tools() {
    let dir = scratch("short-id");
    let manifest = stamp(&dir, |m| m["id"] = json!("dev.example.news-reader"));
    refused_with(&dir, &manifest, "namespace \"news-reader\"");
}

#[test]
fn tools_colliding_as_broker_names_or_declared_twice_are_refused() {
    let dir = scratch("collide");
    edit_tools(&dir, |t| {
        let mut twin = t["tools"][2].clone();
        twin["name"] = json!("news.topics_get");
        t["tools"].as_array_mut().unwrap().push(twin);
        let dup = t["tools"][0].clone();
        t["tools"].as_array_mut().unwrap().push(dup);
    });
    let manifest = stamp(&dir, |_| {});
    refused_with(&dir, &manifest, "collides with another tool as \"topics_get\"");
    refused_with(&dir, &manifest, "news.list is declared twice");
}

#[test]
fn a_tool_without_risk_or_with_an_unknown_field_is_refused() {
    let dir = scratch("risk");
    edit_tools(&dir, |t| {
        t["tools"][0].as_object_mut().unwrap().remove("risk");
    });
    let manifest = stamp(&dir, |_| {});
    refused_with(&dir, &manifest, "missing field `risk`");
    edit_tools(&dir, |t| {
        t["tools"][0]["risk"] = json!("read");
        t["tools"][0]["confirmed_by_app"] = json!(true);
    });
    refused_with(&dir, &manifest, "unknown field `confirmed_by_app`");
    edit_tools(&dir, |t| {
        t["tools"][0].as_object_mut().unwrap().remove("confirmed_by_app");
        t["tools"][0]["risk"] = json!("Destructive");
    });
    assert_eq!(refusals(&dir, &manifest), [], "the broker's spelling is accepted");
    edit_tools(&dir, |t| t["schema"] = json!(2));
    refused_with(&dir, &manifest, "tools.json schema 2 is not 1");
}

#[test]
fn schemas_are_a_small_checked_subset() {
    let ok = json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false});
    check_schema(&ok).unwrap();
    for (bad, why) in [
        (json!({"type":"object","properties":{"a":{"$ref":"#/defs/a"}}}), "$ref"),
        (json!({"anyOf":[{"type":"string"}],"type":"string"}), "anyOf"),
        (json!({"properties":{}}), "\"type\" is required"),
        (json!({"type":"tuple"}), "not one of"),
        (json!({"type":"object","properties":{},"required":["ghost"]}), "does not declare"),
        (json!({"type":"string","maxLength":-1}), "non-negative"),
        (json!({"type":"string","enum":[]}), "enum"),
    ] {
        let err = check_schema(&bad).unwrap_err();
        assert!(err.contains(why), "{bad}: {err}");
    }
    let mut deep = json!({"type":"string"});
    for _ in 0..10 {
        deep = json!({"type":"array","items":deep});
    }
    assert!(check_schema(&deep).unwrap_err().contains("deeper"));
    let huge = json!({"type":"string","description":"x".repeat(1000),"title":"y".repeat(1000),
        "properties":{"a":{"type":"string","description":"z".repeat(1000)}}});
    let mut big = huge.clone();
    for i in 0..8 {
        big["properties"][format!("p{i}")] = huge["properties"]["a"].clone();
    }
    assert!(check_schema(&big).unwrap_err().contains("ceiling"));
}

#[test]
fn a_tools_input_must_be_an_object() {
    let dir = scratch("input-object");
    edit_tools(&dir, |t| t["tools"][1]["input_schema"] = json!({"type":"string"}));
    let manifest = stamp(&dir, |_| {});
    refused_with(&dir, &manifest, "news.read: input_schema must describe an object");
}

#[test]
fn a_destructive_background_tool_is_allowed_but_always_waits_for_approval() {
    let dir = scratch("destructive");
    edit_tools(&dir, |t| {
        t["tools"].as_array_mut().unwrap().push(json!({
            "name": "news.topics.clear", "description": "Forget every followed topic.",
            "input_schema": {"type":"object","properties":{}}, "output_schema": {"type":"object","properties":{}},
            "risk": "destructive", "background": true, "shareable": false, "implemented_by": "host-service"
        }));
    });
    let manifest = stamp(&dir, |_| {});
    let review = review(&dir, &manifest);
    assert_eq!(review.refusals().count(), 0, "{:?}", review.issues);
    assert!(review.warnings().any(|w| w.detail.contains("news.topics.clear is destructive and marked background")));
    assert!(review.warnings().any(|w| w.detail.contains("background agent with destructive tools")));
    let bundle = review.bundle.unwrap();
    let clear = bundle.tool("news.topics.clear").unwrap();
    assert_eq!(clear.supervision(), Supervision::HostApproval);
    assert!(!clear.confirmed_by_app(), "never derived from the risk");
    assert!(!clear.runs_unattended_in_background());
}

#[test]
fn a_local_only_app_may_not_share_a_tool_that_returns_private_data() {
    let dir = scratch("local-only");
    let manifest = stamp(&dir, |m| m["agent"]["model"]["local_only"] = json!(true));
    // news.list is shareable and declares private_data: false.
    assert_eq!(refusals(&dir, &manifest), []);
    edit_tools(&dir, |t| t["tools"][0]["private_data"] = json!(true));
    refused_with(&dir, &manifest, "news.list is shareable but the app's model is local_only");
    edit_tools(&dir, |t| {
        t["tools"][0].as_object_mut().unwrap().remove("private_data");
    });
    refused_with(&dir, &manifest, "must declare \"private_data\": false");
    // Without local_only the same tool is allowed, and the store says so.
    let manifest = stamp(&dir, |m| m["agent"]["model"]["local_only"] = json!(false));
    assert_eq!(refusals(&dir, &manifest), []);
}

// ------------------------------------------------------------------- AGENT.md

#[test]
fn agent_md_must_be_declared_text_and_small() {
    let dir = scratch("agent-md");
    let manifest = stamp(&dir, |m| {
        m["agent"].as_object_mut().unwrap().remove("instructions");
    });
    refused_with(&dir, &manifest, "agent.instructions does not name it");

    let manifest = stamp(&dir, |m| m["agent"]["instructions"] = json!("AGENT.md"));
    std::fs::write(dir.join(AGENT_FILE), "a".repeat(40 * 1024)).unwrap();
    refused_with(&dir, &manifest, "over the 32768 ceiling");
    std::fs::write(dir.join(AGENT_FILE), [0xffu8, 0xfe, 0x00]).unwrap();
    refused_with(&dir, &manifest, "not UTF-8");
    std::fs::write(dir.join(AGENT_FILE), "# Hi\n<script>alert(1)</script>").unwrap();
    refused_with(&dir, &manifest, "executable content");
    std::fs::remove_file(dir.join(AGENT_FILE)).unwrap();
    refused_with(&dir, &manifest, "named but not in the bundle");

    assert!(check_instruction_text("#!/bin/sh\necho hi").unwrap_err().contains("#!"));
    assert!(check_instruction_text("text\u{1b}[2J").unwrap_err().contains("control"));
    assert!(check_instruction_text("[x](javascript:alert(1))").is_err());
    assert!(check_instruction_text("# Fine\n\n```json\n{\"a\":1}\n```\n").is_ok());
}

#[test]
fn agent_files_without_an_agent_are_refused_but_tools_alone_are_not() {
    let dir = scratch("no-agent");
    let manifest = stamp(&dir, |m| {
        m.as_object_mut().unwrap().remove("agent");
    });
    refused_with(&dir, &manifest, "AGENT.md is in the bundle");
    refused_with(&dir, &manifest, "skills/news-digest is in the bundle but agent.skills does not name it");
    // Tools serve other callers too (the person's assistant), so an app may
    // ship tools.json without an agent of its own.
    std::fs::remove_file(dir.join(AGENT_FILE)).unwrap();
    std::fs::remove_dir_all(dir.join(SKILLS_DIR)).unwrap();
    let manifest = stamp(&dir, |_| {});
    let review = review(&dir, &manifest);
    assert_eq!(review.refusals().count(), 0, "{:?}", review.issues);
    let bundle = review.bundle.unwrap();
    assert_eq!(bundle.tools.len(), 5);
    assert!(!bundle.background && bundle.agent_md.is_none());
}

// ------------------------------------------------------------------- skills

#[test]
fn skills_are_declared_data_only_and_use_only_the_apps_tools() {
    let dir = scratch("skills");
    let manifest = stamp(&dir, |_| {});
    let skill = dir.join("skills/news-digest");

    let mut m = read_json(&skill.join("manifest.json"));
    m["uses"] = json!(["news.list", "mail.send"]);
    write_json(&skill.join("manifest.json"), &m);
    refused_with(&dir, &manifest, "uses mail.send, which is neither");

    // A generic tool the manifest grants is fine.
    let manifest = stamp(&dir, |m| m["agent"]["tools"] = json!(["storage.read"]));
    let mut m = read_json(&skill.join("manifest.json"));
    m["uses"] = json!(["news.list", "storage.read"]);
    write_json(&skill.join("manifest.json"), &m);
    assert_eq!(refusals(&dir, &manifest), []);

    let mut exec = m.clone();
    exec["tools"] = json!([{"name":"digest","description":"x"}]);
    write_json(&skill.join("manifest.json"), &exec);
    refused_with(&dir, &manifest, "declares \"tools\": a contained app's skills are data-only");
    let mut hooks = m.clone();
    hooks["hooks"] = json!([]);
    write_json(&skill.join("manifest.json"), &hooks);
    refused_with(&dir, &manifest, "declares \"hooks\"");

    let mut renamed = m.clone();
    renamed["name"] = json!("digest");
    write_json(&skill.join("manifest.json"), &renamed);
    refused_with(&dir, &manifest, "must match its directory");
    write_json(&skill.join("manifest.json"), &m);

    std::fs::write(skill.join("run.sh"), "#!/bin/sh").unwrap();
    refused_with(&dir, &manifest, "holds only .md, .json and .txt");
    std::fs::remove_file(skill.join("run.sh")).unwrap();

    std::fs::create_dir_all(dir.join("skills/stray")).unwrap();
    refused_with(&dir, &manifest, "skills/stray is in the bundle but agent.skills does not name it");
    std::fs::remove_dir_all(dir.join("skills/stray")).unwrap();

    let manifest = stamp(&dir, |m| m["agent"]["skills"] = json!(["news-digest", "missing"]));
    refused_with(&dir, &manifest, "skills/missing/ is not in the bundle");
}

// ------------------------------------------------------------------- listing

#[test]
fn the_store_says_what_the_agent_and_its_tools_may_do() {
    let dir = scratch("lines");
    edit_tools(&dir, |t| {
        t["tools"].as_array_mut().unwrap().push(json!({
            "name": "news.share", "description": "Share a story with a contact.",
            "input_schema": {"type":"object","properties":{}}, "output_schema": {"type":"object"},
            "risk": "destructive", "implemented_by": "host-service"
        }));
        t["tools"][1]["shareable"] = json!(true);
        t["tools"][1]["private_data"] = json!(true);
    });
    let manifest = stamp(&dir, |_| {});
    let tools = agent::read_tools(&dir).unwrap().unwrap().tools;
    let lines = agent_permission_lines(&manifest, &tools);
    assert!(lines[0].contains("may work while the app is closed, on a schedule and when new data arrives"), "{lines:?}");
    assert!(lines.iter().any(|l| l == "Can ask to news.share: nothing of this runs until you approve it."), "{lines:?}");
    assert!(lines.iter().any(|l| l == "Offers news.list, news.read to other assistants you allow."), "{lines:?}");
    assert!(lines.iter().any(|l| l == "news.read can pass your private data to those assistants."), "{lines:?}");
    let quiet = AppManifest::parse(r#"{"schema":1,"id":"a","version":"1","name":"A","integrity":{"bundle_blake3":"00"}}"#).unwrap();
    assert!(agent_permission_lines(&quiet, &[]).is_empty());
    let local = stamp(&dir, |m| m["agent"]["model"]["local_only"] = json!(true));
    assert!(privacy_summary(&local).iter().any(|l| l.contains("only models that run on your own devices")));
}

// ------------------------------------------------------------------- confirm

fn destructive(name: &str, implemented_by: &str, confirm: Option<&str>) -> Value {
    let mut tool = json!({
        "name": name, "description": "Send it.",
        "input_schema": {"type":"object","properties":{}}, "output_schema": {"type":"object"},
        "risk": "destructive", "implemented_by": implemented_by
    });
    if let Some(confirm) = confirm {
        tool["confirm"] = json!(confirm);
    }
    tool
}

#[test]
fn confirm_is_declared_per_tool_and_defaults_to_the_host() {
    let dir = scratch("confirm");
    edit_tools(&dir, |t| {
        let tools = t["tools"].as_array_mut().unwrap();
        tools.push(destructive("news.share", "app", Some("app")));
        tools.push(destructive("news.forget", "app", None));
    });
    let manifest = stamp(&dir, |_| {});
    let review = review(&dir, &manifest);
    assert_eq!(review.refusals().count(), 0, "{:?}", review.issues);
    assert!(review.warnings().any(|w| w.detail.contains("news.share confirms on the app's own sheet")));
    let bundle = review.bundle.unwrap();
    let share = bundle.tool("news.share").unwrap();
    assert_eq!((share.confirm, share.supervision()), (Confirm::App, Supervision::AppConfirmation));
    assert!(share.confirmed_by_app() && share.supervision().needs_person());
    let forget = bundle.tool("news.forget").unwrap();
    assert_eq!((forget.confirm, forget.supervision()), (Confirm::Host, Supervision::HostApproval));
    assert!(!forget.confirmed_by_app());
    // The default is not written back, so a tool without the field
    // serialises as it did before the field existed.
    assert!(serde_json::to_value(forget).unwrap().get("confirm").is_none());

    let lines = agent_permission_lines(&manifest, &bundle.tools);
    assert!(lines.iter().any(|l| l == "Can ask to news.forget: nothing of this runs until you approve it."), "{lines:?}");
    assert!(
        lines.iter().any(|l| l == "Asks you on its own screen before news.share; when you are away, it waits for your approval in the app's conversation."),
        "{lines:?}"
    );
}

#[test]
fn a_contained_app_confirms_only_the_tools_it_implements_itself() {
    let dir = scratch("confirm-host-service");
    edit_tools(&dir, |t| t["tools"].as_array_mut().unwrap().push(destructive("news.share", "host-service", Some("app"))));
    let manifest = stamp(&dir, |_| {});
    refused_with(&dir, &manifest, "news.share says confirm \"app\" but is implemented by the host service");
    edit_tools(&dir, |t| {
        let tools = t["tools"].as_array_mut().unwrap();
        tools.pop();
        tools.push(destructive("news.share", "app", Some("sheet")));
    });
    refused_with(&dir, &manifest, "unknown variant `sheet`");
    // On a tool that is not destructive, confirm "app" confirms nothing.
    edit_tools(&dir, |t| {
        t["tools"].as_array_mut().unwrap().pop();
        t["tools"][0]["implemented_by"] = json!("app");
        t["tools"][0]["confirm"] = json!("app");
    });
    let review = review(&dir, &manifest);
    assert_eq!(review.refusals().count(), 0, "{:?}", review.issues);
    assert!(review.warnings().any(|w| w.detail.contains("news.list says confirm \"app\" but is neither destructive nor outward")));
}

#[test]
fn a_native_modules_tools_json_loads_with_the_same_checks() {
    // What crates/app-peers does with a native module's resource: no bundle,
    // no Card runner, the module id as the namespace.
    let json = json!({"schema": 1, "tools": [
        {"name": "rinx.send_message", "description": "Send a message to a room.",
         "input_schema": {"type":"object","properties":{"room":{"type":"string"},"text":{"type":"string"}},"required":["room","text"]},
         "output_schema": {"type":"object","properties":{"event_id":{"type":"string"}}},
         "risk": "Destructive", "implemented_by": "host-service", "confirm": "app"},
        {"name": "rinx.rooms.list", "description": "List joined rooms.",
         "input_schema": {"type":"object","properties":{}}, "output_schema": {"type":"object"},
         "risk": "read", "implemented_by": "host-service", "shareable": true}
    ]})
    .to_string();
    let (tools, warnings) = ToolManifest::load(&json, "rinx", ToolHost::Native, false).unwrap();
    assert_eq!(tools.tools.len(), 2);
    let send = &tools.tools[0];
    assert!(send.confirmed_by_app());
    assert_eq!((send.broker_name().as_str(), send.risk.broker_name()), ("send_message", "Destructive"));
    assert_eq!(tools.tools[1].broker_name(), "rooms_list");
    assert!(warnings.iter().any(|w| !w.refusal && w.detail.contains("rinx.send_message confirms on the app's own sheet")));

    // The same file is refused as a contained app's, whose host service
    // cannot confirm on the app's sheet, and under another namespace.
    let err = ToolManifest::load(&json, "rinx", ToolHost::Contained, false).unwrap_err();
    assert!(err.contains("implemented by the host service"), "{err}");
    let err = ToolManifest::load(&json, "matrix", ToolHost::Native, false).unwrap_err();
    assert!(err.contains("outside the app's namespace \"matrix\""), "{err}");
    let err = ToolManifest::load(&json, "rinx", ToolHost::Native, true).unwrap_err();
    assert!(err.contains("rinx.rooms.list is shareable but the app's model is local_only"), "{err}");
    let err = ToolManifest::load(&json.replace("\"risk\":\"read\"", "\"risk\":\"low\""), "rinx", ToolHost::Native, false).unwrap_err();
    assert!(err.contains("unknown variant `low`"), "{err}");
}

#[test]
fn a_native_apps_id_or_namespace_or_the_hosts_own_name_is_refused() {
    // OctoSense keys a script app's jail, storage folders, tools, executor
    // and consent by its id, and its tools by its namespace: `terminal`
    // (or `com.example.terminal`, whose tools are `terminal.*`) would stand
    // in for the Terminal, `system` for the system agent.
    for id in ["terminal", "rinx", "apphub", "com.example.terminal", "dev.example.rinx", "system", "toolbox", "org.example.dev", "com.example.notes", "weather"] {
        let dir = scratch(&format!("reserved-{id}"));
        let manifest = stamp(&dir, |m| m["id"] = json!(id));
        refused_with(&dir, &manifest, "reserved");
        let err = AgentBundle::load(&dir, &manifest).unwrap_err();
        assert!(err.contains("reserved"), "{id}: {err}");
    }
    for id in ["os.news", "dev.example.news", "org.example.terminal-notes"] {
        assert!(check_reserved_id(id).is_ok(), "{id}");
    }
    for native in ["apphub", "appcard", "reference", "rinx", "sheets", "terminal", "calculator", "clock", "notes", "reminders", "weather", "browser", "task"] {
        assert!(RESERVED_NAMES.contains(&native), "{native}");
    }
}

#[test]
fn an_outward_tool_waits_for_the_person_and_may_forbid_standing_rules() {
    let dir = scratch("outward");
    edit_tools(&dir, |t| {
        t["tools"].as_array_mut().unwrap().push(json!({
            "name": "news.share", "description": "Share a story with a contact.",
            "input_schema": {"type":"object","properties":{}}, "output_schema": {"type":"object","properties":{}},
            "risk": "act", "outward": true, "auto_approvable": false, "shareable": false, "implemented_by": "host-service"
        }));
    });
    let manifest = stamp(&dir, |_| {});
    let review = review(&dir, &manifest);
    assert_eq!(review.refusals().count(), 0, "{:?}", review.issues);
    let bundle = review.bundle.unwrap();
    let share = bundle.tool("news.share").unwrap();
    assert!(share.outward && !share.auto_approvable);
    assert_eq!(share.supervision(), Supervision::HostApproval, "outward, so the person approves it");
    assert!(!share.runs_unattended_in_background());
    // Omitted, the fields are false and true, and are not written back.
    let list = bundle.tool("news.list").unwrap();
    assert!(!list.outward && list.auto_approvable);
    let written = serde_json::to_value(list).unwrap();
    assert!(written.get("outward").is_none() && written.get("auto_approvable").is_none(), "{written}");
    let lines = agent_permission_lines(&manifest, &bundle.tools);
    assert!(lines.iter().any(|l| l.contains("news.share") && l.contains("until you approve")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("every call of news.share")), "{lines:?}");

    // A call that reaches outside the device is never a read.
    edit_tools(&dir, |t| {
        let last = t["tools"].as_array().unwrap().len() - 1;
        t["tools"][last]["risk"] = json!("read");
    });
    refused_with(&dir, &manifest, "news.share is outward but its risk is read");
}

#[test]
fn billable_media_aliases_need_explicit_private_data_not_family_declarations() {
    for method in ["model.image", "model.audio", "model.embeddings", "model.video", "model.video.cancel"] {
        let dir = scratch(&format!("media-alias-{method}"));
        edit_tools(&dir, |tools| {
            tools["tools"][0]["host_method"] = json!(method);
            tools["tools"][0]["risk"] = json!("act");
            tools["tools"][0]["private_data"] = json!(true);
        });
        let manifest = stamp(&dir, |_| {});
        assert!(AgentBundle::load(&dir, &manifest).is_ok(), "{method}");
        let manifest = stamp(&dir, |manifest| {
            manifest["capabilities"].as_array_mut().unwrap().push(json!("model"));
        });
        assert_eq!(AgentBundle::load(&dir, &manifest).unwrap().unwrap().tool("news.list").unwrap().service_method(), method);
        edit_tools(&dir, |tools| tools["tools"][0]["private_data"] = json!(false));
        let manifest = stamp(&dir, |_| {});
        assert!(AgentBundle::load(&dir, &manifest).unwrap_err().contains("private_data: true"), "{method}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
