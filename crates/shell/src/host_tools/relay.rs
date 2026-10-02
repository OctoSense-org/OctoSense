//! The relay's logic, without globals or I/O: every effect goes through
//! [`Env`], so the tests drive it with their own.
//!
//! One [`Relay::handle`] per [`Event`], on the UI thread. For a call:
//!
//! 0. **Check** (ADR 0004 §3, G8): the arguments against the tool's
//!    declared `input_schema` (and a size cap), and the calling agent's
//!    budget (calls per turn and per day, from its manifest or the
//!    defaults); the result, on its way back, against its `output_schema`
//!    and [`MAX_RESULT_BYTES`] ([`schema`](super::schema)).
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
//! **Audit** (ADR 0004 §8, §12, §13): every call is recorded when it
//! arrives and when it ends, answered, refused or cancelled ([`CallAudit`]:
//! caller, owning app, tool, argument digest, outcome) through the relay's
//! one audit sink ([`Relay::set_audit`]; the shell's writes
//! `logs/tool-calls.jsonl`, owner-only). `dev.run` and `terminal.run` are
//! calls like any other, so they are audited here too.
//!
//! `host_tool` approvals (the kernel's `confirm: host` sheets) go to the
//! router with the owning app, the exact arguments and the caller, and its
//! decision answers the kernel.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use serde_json::Value;

use crate::ai_host::app_peers::host_tools::{self, ApprovalAnswer, CallOrigin, CallerKind, HostToolApproval, HostToolCall, ToolExecutor, ToolOutcome, ToolReply};
use crate::ai_host::app_peers::TurnTrigger;
pub use crate::approvals::audit::CallAudit;
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
/// The Terminal, whose tools the relay runs on the AI bus.
pub const TERMINAL_APP: &str = "terminal";
/// Developer mode's command tool (§13), run by the shell ([`super::dev_run`]).
pub const DEV_RUN: &str = "dev.run";
/// The executor key of the tools the shell itself runs for an app's own
/// agent (`dev.run`): never an app id.
pub const HOST_EXECUTOR: &str = "@shell";

/// Whether `tool` is one the shell runs itself, as the calling app's own
/// tool, whatever its declaration names (never routed to the app).
pub fn is_host_run(tool: &str) -> bool {
    tool == DEV_RUN || super::files::TOOLS.contains(&tool)
}

/// A tool the shell runs, as `owner` declares it (its schemas).
fn host_run_declaration(owner: &str, tool: &str) -> Option<Value> {
    if tool == DEV_RUN {
        return Some(super::dev_run::declaration(owner));
    }
    super::files::declarations(owner).into_iter().find(|d| d["name"] == tool)
}
/// The system toolbox (ADR 0002 §6), the owning app of the toolbox tools
/// (`workflow.run`, `toolbox.search`, …; [`super::toolbox`] with the
/// `toolbox-peers` feature).
pub const TOOLBOX: &str = "toolbox";

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
    /// The turn that raised a `host_tool` approval ended before it was
    /// answered (the broker's `host_tool_approval_closed`).
    ApprovalClosed { approval_id: String },
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
    /// Withdraw a request the router holds (its turn ended unanswered).
    fn withdraw_approval(&mut self, _id: &RequestId, _reason: &str) {}
    /// A process of `app` holds a peer link.
    fn has_link(&self, app: &str) -> bool;
    fn link_call(&mut self, app: &str, call: KernelToolCall) -> Result<(), Refused>;
    fn link_cancel(&mut self, app: &str, call_id: &str);
    /// Call `tool` (its short name) of `app`'s bus service with `args`.
    fn bus_call(&mut self, call_id: &str, app: &str, tool: &str, args: String);
    fn bus_cancel(&mut self, call_id: &str);
    fn log(&mut self, line: String);
    /// Unix seconds (the day a budget counts in).
    fn now(&self) -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
    }
}

/// The largest arguments a call may carry.
pub const MAX_ARGS_BYTES: usize = 64 * 1024;
/// The largest result the relay hands back (octos's own default cap).
pub const MAX_RESULT_BYTES: usize = 256 * 1024;
/// An agent's tool calls per turn and per day, unless its manifest says
/// otherwise (`native-apps.json` `agent.budget`).
pub const DEFAULT_CALLS_PER_TURN: u32 = 32;
pub const DEFAULT_CALLS_PER_DAY: u32 = 1000;

/// One calling agent's budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub per_turn: u32,
    pub per_day: u32,
}

