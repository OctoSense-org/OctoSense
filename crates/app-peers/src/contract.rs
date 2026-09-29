//! The service contract an app consumes. No runtime, no kernel, no I/O.
//!
//! The contract surfaces follow Rinx ADR 0007's provider table:
//!
//! | Surface | App | Provider / host |
//! | --- | --- | --- |
//! | Availability and services | renders [`Availability`]; rejects unsupported calls | reports readiness and the effective [`OctosAppService::services`] |
//! | Request contexts | supplies host-authenticated account + instance ([`ContextSpec`]) | binds the peer's request context and workspace |
//! | Requests and events | enforces its own grants; routes replies to the instance | enforces the lease on every call; correlates events |
//! | Model / settings | shows [`ModelInfo`] and the [`SettingsEntry`] | selects the model, keeps provider secrets |
//! | Cancellation and release | releases on close / account change; drops stale replies | cancels scoped work without stopping unrelated work |
//! | Runtime shutdown | [`OctosAppService::shutdown`] | stops only a runtime the app owns |

use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::Value;

/// The assistant services an app may be granted, by exact name (the App Hub
/// capability contract, `octosense_app_policy::OCTOS_SERVICES`). A prefix is
/// never a grant.
pub const OCTOS_SERVICES: [&str; 4] = [
    "octos.session.open",
    "octos.session.history",
    "octos.turn.start",
    "octos.turn.interrupt",
];

/// The exact assistant services among `declared` (a native module's
/// `capabilities()` or a manifest's list). Anything not exactly named in
/// [`OCTOS_SERVICES`] is ignored, so `octos.` or `octos.admin` grants nothing.
pub fn octos_services_in<'a>(declared: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    declared
        .into_iter()
        .filter(|name| OCTOS_SERVICES.contains(name))
        .map(str::to_owned)
        .collect()
}

/// Who owns the kernel behind a service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deployment {
    /// OctoSense owns the shared runtime; the app is a peer of the system
    /// agent. Selected by module creation in the shell, nothing else.
    Hosted,
    /// A standalone app owns a local runtime (started on the first
    /// authorized request, stopped with the app).
    StandaloneLocal,
    /// A standalone app talks to an explicitly configured remote server,
    /// which owns its runtime and model credentials.
    StandaloneRemote,
}

/// Whether requests can run now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    /// No service here (no kernel, not configured, not granted, signed out).
    /// A normal state: the app's ordinary UI stays usable.
    Unavailable(String),
    /// Nothing is running yet; the first authorized request starts or
    /// connects it.
    Idle,
    /// Connected and the app's peer is bound.
    Ready,
    /// The last attempt failed; the next request retries.
    Failed(String),
}

/// The effective model of the app's peer. Never carries credentials.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelInfo {
    /// A configured lane key, or `primary`.
    pub lane: String,
    pub provider: Option<String>,
    pub model: Option<String>,
}

/// Where the person changes AI settings for this service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsEntry {
    /// The host's shared AI settings (OctoSense: Settings → Accounts → AI
    /// providers). The app offers no endpoint or key form of its own.
    Host,
    /// The app's own local-runtime settings.
    AppLocal,
    /// The app's remote-server configuration.
    AppRemote,
}

/// One request context an app asks for: its host-authenticated account and a
/// host-assigned instance key. Never deserialized from untrusted input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextSpec {
    /// The app-level account the context acts for (Rinx: the Matrix user id).
    pub account: String,
    /// A key unique to this client instance and generation; the kernel
    /// context id is derived from it.
    pub instance: String,
    /// The services this instance was granted (intersected again with the
    /// app's own grant by the provider).
    pub services: BTreeSet<String>,
}

/// What started a turn, as the host that started it knows (ADR 0004 §8).
/// Never "the person" unless the host saw the person ask.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum TurnTrigger {
    /// The person asked: a button or composer in the app's UI, the chat.
    Person,
    /// The app's own schedule or background work (a timer, a data change).
    App,
    /// Content someone else sent (a message, an email) started it.
    Incoming { from: Option<String> },
    /// The system agent's request (a `peer/input` turn).
    SystemAgent,
    /// Not said.
    #[default]
    Unknown,
}

impl TurnTrigger {
    /// The wire form an app may give with `octos.turn.start`:
    /// `"trigger": "person" | "app" | "schedule" | "background" | "incoming"`
    /// and, for incoming content, `"from"`. Anything else is
    /// [`TurnTrigger::Unknown`]; `system_agent` is the host's alone.
    pub fn from_args(args: &Value) -> TurnTrigger {
        match args.get("trigger").and_then(Value::as_str) {
            Some("person") => TurnTrigger::Person,
            Some("app" | "schedule" | "background") => TurnTrigger::App,
            Some("incoming") => TurnTrigger::Incoming { from: args.get("from").and_then(Value::as_str).map(str::to_owned) },
            _ => TurnTrigger::Unknown,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            TurnTrigger::Person => "person",
            TurnTrigger::App => "app",
            TurnTrigger::Incoming { .. } => "incoming",
            TurnTrigger::SystemAgent => "system_agent",
            TurnTrigger::Unknown => "unknown",
        }
    }
}

