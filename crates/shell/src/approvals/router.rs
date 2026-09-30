//! The approval router (ADR 0004 §8, §13): the shell's one place that
//! answers approval requests, on their exact arguments.
//!
//! For each request, in this order:
//!
//! 0. **External clients' turns** ([`Caller::External`]) are not the
//!    shell's: the router holds nothing and answers nothing
//!    ([`Route::LeftToClient`]); the client that started the turn does.
//! 1. **Developer mode** ([`DevModeHooks`], the shell's `dev_mode`): it
//!    approves everything of the apps it covers, `auto_approvable: false`
//!    and `confirm: app` included, never for an external client.
//! 2. **`confirm: app`** tools go to the owning app's own sheet, with the
//!    caller ([`AppConfirm`]); no rule answers them. An app that is not
//!    running gets [`Router::app_wait_s`] to register, else the call is
//!    refused visibly.
//! 3. **`auto_approvable: false`**, **`outcome_unknown`** and external
//!    clients' calls always go to the person.
//! 4. **Standing rules** ([`RuleStore`]), which skip runs started by
//!    incoming content unless a rule opts in.
//! 5. Otherwise a **sheet**: one per request, or the batched sheet of one
//!    system-agent request in the system chat.
//!
//! Every decision reaches the relay once ([`ApprovalRelay`]) and the audit
//! ([`AuditLog`]); every automatic one is also a notice for the person.
//!
//! **Deadline.** A request the shell holds (a sheet line, a `confirm: app`
//! request on the owning app's sheet) that nobody answers within
//! [`Router::sheet_expiry_s`] (`host_tools::prompt_deadline`, 10 min unless
//! `OCTOSENSE_PROMPT_DEADLINE_SECS` says otherwise) expires: denied with the
//! reason ("expired: no answer in 10 min"), never approved, audited `by:
//! expired`, and kept visible as an [`Expired`] record and a notice until
//! the person dismisses it. External connections' requests never expire
//! here: they are not the shell's (octos#2624).

use super::audit::{AuditLog, Entry};
use super::dev_hooks::{DevKind, DevModeHooks};
use super::facts;
use super::relay::{ApprovalIntake, ApprovalRelay};
use super::contacts::{ContactsGate, ContactsSource};
use super::rules::{ApprovalGesture, RuleDraft, RuleStore};
use super::sheet::{app_label, caller_label, Answer, Line, Place, Sheet, Surfaced};
use super::types::{Caller, Confirm, Connection, Decision, Request, RequestContext, RequestId, RuleId, ToolSpec};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// Who answered automatically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutoBy {
    DeveloperMode,
    Rule(RuleId),
}

/// What the router did with a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Approved(AutoBy),
    /// On the shell's sheet (its id).
    Sheet(u64),
    /// On the owning app's own sheet.
    HandedToApp,
    /// The owning app is not running; refused at `until` unless it comes.
    WaitingForApp { until: u64 },
    Refused(String),
    /// An external client's turn ([`Caller::External`]): nothing was held
    /// or answered; that client answers it. No relay decision is sent.
    LeftToClient(String),
}

/// A `confirm: app` request, as the owning app's own sheet gets it.
#[derive(Clone, Debug, PartialEq)]
pub struct AppConfirmRequest {
    pub id: RequestId,
    pub tool: String,
    /// The exact arguments (the owning app is their owner).
    pub args: Value,
    pub caller: Caller,
    /// "Calendar's agent", for the app's sheet.
    pub caller_label: String,
    pub context_id: Option<String>,
}

/// The owning app's own confirmation sheet, registered by the app module
/// (Rinx's send sheet). It answers with [`Router::app_confirm_answered`]
/// (the global [`super::app_confirm_answered`]).
pub trait AppConfirm: Send {
    fn confirm(&mut self, request: &AppConfirmRequest);
    /// `id` is no longer the app's to answer (it expired and was denied):
    /// its sheet shows why; a later answer is refused.
    fn withdrawn(&mut self, _id: &RequestId, _reason: &str) {}
}

/// A request that expired unanswered: withdrawn from pending, kept for the
/// surfaces ("Expired: no answer in 10 min") until the person dismisses it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expired {
    pub id: RequestId,
    /// The owning app.
    pub app: String,
    /// "Mail · mail.send".
    pub heading: String,
    /// "Calendar's agent".
    pub caller: String,
    /// "no answer in 10 min".
    pub reason: String,
    /// Unix seconds.
    pub at: u64,
}

