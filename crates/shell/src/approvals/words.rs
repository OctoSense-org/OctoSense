//! An approval in plain words: what a Mac's sheets, first-use alert and
//! Approvals page say (`mac_view.rs`, `settings_page.rs`). They read this and
//! nothing about the decision changes: the model strings a phone draws, the
//! audit records and the tests pin stay in `sheet.rs`, `rules.rs` and
//! `consent.rs`; no gesture is made here.
//!
//! Three rules keep the words honest:
//!
//! - **The headline carries no text an agent wrote.** A sentence is built from
//!   who asked (stamped by the shell), the app that owns the tool, the tool's
//!   declared name and recipients only when they are plainly addresses. Free
//!   text (a subject, an event's title, a plan) stays in the rows, labelled.
//! - **The rows carry everything a person can read.** Every value of the call has
//!   a row, as it is, but for what is hidden ([`facts::mask_text`]) and for a
//!   plain reference ([`is_reference`]): `evt-1` under an id's name says nothing
//!   to a person, who is told so if that is all there is. Anything with words in
//!   it keeps its row, whatever it is called. Hidden characters are shown as
//!   their code point ([`visible`]). The audit keeps the whole call, as ever.
//! - **A tool without words of its own is named, not described.** An unknown
//!   tool reads "Mail wants to use its “Archive Thread” action": its declared
//!   name, and no guess at what it does.

use serde_json::Value;

use super::consent::AgentSummary;
use super::facts::{self, REDACTED};
use super::rules::{Conditions, Rule, RuleDraft, RuleOrigin, RuleScope};
use super::sheet::{app_label, persons_surface, visible, Surfaced};
use super::types::{Caller, Request};

/// One labelled row of a call's arguments. An empty label continues the row
/// above (the next recipient, the next line of a message).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub label: String,
    pub value: String,
    /// A command's line: drawn in the code face.
    pub code: bool,
}

/// What a sheet says about one call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    /// "Calendar wants Mail to send a message to ana@example.org."
    pub sentence: String,
    /// "Calendar wanted Mail to send a message to ana@example.org."
    pub past: String,
    /// "Mail: send a message to ana@example.org", a batch's item.
    pub item: String,
    /// Who asked, in the one sentence's own name: "Calendar", "The assistant".
    pub actor: String,
    /// Anything more about who asked ("For weather, in Rinx").
    pub detail: Option<String>,
    pub rows: Vec<Row>,
    /// How many values the rows hide.
    pub hidden: usize,
    /// All the call says is a reference: there is nothing to read in its rows.
    pub only_references: bool,
    /// Blue must not be what a habit's click agrees to ([`careful`]).
    pub careful: bool,
}

/// The system agent, as a sentence names it.
pub const SYSTEM_ACTOR: &str = "The assistant";

/// An app whose tools are the shell's own: nothing to name in a sentence.
fn shell_owned(app: &str) -> bool {
    matches!(app, "toolbox" | "agents" | "shell" | "system")
}

/// What a tool does, for the tools that have words: `people` is who it reaches.
fn known_action(tool: &str, people: Option<&str>) -> Option<String> {
    let to = |what: &str| match people {
        Some(people) => format!("{what} to {people}"),
        None => what.to_string(),
    };
    Some(match tool {
        "mail.send" => to("send a message"),
        "news.share" => to("share a story"),
        "calendar.add_event" => "add an event to your calendar".into(),
        "calendar.update_event" => "change an event on your calendar".into(),
        "calendar.remove_event" => "remove an event from your calendar".into(),
        _ => return None,
    })
}

/// A rule's words for a tool: the same words as [`known_action`], in general.
fn known_phrase(tool: &str) -> Option<&'static str> {
    Some(match tool {
        "mail.send" => "send messages",
        "news.share" => "share stories",
        "calendar.add_event" => "add events to your calendar",
        "calendar.update_event" => "change events on your calendar",
        "calendar.remove_event" => "remove events from your calendar",
        "terminal.run" | "dev.run" => "run commands",
        _ => return None,
    })
}

