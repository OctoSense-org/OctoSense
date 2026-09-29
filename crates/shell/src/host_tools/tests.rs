//! The relay against a recording shell: authorization by (owning app, tool)
//! and caller, routing to the owning app's executor, the `confirm: app`
//! hand-off through the real approval router, `host_tool` approvals,
//! cancels, the peer link and the AI bus.

use super::relay::{Catalog, Env, Event, Relay, BUS_PREFIX, CONFIRM_PREFIX, TERMINAL_RUN};
use crate::ai_host::app_peers::host_tools::{ApprovalAnswer, CallOrigin, CallerKind, ConfirmRequest, ConfirmSheet, HostToolApproval, HostToolCall, ToolExecutor, ToolOutcome, ToolReply};
use crate::approvals::audit::AuditLog;
use crate::approvals::contacts::{ContactsGate, NoContacts};
use crate::approvals::dev_hooks::FixedDevMode;
use crate::approvals::rules::RuleStore;
use crate::approvals::{Caller, Decision, RecordingRelay, RequestContext, RequestId, Route, Router, ToolSpec, Trigger};
use crate::peer_link::{KernelToolCall, Refused, ToolCallResult};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

/// The shell as the relay sees it, recorded; approvals go to a real router.
struct World {
    router: Router,
    decisions: RecordingRelay,
    consent: bool,
    dev_all: bool,
    suspended: bool,
    system: BTreeSet<String>,
    links: Vec<String>,
    link_calls: Vec<(String, KernelToolCall)>,
    link_cancels: Vec<String>,
    bus: Vec<(String, String, String, String)>,
    bus_cancels: Vec<String>,
    asked: Vec<(String, ToolSpec, Caller, RequestContext)>,
}

impl World {
    fn new(dev: FixedDevMode) -> World {
        let decisions = RecordingRelay::default();
        let router = Router::new(RuleStore::memory(), AuditLog::memory(), Box::new(dev), ContactsGate::memory(Box::new(NoContacts)), Box::new(decisions.clone()));
        World {
            router,
            decisions,
            consent: true,
            dev_all: false,
            suspended: false,
            system: BTreeSet::new(),
            links: Vec::new(),
            link_calls: Vec::new(),
            link_cancels: Vec::new(),
            bus: Vec::new(),
            bus_cancels: Vec::new(),
            asked: Vec::new(),
        }
    }
    /// The router's decisions, as the shell hands them back to the relay.
    fn decided(&self) -> Vec<Event> {
        self.decisions.take().into_iter().map(|(id, decision, reason)| Event::Decision { id, decision, reason }).collect()
    }
}

impl Env for World {
    fn consent(&self, _app: &str) -> bool {
        self.consent
    }
    fn grants_all(&self, _app: &str) -> bool {
        self.dev_all
    }
    fn suspended(&self, _app: &str, _account: Option<&str>) -> bool {
        self.suspended
    }
    fn system_tools(&self) -> BTreeSet<String> {
        self.system.clone()
    }
    fn tool_rule(&self, _owner: &str, tool: &str) -> (bool, bool) {
        let command = tool == TERMINAL_RUN;
        (!command, command)
    }
    fn request_approval(&mut self, app: &str, tool: ToolSpec, args: Value, caller: Caller, context: RequestContext) -> Route {
        self.asked.push((app.to_string(), tool.clone(), caller.clone(), context.clone()));
        let request = crate::approvals::router::make_request(app, tool, args, caller, context, 1, 0);
        self.router.request(request, 1)
    }
    fn has_link(&self, app: &str) -> bool {
        self.links.iter().any(|l| l == app)
    }
    fn link_call(&mut self, app: &str, call: KernelToolCall) -> Result<(), Refused> {
        self.link_calls.push((app.to_string(), call));
        Ok(())
    }
    fn link_cancel(&mut self, _app: &str, call_id: &str) {
        self.link_cancels.push(call_id.to_string());
    }
    fn bus_call(&mut self, call_id: &str, app: &str, tool: &str, args: String) {
        self.bus.push((call_id.into(), app.into(), tool.into(), args));
    }
    fn bus_cancel(&mut self, call_id: &str) {
        self.bus_cancels.push(call_id.into());
    }
    fn log(&mut self, _line: String) {}
}

