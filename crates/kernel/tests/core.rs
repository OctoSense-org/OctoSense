//! The core against a stand-in kernel (tests/fixtures/fake_kernel.py, which
//! speaks NDJSON JSON-RPC like `octos serve --stdio` and reports its pid).
//! Unix only: the stand-in is a python3 script run as a program.
#![cfg(unix)]

use std::path::PathBuf;
use std::time::Duration;

use octosense_kernel::{CloseReason, Connection, Core, Options, Unavailable};
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