impl Default for Budget {
    fn default() -> Self {
        Budget { per_turn: DEFAULT_CALLS_PER_TURN, per_day: DEFAULT_CALLS_PER_DAY }
    }
}

/// What one calling agent spent.
#[derive(Default)]
struct Usage {
    day: u64,
    today: u32,
    /// Calls per turn, the newest turns only.
    turns: std::collections::VecDeque<(String, u32)>,
}

/// The octos kernel tools an app agent may be granted under developer
/// mode ("everything granted", ADR 0004 §13): its workspace's files, the
/// person, memory, the web, tool discovery. Never octos's shell (§12).
pub const DEV_GENERIC_TOOLS: &[&str] = &[
    "read_file",
    "write_file",
    "edit_file",
    "diff_edit",
    "apply_patch",
    "glob",
    "grep",
    "list_dir",
    "code_structure",
    "check_workspace_contract",
    "ask_user_question",
    "view_image",
    "view_video",
    "recall",
    "recall_memory",
    "memory_search",
    "memory_load",
    "save_memory",
    "memory_note",
    "web_search",
    "web_fetch",
    "deep_search",
    "tool_search",
];

/// octos's own shell and its aliases: never in an app agent's kernel tools
/// (ADR 0004 §12; command execution is a host tool with a live approval).
pub const OCTOS_SHELL: &[&str] = &["shell", "bash", "exec_command", "write_stdin", "group:runtime"];

/// Which apps declare which tools, which are granted to whom, and which
/// octos kernel tools each app's agent keeps.
///
/// **The seams** (ADR 0004 §7, §12; #108's toolbox tools use the same):
/// [`Catalog::declare`] an owning app's `tools.json` entries,
/// [`Catalog::grant`] a caller one of another app's shareable tools (marked
/// with its owner), [`Catalog::set_generic`] the exact kernel tools an
/// app's agent keeps; the relay then routes each call to the owning app's
/// executor ([`Relay::set_executor`]).
#[derive(Default)]
pub struct Catalog {
    /// `tools.json` entries by owning app.
    tools: BTreeMap<String, Vec<Value>>,
    /// Cross-app grants: calling app → (owning app, tool).
    grants: BTreeMap<String, BTreeSet<(String, String)>>,
    /// Exactly the octos kernel tools each app's agent keeps.
    generic: BTreeMap<String, Vec<String>>,
    /// Each calling agent's budget (the defaults unless set).
    budgets: BTreeMap<String, Budget>,
}

impl Catalog {
    /// With what the native apps' reviewed entries declare
    /// (`native-apps.json` `agent`): the Terminal's tools, every native
    /// app's grants and kernel tools.
    pub fn shipped() -> Catalog {
        let mut c = Catalog::default();
        for app in crate::native_apps::APPS {
            c.load_native(app);
        }
        c
    }

    /// One native app's agent block.
    pub fn load_native(&mut self, app: &crate::native_apps::NativeApp) {
        let tools: Vec<Value> = serde_json::from_str(app.tools_json).unwrap_or_default();
        if !tools.is_empty() {
            self.declare(app.id, tools);
        }
        for (owner, tool) in app.grants {
            self.grant(app.id, owner, tool);
        }
        self.set_generic(app.id, app.generic_tools.iter().map(|t| t.to_string()).collect());
        self.set_budget(app.id, app.calls_per_turn, app.calls_per_day);
    }

    /// `app`'s agent's budget; `None` keeps the default.
    pub fn set_budget(&mut self, app: &str, per_turn: Option<u32>, per_day: Option<u32>) {
        let d = Budget::default();
        self.budgets.insert(app.to_string(), Budget { per_turn: per_turn.unwrap_or(d.per_turn), per_day: per_day.unwrap_or(d.per_day) });
    }

    pub fn budget(&self, app: &str) -> Budget {
        self.budgets.get(app).copied().unwrap_or_default()
    }

    /// An app's `tools.json` (replaces what it declared before).
    pub fn declare(&mut self, app: &str, entries: Vec<Value>) {
        self.tools.insert(app.to_string(), entries);
    }

    /// A grant of `owner`'s shareable `tool` to `caller`'s agent.
    pub fn grant(&mut self, caller: &str, owner: &str, tool: &str) {
        self.grants.entry(caller.to_string()).or_default().insert((owner.to_string(), tool.to_string()));
    }
    /// `caller`'s grants of `owner`'s tools become exactly `tools` (a
    /// toolbox grant computed again).
    pub fn set_grants(&mut self, caller: &str, owner: &str, tools: &[&str]) {
        let grants = self.grants.entry(caller.to_string()).or_default();
        grants.retain(|(o, _)| o != owner);
        grants.extend(tools.iter().map(|t| (owner.to_string(), t.to_string())));
    }

