//! The shell as every app agent's tool host (octos UPCR-2026-035, ADR 0004
//! §4, §5, §7, §8, §11, §12): octos#2567's relay.
//!
//! | Kernel side | Here |
//! | --- | --- |
//! | a broker's `peer/tools/register` (after every prepare and reconnect) | [`ShellToolHost::declarations`]: the app's tools and the cross-app tools granted to it ([`relay::Catalog`]) |
//! | `peer/prepare`'s `cwd` for a new peer | [`ShellToolHost::agent_workspace`]: the account's folder (`app_storage`) |
//! | `peer/tool/call` | [`relay::Relay`]: authorize, route to the owning app, confirm, answer once |
//! | `peer/tool/cancel`, an interrupt, a closed connection | the call ends; whoever holds it is told |
//! | `approval/requested` `host_tool` | the approval router ([`crate::approvals::approval_requested`]); its decision answers the kernel; its turn ending first withdraws it ([`crate::approvals::withdraw`]) |
//! | any other `approval/requested` on an app's peer session or context (octos's own tools) | the same router, as the app agent's call on its own app (ADR 0004 §8); the app hears only `approval/handled_by_host` |
//! | `peer/input` | admitted here (consent, a suspended account); the broker starts the turn |
//! | `user_question/requested` on an app peer (octos's `ask_user_question`) | [`crate::questions`]: the app's conversation, or the system chat for a `peer/input` turn; answered only by the person on a shell surface |
//! | the system session's `terminal.run` (Setup › Assistant › Command execution) | [`crate::system_chat`] registers it; its calls come here |
//! | `files.list`, `files.read`, `files.search` on every app peer with a workspace (ADR 0004 §11) | declared as the app's own tools, run by the shell over the calling account's folder, never another context's ([`files`]) |
//! | `dev.run` on a covered app's peer (developer mode, ADR 0004 §13) | offered as the app's own tool, run by the shell ([`dev_run`]); registered again on every peer when the mode changes ([`developer_mode_changed`]) |
//! | the system toolbox's tools (feature `toolbox-peers`) | the `toolbox` owner: its tools declared once, granted per app, offered after consent, run by its executor ([`toolbox`]) |
//!
//! **Threads.** Brokers call in on their own threads and the system chat on
//! its own; every call, cancel and approval is queued and handled on the UI
//! thread in [`pump`] (the shell calls it on every signal and every tick),
//! where the approval router, the peer links and the AI bus live. The
//! router's decisions and the peer links' outcomes come back through queues
//! too, so nothing here re-enters a lock it holds.
//!
//! **Where calls go.** A process app's calls go down its peer link
//! (`crate::peer_link`, which keeps the host obligations for the process);
//! an in-process module's (or a script app's host service's) to the
//! executor it installed through its service
//! (`OctosAppService::set_tool_executor`); the Terminal's `run` to the
//! Terminal on the AI bus. A `confirm: app` tool is confirmed on the owning
//! app's own sheet: an in-process app installs it through its service
//! (`OctosAppService::set_confirm_sheet`), which registers it with the
//! router here ([`SheetBridge`]).

pub mod dev_run;
pub mod files;
pub mod relay;
pub mod schema;
#[cfg(feature = "toolbox-peers")]
pub mod toolbox;
#[cfg(any(feature = "app-hub", native_mobile))]
pub mod script_apps;

#[cfg(test)]
mod tests;
#[cfg(all(test, kernel, any(feature = "app-hub", native_mobile)))]
mod real_kernel_tests;
#[cfg(all(test, kernel, feature = "app-hub"))]
mod scenario_tests;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;

use crate::ai_host::app_peers::host_tools::{self, AgentQuestion, ApprovalAnswer, InputRefusal, ConfirmRequest, ConfirmSheet, HostToolApproval, HostToolCall, PeerInput, QuestionAnswer, ToolExecutor, ToolHost, ToolOutcome, ToolReply};
use crate::approvals::{self, Caller, Decision, RequestContext, RequestId, Route, ToolSpec};
use crate::peer_link::{self, KernelToolCall, Refused, ToolCallResult};
pub use relay::{app_of_peer, Event, Relay, APPROVAL_PREFIX, BUS_PREFIX, CONFIRM_PREFIX, SYSTEM, TERMINAL_RUN, TOOLBOX};