/// A tool's declared name, as words: `calendar.remove_event` → `Remove Event`.
pub fn tool_title(tool: &str) -> String {
    let name = tool.rsplit('.').next().unwrap_or(tool);
    let mut title = Vec::new();
    for word in facts::words(name) {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            title.push(first.to_uppercase().collect::<String>() + chars.as_str());
        }
    }
    if title.is_empty() {
        return visible(tool);
    }
    visible(&title.join(" "))
}

/// Whether a recipient may stand in a headline: plainly an address, in plain
/// characters and of a plain length. Anything else is "the people listed
/// below", so that a headline can never carry a lookalike or a long string.
fn plain_address(text: &str) -> bool {
    let Some((name, domain)) = text.split_once('@') else { return false };
    text.len() <= 64
        && !name.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+' | '@'))
        && !domain.contains('@')
}

/// Who a call reaches, for the headline: its recipients as the rules read them.
fn people(args: &Value) -> Option<String> {
    const LISTED: &str = "the people listed below";
    let list = match facts::recipients_checked(args) {
        Ok(list) => list,
        Err(_) => return Some(LISTED.to_string()),
    };
    if list.is_empty() {
        return None;
    }
    if !list.iter().all(|r| plain_address(r)) {
        return Some(LISTED.to_string());
    }
    Some(match list.len() {
        1 => list[0].clone(),
        2 => format!("{} and {}", list[0], list[1]),
        n => format!("{} and {} others", list[0], n - 1),
    })
}

/// How many lines a command has, for the headline.
fn command_action(args: &Value) -> String {
    let lines = args.get("command").and_then(Value::as_str).map_or(1, |c| c.split('\n').count());
    if lines > 1 {
        format!("run a {lines}-line command")
    } else {
        "run a command".to_string()
    }
}

/// What a tool's name says it undoes for good. A tool declares the rest:
/// [`Request::tool`]'s `auto_approvable` is false for what a person must always see.
const UNDOES: &[&str] = &["remove", "delete", "erase", "destroy", "wipe", "drop", "purge", "uninstall", "reset", "revoke"];
/// What a tool's name says spends money, which can't be had back either.
const SPENDS: &[&str] = &["buy", "purchase", "pay", "order", "sell", "trade", "transfer"];

/// Whether a sheet's default should be its refusal: a command, what a tool
/// declares no rule may answer, what its name says undoes something or spends
/// money, and a retry of what may already have happened. A tool the shell has no words for and that
/// declares nothing stays an ordinary request. Only which button is blue hangs on
/// this: what each button answers, and what the sheet requires, stay as they are.
fn careful(req: &Request) -> bool {
    let name = req.tool.name.rsplit('.').next().unwrap_or(&req.tool.name);
    req.tool.command
        || !req.tool.auto_approvable
        || req.context.outcome_unknown
        || facts::words(name).iter().any(|word| UNDOES.contains(&word.as_str()) || SPENDS.contains(&word.as_str()))
}

