//! The shell's approval surface (ADR 0004 §8, §4, §13; ADR 0002 §10 as
//! amended): only the person approves, live on a shell-drawn sheet or in
//! advance by a standing rule; developer mode overrides every approval.
//!
//! | Part | Module |
//! | --- | --- |
//! | the request, its caller and context; decisions | [`types`] |
//! | the seam with the octos#2567 relay (`approval_requested` in, `approval_decided` out) | [`relay`] |
//! | developer mode's hooks, asked first (the adapter to `dev_mode`) | [`dev_hooks`] |
//! | the router: precedence, sheets, `confirm: app` hand-off, timeouts | [`router`] |
//! | standing rules, their conditions, cap, time box, "all off", the person's gesture | [`rules`] |
//! | what rules and sheets read from the exact arguments; redaction; digest | [`facts`] |
//! | the sheet model | [`sheet`] |
//! | the append-only, owner-only audit | [`audit`] |
//! | consent at first use; `consent::granted(app)` | [`consent`] |
//! | contacts for "recipients in my contacts", behind the person's consent | [`contacts`] |
//! | the shell-drawn sheet, first-use sheet and time-box indicator | [`view`] |
//! | Settings → Assistant → Approvals | [`settings_page`] |
//!
//! Files, per OctoSense home: [`rules::RULES_FILE`], [`consent::CONSENT_FILE`],
//! [`contacts::CONTACTS_FILE`] and [`audit::AUDIT_FILE`], all owner-only.
//!
//! The shell calls [`init`] at startup, [`tick`] once a second (and shows
//! [`take_notices`] as notifications), and gives pointer events to
//! [`pointer`] before anything else while a sheet or the Settings page is
//! up. The relay calls [`approval_requested`] and installs itself with
//! [`set_relay`]; an app module registers its own confirmation sheet with
//! [`register_app_confirm`].

pub mod audit;
pub mod consent;
pub mod contacts;
pub mod dev_hooks;
pub mod facts;
pub mod relay;
pub mod router;
pub mod rules;
pub mod settings_page;
pub mod sheet;
pub mod types;
pub mod view;

#[cfg(test)]
mod tests;

use makepad_widgets::*;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub use relay::{ApprovalIntake, ApprovalRelay, RecordingRelay};
pub use router::{AppConfirm, AppConfirmRequest, Expired, Notice, Route, Router};
pub use types::{Batch, Caller, Confirm, Connection, Decision, Request, RequestContext, RequestId, RuleId, ToolSpec, Trigger};

/// Everything the shell holds for approvals.
pub struct Approvals {
    pub router: Router,
    pub consent: consent::ConsentStore,
    /// Settings → Assistant → Approvals is open.
    pub settings_open: bool,
    /// Decisions for requests other than the bus's, until the relay is
    /// installed.
    queue: RecordingRelay,
    /// Decisions for the AI bus's held calls (`bus:` ids), drained by the
    /// shell ([`take_bus_decisions`]).
    bus: RecordingRelay,
    /// Decisions for the peer link's calls (`peerlink:` ids), drained by
    /// `peer_link` ([`take_peer_decisions`]).
    peer: RecordingRelay,
    /// Decisions for the system chat's approvals (`syschat:` ids), drained
    /// by the chat ([`take_system_chat_decisions`]).
    system_chat: RecordingRelay,
    external: Arc<Mutex<Option<Box<dyn ApprovalRelay>>>>,
}

/// Where the router's decisions go: the AI bus's own held calls to the
/// shell, the rest to the octos#2567 relay (queued until it is installed).
struct Dispatch {
    bus: RecordingRelay,
    peer: RecordingRelay,
    system_chat: RecordingRelay,
    queue: RecordingRelay,
    external: Arc<Mutex<Option<Box<dyn ApprovalRelay>>>>,
}

impl ApprovalRelay for Dispatch {
    fn approval_decided(&mut self, id: &RequestId, decision: Decision, reason: &str) {
        if id.0.starts_with(crate::ai_bus::HELD_PREFIX) {
            return self.bus.approval_decided(id, decision, reason);
        }
        if id.0.starts_with(crate::peer_link::link::HELD_PREFIX) {
            return self.peer.approval_decided(id, decision, reason);
        }
        if id.0.starts_with(crate::system_chat::HELD_PREFIX) {
            return self.system_chat.approval_decided(id, decision, reason);
        }
        match self.external.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            Some(relay) => relay.approval_decided(id, decision, reason),
            None => self.queue.approval_decided(id, decision, reason),
        }
    }
}