static RELAY: Mutex<Option<Relay>> = Mutex::new(None);
static INBOX: Mutex<Vec<Event>> = Mutex::new(Vec::new());

fn with_relay<R>(f: impl FnOnce(&mut Relay) -> R) -> R {
    let mut guard = RELAY.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(|| {
        let mut relay = Relay::default();
        relay.set_audit(Arc::new(write_audit));
        relay
    }))
}

/// The home whose `logs/tool-calls.jsonl` the audit writes (set by
/// [`init`]); unset, lines are only logged (tests, headless runs).
static AUDIT_HOME: OnceLock<PathBuf> = OnceLock::new();

/// An audit sink that writes each line to `home`'s
/// `approvals::audit::CALLS_FILE` at once, from any thread (review of #222:
/// nothing waits in memory for a crash or a quit to lose).
pub fn audit_sink_for(home: PathBuf) -> relay::AuditSink {
    Arc::new(move |entry| {
        if let Err(e) = approvals::audit::append_call(&home, &entry) {
            makepad_widgets::log!("host tools: could not write the tool-call audit: {e}");
        }
    })
}

/// The shell's one tool-call audit (the relay's and the AI bus pane's):
/// written at once to the home's file.
pub fn write_audit(entry: relay::CallAudit) {
    match AUDIT_HOME.get() {
        Some(home) => {
            if let Err(e) = approvals::audit::append_call(home, &entry) {
                makepad_widgets::log!("host tools: could not write the tool-call audit: {e}");
            }
        }
        None => makepad_widgets::log!("host tools: audit {} {} {} {} {}", entry.caller, entry.owner, entry.tool, entry.phase, entry.outcome),
    }
}

/// Queue an event for [`pump`], and wake the UI thread.
pub fn submit(event: Event) {
    INBOX.lock().unwrap_or_else(|e| e.into_inner()).push(event);
    makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
}

/// On the UI thread: handle everything queued (and what that queues), and
/// deliver the host services' answers to script apps' tool calls.
pub fn pump() {
    #[cfg(any(feature = "app-hub", native_mobile))]
    script_apps::poll();
    for _ in 0..8 {
        let events = std::mem::take(&mut *INBOX.lock().unwrap_or_else(|e| e.into_inner()));
        if events.is_empty() {
            break;
        }
        let mut env = ShellEnv;
        with_relay(|r| {
            for event in events {
                r.handle(event, &mut env);
            }
        });
    }
}

/// At startup, after `approvals::init`: install the host (every broker's),
/// the router's relay and the peer links' relay.
pub fn init() {
    static DONE: OnceLock<()> = OnceLock::new();
    DONE.get_or_init(|| {
        if let Some(home) = approvals::with(|a| a.router.audit.home()).flatten() {
            let _ = AUDIT_HOME.set(home);
        }
        host_tools::set_host(Arc::new(ShellToolHost));
        approvals::set_relay(Box::new(DecisionRelay));
        peer_link::set_tool_relay(Box::new(LinkRelay));
        // The tools the shell runs itself for an app's own agent (`dev.run`).
        with_relay(|r| r.set_executor(relay::HOST_EXECUTOR, Some(Arc::new(HostExecutor::new(agent_workspace)))));
        #[cfg(feature = "toolbox-peers")]
        toolbox::init();
    });
}

// ------------------------------------------------------------ the seams
//
// How an app offers tools and who may call them (ADR 0004 §7, §12), in
// three steps; the toolbox (#108) uses the same:
//
// 1. `declare(app, tools)`: the owning app's `tools.json` entries (a native
//    app's from `native-apps.json` `agent.tools`, loaded at startup; a
//    script app's from its admitted bundle, `script_apps`);
// 2. `grant(caller, owning_app, tool)`: another app's shareable tool for a
//    caller's agent, marked with its owner (native: `agent.grants`; script:
//    the dotted names in its manifest's `agent.tools`, at install);
//    `set_generic(app, tools)`: exactly the octos kernel tools its agent
//    keeps (native: `agent.generic_tools`; script: the plain names in
//    `agent.tools`);
// 3. an `Executor` per owning app runs its calls: `set_executor(app, …)`
//    (an in-process module's through `OctosAppService::set_tool_executor`;
//    a script app's host services, `script_apps::HostServiceExecutor`); a
//    process app's calls go down its peer link, the Terminal's on the AI
//    bus. The relay authorizes each call first.

