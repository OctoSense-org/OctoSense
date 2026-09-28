//! Standing approvals (ADR 0004 §8): rules the person sets in advance,
//! keyed to (owning app, tool) whoever calls, checked mechanically on the
//! exact arguments.
//!
//! - **Scope:** one tool of one app ([`RuleScope::Tool`]), or everything
//!   one app asks ([`RuleScope::Everything`]), which is always time-boxed
//!   to at most [`MAX_EVERYTHING_MINUTES`]: there is no "everything,
//!   forever" rule.
//! - **Conditions** ([`Conditions`]): recipients in contacts or in the
//!   thread, no attachments, triggered by the person, amount and count
//!   limits; a daily cap ([`Rule::daily_cap`], per UTC day).
//! - **Never:** `auto_approvable: false` tools and `outcome_unknown` calls
//!   (the router checks those before any rule); runs started by incoming
//!   content unless the rule opts in ([`Rule::include_incoming`]).
//! - **Only the person creates one:** [`RuleStore::create`] takes a
//!   [`PersonGesture`], which only the Settings page and the shell-drawn
//!   sheet construct (a test scans the sources). No agent or app can.
//! - **One tap turns every rule off** ([`RuleStore::all_off`]).
//! - **Persisted per OctoSense home** in [`RULES_FILE`], owner-only.

use super::facts;
use super::types::{Request, RuleId, Trigger};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Relative to the OctoSense home.
pub const RULES_FILE: &str = "approvals/rules.json";
/// The longest time-boxed "approve everything this app asks".
pub const MAX_EVERYTHING_MINUTES: u64 = 60;
/// What a rule made from a sheet's "always for …" is capped at per day.
pub const DEFAULT_DAILY_CAP: u32 = 20;
const DAY_S: u64 = 86_400;

/// Proof that the person, not an agent or app, asked for a rule. Only the
/// shell's Settings page and the shell-drawn sheet construct one.
#[derive(Debug)]
pub struct PersonGesture {
    origin: RuleOrigin,
}

