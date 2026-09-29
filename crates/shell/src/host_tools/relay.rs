//! The relay's logic, without globals or I/O: every effect goes through
//! [`Env`], so the tests drive it with their own.
//!
//! One [`Relay::handle`] per [`Event`], on the UI thread. For a call:
//!
//! 1. **Authorize** by (owning app, tool) and caller: the owning app's own
//!    agent calls its own declared tools; another app's agent only the tools
//!    granted to it ([`Catalog::may_call`]); the system agent only its
//!    grants ([`Env::system_tools`], `terminal.run` behind Setup's switch);
//!    developer mode grants everything (ADR 0004 §13). Consent and a
//!    suspended account refuse it too.
//! 2. **Route** to the owning app's executor: a process app's peer link, an
//!    in-process module's (or a script app's host service's) executor, or
//!    the Terminal's `run` on the AI bus (a live terminal the person sees).
//! 3. **Confirm**: a `confirm: app` call (`confirm_required`) is
//!    acknowledged first, then handed to the owning app's own sheet through
//!    the approval router; a process app's link does that itself.
//! 4. **Answer once** through the call's [`ToolReply`]; a cancel closes it
//!    and tells whoever holds the call.
//!
//! `host_tool` approvals (the kernel's `confirm: host` sheets) go to the
//! router with the owning app, the exact arguments and the caller, and its
//! decision answers the kernel.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use serde_json::Value;

use crate::ai_host::app_peers::host_tools::{self, ApprovalAnswer, CallOrigin, CallerKind, HostToolApproval, HostToolCall, ToolExecutor, ToolOutcome, ToolReply};
use crate::ai_host::app_peers::TurnTrigger;
use crate::approvals::{Caller, Decision, RequestContext, RequestId, Route, ToolSpec, Trigger};
use crate::peer_link::{KernelToolCall, Refused, Risk, ToolCallResult};

/// The router ids of `confirm: app` hand-offs.
pub const CONFIRM_PREFIX: &str = "hosttool:";
/// The router ids of `host_tool` approvals.
pub const APPROVAL_PREFIX: &str = "hostappr:";
/// The call ids of the shell's own calls on the AI bus.
pub const BUS_PREFIX: &str = "hosttool-";
/// The system agent, as a calling app.
pub const SYSTEM: &str = "system";
/// The Terminal's shareable tool (ADR 0004 §10, §12).
pub const TERMINAL_RUN: &str = "terminal.run";
/// Developer mode's command tool (§13).
pub const DEV_RUN: &str = "dev.run";
/// octos's own tools that run commands: always a live approval, never a
/// standing rule (§8, §12).
pub const OCTOS_COMMANDS: &[&str] = &["shell", "bash", "exec", "run_command"];

/// A script app's peer is `card.<app id>`; its tools are `<app id>.*`.
pub fn app_of_peer(app: &str) -> &str {
    app.strip_prefix(crate::ai_host::contained::PEER_PREFIX).unwrap_or(app)
}

/// What the relay is told.
#[derive(Clone, Debug)]
pub enum Event {
    /// A `peer/tool/call`, stamped by its host (a broker or the system chat).
    Call { call: HostToolCall, reply: ToolReply },
    /// The kernel cancelled a call, or its connection closed.
    Cancel { call_id: String, reason: String },
    /// A `host_tool` approval raised on `app`'s peer (or a context of it).
    Approval { app: String, account: Option<String>, approval: HostToolApproval, answer: ApprovalAnswer },
    /// The approval router decided one of the relay's requests.
    Decision { id: RequestId, decision: Decision, reason: String },
    /// A process app's peer link answered (`None`: it acknowledged).
    LinkOutcome { app: String, call_id: String, result: Option<ToolCallResult> },
    /// The AI bus answered one of the shell's own calls.
    BusResult { call_id: String, outcome: ToolOutcome },
}