/// An app's `tools.json` (replaces what it declared before).
pub fn declare(app: &str, entries: Vec<Value>) {
    with_relay(|r| r.catalog.declare(app, entries));
}

/// A cross-app grant: `owning_app`'s shareable `tool` for `caller`'s agent.
pub fn grant(caller: &str, owning_app: &str, tool: &str) {
    with_relay(|r| r.catalog.grant(caller, owning_app, tool));
}

/// Exactly the octos kernel tools `app`'s agent keeps (never octos's shell).
pub fn set_generic(app: &str, tools: Vec<String>) {
    with_relay(|r| r.catalog.set_generic(app, tools));
}

/// `app`'s executor for its own tools (`None` removes it).
pub fn set_executor(app: &str, executor: Option<Arc<dyn ToolExecutor>>) {
    with_relay(|r| r.set_executor(app, executor));
}

/// The owning app of a tool another app is granted ([`relay::Catalog::owner_of`]:
/// by its namespace, never the first app that declares the name).
pub fn owner_of(tool: &str) -> Option<String> {
    with_relay(|r| r.catalog.owner_of(tool))
}

/// A tool's declaration, as a host registers it (the system chat).
pub fn declaration(owner: &str, tool: &str) -> Option<Value> {
    with_relay(|r| r.catalog.entry(owner, tool).and_then(|e| host_tools::declaration(e, Some(owner))))
}

/// A call from the system agent's conversation (the system chat's link).
pub fn system_call(mut call: HostToolCall, reply: ToolReply) {
    call.calling_app = SYSTEM.to_string();
    submit(Event::Call { call, reply });
}

pub fn system_cancel(call_id: &str) {
    submit(Event::Cancel { call_id: call_id.to_string(), reason: "cancelled".into() });
}

// ------------------------------------------------------------ the AI bus

/// What the shell's AI bus must send for the relay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusRequest {
    Call { call_id: String, app: String, tool: String, args: String },
    Cancel { call_id: String },
}

static BUS: Mutex<Vec<BusRequest>> = Mutex::new(Vec::new());

/// The shell drains these after [`pump`] and hands them to its AI bus.
pub fn take_bus_requests() -> Vec<BusRequest> {
    std::mem::take(&mut *BUS.lock().unwrap_or_else(|e| e.into_inner()))
}

/// The bus answered (or failed) one of the relay's calls (`call_id` is the
/// bus id, [`BUS_PREFIX`] + the kernel's).
pub fn bus_result(call_id: &str, outcome: ToolOutcome) {
    if let Some(kernel_id) = call_id.strip_prefix(BUS_PREFIX) {
        submit(Event::BusResult { call_id: kernel_id.to_string(), outcome });
    }
}

// ------------------------------------------------------------ the host

/// The shell, as every broker's [`ToolHost`].
pub struct ShellToolHost;

impl ToolHost for ShellToolHost {
    fn declarations(&self, app_id: &str, account: &str) -> Result<Vec<Value>, String> {
        let app = app_of_peer(app_id).to_string();
        ensure_loaded(app_id);
        let dev = crate::dev_mode::grants_all(&app);
        // The toolbox's tools only once the person allowed this app's agent
        // (ADR 0004 §4); its calls are refused before that too (the relay).
        let consented = approvals::consent_granted(&app) || dev;
        let mut tools = with_relay(|r| r.catalog.offered(&app, dev, consented));
        // The host read tools (ADR 0004 §11) on every consented peer whose
        // agent has a workspace: how its request contexts read the
        // account's data (Unix only, files.rs).
        if files::SUPPORTED && consented && agent_workspace(app_id, account).is_some() {
            tools.extend(files::declarations(&app));
        }
        Ok(tools)
    }

    fn generic_tools(&self, app_id: &str, _account: &str) -> Option<Vec<String>> {
        let app = app_of_peer(app_id).to_string();
        ensure_loaded(app_id);
        let dev = crate::dev_mode::grants_all(&app);
        Some(with_relay(|r| r.catalog.generic(&app, dev)))
    }

