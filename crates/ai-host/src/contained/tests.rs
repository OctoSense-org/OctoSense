//! The `octos` service through App Hub's real dispatch path, with fake peers:
//! no kernel, no network. Each test names the spec scenario it proves
//! (`specs/card-runner-octos.spec.md`).

use super::*;
use octosense_app_peers::{Availability, Deployment, ModelInfo, SettingsEntry, OCTOS_SERVICES};
use octosense_appstore::services::{dispatch, register_host_service, take_replies_for};
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// One `octos` service is registered at a time (the registry is global).
static SERIAL: Mutex<()> = Mutex::new(());
static NEXT_HEAP: AtomicUsize = AtomicUsize::new(52_000);

const APP: &str = "com.example.trip";

struct NoSheets;
impl ServiceHost for NoSheets {
    fn open_sheet(&mut self, _body: String) {}
    fn close_sheet(&mut self) {}
}

/// What a fake context does with a turn.
#[derive(Clone)]
enum Turn {
    /// Complete with this value.
    Reply(Value),
    /// Raise an approval (`id`, `title`), then complete with this value.
    ApproveThenReply(&'static str, &'static str, Value),
    /// The host took the approval (the shell's router): the app hears only
    /// that, then the turn completes with this value.
    HostHasItThenReply(&'static str, Value),
}

struct FakeContext {
    ops: Mutex<Vec<ContextOp>>,
    open: AtomicBool,
    turn: Turn,
    /// Close after the first call (a revoked or expired context).
    close_after_call: bool,
}

impl OctosContext for FakeContext {
    fn call(&self, op: ContextOp, sink: EventSink) -> Result<(), String> {
        self.ops.lock().unwrap().push(op.clone());
        if self.close_after_call {
            self.open.store(false, Ordering::SeqCst);
        }
        match op {
            ContextOp::Approval { .. } => {}
            ContextOp::Turn { .. } | ContextOp::TurnFrom { .. } => match &self.turn {
                Turn::Reply(v) => sink(ContextEvent::Complete(Ok(v.clone()))),
                Turn::ApproveThenReply(id, title, v) => {
                    sink(ContextEvent::Data(json!({
                        "method": "approval/requested",
                        "params": {"approval_id": id, "title": title, "body": "ls"},
                    })));
                    sink(ContextEvent::Complete(Ok(v.clone())));
                }
                Turn::HostHasItThenReply(id, v) => {
                    sink(ContextEvent::Data(json!({
                        "method": octosense_app_peers::host_tools::HANDLED_BY_HOST,
                        "params": {"approval_id": id, "title": "Write notes.md", "body": ""},
                    })));
                    sink(ContextEvent::Complete(Ok(v.clone())));
                }
            },
            _ => sink(ContextEvent::Complete(Ok(json!({"open": true})))),
        }
        Ok(())
    }
    fn close(&self) {
        self.open.store(false, Ordering::SeqCst);
    }
    fn is_open(&self) -> bool {
        self.open.load(Ordering::SeqCst)
    }
}

struct FakeService {
    turn: Turn,
    close_after_call: bool,
    accounts: Mutex<Vec<Option<String>>>,
    specs: Mutex<Vec<ContextSpec>>,
    contexts: Mutex<Vec<Arc<FakeContext>>>,
    /// How many handles were the app's conversation (not a request context).
    conversations: AtomicUsize,
    released: AtomicBool,
    /// How many times the shell prepared it.
    prepared: AtomicUsize,
}

impl OctosAppService for FakeService {
    fn deployment(&self) -> Deployment {
        Deployment::Hosted
    }
    fn availability(&self) -> Availability {
        Availability::Ready
    }
    fn services(&self) -> BTreeSet<String> {
        OCTOS_SERVICES.iter().map(|s| s.to_string()).collect()
    }
    fn model(&self) -> Option<ModelInfo> {
        None
    }
    fn settings_entry(&self) -> SettingsEntry {
        SettingsEntry::Host
    }
    fn set_account(&self, account: Option<&str>) {
        self.accounts.lock().unwrap().push(account.map(str::to_owned));
    }
    fn open_context(&self, spec: ContextSpec) -> Result<Arc<dyn OctosContext>, String> {
        self.specs.lock().unwrap().push(spec);
        let context = Arc::new(FakeContext {
            ops: Mutex::default(),
            open: AtomicBool::new(true),
            turn: self.turn.clone(),
            close_after_call: self.close_after_call,
        });
        self.contexts.lock().unwrap().push(context.clone());
        Ok(context)
    }
    fn open_conversation(&self, spec: ContextSpec) -> Result<Arc<dyn OctosContext>, String> {
        self.conversations.fetch_add(1, Ordering::SeqCst);
        self.open_context(spec)
    }
    fn prepare(&self) -> Result<(), String> {
        self.prepared.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
        for c in self.contexts.lock().unwrap().iter() {
            c.open.store(false, Ordering::SeqCst);
        }
    }
    fn shutdown(&self) {}
}

/// Launches a [`FakeService`] per peer (or none), recording every launch.
struct Peers {
    give: bool,
    turn: Turn,
    close_after_call: bool,
    launched: Mutex<Vec<(String, Arc<FakeService>)>>,
    /// The services each launch was granted.
    granted: Mutex<Vec<(String, BTreeSet<String>)>>,
}

impl Peers {
    fn new(turn: Turn) -> Arc<Self> {
        Arc::new(Peers { give: true, turn, close_after_call: false, launched: Mutex::default(), granted: Mutex::default() })
    }
    fn ids(&self) -> Vec<String> {
        self.launched.lock().unwrap().iter().map(|(id, _)| id.clone()).collect()
    }
    fn service(&self, peer: &str) -> Arc<FakeService> {
        self.launched.lock().unwrap().iter().find(|(id, _)| id == peer).expect("launched").1.clone()
    }
    fn ops(&self, peer: &str) -> Vec<ContextOp> {
        self.service(peer).contexts.lock().unwrap().iter().flat_map(|c| c.ops.lock().unwrap().clone()).collect()
    }
}

impl PeerFactory for Peers {
    fn launch(&self, peer_id: &str, _app_id: &str, services: &BTreeSet<String>) -> Option<Arc<dyn OctosAppService>> {
        self.granted.lock().unwrap().push((peer_id.to_owned(), services.clone()));
        if !self.give {
            return None;
        }
        let service = Arc::new(FakeService {
            turn: self.turn.clone(),
            close_after_call: self.close_after_call,
            accounts: Mutex::default(),
            specs: Mutex::default(),
            contexts: Mutex::default(),
            conversations: AtomicUsize::new(0),
            released: AtomicBool::new(false),
            prepared: AtomicUsize::new(0),
        });
        self.launched.lock().unwrap().push((peer_id.to_owned(), service.clone()));
        Some(service)
    }
}

fn register(enabled: bool, peers: &Arc<Peers>) {
    register_host_service(Box::new(ContainedOctos::new(enabled, peers.clone())));
}

/// One call from `app` through App Hub's dispatch; its reply.
fn ask(app: &str, service: &str, args: Value, from_sheet: bool) -> Result<Value, String> {
    let heap = NEXT_HEAP.fetch_add(1, Ordering::SeqCst);
    let call = ServiceCall {
        app_id: app.to_owned(),
        service: service.to_owned(),
        args,
        from_sheet,
        may_prompt: true,
        host_dir: std::env::temp_dir().join("octosense-contained-tests"),
    };
    dispatch(call, heap, 1, &mut NoSheets);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some((_, _, result)) = take_replies_for(&[heap]).pop() {
            return result.map(|s| serde_json::from_str(&s).expect("a JSON reply"));
        }
        assert!(Instant::now() < deadline, "no reply to {service}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn serial() -> std::sync::MutexGuard<'static, ()> {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    reset_for_tests();
    guard
}

#[test]
fn contained_turn_reaches_app_peer_and_replies() {
    let _g = serial();
    let peers = Peers::new(Turn::Reply(json!({"turn_id": "t1", "text": "你好"})));
    register(true, &peers);
    let reply = ask(APP, "octos.turn.start", json!({"text": "hi"}), false).expect("a reply");
    assert_eq!(reply["text"], "你好");
    assert_eq!(peers.ids(), vec!["card.com.example.trip".to_string()]);
    assert_eq!(peers.ops("card.com.example.trip"), vec![ContextOp::TurnFrom { text: "hi".into(), trigger: TurnTrigger::Unknown }], "a turn that says nothing is unknown");
    let service = peers.service("card.com.example.trip");
    assert_eq!(service.accounts.lock().unwrap().clone(), vec![Some(ACCOUNT.to_string())]);
    assert_eq!(service.specs.lock().unwrap()[0].account, ACCOUNT);
    // ADR 0004 §6: the app's turns go to its conversation (the person's
    // lane, sharing history with the system agent's), not a plain request
    // context.
    assert_eq!(service.conversations.load(Ordering::SeqCst), 1);
}

#[test]
fn contained_turns_carry_what_started_them() {
    let _g = serial();
    let peers = Peers::new(Turn::Reply(json!({"turn_id": "t1", "text": "ok"})));
    register(true, &peers);
    ask(APP, "octos.turn.start", json!({"text": "hi", "trigger": "person"}), false).expect("a reply");
    ask(APP, "octos.turn.start", json!({"text": "new mail", "trigger": "incoming", "from": "bo@example.org"}), false).expect("a reply");
    ask(APP, "octos.turn.start", json!({"text": "tick", "trigger": "schedule"}), false).expect("a reply");
    ask(APP, "octos.turn.start", json!({"text": "hm", "trigger": "system_agent"}), false).expect("a reply");
    let triggers: Vec<TurnTrigger> = peers.ops("card.com.example.trip").into_iter().filter_map(|op| op.turn().map(|(_, t)| t)).collect();
    assert_eq!(
        triggers,
        vec![TurnTrigger::AppSaysPerson, TurnTrigger::Incoming { from: Some("bo@example.org".into()) }, TurnTrigger::App, TurnTrigger::Unknown],
        "an app never claims the system agent, and its word is not the person's (only a shell surface is)"
    );
}

#[test]
fn contained_each_app_gets_its_own_peer() {
    let _g = serial();
    let peers = Peers::new(Turn::Reply(json!({})));
    register(true, &peers);
    for app in ["com.example.a", "com.example.a", "com.example.b"] {
        ask(app, "octos.session.open", json!({}), false).expect("open");
    }
    assert_eq!(peers.ids(), vec!["card.com.example.a".to_string(), "card.com.example.b".to_string()]);
}

#[test]
fn contained_session_calls_map_to_context_ops() {
    let _g = serial();
    let peers = Peers::new(Turn::Reply(json!({})));
    register(true, &peers);
    for service in ["octos.session.open", "octos.session.history", "octos.turn.interrupt"] {
        ask(APP, service, json!({}), false).expect("a reply");
    }
    assert_eq!(
        peers.ops("card.com.example.trip"),
        vec![ContextOp::Open, ContextOp::History, ContextOp::Interrupt]
    );
}

#[test]
fn contained_rejects_unsupported_arguments() {
    let _g = serial();
    let peers = Peers::new(Turn::Reply(json!({})));
    register(true, &peers);
    let err = ask(APP, "octos.turn.start", json!({"text": "hi", "session_id": "x"}), false).unwrap_err();
    assert_eq!(err, UNSUPPORTED_ARGS);
    let err = ask(APP, "octos.session.open", json!({"profile": "_main"}), false).unwrap_err();
    assert_eq!(err, UNSUPPORTED_ARGS);
    assert!(peers.ids().is_empty(), "no peer for a refused call");
}

#[test]
fn contained_rejects_empty_or_oversized_text() {
    let _g = serial();
    let peers = Peers::new(Turn::Reply(json!({})));
    register(true, &peers);
    for text in ["   ".to_string(), "x".repeat(MAX_TEXT_BYTES + 1)] {
        let err = ask(APP, "octos.turn.start", json!({"text": text}), false).unwrap_err();
        assert_eq!(err, BAD_TEXT);
    }
    assert!(peers.ids().is_empty());
}

#[test]
fn contained_rejects_unknown_method() {
    let _g = serial();
    let peers = Peers::new(Turn::Reply(json!({})));
    register(true, &peers);
    let err = ask(APP, "octos.admin", json!({}), false).unwrap_err();
    assert_eq!(err, "Unknown Octos service octos.admin");
}

#[test]
fn contained_refuses_sheet_calls() {
    let _g = serial();
    let peers = Peers::new(Turn::Reply(json!({})));
    register(true, &peers);
    let err = ask(APP, "octos.turn.start", json!({"text": "hi"}), true).unwrap_err();
    assert_eq!(err, NO_SHEET);
    assert!(peers.ids().is_empty());
}

#[test]
fn contained_switch_off_refuses_without_peer() {
    let _g = serial();
    let peers = Peers::new(Turn::Reply(json!({})));
    register(false, &peers);
    let err = ask(APP, "octos.turn.start", json!({"text": "hi"}), false).unwrap_err();
    assert_eq!(err, TURNED_OFF);
    assert!(peers.ids().is_empty());
}

#[test]
fn contained_peer_ids_are_namespaced_and_bounded() {
    assert_eq!(peer_id("rinx").unwrap(), "card.rinx");
    assert_ne!(peer_id("rinx").unwrap(), "rinx");
    let long = "a".repeat(60);
    let err = peer_id(&long).unwrap_err();
    assert!(err.contains("cannot name an assistant peer"), "{err}");
    assert!(peer_id(&"a".repeat(64 - PEER_PREFIX.len())).is_ok());
}

#[test]
fn contained_unavailable_when_no_peer() {
    let _g = serial();
    let peers = Arc::new(Peers { give: false, turn: Turn::Reply(json!({})), close_after_call: false, launched: Mutex::default(), granted: Mutex::default() });
    register(true, &peers);
    let err = ask(APP, "octos.session.open", json!({}), false).unwrap_err();
    assert_eq!(err, UNAVAILABLE);
}

#[test]
fn contained_reopens_closed_context() {
    let _g = serial();
    let peers = Arc::new(Peers { give: true, turn: Turn::Reply(json!({})), close_after_call: true, launched: Mutex::default(), granted: Mutex::default() });
    register(true, &peers);
    ask(APP, "octos.session.open", json!({}), false).expect("first");
    ask(APP, "octos.session.open", json!({}), false).expect("second");
    let instances: Vec<String> =
        peers.service("card.com.example.trip").specs.lock().unwrap().iter().map(|s| s.instance.clone()).collect();
    assert_eq!(instances, vec!["card.com.example.trip-g1".to_string(), "card.com.example.trip-g2".to_string()]);
}

#[test]
fn contained_denies_tool_approvals() {
    let _g = serial();
    let peers = Peers::new(Turn::ApproveThenReply("a1", "Run shell", json!({"turn_id": "t1", "text": "done"})));
    register(true, &peers);
    let reply = ask(APP, "octos.turn.start", json!({"text": "list files"}), false).expect("a reply");
    assert_eq!(reply["text"], "done");
    assert_eq!(reply["denied_approvals"], json!(["Run shell"]));
    // The decline is sent off the delivering thread.
    let deadline = Instant::now() + Duration::from_secs(5);
    let declined = ContextOp::Approval { id: "a1".into(), approve: false };
    while !peers.ops("card.com.example.trip").contains(&declined) {
        assert!(Instant::now() < deadline, "the approval was not declined");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!peers
        .ops("card.com.example.trip")
        .iter()
        .any(|op| matches!(op, ContextOp::Approval { approve: true, .. })));
}

#[test]
fn contained_never_declines_an_approval_the_host_routes() {
    let _g = serial();
    let peers = Peers::new(Turn::HostHasItThenReply("a1", json!({"turn_id": "t1", "text": "done"})));
    register(true, &peers);
    let reply = ask(APP, "octos.turn.start", json!({"text": "save my notes"}), false).expect("a reply");
    assert_eq!(reply["text"], "done");
    assert!(reply.get("denied_approvals").is_none(), "{reply}");
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        !peers.ops("card.com.example.trip").iter().any(|op| matches!(op, ContextOp::Approval { .. })),
        "the shell's sheet answers it, never the contained service"
    );
}

#[test]
fn contained_rejects_oversized_reply() {
    let _g = serial();
    let peers = Peers::new(Turn::Reply(json!({"text": "x".repeat(MAX_REPLY_BYTES)})));
    register(true, &peers);
    let err = ask(APP, "octos.turn.start", json!({"text": "hi"}), false).unwrap_err();
    assert!(err.contains("MAX_REPLY_BYTES"), "{err}");
}

#[test]
fn policy_contained_apps_follow_consent() {
    use crate::{contained_gate_from, ContainedGate};
    // On behind first-use consent (ADR 0004 section 4); the variable is a
    // developer override either way.
    assert_eq!(contained_gate_from(None), ContainedGate::Consent);
    assert_eq!(contained_gate_from(Some("1")), ContainedGate::Everyone);
    assert_eq!(contained_gate_from(Some("0")), ContainedGate::Off);
    if std::env::var("OCTOSENSE_CONTAINED_APPS").is_err() {
        assert_eq!(crate::Policy::shipped().contained_gate(), ContainedGate::Consent);
        assert!(crate::Policy::shipped().contained_apps());
    }
    assert!(!crate::Policy::none().contained_apps());
}

/// The shell's manifest lookup, as a test sets it once for the whole crate:
/// every app declares everything but these two.
fn manifests(app: &str) -> Option<BTreeSet<String>> {
    match app {
        "com.example.reader" => Some(["octos.session.open", "octos.session.history", "not.octos"].iter().map(|s| s.to_string()).collect()),
        "com.example.unknown" => None,
        _ => Some(OCTOS_SERVICES.iter().map(|s| s.to_string()).collect()),
    }
}

#[test]
fn contained_apps_get_only_the_octos_services_their_manifest_declares() {
    let _g = serial();
    set_declared(manifests);
    let peers = Peers::new(Turn::Reply(json!({"turn_id": "t1", "text": "ok"})));
    register(true, &peers);
    ask("com.example.reader", "octos.session.open", json!({}), false).expect("declared");
    // The peer serves the shell's panel too; the app's own context holds
    // only what its manifest declares, never all four.
    assert_eq!(peers.granted.lock().unwrap().len(), 1);
    let specs = peers.service("card.com.example.reader").specs.lock().unwrap().clone();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].services, ["octos.session.history", "octos.session.open"].iter().map(|s| s.to_string()).collect::<BTreeSet<_>>(), "never all four");
    let err = ask("com.example.reader", "octos.turn.start", json!({"text": "hi"}), false).unwrap_err();
    assert_eq!(err, NOT_DECLARED);
    let err = ask("com.example.unknown", "octos.session.open", json!({}), false).unwrap_err();
    assert_eq!(err, NOT_DECLARED);
    assert_eq!(peers.ids(), vec!["card.com.example.reader".to_string()], "no peer for an app the shell does not know");
}