/// Everything the relay does to the rest of the shell.
pub trait Env {
    /// The person allowed `app`'s agent (or developer mode did).
    fn consent(&self, app: &str) -> bool;
    /// Developer mode covers `app` (every grant).
    fn grants_all(&self, app: &str) -> bool;
    /// `app`'s account is signed out or removed (ADR 0004 §11).
    fn suspended(&self, app: &str, account: Option<&str>) -> bool;
    /// The host tools the system agent is granted now.
    fn system_tools(&self) -> BTreeSet<String>;
    /// (auto_approvable, command) for an owning app's tool.
    fn tool_rule(&self, owner: &str, tool: &str) -> (bool, bool);
    fn request_approval(&mut self, app: &str, tool: ToolSpec, args: Value, caller: Caller, context: RequestContext) -> Route;
    /// A process of `app` holds a peer link.
    fn has_link(&self, app: &str) -> bool;
    fn link_call(&mut self, app: &str, call: KernelToolCall) -> Result<(), Refused>;
    fn link_cancel(&mut self, app: &str, call_id: &str);
    /// Call `tool` (its short name) of `app`'s bus service with `args`.
    fn bus_call(&mut self, call_id: &str, app: &str, tool: &str, args: String);
    fn bus_cancel(&mut self, call_id: &str);
    fn log(&mut self, line: String);
}

/// Which apps declare which tools, and which are granted to whom.
#[derive(Default)]
pub struct Catalog {
    /// `tools.json` entries by owning app.
    tools: BTreeMap<String, Vec<Value>>,
    /// Cross-app grants: calling app → declared tool names of other apps.
    grants: BTreeMap<String, BTreeSet<String>>,
}

impl Catalog {
    /// With the tools the shell itself knows: the Terminal's `run`.
    pub fn shipped() -> Catalog {
        let mut c = Catalog::default();
        c.declare("terminal", vec![terminal_run_declaration()]);
        c
    }
    /// An app's `tools.json` (replaces what it declared before).
    pub fn declare(&mut self, app: &str, entries: Vec<Value>) {
        self.tools.insert(app.to_string(), entries);
    }
    /// A grant of another app's shareable tool to `caller`.
    pub fn grant(&mut self, caller: &str, tool: &str) {
        self.grants.entry(caller.to_string()).or_default().insert(tool.to_string());
    }
    pub fn entry(&self, owner: &str, tool: &str) -> Option<&Value> {
        self.tools.get(owner)?.iter().find(|e| e["name"] == tool)
    }
    fn shareable(entry: &Value) -> bool {
        entry["shareable"] == true
    }
    /// Whether `caller`'s agent may call `owner`'s `tool`.
    pub fn may_call(&self, caller: &str, owner: &str, tool: &str, dev_all: bool) -> bool {
        let Some(entry) = self.entry(owner, tool) else { return false };
        if owner == caller {
            return true;
        }
        Self::shareable(entry) && (dev_all || self.grants.get(caller).is_some_and(|g| g.contains(tool)))
    }
    /// What `app`'s peer registers: its own tools, and the shareable tools of
    /// other apps granted to it, each naming its owner.
    pub fn declarations(&self, app: &str, dev_all: bool) -> Vec<Value> {
        let mut out: Vec<Value> = self.tools.get(app).into_iter().flatten().filter_map(|e| host_tools::declaration(e, None)).collect();
        for (owner, entries) in &self.tools {
            if owner == app {
                continue;
            }
            for entry in entries {
                let name = entry["name"].as_str().unwrap_or("");
                if self.may_call(app, owner, name, dev_all) {
                    out.extend(host_tools::declaration(entry, Some(owner)));
                }
            }
        }
        out
    }
}

/// `terminal.run` as the Terminal declares it for the host: destructive, the
/// shell's sheet, shareable; `auto_approvable: false` is the shell's rule
/// (`native-apps.json` `agent.tool_policy`), never a declaration field.
pub fn terminal_run_declaration() -> Value {
    serde_json::json!({
        "name": TERMINAL_RUN,
        "app": "terminal",
        "description": "Type a command followed by Enter into the person's live Terminal. The person approves each command first, on a sheet that shows it exactly; it then runs for real, unsandboxed, in the terminal they see. It returns at once: the output is on the Terminal's screen.",
        "input_schema": {"type": "object", "properties": {"command": {"type": "string", "maxLength": 4096}}, "required": ["command"], "additionalProperties": false},
        "risk": "destructive",
        "confirm": "host",
        "shareable": true,
    })
}