    fn agent_workspace(&self, app_id: &str, account: &str) -> Option<PathBuf> {
        agent_workspace(app_id, account)
    }

    fn suspended(&self, app_id: &str, account: &str) -> bool {
        suspended(app_id, Some(account))
    }

    fn context_reads_account(&self, app_id: &str, account: &str) -> bool {
        context_reads_account(app_id, account)
    }

    fn workspace_refused(&self, app_id: &str, account: &str) -> Option<String> {
        workspace_refused_in(crate::app_storage::host()?, app_id, account)
    }

    fn tool_call(&self, call: HostToolCall, reply: ToolReply) {
        submit(Event::Call { call, reply });
    }

    fn tool_cancel(&self, _app_id: &str, call_id: &str, reason: &str) {
        submit(Event::Cancel { call_id: call_id.to_string(), reason: reason.to_string() });
    }

    fn admit_input(&self, app_id: &str, account: &str, input: &PeerInput) -> Result<(), InputRefusal> {
        let app = app_of_peer(app_id);
        if suspended(app_id, Some(account)) {
            return Err(InputRefusal::SignedOut);
        }
        if let Some(why) = crate::app_storage::host().and_then(|s| workspace_refused_in(s, app_id, account)) {
            return Err(InputRefusal::Other(format!("the app's workspace was refused: {why}")));
        }
        if !approvals::consent_granted(app) && !crate::dev_mode::grants_all(app) {
            return Err(InputRefusal::NoConsent);
        }
        makepad_widgets::log!("host tools: the system agent's input {} starts {app}'s turn {}", input.input_id, input.turn_id);
        Ok(())
    }

    fn host_tool_approval(&self, app_id: &str, account: Option<&str>, approval: HostToolApproval, answer: ApprovalAnswer) -> bool {
        submit(Event::Approval { app: app_id.to_string(), account: account.map(str::to_string), approval, answer });
        true
    }

    fn user_question(&self, app_id: &str, account: Option<&str>, question: AgentQuestion, answer: QuestionAnswer) -> bool {
        crate::questions::requested(app_id, account, question, answer);
        true
    }

    fn user_question_closed(&self, app_id: &str, question_id: &str) {
        crate::questions::closed(app_id, question_id);
    }

    fn host_tool_approval_closed(&self, _app_id: &str, approval_id: &str) {
        submit(Event::ApprovalClosed { approval_id: approval_id.to_string() });
    }

    fn set_executor(&self, app_id: &str, executor: Option<Arc<dyn ToolExecutor>>) {
        with_relay(|r| r.set_executor(app_of_peer(app_id), executor));
    }

    fn set_confirm_sheet(&self, app_id: &str, sheet: Option<Arc<dyn ConfirmSheet>>) {
        let app = app_of_peer(app_id);
        match sheet {
            Some(sheet) => approvals::register_app_confirm(app, Box::new(SheetBridge { app: app.to_string(), sheet })),
            None => approvals::unregister_app_confirm(app),
        }
    }
}

/// Developer mode turned on, off or expired (ADR 0004 §13): every live app
/// peer registers its tools again, so `dev.run` and developer grants come
/// with it and are withdrawn the moment it ends (the relay also refuses a
/// late call). The peers asked.
pub fn developer_mode_changed() -> usize {
    #[cfg(kernel)]
    {
        crate::ai_host::app_peers::broker::reregister_tools_where(|_| true)
    }
    #[cfg(not(kernel))]
    {
        0
    }
}

/// The shell's executor for the tools it runs itself for an app's own
/// agent: `dev.run` ([`dev_run`]) and the host read tools ([`files`]), each
/// over the calling account's workspace.
pub struct HostExecutor {
    dev_run: dev_run::DevRunExecutor,
    workspace: fn(&str, &str) -> Option<PathBuf>,
}

impl HostExecutor {
    pub fn new(workspace: fn(&str, &str) -> Option<PathBuf>) -> HostExecutor {
        HostExecutor { dev_run: dev_run::DevRunExecutor::new(workspace), workspace }
    }
}