impl Expired {
    /// "Expired: no answer in 10 min".
    pub fn status(&self) -> String {
        format!("Expired: {}", self.reason)
    }
}

/// How many expired records the surfaces keep.
const EXPIRED_KEPT: usize = 16;

/// Something the person is told (the shell shows it as a notification).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
}

pub struct Router {
    pub rules: RuleStore,
    pub audit: AuditLog,
    hooks: Box<dyn DevModeHooks>,
    /// "Recipients in my contacts", behind the person's consent.
    contacts: ContactsGate,
    relay: Box<dyn ApprovalRelay>,
    /// Every request not yet decided.
    pending: BTreeMap<RequestId, Request>,
    sheets: Vec<Sheet>,
    next_sheet: u64,
    app_confirms: HashMap<String, Box<dyn AppConfirm>>,
    /// `confirm: app` requests on an app's sheet.
    at_app: BTreeMap<RequestId, String>,
    /// `confirm: app` requests waiting for their app, with a deadline.
    waiting: BTreeMap<RequestId, u64>,
    notices: Vec<Notice>,
    /// How long a `confirm: app` call waits for its app (0: refused at once).
    pub app_wait_s: u64,
    /// A request nobody answers is denied after this (ADR 0002 §10, ADR
    /// 0004 §8): the prompt deadline.
    pub sheet_expiry_s: u64,
    /// Requests that expired, newest last, until dismissed.
    expired: Vec<Expired>,
    generation: u64,
    next_request: u64,
}

impl Router {
    pub fn new(rules: RuleStore, audit: AuditLog, hooks: Box<dyn DevModeHooks>, contacts: ContactsGate, relay: Box<dyn ApprovalRelay>) -> Router {
        Router {
            rules,
            audit,
            hooks,
            contacts,
            relay,
            pending: BTreeMap::new(),
            sheets: Vec::new(),
            next_sheet: 1,
            app_confirms: HashMap::new(),
            at_app: BTreeMap::new(),
            waiting: BTreeMap::new(),
            notices: Vec::new(),
            app_wait_s: 120,
            sheet_expiry_s: crate::ai_host::app_peers::host_tools::prompt_deadline().as_secs(),
            expired: Vec::new(),
            generation: 0,
            next_request: 1,
        }
    }

    /// Replace the relay (the octos#2567 relay installs itself); returns
    /// the old one so queued decisions can be handed over.
    pub fn set_relay(&mut self, relay: Box<dyn ApprovalRelay>) -> Box<dyn ApprovalRelay> {
        std::mem::replace(&mut self.relay, relay)
    }
    pub fn set_hooks(&mut self, hooks: Box<dyn DevModeHooks>) {
        self.hooks = hooks;
    }
    /// Replace where contacts come from; the person's consent stays.
    pub fn set_contacts(&mut self, contacts: Box<dyn ContactsSource>) {
        self.contacts.set_source(contacts);
    }
    pub fn contacts(&self) -> &ContactsGate {
        &self.contacts
    }
    /// Settings' "Use my contacts in approval rules".
    pub fn contacts_mut(&mut self) -> &mut ContactsGate {
        &mut self.contacts
    }
    pub fn hooks(&self) -> &dyn DevModeHooks {
        &*self.hooks
    }