    /// Exactly the octos kernel tools `app`'s agent keeps; octos's shell is
    /// dropped whatever the list says.
    pub fn set_generic(&mut self, app: &str, tools: Vec<String>) {
        let tools = tools.into_iter().filter(|t| !OCTOS_SHELL.contains(&t.as_str())).collect();
        self.generic.insert(app.to_string(), tools);
    }

    /// The kernel tools `app`'s agent keeps (none unless granted);
    /// developer mode's set when it covers the app.
    pub fn generic(&self, app: &str, dev_all: bool) -> Vec<String> {
        if dev_all {
            return DEV_GENERIC_TOOLS.iter().map(|t| t.to_string()).collect();
        }
        self.generic.get(app).cloned().unwrap_or_default()
    }

    /// Whether anything was declared, granted or set for `app`.
    pub fn knows(&self, app: &str) -> bool {
        self.tools.contains_key(app) || self.grants.contains_key(app) || self.generic.contains_key(app)
    }

    pub fn entry(&self, owner: &str, tool: &str) -> Option<&Value> {
        self.tools.get(owner)?.iter().find(|e| e["name"] == tool)
    }

    /// The owning app of a tool another app is granted, resolved
    /// explicitly by its namespace (ADR 0004 §7): the toolbox for its own
    /// (`toolbox.*`, `workflow.*`), the native app of that id (`terminal.run`
    /// → `terminal`), else the system app of the namespace (`mail.send` →
    /// `os.mail`). Never whichever app declared the name first: a store app
    /// may declare `mail.send` for itself, and owns only its own. `None` for
    /// a name without a namespace (a kernel tool).
    pub fn owner_of(&self, tool: &str) -> Option<String> {
        let (ns, _) = tool.split_once('.')?;
        if ns.is_empty() {
            return None;
        }
        if ns == TOOLBOX || ns == "workflow" {
            return Some(TOOLBOX.to_string());
        }
        if crate::native_apps::find(ns).is_some() {
            return Some(ns.to_string());
        }
        Some(format!("os.{ns}"))
    }

    fn shareable(entry: &Value) -> bool {
        entry["shareable"] == true
    }

    /// Whether `caller`'s agent may call `owner`'s `tool`. Developer mode
    /// grants every shareable tool, except the toolbox's: those run under
    /// the grant's scope, which developer mode cannot invent.
    pub fn may_call(&self, caller: &str, owner: &str, tool: &str, dev_all: bool) -> bool {
        let Some(entry) = self.entry(owner, tool) else { return false };
        if owner == caller {
            return true;
        }
        let dev_all = dev_all && owner != TOOLBOX;
        Self::shareable(entry) && (dev_all || self.grants.get(caller).is_some_and(|g| g.contains(&(owner.to_string(), tool.to_string()))))
    }