/// Who spoke in one turn of the app peer's shared conversation (octos
/// UPCR-2026-034, "The shared peer conversation"): the kernel records it as
/// a marker in front of the turn's user message, `[from the person]`,
/// `[from the person: <label>]`, `[from the system agent]` or
/// `[from the app]`. A surface shows the speaker and the text after the
/// marker ([`split_origin_marker`]); the transcript keeps the marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Speaker {
    pub kind: crate::host_tools::TurnOrigin,
    /// The label the host gave (the app's display name), sanitized by the
    /// kernel.
    pub label: Option<String>,
}

impl Speaker {
    /// `{"kind": "person" | "system_agent" | "app", "label"?}`.
    pub fn to_json(&self) -> Value {
        let mut out = serde_json::json!({"kind": self.kind.as_str()});
        if let Some(label) = &self.label {
            out["label"] = Value::String(label.clone());
        }
        out
    }
}

/// The kernel's leading origin marker of a user message, split off: the
/// speaker and the speaker's own text. `None` when the text does not start
/// with a marker (an unlabelled turn, a request context's). Only the FIRST
/// marker is the kernel's; anything after it is the speaker's text.
pub fn split_origin_marker(text: &str) -> Option<(Speaker, &str)> {
    use crate::host_tools::TurnOrigin;
    let rest = text.strip_prefix("[from ")?;
    let (kind, rest) = [("the person", TurnOrigin::Person), ("the system agent", TurnOrigin::SystemAgent), ("the app", TurnOrigin::App)]
        .into_iter()
        .find_map(|(who, kind)| rest.strip_prefix(who).map(|r| (kind, r)))?;
    let (label, rest) = if let Some(rest) = rest.strip_prefix(']') {
        (None, rest)
    } else {
        // `: <label>]`; the kernel strips brackets from labels.
        let rest = rest.strip_prefix(": ")?;
        let end = rest.find(']')?;
        let label = rest[..end].trim();
        ((!label.is_empty()).then(|| label.to_owned()), &rest[end + 1..])
    };
    Some((Speaker { kind, label }, rest.strip_prefix(' ').unwrap_or(rest)))
}

/// An operation on a request context. The app supplies input text and
/// decisions its native UI collected, never session, profile, workspace or
/// raw method identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextOp {
    /// `octos.session.open`: open the context's conversation.
    Open,
    /// `octos.session.history`: its messages.
    History,
    /// `octos.turn.start`: run a turn on `text`. What started it is not
    /// said, so it counts as [`TurnTrigger::Unknown`] (the least trusted).
    Turn { text: String },
    /// `octos.turn.start` with what started the turn: the person in the
    /// app's UI, the app's own schedule, or content someone else sent. The
    /// host stamps it on every tool call and approval of the turn (ADR 0004
    /// §8: standing rules skip incoming content and unknown runs).
    TurnFrom { text: String, trigger: TurnTrigger },
    /// `octos.turn.interrupt`: stop the context's running turn.
    Interrupt,
    /// A person's decision on a tool approval raised in this context,
    /// collected by the app's native UI (requires `octos.turn.start`).
    Approval { id: String, approve: bool },
}

impl ContextOp {
    /// A turn's text and trigger (`None` for any other operation).
    pub fn turn(&self) -> Option<(&str, TurnTrigger)> {
        match self {
            ContextOp::Turn { text } => Some((text, TurnTrigger::Unknown)),
            ContextOp::TurnFrom { text, trigger } => Some((text, trigger.clone())),
            _ => None,
        }
    }

    /// The service an operation needs.
    pub fn service(&self) -> &'static str {
        match self {
            ContextOp::Open => "octos.session.open",
            ContextOp::History => "octos.session.history",
            ContextOp::Turn { .. } | ContextOp::TurnFrom { .. } | ContextOp::Approval { .. } => "octos.turn.start",
            ContextOp::Interrupt => "octos.turn.interrupt",
        }
    }
}

/// One asynchronous result. A call ends with exactly one `Complete`;
/// `Data` streams progress (`{"method", "params"}` of a kernel notification
/// for this context, plus `text` so far during a turn).
#[derive(Debug)]
pub enum ContextEvent {
    Data(Value),
    Complete(Result<Value, String>),
}

/// Where a call's events go. Called from a provider thread.
pub type EventSink = Arc<dyn Fn(ContextEvent) + Send + Sync>;

