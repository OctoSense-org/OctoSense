//! Real host-managed kernel tests: native/browser coexistence, authentication,
//! origin checks, provider restart, and shared system-agent turns using a local
//! scripted model. No external model or real credential is used.
//! Set OCTOS_CORE_TEST_KERNEL to the pinned binary with OctoSense's overlay.
//! Without it these integration tests are skipped (see README.md).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use octosense_llm_config::{profile, Provider, ProviderSet};
use octosense_kernel::{CloseReason, Connection, Core, Options};
use serde_json::{json, Value};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn solo_status(access: &octosense_kernel::ClientAccess, path: &str) -> String {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let mut tcp = tokio::net::TcpStream::connect(access.origin.trim_start_matches("http://")).await.unwrap();
    let body = r#"{"name":"Unauthorized app","username":"intruder","email":"intruder@solo.local"}"#;
    tcp.write_all(format!("POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", access.origin.trim_start_matches("http://"), body.len()).as_bytes()).await.unwrap();
    let mut line = String::new();
    tokio::io::BufReader::new(tcp).read_line(&mut line).await.unwrap();
    line
}

async fn external(access: &octosense_kernel::ClientAccess) -> Socket {
    // Browsers cannot set Authorization; OctosCode Web uses the query token.
    let mut req = format!("{}?token={}&ui_feature=state.session_hydrate.v1,session.workspace_cwd.v1", access.endpoint(), access.token).into_client_request().unwrap();
    req.headers_mut().insert("Origin", "http://localhost:4173".parse().unwrap());
    tokio_tungstenite::connect_async(req).await.map(|(s, _)| s).unwrap_or_else(|_| panic!("external client could not connect"))
}

async fn ws_call(socket: &mut Socket, id: &str, method: &str, params: Value) -> Value {
    socket.send(Message::Text(json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params}).to_string())).await.unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match socket.next().await.expect("external connection closed").expect("frame") {
                Message::Text(text) => {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    if frame["id"] == id {
                        assert!(frame.get("error").is_none(), "{method}: {frame}");
                        return frame["result"].clone();
                    }
                }
                Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await.unwrap(),
                _ => {},
            }
        }
    }).await.expect("external request timed out")
}

async fn call(conn: &mut Connection, id: &str, method: &str, params: Value) -> Value {
    conn.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()).unwrap();
    loop {
        let text = tokio::time::timeout(Duration::from_secs(60), conn.recv())
            .await
            .expect("kernel timed out")
            .expect("kernel frame");
        let frame: Value = serde_json::from_str(&text).unwrap();
        if frame.get("id").and_then(Value::as_str) == Some(id) {
            assert!(frame.get("error").is_none(), "{method} failed: {frame}");
            return frame["result"].clone();
        }
    }
}

/// Save `family/model` as the primary provider, with a fixture key.
fn write_provider(core_dir: &Path, family: &str, model: &str) {
    let primary = Provider::new(family, Some(model.into()));
    let env = BTreeMap::from([(primary.key_env.clone(), format!("sk-octos-core-test-fixture-{family}"))]);
    let set = ProviderSet { primary: Some(primary), fallbacks: vec![] };
    profile::save_merge(&profile::profile_path(core_dir), &set, &env).unwrap();
}