#[derive(Clone, Debug, PartialEq)]
enum At {
    /// On the owning app's sheet (through the router); runs when approved.
    Confirming(Box<Target>),
    Link(String),
    Bus,
    Executor(String),
}

#[derive(Clone, Debug, PartialEq)]
enum Target {
    Executor(String),
    Bus,
}

struct Pending {
    call: HostToolCall,
    reply: ToolReply,
    at: At,
}

/// The relay.
pub struct Relay {
    pub catalog: Catalog,
    executors: HashMap<String, Arc<dyn ToolExecutor>>,
    calls: HashMap<String, Pending>,
    approvals: HashMap<String, ApprovalAnswer>,
}

impl Default for Relay {
    fn default() -> Self {
        Relay { catalog: Catalog::shipped(), executors: HashMap::new(), calls: HashMap::new(), approvals: HashMap::new() }
    }
}

/// What started a turn, as its host stamped it (G2): never "the person"
/// unless the host that started the turn saw the person ask. A turn nobody
/// vouched for is [`Trigger::Unknown`], which standing rules skip unless
/// they opt in, like incoming content (ADR 0004 §8).
pub fn trigger_of(stamped: &TurnTrigger) -> Trigger {
    match stamped {
        TurnTrigger::Person => Trigger::Person,
        TurnTrigger::App => Trigger::App,
        TurnTrigger::Incoming { from } => Trigger::IncomingContent { from: from.clone() },
        TurnTrigger::SystemAgent => Trigger::SystemAgent,
        TurnTrigger::Unknown => Trigger::Unknown,
    }
}

/// A call's trigger: a `peer/input` turn is the system agent's whatever was
/// stamped; otherwise the host's stamp.
fn trigger(call: &HostToolCall) -> Trigger {
    match call.origin {
        CallOrigin::PeerInput => Trigger::SystemAgent,
        _ => trigger_of(&call.trigger),
    }
}

fn error_of(result: &str) -> ToolOutcome {
    let (kind, message) = match result.split_once(':') {
        Some((kind, rest)) if !kind.contains(' ') => (kind, rest.trim()),
        _ => ("app_error", result),
    };
    ToolOutcome::error(kind, message)
}

impl Relay {
    pub fn set_executor(&mut self, app: &str, executor: Option<Arc<dyn ToolExecutor>>) {
        match executor {
            Some(e) => {
                self.executors.insert(app.to_string(), e);
            }
            None => {
                self.executors.remove(app);
            }
        }
    }

    pub fn has_executor(&self, app: &str) -> bool {
        self.executors.contains_key(app)
    }

    /// Calls not answered yet.
    pub fn pending(&self) -> usize {
        self.calls.values().filter(|p| p.reply.is_open()).count()
    }

    pub fn handle(&mut self, event: Event, env: &mut dyn Env) {
        match event {
            Event::Call { call, reply } => self.call(call, reply, env),
            Event::Cancel { call_id, reason } => self.cancel(&call_id, &reason, env),
            Event::Approval { app, account, approval, answer } => self.approval(&app, account, approval, answer, env),
            Event::Decision { id, decision, reason } => self.decided(&id, decision, &reason, env),
            Event::LinkOutcome { call_id, result, .. } => self.link_outcome(&call_id, result),
            Event::BusResult { call_id, outcome } => {
                if let Some(p) = self.calls.remove(&call_id) {
                    p.reply.finish(outcome);
                }
            }
        }
        // Executors answer on their own; forget what they finished.
        self.calls.retain(|_, p| p.reply.is_open());
    }

