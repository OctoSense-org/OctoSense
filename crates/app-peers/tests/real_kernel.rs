//! The broker against a REAL octos kernel (UPCR-2026-034) started by
//! `octosense-kernel` in a temp core dir, with a scripted local model
//! (`tests/fixtures/mock_llm.py`, standard-library Python, no keys).
//!
//! Runs when `OCTOS_APP_PEERS_TEST_KERNEL` names an `octos` binary with the
//! host-owned app peer contract:
//!
//! ```sh
//! cargo build --release -p octos-cli --bin octos --no-default-features --features api,git,ast
//! OCTOS_APP_PEERS_TEST_KERNEL=<target>/release/octos cargo test --features octos-core --test real_kernel -- --nocapture
//! ```
//!
//! Without it the tests say so and pass (CI has no kernel binary).
#![cfg(feature = "octos-core")]

use std::collections::BTreeSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use octosense_app_peers::broker::{Broker, BrokerConfig};
use octosense_app_peers::connectors::CoreConnector;
use octosense_app_peers::*;
use octosense_kernel::{Core, Options};
use serde_json::{json, Value};

struct Model(std::process::Child, u16);
impl Drop for Model {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_model() -> Model {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_llm.py");
    let mut child = std::process::Command::new("python3")
        .arg(script)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("python3 for the scripted model");
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
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
    std::fs::write(
        dir.join("_main.json"),
        serde_json::to_vec_pretty(&profile).unwrap(),
    )
    .unwrap();
}

fn kernel() -> Option<PathBuf> {
    let program = std::env::var_os("OCTOS_APP_PEERS_TEST_KERNEL").map(PathBuf::from);
    if program.is_none() {
        eprintln!("OCTOS_APP_PEERS_TEST_KERNEL is not set: skipping the real-kernel test");
    }
    program
}

fn broker(core: &Core, app: &str, label: &str) -> Broker {
    let services: BTreeSet<String> = OCTOS_SERVICES.iter().map(|s| s.to_string()).collect();
    let mut cfg = BrokerConfig::new(
        Deployment::Hosted,
        "_main",
        "_main:api:octosense#system",
        app,
        label,
        services,
    );
    // The host keeps each peer's host token beside the kernel (shell state).
    cfg.state_dir = core
        .core_dir()
        .map(|d| d.parent().unwrap().join("host-state"));
    Broker::new(cfg, Arc::new(CoreConnector::shared(core.clone())))
}

fn spec(account: &str, instance: &str) -> ContextSpec {
    ContextSpec {
        account: account.into(),
        instance: instance.into(),
        services: OCTOS_SERVICES.iter().map(|s| s.to_string()).collect(),
    }
}

fn run(
    ctx: &Arc<dyn OctosContext>,
    op: ContextOp,
    wait: Duration,
) -> Option<Result<Value, String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = Mutex::new(tx);
    ctx.call(
        op,
        Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }),
    )
    .unwrap();
    let deadline = std::time::Instant::now() + wait;
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok(ContextEvent::Complete(r)) => return Some(r),
            Ok(ContextEvent::Data(_)) => continue,
            Err(_) => return None,
        }
    }
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("app-peers-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn two_apps_share_one_kernel_and_closing_one_leaves_the_other_usable() {
    let Some(program) = kernel() else { return };
    let model = start_model();
    let dir = temp("share");
    let core_dir = dir.join("octos-home/.octos");
    write_profile(&core_dir, model.1);
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program));

    let rinx = broker(&core, "rinx", "Rinx");
    let notes = broker(&core, "notes", "Notes");
    rinx.set_account(Some("@alice:example.org"));
    notes.set_account(Some("@alice:example.org"));
    let rinx_ctx = rinx
        .open_context(spec("@alice:example.org", "mini-a#1"))
        .unwrap();
    let notes_ctx = notes
        .open_context(spec("@alice:example.org", "notes#1"))
        .unwrap();

    let a = run(
        &rinx_ctx,
        ContextOp::Turn {
            text: "hello from rinx".into(),
        },
        Duration::from_secs(90),
    )
    .expect("rinx turn finished")
    .expect("rinx turn ok");
    assert_eq!(a["text"], "ECHO: hello from rinx");
    let b = run(
        &notes_ctx,
        ContextOp::Turn {
            text: "hello from notes".into(),
        },
        Duration::from_secs(90),
    )
    .expect("notes turn finished")
    .expect("notes turn ok");
    assert_eq!(b["text"], "ECHO: hello from notes");

    let status = core.status();
    assert!(status.running);
    assert_eq!(status.generation, 1, "one kernel for both apps");
    assert_eq!(status.connections, 2);
    let (rinx_slug, _) = rinx.peer().expect("rinx peer");
    let (notes_slug, _) = notes.peer().expect("notes peer");
    assert_ne!(
        rinx_slug, notes_slug,
        "each app is its own addressable peer"
    );
    for slug in [&rinx_slug, &notes_slug] {
        let originator = std::fs::read_to_string(
            core_dir
                .join("profiles/_main/data/peers")
                .join(slug)
                .join("originator"),
        )
        .unwrap();
        assert_eq!(originator, "_main:api:octosense#system");
    }

    // Close Rinx: its contexts close on the kernel; the kernel and Notes stay.
    rinx.release();
    std::thread::sleep(Duration::from_secs(2));
    assert!(!rinx_ctx.is_open());
    let closed = std::fs::read_dir(core_dir.join("profiles/_main/data/peers").join(&rinx_slug))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("context-"))
        .all(|e| {
            std::fs::read_to_string(e.path())
                .unwrap()
                .contains("\"closed\":true")
        });
    assert!(closed, "rinx's contexts are closed on the kernel");
    let again = run(
        &notes_ctx,
        ContextOp::Turn {
            text: "still here".into(),
        },
        Duration::from_secs(90),
    )
    .expect("notes turn finished")
    .expect("notes still usable");
    assert_eq!(again["text"], "ECHO: still here");
    assert_eq!(core.status().generation, 1, "no second kernel");
    drop(notes);
    core.shutdown_within(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_account_change_drops_a_late_reply_and_resume_keeps_the_peer_across_restarts() {
    let Some(program) = kernel() else { return };
    let model = start_model();
    let dir = temp("account");
    let core_dir = dir.join("octos-home/.octos");
    write_profile(&core_dir, model.1);
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program));

    let rinx = broker(&core, "rinx", "Rinx");
    rinx.set_account(Some("@alice:example.org"));
    let ctx = rinx
        .open_context(spec("@alice:example.org", "mini#1"))
        .unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = Mutex::new(tx);
    ctx.call(
        ContextOp::Turn {
            text: "SLOW private question".into(),
        },
        Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }),
    )
    .unwrap();
    std::thread::sleep(Duration::from_secs(3));
    rinx.set_account(Some("@bob:example.org"));
    // The slow answer arrives after the switch: nothing reaches the old
    // instance, and the kernel refused the context from then on.
    let mut late = Vec::new();
    while let Ok(event) = rx.recv_timeout(Duration::from_secs(25)) {
        late.push(format!("{event:?}"));
    }
    assert!(
        late.iter().all(|e| !e.starts_with("Complete")),
        "stale reply delivered: {late:?}"
    );

    // Bob gets a different peer (namespace), Alice's resumes after restart.
    let bob = rinx
        .open_context(spec("@bob:example.org", "mini#2"))
        .unwrap();
    run(&bob, ContextOp::Open, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    let (bob_slug, _) = rinx.peer().unwrap();
    rinx.set_account(Some("@alice:example.org"));
    let alice = rinx
        .open_context(spec("@alice:example.org", "mini#3"))
        .unwrap();
    run(&alice, ContextOp::Open, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    let (alice_slug, _) = rinx.peer().unwrap();
    assert_ne!(alice_slug, bob_slug);
    rinx.release();
    drop(rinx);
    core.shutdown_within(Duration::from_secs(5));

    // Process restart: a fresh broker and kernel resume Alice's SAME peer.
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program));
    let rinx = broker(&core, "rinx", "Rinx");
    rinx.set_account(Some("@alice:example.org"));
    let ctx = rinx
        .open_context(spec("@alice:example.org", "mini#1"))
        .unwrap();
    let answer = run(
        &ctx,
        ContextOp::Turn {
            text: "after restart".into(),
        },
        Duration::from_secs(90),
    )
    .unwrap()
    .unwrap();
    assert_eq!(answer["text"], "ECHO: after restart");
    assert_eq!(rinx.peer().unwrap().0, alice_slug, "the same peer resumed");
    rinx.release();
    drop(rinx);
    core.shutdown_within(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Criterion 3 of ADR 0007 with a scripted model: the system agent sends the
/// app peer input, the peer asks a question, the kernel wakes the system
/// agent, which answers, and the peer continues with the answer. Every model
/// request on the way (the host's turn, the kernel's wake continuation, the
/// peer's turns) is offered only tools inside the kernel profile's ceiling
/// (ADR 0004 §12), never octos's `shell`.
#[test]
fn the_system_agent_and_the_app_peer_exchange_a_question_and_answer() {
    let Some(program) = kernel() else { return };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_agent_llm.py");
    let dir = temp("qa");
    std::fs::create_dir_all(&dir).unwrap();
    let offered_log = dir.join("offered.jsonl");
    let mut child = std::process::Command::new("python3")
        .arg(script)
        .env("MOCK_LLM_TOOLS_LOG", &offered_log)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .unwrap();
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let model = Model(child, line.trim().parse().unwrap());
    let core_dir = dir.join("octos-home/.octos");
    write_profile(&core_dir, model.1);
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program));
    let rinx = broker(&core, "rinx", "Rinx");
    rinx.set_account(Some("@alice:example.org"));
    rinx.bind().expect("peer bound without inference");
    let (slug, peer_session) = rinx.peer().unwrap();

    let turn = uuid_like();
    rinx.host_request(
        "turn/start",
        json!({"session_id": "_main:api:octosense#system", "turn_id": turn,
               "input": [{"kind": "text", "text": format!("TELL_PEER:{slug}")}]}),
    )
    .expect("system turn");
    let mut transcript = Value::Null;
    for _ in 0..120 {
        transcript = rinx
            .host_request(
                "session/hydrate",
                json!({"session_id": peer_session, "include": ["messages"]}),
            )
            .unwrap_or(Value::Null);
        if transcript.to_string().contains("PEER GOT 42") {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let text = transcript.to_string();
    assert!(
        text.contains("QUESTION_ME"),
        "the system agent's input reached the peer: {text}"
    );
    assert!(
        text.contains("PEER GOT 42"),
        "the peer continued with the system agent's answer: {text}"
    );
    let allowed = octosense_kernel::system_tools::profile_ceiling();
    let requests: Vec<Value> = std::fs::read_to_string(&offered_log)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    for request in &requests {
        for tool in request["tools"].as_array().unwrap() {
            let tool = tool.as_str().unwrap();
            assert!(allowed.contains(tool), "{tool} offered: {request}");
        }
    }
    assert!(
        requests.iter().any(|r| {
            !r["user"].as_str().unwrap().contains("TELL_PEER")
                && r["tools"].as_array().unwrap().iter().any(|t| t == "peer_respond")
        }),
        "the system agent's wake continuation was seen and checked"
    );

    rinx.release();
    drop(rinx);
    core.shutdown_within(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&dir);
}

fn uuid_like() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let hex = format!("{nanos:032x}");
    format!(
        "{}-{}-4{}-8{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        &hex[17..20],
        &hex[20..32]
    )
}

