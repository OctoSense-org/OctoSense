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
    /// The clock budgets count days by.
    now: u64,
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
            now: 1_000_000,
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
    fn withdraw_approval(&mut self, id: &RequestId, reason: &str) {
        self.router.withdraw(id, reason, 2);
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
    fn now(&self) -> u64 {
        self.now
    }
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

#[cfg(any(feature = "app-hub", native_mobile))]
#[test]
fn cold_mail_loads_calendar_without_preparing_its_agent() {
    // Process-wide registries must start empty; a separate test process also
    // keeps its temporary app root away from concurrently running UI tests.
    const CHILD: &str = "OCTOSENSE_TEST_COLD_MAIL_CALENDAR";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "host_tools::tests::cold_mail_loads_calendar_without_preparing_its_agent", "--nocapture"])
            .env(CHILD, "1").output().unwrap();
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        return;
    }
    use crate::system_chat::session::{ShellSystemHost, SystemHost};
    let root = std::env::temp_dir().join(format!("octosense-cold-mail-calendar-{}", std::process::id()));
    octosense_appstore::set_data_root(root.clone());
    octosense_app_hub_app::system_apps();
    crate::apps::register_mail_services();
    assert!(!super::with_relay(|r| r.catalog.knows("os.calendar")));
    super::script_apps::load("os.mail").unwrap();
    let shared: BTreeSet<String> = super::with_relay(|r| r.catalog.declarations("os.mail", false))
        .iter().filter(|d| d["app"] == "os.calendar")
        .map(|d| d["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(shared, ["calendar.events", "calendar.add_event", "calendar.notify"].into_iter().map(String::from).collect());
    assert_eq!(crate::agents::prepared("os.calendar"), None);
    let system = ShellSystemHost.declarations();
    for name in &shared {
        assert!(system.iter().any(|d| d["name"] == *name && d["app"] == "os.calendar"));
    }
    assert!(!system.iter().any(|d| d["name"] == "calendar.remove_event"));
    let mut request = call("cold-read", "calendar.events", "card.os.mail");
    request.app = "os.calendar".into(); request.args = json!({});
    let (reply, sent) = reply("cold-read");
    let mut world = World::new(FixedDevMode::off());
    super::with_relay(|r| r.handle(Event::Call { call: request, reply }, &mut world));
    super::script_apps::poll();
    assert_eq!(sent.lock().unwrap()[0]["ok"], true, "cold Mail registered Calendar's real host service");
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(any(feature = "app-hub", native_mobile))]
#[test]
fn admitted_mail_calendar_grants_route_to_the_real_calendar_store() {
    use super::script_apps::{self, HostServiceExecutor};
    let calendar_dir = script_apps::tests::stamped_bundle("calendar", "mail-cross-app", |_, _| {});
    let mail_dir = script_apps::tests::stamped_bundle("mail", "calendar-cross-app", |_, _| {});
    let calendar = script_apps::from_bundle(&calendar_dir).unwrap();
    let mail = script_apps::from_bundle(&mail_dir).unwrap();
    let root = calendar_dir.join("test-host");
    octosense_calendar_service::register();
    let mut relay = Relay::default();
    relay.catalog.declare("os.calendar", calendar.tools);
    relay.catalog.declare("os.mail", mail.tools);
    relay.set_executor("os.calendar", Some(Arc::new(HostServiceExecutor {
        app: "os.calendar".into(), tools: calendar.host_service_tools,
        families: calendar.families, host_dir: root.clone(),
    })));
    let mut world = World::new(FixedDevMode::off());
    let mut add = call("delivery-without-grant", "calendar.add_event", "card.os.mail");
    add.app = "os.calendar".into();
    add.args = json!({"title":"Fixture delivery", "start":"2026-10-06T09:00",
        "timezone":"America/Los_Angeles", "request_id":"fictional-delivery"});
    let (r, sent) = reply("delivery-without-grant");
    relay.handle(Event::Call { call: add.clone(), reply:r }, &mut world);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted");
    assert!(octosense_calendar_service::load(&root).is_empty());
    for tool in &mail.asks { relay.catalog.grant("os.mail", "os.calendar", tool); }
    let offered = relay.catalog.declarations("os.mail", false);
    let shared: BTreeSet<_> = offered.iter().filter(|d| d["app"] == "os.calendar")
        .map(|d| d["name"].as_str().unwrap()).collect();
    assert_eq!(shared, ["calendar.events", "calendar.add_event", "calendar.notify"].into_iter().collect());
    assert!(!relay.catalog.may_call("os.mail", "os.calendar", "calendar.remove_event", false));
    assert!(!relay.catalog.may_call("os.news", "os.calendar", "calendar.add_event", false));
    for (id, system) in [("delivery-mail", false), ("delivery-system", true)] {
        let mut request = add.clone(); request.call_id = id.into();
        if system {
            request.caller_kind = CallerKind::System;
            request.origin = CallOrigin::System;
            world.system.insert("calendar.add_event".into());
        }
        let (r, sent) = reply(id);
        relay.handle(Event::Call { call: request, reply:r }, &mut world);
        for _ in 0..100 {
            script_apps::poll();
            if !sent.lock().unwrap().is_empty() { break; }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(sent.lock().unwrap()[0]["ok"], true, "the owner's executor answers {id}");
    }
    let events = octosense_calendar_service::load(&root);
    assert_eq!(events.len(), 1, "the same delivery is not duplicated by another caller's retry");
    assert_eq!(events[0].timezone, "America/Los_Angeles");
    assert_eq!(events[0].start, "2026-10-06T09:00");
    assert!(world.asked.is_empty(), "a granted local calendar write is not an outward send");
    let _ = std::fs::remove_dir_all(calendar_dir);
    let _ = std::fs::remove_dir_all(mail_dir);
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
    relay.catalog.grant("calendar", "notes", "mail.send");
    let (r, sent) = reply("c1b");
    relay.handle(Event::Call { call: call("c1b", "mail.send", "calendar"), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted", "a grant names its owning app");
    relay.catalog.grant("calendar", "mail", "mail.send");
    relay.catalog.grant("calendar", "mail", "mail.purge");
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
    system.args = json!({"command": "ls -la"});
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
    assert_eq!(w.bus, vec![(format!("{BUS_PREFIX}c5"), "terminal".into(), "run".into(), json!({"command": "ls -la"}).to_string())]);
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
    relay.catalog.grant("calendar", "rinx", "rinx.message.send");
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
    assert_eq!(forwarded.trigger, Trigger::Unknown, "a context turn nobody vouched for is never 'the person'");
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

/// The turn that raised a `host_tool` approval ended before anyone answered
/// (the app's own Stop): the sheet stops asking, the audit says so, and the
/// kernel, which dropped the request, is sent nothing.
#[test]
fn a_host_tool_approval_whose_turn_ended_is_withdrawn_from_the_sheet() {
    let mut relay = Relay::default();
    let mut w = World::new(FixedDevMode::off());
    let answers: Arc<Mutex<Vec<bool>>> = Arc::default();
    let a = answers.clone();
    let approval = HostToolApproval::parse(
        &json!({"approval_id": "a1", "turn_id": "t", "approval_kind": "host_tool", "typed_details": {"host_tool": {"app": "news", "tool": "news.share", "args": {"to": "team@example.org"}, "risk": "destructive", "calling_kind": "app_peer", "calling_peer": "news-1"}}}),
        "s#peer-news-1",
    )
    .unwrap();
    relay.handle(Event::Approval { app: "news".into(), account: None, approval, answer: ApprovalAnswer::new(move |ok| a.lock().unwrap().push(ok)) }, &mut w);
    let id = RequestId("hostappr:a1".into());
    assert!(w.router.is_pending(&id) && w.router.front_sheet().is_some(), "on the shell's sheet");
    relay.handle(Event::ApprovalClosed { approval_id: "a1".into() }, &mut w);
    assert!(!w.router.is_pending(&id));
    assert!(w.router.front_sheet().is_none(), "the sheet stops asking");
    let entry = w.router.audit.all().last().unwrap().clone();
    assert_eq!((entry.id.as_str(), entry.by.as_str(), entry.result.as_str()), ("hostappr:a1", "withdrawn", "denied"));
    for event in w.decided() {
        relay.handle(event, &mut w);
    }
    assert!(answers.lock().unwrap().is_empty(), "nothing is answered for a request the kernel dropped");
    // Once more, or for an approval the relay never held: nothing happens.
    relay.handle(Event::ApprovalClosed { approval_id: "a1".into() }, &mut w);
    relay.handle(Event::ApprovalClosed { approval_id: "never".into() }, &mut w);
    assert_eq!(w.router.audit.all().len(), 1);
}

#[test]
fn an_app_that_is_not_running_is_refused_visibly() {
    // An app with neither an executor nor a native app's bus service.
    let mut relay = Relay::default();
    relay.catalog.declare("jot", vec![decl("jot.add", false, "host")]);
    let mut w = World::new(FixedDevMode::off());
    let (r, sent) = reply("c1");
    relay.handle(Event::Call { call: call("c1", "jot.add", "jot"), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "app_not_running");
    let _ = Decision::Deny;
}

#[test]
fn the_shipped_catalog_offers_the_terminals_run_to_those_granted_it() {
    let catalog = Catalog::shipped();
    let run = catalog.entry("terminal", TERMINAL_RUN).unwrap();
    assert_eq!((run["risk"].as_str(), run["confirm"].as_str()), (Some("destructive"), Some("host")));
    assert!(catalog.declarations("rinx", false).is_empty(), "nobody gets it without a grant");
    assert_eq!(catalog.declarations("rinx", true).len(), shareable_native_tools().len(), "developer mode grants every shareable tool");
}

/// G3: the native apps' agent blocks (`native-apps.json`) are the shipped
/// catalog: the Terminal's own tools, App Hub's read tools, each app's
/// exact kernel tools.
#[test]
fn the_shipped_catalog_is_the_native_apps_agent_blocks() {
    let catalog = Catalog::shipped();
    for tool in ["terminal.run", "terminal.read_screen", "terminal.read_scrollback"] {
        assert!(catalog.entry("terminal", tool).is_some(), "{tool}");
        assert_eq!(catalog.owner_of(tool).as_deref(), Some("terminal"));
    }
    let rinx = crate::native_apps::find("rinx").unwrap();
    assert_eq!(catalog.generic("rinx", false), rinx.generic_tools.iter().map(|t| t.to_string()).collect::<Vec<_>>());
    assert!(catalog.generic("rinx", false).contains(&"ask_user_question".to_string()));
    assert!(catalog.generic("sheets", false).is_empty(), "an app granted no kernel tools keeps none");
    assert!(catalog.generic("nowhere", false).is_empty());
    for dev in [false, true] {
        for shell in super::relay::OCTOS_SHELL {
            assert!(!catalog.generic("rinx", dev).iter().any(|t| t == shell), "never octos's shell");
        }
    }
    // Every declaration names its owning app; the Terminal's own agent is
    // offered its read tools only (`agent.own_tools`).
    let own = Catalog::shipped().declarations("terminal", false);
    assert_eq!(own.iter().map(|d| d["name"].as_str().unwrap()).collect::<Vec<_>>(), ["terminal.read_screen", "terminal.read_scrollback"]);
    assert!(own.iter().all(|d| d["app"] == "terminal" && d.get("auto_approvable").is_none()));
    // App Hub's own agent only reads: the catalog, the library, the updates.
    let hub = Catalog::shipped().declarations("apphub", false);
    assert_eq!(hub.iter().map(|d| d["name"].as_str().unwrap()).collect::<Vec<_>>(), ["apphub.search", "apphub.installed", "apphub.updates"]);
    assert!(hub.iter().all(|d| d["app"] == "apphub" && d["risk"] == "read"), "{hub:?}");
}

#[test]
fn a_kernel_tool_list_never_keeps_octos_shell() {
    let mut catalog = Catalog::default();
    catalog.set_generic("notes", vec!["read_file".into(), "shell".into(), "bash".into(), "exec_command".into(), "web_search".into()]);
    assert_eq!(catalog.generic("notes", false), vec!["read_file".to_string(), "web_search".to_string()]);
}

/// The Terminal's read tools run on its AI bus service, in every hosting.
#[test]
fn the_terminals_read_tools_are_granted_then_read_on_the_bus() {
    let mut relay = Relay::default();
    let mut w = World::new(FixedDevMode::off());
    let (r, sent) = reply("c1");
    relay.handle(Event::Call { call: call("c1", "terminal.read_screen", "rinx"), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted");
    relay.catalog.grant("rinx", "terminal", "terminal.read_screen");
    let mut c = call("c2", "terminal.read_screen", "rinx");
    c.args = json!({});
    let (r, sent) = reply("c2");
    relay.handle(Event::Call { call: c, reply: r }, &mut w);
    assert_eq!(w.bus, vec![(format!("{BUS_PREFIX}c2"), "terminal".into(), "read_screen".into(), "{}".into())]);
    relay.handle(Event::BusResult { call_id: "c2".into(), outcome: ToolOutcome::Ok(json!({"text": "$ ls"})) }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["data"]["text"], "$ ls");
}

/// A native app's own read tool (Calculator's `eval`) reaches the system
/// agent once granted, and runs on the app's AI bus service by its short
/// name: the Terminal is no longer the only native app the bus serves.
#[test]
fn a_native_apps_read_tool_runs_on_its_bus_service() {
    assert!(super::relay::serves_on_bus("terminal") && super::relay::serves_on_bus("calculator") && super::relay::serves_on_bus("notes"));
    assert!(!super::relay::serves_on_bus("sheets"), "an app that declares no tools has none to serve");
    assert!(!super::relay::serves_on_bus("os.mail") && !super::relay::serves_on_bus("nowhere"));
    let mut relay = Relay::default();
    let mut w = World::new(FixedDevMode::off());
    let mut system = call("c1", "calculator.eval", "system");
    system.args = json!({"expression": "6*7"});
    system.caller_kind = CallerKind::System;
    system.origin = CallOrigin::System;
    let (r, sent) = reply("c1");
    relay.handle(Event::Call { call: system.clone(), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted");
    w.system.insert("calculator.eval".into());
    system.call_id = "c2".into();
    let (r, sent) = reply("c2");
    relay.handle(Event::Call { call: system, reply: r }, &mut w);
    assert_eq!(w.bus, vec![(format!("{BUS_PREFIX}c2"), "calculator".into(), "eval".into(), json!({"expression": "6*7"}).to_string())]);
    relay.handle(Event::BusResult { call_id: "c2".into(), outcome: ToolOutcome::Ok(json!({"text": "42"})) }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["ok"], true);
}

/// The Terminal's own agent reads, never types: its entry narrows it to
/// `read_screen` and `read_scrollback` (`agent.own_tools`), so it is offered
/// only those and its `run` is refused. The system agent's `run` keeps the
/// host's sheet: a tool the host confirms never takes the app's link.
#[test]
fn the_terminals_own_agent_reads_only_and_run_never_takes_its_link() {
    let catalog = super::relay::Catalog::shipped();
    let offered: Vec<String> = catalog.offered("terminal", false, true).iter().filter_map(|d| d["name"].as_str().map(String::from)).collect();
    assert!(offered.iter().any(|n| n == "terminal.read_screen") && offered.iter().any(|n| n == "terminal.read_scrollback"), "{offered:?}");
    assert!(!offered.iter().any(|n| n == "terminal.run"), "{offered:?}");
    assert!(catalog.own_allows("calculator", "calculator.eval"), "an entry that does not narrow keeps every tool");

    let mut relay = Relay::default();
    let mut w = World::new(FixedDevMode::off());
    w.links.push("terminal".into());
    // Its own agent: `run` refused, a read over its link.
    let mut own_run = call("t1", "terminal.run", "terminal");
    own_run.args = json!({"command": "ls"});
    let (r, sent) = reply("t1");
    relay.handle(Event::Call { call: own_run, reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted");
    let mut own_read = call("t2", "terminal.read_screen", "terminal");
    own_read.args = json!({});
    own_read.risk = "read".into();
    let (r, _sent) = reply("t2");
    relay.handle(Event::Call { call: own_read, reply: r }, &mut w);
    assert_eq!(w.link_calls.iter().map(|(app, c)| (app.as_str(), c.name.as_str())).collect::<Vec<_>>(), [("terminal", "terminal.read_screen")]);
    // The system agent's `run`: the host's sheet, not the link.
    w.system.insert(TERMINAL_RUN.into());
    let mut system_run = call("t3", TERMINAL_RUN, "system");
    system_run.args = json!({"command": "ls"});
    system_run.caller_kind = CallerKind::System;
    system_run.origin = CallOrigin::System;
    system_run.risk = "destructive".into();
    system_run.confirm_required = true;
    let (r, _sent) = reply("t3");
    relay.handle(Event::Call { call: system_run, reply: r }, &mut w);
    assert_eq!(w.link_calls.len(), 1, "run did not take the link");
    assert_eq!(w.asked.iter().map(|(app, tool, ..)| (app.as_str(), tool.name.as_str())).collect::<Vec<_>>(), [("terminal", TERMINAL_RUN)]);
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

/// Every native app's shareable tool (`native-apps.json`): what developer
/// mode grants another app's agent.
fn shareable_native_tools() -> Vec<String> {
    crate::native_apps::APPS
        .iter()
        .flat_map(|app| serde_json::from_str::<Vec<Value>>(app.tools_json).unwrap())
        .filter(|tool| tool["shareable"] == true)
        .map(|tool| tool["name"].as_str().unwrap().to_string())
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
    let mut every = shareable_native_tools();
    every.push(super::relay::DEV_RUN.to_string());
    every.sort();
    assert_eq!(offered_names(&relay, "calendar", true, true), every);
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
        // Arguments the tool declares (the relay checks them, G8).
        c.args = if name == "toolbox.deep_crawl" { json!({"url": "https://example.org"}) } else { json!({"query": "news"}) };
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

// ---------------------------------------------------------------- triggers (G2)

use crate::ai_host::app_peers::TurnTrigger;
use crate::approvals::rules::{ApprovalGesture, Conditions, RuleDraft};

fn approval_with(id: &str, trigger: TurnTrigger) -> HostToolApproval {
    let mut a = HostToolApproval::parse(
        &json!({"approval_id": id, "turn_id": "t", "approval_kind": "host_tool", "typed_details": {"host_tool": {"app": "mail", "tool": "mail.send", "args": {"to": ["bo@example.org"]}, "risk": "act", "calling_kind": "app_peer", "calling_peer": "mail-1", "context_id": "ctx"}}}),
        "s#peerctx-mail-1.ctx",
    )
    .unwrap();
    a.trigger = trigger;
    a
}

#[test]
fn a_calls_trigger_is_the_one_its_host_stamped_and_unknown_by_default() {
    let (mut relay, exec) = relay_with("notes", vec![decl("notes.add", false, "app")]);
    let mut w = World::new(FixedDevMode::off());
    let cases = [
        (TurnTrigger::Unknown, CallOrigin::Context, Trigger::Unknown),
        (TurnTrigger::Person, CallOrigin::Context, Trigger::Person),
        (TurnTrigger::Incoming { from: Some("@bo:x".into()) }, CallOrigin::Context, Trigger::IncomingContent { from: Some("@bo:x".into()) }),
        (TurnTrigger::App, CallOrigin::PeerOwn, Trigger::App),
        (TurnTrigger::Unknown, CallOrigin::PeerOwn, Trigger::Unknown),
        (TurnTrigger::Unknown, CallOrigin::System, Trigger::Unknown),
        (TurnTrigger::Person, CallOrigin::PeerInput, Trigger::SystemAgent),
        // An app saying the person started it is the app's run: only a
        // shell surface makes a turn the person's.
        (TurnTrigger::AppSaysPerson, CallOrigin::Context, Trigger::App),
    ];
    for (i, (stamped, origin, want)) in cases.into_iter().enumerate() {
        let id = format!("c{i}");
        let mut c = call(&id, "notes.add", "notes");
        c.confirm_required = true;
        c.trigger = stamped;
        c.origin = origin;
        let (r, _) = reply(&id);
        relay.handle(Event::Call { call: c, reply: r }, &mut w);
        assert_eq!(w.asked[i].3.trigger, want, "case {i}");
    }
    assert!(exec.0.lock().unwrap().is_empty(), "nothing ran before the app's sheet");
}

#[test]
fn a_context_approval_is_never_triggered_by_the_person_unless_stamped() {
    let mut relay = Relay::default();
    let mut w = World::new(FixedDevMode::off());
    // A rule for Mail's own agent, "only when I asked".
    let by_person = Conditions { triggered_by_person: true, ..Conditions::default() };
    w.router.create_rule(&ApprovalGesture::settings_tap(), RuleDraft::tool("mail", "mail.send", by_person), 1).unwrap();
    let answers: Arc<Mutex<Vec<bool>>> = Arc::default();
    let answer = || {
        let a = answers.clone();
        ApprovalAnswer::new(move |ok| a.lock().unwrap().push(ok))
    };
    // Unknown and incoming content: the rule never answers, the person does.
    for (id, trigger) in [("a1", TurnTrigger::Unknown), ("a2", TurnTrigger::Incoming { from: Some("@eve:x".into()) })] {
        relay.handle(Event::Approval { app: "mail".into(), account: None, approval: approval_with(id, trigger), answer: answer() }, &mut w);
        assert!(w.router.is_pending(&RequestId(format!("hostappr:{id}"))), "{id} waits for the person");
    }
    assert_ne!(w.asked[0].3.trigger, Trigger::Person);
    assert!(matches!(w.asked[1].3.trigger, Trigger::IncomingContent { .. }));
    assert!(w.decided().is_empty() && answers.lock().unwrap().is_empty());
    // The person's own turn: the rule answers.
    relay.handle(Event::Approval { app: "mail".into(), account: None, approval: approval_with("a3", TurnTrigger::Person), answer: answer() }, &mut w);
    for event in w.decided() {
        relay.handle(event, &mut w);
    }
    assert_eq!(answers.lock().unwrap().as_slice(), &[true]);
}

// ---------------------------------------------------------------- every approval (G4)

fn octos_approval(id: &str, tool: &str, peer_app: &str, client: Option<&str>) -> HostToolApproval {
    let mut a = HostToolApproval::parse_octos(&json!({"approval_id": id, "turn_id": "t", "tool_name": tool, "title": "Write notes.md", "body": "b"}), "s#peerctx-x.ctx", peer_app).unwrap();
    a.context_id = client.map(|_| "ctx".to_string());
    a.client = client.map(str::to_string);
    a
}

#[test]
fn octos_own_approvals_on_an_apps_peer_go_to_the_shells_sheet() {
    let mut relay = Relay::default();
    let mut w = World::new(FixedDevMode::off());
    let answers: Arc<Mutex<Vec<bool>>> = Arc::default();
    let answer = || {
        let a = answers.clone();
        ApprovalAnswer::new(move |ok| a.lock().unwrap().push(ok))
    };
    // Rinx's mini app, in a context: Rinx's own agent, for that client.
    relay.handle(Event::Approval { app: "rinx".into(), account: Some("@a:x".into()), approval: octos_approval("o1", "write_file", "rinx", Some("mini.news")), answer: answer() }, &mut w);
    let (app, spec, caller, context) = w.asked[0].clone();
    assert_eq!((app.as_str(), spec.name.as_str()), ("rinx", "write_file"));
    assert_eq!(caller, Caller::OwnAgent { client: Some("mini.news".into()) });
    assert_eq!(context.context_id.as_deref(), Some("ctx"));
    assert!(w.router.is_pending(&RequestId("hostappr:o1".into())), "on the shell's sheet, not the app's");
    // A script app's peer (`card.<id>`): the app, not the peer name.
    relay.handle(Event::Approval { app: "card.com.example.trip".into(), account: None, approval: octos_approval("o2", "write_file", "card.com.example.trip", None), answer: answer() }, &mut w);
    assert_eq!(w.asked[1].0, "com.example.trip");
    assert_eq!(w.asked[1].2, Caller::OwnAgent { client: None });
    // octos's shell is a command: never a rule.
    relay.handle(Event::Approval { app: "rinx".into(), account: None, approval: octos_approval("o3", "shell", "rinx", None), answer: answer() }, &mut w);
    assert!(w.asked[2].1.command && !w.asked[2].1.auto_approvable);
    // The person answers the first: the kernel hears it once.
    let sheet = w.router.sheets().iter().find(|s| s.lines.iter().any(|l| l.request.0 == "hostappr:o1")).unwrap().id;
    w.router.answer(sheet, &RequestId("hostappr:o1".into()), crate::approvals::sheet::Answer::Once, &crate::approvals::rules::ApprovalGesture::sheet_tap(), 2).unwrap();
    for event in w.decided() {
        relay.handle(event, &mut w);
    }
    assert_eq!(answers.lock().unwrap().as_slice(), &[true]);
}

#[test]
fn developer_mode_answers_octos_own_approvals_through_the_router() {
    let mut relay = Relay::default();
    let mut w = World::new(FixedDevMode::all());
    let answers: Arc<Mutex<Vec<bool>>> = Arc::default();
    let a = answers.clone();
    relay.handle(Event::Approval { app: "rinx".into(), account: None, approval: octos_approval("o1", "write_file", "rinx", Some("mini")), answer: ApprovalAnswer::new(move |ok| a.lock().unwrap().push(ok)) }, &mut w);
    for event in w.decided() {
        relay.handle(event, &mut w);
    }
    assert_eq!(answers.lock().unwrap().as_slice(), &[true]);
    assert_eq!(w.router.audit.all()[0].by, "developer_mode");
}


// ---------------------------------------------------------------- G8

fn schema_decl() -> Value {
    json!({"name": "notes.find", "description": "d", "risk": "read", "shareable": false,
        "input_schema": {"type": "object", "properties": {"q": {"type": "string", "maxLength": 8}, "limit": {"type": "integer", "minimum": 1, "maximum": 10}}, "required": ["q"], "additionalProperties": false},
        "output_schema": {"type": "object", "properties": {"hits": {"type": "array", "items": {"type": "string"}}}, "required": ["hits"]}})
}

fn find(id: &str, turn: &str, args: Value) -> HostToolCall {
    let mut c = call(id, "notes.find", "notes");
    c.args = args;
    c.turn_id = turn.to_string();
    c
}

/// ADR 0004 §3 (G8): each call's arguments are checked against the tool's
/// declared schema before anything runs.
#[test]
fn arguments_outside_the_declared_schema_are_refused_before_anything_runs() {
    let (mut relay, exec) = relay_with("notes", vec![schema_decl()]);
    let mut w = World::new(FixedDevMode::off());
    for (id, args, why) in [
        ("c1", json!({}), "q is required"),
        ("c2", json!({"q": 7}), "expected string"),
        ("c3", json!({"q": "far too long"}), "longer than 8"),
        ("c4", json!({"q": "x", "limit": 99}), "above the maximum"),
        ("c5", json!({"q": "x", "sudo": true}), "sudo is not a declared field"),
    ] {
        let (r, sent) = reply(id);
        relay.handle(Event::Call { call: find(id, "t1", args), reply: r }, &mut w);
        let sent = sent.lock().unwrap().clone();
        assert_eq!(sent[0]["error"]["kind"], "invalid_args", "{id}");
        assert!(sent[0]["error"]["message"].as_str().unwrap().contains(why), "{id}: {sent:?}");
    }
    let (r, sent) = reply("c6");
    relay.handle(Event::Call { call: find("c6", "t1", json!({"q": "x" .repeat(70_000)})), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "invalid_args", "over the size cap");
    assert!(exec.0.lock().unwrap().is_empty(), "nothing reached the app");
    let (r, _) = reply("c7");
    relay.handle(Event::Call { call: find("c7", "t1", json!({"q": "ok", "limit": 3})), reply: r }, &mut w);
    assert_eq!(exec.0.lock().unwrap().len(), 1);
}

/// Results are capped and checked against the declared result schema.
#[test]
fn results_are_capped_and_checked_against_the_declared_result() {
    let (mut relay, exec) = relay_with("notes", vec![schema_decl()]);
    let mut w = World::new(FixedDevMode::off());
    let mut sents = Vec::new();
    for id in ["r1", "r2", "r3"] {
        let (r, sent) = reply(id);
        relay.handle(Event::Call { call: find(id, "t1", json!({"q": "x"})), reply: r }, &mut w);
        sents.push(sent);
    }
    let replies: Vec<ToolReply> = exec.0.lock().unwrap().iter().map(|(_, r)| r.clone()).collect();
    replies[0].finish(ToolOutcome::Ok(json!({"hits": ["a", "b"]})));
    replies[1].finish(ToolOutcome::Ok(json!({"hits": [1]})));
    replies[2].finish(ToolOutcome::Ok(json!({"hits": ["x".repeat(super::relay::MAX_RESULT_BYTES)]})));
    assert_eq!(sents[0].lock().unwrap()[0]["data"], json!({"hits": ["a", "b"]}));
    assert_eq!(sents[1].lock().unwrap()[0]["error"]["kind"], "invalid_result");
    assert_eq!(sents[2].lock().unwrap()[0]["error"]["kind"], "result_too_large");
    // An error passes through as the app said it.
    let (r, sent) = reply("r4");
    relay.handle(Event::Call { call: find("r4", "t1", json!({"q": "x"})), reply: r }, &mut w);
    exec.0.lock().unwrap()[3].1.finish(ToolOutcome::error("not_found", "none"));
    assert_eq!(sent.lock().unwrap()[0]["error"], json!({"kind": "not_found", "message": "none"}));
}

/// Per-app budgets: calls per turn and per day, from the manifest (or the
/// defaults); a new turn, and a new day, start again.
#[test]
fn an_agents_calls_are_budgeted_per_turn_and_per_day() {
    let (mut relay, exec) = relay_with("notes", vec![schema_decl()]);
    relay.catalog.set_budget("notes", Some(2), Some(3));
    let mut w = World::new(FixedDevMode::off());
    let mut kinds = Vec::new();
    for (id, turn) in [("b1", "t1"), ("b2", "t1"), ("b3", "t1"), ("b4", "t2"), ("b5", "t2")] {
        let (r, sent) = reply(id);
        relay.handle(Event::Call { call: find(id, turn, json!({"q": "x"})), reply: r }, &mut w);
        kinds.push(sent.lock().unwrap().first().map(|s| s["error"]["kind"].as_str().unwrap_or("").to_string()).unwrap_or_default());
    }
    assert_eq!(kinds, ["", "", "budget_exceeded", "", "budget_exceeded"], "2 per turn, 3 per day");
    assert_eq!(exec.0.lock().unwrap().len(), 3);
    w.now += 86_400;
    let (r, sent) = reply("b6");
    relay.handle(Event::Call { call: find("b6", "t3", json!({"q": "x"})), reply: r }, &mut w);
    assert!(sent.lock().unwrap().is_empty(), "a new day");
    assert_eq!(exec.0.lock().unwrap().len(), 4);
    // The defaults, and a native app's own budget from its manifest.
    assert_eq!(Catalog::default().budget("anyone"), super::relay::Budget::default());
    assert_eq!(super::relay::Budget::default().per_turn, super::relay::DEFAULT_CALLS_PER_TURN);
}


/// An expiry's reason reaches the kernel's record with the denial.
#[test]
fn an_expired_host_tool_approval_is_denied_with_its_reason() {
    let mut relay = Relay::default();
    let mut w = World::new(FixedDevMode::off());
    let answers: Arc<Mutex<Vec<(bool, String)>>> = Arc::default();
    let a = answers.clone();
    let answer = ApprovalAnswer::with_note(move |ok, note| a.lock().unwrap().push((ok, note.to_string())));
    let approval = HostToolApproval::parse(
        &json!({"approval_id": "a9", "turn_id": "t", "approval_kind": "host_tool", "typed_details": {"host_tool": {"app": "mail", "tool": "mail.send", "args": {"to": ["bo@example.org"]}, "risk": "act", "outward": true, "calling_kind": "app_peer", "calling_peer": "calendar-1"}}}),
        "s#peer-calendar-1",
    )
    .unwrap();
    relay.handle(Event::Approval { app: "calendar".into(), account: None, approval, answer }, &mut w);
    w.router.sheet_expiry_s = 600;
    w.router.tick(1 + 599);
    assert!(w.decided().is_empty());
    w.router.tick(1 + 600);
    for event in w.decided() {
        relay.handle(event, &mut w);
    }
    assert_eq!(answers.lock().unwrap().as_slice(), &[(false, "expired: no answer in 10 min".to_string())]);
    assert_eq!(w.router.expired().len(), 1);
}

/// A script app's `tools.json` may say `auto_approvable: false` (App Hub's
/// `ToolSpec`): its approvals then never go to a standing rule, whatever
/// the host's own rule says (ADR 0004 §8).
#[test]
fn a_script_apps_declared_auto_approvable_false_holds_on_its_approvals() {
    let mut relay = Relay::default();
    let mut pay = decl("pay.transfer", true, "host");
    pay["auto_approvable"] = json!(false);
    relay.catalog.declare("com.example.pay", vec![pay, decl("pay.quote", true, "host")]);
    let mut w = World::new(FixedDevMode::off());
    for (id, tool) in [("a1", "pay.transfer"), ("a2", "pay.quote")] {
        let approval = HostToolApproval::parse(
            &json!({"approval_id": id, "turn_id": "t", "approval_kind": "host_tool", "typed_details": {"host_tool": {"app": "com.example.pay", "tool": tool, "args": {}, "risk": "act", "outward": true, "calling_kind": "system"}}}),
            "s#system",
        )
        .unwrap();
        relay.handle(Event::Approval { app: "system".into(), account: None, approval, answer: ApprovalAnswer::new(|_| {}) }, &mut w);
    }
    assert!(!w.asked[0].1.auto_approvable, "declared false: no rule answers it");
    assert!(w.asked[1].1.auto_approvable, "omitted: a rule may");
}

// ------------------------------------------------------------ dev.run (ADR 0004 §13)

fn dev_run_call(id: &str, calling: &str, owner: &str) -> HostToolCall {
    let mut c = call(id, super::relay::DEV_RUN, calling);
    c.app = owner.into();
    c.risk = "destructive".into();
    c.args = json!({"command": "echo hi"});
    c
}

/// `dev.run` is offered only to the peers of apps developer mode covers,
/// as the app's own tool; never to the system agent's session.
#[test]
fn dev_run_is_offered_only_under_developer_mode_as_the_apps_own_tool() {
    let relay = Relay::default();
    let offered = |app: &str, dev: bool| relay.catalog.offered(app, dev, true);
    assert!(!offered("os.news", false).iter().any(|d| d["name"] == super::relay::DEV_RUN), "off: not offered");
    let decl = offered("os.news", true).into_iter().find(|d| d["name"] == super::relay::DEV_RUN).expect("offered under developer mode");
    assert_eq!(decl["app"], "os.news", "the app's own tool");
    assert_eq!((decl["risk"].as_str(), decl["confirm"].as_str()), (Some("destructive"), Some("host")));
    assert_eq!(decl["input_schema"]["required"], json!(["command"]));
    assert!(!offered(super::SYSTEM, true).iter().any(|d| d["name"] == super::relay::DEV_RUN), "never on the system agent's session");
    // The system chat's registration (the session a Talk to Octos client can
    // reach) never carries it, whatever the switches.
    for (commands, process) in [(false, false), (true, true)] {
        assert!(!crate::system_chat::grants::host_tools_given(commands, process).contains(super::relay::DEV_RUN));
    }
}

/// A covered app's own agent's `dev.run` runs on the shell's executor, even
/// for a process app with a peer link; anyone else, or once developer mode
/// is off, is refused before anything runs.
#[test]
fn dev_run_runs_on_the_shell_only_for_a_covered_apps_own_agent() {
    let mut relay = Relay::default();
    let host = Arc::new(Exec::default());
    relay.set_executor(super::relay::HOST_EXECUTOR, Some(host.clone()));
    let mut w = World::new(FixedDevMode::all());
    w.links.push("terminal".into());
    w.dev_all = true;
    let (r, _) = reply("c1");
    relay.handle(Event::Call { call: dev_run_call("c1", "terminal", "terminal"), reply: r }, &mut w);
    assert_eq!(host.0.lock().unwrap().len(), 1, "run by the shell");
    assert!(w.link_calls.is_empty(), "never down the app's link");
    // Arguments are checked against its schema.
    let mut bad = dev_run_call("c2", "terminal", "terminal");
    bad.args = json!({"cmd": "ls"});
    let (r, sent) = reply("c2");
    relay.handle(Event::Call { call: bad, reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "invalid_args");
    // Another app's agent, or the system agent: never.
    let (r, sent) = reply("c3");
    relay.handle(Event::Call { call: dev_run_call("c3", "calendar", "terminal"), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted");
    let mut system = dev_run_call("c4", super::SYSTEM, "terminal");
    system.caller_kind = CallerKind::System;
    let (r, sent) = reply("c4");
    relay.handle(Event::Call { call: system, reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted", "the system session never gets dev.run");
    // Developer mode off: a late call is refused.
    w.dev_all = false;
    let (r, sent) = reply("c5");
    relay.handle(Event::Call { call: dev_run_call("c5", "terminal", "terminal"), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted");
    assert_eq!(host.0.lock().unwrap().len(), 1, "nothing else ran");
}

/// `dev.run`'s approval is a command keyed to the calling app: developer
/// mode answers it (and audits it) for a covered app, the person otherwise.
#[test]
fn dev_runs_approval_is_a_command_developer_mode_answers_for_a_covered_app() {
    let mut relay = Relay::default();
    for (dev, answered) in [(FixedDevMode { all: false, apps: vec!["os.news".into()] }, true), (FixedDevMode::off(), false)] {
        let mut w = World::new(dev);
        let answers: Arc<Mutex<Vec<bool>>> = Arc::default();
        let a = answers.clone();
        let approval = HostToolApproval::parse(
            &json!({"approval_id": "d1", "turn_id": "t", "approval_kind": "host_tool", "typed_details": {"host_tool": {"app": "os.news", "tool": "dev.run", "args": {"command": "ls"}, "risk": "destructive", "calling_kind": "app_peer", "calling_peer": "news-1"}}}),
            "s#peer-news-1",
        )
        .unwrap();
        relay.handle(Event::Approval { app: "os.news".into(), account: None, approval, answer: ApprovalAnswer::new(move |ok| a.lock().unwrap().push(ok)) }, &mut w);
        assert!(w.asked[0].1.command, "a command: never a standing rule");
        assert_eq!(w.asked[0].0, "os.news", "keyed to the calling app");
        for event in w.decided() {
            relay.handle(event, &mut w);
        }
        assert_eq!(answers.lock().unwrap().as_slice(), if answered { &[true][..] } else { &[][..] });
    }
}

/// ADR 0004 §11 gap 7 (octos#2647 `read_parent`): an app's conversation reads
/// its account's folder only where the manifest says its agent works there
/// (`storage.agent_workspace: "account"`, the default) and the agent has
/// that workspace now (an app whose agent has no files, a suspended or a
/// refused account gets none).
#[test]
fn a_conversation_reads_the_account_folder_only_where_the_agent_works_there() {
    use crate::app_storage::{AgentWorkspace, StorageSpec};
    let folder = std::path::Path::new("/octosense/apps/rinx/accounts/a");
    let account = StorageSpec { agent_workspace: AgentWorkspace::Account, ..Default::default() };
    let none = StorageSpec { agent_workspace: AgentWorkspace::None, ..Default::default() };
    assert!(super::reads_account(&account, Some(folder)));
    assert!(super::reads_account(&StorageSpec::default(), Some(folder)), "\"account\" is the default");
    assert!(!super::reads_account(&account, None), "no workspace now: fenced");
    assert!(!super::reads_account(&none, Some(folder)), "agent_workspace none: fenced");
}

// ------------------------------------------------------------ the host read tools (ADR 0004 §11)

/// `files.list`, `files.read`, `files.search`: an app's own agent's calls
/// run on the shell (never down the app's link, no developer mode needed),
/// with their arguments checked; another app's agent and the system agent
/// are refused.
#[test]
fn the_host_read_tools_run_on_the_shell_for_the_apps_own_agent_only() {
    let mut relay = Relay::default();
    let host = Arc::new(Exec::default());
    relay.set_executor(super::relay::HOST_EXECUTOR, Some(host.clone()));
    let mut w = World::new(FixedDevMode::off());
    w.links.push("rinx".into());
    let files_call = |id: &str, name: &str, calling: &str, args: Value| {
        let mut c = call(id, name, calling);
        c.app = "rinx".into();
        c.risk = "read".into();
        c.args = args;
        c
    };
    let (r, _) = reply("c1");
    relay.handle(Event::Call { call: files_call("c1", super::files::READ, "rinx", json!({"path": "exports/room.md"})), reply: r }, &mut w);
    let (r, _) = reply("c2");
    relay.handle(Event::Call { call: files_call("c2", super::files::SEARCH, "rinx", json!({"query": "budget"})), reply: r }, &mut w);
    let ran: Vec<String> = host.0.lock().unwrap().iter().map(|(c, _)| c.name.clone()).collect();
    assert_eq!(ran, [super::files::READ, super::files::SEARCH]);
    assert_eq!(host.0.lock().unwrap()[0].0.context_id, None, "the call's own context is stamped by the host");
    assert!(w.link_calls.is_empty(), "never down the app's link");
    let (r, sent) = reply("c3");
    relay.handle(Event::Call { call: files_call("c3", super::files::READ, "rinx", json!({"file": "x"})), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "invalid_args");
    let (r, sent) = reply("c4");
    relay.handle(Event::Call { call: files_call("c4", super::files::LIST, "calendar", json!({})), reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted", "another app's folder: never");
    let mut system = files_call("c5", super::files::LIST, super::SYSTEM, json!({}));
    system.caller_kind = CallerKind::System;
    let (r, sent) = reply("c5");
    relay.handle(Event::Call { call: system, reply: r }, &mut w);
    assert_eq!(sent.lock().unwrap()[0]["error"]["kind"], "not_granted");
    assert_eq!(host.0.lock().unwrap().len(), 2);
    // Declared as the app's own read tools, no confirmation.
    for d in super::files::declarations("rinx") {
        assert_eq!((d["app"].as_str(), d["risk"].as_str(), d.get("confirm")), (Some("rinx"), Some("read"), None), "{d}");
    }
}

// ---------------------------------------------------------------- the audit (ADR 0004 §8, §12, §13)

/// Every tool call is audited when the relay receives it and when it ends
/// (answered, refused or cancelled): caller, owning app, tool, a digest of
/// the exact arguments (never the arguments), and the outcome.
#[test]
fn every_tool_call_is_audited_when_it_arrives_and_when_it_ends() {
    use super::relay::CallAudit;
    let (mut relay, exec) = relay_with("rinx", vec![decl("rinx.room.list", false, "host")]);
    let log: Arc<Mutex<Vec<CallAudit>>> = Arc::default();
    let sink = log.clone();
    relay.set_audit(Arc::new(move |e| sink.lock().unwrap().push(e)));
    let mut w = World::new(FixedDevMode::off());
    // Answered.
    let (r, _) = reply("a1");
    relay.handle(Event::Call { call: call("a1", "rinx.room.list", "rinx"), reply: r }, &mut w);
    exec.0.lock().unwrap()[0].1.finish(ToolOutcome::Ok(json!({"rooms": []})));
    // Refused (not granted).
    let (r, _) = reply("a2");
    relay.handle(Event::Call { call: call("a2", "rinx.admin.wipe", "rinx"), reply: r }, &mut w);
    // Cancelled while it runs.
    let (r, _) = reply("a3");
    relay.handle(Event::Call { call: call("a3", "rinx.room.list", "rinx"), reply: r }, &mut w);
    relay.handle(Event::Cancel { call_id: "a3".into(), reason: "interrupted".into() }, &mut w);
    let got: Vec<(String, String, String)> = log.lock().unwrap().iter().map(|e| (e.call_id.clone(), e.phase.clone(), e.outcome.clone())).collect();
    let want = [
        ("a1", "call", "received"),
        ("a1", "done", "ok"),
        ("a2", "call", "received"),
        ("a2", "done", "error:not_granted"),
        ("a3", "call", "received"),
        ("a3", "done", "cancelled"),
    ];
    assert_eq!(got, want.iter().map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string())).collect::<Vec<_>>());
    let e = log.lock().unwrap()[0].clone();
    assert_eq!((e.caller.as_str(), e.owner.as_str(), e.tool.as_str()), ("own_agent/mini.news", "rinx", "rinx.room.list"));
    assert_eq!(e.args_digest, crate::approvals::facts::digest(&json!({"to": ["ana@example.org"], "text": "hi"})));
    let line = serde_json::to_string(&e).unwrap();
    assert!(!line.contains("ana@example.org"), "never the arguments: {line}");
}

/// The shell's audit of tool calls is one owner-only JSON-lines file in the
/// home, beside the approvals audit.
#[test]
fn the_tool_call_audit_is_an_owner_only_file_in_the_home() {
    use super::relay::CallAudit;
    let home = std::env::temp_dir().join(format!("octosense-callaudit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let e = CallAudit { ts: 1, call_id: "c1".into(), caller: "system_agent".into(), owner: "terminal".into(), tool: "terminal.run".into(), args_digest: "sha256:x".into(), phase: "call".into(), outcome: "received".into() };
    crate::approvals::audit::append_call(&home, &e).unwrap();
    crate::approvals::audit::append_call(&home, &e).unwrap();
    let path = home.join(crate::approvals::audit::CALLS_FILE);
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().count(), 2);
    assert_eq!(serde_json::from_str::<CallAudit>(text.lines().next().unwrap()).unwrap(), e);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let _ = std::fs::remove_dir_all(home);
}

/// ADR 0004 §7: a grant names its owning app explicitly, by the tool's
/// namespace (the native app of that id, else the system app `os.<ns>`, the
/// toolbox for its own), never whichever app declared the name first.
#[test]
fn a_grants_owner_is_the_namespaces_app_never_the_first_declarer() {
    let mut c = Catalog::shipped();
    c.declare("os.mail", vec![decl("mail.send", true, "host")]);
    c.declare("com.evil.mail", vec![decl("mail.send", true, "host")]);
    assert_eq!(c.owner_of("mail.send"), Some("os.mail".to_string()), "com.evil.mail sorts first but owns nothing");
    c.declare("com.evil.news", vec![decl("news.list", true, "host")]);
    assert_eq!(c.owner_of("news.list"), Some("os.news".to_string()), "whoever declares it, not yet loaded");
    c.declare("com.evil.terminal", vec![decl("terminal.run", true, "host")]);
    assert_eq!(c.owner_of("terminal.run"), Some("terminal".to_string()), "a native app owns its namespace");
    assert_eq!(c.owner_of("toolbox.search"), Some(super::TOOLBOX.to_string()));
    assert_eq!(c.owner_of("workflow.run"), Some(super::TOOLBOX.to_string()));
    assert_eq!(c.owner_of("search"), None, "a kernel tool has no owning app");
    // A grant resolved so reaches only the owner's tool.
    c.grant("com.example.trip", &c.owner_of("mail.send").unwrap(), "mail.send");
    assert!(c.may_call("com.example.trip", "os.mail", "mail.send", false));
    assert!(!c.may_call("com.example.trip", "com.evil.mail", "mail.send", false));
}

#[test]
fn should_run_a_modules_tools_on_its_executor_when_it_also_holds_a_peer_link() {
    // #142: a module that opens Makepad's OctosPeer only to talk keeps its
    // tools on its executor; nothing reroutes them to the link.
    let (mut relay, exec) = relay_with("rinx", vec![decl("rinx.room.list", false, "host")]);
    let mut w = World::new(FixedDevMode::off());
    w.links.push("rinx".into());
    let (r, _) = reply("c1");
    relay.handle(Event::Call { call: call("c1", "rinx.room.list", "rinx"), reply: r }, &mut w);
    assert_eq!(exec.0.lock().unwrap().len(), 1, "the executor ran it");
    assert!(w.link_calls.is_empty(), "nothing went down the link");
}

#[test]
fn should_send_a_modules_tools_down_its_link_when_it_has_no_executor() {
    // A module that serves its tools over its peer link, like a process app.
    let mut relay = Relay::default();
    relay.catalog.declare("probe", vec![decl("probe.ping", false, "host")]);
    let mut w = World::new(FixedDevMode::off());
    w.links.push("probe".into());
    let (r, _) = reply("c1");
    relay.handle(Event::Call { call: call("c1", "probe.ping", "probe"), reply: r }, &mut w);
    assert_eq!(w.link_calls.len(), 1);
    assert_eq!(w.link_calls[0].0, "probe");
}

/// ADR 0004 §8: the owning app's sheet gets who is calling as data, not
/// only a label, so it can check its own grants against it (section 9).
#[test]
fn the_apps_sheet_gets_the_structured_caller() {
    use crate::ai_host::app_peers::host_tools::ConfirmCaller;
    let (mut relay, _) = relay_with("rinx", vec![decl("rinx.message.send", true, "app")]);
    relay.catalog.grant("calendar", "rinx", "rinx.message.send");
    let mut w = World::new(FixedDevMode::off());
    let sheet = Arc::new(SendSheet::default());
    w.router.register_app_confirm("rinx", Box::new(super::SheetBridge { app: "rinx".into(), sheet: sheet.clone() }));
    for (id, calling) in [("s1", "calendar"), ("s2", "rinx")] {
        let mut c = call(id, "rinx.message.send", calling);
        c.confirm_required = true;
        let (r, _) = reply(id);
        relay.handle(Event::Call { call: c, reply: r }, &mut w);
    }
    let shown = sheet.0.lock().unwrap().clone();
    assert_eq!(shown[0].caller, ConfirmCaller::AppAgent { app: "calendar".into() });
    assert_eq!(shown[1].caller, ConfirmCaller::OwnAgent { client: Some("mini.news".into()) });
}

/// A cancelled `confirm: app` call is withdrawn from the owning app's sheet
/// (the router and the app hear it), not left for the person to answer.
#[test]
fn a_cancelled_confirm_app_call_is_withdrawn_from_the_apps_sheet() {
    let (mut relay, exec) = relay_with("rinx", vec![decl("rinx.message.send", true, "app")]);
    let mut w = World::new(FixedDevMode::off());
    let mut c = call("w1", "rinx.message.send", "rinx");
    c.confirm_required = true;
    let (r, _) = reply("w1");
    relay.handle(Event::Call { call: c, reply: r }, &mut w);
    let id = RequestId(format!("{CONFIRM_PREFIX}w1"));
    assert!(w.router.is_pending(&id), "on the app's sheet");
    relay.handle(Event::Cancel { call_id: "w1".into(), reason: "interrupted".into() }, &mut w);
    assert!(!w.router.is_pending(&id), "withdrawn with the call");
    assert!(exec.0.lock().unwrap().is_empty());
}