impl ToolExecutor for HostExecutor {
    fn execute(&self, call: HostToolCall, reply: ToolReply) {
        if call.name == relay::DEV_RUN {
            return self.dev_run.execute(call, reply);
        }
        let Some(root) = (self.workspace)(&call.calling_app, call.account.as_deref().unwrap_or("device")) else {
            reply.finish(ToolOutcome::error("no_workspace", "this agent has no data folder"));
            return;
        };
        // Off the UI thread: a search reads many files.
        std::thread::spawn(move || {
            let scope = files::Scope { root: &root, context: call.context_id.as_deref() };
            reply.finish(match files::run(&call.name, &scope, &call.args) {
                Ok(data) => ToolOutcome::Ok(data),
                Err((kind, message)) => ToolOutcome::error(&kind, message),
            });
        });
    }

    fn cancel(&self, call_id: &str) {
        self.dev_run.cancel(call_id);
    }
}

/// Stop the turns running in both lanes of `app`'s agent's conversation,
/// the person's and the system agent's, whoever started them (the Stop on
/// the shell's app-conversation surface; ADR 0004 §6): every live broker
/// of the app's peer (`app`, or a script app's `card.<app>`). The turns
/// stopped.
pub fn interrupt_agent(app: &str) -> Vec<String> {
    #[cfg(kernel)]
    {
        let app = app.to_string();
        crate::ai_host::app_peers::broker::interrupt_where(move |peer_app| app_of_peer(peer_app) == app)
    }
    #[cfg(not(kernel))]
    {
        let _ = app;
        Vec::new()
    }
}

/// Stop only the turns of one `lane` of `app`'s agent's conversation
/// (`person` or `system_agent`, as the broker names them): the Stop of the
/// "Ask <app>" panel stops the person's own turn and leaves the system
/// agent's running; stopping the system agent's is a separate gesture.
/// The turns stopped.
pub fn interrupt_agent_lane(app: &str, lane: &str) -> Vec<String> {
    #[cfg(kernel)]
    {
        let app = app.to_string();
        crate::ai_host::app_peers::broker::interrupt_lane_where(move |peer_app| app_of_peer(peer_app) == app, lane)
    }
    #[cfg(not(kernel))]
    {
        let _ = (app, lane);
        Vec::new()
    }
}

/// A script app's agent block, loaded from its admitted bundle the first
/// time its peer registers (a native app's is in the shipped catalog).
fn ensure_loaded(app_id: &str) {
    if app_id == app_of_peer(app_id) {
        return;
    }
    let app = app_of_peer(app_id);
    if with_relay(|r| r.catalog.knows(app)) {
        return;
    }
    #[cfg(any(feature = "app-hub", native_mobile))]
    if let Err(e) = script_apps::load(app) {
        makepad_widgets::log!("host tools: {app}'s tools: {e}");
    }
}

/// App Hub installed or updated a script app: its tools, grants and
/// kernel tools again (the next registration of its peer takes them).
pub fn script_app_installed(app: &str) {
    #[cfg(any(feature = "app-hub", native_mobile))]
    if let Err(e) = script_apps::load(app) {
        makepad_widgets::log!("host tools: {app}'s tools: {e}");
    }
    #[cfg(not(any(feature = "app-hub", native_mobile)))]
    let _ = app;
}

/// An app's own sheet, as the approval router's `confirm: app` handler: the
/// request carries the tool, the exact arguments and the caller.
pub struct SheetBridge {
    pub app: String,
    pub sheet: Arc<dyn ConfirmSheet>,
}

impl approvals::AppConfirm for SheetBridge {
    fn withdrawn(&mut self, id: &RequestId, reason: &str) {
        self.sheet.withdrawn(&id.0, reason);
    }

    fn confirm(&mut self, request: &approvals::AppConfirmRequest) {
        let id = request.id.clone();
        let client = match &request.caller {
            Caller::OwnAgent { client } => client.clone(),
            _ => None,
        };
        self.sheet.confirm(ConfirmRequest::new(
            request.id.0.clone(),
            request.tool.clone(),
            request.args.clone(),
            request.caller_label.clone(),
            request.context_id.clone(),
            client,
            move |approved, reason| {
                // Never inside the router's own call (it holds its lock).
                let (id, reason) = (id.clone(), reason.to_string());
                std::thread::spawn(move || {
                    if let Err(e) = approvals::app_confirm_answered(&id, approved, &reason) {
                        makepad_widgets::log!("host tools: the app's answer to {id}: {e}");
                    }
                });
            },
        ));
    }
}