/// The sentence for one call.
pub fn said(req: &Request) -> Said {
    let owner = app_label(&req.app);
    let (actor, detail, own) = match &req.caller {
        Caller::OwnAgent { client: None } => (owner.clone(), None, true),
        Caller::OwnAgent { client: Some(c) } if persons_surface(c) => (owner.clone(), Some("For something you asked it.".to_string()), true),
        Caller::OwnAgent { client: Some(c) } => (owner.clone(), Some(format!("For {} in {owner}.", plain_client(c))), true),
        Caller::AppAgent { app } => (app_label(app), None, app_label(app) == owner),
        Caller::SystemAgent => (SYSTEM_ACTOR.to_string(), None, false),
        Caller::External { client: None } => ("An outside client".to_string(), None, false),
        Caller::External { client: Some(c) } => (format!("An outside client ({})", plain_client(c)), None, false),
    };
    let shell_tool = shell_owned(&req.app);
    let action = if req.tool.command {
        Some(command_action(&req.args))
    } else {
        known_action(&req.tool.name, people(&req.args).as_deref())
    };
    let title = tool_title(&req.tool.name);
    let body = |verb: &str| match &action {
        Some(action) if own || shell_tool => format!("{actor} {verb} to {action}."),
        Some(action) => format!("{actor} {verb} {owner} to {action}."),
        None if own => format!("{actor} {verb} to use its \u{201c}{title}\u{201d} action."),
        None if shell_tool => format!("{actor} {verb} to use the \u{201c}{title}\u{201d} action."),
        None => format!("{actor} {verb} to use the \u{201c}{title}\u{201d} action in {owner}."),
    };
    let item = match &action {
        Some(action) if shell_tool => action.clone(),
        Some(action) => format!("{owner}: {action}"),
        None if shell_tool => format!("use the \u{201c}{title}\u{201d} action"),
        None => format!("{owner}: use the \u{201c}{title}\u{201d} action"),
    };
    let (rows, hidden) = rows(req);
    let only_references = rows.is_empty() && req.args.as_object().is_some_and(|args| !args.is_empty());
    Said { sentence: body("wants"), past: body("wanted"), item: capitalized(&item), actor, detail, rows, hidden, only_references, careful: careful(req) }
}

/// A client's name, as a request context gives it: the app's own text.
fn plain_client(client: &str) -> String {
    visible(&facts::mask_text(client, None).0)
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The arguments that lead, in the order a person reads them.
const READING_FIRST: &[&str] = &["to", "cc", "bcc", "from", "title", "subject", "start", "end", "when", "location", "attachments", "files"];
/// Free text goes last, however long it is: the short facts stay together above it.
const READING_LAST: &[&str] = &["body", "message", "text", "notes"];

/// A word written the way people write it.
fn word_as_written(word: &str) -> Option<&'static str> {
    Some(match word {
        "id" => "ID",
        "url" => "URL",
        "uri" => "URI",
        "api" => "API",
        "smtp" => "SMTP",
        "imap" => "IMAP",
        "ip" => "IP",
        "http" => "HTTP",
        "https" => "HTTPS",
        "ssh" => "SSH",
        "pdf" => "PDF",
        "html" => "HTML",
        "json" => "JSON",
        "sms" => "SMS",
        "otp" => "OTP",
        "pin" => "PIN",
        "utc" => "UTC",
        "ai" => "AI",
        _ => return None,
    })
}

/// An argument's name as a label: `to` → `To`, `smtp_password` → `SMTP password`.
pub fn label_of(key: &str) -> String {
    match key.to_ascii_lowercase().as_str() {
        "cc" => return "Cc".into(),
        "bcc" => return "Bcc".into(),
        "body" => return "Message".into(),
        "start" => return "Starts".into(),
        "end" => return "Ends".into(),
        _ => {}
    }
    let words = facts::words(key);
    if words.is_empty() {
        return visible(key);
    }
    let mut out = String::new();
    for (i, word) in words.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        match word_as_written(word) {
            Some(written) => out.push_str(written),
            None if i == 0 => out.push_str(&capitalized(word)),
            None => out.push_str(word),
        }
    }
    visible(&out)
}

/// A label for a field inside another: "Order" and "Limit price" read "Order
/// limit price". A word written in capitals keeps them: "Order ID".
fn nested_label(outer: &str, inner: &str) -> String {
    let mut chars = inner.chars();
    let keeps_its_case = inner.chars().nth(1).is_some_and(char::is_uppercase);
    match chars.next() {
        Some(first) if !keeps_its_case => format!("{outer} {}{}", first.to_lowercase(), chars.as_str()),
        _ => format!("{outer} {inner}"),
    }
}

