//! The core against a stand-in kernel (tests/fixtures/fake_kernel.py, which
//! speaks NDJSON JSON-RPC like `octos serve --stdio` and reports its pid).
//! Unix only: the stand-in is a python3 script run as a program.
#![cfg(unix)]

use std::path::PathBuf;
use std::time::Duration;

use octosense_kernel::{CloseReason, Connection, Core, Launch, Options, Unavailable};
use serde_json::{json, Value};

fn fake_kernel() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_kernel.py")
}

fn core_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("octos-core-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn core(tag: &str) -> (Core, PathBuf) {
    let dir = core_dir(tag);
    let log = dir.with_extension("log");
    let _ = std::fs::remove_file(&log);
    let core = Core::new(
        Options::default()
            .core_dir(&dir)
            .program(fake_kernel())
            .env("FAKE_KERNEL_LOG", log.to_string_lossy()),
    );
    (core, log)
}

async fn call(conn: &mut Connection, id: &str, method: &str, params: Value) -> Value {
    conn.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string())
        .expect("send");
    loop {
        let frame: Value = serde_json::from_str(&next(conn).await.expect("reply")).unwrap();
        if frame.get("id").and_then(Value::as_str) == Some(id) {
            return frame["result"].clone();
        }
    }
}

async fn next(conn: &mut Connection) -> Result<String, CloseReason> {
    tokio::time::timeout(Duration::from_secs(20), conn.recv()).await.expect("timed out waiting for the kernel")
}

fn alive(pid: u64) -> bool {
    std::process::Command::new("kill").args(["-0", &pid.to_string()]).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success())
}

