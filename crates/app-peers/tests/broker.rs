//! The broker against a scripted kernel speaking the UPCR-2026-034 subset.
#![cfg(feature = "broker")]

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use octosense_app_peers::broker::{BoxFuture, Broker, BrokerConfig, Connector, Link, ToolHostHandle};
use octosense_app_peers::host_tools::{AgentQuestion, ApprovalAnswer, CallOrigin, HostToolApproval, HostToolCall, PeerInput, QuestionAnswer, QuestionReply, ToolHost, ToolOutcome, ToolReply};
use octosense_app_peers::*;
use serde_json::{json, Value};
use tokio::sync::mpsc;

/// What the scripted kernel saw and how it behaves.
#[derive(Default)]
struct Script {
    calls: Vec<(String, Value)>,
    /// The connection (0-based, in connect order) each call came on.
    conns: Vec<usize>,
    /// peer/prepare ignores the host binding (a pre-UPCR kernel).
    legacy: bool,
    /// turn/start never completes on its own.
    hold_turns: bool,
    /// peer/tools/register is refused.
    refuse_register: bool,
    connects: usize,
    /// Each connection's kernel-to-broker half (`None`: closed).
    out: Vec<Option<mpsc::UnboundedSender<String>>>,
}

struct FakeConnector(Arc<Mutex<Script>>);

struct FakeLink {
    to_kernel: mpsc::UnboundedSender<String>,
    from_kernel: mpsc::UnboundedReceiver<String>,
}

impl Link for FakeLink {
    fn send(&mut self, frame: String) -> Result<(), String> {
        self.to_kernel.send(frame).map_err(|_| "gone".to_owned())
    }
    fn recv(&mut self) -> BoxFuture<'_, Result<String, String>> {
        Box::pin(async move {
            self.from_kernel
                .recv()
                .await
                .ok_or_else(|| "closed".to_owned())
        })
    }
}

/// Send a kernel frame on connection `conn`.
fn emit(script: &Arc<Mutex<Script>>, conn: usize, frame: String) {
    let out = script.lock().unwrap().out.get(conn).cloned().flatten();
    if let Some(out) = out {
        let _ = out.send(frame);
    }
}

/// A notification on the newest connection.
fn notify(script: &Arc<Mutex<Script>>, method: &str, params: Value) {
    let conn = script.lock().unwrap().out.len() - 1;
    emit(script, conn, json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string());
}

/// Close the newest connection from the kernel's side.
fn kill_link(script: &Arc<Mutex<Script>>) {
    let mut s = script.lock().unwrap();
    if let Some(last) = s.out.last_mut() {
        *last = None;
    }
}

impl Connector for FakeConnector {
    fn available(&self) -> Result<(), String> {
        Ok(())
    }
    fn connect(&self) -> BoxFuture<'static, Result<Box<dyn Link>, String>> {
        let script = self.0.clone();
        Box::pin(async move {
            let (to_kernel, mut kernel_in) = mpsc::unbounded_channel::<String>();
            let (kernel_out, from_kernel) = mpsc::unbounded_channel::<String>();
            let conn = {
                let mut s = script.lock().unwrap();
                s.connects += 1;
                s.out.push(Some(kernel_out));
                s.out.len() - 1
            };
            tokio::spawn(async move {
                while let Some(frame) = kernel_in.recv().await {
                    let frame: Value = serde_json::from_str(&frame).unwrap();
                    let method = frame["method"].as_str().unwrap().to_owned();
                    let params = frame["params"].clone();
                    let id = frame["id"].clone();
                    let (legacy, hold, refuse_register) = {
                        let mut s = script.lock().unwrap();
                        s.calls.push((method.clone(), params.clone()));
                        s.conns.push(conn);
                        (s.legacy, s.hold_turns, s.refuse_register)
                    };
                    let send = |frame: String| emit(&script, conn, frame);
                    let reply = |result: Value| {
                        json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
                    };
                    let refuse = |kind: &str| {
                        json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32001, "message": "refused", "data": {"kind": kind}}}).to_string()
                    };
                    match method.as_str() {
                        "peer/prepare" => {
                            let name = params["names"][0]
                                .as_str()
                                .unwrap()
                                .to_lowercase()
                                .replace(' ', "-");
                            let cwd = params["cwd"].as_str().unwrap_or("/kernel/ws").to_owned();
                            let mut result = json!({"slug": name, "cwd": cwd, "model": {"lane": "primary"}});
                            if !legacy {
                                result["memory_namespace"] = params["memory_namespace"].clone();
                                result["resumed"] = json!(params.get("host_token").is_some());
                                if params.get("host_token").is_none() {
                                    result["host_token"] = json!("fixture-host-token");
                                }
                            }
                            send(reply(result));
                        }
                        "peer/tools/register" if refuse_register => send(refuse("peer_tools_invalid")),
                        "peer/tools/register" | "peer/context/open" | "peer/context/close" | "peer/tool/result"
                            if params["host_token"] != "fixture-host-token" =>
                        {
                            send(refuse("peer_host_token_mismatch"));
                        }
                        "peer/tools/register" => {
                            let tools = params["tools"].clone();
                            send(reply(json!({"slug": params["peer"], "version": 1, "tools": tools, "generic_tools": null, "applies": "next_turn"})));
                        }
                        "peer/context/open" => {
                            let session = format!(
                                "{}#peerctx-{}.{}",
                                params["session_id"]
                                    .as_str()
                                    .unwrap()
                                    .split('#')
                                    .next()
                                    .unwrap(),
                                params["peer"].as_str().unwrap(),
                                params["context_id"].as_str().unwrap()
                            );
                            send(reply(json!({"session_id": session, "created": true})));
                        }
                        "turn/start" => {
                            let session = params["session_id"].clone();
                            let turn = params["turn_id"].clone();
                            send(reply(json!({"accepted": true})));
                            let note = |method: &str, extra: Value| {
                                let mut p = json!({"session_id": session, "turn_id": turn});
                                for (k, v) in extra.as_object().unwrap() {
                                    p[k] = v.clone();
                                }
                                json!({"jsonrpc": "2.0", "method": method, "params": p}).to_string()
                            };
                            send(note("turn/started", json!({})));
                            send(note("message/delta", json!({"text": "Hello "})));
                            send(note("message/delta", json!({"text": "there"})));
                            if !hold {
                                send(note("turn/completed", json!({})));
                            }
                        }
                        "session/hydrate" => send(reply(json!({"messages": []}))),
                        _ => send(reply(json!({}))),
                    }
                }
            });
            Ok(Box::new(FakeLink {
                to_kernel,
                from_kernel,
            }) as Box<dyn Link>)
        })
    }
    fn owns_runtime(&self) -> bool {
        false
    }
    fn shutdown(&self) {}
}