    /// Bumped on every visible change (sheets, rules, indicator).
    pub fn generation(&self) -> u64 {
        self.generation
    }
    fn changed(&mut self) {
        self.generation += 1;
    }
    pub fn sheets(&self) -> &[Sheet] {
        &self.sheets
    }
    /// The sheet the person sees first.
    pub fn front_sheet(&self) -> Option<&Sheet> {
        self.sheets.first()
    }
    pub fn pending(&self) -> usize {
        self.pending.len()
    }
    pub fn is_pending(&self, id: &RequestId) -> bool {
        self.pending.contains_key(id)
    }
    /// A request not decided yet: its app, tool, exact arguments, caller
    /// and context (trigger, connection).
    pub fn pending_request(&self, id: &RequestId) -> Option<&Request> {
        self.pending.get(id)
    }
    /// What expired unanswered and is not dismissed yet, oldest first.
    pub fn expired(&self) -> &[Expired] {
        &self.expired
    }
    /// The person saw an expired record.
    pub fn dismiss_expired(&mut self, id: &RequestId) {
        let before = self.expired.len();
        self.expired.retain(|e| e.id != *id);
        if self.expired.len() != before {
            self.changed();
        }
    }
    /// The Stop of an app agent's turn (the app conversation's surface):
    /// what `app`'s agent asks and the shell holds is denied, with why.
    /// The requests denied.
    pub fn stop_agent(&mut self, app: &str, now: u64) -> Vec<RequestId> {
        let ids: Vec<RequestId> = self
            .pending
            .values()
            .filter(|r| r.context.connection == Connection::Host && agent_of(r).as_deref() == Some(app))
            .map(|r| r.id.clone())
            .collect();
        for id in &ids {
            self.decide(id, Decision::Deny, "person", None, "the person stopped the agent's turn", now);
            if let Some(line) = self.sheets.iter_mut().flat_map(|s| s.lines.iter_mut()).find(|l| l.request == *id) {
                line.answer = Some(super::sheet::Answer::Deny);
            }
        }
        self.sheets.retain(|s| !s.done());
        self.changed();
        ids
    }
    /// `id`'s turn ended before anyone answered it (the app's own Stop,
    /// an interrupt, a failed turn): the kernel dropped the request, so it
    /// is withdrawn from its sheet, never approved, audited `by: withdrawn`
    /// with `reason`. False when it was not pending.
    pub fn withdraw(&mut self, id: &RequestId, reason: &str, now: u64) -> bool {
        if !self.pending.contains_key(id) {
            return false;
        }
        self.decide(id, Decision::Deny, "withdrawn", None, reason, now);
        for sheet in &mut self.sheets {
            if let Some(line) = sheet.lines.iter_mut().find(|l| l.request == *id) {
                line.answer = Some(super::sheet::Answer::Deny);
            }
        }
        self.sheets.retain(|s| !s.done());
        self.changed();
        true
    }
    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }
    fn notice(&mut self, title: impl Into<String>, body: impl Into<String>) {
        self.notices.push(Notice { title: title.into(), body: body.into() });
    }

    /// Route one request. The decision may be given before this returns.
    pub fn request(&mut self, req: Request, now: u64) -> Route {
        if req.caller.is_external() {
            // Not the shell's to answer, by any path: no developer mode, no
            // rule, no sheet, and no decision on the relay.
            self.notice(
                format!("Outside request: {} \u{00b7} {}", app_label(&req.app), req.tool.name),
                "An outside client's turn asked for this; that client answers it.",
            );
            self.changed();
            return Route::LeftToClient("an external client's approval is answered by that client".into());
        }
        if self.pending.contains_key(&req.id) {
            // octos approvals are once-only; a repeat is not a second ask.
            return Route::Refused(format!("request {} is already pending", req.id));
        }
        self.pending.insert(req.id.clone(), req.clone());
        let host = req.context.connection == Connection::Host && !req.caller.is_external();
        let app = req.app.as_str();

        // 1. Developer mode, before anything else.
        let dev = if !host {
            // Developer mode never answers for an external client.
            false
        } else if req.tool.command {
            self.hooks.approves_command(app, req.context.connection)
        } else if req.tool.confirm == Confirm::App {
            host && self.hooks.overrides_app_confirm(app)
        } else {
            self.hooks.answers_approval(app, req.tool.auto_approvable, req.context.connection)
        };
        if dev {
            let kind = if req.tool.command {
                DevKind::Command
            } else if req.tool.confirm == Confirm::App {
                DevKind::AppConfirm
            } else {
                DevKind::HostConfirm
            };
            self.hooks.audit_auto_approval(app, &req.tool.name, &req.args.to_string(), &req.caller.as_audit(), kind);
            self.decide(&req.id, Decision::ApproveOnce, "developer_mode", None, "developer mode", now);
            return Route::Approved(AutoBy::DeveloperMode);
        }

        // 2. `confirm: app`: the owning app's own sheet.
        if req.tool.confirm == Confirm::App {
            return self.hand_to_app(req, now);
        }

        // 3. Always the person.
        let always_person = if !req.tool.auto_approvable {
            Some(Surfaced::NotAutoApprovable)
        } else if req.context.outcome_unknown {
            Some(Surfaced::OutcomeUnknown)
        } else if !host {
            Some(Surfaced::External)
        } else {
            None
        };

        // 4. Standing rules.
        let surfaced = match always_person {
            Some(s) => s,
            None => match self.rules.find(&req, &self.contacts, now) {
                Some(rule) => {
                    self.rules.record_use(&rule, now);
                    self.decide(&req.id, Decision::ApproveByRule(rule.clone()), "rule", Some(&rule), "standing rule", now);
                    self.changed();
                    return Route::Approved(AutoBy::Rule(rule));
                }
                None if req.context.trigger.excluded_by_default() => Surfaced::IncomingContent,
                None => Surfaced::NoRule,
            },
        };

        // 5. A sheet.
        Route::Sheet(self.surface(&req, surfaced, now))
    }

    fn surface(&mut self, req: &Request, surfaced: Surfaced, now: u64) -> u64 {
        let line = Line::for_request(req, surfaced, &self.contacts);
        let batch = req.context.batch.clone();
        self.notice(format!("Needs you: {}", line.heading()), format!("{} asks. Open the sheet to approve or deny.", line.caller));
        self.changed();
        if let Some(b) = &batch {
            if let Some(sheet) = self.sheets.iter_mut().find(|s| s.batch_id() == Some(&b.id) && !s.done()) {
                sheet.lines.push(line);
                return sheet.id;
            }
        }
        let id = self.next_sheet;
        self.next_sheet += 1;
        let place = match batch {
            Some(b) => Place::SystemChat { batch: b.id, plan: b.plan },
            None => Place::AppConversation { app: req.app.clone() },
        };
        self.sheets.push(Sheet { id, place, lines: vec![line], opened: now });
        id
    }

    fn hand_to_app(&mut self, req: Request, now: u64) -> Route {
        if let Some(handler) = self.app_confirms.get_mut(&req.app) {
            handler.confirm(&app_request(&req));
            self.at_app.insert(req.id.clone(), req.app.clone());
            return Route::HandedToApp;
        }
        if self.app_wait_s == 0 {
            let reason = format!("{} isn't running", app_label(&req.app));
            self.refuse_visibly(&req.id, &reason, now);
            return Route::Refused(reason);
        }
        let until = now + self.app_wait_s;
        self.waiting.insert(req.id.clone(), until);
        self.notice(format!("Waiting for {}", app_label(&req.app)), format!("{} wants {}; open {} to confirm it there.", caller_label(&req.app, &req.caller), req.tool.name, app_label(&req.app)));
        self.changed();
        Route::WaitingForApp { until }
    }

    fn refuse_visibly(&mut self, id: &RequestId, reason: &str, now: u64) {
        let Some(req) = self.pending.get(id).cloned() else { return };
        self.decide(id, Decision::Deny, "refused", None, reason, now);
        self.notice(format!("Refused: {} \u{00b7} {}", app_label(&req.app), req.tool.name), reason.to_string());
        self.changed();
    }

    /// The owning app registers its own sheet (at its module's start).
    /// Requests waiting for it are handed over now.
    pub fn register_app_confirm(&mut self, app: &str, mut handler: Box<dyn AppConfirm>) {
        let waiting: Vec<RequestId> = self.waiting.keys().filter(|id| self.pending.get(*id).is_some_and(|r| r.app == app)).cloned().collect();
        for id in waiting {
            self.waiting.remove(&id);
            if let Some(req) = self.pending.get(&id) {
                handler.confirm(&app_request(req));
                self.at_app.insert(id, app.to_string());
            }
        }
        self.app_confirms.insert(app.to_string(), handler);
        self.changed();
    }

    /// The app went away: what was on its sheet is refused visibly.
    pub fn unregister_app_confirm(&mut self, app: &str, now: u64) {
        self.app_confirms.remove(app);
        let on_sheet: Vec<RequestId> = self.at_app.iter().filter(|(_, a)| a.as_str() == app).map(|(id, _)| id.clone()).collect();
        for id in on_sheet {
            self.at_app.remove(&id);
            self.refuse_visibly(&id, &format!("{} closed before it was confirmed", app_label(app)), now);
        }
    }

    /// The owning app's sheet answered.
    pub fn app_confirm_answered(&mut self, id: &RequestId, approved: bool, reason: &str, now: u64) -> Result<(), String> {
        if self.at_app.remove(id).is_none() {
            return Err(format!("{id} is not on an app's sheet"));
        }
        let decision = if approved { Decision::ApproveOnce } else { Decision::Deny };
        self.decide(id, decision, "app_sheet", None, reason, now);
        self.changed();
        Ok(())
    }

    /// The person answered one line of a shell-drawn sheet.
    pub fn answer(&mut self, sheet: u64, request: &RequestId, answer: Answer, gesture: &ApprovalGesture, now: u64) -> Result<Option<RuleId>, String> {
        let s = self.sheets.iter().position(|s| s.id == sheet).ok_or("that sheet is closed")?;
        let l = self.sheets[s].lines.iter().position(|l| l.request == *request).ok_or("that line is not on the sheet")?;
        if self.sheets[s].lines[l].answer.is_some() {
            return Err("already answered".into());
        }
        let mut made = None;
        match answer {
            Answer::Once => self.decide(request, Decision::ApproveOnce, "person", None, "approved on the sheet", now),
            Answer::Deny => self.decide(request, Decision::Deny, "person", None, "denied on the sheet", now),
            Answer::Always(i) => {
                let choice = self.sheets[s].lines[l].always.get(i).cloned().ok_or("no such choice")?;
                let rule = self.rules.create(gesture, choice.draft, now)?;
                self.rules.record_use(&rule, now);
                self.decide(request, Decision::ApproveByRule(rule.clone()), "person", Some(&rule), &format!("approved and made a rule: {}", choice.label), now);
                made = Some(rule);
            }
        }
        self.sheets[s].lines[l].answer = Some(answer);
        if made.is_some() {
            self.reconsider(now);
        }
        self.sheets.retain(|s| !s.done());
        self.changed();
        Ok(made)
    }

    /// A new rule may answer lines still open: the ones no rule answered.
    fn reconsider(&mut self, now: u64) {
        let open: Vec<(u64, RequestId)> = self
            .sheets
            .iter()
            .flat_map(|s| s.open_lines().filter(|l| l.surfaced.rules_could_answer()).map(move |l| (s.id, l.request.clone())))
            .collect();
        for (sheet, id) in open {
            let Some(req) = self.pending.get(&id).cloned() else { continue };
            if let Some(rule) = self.rules.find(&req, &self.contacts, now) {
                self.rules.record_use(&rule, now);
                self.decide(&id, Decision::ApproveByRule(rule.clone()), "rule", Some(&rule), "standing rule", now);
                if let Some(line) = self.sheets.iter_mut().find(|s| s.id == sheet).and_then(|s| s.lines.iter_mut().find(|l| l.request == id)) {
                    line.answer = Some(Answer::Once);
                }
            }
        }
    }

    /// The person creates a rule in Settings.
    pub fn create_rule(&mut self, gesture: &ApprovalGesture, draft: RuleDraft, now: u64) -> Result<RuleId, String> {
        let id = self.rules.create(gesture, draft, now)?;
        self.reconsider(now);
        self.sheets.retain(|s| !s.done());
        self.changed();
        Ok(id)
    }
    pub fn delete_rule(&mut self, id: &RuleId) -> bool {
        let changed = self.rules.delete(id);
        self.changed();
        changed
    }
    /// One tap: every rule off.
    pub fn all_off(&mut self) -> usize {
        let n = self.rules.all_off();
        if n > 0 {
            self.notice("Approval rules are off", format!("{n} rule{} turned off. Every approval now asks you.", if n == 1 { "" } else { "s" }));
        }
        self.changed();
        n
    }

    /// Once a second: rules that ran out, apps that never came, sheets
    /// nobody answered. True when anything visible changed.
    pub fn tick(&mut self, now: u64) -> bool {
        let before = self.generation;
        for rule in self.rules.expire(now) {
            self.notice("Approval rule ended", format!("{} no longer approves on its own.", rule.describe()));
            self.changed();
        }
        let late: Vec<RequestId> = self.waiting.iter().filter(|(_, until)| now >= **until).map(|(id, _)| id.clone()).collect();
        for id in late {
            self.waiting.remove(&id);
            let app = self.pending.get(&id).map(|r| app_label(&r.app)).unwrap_or_default();
            self.refuse_visibly(&id, &format!("{app} wasn't opened in time to confirm it"), now);
        }
        // The prompt deadline, per request (a batched sheet's later lines
        // keep their own): sheet lines and `confirm: app` requests on an
        // app's sheet. Never an external connection's.
        let deadline = self.sheet_expiry_s;
        let due: Vec<RequestId> = self
            .pending
            .values()
            .filter(|r| r.context.connection == Connection::Host && now >= r.received.saturating_add(deadline))
            .filter(|r| self.at_app.contains_key(&r.id) || self.sheets.iter().any(|s| s.open_lines().any(|l| l.request == r.id)))
            .map(|r| r.id.clone())
            .collect();
        for id in due {
            self.expire(&id, now);
        }
        self.sheets.retain(|s| !s.done());
        self.generation != before
    }

    /// `id` reached the deadline unanswered: denied (never approved), with
    /// the reason, audited `by: expired`; withdrawn from its sheet (or the
    /// app's) and kept visible as an [`Expired`] record and a notice.
    fn expire(&mut self, id: &RequestId, now: u64) {
        let Some(req) = self.pending.get(id).cloned() else { return };
        let reason = crate::ai_host::app_peers::host_tools::expiry_reason(std::time::Duration::from_secs(self.sheet_expiry_s));
        let on_app = self.at_app.get(id).cloned();
        // The expiry note: the broker reads a deny carrying it as an expiry,
        // not an answer in time, so its grace still frees a stuck turn.
        let note = crate::ai_host::app_peers::host_tools::expired_note(&reason);
        self.decide(id, Decision::Deny, "expired", None, &note, now);
        if let Some(app) = on_app {
            if let Some(handler) = self.app_confirms.get_mut(&app) {
                handler.withdrawn(id, &note);
            }
        }
        for sheet in &mut self.sheets {
            if let Some(line) = sheet.lines.iter_mut().find(|l| l.request == *id) {
                line.answer = Some(super::sheet::Answer::Deny);
            }
        }
        let heading = format!("{} \u{00b7} {}", app_label(&req.app), req.tool.name);
        let caller = caller_label(&req.app, &req.caller);
        self.notice(format!("Expired: {heading}"), format!("{caller} asked; no answer in time ({reason}), so it was denied. Nothing was approved."));
        self.expired.push(Expired { id: id.clone(), app: req.app.clone(), heading, caller, reason, at: now });
        if self.expired.len() > EXPIRED_KEPT {
            self.expired.remove(0);
        }
        self.changed();
    }

    /// The one exit: relay, audit, notice.
    fn decide(&mut self, id: &RequestId, decision: Decision, by: &str, rule: Option<&RuleId>, reason: &str, now: u64) {
        let Some(req) = self.pending.remove(id) else { return };
        self.waiting.remove(id);
        self.at_app.remove(id);
        let entry = Entry {
            ts: now,
            id: id.0.clone(),
            app: req.app.clone(),
            tool: req.tool.name.clone(),
            args_digest: facts::digest(&req.args),
            caller: req.caller.as_audit(),
            trigger: req.context.trigger.as_str().into(),
            by: by.into(),
            rule: rule.map(|r| r.0.clone()),
            result: if decision.approved() { "approved".into() } else { "denied".into() },
            reason: reason.into(),
        };
        let automatic = entry.automatic();
        self.audit.append(entry);
        if automatic {
            let why = match rule.and_then(|r| self.rules.get(r)) {
                Some(r) => format!("By your rule: {}", r.describe()),
                None => "Developer mode approves everything.".to_string(),
            };
            self.notice(format!("Approved automatically: {} \u{00b7} {}", app_label(&req.app), req.tool.name), format!("{why} Asked by {}.", caller_label(&req.app, &req.caller)));
        }
        self.relay.approval_decided(id, decision, reason);
    }
}

