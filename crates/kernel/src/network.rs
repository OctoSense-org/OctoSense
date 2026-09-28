//! The shell-owned server's authenticated loopback connection.
//! A server binds port zero on first start; its actual port and token survive
//! provider restarts. Stdout is private (it also carries Octos's pairing code).

use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

pub const SYSTEM_SESSION: &str = "_main:api:octosense#system";
pub const CONNECTION_FILE: &str = "client-connection.json";
pub(crate) const SYSTEM_WORKSPACE_FILE: &str = "system-workspace.txt";

pub(crate) fn save_system_workspace(dir: &Path, workspace: &Path) -> Result<(), String> {
    let path = dir.join(SYSTEM_WORKSPACE_FILE);
    let tmp = dir.join(format!(".system-workspace-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        std::fs::write(&tmp, workspace.to_string_lossy().as_bytes())?;
        std::fs::rename(&tmp, path)
    })();
    let _ = std::fs::remove_file(tmp);
    result.map_err(|e: std::io::Error| format!("Could not save the system workspace: {e}"))
}

// Preserve the pinned kernel's stdio_defaults feature contract for native
// consumers. External clients negotiate their own features independently.
const NATIVE_FEATURES: &str = concat!(
    "approval.typed.v1,pane.snapshots.v1,session.workspace_cwd.v1,session.sandbox.v1,",
    "harness.task_control.v1,harness.task_artifacts.v1,state.session_hydrate.v1,",
    "state.thread_graph.v1,state.turn_state_get.v1,event.spawn_complete.v1,",
    "event.file_attached.v1,event.voice_audio.v1,voice.asr_admission.v1,plan.todos.v1,",
    "event.background_activity.v1,auxiliary.rest_to_ws.v1,coding.autonomy.v1,",
    "coding.agent_control.v1,coding.goal_runtime.v1,coding.loop_runtime.v1,",
    "coding.monitor_runtime.v1,review.start.v1,context.lifecycle.v1,user_question.v1,",
    "skill.actions.v1,skill.action_jobs.v1,event.turn_steer_dropped.v1"
);

pub(crate) fn validate_origin(origin: &str) -> Result<String, String> {
    if origin.is_empty() { return Ok(String::new()); }
    let url = url::Url::parse(origin).map_err(|_| "Enter a web client origin such as http://localhost:4173".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(|h| h.contains('*'))
        || !url.username().is_empty() || url.password().is_some() || url.path() != "/"
        || url.query().is_some() || url.fragment().is_some()
    {
        return Err("Use an http(s) origin without a path, credentials, query or fragment.".into());
    }
    Ok(url.origin().ascii_serialization())
}

/// For a trusted host sheet or a user-owned client. Never pass to an app
/// script, log it, or put its token in a command line.
#[derive(Clone)]
pub struct ClientAccess {
    pub origin: String,
    pub token: String,
}

impl std::fmt::Debug for ClientAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientAccess").field("origin", &self.origin).finish_non_exhaustive()
    }
}

impl ClientAccess {
    pub fn endpoint(&self) -> String {
        format!("ws://{}/api/ui-protocol/ws", self.origin.trim_start_matches("http://"))
    }

    pub(crate) fn save(&self, dir: &Path) -> std::io::Result<()> {
        use std::io::Write;
        let path = dir.join(CONNECTION_FILE);
        let tmp = dir.join(format!(".client-connection-{}", uuid::Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| {
            let mut file = options.open(&tmp)?;
            let data = json!({"origin": self.origin, "endpoint": self.endpoint(), "token": self.token,
                "profile_id": "_main", "session_id": SYSTEM_SESSION, "pid": std::process::id()});
            file.write_all(data.to_string().as_bytes())?;
            std::fs::rename(&tmp, &path)
        })();
        let _ = std::fs::remove_file(tmp);
        result
    }
}

pub(crate) struct Network {
    pub port: AtomicU16,
    pub token: String,
}

impl Default for Network {
    fn default() -> Self {
        Self { port: AtomicU16::new(0), token: format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple()) }
    }
}