fn new_broker(services: &[&str]) -> (Broker, Arc<Mutex<Script>>) {
    new_broker_with(services, None, None)
}

fn new_broker_with(services: &[&str], host: Option<Arc<RecordingHost>>, state_dir: Option<std::path::PathBuf>) -> (Broker, Arc<Mutex<Script>>) {
    let script = Arc::new(Mutex::new(Script::default()));
    let mut cfg = BrokerConfig::new(
        Deployment::Hosted,
        "_main",
        "_main:api:octosense#system",
        "rinx",
        "Rinx",
        services.iter().map(|s| s.to_string()).collect(),
    );
    cfg.tool_host = host.map(|h| ToolHostHandle(h as Arc<dyn ToolHost>));
    cfg.state_dir = state_dir;
    (
        Broker::new(cfg, Arc::new(FakeConnector(script.clone()))),
        script,
    )
}

fn spec(account: &str, instance: &str, services: &[&str]) -> ContextSpec {
    ContextSpec {
        account: account.into(),
        instance: instance.into(),
        services: services
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<_>>(),
    }
}

fn collect() -> (EventSink, std::sync::mpsc::Receiver<ContextEvent>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = Mutex::new(tx);
    (
        Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }),
        rx,
    )
}

fn complete(rx: &std::sync::mpsc::Receiver<ContextEvent>) -> Result<Value, String> {
    loop {
        match rx
            .recv_timeout(Duration::from_secs(5))
            .expect("a completion")
        {
            ContextEvent::Complete(r) => return r,
            ContextEvent::Data(_) => continue,
        }
    }
}

fn methods(script: &Arc<Mutex<Script>>) -> Vec<String> {
    script
        .lock()
        .unwrap()
        .calls
        .iter()
        .map(|(m, _)| m.clone())
        .collect()
}

const ALL: [&str; 4] = OCTOS_SERVICES;

#[test]
fn a_turn_runs_in_a_bound_request_context_of_the_system_owned_peer() {
    let (broker, script) = new_broker(&ALL);
    broker.set_account(Some("@alice:example.org"));
    let ctx = broker
        .open_context(spec("@alice:example.org", "dev.example.app#1", &ALL))
        .unwrap();
    let (sink, rx) = collect();
    ctx.call(ContextOp::Turn { text: "hi".into() }, sink)
        .unwrap();
    let mut streamed = Vec::new();
    let result = loop {
        match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
            ContextEvent::Data(d) => streamed.push(d),
            ContextEvent::Complete(r) => break r,
        }
    };
    assert_eq!(result.unwrap()["text"], "Hello there");
    assert!(streamed.iter().any(|d| d["text"] == "Hello there"));
    let calls = script.lock().unwrap().calls.clone();
    let prepare = &calls.iter().find(|(m, _)| m == "peer/prepare").unwrap().1;
    assert_eq!(
        prepare["session_id"], "_main:api:octosense#system",
        "the system agent owns the peer"
    );
    assert_eq!(prepare["resume"], true);
    assert!(prepare["memory_namespace"]
        .as_str()
        .unwrap()
        .starts_with("app/rinx/acct-"));
    assert!(
        prepare.get("cwd").is_none(),
        "the kernel provisions the workspace"
    );
    let turn = &calls.iter().find(|(m, _)| m == "turn/start").unwrap().1;
    assert!(turn["session_id"].as_str().unwrap().contains("#peerctx-"));
    assert_eq!(broker.availability(), Availability::Ready);
}

#[test]
fn history_access_does_not_allow_a_turn_and_ungranted_apps_get_no_context() {
    let (broker, script) = new_broker(&["octos.session.open", "octos.session.history"]);
    broker.set_account(Some("@a:x"));
    let ctx = broker.open_context(spec("@a:x", "app#1", &ALL)).unwrap();
    let (sink, _rx) = collect();
    let err = ctx
        .call(ContextOp::Turn { text: "hi".into() }, sink.clone())
        .unwrap_err();
    assert!(err.contains("not granted octos.turn.start"), "{err}");
    assert!(ctx.call(ContextOp::History, sink).is_ok());
    std::thread::sleep(Duration::from_millis(300));
    assert!(!methods(&script).contains(&"turn/start".to_owned()));

    let (none, script) = new_broker(&[]);
    none.set_account(Some("@a:x"));
    assert!(none.open_context(spec("@a:x", "app#1", &ALL)).is_err());
    assert!(matches!(none.availability(), Availability::Unavailable(_)));
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        script.lock().unwrap().connects,
        0,
        "no peer, no kernel for an app without access"
    );
}

