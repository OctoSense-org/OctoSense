//! The system toolbox for app agents (feature `toolbox-peers`): the grants,
//! and the executor the shell's host-tool relay routes the `toolbox`
//! owner's calls to, driven through main's broker (its registration after
//! `peer/prepare`, its `peer/tool/call` handling) against a scripted kernel
//! speaking octos UPCR-2026-034/035. The `ToolHost` here stands in for the
//! shell's relay (`crates/shell/src/host_tools`, tested there): the
//! toolbox's catalog narrowed to the app's grant, nothing before consent.
//! The toolbox runs over its fixture backends (recorded searches and pages,
//! `FakeModel`) or the `model` service's host over a fake provider. No
//! network, no kernel binary.
#![cfg(feature = "toolbox-peers")]

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use octosense_ai_host::app_peers::broker::{BoxFuture, Broker, BrokerConfig, Connector, Link, ToolHostHandle};
use octosense_ai_host::app_peers::host_tools::{HostToolCall, ToolExecutor, ToolHost, ToolOutcome, ToolReply};
use octosense_ai_host::app_peers::{Deployment, OCTOS_SERVICES};
use octosense_ai_host::toolbox_peers::{catalog, ModelHostClient, ToolboxExecutor, ToolboxGrant, OWNER};
use octosense_llm_config::Provider;
use octosense_llm_service::complete::{self, ledger::Limits, Candidate, ModelHost, Providers, Transport};
use octosense_toolbox::fixture::{self, FixtureBackend, FixtureCase, FixtureData};
use octosense_toolbox::host::CallContext;
use octosense_toolbox::peer::{PeerToolbox, DEEP_CRAWL, FORK, RUN, SEARCH, WEB_READ};
use octosense_toolbox::research::{ModelClient, ModelRequest, ModelTask};
use octosense_toolbox::Library;
use serde_json::{json, Value};
use tokio::sync::mpsc;

const TOKEN: &str = "fixture-host-token";
const APP: &str = "os.news";

// ---- a scripted kernel ---------------------------------------------------------

#[derive(Default)]
struct Script {
    calls: Vec<(String, Value)>,
    out: Option<mpsc::UnboundedSender<String>>,
}

struct FakeConnector(Arc<Mutex<Script>>);

struct FakeLink {
    to_kernel: mpsc::UnboundedSender<String>,
    from_kernel: mpsc::UnboundedReceiver<String>,
}

impl Link for FakeLink {
    fn send(&mut self, frame: String) -> Result<(), String> {
        self.to_kernel.send(frame).map_err(|_| "gone".to_owned())
    }
    fn recv(&mut self) -> BoxFuture<'_, Result<String, String>> {
        Box::pin(async move { self.from_kernel.recv().await.ok_or_else(|| "closed".to_owned()) })
    }
}

impl Connector for FakeConnector {
    fn available(&self) -> Result<(), String> {
        Ok(())
    }
    fn connect(&self) -> BoxFuture<'static, Result<Box<dyn Link>, String>> {
        let script = self.0.clone();
        Box::pin(async move {
            let (to_kernel, mut kernel_in) = mpsc::unbounded_channel::<String>();
            let (kernel_out, from_kernel) = mpsc::unbounded_channel::<String>();
            script.lock().unwrap().out = Some(kernel_out.clone());
            tokio::spawn(async move {
                while let Some(frame) = kernel_in.recv().await {
                    let frame: Value = serde_json::from_str(&frame).unwrap();
                    let Some(method) = frame["method"].as_str().map(str::to_owned) else { continue };
                    let params = frame["params"].clone();
                    script.lock().unwrap().calls.push((method.clone(), params.clone()));
                    let result = match method.as_str() {
                        "peer/prepare" => json!({"slug": "news-1", "cwd": "/kernel/ws",
                            "memory_namespace": params["memory_namespace"], "host_token": TOKEN}),
                        "peer/tools/register" => json!({"slug": "news-1", "version": 1, "tools": params["tools"], "generic_tools": null, "applies": "next_turn"}),
                        "peer/tool/result" => json!({"call_id": params["call_id"], "accepted": true}),
                        _ => json!({}),
                    };
                    let _ = kernel_out.send(json!({"jsonrpc": "2.0", "id": frame["id"], "result": result}).to_string());
                }
            });
            Ok(Box::new(FakeLink { to_kernel, from_kernel }) as Box<dyn Link>)
        })
    }
    fn owns_runtime(&self) -> bool {
        false
    }
    fn shutdown(&self) {}
}

