//! Host-registered peer tools (octos UPCR-2026-035, octos#2567), the host
//! side. Feature `peer-tools`.
//!
//! With the feature, a [`crate::broker::Broker`] follows the registration
//! rules of OctoSense issue #62 and the UPCR's host obligations:
//!
//! 1. **Register on every prepare and reconnect.** Right after every
//!    successful `peer/prepare`, on the same link that later sends
//!    `peer/context/open` and `turn/start`, the broker calls
//!    `peer/tools/register` with the app's set from its [`HostTools`], or an
//!    **empty set** when it has none (Rinx's case today). A reconnect drops
//!    the bound peer, so the next request prepares and registers again
//!    before any turn.
//! 2. **No turns without registration.** When registering fails, the peer is
//!    not bound: no context opens and no turn starts, and the service reports
//!    `Availability::Failed` with the reason. It never falls back to
//!    memory-less turns.
//! 3. **One link per broker**: every peer registers on the link its turns
//!    use.
//!
//! The kernel then sends `peer/tool/call` for a registered tool to this
//! link. The broker:
//!
//! - refuses a call whose peer is not its bound peer (ignored: it cannot
//!   authenticate an answer) or whose tool is not in the set it registered
//!   (`not_registered`), whatever the kernel sent;
//! - runs a call at most once per `(session_id, turn_id, tool_call_id,
//!   args_digest)`, answering a repeat with the first result;
//! - never runs a call after `peer/tool/cancel` or after its `timeout_ms`:
//!   the call's future is dropped and its [`Cancel`] fires, and no result is
//!   sent; a release or a lost link cancels every call in flight;
//! - answers with `peer/tool/result` (`ok` with `data`, or `error: {kind,
//!   message}` with `kind` in `[a-z0-9_]{1,32}`).
//!
//! Without the feature none of this exists and the broker behaves exactly
//! as before.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::Notify;

/// Host → kernel: declare the peer's tools.
pub const REGISTER: &str = "peer/tools/register";
/// Kernel → host: run one call.
pub const CALL: &str = "peer/tool/call";
/// Kernel → host: stop a call.
pub const CANCEL: &str = "peer/tool/cancel";
/// Host → kernel: one call's answer.
pub const RESULT: &str = "peer/tool/result";

/// What the broker registers for its peer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Registration {
    /// `tools.json` entries: `{name, description, input_schema, risk,
    /// background?, outward?, confirm?, …}`.
    pub tools: Vec<Value>,
    /// Kernel tools from octos's allowlist the app may use.
    pub generic_tools: Vec<String>,
    /// How long the kernel waits for one call (1–300 000 ms).
    pub call_timeout_ms: Option<u64>,
    /// Largest serialized result the kernel accepts (up to 1 MiB).
    pub max_result_bytes: Option<u64>,
}

impl Registration {
    /// The empty set: no tools at all.
    pub fn empty() -> Self {
        Self::default()
    }

    /// The registered tool names.
    pub fn names(&self) -> BTreeSet<String> {
        self.tools
            .iter()
            .filter_map(|t| t["name"].as_str().map(str::to_owned))
            .collect()
    }

    /// `peer/tools/register` params for `peer`, authorized by its host token.
    pub fn params(
        &self,
        profile: &str,
        originator: &str,
        peer: &str,
        token: Option<&str>,
    ) -> Value {
        let mut params = json!({
            "profile_id": profile,
            "session_id": originator,
            "peer": peer,
            "host_token": token,
            "tools": self.tools,
            "generic_tools": self.generic_tools,
        });
        if let Some(ms) = self.call_timeout_ms {
            params["call_timeout_ms"] = json!(ms);
        }
        if let Some(bytes) = self.max_result_bytes {
            params["max_result_bytes"] = json!(bytes);
        }
        params
    }
}

/// One `peer/tool/call`.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub peer: String,
    /// The calling session: the peer's own, or one of its request contexts.
    pub session_id: String,
    pub context_id: Option<String>,
    pub turn_id: String,
    pub call_id: String,
    pub tool_call_id: String,
    pub args_digest: String,
    /// The declared name (`toolbox.search`).
    pub name: String,
    pub args: Value,
    pub risk: String,
    pub confirm_required: bool,
    /// How long the kernel waits for the answer.
    pub timeout_ms: u64,
    pub tools_version: u64,
}