/// Whether a value is only a reference to something: an argument named as an id
/// that holds a number, or one token with a digit in it or as long as a generated
/// one ("evt-1", a UUID). A word is not ("Trash", "alice"), and neither is
/// anything with a space or an address in it: those keep their row, so that
/// nothing can be slipped past a person under an id's name.
fn is_reference(key: &str, value: &Value) -> bool {
    let token = |value: &Value| match value {
        Value::Number(_) => true,
        Value::String(text) => {
            text.len() <= 64
                && text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
                && (text.len() >= 16 || text.chars().any(|c| c.is_ascii_digit()))
        }
        _ => false,
    };
    let named_so = matches!(facts::words(key).last().map(String::as_str), Some("id" | "ids" | "uuid" | "guid"));
    named_so
        && match value {
            Value::Array(items) => !items.is_empty() && items.iter().all(token),
            other => token(other),
        }
}

#[derive(Default)]
struct Rows {
    rows: Vec<Row>,
    hidden: usize,
}

impl Rows {
    fn push(&mut self, label: &str, value: String) {
        self.rows.push(Row { label: label.to_string(), value, code: false });
    }
    fn hide(&mut self, label: &str) {
        self.hidden += 1;
        self.push(label, REDACTED.to_string());
    }
    /// A value. A list's next items and a text's next lines have no label of
    /// their own: they continue the row above.
    fn value(&mut self, label: &str, name: Option<&str>, value: &Value, declared: &[String]) {
        match value {
            Value::Null => self.push(label, "None".into()),
            Value::Bool(yes) => self.push(label, if *yes { "Yes" } else { "No" }.into()),
            Value::Number(n) => self.push(label, n.to_string()),
            Value::String(text) => {
                let (text, hid) = facts::mask_text(text, name);
                if hid {
                    self.hidden += 1;
                }
                if text.is_empty() {
                    self.push(label, "Empty".into());
                }
                for (i, line) in text.split('\n').enumerate() {
                    if text.is_empty() {
                        break;
                    }
                    self.push(if i == 0 { label } else { "" }, visible(line));
                }
            }
            Value::Array(items) if items.is_empty() => self.push(label, "None".into()),
            Value::Array(items) => {
                let scalars = items.iter().all(|i| !matches!(i, Value::Array(_) | Value::Object(_)));
                for (i, item) in items.iter().enumerate() {
                    if scalars {
                        self.value(if i == 0 { label } else { "" }, name, item, declared);
                    } else {
                        self.value(&format!("{label} {}", i + 1), name, item, declared);
                    }
                }
            }
            Value::Object(map) if map.is_empty() => self.push(label, "None".into()),
            Value::Object(map) => {
                for (key, value) in map {
                    let inner = label_of(&facts::mask_text(key, None).0);
                    let inner = if label.is_empty() { inner } else { nested_label(label, &inner) };
                    self.field(&inner, key, value, declared);
                }
            }
        }
    }
    fn field(&mut self, label: &str, key: &str, value: &Value, declared: &[String]) {
        if facts::is_secret(key, declared) && !facts::says_nothing(key, value, declared) {
            self.hide(label);
        } else if !is_reference(key, value) {
            self.value(label, Some(key), value, declared);
        }
    }
}

/// What a change sends to a website, as labelled lines of text: its message's
/// fields, then what goes in the address ("Folder in the address: Trail"). No
/// JSON, and anything that reads as a secret hidden, as on an approval's rows. A
/// part with nothing in it has no line.
pub fn change_lines(data: &Value) -> String {
    let lines: Vec<String> = change_parts(data).into_iter().map(|(label, value, in_address)| match (label.is_empty(), in_address) {
        (true, true) => format!("In the address: {value}"),
        (true, false) => value,
        (false, true) => format!("{label} in the address: {value}"),
        (false, false) => format!("{label}: {value}"),
    }).collect();
    lines.join("\n")
}

/// The same change as a Mac's review lists it: a labelled row per field, what
/// goes in the address saying so beside its value ("Folder", "Trail (in the
/// address)"), secrets hidden.
pub fn change_rows(data: &Value) -> Vec<(String, String)> {
    change_parts(data).into_iter().map(|(label, value, in_address)| {
        let label = if label.is_empty() { "Content".to_string() } else { label };
        (label, if in_address { format!("{value} (in the address)") } else { value })
    }).collect()
}

