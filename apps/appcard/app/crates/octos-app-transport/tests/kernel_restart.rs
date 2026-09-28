//! The kernel transport over the shell's octos core, against the core's
//! stand-in kernel (crates/kernel/tests/fixtures/fake_kernel.py): a
//! session opens, the shell restarts the kernel (the AI providers changed),
//! and the transport reconnects to the new kernel and opens the session
//! again by itself, without a second `SessionOpen` for the app.
#![cfg(unix)]

use std::path::PathBuf;
use std::time::Duration;

use octos_app_transport::{
    kernel, Capabilities, ConnectionState, LifecycleResult, OutboundCommand, ProfileId,
    SecretString, TransportConfig, TransportEvent,
};
use octos_core::app_ui::AppUiOpenSession;
use octos_core::SessionKey;
use tokio::sync::mpsc::Receiver;
use url::Url;

fn fake_kernel() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../../crates/kernel/tests/fixtures/fake_kernel.py")
}

async fn until<T>(events: &mut Receiver<TransportEvent>, mut f: impl FnMut(&TransportEvent) -> Option<T>) -> (T, Vec<String>) {
    let mut seen = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(20), events.recv())
            .await
            .unwrap_or_else(|_| panic!("timed out; saw {seen:#?}"))
            .expect("transport ended");
        if let Some(t) = f(&event) {
            return (t, seen);
        }
        seen.push(format!("{event:?}"));
    }
}

fn listed_pid(event: &TransportEvent) -> Option<u64> {
    match event {
        TransportEvent::SessionsListed { sessions } => sessions[0]["pid"].as_u64(),
        _ => None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restarted_kernel_is_rejoined_and_the_session_reopened() {
    let dir = std::env::temp_dir().join(format!("octos-app-kernel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    octosense_kernel::configure(octosense_kernel::Options::default().stdio().core_dir(&dir).program(fake_kernel()));

    let cfg = TransportConfig {
        base_url: Url::parse("http://127.0.0.1").unwrap(),
        bearer: SecretString::new(""),
        profile_id: ProfileId::new("_main"),
        cursor: None,
        cursor_file: None,
        requested_capabilities: Capabilities::requested(),
        workspace_cwd: None,
        local_kernel: true,
    };
    let (commands, mut events) = kernel::spawn(cfg);
    let session = SessionKey("_main:api:test".into());
    let open: AppUiOpenSession = serde_json::from_value(serde_json::json!({"session_id": session.0, "profile_id": "_main"})).unwrap();
    commands.send(OutboundCommand::OpenSession(open)).await.unwrap();
    until(&mut events, |e| matches!(e, TransportEvent::RpcResult(LifecycleResult::SessionOpen(_))).then_some(())).await;
    until(&mut events, |e| matches!(e, TransportEvent::ConnectionState(ConnectionState::Live)).then_some(())).await;
    commands.send(OutboundCommand::ListSessions).await.unwrap();
    let (first, _) = until(&mut events, listed_pid).await;

    // The providers changed: the shell restarts its kernel.
    assert!(octosense_kernel::restart());
    until(&mut events, |e| matches!(e, TransportEvent::ConnectionState(ConnectionState::Reconnecting { attempt: 1 })).then_some(())).await;
    let (_, between) = until(&mut events, |e| matches!(e, TransportEvent::ConnectionState(ConnectionState::Live)).then_some(())).await;
    assert!(
        !between.iter().any(|e| e.contains("SessionOpen")),
        "the re-open is the transport's own, not a new SessionOpen for the app: {between:#?}"
    );
    commands.send(OutboundCommand::ListSessions).await.unwrap();
    let (second, _) = until(&mut events, listed_pid).await;
    assert_ne!(first, second, "a new kernel answers");
    assert_eq!(octosense_kernel::status().generation, 2);

    drop(commands);
    let _ = std::fs::remove_dir_all(dir);
}