impl PersonGesture {
    /// A tap on Settings → Assistant → Approvals.
    pub(crate) fn settings_tap() -> PersonGesture {
        PersonGesture { origin: RuleOrigin::Settings }
    }
    /// A tap on a shell-drawn approval sheet's "always for …".
    pub(crate) fn sheet_tap() -> PersonGesture {
        PersonGesture { origin: RuleOrigin::Sheet }
    }
    pub fn origin(&self) -> RuleOrigin {
        self.origin
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleOrigin {
    Settings,
    Sheet,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleScope {
    Tool(String),
    /// Everything the app asks, time-boxed.
    Everything,
}

/// Every condition set must hold. A fact the call does not carry fails the
/// condition that needs it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Conditions {
    /// Every recipient is in the person's contacts (or, with
    /// `recipients_in_thread` also set, in contacts or the thread).
    #[serde(default)]
    pub recipients_in_contacts: bool,
    /// Every recipient is in the thread the run is about.
    #[serde(default)]
    pub recipients_in_thread: bool,
    #[serde(default)]
    pub no_attachments: bool,
    /// The run was started by the person.
    #[serde(default)]
    pub triggered_by_person: bool,
    #[serde(default)]
    pub max_amount: Option<f64>,
    #[serde(default)]
    pub max_count: Option<u64>,
}

impl Conditions {
    pub fn is_empty(&self) -> bool {
        *self == Conditions::default()
    }
    /// For Settings and sheets: "people in my contacts, no attachments".
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        match (self.recipients_in_contacts, self.recipients_in_thread) {
            (true, true) => parts.push("people in my contacts or the thread".to_string()),
            (true, false) => parts.push("people in my contacts".into()),
            (false, true) => parts.push("people in the thread".into()),
            _ => {}
        }
        if self.no_attachments {
            parts.push("no attachments".into());
        }
        if self.triggered_by_person {
            parts.push("when I start it".into());
        }
        if let Some(a) = self.max_amount {
            parts.push(format!("up to {a}"));
        }
        if let Some(c) = self.max_count {
            parts.push(format!("at most {c} at a time"));
        }
        parts.join(", ")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub id: RuleId,
    /// The owning app.
    pub app: String,
    pub scope: RuleScope,
    #[serde(default)]
    pub conditions: Conditions,
    #[serde(default)]
    pub daily_cap: Option<u32>,
    /// Unix seconds; required for [`RuleScope::Everything`].
    #[serde(default)]
    pub expires: Option<u64>,
    /// Also answer runs started by incoming content (off by default).
    #[serde(default)]
    pub include_incoming: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub created: u64,
    pub origin: RuleOrigin,
    /// Uses on `used_day` (UTC day number).
    #[serde(default)]
    pub used_day: u64,
    #[serde(default)]
    pub used: u32,
}

fn yes() -> bool {
    true
}

/// What the person asks for; [`RuleStore::create`] validates it.
#[derive(Clone, Debug, PartialEq)]
pub struct RuleDraft {
    pub app: String,
    pub scope: RuleScope,
    pub conditions: Conditions,
    pub daily_cap: Option<u32>,
    /// Minutes from now; required for [`RuleScope::Everything`].
    pub minutes: Option<u64>,
    pub include_incoming: bool,
}

impl RuleDraft {
    pub fn tool(app: &str, tool: &str, conditions: Conditions) -> RuleDraft {
        RuleDraft { app: app.into(), scope: RuleScope::Tool(tool.into()), conditions, daily_cap: Some(DEFAULT_DAILY_CAP), minutes: None, include_incoming: false }
    }
    pub fn everything(app: &str, minutes: u64) -> RuleDraft {
        RuleDraft { app: app.into(), scope: RuleScope::Everything, conditions: Conditions::default(), daily_cap: None, minutes: Some(minutes), include_incoming: false }
    }
    pub fn for_minutes(mut self, minutes: u64) -> RuleDraft {
        self.minutes = Some(minutes);
        self
    }
}

/// Why a rule did not answer a request (for tests and the audit).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Miss {
    OtherApp,
    OtherTool,
    Off,
    Expired,
    Incoming,
    RecipientsNotKnown,
    NoRecipients,
    Attachments,
    NotByPerson,
    Amount,
    Count,
    CapReached,
}

/// The person's contacts, for "recipients in contacts".
pub trait Contacts: Send {
    fn contains(&self, address: &str) -> bool;
}

/// Nobody is in contacts: the contacts condition never holds until the
/// shell has a contacts source.
#[derive(Default)]
pub struct NoContacts;

impl Contacts for NoContacts {
    fn contains(&self, _address: &str) -> bool {
        false
    }
}

/// A fixed list (tests, and a host-provided snapshot).
#[derive(Default, Clone)]
pub struct ContactList(pub Vec<String>);

impl Contacts for ContactList {
    fn contains(&self, address: &str) -> bool {
        self.0.iter().any(|c| c.eq_ignore_ascii_case(address))
    }
}

pub fn day_of(now: u64) -> u64 {
    now / DAY_S
}

impl Rule {
    pub fn is_everything(&self) -> bool {
        self.scope == RuleScope::Everything
    }
    pub fn expired(&self, now: u64) -> bool {
        self.expires.is_some_and(|e| now >= e)
    }
    /// Uses today.
    pub fn used_today(&self, now: u64) -> u32 {
        if self.used_day == day_of(now) {
            self.used
        } else {
            0
        }
    }
    pub fn minutes_left(&self, now: u64) -> Option<u64> {
        self.expires.map(|e| e.saturating_sub(now).div_ceil(60))
    }