/// The account's agent workspace (ADR 0004 §11), for a new peer: its folder
/// under the app's jail, when the host's storage is set up, the app's agent
/// has files, and the account is not suspended or refused.
pub fn agent_workspace(app_id: &str, account: &str) -> Option<PathBuf> {
    agent_workspace_in(crate::app_storage::host()?, app_id, account)
}

/// Whether `app_id`'s agents are per account: a native app's entry says so,
/// a script app's (`card.<id>`) manifest `storage.accounts` (Mail); else it
/// acts for the device. The one rule every per-account decision here uses
/// ([`account_key`]).
fn keeps_accounts(storage: &crate::app_storage::Storage, app_id: &str) -> bool {
    let app = app_of_peer(app_id);
    match crate::native_apps::find(app) {
        Some(entry) => entry.accounts,
        None => app_id != app && storage.spec(app).accounts,
    }
}

/// The account `app_id`'s storage is keyed by for `account`: the account
/// for an app that keeps accounts ([`keeps_accounts`]), else the device
/// (`None`). Workspace, suspension, refusal and `read_parent` all use it.
fn account_key<'a>(storage: &crate::app_storage::Storage, app_id: &str, account: Option<&'a str>) -> Option<&'a str> {
    account.filter(|_| keeps_accounts(storage, app_id))
}

/// [`agent_workspace`] on `storage`.
pub fn agent_workspace_in(storage: &crate::app_storage::Storage, app_id: &str, account: &str) -> Option<PathBuf> {
    let app = app_of_peer(app_id);
    let has_files = match crate::native_apps::find(app) {
        Some(entry) => !entry.octos.is_empty() || crate::dev_mode::grants_all(app),
        None => app_id != app && storage.spec(app).agent_workspace != crate::app_storage::AgentWorkspace::None,
    };
    if !has_files {
        return None;
    }
    let account = account_key(storage, app_id, Some(account));
    if storage.is_signed_out(app, account) || storage.refused(app, account).is_some() {
        return None;
    }
    let dir = storage.layout().app(app).ok()?.account(account);
    crate::app_storage::ensure_private_dir(storage.layout().apps_root(), &dir).ok()?;
    Some(dir)
}

/// [`suspended`] on `storage`.
pub fn suspended_in(storage: &crate::app_storage::Storage, app_id: &str, account: Option<&str>) -> bool {
    storage.is_signed_out(app_of_peer(app_id), account_key(storage, app_id, account))
}

/// Whether the app's conversation reads its account's folder (octos
/// `read_parent`, ADR 0004 §11): the manifest's `storage.agent_workspace`
/// is `"account"` and the agent has that folder as its workspace now
/// ([`agent_workspace`]: files, not suspended, not refused, keyed by
/// [`account_key`]).
pub fn context_reads_account(app_id: &str, account: &str) -> bool {
    let Some(storage) = crate::app_storage::host() else { return false };
    context_reads_account_in(storage, app_id, account)
}

/// [`context_reads_account`] on `storage`.
pub fn context_reads_account_in(storage: &crate::app_storage::Storage, app_id: &str, account: &str) -> bool {
    reads_account(&storage.spec(app_of_peer(app_id)), agent_workspace_in(storage, app_id, account).as_deref())
}

/// [`context_reads_account`]'s rule, on the app's declared storage and the
/// agent's workspace now.
pub(crate) fn reads_account(spec: &crate::app_storage::StorageSpec, workspace: Option<&std::path::Path>) -> bool {
    spec.agent_workspace == crate::app_storage::AgentWorkspace::Account && workspace.is_some()
}

/// Why the startup check refused the workspace of `app_id`'s `account`,
/// keyed like [`agent_workspace`] ([`account_key`]): the account for an app
/// that keeps accounts, native or script (Mail), else the device folder.
pub fn workspace_refused_in(storage: &crate::app_storage::Storage, app_id: &str, account: &str) -> Option<String> {
    storage.refused(app_of_peer(app_id), account_key(storage, app_id, Some(account)))
}

