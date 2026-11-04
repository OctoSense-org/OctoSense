//! The manifest's growth rules within 1.x (ADR 0005 §2): unknown fields are
//! refused at this build's minor and ignored (and reported) in a manifest
//! written for a newer 1.x, a required feature this build does not know
//! refuses the app at every minor, and the two new fields leave older
//! manifests' signing bytes alone.
use octosense_app_contract::*;

fn manifest_with(body: &str) -> String {
    format!(
        r#"{{"schema":1,"id":"forecast","version":"1.0.0","name":"Forecast","integrity":{{"bundle_blake3":"00"}}{}}}"#,
        if body.is_empty() { String::new() } else { format!(",{body}") }
    )
}

/// A `schema_minor` newer than this build's: what a manifest written for a
/// later 1.x says. Relative, so the lenient-path tests keep testing the
/// lenient path when this build's minor moves on.
fn newer_minor() -> u32 {
    SCHEMA_MINOR + 1
}

#[test]
fn palpo_extension_preserves_schema_and_declares_its_host_feature() {
    assert_eq!(SCHEMA, 1);
    // 1 since 1.12.0, which added the optional `window` hint; Palpo's
    // operations (1.2.0) left it at 0.
    assert_eq!(SCHEMA_MINOR, 1);
    assert_eq!(
        KNOWN_FEATURES,
        &[
            "palpo-admin-v1",
            "host-api-v1",
            "backend-api-v1",
            "script-tools-v1",
            "publisher-github-v1",
            "wasm-components-v1",
            "wasm-shared-components-v1"
        ]
    );
}

#[test]
fn wasm_component_abi_is_independent_of_usage_declarations() {
    for fields in [r#""requires":["wasm-components-v1"]"#, r#""requires":["wasm-components-v1"],"capabilities":["storage"]"#] {
        let manifest = parse(&manifest_with(fields)).unwrap();
        assert_eq!(manifest.requires, ["wasm-components-v1"]);
    }
}

// ---- shared components (App Hub ADR 0003) --------------------------------

const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn with_components(components: &str) -> String {
    manifest_with(&format!(
        r#""requires":["wasm-shared-components-v1"],"capabilities":["wasm"],"components":{components}"#
    ))
}

