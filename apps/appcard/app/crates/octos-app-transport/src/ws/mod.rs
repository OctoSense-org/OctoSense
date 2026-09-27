//! WebSocket transport task. Owns the `tokio-tungstenite` socket and runs
//! the connection state machine (`Idle → Dialing → Handshaking → Live ↔
//! Reconnecting → Failed`). The inner `select!` arbitrates outbound commands,
//! inbound frames, and 30-s heartbeat ticks; reconnect uses W01's full-jitter
//! backoff with a 5-min cumulative budget. Command dispatch and inbound frame
//! handling are delegated to `crate::proto` (shared with the stdio transport);
//! this module only owns the socket and reconnect policy.

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::{interval, MissedTickBehavior};
use tokio_tungstenite::tungstenite::handshake::client::Request as WsRequest;
use tokio_tungstenite::tungstenite::http::Uri as WsUri;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::capability::Capabilities;
use crate::proto::{
    build_outbound, handle_inbound_text, try_emit, Outbound, SharedState, CHANNEL_BUFFER,
};
use crate::{
    ConnectionState, OutboundCommand, ProfileId, SecretString, TransportConfig, TransportEvent,
};

pub const RECONNECT_DELAY_MAX: Duration = Duration::from_secs(30);
pub const RECONNECT_BUDGET: Duration = Duration::from_secs(5 * 60);
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

pub struct WsTransport {
    pub(crate) join: tokio::task::JoinHandle<()>,
}

impl WsTransport {
    pub fn abort(self) {
        self.join.abort();
    }
}

pub fn spawn(
    cfg: TransportConfig,
) -> (mpsc::Sender<OutboundCommand>, mpsc::Receiver<TransportEvent>) {
    spawn_with_waker(cfg, None)
}

/// Like [`spawn`], but invokes `waker` after every event is queued so a
/// UI-thread consumer that only drains on UI events can be woken (e.g.
/// makepad's `SignalToUI::set_ui_signal`). Without it, an RPC reply that
/// lands while the app is idle (no touches, no animation) sits in the
/// channel until the next unrelated event — observed as session-resume
/// history not appearing until the screen was tapped.
pub fn spawn_with_waker(
    cfg: TransportConfig,
    waker: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
) -> (mpsc::Sender<OutboundCommand>, mpsc::Receiver<TransportEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<OutboundCommand>(CHANNEL_BUFFER);
    let (evt_tx, evt_rx) = mpsc::channel::<TransportEvent>(CHANNEL_BUFFER);
    match waker {
        None => {
            tokio::spawn(async move { run_state_machine(cfg, cmd_rx, evt_tx).await });
        }
        Some(wake) => {
            // Forwarder tap: the state machine emits into an inner channel;
            // each event is re-queued for the consumer and then the waker
            // fires. Keeps the state machine itself waker-free.
            let (inner_tx, mut inner_rx) = mpsc::channel::<TransportEvent>(CHANNEL_BUFFER);
            tokio::spawn(async move { run_state_machine(cfg, cmd_rx, inner_tx).await });
            tokio::spawn(async move {
                while let Some(evt) = inner_rx.recv().await {
                    if evt_tx.send(evt).await.is_err() {
                        break;
                    }
                    wake();
                }
            });
        }
    }
    (cmd_tx, evt_rx)
}

fn build_ws_uri(base: &url::Url) -> Result<WsUri, String> {
    let mut url = base.clone();
    let scheme = match url.scheme() {
        "https" | "wss" => "wss",
        "http" | "ws" => "ws",
        other => return Err(format!("unsupported scheme: {other}")),
    };
    url.set_scheme(scheme).map_err(|_| "set_scheme failed".to_owned())?;
    url.path_segments_mut()
        .map_err(|_| "cannot-be-a-base url".to_owned())?
        .pop_if_empty()
        .extend(["api", "ui-protocol", "ws"]);
    url.as_str().parse::<WsUri>().map_err(|e| format!("uri parse: {e}"))
}

