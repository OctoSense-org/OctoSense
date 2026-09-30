//! G3 end to end against a REAL octos kernel (the pinned rev): the app
//! peers' broker registers News's tools from its admitted bundle, a turn in
//! News's request context calls `news.list`, the kernel sends the call to
//! the broker, the broker to the shell's tool host and relay
//! ([`super::ShellToolHost`], [`super::pump`]), the relay to News's real
//! executor (`script_apps::HostServiceExecutor` → App Hub's host-service
//! dispatch → the News host service), and the result goes back to the
//! model's turn. The model is scripted (`crates/app-peers/tests/fixtures/
//! mock_agent_llm.py`, standard-library Python, no keys).
//!
//! Runs when `OCTOS_SHELL_TEST_KERNEL` (or `OCTOS_APP_PEERS_TEST_KERNEL`)
//! names an `octos` binary built at the pinned revision
//! (`python3 tools/kernel-artifact.py --host`, as CI does); says so and
//! passes without one:
//!
//! ```sh
//! OCTOS_SHELL_TEST_KERNEL=<octos> cargo test --locked --features mobile-apps -p octosense-shell real_kernel -- --nocapture   # from phone/
//! ```

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::ai_host::app_peers::broker::{Broker, BrokerConfig, ToolHostHandle};
use crate::ai_host::app_peers::connectors::CoreConnector;
use crate::ai_host::app_peers::host_tools::ToolHost;
use crate::ai_host::app_peers::{ContextEvent, ContextOp, ContextSpec, Deployment, OctosAppService, OCTOS_SERVICES};
use crate::ai_host::kernel::{Core, Options};