/// Whether `app_id`'s `account` is signed out or removed (ADR 0004 §11).
pub fn suspended(app_id: &str, account: Option<&str>) -> bool {
    let Some(storage) = crate::app_storage::host() else { return false };
    suspended_in(storage, app_id, account)
}

// ------------------------------------------------------------ the env

struct ShellEnv;

impl relay::Env for ShellEnv {
    fn consent(&self, app: &str) -> bool {
        approvals::consent_granted(app) || crate::dev_mode::grants_all(app)
    }
    fn grants_all(&self, app: &str) -> bool {
        crate::dev_mode::grants_all(app)
    }
    fn suspended(&self, app: &str, account: Option<&str>) -> bool {
        suspended(app, account)
    }
    fn system_tools(&self) -> BTreeSet<String> {
        crate::system_chat::grants::host_tools()
    }
    fn tool_rule(&self, owner: &str, tool: &str) -> (bool, bool) {
        let short = tool.split_once('.').map(|(_, t)| t).unwrap_or(tool);
        let rule = crate::native_apps::find(owner).and_then(|a| a.tool(short).or_else(|| a.tool(tool)));
        let command = tool == TERMINAL_RUN || tool == relay::DEV_RUN;
        (rule.map(|r| r.auto_approvable).unwrap_or(true) && !command, command)
    }
    fn request_approval(&mut self, app: &str, tool: ToolSpec, args: Value, caller: Caller, context: RequestContext) -> Route {
        approvals::approval_requested(app, tool, args, caller, context)
    }
    fn withdraw_approval(&mut self, id: &RequestId, reason: &str) {
        approvals::withdraw(id, reason);
    }
    fn has_link(&self, app: &str) -> bool {
        peer_link::has_link(app)
    }
    fn link_call(&mut self, app: &str, call: KernelToolCall) -> Result<(), Refused> {
        peer_link::tool_call(app, call)
    }
    fn link_cancel(&mut self, app: &str, call_id: &str) {
        peer_link::tool_cancel(app, call_id)
    }
    fn bus_call(&mut self, call_id: &str, app: &str, tool: &str, args: String) {
        BUS.lock().unwrap_or_else(|e| e.into_inner()).push(BusRequest::Call { call_id: call_id.into(), app: app.into(), tool: tool.into(), args });
    }
    fn bus_cancel(&mut self, call_id: &str) {
        BUS.lock().unwrap_or_else(|e| e.into_inner()).push(BusRequest::Cancel { call_id: call_id.into() });
    }
    fn log(&mut self, line: String) {
        makepad_widgets::log!("{line}");
    }
}

/// The router's decisions on the relay's requests (every id no other part
/// of the shell owns reaches the installed relay; ours are queued).
struct DecisionRelay;

impl approvals::ApprovalRelay for DecisionRelay {
    fn approval_decided(&mut self, id: &RequestId, decision: Decision, reason: &str) {
        if id.0.starts_with(CONFIRM_PREFIX) || id.0.starts_with(APPROVAL_PREFIX) {
            submit(Event::Decision { id: id.clone(), decision, reason: reason.to_string() });
        } else {
            makepad_widgets::log!("host tools: a decision for {id} nobody holds ({decision:?})");
        }
    }
}

/// The peer links' outcomes for calls the relay forwarded.
struct LinkRelay;

impl peer_link::ToolRelay for LinkRelay {
    fn acknowledged(&mut self, app: &str, call_id: &str) {
        submit(Event::LinkOutcome { app: app.to_string(), call_id: call_id.to_string(), result: None });
    }
    fn finished(&mut self, app: &str, call_id: &str, result: ToolCallResult) {
        submit(Event::LinkOutcome { app: app.to_string(), call_id: call_id.to_string(), result: Some(result) });
    }
}

/// The AI bus's answer, as a tool outcome.
pub fn outcome_of(result: &makepad_ai_services::wire::ToolResult) -> ToolOutcome {
    use makepad_ai_services::wire::ToolOutcome as Bus;
    match result.outcome {
        Bus::Ok => {
            let data = serde_json::from_str::<Value>(&result.data).unwrap_or(Value::Null);
            ToolOutcome::Ok(serde_json::json!({"text": result.text, "note": result.note, "data": data}))
        }
        other => ToolOutcome::error(&format!("{other:?}"), result.text.clone()),
    }
}