fn dependency(alias: &str, id: &str, version: &str, blake3: &str) -> String {
    format!(r#"{{"as":"{alias}","id":"{id}","version":"{version}","blake3":"{blake3}"}}"#)
}

#[test]
fn shared_components_parse_and_pin_one_exact_version_and_digest() {
    let manifest = parse(&with_components(&format!("[{}]", dependency("markdown", "org.example.markdown", "1.2.0", DIGEST)))).unwrap();
    assert_eq!(manifest.components, [ComponentDependency::new("markdown", "org.example.markdown", "1.2.0", DIGEST)]);
    assert_eq!(manifest.components[0].describe(), "markdown (org.example.markdown 1.2.0)");
    // The field serialises as `as`, and the policy grants nothing new.
    let json = serde_json::to_value(&manifest).unwrap();
    assert_eq!(json["components"][0]["as"], "markdown");
    let policy = resolve(&manifest, &HostLimits::default().with_require_signature(false)).unwrap();
    assert_eq!(policy.capabilities.iter().collect::<Vec<_>>(), ["wasm"]);
    // Pre-releases and build metadata are exact versions too.
    for version in ["0.1.0", "10.20.30", "1.0.0-alpha.1", "1.0.0+build.5", "1.0.0-rc.1+sha.abc"] {
        assert!(parse(&with_components(&format!("[{}]", dependency("m", "org.example.m", version, DIGEST)))).is_ok(), "{version}");
    }
}

#[test]
fn shared_components_need_their_abi_not_a_capability_grant() {
    let one = format!("[{}]", dependency("markdown", "org.example.markdown", "1.2.0", DIGEST));
    let err = parse(&manifest_with(&format!(r#""components":{one}"#))).unwrap_err();
    assert_eq!(err, "components requires wasm-shared-components-v1");
    let manifest = parse(&manifest_with(&format!(r#""requires":["wasm-shared-components-v1"],"components":{one}"#))).unwrap();
    assert!(manifest.capabilities.is_empty());
    assert_eq!(manifest.components.len(), 1);
}

#[test]
fn shared_component_entries_are_checked_one_by_one() {
    let refused = |components: String| parse(&with_components(&components)).unwrap_err();
    for alias in ["", "Markdown", "1md", "md-x", "a".repeat(33).as_str()] {
        let err = refused(format!("[{}]", dependency(alias, "org.example.markdown", "1.2.0", DIGEST)));
        assert!(err.contains("must match [a-z][a-z0-9_]{0,31}"), "{alias:?}: {err}");
    }
    assert!(parse(&with_components(&format!("[{}]", dependency(&"a".repeat(32), "org.example.m", "1.0.0", DIGEST)))).is_ok());
    let twice = format!(
        "[{},{}]",
        dependency("md", "org.example.markdown", "1.2.0", DIGEST),
        dependency("md", "org.example.other", "1.0.0", DIGEST)
    );
    assert_eq!(refused(twice), r#"component name "md" is used twice"#);
    for version in ["1.2", "^1.2.0", "~1.2.0", ">=1.0.0", "1.x", "*", "01.2.0", "1.2.0-", "1.2.0-01", "v1.2.0", "1.2.0 "] {
        let err = refused(format!("[{}]", dependency("md", "org.example.markdown", version, DIGEST)));
        assert!(err.contains("one exact semantic version"), "{version:?}: {err}");
    }
    for digest in ["", "00", &DIGEST.to_uppercase(), &format!("{}g", &DIGEST[..63])] {
        let err = refused(format!("[{}]", dependency("md", "org.example.markdown", "1.2.0", digest)));
        assert!(err.contains("64 lowercase hex"), "{digest:?}: {err}");
    }
    for (id, needle) in [
        ("os.markdown", "under os."),
        ("Org.Example", "may hold only"),
        ("../escape", "may hold only"),
        ("org.example.notes", "reserved"),
        ("", "1 to 64 characters"),
    ] {
        let err = refused(format!("[{}]", dependency("md", id, "1.2.0", DIGEST)));
        assert!(err.starts_with("component md: component id") && err.contains(needle), "{id:?}: {err}");
    }
    let nine: Vec<String> = (0..9).map(|i| dependency(&format!("c{i}"), "org.example.markdown", "1.2.0", DIGEST)).collect();
    assert_eq!(refused(format!("[{}]", nine.join(","))), "an app may name at most 8 components, not 9");
    let eight = &nine[..8];
    assert!(parse(&with_components(&format!("[{}]", eight.join(",")))).is_ok());
    // An unknown field inside an entry is refused like any other.
    let err = refused(r#"[{"as":"md","id":"org.example.markdown","version":"1.2.0","blake3":"x","range":"^1"}]"#.into());
    assert!(err.contains("unknown field `range`"), "{err}");
}

#[test]
fn a_manifest_without_components_signs_as_before() {
    let text = manifest_with(r#""capabilities":["wasm"]"#);
    let manifest = parse(&text).unwrap();
    let json = serde_json::to_string(&manifest).unwrap();
    assert!(!json.contains("components"), "{json}");
    assert_eq!(
        String::from_utf8(manifest.signing_bytes().unwrap()).unwrap(),
        r#"{"agent":null,"capabilities":["wasm"],"compute":{"instruction_budget":null,"memory_bytes":null},"id":"forecast","integrity":{"bundle_blake3":"00","signature":null},"name":"Forecast","network":{"hosts":[]},"schema":1,"storage":{"max_bytes":null},"version":"1.0.0"}"#
    );
    assert!(check_component_id("org.example.markdown").is_ok());
    assert!(is_exact_version("1.2.3") && !is_exact_version("1.2"));
}

#[test]
fn host_api_requirements_need_the_feature_and_do_not_grant_access() {
    assert!(parse(&manifest_with(r#""host_api":{"required":{"camera.permission.status":1}}"#)).is_err());
    let manifest = parse(&manifest_with(r#""requires":["host-api-v1"],"host_api":{"required":{"camera.permission.status":1}},"capabilities":["runtime"]"#)).unwrap();
    let policy = resolve(&manifest, &HostLimits::default().with_require_signature(false)).unwrap();
    assert!(policy.allows("runtime"));
    assert!(!policy.allows("camera"));
}

#[test]
fn selected_file_access_needs_its_own_grant_and_an_implemented_host() {
    let limits = HostLimits::default().with_require_signature(false);
    let manifest = parse(&manifest_with(r#""requires":["host-api-v1"],"capabilities":["files"],"host_api":{"required":{"files.import":1}}"#)).unwrap();
    let policy = resolve(&manifest, &limits).unwrap();
    assert!(policy.allows("files"));
    for other in ["storage", "photos", "library", "camera", "net"] {
        assert!(!policy.allows(other), "files must not imply {other}");
    }
    let storage = resolve(&parse(&manifest_with(r#""capabilities":["storage"]"#)).unwrap(), &limits).unwrap();
    assert!(!storage.allows("files"));
    let mut versions = std::collections::BTreeMap::from([("app_policy.device_consent".into(), 1)]);
    assert!(manifest.check_host_apis(&versions).unwrap_err().contains("files.import@1"));
    versions.insert("files.import".into(), 1);
    assert!(manifest.check_host_apis(&versions).is_ok());
    for near in ["files.*", "files.import", "FILES"] {
        let manifest = parse(&manifest_with(&format!(r#""capabilities":["{near}"]"#))).unwrap();
        assert!(resolve(&manifest, &limits).is_err(), "{near} must be unknown");
    }
}

#[test]
fn host_api_marker_requires_the_policy_abi_even_without_a_device_method() {
    let manifest = parse(&manifest_with(r#""requires":["host-api-v1"]"#)).unwrap();
    let mut versions = std::collections::BTreeMap::new();
    assert!(manifest.check_host_apis(&versions).unwrap_err().contains("app_policy.device_consent@1"));
    versions.insert("camera.permission.status".into(), 1);
    assert!(manifest.check_host_apis(&versions).is_err(), "a method is not the policy ABI");
    versions.insert("app_policy.device_consent".into(), 2);
    assert!(manifest.check_host_apis(&versions).is_err(), "ABI majors match exactly");
    versions.insert("app_policy.device_consent".into(), 1);
    assert!(manifest.check_host_apis(&versions).is_ok());
}

#[test]
fn palpo_uses_exact_service_grants() {
    let services = serde_json::to_string(octosense_app_contract::palpo::SERVICES).unwrap();
    let manifest = parse(&manifest_with(&format!(
        r#""requires":["palpo-admin-v1"],"capabilities":{services}"#
    ))).unwrap();
    let limits = HostLimits::system().with_require_signature(false);
    let resolved = resolve(&manifest, &limits).unwrap();
    for service in octosense_app_contract::palpo::SERVICES {
        assert!(resolved.allows(service));
    }
    assert!(!resolved.allows("palpo.*"));
    assert!(!resolved.allows("palpo.users.delete"));
    let unknown = parse(&manifest_with(r#""capabilities":["palpo.users.delete"]"#)).unwrap();
    assert!(resolve(&unknown, &limits).is_err());
}

#[test]
fn calendar_ui_access_is_explicit_and_does_not_grant_other_services() {
    let limits = HostLimits::system();
    let old = resolve(&parse(&manifest_with("")).unwrap(), &limits).unwrap();
    assert!(!old.allows("calendar"));
    let calendar = resolve(&parse(&manifest_with(r#""capabilities":["calendar"]"#)).unwrap(), &limits).unwrap();
    assert!(calendar.allows("calendar"));
    assert!(!calendar.allows("mail") && !calendar.allows("calendar.*"));
    assert!(!calendar.allows("glance"));
    assert!(resolve(&parse(&manifest_with(r#""capabilities":["calendar.*"]"#)).unwrap(), &limits).is_err());
}

#[test]
fn media_services_require_exact_separate_grants() {
    let limits = HostLimits::system();
    for capability in ["photos", "youtube"] {
        let manifest = parse(&manifest_with(&format!(r#""capabilities":["{capability}"]"#))).unwrap();
        let policy = resolve(&manifest, &limits).unwrap();
        assert!(policy.allows(capability));
        assert!(!policy.allows("mail") && !policy.allows("glance") && !policy.allows("net"));
        assert!(!policy.allows(if capability == "photos" { "youtube" } else { "photos" }));
        let wildcard = parse(&manifest_with(&format!(r#""capabilities":["{capability}.*"]"#))).unwrap();
        assert!(resolve(&wildcard, &limits).is_err());
    }
}

#[test]
fn native_calendar_grant_is_separate_and_required_methods_must_exist() {
    let manifest = parse(&manifest_with(r#""capabilities":["device_calendar"],"requires":["host-api-v1"],"host_api":{"required":{"device_calendar.events.list":1}}"#)).unwrap();
    let policy = resolve(&manifest, &HostLimits::system()).unwrap();
    assert!(policy.allows("device_calendar"));
    for unrelated in ["calendar", "gcalendar", "auth", "mail", "location", "device_calendar.*"] {
        assert!(!policy.allows(unrelated), "native calendar must not imply {unrelated}");
    }
    let mut versions = std::collections::BTreeMap::from([("app_policy.device_consent".into(), 1)]);
    assert!(manifest.check_host_apis(&versions).unwrap_err().contains("device_calendar.events.list"));
    versions.insert("device_calendar.events.list".into(), 2);
    assert!(manifest.check_host_apis(&versions).is_err(), "ABI 2 does not silently satisfy ABI 1");
    versions.insert("device_calendar.events.list".into(), 1);
    manifest.check_host_apis(&versions).unwrap();
    let old = resolve(&parse(&manifest_with(r#""capabilities":["calendar","gcalendar"]"#)).unwrap(), &HostLimits::system()).unwrap();
    assert!(!old.allows("device_calendar"));
}

#[test]
fn playback_never_grants_recording_storage_or_background_authority() {
    let manifest = parse(&manifest_with(r#""capabilities":["audio"],"requires":["host-api-v1"],"host_api":{"required":{"audio.play":1}}"#)).unwrap();
    let policy = resolve(&manifest, &HostLimits::system()).unwrap();
    assert!(policy.allows("audio"));
    for unrelated in ["microphone", "storage", "camera", "net", "audio.*"] {
        assert!(!policy.allows(unrelated));
    }
    let mut versions = std::collections::BTreeMap::from([("app_policy.device_consent".into(), 1)]);
    assert!(manifest.check_host_apis(&versions).is_err());
    versions.insert("audio.play".into(), 1);
    manifest.check_host_apis(&versions).unwrap();
    let capture = resolve(&parse(&manifest_with(r#""capabilities":["microphone","storage"]"#)).unwrap(), &HostLimits::system()).unwrap();
    assert!(!capture.allows("audio"));
}

#[test]
fn an_apps_own_functions_are_a_grant_of_their_own() {
    let limits = HostLimits::system();
    let manifest = parse(&manifest_with(r#""capabilities":["wasm"]"#)).unwrap();
    let policy = resolve(&manifest, &limits).unwrap();
    assert!(policy.allows("wasm"));
    for other in ["net", "storage", "model", "glance", "mail", "runtime"] {
        assert!(!policy.allows(other), "wasm must not imply {other}");
    }
    for near in ["wasm.*", "wasm.find_slots", "WASM"] {
        let manifest = parse(&manifest_with(&format!(r#""capabilities":["{near}"]"#))).unwrap();
        assert!(resolve(&manifest, &limits).is_err(), "{near} must be unknown");
    }
}

#[test]
fn an_unknown_required_feature_needs_a_newer_host() {
    let err = parse(&manifest_with(r#""requires":["storage.encrypted"]"#)).unwrap_err();
    assert_eq!(err, "app forecast needs a newer host: storage.encrypted");
    let err = parse(&manifest_with(r#""requires":["a","b"],"schema_minor":2"#)).unwrap_err();
    assert!(err.ends_with("needs a newer host: a, b"), "{err}");
}

#[test]
fn oauth_provider_services_are_independent_grants() {
    let limits = HostLimits::default().with_require_signature(false);
    let capabilities = ["auth", "github", "gcalendar", "gmail"];
    for granted in capabilities {
        let manifest = parse(&manifest_with(&format!(r#""capabilities":["{granted}"]"#))).unwrap();
        let policy = resolve(&manifest, &limits).unwrap();
        for candidate in capabilities {
            assert_eq!(policy.allows(candidate), candidate == granted);
        }
        for unrelated in ["mail", "calendar", "net", "glance", "auth.*"] {
            assert!(!policy.allows(unrelated));
        }
        let wildcard = parse(&manifest_with(&format!(r#""capabilities":["{granted}.*"]"#))).unwrap();
        assert!(resolve(&wildcard, &limits).is_err());
    }
}

#[test]
fn resolve_checks_requires_even_for_a_manifest_not_read_by_parse() {
    let manifest: AppManifest = serde_json::from_str(&manifest_with(r#""requires":["x"]"#)).unwrap();
    let limits = HostLimits::default().with_require_signature(false);
    let err = resolve(&manifest, &limits).unwrap_err();
    assert!(err.contains("needs a newer host: x"), "{err}");
}

#[test]
fn an_empty_requires_and_a_newer_minor_are_accepted() {
    let manifest = parse(&manifest_with(r#""requires":[],"schema_minor":3"#)).unwrap();
    assert!(manifest.requires.is_empty());
    assert_eq!(manifest.schema_minor, 3);
}

#[test]
fn unknown_fields_are_still_refused() {
    let err = parse(&manifest_with(r#""sandbox":"off""#)).unwrap_err();
    assert!(err.contains("unknown field"), "{err}");
    // At this build's own minor, said or not, the read is strict.
    let err = parse(&manifest_with(r#""schema_minor":0,"sandbox":"off""#)).unwrap_err();
    assert!(err.contains("unknown field"), "{err}");
    let err = parse(&manifest_with(r#""schema_minor":0,"network":{"hosts":[],"proxy":"x"}"#)).unwrap_err();
    assert!(err.contains("unknown field `proxy`"), "{err}");
    // Minor 1 is this build's own minor now: no longer read leniently.
    let err = parse(&manifest_with(r#""schema_minor":1,"sandbox":"off""#)).unwrap_err();
    assert!(err.contains("unknown field `sandbox`"), "{err}");
    let err = parse(&manifest_with(r#""schema_minor":1,"window":{"width":800,"height":600,"x":0}"#)).unwrap_err();
    assert!(err.contains("unknown field `x`"), "{err}");
}

#[test]
fn the_new_fields_do_not_change_an_older_manifests_signing_bytes() {
    let plain = parse(&manifest_with("")).unwrap();
    let explicit = parse(&manifest_with(r#""requires":[],"schema_minor":0"#)).unwrap();
    assert_eq!(plain.signing_bytes().unwrap(), explicit.signing_bytes().unwrap());
    let bytes = String::from_utf8(plain.signing_bytes().unwrap()).unwrap();
    assert!(!bytes.contains("requires") && !bytes.contains("schema_minor"), "{bytes}");
    let minor = parse(&manifest_with(r#""schema_minor":1"#)).unwrap();
    assert!(String::from_utf8(minor.signing_bytes().unwrap()).unwrap().contains(r#""schema_minor":1"#));
}

#[test]
fn a_newer_minor_ignores_an_unknown_optional_field_and_reports_it() {
    let minor = newer_minor();
    let manifest = parse(&manifest_with(&format!(r#""schema_minor":{minor},"capabilities":["storage"],"theme_color":"slate""#))).unwrap();
    assert_eq!(manifest.ignored_fields(), ["theme_color"]);
    assert_eq!(manifest.capabilities, ["storage"]);
    let limits = HostLimits::default().with_require_signature(false);
    assert!(resolve(&manifest, &limits).unwrap().allows("storage"));
}

#[test]
fn a_newer_minor_with_an_unknown_required_feature_is_refused() {
    let err = parse(&manifest_with(r#""schema_minor":2,"requires":["net.proxy"],"network":{"hosts":[],"proxy":"p.example"}"#))
        .unwrap_err();
    assert_eq!(err, "app forecast needs a newer host: net.proxy");
    // Without unknown fields, the strict read reaches the same refusal.
    let err = parse(&manifest_with(r#""schema_minor":2,"requires":["net.proxy"]"#)).unwrap_err();
    assert_eq!(err, "app forecast needs a newer host: net.proxy");
}

#[test]
fn a_newer_minor_ignores_unknown_fields_at_every_level() {
    let manifest = parse(&manifest_with(
        r#""schema_minor":3,
           "capabilities":["net","research"],
           "network":{"hosts":["api.weather.example"],"retry":2},
           "storage":{"max_bytes":4096,"backup":"never"},
           "compute":{"memory_bytes":1024,"gpu":false},
           "research":{"langs":["en"],"safe_search":true},
           "integrity_extra":1,
           "agent":{"profile":"read-only","voice":"calm",
                    "model":{"needs":["tool_calling"],"temperature":0.2,
                             "per_task":{"triage":{"tier":"fast","budget":5}}},
                    "triggers":{"schedule":["0 7 * * *"],"geofence":"home"}}"#,
    ))
    .unwrap();
    assert_eq!(
        manifest.ignored_fields(),
        [
            "agent.model.per_task.triage.budget",
            "agent.model.temperature",
            "agent.triggers.geofence",
            "agent.voice",
            "compute.gpu",
            "integrity_extra",
            "network.retry",
            "research.safe_search",
            "storage.backup",
        ]
    );
    // What the build knows is read as written.
    assert_eq!(manifest.network.hosts, ["api.weather.example"]);
    assert_eq!(manifest.storage.max_bytes, Some(4096));
    assert_eq!(manifest.research.as_ref().unwrap().langs, ["en"]);
    assert_eq!(manifest.agent.as_ref().unwrap().model.as_ref().unwrap().per_task["triage"].tier, ModelTier::Fast);
}

#[test]
fn a_newer_minor_still_refuses_what_it_knows_to_be_wrong() {
    // Ignoring is for unknown fields only: a known field with a bad value,
    // a foreign schema and the old research shape are refused as before.
    let minor = newer_minor();
    let err = parse(&manifest_with(&format!(r#""schema_minor":{minor},"storage":{{"agent_workspace":"everything"}}"#))).unwrap_err();
    assert!(err.contains("manifest is not valid"), "{err}");
    let err = parse(&manifest_with(&format!(r#""schema_minor":{minor},"x":1"#)).replace(r#""schema":1"#, r#""schema":2"#)).unwrap_err();
    assert!(err.contains("manifest is not valid"), "{err}");
    let err = parse(&manifest_with(&format!(
        r#""schema_minor":{minor},"capabilities":["research"],"research":{{"languages":["en"]}}"#
    )))
    .unwrap_err();
    assert!(err.contains("old toolbox shape"), "{err}");
    // So is a window out of range.
    let err = parse(&manifest_with(&format!(r#""schema_minor":{minor},"window":{{"width":100,"height":600}}"#))).unwrap_err();
    assert_eq!(err, "window width must be 320 to 4096 points, not 100");
}

#[test]
fn a_newer_manifest_signs_with_the_fields_it_was_signed_with() {
    let minor = newer_minor();
    let manifest =
        parse(&manifest_with(&format!(r#""schema_minor":{minor},"theme_color":"blue","network":{{"hosts":[],"retry":2}}"#))).unwrap();
    let bytes = String::from_utf8(manifest.signing_bytes().unwrap()).unwrap();
    assert!(bytes.contains(r#""theme_color":"blue""#), "{bytes}");
    assert!(bytes.contains(r#""network":{"hosts":[],"retry":2}"#), "{bytes}");
}

// ---- the window hint (schema_minor 1, contract 1.12.0) -------------------

/// The signing bytes of `manifest_with(r#""capabilities":["storage"]"#)`, as
/// every contract before 1.12.0 computed them.
const PLAIN_SIGNING_BYTES: &str = r#"{"agent":null,"capabilities":["storage"],"compute":{"instruction_budget":null,"memory_bytes":null},"id":"forecast","integrity":{"bundle_blake3":"00","signature":null},"name":"Forecast","network":{"hosts":[]},"schema":1,"storage":{"max_bytes":null},"version":"1.0.0"}"#;

#[test]
fn a_window_hint_parses_at_schema_minor_1_and_grants_nothing() {
    let manifest =
        parse(&manifest_with(r#""schema_minor":1,"capabilities":["storage"],"window":{"width":1536,"height":1024}"#)).unwrap();
    assert_eq!(manifest.window, Some(WindowHint::new(1536, 1024)));
    assert!(manifest.ignored_fields().is_empty());
    // It serialises exactly as written, and its signing bytes carry it.
    let json = serde_json::to_value(&manifest).unwrap();
    assert_eq!(json["window"], serde_json::json!({"width": 1536, "height": 1024}));
    let bytes = String::from_utf8(manifest.signing_bytes().unwrap()).unwrap();
    assert!(bytes.contains(r#""schema_minor":1"#) && bytes.contains(r#""window":{"height":1024,"width":1536}"#), "{bytes}");
    // A hint, not a grant: the policy is the one the same manifest without
    // it resolves to.
    let limits = HostLimits::default().with_require_signature(false);
    let plain = parse(&manifest_with(r#""capabilities":["storage"]"#)).unwrap();
    assert_eq!(resolve(&manifest, &limits).unwrap(), resolve(&plain, &limits).unwrap());
    let system = HostLimits::system();
    assert_eq!(resolve(&manifest, &system).unwrap(), resolve(&plain, &system).unwrap());
}

#[test]
fn a_manifest_without_a_window_parses_and_signs_as_before() {
    let manifest = parse(&manifest_with(r#""capabilities":["storage"]"#)).unwrap();
    assert_eq!(manifest.window, None);
    let json = serde_json::to_string(&manifest).unwrap();
    assert!(!json.contains("window"), "{json}");
    assert_eq!(String::from_utf8(manifest.signing_bytes().unwrap()).unwrap(), PLAIN_SIGNING_BYTES);
    // Declaring this build's minor without using window changes only the
    // schema_minor key.
    let minor = parse(&manifest_with(r#""schema_minor":1,"capabilities":["storage"]"#)).unwrap();
    assert_eq!(minor.window, None);
    let bytes = String::from_utf8(minor.signing_bytes().unwrap()).unwrap();
    assert_eq!(bytes.replace(r#","schema_minor":1"#, ""), PLAIN_SIGNING_BYTES);
}

#[test]
fn a_window_hint_needs_schema_minor_1() {
    let expected = r#"window requires "schema_minor": 1 or later, so that hosts which predate window ignore it instead of refusing the app"#;
    for minor in ["", r#""schema_minor":0,"#] {
        let err = parse(&manifest_with(&format!(r#"{minor}"window":{{"width":1536,"height":1024}}"#))).unwrap_err();
        assert_eq!(err, expected, "{minor:?}");
    }
    // resolve checks it too, for a manifest that did not come through parse.
    let unparsed: AppManifest = serde_json::from_str(&manifest_with(r#""window":{"width":1536,"height":1024}"#)).unwrap();
    assert_eq!(resolve(&unparsed, &HostLimits::system()).unwrap_err(), expected);
}

#[test]
fn a_window_hint_is_held_to_its_range() {
    let at = |width: i64, height: i64| {
        parse(&manifest_with(&format!(r#""schema_minor":1,"window":{{"width":{width},"height":{height}}}"#)))
    };
    for (width, height) in [(320, 240), (4096, 4096), (1536, 1024), (320, 4096), (4096, 240)] {
        assert_eq!(at(width, height).unwrap().window, Some(WindowHint::new(width as u32, height as u32)));
    }
    for (width, height, expected) in [
        (319, 600, "window width must be 320 to 4096 points, not 319"),
        (4097, 600, "window width must be 320 to 4096 points, not 4097"),
        (0, 600, "window width must be 320 to 4096 points, not 0"),
        (800, 239, "window height must be 240 to 4096 points, not 239"),
        (800, 4097, "window height must be 240 to 4096 points, not 4097"),
    ] {
        assert_eq!(at(width, height).unwrap_err(), expected, "{width}x{height}");
    }
    // Not a size at all: refused by the parser like any malformed field.
    for (window, why) in [
        (r#"{"width":800}"#, "missing field `height`"),
        (r#"{"height":600}"#, "missing field `width`"),
        (r#"{"width":800,"width":900,"height":600}"#, "duplicate field `width`"),
        (r#"{"width":-1,"height":600}"#, "invalid value"),
        (r#"{"width":800.5,"height":600}"#, "invalid type: floating point"),
        (r#"{"width":1536.0,"height":1024}"#, "invalid type: floating point"),
        (r#"{"width":4294967296,"height":600}"#, "invalid value"),
        (r#"{"width":"800","height":600}"#, "invalid type: string"),
        // Only an object: an array would sign as an object, not as written.
        (r#"[800,600]"#, "invalid type: sequence"),
        (r#""800x600""#, "invalid type: string"),
    ] {
        let err = parse(&manifest_with(&format!(r#""schema_minor":1,"window":{window}"#))).unwrap_err();
        assert!(err.starts_with("manifest is not valid") && err.contains(why), "{window}: {err}");
    }
}

#[test]
fn a_newer_minor_reads_the_window_and_still_ignores_what_it_does_not_know() {
    // A manifest written for a later 1.x: window is known and checked, the
    // rest is ignored, listed, and kept in the signing bytes.
    let minor = newer_minor();
    let manifest = parse(&manifest_with(&format!(
        r#""schema_minor":{minor},"capabilities":["storage"],"theme_color":"slate",
           "window":{{"width":1536,"height":1024,"maximized":true}},"storage":{{"backup":"never"}}"#
    )))
    .unwrap();
    assert_eq!(manifest.window, Some(WindowHint::new(1536, 1024)));
    assert_eq!(manifest.ignored_fields(), ["storage.backup", "theme_color", "window.maximized"]);
    let bytes = String::from_utf8(manifest.signing_bytes().unwrap()).unwrap();
    assert!(bytes.contains(r#""window":{"height":1024,"maximized":true,"width":1536}"#), "{bytes}");
    assert!(bytes.contains(r#""theme_color":"slate""#), "{bytes}");
}

#[test]
fn hostlimits_builders_set_each_ceiling() {
    let limits = HostLimits::system()
        .with_require_signature(true)
        .with_max_storage_bytes(1)
        .with_max_instruction_budget(2)
        .with_max_memory_bytes(3)
        .with_max_iterations(4)
        .with_max_token_budget(5)
        .with_offered_tools(["net.fetch"]);
    assert!(limits.require_signature);
    assert_eq!(
        (limits.max_storage_bytes, limits.max_instruction_budget, limits.max_memory_bytes, limits.max_iterations, limits.max_token_budget),
        (1, 2, 3, 4, 5)
    );
    assert_eq!(limits.offered_tools, ["net.fetch"]);
}

#[test]
fn backend_configuration_keeps_account_and_abi_boundaries_without_auth_disclosure() {
    let registration = r#""backend":{"id":"notes","client_id":"public-test-client","authorization_url":"https://example.com/authorize","token_url":"https://example.com/token","me_url":"https://example.com/me","logout_url":"https://example.com/logout","scopes":["app.session"]}"#;
    let body = format!(r#""requires":["backend-api-v1"],"storage":{{"accounts":true}},{registration}"#);
    let manifest = parse(&manifest_with(&body)).unwrap();
    assert!(manifest.capabilities.is_empty());
    assert!(manifest.backend.is_some());
    assert!(parse(&manifest_with(&format!(r#""requires":["backend-api-v1"],{registration}"#))).unwrap_err().contains("storage.accounts"));
    assert!(parse(&manifest_with(&format!(r#""storage":{{"accounts":true}},{registration}"#))).unwrap_err().contains("backend-api-v1"));
}
