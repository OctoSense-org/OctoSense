//! The host's half of octos UPCR-2026-035 (host-registered tools per app
//! peer, octos#2567): what the broker needs from the shell, and the handles
//! it gives back. No runtime, no I/O: serde_json only.
//!
//! The kernel never talks to an app. The shell is every app peer's **tool
//! host**: the broker registers the app's tools on the connection that
//! drives the peer's turns (`peer/tools/register`, after every `peer/prepare`
//! and every reconnect, `generic_tools` omitted so the peer keeps its kernel
//! roster), receives each `peer/tool/call` on that connection and hands it
//! to the installed [`ToolHost`] with the identity the host stamps (account,
//! client, calling app). The host authorizes it, routes it to the owning
//! app's executor and answers once through the [`ToolReply`] it was given.
//!
//! | Kernel → host | Here |
//! | --- | --- |
//! | `peer/tool/call` | [`ToolHost::tool_call`] with a [`HostToolCall`] and its [`ToolReply`] |
//! | `peer/tool/cancel` (timeout, interrupt), the link closing | [`ToolHost::tool_cancel`]; the reply is closed first |
//! | `peer/input` (the system agent's input) | the broker starts the turn; [`ToolHost::admit_input`] may refuse it |
//! | `approval/requested` with `approval_kind: "host_tool"` | [`ToolHost::host_tool_approval`] with an [`ApprovalAnswer`] |
//!
//! Apps reach the host through their service ([`crate::OctosAppService`]):
//! an in-process module installs its executor
//! ([`crate::OctosAppService::set_tool_executor`]) and its own confirmation
//! sheet for `confirm: app` tools
//! ([`crate::OctosAppService::set_confirm_sheet`]).

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{json, Value};

/// Raw methods and notifications of UPCR-2026-035.
pub const REGISTER: &str = "peer/tools/register";
pub const TOOL_CALL: &str = "peer/tool/call";
pub const TOOL_RESULT: &str = "peer/tool/result";
pub const TOOL_CANCEL: &str = "peer/tool/cancel";
pub const PEER_INPUT: &str = "peer/input";
/// `approval/requested`'s `approval_kind` for a host-routed tool.
pub const HOST_TOOL_KIND: &str = "host_tool";
/// What an app's context sees instead of a `host_tool` approval: the host
/// draws that sheet, the app never answers it.
pub const HANDLED_BY_HOST: &str = "approval/handled_by_host";

/// The fields a `tools.json` entry (octos `ToolDecl`) may carry; the kernel
/// refuses any other.
pub const DECL_FIELDS: &[&str] = &[
    "name",
    "app",
    "description",
    "input_schema",
    "output_schema",
    "risk",
    "background",
    "outward",
    "confirm",
    "shareable",
];

/// A declaration as the kernel takes it: only [`DECL_FIELDS`] kept, and
/// `app` set to `owner` when given (a cross-app grant names its owning app;
/// without it the kernel takes the name's first segment).
pub fn declaration(entry: &Value, owner: Option<&str>) -> Option<Value> {
    let object = entry.as_object()?;
    object.get("name")?.as_str()?;
    let mut out = serde_json::Map::new();
    for (key, value) in object {
        if DECL_FIELDS.contains(&key.as_str()) {
            out.insert(key.clone(), value.clone());
        }
    }
    if let Some(owner) = owner {
        out.insert("app".into(), json!(owner));
    }
    Some(Value::Object(out))
}

/// Who is calling, as the kernel reports it (`caller.kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallerKind {
    /// An app peer's session (the peer the set is registered on).
    AppPeer,
    /// A host session that is not a peer: the system agent's conversation.
    System,
}

/// Which turn made the call, as the host that drove it knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallOrigin {
    /// One of the app's request contexts (the person, in the app).
    Context,
    /// The peer's own session, a turn the host started for a `peer/input`
    /// (the system agent's request, made for the person).
    PeerInput,
    /// The peer's own session, any other turn (the app agent's own runs).
    PeerOwn,
    /// The system agent's conversation.
    System,
}

