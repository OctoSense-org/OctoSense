//! A scoped consumer the kernel's own runtime serves: an app's kernel port
//! (ADR 0003, "An app that is an octos client"), without the shell needing
//! an async runtime of its own.
//!
//! Frames from the app go to the kernel through the router, which holds
//! them to the consumer's [`Scope`]; the kernel's frames for it go to its
//! `deliver` callback. A kernel restart is a [`PortEvent::Reset`]: the
//! connection is made again behind the port and the app opens its sessions
//! again. Any other end of the kernel is [`PortEvent::Closed`], as is the
//! app dropping its end. Dropping the [`PortHandle`] ends it too.

use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

use crate::{CloseReason, Core, Scope};

/// What a port's consumer is told.
#[derive(Clone, Debug, PartialEq)]
pub enum PortEvent {
    /// A kernel frame for it: a reply, a notification or a kernel request.
    Frame(String),
    /// The kernel restarted; the port goes on with the new one.
    Reset(String),
    /// The port ended.
    Closed(String),
}

/// Delivers [`PortEvent`]s; `false` when the consumer is gone.
pub type Deliver = Box<dyn Fn(PortEvent) -> bool + Send + Sync>;

/// A served port. Dropping it ends the port's connection.
pub struct PortHandle {
    _stop: oneshot::Sender<()>,
}

impl Core {
    /// Serve a scoped consumer: its frames arrive on `up` (one JSON-RPC
    /// frame per message; the port ends when the sender is dropped), and
    /// what the kernel says to it goes to `deliver`.
    pub fn serve_scoped(&self, label: &str, scope: Arc<dyn Scope>, up: std::sync::mpsc::Receiver<String>, deliver: Deliver) -> PortHandle {
        let (stop, stopped) = oneshot::channel();
        let (up_tx, up_rx) = mpsc::unbounded_channel();
        let forwarded = std::thread::Builder::new().name(format!("kernel-port {label}")).spawn(move || {
            while let Ok(frame) = up.recv() {
                if up_tx.send(frame).is_err() {
                    break;
                }
            }
        });
        if let Err(e) = forwarded {
            deliver(PortEvent::Closed(format!("the kernel port could not start: {e}")));
            return PortHandle { _stop: stop };
        }
        let core = self.clone();
        let label = label.to_string();
        self.0.runtime().spawn(serve(core, label, scope, up_rx, deliver, stopped));
        PortHandle { _stop: stop }
    }
}

async fn serve(
    core: Core,
    label: String,
    scope: Arc<dyn Scope>,
    mut up: mpsc::UnboundedReceiver<String>,
    deliver: Deliver,
    mut stopped: oneshot::Receiver<()>,
) {
    loop {
        let mut conn = match core.connect_scoped(scope.clone()) {
            Ok(conn) => conn,
            Err(e) => {
                deliver(PortEvent::Closed(format!("no octos kernel here: {e}")));
                return;
            }
        };
        log::info!("octos-core: kernel port {label} on kernel {}", conn.generation());
        let reason = loop {
            tokio::select! {
                biased;
                // The handle was dropped: the app instance closed.
                _ = &mut stopped => return,
                frame = up.recv() => match frame {
                    Some(frame) => {
                        if let Err(reason) = conn.send(frame) {
                            break reason;
                        }
                    }
                    // The app dropped its end.
                    None => return,
                },
                frame = conn.recv() => match frame {
                    Ok(text) => {
                        if !deliver(PortEvent::Frame(text)) {
                            return;
                        }
                    }
                    Err(reason) => break reason,
                },
            }
        };
        if matches!(reason, CloseReason::Restarted) {
            log::info!("octos-core: kernel port {label}: the kernel restarted; connecting again");
            if !deliver(PortEvent::Reset(reason.to_string())) {
                return;
            }
            continue;
        }
        log::info!("octos-core: kernel port {label}: {reason}");
        deliver(PortEvent::Closed(reason.to_string()));
        return;
    }
}