/// What the kernel says it runs on: (family, model).
async fn running_on(conn: &mut Connection, id: &str) -> (String, String) {
    let list = call(conn, id, "profile/llm/list", json!({"profile_id": "_main"})).await;
    let primary = &list["primary"];
    assert_eq!(list["runtime_policy_stamp"]["model"], primary["model"], "{list}");
    (primary["family_id"].as_str().unwrap_or_default().into(), primary["model"].as_str().unwrap_or_default().into())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_real_kernel_runs_on_the_profile_the_providers_app_wrote() {
    let Some(program) = std::env::var_os("OCTOS_CORE_TEST_KERNEL").map(PathBuf::from) else {
        eprintln!("OCTOS_CORE_TEST_KERNEL is not set: skipping the real-kernel test");
        return;
    };
    let dir = std::env::temp_dir().join(format!("octos-core-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let core_dir = dir.join("octos-home/.octos");
    write_provider(&core_dir, "deepseek", "deepseek-v4-flash");
    std::fs::write(core_dir.join("web-client-origin.txt"), "http://localhost:4173").unwrap();

    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = lines.clone();
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program).log(move |l| {
        sink.lock().unwrap().push(l.to_owned());
    }));
    let dump = lines.clone();
    let _print_log = Defer(move || eprintln!("kernel log:\n{}", dump.lock().unwrap().join("\n")));

    // A consumer opens a session on the kernel the core started.
    let mut conn = core.connect().expect("a kernel");
    let open = call(&mut conn, "open", "session/open", json!({"session_id": "_main:octos-core-it", "profile_id": "_main"})).await;
    assert_eq!(open["opened"]["session_id"], "_main:octos-core-it");
    assert_eq!(open["opened"]["active_profile_id"], "_main");
    assert_eq!(running_on(&mut conn, "llm1").await, ("deepseek".into(), "deepseek-v4-flash".into()));
    let access = conn.client_access().await.unwrap();
    assert!(!format!("{access:?}").contains(&access.token));
    let mut browser = external(&access).await;
    let open = ws_call(&mut browser, "open", "session/open", json!({"session_id":"_main:octos-core-it", "profile_id":"_main"})).await;
    assert_eq!(open["opened"]["session_id"], "_main:octos-core-it");
    let list = ws_call(&mut browser, "llm1", "profile/llm/list", json!({"profile_id":"_main"})).await;
    assert_eq!(list["primary"]["model"], "deepseek-v4-flash");
    // Reject both a missing token and an untrusted browser origin.
    assert!(tokio_tungstenite::connect_async(access.endpoint()).await.is_err());
    let mut spoofed = access.endpoint().into_client_request().unwrap();
    spoofed.headers_mut().insert("X-Profile-Id", "_main".parse().unwrap());
    assert!(tokio_tungstenite::connect_async(spoofed).await.is_err(), "another local APK cannot impersonate a trusted proxy");
    assert!(solo_status(&access, "/api/auth/solo").await.contains("403"), "no password-free local login");
    assert!(solo_status(&access, "/api/auth/solo/create").await.contains("403"), "no unauthenticated local owner creation");
    let mut denied = format!("{}?token={}", access.endpoint(), access.token).into_client_request().unwrap();
    denied.headers_mut().insert("Origin", "https://untrusted.example".parse().unwrap());
    assert!(tokio_tungstenite::connect_async(denied).await.is_err());
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(core_dir.join(octosense_kernel::CONNECTION_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
    }
    // A second consumer shares that kernel.
    let mut other = core.connect().unwrap();
    assert_eq!(other.generation(), conn.generation());
    assert_eq!(running_on(&mut other, "llm1").await.1, "deepseek-v4-flash");

    // The providers app saves another provider and restarts the kernel.
    write_provider(&core_dir, "moonshot", "kimi-k2.5");
    assert!(core.restart());
    while conn.recv().await.is_ok() {}
    assert_eq!(conn.closed(), Some(&CloseReason::Restarted));
    while other.recv().await.is_ok() {}
    assert_eq!(other.closed(), Some(&CloseReason::Restarted));
    let mut conn = core.connect().unwrap();
    assert_eq!(conn.generation(), 2);
    call(&mut conn, "open", "session/open", json!({"session_id": "_main:octos-core-it", "profile_id": "_main"})).await;
    assert_eq!(running_on(&mut conn, "llm2").await, ("moonshot".into(), "kimi-k2.5".into()));
    let restarted = conn.client_access().await.unwrap();
    assert_eq!(access.origin, restarted.origin, "external clients reconnect to the same port");
    assert!(access.token == restarted.token, "restart preserves the access token");
    drop(browser);
    let mut browser = external(&access).await;
    let list = ws_call(&mut browser, "llm2", "profile/llm/list", json!({"profile_id":"_main"})).await;
    assert_eq!(list["primary"]["model"], "kimi-k2.5");
    let log = lines.lock().unwrap().join("\n");
    assert!(log.contains("Model: deepseek-v4-flash") && log.contains("Model: kimi-k2.5"), "{log}");

    drop((conn, other));
    assert!(core.status().running, "external clients keep the shared server available");
    let list = ws_call(&mut browser, "after-native-close", "profile/llm/list", json!({"profile_id":"_main"})).await;
    assert_eq!(list["primary"]["model"], "kimi-k2.5");
    drop(browser);
    assert!(!lines.lock().unwrap().iter().any(|line| line.contains(&access.token)), "no access token in logs");
    assert!(core.shutdown_within(Duration::from_secs(5)));
    assert!(!core_dir.join(octosense_kernel::CONNECTION_FILE).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

struct Defer<F: FnMut()>(F);
impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn native_and_browser_talk_to_the_same_system_agent() {
    use std::io::BufRead;
    use octosense_kernel::SYSTEM_SESSION;
    let Some(program) = std::env::var_os("OCTOS_CORE_TEST_KERNEL") else { return };
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app-peers/tests/fixtures/mock_llm.py");
    let mut model = std::process::Command::new("python3").arg(script)
        .stdout(std::process::Stdio::piped()).spawn().unwrap();
    let mut line = String::new();
    std::io::BufReader::new(model.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let port: u16 = line.trim().parse().unwrap();
    let _model = Defer(move || { let _ = model.kill(); let _ = model.wait(); });
    let dir = std::env::temp_dir().join(format!("octos-shared-turn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("profiles")).unwrap();
    std::fs::write(dir.join("profiles/_main.json"), json!({
        "id":"_main", "name":"Main", "enabled":true,
        "created_at":"2026-09-27T00:00:00Z", "updated_at":"2026-09-27T00:00:00Z",
        "config":{"llm":{"primary":{"family_id":"local", "model_id":"mock-model",
            "route":{"base_url":format!("http://127.0.0.1:{port}/v1"), "api_type":"openai"}}}}
    }).to_string()).unwrap();
    std::fs::write(dir.join("web-client-origin.txt"), "http://localhost:4173").unwrap();
    let core = Core::new(Options::default().program(program).core_dir(&dir));
    let mut native = core.connect().unwrap();
    let access = native.client_access().await.unwrap();
    let link = native.system_web_url("http://localhost:4173").await.unwrap();
    let url = url::Url::parse(&link).unwrap();
    assert!(!link.contains(&access.token));
    let reference: Value = serde_json::from_str(&url.query_pairs().find(|(k, _)| k == "s").unwrap().1).unwrap();
    assert!(reference[0].as_str().unwrap().starts_with('/'));
    assert_eq!(reference[1], "_main");
    assert_eq!(reference[2], SYSTEM_SESSION);
    let mut browser = external(&access).await;
    let open = json!({"session_id":SYSTEM_SESSION,"profile_id":"_main"});
    call(&mut native, "open", "session/open", open.clone()).await;
    let mut web_open = open;
    web_open["cwd"] = reference[0].clone();
    let opened = ws_call(&mut browser, "open", "session/open", web_open).await;
    assert_eq!(opened["opened"]["workspace_root"], reference[0], "Web confirms the canonical saved-link workspace");
    let input = |text| json!({"session_id":SYSTEM_SESSION,"turn_id":uuid::Uuid::new_v4(),
        "input":[{"kind":"text","text":text}]});
    let first = input("hello from native");
    call(&mut native, "turn", "turn/start", first.clone()).await;
    let hydrate = json!({"session_id":SYSTEM_SESSION,"include":["messages"]});
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let transcript = ws_call(&mut browser, "history", "session/hydrate", hydrate.clone()).await;
            if transcript.to_string().contains("ECHO: hello from native") { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("browser sees native turn");
    // Completion can precede the turn/start reply; inspect durable turn
    // state rather than depending on the relative delivery of notifications.
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let state = call(&mut native, "state", "turn/state/get", json!({
                "session_id":SYSTEM_SESSION,"turn_id":first["turn_id"]})).await;
            if state["state"] == "completed" { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("native turn completed");
    ws_call(&mut browser, "turn", "turn/start", input("hello from browser")).await;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let transcript = call(&mut native, "history", "session/hydrate", hydrate.clone()).await;
            if transcript.to_string().contains("ECHO: hello from browser") { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("native client sees browser turn");
    assert_eq!(core.status().generation, 1);
    drop((native, browser));
    core.shutdown_within(Duration::from_secs(5));
    // A Web cwd makes the session persistently scoped. After even a full
    // host restart, native app peers must resume it without having to know
    // the browser's workspace routing contract.
    let restarted = Core::new(Options::default()
        .program(std::env::var_os("OCTOS_CORE_TEST_KERNEL").unwrap()).core_dir(&dir));
    let mut native = restarted.connect().unwrap();
    let opened = call(&mut native, "resume", "session/open", json!({
        "session_id":SYSTEM_SESSION,"profile_id":"_main"})).await;
    assert_eq!(opened["opened"]["workspace_root"], reference[0]);
    assert!(native.system_web_url("http://localhost:4173").await.is_ok());
    drop(native);
    restarted.shutdown_within(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(dir);
}