/// One `peer/tool/call`, with the identity the host stamps. The app never
/// supplies any of it.
#[derive(Clone, Debug, PartialEq)]
pub struct HostToolCall {
    pub call_id: String,
    pub tool_call_id: String,
    pub args_digest: String,
    /// The declared name (`mail.send`).
    pub name: String,
    /// The app that owns the tool.
    pub app: String,
    pub args: Value,
    /// `read` | `act` | `destructive`.
    pub risk: String,
    /// `confirm: app`: the owning app's own sheet asks the person.
    pub confirm_required: bool,
    pub timeout_ms: u64,
    pub tools_version: u64,
    /// The peer the set is registered on (`None`: a host session set).
    pub peer: Option<String>,
    pub session_id: String,
    pub context_id: Option<String>,
    pub turn_id: String,
    pub caller_kind: CallerKind,
    // --- stamped by the host (the broker, or the system chat) ---
    /// The app whose agent calls (`system` for the system agent).
    pub calling_app: String,
    /// The calling peer's account (`None`: signed out, or the system agent).
    pub account: Option<String>,
    /// The client of the calling request context (a Rinx mini app), from
    /// the host's own context table.
    pub client: Option<String>,
    pub origin: CallOrigin,
    /// What started the call's turn, stamped by the host from its own
    /// record of the turns it started ([`crate::TurnTrigger::Unknown`]
    /// when it did not start it).
    pub trigger: crate::TurnTrigger,
}

impl HostToolCall {
    /// A `peer/tool/call`'s params. The stamped fields start empty.
    pub fn parse(params: &Value) -> Result<HostToolCall, String> {
        let s = |key: &str| params.get(key).and_then(Value::as_str).map(str::to_owned);
        let call_id = s("call_id").filter(|c| !c.is_empty()).ok_or("peer/tool/call without call_id")?;
        let name = s("name").ok_or("peer/tool/call without name")?;
        let caller = params.get("caller").cloned().unwrap_or(Value::Null);
        let caller_kind = match caller.get("kind").and_then(Value::as_str) {
            Some("system") => CallerKind::System,
            _ if params.get("peer").is_none_or(Value::is_null) => CallerKind::System,
            _ => CallerKind::AppPeer,
        };
        let app = s("app").unwrap_or_else(|| name.split('.').next().unwrap_or(&name).to_owned());
        Ok(HostToolCall {
            call_id,
            tool_call_id: s("tool_call_id").unwrap_or_default(),
            args_digest: s("args_digest").unwrap_or_default(),
            app,
            args: params.get("args").cloned().unwrap_or_else(|| json!({})),
            risk: s("risk").unwrap_or_else(|| "act".into()),
            confirm_required: params.get("confirm_required").and_then(Value::as_bool).unwrap_or(false),
            timeout_ms: params.get("timeout_ms").and_then(Value::as_u64).unwrap_or(30_000),
            tools_version: params.get("tools_version").and_then(Value::as_u64).unwrap_or(0),
            peer: s("peer"),
            session_id: s("session_id").unwrap_or_default(),
            context_id: s("context_id").filter(|c| !c.is_empty()),
            turn_id: s("turn_id").or_else(|| caller.get("turn_id").and_then(Value::as_str).map(str::to_owned)).unwrap_or_default(),
            caller_kind,
            calling_app: String::new(),
            account: None,
            client: None,
            origin: if caller_kind == CallerKind::System { CallOrigin::System } else { CallOrigin::PeerOwn },
            trigger: crate::TurnTrigger::Unknown,
            name,
        })
    }

    /// The occurrence the host executes at most once: `(session_id,
    /// turn_id, tool_call_id, args_digest)` (UPCR-2026-035, host MUSTs).
    pub fn occurrence(&self) -> String {
        if self.tool_call_id.is_empty() {
            // No occurrence id from the kernel: the call id is the occurrence.
            return format!("call\u{1f}{}", self.call_id);
        }
        format!("{}\u{1f}{}\u{1f}{}\u{1f}{}", self.session_id, self.turn_id, self.tool_call_id, self.args_digest)
    }

    /// Whether the tool is gated (the kernel asks, or the app's sheet does).
    pub fn is_read(&self) -> bool {
        self.risk == "read"
    }
}

/// How a call ended, as the host answers it.
#[derive(Clone, Debug, PartialEq)]
pub enum ToolOutcome {
    Ok(Value),
    /// `kind` must be `[a-z0-9_]{1,32}` (the model sees `host:<kind>`).
    Error { kind: String, message: String },
}

impl ToolOutcome {
    pub fn error(kind: &str, message: impl Into<String>) -> ToolOutcome {
        ToolOutcome::Error { kind: error_kind(kind), message: message.into() }
    }

