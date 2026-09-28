//! The system toolbox as app peers' host-registered tools (feature
//! `toolbox-peers`), against a scripted kernel speaking octos
//! UPCR-2026-034/035, with the toolbox's fixture backends (recorded
//! searches and pages, `FakeModel`) or the `model` service's host over a
//! fake provider. No network, no kernel binary.
#![cfg(feature = "toolbox-peers")]

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use octosense_ai_host::app_peers::broker::{BoxFuture, Broker, BrokerConfig, Connector, Link};
use octosense_ai_host::app_peers::peer_tools::HostTools;
use octosense_ai_host::app_peers::{Deployment, OCTOS_SERVICES};
use octosense_ai_host::toolbox_peers::{
    app_context, grant_from_capabilities, grant_from_manifest, ModelHostClient, ToolboxGrant, ToolboxTools,
};
use octosense_llm_config::Provider;
use octosense_llm_service::complete::{self, ledger::Limits, Candidate, ModelHost, Providers, Transport};
use octosense_toolbox::fixture::{self, FixtureBackend, FixtureCase, FixtureData};
use octosense_toolbox::host::CallContext;
use octosense_toolbox::peer::{PeerToolbox, DEEP_CRAWL, FORK, RUN, SEARCH, WEB_READ};
use octosense_toolbox::research::{ModelClient, ModelRequest, ModelTask};
use octosense_toolbox::{AppContext, Library};
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
                    let method = frame["method"].as_str().unwrap().to_owned();
                    let params = frame["params"].clone();
                    script.lock().unwrap().calls.push((method.clone(), params.clone()));
                    let result = match method.as_str() {
                        "peer/prepare" => json!({"slug": "news-1", "cwd": "/kernel/ws",
                            "memory_namespace": params["memory_namespace"], "host_token": TOKEN}),
                        "peer/tools/register" => json!({"slug": "news-1", "version": 1, "applies": "next_turn"}),
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

/// A broker for `os.news` with `tools`, bound to its peer.
fn bound(tools: Option<Arc<dyn HostTools>>) -> (Broker, Arc<Mutex<Script>>) {
    let script = Arc::new(Mutex::new(Script::default()));
    let services: BTreeSet<String> = OCTOS_SERVICES.iter().map(|s| s.to_string()).collect();
    let mut cfg = BrokerConfig::new(Deployment::Hosted, "_main", "_main:api:octosense#system", APP, "News", services);
    cfg.host_tools = tools;
    let broker = Broker::new(cfg, Arc::new(FakeConnector(script.clone())));
    octosense_ai_host::app_peers::OctosAppService::set_account(&broker, Some("device"));
    broker.bind().unwrap();
    (broker, script)
}

fn registered(script: &Arc<Mutex<Script>>) -> Vec<String> {
    let calls = script.lock().unwrap().calls.clone();
    let (_, params) = calls.iter().find(|(m, _)| m == "peer/tools/register").expect("a registration");
    assert_eq!(params["host_token"], TOKEN);
    params["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect()
}

fn inject(script: &Arc<Mutex<Script>>, method: &str, params: Value) {
    let out = script.lock().unwrap().out.clone().expect("a link");
    out.send(json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string()).unwrap();
}

static CALLS: AtomicU32 = AtomicU32::new(0);

/// Sends one `peer/tool/call`; its call id.
fn call(script: &Arc<Mutex<Script>>, name: &str, args: Value) -> String {
    let n = CALLS.fetch_add(1, Ordering::Relaxed);
    let call_id = format!("call-{n}");
    inject(
        script,
        "peer/tool/call",
        json!({"peer": "news-1", "session_id": "_main:api:octosense#peer-news-1", "context_id": null,
               "turn_id": "turn-1", "call_id": call_id, "tool_call_id": format!("tc-{n}"), "args_digest": "d",
               "name": name, "args": args, "risk": "read", "confirm_required": false,
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

fn manifest(capabilities: &[&str], scope: Option<Value>) -> Value {
    let mut m = json!({"id": APP, "capabilities": capabilities});
    if let Some(scope) = scope {
        m["research"] = scope;
    }
    m
}

/// The toolbox over recorded searches and pages and the fake model.
fn fixture_tools(app: AppContext, data: FixtureData) -> ToolboxTools {
    ToolboxTools::new(app, move || {
        let library = Library::builtin().map_err(|e| e.to_string())?;
        let backend = Arc::new(FixtureBackend::new(data.clone()));
        Ok(PeerToolbox::from_host(library, fixture::host(&data), backend))
    })
}

fn news_app(root: &Path, grant: &ToolboxGrant) -> AppContext {
    app_context(APP, grant, root).unwrap()
}

// ---- registration ---------------------------------------------------------------

#[test]
fn the_registration_follows_the_grant() {
    let root = temp("register");
    let data = case("news-digest", "city-infrastructure").fixture;
    let names = |grant: ToolboxGrant| -> Vec<String> {
        let tools = (!grant.is_empty())
            .then(|| Arc::new(fixture_tools(news_app(&root, &grant), data.clone())) as Arc<dyn HostTools>);
        let (_broker, script) = bound(tools);
        registered(&script)
    };
    let crawl_scope = Some(json!({"max_depth": 2, "max_pages": 5}));
    // research: the templates and the single research tools.
    assert_eq!(names(grant_from_manifest(APP, &manifest(&["storage", "research"], None))), [RUN, FORK, SEARCH, WEB_READ]);
    // research and crawl, with crawl limits.
    assert_eq!(
        names(grant_from_manifest(APP, &manifest(&["research", "crawl"], crawl_scope.clone()))),
        [RUN, FORK, SEARCH, WEB_READ, DEEP_CRAWL]
    );
    // crawl alone: only the crawl.
    assert_eq!(names(grant_from_manifest(APP, &manifest(&["crawl"], crawl_scope.clone()))), [DEEP_CRAWL]);
    // crawl without limits in the scope: no crawling.
    assert!(names(grant_from_manifest(APP, &manifest(&["research", "crawl"], None))).iter().all(|n| n != DEEP_CRAWL));
    // No grant: the empty set is still registered.
    assert!(names(grant_from_manifest(APP, &manifest(&["storage", "net"], None))).is_empty());
    // A native module declaring nothing of the toolbox.
    assert!(names(grant_from_capabilities("rinx", ["octos.turn.start"])).is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn until_app_hub_verifies_the_grant_only_system_apps_get_it() {
    let store = json!({"id": "com.example.news", "capabilities": ["research", "crawl"], "research": {"max_depth": 1, "max_pages": 3}});
    let grant = grant_from_manifest("com.example.news", &store);
    assert!(grant.is_empty());
    assert!(grant.notes[0].contains("only system apps"), "{:?}", grant.notes);
    assert!(grant_from_capabilities("rinx", ["research"]).is_empty());
    // A manifest for another app, or a scope octos refuses, grants nothing.
    assert!(grant_from_manifest("os.mail", &manifest(&["research"], None)).is_empty());
    let bad = grant_from_manifest(APP, &manifest(&["research"], Some(json!({"languages": ["en"]}))));
    assert!(bad.is_empty() && bad.notes[0].contains("old toolbox shape"), "{:?}", bad.notes);
    let good = grant_from_manifest(APP, &manifest(&["research"], Some(json!({"langs": ["en"], "max_age_days": 2}))));
    assert_eq!(good.grants, BTreeSet::from(["research".to_owned()]));
    assert_eq!(good.scope.langs, ["en"]);
}

// ---- routing --------------------------------------------------------------------

#[test]
fn a_workflow_run_is_routed_to_the_toolbox_and_its_result_lands_in_the_host_folder() {
    let root = temp("run");
    let case = case("news-digest", "city-infrastructure");
    let grant = grant_from_manifest(APP, &manifest(&["research"], None));
    let tools = fixture_tools(news_app(&root, &grant), case.fixture.clone());
    let (_broker, script) = bound(Some(Arc::new(tools)));
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
    let grant = grant_from_manifest(
        APP,
        &manifest(&["research"], Some(json!({"langs": ["en"], "domains_deny": ["example.invalid"]}))),
    );
    let (_broker, script) = bound(Some(Arc::new(fixture_tools(news_app(&root, &grant), case.fixture.clone()))));
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
    let grant = grant_from_manifest(APP, &manifest(&["research"], Some(json!({"max_depth": 2, "max_pages": 5}))));
    let app = news_app(&root, &grant);
    let (_broker, script) = bound(Some(Arc::new(fixture_tools(app.clone(), data.clone()))));
    assert!(!registered(&script).iter().any(|n| n == DEEP_CRAWL));
    // The kernel would not send it; if it did, the broker refuses it.
    let id = call(&script, DEEP_CRAWL, json!({"url": "https://example.invalid/"}));
    let refused = wait(&script, &id);
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["error"]["kind"], "not_registered");
    // And the toolbox refuses it on its own, whoever asks.
    let toolbox = PeerToolbox::from_host(
        Library::builtin().unwrap(),
        fixture::host(&data),
        Arc::new(FixtureBackend::new(data.clone())),
    );
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let refused = runtime
        .block_on(toolbox.call(&app, DEEP_CRAWL, json!({"url": "https://example.invalid/"})))
        .unwrap_err();
    assert_eq!(refused.kind, "not_granted");
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
    let grant = grant_from_manifest(APP, &manifest(&["research"], None));
    let (_broker, script) = bound(Some(Arc::new(fixture_tools(news_app(&root, &grant), data))));
    let id = call(&script, RUN, json!({"id": "news-digest", "params": case.params, "run_id": "cancelled"}));
    std::thread::sleep(Duration::from_millis(200));
    inject(&script, "peer/tool/cancel", json!({"call_id": id, "reason": "cancelled"}));
    std::thread::sleep(Duration::from_millis(800));
    assert!(result_of(&script, &id).is_none());
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
    let grant = grant_from_manifest(APP, &manifest(&["research"], None));
    let app = news_app(&root, &grant);
    let ctx = CallContext {
        app: Arc::new(app.clone()),
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
    let tools = ToolboxTools::new(app, move || {
        let backend = Arc::new(FixtureBackend::new(fixture.clone()));
        let model = Arc::new(ModelHostClient::new(host, &root2));
        Ok(PeerToolbox::new(Library::builtin().map_err(|e| e.to_string())?, backend, model))
    });
    let (_broker, script) = bound(Some(Arc::new(tools)));
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