#[test]
fn an_account_change_revokes_contexts_and_drops_their_late_replies() {
    let (broker, script) = new_broker(&ALL);
    script.lock().unwrap().hold_turns = true;
    broker.set_account(Some("@alice:x"));
    let ctx = broker
        .open_context(spec("@alice:x", "app#1", &ALL))
        .unwrap();
    let (sink, rx) = collect();
    ctx.call(ContextOp::Turn { text: "hi".into() }, sink)
        .unwrap();
    // Wait until the turn is running on the kernel.
    for _ in 0..50 {
        if methods(&script).contains(&"turn/start".to_owned()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let generation = broker.generation();
    broker.set_account(Some("@bob:x"));
    assert!(broker.generation() > generation);
    assert!(!ctx.is_open());
    std::thread::sleep(Duration::from_millis(500));
    // No completion (not even an error) reaches the old instance.
    while let Ok(event) = rx.try_recv() {
        assert!(
            !matches!(event, ContextEvent::Complete(_)),
            "a stale reply was delivered"
        );
    }
    let (sink, _rx) = collect();
    assert!(ctx.call(ContextOp::History, sink).is_err());
    let seen = methods(&script);
    assert!(seen.contains(&"turn/interrupt".to_owned()), "{seen:?}");
    assert!(seen.contains(&"peer/context/close".to_owned()), "{seen:?}");
    // The old account's context cannot be opened for the new account.
    assert!(broker
        .open_context(spec("@alice:x", "app#2", &ALL))
        .is_err());
    // The new account gets its own peer namespace.
    let bob = broker.open_context(spec("@bob:x", "app#3", &ALL)).unwrap();
    let (sink, rx) = collect();
    script.lock().unwrap().hold_turns = false;
    bob.call(ContextOp::Open, sink).unwrap();
    complete(&rx).unwrap();
    let namespaces: BTreeSet<String> = script
        .lock()
        .unwrap()
        .calls
        .iter()
        .filter(|(m, _)| m == "peer/prepare")
        .map(|(_, p)| p["memory_namespace"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        namespaces.len(),
        2,
        "one namespace per account: {namespaces:?}"
    );
}

#[test]
fn two_instances_get_separate_contexts_and_events() {
    let (broker, script) = new_broker(&ALL);
    broker.set_account(Some("@a:x"));
    let one = broker.open_context(spec("@a:x", "notes#1", &ALL)).unwrap();
    let two = broker.open_context(spec("@a:x", "poll#1", &ALL)).unwrap();
    let (s1, r1) = collect();
    let (s2, r2) = collect();
    one.call(ContextOp::Turn { text: "a".into() }, s1).unwrap();
    two.call(ContextOp::Turn { text: "b".into() }, s2).unwrap();
    assert_eq!(complete(&r1).unwrap()["text"], "Hello there");
    assert_eq!(complete(&r2).unwrap()["text"], "Hello there");
    let sessions: BTreeSet<String> = script
        .lock()
        .unwrap()
        .calls
        .iter()
        .filter(|(m, _)| m == "turn/start")
        .map(|(_, p)| p["session_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(sessions.len(), 2, "{sessions:?}");
    let contexts = script
        .lock()
        .unwrap()
        .calls
        .iter()
        .filter(|(m, _)| m == "peer/context/open")
        .count();
    assert_eq!(contexts, 2);
    assert_eq!(
        script
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(m, _)| m == "peer/prepare")
            .count(),
        1,
        "one peer for the app, not one per instance"
    );
}

#[test]
fn a_kernel_without_host_owned_peers_is_refused_not_substituted() {
    let (broker, script) = new_broker(&ALL);
    script.lock().unwrap().legacy = true;
    broker.set_account(Some("@a:x"));
    let ctx = broker.open_context(spec("@a:x", "app#1", &ALL)).unwrap();
    let (sink, rx) = collect();
    ctx.call(ContextOp::Turn { text: "hi".into() }, sink)
        .unwrap();
    let err = complete(&rx).unwrap_err();
    assert!(err.contains("UPCR-2026-034"), "{err}");
    assert!(!methods(&script).contains(&"turn/start".to_owned()));
    assert!(matches!(broker.availability(), Availability::Failed(_)));
}

#[test]
fn release_closes_the_apps_contexts_without_stopping_a_shared_kernel() {
    let (broker, script) = new_broker(&ALL);
    broker.set_account(Some("@a:x"));
    let ctx = broker.open_context(spec("@a:x", "app#1", &ALL)).unwrap();
    let (sink, rx) = collect();
    ctx.call(ContextOp::Open, sink).unwrap();
    complete(&rx).unwrap();
    broker.release();
    assert!(!ctx.is_open());
    std::thread::sleep(Duration::from_millis(300));
    assert!(methods(&script).contains(&"peer/context/close".to_owned()));
    assert!(broker.open_context(spec("@a:x", "app#2", &ALL)).is_err());
    assert!(matches!(
        broker.availability(),
        Availability::Unavailable(_)
    ));
    broker.shutdown(); // not owned: a no-op on the kernel
}


// ---------------------------------------------------------------- UPCR-2026-035

/// The shell's side, recorded: what it declares, what it was handed.
#[derive(Default)]
struct RecordingHost {
    declared: Mutex<Vec<Value>>,
    workspace: Mutex<Option<std::path::PathBuf>>,
    suspended: Mutex<bool>,
    /// Answer each call at once with this (else hold it).
    answer: Mutex<Option<ToolOutcome>>,
    calls: Mutex<Vec<(HostToolCall, ToolReply)>>,
    cancels: Mutex<Vec<(String, String)>>,
    inputs: Mutex<Vec<PeerInput>>,
    approvals: Mutex<Vec<(HostToolApproval, ApprovalAnswer)>>,
    questions: Mutex<Vec<(AgentQuestion, QuestionAnswer)>>,
    closed_questions: Mutex<Vec<String>>,
}

impl ToolHost for RecordingHost {
    fn declarations(&self, _app: &str, _account: &str) -> Result<Vec<Value>, String> {
        Ok(self.declared.lock().unwrap().clone())
    }
    fn agent_workspace(&self, _app: &str, _account: &str) -> Option<std::path::PathBuf> {
        self.workspace.lock().unwrap().clone()
    }
    fn suspended(&self, _app: &str, _account: &str) -> bool {
        *self.suspended.lock().unwrap()
    }
    fn tool_call(&self, call: HostToolCall, reply: ToolReply) {
        if let Some(outcome) = self.answer.lock().unwrap().clone() {
            reply.finish(outcome);
        }
        self.calls.lock().unwrap().push((call, reply));
    }
    fn tool_cancel(&self, _app: &str, call_id: &str, reason: &str) {
        self.cancels.lock().unwrap().push((call_id.into(), reason.into()));
    }
    fn admit_input(&self, _app: &str, _account: &str, input: &PeerInput) -> Result<(), String> {
        self.inputs.lock().unwrap().push(input.clone());
        Ok(())
    }
    fn host_tool_approval(&self, _app: &str, _account: Option<&str>, approval: HostToolApproval, answer: ApprovalAnswer) -> bool {
        self.approvals.lock().unwrap().push((approval, answer));
        true
    }
    fn user_question(&self, _app: &str, _account: Option<&str>, question: AgentQuestion, answer: QuestionAnswer) -> bool {
        self.questions.lock().unwrap().push((question, answer));
        true
    }
    fn user_question_closed(&self, _app: &str, question_id: &str) {
        self.closed_questions.lock().unwrap().push(question_id.to_owned());
    }
}

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    for _ in 0..100 {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}");
}

fn calls_of(script: &Arc<Mutex<Script>>, method: &str) -> Vec<(usize, Value)> {
    let s = script.lock().unwrap();
    s.calls.iter().zip(&s.conns).filter(|((m, _), _)| m == method).map(|((_, p), c)| (*c, p.clone())).collect()
}

fn position(script: &Arc<Mutex<Script>>, method: &str) -> Option<usize> {
    script.lock().unwrap().calls.iter().position(|(m, _)| m == method)
}

fn peer_slug(script: &Arc<Mutex<Script>>) -> String {
    calls_of(script, "peer/tools/register")[0].1["peer"].as_str().unwrap().to_owned()
}

fn tool_call_params(slug: &str, call_id: &str, turn: &str, context: Option<&str>) -> Value {
    let session = match context {
        Some(c) => format!("_main:api:octosense#peerctx-{slug}.{c}"),
        None => format!("_main:api:octosense#peer-{slug}"),
    };
    json!({"peer": slug, "session_id": session, "context_id": context, "turn_id": turn, "call_id": call_id,
        "tool_call_id": format!("tc-{call_id}"), "args_digest": "d", "name": "rinx.message.send", "app": "rinx",
        "caller": {"kind": "app_peer", "peer": slug, "session_id": session, "context_id": context, "turn_id": turn},
        "args": {"room": "!r", "text": "hi"}, "risk": "act", "confirm_required": false, "timeout_ms": 30000, "tools_version": 1})
}

#[test]
fn the_apps_tools_are_registered_on_the_driving_link_after_prepare_and_before_any_turn() {
    let host = Arc::new(RecordingHost::default());
    *host.declared.lock().unwrap() = vec![json!({"name": "rinx.message.send", "description": "Send", "input_schema": {"type": "object"}, "risk": "act", "outward": true, "confirm": "app"})];
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    broker.set_account(Some("@alice:example.org"));
    let ctx = broker.open_context(spec("@alice:example.org", "notes#1", &ALL)).unwrap();
    let (sink, rx) = collect();
    ctx.call(ContextOp::Turn { text: "hi".into() }, sink).unwrap();
    complete(&rx).unwrap();
    let prepare = position(&script, "peer/prepare").unwrap();
    let register = position(&script, "peer/tools/register").expect("registered");
    let turn = position(&script, "turn/start").unwrap();
    assert!(prepare < register && register < turn, "prepare, register, then turns");
    let registrations = calls_of(&script, "peer/tools/register");
    assert_eq!(registrations.len(), 1);
    let (conn, params) = &registrations[0];
    assert_eq!(params["session_id"], "_main:api:octosense#system", "the originator names the peer");
    assert_eq!(params["host_token"], "fixture-host-token");
    assert!(params.get("generic_tools").is_none(), "omitted: the peer keeps its kernel roster");
    assert_eq!(params["tools"][0]["name"], "rinx.message.send");
    let (turn_conn, _) = &calls_of(&script, "turn/start")[0];
    assert_eq!(conn, turn_conn, "registered on the connection that drives the turns");
}

#[test]
fn an_app_with_no_tools_registers_an_empty_set_and_again_after_a_reconnect() {
    let (broker, script) = new_broker(&ALL);
    broker.set_account(Some("@a:x"));
    wait_for("the first registration", || calls_of(&script, "peer/tools/register").len() == 1);
    assert_eq!(calls_of(&script, "peer/tools/register")[0].1["tools"], json!([]), "an empty set until the app declares tools");
    wait_for("ready", || broker.availability() == Availability::Ready);
    kill_link(&script);
    // The broker binds again on a new link by itself: prepare, then register.
    wait_for("a registration on the new link", || calls_of(&script, "peer/tools/register").iter().any(|(c, _)| *c == 1));
    let prepares: Vec<usize> = calls_of(&script, "peer/prepare").iter().map(|(c, _)| *c).collect();
    assert_eq!(prepares, vec![0, 1]);
    let ctx = broker.open_context(spec("@a:x", "app#1", &ALL)).unwrap();
    let (sink, rx) = collect();
    ctx.call(ContextOp::Turn { text: "hi".into() }, sink).unwrap();
    complete(&rx).unwrap();
    let (turn_conn, _) = &calls_of(&script, "turn/start")[0];
    assert_eq!(*turn_conn, 1);
    let s = script.lock().unwrap();
    let last_register = s.calls.iter().rposition(|(m, _)| m == "peer/tools/register").unwrap();
    let first_turn = s.calls.iter().position(|(m, _)| m == "turn/start").unwrap();
    assert!(last_register < first_turn, "registered again before the next turn");
}

#[test]
fn a_peer_that_could_not_register_runs_no_turn_and_says_so() {
    let (broker, script) = new_broker(&ALL);
    script.lock().unwrap().refuse_register = true;
    broker.set_account(Some("@a:x"));
    let ctx = broker.open_context(spec("@a:x", "app#1", &ALL)).unwrap();
    let (sink, rx) = collect();
    ctx.call(ContextOp::Turn { text: "hi".into() }, sink).unwrap();
    let err = complete(&rx).unwrap_err();
    assert!(err.contains("did not take the app's tools"), "{err}");
    assert!(position(&script, "turn/start").is_none(), "never a memory-less turn");
    assert!(position(&script, "peer/context/open").is_none());
    assert!(matches!(broker.availability(), Availability::Failed(_)));
}

#[test]
fn a_tool_call_is_stamped_run_once_answered_on_its_link_and_never_after_a_cancel() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    broker.set_account(Some("@alice:x"));
    let ctx = broker.open_context(spec("@alice:x", "mini.news#1", &ALL)).unwrap();
    let (sink, rx) = collect();
    ctx.call(ContextOp::Open, sink).unwrap();
    complete(&rx).unwrap();
    let slug = peer_slug(&script);
    let context_id = calls_of(&script, "peer/context/open")[0].1["context_id"].as_str().unwrap().to_owned();

    notify(&script, "peer/tool/call", tool_call_params(&slug, "c1", "t1", Some(&context_id)));
    wait_for("the host to get the call", || host.calls.lock().unwrap().len() == 1);
    let (call, reply) = host.calls.lock().unwrap()[0].clone();
    assert_eq!(call.account.as_deref(), Some("@alice:x"), "the account is the host's, never the app's");
    assert_eq!(call.client.as_deref(), Some("mini.news#1"), "the client comes from the context table");
    assert_eq!(call.calling_app, "rinx");
    assert_eq!(call.origin, CallOrigin::Context);
    assert!(reply.acknowledge());
    assert!(reply.finish(ToolOutcome::Ok(json!({"sent": true}))));
    wait_for("two results", || calls_of(&script, "peer/tool/result").len() == 2);
    let results = calls_of(&script, "peer/tool/result");
    assert_eq!(results[0].1["status"], "awaiting_confirmation");
    assert_eq!(results[1].1["ok"], true);
    assert_eq!(results[1].1["host_token"], "fixture-host-token");
    assert_eq!(results[1].1["peer"], slug.as_str());
    assert!(results.iter().all(|(c, _)| *c == 0), "on the connection the call came on");

    // The kernel re-dispatches the same occurrence: the first answer, nothing runs again.
    let mut again = tool_call_params(&slug, "c1-again", "t1", Some(&context_id));
    again["tool_call_id"] = json!("tc-c1");
    notify(&script, "peer/tool/call", again);
    wait_for("the repeat's answer", || calls_of(&script, "peer/tool/result").len() == 3);
    assert_eq!(host.calls.lock().unwrap().len(), 1, "executed at most once");
    assert_eq!(calls_of(&script, "peer/tool/result")[2].1["call_id"], "c1-again");

    // Cancelled before the host answered: nothing reaches the kernel.
    notify(&script, "peer/tool/call", tool_call_params(&slug, "c2", "t2", None));
    wait_for("the second call", || host.calls.lock().unwrap().len() == 2);
    notify(&script, "peer/tool/cancel", json!({"call_id": "c2", "reason": "timeout"}));
    wait_for("the cancel", || !host.cancels.lock().unwrap().is_empty());
    assert_eq!(host.cancels.lock().unwrap()[0], ("c2".to_string(), "timeout".to_string()));
    let (call2, reply2) = host.calls.lock().unwrap()[1].clone();
    assert_eq!(call2.origin, CallOrigin::PeerOwn);
    assert!(!reply2.finish(ToolOutcome::Ok(json!({}))), "nothing after cancel");
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(calls_of(&script, "peer/tool/result").len(), 3);

    // Another peer's call (or the system session's) is not this broker's:
    // it leaves it to its own host and answers nothing.
    let mut foreign = tool_call_params("other-peer", "c-foreign", "t9", None);
    foreign["peer"] = json!("other-peer");
    notify(&script, "peer/tool/call", foreign);
    let mut system = tool_call_params(&slug, "c-system", "t9", None);
    system["peer"] = Value::Null;
    system["caller"]["kind"] = json!("system");
    notify(&script, "peer/tool/call", system);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(calls_of(&script, "peer/tool/result").len(), 3);
    assert_eq!(host.calls.lock().unwrap().len(), 2);

    // A context this app never opened is refused.
    notify(&script, "peer/tool/call", tool_call_params(&slug, "c3", "t3", Some("forged")));
    wait_for("the refusal", || calls_of(&script, "peer/tool/result").len() == 4);
    assert_eq!(calls_of(&script, "peer/tool/result")[3].1["error"]["kind"], "unknown_context");
    assert_eq!(host.calls.lock().unwrap().len(), 2);
}

#[test]
fn calls_of_an_interrupted_turn_are_refused_and_a_closed_link_ends_calls_in_flight() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    script.lock().unwrap().hold_turns = true;
    broker.set_account(Some("@a:x"));
    let ctx = broker.open_context(spec("@a:x", "app#1", &ALL)).unwrap();
    let (sink, _rx) = collect();
    ctx.call(ContextOp::Turn { text: "hi".into() }, sink).unwrap();
    wait_for("the turn", || position(&script, "turn/start").is_some());
    let turn = calls_of(&script, "turn/start")[0].1["turn_id"].as_str().unwrap().to_owned();
    let context_id = calls_of(&script, "peer/context/open")[0].1["context_id"].as_str().unwrap().to_owned();
    let slug = peer_slug(&script);
    // One call in flight when the person stops the turn.
    notify(&script, "peer/tool/call", tool_call_params(&slug, "c1", &turn, Some(&context_id)));
    wait_for("the call", || host.calls.lock().unwrap().len() == 1);
    let (sink, rx) = collect();
    ctx.call(ContextOp::Interrupt, sink).unwrap();
    complete(&rx).unwrap();
    assert!(host.cancels.lock().unwrap().contains(&("c1".to_string(), "cancelled".to_string())), "the interrupt ends its calls");
    // N1: a call of that turn arriving after the interrupt never runs.
    notify(&script, "peer/tool/call", tool_call_params(&slug, "c-late", &turn, Some(&context_id)));
    wait_for("the refusal", || calls_of(&script, "peer/tool/result").iter().any(|(_, p)| p["call_id"] == "c-late"));
    let late = calls_of(&script, "peer/tool/result").into_iter().find(|(_, p)| p["call_id"] == "c-late").unwrap().1;
    assert_eq!(late["error"]["kind"], "turn_interrupted");
    assert_eq!(host.calls.lock().unwrap().len(), 1);

    notify(&script, "peer/tool/call", tool_call_params(&slug, "c2", "other-turn", None));
    wait_for("the second call", || host.calls.lock().unwrap().len() == 2);
    kill_link(&script);
    wait_for("the disconnect", || host.cancels.lock().unwrap().iter().any(|(c, r)| c == "c2" && r == "disconnected"));
    assert!(!host.calls.lock().unwrap()[1].1.is_open(), "a dropped connection fails the call");
    assert_eq!(broker.calls_in_flight(), 0);
}

#[test]
fn the_system_agents_input_starts_the_peers_turn_once_and_queues_while_busy() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    script.lock().unwrap().hold_turns = true;
    broker.set_account(Some("@a:x"));
    wait_for("ready", || broker.availability() == Availability::Ready);
    let slug = peer_slug(&script);
    let session = format!("_main:api:octosense#peer-{slug}");
    let input = |id: &str, turn: &str| json!({"peer": slug, "session_id": session, "input_id": id, "turn_id": turn, "text": format!("brief {id}")});
    notify(&script, "peer/input", input("i1", "turn-1"));
    wait_for("the turn", || position(&script, "turn/start").is_some());
    let (conn, start) = calls_of(&script, "turn/start")[0].clone();
    assert_eq!(start["turn_id"], "turn-1", "the kernel's turn id");
    assert_eq!(start["session_id"], session.as_str());
    assert_eq!(start["input"][0]["text"], "brief i1");
    assert_eq!(conn, calls_of(&script, "peer/tools/register")[0].0, "on the registering connection");
    // A repeat is dropped; another input waits for the running turn.
    notify(&script, "peer/input", input("i1", "turn-1"));
    notify(&script, "peer/input", input("i2", "turn-2"));
    wait_for("the queue", || broker.queued_inputs() == 1);
    assert_eq!(calls_of(&script, "turn/start").len(), 1);
    // A call from the input's turn is the system agent's request for the person.
    notify(&script, "peer/tool/call", tool_call_params(&slug, "c1", "turn-1", None));
    wait_for("the call", || host.calls.lock().unwrap().len() == 1);
    assert_eq!(host.calls.lock().unwrap()[0].0.origin, CallOrigin::PeerInput);
    assert_eq!(host.calls.lock().unwrap()[0].0.trigger, TurnTrigger::SystemAgent);
    notify(&script, "turn/completed", json!({"session_id": session, "turn_id": "turn-1"}));
    wait_for("the queued input", || calls_of(&script, "turn/start").len() == 2);
    assert_eq!(calls_of(&script, "turn/start")[1].1["turn_id"], "turn-2");
    assert_eq!(host.inputs.lock().unwrap().len(), 2, "each input admitted once");

    // A suspended (signed-out) account starts nothing.
    notify(&script, "turn/completed", json!({"session_id": session, "turn_id": "turn-2"}));
    *host.suspended.lock().unwrap() = true;
    notify(&script, "peer/input", input("i3", "turn-3"));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(calls_of(&script, "turn/start").len(), 2);
}