#[test]
fn turning_an_agent_off_releases_its_live_peer_at_once() {
    let _g = serial();
    let peers = Peers::new(Turn::Reply(json!({"turn_id": "t1", "text": "ok"})));
    register(true, &peers);
    ask(APP, "octos.turn.start", json!({"text": "hi"}), false).expect("a reply");
    let first = peers.service("card.com.example.trip");
    assert!(revoke(APP), "the peer was live");
    assert!(first.released.load(Ordering::SeqCst), "released now, not at the next launch");
    assert!(first.contexts.lock().unwrap().iter().all(|c| !c.open.load(Ordering::SeqCst)), "its contexts are closed");
    assert!(!revoke(APP), "once");
    // Allowed again later: a fresh peer, never the revoked one.
    ask(APP, "octos.turn.start", json!({"text": "again"}), false).expect("a reply");
    assert_eq!(peers.ids().len(), 2);
}

/// ADR 0004 §4, an agent for every app that declares one: the shell
/// prepares a consented app's peer without the app calling `octos` (News
/// ships tools.json and declares no `octos.*`); the app's own calls, the
/// shell's panel and the preparation share ONE peer; the panel's
/// conversation holds every service while the app's own context holds only
/// what it declares; revoking releases the peer and a later preparation
/// gets a fresh one.
#[test]
fn the_shell_prepares_a_consented_apps_peer_and_its_panel_shares_it() {
    let _g = serial();
    set_declared(manifests);
    let peers = Peers::new(Turn::Reply(json!({"turn_id": "t1", "text": "ok"})));
    register(true, &peers);
    assert_eq!(prepare("os.news"), Err(UNAVAILABLE.to_string()), "no factory: nothing to prepare with");
    set_factory(peers.clone());
    prepare("os.news").expect("prepared");
    assert!(is_live("os.news"));
    assert_eq!(peers.ids(), vec!["card.os.news".to_string()]);
    let news = peers.service("card.os.news");
    assert_eq!(news.prepared.load(Ordering::SeqCst), 1, "bound now, before any turn");
    assert_eq!(news.accounts.lock().unwrap().as_slice(), [Some(ACCOUNT.to_string())]);
    // Preparing again is the same peer.
    prepare("os.news").unwrap();
    assert_eq!(peers.ids().len(), 1);
    // The shell's panel: a conversation on the same peer, every service.
    let panel = conversation("os.news", "shell-ask-1").expect("the person's lane");
    assert!(panel.is_open());
    assert_eq!(news.conversations.load(Ordering::SeqCst), 1);
    assert_eq!(news.specs.lock().unwrap()[0].services.len(), OCTOS_SERVICES.len());
    // An app that also calls `octos` itself uses that peer, with only what
    // it declares.
    prepare("com.example.reader").unwrap();
    ask("com.example.reader", "octos.session.open", json!({}), false).expect("declared");
    assert_eq!(peers.ids(), vec!["card.os.news".to_string(), "card.com.example.reader".to_string()], "no second peer");
    let reader = peers.service("card.com.example.reader").specs.lock().unwrap().clone();
    assert_eq!(reader[0].services, ["octos.session.history", "octos.session.open"].iter().map(|s| s.to_string()).collect::<BTreeSet<_>>());
    // Turned off: released at once; allowed again, a fresh peer.
    assert!(revoke("os.news"));
    assert!(news.released.load(Ordering::SeqCst));
    assert!(!panel.is_open(), "the panel's conversation closed with it");
    assert!(!is_live("os.news"));
    prepare("os.news").unwrap();
    assert_eq!(peers.ids().iter().filter(|id| *id == "card.os.news").count(), 2);
}

