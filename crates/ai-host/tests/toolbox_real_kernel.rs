//! The toolbox's tools against a REAL octos kernel with host-registered
//! peer tools (octos#2567, UPCR-2026-035): main's broker registers them
//! after `peer/prepare`, a real turn calls them, and the toolbox's executor
//! runs them (over its fixture backends), with a scripted model
//! (`crates/app-peers/tests/fixtures/mock_agent_llm.py`). The `ToolHost`
//! stands in for the shell's relay (`crates/shell/src/host_tools`): the
//! toolbox's catalog narrowed to the app's grant, nothing before consent.
//! Runs when `OCTOS_APP_PEERS_TEST_KERNEL` names an `octos` binary at the
//! pinned revision; otherwise it says so and passes:
//!
//! ```sh
//! OCTOS_APP_PEERS_TEST_KERNEL=<octos build>/octos \
//!   cargo test -p octosense-ai-host --features toolbox-peers --test toolbox_real_kernel -- --nocapture
//! ```
#![cfg(feature = "toolbox-peers")]

use std::collections::BTreeSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use octosense_ai_host::app_peers::broker::{Broker, BrokerConfig, ToolHostHandle};
use octosense_ai_host::app_peers::connectors::CoreConnector;
use octosense_ai_host::app_peers::host_tools::{HostToolCall, ToolExecutor, ToolHost, ToolOutcome, ToolReply};
use octosense_ai_host::app_peers::{ContextEvent, ContextOp, ContextSpec, Deployment, OctosAppService, OCTOS_SERVICES};
use octosense_ai_host::kernel::{Core, Options};
use octosense_ai_host::toolbox_peers::{catalog, ToolboxExecutor, ToolboxGrant, OWNER};
use octosense_toolbox::fixture::{self, FixtureBackend, FixtureCase};
use octosense_toolbox::peer::PeerToolbox;
use octosense_toolbox::Library;
use serde_json::{json, Value};

struct Model(std::process::Child);
impl Drop for Model {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The shell's relay for the `toolbox` owner, as far as this test needs it.
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
        assert_eq!(call.app, OWNER, "the kernel names the owning app");
        self.ran.lock().unwrap().push(call.name.clone());
        self.executor.execute(call, reply);
    }
    fn tool_cancel(&self, _app_id: &str, call_id: &str, _reason: &str) {
        self.executor.cancel(call_id);
    }
}

fn offered(log: &Path, marker: &str) -> BTreeSet<String> {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let request = text
        .lines()
        .rev()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|r| r["user"].as_str().unwrap_or("").contains(marker))
        .unwrap_or_else(|| panic!("no model request for {marker}"));
    request["tools"].as_array().unwrap().iter().filter_map(Value::as_str).map(str::to_owned).collect()
}

/// The toolbox's tools as the model sees them (`workflow.run` is `workflow_run`).
fn toolbox_names(tools: &BTreeSet<String>) -> BTreeSet<String> {
    let all: BTreeSet<String> = catalog().iter().map(|d| d["name"].as_str().unwrap().replace('.', "_")).collect();
    tools.intersection(&all).cloned().collect()
}