impl Approvals {
    pub fn in_home(home: &Path) -> Approvals {
        let contacts = contacts::ContactsGate::in_home(home, contacts::shell_source());
        Approvals::with_parts(rules::RuleStore::in_home(home), audit::AuditLog::in_home(home), consent::ConsentStore::in_home(home), contacts)
    }
    pub fn memory() -> Approvals {
        Approvals::with_parts(rules::RuleStore::memory(), audit::AuditLog::memory(), consent::ConsentStore::memory(), contacts::ContactsGate::memory(Box::new(contacts::NoContacts)))
    }
    fn with_parts(rules: rules::RuleStore, audit: audit::AuditLog, consent: consent::ConsentStore, contacts: contacts::ContactsGate) -> Approvals {
        let queue = RecordingRelay::default();
        let bus = RecordingRelay::default();
        let peer = RecordingRelay::default();
        let system_chat = RecordingRelay::default();
        let external = Arc::new(Mutex::new(None));
        let dispatch = Dispatch { bus: bus.clone(), peer: peer.clone(), system_chat: system_chat.clone(), queue: queue.clone(), external: external.clone() };
        let router = Router::new(rules, audit, Box::new(dev_hooks::ShellDevMode), contacts, Box::new(dispatch));
        Approvals { router, consent, settings_open: false, queue, bus, peer, system_chat, external }
    }
    /// Sheets, rules, consent and the page: one number for "redraw".
    pub fn generation(&self) -> u64 {
        let now = now();
        // The time-box indicator counts minutes down.
        let minute = if self.router.rules.active_everything(now).is_empty() { 0 } else { now / 60 };
        // An app agent's question is drawn on the same surface.
        self.router.generation() + self.consent.generation() + self.router.contacts().generation() + u64::from(self.settings_open) + minute + crate::questions::generation()
    }
    pub fn consent_granted(&self, app: &str) -> bool {
        self.consent.granted(app, self.router.hooks().grants_all(app))
    }
}

static STATE: Mutex<Option<Approvals>> = Mutex::new(None);

/// Run `f` on the shell's approvals, once [`init`] has run.
pub fn with<R>(f: impl FnOnce(&mut Approvals) -> R) -> Option<R> {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(f)
}

/// At startup, once: this home's rules, consent and audit.
pub fn init(home: &Path) {
    let a = Approvals::in_home(home);
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(a);
    // Contained apps' `octos` service asks consent at first use too, and
    // grants only the `octos.*` services the app's manifest declares.
    crate::ai_host::contained::set_consent(consent_for_contained);
    crate::ai_host::contained::set_declared(crate::apps::declared_octos);
}

/// For tests and headless runs: approvals kept in memory only.
pub fn init_memory() {
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Approvals::memory());
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ------------------------------------------------------------ the relay

/// The relay's entry point (octos#2567): a call needs an approval.
pub fn approval_requested(app: &str, tool: ToolSpec, args: serde_json::Value, caller: Caller, context: RequestContext) -> Route {
    with(|a| a.router.approval_requested(app, tool, args, caller, context)).unwrap_or_else(|| Route::Refused("approvals are not set up".into()))
}

/// The relay installs itself; decisions made before are handed over first.
pub fn set_relay(mut relay: Box<dyn ApprovalRelay>) {
    with(|a| {
        for (id, decision, reason) in a.queue.take() {
            relay.approval_decided(&id, decision, &reason);
        }
        *a.external.lock().unwrap_or_else(|e| e.into_inner()) = Some(relay);
    });
}

/// The AI bus's `confirm: host` calls (`ai_bus::Route::Approval`): each is
/// a request like any other; the shell drains the answers here and
/// releases the held call (`AiBus::release`).
pub fn bus_requested(held: &crate::ai_bus::HeldCall) -> Route {
    let mut tool = ToolSpec::host(&held.tool);
    tool.auto_approvable = held.auto_approvable;
    if held.command {
        tool = tool.command();
    }
    let args = serde_json::from_str(&held.args).unwrap_or_else(|_| serde_json::Value::String(held.args.clone()));
    // The pane is the person's own conversation with the system agent.
    let context = RequestContext { call_id: held.key.clone(), trigger: Trigger::Person, ..RequestContext::default() };
    approval_requested(&held.app, tool, args, Caller::SystemAgent, context)
}

