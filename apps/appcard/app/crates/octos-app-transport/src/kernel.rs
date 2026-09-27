//! Transport over the shell's octos kernel (`octosense-octos-core`).
//!
//! The kernel is a shell service: one per process, started on the first
//! `octosense_octos_core::connect()` (`octos serve --stdio` as a child on
//! desktop and Android, the canonical core in-process on OpenHarmony) and
//! shared with every other consumer. This task holds one connection to it
//! and speaks the same NDJSON JSON-RPC frames the stdio transport used to
//! write to its own child; framing and reduction are `crate::proto`'s.
//!
//! Lifecycle: `Idle → Dialing → Handshaking (awaiting session/open) → Live`.
//! When the shell restarts the kernel (the AI providers app changed the
//! providers), the connection closes with `Restarted`: requests still
//! waiting fail with a clear error, the task reconnects (which starts the new
//! kernel), emits `Reconnecting`, and opens every session it had opened
//! again from its replay cursor, so the app carries on without rebuilding
//! its agent. Any other end (the kernel crashed, could not start) is
//! `Failed`, as the stdio child's death was. A clean shutdown (command
//! channel closed / `Disconnect`) drops the connection; the kernel stops
//! when its last consumer leaves.

use std::sync::Arc;

use octos_core::ui_protocol::RpcError;
use octosense_octos_core::CloseReason;
use tokio::sync::mpsc;

use crate::proto::{
    build_outbound, build_reopens, fail_all_pending, handle_inbound_text, try_emit, Outbound,
    SharedState, CHANNEL_BUFFER,
};
use crate::{ConnectionState, OutboundCommand, TransportConfig, TransportEvent};

/// JSON-RPC error code for requests the restarted kernel never answered.
pub const KERNEL_RESTARTED: i64 = -32099;

pub fn spawn(cfg: TransportConfig) -> (mpsc::Sender<OutboundCommand>, mpsc::Receiver<TransportEvent>) {
    spawn_with_waker(cfg, None)
}

/// Like [`spawn`], but invokes `waker` after every event is queued so a
/// UI-thread consumer that only drains on UI events can be woken.
pub fn spawn_with_waker(
    cfg: TransportConfig,
    waker: Option<Arc<dyn Fn() + Send + Sync>>,
) -> (mpsc::Sender<OutboundCommand>, mpsc::Receiver<TransportEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<OutboundCommand>(CHANNEL_BUFFER);
    let (evt_tx, evt_rx) = mpsc::channel::<TransportEvent>(CHANNEL_BUFFER);
    match waker {
        None => {
            tokio::spawn(run(cfg, cmd_rx, evt_tx));
        }
        Some(wake) => {
            let (inner_tx, mut inner_rx) = mpsc::channel::<TransportEvent>(CHANNEL_BUFFER);
            tokio::spawn(run(cfg, cmd_rx, inner_tx));
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

enum Exit {
    /// The command channel closed or `Disconnect`: stop quietly.
    Clean,
    /// The connection ended.
    Closed(CloseReason),
}

async fn run(
    cfg: TransportConfig,
    mut commands: mpsc::Receiver<OutboundCommand>,
    events: mpsc::Sender<TransportEvent>,
) {
    try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Idle));
    try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Dialing));
    let persist = cfg.cursor_file.clone().map(|p| {
        Arc::new(crate::cursor::FileCursorPersist::new(p)) as Arc<dyn crate::cursor::CursorPersist>
    });
    let mut shared = SharedState::new(cfg.cursor.clone(), persist);
    let mut attempt: u32 = 0;
    loop {
        let mut conn = match octosense_octos_core::connect() {
            Ok(conn) => conn,
            Err(e) => {
                log::error!("kernel: {e}");
                try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Failed));
                return;
            }
        };
        log::info!("kernel: connected to octos kernel {}", conn.generation());
        let mut state = ConnectionState::Handshaking;
        try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Handshaking));
        // After a restart: the sessions this connection had, again.
        let mut exit = None;
        if attempt > 0 {
            for (id, frame, pending) in build_reopens(&mut shared) {
                if let Err(reason) = conn.send(frame) {
                    exit = Some(Exit::Closed(reason));
                    break;
                }
                shared.pending.insert(id, pending);
            }
        }
        let exit = match exit {
            Some(exit) => exit,
            None => loop {
                tokio::select! {
                    biased;
                    cmd = commands.recv() => {
                        let Some(cmd) = cmd else { break Exit::Clean };
                        match build_outbound(cmd, &mut shared) {
                            Outbound::Disconnect => break Exit::Clean,
                            Outbound::Skip => {}
                            Outbound::Send { id, frame, pending } => {
                                if let Err(reason) = conn.send(frame) {
                                    break Exit::Closed(reason);
                                }
                                if let Some(p) = pending {
                                    shared.pending.insert(id, p);
                                }
                            }
                        }
                    }
                    frame = conn.recv() => match frame {
                        Ok(text) => {
                            if text.trim().is_empty() {
                                continue;
                            }
                            if let Some(t) = handle_inbound_text(&text, &mut shared, &events, &mut state).await {
                                try_emit(&events, TransportEvent::ConnectionState(t));
                            }
                        }
                        Err(reason) => break Exit::Closed(reason),
                    },
                }
            },
        };
        match exit {
            Exit::Clean => {
                log::info!("kernel: disconnecting");
                break;
            }
            Exit::Closed(CloseReason::Restarted) => {
                log::info!("kernel: the octos kernel restarted; reconnecting");
                fail_all_pending(
                    &mut shared,
                    &events,
                    RpcError::new(KERNEL_RESTARTED, "the octos kernel restarted (its AI providers changed); try again"),
                );
                attempt += 1;
                try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Reconnecting { attempt }));
            }
            Exit::Closed(reason) => {
                log::warn!("kernel: {reason}");
                shared.registry.cancel_all();
                shared.pending.clear();
                try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Failed));
                break;
            }
        }
    }
    while commands.try_recv().is_ok() {}
}