#[test]
fn a_real_peer_is_offered_exactly_its_granted_toolbox_tools_after_consent_and_a_real_turn_runs_them() {
    let Some(program) = std::env::var_os("OCTOS_APP_PEERS_TEST_KERNEL").map(PathBuf::from) else {
        eprintln!("OCTOS_APP_PEERS_TEST_KERNEL is not set: skipping the real-kernel toolbox test");
        return;
    };
    let dir = std::env::temp_dir().join(format!("toolbox-real-kernel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("offered.jsonl");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app-peers/tests/fixtures/mock_agent_llm.py");
    let mut child = std::process::Command::new("python3")
        .arg(script)
        .env("MOCK_LLM_TOOLS_LOG", &log)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("python3");
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let port: u16 = line.trim().parse().unwrap();
    let _model = Model(child);

    let core_dir = dir.join("octos-home/.octos");
    std::fs::create_dir_all(core_dir.join("profiles")).unwrap();
    let profile = json!({
        "id": "_main", "name": "Main", "enabled": true,
        "created_at": "2026-09-28T00:00:00Z", "updated_at": "2026-09-28T00:00:00Z",
        "config": {"llm": {"primary": {"family_id": "local", "model_id": "mock-model",
            "route": {"base_url": format!("http://127.0.0.1:{port}/v1"), "api_type": "openai"}}}}
    });
    std::fs::write(core_dir.join("profiles/_main.json"), profile.to_string()).unwrap();
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program));

    let apps_root = dir.join("apps");
    let case: FixtureCase = serde_json::from_str(
        &std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../toolbox/templates/news-digest/fixtures/city-infrastructure.json")).unwrap(),
    )
    .unwrap();
    let data = case.fixture.clone();
    let executor = ToolboxExecutor::new(
        &apps_root,
        Arc::new(move || {
            let backend = Arc::new(FixtureBackend::new(data.clone()));
            Ok(PeerToolbox::from_host(Library::builtin().map_err(|e| e.to_string())?, fixture::host(&data), backend))
        }),
    );
    // This admitted fixture requests the four research tools explicitly;
    // omitted capability disclosures do not deny those shared tool requests.
    executor.set_grant("os.news", ToolboxGrant::for_manifest("os.news", &json!({
        "id": "os.news", "capabilities": [],
        "agent": {"tools": ["workflow.run", "workflow.fork", "toolbox.search", "toolbox.web_read"]}
    })));
    let host = Arc::new(Relay { executor, consent: AtomicBool::new(false), ran: Mutex::new(Vec::new()) });

    let services: BTreeSet<String> = OCTOS_SERVICES.iter().map(|s| s.to_string()).collect();
    let news = || {
        let mut cfg = BrokerConfig::new(Deployment::Hosted, "_main", "_main:api:octosense#system", "os.news", "News", services.clone());
        cfg.state_dir = Some(dir.join("host-state"));
        cfg.tool_host = Some(ToolHostHandle(host.clone() as Arc<dyn ToolHost>));
        let broker = Broker::new(cfg, Arc::new(CoreConnector::shared(core.clone())));
        broker.set_account(Some("device"));
        broker.bind().expect("the peer binds and its tools are registered");
        broker
    };
    let turn = |broker: &Broker, instance: &str, text: String| -> String {
        let ctx = broker.open_context(ContextSpec { account: "device".into(), instance: instance.into(), services: services.clone() }).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        ctx.call(
            ContextOp::Turn { text },
            Arc::new(move |e| {
                let _ = tx.lock().unwrap().send(e);
            }),
        )
        .unwrap();
        loop {
            match rx.recv_timeout(Duration::from_secs(90)).expect("the turn completes") {
                ContextEvent::Complete(r) => return r.expect("the turn succeeds")["text"].as_str().unwrap_or_default().to_owned(),
                ContextEvent::Data(_) => continue,
            }
        }
    };

    // Before the person allowed News's agent: no toolbox tool is offered,
    // and none runs.
    let before = news();
    let said = turn(&before, "news#0", r#"CALL_TOOL:toolbox_search {"query": "city infrastructure"}"#.into());
    assert!(said.starts_with("NO TOOL toolbox_search"), "{said}");
    assert!(toolbox_names(&offered(&log, "CALL_TOOL:toolbox_search")).is_empty());
    assert!(host.ran.lock().unwrap().is_empty());
    before.release();
    drop(before);

    // Allowed: the next registration offers exactly the granted tools.
    host.consent.store(true, Ordering::SeqCst);
    let news = news();
    let searched = turn(&news, "news#1", r#"CALL_TOOL:toolbox_search {"query": "city infrastructure", "lang": "en", "count": 2}"#.into());
    assert!(searched.starts_with("TOOL SAID"), "{searched}");
    assert!(searched.contains("Harbor City approves electric bus order"), "{searched}");
    let tools = offered(&log, "CALL_TOOL:toolbox_search {\"query\": \"city infrastructure\", \"lang\"");
    let expected: BTreeSet<String> = ["workflow_run", "workflow_fork", "toolbox_search", "toolbox_web_read"].iter().map(|s| s.to_string()).collect();
    assert_eq!(toolbox_names(&tools), expected, "exactly the granted toolbox tools: {tools:?}");
    // Registration is additive: the kernel's own tools stay (generic_tools
    // omitted), and OctoSense holds none back, octos's deep_research
    // included wherever the kernel offers it to a peer.
    eprintln!("offered to the peer: {tools:?}");
    assert!(tools.len() > expected.len(), "the peer keeps its kernel tools: {tools:?}");

    let run = turn(&news, "news#2", format!("CALL_TOOL:workflow_run {}", json!({"id": "news-digest", "params": case.params, "run_id": "real-kernel"})));
    assert!(run.starts_with("TOOL SAID") && run.contains("\"status\":\"ready\""), "{run}");
    assert!(apps_root.join(".host/toolbox/os.news/toolbox/runs/news-digest/real-kernel.json").exists());

    // Not granted crawl: not offered.
    let crawl = turn(&news, "news#3", r#"CALL_TOOL:toolbox_deep_crawl {"url": "https://example.invalid/"}"#.into());
    assert!(crawl.starts_with("NO TOOL toolbox_deep_crawl"), "{crawl}");
    assert_eq!(*host.ran.lock().unwrap(), ["toolbox.search", "workflow.run"]);

    news.release();
    drop(news);
    core.shutdown_within(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&dir);
}
