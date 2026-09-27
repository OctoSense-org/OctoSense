//! The broker against a REAL octos kernel (UPCR-2026-034) started by
//! `octosense-octos-core` in a temp core dir, with a scripted local model
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
use octosense_octos_core::{Core, Options};
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
    let cfg = BrokerConfig::new(
        Deployment::Hosted,
        "_main",
        "_main:api:octosense#system",
        app,
        label,
        services,
    );
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
