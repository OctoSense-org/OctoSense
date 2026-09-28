//! The approval router (ADR 0004 §8, §13): the shell's one place that
//! answers approval requests, on their exact arguments.
//!
//! For each request, in this order:
//!
//! 1. **Developer mode** ([`DevModeHooks`]): it approves everything of the
//!    apps it covers, `auto_approvable: false` and `confirm: app` included,
//!    never for an external client.
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

use super::audit::{AuditLog, Entry};
use super::dev_hooks::DevModeHooks;
use super::facts;
use super::relay::{ApprovalIntake, ApprovalRelay};
use super::rules::{Contacts, PersonGesture, RuleDraft, RuleStore};
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
}

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
    contacts: Box<dyn Contacts>,
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
    /// A sheet line nobody answers is declined after this (ADR 0002 §10).
    pub sheet_expiry_s: u64,
    generation: u64,
    next_request: u64,
}

impl Router {
    pub fn new(rules: RuleStore, audit: AuditLog, hooks: Box<dyn DevModeHooks>, contacts: Box<dyn Contacts>, relay: Box<dyn ApprovalRelay>) -> Router {
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
            sheet_expiry_s: 600,
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
    pub fn set_contacts(&mut self, contacts: Box<dyn Contacts>) {
        self.contacts = contacts;
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
    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }
    fn notice(&mut self, title: impl Into<String>, body: impl Into<String>) {
        self.notices.push(Notice { title: title.into(), body: body.into() });
    }

    /// Route one request. The decision may be given before this returns.
    pub fn request(&mut self, req: Request, now: u64) -> Route {
        if self.pending.contains_key(&req.id) {
            // octos approvals are once-only; a repeat is not a second ask.
            return Route::Refused(format!("request {} is already pending", req.id));
        }
        self.pending.insert(req.id.clone(), req.clone());
        let host = req.context.connection == Connection::Host;
        let app = req.app.as_str();

        // 1. Developer mode, before anything else.
        let dev = if req.tool.command {
            self.hooks.approves_command(app, req.context.connection)
        } else if req.tool.confirm == Confirm::App {
            host && self.hooks.overrides_app_confirm(app)
        } else {
            self.hooks.answers_approval(app, req.tool.auto_approvable, req.context.connection)
        };
        if dev {
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
            None => match self.rules.find(&req, &*self.contacts, now) {
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
        let line = Line::for_request(req, surfaced, &*self.contacts);
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
    pub fn answer(&mut self, sheet: u64, request: &RequestId, answer: Answer, gesture: &PersonGesture, now: u64) -> Result<Option<RuleId>, String> {
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
            if let Some(rule) = self.rules.find(&req, &*self.contacts, now) {
                self.rules.record_use(&rule, now);
                self.decide(&id, Decision::ApproveByRule(rule.clone()), "rule", Some(&rule), "standing rule", now);
                if let Some(line) = self.sheets.iter_mut().find(|s| s.id == sheet).and_then(|s| s.lines.iter_mut().find(|l| l.request == id)) {
                    line.answer = Some(Answer::Once);
                }
            }
        }
    }

    /// The person creates a rule in Settings.
    pub fn create_rule(&mut self, gesture: &PersonGesture, draft: RuleDraft, now: u64) -> Result<RuleId, String> {
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
        let expired: Vec<(u64, RequestId)> = self
            .sheets
            .iter()
            .filter(|s| now >= s.opened + self.sheet_expiry_s)
            .flat_map(|s| s.open_lines().map(move |l| (s.id, l.request.clone())))
            .collect();
        for (sheet, id) in expired {
            self.decide(&id, Decision::Deny, "timeout", None, "nobody answered; an expired request is declined", now);
            if let Some(line) = self.sheets.iter_mut().find(|s| s.id == sheet).and_then(|s| s.lines.iter_mut().find(|l| l.request == id)) {
                line.answer = Some(Answer::Deny);
            }
            self.changed();
        }
        self.sheets.retain(|s| !s.done());
        self.generation != before
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