struct Model(std::process::Child, u16);
impl Drop for Model {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn kernel() -> Option<PathBuf> {
    let program = std::env::var_os("OCTOS_SHELL_TEST_KERNEL").or_else(|| std::env::var_os("OCTOS_APP_PEERS_TEST_KERNEL")).map(PathBuf::from);
    if program.is_none() {
        eprintln!("OCTOS_SHELL_TEST_KERNEL is not set: skipping the shell's real-kernel test");
    }
    program
}

fn start_model(log: &Path) -> Model {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app-peers/tests/fixtures/mock_agent_llm.py");
    let mut child = std::process::Command::new("python3")
        .arg(script)
        .env("MOCK_LLM_TOOLS_LOG", log)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("python3 for the scripted model");
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    Model(child, line.trim().parse().expect("model port"))
}

fn write_profile(core_dir: &Path, port: u16) {
    let dir = core_dir.join("profiles");
    std::fs::create_dir_all(&dir).unwrap();
    let profile = json!({
        "id": "_main", "name": "Main", "enabled": true,
        "created_at": "2026-09-27T00:00:00Z", "updated_at": "2026-09-27T00:00:00Z",
        "config": {"llm": {"primary": {"family_id": "local", "model_id": "mock-model",
            "route": {"base_url": format!("http://127.0.0.1:{port}/v1"), "api_type": "openai"}}}}
    });
    std::fs::write(dir.join("_main.json"), serde_json::to_vec_pretty(&profile).unwrap()).unwrap();
}

#[test]
fn real_kernel_an_app_agents_turn_calls_news_list_through_the_shells_relay() {
    let Some(program) = kernel() else { return };
    let dir = std::env::temp_dir().join(format!("octosense-shell-real-kernel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let offered_log = dir.join("offered.jsonl");
    let model = start_model(&offered_log);
    let core_dir = dir.join("octos-home/.octos");
    write_profile(&core_dir, model.1);
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program));

    // The person allowed News's agent (the first-use sheet, ADR 0004 §4).
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    if crate::approvals::with(|_| ()).is_none() {
        crate::approvals::init(&home);
    }
    crate::approvals::with(|a| a.consent.set(&crate::approvals::rules::ApprovalGesture::sheet_tap(), "os.news", true, crate::approvals::now()));

    // News's real host service, on its own host dir (no timer, no fetch).
    let host_dir = dir.join("apps/.host");
    std::fs::create_dir_all(&host_dir).unwrap();
    octosense_news_service::register_with(octosense_news_service::Options::default().host_dir(&host_dir).timer(false));
    // News's tools, from its bundle as App Hub admits it.
    let bundle = super::script_apps::tests::stamped_bundle("news", "real-kernel", |_, _| {});
    let loaded = super::script_apps::from_bundle(&bundle).expect("News's bundle is admitted");
    super::script_apps::install("os.news", loaded, host_dir.clone());

    // News's peer (`card.os.news`), with the shell as its tool host.
    let services = OCTOS_SERVICES.iter().map(|s| s.to_string()).collect();
    let mut cfg = BrokerConfig::new(Deployment::Hosted, "_main", "_main:api:octosense#system", "card.os.news", "News", services);
    cfg.state_dir = Some(dir.join("host-state"));
    cfg.tool_host = Some(ToolHostHandle(Arc::new(super::ShellToolHost) as Arc<dyn ToolHost>));
    let news = Broker::new(cfg, Arc::new(CoreConnector::shared(core.clone())));
    news.set_account(Some(crate::ai_host::contained::ACCOUNT));
    let ctx = news
        .open_context(ContextSpec { account: crate::ai_host::contained::ACCOUNT.into(), instance: "card.os.news-g1".into(), services: OCTOS_SERVICES.iter().map(|s| s.to_string()).collect() })
        .expect("a request context");

    // The person asks News's agent; the scripted model calls `news_list`.
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = Mutex::new(tx);
    ctx.call(ContextOp::Turn { text: r#"CALL_TOOL:news_list:{"limit":5}"#.into() }, Arc::new(move |e| { let _ = tx.lock().unwrap().send(e); })).unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let answer = loop {
        // The shell's UI thread: the relay handles the call, and the host
        // service's answer is delivered.
        super::pump();
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(ContextEvent::Complete(result)) => break result,
            Ok(ContextEvent::Data(_)) | Err(_) => {}
        }
        assert!(Instant::now() < deadline, "the turn did not finish");
    };
    let answer = answer.expect("the turn completed");
    let text = answer["text"].as_str().unwrap_or("").to_string();
    eprintln!("[real-kernel] the turn said: {text}");

    let requests: Vec<Value> = std::fs::read_to_string(&offered_log).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let _ = std::fs::remove_dir_all(&bundle);
    news.release();
    drop(news);
    core.shutdown_within(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&dir);

    assert!(text.starts_with("TOOL SAID"), "the tool's result reached the model's turn: {text}");
    assert!(text.contains("\"total\"") && text.contains("\"items\""), "News's own answer, through the relay: {text}");
    let first = requests.iter().find(|r| r["user"].as_str().unwrap_or("").contains("CALL_TOOL")).expect("the model was asked");
    let tools: Vec<&str> = first["tools"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(tools.contains(&"news_list") && tools.contains(&"news_read"), "News's tools are offered: {tools:?}");
    for shell in ["shell", "bash", "exec_command"] {
        assert!(!tools.contains(&shell), "never octos's shell: {tools:?}");
    }
    assert!(!tools.contains(&"read_file"), "News's manifest grants no kernel tools, so its peer keeps none: {tools:?}");
}

/// Brokers for the contained service's factory, on the test's kernel.
struct TestPeers {
    core: Core,
    state: PathBuf,
    made: Mutex<Vec<Broker>>,
}

impl crate::ai_host::contained::PeerFactory for TestPeers {
    fn launch(&self, peer_id: &str, app_id: &str, services: &std::collections::BTreeSet<String>) -> Option<Arc<dyn OctosAppService>> {
        let mut cfg = BrokerConfig::new(Deployment::Hosted, "_main", "_main:api:octosense#system", peer_id, app_id, services.clone());
        cfg.state_dir = Some(self.state.clone());
        cfg.tool_host = Some(ToolHostHandle(Arc::new(super::ShellToolHost) as Arc<dyn ToolHost>));
        let broker = Broker::new(cfg, Arc::new(CoreConnector::shared(self.core.clone())));
        self.made.lock().unwrap().push(broker.clone());
        Some(Arc::new(broker))
    }
}

/// ADR 0004 §4 on a REAL kernel: News (tools.json, no `octos.*`) has an
/// agent; once the person allowed it the shell prepares its peer with News's
/// tools registered, before any turn; the system agent's `peer_list` shows
/// it; the "Ask News" panel's conversation is a sharing context on that
/// peer whose person turns the system agent's `peer/input` turn then sees
/// (the shared history); turning the agent off releases the peer.
#[test]
fn real_kernel_an_allowed_apps_agent_is_prepared_listed_and_shares_its_conversation() {
    let Some(program) = kernel() else { return };
    let _factory = crate::agents::FACTORY_TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("octosense-shell-agents-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let offered_log = dir.join("offered.jsonl");
    let model = start_model(&offered_log);
    let core_dir = dir.join("octos-home/.octos");
    write_profile(&core_dir, model.1);
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program));
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    if crate::approvals::with(|_| ()).is_none() {
        crate::approvals::init(&home);
    }
    // News's tools, from its bundle as App Hub admits it.
    let host_dir = dir.join("apps/.host");
    std::fs::create_dir_all(&host_dir).unwrap();
    octosense_news_service::register_with(octosense_news_service::Options::default().host_dir(&host_dir).timer(false));
    let bundle = super::script_apps::tests::stamped_bundle("news", "agents", |_, _| {});
    let news = crate::apps::script_agent_app(&bundle.join("manifest.json"), "os.news", "News").expect("News has an agent");
    super::script_apps::install("os.news", super::script_apps::from_bundle(&bundle).unwrap(), host_dir.clone());
    let peers = Arc::new(TestPeers { core: core.clone(), state: dir.join("host-state"), made: Mutex::default() });
    crate::ai_host::contained::set_factory(peers.clone());

    // Not allowed yet: nothing is prepared.
    crate::approvals::with(|a| a.consent.turn_off("os.news", crate::approvals::now()));
    crate::agents::prepare(&news);
    assert!(peers.made.lock().unwrap().is_empty(), "no peer before the person allows it");
    // The person allows it: the shell prepares the peer, tools registered.
    crate::approvals::with(|a| a.consent.set(&crate::approvals::rules::ApprovalGesture::sheet_tap(), "os.news", true, crate::approvals::now()));
    crate::agents::prepare(&news);
    let deadline = Instant::now() + Duration::from_secs(90);
    while crate::agents::prepared("os.news") != Some(crate::agents::Prepared::Ready) {
        super::pump();
        assert!(Instant::now() < deadline, "not prepared: {:?}", crate::agents::prepared("os.news"));
        std::thread::sleep(Duration::from_millis(50));
    }
    let broker = peers.made.lock().unwrap()[0].clone();
    let (slug, _) = broker.peer().expect("bound before any turn");

    // The system agent lists its peers.
    let mut chat = crate::system_chat::session::Driver::new(Box::new(SystemLink(core.clone())));
    chat.command(crate::system_chat::session::Command::Open);
    let turn_text = |chat: &mut crate::system_chat::session::Driver, prompt: &str, want: &str| -> String {
        chat.command(crate::system_chat::session::Command::Send(prompt.into()));
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            chat.step(Duration::from_millis(50));
            super::pump();
            let answers: Vec<String> = chat.model.items.iter().filter_map(|i| match i {
                crate::system_chat::model::Item::Message { role: crate::system_chat::model::Role::Assistant, text, .. } => Some(text.clone()),
                _ => None,
            }).collect();
            if let Some(found) = answers.iter().find(|t| t.contains(want)) {
                if chat.model.phase().running_turn().is_none() {
                    return found.clone();
                }
            }
            assert!(Instant::now() < deadline, "no {want:?} in {:?} ({:?})", chat.model.items, chat.model.phase());
        }
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    while chat.model.phase() != &crate::system_chat::model::Phase::Ready {
        chat.step(Duration::from_millis(50));
        assert!(Instant::now() < deadline, "the system session did not open: {:?}", chat.model.phase());
    }
    let listed = turn_text(&mut chat, "CALL_TOOL:peer_list:{}", "TOOL SAID");
    eprintln!("[real-kernel] peer_list: {listed}");
    assert!(listed.contains(&slug), "the system agent's peer_list shows News's peer {slug}: {listed}");

    // The "Ask News" panel: a sharing context on the same peer; the
    // person's turn runs there, with who spoke.
    let panel = crate::agents::conversation(&news, "shell-ask-real").expect("the person's lane");
    let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
    let s = seen.clone();
    panel.subscribe(Some(Arc::new(move |e| {
        if let ContextEvent::Data(d) = e {
            s.lock().unwrap().push(d);
        }
    })));
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = Mutex::new(tx);
    panel.call(ContextOp::TurnFrom { text: "focus the digest on technology".into(), trigger: crate::ai_host::app_peers::TurnTrigger::Person }, Arc::new(move |e| {
        if let ContextEvent::Complete(r) = e {
            let _ = tx.lock().unwrap().send(r);
        }
    })).unwrap();
    let answer = loop {
        super::pump();
        if let Ok(r) = rx.recv_timeout(Duration::from_millis(50)) {
            break r.expect("the person's turn");
        }
    };
    assert!(answer["text"].as_str().unwrap_or("").contains("focus the digest on technology"), "{answer}");
    assert_eq!(answer["lane"], "person");
    assert!(seen.lock().unwrap().iter().any(|d| d["lane"] == "person" && d["speaker"]["kind"] == "person"), "the follower hears the person's lane with its speaker");
    assert_eq!(peers.made.lock().unwrap().len(), 1, "the panel uses the prepared peer");

    // The system agent's turn on News's agent sees the person's lane.
    let shared = turn_text(&mut chat, &format!("TELL_PEER_SHOW:{slug}"), "message sent");
    eprintln!("[real-kernel] system agent: {shared}");
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        super::pump();
        let sys_lane = seen.lock().unwrap().iter().filter(|d| d["lane"] == "system_agent").map(|d| d.to_string()).collect::<Vec<_>>().join("\n");
        // The model's answer ("SHARED - <rows>" or "SHARED NONE"), not the
        // turn's own request ("SHOW_SHARED"), which arrives first.
        if sys_lane.contains("SHARED -") || sys_lane.contains("SHARED NONE") {
            assert!(sys_lane.contains("focus the digest on technology"), "News's agent, asked by the system agent, sees the person's turn: {sys_lane}");
            break;
        }
        assert!(Instant::now() < deadline, "the panel never heard the system agent's lane: {sys_lane}");
        std::thread::sleep(Duration::from_millis(50));
    }