    /// Whether this rule answers `req`, on its exact arguments. The router
    /// has already excluded `auto_approvable: false` and unknown outcomes.
    pub fn check(&self, req: &Request, contacts: &dyn Contacts, now: u64) -> Result<(), Miss> {
        if self.app != req.app {
            return Err(Miss::OtherApp);
        }
        if let RuleScope::Tool(t) = &self.scope {
            if *t != req.tool.name {
                return Err(Miss::OtherTool);
            }
        }
        if !self.enabled {
            return Err(Miss::Off);
        }
        if self.expired(now) || (self.is_everything() && self.expires.is_none()) {
            return Err(Miss::Expired);
        }
        if req.context.trigger.excluded_by_default() && !self.include_incoming {
            return Err(Miss::Incoming);
        }
        let c = &self.conditions;
        if c.recipients_in_contacts || c.recipients_in_thread {
            let recipients = facts::recipients(&req.args);
            if recipients.is_empty() {
                return Err(Miss::NoRecipients);
            }
            let in_thread = |r: &String| req.context.thread.iter().any(|t| t.eq_ignore_ascii_case(r));
            let ok = recipients.iter().all(|r| (c.recipients_in_contacts && contacts.contains(r)) || (c.recipients_in_thread && in_thread(r)));
            if !ok {
                return Err(Miss::RecipientsNotKnown);
            }
        }
        if c.no_attachments && facts::has_attachments(&req.args) {
            return Err(Miss::Attachments);
        }
        if c.triggered_by_person && req.context.trigger != Trigger::Person {
            return Err(Miss::NotByPerson);
        }
        if let Some(max) = c.max_amount {
            match facts::amount(&req.args) {
                Some(a) if a <= max => {}
                _ => return Err(Miss::Amount),
            }
        }
        if let Some(max) = c.max_count {
            if facts::count(&req.args) > max {
                return Err(Miss::Count);
            }
        }
        if let Some(cap) = self.daily_cap {
            if self.used_today(now) >= cap {
                return Err(Miss::CapReached);
            }
        }
        Ok(())
    }