    /// What `app`'s peer registers: its own tools, and the shareable tools of
    /// other apps granted to it; each names its owning app (`app`).
    pub fn declarations(&self, app: &str, dev_all: bool) -> Vec<Value> {
        let mut out: Vec<Value> = self.tools.get(app).into_iter().flatten().filter_map(|e| host_tools::declaration(e, Some(app))).collect();
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
    /// What `app`'s peer is offered now: [`Catalog::declarations`], without
    /// the toolbox's tools until the person allowed `app`'s agent (the #120
    /// first-use consent; `consented`).
    ///
    /// Under developer mode, also `dev.run` as the app's own tool: only here,
    /// where the brokers register app peers on the shell's host connection
    /// (never the system agent's session, which a Talk to Octos client can
    /// reach; ADR 0004 §13).
    pub fn offered(&self, app: &str, dev_all: bool, consented: bool) -> Vec<Value> {
        let mut out = self.declarations(app, dev_all);
        if !consented {
            out.retain(|d| d["app"] != TOOLBOX);
        }
        out.retain(|d| !d["name"].as_str().is_some_and(is_host_run));
        if dev_all && app != SYSTEM {
            out.push(super::dev_run::declaration(app));
        }
        out
    }
}

/// `terminal.run` as the Terminal declares it (`native-apps.json`):
/// destructive, the shell's sheet, shareable; `auto_approvable: false` is
/// the shell's rule (`agent.tool_policy`), never a declaration field.
pub fn terminal_run_declaration() -> Value {
    Catalog::shipped().entry("terminal", TERMINAL_RUN).cloned().expect("native-apps.json declares terminal.run")
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

/// Where the relay's audit lines go.
pub type AuditSink = Arc<dyn Fn(CallAudit) + Send + Sync>;

/// The audit's one clock (every phase of every call, the AI bus pane's
/// too): Unix seconds of the system clock.
pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `reply`, audited: its answer (a result or an error) is recorded once, as
/// `record` with phase `done`. Acknowledgements pass through.
fn audited_reply(reply: ToolReply, record: CallAudit, sink: AuditSink) -> ToolReply {
    let outer = reply.clone();
    ToolReply::new(reply.call_id().to_string(), move |fields: Value| {
        if fields.get("status").is_some() {
            outer.acknowledge();
            return;
        }
        let (outcome, audit) = if fields["ok"] == true {
            (ToolOutcome::Ok(fields.get("data").cloned().unwrap_or(Value::Null)), "ok".to_string())
        } else {
            let kind = fields["error"]["kind"].as_str().unwrap_or("error").to_string();
            (ToolOutcome::error(&kind, fields["error"]["message"].as_str().unwrap_or("").to_string()), format!("error:{kind}"))
        };
        if outer.finish(outcome) {
            sink(CallAudit { ts: unix_now(), phase: "done".into(), outcome: audit, ..record.clone() });
        }
    })
}

/// The relay.
pub struct Relay {
    pub catalog: Catalog,
    executors: HashMap<String, Arc<dyn ToolExecutor>>,
    calls: HashMap<String, Pending>,
    approvals: HashMap<String, ApprovalAnswer>,
    /// Each calling agent's spending against its budget.
    usage: HashMap<String, Usage>,
    /// The caller's own reply for each checked one (a cancel closes both).
    outers: HashMap<String, ToolReply>,
    /// Every call's audit line (ADR 0004 §8); none in a bare relay.
    audit: Option<AuditSink>,
    /// The audit line of each call still open, for its cancel.
    audited: HashMap<String, CallAudit>,
}

impl Default for Relay {
    fn default() -> Self {
        Relay {
            catalog: Catalog::shipped(),
            executors: HashMap::new(),
            calls: HashMap::new(),
            approvals: HashMap::new(),
            usage: HashMap::new(),
            outers: HashMap::new(),
            audit: None,
            audited: HashMap::new(),
        }
    }
}

/// How many turns' counts are kept per agent.
const TURNS_KEPT: usize = 64;

/// `reply`, with the result checked on its way back (G8): at most
/// `max_bytes`, and matching `schema` (the tool's `output_schema`) when it
/// declares one. Acknowledgements and errors pass through.
fn checked_reply(reply: ToolReply, tool: &str, schema: Option<Value>, max_bytes: usize) -> ToolReply {
    let outer = reply.clone();
    let tool = tool.to_string();
    ToolReply::new(reply.call_id().to_string(), move |fields: Value| {
        if fields.get("status").is_some() {
            outer.acknowledge();
            return;
        }
        if fields["ok"] != true {
            let kind = fields["error"]["kind"].as_str().unwrap_or("error");
            outer.finish(ToolOutcome::error(kind, fields["error"]["message"].as_str().unwrap_or("").to_string()));
            return;
        }
        let data = fields.get("data").cloned().unwrap_or(Value::Null);
        let size = data.to_string().len();
        let outcome = if size > max_bytes {
            ToolOutcome::error("result_too_large", format!("{tool} answered {size} bytes, over the {max_bytes}-byte cap"))
        } else if let Some(Err(why)) = schema.as_ref().map(|s| super::schema::check(s, &data)) {
            ToolOutcome::error("invalid_result", format!("{tool} answered outside its declared result: {why}"))
        } else {
            ToolOutcome::Ok(data)
        };
        outer.finish(outcome);
    })
}

/// What started a turn, as its host stamped it (G2): never "the person"
/// unless the host that started the turn saw the person ask. A turn nobody
/// vouched for is [`Trigger::Unknown`], which standing rules skip unless
/// they opt in, like incoming content (ADR 0004 §8).
pub fn trigger_of(stamped: &TurnTrigger) -> Trigger {
    match stamped {
        TurnTrigger::Person => Trigger::Person,
        // The app's word that the person asked: its run, not the person's
        // (only a shell surface vouches for the person).
        TurnTrigger::AppSaysPerson | TurnTrigger::App => Trigger::App,
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
    /// Where every call's audit lines go.
    pub fn set_audit(&mut self, sink: AuditSink) {
        self.audit = Some(sink);
    }

    /// (auto_approvable, command) for `owner`'s `tool`: the host's rule
    /// (`native-apps.json` `tool_policy`, commands), and a script app's own
    /// `tools.json` declaration (App Hub's `auto_approvable`), whichever is
    /// stricter.
    fn tool_rule(&self, env: &dyn Env, owner: &str, tool: &str) -> (bool, bool) {
        let (auto, command) = env.tool_rule(owner, tool);
        let declared = self.catalog.entry(owner, tool).and_then(|e| e.get("auto_approvable")).and_then(Value::as_bool).unwrap_or(true);
        (auto && declared, command)
    }

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
            Event::ApprovalClosed { approval_id } => {
                // The kernel dropped it: nothing is answered, and the sheet
                // stops asking.
                let id = format!("{APPROVAL_PREFIX}{approval_id}");
                if self.approvals.remove(&id).is_some() {
                    env.withdraw_approval(&RequestId(id), "its turn ended before anyone answered");
                }
            }
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
        let calls = &self.calls;
        self.outers.retain(|id, outer| outer.is_open() && calls.contains_key(id));
        let outers = &self.outers;
        self.audited.retain(|id, _| calls.contains_key(id) || outers.contains_key(id));
    }

    /// Spend one call of `agent`'s budget in `turn`; why not, when spent.
    fn spend(&mut self, agent: &str, turn: &str, now: u64) -> Result<(), String> {
        let budget = self.catalog.budget(agent);
        let usage = self.usage.entry(agent.to_string()).or_default();
        let day = now / 86_400;
        if usage.day != day {
            usage.day = day;
            usage.today = 0;
        }
        if usage.today >= budget.per_day {
            return Err(format!("its agent used its {} tool calls for today", budget.per_day));
        }
        let this_turn = usage.turns.iter().find(|(t, _)| t == turn).map_or(0, |(_, n)| *n);
        if !turn.is_empty() && this_turn >= budget.per_turn {
            return Err(format!("its agent used its {} tool calls for this turn", budget.per_turn));
        }
        usage.today += 1;
        match usage.turns.iter_mut().find(|(t, _)| t == turn) {
            Some((_, n)) => *n += 1,
            None => {
                usage.turns.push_back((turn.to_string(), 1));
                while usage.turns.len() > TURNS_KEPT {
                    usage.turns.pop_front();
                }
            }
        }
        Ok(())
    }

    fn call(&mut self, call: HostToolCall, reply: ToolReply, env: &mut dyn Env) {
        if !reply.is_open() {
            return;
        }
        let owner = call.app.clone();
        let tool = call.name.clone();
        let calling = app_of_peer(&call.calling_app).to_string();
        // 1. Authorize by (owning app, tool) and caller.
        let host_run = is_host_run(&tool);
        let (caller, granted) = match call.caller_kind {
            // The shell's own: only the app's own agent, on its own peer;
            // `dev.run` only while developer mode covers the app.
            _ if host_run => (
                if call.caller_kind == CallerKind::System { Caller::SystemAgent } else { Caller::OwnAgent { client: call.client.clone() } },
                call.caller_kind == CallerKind::AppPeer && calling == owner && (tool != DEV_RUN || env.grants_all(&calling)),
            ),
            CallerKind::System => (Caller::SystemAgent, env.system_tools().contains(&tool) || env.grants_all(SYSTEM)),
            CallerKind::AppPeer if calling == owner => (Caller::OwnAgent { client: call.client.clone() }, self.catalog.entry(&owner, &tool).is_some() || env.grants_all(&owner)),
            CallerKind::AppPeer => {
                let dev = env.grants_all(&calling);
                (Caller::AppAgent { app: calling.clone() }, self.catalog.may_call(&calling, &owner, &tool, dev) || (dev && self.catalog.entry(&owner, &tool).is_none()))
            }
        };
        // The kernel's own reply: a cancel closes it (nothing is sent after).
        let kernel_reply = reply.clone();
        // Audited from here on: received now, and its end, whatever it is.
        let reply = match self.audit.clone() {
            Some(sink) => {
                let record = CallAudit {
                    ts: unix_now(),
                    call_id: call.call_id.clone(),
                    caller: caller.as_audit(),
                    owner: owner.clone(),
                    tool: tool.clone(),
                    args_digest: crate::approvals::facts::digest(&call.args),
                    phase: "call".into(),
                    outcome: "received".into(),
                };
                sink(record.clone());
                self.audited.insert(call.call_id.clone(), record.clone());
                audited_reply(reply, record, sink)
            }
            None => reply,
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
        // 0. The arguments against the declared schema (G8).
        let entry = if host_run { host_run_declaration(&owner, &tool) } else { self.catalog.entry(&owner, &tool).cloned() };
        let size = call.args.to_string().len();
        if size > MAX_ARGS_BYTES {
            return refuse(&reply, "invalid_args", format!("{tool}'s arguments are {size} bytes, over the {MAX_ARGS_BYTES}-byte cap"));
        }
        if let Some(schema) = entry.as_ref().and_then(|e| e.get("input_schema")) {
            if let Err(why) = super::schema::check(schema, &call.args) {
                env.log(format!("host tools: {tool} refused for {}: arguments {why}", caller.as_audit()));
                return refuse(&reply, "invalid_args", format!("{tool}: {why}"));
            }
        }
        // ... and the calling agent's budget.
        let spender = if call.caller_kind == CallerKind::System { SYSTEM.to_string() } else { calling.clone() };
        if let Err(why) = self.spend(&spender, &call.turn_id, env.now()) {
            env.log(format!("host tools: {tool} refused for {}: {why}", caller.as_audit()));
            return refuse(&reply, "budget_exceeded", why);
        }
        // The result is checked on its way back.
        let reply = checked_reply(reply, &tool, entry.as_ref().and_then(|e| e.get("output_schema")).filter(|s| !s.is_null()).cloned(), MAX_RESULT_BYTES);
        self.outers.insert(call.call_id.clone(), kernel_reply);
        // 2. Route to the owning app's executor (the shell's own for
        // `dev.run` and the host read tools).
        if host_run {
            if !self.executors.contains_key(HOST_EXECUTOR) {
                return refuse(&reply, "app_not_running", format!("nothing on this host runs {tool}"));
            }
            return self.run(call, reply, Target::Executor(HOST_EXECUTOR.to_string()), env);
        }
        // An app with its own executor (an in-process module's, a script
        // app's host service) runs its tools there, even when it also holds
        // a peer link for its conversation (#142): the link serves the tools
        // of an app that has nothing else, a process app or a module that
        // serves its tools over the link.
        if env.has_link(&owner) && !self.executors.contains_key(&owner) {
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
        } else if owner == TERMINAL_APP {
            // The Terminal's tools run on its AI bus service, in the
            // terminal the person sees.
            Target::Bus
        } else {
            return refuse(&reply, "app_not_running", format!("{} isn't running", crate::approvals::sheet::app_label(&owner)));
        };
        // 3. `confirm: app`: acknowledge, then the owning app's own sheet.
        if call.confirm_required {
            reply.acknowledge();
            let (auto, _) = self.tool_rule(&*env, &owner, &tool);
            let mut spec = ToolSpec::app(&tool);
            spec.auto_approvable = auto;
            // Rule conditions read only what the tool declares.
            if let Some(schema) = entry.as_ref().and_then(|e| e.get("input_schema")) {
                spec = spec.schema(schema.clone());
            }
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
        let mut cancelled = false;
        if let Some(outer) = self.outers.remove(call_id) {
            cancelled |= outer.cancel();
        }
        if let (Some(record), Some(sink)) = (self.audited.remove(call_id), self.audit.clone()) {
            let open = self.calls.get(call_id).is_some_and(|p| p.reply.is_open());
            if cancelled || open {
                sink(CallAudit { ts: unix_now(), phase: "done".into(), outcome: "cancelled".into(), ..record });
            }
        }
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
        let (auto, command) = self.tool_rule(&*env, &owner, &approval.tool);
        let mut spec = ToolSpec::host(&approval.tool);
        spec.auto_approvable = auto;
        // Rule conditions read only what the owning app's tool declares
        // (octos's own tools declare none here: their rules read no facts).
        if let Some(schema) = self.catalog.entry(&owner, &approval.tool).and_then(|e| e.get("input_schema")) {
            spec = spec.schema(schema.clone());
        }
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
            // The reason reaches the kernel's record (`client_note`): an
            // expiry says so ("expired: no answer in 10 min").
            answer.respond_with(decision.approved(), reason);
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
