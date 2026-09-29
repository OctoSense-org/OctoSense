//! The peer link without a kernel: framing, identity stamping, grants and
//! consent, the host obligations (once per call, nothing after cancel, an
//! acknowledgement before a sheet) and what a process's death does.

use super::link::*;
use super::wire::{self, Down, Outcome, Up};
use crate::ai_host::app_peers::{Availability, ContextEvent, ContextOp, ContextSpec, Deployment, EventSink, ModelInfo, OctosAppService, OctosContext, SettingsEntry};
use crate::approvals::{Caller, Decision, RequestContext, RequestId, Route, ToolSpec, Trigger};
use crate::native_apps::Confirm;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// ------------------------------------------------------------ doubles

#[derive(Default)]
struct Shared {
    /// Everything that happened, in order ("ack c1", "approval c1", …).
    order: Vec<String>,
    consent: bool,
    route: Option<Route>,
    decisions: Vec<(RequestId, Decision, String)>,
    confirmed: Vec<(RequestId, bool)>,
    services_made: usize,
    contexts: Vec<Arc<FakeContext>>,
    specs: Vec<ContextSpec>,
}

#[derive(Clone, Default)]
struct World(Arc<Mutex<Shared>>);

impl World {
    fn with<R>(&self, f: impl FnOnce(&mut Shared) -> R) -> R {
        f(&mut self.0.lock().unwrap())
    }
}

struct FakeHost(World);

impl PeerHost for FakeHost {
    fn granted(&self, app: &str) -> BTreeSet<String> {
        match app {
            "notes" => ["octos.session.open", "octos.turn.start", "octos.session.history"].iter().map(|s| s.to_string()).collect(),
            "mail" => ["octos.session.open"].iter().map(|s| s.to_string()).collect(),
            _ => BTreeSet::new(),
        }
    }
    fn keeps_accounts(&self, app: &str) -> bool {
        app == "mail"
    }
    fn consent(&mut self, _app: &str) -> bool {
        self.0.with(|s| s.consent)
    }
    fn service(&mut self, _app: &str, _services: &BTreeSet<String>) -> Option<Arc<dyn OctosAppService>> {
        self.0.with(|s| s.services_made += 1);
        Some(Arc::new(FakeService(self.0.clone())))
    }
    fn tool_rule(&self, _app: &str, tool: &str) -> Option<(Confirm, bool)> {
        match tool {
            "send" => Some((Confirm::App, true)),
            "run" => Some((Confirm::Host, false)),
            _ => None,
        }
    }
    fn request_approval(&mut self, _app: &str, _tool: ToolSpec, _args: Value, caller: Caller, context: RequestContext) -> Route {
        self.0.with(|s| {
            s.order.push(format!("approval {} {:?} {:?}", context.call_id, caller, context.account));
            s.route.clone().unwrap_or(Route::Sheet(1))
        })
    }
    fn take_decisions(&mut self) -> Vec<(RequestId, Decision, String)> {
        self.0.with(|s| std::mem::take(&mut s.decisions))
    }
    fn app_confirm_answered(&mut self, id: &RequestId, approved: bool, _reason: &str) {
        self.0.with(|s| s.confirmed.push((id.clone(), approved)));
    }
    fn link_opened(&mut self, app: &str) {
        self.0.with(|s| s.order.push(format!("opened {app}")));
    }
    fn link_closed(&mut self, app: &str) {
        self.0.with(|s| s.order.push(format!("closed {app}")));
    }
}

struct FakeService(World);

impl OctosAppService for FakeService {
    fn deployment(&self) -> Deployment {
        Deployment::Hosted
    }
    fn availability(&self) -> Availability {
        Availability::Ready
    }
    fn services(&self) -> BTreeSet<String> {
        BTreeSet::new()
    }
    fn model(&self) -> Option<ModelInfo> {
        None
    }
    fn settings_entry(&self) -> SettingsEntry {
        SettingsEntry::Host
    }
    fn set_account(&self, _account: Option<&str>) {}
    fn open_context(&self, spec: ContextSpec) -> Result<Arc<dyn OctosContext>, String> {
        let ctx = Arc::new(FakeContext { closed: AtomicBool::new(false), calls: AtomicUsize::new(0) });
        self.0.with(|s| {
            s.contexts.push(ctx.clone());
            s.specs.push(spec);
        });
        Ok(ctx)
    }
    fn release(&self) {}
    fn shutdown(&self) {}
}