pub fn take_bus_decisions() -> Vec<(RequestId, Decision, String)> {
    with(|a| a.bus.take()).unwrap_or_default()
}

/// The peer link's decisions (`peer_link::link::HELD_PREFIX` ids).
pub fn take_peer_decisions() -> Vec<(RequestId, Decision, String)> {
    with(|a| a.peer.take()).unwrap_or_default()
}

/// The system chat's approvals (`crate::system_chat`): the router's
/// decisions, which the chat alone sends to the kernel.
pub fn take_system_chat_decisions() -> Vec<(RequestId, Decision, String)> {
    with(|a| a.system_chat.take()).unwrap_or_default()
}

/// An app module registers its own confirmation sheet (`confirm: app`).
pub fn register_app_confirm(app: &str, handler: Box<dyn AppConfirm>) {
    with(|a| a.router.register_app_confirm(app, handler));
}
pub fn unregister_app_confirm(app: &str) {
    with(|a| a.router.unregister_app_confirm(app, now()));
}
/// The owning app's sheet answered a `confirm: app` request.
pub fn app_confirm_answered(id: &RequestId, approved: bool, reason: &str) -> Result<(), String> {
    with(|a| a.router.app_confirm_answered(id, approved, reason, now())).unwrap_or_else(|| Err("approvals are not set up".into()))
}

// ------------------------------------------------------------ consent

/// `consent::granted(app)`: whether `app` may have its agent now (the
/// person allowed it, or developer mode grants everything). For #106's
/// contained apps and the Rinx/native offer path.
pub fn consent_granted(app: &str) -> bool {
    with(|a| a.consent_granted(app)).unwrap_or(false)
}

/// An app asks for its agent: shows the first-use sheet if the person has
/// not decided yet.
pub fn consent_ask(summary: consent::AgentSummary) -> consent::State {
    with(|a| {
        let all = a.router.hooks().grants_all(&summary.app);
        a.consent.ask(summary, all)
    })
    .unwrap_or(consent::State::Undecided)
}

/// The module host's gate for an in-process module's assistant: granted,
/// or the first-use sheet is shown (once) and this instance goes without.
pub fn consent_for_module(app: &str, label: &str, capabilities: &[&str]) -> bool {
    // The module host's own tests run in parallel with these; the gate
    // itself is tested on [`module_gate`].
    if cfg!(test) {
        return true;
    }
    with(|a| module_gate(a, app, label, capabilities)).unwrap_or(false)
}

/// [`consent_for_module`] on one `Approvals`.
pub fn module_gate(a: &mut Approvals, app: &str, label: &str, capabilities: &[&str]) -> bool {
    let all = a.router.hooks().grants_all(app);
    if a.consent.granted(app, all) {
        return true;
    }
    let manifest = serde_json::json!({ "capabilities": capabilities });
    let granted: Vec<String> = capabilities.iter().map(|c| c.to_string()).collect();
    let summary = consent::AgentSummary::from_manifest(app, label, &manifest, &granted, "The model set in AI providers");
    a.consent.ask(summary, all) == consent::State::Allowed
}

/// #106's contained apps (`ai_host::contained`): the same gate, for a
/// Card runner app asking the `octos` service.
pub fn consent_for_contained(app: &str) -> bool {
    consent_for_module(app, &sheet::app_label(app), &["octos.session.open", "octos.turn.start"])
}

// ------------------------------------------------------------ the shell

/// Once a second. True when something visible changed. App agents'
/// questions expire here too ([`crate::questions::tick`]).
pub fn tick() -> bool {
    let questions = crate::questions::tick(now());
    with(|a| a.router.tick(now())).unwrap_or(false) || questions
}

/// The Stop on an app agent's conversation (the shell's surface): what its
/// agent asks and the shell holds is denied or declined, with why, and the
/// turn running on its peer stops (the system agent's too: the person owns
/// the device). The turns stopped.
pub fn stop_agent(app: &str) -> Vec<String> {
    with(|a| a.router.stop_agent(app, now()));
    crate::questions::stop_agent(app);
    crate::host_tools::interrupt_agent(app)
}

/// The turn that raised `id` ended before anyone answered it: withdrawn
/// from its sheet ([`Router::withdraw`]).
pub fn withdraw(id: &RequestId, reason: &str) -> bool {
    with(|a| a.router.withdraw(id, reason, now())).unwrap_or(false)
}