    /// The `peer/tool/result` fields for this outcome (without credentials).
    pub fn to_params(&self, call_id: &str) -> Value {
        match self {
            ToolOutcome::Ok(data) => json!({"call_id": call_id, "ok": true, "data": data}),
            ToolOutcome::Error { kind, message } => {
                json!({"call_id": call_id, "ok": false, "error": {"kind": kind, "message": message}})
            }
        }
    }
}

/// A kind the kernel accepts: lowercase, digits and `_`, 1–32 characters.
fn error_kind(kind: &str) -> String {
    let mut out: String = kind
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() { c } else { '_' })
        .take(32)
        .collect();
    if out.is_empty() {
        out.push_str("error");
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplyState {
    Open { acked: bool },
    Cancelled,
    Finished,
}

struct ReplyInner {
    call_id: String,
    state: Mutex<ReplyState>,
    send: Box<dyn Fn(Value) + Send + Sync>,
}

/// The one answer to one call: at most one acknowledgement, exactly one
/// result, nothing after a cancel. Cheap to clone; every clone is the same
/// answer.
#[derive(Clone)]
pub struct ToolReply(Arc<ReplyInner>);

impl std::fmt::Debug for ToolReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ToolReply({}, {:?})", self.0.call_id, self.state())
    }
}

impl PartialEq for ToolReply {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl ToolReply {
    /// `send` gets the `peer/tool/result` fields (`call_id` and `ok`/`data`/
    /// `error`, or `status`); its owner adds the session and credentials.
    pub fn new(call_id: impl Into<String>, send: impl Fn(Value) + Send + Sync + 'static) -> ToolReply {
        ToolReply(Arc::new(ReplyInner { call_id: call_id.into(), state: Mutex::new(ReplyState::Open { acked: false }), send: Box::new(send) }))
    }

    pub fn call_id(&self) -> &str {
        &self.0.call_id
    }

    fn state(&self) -> ReplyState {
        *self.0.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Neither answered nor cancelled: the call may still run.
    pub fn is_open(&self) -> bool {
        matches!(self.state(), ReplyState::Open { .. })
    }

    pub fn is_cancelled(&self) -> bool {
        self.state() == ReplyState::Cancelled
    }

    /// `status: "awaiting_confirmation"`, before a confirmation sheet. Sent
    /// once; false when the call is no longer open.
    pub fn acknowledge(&self) -> bool {
        {
            let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
            match *state {
                ReplyState::Open { acked: false } => *state = ReplyState::Open { acked: true },
                ReplyState::Open { acked: true } => return true,
                _ => return false,
            }
        }
        (self.0.send)(json!({"call_id": self.0.call_id, "status": "awaiting_confirmation"}));
        true
    }

    /// The result. Sent once; false (and nothing sent) when the call was
    /// cancelled or already answered.
    pub fn finish(&self, outcome: ToolOutcome) -> bool {
        {
            let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
            if !matches!(*state, ReplyState::Open { .. }) {
                return false;
            }
            *state = ReplyState::Finished;
        }
        (self.0.send)(outcome.to_params(&self.0.call_id));
        true
    }

    /// The kernel cancelled the call (or its connection closed): nothing of
    /// it is sent or executed afterwards. True when it was still open.
    pub fn cancel(&self) -> bool {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(*state, ReplyState::Open { .. }) {
            *state = ReplyState::Cancelled;
            true
        } else {
            false
        }
    }
}

/// One `host_tool` approval (`typed_details.host_tool`).
#[derive(Clone, Debug, PartialEq)]
pub struct HostToolApproval {
    pub approval_id: String,
    /// The session the approval was raised on (the calling session).
    pub session_id: String,
    pub turn_id: String,
    /// The app that owns the tool.
    pub app: String,
    pub tool: String,
    pub args: Value,
    pub risk: String,
    pub outward: bool,
    pub calling_kind: CallerKind,
    pub calling_peer: Option<String>,
    pub context_id: Option<String>,
    pub tool_call_id: Option<String>,
    /// The same call ran before and its outcome is unknown: always the person.
    pub outcome_unknown_before: bool,
    /// What started the turn, stamped by the host (see
    /// [`HostToolCall::trigger`]); `Unknown` until then.
    pub trigger: crate::TurnTrigger,
}

impl HostToolApproval {
    /// An `approval/requested`'s params, when it is a `host_tool` approval.
    /// `session` is the full calling session key (topic joined).
    pub fn parse(params: &Value, session: &str) -> Option<HostToolApproval> {
        if params.get("approval_kind").and_then(Value::as_str) != Some(HOST_TOOL_KIND) {
            return None;
        }
        let d = params.get("typed_details")?.get("host_tool")?;
        let s = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_owned);
        Some(HostToolApproval {
            approval_id: s(params, "approval_id")?,
            session_id: session.to_owned(),
            turn_id: s(params, "turn_id").unwrap_or_default(),
            app: s(d, "app")?,
            tool: s(d, "tool")?,
            args: d.get("args").cloned().unwrap_or(Value::Null),
            risk: s(d, "risk").unwrap_or_default(),
            outward: d.get("outward").and_then(Value::as_bool).unwrap_or(false),
            calling_kind: if s(d, "calling_kind").as_deref() == Some("system") { CallerKind::System } else { CallerKind::AppPeer },
            calling_peer: s(d, "calling_peer"),
            context_id: s(d, "context_id"),
            tool_call_id: s(d, "tool_call_id"),
            outcome_unknown_before: d.get("outcome_unknown_before").and_then(Value::as_bool).unwrap_or(false),
            trigger: crate::TurnTrigger::Unknown,
        })
    }
}

/// The one answer to one `host_tool` approval (`approval/respond` on the
/// connection it was raised to). Sent once.
#[derive(Clone)]
pub struct ApprovalAnswer {
    sent: Arc<Mutex<bool>>,
    send: Arc<dyn Fn(bool) + Send + Sync>,
}

impl std::fmt::Debug for ApprovalAnswer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ApprovalAnswer(sent: {})", *self.sent.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

impl ApprovalAnswer {
    pub fn new(send: impl Fn(bool) + Send + Sync + 'static) -> ApprovalAnswer {
        ApprovalAnswer { sent: Arc::new(Mutex::new(false)), send: Arc::new(send) }
    }
    /// The person's (or the host's rule's) decision. False when already sent.
    pub fn respond(&self, approve: bool) -> bool {
        {
            let mut sent = self.sent.lock().unwrap_or_else(|e| e.into_inner());
            if *sent {
                return false;
            }
            *sent = true;
        }
        (self.send)(approve);
        true
    }
}

/// The system agent's input to a host-owned peer (`peer/input`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerInput {
    pub peer: String,
    pub session_id: String,
    pub input_id: String,
    pub turn_id: String,
    pub text: String,
}

