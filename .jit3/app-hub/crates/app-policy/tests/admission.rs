//! What an app may and may not obtain by asking. Every test here is a rule
//! from ADR 0002; a failure means an app got more reach than it declared, or
//! a well-formed app was refused.
use octosense_app_policy::*;
use std::path::Path;

const BUNDLE: &[u8] = b"a card bundle";

fn manifest_with(body: &str) -> String {
    format!(r#"{{"schema":1,"id":"forecast","version":"1.0.0","name":"Forecast","integrity":{{"bundle_blake3":"{}"}}{}}}"#,
        bundle_digest(BUNDLE),
        if body.is_empty() { String::new() } else { format!(",{body}") })
}

fn open_limits() -> HostLimits {
    HostLimits::default().with_require_signature(false)
}

fn resolve(body: &str) -> Result<AppPolicy, String> {
    admit_and_resolve(&manifest_with(body), BUNDLE, &open_limits(), &RefuseAllSignatures)
}

// --------------------------------------------------------------- admission

#[test]
fn a_changed_bundle_is_refused_before_anything_else() {
    let err = admit_and_resolve(&manifest_with(""), b"tampered", &open_limits(), &RefuseAllSignatures).unwrap_err();
    assert!(err.contains("does not match the manifest"), "{err}");
}

#[test]
fn an_unsigned_bundle_is_refused_when_the_host_requires_signing() {
    let err = admit_and_resolve(&manifest_with(""), BUNDLE, &HostLimits::default(), &RefuseAllSignatures).unwrap_err();
    assert!(err.contains("unsigned"), "{err}");
}