struct FakeContext {
    closed: AtomicBool,
    calls: AtomicUsize,
}

impl OctosContext for FakeContext {
    fn call(&self, op: ContextOp, sink: EventSink) -> Result<(), String> {
        if self.closed.load(Ordering::SeqCst) {
            return Err("closed".into());
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        match op {
            ContextOp::Open => sink(ContextEvent::Complete(Ok(json!({"session_id": "s"})))),
            ContextOp::Turn { text } => {
                sink(ContextEvent::Data(json!({"method": "message/delta", "text": text})));
                sink(ContextEvent::Complete(Ok(json!({"text": "done"}))));
            }
            _ => sink(ContextEvent::Complete(Ok(Value::Null))),
        }
        Ok(())
    }
    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
    fn is_open(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }
}

type Frames = Arc<Mutex<Vec<String>>>;

fn out() -> (FrameOut, Frames) {
    let frames: Frames = Arc::default();
    let f = frames.clone();
    (Arc::new(move |json: String| f.lock().unwrap().push(json)), frames)
}

fn downs(frames: &Frames) -> Vec<Down> {
    std::mem::take(&mut *frames.lock().unwrap()).iter().map(|f| Down::parse(f).expect(f)).collect()
}

fn setup() -> (PeerLinks, World, RecordingToolRelay) {
    let world = World::default();
    world.with(|s| s.consent = true);
    let relay = RecordingToolRelay::default();
    (PeerLinks::new(Box::new(FakeHost(world.clone())), Box::new(relay.clone())), world, relay)
}

fn request(req_id: u64, method: &str, args: Value) -> String {
    json!({"octos_peer": {"up": "request", "req_id": req_id, "method": method, "args": args}}).to_string()
}

fn result(call_id: &str, outcome: Value) -> String {
    let mut inner = json!({"up": "tool_result", "call_id": call_id});
    for (k, v) in outcome.as_object().unwrap() {
        inner[k] = v.clone();
    }
    json!({ "octos_peer": inner }).to_string()
}

/// Open a context on `client` for `notes`; its handle.
fn open(links: &mut PeerLinks, client: u64, frames: &Frames, label: Option<&str>) -> String {
    assert!(links.on_frame(client, "notes", &request(1, "octos.session.open", json!({"client": label})), None));
    match downs(frames).pop() {
        Some(Down::Reply { req_id: 1, result: Ok(v) }) => v["context"].as_str().unwrap().to_string(),
        other => panic!("{other:?}"),
    }
}

fn call(id: &str, name: &str, risk: wire::Risk, context: Option<&str>) -> KernelToolCall {
    KernelToolCall {
        call_id: id.into(),
        name: name.into(),
        args: json!({"to": "#room", "text": "hi"}),
        risk,
        timeout_ms: 30_000,
        context_id: context.map(str::to_string),
        caller: Caller::OwnAgent { client: None },
        trigger: Trigger::Person,
        outcome_unknown: false,
        approved: false,
        confirm_required: false,
    }
}

// ------------------------------------------------------------ framing

/// The frames Makepad's client (`makepad_ai_services::peer`) writes, byte
/// for byte, parse here; and the shell's frames carry its field names.
#[test]
fn the_shell_reads_the_makepad_clients_frames() {
    let from_makepad = [
        r#"{"octos_peer":{"up":"request","req_id":7,"method":"octos.turn.start","args":{"context":"c","text":"hi"}}}"#,
        r#"{"octos_peer":{"up":"tool_result","call_id":"k1","ok":true,"data":{"n":3}}}"#,
        r#"{"octos_peer":{"up":"tool_result","call_id":"k1","ok":false,"error":"no"}}"#,
        r#"{"octos_peer":{"up":"tool_result","call_id":"k1","ok":false,"awaiting_confirmation":true}}"#,
    ];
    let parsed: Vec<Up> = from_makepad.iter().map(|f| Up::parse(f).expect(f)).collect();
    assert!(matches!(&parsed[0], Up::Request { req_id: 7, method, args } if method == "octos.turn.start" && args["text"] == "hi"));
    assert_eq!(parsed[1], Up::ToolResult { call_id: "k1".into(), outcome: Outcome::Ok(json!({"n": 3})) });
    assert_eq!(parsed[2], Up::ToolResult { call_id: "k1".into(), outcome: Outcome::Error("no".into()) });
    assert_eq!(parsed[3], Up::ToolResult { call_id: "k1".into(), outcome: Outcome::AwaitingConfirmation });
    // Not the link's: the AI bus's and the WM's envelopes, unknown methods,
    // non-object args, a second top-level key, an oversized frame.
    assert!(!wire::is_peer_frame(r#"{"wm_ai":{"from":null,"msg":"Unregister"}}"#));
    assert_eq!(Up::parse(&request(1, "octos.admin", json!({}))), None);
    assert_eq!(Up::parse(r#"{"octos_peer":{"up":"request","req_id":1,"method":"octos.turn.start","args":[1]}}"#), None);
    assert_eq!(Up::parse(r#"{"octos_peer":{"up":"request","req_id":1,"method":"octos.turn.start"},"from":"rinx"}"#), None);
    let big = request(1, "octos.turn.start", json!({"text": "a".repeat(wire::MAX_FRAME_BYTES)}));
    assert_eq!(Up::parse(&big), None);
    let call = Down::ToolCall(wire::ToolCallDown {
        call_id: "k2".into(),
        name: "send".into(),
        args: json!({}),
        risk: wire::Risk::Destructive,
        confirm_required: true,
        timeout_ms: 1,
        account: Some("a".into()),
        context_id: None,
        client: None,
        caller: "system_agent".into(),
    })
    .to_json();
    for key in ["\"down\":\"tool_call\"", "\"confirm_required\":true", "\"caller\":\"system_agent\"", "\"context_id\":null"] {
        assert!(call.contains(key), "{call}");
    }
}

#[test]
fn bus_frames_are_not_consumed_and_malformed_peer_frames_are() {
    let (mut links, _, _) = setup();
    let (o, frames) = out();
    links.connected(1, "notes", o);
    assert!(!links.on_frame(1, "notes", r#"{"wm_ai":{"from":null,"msg":"Unregister"}}"#, None));
    assert!(links.on_frame(1, "notes", r#"{"octos_peer":{"up":"nonsense"}}"#, None));
    assert!(downs(&frames).is_empty());
}

// --------------------------------------------- grants, consent, identity

#[test]
fn only_granted_and_consented_apps_get_a_link_and_only_granted_methods() {
    let (mut links, world, _) = setup();
    let (o, frames) = out();
    assert!(!links.connected(1, "sheets", o.clone()), "no agent.octos grant: no link");
    assert!(links.on_frame(1, "sheets", &request(1, "octos.session.open", json!({})), Some(o.clone())));
    assert!(matches!(downs(&frames).as_slice(), [Down::Reply { result: Err(e), .. }] if e.starts_with("no_agent")));
    assert!(links.connected(2, "notes", o.clone()));
    world.with(|s| s.consent = false);
    links.on_frame(2, "notes", &request(2, "octos.session.open", json!({})), None);
    assert!(matches!(downs(&frames).as_slice(), [Down::Reply { result: Err(e), .. }] if e.starts_with("consent_pending")));
    assert_eq!(world.with(|s| s.services_made), 0, "no peer before consent");
    world.with(|s| s.consent = true);
    let ctx = open(&mut links, 2, &frames, None);
    links.on_frame(2, "notes", &request(3, "octos.turn.interrupt", json!({"context": ctx})), None);
    assert!(matches!(downs(&frames).as_slice(), [Down::Reply { req_id: 3, result: Err(e) }] if e == "not_granted: octos.turn.interrupt"));
}

#[test]
fn identity_is_the_sockets_and_a_process_uses_only_its_own_contexts() {
    let (mut links, world, _) = setup();
    let (a, frames_a) = out();
    let (b, frames_b) = out();
    links.connected(1, "notes", a);
    links.connected(2, "notes", b);
    // A frame that claims another app changes nothing: the socket's app runs.
    let claimed = r#"{"octos_peer":{"up":"request","req_id":1,"method":"octos.session.open","args":{"client":"mini","app":"mail","account":"someone"}}}"#;
    links.on_frame(1, "notes", claimed, None);
    let ctx = match downs(&frames_a).pop() {
        Some(Down::Reply { result: Ok(v), .. }) => v["context"].as_str().unwrap().to_string(),
        other => panic!("{other:?}"),
    };
    let spec = world.with(|s| s.specs[0].clone());
    assert_eq!(spec.account, DEVICE, "notes keeps no accounts: the frame's account is ignored");
    assert_eq!(spec.instance, ctx);
    let owner = links.context_owner("notes", &format!("ab12cd34-{ctx}")).expect("the kernel id resolves");
    assert_eq!((owner.client_id, owner.client.as_deref()), (1, Some("mini")));
    assert!(links.context_owner("mail", &ctx).is_none(), "another app's records");
    // Process 2 of the same app cannot drive process 1's context.
    links.on_frame(2, "notes", &request(5, "octos.turn.start", json!({"context": ctx, "text": "x"})), None);
    assert!(matches!(downs(&frames_b).as_slice(), [Down::Reply { req_id: 5, result: Err(e) }] if e == "unknown_context"));
    // Process 1 can, and its turn streams then replies.
    links.on_frame(1, "notes", &request(6, "octos.turn.start", json!({"context": ctx, "text": "x"})), None);
    let got = downs(&frames_a);
    assert!(matches!(got.as_slice(), [Down::Event { req_id: 6, .. }, Down::Reply { req_id: 6, result: Ok(_) }]), "{got:?}");
}

#[test]
fn an_app_that_keeps_accounts_names_one() {
    let (mut links, world, _) = setup();
    let (o, frames) = out();
    links.connected(1, "mail", o);
    links.on_frame(1, "mail", &request(1, "octos.session.open", json!({})), None);
    assert!(matches!(downs(&frames).as_slice(), [Down::Reply { result: Err(e), .. }] if e.contains("account")));
    links.on_frame(1, "mail", &request(2, "octos.session.open", json!({"account": "me@example.org"})), None);
    assert!(matches!(downs(&frames).as_slice(), [Down::Reply { result: Ok(_), .. }]));
    assert_eq!(world.with(|s| s.specs[0].account.clone()), "me@example.org");
}

// ------------------------------------------------------------ tool calls

#[test]
fn a_tool_call_is_stamped_from_the_shells_records() {
    let (mut links, _, relay) = setup();
    let (o, frames) = out();
    links.connected(1, "notes", o);
    let ctx = open(&mut links, 1, &frames, Some("mini"));
    links.tool_call("notes", call("c1", "lookup", wire::Risk::Read, Some(&format!("n0nce-{ctx}"))), 0.0).unwrap();
    match downs(&frames).as_slice() {
        [Down::ToolCall(c)] => {
            assert_eq!((c.account.as_deref(), c.client.as_deref(), c.caller.as_str()), (Some(DEVICE), Some("mini"), "own_agent"));
            assert!(!c.confirm_required);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(links.tool_call("notes", call("c1", "lookup", wire::Risk::Read, None), 0.0), Err(Refused::Duplicate), "once per call");
    assert_eq!(links.tool_call("notes", call("c2", "lookup", wire::Risk::Read, Some("n0nce-pl9-9")), 0.0), Err(Refused::UnknownContext));
    assert_eq!(links.tool_call("mail", call("c3", "lookup", wire::Risk::Read, None), 0.0), Err(Refused::NotConnected));
    // One result reaches the relay; a second is ignored.
    links.on_frame(1, "notes", &result("c1", json!({"ok": true, "data": {"hits": 2}})), None);
    links.on_frame(1, "notes", &result("c1", json!({"ok": true, "data": {"hits": 3}})), None);
    assert_eq!(relay.take(), vec![("notes".into(), "c1".into(), Some(ToolCallResult::Ok(json!({"hits": 2}))))]);
}

#[test]
fn nothing_after_cancel() {
    let (mut links, _, relay) = setup();
    let (o, frames) = out();
    links.connected(1, "notes", o);
    links.tool_call("notes", call("c1", "lookup", wire::Risk::Act, None), 0.0).unwrap();
    links.tool_cancel("notes", "c1");
    assert!(matches!(downs(&frames).as_slice(), [Down::ToolCall(_), Down::ToolCancel { call_id }] if call_id == "c1"));
    links.on_frame(1, "notes", &result("c1", json!({"ok": true, "data": null})), None);
    assert!(relay.take().is_empty(), "a result after cancel never reaches the kernel");
}

#[test]
fn a_host_confirmed_call_is_acknowledged_before_its_sheet_and_waits_for_the_person() {
    let (mut links, world, relay) = setup();
    let (o, frames) = out();
    links.connected(1, "notes", o);
    world.with(|s| s.route = Some(Route::Sheet(4)));
    links.tool_call("notes", call("c1", "run", wire::Risk::Destructive, None), 0.0).unwrap();
    assert!(downs(&frames).is_empty(), "nothing reaches the app while the sheet is up");
    let events = relay.take();
    assert_eq!(events, vec![("notes".into(), "c1".into(), None)], "acknowledged first");
    let order = world.with(|s| s.order.clone());
    assert!(order.iter().any(|l| l.starts_with("approval peerlink:notes:c1")), "{order:?}");
    // The person approves: the call goes to the app, once.
    world.with(|s| s.decisions.push((RequestId("peerlink:notes:c1".into()), Decision::ApproveOnce, "sheet".into())));
    links.tick(1.0);
    links.tick(2.0);
    assert!(matches!(downs(&frames).as_slice(), [Down::ToolCall(c)] if c.call_id == "c1" && !c.confirm_required));
    // A denial of another call finishes it with the reason.
    links.tool_call("notes", call("c2", "run", wire::Risk::Destructive, None), 0.0).unwrap();
    world.with(|s| s.decisions.push((RequestId("peerlink:notes:c2".into()), Decision::Deny, "denied on the sheet".into())));
    links.tick(1.0);
    let events = relay.take();
    assert!(events.contains(&("notes".into(), "c2".into(), Some(ToolCallResult::Error("declined: denied on the sheet".into())))), "{events:?}");
}

#[test]
fn an_app_confirmed_call_needs_the_apps_acknowledgement_before_its_result() {
    let (mut links, world, relay) = setup();
    let (o, frames) = out();
    links.connected(1, "notes", o);
    world.with(|s| s.route = Some(Route::HandedToApp));
    links.tool_call("notes", call("c1", "send", wire::Risk::Destructive, None), 0.0).unwrap();
    assert!(matches!(downs(&frames).as_slice(), [Down::ToolCall(c)] if c.confirm_required));
    // Answering without the acknowledgement is refused and the call stops.
    links.on_frame(1, "notes", &result("c1", json!({"ok": true, "data": null})), None);
    let events = relay.take();
    assert!(matches!(events.as_slice(), [_, (_, _, Some(ToolCallResult::Error(e)))] if e.contains("without acknowledging")), "{events:?}");
    assert!(matches!(downs(&frames).as_slice(), [Down::ToolCancel { .. }]));
    assert_eq!(world.with(|s| s.confirmed.clone()), vec![(RequestId("peerlink:notes:c1".into()), false)]);
    // The right order: acknowledge, show the sheet, answer.
    links.tool_call("notes", call("c2", "send", wire::Risk::Destructive, None), 0.0).unwrap();
    links.on_frame(1, "notes", &result("c2", json!({"ok": false, "awaiting_confirmation": true})), None);
    links.on_frame(1, "notes", &result("c2", json!({"ok": true, "data": {"sent": true}})), None);
    let events = relay.take();
    assert_eq!(events.last(), Some(&("notes".into(), "c2".into(), Some(ToolCallResult::Ok(json!({"sent": true}))))));
    assert_eq!(world.with(|s| s.confirmed.last().cloned()), Some((RequestId("peerlink:notes:c2".into()), true)));
}

#[test]
fn a_call_past_its_deadline_times_out_and_the_app_is_told() {
    let (mut links, _, relay) = setup();
    let (o, frames) = out();
    links.connected(1, "notes", o);
    links.tool_call("notes", call("c1", "lookup", wire::Risk::Read, None), 10.0).unwrap();
    links.tick(39.0);
    assert!(relay.take().is_empty());
    links.tick(40.5);
    assert_eq!(relay.take(), vec![("notes".into(), "c1".into(), Some(ToolCallResult::Error("timed_out".into())))]);
    assert!(matches!(downs(&frames).as_slice(), [Down::ToolCall(_), Down::ToolCancel { .. }]));
}

// ------------------------------------------------------------ death

#[test]
fn a_dead_process_fails_its_calls_closes_its_contexts_and_keeps_the_peer() {
    let (mut links, world, relay) = setup();
    let (o, frames) = out();
    links.connected(1, "notes", o.clone());
    let _ctx = open(&mut links, 1, &frames, None);
    links.tool_call("notes", call("read", "lookup", wire::Risk::Read, None), 0.0).unwrap();
    links.tool_call("notes", call("act", "save", wire::Risk::Act, None), 0.0).unwrap();
    links.process_gone(1);
    let mut events = relay.take();
    events.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(
        events,
        vec![
            ("notes".into(), "act".into(), Some(ToolCallResult::OutcomeUnknown)),
            ("notes".into(), "read".into(), Some(ToolCallResult::Error("app_exited".into()))),
        ]
    );
    assert!(world.with(|s| s.contexts.iter().all(|c| !c.is_open())), "its request contexts are closed");
    assert_eq!(links.open_contexts("notes"), 0);
    assert!(!links.has_link("notes"));
    assert!(links.keeps_peer("notes"), "the peer stays");
    assert!(world.with(|s| s.order.contains(&"closed notes".to_string())));
    // A result from the dead process's calls can no longer arrive; a
    // restarted process gets the same peer, not a new one.
    assert_eq!(links.tool_call("notes", call("later", "lookup", wire::Risk::Read, None), 0.0), Err(Refused::NotConnected));
    links.connected(3, "notes", o);
    let _ = open(&mut links, 3, &frames, None);
    assert_eq!(world.with(|s| s.services_made), 1, "the same peer after a restart");
}

#[test]
fn signing_out_closes_the_accounts_contexts_and_tells_the_app() {
    let (mut links, _, _) = setup();
    let (o, frames) = out();
    links.connected(1, "notes", o);
    let ctx = open(&mut links, 1, &frames, None);
    links.close_account("notes", DEVICE, "signed_out");
    assert!(matches!(downs(&frames).as_slice(), [Down::ContextClosed { context, reason }] if *context == ctx && reason == "signed_out"));
    assert_eq!(links.tool_call("notes", call("c1", "lookup", wire::Risk::Read, Some(&ctx)), 0.0), Err(Refused::UnknownContext));
}


#[test]
fn a_call_the_kernel_already_approved_is_not_asked_again_and_confirm_app_goes_to_the_apps_sheet() {
    let (mut links, world, relay) = setup();
    let (o, frames) = out();
    links.connected(1, "notes", o);
    // UPCR-2026-035: a gated confirm: host call reaches the host only after
    // the kernel's approval; the link sends it at once.
    let mut approved = call("c1", "run", wire::Risk::Destructive, None);
    approved.approved = true;
    links.tool_call("notes", approved, 0.0).unwrap();
    assert!(matches!(downs(&frames).as_slice(), [Down::ToolCall(c)] if c.call_id == "c1" && !c.confirm_required));
    assert!(!world.with(|s| s.order.iter().any(|l| l.starts_with("approval"))), "nobody is asked twice");
    assert!(relay.take().is_empty(), "no acknowledgement: no sheet follows");
    // `confirm_required` (confirm: app): acknowledged, then the app's own sheet.
    world.with(|s| s.route = Some(Route::HandedToApp));
    let mut confirm = call("c2", "add", wire::Risk::Act, None);
    confirm.confirm_required = true;
    links.tool_call("notes", confirm, 0.0).unwrap();
    assert_eq!(relay.take(), vec![("notes".into(), "c2".into(), None)], "acknowledged before the sheet");
    assert!(world.with(|s| s.order.iter().any(|l| l.starts_with("approval peerlink:notes:c2"))));
    assert!(matches!(downs(&frames).as_slice(), [Down::ToolCall(c)] if c.call_id == "c2" && c.confirm_required));
}