#[test]
fn a_host_tool_approval_goes_to_the_host_and_is_answered_on_its_link() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    broker.set_account(Some("@a:x"));
    let ctx = broker.open_context(spec("@a:x", "app#1", &ALL)).unwrap();
    let (sink, rx) = collect();
    ctx.call(ContextOp::Open, sink).unwrap();
    complete(&rx).unwrap();
    let session = calls_of(&script, "session/open").last().unwrap().1["session_id"].as_str().unwrap().to_owned();
    notify(&script, "approval/requested", json!({"session_id": session, "approval_id": "a1", "turn_id": "t", "tool_name": "mail_send", "title": "Send", "body": "",
        "approval_kind": "host_tool", "typed_details": {"kind": "host_tool", "host_tool": {"app": "mail", "tool": "mail.send", "args": {"to": ["ana@example.org"]}, "risk": "act", "outward": true, "calling_kind": "app_peer", "calling_session_id": session}}}));
    wait_for("the host", || host.approvals.lock().unwrap().len() == 1);
    let (approval, answer) = host.approvals.lock().unwrap()[0].clone();
    assert_eq!((approval.app.as_str(), approval.tool.as_str()), ("mail", "mail.send"));
    assert!(answer.respond(false));
    assert!(!answer.respond(true), "answered once");
    wait_for("the answer", || position(&script, "approval/respond").is_some());
    let respond = calls_of(&script, "approval/respond")[0].1.clone();
    assert_eq!((respond["approval_id"].as_str(), respond["decision"].as_str()), (Some("a1"), Some("deny")));
    // The app's context heard that the host has it, and was never asked.
    let mut seen = Vec::new();
    while let Ok(ContextEvent::Data(d)) = rx.recv_timeout(Duration::from_millis(200)) {
        seen.push(d["method"].as_str().unwrap_or("").to_owned());
    }
    assert!(!seen.iter().any(|m| m == "approval/requested"), "{seen:?}");
    drop(broker);
}

