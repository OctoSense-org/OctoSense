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
