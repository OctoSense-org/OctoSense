//! A real `octos` kernel started by the core from a temp core dir whose
//! profile `octosense_llm_config` wrote, as the AI providers app's `llm`
//! service does. No provider is called: keys are fixtures and the kernel is
//! only asked what it runs on (`profile/llm/list`).
//!
//! Runs when `OCTOS_CORE_TEST_KERNEL` names an `octos` binary built from the
//! octos rev AppCard pins:
//!
//! ```sh
//! cargo build --release -p octos-cli --bin octos --no-default-features --features api,git,ast
//! ```
//!
//! Without it the test says so and passes (CI has no kernel binary).
//!
//! The Talk to Octos tests need an octos with `serve --host-managed`
//! (octos#2591); they drive the server as an external client would, and
//! probe what that client must NOT be able to do.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use octosense_kernel::{ClientAccess, CloseReason, Connection, Core, Options, SYSTEM_SESSION};
use octosense_llm_config::{profile, Provider, ProviderSet};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

const WEB: &str = "http://localhost:4173";

fn kernel() -> Option<PathBuf> {
    let program = std::env::var_os("OCTOS_CORE_TEST_KERNEL").map(PathBuf::from);
    if program.is_none() {
        eprintln!("OCTOS_CORE_TEST_KERNEL is not set: skipping the real-kernel test");
    }
    program
}

/// A blocking core call (they wait on the kernel) off the test's runtime.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.unwrap()
}

/// One raw HTTP/1.1 request; returns the status code and body.
async fn http(port: u16, method: &str, path: &str, host: &str, token: Option<&str>, body: &str) -> (u16, String) {
    let mut tcp = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let auth = token.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    tcp.write_all(format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    let mut response = String::new();
    let _ = tokio::time::timeout(Duration::from_secs(10), tcp.read_to_string(&mut response)).await;
    let status = response.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    (status, response.split_once("\r\n\r\n").map(|(_, b)| b.to_owned()).unwrap_or_default())
}

fn port_of(access: &ClientAccess) -> u16 {
    access.origin.rsplit(':').next().unwrap().parse().unwrap()
}

/// An external client the way a browser connects: the bearer subprotocol,
/// an allowed Origin, and its own feature request.
async fn external(access: &ClientAccess) -> Socket {
    let mut req = format!("{}?ui_feature=state.session_hydrate.v1,session.workspace_cwd.v1", access.endpoint())
        .into_client_request().unwrap();
    req.headers_mut().insert("Sec-WebSocket-Protocol", format!("octos-ui, octos.bearer.{}", access.token).parse().unwrap());
    req.headers_mut().insert("Origin", WEB.parse().unwrap());
    let (socket, response) = tokio_tungstenite::connect_async(req).await.expect("external client could not connect");
    assert_eq!(response.headers()["sec-websocket-protocol"], "octos-ui");
    socket
}

async fn ws_frame(socket: &mut Socket, id: &str, method: &str, params: Value) -> Value {
    socket.send(Message::Text(json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params}).to_string())).await.unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match socket.next().await.expect("external connection closed").expect("frame") {
                Message::Text(text) => {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    if frame["id"] == id {
                        return frame;
                    }
                }
                Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await.unwrap(),
                _ => {}
            }
        }
    }).await.expect("external request timed out")
}

async fn ws_call(socket: &mut Socket, id: &str, method: &str, params: Value) -> Value {
    let frame = ws_frame(socket, id, method, params).await;
    assert!(frame.get("error").is_none(), "{method}: {frame}");
    frame["result"].clone()
}

