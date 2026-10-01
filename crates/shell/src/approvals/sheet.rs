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
    /// The exact arguments, every row of them ([`argument_rows`]): one
    /// printed line each, secrets redacted, hidden characters made
    /// visible. The view wraps long rows and scrolls, and never cuts one.
    pub args: Vec<String>,
    pub surfaced: Surfaced,
    pub always: Vec<AlwaysChoice>,
    pub answer: Option<Answer>,
    /// The app whose agent asked (its conversation's Stop stops that
    /// agent's turn); `None` for the system agent.
    pub agent: Option<String>,
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
            args: argument_rows(req),
            surfaced,
            always,
            answer: None,
            agent: super::router::agent_of(req),
        }
    }
    /// "Mail · mail.send".
    pub fn heading(&self) -> String {
        format!("{} \u{00b7} {}", self.app_label, self.tool)
    }
}

/// Characters that draw as nothing, or that change how the text around
/// them reads (bidi overrides and isolates, zero-width characters, line and
/// paragraph separators, tag characters): shown as their code point.
fn hidden(c: char) -> bool {
    c.is_control()
        || matches!(c as u32,
            0x00AD | 0x034F | 0x061C | 0x115F | 0x1160 | 0x17B4 | 0x17B5 | 0x180B..=0x180F | 0x200B..=0x200F | 0x2028..=0x202E | 0x2060..=0x206F | 0x3164 | 0xFE00..=0xFE0F | 0xFEFF | 0xFFA0 | 0xFFF0..=0xFFFB | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A | 0xE0000..=0xE0FFF)
}

/// `text` with every control and format character shown as `⟨U+202E⟩`, so
/// what the sheet draws is what the call carries.
pub fn visible(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if hidden(c) {
            out.push_str(&format!("\u{27E8}U+{:04X}\u{27E9}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

/// Every row a sheet shows for a request's arguments. A command tool's
/// `command` comes first, one numbered row per line of it (`terminal.run`
/// types every line: a second command after a newline is in plain sight),
/// then the other arguments; any other tool's arguments are pretty-printed
/// JSON. Secrets are redacted and hidden characters made [`visible`].
pub fn argument_rows(req: &Request) -> Vec<String> {
    let secret = &req.tool.secret_fields;
    if req.tool.command {
        if let Some(command) = req.args.get("command").and_then(serde_json::Value::as_str).filter(|_| !facts::is_secret("command", secret)) {
            let lines: Vec<&str> = command.split('\n').collect();
            let mut rows = vec![format!("command ({} line{}):", lines.len(), if lines.len() == 1 { "" } else { "s" })];
            let width = lines.len().to_string().len().max(1);
            rows.extend(lines.iter().enumerate().map(|(i, l)| format!("  {:>width$} \u{2502} {}", i + 1, visible(l))));
            let mut rest = req.args.clone();
            if let Some(map) = rest.as_object_mut() {
                map.remove("command");
                if !map.is_empty() {
                    rows.extend(facts::pretty(&rest, secret).iter().map(|r| visible(r)));
                }
            }
            return rows;
        }
    }
    facts::pretty(&req.args, secret).iter().map(|r| visible(r)).collect()
}

/// What starts a wrapped row's continuation.
pub const WRAP_MARK: &str = "\u{21B3} ";

/// `row` in pieces no wider than `max_w`; every piece after the first
/// starts with [`WRAP_MARK`]. `measure` is asked for single characters
/// (and the mark), so wrapping a long row costs one measure per distinct
/// character. Nothing is dropped: the pieces, without the marks, are `row`.
pub fn wrap(row: &str, max_w: f64, mut measure: impl FnMut(&str) -> f64) -> Vec<String> {
    let mut widths: std::collections::HashMap<char, f64> = std::collections::HashMap::new();
    let mut char_w = |c: char, measure: &mut dyn FnMut(&str) -> f64| *widths.entry(c).or_insert_with(|| measure(c.encode_utf8(&mut [0u8; 4])));
    let mark_w = measure(WRAP_MARK);
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0.0;
    for c in row.chars() {
        let cw = char_w(c, &mut measure);
        let lead = if out.is_empty() { 0.0 } else { mark_w };
        if !cur.is_empty() && lead + cur_w + cw > max_w {
            let prefix = if out.is_empty() { "" } else { WRAP_MARK };
            out.push(format!("{prefix}{}", std::mem::take(&mut cur)));
            cur_w = 0.0;
        }
        cur.push(c);
        cur_w += cw;
    }
    let prefix = if out.is_empty() { "" } else { WRAP_MARK };
    if !cur.is_empty() || out.is_empty() {
        out.push(format!("{prefix}{cur}"));
    }
    out
}

/// Which rows of a line's argument area are on screen, and how far down the
/// person has seen. Approve is enabled only once every row was on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArgWindow {
    pub top: usize,
    /// One past the last row ever on screen.
    pub seen_to: usize,
}

impl ArgWindow {
    /// The rows on screen out of `total`, `visible` at a time (and they
    /// count as seen).
    pub fn show(&mut self, total: usize, visible: usize) -> std::ops::Range<usize> {
        let visible = visible.max(1);
        self.top = self.top.min(total.saturating_sub(visible));
        let end = (self.top + visible).min(total);
        self.seen_to = self.seen_to.max(end);
        self.top..end
    }
    /// Move by `rows` (down when positive).
    pub fn scroll(&mut self, rows: isize, total: usize, visible: usize) {
        self.top = if rows < 0 { self.top.saturating_sub(rows.unsigned_abs()) } else { self.top.saturating_add(rows as usize) };
        self.show(total, visible);
    }
    pub fn at_end(&self, total: usize, visible: usize) -> bool {
        self.top + visible.max(1) >= total
    }
    pub fn seen_all(&self, total: usize) -> bool {
        self.seen_to >= total
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
    /// The agent a Stop on this sheet stops: an app-conversation sheet's
    /// asking agent.
    pub fn stop_target(&self) -> Option<&str> {
        match &self.place {
            Place::AppConversation { .. } => self.open_lines().find_map(|l| l.agent.as_deref()),
            Place::SystemChat { .. } => None,
        }
    }
    pub fn batch_id(&self) -> Option<&str> {
        match &self.place {
            Place::SystemChat { batch, .. } => Some(batch),
            Place::AppConversation { .. } => None,
        }
    }
}