async fn gone(pid: u64) -> bool {
    for _ in 0..100 {
        if !alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_during_websocket_startup_reaps_the_child() {
    let dir = core_dir("pending-listener");
    let log = dir.with_extension("log");
    let _ = std::fs::remove_file(&log);
    // This fixture never announces a listener. Stop must interrupt readiness,
    // not wait for its 90-second timeout or leak a child/data-directory lock.
    let core = Core::new(Options::default().program(fake_kernel()).core_dir(&dir)
        .env("FAKE_KERNEL_LOG", log.to_string_lossy()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("external-access.json"), r#"{"enabled":true}"#).unwrap();
    let mut conn = core.connect().unwrap();
    let started = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(&log) {
                if let Ok(value) = serde_json::from_str::<Value>(text.trim()) { break value; }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    let pid = started["pid"].as_u64().unwrap();
    assert!(alive(pid));
    core.shutdown_within(Duration::from_secs(5));
    assert!(gone(pid).await);
    assert_eq!(next(&mut conn).await.unwrap_err(), CloseReason::Shutdown);
    assert!(!octosense_kernel::connection_file(&dir).exists());
    let _ = std::fs::remove_file(log);
    let _ = std::fs::remove_dir_all(dir);
}

fn args(launch: &Launch) -> Vec<String> {
    match launch {
        Launch::Stdio { args, .. } | Launch::WebSocket { args, .. } => args.clone(),
        Launch::Embedded { .. } => Vec::new(),
    }
}

fn env(launch: &Launch) -> Vec<(String, String)> {
    match launch {
        Launch::Stdio { env, .. } | Launch::WebSocket { env, .. } => env.clone(),
        Launch::Embedded { .. } => Vec::new(),
    }
}

#[test]
fn talk_to_octos_is_off_by_default_and_the_toggle_switches_the_launch() {
    let (core, _) = core("toggle");
    // Off: the private stdio pipe, no listener, no external access.
    let launch = core.launch().unwrap();
    assert!(matches!(launch, Launch::Stdio { .. }));
    assert!(args(&launch).contains(&"--stdio".to_string()));
    assert!(!core.external_access());
    assert!(core.client_access().unwrap_err().contains("Talk to Octos"));
    assert!(core.pairing().is_err());
    // On: octos's host-managed loopback server.
    core.set_external_access(true).unwrap();
    assert!(core.external_access());
    let launch = core.launch().unwrap();
    let on = args(&launch);
    assert!(matches!(launch, Launch::WebSocket { .. }));
    assert!(!on.contains(&"--stdio".to_string()));
    for flag in ["--host-managed", "--host", "127.0.0.1"] {
        assert!(on.contains(&flag.to_string()), "{on:?}");
    }
    // Off again: back to the pipe; the setting and descriptor are gone.
    core.set_external_access(false).unwrap();
    assert!(!core.external_access());
    assert!(matches!(core.launch().unwrap(), Launch::Stdio { .. }));
    let dir = core.core_dir().unwrap();
    assert!(!dir.join("external-access.json").exists());
    assert!(!octosense_kernel::connection_file(&dir).exists());
    core.shutdown_within(Duration::from_secs(5));
}

#[test]
fn a_malformed_web_origin_leaves_the_kernel_available() {
    let (core, _) = core("bad-origin");
    let dir = core.core_dir().unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("external-access.json"), r#"{"enabled":true}"#).unwrap();
    for bad in ["http://evil.example", "not a url", "https://web.example/path"] {
        std::fs::write(dir.join("web-client-origin.txt"), bad).unwrap();
        let launch = core.launch().expect("a bad origin never makes the kernel unavailable");
        assert!(!env(&launch).iter().any(|(k, _)| k == "OCTOS_APPUI_ALLOWED_ORIGINS"), "{bad}");
    }
    std::fs::write(dir.join("web-client-origin.txt"), "https://web.example").unwrap();
    let launch = core.launch().unwrap();
    assert!(env(&launch).contains(&("OCTOS_APPUI_ALLOWED_ORIGINS".into(), "https://web.example".into())));
}

#[tokio::test(flavor = "multi_thread")]
async fn one_kernel_serves_every_consumer() {
    let (core, log) = core("single");
    assert!(!core.status().running);
    let mut a = core.connect().unwrap();
    let mut b = core.connect().unwrap();
    // Both use id "1": each gets its own reply.
    let ra = call(&mut a, "1", "session/list", json!({})).await;
    let rb = call(&mut b, "1", "session/list", json!({})).await;
    assert_eq!(ra["pid"], rb["pid"], "one kernel process");
    let status = core.status();
    assert!(status.running);
    assert_eq!(status.generation, 1);
    assert_eq!(status.connections, 2);
    assert_eq!(a.generation(), b.generation());
    // It was started as the desktop launch describes, in the core dir.
    let started: Vec<Value> = std::fs::read_to_string(&log).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(started.len(), 1, "started once");
    let dir = core.core_dir().unwrap();
    assert_eq!(started[0]["OCTOS_HOME"], dir.to_string_lossy().as_ref());
    assert_eq!(started[0]["argv"][0], "serve");
    assert_eq!(started[0]["argv"][1], "--stdio");
    assert!(dir.join("workspace").is_dir(), "the cwd was made");
}

#[tokio::test(flavor = "multi_thread")]
async fn notifications_reach_the_consumer_whose_session_it_is() {
    let (core, _) = core("route");
    let mut a = core.connect().unwrap();
    let mut b = core.connect().unwrap();
    let opened = call(&mut a, "o", "session/open", json!({"session_id": "_main:a"})).await;
    assert_eq!(opened["opened"]["session_id"], "_main:a");
    let ping: Value = serde_json::from_str(&next(&mut a).await.unwrap()).unwrap();
    assert_eq!(ping["method"], "session/ping");
    call(&mut b, "o", "session/open", json!({"session_id": "_main:b"})).await;
    let ping: Value = serde_json::from_str(&next(&mut b).await.unwrap()).unwrap();
    assert_eq!(ping["params"]["session_id"], "_main:b");
    // a's session notifies a only: b's next frame is its own reply.
    call(&mut a, "n", "test/notify", json!({"session_id": "_main:a"})).await;
    let r = call(&mut b, "x", "test/echo", json!({})).await;
    assert_eq!(r["method"], "test/echo");
    let ping: Value = serde_json::from_str(&next(&mut a).await.unwrap()).unwrap();
    assert_eq!(ping["params"]["session_id"], "_main:a");
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_replaces_a_running_kernel_and_consumers_reconnect() {
    let (core, _) = core("restart");
    assert!(!core.restart(), "nothing runs: nothing to restart");
    let mut a = core.connect().unwrap();
    let first = call(&mut a, "1", "session/list", json!({})).await["pid"].as_u64().unwrap();
    assert!(core.restart());
    assert_eq!(next(&mut a).await, Err(CloseReason::Restarted));
    assert_eq!(a.send("{}"), Err(CloseReason::Restarted));
    assert!(gone(first).await, "the old kernel exited");
    let mut a2 = core.connect().unwrap();
    assert_eq!(a2.generation(), 2);
    let second = call(&mut a2, "1", "session/list", json!({})).await["pid"].as_u64().unwrap();
    assert_ne!(first, second, "a fresh kernel");
    drop(a);
    assert!(core.status().running, "dropping a closed connection leaves the new kernel alone");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_kernel_stops_when_the_last_consumer_leaves() {
    let (core, _) = core("idle");
    let mut a = core.connect().unwrap();
    let b = core.connect().unwrap();
    let pid = call(&mut a, "1", "session/list", json!({})).await["pid"].as_u64().unwrap();
    drop(b);
    assert!(core.status().running);
    drop(a);
    assert!(!core.status().running);
    assert!(gone(pid).await, "the kernel exited");
    // The next consumer starts a new one.
    let mut c = core.connect().unwrap();
    let again = call(&mut c, "1", "session/list", json!({})).await["pid"].as_u64().unwrap();
    assert_ne!(pid, again);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_kernel_that_dies_closes_its_connections_with_its_last_words() {
    let (core, _) = core("exit");
    let mut a = core.connect().unwrap();
    call(&mut a, "1", "session/list", json!({})).await;
    a.send(json!({"jsonrpc": "2.0", "method": "test/exit", "params": {}}).to_string()).unwrap();
    match next(&mut a).await {
        Err(CloseReason::Exited(why)) => assert!(why.contains("asked to exit") || why.contains("exit"), "{why}"),
        other => panic!("expected Exited, got {other:?}"),
    }
    for _ in 0..100 {
        if !core.status().running {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!core.status().running);
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_stops_and_waits() {
    let (core, _) = core("shutdown");
    let mut a = core.connect().unwrap();
    let pid = call(&mut a, "1", "session/list", json!({})).await["pid"].as_u64().unwrap();
    let c = core.clone();
    assert!(tokio::task::spawn_blocking(move || c.shutdown_within(Duration::from_secs(5))).await.unwrap());
    assert!(!alive(pid), "exited before shutdown returned");
    assert_eq!(next(&mut a).await, Err(CloseReason::Shutdown));
}

#[test]
fn no_kernel_binary_means_no_connection() {
    let core = Core::new(Options::default().core_dir(core_dir("none")).program("/nonexistent/octos"));
    assert!(matches!(core.connect(), Err(Unavailable::NoKernel(_))));
    assert!(!core.status().running);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_kernel_that_cannot_start_fails_the_connection() {
    use std::os::unix::fs::PermissionsExt;
    // Executable permissions pass resolution; a missing interpreter makes
    // the actual spawn fail, exercising the asynchronous failure path.
    let dir = core_dir("bad-interpreter");
    std::fs::create_dir_all(&dir).unwrap();
    let program = dir.join("octos");
    std::fs::write(&program, "#!/nonexistent/octos-interpreter\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let core = Core::new(Options::default().core_dir(dir.join("core")).program(&program));
    let mut a = core.connect().unwrap();
    assert!(matches!(next(&mut a).await, Err(CloseReason::Failed(_))));
}

/// G13 (ADR 0004 §12): a tool policy that cannot be enforced (here a
/// foreign one in the profile) starts no kernel: the connection closes
/// with the reason and the program never runs. Once the profile is
/// OctoSense's again, the next connection starts it with the policy.
#[tokio::test(flavor = "multi_thread")]
async fn a_tool_policy_that_cannot_be_enforced_starts_no_kernel() {
    let (core, log) = core("foreign-policy");
    let dir = core.core_dir().unwrap();
    let profile = octosense_kernel::dirs::profile_path(&dir);
    std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
    let foreign = r#"{"id":"_main","config":{"tool_policy":{"allow":["*"]}}}"#;
    std::fs::write(&profile, foreign).unwrap();
    let mut conn = core.connect().unwrap();
    match next(&mut conn).await {
        Err(CloseReason::Failed(why)) => assert!(why.contains("tool policy") && why.contains("not started"), "{why}"),
        other => panic!("the kernel must not start: {other:?}"),
    }
    assert!(!log.exists(), "the kernel program never ran");
    assert_eq!(std::fs::read_to_string(&profile).unwrap(), foreign, "the person's policy is left alone");
    // The profile is OctoSense's again: the next start writes the policy.
    std::fs::write(&profile, r#"{"id":"_main","config":{}}"#).unwrap();
    let mut conn = core.connect().unwrap();
    let listed = call(&mut conn, "1", "session/list", json!({})).await;
    assert!(listed["pid"].is_u64(), "{listed}");
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&profile).unwrap()).unwrap();
    assert_eq!(written["config"]["tool_policy"], octosense_kernel::system_tools::tool_policy());
}

/// ADR 0004 §12, plan step 4: every kernel start sets the system agent's
/// EXACT kernel tool list on its session (octos `session/tool_list/set`,
/// octos#2648) on the host's own connection, with no token, before any
/// consumer's frame reaches the kernel; a grant change sets it again on the
/// running kernel, and a restart sets it again on the new one.
#[tokio::test(flavor = "multi_thread")]
async fn every_kernel_start_sets_the_system_agents_exact_tool_list_first() {
    use octosense_kernel::system_tools::{self, SystemAgentTools};
    let dir = core_dir("tool-list");
    let frames = dir.with_extension("frames");
    let _ = std::fs::remove_file(&frames);
    let core = Core::new(Options::default().core_dir(&dir).program(fake_kernel())
        .env("FAKE_KERNEL_FRAMES", frames.to_string_lossy()));
    let sets = || -> Vec<Value> {
        std::fs::read_to_string(&frames).unwrap_or_default().lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap()).collect()
    };
    let mut conn = core.connect().unwrap();
    call(&mut conn, "1", "session/list", json!({})).await;
    let seen = sets();
    assert_eq!(seen[0]["method"], "session/tool_list/set", "first, before any consumer's frame: {seen:?}");
    let expected = json!({"session_id": octosense_kernel::SYSTEM_SESSION, "profile_id": "_main",
        "generic_tools": SystemAgentTools::new().kernel_tools()});
    assert_eq!(seen[0]["params"], expected, "no host token: the host's own connection");
    assert_eq!(seen[1]["method"], "session/list");

    // The person's grants change while the kernel runs: set again.
    let mut granted = SystemAgentTools::new();
    granted.grant_command_execution(true);
    system_tools::set_grants(granted.clone());
    core.apply_system_agent_tool_list();
    call(&mut conn, "2", "session/list", json!({})).await;
    let seen = sets();
    assert_eq!(seen[2]["method"], "session/tool_list/set", "{seen:?}");
    assert_eq!(seen[2]["params"]["generic_tools"], json!(granted.kernel_tools()));
    system_tools::set_grants(SystemAgentTools::new());

    // A restart: the new kernel gets the list first too.
    core.restart();
    let mut conn = core.connect().unwrap();
    call(&mut conn, "3", "session/list", json!({})).await;
    let seen = sets();
    assert_eq!(seen[4]["method"], "session/tool_list/set", "{seen:?}");
    assert_eq!(seen[5]["method"], "session/list");
}
