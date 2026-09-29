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

use octosense_app_peers::broker::{Broker, BrokerConfig, ToolHostHandle};
use octosense_app_peers::host_tools::{CallOrigin, HostToolCall, ToolHost, ToolOutcome, ToolReply};
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
/// peer's turns) is offered none of octos's shell (ADR 0004 §12: command
/// execution is a granted host tool, never octos's `shell`).
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
    let requests: Vec<Value> = std::fs::read_to_string(&offered_log)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    for request in &requests {
        for tool in request["tools"].as_array().unwrap() {
            let tool = tool.as_str().unwrap();
            assert!(
                !["shell", "bash", "exec_command", "write_stdin"].contains(&tool),
                "{tool} offered: {request}"
            );
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
        // The kernel marks the system agent's input (octos#2626).
        let transcript = hydrate(&rinx);
        if transcript.contains("ECHO: [from the system agent] SECOND_INPUT") || transcript.contains("ECHO: SECOND_INPUT") {
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
/// offered to no `_main` session (the kernel profile's policy, written by
/// `octosense-kernel` at every start, denies it and nothing else), so the system agent cannot
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


/// UPCR-2026-035 end to end: the broker registers Rinx's tools after
/// `peer/prepare`; the system agent's `peer_send_input` reaches the shell as
/// `peer/input`, the broker starts that turn on its own connection, and the
/// turn (host-driven: app tools, attended) calls the registered app tool,
/// which the host answers once. A request context's turn gets the tool too,
/// stamped with its client. The peer is never offered octos's shell.
#[test]
fn the_system_agents_input_runs_as_a_host_driven_turn_with_the_apps_tools() {
    let Some(program) = kernel() else { return };
    struct EchoHost(Mutex<Vec<HostToolCall>>);
    impl ToolHost for EchoHost {
        fn declarations(&self, _app: &str, _account: &str) -> Result<Vec<Value>, String> {
            Ok(vec![json!({"name": "rinx.echo", "description": "Echo a text back.", "risk": "read",
                "input_schema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}})])
        }
        fn tool_call(&self, call: HostToolCall, reply: ToolReply) {
            let text = call.args["text"].as_str().unwrap_or("").to_owned();
            self.0.lock().unwrap().push(call);
            reply.finish(ToolOutcome::Ok(json!({"echo": text})));
        }
    }
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_agent_llm.py");
    let dir = temp("host-tools");
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
    std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let model = Model(child, line.trim().parse().unwrap());
    let core_dir = dir.join("octos-home/.octos");
    write_profile(&core_dir, model.1);
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program));
    let host = Arc::new(EchoHost(Mutex::new(Vec::new())));
    let services: BTreeSet<String> = OCTOS_SERVICES.iter().map(|s| s.to_string()).collect();
    let mut cfg = BrokerConfig::new(Deployment::Hosted, "_main", "_main:api:octosense#system", "rinx", "Rinx", services);
    cfg.state_dir = core.core_dir().map(|d| d.parent().unwrap().join("host-state"));
    cfg.tool_host = Some(ToolHostHandle(host.clone() as Arc<dyn ToolHost>));
    let rinx = Broker::new(cfg, Arc::new(CoreConnector::shared(core.clone())));
    rinx.set_account(Some("@alice:example.org"));
    rinx.bind().expect("peer bound and its tools registered");
    let (slug, peer_session) = rinx.peer().unwrap();

    rinx.host_request(
        "turn/start",
        json!({"session_id": "_main:api:octosense#system", "turn_id": uuid_like(),
               "input": [{"kind": "text", "text": format!("TELL_PEER_TOOL:{slug}")}]}),
    )
    .expect("system turn");
    for _ in 0..120 {
        if !host.0.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let calls = host.0.lock().unwrap().clone();
    assert_eq!(calls.len(), 1, "the app tool ran once: {calls:?}");
    let call = &calls[0];
    assert_eq!(call.name, "rinx.echo");
    assert_eq!(call.origin, CallOrigin::PeerInput, "the turn the broker started for peer/input");
    assert_eq!(call.account.as_deref(), Some("@alice:example.org"));
    assert_eq!(call.session_id, peer_session);
    let mut transcript = String::new();
    for _ in 0..60 {
        transcript = rinx
            .host_request("session/hydrate", json!({"session_id": peer_session, "include": ["messages"]}))
            .unwrap_or(Value::Null)
            .to_string();
        if transcript.contains("CALL_APP_TOOL") && transcript.contains("OK") {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    assert!(transcript.contains("CALL_APP_TOOL"), "the system agent's input ran on the peer: {transcript}");

    // A request context of the app (a mini app) gets the tool too.
    let ctx = rinx.open_context(spec("@alice:example.org", "mini.echo#1")).unwrap();
    let answer = run(&ctx, ContextOp::Turn { text: "CALL_APP_TOOL".into() }, Duration::from_secs(60)).expect("a completion");
    answer.expect("the context turn completed");
    let calls = host.0.lock().unwrap().clone();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert_eq!(calls[1].origin, CallOrigin::Context);
    assert_eq!(calls[1].client.as_deref(), Some("mini.echo#1"), "stamped from the host's context table");

    let requests: Vec<Value> = std::fs::read_to_string(&offered_log)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let peer_requests: Vec<&Value> = requests.iter().filter(|r| r["user"].as_str().unwrap_or("").contains("CALL_APP_TOOL")).collect();
    assert!(!peer_requests.is_empty());
    for request in &peer_requests {
        let tools: Vec<&str> = request["tools"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert!(tools.contains(&"rinx_echo"), "the registered app tool is offered: {tools:?}");
        assert!(!tools.iter().any(|t| ["shell", "bash", "exec_command"].contains(t)), "{tools:?}");
    }
    rinx.release();
    drop(rinx);
    core.shutdown_within(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&dir);
}


/// The system session as a tool host (UPCR-2026-035, host session target),
/// the way the shell's system chat uses it: with an app peer's host token
/// it registers `terminal.run` on the system session over its own
/// connection (no `peer`, `generic_tools` omitted); the system agent's turn
/// is offered it, the kernel asks that connection for a `host_tool`
/// approval naming the owning app, then sends the call with `caller.kind:
/// "system"`, answered without `peer`. An empty set withdraws it. The
/// shell's consumers share ONE kernel connection, so the call also reaches
/// the Rinx broker (it named the system session): the broker leaves it to
/// the system session's host and answers nothing.
#[test]
fn the_system_session_hosts_terminal_run_while_it_is_registered() {
    let Some(program) = kernel() else { return };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_agent_llm.py");
    let dir = temp("system-host");
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
    std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let model = Model(child, line.trim().parse().unwrap());
    let core_dir = dir.join("octos-home/.octos");
    write_profile(&core_dir, model.1);
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program));
    // An app peer the system session prepared: its host token is the credential.
    let rinx = broker(&core, "rinx", "Rinx");
    rinx.set_account(Some("@alice:example.org"));
    rinx.bind().expect("peer bound");
    let state = core.core_dir().unwrap().parent().unwrap().join("host-state");
    let token = octosense_app_peers::hosted::newest_token(&state).expect("a kept host token");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        const SYSTEM: &str = "_main:api:octosense#system";
        let mut conn = core.connect().unwrap();
        let mut backlog: Vec<Value> = Vec::new();
        let mut next = 0u64;
        // Send a request; keep notifications that arrive before its reply.
        let mut call = |conn: &mut octosense_kernel::Connection, method: &str, params: Value| {
            next += 1;
            let id = format!("t-{next}");
            conn.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()).unwrap();
            id
        };
        async fn until(conn: &mut octosense_kernel::Connection, backlog: &mut Vec<Value>, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
            if let Some(i) = backlog.iter().position(&pred) {
                return backlog.remove(i);
            }
            loop {
                let text = tokio::time::timeout(Duration::from_secs(60), conn.recv()).await.unwrap_or_else(|_| panic!("waiting for {what}")).unwrap();
                let frame: Value = serde_json::from_str(&text).unwrap();
                if pred(&frame) {
                    return frame;
                }
                backlog.push(frame);
            }
        }
        let reply = |id: String| move |f: &Value| f["id"] == id.as_str();
        let id = call(&mut conn, "session/open", json!({"session_id": SYSTEM, "profile_id": "_main"}));
        until(&mut conn, &mut backlog, "session/open", reply(id)).await;
        let run = json!({"name": "terminal.run", "app": "terminal", "description": "Type a command into the live Terminal.", "risk": "destructive", "confirm": "host", "shareable": true,
            "input_schema": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"], "additionalProperties": false}});
        let id = call(&mut conn, "peer/tools/register", json!({"session_id": SYSTEM, "profile_id": "_main", "host_token": token, "tools": [run]}));
        let registered = until(&mut conn, &mut backlog, "register", reply(id)).await;
        assert!(registered.get("error").is_none(), "{registered}");
        assert_eq!(registered["result"]["session_id"], SYSTEM, "a host session set: {registered}");
        assert!(registered["result"]["generic_tools"].is_null(), "the system agent keeps its kernel tools");

        let turn = uuid_like();
        let id = call(&mut conn, "turn/start", json!({"session_id": SYSTEM, "turn_id": turn, "input": [{"kind": "text", "text": "RUN_TERMINAL please"}]}));
        until(&mut conn, &mut backlog, "turn/start", reply(id)).await;
        let approval = until(&mut conn, &mut backlog, "the host_tool approval", |f| f["method"] == "approval/requested").await;
        let host = &approval["params"]["typed_details"]["host_tool"];
        assert_eq!(approval["params"]["approval_kind"], "host_tool", "{approval}");
        assert_eq!((host["app"].as_str(), host["tool"].as_str()), (Some("terminal"), Some("terminal.run")));
        assert_eq!(host["args"]["command"], "ls", "the exact arguments");
        assert_eq!(host["calling_kind"], "system");
        let id = call(&mut conn, "approval/respond", json!({"session_id": SYSTEM, "approval_id": approval["params"]["approval_id"], "decision": "approve"}));
        until(&mut conn, &mut backlog, "approval/respond", reply(id)).await;
        let tool_call = until(&mut conn, &mut backlog, "the tool call", |f| f["method"] == "peer/tool/call").await;
        let p = &tool_call["params"];
        assert_eq!(p["caller"]["kind"], "system", "{tool_call}");
        assert!(p["peer"].is_null());
        assert_eq!((p["app"].as_str(), p["name"].as_str()), (Some("terminal"), Some("terminal.run")));
        assert_eq!(p["confirm_required"], false, "the kernel already holds the person's approval");
        let id = call(&mut conn, "peer/tool/result", json!({"session_id": SYSTEM, "profile_id": "_main", "host_token": token, "call_id": p["call_id"], "ok": true, "data": {"text": "typed"}}));
        let accepted = until(&mut conn, &mut backlog, "the result", reply(id)).await;
        assert_eq!(accepted["result"]["accepted"], true, "{accepted}");

        // Withdrawn: the next turn is not offered it.
        let id = call(&mut conn, "peer/tools/register", json!({"session_id": SYSTEM, "profile_id": "_main", "host_token": token, "tools": []}));
        until(&mut conn, &mut backlog, "withdraw", reply(id)).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
        let second = uuid_like();
        let id = call(&mut conn, "turn/start", json!({"session_id": SYSTEM, "turn_id": second, "input": [{"kind": "text", "text": "RUN_TERMINAL again"}]}));
        until(&mut conn, &mut backlog, "turn/start 2", reply(id)).await;
        for _ in 0..120 {
            let text = std::fs::read_to_string(&offered_log).unwrap_or_default();
            if text.contains("RUN_TERMINAL again") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });
    let requests: Vec<Value> = std::fs::read_to_string(&offered_log).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let tools_of = |probe: &str| -> Vec<String> {
        requests.iter().find(|r| r["user"].as_str().unwrap_or("").contains(probe)).map(|r| r["tools"].as_array().unwrap().iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default()
    };
    assert!(tools_of("RUN_TERMINAL please").contains(&"terminal_run".to_string()));
    let after = tools_of("RUN_TERMINAL again");
    assert!(!after.is_empty() && !after.contains(&"terminal_run".to_string()), "withdrawn: {after:?}");
    rinx.release();
    drop(rinx);
    core.shutdown_within(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&dir);
}

/// ADR 0004 §6 against the real kernel (octos UPCR-2026-034, "Parallel
/// person context with shared history"): the person, through the app's
/// conversation, talks in their own lane (a sharing request context), and
/// the system agent (`peer_send_input` → `peer/input`) in the peer's
/// session; each lane's model sees the other's recent turns read-only. The
/// person's turn carries `origin: person` (labelled with the app), the
/// input's none (the kernel labels it); the app's history merges both
/// transcripts with each row's lane and speaker; the app's follower sees
/// the system agent's turn stream.
#[test]
fn the_person_and_the_system_agent_talk_in_parallel_lanes_that_share_history() {
    let Some(program) = kernel() else { return };
    struct EchoHost;
    impl ToolHost for EchoHost {
        fn declarations(&self, _app: &str, _account: &str) -> Result<Vec<Value>, String> {
            Ok(vec![json!({"name": "rinx.echo", "description": "Echo a text back.", "risk": "read",
                "input_schema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}})])
        }
        fn tool_call(&self, call: HostToolCall, reply: ToolReply) {
            reply.finish(ToolOutcome::Ok(json!({"echo": call.args["text"].clone()})));
        }
    }
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_agent_llm.py");
    let dir = temp("parallel-lanes");
    std::fs::create_dir_all(&dir).unwrap();
    let mut child = std::process::Command::new("python3")
        .arg(script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let model = Model(child, line.trim().parse().unwrap());
    let core_dir = dir.join("octos-home/.octos");
    write_profile(&core_dir, model.1);
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program));
    let services: BTreeSet<String> = OCTOS_SERVICES.iter().map(|s| s.to_string()).collect();
    let mut cfg = BrokerConfig::new(Deployment::Hosted, "_main", "_main:api:octosense#system", "rinx", "Rinx", services);
    cfg.state_dir = core.core_dir().map(|d| d.parent().unwrap().join("host-state"));
    cfg.tool_host = Some(ToolHostHandle(Arc::new(EchoHost) as Arc<dyn ToolHost>));
    let rinx = Broker::new(cfg, Arc::new(CoreConnector::shared(core.clone())));
    rinx.set_account(Some("@alice:example.org"));
    rinx.bind().expect("peer bound and its tools registered");
    let (slug, peer_session) = rinx.peer().unwrap();

    let chat = rinx.open_conversation(spec("@alice:example.org", "rinx-chat")).unwrap();
    let followed: Arc<Mutex<Vec<Value>>> = Arc::default();
    let log = followed.clone();
    chat.subscribe(Some(Arc::new(move |e| {
        if let ContextEvent::Data(d) = e {
            log.lock().unwrap().push(d);
        }
    })));
    // The person's message: in the person's lane, labelled.
    let answer = run(&chat, ContextOp::TurnFrom { text: "hello from the app".into(), trigger: TurnTrigger::Person }, Duration::from_secs(60))
        .expect("a completion")
        .expect("the person's turn completed");
    assert_eq!(answer["speaker"], json!({"kind": "person", "label": "Rinx"}));
    assert_eq!(answer["lane"], "person");
    assert!(answer["text"].as_str().unwrap().contains("hello from the app"), "{answer}");

    // The system agent's input to the same peer, in its own lane.
    let system_turn = |text: String| {
        rinx.host_request(
            "turn/start",
            json!({"session_id": "_main:api:octosense#system", "turn_id": uuid_like(),
                   "input": [{"kind": "text", "text": text}]}),
        )
        .expect("system turn");
    };
    let history_until = |done: &dyn Fn(&[Value]) -> bool| -> Vec<Value> {
        let mut rows = Vec::new();
        for _ in 0..120 {
            rows = run(&chat, ContextOp::History, Duration::from_secs(30)).expect("history").expect("history ok")["messages"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if done(&rows) {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        rows
    };
    system_turn(format!("TELL_PEER_TOOL:{slug}"));
    let rows = history_until(&|rows| rows.iter().any(|m| m["lane"] == "system_agent" && m["role"] == "assistant" && m["content"].as_str().is_some_and(|c| c.contains("OK"))));
    let users: Vec<&Value> = rows.iter().filter(|m| m["role"] == "user").collect();
    let person = users.iter().find(|m| m["display_text"] == "hello from the app").unwrap_or_else(|| panic!("{rows:?}"));
    assert_eq!(person["content"], "[from the person: Rinx] hello from the app", "the transcript keeps the kernel's marker");
    assert_eq!((person["speaker"].clone(), person["lane"].clone()), (json!({"kind": "person", "label": "Rinx"}), json!("person")));
    let agent = users.iter().find(|m| m["display_text"] == "CALL_APP_TOOL").unwrap_or_else(|| panic!("{rows:?}"));
    assert_eq!((agent["speaker"].clone(), agent["lane"].clone()), (json!({"kind": "system_agent"}), json!("system_agent")));
    assert_eq!(rinx.peer().unwrap().1, peer_session);
    // The app followed the system agent's turn as it ran.
    let seen = followed.lock().unwrap().clone();
    assert!(
        seen.iter().any(|d| d["speaker"]["kind"] == "system_agent" && d["lane"] == "system_agent" && d["params"]["session_id"].as_str().is_some_and(|s| peer_session.starts_with(s))),
        "the system agent's turn streamed to the app: {} events",
        seen.len()
    );

    // The person's lane sees the system agent's recent turns, read-only...
    let shown = run(&chat, ContextOp::TurnFrom { text: "SHOW_SHARED".into(), trigger: TurnTrigger::Person }, Duration::from_secs(60))
        .expect("a completion")
        .expect("the person's turn completed");
    let shown = shown["text"].as_str().unwrap().to_owned();
    assert!(shown.contains("[from the system agent] CALL_APP_TOOL"), "{shown}");
    assert!(!shown.contains("hello from the app"), "only the other lane: {shown}");
    // ...and the system agent's lane the person's.
    system_turn(format!("TELL_PEER_SHOW:{slug}"));
    let rows = history_until(&|rows| rows.iter().any(|m| m["lane"] == "system_agent" && m["content"].as_str().is_some_and(|c| c.starts_with("SHARED"))));
    let seen_by_agent = rows
        .iter()
        .find(|m| m["lane"] == "system_agent" && m["content"].as_str().is_some_and(|c| c.starts_with("SHARED")))
        .unwrap_or_else(|| panic!("{rows:?}"))["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(seen_by_agent.contains("[from the person: Rinx] hello from the app"), "{seen_by_agent}");
    // The block is never part of either transcript.
    assert!(rows.iter().all(|m| !m["content"].as_str().unwrap_or("").contains("<shared_history")), "{rows:?}");
    rinx.release();
    drop(rinx);
    core.shutdown_within(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&dir);
}