impl PeerInput {
    pub fn parse(params: &Value) -> Option<PeerInput> {
        let s = |key: &str| params.get(key).and_then(Value::as_str).map(str::to_owned);
        Some(PeerInput {
            peer: s("peer")?,
            session_id: s("session_id")?,
            input_id: s("input_id")?,
            turn_id: s("turn_id").filter(|t| !t.is_empty())?,
            text: s("text").unwrap_or_default(),
        })
    }
}

/// How an app's sheet answers: (approved, reason).
type ConfirmFn = Arc<dyn Fn(bool, &str) + Send + Sync>;

/// A `confirm: app` call, as the owning app's own sheet gets it: the tool,
/// the exact arguments and who is calling. The app answers once with
/// [`ConfirmRequest::answer`].
#[derive(Clone)]
pub struct ConfirmRequest {
    pub id: String,
    pub tool: String,
    pub args: Value,
    /// "Calendar's agent", "The assistant", "Your mini app news".
    pub caller_label: String,
    pub context_id: Option<String>,
    pub client: Option<String>,
    answer: ConfirmFn,
    answered: Arc<Mutex<bool>>,
}

impl std::fmt::Debug for ConfirmRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfirmRequest").field("id", &self.id).field("tool", &self.tool).field("caller_label", &self.caller_label).finish()
    }
}

impl ConfirmRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        tool: impl Into<String>,
        args: Value,
        caller_label: impl Into<String>,
        context_id: Option<String>,
        client: Option<String>,
        answer: impl Fn(bool, &str) + Send + Sync + 'static,
    ) -> ConfirmRequest {
        ConfirmRequest {
            id: id.into(),
            tool: tool.into(),
            args,
            caller_label: caller_label.into(),
            context_id,
            client,
            answer: Arc::new(answer),
            answered: Arc::new(Mutex::new(false)),
        }
    }
    /// The person's answer on the app's sheet. Once; false after that.
    pub fn answer(&self, approved: bool, reason: &str) -> bool {
        {
            let mut done = self.answered.lock().unwrap_or_else(|e| e.into_inner());
            if *done {
                return false;
            }
            *done = true;
        }
        (self.answer)(approved, reason);
        true
    }
}

