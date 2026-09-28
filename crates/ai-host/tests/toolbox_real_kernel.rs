//! The toolbox's registration and calls against a REAL octos kernel with
//! host-registered peer tools (octos#2567, UPCR-2026-035), a scripted model
//! (`crates/app-peers/tests/fixtures/mock_agent_llm.py`) and the toolbox's
//! fixture backends. Runs when `OCTOS_APP_PEERS_TEST_KERNEL` names such an
//! `octos` binary; otherwise it says so and passes (CI has none):
//!
//! ```sh
//! OCTOS_APP_PEERS_TEST_KERNEL=<octos#2567 build>/octos \
//!   cargo test -p octosense-ai-host --features toolbox-peers --test toolbox_real_kernel -- --nocapture
//! ```
#![cfg(feature = "toolbox-peers")]

use std::collections::BTreeSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use octosense_ai_host::app_peers::broker::{Broker, BrokerConfig};
use octosense_ai_host::app_peers::connectors::CoreConnector;
use octosense_ai_host::app_peers::{ContextEvent, ContextOp, ContextSpec, Deployment, OctosAppService, OCTOS_SERVICES};
use octosense_ai_host::kernel::{Core, Options};
use octosense_ai_host::toolbox_peers::{app_context, grant_from_manifest, ToolboxTools};
use octosense_toolbox::fixture::{self, FixtureBackend, FixtureCase};
use octosense_toolbox::peer::PeerToolbox;
use octosense_toolbox::Library;
use serde_json::json;

struct Model(std::process::Child);
impl Drop for Model {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn a_real_turn_calls_the_toolbox_and_a_tool_outside_the_grant_is_not_offered() {
    let Some(program) = std::env::var_os("OCTOS_APP_PEERS_TEST_KERNEL").map(PathBuf::from) else {
        eprintln!("OCTOS_APP_PEERS_TEST_KERNEL is not set: skipping the real-kernel toolbox test");
        return;
    };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app-peers/tests/fixtures/mock_agent_llm.py");
    let mut child = std::process::Command::new("python3")
        .arg(script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("python3");
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let port: u16 = line.trim().parse().unwrap();
    let _model = Model(child);

    let dir = std::env::temp_dir().join(format!("toolbox-real-kernel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
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
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../toolbox/templates/news-digest/fixtures/city-infrastructure.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let grant = grant_from_manifest("os.news", &json!({"id": "os.news", "capabilities": ["research"]}));
    let app = app_context("os.news", &grant, &apps_root).unwrap();
    let data = case.fixture.clone();
    let tools = ToolboxTools::new(app, move || {
        let backend = Arc::new(FixtureBackend::new(data.clone()));
        Ok(PeerToolbox::from_host(Library::builtin().map_err(|e| e.to_string())?, fixture::host(&data), backend))
    });

    let services: BTreeSet<String> = OCTOS_SERVICES.iter().map(|s| s.to_string()).collect();
    let mut cfg = BrokerConfig::new(Deployment::Hosted, "_main", "_main:api:octosense#system", "os.news", "News", services.clone());
    cfg.state_dir = Some(dir.join("host-state"));
    cfg.host_tools = Some(Arc::new(tools));
    let news = Broker::new(cfg, Arc::new(CoreConnector::shared(core.clone())));
    news.set_account(Some("device"));
    // The real kernel accepts the toolbox's declarations (names, schemas,
    // risk, flags) or this fails.
    news.bind().expect("the peer binds and its tools are registered");
    let ctx = news
        .open_context(ContextSpec { account: "device".into(), instance: "news#1".into(), services })
        .unwrap();
    let turn = |text: String| -> String {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        ctx.call(ContextOp::Turn { text }, Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }))
        .unwrap();
        loop {
            match rx.recv_timeout(Duration::from_secs(90)).expect("the turn completes") {
                ContextEvent::Complete(r) => return r.expect("the turn succeeds")["text"].as_str().unwrap_or_default().to_owned(),
                ContextEvent::Data(_) => continue,
            }
        }
    };

    let searched = turn(r#"CALL_TOOL:toolbox_search {"query": "city infrastructure", "lang": "en", "count": 2}"#.into());
    assert!(searched.starts_with("TOOL SAID"), "{searched}");
    assert!(searched.contains("Harbor City approves electric bus order"), "{searched}");

    let run = turn(format!(
        "CALL_TOOL:workflow_run {}",
        json!({"id": "news-digest", "params": case.params, "run_id": "real-kernel"})
    ));
    assert!(run.starts_with("TOOL SAID") && run.contains("\"status\":\"ready\""), "{run}");
    assert!(apps_root.join(".host/toolbox/os.news/toolbox/runs/news-digest/real-kernel.json").exists());

    // Not granted crawl: not offered, and the offered set is the toolbox's.
    let crawl = turn(r#"CALL_TOOL:toolbox_deep_crawl {"url": "https://example.invalid/"}"#.into());
    assert_eq!(
        crawl.trim(),
        "NO TOOL toolbox_deep_crawl AMONG toolbox_search,toolbox_web_read,workflow_fork,workflow_run",
        "{crawl}"
    );

    news.release();
    drop(news);
    core.shutdown_within(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&dir);
}