    fn call(&mut self, call: HostToolCall, reply: ToolReply, env: &mut dyn Env) {
        if !reply.is_open() {
            return;
        }
        let owner = call.app.clone();
        let tool = call.name.clone();
        let calling = app_of_peer(&call.calling_app).to_string();
        // 1. Authorize by (owning app, tool) and caller.
        let (caller, granted) = match call.caller_kind {
            CallerKind::System => (Caller::SystemAgent, env.system_tools().contains(&tool) || env.grants_all(SYSTEM)),
            CallerKind::AppPeer if calling == owner => (Caller::OwnAgent { client: call.client.clone() }, self.catalog.entry(&owner, &tool).is_some() || env.grants_all(&owner)),
            CallerKind::AppPeer => {
                let dev = env.grants_all(&calling);
                (Caller::AppAgent { app: calling.clone() }, self.catalog.may_call(&calling, &owner, &tool, dev) || (dev && self.catalog.entry(&owner, &tool).is_none()))
            }
        };
        let refuse = |reply: &ToolReply, kind: &str, message: String| {
            reply.finish(ToolOutcome::error(kind, message));
        };
        if !granted {
            env.log(format!("host tools: {} refused {tool} for {} (not granted)", owner, caller.as_audit()));
            return refuse(&reply, "not_granted", format!("{tool} is not granted to {}", crate::approvals::sheet::caller_label(&owner, &caller)));
        }
        if call.caller_kind == CallerKind::AppPeer {
            if !env.consent(&calling) {
                return refuse(&reply, "consent_pending", "the person has not allowed this app's agent".into());
            }
            if env.suspended(&call.calling_app, call.account.as_deref()) {
                return refuse(&reply, "signed_out", "the account is signed out".into());
            }
        }
        // 2. Route to the owning app's executor.
        if env.has_link(&owner) {
            let kernel_call = KernelToolCall {
                call_id: call.call_id.clone(),
                name: tool.clone(),
                args: call.args.clone(),
                risk: match call.risk.as_str() {
                    "read" => Risk::Read,
                    "destructive" => Risk::Destructive,
                    _ => Risk::Act,
                },
                timeout_ms: call.timeout_ms,
                context_id: call.context_id.clone(),
                caller,
                trigger: trigger(&call),
                outcome_unknown: false,
                approved: !call.confirm_required,
                confirm_required: call.confirm_required,
            };
            let call_id = call.call_id.clone();
            self.calls.insert(call_id.clone(), Pending { call, reply: reply.clone(), at: At::Link(owner.clone()) });
            if let Err(why) = env.link_call(&owner, kernel_call) {
                self.calls.remove(&call_id);
                let (kind, message) = match why {
                    Refused::NotConnected => ("app_not_running", format!("{} isn't running", crate::approvals::sheet::app_label(&owner))),
                    Refused::Duplicate => ("duplicate", "the call is already running".to_string()),
                    Refused::UnknownContext => ("unknown_context", "a request context the app did not open".to_string()),
                    Refused::Declined(why) => ("declined", why),
                };
                refuse(&reply, kind, message);
            }
            return;
        }
        let target = if self.executors.contains_key(&owner) {
            Target::Executor(owner.clone())
        } else if tool == TERMINAL_RUN && owner == "terminal" {
            Target::Bus
        } else {
            return refuse(&reply, "app_not_running", format!("{} isn't running", crate::approvals::sheet::app_label(&owner)));
        };
        // 3. `confirm: app`: acknowledge, then the owning app's own sheet.
        if call.confirm_required {
            reply.acknowledge();
            let (auto, _) = env.tool_rule(&owner, &tool);
            let mut spec = ToolSpec::app(&tool);
            spec.auto_approvable = auto;
            let id = format!("{CONFIRM_PREFIX}{}", call.call_id);
            let context = RequestContext {
                call_id: id.clone(),
                trigger: trigger(&call),
                context_id: call.context_id.clone(),
                account: call.account.clone(),
                ..RequestContext::default()
            };
            let args = call.args.clone();
            let call_id = call.call_id.clone();
            self.calls.insert(call_id.clone(), Pending { call, reply: reply.clone(), at: At::Confirming(Box::new(target)) });
            if let Route::Refused(why) = env.request_approval(&owner, spec, args, caller, context) {
                self.calls.remove(&call_id);
                refuse(&reply, "declined", why);
            }
            return;
        }
        self.run(call, reply, target, env);
    }

    /// Execute, once.
    fn run(&mut self, call: HostToolCall, reply: ToolReply, target: Target, env: &mut dyn Env) {
        if !reply.is_open() {
            return;
        }
        let call_id = call.call_id.clone();
        match target {
            Target::Executor(owner) => {
                let Some(executor) = self.executors.get(&owner).cloned() else {
                    reply.finish(ToolOutcome::error("app_not_running", format!("{} isn't running", crate::approvals::sheet::app_label(&owner))));
                    return;
                };
                self.calls.insert(call_id, Pending { call: call.clone(), reply: reply.clone(), at: At::Executor(owner) });
                executor.execute(call, reply);
            }
            Target::Bus => {
                let short = call.name.split_once('.').map(|(_, t)| t).unwrap_or(&call.name).to_string();
                let bus_id = format!("{BUS_PREFIX}{call_id}");
                let args = call.args.to_string();
                let app = call.app.clone();
                self.calls.insert(call_id, Pending { call, reply, at: At::Bus });
                env.bus_call(&bus_id, &app, &short, args);
            }
        }
    }