/// ADR 0004 §6 (G11): an agent's `ask_user_question` goes to the host with
/// the turn's origin, never to the app; only the host's answer reaches the
/// kernel, on the link it came on; the app's context cannot answer it (nor
/// a `host_tool` approval the host holds); the turn's end closes it.
#[test]
fn an_agents_question_goes_to_the_host_and_only_the_host_answers_it() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    script.lock().unwrap().hold_turns = true;
    broker.set_account(Some("@a:x"));
    let ctx = broker.open_context(spec("@a:x", "mini.news#1", &ALL)).unwrap();
    let (sink, rx) = collect();
    ctx.call(ContextOp::Open, sink).unwrap();
    complete(&rx).unwrap();
    let ctx_session = calls_of(&script, "session/open").last().unwrap().1["session_id"].as_str().unwrap().to_owned();
    let question = |session: &str, id: &str, turn: &str| {
        json!({"session_id": session, "question_id": id, "turn_id": turn, "title": "Which room?", "body": "Pick one",
            "questions": [{"header": "Room", "question": "Post where?", "options": [{"label": "#a", "description": ""}, {"label": "#b", "description": ""}], "allow_free_text": true}]})
    };
    // A context's turn (the person, in the app): the app's conversation.
    let (sink, rx) = collect();
    ctx.call(ContextOp::Turn { text: "post it".into() }, sink).unwrap();
    wait_for("the turn", || position(&script, "turn/start").is_some());
    let ctx_turn = calls_of(&script, "turn/start")[0].1["turn_id"].as_str().unwrap().to_owned();
    notify(&script, "user_question/requested", question(&ctx_session, "q1", &ctx_turn));
    wait_for("the host", || host.questions.lock().unwrap().len() == 1);
    let (q, answer) = host.questions.lock().unwrap()[0].clone();
    assert_eq!(q.origin, CallOrigin::Context);
    assert_eq!(q.client.as_deref(), Some("mini.news#1"), "stamped from the host's context table");
    assert!(q.context_id.is_some());
    assert_eq!(q.questions[0].options.len(), 2);
    let mut seen = Vec::new();
    while let Ok(ContextEvent::Data(d)) = rx.recv_timeout(Duration::from_millis(300)) {
        seen.push(d["method"].as_str().unwrap_or("").to_owned());
    }
    assert!(seen.iter().any(|m| m == "user_question/handled_by_host"), "{seen:?}");
    assert!(!seen.iter().any(|m| m == "user_question/requested"), "the app is never asked: {seen:?}");
    // The app cannot answer it (nor anything else the host holds).
    let (sink, rx2) = collect();
    ctx.call(ContextOp::Approval { id: "q1".into(), approve: true }, sink).unwrap();
    let refused = complete(&rx2).unwrap_err();
    assert!(refused.contains("OctoSense"), "{refused}");
    assert!(position(&script, "approval/respond").is_none() && position(&script, "user_question/respond").is_none());
    // The host's answer goes to the kernel, once, on the link it came on.
    assert!(answer.respond(&[QuestionReply::option("#b")]));
    wait_for("the answer", || position(&script, "user_question/respond").is_some());
    let (conn, respond) = calls_of(&script, "user_question/respond")[0].clone();
    assert_eq!(respond["question_id"], "q1");
    assert_eq!(respond["session_id"], ctx_session.as_str());
    assert_eq!(respond["answers"], json!([{"selected_labels": ["#b"]}]));
    assert_eq!(conn, calls_of(&script, "turn/start")[0].0);

    // The system agent's `peer/input` turn: its question is the system chat's.
    let slug = peer_slug(&script);
    let peer_session = format!("_main:api:octosense#peer-{slug}");
    notify(&script, "peer/input", json!({"peer": slug, "session_id": peer_session, "input_id": "i1", "turn_id": "turn-in", "text": "ask them"}));
    wait_for("the input turn", || calls_of(&script, "turn/start").len() == 2);
    notify(&script, "user_question/requested", question(&peer_session, "q2", "turn-in"));
    // The peer's own turn (the app's agent): the app's conversation.
    notify(&script, "user_question/requested", question(&peer_session, "q3", "turn-own"));
    wait_for("both", || host.questions.lock().unwrap().len() == 3);
    let origins: Vec<(String, CallOrigin)> = host.questions.lock().unwrap().iter().map(|(q, _)| (q.question_id.clone(), q.origin)).collect();
    assert_eq!(origins[1], ("q2".to_string(), CallOrigin::PeerInput));
    assert_eq!(origins[2], ("q3".to_string(), CallOrigin::PeerOwn));
    // A turn that ends closes its unanswered question.
    notify(&script, "turn/completed", json!({"session_id": peer_session, "turn_id": "turn-in"}));
    wait_for("closed", || host.closed_questions.lock().unwrap().contains(&"q2".to_string()));
    assert!(!host.closed_questions.lock().unwrap().contains(&"q3".to_string()));
    drop(broker);
}