type Sent = Arc<Mutex<Vec<Value>>>;

fn reply(id: &str) -> (ToolReply, Sent) {
    let sent: Sent = Arc::default();
    let s = sent.clone();
    (ToolReply::new(id, move |v| s.lock().unwrap().push(v)), sent)
}

fn call(id: &str, name: &str, calling: &str) -> HostToolCall {
    let mut c = HostToolCall::parse(&json!({"peer": "p1", "session_id": "s#peer-p1", "turn_id": "t1", "call_id": id, "tool_call_id": format!("tc-{id}"), "args_digest": "d",
        "name": name, "caller": {"kind": "app_peer"}, "args": {"to": ["ana@example.org"], "text": "hi"}, "risk": "act", "confirm_required": false})).unwrap();
    c.calling_app = calling.to_string();
    c.account = Some("@alice:x".into());
    c.origin = CallOrigin::Context;
    c.client = Some("mini.news".into());
    c
}

fn decl(name: &str, shareable: bool, confirm: &str) -> Value {
    json!({"name": name, "description": "d", "input_schema": {"type": "object"}, "risk": "act", "outward": true, "confirm": confirm, "shareable": shareable})
}

/// An in-process app's executor, recorded.
#[derive(Default)]
struct Exec(Mutex<Vec<(HostToolCall, ToolReply)>>, Mutex<Vec<String>>);
impl ToolExecutor for Exec {
    fn execute(&self, call: HostToolCall, reply: ToolReply) {
        self.0.lock().unwrap().push((call, reply));
    }
    fn cancel(&self, call_id: &str) {
        self.1.lock().unwrap().push(call_id.to_string());
    }
}

fn relay_with(app: &str, tools: Vec<Value>) -> (Relay, Arc<Exec>) {
    let mut relay = Relay::default();
    relay.catalog.declare(app, tools);
    let exec = Arc::new(Exec::default());
    relay.set_executor(app, Some(exec.clone()));
    (relay, exec)
}