    // The prepared peer's turns are offered News's own tools.
    let requests: Vec<Value> = std::fs::read_to_string(&offered_log).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let peer_turn = requests.iter().find(|r| r["user"].as_str().unwrap_or("").contains("SHOW_SHARED")).expect("the peer's turn asked the model");
    let tools: Vec<&str> = peer_turn["tools"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(tools.contains(&"news_list") && tools.contains(&"news_read"), "News's tools are registered on its prepared peer: {tools:?}");

    // Turned off: the peer is released and the panel cannot reopen.
    crate::approvals::with(|a| a.consent.turn_off("os.news", crate::approvals::now()));
    assert!(crate::ai_host::contained::revoke("os.news"));
    assert!(!crate::ai_host::contained::is_live("os.news"));
    assert!(!panel.is_open());
    assert!(crate::agents::conversation(&news, "again").is_err());

    drop(chat);
    let _ = std::fs::remove_dir_all(&bundle);
    core.shutdown_within(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&dir);
    drop(model);
}

struct SystemLink(Core);
impl crate::system_chat::session::Connector for SystemLink {
    fn connect(&mut self) -> Result<Box<dyn crate::system_chat::session::Link>, crate::system_chat::session::Unavailable> {
        self.0.connect().map(crate::system_chat::link::link).map_err(|e| crate::system_chat::session::Unavailable::Failed(e.to_string()))
    }
}