impl ToolCall {
    /// Parses the notification's params.
    pub fn from_params(params: &Value) -> Result<ToolCall, String> {
        let text = |key: &str| -> Result<String, String> {
            params[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("peer/tool/call without {key}"))
        };
        Ok(ToolCall {
            peer: text("peer")?,
            session_id: text("session_id")?,
            context_id: params["context_id"].as_str().map(str::to_owned),
            turn_id: params["turn_id"].as_str().unwrap_or_default().to_owned(),
            call_id: text("call_id")?,
            tool_call_id: params["tool_call_id"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            args_digest: params["args_digest"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            name: text("name")?,
            args: params.get("args").cloned().unwrap_or(Value::Null),
            risk: params["risk"].as_str().unwrap_or_default().to_owned(),
            confirm_required: params["confirm_required"].as_bool().unwrap_or(false),
            timeout_ms: params["timeout_ms"].as_u64().unwrap_or(30_000),
            tools_version: params["tools_version"].as_u64().unwrap_or(0),
        })
    }

    /// The call's occurrence: a host runs each at most once.
    pub fn occurrence(&self) -> Occurrence {
        (
            self.session_id.clone(),
            self.turn_id.clone(),
            self.tool_call_id.clone(),
            self.args_digest.clone(),
        )
    }
}

/// `(session_id, turn_id, tool_call_id, args_digest)`.
pub type Occurrence = (String, String, String, String);

/// A call's error, as the kernel accepts it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolError {
    /// `[a-z0-9_]{1,32}`; anything else becomes `error`.
    pub kind: String,
    pub message: String,
}

impl ToolError {
    pub fn new(kind: &str, message: impl Into<String>) -> Self {
        let valid = !kind.is_empty()
            && kind.len() <= 32
            && kind
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        let mut message: String = message.into();
        if message.len() > 4096 {
            let mut end = 4096;
            while !message.is_char_boundary(end) {
                end -= 1;
            }
            message.truncate(end);
        }
        Self {
            kind: if valid {
                kind.to_owned()
            } else {
                "error".into()
            },
            message,
        }
    }
}

/// A call's cancellation: fired by `peer/tool/cancel`, the call's deadline,
/// a release or a lost link. A [`HostTools`] implementation may watch it to
/// stop early; the broker drops the call's future either way.
#[derive(Clone, Default)]
pub struct Cancel(Arc<CancelInner>);

#[derive(Default)]
struct CancelInner {
    fired: AtomicBool,
    notify: Notify,
}

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.fired.store(true, Ordering::Release);
        self.0.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.fired.load(Ordering::Acquire)
    }

    /// Resolves once cancelled.
    pub async fn cancelled(&self) {
        loop {
            let notified = self.0.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

/// A future a [`HostTools`] call returns.
pub type ToolFuture = Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + 'static>>;

/// What a host offers one app's peer and runs for it.
pub trait HostTools: Send + Sync {
    /// The set to register (asked again at every registration).
    fn registration(&self) -> Registration;
    /// Runs one call of a registered tool. `cancel` fires when the kernel no
    /// longer waits; the broker then drops the future.
    fn call(&self, call: ToolCall, cancel: Cancel) -> ToolFuture;
}

impl std::fmt::Debug for dyn HostTools {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostTools")
    }
}

/// Results kept for answering repeats of a finished occurrence.
const DONE_KEPT: usize = 256;

enum Run {
    /// Running; the call ids waiting for it.
    Running(Vec<String>),
    Done(Result<Value, ToolError>),
}

/// The broker's calls in flight and recent results.
#[derive(Default)]
pub(crate) struct Calls {
    /// The registered names, once registered on the current link.
    pub(crate) registered: BTreeSet<String>,
    /// Call id → (occurrence, its cancel).
    by_call: HashMap<String, (Occurrence, Cancel)>,
    runs: HashMap<Occurrence, Run>,
    done: VecDeque<Occurrence>,
}

/// What to do with a new call.
pub(crate) enum Admit {
    /// Start it: this call's cancel.
    Start(Cancel),
    /// The same occurrence is running; this call waits for it.
    Wait,
    /// The same occurrence finished: answer with its result.
    Answer(Result<Value, ToolError>),
}

impl Calls {
    pub(crate) fn admit(&mut self, call: &ToolCall) -> Admit {
        let occurrence = call.occurrence();
        match self.runs.get_mut(&occurrence) {
            Some(Run::Done(result)) => Admit::Answer(result.clone()),
            Some(Run::Running(waiters)) => {
                waiters.push(call.call_id.clone());
                let cancel = self
                    .by_call
                    .values()
                    .find(|(o, _)| *o == occurrence)
                    .map(|(_, c)| c.clone())
                    .unwrap_or_default();
                self.by_call
                    .insert(call.call_id.clone(), (occurrence, cancel));
                Admit::Wait
            }
            None => {
                let cancel = Cancel::new();
                self.runs
                    .insert(occurrence.clone(), Run::Running(vec![call.call_id.clone()]));
                self.by_call
                    .insert(call.call_id.clone(), (occurrence, cancel.clone()));
                Admit::Start(cancel)
            }
        }
    }

    /// `peer/tool/cancel` for `call_id`: it is not answered; the run stops
    /// when nobody else waits for it.
    pub(crate) fn cancel(&mut self, call_id: &str) {
        let Some((occurrence, cancel)) = self.by_call.remove(call_id) else {
            return;
        };
        if let Some(Run::Running(waiters)) = self.runs.get_mut(&occurrence) {
            waiters.retain(|w| w != call_id);
            if waiters.is_empty() {
                self.runs.remove(&occurrence);
                cancel.cancel();
            }
        }
    }

    /// Cancels everything in flight (release, lost link).
    pub(crate) fn cancel_all(&mut self) {
        for (_, (_, cancel)) in self.by_call.drain() {
            cancel.cancel();
        }
        self.runs.retain(|_, run| matches!(run, Run::Done(_)));
    }

    /// A run ended: the call ids to answer (none when it was cancelled).
    pub(crate) fn finish(
        &mut self,
        occurrence: &Occurrence,
        result: &Result<Value, ToolError>,
    ) -> Vec<String> {
        let Some(Run::Running(waiters)) = self.runs.remove(occurrence) else {
            return Vec::new();
        };
        for id in &waiters {
            self.by_call.remove(id);
        }
        self.runs
            .insert(occurrence.clone(), Run::Done(result.clone()));
        self.done.push_back(occurrence.clone());
        while self.done.len() > DONE_KEPT {
            if let Some(old) = self.done.pop_front() {
                self.runs.remove(&old);
            }
        }
        waiters
    }

    /// A run ended without a result (cancelled or timed out): forget it, so
    /// a later dispatch of a `read` may run again.
    pub(crate) fn abandon(&mut self, occurrence: &Occurrence) {
        if let Some(Run::Running(waiters)) = self.runs.remove(occurrence) {
            for id in waiters {
                self.by_call.remove(&id);
            }
        }
    }
}

/// `peer/tool/result` params for one answer.
pub(crate) fn result_params(
    profile: &str,
    originator: &str,
    peer: &str,
    token: Option<&str>,
    call_id: &str,
    result: &Result<Value, ToolError>,
) -> Value {
    let mut params = json!({
        "profile_id": profile,
        "session_id": originator,
        "peer": peer,
        "host_token": token,
        "call_id": call_id,
    });
    match result {
        Ok(data) => {
            params["ok"] = json!(true);
            params["data"] = data.clone();
        }
        Err(error) => {
            params["ok"] = json!(false);
            params["error"] = json!({"kind": error.kind, "message": error.message});
        }
    }
    params
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(call_id: &str, tool_call_id: &str) -> ToolCall {
        ToolCall::from_params(&json!({
            "peer": "news-1", "session_id": "s", "turn_id": "t", "call_id": call_id,
            "tool_call_id": tool_call_id, "args_digest": "d", "name": "toolbox.search",
            "args": {}, "risk": "read", "confirm_required": false, "timeout_ms": 1000, "tools_version": 1
        }))
        .unwrap()
    }

    #[test]
    fn an_occurrence_runs_once_and_repeats_get_its_result() {
        let mut calls = Calls::default();
        let a = call("c1", "tc1");
        assert!(matches!(calls.admit(&a), Admit::Start(_)));
        assert!(matches!(calls.admit(&call("c2", "tc1")), Admit::Wait));
        let waiters = calls.finish(&a.occurrence(), &Ok(json!(1)));
        assert_eq!(waiters, ["c1", "c2"]);
        assert!(matches!(
            calls.admit(&call("c3", "tc1")),
            Admit::Answer(Ok(_))
        ));
        assert!(matches!(calls.admit(&call("c4", "tc2")), Admit::Start(_)));
    }

    #[test]
    fn a_cancelled_call_stops_only_when_nobody_waits() {
        let mut calls = Calls::default();
        let a = call("c1", "tc1");
        let Admit::Start(cancel) = calls.admit(&a) else {
            panic!()
        };
        calls.admit(&call("c2", "tc1"));
        calls.cancel("c1");
        assert!(!cancel.is_cancelled());
        calls.cancel("c2");
        assert!(cancel.is_cancelled());
        assert!(calls.finish(&a.occurrence(), &Ok(json!(1))).is_empty());
    }

    #[test]
    fn error_kinds_are_what_the_kernel_accepts() {
        assert_eq!(ToolError::new("not_granted", "x").kind, "not_granted");
        assert_eq!(ToolError::new("Bad-Kind", "x").kind, "error");
        assert_eq!(ToolError::new("", "x").kind, "error");
        assert!(ToolError::new("x", "é".repeat(3000)).message.len() <= 4096);
    }

    #[test]
    fn the_empty_registration_registers_no_tools() {
        let params = Registration::empty().params("_main", "o", "p", Some("t"));
        assert_eq!(params["tools"], json!([]));
        assert_eq!(params["generic_tools"], json!([]));
        assert!(params.get("call_timeout_ms").is_none());
    }
}
