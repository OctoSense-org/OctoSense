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
            ContextOp::Turn { .. } => match &self.turn {
                Turn::Reply(v) => sink(ContextEvent::Complete(Ok(v.clone()))),
                Turn::ApproveThenReply(id, title, v) => {
                    sink(ContextEvent::Data(json!({
                        "method": "approval/requested",
                        "params": {"approval_id": id, "title": title, "body": "ls"},
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
    fn release(&self) {}
    fn shutdown(&self) {}
}

/// Launches a [`FakeService`] per peer (or none), recording every launch.
struct Peers {
    give: bool,
    turn: Turn,
    close_after_call: bool,
    launched: Mutex<Vec<(String, Arc<FakeService>)>>,
}

impl Peers {
    fn new(turn: Turn) -> Arc<Self> {
        Arc::new(Peers { give: true, turn, close_after_call: false, launched: Mutex::default() })
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
    fn launch(&self, peer_id: &str, _app_id: &str) -> Option<Arc<dyn OctosAppService>> {
        if !self.give {
            return None;
        }
        let service = Arc::new(FakeService {
            turn: self.turn.clone(),
            close_after_call: self.close_after_call,
            accounts: Mutex::default(),
            specs: Mutex::default(),
            contexts: Mutex::default(),
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
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn contained_turn_reaches_app_peer_and_replies() {
    let _g = serial();
    let peers = Peers::new(Turn::Reply(json!({"turn_id": "t1", "text": "你好"})));
    register(true, &peers);
    let reply = ask(APP, "octos.turn.start", json!({"text": "hi"}), false).expect("a reply");
    assert_eq!(reply["text"], "你好");
    assert_eq!(peers.ids(), vec!["card.com.example.trip".to_string()]);
    assert_eq!(peers.ops("card.com.example.trip"), vec![ContextOp::Turn { text: "hi".into() }]);
    let service = peers.service("card.com.example.trip");
    assert_eq!(service.accounts.lock().unwrap().clone(), vec![Some(ACCOUNT.to_string())]);
    assert_eq!(service.specs.lock().unwrap()[0].account, ACCOUNT);
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
    let peers = Arc::new(Peers { give: false, turn: Turn::Reply(json!({})), close_after_call: false, launched: Mutex::default() });
    register(true, &peers);
    let err = ask(APP, "octos.session.open", json!({}), false).unwrap_err();
    assert_eq!(err, UNAVAILABLE);
}

#[test]
fn contained_reopens_closed_context() {
    let _g = serial();
    let peers = Arc::new(Peers { give: true, turn: Turn::Reply(json!({})), close_after_call: true, launched: Mutex::default() });
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
fn contained_rejects_oversized_reply() {
    let _g = serial();
    let peers = Peers::new(Turn::Reply(json!({"text": "x".repeat(MAX_REPLY_BYTES)})));
    register(true, &peers);
    let err = ask(APP, "octos.turn.start", json!({"text": "hi"}), false).unwrap_err();
    assert!(err.contains("MAX_REPLY_BYTES"), "{err}");
}

#[test]
fn policy_contained_apps_defaults() {
    assert!(crate::Policy::shipped().contained_apps());
    assert!(!crate::Policy::none().contained_apps());
}
