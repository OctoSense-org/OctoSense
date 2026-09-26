//! Stdio transport task. Spawns a local `octos` process (`serve --stdio`) and
//! speaks NDJSON JSON-RPC over its stdin/stdout — one JSON-RPC frame per line.
//! There is no TCP socket, no bearer handshake, and no reconnect budget: the
//! child *is* the connection. Command dispatch and inbound frame handling are
//! delegated to `crate::proto` (shared with the WebSocket transport); this
//! module only owns the child process and the pipe framing.
//!
//! Lifecycle vs. `ws`: `Idle → Dialing (spawning) → Handshaking (spawned,
//! awaiting `session/open` reply) → Live`. If the child dies, exits, or its
//! pipes break, the transport emits `Failed` (no auto-restart yet — the app
//! surfaces a retry). A clean shutdown (command channel closed / `Disconnect`)
//! kills the child without emitting `Failed`.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::mpsc;

use crate::proto::{
    build_outbound, handle_inbound_text, try_emit, Outbound, SharedState, CHANNEL_BUFFER,
};
use crate::{ConnectionState, OutboundCommand, TransportConfig, TransportEvent};

pub fn spawn(
    cfg: TransportConfig,
) -> (mpsc::Sender<OutboundCommand>, mpsc::Receiver<TransportEvent>) {
    spawn_with_waker(cfg, None)
}

/// Like [`spawn`], but invokes `waker` after every event is queued so a
/// UI-thread consumer that only drains on UI events can be woken (mirrors
/// `ws::spawn_with_waker`; see that doc for the idle-drain rationale).
pub fn spawn_with_waker(
    cfg: TransportConfig,
    waker: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
) -> (mpsc::Sender<OutboundCommand>, mpsc::Receiver<TransportEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<OutboundCommand>(CHANNEL_BUFFER);
    let (evt_tx, evt_rx) = mpsc::channel::<TransportEvent>(CHANNEL_BUFFER);
    match waker {
        None => {
            tokio::spawn(async move { run_child(cfg, cmd_rx, evt_tx).await });
        }
        Some(wake) => {
            let (inner_tx, mut inner_rx) = mpsc::channel::<TransportEvent>(CHANNEL_BUFFER);
            tokio::spawn(async move { run_child(cfg, cmd_rx, inner_tx).await });
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

async fn write_frame(stdin: &mut ChildStdin, frame: &str) -> std::io::Result<()> {
    stdin.write_all(frame.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await
}

async fn run_child(
    cfg: TransportConfig,
    mut commands: mpsc::Receiver<OutboundCommand>,
    events: mpsc::Sender<TransportEvent>,
) {
    try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Idle));

    let Some(spawn) = cfg.stdio.clone() else {
        log::error!("stdio: TransportConfig.stdio is None; cannot spawn octos");
        try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Failed));
        return;
    };

    try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Dialing));

    let mut command = Command::new(&spawn.program);
    command
        .args(&spawn.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for (k, v) in &spawn.env {
        command.env(k, v);
    }
    if let Some(cwd) = &spawn.cwd {
        command.current_dir(cwd);
    }

    log::info!("stdio: spawning {} {:?}", spawn.program.display(), spawn.args);
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            log::error!("stdio: failed to spawn octos ({}): {e}", spawn.program.display());
            try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Failed));
            return;
        }
    };

    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        log::error!("stdio: child stdin/stdout not piped");
        let _ = child.start_kill();
        try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Failed));
        return;
    };
    let mut lines = BufReader::new(stdout).lines();

    // Forward the child's stderr (octos logs there) to our log for diagnosis;
    // it never carries protocol frames, so it stays off the NDJSON path.
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut err = BufReader::new(stderr).lines();
            while let Ok(Some(l)) = err.next_line().await {
                log::info!("octos: {l}");
            }
        });
    }

    let persist = cfg.cursor_file.clone().map(|p| {
        std::sync::Arc::new(crate::cursor::FileCursorPersist::new(p))
            as std::sync::Arc<dyn crate::cursor::CursorPersist>
    });
    let mut shared = SharedState::new(cfg.cursor.clone(), persist);
    let mut state = ConnectionState::Handshaking;
    try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Handshaking));

    // `failed` distinguishes an abnormal exit (child died / pipe broke) from a
    // clean shutdown (command channel closed / `Disconnect`). Only the former
    // surfaces `ConnectionState::Failed`.
    let mut failed = true;
    loop {
        tokio::select! {
            biased;
            cmd = commands.recv() => {
                let Some(cmd) = cmd else {
                    log::info!("stdio: command channel closed; stopping octos");
                    failed = false;
                    break;
                };
                match build_outbound(cmd, &mut shared) {
                    Outbound::Disconnect => {
                        log::info!("stdio: disconnect requested");
                        failed = false;
                        break;
                    }
                    Outbound::Skip => {}
                    Outbound::Send { id, frame, pending } => {
                        if let Err(e) = write_frame(&mut stdin, &frame).await {
                            log::warn!("stdio: write to octos stdin failed: {e}");
                            break;
                        }
                        if let Some(p) = pending {
                            shared.pending.insert(id, p);
                        }
                    }
                }
            }
            line = lines.next_line() => {
                match line {
                    Ok(Some(text)) => {
                        if text.trim().is_empty() {
                            continue;
                        }
                        if let Some(t) =
                            handle_inbound_text(&text, &mut shared, &events, &mut state).await
                        {
                            try_emit(&events, TransportEvent::ConnectionState(t));
                        }
                    }
                    Ok(None) => {
                        log::info!("stdio: octos stdout closed (EOF)");
                        break;
                    }
                    Err(e) => {
                        log::warn!("stdio: read from octos stdout failed: {e}");
                        break;
                    }
                }
            }
            status = child.wait() => {
                log::warn!("stdio: octos process exited: {status:?}");
                break;
            }
        }
    }

    shared.registry.cancel_all();
    shared.pending.clear();
    let _ = child.start_kill();
    if failed {
        try_emit(&events, TransportEvent::ConnectionState(ConnectionState::Failed));
    }
    while commands.try_recv().is_ok() {}
}
