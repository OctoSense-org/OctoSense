//! The shell as every app agent's tool host (octos UPCR-2026-035, ADR 0004
//! §4, §5, §7, §8, §11, §12): octos#2567's relay.
//!
//! | Kernel side | Here |
//! | --- | --- |
//! | a broker's `peer/tools/register` (after every prepare and reconnect) | [`ShellToolHost::declarations`]: the app's tools and the cross-app tools granted to it ([`relay::Catalog`]) |
//! | `peer/prepare`'s `cwd` for a new peer | [`ShellToolHost::agent_workspace`]: the account's folder (`app_storage`) |
//! | `peer/tool/call` | [`relay::Relay`]: authorize, route to the owning app, confirm, answer once |
//! | `peer/tool/cancel`, an interrupt, a closed connection | the call ends; whoever holds it is told |
//! | `approval/requested` `host_tool` | the approval router ([`crate::approvals::approval_requested`]); its decision answers the kernel |
//! | any other `approval/requested` on an app's peer session or context (octos's own tools) | the same router, as the app agent's call on its own app (ADR 0004 §8); the app hears only `approval/handled_by_host` |
//! | `peer/input` | admitted here (consent, a suspended account); the broker starts the turn |
//! | `user_question/requested` on an app peer (octos's `ask_user_question`) | [`crate::questions`]: the app's conversation, or the system chat for a `peer/input` turn; answered only by the person on a shell surface |
//! | the system session's `terminal.run` (Setup › Assistant › Command execution) | [`crate::system_chat`] registers it; its calls come here |
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

pub mod relay;
#[cfg(feature = "toolbox-peers")]
pub mod toolbox;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;

use crate::ai_host::app_peers::host_tools::{self, AgentQuestion, ApprovalAnswer, ConfirmRequest, ConfirmSheet, HostToolApproval, HostToolCall, PeerInput, QuestionAnswer, ToolExecutor, ToolHost, ToolOutcome, ToolReply};
use crate::approvals::{self, Caller, Decision, RequestContext, RequestId, Route, ToolSpec};
use crate::peer_link::{self, KernelToolCall, Refused, ToolCallResult};
pub use relay::{app_of_peer, Event, Relay, APPROVAL_PREFIX, BUS_PREFIX, CONFIRM_PREFIX, SYSTEM, TERMINAL_RUN, TOOLBOX};

static RELAY: Mutex<Option<Relay>> = Mutex::new(None);
static INBOX: Mutex<Vec<Event>> = Mutex::new(Vec::new());

fn with_relay<R>(f: impl FnOnce(&mut Relay) -> R) -> R {
    let mut guard = RELAY.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Relay::default))
}

/// Queue an event for [`pump`], and wake the UI thread.
pub fn submit(event: Event) {
    INBOX.lock().unwrap_or_else(|e| e.into_inner()).push(event);
    makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
}

/// On the UI thread: handle everything queued (and what that queues).
pub fn pump() {
    for _ in 0..8 {
        let events = std::mem::take(&mut *INBOX.lock().unwrap_or_else(|e| e.into_inner()));
        if events.is_empty() {
            return;
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
        host_tools::set_host(Arc::new(ShellToolHost));
        approvals::set_relay(Box::new(DecisionRelay));
        peer_link::set_tool_relay(Box::new(LinkRelay));
        #[cfg(feature = "toolbox-peers")]
        toolbox::init();
    });
}

/// An app's `tools.json` (App Hub at install; tests).
pub fn declare(app: &str, entries: Vec<Value>) {
    with_relay(|r| r.catalog.declare(app, entries));
}

/// A cross-app grant (App Hub at install for a script app; a native app's
/// reviewed `native-apps.json` entry).
pub fn grant(caller: &str, tool: &str) {
    with_relay(|r| r.catalog.grant(caller, tool));
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
    fn declarations(&self, app_id: &str, _account: &str) -> Result<Vec<Value>, String> {
        let app = app_of_peer(app_id).to_string();
        let dev = crate::dev_mode::grants_all(&app);
        // The toolbox's tools only once the person allowed this app's agent
        // (ADR 0004 §4); its calls are refused before that too (the relay).
        let consented = approvals::consent_granted(&app) || dev;
        Ok(with_relay(|r| r.catalog.offered(&app, dev, consented)))
    }

    fn agent_workspace(&self, app_id: &str, account: &str) -> Option<PathBuf> {
        agent_workspace(app_id, account)
    }

    fn suspended(&self, app_id: &str, account: &str) -> bool {
        suspended(app_id, Some(account))
    }

    fn tool_call(&self, call: HostToolCall, reply: ToolReply) {
        submit(Event::Call { call, reply });
    }

    fn tool_cancel(&self, _app_id: &str, call_id: &str, reason: &str) {
        submit(Event::Cancel { call_id: call_id.to_string(), reason: reason.to_string() });
    }

    fn admit_input(&self, app_id: &str, account: &str, input: &PeerInput) -> Result<(), String> {
        let app = app_of_peer(app_id);
        if suspended(app_id, Some(account)) {
            return Err("the account is signed out".into());
        }
        if !approvals::consent_granted(app) && !crate::dev_mode::grants_all(app) {
            return Err("the person has not allowed this app's agent".into());
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

/// An app's own sheet, as the approval router's `confirm: app` handler: the
/// request carries the tool, the exact arguments and the caller.
pub struct SheetBridge {
    pub app: String,
    pub sheet: Arc<dyn ConfirmSheet>,
}

impl approvals::AppConfirm for SheetBridge {
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
    let storage = crate::app_storage::host()?;
    let app = app_of_peer(app_id);
    let (keeps_accounts, has_files) = match crate::native_apps::find(app) {
        Some(entry) => (entry.accounts, !entry.octos.is_empty() || crate::dev_mode::grants_all(app)),
        // A script app (`card.<id>`) acts for the device.
        None => (false, app_id != app),
    };
    if !has_files {
        return None;
    }
    let account = keeps_accounts.then_some(account);
    if storage.is_signed_out(app, account) || storage.refused(app, account).is_some() {
        return None;
    }
    let dir = storage.layout().app(app).ok()?.account(account);
    crate::app_storage::ensure_private_dir(storage.layout().apps_root(), &dir).ok()?;
    Some(dir)
}

/// Whether `app_id`'s `account` is signed out or removed (ADR 0004 §11).
pub fn suspended(app_id: &str, account: Option<&str>) -> bool {
    let Some(storage) = crate::app_storage::host() else { return false };
    let app = app_of_peer(app_id);
    let keeps_accounts = crate::native_apps::find(app).is_some_and(|e| e.accounts);
    storage.is_signed_out(app, if keeps_accounts { account } else { None })
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
