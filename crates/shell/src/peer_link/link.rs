//! The peer links of every process app: identity, grants and consent,
//! request contexts, the tool-call half and its host obligations, and what
//! a process's death does. No I/O of its own: frames go out through each
//! link's [`FrameOut`], the shell's decisions come from a [`PeerHost`], and
//! tool results go to the [`ToolRelay`] (octos#2567's relay, home-96's).

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use serde_json::{json, Map, Value};

use super::wire::{self, Down, Outcome, Risk, ToolCallDown, Up};
use crate::ai_host::app_peers::{ContextEvent, ContextOp, ContextSpec, OctosAppService, OctosContext, OCTOS_SERVICES};
use crate::approvals::{Caller, Decision, RequestContext, RequestId, Route, ToolSpec, Trigger};
use crate::hub::ClientId;
use crate::native_apps::Confirm;

/// Sends one peer frame (JSON) to one app process. Called from provider
/// threads too (streamed turn events), so it is `Send + Sync`.
pub type FrameOut = Arc<dyn Fn(String) + Send + Sync>;

/// The account of an app that keeps no accounts (ADR 0004 §11: "device").
pub const DEVICE: &str = "device";
/// The approval ids the peer link asks the router with.
pub const HELD_PREFIX: &str = "peerlink:";

/// What the link needs from the rest of the shell. [`super::ShellHost`] is
/// the real one; the tests have their own.
pub trait PeerHost: Send {
    /// The `octos.*` services the app's reviewed `native-apps.json` entry
    /// grants it (every one in developer mode); empty: no peer link.
    fn granted(&self, app: &str) -> BTreeSet<String>;
    /// `storage.accounts`: the app names the account a context acts for.
    fn keeps_accounts(&self, app: &str) -> bool;
    /// `consent::granted(app)`; when undecided, the first-use sheet is
    /// shown (once) and this answers false until the person allows it.
    fn consent(&mut self, app: &str) -> bool;
    /// The app's assistant service (its one peer), made once and kept by
    /// the link across the process's restarts.
    fn service(&mut self, app: &str, services: &BTreeSet<String>) -> Option<Arc<dyn OctosAppService>>;
    /// `agent.tool_policy` for one tool.
    fn tool_rule(&self, app: &str, tool: &str) -> Option<(Confirm, bool)>;
    /// The #120 router: a call needs the person (or a rule, or developer mode).
    fn request_approval(&mut self, app: &str, tool: ToolSpec, args: Value, caller: Caller, context: RequestContext) -> Route;
    /// The router's decisions on this link's requests since the last call.
    fn take_decisions(&mut self) -> Vec<(RequestId, Decision, String)>;
    /// The owning app's own sheet answered a `confirm: app` request.
    fn app_confirm_answered(&mut self, id: &RequestId, approved: bool, reason: &str);
    /// A link opened (the app's own sheet can take `confirm: app` requests) or closed.
    fn link_opened(&mut self, app: &str);
    fn link_closed(&mut self, app: &str);
}

/// A call's final answer, as the relay hands it to the kernel.
#[derive(Clone, Debug, PartialEq)]
pub enum ToolCallResult {
    Ok(Value),
    Error(String),
    /// The call may or may not have happened (the process died with it in
    /// flight): octos marks it `outcome_unknown`, never retried without the person.
    OutcomeUnknown,
}

/// The seam home-96's octos#2567 relay implements: where tool-call
/// outcomes go. Each call gets at most one `acknowledged` and exactly one
/// `finished`.
pub trait ToolRelay: Send {
    /// The host took the call and a confirmation may follow (sent before
    /// any confirmation sheet is shown).
    fn acknowledged(&mut self, app: &str, call_id: &str);
    fn finished(&mut self, app: &str, call_id: &str, result: ToolCallResult);
}

/// The test double, and the queue until a relay is installed.
#[derive(Clone, Default)]
pub struct RecordingToolRelay {
    pub events: Arc<std::sync::Mutex<Vec<(String, String, Option<ToolCallResult>)>>>,
}