/// Criterion 3, interrupt: the system agent hands the peer work that parks
/// on a question nobody answers; closing the app stops the peer's turn (the
/// conservative background policy) without stopping the kernel.
#[test]
fn closing_the_app_interrupts_its_peers_running_work() {
    let Some(program) = kernel() else { return };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_agent_llm.py");
    let mut child = std::process::Command::new("python3")
        .arg(script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .unwrap();
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let model = Model(child, line.trim().parse().unwrap());
    let dir = temp("interrupt");
    let core_dir = dir.join("octos-home/.octos");
    write_profile(&core_dir, model.1);
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program));
    let rinx = broker(&core, "rinx", "Rinx");
    rinx.set_account(Some("@alice:example.org"));
    rinx.bind().expect("peer bound");
    let (slug, peer_session) = rinx.peer().unwrap();
    rinx.host_request(
        "turn/start",
        json!({"session_id": "_main:api:octosense#system", "turn_id": uuid_like(),
               "input": [{"kind": "text", "text": format!("TELL_PEER_HOLD:{slug}")}]}),
    )
    .expect("system turn");
    // The peer runs the system agent's input and parks on its question.
    let mut turn = None;
    for _ in 0..120 {
        turn = rinx.peer_active_turn();
        if turn.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let turn = turn.expect("the peer is working on the system agent's input");
    std::thread::sleep(Duration::from_secs(2));
    let observer = broker(&core, "observer", "Observer");
    let state = |observer: &Broker| {
        observer
            .host_request(
                "turn/state/get",
                json!({"session_id": peer_session, "turn_id": turn}),
            )
            .map(|r| r["state"].clone())
    };
    let before = state(&observer).expect("turn state");
    eprintln!("peer turn before release: {before}");
    assert!(
        before == "running" || before == "awaiting_input" || before == "active",
        "{before}"
    );
    rinx.release();
    let mut after = Value::Null;
    for _ in 0..40 {
        after = state(&observer).unwrap_or(Value::Null);
        if after != before {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    eprintln!("peer turn after release: {after}");
    assert_ne!(after, before, "closing the app stopped the peer's turn");
    assert!(
        core.status().running,
        "the kernel keeps running for other apps"
    );
    drop(observer);
    drop(rinx);
    core.shutdown_within(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&dir);
}

/// After a full question/answer exchange the system agent sends the SAME peer
/// a second input, which must run as the peer's next turn. The scripted model
/// reuses the tool-call id `call_1` by default (MOCK_CALL_IDS=unique gives
/// distinct ids), as some providers do: the kernel once deduped the second
/// `peer_send_input` against the first and dropped it while reporting
/// success. SECOND_DELAY_SECS waits before the second send.
#[test]
fn a_second_input_to_an_answered_peer_runs() {
    let Some(program) = kernel() else { return };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_agent_llm.py");
    let mut child = std::process::Command::new("python3")
        .arg(script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .unwrap();
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let model = Model(child, line.trim().parse().unwrap());
    let dir = temp("second");
    let core_dir = dir.join("octos-home/.octos");
    write_profile(&core_dir, model.1);
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program));
    let rinx = broker(&core, "rinx", "Rinx");
    rinx.set_account(Some("@alice:example.org"));
    rinx.bind().expect("peer bound without inference");
    let (slug, peer_session) = rinx.peer().unwrap();
    let hydrate = |rinx: &Broker| {
        rinx.host_request(
            "session/hydrate",
            json!({"session_id": peer_session, "include": ["messages", "turns"]}),
        )
        .unwrap_or(Value::Null)
        .to_string()
    };
    let t0 = std::time::Instant::now();
    rinx.host_request(
        "turn/start",
        json!({"session_id": "_main:api:octosense#system", "turn_id": uuid_like(),
               "input": [{"kind": "text", "text": format!("TELL_PEER:{slug}")}]}),
    )
    .expect("system turn");
    let mut first = false;
    for _ in 0..120 {
        if hydrate(&rinx).contains("PEER GOT 42") {
            first = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    assert!(first, "first exchange completed");
    eprintln!("[second] first exchange done after {:?}", t0.elapsed());
    let delay: u64 = std::env::var("SECOND_DELAY_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    std::thread::sleep(Duration::from_secs(delay));
    // The system agent's wake continuation may still be finishing.
    let mut started = Err(String::new());
    for _ in 0..80 {
        started = rinx.host_request(
            "turn/start",
            json!({"session_id": "_main:api:octosense#system", "turn_id": uuid_like(),
                   "input": [{"kind": "text", "text": format!("TELL_PEER_AGAIN:{slug}")}]}),
        );
        if started.is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    started.expect("second system turn");
    eprintln!(
        "[second] second system turn started after {:?}",
        t0.elapsed()
    );
    let mut second = false;
    for _ in 0..180 {
        if hydrate(&rinx).contains("ECHO: SECOND_INPUT") {
            second = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    eprintln!("[second] input ran: {second} after {:?}", t0.elapsed());
    rinx.release();
    drop(rinx);
    core.shutdown_within(Duration::from_secs(5));
    assert!(second, "the peer ran the second input as its next turn");
    let _ = std::fs::remove_dir_all(&dir);
}

/// ADR 0004 §12 (was ADR 0007's approval test): octos's own `shell` is
/// offered to no `_main` session (the kernel profile's ceiling, written by
/// `octosense-kernel` at every start, denies it), so the system agent cannot
/// get a command run through an app peer by asking or by "approving". The
/// system agent handing the peer a command gets no tool approval parked and
/// nothing run; its `peer_respond` "approval" finds nothing to approve.
/// Command execution an app is granted arrives as a host tool with a live
/// approval (for example `terminal.run`), registered by the shell (plan
/// steps 6 and 7); approvals of app tools return here then. That octos
/// refuses the system agent an app peer's approval is octos's own test
/// (`ui_protocol_tests.rs`, ADR 0007).
#[test]
fn the_system_agent_cannot_get_a_command_run_through_an_app_peer() {
    let Some(program) = kernel() else { return };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_agent_llm.py");
    let mut child = std::process::Command::new("python3")
        .arg(script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .unwrap();
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let model = Model(child, line.trim().parse().unwrap());
    let dir = temp("approval");
    let core_dir = dir.join("octos-home/.octos");
    write_profile(&core_dir, model.1);
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program));
    let rinx = broker(&core, "rinx", "Rinx");
    rinx.set_account(Some("@alice:example.org"));
    rinx.bind().expect("peer bound without inference");
    let (slug, peer_session) = rinx.peer().unwrap();
    let system = "_main:api:octosense#system";
    let hydrate = |session: &str, include: &[&str]| {
        rinx.host_request(
            "session/hydrate",
            json!({"session_id": session, "include": include}),
        )
        .unwrap_or(Value::Null)
    };
    let pending_approvals = || {
        hydrate(&peer_session, &["pending_approvals"])["pending_approvals"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    let start_system_turn = |text: String| {
        let mut started = Err(String::new());
        for _ in 0..80 {
            started = rinx.host_request(
                "turn/start",
                json!({"session_id": system, "turn_id": uuid_like(),
                       "input": [{"kind": "text", "text": text}]}),
            );
            if started.is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        started.expect("system turn");
    };

    start_system_turn(format!("TELL_PEER_SUDO:{slug}"));
    let mut peer_text = String::new();
    for _ in 0..120 {
        peer_text = hydrate(&peer_session, &["messages"]).to_string();
        if peer_text.contains("NO SHELL OFFERED") {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    assert!(
        peer_text.contains("NO SHELL OFFERED"),
        "the peer ran the system agent's input without a shell tool: {peer_text}"
    );
    assert!(pending_approvals().is_empty(), "no tool approval parked");

    start_system_turn(format!("APPROVE_PEER:{slug}"));
    let mut system_text = String::new();
    for _ in 0..60 {
        system_text = hydrate(system, &["messages"]).to_string();
        if system_text.contains("not awaiting input") {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    assert!(
        system_text.contains("not awaiting input"),
        "peer_respond had nothing to approve: {system_text}"
    );
    std::thread::sleep(Duration::from_secs(2));
    assert!(pending_approvals().is_empty());
    assert!(
        !hydrate(&peer_session, &["messages"])
            .to_string()
            .contains("APPROVED_RAN"),
        "no command ran"
    );

    rinx.release();
    drop(rinx);
    core.shutdown_within(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&dir);
}