/// A change's labelled values, the message's first, each with whether it goes
/// in the address.
fn change_parts(data: &Value) -> Vec<(String, String, bool)> {
    let says_something = |part: &Value| !(part.is_null() || part.as_object().is_some_and(|fields| fields.is_empty()));
    let mut parts = Vec::new();
    for (part, in_address) in [(&data["body"], false), (&data["query"], true)] {
        if says_something(part) {
            let mut out = Rows::default();
            out.value("", None, part, &[]);
            parts.extend(out.rows.into_iter().map(|row| (row.label, row.value, in_address)));
        }
    }
    parts
}

/// Every value of a call a person can read, labelled, and how many of them are
/// hidden. An empty one reads "Empty" or "None"; only a plain reference has no row
/// ([`is_reference`]). A command tool's command comes first, one numbered row per
/// line of it (ADR 0004 §8).
pub fn rows(req: &Request) -> (Vec<Row>, usize) {
    let declared = &req.tool.secret_fields;
    let mut out = Rows::default();
    let mut skip_command = false;
    if req.tool.command && !facts::is_secret("command", declared) {
        if let Some(command) = req.args.get("command").and_then(Value::as_str) {
            let (command, hid) = facts::mask_text(command, Some("command"));
            if hid {
                out.hidden += 1;
            }
            let lines: Vec<&str> = command.split('\n').collect();
            let width = lines.len().to_string().len();
            for (i, line) in lines.iter().enumerate() {
                out.rows.push(Row { label: if i == 0 { "Command".into() } else { String::new() }, value: format!("{:>width$}  {}", i + 1, visible(line)), code: true });
            }
            skip_command = true;
        }
    }
    match &req.args {
        Value::Object(map) => {
            let lead = READING_FIRST.iter().filter_map(|k| map.get_key_value(*k));
            let rest = map.iter().filter(|(k, _)| !READING_FIRST.contains(&k.as_str()) && !READING_LAST.contains(&k.as_str()));
            let last = READING_LAST.iter().filter_map(|k| map.get_key_value(*k));
            for (key, value) in lead.chain(rest).chain(last) {
                if skip_command && key == "command" {
                    continue;
                }
                let label = label_of(&facts::mask_text(key, None).0);
                out.field(&label, key, value, declared);
            }
        }
        Value::Null => {}
        other => out.value("Details", None, other, declared),
    }
    (out.rows, out.hidden)
}

/// How long, in a button's words.
fn hours(minutes: u64) -> String {
    match minutes {
        60 => "1 Hour".to_string(),
        m if m % 60 == 0 => format!("{} Hours", m / 60),
        m => format!("{m} Minutes"),
    }
}

/// What a sheet's default button says while a rule is chosen: the rule it makes.
pub fn choice_title(draft: &RuleDraft) -> String {
    match (&draft.scope, draft.minutes) {
        (RuleScope::Everything, Some(m)) => format!("Allow Everything for {}", hours(m)),
        (RuleScope::Everything, None) => "Allow Everything".into(),
        (RuleScope::Tool(_), Some(m)) => format!("Allow for {}", hours(m)),
        (RuleScope::Tool(_), None) => {
            let c = &draft.conditions;
            if c.recipients_in_contacts {
                "Always for My Contacts".into()
            } else if c.recipients_in_thread {
                "Always for This Thread".into()
            } else if c.triggered_by_person {
                "Always When I Start It".into()
            } else {
                "Always Allow".into()
            }
        }
    }
}

/// The same choice under a sheet's "Allow:" label: the pop-up says how far.
pub fn choice_scope(draft: &RuleDraft) -> String {
    match (&draft.scope, draft.minutes) {
        (RuleScope::Everything, Some(m)) => format!("Everything for {}", hours(m)),
        (RuleScope::Everything, None) => "Everything".into(),
        (RuleScope::Tool(_), Some(m)) => format!("For {}", hours(m)),
        (RuleScope::Tool(_), None) => choice_title(draft).replace("Always Allow", "Always"),
    }
}