impl RecordingToolRelay {
    pub fn take(&self) -> Vec<(String, String, Option<ToolCallResult>)> {
        std::mem::take(&mut *self.events.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

impl ToolRelay for RecordingToolRelay {
    fn acknowledged(&mut self, app: &str, call_id: &str) {
        self.events.lock().unwrap_or_else(|e| e.into_inner()).push((app.into(), call_id.into(), None));
    }
    fn finished(&mut self, app: &str, call_id: &str, result: ToolCallResult) {
        self.events.lock().unwrap_or_else(|e| e.into_inner()).push((app.into(), call_id.into(), Some(result)));
    }
}

/// A kernel `peer/tool/call` for an app, as the relay hands it over. It
/// carries the kernel's `context_id`, never an account or client: the
/// shell stamps those from its own records.
#[derive(Clone, Debug, PartialEq)]
pub struct KernelToolCall {
    pub call_id: String,
    pub name: String,
    pub args: Value,
    pub risk: Risk,
    pub timeout_ms: u64,
    /// The kernel's request context (`None`: the agent's own session).
    pub context_id: Option<String>,
    /// Who is calling, from the relay's own connection records.
    pub caller: Caller,
    pub trigger: Trigger,
    pub outcome_unknown: bool,
    /// The kernel already holds the person's approval (a gated `confirm:
    /// host` call it asked for, UPCR-2026-035): the link asks nobody again.
    pub approved: bool,
    /// `confirm: app` (the kernel's `confirm_required`): the app's own sheet
    /// asks the person, for callers of every kind.
    pub confirm_required: bool,
}

/// Why a call was not taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// No process of the app holds a peer link now.
    NotConnected,
    /// The same call id again: once per call.
    Duplicate,
    /// A context this app never opened (or one already closed).
    UnknownContext,
    /// The router refused it outright.
    Declined(String),
}

/// Who owns a request context, as the shell recorded it at open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextOwner {
    pub app: String,
    pub client_id: ClientId,
    pub account: String,
    pub client: Option<String>,
}

struct Link {
    app: String,
    out: FrameOut,
    /// Contexts this process opened (its handles).
    contexts: Vec<String>,
}

struct Ctx {
    owner: ContextOwner,
    ctx: Arc<dyn OctosContext>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallState {
    /// Waiting for the shell's sheet (nothing sent to the app yet).
    Held,
    /// Sent to the app.
    Sent,
}

struct Call {
    client_id: ClientId,
    risk: Risk,
    state: CallState,
    confirm_required: bool,
    acked: bool,
    /// The approval id, when the router was asked.
    approval: Option<RequestId>,
    deadline: f64,
    down: ToolCallDown,
}

/// Every process app's peer link.
pub struct PeerLinks {
    host: Box<dyn PeerHost>,
    relay: Box<dyn ToolRelay>,
    links: HashMap<ClientId, Link>,
    /// One service (peer) per app, kept when its process dies.
    services: HashMap<String, Arc<dyn OctosAppService>>,
    contexts: HashMap<String, Ctx>,
    calls: HashMap<(String, String), Call>,
    next_context: u64,
    /// Lines for the shell's log (drained by the caller).
    pub log: Vec<String>,
}

fn caller_wire(caller: &Caller) -> String {
    match caller {
        Caller::OwnAgent { .. } => "own_agent".into(),
        Caller::AppAgent { app } => format!("app:{app}"),
        Caller::SystemAgent => "system_agent".into(),
    }
}

impl PeerLinks {
    pub fn new(host: Box<dyn PeerHost>, relay: Box<dyn ToolRelay>) -> PeerLinks {
        PeerLinks {
            host,
            relay,
            links: HashMap::new(),
            services: HashMap::new(),
            contexts: HashMap::new(),
            calls: HashMap::new(),
            next_context: 1,
            log: Vec::new(),
        }
    }

    pub fn set_relay(&mut self, relay: Box<dyn ToolRelay>) {
        self.relay = relay;
    }

    /// A process connected its hub socket. `app` is the app the shell
    /// launched on that socket (its client slot), never a claim of the
    /// process. A link opens only for an app granted assistant services.
    pub fn connected(&mut self, client_id: ClientId, app: &str, out: FrameOut) -> bool {
        if self.host.granted(app).is_empty() {
            return false;
        }
        self.links.insert(client_id, Link { app: app.to_string(), out, contexts: Vec::new() });
        self.host.link_opened(app);
        self.log.push(format!("peer link: {app} (client {client_id}) opened"));
        true
    }

