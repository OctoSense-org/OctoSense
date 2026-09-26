//! Mock-server contract test: WS upgrade, framing, durability routing.
//!
//! Boots a `tokio-tungstenite` listener on `127.0.0.1:0`, points the transport
//! at it, pushes a fake `tool/started` notification, and asserts the matching
//! `TransportEvent::DurableNotification` arrives.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use octos_core::{SessionKey, ui_protocol::{methods, ToolStartedEvent, TurnId, UiNotification}};
use octos_app_transport::{
    Capabilities, ProfileId, SecretString, TransportConfig, TransportEvent, ws,
};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message as WsMessage};
use url::Url;

#[tokio::test(flavor = "current_thread")]
async fn tool_started_arrives_as_durable_notification() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    let session_id = SessionKey::new("cli", "demo");
    let notif = UiNotification::ToolStarted(ToolStartedEvent {
        session_id,
        topic: None,
        turn_id: TurnId::new(),
        tool_call_id: "tc-1".into(),
        tool_name: "shell".into(),
        arguments: None,
    });
    let frame = serde_json::to_string(
        &notif.clone().into_rpc_notification().expect("notification serializes"),
    )
    .expect("frame serializes");

    tokio::spawn(async move {
        let (sock, _) = listener.accept().await.expect("server accept");
        let mut ws = accept_async(sock).await.expect("ws upgrade");
        let _ = ws.send(WsMessage::Text(frame)).await;
        loop {
            tokio::select! {
                msg = ws.next() => match msg { Some(Ok(_)) => continue, _ => break },
                _ = tokio::time::sleep(Duration::from_secs(2)) => break,
            }
        }
    });

    let cfg = TransportConfig {
        base_url: Url::parse(&format!("http://{addr}")).unwrap(),
        bearer: SecretString::new("tk-1"),
        profile_id: ProfileId::new("p1"),
        cursor: None,
        cursor_file: None,
        requested_capabilities: Capabilities::requested(),
        workspace_cwd: Some("/tmp".to_owned()),
        stdio: None,
    };
    let (_cmd_tx, mut events) = ws::spawn(cfg);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut saw_durable = false;
    let mut seen: Vec<String> = Vec::new();
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, events.recv()).await {
            Ok(Some(TransportEvent::DurableNotification { payload, .. })) => {
                assert_eq!(payload.method(), methods::TOOL_STARTED);
                saw_durable = true;
                break;
            }
            Ok(Some(other)) => {
                seen.push(format!("{other:?}"));
                continue;
            }
            _ => break,
        }
    }
    assert!(saw_durable, "expected a DurableNotification within 5s; saw: {seen:#?}");
}