async fn ws_refused(req: impl IntoClientRequest + Unpin) -> u16 {
    match tokio_tungstenite::connect_async(req).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => response.status().as_u16(),
        Err(e) => panic!("unexpected error: {e}"),
        Ok(_) => 101,
    }
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
    let Some(program) = kernel() else { return };
    let dir = std::env::temp_dir().join(format!("octos-core-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let core_dir = dir.join("octos-home/.octos");
    write_provider(&core_dir, "deepseek", "deepseek-v4-flash");

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
    // A second consumer shares that kernel.
    let mut other = core.connect().unwrap();
    assert_eq!(other.generation(), conn.generation());
    assert_eq!(running_on(&mut other, "llm1").await.1, "deepseek-v4-flash");

    // The providers app saves another provider and restarts the kernel.
    write_provider(&core_dir, "moonshot", "kimi-k2.5");
    assert!(core.restart());
    assert_eq!(conn.recv().await, Err(CloseReason::Restarted));
    assert_eq!(other.recv().await, Err(CloseReason::Restarted));
    let mut conn = core.connect().unwrap();
    assert_eq!(conn.generation(), 2);
    call(&mut conn, "open", "session/open", json!({"session_id": "_main:octos-core-it", "profile_id": "_main"})).await;
    assert_eq!(running_on(&mut conn, "llm2").await, ("moonshot".into(), "kimi-k2.5".into()));
    let log = lines.lock().unwrap().join("\n");
    assert!(log.contains("Model: deepseek-v4-flash") && log.contains("Model: kimi-k2.5"), "{log}");

    // Talk to Octos is off: the kernel is the private pipe, nothing listens
    // and there is no descriptor.
    assert!(!core.external_access());
    assert!(!octosense_kernel::connection_file(&core_dir).exists());
    drop((conn, other));
    assert!(!core.status().running);
    let _ = std::fs::remove_dir_all(&dir);
}

struct Defer<F: FnMut()>(F);
impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn talk_to_octos_admits_an_external_client_to_the_ui_protocol_only() {
    let Some(program) = kernel() else { return };
    let dir = std::env::temp_dir().join(format!("octos-core-talk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let core_dir = dir.join("octos-home/.octos");
    write_provider(&core_dir, "deepseek", "deepseek-v4-flash");
    std::fs::write(core_dir.join("web-client-origin.txt"), WEB).unwrap();
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = lines.clone();
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program).log(move |l| {
        sink.lock().unwrap().push(l.to_owned());
    }));
    let dump = lines.clone();
    let _print_log = Defer(move || eprintln!("kernel log:\n{}", dump.lock().unwrap().join("\n")));

    // Turning it on starts the host-managed server and mints the token.
    let c = core.clone();
    blocking(move || c.set_external_access(true)).await.unwrap();
    let c = core.clone();
    let access = blocking(move || c.client_access()).await.unwrap();
    let port = port_of(&access);
    let authority = format!("127.0.0.1:{port}");
    assert!(!format!("{access:?}").contains(&access.token));

    // Native consumers share the same kernel over the host token.
    let mut native = core.connect().unwrap();
    assert_eq!(running_on(&mut native, "llm1").await.1, "deepseek-v4-flash");
    let mut browser = external(&access).await;
    let open = ws_call(&mut browser, "open", "session/open", json!({"session_id": SYSTEM_SESSION, "profile_id": "_main"})).await;
    assert_eq!(open["opened"]["session_id"], SYSTEM_SESSION);
    // Profile configuration is not the external client's, not even to read.
    let list = ws_frame(&mut browser, "llm1", "profile/llm/list", json!({"profile_id":"_main"})).await;
    assert_eq!(list["error"]["data"]["kind"], "external_method_denied", "{list}");

    // What the external token must not reach.
    for path in ["/api/admin/overview", "/api/admin/stop-all", "/api/admin/token/rotate", "/api/admin/host/pairing"] {
        let (status, _) = http(port, "POST", path, &authority, Some(&access.token), "").await;
        assert!(status == 401 || status == 403, "{path}: {status}");
    }
    let (status, _) = http(port, "GET", "/api/my/profile", &authority, Some(&access.token), "").await;
    assert_eq!(status, 403, "REST is closed to the external token");
    let shutdown = ws_frame(&mut browser, "stop", "server/shutdown", json!({})).await;
    assert!(shutdown.get("error").is_some(), "external clients cannot stop the server: {shutdown}");
    let caps = ws_call(&mut browser, "caps", "config/capabilities/list", json!({})).await;
    assert!(!caps.to_string().contains("server/shutdown"), "not advertised: {caps}");
    for topic in ["peer-rinx", "peerctx-rinx.app-a"] {
        let peer = format!("_main:api:octosense#{topic}");
        let answer = ws_frame(&mut browser, "approve", "approval/respond", json!({
            "session_id": peer, "approval_id": uuid::Uuid::new_v4().to_string(), "decision": "approve"})).await;
        assert_eq!(answer["error"]["data"]["kind"], "host_owned_peer_answer_denied", "{answer}");
    }
    // Nor manage host-owned app peers (octos UPCR-2026-036).
    let control = ws_frame(&mut browser, "ctx", "peer/context/open", json!({
        "session_id": SYSTEM_SESSION, "peer": "rinx", "context_id": "a"})).await;
    assert_eq!(control["error"]["data"]["kind"], "external_method_denied", "{control}");
    // Nor configure providers (a redirected base_url would receive the key),
    // skills or snapshots, nor open an app peer's session.
    for method in ["profile/llm/upsert", "profile/sub_providers/upsert", "profile/skills/install", "snapshot/restore"] {
        let denied = ws_frame(&mut browser, method, method, json!({"profile_id": "_main"})).await;
        assert_eq!(denied["error"]["data"]["kind"], "external_method_denied", "{method}: {denied}");
    }
    let peer_open = ws_frame(&mut browser, "peer-open", "session/open", json!({
        "session_id": "_main:api:octosense#peer-rinx", "profile_id": "_main"})).await;
    assert_eq!(peer_open["error"]["data"]["kind"], "host_owned_peer_session_denied", "{peer_open}");
    // DNS rebinding and other local apps.
    let (status, _) = http(port, "GET", "/health", &format!("rebind.example:{port}"), None, "").await;
    assert_eq!(status, 421, "a foreign Host header is refused");
    let (status, _) = http(port, "GET", "/health", &authority, None, "").await;
    assert_eq!(status, 200);
    assert_eq!(ws_refused(access.endpoint()).await, 401, "no token");
    let mut spoofed = access.endpoint().into_client_request().unwrap();
    spoofed.headers_mut().insert("X-Profile-Id", "_main".parse().unwrap());
    assert_eq!(ws_refused(spoofed).await, 401, "no trusted-proxy impersonation");
    for solo in ["/api/auth/solo", "/api/auth/solo/create"] {
        let body = r#"{"name":"Unauthorized app","username":"intruder","email":"intruder@solo.local"}"#;
        let (status, _) = http(port, "POST", solo, &authority, None, body).await;
        assert!(status == 403 || status == 404, "{solo}: {status}");
    }
    for origin in ["https://untrusted.example", "http://localhost:5173"] {
        let mut denied = format!("{}?token={}", access.endpoint(), access.token).into_client_request().unwrap();
        denied.headers_mut().insert("Origin", origin.parse().unwrap());
        assert_eq!(ws_refused(denied).await, 403, "{origin}");
    }
    // The descriptor carries the external token only, privately.
    let descriptor = octosense_kernel::connection_file(&core_dir);
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&descriptor).unwrap()).unwrap();
    assert_eq!(saved["token"], access.token.as_str());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&descriptor).unwrap().permissions().mode() & 0o777, 0o600);
    }

    // A provider restart keeps the port and token (the host keeps the
    // listener), and the server comes straight back.
    write_provider(&core_dir, "moonshot", "kimi-k2.5");
    drop(browser);
    assert!(core.restart());
    while native.recv().await.is_ok() {}
    let c = core.clone();
    let restarted = blocking(move || c.client_access()).await.unwrap();
    assert_eq!(restarted.origin, access.origin, "the same port");
    assert_eq!(restarted.token, access.token, "the same external token");
    let mut browser = external(&restarted).await;
    ws_call(&mut browser, "open2", "session/open", json!({"session_id": SYSTEM_SESSION, "profile_id": "_main"})).await;
    let mut native_check = core.connect().unwrap();
    assert_eq!(running_on(&mut native_check, "llm2").await.1, "kimi-k2.5");
    drop(native_check);
    drop(native);
    assert!(core.status().running, "external clients keep the server up");

    // Rotating retires the old token and ends the live connection.
    let c = core.clone();
    blocking(move || c.rotate_external_access()).await.unwrap();
    let ended = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match browser.next().await {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(_)) => {}
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "Revoke all ends a live external session");
    let c = core.clone();
    let rotated = blocking(move || c.client_access()).await.unwrap();
    assert_ne!(rotated.token, access.token);
    let mut old = format!("{}?token={}", access.endpoint(), access.token).into_client_request().unwrap();
    old.headers_mut().insert("Origin", WEB.parse().unwrap());
    assert_eq!(ws_refused(old).await, 401, "the old token is dead");

    // Turning it off stops external access: nothing listens any more.
    let c = core.clone();
    blocking(move || c.set_external_access(false)).await.unwrap();
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_err() { break; }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_err(), "nothing listens with Talk to Octos off");
    assert!(!descriptor.exists());
    assert!(!lines.lock().unwrap().iter().any(|l| l.contains(&access.token) || l.contains(&rotated.token)), "no token in logs");
    core.shutdown_within(Duration::from_secs(15));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn pairing_hands_a_web_client_the_external_token_once() {
    let Some(program) = kernel() else { return };
    let dir = std::env::temp_dir().join(format!("octos-core-pair-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let core_dir = dir.join("octos-home/.octos");
    write_provider(&core_dir, "deepseek", "deepseek-v4-flash");
    let core = Core::new(Options::default().core_dir(&core_dir).program(&program));
    let c = core.clone();
    blocking(move || c.set_external_access(true)).await.unwrap();
    let c = core.clone();
    let access = blocking(move || c.client_access()).await.unwrap();
    let port = port_of(&access);
    let authority = format!("127.0.0.1:{port}");
    let (status, _) = http(port, "GET", "/pair/info", &authority, None, "").await;
    assert_eq!(status, 404, "pairing is off until the host enables it");
    let c = core.clone();
    let pairing = blocking(move || c.pairing()).await.unwrap();
    assert_eq!(pairing.code.len(), 8);
    assert_eq!(pairing.expires_in_secs, 300);
    let link = octosense_kernel::pairing_link(WEB, &pairing, None).unwrap();
    assert!(link.contains(&pairing.code) && !link.contains(&access.token));
    let claim = json!({"code": pairing.code}).to_string();
    let (status, body) = http(port, "POST", "/pair/claim", &authority, None, &claim).await;
    assert_eq!(status, 200);
    let claimed: Value = serde_json::from_str(body.trim()).unwrap();
    assert_eq!(claimed["token"], access.token.as_str(), "the external token, never the host's");
    let (status, _) = http(port, "POST", "/pair/claim", &authority, None, &claim).await;
    assert_eq!(status, 400, "single use");
    let c = core.clone();
    blocking(move || c.end_pairing()).await;
    let (status, _) = http(port, "GET", "/pair/info", &authority, None, "").await;
    assert_eq!(status, 404, "off again when the sheet closes");
    let c = core.clone();
    blocking(move || c.set_external_access(false)).await.unwrap();
    core.shutdown_within(Duration::from_secs(15));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn native_and_browser_talk_to_the_same_system_agent() {
    use std::io::BufRead;
    let Some(program) = kernel() else { return };
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
    std::fs::write(dir.join("web-client-origin.txt"), WEB).unwrap();
    let core = Core::new(Options::default().program(&program).core_dir(&dir));
    let c = core.clone();
    blocking(move || c.set_external_access(true)).await.unwrap();
    let mut native = core.connect().unwrap();
    let access = native.client_access().await.unwrap();
    let reference: Value = serde_json::from_str(&native.system_reference().await.unwrap()).unwrap();
    assert!(reference[0].as_str().unwrap().starts_with('/'));
    assert_eq!(reference[1], "_main");
    assert_eq!(reference[2], SYSTEM_SESSION);
    let mut browser = external(&access).await;
    let open = json!({"session_id":SYSTEM_SESSION,"profile_id":"_main"});
    call(&mut native, "open", "session/open", open.clone()).await;
    // An external client never chooses the workspace: a cwd is refused, and
    // without one it lands in the workspace the system session is bound to.
    let mut web_open = open.clone();
    web_open["cwd"] = reference[0].clone();
    let refused = ws_frame(&mut browser, "open-cwd", "session/open", web_open).await;
    assert_eq!(refused["error"]["data"]["kind"], "external_parameter_denied", "{refused}");
    let opened = ws_call(&mut browser, "open", "session/open", open).await;
    assert_eq!(opened["opened"]["workspace_root"], reference[0], "Web lands in the system workspace");
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
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let state = call(&mut native, "state", "turn/state/get", json!({
                "session_id":SYSTEM_SESSION,"turn_id":first["turn_id"]})).await;
            if state["state"] == "completed" { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("native turn completed");
    // The browser cannot interrupt the host's turn on the shared session.
    let interrupt = ws_frame(&mut browser, "int", "turn/interrupt", json!({
        "session_id":SYSTEM_SESSION,"turn_id":first["turn_id"]})).await;
    assert_eq!(interrupt["error"]["data"]["kind"], "external_turn_denied", "{interrupt}");
    ws_call(&mut browser, "turn", "turn/start", input("hello from browser")).await;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let transcript = call(&mut native, "history", "session/hydrate", hydrate.clone()).await;
            if transcript.to_string().contains("ECHO: hello from browser") { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("native client sees browser turn");
    drop((native, browser));
    core.shutdown_within(Duration::from_secs(15));
    // After a full host restart with Talk to Octos off, native app peers
    // still resume the system session in its saved workspace.
    std::fs::remove_file(dir.join("external-access.json")).unwrap();
    let restarted = Core::new(Options::default().program(&program).core_dir(&dir));
    let mut native = restarted.connect().unwrap();
    let opened = call(&mut native, "resume", "session/open", json!({
        "session_id":SYSTEM_SESSION,"profile_id":"_main"})).await;
    assert_eq!(opened["opened"]["workspace_root"], reference[0]);
    drop(native);
    restarted.shutdown_within(Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(dir);
}

/// What the system agent's turns were offered: `(stdio, host over Talk to
/// Octos, external client)`. The scripted model logs the tool names each
/// request offered. The profile carries an older OctoSense policy that allows
/// `shell`: the kernel start replaces it with the current one.
async fn offered_to_system_turns(program: &Path, tag: &str) -> [std::collections::BTreeSet<String>; 3] {
    use std::collections::BTreeSet;
    use std::io::BufRead;
    let dir = std::env::temp_dir().join(format!("octos-system-tools-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("profiles")).unwrap();
    let log = dir.join("offered.jsonl");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app-peers/tests/fixtures/mock_llm.py");
    let mut model = std::process::Command::new("python3").arg(script).env("MOCK_LLM_TOOLS_LOG", &log)
        .stdout(std::process::Stdio::piped()).spawn().unwrap();
    let mut line = String::new();
    std::io::BufReader::new(model.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let port: u16 = line.trim().parse().unwrap();
    let _model = Defer(move || { let _ = model.kill(); let _ = model.wait(); });
    std::fs::write(dir.join("profiles/_main.json"), json!({
        "id":"_main", "name":"Main", "enabled":true,
        "created_at":"2026-09-27T00:00:00Z", "updated_at":"2026-09-27T00:00:00Z",
        "config":{"llm":{"primary":{"family_id":"local", "model_id":"mock-model",
            "route":{"base_url":format!("http://127.0.0.1:{port}/v1"), "api_type":"openai"}}},
            "tool_policy":{"allow":["read_file","shell"],"owner":"octosense"}}
    }).to_string()).unwrap();
    std::fs::write(dir.join("web-client-origin.txt"), WEB).unwrap();
    let offered = |probe: &str| -> Option<BTreeSet<String>> {
        let text = std::fs::read_to_string(&log).ok()?;
        text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|v| v["user"].as_str().is_some_and(|u| u.contains(probe)))
            .find(|v| v["tools"].as_array().is_some_and(|t| !t.is_empty()))
            .map(|v| v["tools"].as_array().unwrap().iter().filter_map(Value::as_str).map(str::to_owned).collect())
    };
    let wait_for = |probe: &'static str| {
        let offered = &offered;
        async move {
            tokio::time::timeout(Duration::from_secs(60), async {
                loop {
                    if let Some(tools) = offered(probe) { return tools; }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }).await.unwrap_or_else(|_| panic!("the model never saw the {probe} turn"))
        }
    };
    let input = |text: &str| json!({"session_id":SYSTEM_SESSION,"turn_id":uuid::Uuid::new_v4(),
        "input":[{"kind":"text","text":text}]});
    let open = json!({"session_id":SYSTEM_SESSION,"profile_id":"_main"});

    // The private stdio kernel (Talk to Octos off): the host's own turn.
    let core = Core::new(Options::default().program(program).core_dir(&dir));
    let mut native = core.connect().unwrap();
    call(&mut native, "open", "session/open", open.clone()).await;
    call(&mut native, "turn", "turn/start", input("SYSTEM_TOOLS_STDIO")).await;
    let stdio = wait_for("SYSTEM_TOOLS_STDIO").await;
    drop(native);
    core.shutdown_within(Duration::from_secs(15));

    // Talk to Octos on: the host's turn over the WebSocket, then an external
    // client's turn on the same system conversation.
    let core = Core::new(Options::default().program(program).core_dir(&dir));
    let c = core.clone();
    blocking(move || c.set_external_access(true)).await.unwrap();
    let mut native = core.connect().unwrap();
    let access = native.client_access().await.unwrap();
    call(&mut native, "open", "session/open", open.clone()).await;
    let host_turn = input("SYSTEM_TOOLS_HOST");
    call(&mut native, "turn", "turn/start", host_turn.clone()).await;
    let host = wait_for("SYSTEM_TOOLS_HOST").await;
    // One turn at a time on a session: let the host's finish first.
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let state = call(&mut native, "state", "turn/state/get", json!({
                "session_id":SYSTEM_SESSION,"turn_id":host_turn["turn_id"]})).await;
            if state["state"] == "completed" { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("the host's turn completed");
    let mut browser = external(&access).await;
    ws_call(&mut browser, "open", "session/open", open).await;
    ws_call(&mut browser, "turn", "turn/start", input("SYSTEM_TOOLS_EXTERNAL")).await;
    let external_tools = wait_for("SYSTEM_TOOLS_EXTERNAL").await;
    drop((native, browser));
    let c = core.clone();
    blocking(move || c.set_external_access(false)).await.unwrap();
    core.shutdown_within(Duration::from_secs(15));
    let _ = std::fs::remove_dir_all(dir);
    [stdio, host, external_tools]
}

/// ADR 0004 §12, what is enforced today: a system-agent turn, whoever starts
/// it, is offered none of octos's shell (`group:runtime`: `shell`, `bash`,
/// `exec_command`, `write_stdin`), never `peer_close`, and every tool of its own list octos
/// registers (the exact list is the next test). A Talk to Octos external client's
/// turn keeps octos's external allowlist.
#[tokio::test(flavor = "multi_thread")]
async fn a_system_agent_turn_is_offered_no_octos_shell() {
    use octosense_kernel::system_tools::{EXTERNAL_TURN_TOOLS, SYSTEM_AGENT_TOOLS};
    use std::collections::BTreeSet;
    let Some(program) = kernel() else { return };
    let [stdio, host, external_tools] = offered_to_system_turns(&program, "ceiling").await;
    // `recall` (a session's evicted tool outputs) is registered only on
    // octos's session-actor turns, not on UI Protocol turns.
    let own: BTreeSet<String> = SYSTEM_AGENT_TOOLS.iter().filter(|t| **t != "recall")
        .map(|t| t.to_string()).collect();
    for (how, offered) in [("stdio", &stdio), ("Talk to Octos", &host)] {
        for shell in ["shell", "bash", "exec_command", "write_stdin"] {
            assert!(!offered.contains(shell), "{how}: {shell} offered: {offered:?}");
        }
        // A closed app peer cannot be resumed or replaced: no agent closes one.
        assert!(!offered.contains("peer_close"), "{how}: peer_close offered: {offered:?}");
        assert!(own.is_subset(offered), "{how}: missing its own tools: {:?}", &own - offered);
    }
    assert_eq!(stdio, host, "the same set over Talk to Octos");
    let allowlist: BTreeSet<String> = EXTERNAL_TURN_TOOLS.iter().map(|t| t.to_string()).collect();
    assert!(external_tools.is_subset(&allowlist), "{external_tools:?}");
    assert_eq!(external_tools, &stdio & &allowlist, "external clients keep every allowlisted tool the kernel offers");
}

/// ADR 0004 §12, plan step 4: a system-agent turn is offered EXACTLY its
/// list (its default kernel tools; nothing granted here), on the private
/// pipe and over Talk to Octos alike. Each kernel start sets it on the
/// system session (`session/tool_list/set`, octos#2648).
#[tokio::test(flavor = "multi_thread")]
async fn a_system_agent_turn_is_offered_exactly_the_system_agent_tools() {
    use octosense_kernel::system_tools::SystemAgentTools;
    use std::collections::BTreeSet;
    let Some(program) = kernel() else { return };
    let [stdio, host, _] = offered_to_system_turns(&program, "exact").await;
    let expected: BTreeSet<String> = SystemAgentTools::new().names().into_iter()
        .filter(|t| t != "recall").collect();
    assert_eq!(stdio, expected);
    assert_eq!(host, expected);
}

/// The kernel must not outlive a host that dies without stopping it
/// (a crash or SIGKILL): its stdin closes and it stops. This test re-runs
/// itself as the host in a child process and kills that process.
#[test]
fn a_host_that_dies_takes_its_kernel_with_it() {
    const HOST: &str = "OCTOSENSE_KERNEL_TEST_HOST";
    let Some(program) = kernel() else { return };
    if let Some(dir) = std::env::var_os(HOST).map(PathBuf::from) {
        // The host: start the kernel, report ready, then wait to be killed.
        let shared = dir.join("shared").exists();
        write_provider(&dir, "deepseek", "deepseek-v4-flash");
        let core = Core::new(Options::default().program(&program).core_dir(&dir));
        if shared {
            core.set_external_access(true).unwrap();
            core.client_access().unwrap();
        }
        let _conn = core.connect().unwrap();
        std::thread::sleep(Duration::from_secs(3));
        std::fs::write(dir.join("ready"), "").unwrap();
        std::thread::sleep(Duration::from_secs(600));
        return;
    }
    for shared in [false, true] {
        let dir = std::env::temp_dir().join(format!("octos-core-orphan-{}-{shared}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        if shared {
            std::fs::write(dir.join("shared"), "").unwrap();
        }
        let mut host = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "a_host_that_dies_takes_its_kernel_with_it", "--nocapture", "--test-threads=1"])
            .env(HOST, &dir)
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        while !dir.join("ready").exists() {
            assert!(std::time::Instant::now() < deadline, "the host did not start its kernel");
            std::thread::sleep(Duration::from_millis(100));
        }
        let kernels = || {
            let out = std::process::Command::new("pgrep").args(["-f", &dir.to_string_lossy()]).output().unwrap();
            String::from_utf8_lossy(&out.stdout).lines().map(str::to_owned).collect::<Vec<_>>()
        };
        assert!(!kernels().is_empty(), "the kernel runs (shared: {shared})");
        host.kill().unwrap(); // SIGKILL: no orderly stop
        host.wait().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while !kernels().is_empty() {
            assert!(std::time::Instant::now() < deadline, "the kernel outlived its host (shared: {shared}): {:?}", kernels());
            std::thread::sleep(Duration::from_millis(200));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
