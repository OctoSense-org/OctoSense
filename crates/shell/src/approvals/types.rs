//! What an approval request carries and how it is answered (ADR 0004 §5, §8).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The relay's id for one approval request: octos's call or approval id, so
/// [`super::relay::ApprovalRelay::approval_decided`] needs no mapping.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RequestId(pub String);

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A standing rule's id (`r1`, `r2`, …), stable in the rules file.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RuleId(pub String);

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The answer the shell gives the relay for one request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    /// The person approved this one call (on a sheet, or the owning app's
    /// own sheet for `confirm: app`), or developer mode answered it.
    ApproveOnce,
    Deny,
    /// A standing rule answered it.
    ApproveByRule(RuleId),
}

impl Decision {
    pub fn approved(&self) -> bool {
        !matches!(self, Decision::Deny)
    }
}

/// Who draws the confirmation for a tool (`tools.json` `confirm`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Confirm {
    /// The shell's sheet; a standing rule may answer it.
    #[default]
    Host,
    /// The owning app's own sheet (Rinx's send sheet), for every caller.
    App,
}

/// The declared facts of the tool being called, from the owning app's
/// `tools.json` (the relay has it; the router never trusts the caller for
/// these).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolSpec {
    /// `mail.send`.
    pub name: String,
    pub confirm: Confirm,
    /// `false` for permanent deletion, payments, sharing outside the
    /// device, account and security changes, and typed commands: no rule
    /// ever answers it (outside developer mode).
    pub auto_approvable: bool,
    /// Command execution (`terminal.run`, `dev.run`, granted commands).
    pub command: bool,
    /// Argument names the schema types as secrets (`"format": "password"`,
    /// `"secret": true`), redacted on every sheet.
    pub secret_fields: Vec<String>,
    /// The tool's declared `input_schema`: rule conditions read only the
    /// fields it declares (`facts`); `None`, they read nothing.
    pub input_schema: Option<Value>,
}

impl ToolSpec {
    pub fn host(name: &str) -> ToolSpec {
        ToolSpec { name: name.into(), confirm: Confirm::Host, auto_approvable: true, command: false, secret_fields: Vec::new(), input_schema: None }
    }
    pub fn app(name: &str) -> ToolSpec {
        ToolSpec { confirm: Confirm::App, ..ToolSpec::host(name) }
    }
    pub fn not_auto_approvable(mut self) -> ToolSpec {
        self.auto_approvable = false;
        self
    }
    pub fn command(mut self) -> ToolSpec {
        self.command = true;
        self.auto_approvable = false;
        self
    }
    /// With its declared `input_schema` (an object schema; anything else is
    /// no schema).
    pub fn schema(mut self, schema: Value) -> ToolSpec {
        self.input_schema = schema.is_object().then_some(schema);
        self
    }
    pub fn secret(mut self, field: &str) -> ToolSpec {
        self.secret_fields.push(field.into());
        self
    }
}

/// Who is calling, stamped by the shell (ADR 0004 §5), never by the app.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Caller {
    /// The owning app's own agent (on its own session, or in one of its
    /// request contexts for `client`, e.g. a Rinx mini app).
    OwnAgent { client: Option<String> },
    /// Another app's agent (a cross-app call, §7).
    AppAgent { app: String },
    /// The system agent.
    SystemAgent,
    /// An external client's turn (Talk to Octos, another UI Protocol client
    /// on a session the shell also has open): not the shell's to answer.
    /// The router never approves it, by developer mode, a rule or a sheet
    /// ([`super::Route::LeftToClient`]); the client that started the turn
    /// answers it (ADR 0003, ADR 0004 §8, §13).
    External { client: Option<String> },
}

impl Caller {
    /// The audit's short form.
    pub fn is_external(&self) -> bool {
        matches!(self, Caller::External { .. })
    }
    pub fn as_audit(&self) -> String {
        match self {
            Caller::OwnAgent { client: None } => "own_agent".into(),
            Caller::OwnAgent { client: Some(c) } => format!("own_agent/{c}"),
            Caller::AppAgent { app } => format!("app/{app}"),
            Caller::SystemAgent => "system_agent".into(),
            Caller::External { client: None } => "external".into(),
            Caller::External { client: Some(c) } => format!("external/{c}"),
        }
    }
}

/// What started the run the call belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Trigger {
    /// The person asked (a button, the chat, voice).
    Person,
    /// The app's own trigger (a schedule, a data or file change).
    App,
    /// Content someone else sent (an email, a message): excluded from
    /// standing rules by default (§8).
    IncomingContent { from: Option<String> },
    /// The system agent's own plan.
    SystemAgent,
    /// Not said: treated as the least trusted, like incoming content for
    /// rules, never as the person.
    #[default]
    Unknown,
}

impl Trigger {
    /// Whether standing rules skip this run unless a rule opts in.
    pub fn excluded_by_default(&self) -> bool {
        matches!(self, Trigger::IncomingContent { .. } | Trigger::Unknown)
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Trigger::Person => "person",
            Trigger::App => "app",
            Trigger::IncomingContent { .. } => "incoming_content",
            Trigger::SystemAgent => "system_agent",
            Trigger::Unknown => "unknown",
        }
    }
}

/// Which kernel connection the call came in on (ADR 0003: an external
/// client's approvals are never answered by developer mode).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Connection {
    #[default]
    Host,
    External,
}

/// One request of the system agent's that several approvals belong to:
/// its `confirm: host` approvals are batched into one sheet in the system
/// chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    pub id: String,
    /// The plan, as the system agent put it ("Book Tue 3–4 pm and invite 4").
    pub plan: String,
}

/// Everything else the relay knows about the call.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RequestContext {
    /// The relay's id for this request (octos's call or approval id).
    pub call_id: String,
    pub trigger: Trigger,
    pub connection: Connection,
    /// The kernel's request context, if any (§5).
    pub context_id: Option<String>,
    pub account: Option<String>,
    /// The thread the run is about (its participants' addresses), for the
    /// "recipients in the thread" condition.
    pub thread: Vec<String>,
    /// octos marked the call `outcome_unknown` (a retry of a call that may
    /// already have happened): always the person.
    pub outcome_unknown: bool,
    pub batch: Option<Batch>,
}

/// One request, as the router holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub id: RequestId,
    /// The app that owns the tool (`mail`).
    pub app: String,
    pub tool: ToolSpec,
    /// The exact arguments.
    pub args: Value,
    pub caller: Caller,
    pub context: RequestContext,
    /// Unix seconds.
    pub received: u64,
}
