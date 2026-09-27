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

/// An operation on a request context. The app supplies input text and
/// decisions its native UI collected, never session, profile, workspace or
/// raw method identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextOp {
    /// `octos.session.open`: open the context's conversation.
    Open,
    /// `octos.session.history`: its messages.
    History,
    /// `octos.turn.start`: run a turn on `text`.
    Turn { text: String },
    /// `octos.turn.interrupt`: stop the context's running turn.
    Interrupt,
    /// A person's decision on a tool approval raised in this context,
    /// collected by the app's native UI (requires `octos.turn.start`).
    Approval { id: String, approve: bool },
}

impl ContextOp {
    /// The service an operation needs.
    pub fn service(&self) -> &'static str {
        match self {
            ContextOp::Open => "octos.session.open",
            ContextOp::History => "octos.session.history",
            ContextOp::Turn { .. } | ContextOp::Approval { .. } => "octos.turn.start",
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
    /// A new request context for one client instance.
    fn open_context(&self, spec: ContextSpec) -> Result<Arc<dyn OctosContext>, String>;
    /// The app is closing: close every context and release subscriptions.
    /// Does not stop a shared kernel or other apps' work.
    fn release(&self);
    /// Stop the runtime, only when this service owns it (standalone local).
    fn shutdown(&self);
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