#[test]
fn an_apps_own_agent_calls_its_declared_tools_with_the_stamped_identity() {
    let (mut relay, exec) = relay_with("rinx", vec![decl("rinx.room.list", false, "host")]);
    let mut w = World::new(FixedDevMode::off());
    let (r, sent) = reply("c1");
    relay.handle(Event::Call { call: call("c1", "rinx.room.list", "rinx"), reply: r }, &mut w);
    let (got, reply) = exec.0.lock().unwrap()[0].clone();
    assert_eq!(got.client.as_deref(), Some("mini.news"));
    assert_eq!(got.account.as_deref(), Some("@alice:x"));
    assert!(reply.finish(ToolOutcome::Ok(json!({"rooms": []}))));
    assert_eq!(sent.lock().unwrap().len(), 1, "answered once");
    // A tool it never declared is not granted.
    let (r, sent) = reply_pair("c2");
    relay.handle(Event::Call { call: call("c2", "rinx.admin.wipe", "rinx"), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted");
    assert_eq!(exec.0.lock().unwrap().len(), 1);
}

fn reply_pair(id: &str) -> (ToolReply, Sent) {
    reply(id)
}

#[test]
fn another_apps_agent_needs_a_grant_and_the_system_agent_its_own() {
    let mut relay = Relay::default();
    relay.catalog.declare("mail", vec![decl("mail.send", true, "host"), decl("mail.purge", false, "host")]);
    let exec = Arc::new(Exec::default());
    relay.set_executor("mail", Some(exec.clone()));
    let mut w = World::new(FixedDevMode::off());
    let (r, sent) = reply("c1");
    relay.handle(Event::Call { call: call("c1", "mail.send", "calendar"), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted", "no grant yet");
    relay.catalog.grant("calendar", "mail.send");
    relay.catalog.grant("calendar", "mail.purge");
    let (r, _) = reply("c2");
    relay.handle(Event::Call { call: call("c2", "mail.send", "calendar"), reply: r }, &mut w);
    let (r, sent) = reply("c3");
    relay.handle(Event::Call { call: call("c3", "mail.purge", "calendar"), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted", "only shareable tools are granted across apps");
    let calls = exec.0.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    // Declarations: Calendar's peer registers Mail's granted tool, naming its owner.
    let decls = relay.catalog.declarations("calendar", false);
    assert_eq!(decls.len(), 1);
    assert_eq!((decls[0]["name"].as_str(), decls[0]["app"].as_str()), (Some("mail.send"), Some("mail")));

    // The system agent calls only what Setup granted it.
    let mut system = call("c4", TERMINAL_RUN, "system");
    system.caller_kind = CallerKind::System;
    system.origin = CallOrigin::System;
    let (r, sent) = reply("c4");
    relay.handle(Event::Call { call: system.clone(), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted");
    w.system.insert(TERMINAL_RUN.into());
    let (r, sent) = reply("c5");
    system.call_id = "c5".into();
    relay.handle(Event::Call { call: system, reply: r }, &mut w);
    // Typed into the Terminal the person sees, on the AI bus, by its short name.
    assert_eq!(w.bus, vec![(format!("{BUS_PREFIX}c5"), "terminal".into(), "run".into(), json!({"to": ["ana@example.org"], "text": "hi"}).to_string())]);
    relay.handle(Event::BusResult { call_id: "c5".into(), outcome: ToolOutcome::Ok(json!({"text": "typed"})) }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["ok"], true);
}

#[test]
fn consent_and_a_signed_out_account_refuse_calls() {
    let (mut relay, exec) = relay_with("rinx", vec![decl("rinx.room.list", false, "host")]);
    let mut w = World::new(FixedDevMode::off());
    w.consent = false;
    let (r, sent) = reply("c1");
    relay.handle(Event::Call { call: call("c1", "rinx.room.list", "rinx"), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "consent_pending");
    w.consent = true;
    w.suspended = true;
    let (r, sent) = reply("c2");
    relay.handle(Event::Call { call: call("c2", "rinx.room.list", "rinx"), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "signed_out");
    assert!(exec.0.lock().unwrap().is_empty());
}

/// Rinx's send sheet, recorded (the owning app's own `confirm: app` sheet).
#[derive(Default)]
struct SendSheet(Mutex<Vec<ConfirmRequest>>);
impl ConfirmSheet for SendSheet {
    fn confirm(&self, request: ConfirmRequest) {
        self.0.lock().unwrap().push(request);
    }
}

#[test]
fn a_confirm_app_call_is_acknowledged_then_handed_to_the_owning_apps_sheet_and_runs_once_approved() {
    let (mut relay, exec) = relay_with("rinx", vec![decl("rinx.message.send", true, "app")]);
    relay.catalog.grant("calendar", "rinx.message.send");
    let mut w = World::new(FixedDevMode::off());
    // Rinx's send sheet, registered as the router's confirm: app handler.
    let sheet = Arc::new(SendSheet::default());
    w.router.register_app_confirm("rinx", Box::new(super::SheetBridge { app: "rinx".into(), sheet: sheet.clone() }));
    let mut c = call("c1", "rinx.message.send", "calendar");
    c.confirm_required = true;
    let (r, sent) = reply("c1");
    relay.handle(Event::Call { call: c, reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap().as_slice(), &[json!({"call_id": "c1", "status": "awaiting_confirmation"})], "acknowledged before the sheet");
    assert!(exec.0.lock().unwrap().is_empty(), "nothing runs before the person confirms");
    let shown = sheet.0.lock().unwrap().clone();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].caller_label, "Calendar's agent", "the sheet shows who is calling");
    assert_eq!(shown[0].args["text"], "hi", "and the exact arguments");
    assert_eq!(w.asked[0].1, ToolSpec::app("rinx.message.send"));
    // The person approves on Rinx's sheet.
    w.router.app_confirm_answered(&RequestId(format!("{CONFIRM_PREFIX}c1")), true, "sent", 2).unwrap();
    for event in w.decided() {
        relay.handle(event, &mut w);
    }
    assert_eq!(exec.0.lock().unwrap().len(), 1, "runs once, after the approval");
    // A denial is an error result, and nothing runs.
    let mut c = call("c2", "rinx.message.send", "calendar");
    c.confirm_required = true;
    let (r, sent) = reply("c2");
    relay.handle(Event::Call { call: c, reply: r }, &mut w);
    w.router.app_confirm_answered(&RequestId(format!("{CONFIRM_PREFIX}c2")), false, "not now", 2).unwrap();
    for event in w.decided() {
        relay.handle(event, &mut w);
    }
    assert_eq!(sent.lock().unwrap()[1]["error"]["kind"], "declined");
    assert_eq!(exec.0.lock().unwrap().len(), 1);
}

#[test]
fn developer_mode_overrides_the_apps_sheet() {
    let (mut relay, exec) = relay_with("rinx", vec![decl("rinx.message.send", false, "app")]);
    let mut w = World::new(FixedDevMode::all());
    let mut c = call("c1", "rinx.message.send", "rinx");
    c.confirm_required = true;
    let (r, _) = reply("c1");
    relay.handle(Event::Call { call: c, reply: r }, &mut w);
    for event in w.decided() {
        relay.handle(event, &mut w);
    }
    assert_eq!(exec.0.lock().unwrap().len(), 1, "no sheet in developer mode (ADR 0004 §13)");
}

#[test]
fn a_cancel_ends_the_call_wherever_it_is_and_nothing_answers_after() {
    let (mut relay, exec) = relay_with("rinx", vec![decl("rinx.room.list", false, "host")]);
    let mut w = World::new(FixedDevMode::off());
    let (r, sent) = reply("c1");
    relay.handle(Event::Call { call: call("c1", "rinx.room.list", "rinx"), reply: r }, &mut w);
    relay.handle(Event::Cancel { call_id: "c1".into(), reason: "cancelled".into() }, &mut w);
    assert_eq!(exec.1.lock().unwrap().as_slice(), &["c1".to_string()], "the executor is told");
    let (_, late) = exec.0.lock().unwrap()[0].clone();
    assert!(!late.finish(ToolOutcome::Ok(json!({}))));
    assert!(sent.lock().unwrap().is_empty());
    // A process app's call goes down its link, and a cancel follows it.
    relay.catalog.declare("notes", vec![decl("notes.add", false, "app")]);
    w.links.push("notes".into());
    let mut c = call("c2", "notes.add", "notes");
    c.confirm_required = true;
    let (r, sent) = reply("c2");
    relay.handle(Event::Call { call: c, reply: r }, &mut w);
    let (app, forwarded) = w.link_calls[0].clone();
    assert_eq!(app, "notes");
    assert!(forwarded.confirm_required && !forwarded.approved, "the link hands it to the app's own sheet");
    assert_eq!(forwarded.caller, Caller::OwnAgent { client: Some("mini.news".into()) });
    assert_eq!(forwarded.trigger, Trigger::Person);
    relay.handle(Event::LinkOutcome { app: "notes".into(), call_id: "c2".into(), result: None }, &mut w);
    relay.handle(Event::LinkOutcome { app: "notes".into(), call_id: "c2".into(), result: Some(ToolCallResult::Error("declined: not now".into())) }, &mut w);
    let sent = sent.lock().unwrap().clone();
    assert_eq!(sent[0]["status"], "awaiting_confirmation");
    assert_eq!(sent[1]["error"]["kind"], "declined");
    let (r, _) = reply("c3");
    relay.handle(Event::Call { call: call("c3", "notes.add", "notes"), reply: r }, &mut w);
    assert!(w.link_calls[1].1.approved, "a call the kernel already approved is not asked again");
    relay.handle(Event::Cancel { call_id: "c3".into(), reason: "timeout".into() }, &mut w);
    assert_eq!(w.link_cancels, vec!["c3".to_string()]);
}

#[test]
fn a_host_tool_approval_goes_to_the_router_and_its_decision_answers_the_kernel() {
    let mut relay = Relay::default();
    let mut w = World::new(FixedDevMode::off());
    let answers: Arc<Mutex<Vec<bool>>> = Arc::default();
    let a = answers.clone();
    let answer = ApprovalAnswer::new(move |ok| a.lock().unwrap().push(ok));
    let approval = HostToolApproval::parse(
        &json!({"approval_id": "a1", "turn_id": "t", "approval_kind": "host_tool", "typed_details": {"host_tool": {"app": "mail", "tool": "mail.send", "args": {"to": ["bo@example.org"]}, "risk": "act", "outward": true, "calling_kind": "app_peer", "calling_peer": "calendar-1", "context_id": "ctx", "outcome_unknown_before": true}}}),
        "s#peerctx-calendar-1.ctx",
    )
    .unwrap();
    relay.handle(Event::Approval { app: "calendar".into(), account: Some("@a:x".into()), approval, answer }, &mut w);
    let (app, spec, caller, context) = w.asked[0].clone();
    assert_eq!((app.as_str(), spec.name.as_str()), ("mail", "mail.send"), "keyed to the owning app and tool");
    assert_eq!(caller, Caller::AppAgent { app: "calendar".into() }, "the sheet shows the calling app");
    assert!(context.outcome_unknown, "an unknown earlier outcome always asks the person");
    assert!(answers.lock().unwrap().is_empty(), "the person has not answered");
    assert!(w.router.is_pending(&RequestId("hostappr:a1".into())), "on the shell's sheet");
    // Nobody answers: the sheet expires, and the kernel hears a denial.
    w.router.sheet_expiry_s = 0;
    w.router.tick(5);
    for event in w.decided() {
        relay.handle(event, &mut w);
    }
    assert_eq!(answers.lock().unwrap().as_slice(), &[false]);
    // terminal.run is a command: never a rule, developer mode may answer it.
    let mut w = World::new(FixedDevMode::all());
    let answers: Arc<Mutex<Vec<bool>>> = Arc::default();
    let a = answers.clone();
    let approval = HostToolApproval::parse(
        &json!({"approval_id": "a2", "approval_kind": "host_tool", "typed_details": {"host_tool": {"app": "terminal", "tool": "terminal.run", "args": {"command": "ls"}, "risk": "destructive", "calling_kind": "system"}}}),
        "s#system",
    )
    .unwrap();
    relay.handle(Event::Approval { app: "system".into(), account: None, approval, answer: ApprovalAnswer::new(move |ok| a.lock().unwrap().push(ok)) }, &mut w);
    assert!(w.asked[0].1.command && !w.asked[0].1.auto_approvable);
    assert_eq!(w.asked[0].2, Caller::SystemAgent);
    for event in w.decided() {
        relay.handle(event, &mut w);
    }
    assert_eq!(answers.lock().unwrap().as_slice(), &[true]);
}

#[test]
fn an_app_that_is_not_running_is_refused_visibly() {
    let mut relay = Relay::default();
    relay.catalog.declare("notes", vec![decl("notes.add", false, "host")]);
    let mut w = World::new(FixedDevMode::off());
    let (r, sent) = reply("c1");
    relay.handle(Event::Call { call: call("c1", "notes.add", "notes"), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "app_not_running");
    let _ = Decision::Deny;
}

#[test]
fn the_shipped_catalog_offers_the_terminals_run_to_those_granted_it() {
    let catalog = Catalog::shipped();
    let run = catalog.entry("terminal", TERMINAL_RUN).unwrap();
    assert_eq!((run["risk"].as_str(), run["confirm"].as_str()), (Some("destructive"), Some("host")));
    assert!(catalog.declarations("rinx", false).is_empty(), "nobody gets it without a grant");
    assert_eq!(catalog.declarations("rinx", true).len(), 1, "developer mode grants every shareable tool");
}

/// The system toolbox's tools as its catalog declares them (the real ones
/// with the `toolbox-peers` feature): owned by `toolbox`, shareable, read
/// except `workflow.fork`.
fn toolbox_catalog() -> Vec<Value> {
    #[cfg(feature = "toolbox-peers")]
    return crate::ai_host::toolbox_peers::catalog();
    #[cfg(not(feature = "toolbox-peers"))]
    ["workflow.run", "workflow.fork", "toolbox.search", "toolbox.web_read", "toolbox.deep_crawl"]
        .iter()
        .map(|name| {
            let risk = if *name == "workflow.fork" { "act" } else { "read" };
            json!({"name": name, "app": super::TOOLBOX, "description": "d", "input_schema": {"type": "object"}, "risk": risk, "background": true, "outward": false, "confirm": "host", "shareable": true})
        })
        .collect()
}

fn offered_names(relay: &Relay, app: &str, dev: bool, consented: bool) -> Vec<String> {
    let mut names: Vec<String> = relay.catalog.offered(app, dev, consented).iter().map(|d| d["name"].as_str().unwrap().to_string()).collect();
    names.sort();
    names
}

#[test]
fn a_peer_is_offered_exactly_its_granted_toolbox_tools_marked_with_their_owner_and_only_after_consent() {
    let (mut relay, _) = relay_with(super::TOOLBOX, toolbox_catalog());
    relay.catalog.declare("os.news", vec![decl("news.item.save", false, "host")]);
    // `research` granted: its four tools; `crawl` not granted: no deep_crawl.
    relay.catalog.set_grants("os.news", super::TOOLBOX, &["workflow.run", "workflow.fork", "toolbox.search", "toolbox.web_read"]);
    // Before the person allowed News's agent: its own tools, no toolbox tool.
    assert_eq!(offered_names(&relay, "os.news", false, false), ["news.item.save"]);
    // After: exactly the granted ones, each marked with its owning app.
    assert_eq!(offered_names(&relay, "os.news", false, true), ["news.item.save", "toolbox.search", "toolbox.web_read", "workflow.fork", "workflow.run"]);
    for d in relay.catalog.offered("os.news", false, true).iter().filter(|d| d["name"] != "news.item.save") {
        assert_eq!(d["app"], super::TOOLBOX, "{d}");
        assert_eq!(d["risk"], if d["name"] == "workflow.fork" { "act" } else { "read" }, "{d}");
    }
    // A grant computed again (crawl now granted) replaces the old one.
    relay.catalog.set_grants("os.news", super::TOOLBOX, &["toolbox.deep_crawl"]);
    assert_eq!(offered_names(&relay, "os.news", false, true), ["news.item.save", "toolbox.deep_crawl"]);
    // No grant, no toolbox tools; developer mode does not invent a grant
    // (the toolbox needs its scope), though it grants other shareable tools.
    assert!(offered_names(&relay, "calendar", false, true).is_empty());
    assert_eq!(offered_names(&relay, "calendar", true, true), [TERMINAL_RUN]);
}

#[test]
fn no_toolbox_call_runs_before_consent_or_without_a_grant() {
    let (mut relay, exec) = relay_with(super::TOOLBOX, toolbox_catalog());
    relay.catalog.set_grants("os.news", super::TOOLBOX, &["toolbox.search"]);
    let mut w = World::new(FixedDevMode::off());
    let toolbox_call = |id: &str, name: &str, calling: &str| {
        let mut c = call(id, name, calling);
        c.app = super::TOOLBOX.into();
        c.risk = "read".into();
        c
    };
    w.consent = false;
    let (r, sent) = reply("c1");
    relay.handle(Event::Call { call: toolbox_call("c1", "toolbox.search", "os.news"), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "consent_pending");
    assert!(exec.0.lock().unwrap().is_empty(), "nothing ran before consent");
    w.consent = true;
    // Not granted (a forged call): refused before the toolbox sees it.
    let (r, sent) = reply("c2");
    relay.handle(Event::Call { call: toolbox_call("c2", "toolbox.deep_crawl", "os.news"), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted");
    w.dev_all = true;
    let (r, sent) = reply("c3");
    relay.handle(Event::Call { call: toolbox_call("c3", "toolbox.search", "calendar"), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted", "developer mode grants no toolbox scope");
    w.dev_all = false;
    assert!(exec.0.lock().unwrap().is_empty());
    // Consented and granted: routed to the toolbox's executor, once.
    let (r, _) = reply("c4");
    relay.handle(Event::Call { call: toolbox_call("c4", "toolbox.search", "os.news"), reply: r }, &mut w);
    let calls = exec.0.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!((calls[0].0.name.as_str(), calls[0].0.calling_app.as_str()), ("toolbox.search", "os.news"));
    // A cancel reaches the toolbox.
    relay.handle(Event::Cancel { call_id: "c4".into(), reason: "timeout".into() }, &mut w);
    assert_eq!(*exec.1.lock().unwrap(), ["c4"]);
}