/// An app's own confirmation sheet for its `confirm: app` tools (Rinx's
/// send sheet), shown for callers of every kind.
pub trait ConfirmSheet: Send + Sync {
    fn confirm(&self, request: ConfirmRequest);
}

/// An in-process app's executor for its own tools. It answers each call
/// once through `reply` and must not run a call whose reply is closed.
pub trait ToolExecutor: Send + Sync {
    fn execute(&self, call: HostToolCall, reply: ToolReply);
    /// The kernel cancelled `call_id` (its reply is already closed).
    fn cancel(&self, _call_id: &str) {}
}

/// What the broker needs from the shell. Every method has a safe default
/// (no tools, calls refused, approvals left to the app), so a host that
/// installs nothing behaves like a host that predates UPCR-2026-035 except
/// that it registers an empty set.
pub trait ToolHost: Send + Sync {
    /// The declarations to register on `app_id`'s peer for `account`: the
    /// app's granted `tools.json` entries plus the cross-app tools granted to
    /// it (each with `app` naming its owner). `Err`: registering must not
    /// happen (the peer then runs no turn).
    fn declarations(&self, _app_id: &str, _account: &str) -> Result<Vec<Value>, String> {
        Ok(Vec::new())
    }

    /// The account's agent workspace (ADR 0004 §11), the `cwd` a NEW peer
    /// is prepared with. `None`: the kernel provisions one.
    fn agent_workspace(&self, _app_id: &str, _account: &str) -> Option<PathBuf> {
        None
    }

    /// Whether `app_id`'s `account` is suspended (signed out, removed):
    /// its calls are answered `signed_out` and no turn starts for it.
    fn suspended(&self, _app_id: &str, _account: &str) -> bool {
        false
    }

    /// One call, stamped. Answer through `reply`, once.
    fn tool_call(&self, call: HostToolCall, reply: ToolReply) {
        reply.finish(ToolOutcome::error("no_executor", format!("nothing on this host runs {}", call.name)));
    }

    /// The kernel cancelled a call, or the connection it came on closed
    /// (`reason`: `timeout`, `cancelled`, `disconnected`).
    fn tool_cancel(&self, _app_id: &str, _call_id: &str, _reason: &str) {}

    /// A `peer/input` for `app_id`'s peer, before its turn starts: `Err`
    /// refuses it (no turn runs).
    fn admit_input(&self, _app_id: &str, _account: &str, _input: &PeerInput) -> Result<(), String> {
        Ok(())
    }

    /// A `host_tool` approval raised on `app_id`'s peer or one of its
    /// contexts. True when the host took it (it answers through `answer`);
    /// false leaves it to the app's context, as before.
    fn host_tool_approval(&self, _app_id: &str, _account: Option<&str>, _approval: HostToolApproval, _answer: ApprovalAnswer) -> bool {
        false
    }

    /// An in-process app installs its executor (through its service).
    fn set_executor(&self, _app_id: &str, _executor: Option<Arc<dyn ToolExecutor>>) {}

    /// An in-process app installs its own confirmation sheet.
    fn set_confirm_sheet(&self, _app_id: &str, _sheet: Option<Arc<dyn ConfirmSheet>>) {}
}

/// The host that installed nothing.
pub struct NoToolHost;
impl ToolHost for NoToolHost {}

static HOST: OnceLock<Arc<dyn ToolHost>> = OnceLock::new();

/// Install the shell's host, once per process (later calls are ignored).
pub fn set_host(host: Arc<dyn ToolHost>) {
    let _ = HOST.set(host);
}