/// The app whose agent asked (a Stop on its conversation stops it).
pub fn agent_of(req: &Request) -> Option<String> {
    match &req.caller {
        Caller::OwnAgent { .. } => Some(req.app.clone()),
        Caller::AppAgent { app } => Some(app.clone()),
        Caller::SystemAgent | Caller::External { .. } => None,
    }
}

fn app_request(req: &Request) -> AppConfirmRequest {
    AppConfirmRequest {
        id: req.id.clone(),
        tool: req.tool.name.clone(),
        args: req.args.clone(),
        caller: req.caller.clone(),
        caller_label: caller_label(&req.app, &req.caller),
        context_id: req.context.context_id.clone(),
    }
}

/// The request as the router keeps it. An empty `call_id` gets one.
pub fn make_request(app: &str, tool: ToolSpec, args: Value, caller: Caller, context: RequestContext, now: u64, fallback_id: u64) -> Request {
    let id = if context.call_id.trim().is_empty() { RequestId(format!("shell-{fallback_id}")) } else { RequestId(context.call_id.clone()) };
    Request { id, app: app.to_string(), tool, args, caller, context, received: now }
}

impl ApprovalIntake for Router {
    fn approval_requested(&mut self, app: &str, tool: ToolSpec, args: Value, caller: Caller, context: RequestContext) -> Route {
        let now = super::now();
        let n = self.next_request;
        self.next_request += 1;
        self.request(make_request(app, tool, args, caller, context, now, n), now)
    }
}