#[test]
fn a_signature_without_a_verifier_is_refused_rather_than_ignored() {
    let body = r#""#;
    let json = manifest_with(body).replace(
        &format!(r#""bundle_blake3":"{}""#, bundle_digest(BUNDLE)),
        &format!(r#""bundle_blake3":"{}","signature":{{"key_id":"release","value":"aabb"}}"#, bundle_digest(BUNDLE)),
    );
    let err = admit_and_resolve(&json, BUNDLE, &HostLimits::default(), &RefuseAllSignatures).unwrap_err();
    assert!(err.contains("no signature verifier"), "{err}");
}

#[test]
fn an_unknown_field_is_refused_so_a_newer_manifest_cannot_run_under_looser_rules() {
    let err = admit_and_resolve(
        &manifest_with(r#""sandbox":"off""#),
        BUNDLE,
        &open_limits(),
        &RefuseAllSignatures,
    )
    .unwrap_err();
    assert!(err.contains("manifest is not valid"), "{err}");
}

#[test]
fn a_foreign_schema_is_refused() {
    let json = manifest_with("").replace(r#""schema":1"#, r#""schema":2"#);
    let err = admit_and_resolve(&json, BUNDLE, &open_limits(), &RefuseAllSignatures).unwrap_err();
    assert!(err.contains("schema 2 is not 1"), "{err}");
}

// ------------------------------------------------------------ capabilities

#[test]
fn empty_declarations_do_not_remove_public_runtime_or_storage() {
    let policy = resolve("").unwrap();
    assert!(policy.capabilities.is_empty());
    assert!(!policy.allows("net"));
    assert!(!policy.may_prompt);
    assert!(policy.agent.is_none());
    let settings = policy.isolate_settings(Path::new("/data"));
    assert!(settings.allow_net);
    assert_eq!(settings.storage_root().as_deref(), Some(Path::new("/data/forecast")));
    assert_eq!(settings.granted_storage_quota(), HostLimits::default().max_storage_bytes);
}

#[test]
fn an_unknown_capability_is_refused() {
    let err = resolve(r#""capabilities":["root"]"#).unwrap_err();
    assert!(err.contains("unknown capability"), "{err}");
}

#[test]
fn prompting_is_a_capability_of_its_own() {
    assert!(!resolve(r#""capabilities":["storage"]"#).unwrap().may_prompt);
    assert!(resolve(r#""capabilities":["prompt"]"#).unwrap().may_prompt);
}

// ----------------------------------------------------------------- network

#[test]
fn only_listed_hosts_are_reachable() {
    let policy = resolve(r#""capabilities":["net"],"network":{"hosts":["api.weather.example"]}"#).unwrap();
    assert!(policy.allows_host("api.weather.example"));
    assert!(!policy.allows_host("evil.example"));
}

/// The host list is a declaration (makepad#117, OctoSense #450): `net`
/// alone hands the isolate its network module.
#[test]
fn the_net_capability_without_hosts_gets_the_network_module() {
    let policy = resolve(r#""capabilities":["net"]"#).unwrap();
    assert!(!policy.allows_host("api.weather.example"), "no host is declared");
    assert!(policy.isolate_settings(Path::new("/data")).allow_net);
}

#[test]
fn hosts_without_net_are_valid_disclosures() {
    let policy = resolve(r#""network":{"hosts":["api.weather.example"]}"#).unwrap();
    assert!(policy.capabilities.is_empty());
    assert!(policy.hosts.contains("api.weather.example"));
    assert!(policy.isolate_settings(Path::new("/data")).allow_net);
}

#[test]
fn a_host_may_not_be_a_wildcard_a_url_a_port_or_a_scheme() {
    for host in ["*.example", "https://api.example", "api.example/path", "api.example:8443", "..", ".api.example"] {
        let body = format!(r#""capabilities":["net"],"network":{{"hosts":["{host}"]}}"#);
        assert!(resolve(&body).is_err(), "host {host:?} should be refused");
    }
}

// ------------------------------------------------------------------ quotas

#[test]
fn a_quota_above_the_hosts_ceiling_is_clamped_not_granted() {
    let body = r#""storage":{"max_bytes":999999999},"compute":{"instruction_budget":999999999999,"memory_bytes":999999999}"#;
    let limits = open_limits();
    let policy = admit_and_resolve(&manifest_with(body), BUNDLE, &limits, &RefuseAllSignatures).unwrap();
    assert_eq!(policy.storage_bytes, limits.max_storage_bytes);
    assert_eq!(policy.instruction_budget, limits.max_instruction_budget);
    assert_eq!(policy.memory_bytes, limits.max_memory_bytes);
}

#[test]
fn a_smaller_quota_is_honoured() {
    let policy = resolve(r#""storage":{"max_bytes":4096}"#).unwrap();
    assert_eq!(policy.storage_bytes, 4096);
}

// ---------------------------------------------------------------- identity

#[test]
fn an_id_may_not_navigate_the_filesystem() {
    for id in ["../escape", ".hidden", "Forecast", "with space", ""] {
        let json = manifest_with("").replace(r#""id":"forecast""#, &format!(r#""id":"{id}""#));
        assert!(
            admit_and_resolve(&json, BUNDLE, &open_limits(), &RefuseAllSignatures).is_err(),
            "id {id:?} should be refused"
        );
    }
}

#[test]
fn the_jail_is_one_directory_per_app() {
    let policy = resolve("").unwrap();
    assert_eq!(policy.jail_root(Path::new("/data/apps")), Path::new("/data/apps/forecast"));
}

// ------------------------------------------------------------------- agent

#[test]
fn full_access_is_not_expressible_in_a_manifest() {
    let err = resolve(r#""agent":{"profile":"full-access"}"#).unwrap_err();
    assert!(err.contains("manifest is not valid"), "{err}");
}

#[test]
fn an_agent_may_only_use_tools_the_host_offers_contained_apps() {
    let err = resolve(r#""agent":{"profile":"read-only","tools":["shell"]}"#).unwrap_err();
    assert!(err.contains("may keep only ask_user_question"), "{err}");
    let err = resolve(r#""agent":{"profile":"read-only","tools":["mail.send"]}"#).unwrap_err();
    assert!(err.contains("does not offer contained apps"), "{err}");
}

/// Of the octos kernel's own tools, a contained app's agent may keep only
/// `ask_user_question` (a question to the person, no side effect). Every
/// other kernel tool is refused, even by a host that lists it.
#[test]
fn an_agent_may_keep_ask_user_question_and_no_other_kernel_tool() {
    assert_eq!(KERNEL_TOOLS, ["ask_user_question"]);
    let policy = resolve(r#""agent":{"profile":"read-only","tools":["ask_user_question","net.fetch"]}"#).unwrap();
    assert_eq!(policy.agent.unwrap().tools.into_iter().collect::<Vec<_>>(), ["ask_user_question", "net.fetch"]);
    let generous = open_limits().with_offered_tools(["ask_user_question", "shell", "read_file", "web_fetch", "peer_send_input", "spawn", "save_memory"]);
    for tool in ["shell", "read_file", "web_fetch", "peer_send_input", "spawn", "save_memory", "ask_user_questions", "Ask_user_question"] {
        let body = format!(r#""agent":{{"profile":"read-only","tools":["{tool}"]}}"#);
        let err = admit_and_resolve(&manifest_with(&body), BUNDLE, &generous, &RefuseAllSignatures).unwrap_err();
        assert!(err.contains("may keep only ask_user_question"), "{tool}: {err}");
    }
    // A system app's ceilings offer it too.
    let system = HostLimits::system();
    assert!(system.offered_tools.iter().any(|t| t == "ask_user_question"));
    let body = r#""agent":{"profile":"read-only","tools":["ask_user_question"]}"#;
    assert!(admit_and_resolve(&manifest_with(body), BUNDLE, &system, &RefuseAllSignatures).is_ok());
    // The store says it in plain words.
    let manifest = AppManifest::parse(&manifest_with(body)).unwrap();
    let lines = privacy_summary(&manifest);
    assert!(lines.contains(&"Its assistant may ask you questions.".to_string()), "{lines:?}");
    assert!(lines.contains(&"Runs an assistant limited to this app's own data.".to_string()), "{lines:?}");
}

#[test]
fn the_agents_workspace_is_the_apps_own_jail_and_nothing_else_is_readable() {
    let body = r#""capabilities":["net"],"network":{"hosts":["api.weather.example"]},
        "agent":{"profile":"workspace-write-never-ask","tools":["net.fetch"],"max_iterations":3}"#;
    let policy = resolve(body).unwrap();
    let session = policy.session_profile(Path::new("/data/apps")).unwrap();
    assert_eq!(session.workspace, Path::new("/data/apps/forecast"));
    assert!(session.read_allow_paths.is_empty());
    assert_eq!(session.mode, "workspace-write-never");
    assert_eq!(session.max_iterations, 3);
    // The agent reaches exactly the hosts the app does.
    assert_eq!(session.hosts, vec!["api.weather.example".to_string()]);
}

#[test]
fn an_agents_budget_is_clamped_like_any_other_quota() {
    let limits = open_limits();
    let body = r#""agent":{"profile":"read-only","max_iterations":9999,"token_budget":99999999}"#;
    let policy = admit_and_resolve(&manifest_with(body), BUNDLE, &limits, &RefuseAllSignatures).unwrap();
    let agent = policy.agent.unwrap();
    assert_eq!(agent.max_iterations, limits.max_iterations);
    assert_eq!(agent.token_budget, limits.max_token_budget);
}

#[test]
fn every_agent_session_carries_the_app_it_acts_for() {
    let policy = resolve(r#""agent":{"profile":"read-only"}"#).unwrap();
    let session = policy.session_profile(Path::new("/data/apps")).unwrap();
    assert_eq!(session.session_id, "forecast.agent");
    assert_eq!(session.provenance.app_id, "forecast");
    assert_eq!(session.provenance.app_version, "1.0.0");
}

// ------------------------------------------------- the two containers agree

#[test]
fn the_isolate_and_the_session_come_from_the_same_declaration() {
    let body = r#""capabilities":["storage","net","prompt"],"network":{"hosts":["a.example","b.example"]},
        "storage":{"max_bytes":8192},"agent":{"profile":"workspace-write","tools":["storage.read","net.fetch"]}"#;
    let policy = resolve(body).unwrap();
    let isolate = policy.isolate_settings(Path::new("/data/apps"));
    let session = policy.session_profile(Path::new("/data/apps")).unwrap();
    assert_eq!(isolate.jail_root, session.workspace, "the agent writes where the app writes");
    assert_eq!(isolate.storage_quota, 8192);
    assert!(isolate.allow_net);
    assert!(isolate.host_prompts);
    assert_eq!(session.hosts.len(), 2);
    assert_eq!(isolate.capabilities, vec!["net".to_string(), "prompt".to_string(), "storage".to_string()]);
}

#[test]
fn the_surface_not_the_prompt_capability_decides_whether_a_service_may_raise_a_sheet() {
    // Mail raises its own sign-in sheet without asking for `prompt`, which
    // names an app's own questions. A foreground isolate may prompt; a host
    // running the app in the background turns that off itself.
    let policy = resolve(r#""capabilities":["storage","mail"]"#).unwrap();
    assert!(!policy.may_prompt, "the capability is still what the manifest asked for");
    assert!(policy.isolate_settings(Path::new("/data/apps")).host_prompts);
}

#[test]
fn an_app_gets_bounded_private_storage_with_or_without_a_declaration() {
    // Storage is always app-scoped and bounded; declarations do not
    // control whether a jail exists.
    let root = Path::new("/data/apps");
    let without = resolve(r#""capabilities":["net"],"network":{"hosts":["api.example.com"]}"#).unwrap();
    assert_eq!(without.isolate_settings(root).storage_root(), Some(root.join("forecast")));
    assert_eq!(without.isolate_settings(root).granted_storage_quota(), HostLimits::default().max_storage_bytes);
    let with = resolve(r#""capabilities":["storage"]"#).unwrap();
    let settings = with.isolate_settings(root);
    assert_eq!(settings.storage_root(), Some(settings.jail_root.clone()));
    assert!(settings.storage_quota > 0);
    assert_eq!(settings.granted_storage_quota(), settings.storage_quota);
    assert_eq!(settings.jail_root, root.join("forecast"));
}

#[test]
fn a_directory_bundle_is_admitted_by_its_precomputed_digest() {
    let digest = bundle_digest(BUNDLE);
    let policy =
        admit_and_resolve_dir(&manifest_with(""), &digest, &open_limits(), &RefuseAllSignatures).unwrap();
    assert_eq!(policy.app_id, "forecast");
    // And a digest for different content is still refused.
    let err = admit_and_resolve_dir(&manifest_with(""), &bundle_digest(b"other"), &open_limits(), &RefuseAllSignatures)
        .unwrap_err();
    assert!(err.contains("does not match the manifest"), "{err}");
}

// ------------------------------------------------------- host services

#[test]
fn each_published_assistant_service_is_admitted_by_its_exact_name() {
    for service in OCTOS_SERVICES {
        let policy = resolve(&format!(r#""capabilities":["{service}"]"#)).unwrap();
        assert!(policy.allows(service), "{service}");
        assert_eq!(policy.capabilities.len(), 1, "{service} grants nothing else");
    }
}

#[test]
fn an_assistant_prefix_or_invented_service_is_refused() {
    for name in ["octos.", "octos.session", "octos.admin", "octos.peer.prepare", "octos.session.open.all", "Octos.turn.start"] {
        let err = resolve(&format!(r#""capabilities":["{name}"]"#)).unwrap_err();
        assert!(err.contains("unknown capability"), "{name}: {err}");
    }
}

#[test]
fn reading_assistant_history_does_not_grant_starting_a_turn() {
    let policy = resolve(r#""capabilities":["octos.session.open","octos.session.history"]"#).unwrap();
    assert!(policy.allows("octos.session.history"));
    assert!(!policy.allows("octos.turn.start"));
    assert!(!policy.allows("octos.turn.interrupt"));
    assert!(policy.agent.is_none(), "a service grant is not an app agent");
}

#[test]
fn matrix_services_are_exact_and_reading_does_not_grant_sending() {
    let policy = resolve(r#""capabilities":["matrix.read_messages","matrix.profile"]"#).unwrap();
    assert!(policy.allows("matrix.read_messages"));
    assert!(!policy.allows("matrix.send_message"));
    let err = resolve(r#""capabilities":["matrix.admin"]"#).unwrap_err();
    assert!(err.contains("unknown capability"), "{err}");
}

#[test]
fn the_store_says_what_a_service_grant_allows() {
    let manifest = AppManifest::parse(&manifest_with(
        r#""capabilities":["matrix.read_messages","matrix.send_message","octos.session.history"]"#,
    ))
    .unwrap();
    let lines = privacy_summary(&manifest);
    assert!(lines.iter().any(|l| l.starts_with("Reads from your Matrix account")), "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("Acts on your Matrix account")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("cannot ask it to work")), "{lines:?}");
    let manifest = AppManifest::parse(&manifest_with(r#""capabilities":["octos.turn.start"]"#)).unwrap();
    assert!(privacy_summary(&manifest).iter().any(|l| l.contains("keys stay with the device")));
}

#[test]
fn glance_is_its_own_capability_and_implies_nothing_else() {
    let policy = resolve(r#""capabilities":["glance"]"#).unwrap();
    assert!(policy.allows("glance"));
    assert_eq!(policy.capabilities.len(), 1, "glance grants nothing else");
    assert!(!policy.allows("news") && !policy.allows("net") && !policy.allows("storage"));
    assert!(!policy.may_prompt);
    // Only the exact name: a method or a near name is not a grant.
    for name in ["glance.", "glance.publish", "glance.list", "Glance", "glances"] {
        let err = resolve(&format!(r#""capabilities":["{name}"]"#)).unwrap_err();
        assert!(err.contains("unknown capability"), "{name}: {err}");
    }
}

#[test]
fn model_is_its_own_capability_and_implies_nothing_else() {
    let policy = resolve(r#""capabilities":["model"]"#).unwrap();
    assert!(policy.allows("model"));
    assert_eq!(policy.capabilities.len(), 1, "model grants nothing else");
    // Not the provider manager, not the assistant, not the network.
    assert!(!policy.allows("llm") && !policy.allows("net") && !policy.allows("octos.turn.start"));
    assert!(!policy.may_prompt);
    for name in ["model.", "model.complete", "Model", "models"] {
        let err = resolve(&format!(r#""capabilities":["{name}"]"#)).unwrap_err();
        assert!(err.contains("unknown capability"), "{name}: {err}");
    }
    let manifest = AppManifest::parse(&manifest_with(r#""capabilities":["model"]"#)).unwrap();
    assert!(privacy_summary(&manifest).iter().any(|l| l.contains("AI provider you configured")));
}

// ------------------------------------------------------- research and crawl

#[test]
fn research_is_its_own_capability_with_octos_scope() {
    let policy = resolve(
        r#""capabilities":["research"],"research":{"langs":["en","zh_cn"],"regions":["us"],"categories":["news"],"max_age_days":7}"#,
    )
    .unwrap();
    assert!(policy.allows("research"));
    assert_eq!(policy.capabilities.len(), 1, "research grants nothing else");
    // Not crawling, not the app's own network, not the news service.
    assert!(!policy.allows("crawl") && !policy.allows("net") && !policy.allows("news"));
    assert!(policy.hosts.is_empty());
    let scope = policy.app.research.expect("the scope is granted with the capability");
    // Normalised as octos's Scope::from_grant does.
    assert_eq!(scope.langs, ["en", "zh-CN"]);
    assert_eq!(scope.regions, ["US"]);
    assert_eq!(scope.max_results, 20, "octos's default");
    assert!(!scope.crawls());
    for name in ["research.", "research.search", "deep_research", "Research", "crawl.", "deep_crawl"] {
        let err = resolve(&format!(r#""capabilities":["{name}"],"research":{{}}"#)).unwrap_err();
        assert!(err.contains("unknown capability"), "{name}: {err}");
    }
    // An app without either capability gets no scope.
    assert!(resolve(r#""capabilities":["storage"]"#).unwrap().research.is_none());
}

#[test]
fn research_and_crawl_need_a_scope_and_a_scope_needs_one_of_them() {
    for cap in ["research", "crawl"] {
        let err = resolve(&format!(r#""capabilities":["{cap}"]"#)).unwrap_err();
        assert!(err.contains("declares no research scope"), "{cap}: {err}");
    }
    let scoped = resolve(r#""research":{"langs":["en"]}"#).unwrap();
    assert_eq!(scoped.research.as_ref().unwrap().langs, ["en"]);
    // `{}` is a scope: no limits, which the store then says in words.
    let policy = resolve(r#""capabilities":["research"],"research":{}"#).unwrap();
    assert!(policy.research.is_some());
}

#[test]
fn the_scope_is_checked_with_octos_rules() {
    let refused = |scope: &str| resolve(&format!(r#""capabilities":["research"],"research":{scope}"#)).unwrap_err();
    assert!(refused(r#"{"langs":["english"]}"#).contains("bad language"));
    assert!(refused(r#"{"regions":["USA"]}"#).contains("bad region"));
    assert!(refused(r#"{"categories":["video"]}"#).contains("unknown category"));
    assert!(refused(r#"{"max_results":0}"#).contains("max_results must be > 0"));
    assert!(refused(r#"{"domains_deny":["https://x.com/"]}"#).contains("bare domain"));
    // Unknown fields are refused: the schema is exactly octos's.
    let err = refused(r#"{"langs":["en"],"sites":["x.com"]}"#);
    assert!(err.contains("manifest is not valid") && err.contains("unknown field"), "{err}");
}

#[test]
fn the_old_toolbox_scope_shape_is_refused_with_the_fields_to_rename() {
    let err = resolve(
        r#""capabilities":["research"],"research":{"languages":["en"],"allowed_domains":["bbc.co.uk"],"recency_hours":24}"#,
    )
    .unwrap_err();
    assert!(err.contains("old toolbox shape"), "{err}");
    assert!(err.contains("`languages` is now `langs`"), "{err}");
    assert!(err.contains("`allowed_domains` is now `domains_allow`"), "{err}");
    assert!(err.contains("`recency_hours` is now `max_age_days`"), "{err}");
}

#[test]
fn crawl_needs_its_limits_and_the_limits_need_crawl() {
    let policy = resolve(r#""capabilities":["crawl"],"research":{"domains_allow":["docs.rs"],"max_depth":2,"max_pages":40}"#).unwrap();
    assert!(policy.allows("crawl"));
    assert!(!policy.allows("research"), "crawl does not imply research");
    assert_eq!(policy.capabilities.len(), 1);
    assert!(policy.app.research.unwrap().crawls());
    for limits in [r#""max_depth":2"#, r#""max_pages":40"#, r#""max_depth":0,"max_pages":40"#, ""] {
        let err = resolve(&format!(r#""capabilities":["crawl"],"research":{{{limits}}}"#)).unwrap_err();
        assert!(err.contains("needs max_depth and max_pages above 0"), "{limits}: {err}");
    }
    // research alone may not carry crawl limits: octos would crawl on them.
    let scoped = resolve(r#""research":{"max_depth":1,"max_pages":5}"#).unwrap();
    assert!(scoped.research.as_ref().unwrap().crawls());
    let both = resolve(r#""capabilities":["research","crawl"],"research":{"max_depth":1,"max_pages":5}"#).unwrap();
    assert!(both.allows("research") && both.allows("crawl"));
}

#[test]
fn a_manifest_without_a_research_scope_signs_as_it_did() {
    let manifest = AppManifest::parse(&manifest_with(r#""capabilities":["storage"]"#)).unwrap();
    let bytes = String::from_utf8(manifest.signing_bytes().unwrap()).unwrap();
    assert!(!bytes.contains("research"), "{bytes}");
    // A scope round-trips in octos's shape, defaults left out.
    let manifest = AppManifest::parse(&manifest_with(
        r#""capabilities":["research"],"research":{"langs":["en"],"max_age_days":3}"#,
    ))
    .unwrap();
    let bytes = String::from_utf8(manifest.signing_bytes().unwrap()).unwrap();
    assert!(bytes.contains(r#""research":{"langs":["en"],"max_age_days":3,"max_results":20}"#), "{bytes}");
}

#[test]
fn the_store_says_the_research_scope_in_plain_words() {
    let manifest = AppManifest::parse(&manifest_with(
        r#""capabilities":["research","crawl"],"research":{"langs":["en","zh"],"categories":["news"],"max_age_days":7,"max_depth":2,"max_pages":20}"#,
    ))
    .unwrap();
    let lines = privacy_summary(&manifest);
    assert!(lines.iter().any(|l| l.starts_with("Searches news in English and Chinese, from the last 7 days")), "{lines:?}");
    assert!(
        lines.iter().any(|l| l.starts_with("Crawls websites, following links up to 2 deep and reading up to 20 pages a crawl, on any site")
            && l.contains("reaches more of the web than searching")),
        "{lines:?}"
    );
}

// ----------------------------------------------------------------- storage

#[test]
fn the_storage_block_carries_accounts_the_agent_workspace_and_a_cache_ceiling() {
    // OctoSense ADR 0004 §11: the same block as native-apps.json, less
    // `external`, which only a reviewed native app may declare.
    let body = r#""storage":{"max_bytes":4096,"accounts":true,"agent_workspace":"none","cache_max_bytes":2048}"#;
    let manifest = AppManifest::parse(&manifest_with(body)).unwrap();
    assert!(manifest.storage.accounts);
    assert_eq!(manifest.storage.agent_workspace, Some(AgentWorkspace::None));
    assert_eq!(manifest.storage.cache_max_bytes, Some(2048));
    assert_eq!(resolve(body).unwrap().storage_bytes, 4096);
    assert!(AppManifest::parse(&manifest_with(r#""storage":{"agent_workspace":"account"}"#)).is_ok());

    let err = resolve(r#""storage":{"external":["home:rw"]}"#).unwrap_err();
    assert!(err.contains("unknown field `external`"), "{err}");
    let err = resolve(r#""storage":{"agent_workspace":"everything"}"#).unwrap_err();
    assert!(err.contains("manifest is not valid"), "{err}");
    let err = resolve(r#""storage":{"cache_max_bytes":0}"#).unwrap_err();
    assert!(err.contains("cache_max_bytes must be positive"), "{err}");
}

#[test]
fn a_manifest_without_the_new_storage_fields_signs_as_before() {
    let with = AppManifest::parse(&manifest_with(r#""storage":{"max_bytes":4096}"#)).unwrap();
    let bytes = String::from_utf8(with.signing_bytes().unwrap()).unwrap();
    assert!(bytes.contains(r#""storage":{"max_bytes":4096}"#), "{bytes}");
}

#[test]
fn a_native_apps_id_is_refused_at_resolve() {
    let json = manifest_with("").replace(r#""id":"forecast""#, r#""id":"terminal""#);
    let err = admit_and_resolve(&json, BUNDLE, &open_limits(), &RefuseAllSignatures).unwrap_err();
    assert!(err.contains("reserved"), "{err}");
    let json = manifest_with("").replace(r#""id":"forecast""#, r#""id":"com.example.rinx""#);
    let err = admit_and_resolve(&json, BUNDLE, &open_limits(), &RefuseAllSignatures).unwrap_err();
    assert!(err.contains("reserved"), "{err}");
}

// ---------------------------------------------------------------- contract

#[test]
fn the_app_part_is_the_contracts_policy() {
    let body = r#""capabilities":["storage","net"],"network":{"hosts":["api.weather.example"]},"agent":{"profile":"read-only"}"#;
    let policy = resolve(body).unwrap();
    let manifest = AppManifest::parse(&manifest_with(body)).unwrap();
    assert_eq!(policy.app, contract::policy::resolve(&manifest, &open_limits()).unwrap());
    assert!(policy.agent.is_some(), "the agent is the host's part");
}

#[test]
fn the_default_tools_offer_every_kernel_tool_a_contained_agent_may_keep() {
    for tool in KERNEL_TOOLS {
        assert!(HostLimits::default().offered_tools.iter().any(|t| t == tool), "{tool}");
    }
}

#[test]
fn runtime_flags_cannot_expose_host_private_data_or_bypass_device_consent() {
    use octosense_app_policy::containers::public_runtime_capabilities;
    let declarations = vec!["camera".into(), "profile".into(), "agent".into()];
    let standalone = public_runtime_capabilities(&declarations, false, false);
    for flag in ["net", "web", "storage"] { assert!(standalone.iter().any(|v| v == flag)); }
    for flag in ["location", "camera", "microphone", "library", "profile", "agent"] {
        assert!(!standalone.iter().any(|v| v == flag), "{flag}");
    }
    let consent_host = public_runtime_capabilities(&[], true, true);
    for flag in ["location", "camera", "microphone", "library"] { assert!(consent_host.iter().any(|v| v == flag)); }
    for flag in ["profile", "agent", "llm", "ledger.read"] { assert!(!consent_host.iter().any(|v| v == flag)); }
}

#[test]
fn old_runtime_or_missing_device_broker_never_activates_capture_side_effects() {
    use octosense_app_policy::containers::public_runtime_capabilities;
    let declared = vec!["camera".into(), "microphone".into(), "library".into()];
    let old_runtime = public_runtime_capabilities(&declared, true, false);
    assert!(old_runtime.iter().any(|v| v == "camera"));
    assert!(old_runtime.iter().any(|v| v == "location"));
    for flag in ["microphone", "library"] { assert!(!old_runtime.iter().any(|v| v == flag)); }
    let no_broker = public_runtime_capabilities(&declared, false, true);
    for flag in ["camera", "location", "microphone", "library"] { assert!(!no_broker.iter().any(|v| v == flag)); }
    // Declaring a capability does not manufacture an implemented host ABI.
    assert_eq!(old_runtime, public_runtime_capabilities(&[], true, false));
}