/// The installed host, or [`NoToolHost`].
pub fn host() -> Arc<dyn ToolHost> {
    HOST.get().cloned().unwrap_or_else(|| Arc::new(NoToolHost))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_parses_with_its_caller_and_occurrence() {
        let params = json!({
            "peer": "rinx-1", "session_id": "_main:api:octosense#peerctx-rinx-1.ab-mini", "context_id": "ab-mini",
            "turn_id": "t1", "call_id": "c1", "tool_call_id": "tc1", "args_digest": "d1", "name": "rinx.message.send",
            "app": "rinx", "caller": {"kind": "app_peer", "peer": "rinx-1", "session_id": "s", "context_id": "ab-mini", "turn_id": "t1"},
            "args": {"room": "!r", "text": "hi"}, "risk": "act", "confirm_required": true, "timeout_ms": 5000, "tools_version": 3
        });
        let call = HostToolCall::parse(&params).unwrap();
        assert_eq!(call.caller_kind, CallerKind::AppPeer);
        assert!(call.confirm_required);
        assert_eq!(call.context_id.as_deref(), Some("ab-mini"));
        assert_eq!(call.occurrence(), "_main:api:octosense#peerctx-rinx-1.ab-mini\u{1f}t1\u{1f}tc1\u{1f}d1");
        let system = HostToolCall::parse(&json!({"peer": null, "session_id": "_main:api:octosense#system", "turn_id": "t", "call_id": "c2", "name": "terminal.run", "app": "terminal", "caller": {"kind": "system"}})).unwrap();
        assert_eq!(system.caller_kind, CallerKind::System);
        assert_eq!(system.origin, CallOrigin::System);
        assert!(HostToolCall::parse(&json!({"name": "x.y"})).is_err(), "no call id, nothing to answer");
    }

    #[test]
    fn a_reply_acknowledges_once_finishes_once_and_never_after_a_cancel() {
        let sent = Arc::new(Mutex::new(Vec::<Value>::new()));
        let s = sent.clone();
        let reply = ToolReply::new("c1", move |v| s.lock().unwrap().push(v));
        assert!(reply.acknowledge());
        assert!(reply.acknowledge(), "a second ack is not sent again");
        assert!(reply.finish(ToolOutcome::Ok(json!({"n": 1}))));
        assert!(!reply.finish(ToolOutcome::Ok(json!({"n": 2}))), "exactly one result");
        assert!(!reply.cancel());
        let sent_now = sent.lock().unwrap().clone();
        assert_eq!(sent_now, vec![json!({"call_id": "c1", "status": "awaiting_confirmation"}), json!({"call_id": "c1", "ok": true, "data": {"n": 1}})]);

        let s = sent.clone();
        let cancelled = ToolReply::new("c2", move |v| s.lock().unwrap().push(v));
        assert!(cancelled.cancel());
        assert!(!cancelled.is_open());
        assert!(!cancelled.acknowledge());
        assert!(!cancelled.finish(ToolOutcome::Ok(json!({}))), "nothing after cancel");
        assert_eq!(sent.lock().unwrap().len(), 2);
    }

    #[test]
    fn error_kinds_are_what_the_kernel_accepts() {
        assert_eq!(ToolOutcome::error("Signed-Out", "x"), ToolOutcome::Error { kind: "signed_out".into(), message: "x".into() });
        assert_eq!(ToolOutcome::error("", "x"), ToolOutcome::Error { kind: "error".into(), message: "x".into() });
        let long = "a".repeat(40);
        let ToolOutcome::Error { kind, .. } = ToolOutcome::error(&long, "x") else { unreachable!() };
        assert_eq!(kind.len(), 32);
    }

    #[test]
    fn declarations_keep_only_the_kernels_fields_and_name_a_foreign_owner() {
        let entry = json!({"name": "mail.send", "description": "Send", "input_schema": {"type": "object"}, "risk": "act", "outward": true, "confirm": "host", "shareable": true, "auto_approvable": false});
        let own = declaration(&entry, None).unwrap();
        assert!(own.get("auto_approvable").is_none(), "unknown fields are refused by the kernel");
        assert!(own.get("app").is_none(), "the name's first segment names its owner");
        let cross = declaration(&entry, Some("mail")).unwrap();
        assert_eq!(cross["app"], "mail", "a cross-app grant is marked with its owning app");
        assert!(declaration(&json!({"description": "no name"}), None).is_none());
    }

    #[test]
    fn a_host_tool_approval_parses_from_typed_details() {
        let params = json!({"session_id": "_main:api:octosense", "topic": "peer-rinx-1", "approval_id": "a1", "turn_id": "t", "tool_name": "mail_send",
            "approval_kind": "host_tool", "typed_details": {"kind": "host_tool", "host_tool": {"app": "mail", "tool": "mail.send", "args": {"to": "a"}, "risk": "act", "outward": true, "calling_kind": "app_peer", "calling_peer": "rinx-1", "calling_session_id": "s", "outcome_unknown_before": true}}});
        let a = HostToolApproval::parse(&params, "_main:api:octosense#peer-rinx-1").unwrap();
        assert_eq!((a.app.as_str(), a.tool.as_str(), a.outward, a.outcome_unknown_before), ("mail", "mail.send", true, true));
        assert!(HostToolApproval::parse(&json!({"approval_id": "a", "approval_kind": "command"}), "s").is_none());
    }
}
