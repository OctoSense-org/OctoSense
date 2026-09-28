//! The seam with the octos#2567 relay (the shell's host connection, which
//! receives every `peer/tool/call` and octos approval, ADR 0004 §5 and §8).
//!
//! - **In:** the relay calls [`ApprovalIntake::approval_requested`] (the
//!   router implements it; [`super::approval_requested`] is the global) for
//!   every call that needs an approval, with the exact arguments, the
//!   stamped caller and its context.
//! - **Out:** the router answers each request exactly once through
//!   [`ApprovalRelay::approval_decided`], which the relay implements and
//!   installs with [`super::set_relay`]. Until one is installed the
//!   decisions wait in a [`RecordingRelay`] and are handed over on install,
//!   so none is lost.
//!
//! [`RecordingRelay`] is also the test double.

use super::types::{Caller, Decision, RequestContext, RequestId, ToolSpec};
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// Where the router sends each decision. Called once per request.
pub trait ApprovalRelay: Send {
    fn approval_decided(&mut self, id: &RequestId, decision: Decision, reason: &str);
}

/// What the router offers the relay.
pub trait ApprovalIntake {
    /// A call needs an approval. `context.call_id` is the relay's id; the
    /// decision comes back through [`ApprovalRelay::approval_decided`],
    /// possibly before this returns (developer mode, a standing rule).
    fn approval_requested(
        &mut self,
        app: &str,
        tool: ToolSpec,
        args: Value,
        caller: Caller,
        context: RequestContext,
    ) -> super::router::Route;
}

/// The test double: records every decision, shareable with the test.
#[derive(Clone, Default)]
pub struct RecordingRelay {
    pub decided: Arc<Mutex<Vec<(RequestId, Decision, String)>>>,
}

impl RecordingRelay {
    pub fn take(&self) -> Vec<(RequestId, Decision, String)> {
        std::mem::take(&mut *self.decided.lock().unwrap())
    }
    pub fn last(&self) -> Option<(RequestId, Decision, String)> {
        self.decided.lock().unwrap().last().cloned()
    }
}

impl ApprovalRelay for RecordingRelay {
    fn approval_decided(&mut self, id: &RequestId, decision: Decision, reason: &str) {
        self.decided.lock().unwrap().push((id.clone(), decision, reason.to_string()));
    }
}