#[test]
fn a_new_peers_workspace_is_the_account_folder_and_a_resume_keeps_the_one_it_was_made_with() {
    let dir = std::env::temp_dir().join(format!("app-peers-cwd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let host = Arc::new(RecordingHost::default());
    *host.workspace.lock().unwrap() = Some("/home/apps/rinx/accounts/abc".into());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), Some(dir.clone()));
    broker.set_account(Some("@a:x"));
    wait_for("the peer", || broker.availability() == Availability::Ready);
    assert_eq!(calls_of(&script, "peer/prepare")[0].1["cwd"], "/home/apps/rinx/accounts/abc");
    drop(broker);
    // A later run resumes with the SAME workspace, whatever the host says now.
    *host.workspace.lock().unwrap() = Some("/elsewhere".into());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), Some(dir.clone()));
    broker.set_account(Some("@a:x"));
    wait_for("the peer", || broker.availability() == Availability::Ready);
    let prepare = calls_of(&script, "peer/prepare")[0].1.clone();
    assert_eq!(prepare["host_token"], "fixture-host-token");
    assert_eq!(prepare["cwd"], "/home/apps/rinx/accounts/abc");
    drop(broker);
    // A peer created before (a token, no recorded workspace) keeps the kernel's.
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        if entry.path().extension().is_some_and(|e| e == "cwd") {
            std::fs::remove_file(entry.path()).unwrap();
        }
    }
    let (broker, script) = new_broker_with(&ALL, Some(host), Some(dir.clone()));
    broker.set_account(Some("@a:x"));
    wait_for("the peer", || broker.availability() == Availability::Ready);
    assert!(calls_of(&script, "peer/prepare")[0].1.get("cwd").is_none());
    drop(broker);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn each_turns_trigger_is_stamped_on_its_calls_and_approvals() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    script.lock().unwrap().hold_turns = true;
    broker.set_account(Some("@a:x"));
    let ctx = broker.open_context(spec("@a:x", "app#1", &ALL)).unwrap();
    // A turn the app started because a message arrived.
    let (sink, _rx) = collect();
    let incoming = TurnTrigger::Incoming { from: Some("@bo:x".into()) };
    ctx.call(ContextOp::TurnFrom { text: "reply to Bo".into(), trigger: incoming.clone() }, sink).unwrap();
    wait_for("the turn", || position(&script, "turn/start").is_some());
    let turn = calls_of(&script, "turn/start")[0].1["turn_id"].as_str().unwrap().to_owned();
    let context_id = calls_of(&script, "peer/context/open")[0].1["context_id"].as_str().unwrap().to_owned();
    let slug = peer_slug(&script);
    notify(&script, "peer/tool/call", tool_call_params(&slug, "c1", &turn, Some(&context_id)));
    wait_for("the call", || host.calls.lock().unwrap().len() == 1);
    assert_eq!(host.calls.lock().unwrap()[0].0.trigger, incoming, "the turn's trigger, never 'the person'");
    // Its approval carries the same trigger.
    let session = format!("_main:api:octosense#peerctx-{slug}.{context_id}");
    notify(&script, "approval/requested", json!({"session_id": session, "approval_id": "a1", "turn_id": turn, "approval_kind": "host_tool",
        "typed_details": {"host_tool": {"app": "mail", "tool": "mail.send", "args": {}, "risk": "act", "calling_kind": "app_peer", "context_id": context_id}}}));
    wait_for("the approval", || host.approvals.lock().unwrap().len() == 1);
    assert_eq!(host.approvals.lock().unwrap()[0].0.trigger, incoming);
    // A turn this broker did not start (the peer's own), and a legacy
    // `Turn` that says nothing: unknown.
    notify(&script, "peer/tool/call", tool_call_params(&slug, "c2", "someone-elses-turn", None));
    wait_for("the second call", || host.calls.lock().unwrap().len() == 2);
    assert_eq!(host.calls.lock().unwrap()[1].0.trigger, TurnTrigger::Unknown);
    let parsed = HostToolCall::parse(&tool_call_params(&slug, "c3", &turn, None)).unwrap();
    assert_eq!(parsed.trigger, TurnTrigger::Unknown, "the default until the host stamps it");
    drop(broker);
}