fn build_request(
    base: &url::Url,
    token: &SecretString,
    profile: &ProfileId,
    requested_capabilities: &Capabilities,
) -> Result<WsRequest, String> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let uri = build_ws_uri(base)?;
    let mut req = uri.into_client_request().map_err(|e| format!("into_client_request: {e}"))?;
    let h = req.headers_mut();
    h.insert(
        "authorization",
        format!("Bearer {}", token.expose())
            .parse()
            .map_err(|e| format!("auth header: {e}"))?,
    );
    h.insert(
        "x-profile-id",
        profile.0.parse().map_err(|e| format!("profile header: {e}"))?,
    );
    if let Some(features) = requested_capabilities.handshake_header_value() {
        h.insert(
            "x-octos-ui-features",
            features
                .parse()
                .map_err(|e| format!("ui features header: {e}"))?,
        );
    }
    Ok(req)
}

async fn run_state_machine(
    cfg: TransportConfig,
    mut commands: mpsc::Receiver<OutboundCommand>,
    events: mpsc::Sender<TransportEvent>,
) {
    let persist = cfg.cursor_file.clone().map(|p| {
        std::sync::Arc::new(crate::cursor::FileCursorPersist::new(p))
            as std::sync::Arc<dyn crate::cursor::CursorPersist>
    });
    let mut shared = SharedState::new(cfg.cursor.clone(), persist);
    let mut attempt: u32 = 0;
    let mut total_wait = Duration::ZERO;

    log::info!("ws: state machine up (base_url={})", cfg.base_url);
    try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Idle));

    loop {
        log::info!("ws: dialing");
        try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Dialing));
        let req = match build_request(
            &cfg.base_url,
            &cfg.bearer,
            &cfg.profile_id,
            &cfg.requested_capabilities,
        ) {
            Ok(r) => r,
            Err(e) => {
                log::error!("ws: bad upgrade request: {e}");
                try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Failed));
                break;
            }
        };
        let socket = match tokio_tungstenite::connect_async(req).await {
            Ok((s, _)) => s,
            Err(e) => {
                log::warn!("ws: connect failed: {e}");
                if !run_reconnect(&events, &mut attempt, &mut total_wait).await {
                    break;
                }
                continue;
            }
        };
        attempt = 0;
        total_wait = Duration::ZERO;
        log::info!("ws: connected; handshaking");
        try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Handshaking));
        match run_live(socket, &mut shared, &mut commands, &events).await {
            LiveExit::Disconnect => break,
            LiveExit::Reconnect => {
                if !run_reconnect(&events, &mut attempt, &mut total_wait).await {
                    break;
                }
            }
        }
    }
    shared.registry.cancel_all();
    shared.pending.clear();
    while commands.try_recv().is_ok() {}
}

enum LiveExit {
    Disconnect,
    Reconnect,
}

async fn run_live(
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    shared: &mut SharedState,
    commands: &mut mpsc::Receiver<OutboundCommand>,
    events: &mpsc::Sender<TransportEvent>,
) -> LiveExit {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let mut state = ConnectionState::Handshaking;

    let mut hb = interval(HEARTBEAT_INTERVAL);
    hb.set_missed_tick_behavior(MissedTickBehavior::Delay);
    hb.tick().await; // skip immediate first tick

    loop {
        tokio::select! {
            biased;
            cmd = commands.recv() => {
                let Some(cmd) = cmd else {
                    let _ = ws_tx.send(WsMessage::Close(None)).await;
                    return LiveExit::Disconnect;
                };
                match handle_command(cmd, &mut ws_tx, shared).await {
                    CommandOutcome::Continue => {}
                    CommandOutcome::Disconnect => {
                        let _ = ws_tx.send(WsMessage::Close(None)).await;
                        return LiveExit::Disconnect;
                    }
                    CommandOutcome::SocketError => return LiveExit::Reconnect,
                }
            }
            frame = ws_rx.next() => {
                let Some(frame) = frame else {
                    log::info!("ws: stream ended");
                    return LiveExit::Reconnect;
                };
                match frame {
                    Ok(WsMessage::Text(text)) => {
                        if let Some(t) = handle_inbound_text(&text, shared, events, &mut state).await {
                            try_emit(events, TransportEvent::ConnectionState(t));
                        }
                    }
                    Ok(WsMessage::Binary(_)) => log::warn!("ws: unexpected binary; ignoring"),
                    Ok(WsMessage::Ping(p)) => {
                        if ws_tx.send(WsMessage::Pong(p)).await.is_err() {
                            return LiveExit::Reconnect;
                        }
                    }
                    Ok(WsMessage::Pong(_)) | Ok(WsMessage::Frame(_)) => {}
                    Ok(WsMessage::Close(_)) => return LiveExit::Reconnect,
                    Err(e) => {
                        log::warn!("ws: read error: {e}");
                        return LiveExit::Reconnect;
                    }
                }
            }
            _ = hb.tick() => {
                if ws_tx.send(WsMessage::Ping(Vec::new())).await.is_err() {
                    return LiveExit::Reconnect;
                }
            }
        }
    }
}