impl Network {
    /// Parse only the pinned server's listener announcement, with NO_COLOR.
    /// Never infer readiness from a token, a pairing URL or arbitrary logs.
    pub fn announced(&self, line: &str) -> Option<ClientAccess> {
        let origin = line.strip_prefix("Listening: ")?;
        let url = url::Url::parse(origin).ok()?;
        if url.scheme() != "http" || url.host_str() != Some("127.0.0.1")
            || !url.username().is_empty() || url.password().is_some()
            || url.path() != "/" || url.query().is_some() || url.fragment().is_some()
        {
            return None;
        }
        let port = url.port().filter(|p| *p != 0)?;
        let expected = self.port.load(Ordering::Relaxed);
        if expected != 0 && expected != port { return None; }
        self.port.store(port, Ordering::Relaxed);
        Some(ClientAccess { origin: format!("http://127.0.0.1:{port}"), token: self.token.clone() })
    }
}

/// Native consumers keep their existing frame API. This private adapter
/// carries those frames over the same WebSocket endpoint external clients use.
pub(crate) async fn connect(access: &ClientAccess) -> Result<tokio::io::DuplexStream, String> {
    let mut request = access.endpoint().into_client_request().map_err(|e| e.to_string())?;
    request.headers_mut().insert("Authorization", format!("Bearer {}", access.token).parse().unwrap());
    request.headers_mut().insert("X-Profile-Id", "_main".parse().unwrap());
    request.headers_mut().insert("X-Octos-Ui-Features", NATIVE_FEATURES.parse().unwrap());
    // The startup announcement precedes axum's accept loop by a few statements.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let socket = loop {
        match tokio_tungstenite::connect_async(request.clone()).await {
            Ok((socket, _)) => break socket,
            Err(tokio_tungstenite::tungstenite::Error::Io(_)) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(_) => return Err("the kernel's authenticated WebSocket connection failed".into()),
        }
    };
    let (client, bridge) = tokio::io::duplex(1024 * 1024);
    tokio::spawn(async move {
        let (reader, mut writer) = tokio::io::split(bridge);
        let mut lines = BufReader::new(reader).lines();
        let (mut sink, mut stream) = socket.split();
        loop {
            tokio::select! {
                line = lines.next_line() => match line {
                    Ok(Some(text)) => if sink.send(Message::Text(text)).await.is_err() { break; },
                    _ => break,
                },
                frame = stream.next() => match frame {
                    Some(Ok(Message::Text(text))) => {
                        if writer.write_all(text.as_bytes()).await.is_err() || writer.write_all(b"\n").await.is_err() { break; }
                    }
                    Some(Ok(Message::Ping(bytes))) => if sink.send(Message::Pong(bytes)).await.is_err() { break; },
                    Some(Ok(Message::Pong(_))) => {},
                    _ => break,
                },
            }
        }
        let _ = sink.close().await;
    });
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicit_web_origins_are_accepted() {
        for invalid in ["*", "http://*.example.com", "file:///tmp/client", "https://u:p@example.com",
            "https://example.com/app", "https://example.com?token=secret", "https://example.com/#x"] {
            assert!(validate_origin(invalid).is_err(), "{invalid}");
        }
        assert_eq!(validate_origin("http://localhost:4173/").unwrap(), "http://localhost:4173");
        assert_eq!(validate_origin("").unwrap(), "");
    }

    #[test]
    fn listener_discovery_rejects_untrusted_and_changed_addresses() {
        let network = Network::default();
        for invalid in ["Pair: http://127.0.0.1:1234", "Listening: http://0.0.0.0:1234",
            "Listening: http://127.0.0.1:1234/?token=x", "Listening: http://127.0.0.1:0"] {
            assert!(network.announced(invalid).is_none());
        }
        let first = network.announced("Listening: http://127.0.0.1:1234").unwrap();
        assert!(network.announced("Listening: http://127.0.0.1:1235").is_none());
        let second = network.announced("Listening: http://127.0.0.1:1234").unwrap();
        assert!(first.token == second.token);
    }
}