    pub fn has_link(&self, app: &str) -> bool {
        self.links.values().any(|l| l.app == app)
    }

    /// A peer frame from `client_id`. `app` is the app of that socket. The
    /// frame is consumed (true) whenever it carries the peer envelope, so it
    /// never reaches the AI bus.
    pub fn on_frame(&mut self, client_id: ClientId, app: &str, frame: &str, reply_out: Option<FrameOut>) -> bool {
        if !wire::is_peer_frame(frame) {
            return false;
        }
        let Some(up) = Up::parse(frame) else {
            self.log.push(format!("peer link: {app} (client {client_id}): malformed frame dropped"));
            return true;
        };
        match up {
            Up::Request { req_id, method, args } => self.on_request(client_id, app, req_id, &method, args, reply_out),
            Up::ToolResult { call_id, outcome } => self.on_tool_result(client_id, app, &call_id, outcome),
        }
        true
    }

    fn reply(out: &FrameOut, req_id: u64, result: Result<Value, String>) {
        out(Down::Reply { req_id, result }.to_json());
    }

    fn on_request(&mut self, client_id: ClientId, app: &str, req_id: u64, method: &str, args: Map<String, Value>, reply_out: Option<FrameOut>) {
        let Some(link) = self.links.get(&client_id) else {
            // No link: the app has no granted agent. Answer so it can say so.
            if let Some(out) = reply_out {
                Self::reply(&out, req_id, Err("no_agent: this app is not granted an agent".into()));
            }
            return;
        };
        // The socket's app, never the frame's.
        debug_assert_eq!(link.app, app);
        let out = link.out.clone();
        let app = link.app.clone();
        let granted = self.host.granted(&app);
        if method != "octos.context.close" && !granted.contains(method) {
            return Self::reply(&out, req_id, Err(format!("not_granted: {method}")));
        }
        if !self.host.consent(&app) {
            return Self::reply(&out, req_id, Err("consent_pending: the person has not allowed this app's agent".into()));
        }
        let services: BTreeSet<String> = granted.iter().filter(|s| OCTOS_SERVICES.contains(&s.as_str())).cloned().collect();
        let service = match self.services.get(&app) {
            Some(s) => s.clone(),
            None => match self.host.service(&app, &services) {
                Some(s) => {
                    self.services.insert(app.clone(), s.clone());
                    s
                }
                None => return Self::reply(&out, req_id, Err("unavailable: no assistant on this shell".into())),
            },
        };
        match method {
            "octos.session.open" => self.open_session(client_id, &app, req_id, &args, service, services, out),
            _ => {
                let Some(handle) = args.get("context").and_then(Value::as_str).map(str::to_string) else {
                    return Self::reply(&out, req_id, Err("bad_args: context is required".into()));
                };
                // A process may use only the contexts it opened.
                let owned = self.contexts.get(&handle).is_some_and(|c| c.owner.client_id == client_id && c.owner.app == app);
                if !owned {
                    return Self::reply(&out, req_id, Err("unknown_context".into()));
                }
                if method == "octos.context.close" {
                    self.close_context(&handle);
                    return Self::reply(&out, req_id, Ok(json!({"closed": handle})));
                }
                let op = match method {
                    "octos.session.history" => ContextOp::History,
                    "octos.turn.interrupt" => ContextOp::Interrupt,
                    "octos.turn.start" => match args.get("text").and_then(Value::as_str) {
                        Some(text) => ContextOp::Turn { text: text.to_string() },
                        None => return Self::reply(&out, req_id, Err("bad_args: text is required".into())),
                    },
                    _ => return Self::reply(&out, req_id, Err(format!("unsupported: {method}"))),
                };
                let ctx = self.contexts[&handle].ctx.clone();
                let sink_out = out.clone();
                let sink = Arc::new(move |event: ContextEvent| match event {
                    ContextEvent::Data(event) => sink_out(Down::Event { req_id, event }.to_json()),
                    ContextEvent::Complete(result) => sink_out(Down::Reply { req_id, result }.to_json()),
                });
                if let Err(e) = ctx.call(op, sink) {
                    Self::reply(&out, req_id, Err(e));
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn open_session(&mut self, client_id: ClientId, app: &str, req_id: u64, args: &Map<String, Value>, service: Arc<dyn OctosAppService>, services: BTreeSet<String>, out: FrameOut) {
        let client = match args.get("client") {
            None | Some(Value::Null) => None,
            Some(Value::String(c)) if wire::id_ok(c) => Some(c.clone()),
            Some(_) => return Self::reply(&out, req_id, Err("bad_args: client".into())),
        };
        let account = if self.host.keeps_accounts(app) {
            match args.get("account").and_then(Value::as_str) {
                Some(a) if wire::id_ok(a) => a.to_string(),
                _ => return Self::reply(&out, req_id, Err("bad_args: account is required".into())),
            }
        } else {
            DEVICE.to_string()
        };
        let handle = format!("pl{client_id}-{}", self.next_context);
        self.next_context += 1;
        service.set_account(Some(&account));
        let spec = ContextSpec { account: account.clone(), instance: handle.clone(), services };
        let ctx = match service.open_context(spec) {
            Ok(ctx) => ctx,
            Err(e) => return Self::reply(&out, req_id, Err(e)),
        };
        let owner = ContextOwner { app: app.to_string(), client_id, account, client };
        self.contexts.insert(handle.clone(), Ctx { owner, ctx: ctx.clone() });
        if let Some(link) = self.links.get_mut(&client_id) {
            link.contexts.push(handle.clone());
        }
        let sink_out = out.clone();
        let reply_handle = handle.clone();
        let sink = Arc::new(move |event: ContextEvent| match event {
            ContextEvent::Data(event) => sink_out(Down::Event { req_id, event }.to_json()),
            ContextEvent::Complete(result) => {
                let result = result.map(|session| json!({"context": reply_handle, "session": session}));
                sink_out(Down::Reply { req_id, result }.to_json())
            }
        });
        if let Err(e) = ctx.call(ContextOp::Open, sink) {
            self.close_context(&handle);
            Self::reply(&out, req_id, Err(e));
        }
    }

    fn close_context(&mut self, handle: &str) {
        if let Some(c) = self.contexts.remove(handle) {
            c.ctx.close();
            if let Some(link) = self.links.get_mut(&c.owner.client_id) {
                link.contexts.retain(|h| h != handle);
            }
        }
    }

    /// Who owns the kernel's `context_id` of `app` (octos derives it from
    /// the handle the shell chose: `<nonce>-<handle>`).
    pub fn context_owner(&self, app: &str, kernel_context_id: &str) -> Option<ContextOwner> {
        self.contexts
            .iter()
            .find(|(handle, c)| {
                c.owner.app == app && (kernel_context_id == handle.as_str() || kernel_context_id.ends_with(&format!("-{handle}")))
            })
            .map(|(_, c)| c.owner.clone())
    }

    /// The relay hands over a kernel `peer/tool/call` for `app`.
    pub fn tool_call(&mut self, app: &str, call: KernelToolCall, now: f64) -> Result<(), Refused> {
        let key = (app.to_string(), call.call_id.clone());
        if self.calls.contains_key(&key) {
            return Err(Refused::Duplicate);
        }
        // The newest process of the app with a link takes the call.
        let Some((&client_id, link)) = self.links.iter().filter(|(_, l)| l.app == app).max_by_key(|(id, _)| **id) else {
            return Err(Refused::NotConnected);
        };
        let out = link.out.clone();
        // Identity stamped from the shell's records, never the app's.
        let (account, context_id, client) = match &call.context_id {
            Some(kernel) => match self.context_owner(app, kernel) {
                Some(owner) => (Some(owner.account), Some(kernel.clone()), owner.client),
                None => return Err(Refused::UnknownContext),
            },
            None => (Some(self.contexts.values().find(|c| c.owner.app == app).map(|c| c.owner.account.clone()).unwrap_or_else(|| DEVICE.into())), None, None),
        };
        let caller = match &call.caller {
            Caller::OwnAgent { .. } => Caller::OwnAgent { client: client.clone() },
            other => other.clone(),
        };
        // The kernel's own say comes first (UPCR-2026-035): `confirm_required`
        // is the app's sheet, `approved` means the person already answered.
        let rule = if call.confirm_required {
            Some((Confirm::App, self.host.tool_rule(app, &call.name).map(|(_, auto)| auto).unwrap_or(true)))
        } else if call.approved {
            None
        } else {
            self.host.tool_rule(app, &call.name)
        };
        let needs_approval = call.confirm_required || (!call.approved && (rule.is_some() || call.risk == Risk::Destructive || call.outcome_unknown));
        let mut down = ToolCallDown {
            call_id: call.call_id.clone(),
            name: call.name.clone(),
            args: call.args.clone(),
            risk: call.risk,
            confirm_required: false,
            timeout_ms: call.timeout_ms,
            account: account.clone(),
            context_id: context_id.clone(),
            client,
            caller: caller_wire(&caller),
        };
        let deadline = now + call.timeout_ms as f64 / 1000.0;
        let mut record = Call { client_id, risk: call.risk, state: CallState::Sent, confirm_required: false, acked: false, approval: None, deadline, down: down.clone() };
        if needs_approval {
            // The obligation: acknowledge before any confirmation sheet.
            self.relay.acknowledged(app, &call.call_id);
            let (confirm, auto) = rule.unwrap_or((Confirm::Host, true));
            let mut spec = match confirm {
                Confirm::Host => ToolSpec::host(&call.name),
                Confirm::App => ToolSpec::app(&call.name),
            };
            spec.auto_approvable = auto;
            let id = RequestId(format!("{HELD_PREFIX}{app}:{}", call.call_id));
            let context = RequestContext {
                call_id: id.0.clone(),
                trigger: call.trigger.clone(),
                context_id: context_id.clone(),
                account: account.clone(),
                outcome_unknown: call.outcome_unknown,
                ..RequestContext::default()
            };
            record.approval = Some(id);
            match self.host.request_approval(app, spec, call.args.clone(), caller, context) {
                Route::Approved(_) => {}
                Route::HandedToApp => {
                    down.confirm_required = true;
                    record.confirm_required = true;
                    record.down = down.clone();
                }
                Route::Sheet(_) | Route::WaitingForApp { .. } => record.state = CallState::Held,
                Route::Refused(why) => {
                    self.relay.finished(app, &call.call_id, ToolCallResult::Error(format!("declined: {why}")));
                    return Err(Refused::Declined(why));
                }
            }
        }
        if record.state == CallState::Sent {
            out(Down::ToolCall(down).to_json());
        }
        self.calls.insert(key, record);
        Ok(())
    }

    /// The relay (the kernel) cancels a call: nothing of it reaches the
    /// kernel afterwards, and the app is told to stop.
    pub fn tool_cancel(&mut self, app: &str, call_id: &str) {
        if let Some(call) = self.calls.remove(&(app.to_string(), call_id.to_string())) {
            if call.state == CallState::Sent {
                if let Some(link) = self.links.get(&call.client_id) {
                    (link.out)(Down::ToolCancel { call_id: call_id.to_string() }.to_json());
                }
            }
            self.log.push(format!("peer link: {app}: call {call_id} cancelled"));
        }
    }

    fn on_tool_result(&mut self, client_id: ClientId, app: &str, call_id: &str, outcome: Outcome) {
        let key = (app.to_string(), call_id.to_string());
        let Some(call) = self.calls.get_mut(&key) else {
            self.log.push(format!("peer link: {app}: result for {call_id} ignored (answered, cancelled or never made)"));
            return;
        };
        if call.client_id != client_id || call.state != CallState::Sent {
            self.log.push(format!("peer link: {app}: result for {call_id} ignored (not this process's, or not sent)"));
            return;
        }
        match outcome {
            Outcome::AwaitingConfirmation => {
                if call.confirm_required && !call.acked {
                    call.acked = true;
                }
            }
            Outcome::Ok(_) | Outcome::Error(_) if call.confirm_required && !call.acked => {
                // The app ran a confirm-required call without its sheet.
                let call = self.calls.remove(&key).expect("present");
                if let Some(id) = &call.approval {
                    self.host.app_confirm_answered(id, false, "the app skipped its confirmation");
                }
                self.relay.finished(app, call_id, ToolCallResult::Error("refused: the app answered without acknowledging its confirmation".into()));
                if let Some(link) = self.links.get(&client_id) {
                    (link.out)(Down::ToolCancel { call_id: call_id.to_string() }.to_json());
                }
                self.log.push(format!("peer link: {app}: call {call_id} refused (no acknowledgement before its sheet)"));
            }
            Outcome::Ok(data) => {
                let call = self.calls.remove(&key).expect("present");
                if call.confirm_required {
                    if let Some(id) = &call.approval {
                        self.host.app_confirm_answered(id, true, "approved on the app's sheet");
                    }
                }
                self.relay.finished(app, call_id, ToolCallResult::Ok(data));
            }
            Outcome::Error(error) => {
                let call = self.calls.remove(&key).expect("present");
                if call.confirm_required {
                    if let Some(id) = &call.approval {
                        self.host.app_confirm_answered(id, false, &error);
                    }
                }
                self.relay.finished(app, call_id, ToolCallResult::Error(error));
            }
        }
    }

    /// The router's decisions and the deadlines; call on the shell's tick.
    pub fn tick(&mut self, now: f64) {
        for (id, decision, reason) in self.host.take_decisions() {
            let Some(key) = self.calls.iter().find(|(_, c)| c.approval.as_ref() == Some(&id) && c.state == CallState::Held).map(|(k, _)| k.clone()) else {
                continue;
            };
            if decision.approved() {
                let call = self.calls.get_mut(&key).expect("present");
                call.state = CallState::Sent;
                if let Some(link) = self.links.get(&call.client_id) {
                    (link.out)(Down::ToolCall(call.down.clone()).to_json());
                }
            } else {
                self.calls.remove(&key);
                self.relay.finished(&key.0, &key.1, ToolCallResult::Error(format!("declined: {reason}")));
            }
        }
        let late: Vec<(String, String)> = self.calls.iter().filter(|(_, c)| now >= c.deadline).map(|(k, _)| k.clone()).collect();
        for key in late {
            let call = self.calls.remove(&key).expect("present");
            if call.state == CallState::Sent {
                if let Some(link) = self.links.get(&call.client_id) {
                    (link.out)(Down::ToolCancel { call_id: key.1.clone() }.to_json());
                }
            }
            self.relay.finished(&key.0, &key.1, ToolCallResult::Error("timed_out".into()));
        }
    }

    /// The process on `client_id` is gone (exited, killed, or its socket
    /// closed): its outstanding calls fail (`outcome_unknown` unless they
    /// only read), its request contexts close, and its app's peer stays.
    pub fn process_gone(&mut self, client_id: ClientId) {
        let Some(link) = self.links.remove(&client_id) else { return };
        let keys: Vec<(String, String)> = self.calls.iter().filter(|(_, c)| c.client_id == client_id).map(|(k, _)| k.clone()).collect();
        for key in keys {
            let call = self.calls.remove(&key).expect("present");
            let result = if call.risk == Risk::Read || call.state == CallState::Held {
                ToolCallResult::Error("app_exited".into())
            } else {
                ToolCallResult::OutcomeUnknown
            };
            self.relay.finished(&key.0, &key.1, result);
        }
        for handle in link.contexts.clone() {
            self.close_context(&handle);
        }
        if !self.has_link(&link.app) {
            self.host.link_closed(&link.app);
        }
        self.log.push(format!("peer link: {} (client {client_id}) closed; its peer is kept", link.app));
    }

    /// Close every context of `app`'s `account` (signing out, §11): the
    /// app is told, and later calls stamped for it are refused.
    pub fn close_account(&mut self, app: &str, account: &str, reason: &str) {
        let handles: Vec<String> = self.contexts.iter().filter(|(_, c)| c.owner.app == app && c.owner.account == account).map(|(h, _)| h.clone()).collect();
        for handle in handles {
            let client_id = self.contexts[&handle].owner.client_id;
            self.close_context(&handle);
            if let Some(link) = self.links.get(&client_id) {
                (link.out)(Down::ContextClosed { context: handle.clone(), reason: reason.to_string() }.to_json());
            }
        }
    }

    /// Whether the shell keeps a peer (service) for `app`.
    pub fn keeps_peer(&self, app: &str) -> bool {
        self.services.contains_key(app)
    }

    pub fn open_calls(&self) -> usize {
        self.calls.len()
    }

    pub fn open_contexts(&self, app: &str) -> usize {
        self.contexts.values().filter(|c| c.owner.app == app).count()
    }
}
