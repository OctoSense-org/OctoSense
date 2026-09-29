//! The system chat's link to the shell's ONE kernel: a consumer
//! connection from `octosense_kernel::connect()` (through
//! `octosense_ai_host::kernel`), the same host connection every native
//! consumer shares: stdio frames, or the host-token WebSocket while Talk to
//! Octos is on. No second kernel, no external token.
//!
//! The kernel's `Connection::recv` is async; the chat's thread polls it
//! with a waker that unparks the thread, so the shell needs no runtime of
//! its own.

use super::session::{Closed, Connector, Link, Recv, Unavailable};
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant};

use octosense_ai_host::kernel as core;

struct Unpark(std::thread::Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

/// Poll `fut` on this thread until it is ready or `wait` passes. Dropping
/// the future on timeout is safe for `Connection::recv` (a channel receive).
fn poll_for<F: Future>(fut: F, wait: Duration) -> Option<F::Output> {
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut fut = pin!(fut);
    let deadline = Instant::now() + wait;
    loop {
        if let Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
            return Some(out);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        std::thread::park_timeout(left);
    }
}

struct KernelLink(core::Connection);

impl Link for KernelLink {
    fn send(&mut self, frame: String) -> Result<(), Closed> {
        self.0.send(frame).map_err(closed)
    }
    fn recv(&mut self, wait: Duration) -> Recv {
        match poll_for(self.0.recv(), wait) {
            None => Recv::Idle,
            Some(Ok(frame)) => Recv::Frame(frame),
            Some(Err(reason)) => Recv::Closed(closed(reason)),
        }
    }
}

fn closed(reason: core::CloseReason) -> Closed {
    Closed { restarted: reason == core::CloseReason::Restarted, why: reason.to_string() }
}

/// A link over one kernel connection (tests use a kernel of their own).
pub(crate) fn link(conn: core::Connection) -> Box<dyn Link> {
    Box::new(KernelLink(conn))
}

/// The shell's kernel.
pub struct KernelConnector;

impl Connector for KernelConnector {
    fn connect(&mut self) -> Result<Box<dyn Link>, Unavailable> {
        if let Err(why) = core::launch() {
            return Err(Unavailable::NoKernel(why.to_string()));
        }
        if !provider_configured(core::profile().as_deref()) {
            return Err(Unavailable::NoProvider);
        }
        match core::connect() {
            Ok(conn) => Ok(Box::new(KernelLink(conn))),
            Err(why) => Err(Unavailable::Failed(why.to_string())),
        }
    }
}

/// Whether the kernel's profile names a model provider (the AI providers
/// app writes `config.llm.primary`). Nothing is started without one.
pub fn provider_configured(profile: Option<&std::path::Path>) -> bool {
    let Some(path) = profile else { return false };
    let Ok(bytes) = std::fs::read(path) else { return false };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return false };
    v["config"]["llm"]["primary"].is_object()
}