/// A rule's conditions, each after a comma.
fn conditions_said(c: &Conditions) -> String {
    let mut out = String::new();
    match (c.recipients_in_contacts, c.recipients_in_thread) {
        (true, true) => out.push_str(", for people in your contacts or the thread"),
        (true, false) => out.push_str(", for people in your contacts"),
        (false, true) => out.push_str(", for people in the thread"),
        _ => {}
    }
    if c.no_attachments {
        out.push_str(", with no attachments");
    }
    if c.triggered_by_person {
        out.push_str(", when you start it");
    }
    if let Some(a) = c.max_amount {
        out.push_str(&format!(", up to {a}"));
    }
    if let Some(n) = c.max_count {
        out.push_str(&format!(", at most {n} at a time"));
    }
    out
}

/// "Mail may send messages, for people in your contacts".
fn tool_rule_said(app: &str, tool: &str, c: &Conditions) -> String {
    let app = app_label(app);
    let what = match known_phrase(tool) {
        Some(phrase) => phrase.to_string(),
        None => format!("use its \u{201c}{}\u{201d} action", tool_title(tool)),
    };
    format!("{app} may {what}{}", conditions_said(c))
}

/// A rule, in Settings and on a notice: what it lets happen, on its own.
pub fn rule_said(rule: &Rule) -> String {
    match &rule.scope {
        RuleScope::Everything => format!("Everything {} asks", app_label(&rule.app)),
        RuleScope::Tool(tool) => tool_rule_said(&rule.app, tool, &rule.conditions),
    }
}

/// What choosing a rule on a sheet does, in full, before the person presses
/// Allow: its scope, its conditions, its limits and what it leaves to the person.
pub fn choice_said(draft: &RuleDraft) -> String {
    let incoming = if draft.include_incoming { "" } else { " Anything started by what someone else sent still asks you." };
    let time = |m: u64| if m == 60 { "the next hour".to_string() } else { format!("the next {m} minutes") };
    match &draft.scope {
        RuleScope::Everything => {
            let minutes = draft.minutes.unwrap_or(0);
            format!("For {}, everything {} asks is approved without you, except what always needs your approval.{incoming}", time(minutes), app_label(&draft.app))
        }
        RuleScope::Tool(tool) => {
            let mut out = format!("This makes a rule: {}", tool_rule_said(&draft.app, tool, &draft.conditions));
            if let Some(cap) = draft.daily_cap {
                out.push_str(&format!(", up to {cap} times a day"));
            }
            match draft.minutes {
                Some(m) => out.push_str(&format!(", for {}.", time(m))),
                None => out.push_str(", until you turn it off in Settings."),
            }
            out.push_str(incoming);
            out
        }
    }
}

/// A line of the recent automatic approvals: what happened, then when and who.
pub fn log_said(entry: &super::audit::Entry, now: u64) -> (String, String) {
    let app = app_label(&entry.app);
    let what = match known_phrase(&entry.tool) {
        Some(phrase) => format!("{app}: {phrase}"),
        None => format!("{app}: the \u{201c}{}\u{201d} action", tool_title(&entry.tool)),
    };
    let who = match entry.caller.as_str() {
        "own_agent" => app.clone(),
        "system_agent" => "the assistant".to_string(),
        "external" => "an outside client".to_string(),
        other => match other.split_once('/') {
            Some(("app", caller)) => app_label(caller),
            Some(("own_agent", client)) if persons_surface(client) => app.clone(),
            Some(("own_agent", client)) => format!("{} in {app}", plain_client(client)),
            Some(("external", client)) => format!("an outside client ({})", plain_client(client)),
            _ => plain_client(other),
        },
    };
    let by = match (&entry.rule, entry.by.as_str()) {
        (Some(_), _) => "your rule",
        (None, "developer_mode") => "developer mode",
        _ => "an automatic answer",
    };
    let result = if entry.result == "approved" { "Approved" } else { "Denied" };
    (what, format!("{} \u{00b7} Asked by {who} \u{00b7} {result} by {by}", capitalized(&ago_said(now, entry.ts))))
}