enum CommandOutcome {
    Continue,
    Disconnect,
    SocketError,
}

async fn handle_command<S>(
    cmd: OutboundCommand,
    ws_tx: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<S>,
        WsMessage,
    >,
    shared: &mut SharedState,
) -> CommandOutcome
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    match build_outbound(cmd, shared) {
        Outbound::Disconnect => CommandOutcome::Disconnect,
        Outbound::Skip => CommandOutcome::Continue,
        Outbound::Send { id, frame, pending } => {
            if ws_tx.send(WsMessage::Text(frame)).await.is_err() {
                return CommandOutcome::SocketError;
            }
            if let Some(p) = pending {
                shared.pending.insert(id, p);
            }
            CommandOutcome::Continue
        }
    }
}

/// Sleep on backoff. Returns `true` to retry, `false` if the cumulative
/// budget is exhausted (caller transitions to `Failed`).
async fn run_reconnect(
    events: &mpsc::Sender<TransportEvent>,
    attempt: &mut u32,
    total_wait: &mut Duration,
) -> bool {
    *attempt = attempt.saturating_add(1);
    let delay = next_backoff(*attempt);
    *total_wait += delay;
    if *total_wait > RECONNECT_BUDGET {
        log::error!("ws: reconnect budget exhausted ({:.1?})", *total_wait);
        try_emit(events, TransportEvent::ConnectionState(ConnectionState::Failed));
        return false;
    }
    try_emit(
        events,
        TransportEvent::ConnectionState(ConnectionState::Reconnecting { attempt: *attempt }),
    );
    let start = Instant::now();
    tokio::time::sleep(delay).await;
    log::debug!("ws: reconnect waited {:.2?} (#{}.)", start.elapsed(), *attempt);
    true
}

/// Full-jitter exponential backoff (W01 § Reconnect algorithm).
pub fn next_backoff(attempt: u32) -> Duration {
    if attempt == 0 {
        return Duration::ZERO;
    }
    let exp = attempt.saturating_sub(1).min(5);
    Duration::from_secs(pseudo_jitter_secs(1u64 << exp)).min(RECONNECT_DELAY_MAX)
}

fn pseudo_jitter_secs(max_exclusive: u64) -> u64 {
    if max_exclusive <= 1 {
        return 0;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_nanos() as u64) % max_exclusive)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_attempt_zero_is_immediate() {
        assert_eq!(next_backoff(0), Duration::ZERO);
    }

    #[test]
    fn backoff_is_capped_at_thirty_seconds() {
        for attempt in 1..=20 {
            assert!(next_backoff(attempt) <= RECONNECT_DELAY_MAX);
        }
    }

    #[test]
    fn build_ws_uri_swaps_scheme() {
        let base = url::Url::parse("https://example.test").unwrap();
        let uri = build_ws_uri(&base).unwrap();
        assert!(uri.to_string().starts_with("wss://"));
        assert!(uri.path().ends_with("/api/ui-protocol/ws"));
    }

    #[test]
    fn build_request_sends_requested_capability_header() {
        let base = url::Url::parse("https://example.test").unwrap();
        let req = build_request(
            &base,
            &SecretString::new("tk"),
            &ProfileId::new("p1"),
            &Capabilities::requested(),
        )
        .unwrap();
        assert_eq!(
            req.headers()
                .get("x-octos-ui-features")
                .and_then(|v| v.to_str().ok()),
            Some("approval.typed.v1, pane.snapshots.v1, session.workspace_cwd.v1, context.lifecycle.v1, state.session_hydrate.v1, auxiliary.rest_to_ws.v1")
        );
    }
}