    fn cancel(&mut self, call_id: &str, reason: &str, env: &mut dyn Env) {
        let Some(p) = self.calls.remove(call_id) else { return };
        p.reply.cancel();
        match p.at {
            At::Link(owner) => env.link_cancel(&owner, call_id),
            At::Bus => env.bus_cancel(&format!("{BUS_PREFIX}{call_id}")),
            At::Executor(owner) => {
                if let Some(e) = self.executors.get(&owner) {
                    e.cancel(call_id);
                }
            }
            At::Confirming(_) => {}
        }
        env.log(format!("host tools: {} ({}) cancelled: {reason}", p.call.name, call_id));
    }

    fn approval(&mut self, app: &str, account: Option<String>, approval: HostToolApproval, answer: ApprovalAnswer, env: &mut dyn Env) {
        let calling = app_of_peer(app).to_string();
        // octos's own tool approval on an app's peer or context is the app
        // agent's call on a tool the app owns; a `host_tool` one names its
        // owning app.
        let owner = if approval.octos { calling.clone() } else { approval.app.clone() };
        let (auto, command) = env.tool_rule(&owner, &approval.tool);
        let mut spec = ToolSpec::host(&approval.tool);
        spec.auto_approvable = auto;
        if command || approval.tool == TERMINAL_RUN || approval.tool == DEV_RUN || (approval.octos && OCTOS_COMMANDS.contains(&approval.tool.as_str())) {
            spec = spec.command();
        }
        let caller = match approval.calling_kind {
            CallerKind::System => Caller::SystemAgent,
            CallerKind::AppPeer if calling == owner => Caller::OwnAgent { client: approval.client.clone() },
            CallerKind::AppPeer => Caller::AppAgent { app: calling },
        };
        let id = format!("{APPROVAL_PREFIX}{}", approval.approval_id);
        let context = RequestContext {
            call_id: id.clone(),
            trigger: trigger_of(&approval.trigger),
            context_id: approval.context_id.clone(),
            account,
            outcome_unknown: approval.outcome_unknown_before,
            ..RequestContext::default()
        };
        self.approvals.insert(id.clone(), answer.clone());
        if let Route::Refused(why) | Route::LeftToClient(why) = env.request_approval(&owner, spec, approval.args.clone(), caller, context) {
            self.approvals.remove(&id);
            answer.respond(false);
            env.log(format!("host tools: approval {} refused: {why}", approval.approval_id));
        }
    }

    fn decided(&mut self, id: &RequestId, decision: Decision, reason: &str, env: &mut dyn Env) {
        if let Some(answer) = self.approvals.remove(&id.0) {
            answer.respond(decision.approved());
            return;
        }
        let Some(call_id) = id.0.strip_prefix(CONFIRM_PREFIX) else { return };
        let Some(p) = self.calls.remove(call_id) else { return };
        let At::Confirming(target) = p.at else { return };
        if decision.approved() {
            self.run(p.call, p.reply, *target, env);
        } else {
            p.reply.finish(ToolOutcome::error("declined", reason.to_string()));
        }
    }

    fn link_outcome(&mut self, call_id: &str, result: Option<ToolCallResult>) {
        match result {
            None => {
                if let Some(p) = self.calls.get(call_id) {
                    p.reply.acknowledge();
                }
            }
            Some(result) => {
                let Some(p) = self.calls.remove(call_id) else { return };
                p.reply.finish(match result {
                    ToolCallResult::Ok(data) => ToolOutcome::Ok(data),
                    ToolCallResult::Error(e) => error_of(&e),
                    ToolCallResult::OutcomeUnknown => ToolOutcome::error("outcome_unknown", "the app stopped while it ran the call; it may or may not have happened"),
                });
            }
        }
    }
}