// ---- the shell's relay, as far as the toolbox needs it --------------------------

/// The relay's part for the `toolbox` owner (`crates/shell/src/host_tools`):
/// the catalog narrowed to the app's grant, offered and run only once the
/// person allowed the app's agent; calls and cancels go to the executor.
struct Relay {
    executor: ToolboxExecutor,
    consent: AtomicBool,
    ran: Mutex<Vec<String>>,
}

impl ToolHost for Relay {
    fn declarations(&self, app_id: &str, _account: &str) -> Result<Vec<Value>, String> {
        if !self.consent.load(Ordering::SeqCst) {
            return Ok(Vec::new());
        }
        let granted = self.executor.tools(app_id);
        Ok(catalog().into_iter().filter(|d| d["name"].as_str().is_some_and(|n| granted.contains(n))).collect())
    }
    fn tool_call(&self, call: HostToolCall, reply: ToolReply) {
        if !self.consent.load(Ordering::SeqCst) {
            reply.finish(ToolOutcome::error("consent_pending", "the person has not allowed this app's agent"));
            return;
        }
        if call.app != OWNER || !self.executor.tools(&call.calling_app).contains(call.name.as_str()) {
            reply.finish(ToolOutcome::error("not_granted", format!("{} is not granted", call.name)));
            return;
        }
        self.ran.lock().unwrap().push(call.name.clone());
        self.executor.execute(call, reply);
    }
    fn tool_cancel(&self, _app_id: &str, call_id: &str, _reason: &str) {
        self.executor.cancel(call_id);
    }
}

fn relay(executor: ToolboxExecutor) -> Arc<Relay> {
    Arc::new(Relay { executor, consent: AtomicBool::new(true), ran: Mutex::new(Vec::new()) })
}

/// A broker for `os.news` whose tool host is `host`, bound to its peer.
fn bound(host: Arc<Relay>) -> (Broker, Arc<Mutex<Script>>) {
    let script = Arc::new(Mutex::new(Script::default()));
    let services: BTreeSet<String> = OCTOS_SERVICES.iter().map(|s| s.to_string()).collect();
    let mut cfg = BrokerConfig::new(Deployment::Hosted, "_main", "_main:api:octosense#system", APP, "News", services);
    cfg.tool_host = Some(ToolHostHandle(host));
    let broker = Broker::new(cfg, Arc::new(FakeConnector(script.clone())));
    octosense_ai_host::app_peers::OctosAppService::set_account(&broker, Some("device"));
    broker.bind().unwrap();
    (broker, script)
}

fn registered(script: &Arc<Mutex<Script>>) -> Vec<Value> {
    let calls = script.lock().unwrap().calls.clone();
    let (_, params) = calls.iter().rev().find(|(m, _)| m == "peer/tools/register").expect("a registration");
    assert_eq!(params["host_token"], TOKEN);
    assert!(params.get("generic_tools").is_none(), "omitted: the peer keeps its kernel roster");
    params["tools"].as_array().unwrap().clone()
}

fn registered_names(script: &Arc<Mutex<Script>>) -> Vec<String> {
    registered(script).iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect()
}

fn inject(script: &Arc<Mutex<Script>>, method: &str, params: Value) {
    let out = script.lock().unwrap().out.clone().expect("a link");
    out.send(json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string()).unwrap();
}

static CALLS: AtomicU32 = AtomicU32::new(0);

/// Sends one `peer/tool/call` for a toolbox tool, as the kernel does (the
/// owning app from the declaration); its call id.
fn call(script: &Arc<Mutex<Script>>, name: &str, args: Value) -> String {
    let n = CALLS.fetch_add(1, Ordering::Relaxed);
    let call_id = format!("call-{n}");
    inject(
        script,
        "peer/tool/call",
        json!({"peer": "news-1", "session_id": "_main:api:octosense#peer-news-1", "context_id": null,
               "turn_id": "turn-1", "call_id": call_id, "tool_call_id": format!("tc-{n}"), "args_digest": "d",
               "name": name, "app": OWNER, "caller": {"kind": "app_peer", "peer": "news-1"},
               "args": args, "risk": "read", "confirm_required": false,
               "timeout_ms": 60_000, "tools_version": 1}),
    );
    call_id
}

