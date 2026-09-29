//! The model of a shell-drawn approval sheet (ADR 0004 §8; ADR 0002 §10).
//!
//! One sheet per request, in the owning app's conversation; or one batched
//! sheet in the system chat for the `confirm: host` approvals of one of the
//! system agent's requests. Each line shows the **owning app, the tool, the
//! exact arguments** (pretty-printed, secret-typed fields redacted) and
//! **who is calling**, and is answered on its own: approve once, deny, or
//! "always for …" (which creates a standing rule; only offered where a rule
//! could ever answer the call). The view (`view.rs`) draws this model and
//! nothing else; an agent's own text is never an approval surface.

use super::facts;
use super::contacts::ContactsSource;
use super::rules::{Conditions, RuleDraft, MAX_EVERYTHING_MINUTES};
use super::types::{Caller, Connection, Request, RequestId, Trigger};

/// Why the router put a request in front of the person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Surfaced {
    /// The tool declares `auto_approvable: false`.
    NotAutoApprovable,
    /// octos marked the call `outcome_unknown`.
    OutcomeUnknown,
    /// A run started by incoming content; rules are off for it by default.
    IncomingContent,
    /// A Talk to Octos external client asked.
    External,
    /// No rule answered it.
    NoRule,
}

impl Surfaced {
    pub fn note(&self) -> &'static str {
        match self {
            Surfaced::NotAutoApprovable => "This tool always needs you: no rule can approve it.",
            Surfaced::OutcomeUnknown => "An earlier try may already have happened; it is never retried without you.",
            Surfaced::IncomingContent => "Started by something someone else sent, so rules don't apply.",
            Surfaced::External => "Asked by an outside client.",
            Surfaced::NoRule => "",
        }
    }
    /// Whether an "always for …" answer makes sense.
    pub fn rules_could_answer(&self) -> bool {
        matches!(self, Surfaced::NoRule | Surfaced::IncomingContent)
    }
}

/// Where the sheet is drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    AppConversation { app: String },
    SystemChat { batch: String, plan: String },
}