/// How long ago, in the words of a Mac's lists: "just now", "15 min ago",
/// "2 hr ago", "yesterday", "3 days ago".
fn ago_said(now: u64, ts: u64) -> String {
    let s = now.saturating_sub(ts);
    match s {
        0..=59 => "just now".into(),
        60..=3_599 => format!("{} min ago", s / 60),
        3_600..=86_399 => format!("{} hr ago", s / 3_600),
        86_400..=172_799 => "yesterday".into(),
        _ => format!("{} days ago", s / 86_400),
    }
}

/// What a rule lets an app do, in the few words of a list's title.
fn short_phrase(tool: &str) -> Option<&'static str> {
    Some(match tool {
        "calendar.add_event" => "add events",
        "calendar.update_event" => "change events",
        "calendar.remove_event" => "remove events",
        other => return known_phrase(other),
    })
}

/// A rule's title in a Mac's list, short enough to scan: what it lets the app
/// do on its own. Its conditions are in its details ([`rule_details`]).
pub fn rule_title(rule: &Rule) -> String {
    let app = app_label(&rule.app);
    match &rule.scope {
        RuleScope::Everything => format!("Everything {app} asks"),
        RuleScope::Tool(tool) => match short_phrase(tool) {
            Some(phrase) => format!("{app} may {phrase}"),
            None => format!("{app} may use \u{201c}{}\u{201d}", tool_title(tool)),
        },
    }
}

/// What tells a rule from another of the same title, in its list: its
/// conditions ("For people in your contacts, with no attachments").
pub fn rule_qualifier(rule: &Rule) -> String {
    match &rule.scope {
        RuleScope::Everything => String::new(),
        RuleScope::Tool(_) => match conditions_said(&rule.conditions).strip_prefix(", ") {
            Some(said) => capitalized(said),
            None => "With no conditions".to_string(),
        },
    }
}

/// The line under a rule's title in a Mac's list: that it is off, or how much
/// of its day it has used and how long it has left.
pub fn rule_state_said(rule: &Rule, now: u64) -> String {
    if !rule.enabled {
        return "Off".to_string();
    }
    let mut parts = Vec::new();
    match (&rule.scope, rule.daily_cap) {
        (RuleScope::Everything, _) => {}
        (_, Some(cap)) => parts.push(format!("{} of {cap} today", rule.used_today(now))),
        (_, None) => parts.push(format!("{} today", rule.used_today(now))),
    }
    if let Some(m) = rule.minutes_left(now) {
        parts.push(format!("{m} min left"));
    }
    parts.join(" \u{00b7} ")
}

/// The line under a rule's title on its details: on or off, and its time.
pub fn rule_on_said(rule: &Rule, now: u64) -> String {
    let state = if rule.enabled { "On" } else { "Off" };
    match rule.minutes_left(now).filter(|_| rule.enabled) {
        Some(m) => format!("{state} \u{00b7} {m} min left"),
        None => state.to_string(),
    }
}

/// A rule's details, as a Mac's details sheet lists them under its title: what
/// it covers, its limits, its time, what it answers beyond the usual and where
/// it came from. Every condition the rule holds has its row.
pub fn rule_details(rule: &Rule, now: u64) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = Vec::new();
    let mut row = |label: &str, value: String| rows.push((label.to_string(), value));
    let used = rule.used_today(now);
    match &rule.scope {
        RuleScope::Everything => {
            row("Approves", format!("Everything {} asks, except what always needs your approval", app_label(&rule.app)));
            row("Today", format!("{used} approved"));
        }
        RuleScope::Tool(_) => {
            let c = &rule.conditions;
            match (c.recipients_in_contacts, c.recipients_in_thread) {
                (true, true) => row("To", "People in your contacts or the thread".into()),
                (true, false) => row("To", "People in your contacts".into()),
                (false, true) => row("To", "People in the thread".into()),
                _ => {}
            }
            if c.no_attachments {
                row("Attachments", "None".into());
            }
            if c.triggered_by_person {
                row("Started by", "You".into());
            }
            if let Some(a) = c.max_amount {
                row("Amount", format!("Up to {a}"));
            }
            if let Some(n) = c.max_count {
                row("At a time", format!("Up to {n}"));
            }
            if c.is_empty() {
                row("Conditions", "None".into());
            }
            match rule.daily_cap {
                Some(cap) => row("Each day", format!("Up to {cap} \u{00b7} {used} so far today")),
                None => row("Each day", format!("No limit \u{00b7} {used} so far today")),
            }
        }
    }
    if let Some(m) = rule.minutes_left(now) {
        row("Ends", if m == 1 { "In 1 minute".to_string() } else { format!("In {m} minutes") });
    }
    if rule.include_incoming {
        row("Also", "What someone else sends".into());
    }
    let made = match rule.origin {
        RuleOrigin::Settings => "In Settings",
        RuleOrigin::Sheet => "When you approved a request",
    };
    row("Made", format!("{made}, {}", ago_said(now, rule.created)));
    rows
}