/// ADR 0004 §11: a contained app that keeps accounts (Mail) gets a peer for
/// its real account, the host's answer, not the fixed `device`; the host
/// rebinds it when the account changes.
#[test]
fn should_bind_a_contained_peer_to_the_apps_account_when_it_keeps_accounts() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    reset_for_tests();
    fn first(app: &str) -> Option<String> {
        (app == "os.mail").then(|| "acct-1".to_owned()).or_else(|| Some(ACCOUNT.to_owned()))
    }
    fn second(app: &str) -> Option<String> {
        (app == "os.mail").then(|| "acct-2".to_owned()).or_else(|| Some(ACCOUNT.to_owned()))
    }
    let peers = Peers::new(Turn::Reply(json!({"text": "ok"})));
    set_factory(peers.clone());
    set_account_of(Some(first));
    prepare("os.mail").unwrap();
    prepare(APP).unwrap();
    let mail = peers.service("card.os.mail");
    assert_eq!(*mail.accounts.lock().unwrap(), [Some("acct-1".to_owned())]);
    assert_eq!(*peers.service(&format!("card.{APP}")).accounts.lock().unwrap(), [Some(ACCOUNT.to_owned())], "an app without accounts acts for the device");
    set_account_of(Some(second));
    assert!(account_changed("os.mail"));
    assert_eq!(mail.accounts.lock().unwrap().last().cloned().flatten().as_deref(), Some("acct-2"));
    set_account_of(None);
    reset_for_tests();
}