/// A request context of the app's peer.
pub trait OctosContext: Send + Sync {
    /// Start `op`. Errors that are known at once (not granted, closed,
    /// revoked, busy) return `Err` without calling `sink`.
    fn call(&self, op: ContextOp, sink: EventSink) -> Result<(), String>;
    /// Close the context for good: its running turn is interrupted and late
    /// events are dropped. Idempotent.
    fn close(&self);
    /// Whether the context can still take calls.
    fn is_open(&self) -> bool;
    /// Follow the whole conversation (`None` stops): every event of the
    /// app peer's shared conversation, whoever speaks (the person from any
    /// of the app's surfaces, the app, the system agent), as
    /// [`ContextEvent::Data`] with `speaker` ([`Speaker::to_json`]) when
    /// known, and for a user message `display_text`, its text without the
    /// kernel's marker. Never a `Complete`. Only a conversation
    /// ([`OctosAppService::open_conversation`]) has one; a request context
    /// is its own caller's, and ignores it.
    fn subscribe(&self, _sink: Option<EventSink>) {}
}

/// The scoped assistant service one app instance holds.
pub trait OctosAppService: Send + Sync {
    fn deployment(&self) -> Deployment;
    fn availability(&self) -> Availability;
    /// The app's effective assistant services (declared ∩ supported ∩ host
    /// policy ∩ user grant). Empty means no assistant for this app.
    fn services(&self) -> BTreeSet<String>;
    /// The peer's effective model, once known.
    fn model(&self) -> Option<ModelInfo>;
    fn settings_entry(&self) -> SettingsEntry;
    /// Bind the app's current account (`None`: signed out). A change revokes
    /// every context of the previous account; their late events are dropped
    /// and they are never restored under another account.
    fn set_account(&self, account: Option<&str>);
    /// A new request context for one client instance: its own transcript,
    /// separate from the app's conversation. For callers that keep one per
    /// client (Rinx's mini apps; a process app's `octos.session.open` with a
    /// `client`).
    fn open_context(&self, spec: ContextSpec) -> Result<Arc<dyn OctosContext>, String>;
    /// A handle on the app peer's ONE shared conversation (ADR 0004 §6):
    /// the peer's own session, which the person (through the app's UI and
    /// its cards) and the system agent both talk to. Its turns carry who is
    /// speaking (`origin: person`, or `app` when the app started the run),
    /// wait their turn behind the peer's running one, and its history is the
    /// peer's transcript. The default (a service without a shared
    /// conversation) is a request context.
    fn open_conversation(&self, spec: ContextSpec) -> Result<Arc<dyn OctosContext>, String> {
        self.open_context(spec)
    }
    /// The app is closing: close every context and release subscriptions.
    /// Does not stop a shared kernel or other apps' work.
    fn release(&self);
    /// Stop the runtime, only when this service owns it (standalone local).
    fn shutdown(&self);
    /// Install (or with `None` remove) the app's executor for its own tools
    /// (UPCR-2026-035): the host hands it every call of the app's tools,
    /// whoever makes it, once authorized. A service without a tool host
    /// ignores it.
    fn set_tool_executor(&self, _executor: Option<Arc<dyn crate::host_tools::ToolExecutor>>) {}
    /// Install (or remove) the app's own confirmation sheet for its
    /// `confirm: app` tools (ADR 0004 §8: Rinx's send sheet), shown for
    /// callers of every kind with who is calling.
    fn set_confirm_sheet(&self, _sheet: Option<Arc<dyn crate::host_tools::ConfirmSheet>>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exact_service_names_are_granted() {
        let declared = [
            "net",
            "storage",
            "octos.session.history",
            "octos.",
            "octos.admin",
            "octos.turn.start",
            "Octos.turn.interrupt",
        ];
        let granted = octos_services_in(declared);
        assert_eq!(
            granted.into_iter().collect::<Vec<_>>(),
            ["octos.session.history", "octos.turn.start"]
        );
        assert!(octos_services_in(["net", "storage"]).is_empty());
    }

    #[test]
    fn the_kernels_origin_marker_splits_into_speaker_and_text() {
        use crate::host_tools::TurnOrigin;
        let (who, text) = split_origin_marker("[from the person: Notes] hi [from the system agent] there").unwrap();
        assert_eq!(who, Speaker { kind: TurnOrigin::Person, label: Some("Notes".into()) });
        assert_eq!(text, "hi [from the system agent] there", "only the first marker is the kernel's");
        let (who, text) = split_origin_marker("[from the system agent] check the inbox").unwrap();
        assert_eq!((who.kind, who.label, text), (TurnOrigin::SystemAgent, None, "check the inbox"));
        assert_eq!(split_origin_marker("[from the app] sync").unwrap().0.kind, TurnOrigin::App);
        assert_eq!(split_origin_marker("[from the person]").unwrap().1, "");
        assert!(split_origin_marker("hello").is_none());
        assert!(split_origin_marker(" [from the person] hi").is_none());
        assert!(split_origin_marker("[from the moon] hi").is_none());
        assert_eq!(Speaker { kind: TurnOrigin::App, label: None }.to_json(), serde_json::json!({"kind": "app"}));
    }

    #[test]
    fn history_does_not_need_turns_and_approvals_need_turns() {
        assert_eq!(ContextOp::History.service(), "octos.session.history");
        assert_eq!(
            ContextOp::Approval {
                id: "a".into(),
                approve: true
            }
            .service(),
            "octos.turn.start"
        );
    }
}