/// "News would like to use its agent".
pub fn consent_title(summary: &AgentSummary) -> String {
    format!("{} would like to use its assistant", summary.name)
}

pub const CONSENT_MESSAGE: &str = "It can read and use what's listed here. You can turn it off in Settings.";

/// One disclosure of the first-use alert: everything the model says, in the
/// sentence's own voice. `News's files on this device` → `Its files on this
/// device`; another app's tool in words instead of its id.
pub fn consent_line(summary: &AgentSummary, line: &str) -> String {
    let line = line.strip_suffix(" (the system toolbox)").unwrap_or(line);
    let own = format!("{}'s ", summary.name);
    if let Some(rest) = line.strip_prefix(&own) {
        return format!("Its {rest}");
    }
    if let Some(tool) = line.strip_prefix("Another app's tool: ") {
        let app = tool.split('.').next().unwrap_or(tool);
        return match known_phrase(tool) {
            Some(phrase) => format!("{}: {phrase}", app_label(app)),
            None => format!("{}: the \u{201c}{}\u{201d} action", app_label(app), tool_title(tool)),
        };
    }
    // The page's name, as a Mac titles it.
    capitalized(&visible(line)).replace("AI providers", "AI Providers")
}

/// Why a line needs the person, when there is more to say than that no rule
/// answered it.
pub fn surfaced_said(surfaced: &Surfaced) -> &'static str {
    match surfaced {
        Surfaced::NotAutoApprovable => "This always needs your approval. No rule can answer it.",
        Surfaced::OutcomeUnknown => "An earlier try may already have happened. It is never retried without you.",
        Surfaced::IncomingContent => "Started by something someone else sent, so your rules didn't answer it.",
        Surfaced::External => "Asked by an outside client.",
        Surfaced::NoRule => "",
    }
}

/// Who a question is for, when there is more to say than which agent asks.
pub fn question_said(q: &crate::questions::Request) -> Option<String> {
    use crate::questions::Origin;
    match (&q.origin, &q.client) {
        (Origin::SystemAgent, _) => Some("For the assistant.".to_string()),
        (_, Some(client)) if persons_surface(client) => Some("For something you asked it.".to_string()),
        (_, Some(client)) => Some(format!("For {} in {}.", plain_client(client), app_label(&q.app))),
        _ => None,
    }
}

/// "the News agent": an agent's name in a sentence, for any app's name.
pub fn agent_named(app: &str) -> String {
    format!("the {} assistant", app_label(app))
}

/// "Stop News Assistant".
pub fn stop_title(app: &str) -> String {
    format!("Stop {} Assistant", app_label(app))
}

/// [`stop_title`] where the name does not fit: the sentence above it says whose.
pub const STOP_SHORT: &str = "Stop Assistant";

/// What a Mac's notification says for a sheet's request elsewhere.
pub fn waiting_said(req: &Request) -> (String, String) {
    let said = said(req);
    (format!("Waiting for {}", app_label(&req.app)), format!("{} Open {} to confirm it there.", said.sentence, app_label(&req.app)))
}