#[test]
fn octos_own_approvals_on_a_context_or_the_peers_session_go_to_the_host() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    broker.set_account(Some("@a:x"));
    let ctx = broker.open_context(spec("@a:x", "mini.notes#1", &ALL)).unwrap();
    let (sink, rx) = collect();
    ctx.call(ContextOp::Open, sink).unwrap();
    complete(&rx).unwrap();
    let slug = peer_slug(&script);
    let context_id = calls_of(&script, "peer/context/open")[0].1["context_id"].as_str().unwrap().to_owned();
    let context_session = calls_of(&script, "session/open").last().unwrap().1["session_id"].as_str().unwrap().to_owned();
    // octos's own write_file approval in the app's context.
    notify(&script, "approval/requested", json!({"session_id": context_session, "approval_id": "w1", "turn_id": "t", "tool_name": "write_file", "title": "Write notes.md", "body": "outside the workspace"}));
    wait_for("the host", || host.approvals.lock().unwrap().len() == 1);
    let (approval, answer) = host.approvals.lock().unwrap()[0].clone();
    assert!(approval.octos, "octos's own tool, not a host tool");
    assert_eq!((approval.app.as_str(), approval.tool.as_str()), ("rinx", "write_file"), "owned by the peer's app");
    assert_eq!(approval.args, json!({"title": "Write notes.md", "body": "outside the workspace"}));
    assert_eq!(approval.context_id.as_deref(), Some(context_id.as_str()));
    assert_eq!(approval.client.as_deref(), Some("mini.notes#1"), "the client from the host's own context table");
    // The app hears only that the host has it, never the approval itself.
    let mut seen = Vec::new();
    while let Ok(ContextEvent::Data(d)) = rx.recv_timeout(Duration::from_millis(200)) {
        seen.push(d["method"].as_str().unwrap_or("").to_owned());
    }
    assert!(seen.iter().any(|m| m == host_tools::HANDLED_BY_HOST), "{seen:?}");
    assert!(!seen.iter().any(|m| m == "approval/requested"), "{seen:?}");
    assert!(answer.respond(true));
    wait_for("the answer", || position(&script, "approval/respond").is_some());
    assert_eq!(calls_of(&script, "approval/respond")[0].1["decision"], "approve");
    // One on the peer's own session (a peer/input turn): no longer dropped.
    let own = format!("_main:api:octosense#peer-{slug}");
    notify(&script, "approval/requested", json!({"session_id": own, "approval_id": "w2", "turn_id": "turn-9", "tool_name": "shell", "title": "Run", "body": "ls"}));
    wait_for("the host", || host.approvals.lock().unwrap().len() == 2);
    let (approval, _) = host.approvals.lock().unwrap()[1].clone();
    assert!(approval.octos && approval.context_id.is_none() && approval.client.is_none());
    drop(broker);
}