    /// "Mail · mail.send · people in my contacts" for Settings.
    pub fn describe(&self) -> String {
        match &self.scope {
            RuleScope::Everything => format!("Everything {} asks", super::sheet::app_label(&self.app)),
            RuleScope::Tool(t) => {
                let app = super::sheet::app_label(&self.app);
                let c = self.conditions.describe();
                if c.is_empty() {
                    format!("{app} · {t}")
                } else {
                    format!("{app} · {t} · {c}")
                }
            }
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RulesFile {
    schema: u32,
    next_id: u64,
    rules: Vec<Rule>,
}

/// The rules of one OctoSense home.
#[derive(Debug)]
pub struct RuleStore {
    path: Option<PathBuf>,
    next_id: u64,
    rules: Vec<Rule>,
}

impl RuleStore {
    /// In memory only (tests).
    pub fn memory() -> RuleStore {
        RuleStore { path: None, next_id: 1, rules: Vec::new() }
    }
    /// The home's rules; a missing or unreadable file is no rules.
    pub fn in_home(home: &Path) -> RuleStore {
        let path = home.join(RULES_FILE);
        let file: RulesFile = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        RuleStore { path: Some(path), next_id: file.next_id.max(1), rules: file.rules }
    }
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }
    pub fn get(&self, id: &RuleId) -> Option<&Rule> {
        self.rules.iter().find(|r| r.id == *id)
    }

    /// Only the person creates a rule.
    pub fn create(&mut self, gesture: &PersonGesture, draft: RuleDraft, now: u64) -> Result<RuleId, String> {
        let expires = match (&draft.scope, draft.minutes) {
            (RuleScope::Everything, None) => return Err("there is no \u{201c}everything, forever\u{201d} rule: choose how many minutes".into()),
            (RuleScope::Everything, Some(m)) if m == 0 || m > MAX_EVERYTHING_MINUTES => {
                return Err(format!("everything an app asks can be approved for {MAX_EVERYTHING_MINUTES} minutes at most"))
            }
            (_, Some(0)) => return Err("a rule needs at least one minute".into()),
            (_, m) => m.map(|m| now + m * 60),
        };
        if draft.app.trim().is_empty() {
            return Err("a rule needs the app that owns the tool".into());
        }
        if let RuleScope::Tool(t) = &draft.scope {
            if t.trim().is_empty() {
                return Err("a rule needs a tool".into());
            }
        }
        let id = RuleId(format!("r{}", self.next_id));
        self.next_id += 1;
        // A new time-boxed "everything" replaces the app's previous one.
        if draft.scope == RuleScope::Everything {
            self.rules.retain(|r| !(r.app == draft.app && r.is_everything()));
        }
        self.rules.push(Rule {
            id: id.clone(),
            app: draft.app,
            scope: draft.scope,
            conditions: draft.conditions,
            daily_cap: draft.daily_cap,
            expires,
            include_incoming: draft.include_incoming,
            enabled: true,
            created: now,
            origin: gesture.origin(),
            used_day: day_of(now),
            used: 0,
        });
        self.save();
        Ok(id)
    }

    /// The first rule that answers `req`. Narrow rules first, then the
    /// app's time-boxed "everything".
    pub fn find(&self, req: &Request, contacts: &dyn Contacts, now: u64) -> Option<RuleId> {
        let narrow = self.rules.iter().filter(|r| !r.is_everything());
        let broad = self.rules.iter().filter(|r| r.is_everything());
        narrow.chain(broad).find(|r| r.check(req, contacts, now).is_ok()).map(|r| r.id.clone())
    }

    /// Count one automatic approval against the rule's daily cap.
    pub fn record_use(&mut self, id: &RuleId, now: u64) {
        let today = day_of(now);
        if let Some(r) = self.rules.iter_mut().find(|r| r.id == *id) {
            if r.used_day != today {
                r.used_day = today;
                r.used = 0;
            }
            r.used += 1;
        }
        self.save();
    }

    /// Removing a rule needs no gesture: turning approvals off is always
    /// allowed.
    pub fn delete(&mut self, id: &RuleId) -> bool {
        let before = self.rules.len();
        self.rules.retain(|r| r.id != *id);
        let changed = self.rules.len() != before;
        if changed {
            self.save();
        }
        changed
    }

    /// Turn a rule back on: the person again.
    pub fn enable(&mut self, _gesture: &PersonGesture, id: &RuleId) -> bool {
        let Some(r) = self.rules.iter_mut().find(|r| r.id == *id) else { return false };
        r.enabled = true;
        self.save();
        true
    }

    /// One tap: every rule off. Time-boxed "everything" rules end (they are
    /// never turned back on); the others stay listed, off.
    pub fn all_off(&mut self) -> usize {
        let n = self.rules.iter().filter(|r| r.enabled).count();
        self.rules.retain(|r| !r.is_everything());
        for r in &mut self.rules {
            r.enabled = false;
        }
        self.save();
        n
    }

    /// Drop expired rules; returns them (so the person is told).
    pub fn expire(&mut self, now: u64) -> Vec<Rule> {
        let (gone, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.rules).into_iter().partition(|r| r.expired(now));
        self.rules = keep;
        if !gone.is_empty() {
            self.save();
        }
        gone
    }

    /// The time-boxed "everything" rules in force: the visible indicator.
    pub fn active_everything(&self, now: u64) -> Vec<&Rule> {
        self.rules.iter().filter(|r| r.is_everything() && r.enabled && !r.expired(now)).collect()
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        let file = RulesFile { schema: 1, next_id: self.next_id, rules: self.rules.clone() };
        let Ok(bytes) = serde_json::to_vec_pretty(&file) else { return };
        if let Err(e) = super::write_private(path, &bytes) {
            eprintln!("approvals: could not save the rules: {e}");
        }
    }
}