fn result_of(script: &Arc<Mutex<Script>>, call_id: &str) -> Option<Value> {
    script
        .lock()
        .unwrap()
        .calls
        .iter()
        .find(|(m, p)| m == "peer/tool/result" && p["call_id"] == call_id)
        .map(|(_, p)| p.clone())
}

fn wait(script: &Arc<Mutex<Script>>, call_id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(result) = result_of(script, call_id) {
            return result;
        }
        assert!(Instant::now() < deadline, "no result for {call_id}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

// ---- fixtures -------------------------------------------------------------------

fn temp(tag: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("toolbox-peers-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn case(template: &str, name: &str) -> FixtureCase {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../toolbox/templates")
        .join(template)
        .join("fixtures")
        .join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn manifest(tools: &[&str], scope: Option<Value>) -> Value {
    let mut m = json!({"id": APP, "capabilities": [], "agent": {"tools": tools}});
    if let Some(scope) = scope {
        m["research"] = scope;
    }
    m
}

/// The toolbox executor over recorded searches and pages and the fake model.
fn fixture_executor(root: &Path, data: FixtureData) -> ToolboxExecutor {
    ToolboxExecutor::new(
        root,
        Arc::new(move || {
            let library = Library::builtin().map_err(|e| e.to_string())?;
            let backend = Arc::new(FixtureBackend::new(data.clone()));
            Ok(PeerToolbox::from_host(library, fixture::host(&data), backend))
        }),
    )
}

/// A broker for News with `grant`, over the fixture toolbox.
fn news(root: &Path, data: FixtureData, grant: ToolboxGrant) -> (Broker, Arc<Mutex<Script>>, Arc<Relay>) {
    let executor = fixture_executor(root, data);
    executor.set_grant(APP, grant);
    let host = relay(executor);
    let (broker, script) = bound(host.clone());
    (broker, script, host)
}

// ---- registration ---------------------------------------------------------------

#[test]
fn a_peer_is_offered_exactly_its_requested_shared_tool_names() {
    let root = temp("register");
    let data = case("news-digest", "city-infrastructure").fixture;
    let names = |grant: ToolboxGrant| -> Vec<String> { registered_names(&news(&root, data.clone(), grant).1) };
    let crawl_scope = Some(json!({"max_depth": 2, "max_pages": 5}));
    let requested = [RUN, FORK, SEARCH, WEB_READ, DEEP_CRAWL];
    // Omitted, empty, and partial family disclosures have identical behavior.
    for capabilities in [None, Some(json!([])), Some(json!(["research"])), Some(json!(["research", "crawl"]))] {
        let mut m = manifest(&requested, crawl_scope.clone());
        if let Some(capabilities) = capabilities { m["capabilities"] = capabilities; }
        else { m.as_object_mut().unwrap().remove("capabilities"); }
        assert_eq!(names(ToolboxGrant::for_manifest(APP, &m)), requested);
    }
    // Selecting search never grants a write, workflow run, reader, or crawler.
    assert_eq!(names(ToolboxGrant::for_manifest(APP, &manifest(&[SEARCH], crawl_scope.clone()))), [SEARCH]);
    assert_eq!(names(ToolboxGrant::for_manifest(APP, &manifest(&[DEEP_CRAWL], crawl_scope.clone()))), [DEEP_CRAWL]);
    // Scope limits remain mandatory even for an explicitly requested crawl.
    assert!(names(ToolboxGrant::for_manifest(APP, &manifest(&[DEEP_CRAWL], None))).is_empty());
    // A disclosure alone, unknown tools, or a plain app grants no shared tool.
    let mut no_requests = manifest(&[], crawl_scope.clone());
    no_requests["capabilities"] = json!(["research", "crawl"]);
    assert!(names(ToolboxGrant::for_manifest(APP, &no_requests)).is_empty());
    no_requests["agent"] = Value::Null;
    assert!(names(ToolboxGrant::for_manifest(APP, &no_requests)).is_empty());
    assert!(names(ToolboxGrant::for_manifest(APP, &manifest(&["toolbox.unknown", "mail.send"], None))).is_empty());
    // The native host's explicit reviewed family offer is unchanged.
    assert!(names(ToolboxGrant::new(APP, ["research", "crawl"], ["storage"], crawl_scope.as_ref())).is_empty());
    assert_eq!(names(ToolboxGrant::new(APP, ["research", "crawl"], ["crawl"], crawl_scope.as_ref())), [DEEP_CRAWL]);
    assert_eq!(ToolboxGrant::for_module("rinx", &["octos.turn.start"]).tools(), BTreeSet::new());
    assert_eq!(ToolboxGrant::for_module("reference", &["research"]).tools(), BTreeSet::from([RUN, FORK, SEARCH, WEB_READ]));
    let full = ToolboxGrant::for_manifest(APP, &manifest(&requested, crawl_scope));
    for d in registered(&news(&root, data, full).1) {
        assert_eq!(d["app"], OWNER, "{d}");
        assert_eq!(d["risk"], if d["name"] == FORK { "act" } else { "read" }, "{d}");
        assert_eq!((d["background"].as_bool(), d["outward"].as_bool(), d["confirm"].as_str()), (Some(true), Some(false), Some("host")));
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn no_toolbox_tool_is_offered_or_run_before_consent() {
    let root = temp("consent");
    let case = case("news-digest", "city-infrastructure");
    let executor = fixture_executor(&root, case.fixture.clone());
    executor.set_grant(APP, ToolboxGrant::for_manifest(APP, &manifest(&[RUN, FORK, SEARCH, WEB_READ], None)));
    let host = relay(executor);
    host.consent.store(false, Ordering::SeqCst);
    let (broker, script) = bound(host.clone());
    assert!(registered_names(&script).is_empty(), "nothing offered before consent");
    let id = call(&script, SEARCH, json!({"query": "city infrastructure", "lang": "en"}));
    let refused = wait(&script, &id);
    assert_eq!(refused["error"]["kind"], "consent_pending", "{refused}");
    assert!(host.ran.lock().unwrap().is_empty(), "nothing ran");
    assert!(!root.join(".host/toolbox/os.news").exists(), "nothing written");
    // Once allowed, the next registration (a new peer or a reconnect) offers them.
    host.consent.store(true, Ordering::SeqCst);
    drop(broker);
    let (_broker, script) = bound(host);
    assert_eq!(registered_names(&script), [RUN, FORK, SEARCH, WEB_READ]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn admitted_store_agents_use_the_same_toolbox_policy_as_system_agents() {
    let store = json!({"id": "com.example.news", "capabilities": [], "agent": {"tools": [RUN, FORK, SEARCH, WEB_READ, DEEP_CRAWL]}, "research": {"max_depth": 1, "max_pages": 3}});
    let grant = ToolboxGrant::for_manifest("com.example.news", &store);
    assert_eq!(grant.grants, BTreeSet::from(["research".to_owned(), "crawl".to_owned()]));
    assert_eq!(grant.tools(), BTreeSet::from([RUN, FORK, SEARCH, WEB_READ, DEEP_CRAWL]));
    assert!(grant.notes.is_empty());
    // A manifest for another app, or a scope octos refuses, grants nothing.
    assert!(ToolboxGrant::for_manifest("os.mail", &manifest(&[RUN, FORK, SEARCH, WEB_READ], None)).is_empty());
    let bad = ToolboxGrant::for_manifest(APP, &manifest(&[RUN, FORK, SEARCH, WEB_READ], Some(json!({"languages": ["en"]}))));
    assert!(bad.is_empty() && bad.notes[0].contains("old toolbox shape"), "{:?}", bad.notes);
    let good = ToolboxGrant::for_manifest(APP, &manifest(&[RUN, FORK, SEARCH, WEB_READ], Some(json!({"langs": ["en"], "max_age_days": 2}))));
    assert_eq!(good.grants, BTreeSet::from(["research".to_owned()]));
    assert_eq!(good.scope.langs, ["en"]);
}

#[test]
fn a_search_request_never_grants_other_research_tools_to_a_forged_call() {
    let root = temp("exact-tools");
    let executor = fixture_executor(&root, case("news-digest", "city-infrastructure").fixture);
    executor.set_grant(APP, ToolboxGrant::for_manifest(APP, &manifest(&[SEARCH], Some(json!({"max_depth": 2, "max_pages": 5})))));
    assert_eq!(executor.tools(APP), BTreeSet::from([SEARCH]));
    for name in [RUN, FORK, WEB_READ, DEEP_CRAWL] {
        let sent = Arc::new(Mutex::new(Vec::<Value>::new()));
        let answer = sent.clone();
        let mut forged = HostToolCall::parse(&json!({"peer": "news-1", "session_id": "s", "turn_id": "t", "call_id": "forged", "name": name, "app": OWNER, "caller": {"kind": "app_peer"}, "args": {}})).unwrap();
        forged.calling_app = APP.into();
        executor.execute(forged, ToolReply::new("forged", move |value| answer.lock().unwrap().push(value)));
        assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted", "{name}");
    }
    assert!(!root.join(".host/toolbox/os.news").exists(), "refusals never start a worker or create results");
    let _ = std::fs::remove_dir_all(root);
}

// ---- calls ----------------------------------------------------------------------

#[test]
fn a_workflow_run_is_executed_by_the_toolbox_and_its_result_lands_in_the_host_folder() {
    let root = temp("run");
    let case = case("news-digest", "city-infrastructure");
    let (_broker, script, _) = news(&root, case.fixture.clone(), ToolboxGrant::for_manifest(APP, &manifest(&[RUN, FORK, SEARCH, WEB_READ], None)));
    let id = call(&script, RUN, json!({"id": "news-digest", "params": case.params, "run_id": "glance"}));
    let result = wait(&script, &id);
    assert_eq!(result["ok"], true, "{result}");
    let data = &result["data"];
    assert_eq!(data["status"], "ready", "{data}");
    assert_eq!(data["run_id"], "glance");
    assert_eq!(data["result"], "toolbox/runs/news-digest/glance.json");
    assert!(!data["sources"].as_array().unwrap().is_empty());
    // In the host's folder, where sys.digest reads; never in the app's jail.
    let file = octosense_ai_host::toolbox_folder(&root, APP).unwrap().join("toolbox/runs/news-digest/glance.json");
    assert_eq!(file, root.join(".host/toolbox/os.news/toolbox/runs/news-digest/glance.json"));
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(saved["app_id"], APP);
    assert!(!root.join(APP).exists(), "nothing is written into the app's jail");

    // A fork is written there too, and is runnable by its new id.
    let id = call(&script, FORK, json!({"id": "news-digest", "new_id": "my-digest"}));
    let fork = wait(&script, &id);
    assert_eq!(fork["ok"], true, "{fork}");
    assert!(root.join(".host/toolbox/os.news/toolbox/templates/my-digest/template.json").exists());

    // The single tools save their items in the host folder too.
    let id = call(&script, SEARCH, json!({"query": "city infrastructure", "lang": "en", "count": 2}));
    let search = wait(&script, &id);
    assert_eq!(search["ok"], true, "{search}");
    assert_eq!(search["data"]["items"].as_array().unwrap().len(), 2);
    let saved = search["data"]["file"].as_str().unwrap();
    assert!(saved.starts_with("research/search-"), "{saved}");
    assert!(root.join(".host/toolbox/os.news").join(saved).exists());
    let id = call(&script, WEB_READ, json!({"url": "https://example.invalid/news/buses"}));
    let read = wait(&script, &id);
    assert_eq!(read["ok"], true, "{read}");
    assert!(read["data"]["text"].as_str().unwrap().contains("electric buses"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn calls_outside_the_scope_are_refused() {
    let root = temp("scope");
    let case = case("news-digest", "city-infrastructure");
    let grant = ToolboxGrant::for_manifest(APP, &manifest(&[RUN, FORK, SEARCH, WEB_READ], Some(json!({"langs": ["en"], "domains_deny": ["example.invalid"]}))));
    let (_broker, script, _) = news(&root, case.fixture.clone(), grant);
    let id = call(&script, SEARCH, json!({"query": "infrastructure", "lang": "fr"}));
    let refused = wait(&script, &id);
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["error"]["kind"], "denied");
    assert!(refused["error"]["message"].as_str().unwrap().contains("not in this app's research grant"));
    let id = call(&script, WEB_READ, json!({"url": "https://example.invalid/news/buses"}));
    let refused = wait(&script, &id);
    assert_eq!(refused["error"]["kind"], "denied", "{refused}");
    // A template run keeps to the scope too: every read is refused, so the
    // run has nothing to digest.
    let id = call(&script, RUN, json!({"id": "news-digest", "params": case.params}));
    let run = wait(&script, &id);
    assert_eq!(run["ok"], true, "{run}");
    assert_ne!(run["data"]["status"], "ready", "{run}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_app_without_crawl_cannot_crawl_even_with_a_forged_call() {
    let root = temp("crawl");
    let data = case("news-digest", "city-infrastructure").fixture;
    let grant = ToolboxGrant::for_manifest(APP, &manifest(&[RUN, FORK, SEARCH, WEB_READ], Some(json!({"max_depth": 2, "max_pages": 5}))));
    let executor = fixture_executor(&root, data);
    executor.set_grant(APP, grant);
    let (_broker, script) = bound(relay(executor.clone()));
    assert!(!registered_names(&script).iter().any(|n| n == DEEP_CRAWL));
    // The kernel would not send it; if it did, the relay refuses it.
    let id = call(&script, DEEP_CRAWL, json!({"url": "https://example.invalid/"}));
    assert_eq!(wait(&script, &id)["error"]["kind"], "not_granted");
    // And the toolbox's executor refuses it on its own, whoever asks.
    let sent = Arc::new(Mutex::new(Vec::<Value>::new()));
    let s = sent.clone();
    let mut forged = HostToolCall::parse(&json!({"peer": "news-1", "session_id": "s", "turn_id": "t", "call_id": "forged", "name": DEEP_CRAWL, "app": OWNER, "caller": {"kind": "app_peer"}, "args": {"url": "https://example.invalid/"}})).unwrap();
    forged.calling_app = APP.into();
    executor.execute(forged, ToolReply::new("forged", move |v| s.lock().unwrap().push(v)));
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted");
    assert!(!root.join(".host/toolbox/os.news/research").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_cancelled_run_is_stopped_and_never_answered() {
    let root = temp("cancel");
    let case = case("news-digest", "city-infrastructure");
    let mut data = case.fixture.clone();
    for search in &mut data.searches {
        search.delay_ms = 5_000;
    }
    let (_broker, script, host) = news(&root, data, ToolboxGrant::for_manifest(APP, &manifest(&[RUN, FORK, SEARCH, WEB_READ], None)));
    let id = call(&script, RUN, json!({"id": "news-digest", "params": case.params, "run_id": "cancelled"}));
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(host.executor.running(), 1);
    inject(&script, "peer/tool/cancel", json!({"call_id": id, "reason": "cancelled"}));
    std::thread::sleep(Duration::from_millis(800));
    assert!(result_of(&script, &id).is_none());
    assert_eq!(host.executor.running(), 0, "stopped");
    assert!(!root.join(".host/toolbox/os.news/toolbox/runs/news-digest/cancelled.json").exists());
    let _ = std::fs::remove_dir_all(&root);
}

// ---- model calls through the model service's host ----------------------------

/// A provider answering OpenAI chat completions from a queue.
#[derive(Default)]
struct FakeProvider {
    answers: Mutex<VecDeque<String>>,
    seen: Mutex<Vec<Value>>,
}

impl Transport for FakeProvider {
    fn post(&self, _url: &str, _headers: &[(String, String)], body: &str) -> Result<(u16, Vec<u8>), String> {
        self.seen.lock().unwrap().push(serde_json::from_str(body).unwrap());
        let content = self.answers.lock().unwrap().pop_front().ok_or("connection refused")?;
        let reply = json!({"choices": [{"message": {"content": content}}], "usage": {"prompt_tokens": 100, "completion_tokens": 20}});
        Ok((200, reply.to_string().into_bytes()))
    }
}

struct Fixed;
impl Providers for Fixed {
    fn candidates(&self) -> Result<Vec<Candidate>, String> {
        Ok(vec![Candidate {
            provider: Provider::new("deepseek", Some("deepseek-v4-flash".into())),
            key: Some("sk-test-deepseek-0000aaaa1234".into()),
        }])
    }
}

fn model_host(provider: Arc<FakeProvider>) -> Arc<ModelHost> {
    Arc::new(ModelHost::new(
        &complete::Options::default().providers(Arc::new(Fixed)).transport(provider).clock(|| 1_790_000_000_000),
    ))
}

#[test]
fn template_model_calls_use_the_model_service_and_the_apps_daily_budget() {
    let root = temp("budget");
    let provider = Arc::new(FakeProvider::default());
    let host = model_host(provider.clone());
    let client = ModelHostClient::new(host.clone(), &root);
    let grant = ToolboxGrant::for_manifest(APP, &manifest(&[RUN, FORK, SEARCH, WEB_READ], None));
    let app = grant.app_context(APP, &root).unwrap();
    let ctx = CallContext {
        app: Arc::new(app),
        run_id: "r".into(),
        template_id: "news-digest".into(),
        template_digest: String::new(),
        budget: octosense_toolbox::Budget { max_calls: 8, max_model_calls: 2, max_reads: 4, max_ms: 60_000, max_concurrency: 1 },
        remaining: octosense_toolbox::host::Remaining { calls: 8, model_calls: 2, reads: 4, ms: 60_000 },
        call_index: 0,
    };
    let request = ModelRequest {
        task: ModelTask::TranslateQuery,
        system: "Translate the research query.".into(),
        user: json!({"query": "typhoon", "language": "zh"}).to_string(),
        max_output_tokens: None,
        output_schema: json!({"type": "object", "required": ["query"], "properties": {"query": {"type": "string"}}}),
    };
    provider.answers.lock().unwrap().push_back(json!({"query": "台风"}).to_string());
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let reply = runtime.block_on(client.complete(&ctx, request.clone())).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&reply).unwrap()["query"], "台风");
    // The template's own prompt and document went out, and no token cap.
    let sent = provider.seen.lock().unwrap()[0].clone();
    assert!(sent["messages"][0]["content"].as_str().unwrap().starts_with("Translate the research query."));
    assert!(sent.get("max_tokens").is_none());
    // Charged to the app in the model service's ledger, under the host dir.
    assert_eq!(host.budget(APP).calls_today, 1);
    assert_eq!(host.budget(APP).tokens_today, 120);
    let ledger: Value = serde_json::from_str(&std::fs::read_to_string(root.join(".host/model/ledger.json")).unwrap()).unwrap();
    assert_eq!(ledger["apps"][APP]["calls"], 1, "{ledger}");

    // The day's budget is spent: the next model call is refused as budget.
    host.set_limits(APP, Limits { per_minute: 6, calls_per_day: 1, tokens_per_day: 100_000 });
    let refused = runtime.block_on(client.complete(&ctx, request)).unwrap_err();
    assert!(refused.to_string().contains("budget"), "{refused}");
    assert!(matches!(refused, octosense_toolbox::HostError::Denied(_)));

    // A template run through the peer gets the refusal, not a digest.
    let data = case("news-digest", "city-infrastructure");
    let fixture = data.fixture.clone();
    let root2 = root.clone();
    let executor = ToolboxExecutor::new(
        &root,
        Arc::new(move || {
            let backend = Arc::new(FixtureBackend::new(fixture.clone()));
            let model = Arc::new(ModelHostClient::new(host.clone(), &root2));
            Ok(PeerToolbox::new(Library::builtin().map_err(|e| e.to_string())?, backend, model))
        }),
    );
    executor.set_grant(APP, grant);
    let (_broker, script) = bound(relay(executor));
    let id = call(&script, RUN, json!({"id": "news-digest", "params": data.params, "run_id": "over-budget"}));
    let run = wait(&script, &id);
    assert_eq!(run["ok"], true, "{run}");
    // news-digest lists its sources without a digest when the model call
    // fails: partial, and no model text.
    assert_eq!(run["data"]["status"], "partial", "{run}");
    assert!(run["data"]["data"]["digest"].is_null(), "{run}");
    let saved = std::fs::read_to_string(root.join(".host/toolbox/os.news/toolbox/runs/news-digest/over-budget.json")).unwrap();
    assert!(saved.contains("budget"), "the refusal is in the run's diagnostics: {saved}");
    let _ = std::fs::remove_dir_all(&root);
}