/// One "always for …" choice: the rule a tap on it creates.
#[derive(Clone, Debug, PartialEq)]
pub struct AlwaysChoice {
    pub label: String,
    pub draft: RuleDraft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Once,
    Deny,
    /// The index into [`Line::always`].
    Always(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub request: RequestId,
    pub app: String,
    pub app_label: String,
    pub tool: String,
    pub caller: String,
    /// The exact arguments, one printed line each, secrets redacted.
    pub args: Vec<String>,
    pub surfaced: Surfaced,
    pub always: Vec<AlwaysChoice>,
    pub answer: Option<Answer>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    pub id: u64,
    pub place: Place,
    pub lines: Vec<Line>,
    /// Unix seconds.
    pub opened: u64,
}

/// `os.mail` → `Mail`.
pub fn app_label(app: &str) -> String {
    let id = app.strip_prefix("os.").unwrap_or(app);
    let mut chars = id.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// "Calendar's agent", "The system agent", "Rinx's agent for weather".
pub fn caller_label(owning_app: &str, caller: &Caller) -> String {
    match caller {
        Caller::OwnAgent { client: None } => format!("{}'s agent", app_label(owning_app)),
        Caller::OwnAgent { client: Some(c) } => format!("{}'s agent for {c}", app_label(owning_app)),
        Caller::AppAgent { app } => format!("{}'s agent", app_label(app)),
        Caller::SystemAgent => "The system agent".into(),
        Caller::External { client: None } => "An outside client".into(),
        Caller::External { client: Some(c) } => format!("An outside client ({c})"),
    }
}

impl Line {
    pub fn for_request(req: &Request, surfaced: Surfaced, contacts: &dyn ContactsSource) -> Line {
        let always = if surfaced.rules_could_answer() && req.tool.auto_approvable && !req.context.outcome_unknown && req.context.connection == Connection::Host {
            always_choices(req, contacts)
        } else {
            Vec::new()
        };
        Line {
            request: req.id.clone(),
            app: req.app.clone(),
            app_label: app_label(&req.app),
            tool: req.tool.name.clone(),
            caller: caller_label(&req.app, &req.caller),
            args: facts::pretty(&req.args, &req.tool.secret_fields),
            surfaced,
            always,
            answer: None,
        }
    }
    /// "Mail · mail.send".
    pub fn heading(&self) -> String {
        format!("{} \u{00b7} {}", self.app_label, self.tool)
    }
}

/// The rules a sheet offers for this call, narrowest first.
fn always_choices(req: &Request, contacts: &dyn ContactsSource) -> Vec<AlwaysChoice> {
    let mut out = Vec::new();
    let recipients = facts::recipients(&req.args);
    let no_attachments = !facts::has_attachments(&req.args);
    let tool = &req.tool.name;
    let with_attachments = |mut c: Conditions| {
        c.no_attachments = no_attachments;
        c
    };
    if !recipients.is_empty() {
        if recipients.iter().all(|r| contacts.is_known(r)) {
            let c = with_attachments(Conditions { recipients_in_contacts: true, ..Conditions::default() });
            out.push(AlwaysChoice { label: format!("Always for people in my contacts{}", if no_attachments { ", no attachments" } else { "" }), draft: RuleDraft::tool(&req.app, tool, c) });
        }
        if recipients.iter().all(|r| req.context.thread.iter().any(|t| t.eq_ignore_ascii_case(r))) {
            let c = with_attachments(Conditions { recipients_in_thread: true, ..Conditions::default() });
            out.push(AlwaysChoice { label: "Always for people in this thread".into(), draft: RuleDraft::tool(&req.app, tool, c) });
        }
    }
    if req.context.trigger == Trigger::Person {
        let c = Conditions { triggered_by_person: true, ..Conditions::default() };
        out.push(AlwaysChoice { label: format!("Always when I start it"), draft: RuleDraft::tool(&req.app, tool, c) });
    }
    out.push(AlwaysChoice { label: format!("Allow {tool} for 1 hour"), draft: RuleDraft::tool(&req.app, tool, Conditions::default()).for_minutes(60) });
    out.push(AlwaysChoice {
        label: format!("Everything {} asks, next {MAX_EVERYTHING_MINUTES} min", app_label(&req.app)),
        draft: RuleDraft::everything(&req.app, MAX_EVERYTHING_MINUTES),
    });
    out
}

impl Sheet {
    pub fn title(&self) -> String {
        match &self.place {
            Place::SystemChat { plan, .. } => {
                if plan.trim().is_empty() {
                    "The system agent asks".into()
                } else {
                    plan.clone()
                }
            }
            Place::AppConversation { app } => {
                let tool = self.lines.first().map(|l| l.tool.as_str()).unwrap_or("");
                format!("{} wants to use {tool}", app_label(app))
            }
        }
    }
    /// Under the title: where the sheet is.
    pub fn subtitle(&self) -> String {
        match &self.place {
            Place::SystemChat { .. } => {
                let n = self.open_lines().count();
                format!("In the system chat \u{00b7} {n} action{} to approve", if n == 1 { "" } else { "s" })
            }
            Place::AppConversation { app } => format!("In {}'s conversation", app_label(app)),
        }
    }
    pub fn line(&self, id: &RequestId) -> Option<&Line> {
        self.lines.iter().find(|l| l.request == *id)
    }
    pub fn open_lines(&self) -> impl Iterator<Item = &Line> {
        self.lines.iter().filter(|l| l.answer.is_none())
    }
    /// Every line answered: the sheet closes.
    pub fn done(&self) -> bool {
        self.lines.iter().all(|l| l.answer.is_some())
    }
    pub fn batch_id(&self) -> Option<&str> {
        match &self.place {
            Place::SystemChat { batch, .. } => Some(batch),
            Place::AppConversation { .. } => None,
        }
    }
}