/// The person dismissed an expired approval's record.
pub fn dismiss_expired(id: &RequestId) {
    with(|a| a.router.dismiss_expired(id));
}
pub fn take_notices() -> Vec<Notice> {
    with(|a| a.router.take_notices()).unwrap_or_default()
}
pub fn generation() -> u64 {
    with(|a| a.generation()).unwrap_or(0)
}
/// Settings → Assistant → Approvals: every app that declares an agent is
/// listed with its switch, whether or not it has asked yet.
pub fn open_settings() {
    let apps = crate::apps::agent_apps();
    with(|a| {
        register_agents(a, &apps);
        a.settings_open = true;
    });
}

/// Settings learns of every app that declares an agent.
pub fn register_agents(a: &mut Approvals, apps: &[crate::apps::AgentApp]) {
    for app in apps {
        let summary = consent::AgentSummary::from_manifest(&app.id, &app.name, &app.manifest, &app.octos, "The model set in AI providers");
        a.consent.register(summary);
    }
}

/// The apps whose agent the person just turned off (Settings, or a denial
/// on the first-use sheet): the shell revokes their live services.
pub fn take_revoked() -> Vec<String> {
    with(|a| a.consent.take_revoked()).unwrap_or_default()
}
pub fn close_settings() {
    with(|a| a.settings_open = false);
}

/// Pointer events, before the rest of the shell. True when the approval
/// surface took the event (it is modal while a sheet or the page is up).
pub fn pointer(ui: &WidgetRef, cx: &mut Cx, event: &Event) -> bool {
    if !matches!(event, Event::MouseDown(_) | Event::MouseUp(_) | Event::MouseMove(_) | Event::TouchUpdate(_) | Event::Scroll(_)) {
        return false;
    }
    let settings = ui.widget(cx, ids!(shell_approvals_settings));
    let taken = settings.borrow_mut::<settings_page::ShellApprovalsSettings>().map(|mut s| s.pointer(cx, event)).unwrap_or(false);
    if taken {
        ui.redraw(cx);
        return true;
    }
    let sheets = ui.widget(cx, ids!(shell_approvals));
    let taken = sheets.borrow_mut::<view::ShellApprovals>().map(|mut s| s.pointer(cx, event)).unwrap_or(false);
    if taken {
        ui.redraw(cx);
    }
    taken
}

/// `--test-action approval-sheet` / `approval-batch` / `approval-consent`
/// / `approvals-settings`: put a sample in front, for hidden-window runs.
/// Nothing here approves anything: the samples wait for the person.
pub fn test_action(name: &str) -> bool {
    use serde_json::json;
    let ctx = |id: &str| RequestContext { call_id: id.into(), trigger: Trigger::Person, ..RequestContext::default() };
    match name {
        "approval-sheet" => {
            approval_requested(
                "os.mail",
                ToolSpec::host("mail.send"),
                json!({"to": ["ana@example.org"], "subject": "Tuesday", "body": "See you at 3.", "smtp_password": "not-shown"}),
                Caller::AppAgent { app: "calendar".into() },
                ctx("sample-1"),
            );
        }
        "approval-batch" => {
            let batch = Some(Batch { id: "plan-1".into(), plan: "Book Tue 3\u{2013}4 pm and invite 2".into() });
            for (i, to) in ["ana@example.org", "bo@example.org"].iter().enumerate() {
                approval_requested(
                    "os.mail",
                    ToolSpec::host("mail.send"),
                    json!({"to": [to], "subject": "Meeting Tue 3 pm"}),
                    Caller::AppAgent { app: "calendar".into() },
                    RequestContext { batch: batch.clone(), trigger: Trigger::SystemAgent, ..ctx(&format!("batch-{i}")) },
                );
            }
        }
        "approval-consent" => {
            consent_ask(consent::AgentSummary {
                app: "os.news".into(),
                name: "News".into(),
                reads: vec!["News's files for the signed-in account".into(), "Its own memory".into()],
                uses: vec!["Web search and page reading (the system toolbox)".into(), "News's own tools".into()],
                model: "The model set in AI providers".into(),
            });
        }
        "approvals-settings" => open_settings(),
        _ => return false,
    }
    true
}

pub fn script_mod(vm: &mut ScriptVm) {
    view::script_mod(vm);
    settings_page::script_mod(vm);
}

// ------------------------------------------------------------ files

pub(crate) fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

/// Write a whole file owner-only, atomically (a temporary file, renamed).
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        create_private_dir(dir)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut options = std::fs::OpenOptions::new();
        options.create(true).write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}
