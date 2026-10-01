//! The broker against a scripted kernel speaking the UPCR-2026-034 subset.
#![cfg(feature = "broker")]

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use octosense_app_peers::broker::{account_tag, app_namespace, BoxFuture, Broker, BrokerConfig, Connector, Link, ToolHostHandle};
use octosense_app_peers::host_tools::{AgentQuestion, ApprovalAnswer, CallOrigin, HostToolApproval, HostToolCall, InputRefusal, PeerInput, QuestionAnswer, QuestionReply, ToolHost, ToolOutcome, ToolReply, TurnOrigin};
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
    /// The next this-many turn/starts are refused `turn_in_progress` (the
    /// kernel still runs a turn the host has not seen end).
    busy_starts: usize,
    /// turn/interrupt ends the turn (a v2 `turn_terminal`, `interrupted`).
    interrupts_end: bool,
    /// Every turn/start is refused with this kind (a refusal other than
    /// `turn_in_progress`).
    refuse_starts: Option<String>,
    /// What session/hydrate answers (`messages`) for a request context.
    history: Vec<Value>,
    /// What session/hydrate answers for the peer's own session.
    peer_history: Vec<Value>,
    /// peer/context/open ignores `share_history` (a kernel before it).
    no_share_history: bool,
    connects: usize,
    /// Like octos: each staged peer's workspace by name; a resume must name
    /// the same one (no `cwd`: the kernel's `/kernel/ws`), and a `cwd` must
    /// exist. `None`: nothing is checked.
    bindings: Option<std::collections::HashMap<String, String>>,
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
    fn kernel_id(&self) -> Option<String> {
        Some(format!("fake:{:p}", Arc::as_ptr(&self.0)))
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
                    let interrupts_end = script.lock().unwrap().interrupts_end;
                    let refuse_start = script.lock().unwrap().refuse_starts.clone().filter(|_| method == "turn/start");
                    let (legacy, hold, refuse_register, busy, history, no_share) = {
                        let mut s = script.lock().unwrap();
                        s.calls.push((method.clone(), params.clone()));
                        s.conns.push(conn);
                        let busy = method == "turn/start" && s.busy_starts > 0;
                        if busy {
                            s.busy_starts -= 1;
                        }
                        let on_peer = params["session_id"].as_str().is_some_and(|id| id.contains("#peer-"));
                        let history = if on_peer { s.peer_history.clone() } else { s.history.clone() };
                        (s.legacy, s.hold_turns, s.refuse_register, busy, history, s.no_share_history)
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
                            let checked = {
                                let mut s = script.lock().unwrap();
                                let resume = params.get("host_token").is_some();
                                match s.bindings.as_mut() {
                                    Some(_) if params.get("cwd").is_some() && cwd != "/kernel/ws" && !std::path::Path::new(&cwd).is_dir() => Err("invalid_params"),
                                    Some(b) if resume => match b.get(&name) {
                                        Some(bound) if *bound != cwd => Err("peer_binding_mismatch"),
                                        _ => Ok(()),
                                    },
                                    Some(b) => {
                                        b.insert(name.clone(), cwd.clone());
                                        Ok(())
                                    }
                                    None => Ok(()),
                                }
                            };
                            if let Err(kind) = checked {
                                send(refuse(kind));
                                continue;
                            }
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
                        "peer/tools/register" | "peer/tools/unregister" | "peer/context/open" | "peer/context/close" | "peer/tool/result"
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
                            let mut result = json!({"session_id": session, "created": true, "share_history": null});
                            if params.get("share_history").is_some() && !no_share {
                                result["share_history"] = json!({"last_n": 20, "max_bytes": 16384});
                            }
                            send(reply(result));
                        }
                        "turn/start" if busy => send(refuse("turn_in_progress")),
                        "turn/start" if refuse_start.is_some() => send(refuse(refuse_start.as_deref().unwrap())),
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
                        "session/hydrate" => send(reply(json!({"messages": history}))),
                        "peer/tools/unregister" => send(reply(json!({"slug": params["peer"], "profile_id": "_main", "unregistered": true}))),
                        "turn/interrupt" => {
                            send(reply(json!({"interrupted": true})));
                            if interrupts_end {
                                let session = params["session_id"].as_str().unwrap_or("");
                                let (base, topic) = session.split_once('#').unwrap_or((session, ""));
                                send(json!({"jsonrpc": "2.0", "method": "projection/envelope", "params": {"session_id": base, "topic": topic, "turn_id": params["turn_id"], "thread_id": params["turn_id"], "seq": 99,
                                    "payload": {"type": "turn_terminal", "data": {"outcome": "interrupted"}}}}).to_string());
                            }
                        }
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
    new_broker_timed(services, host, state_dir, None)
}

/// With a short prompt deadline and grace (ms), injected (never the env:
/// tests share a process).
fn new_broker_timed(services: &[&str], host: Option<Arc<RecordingHost>>, state_dir: Option<std::path::PathBuf>, timing: Option<(u64, u64)>) -> (Broker, Arc<Mutex<Script>>) {
    new_broker_app("rinx", services, host, state_dir, timing)
}

fn new_broker_app(app: &str, services: &[&str], host: Option<Arc<RecordingHost>>, state_dir: Option<std::path::PathBuf>, timing: Option<(u64, u64)>) -> (Broker, Arc<Mutex<Script>>) {
    new_broker_cfg(app, services, host, state_dir, timing, None)
}

fn new_broker_cfg(app: &str, services: &[&str], host: Option<Arc<RecordingHost>>, state_dir: Option<std::path::PathBuf>, timing: Option<(u64, u64)>, turn_timeout: Option<Duration>) -> (Broker, Arc<Mutex<Script>>) {
    let script = Arc::new(Mutex::new(Script::default()));
    let mut cfg = BrokerConfig::new(
        Deployment::Hosted,
        "_main",
        "_main:api:octosense#system",
        app,
        "Rinx",
        services.iter().map(|s| s.to_string()).collect(),
    );
    cfg.tool_host = host.map(|h| ToolHostHandle(h as Arc<dyn ToolHost>));
    cfg.state_dir = state_dir;
    if let Some((deadline, grace)) = timing {
        cfg.prompt_deadline = Duration::from_millis(deadline);
        cfg.expiry_grace = Duration::from_millis(grace);
    }
    if let Some(timeout) = turn_timeout {
        cfg.turn_timeout = timeout;
    }
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
    /// The startup check refused the account's workspace, and why.
    refused: Mutex<Option<String>>,
    /// Answer each call at once with this (else hold it).
    answer: Mutex<Option<ToolOutcome>>,
    calls: Mutex<Vec<(HostToolCall, ToolReply)>>,
    cancels: Mutex<Vec<(String, String)>>,
    inputs: Mutex<Vec<PeerInput>>,
    approvals: Mutex<Vec<(HostToolApproval, ApprovalAnswer)>>,
    questions: Mutex<Vec<(AgentQuestion, QuestionAnswer)>>,
    closed_questions: Mutex<Vec<String>>,
    closed_approvals: Mutex<Vec<String>>,
    generic: Mutex<Option<Vec<String>>>,
    /// Refuse each `peer/input` with this.
    refuse_input: Mutex<Option<InputRefusal>>,
}

impl ToolHost for RecordingHost {
    fn declarations(&self, _app: &str, _account: &str) -> Result<Vec<Value>, String> {
        Ok(self.declared.lock().unwrap().clone())
    }
    fn agent_workspace(&self, _app: &str, _account: &str) -> Option<std::path::PathBuf> {
        self.workspace.lock().unwrap().clone()
    }
    fn generic_tools(&self, _app: &str, _account: &str) -> Option<Vec<String>> {
        self.generic.lock().unwrap().clone()
    }
    fn suspended(&self, _app: &str, _account: &str) -> bool {
        *self.suspended.lock().unwrap()
    }
    fn workspace_refused(&self, _app: &str, _account: &str) -> Option<String> {
        self.refused.lock().unwrap().clone()
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
    fn admit_input(&self, _app: &str, _account: &str, input: &PeerInput) -> Result<(), InputRefusal> {
        self.inputs.lock().unwrap().push(input.clone());
        match self.refuse_input.lock().unwrap().clone() {
            Some(why) => Err(why),
            None => Ok(()),
        }
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
    fn host_tool_approval_closed(&self, _app: &str, approval_id: &str) {
        self.closed_approvals.lock().unwrap().push(approval_id.to_owned());
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
    assert!(params.get("generic_tools").is_none(), "a host that sets none: omitted");
    assert_eq!(params["tools"][0]["name"], "rinx.message.send");
    let (turn_conn, _) = &calls_of(&script, "turn/start")[0];
    assert_eq!(conn, turn_conn, "registered on the connection that drives the turns");
}

/// ADR 0004 §12: the peer keeps exactly the kernel tools the host grants
/// its agent; an empty list keeps none.
#[test]
fn the_hosts_exact_kernel_tools_are_registered_with_the_apps_tools() {
    for generic in [vec!["read_file".to_string(), "ask_user_question".to_string()], Vec::new()] {
        let host = Arc::new(RecordingHost::default());
        *host.generic.lock().unwrap() = Some(generic.clone());
        let (broker, script) = new_broker_with(&ALL, Some(host), None);
        broker.set_account(Some("@a:x"));
        wait_for("registered", || calls_of(&script, "peer/tools/register").len() == 1);
        assert_eq!(calls_of(&script, "peer/tools/register")[0].1["generic_tools"], json!(generic), "exact, never omitted");
    }
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

/// A turn that ends before the host answered its `host_tool` approval (the
/// app's own Stop, an interrupt) withdraws it from the host: the kernel
/// dropped it with the turn, so no sheet may keep asking. An approval the
/// host answered, or one of a turn still running, is left alone.
#[test]
fn a_turn_that_ends_withdraws_its_unanswered_host_tool_approval_from_the_host() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    let (_slug, session) = busy_peer(&broker, &script);
    let host_tool = |id: &str, turn: &str| {
        json!({"session_id": session, "approval_id": id, "turn_id": turn, "tool_name": "mail_send", "title": "Send", "body": "",
            "approval_kind": "host_tool", "typed_details": {"kind": "host_tool", "host_tool": {"app": "mail", "tool": "mail.send", "args": {}, "risk": "destructive", "outward": true, "calling_kind": "app_peer", "calling_session_id": session}}})
    };
    notify(&script, "approval/requested", host_tool("a-open", "turn-i1"));
    notify(&script, "approval/requested", host_tool("a-answered", "turn-i1"));
    notify(&script, "approval/requested", host_tool("a-other", "turn-other"));
    wait_for("the host", || host.approvals.lock().unwrap().len() == 3);
    assert!(host.approvals.lock().unwrap()[1].1.respond(true));
    notify(&script, "turn/error", json!({"session_id": session, "turn_id": "turn-i1", "message": "interrupted"}));
    wait_for("withdrawn", || !host.closed_approvals.lock().unwrap().is_empty());
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(*host.closed_approvals.lock().unwrap(), ["a-open"], "only the unanswered one of the ended turn");
    assert_eq!(broker.pending_prompts(), 1, "the other turn's approval still waits");
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
    let turn_origins: Vec<(TurnOrigin, bool)> = host.questions.lock().unwrap().iter().map(|(q, _)| (q.turn_origin, q.origin_reported)).collect();
    assert_eq!(turn_origins, [(TurnOrigin::Person, false), (TurnOrigin::SystemAgent, false), (TurnOrigin::App, false)], "derived by the host");
    // A turn origin the kernel reports replaces the derivation.
    let mut reported = question(&peer_session, "q4", "turn-own");
    reported["origin"] = json!("system_agent");
    notify(&script, "user_question/requested", reported);
    wait_for("the reported one", || host.questions.lock().unwrap().len() == 4);
    let fourth = host.questions.lock().unwrap()[3].0.clone();
    assert_eq!((fourth.turn_origin, fourth.origin_reported), (TurnOrigin::SystemAgent, true));
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
    let _ = std::fs::remove_dir_all(&dir);
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("app-peers-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

/// The files a broker kept before one record held both (`<ns>.token`,
/// `<ns>.cwd`).
fn legacy_files(dir: &std::path::Path, account: &str, cwd: Option<&str>) {
    let stem = app_namespace("rinx", account).replace('/', "_");
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(format!("{stem}.token")), "fixture-host-token").unwrap();
    if let Some(cwd) = cwd {
        std::fs::write(dir.join(format!("{stem}.cwd")), cwd).unwrap();
    }
}

fn files_in(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

fn strict(script: &Arc<Mutex<Script>>, bound: &[(&str, &str)]) {
    script.lock().unwrap().bindings = Some(bound.iter().map(|(n, c)| (n.to_string(), c.to_string())).collect());
}

/// The slug the fake kernel gives the Rinx peer of `account`.
fn slug_of(account: &str) -> String {
    format!("rinx-{}", account_tag(account))
}

/// The slug of a peer recorded before names carried the whole tag.
fn short_slug_of(account: &str) -> String {
    format!("rinx-{}", &account_tag(account)[..8])
}

#[cfg(unix)]
fn mode(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// ADR 0004 §11: the host token and the workspace are one record, written
/// at once, owner-only, so a resume never finds one without the other.
#[test]
fn should_keep_the_token_and_workspace_in_one_owner_only_record_when_a_peer_is_created() {
    let root = scratch("record");
    let dir = root.join("peers");
    let ws = root.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let host = Arc::new(RecordingHost::default());
    *host.workspace.lock().unwrap() = Some(ws.clone());
    let (broker, _script) = new_broker_with(&ALL, Some(host), Some(dir.clone()));
    broker.set_account(Some("@a:x"));
    wait_for("the peer", || broker.availability() == Availability::Ready);
    drop(broker);
    let files = files_in(&dir);
    assert_eq!(files.len(), 1, "one record, no token or cwd files: {files:?}");
    assert!(files[0].ends_with(".peer"), "{files:?}");
    let record: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(&files[0])).unwrap()).unwrap();
    assert_eq!(record["token"], "fixture-host-token");
    assert_eq!(record["cwd"], ws.to_string_lossy().as_ref());
    #[cfg(unix)]
    {
        assert_eq!(mode(&dir), 0o700, "the state directory is owner-only");
        assert_eq!(mode(&dir.join(&files[0])), 0o600, "the record is owner-only");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn should_resume_from_the_legacy_token_and_cwd_files_and_migrate_them_to_one_record() {
    let root = scratch("migrate");
    let dir = root.join("peers");
    let ws = root.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    legacy_files(&dir, "@a:x", Some(&ws.to_string_lossy()));
    let host = Arc::new(RecordingHost::default());
    *host.workspace.lock().unwrap() = Some(root.join("elsewhere"));
    let (broker, script) = new_broker_with(&ALL, Some(host), Some(dir.clone()));
    strict(&script, &[(short_slug_of("@a:x").as_str(), &*ws.to_string_lossy())]);
    broker.set_account(Some("@a:x"));
    wait_for("the peer", || broker.availability() == Availability::Ready);
    let prepare = calls_of(&script, "peer/prepare")[0].1.clone();
    assert_eq!(prepare["host_token"], "fixture-host-token");
    assert_eq!(prepare["cwd"], ws.to_string_lossy().as_ref());
    drop(broker);
    let files = files_in(&dir);
    assert_eq!(files.len(), 1, "migrated: {files:?}");
    assert!(files[0].ends_with(".peer"));
    let _ = std::fs::remove_dir_all(&root);
}

/// A token with no saved workspace: the account folder first (a peer made
/// there whose cwd was never recorded), else the kernel's own (a peer made
/// before the account folder was its workspace). Either way it is saved.
#[test]
fn should_resume_with_the_account_folder_when_no_workspace_was_saved() {
    let root = scratch("nocwd");
    let dir = root.join("peers");
    let ws = root.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    legacy_files(&dir, "@a:x", None);
    let host = Arc::new(RecordingHost::default());
    *host.workspace.lock().unwrap() = Some(ws.clone());
    let (broker, script) = new_broker_with(&ALL, Some(host), Some(dir.clone()));
    strict(&script, &[(short_slug_of("@a:x").as_str(), &*ws.to_string_lossy())]);
    broker.set_account(Some("@a:x"));
    wait_for("the peer", || broker.availability() == Availability::Ready);
    let prepares = calls_of(&script, "peer/prepare");
    assert_eq!(prepares.len(), 1);
    assert_eq!(prepares[0].1["cwd"], ws.to_string_lossy().as_ref());
    drop(broker);
    let record = std::fs::read_to_string(dir.join(&files_in(&dir)[0])).unwrap();
    assert!(record.contains(&*ws.to_string_lossy()), "{record}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn should_fall_back_to_the_kernels_workspace_when_a_legacy_peer_was_made_there() {
    let root = scratch("kernelws");
    let dir = root.join("peers");
    let ws = root.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    legacy_files(&dir, "@a:x", None);
    let host = Arc::new(RecordingHost::default());
    *host.workspace.lock().unwrap() = Some(ws.clone());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), Some(dir.clone()));
    strict(&script, &[(short_slug_of("@a:x").as_str(), "/kernel/ws")]);
    broker.set_account(Some("@a:x"));
    wait_for("the peer", || broker.availability() == Availability::Ready);
    let prepares = calls_of(&script, "peer/prepare");
    assert_eq!(prepares.len(), 2, "the account folder, then the kernel's");
    assert_eq!(prepares[0].1["cwd"], ws.to_string_lossy().as_ref());
    assert!(prepares[1].1.get("cwd").is_none());
    drop(broker);
    // Saved: the next run resumes at once.
    let (broker, script) = new_broker_with(&ALL, Some(host), Some(dir.clone()));
    strict(&script, &[(short_slug_of("@a:x").as_str(), "/kernel/ws")]);
    broker.set_account(Some("@a:x"));
    wait_for("the peer", || broker.availability() == Availability::Ready);
    assert_eq!(calls_of(&script, "peer/prepare").len(), 1);
    drop(broker);
    let _ = std::fs::remove_dir_all(&root);
}

/// FNV-1a of the raw bytes: the tag before accounts were normalized.
fn raw_tag(account: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in account.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// A new peer's name carries the whole 64-bit tag (it is what a resume
/// finds the peer by), not 32 bits of it.
#[test]
fn should_name_a_new_peer_with_the_whole_account_tag() {
    let (broker, script) = new_broker(&ALL);
    broker.set_account(Some("@a:x"));
    wait_for("the peer", || broker.availability() == Availability::Ready);
    assert_eq!(calls_of(&script, "peer/prepare")[0].1["names"][0], format!("Rinx {}", account_tag("@a:x")));
}

/// A peer recorded before names were widened resumes under its old name;
/// one made under an un-normalized account keeps its namespace, and the
/// record keeps both.
#[test]
fn should_resume_an_older_peer_under_its_own_name_and_namespace() {
    let root = scratch("oldname");
    let dir = root.join("peers");
    let raw = "@A:X ";
    let old_ns = format!("app/rinx/acct-{}", raw_tag(raw));
    assert_ne!(old_ns, app_namespace("rinx", raw), "the tag changed for this id");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{}.token", old_ns.replace('/', "_"))), "fixture-host-token").unwrap();
    for run in 0..2 {
        let (broker, script) = new_broker_with(&ALL, None, Some(dir.clone()));
        broker.set_account(Some(raw));
        wait_for("the peer", || broker.availability() == Availability::Ready);
        let prepare = calls_of(&script, "peer/prepare")[0].1.clone();
        assert_eq!(prepare["host_token"], "fixture-host-token", "run {run}");
        assert_eq!(prepare["memory_namespace"], old_ns.as_str(), "run {run}");
        assert_eq!(prepare["names"][0], format!("Rinx {}", &raw_tag(raw)[..8]), "run {run}");
        drop(broker);
    }
    let files = files_in(&dir);
    assert_eq!(files.len(), 1, "one record: {files:?}");
    let record: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(&files[0])).unwrap()).unwrap();
    assert_eq!(record["namespace"], old_ns.as_str());
    let _ = std::fs::remove_dir_all(&root);
}

/// Removing an account deletes its folder; adding it again resumes the same
/// peer, whose workspace must exist first.
#[test]
fn should_recreate_the_account_folder_before_resuming_when_it_was_removed() {
    let root = scratch("recreate");
    let dir = root.join("peers");
    let ws = root.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let host = Arc::new(RecordingHost::default());
    *host.workspace.lock().unwrap() = Some(ws.clone());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), Some(dir.clone()));
    strict(&script, &[]);
    broker.set_account(Some("@a:x"));
    wait_for("the peer", || broker.availability() == Availability::Ready);
    drop(broker);
    std::fs::remove_dir_all(&ws).unwrap();
    let (broker, script) = new_broker_with(&ALL, Some(host), Some(dir.clone()));
    strict(&script, &[(slug_of("@a:x").as_str(), &*ws.to_string_lossy())]);
    broker.set_account(Some("@a:x"));
    wait_for("the peer", || broker.availability() == Availability::Ready);
    assert!(ws.is_dir(), "recreated");
    #[cfg(unix)]
    assert_eq!(mode(&ws), 0o700);
    drop(broker);
    let _ = std::fs::remove_dir_all(&root);
}

/// ADR 0004 §11: a workspace the startup check refused stays refused for a
/// resumed peer too, not only for a new one.
#[test]
fn should_prepare_no_peer_when_the_workspace_is_refused_new_or_resumed() {
    let dir = std::env::temp_dir().join(format!("app-peers-refused-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let host = Arc::new(RecordingHost::default());
    *host.workspace.lock().unwrap() = Some("/home/apps/rinx/accounts/abc".into());
    *host.refused.lock().unwrap() = Some("a symlink into the secrets".into());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), Some(dir.clone()));
    broker.set_account(Some("@a:x"));
    std::thread::sleep(Duration::from_millis(300));
    assert!(calls_of(&script, "peer/prepare").is_empty(), "no new peer on a refused workspace");
    assert_ne!(broker.availability(), Availability::Ready);
    drop(broker);
    // A peer made while the workspace was clean...
    *host.refused.lock().unwrap() = None;
    let (broker, _script) = new_broker_with(&ALL, Some(host.clone()), Some(dir.clone()));
    broker.set_account(Some("@a:x"));
    wait_for("the peer", || broker.availability() == Availability::Ready);
    drop(broker);
    // ...is not resumed once a later start refuses it.
    *host.refused.lock().unwrap() = Some("a symlink into the secrets".into());
    let (broker, script) = new_broker_with(&ALL, Some(host), Some(dir.clone()));
    broker.set_account(Some("@a:x"));
    std::thread::sleep(Duration::from_millis(300));
    assert!(calls_of(&script, "peer/prepare").is_empty(), "no resume on a refused workspace");
    assert_ne!(broker.availability(), Availability::Ready);
    drop(broker);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn should_refuse_input_and_tool_calls_when_the_workspace_is_refused() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    script.lock().unwrap().hold_turns = true;
    broker.set_account(Some("@a:x"));
    wait_for("ready", || broker.availability() == Availability::Ready);
    let slug = peer_slug(&script);
    let session = format!("_main:api:octosense#peer-{slug}");
    *host.refused.lock().unwrap() = Some("a hard link to a secret".into());
    notify(&script, "peer/input", json!({"peer": slug, "session_id": session, "input_id": "r1", "turn_id": "turn-r1", "text": "hi"}));
    wait_for("the rejection", || !calls_of(&script, "peer/input/reject").is_empty());
    let reject = calls_of(&script, "peer/input/reject")[0].1.clone();
    assert_eq!(reject["reason"], "other");
    assert!(reject["message"].as_str().unwrap().contains("workspace"), "{reject}");
    assert!(host.inputs.lock().unwrap().is_empty(), "refused before the host is asked");
    notify(&script, "peer/tool/call", tool_call_params(&slug, "c1", "t1", None));
    wait_for("the refusal", || !calls_of(&script, "peer/tool/result").is_empty());
    assert_eq!(calls_of(&script, "peer/tool/result")[0].1["error"]["kind"], "workspace_refused");
    assert!(host.calls.lock().unwrap().is_empty(), "nothing ran");
    assert!(calls_of(&script, "turn/start").is_empty());
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

/// ADR 0004 §8 (the 2026-09-29 review's first gap): a kernel tool's
/// approval on the system agent's `peer/input` turn reaches the host (the
/// person decides; the system agent never answers it), stamped as the
/// system agent's turn, and the app only hears that the host has it. When
/// its turn ends unanswered it is withdrawn from the host.
#[test]
fn a_kernel_tool_approval_on_a_peer_input_turn_goes_to_the_host_and_ends_with_its_turn() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    let (_slug, session) = busy_peer(&broker, &script);
    let conversation = broker.open_conversation(spec("@a:x", "ask#1", &ALL)).unwrap();
    let (sink, rx) = collect();
    conversation.subscribe(Some(sink));
    notify(&script, "approval/requested", json!({"session_id": session, "approval_id": "k1", "turn_id": "turn-i1", "tool_name": "shell", "title": "Run", "body": "ls",
        "risk_level": "high"}));
    wait_for("the host", || host.approvals.lock().unwrap().len() == 1);
    let (approval, answer) = host.approvals.lock().unwrap()[0].clone();
    assert!(approval.octos, "a kernel tool, not a host tool");
    assert_eq!((approval.app.as_str(), approval.tool.as_str(), approval.turn_id.as_str()), ("rinx", "shell", "turn-i1"));
    assert_eq!(approval.trigger, TurnTrigger::SystemAgent, "the system agent's turn: its rules, never 'the person'");
    assert_eq!(approval.calling_kind, octosense_app_peers::host_tools::CallerKind::AppPeer, "the app's own agent calls; the person decides");
    assert!(approval.context_id.is_none() && approval.client.is_none());
    let seen: Vec<String> = events(&rx, Duration::from_millis(300)).iter().map(|d| d["method"].as_str().unwrap_or("").to_owned()).collect();
    assert!(seen.iter().any(|m| m == host_tools::HANDLED_BY_HOST), "{seen:?}");
    assert!(!seen.iter().any(|m| m == "approval/requested"), "the app never answers it: {seen:?}");
    assert!(position(&script, "approval/respond").is_none(), "nobody answered for the person");
    // The turn ends before anyone answered: the host's sheet stops asking,
    // and the next queued input starts.
    notify(&script, "turn/completed", json!({"session_id": session, "turn_id": "turn-i1"}));
    wait_for("withdrawn", || host.closed_approvals.lock().unwrap().contains(&"k1".to_string()));
    assert!(!answer.is_sent());
    wait_for("the queued input", || calls_of(&script, "turn/start").len() == 2);
    assert_eq!(broker.pending_prompts(), 0);
    drop(broker);
}

/// The link an approval or question came on closed (a kernel restart, the
/// app released): nothing can answer it there any more, so the host
/// withdraws what it holds instead of asking until the deadline.
#[test]
fn a_closed_link_withdraws_the_approvals_and_questions_the_host_holds() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    let (_slug, session) = busy_peer(&broker, &script);
    notify(&script, "approval/requested", json!({"session_id": session, "approval_id": "k1", "turn_id": "turn-i1", "tool_name": "shell", "title": "Run", "body": "ls"}));
    notify(&script, "approval/requested", json!({"session_id": session, "approval_id": "k2", "turn_id": "turn-i1", "tool_name": "write_file", "title": "Write", "body": "b"}));
    notify(&script, "user_question/requested", json!({"session_id": session, "question_id": "q1", "turn_id": "turn-i1", "title": "Which?", "body": "",
        "questions": [{"header": "H", "question": "Which?", "options": [], "allow_free_text": true}]}));
    wait_for("the host", || host.approvals.lock().unwrap().len() == 2 && host.questions.lock().unwrap().len() == 1);
    // One answered before the link went: that one is not withdrawn.
    assert!(host.approvals.lock().unwrap()[1].1.respond(true));
    kill_link(&script);
    wait_for("withdrawn", || !host.closed_approvals.lock().unwrap().is_empty() && !host.closed_questions.lock().unwrap().is_empty());
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(*host.closed_approvals.lock().unwrap(), ["k1"]);
    assert_eq!(*host.closed_questions.lock().unwrap(), ["q1"]);
    assert_eq!(broker.pending_prompts(), 0);
    drop(broker);
}


/// octos#2621: a `peer/input` the host will not act on is refused on the
/// connection it came on, with the reason (signed_out, no_consent, busy,
/// other + message), before any `turn/start` with its turn id; queued
/// inputs still start later with theirs.
#[test]
fn a_refused_input_is_rejected_with_its_reason_before_any_turn() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    script.lock().unwrap().hold_turns = true;
    broker.set_account(Some("@a:x"));
    wait_for("ready", || broker.availability() == Availability::Ready);
    let slug = peer_slug(&script);
    let session = format!("_main:api:octosense#peer-{slug}");
    let input = |id: &str| json!({"peer": slug, "session_id": session, "input_id": id, "turn_id": format!("turn-{id}"), "text": id});
    let rejects = || calls_of(&script, "peer/input/reject");

    // no_consent, other (with its one-line message), from the host.
    *host.refuse_input.lock().unwrap() = Some(InputRefusal::NoConsent);
    notify(&script, "peer/input", input("n1"));
    wait_for("no_consent", || rejects().len() == 1);
    *host.refuse_input.lock().unwrap() = Some(InputRefusal::Other("the app is updating".into()));
    notify(&script, "peer/input", input("o1"));
    wait_for("other", || rejects().len() == 2);
    *host.refuse_input.lock().unwrap() = None;
    // signed_out: the account is suspended.
    *host.suspended.lock().unwrap() = true;
    notify(&script, "peer/input", input("s1"));
    wait_for("signed_out", || rejects().len() == 3);
    *host.suspended.lock().unwrap() = false;
    let got: Vec<(String, Value)> = rejects().iter().map(|(_, p)| (p["input_id"].as_str().unwrap().to_string(), p.clone())).collect();
    assert_eq!(got[0].1["reason"], "no_consent");
    assert!(got[0].1.get("message").is_none(), "a message only with other");
    assert_eq!((got[1].1["reason"].as_str(), got[1].1["message"].as_str()), (Some("other"), Some("the app is updating")));
    assert_eq!(got[2].1["reason"], "signed_out");
    for (_, p) in &got {
        assert_eq!(p["peer"], slug.as_str());
        assert_eq!(p["session_id"], "_main:api:octosense#system", "the peer's originator");
        assert_eq!(p["host_token"], "fixture-host-token");
    }
    assert!(calls_of(&script, "turn/start").is_empty(), "no turn for a refused input");
    let register_conn = calls_of(&script, "peer/tools/register")[0].0;
    assert!(rejects().iter().all(|(c, _)| *c == register_conn), "on the connection the input came on");

    // busy: one running, MAX_QUEUED_INPUTS queued, the next refused.
    notify(&script, "peer/input", input("b0"));
    wait_for("the running turn", || calls_of(&script, "turn/start").len() == 1);
    for i in 1..=octosense_app_peers::host_tools::MAX_QUEUED_INPUTS {
        notify(&script, "peer/input", input(&format!("b{i}")));
    }
    wait_for("the queue", || broker.queued_inputs() == octosense_app_peers::host_tools::MAX_QUEUED_INPUTS);
    notify(&script, "peer/input", input("full"));
    wait_for("busy", || rejects().len() == 4);
    assert_eq!(rejects()[3].1["reason"], "busy");
    assert_eq!(rejects()[3].1["input_id"], "full");
    // The queued ones still start, each with its own turn id.
    notify(&script, "turn/completed", json!({"session_id": session, "turn_id": "turn-b0"}));
    wait_for("the next queued input", || calls_of(&script, "turn/start").len() == 2);
    assert_eq!(calls_of(&script, "turn/start")[1].1["turn_id"], "turn-b1");
    assert!(!calls_of(&script, "turn/start").iter().any(|(_, p)| p["turn_id"] == "turn-full"));
}


/// octos#2658: when the app's last instance releases (the app closed, or
/// the person turned its agent off), the broker releases the peer's route
/// with `peer/tools/unregister` (the originator, the peer, its host token),
/// so the system agent's later `peer_send_input` fails in the kernel
/// instead of being accepted with nobody to run it. An input that still
/// reaches the released broker before then is refused, never started.
#[test]
fn the_last_instance_releasing_unregisters_its_peers_route_and_refuses_a_late_input() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    broker.set_account(Some("@a:x"));
    wait_for("ready", || broker.availability() == Availability::Ready);
    let slug = peer_slug(&script);
    let session = format!("_main:api:octosense#peer-{slug}");
    let register_conn = calls_of(&script, "peer/tools/register")[0].0;
    broker.release();
    // Before its link goes: a late input is refused, not started.
    notify(&script, "peer/input", json!({"peer": slug, "session_id": session, "input_id": "late", "turn_id": "turn-late", "text": "late"}));
    wait_for("the unregister", || !calls_of(&script, "peer/tools/unregister").is_empty());
    let unregister = calls_of(&script, "peer/tools/unregister");
    assert_eq!(unregister.len(), 1);
    let (conn, params) = &unregister[0];
    assert_eq!(*conn, register_conn, "on the connection that holds the route");
    assert_eq!(params["peer"], slug.as_str());
    assert_eq!(params["session_id"], "_main:api:octosense#system", "the peer's originator");
    assert_eq!(params["host_token"], "fixture-host-token");
    assert_eq!(params["profile_id"], "_main");
    wait_for("the late input's refusal", || !calls_of(&script, "peer/input/reject").is_empty());
    let reject = &calls_of(&script, "peer/input/reject")[0].1;
    assert_eq!((reject["input_id"].as_str(), reject["reason"].as_str()), (Some("late"), Some("other")));
    assert!(calls_of(&script, "turn/start").is_empty(), "no turn for a released app");
    assert!(host.inputs.lock().unwrap().is_empty(), "never admitted");
}

/// A broker that never bound a peer (no account) has no route to release.
#[test]
fn a_release_without_a_bound_peer_unregisters_nothing() {
    let (broker, script) = new_broker(&ALL);
    broker.release();
    std::thread::sleep(Duration::from_millis(700));
    assert!(calls_of(&script, "peer/tools/unregister").is_empty());
}

/// A second instance of the app on the same kernel: its broker for the same
/// peer, on a connection of its own.
fn second_instance(script: &Arc<Mutex<Script>>, host: &Arc<RecordingHost>) -> Broker {
    let mut cfg = BrokerConfig::new(Deployment::Hosted, "_main", "_main:api:octosense#system", "rinx", "Rinx", ALL.iter().map(|s| s.to_string()).collect());
    cfg.tool_host = Some(ToolHostHandle(host.clone() as Arc<dyn ToolHost>));
    Broker::new(cfg, Arc::new(FakeConnector(script.clone())))
}

/// Every connection the fake kernel has open (the shell's kernel router
/// sends a peer session's frames to every consumer that named it).
fn broadcast(script: &Arc<Mutex<Script>>, method: &str, params: Value) {
    let conns = script.lock().unwrap().out.len();
    for conn in 0..conns {
        emit(script, conn, json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string());
    }
}

/// ADR 0004 §5 (the 2026-09-29 review): octos routes a peer's `peer/input`
/// and tool calls to the connection that registered its tools LAST, and the
/// shell's kernel router hands a peer session's frames to every consumer
/// that named it. With two instances of one app (two windows of one
/// module), exactly one broker drives the peer: the oldest live
/// instance registers the tools and takes the peer's inputs, calls,
/// approvals and questions, once; the other takes none. When the driving
/// instance closes, the next one registers on its own connection and takes
/// over (with the queue), and the peer's running turn is not stopped while
/// an instance of the app is still open.
#[test]
fn two_instances_of_one_app_drive_the_peer_once_and_hand_over_on_close() {
    let host = Arc::new(RecordingHost::default());
    let (first, script) = new_broker_with(&ALL, Some(host.clone()), None);
    script.lock().unwrap().hold_turns = true;
    first.set_account(Some("@a:x"));
    wait_for("the first", || first.availability() == Availability::Ready);
    let second = second_instance(&script, &host);
    second.set_account(Some("@a:x"));
    wait_for("the second", || second.availability() == Availability::Ready && second.peer().is_some());
    std::thread::sleep(Duration::from_millis(200));
    let registers = calls_of(&script, "peer/tools/register");
    assert_eq!(registers.len(), 1, "only the driving instance registers: {registers:?}");
    let first_conn = registers[0].0;
    let slug = peer_slug(&script);
    let session = format!("_main:api:octosense#peer-{slug}");
    assert_eq!(second.peer().unwrap().0, slug, "the same peer");

    // An input reaches both: one turn, on the driver's connection.
    let input = |id: &str| json!({"peer": slug, "session_id": session, "input_id": id, "turn_id": format!("turn-{id}"), "text": id});
    broadcast(&script, "peer/input", input("i1"));
    wait_for("the turn", || !calls_of(&script, "turn/start").is_empty());
    std::thread::sleep(Duration::from_millis(200));
    let starts = calls_of(&script, "turn/start");
    assert_eq!(starts.len(), 1, "one turn per input: {starts:?}");
    assert_eq!(starts[0].0, first_conn);
    assert_eq!(host.inputs.lock().unwrap().len(), 1, "admitted once");
    // A call and an approval of the peer's session reach both: the host gets each once.
    broadcast(&script, "peer/tool/call", tool_call_params(&slug, "c1", "turn-i1", None));
    broadcast(&script, "approval/requested", json!({"session_id": session, "approval_id": "k1", "turn_id": "turn-i1", "tool_name": "shell", "title": "Run", "body": "ls"}));
    wait_for("the call and the approval", || host.calls.lock().unwrap().len() == 1 && host.approvals.lock().unwrap().len() == 1);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(host.calls.lock().unwrap().len(), 1, "run once");
    assert_eq!(host.calls.lock().unwrap()[0].0.origin, CallOrigin::PeerInput);
    assert_eq!(host.approvals.lock().unwrap().len(), 1, "one sheet");
    // A second input waits behind the running turn.
    broadcast(&script, "peer/input", input("i2"));
    wait_for("queued", || first.queued_inputs() == 1);
    assert_eq!(second.queued_inputs(), 0);

    // The driving instance closes: the running turn goes on, the other
    // instance registers on its own connection and takes over.
    first.release();
    wait_for("the hand-over", || calls_of(&script, "peer/tools/register").len() == 2);
    assert!(calls_of(&script, "peer/tools/unregister").is_empty(), "an instance still drives the peer: its route stays");
    let registers = calls_of(&script, "peer/tools/register");
    assert_ne!(registers[1].0, first_conn, "on the second instance's connection");
    std::thread::sleep(Duration::from_millis(700));
    assert!(position(&script, "turn/interrupt").is_none(), "an instance is still open: the turn goes on");
    assert_eq!(second.queued_inputs(), 1, "the queue moved over");
    // The turn ends: the queued input starts on the new driver's connection.
    broadcast(&script, "turn/completed", json!({"session_id": session, "turn_id": "turn-i1"}));
    wait_for("the queued input", || calls_of(&script, "turn/start").len() == 2);
    assert_eq!(calls_of(&script, "turn/start")[1].1["turn_id"], "turn-i2");
    assert_eq!(calls_of(&script, "turn/start")[1].0, registers[1].0);
    // Its calls are the new driver's, still stamped as the system agent's.
    broadcast(&script, "peer/tool/call", tool_call_params(&slug, "c2", "turn-i2", None));
    wait_for("the second call", || host.calls.lock().unwrap().len() == 2);
    assert_eq!(host.calls.lock().unwrap()[1].0.origin, CallOrigin::PeerInput);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(host.calls.lock().unwrap().len(), 2);
    // The last instance closes: now its running turn is stopped.
    second.release();
    wait_for("the interrupt", || position(&script, "turn/interrupt").is_some());
    // ...and its route is released (octos#2658), on its own connection.
    wait_for("the unregister", || !calls_of(&script, "peer/tools/unregister").is_empty());
    let unregister = calls_of(&script, "peer/tools/unregister");
    assert_eq!(unregister.len(), 1);
    assert_eq!(unregister[0].0, registers[1].0, "on the driving instance's connection");
    assert!(position(&script, "turn/interrupt") < position(&script, "peer/tools/unregister"), "after its turn is stopped");
    drop(first);
    drop(second);
}

/// The 2026-09-29 review: a host `turn/start` for a `peer/input` that the
/// kernel refuses (anything but `turn_in_progress`, which is retried) is
/// said to the kernel with `peer/input/reject` and the reason, on the
/// connection the input came on, so the system agent learns why the app
/// did not act; the peer's queue moves on. Retrying `turn_in_progress`
/// until the turn timeout ends as `busy`.
#[test]
fn a_refused_turn_start_for_an_input_is_rejected_with_its_reason() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_cfg("rinx", &ALL, Some(host.clone()), None, None, Some(Duration::from_millis(600)));
    broker.set_account(Some("@a:x"));
    wait_for("ready", || broker.availability() == Availability::Ready);
    let slug = peer_slug(&script);
    let session = format!("_main:api:octosense#peer-{slug}");
    let input = |id: &str| json!({"peer": slug, "session_id": session, "input_id": id, "turn_id": format!("turn-{id}"), "text": id});
    let rejects = || calls_of(&script, "peer/input/reject");

    // A refusal other than turn_in_progress: `other`, with the kernel's words.
    script.lock().unwrap().refuse_starts = Some("session_not_found".into());
    notify(&script, "peer/input", input("r1"));
    wait_for("the reject", || rejects().len() == 1);
    let (conn, reject) = rejects()[0].clone();
    assert_eq!(reject["input_id"], "r1");
    assert_eq!(reject["reason"], "other");
    let message = reject["message"].as_str().unwrap();
    assert!(message.contains("session_not_found"), "{message}");
    assert!(message.len() <= 256 && !message.chars().any(char::is_control));
    assert_eq!((reject["peer"].as_str(), reject["host_token"].as_str()), (Some(slug.as_str()), Some("fixture-host-token")));
    assert_eq!(conn, calls_of(&script, "peer/tools/register")[0].0, "on the connection the input came on");
    assert!(broker.peer_active_turn().is_none(), "the peer is free again");

    // Still busy after the turn timeout: `busy`.
    script.lock().unwrap().refuse_starts = None;
    script.lock().unwrap().busy_starts = 1000;
    notify(&script, "peer/input", input("b1"));
    wait_for("the busy reject", || rejects().len() == 2);
    assert_eq!(rejects()[1].1["input_id"], "b1");
    assert_eq!(rejects()[1].1["reason"], "busy");
    assert!(rejects()[1].1.get("message").is_none());

    // The next input starts as usual.
    script.lock().unwrap().busy_starts = 0;
    notify(&script, "peer/input", input("ok"));
    wait_for("the next turn", || calls_of(&script, "turn/start").iter().any(|(_, p)| p["turn_id"] == "turn-ok"));
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(rejects().len(), 2, "a started input is never refused");
    drop(broker);
}

// ---------------------------------------------------------------------------
// The shared peer conversation (ADR 0004 §6, octos#2626): the person and the
// system agent talk to the peer's own session; each turn says who speaks.

fn events(rx: &std::sync::mpsc::Receiver<ContextEvent>, wait: Duration) -> Vec<Value> {
    let mut out = Vec::new();
    while let Ok(ContextEvent::Data(d)) = rx.recv_timeout(wait) {
        out.push(d);
    }
    out
}

/// The person's lane of a conversation (the peer context opened for it).
fn person_lane(script: &Arc<Mutex<Script>>, n: usize) -> (String, String) {
    let (_, open) = calls_of(script, "peer/context/open")[n].clone();
    let context = open["context_id"].as_str().unwrap().to_owned();
    let slug = open["peer"].as_str().unwrap().to_owned();
    (context.clone(), format!("_main:api:octosense#peerctx-{slug}.{context}"))
}

/// A person's message from the app (a module, a card, a process app
/// without a client) runs in the person's lane: a request context opened
/// with `share_history` (ADR 0004 §6, 2026-09-29), on the connection that
/// registered the peer's tools, labelled `person` with the app's name, or
/// `app` when the app itself started the run; its calls keep G2's trigger
/// and its question is the app conversation's.
#[test]
fn a_persons_message_runs_in_its_sharing_context_with_its_origin() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    broker.set_account(Some("@a:x"));
    let chat = broker.open_conversation(spec("@a:x", "rinx-ui", &ALL)).unwrap();
    let (sink, rx) = collect();
    chat.call(ContextOp::TurnFrom { text: "summarize my day".into(), trigger: TurnTrigger::Person }, sink).unwrap();
    let result = complete(&rx).unwrap();
    assert_eq!(result["text"], "Hello there");
    assert_eq!(result["speaker"], json!({"kind": "person", "label": "Rinx"}));
    assert_eq!(result["lane"], "person");
    let slug = peer_slug(&script);
    let (open_conn, open) = calls_of(&script, "peer/context/open")[0].clone();
    assert_eq!(open["share_history"], json!({}), "the kernel's defaults");
    let (context, lane) = person_lane(&script, 0);
    let (conn, start) = calls_of(&script, "turn/start")[0].clone();
    assert_eq!(start["session_id"], lane.as_str(), "the person's lane, not the peer's session");
    assert_eq!(start["origin"], json!({"kind": "person", "label": "Rinx"}));
    assert_eq!(start["input"][0]["text"], "summarize my day", "the kernel adds the marker, not the host");
    let register_conn = calls_of(&script, "peer/tools/register")[0].0;
    assert_eq!((conn, open_conn), (register_conn, register_conn), "on the connection that registered the tools");

    // The app's own run speaks as the app; an unsaid turn is the person's
    // for the label, and stays Unknown for approval rules.
    script.lock().unwrap().hold_turns = true;
    let (sink, _rx) = collect();
    chat.call(ContextOp::TurnFrom { text: "sync".into(), trigger: TurnTrigger::App }, sink).unwrap();
    wait_for("the app's turn", || calls_of(&script, "turn/start").len() == 2);
    let app_turn = calls_of(&script, "turn/start")[1].1.clone();
    assert_eq!(app_turn["origin"]["kind"], "app");
    let app_turn_id = app_turn["turn_id"].as_str().unwrap().to_owned();
    notify(&script, "turn/completed", json!({"session_id": lane, "turn_id": app_turn_id}));
    std::thread::sleep(Duration::from_millis(300));
    let (sink, _rx) = collect();
    chat.call(ContextOp::Turn { text: "hi".into() }, sink).unwrap();
    wait_for("the unsaid turn", || calls_of(&script, "turn/start").len() == 3);
    let unsaid = calls_of(&script, "turn/start")[2].1.clone();
    assert_eq!(unsaid["origin"]["kind"], "person");
    let unsaid_id = unsaid["turn_id"].as_str().unwrap().to_owned();
    notify(&script, "peer/tool/call", tool_call_params(&slug, "c1", &unsaid_id, Some(&context)));
    wait_for("the call", || host.calls.lock().unwrap().len() == 1);
    let call = host.calls.lock().unwrap()[0].0.clone();
    assert_eq!((call.origin, call.trigger.clone()), (CallOrigin::Context, TurnTrigger::Unknown), "G2's trigger is the authority for rules");
    notify(&script, "user_question/requested", json!({"session_id": lane, "question_id": "q1", "turn_id": unsaid_id, "title": "Which?", "body": "", "questions": []}));
    wait_for("the question", || host.questions.lock().unwrap().len() == 1);
    assert_eq!(host.questions.lock().unwrap()[0].0.turn_origin, TurnOrigin::Person, "the person's turn: the app's conversation");
    // A person-said turn keeps its Person trigger (question routing, G11).
    notify(&script, "turn/completed", json!({"session_id": lane, "turn_id": unsaid_id}));
    std::thread::sleep(Duration::from_millis(300));
    let (sink, _rx) = collect();
    chat.call(ContextOp::TurnFrom { text: "and?".into(), trigger: TurnTrigger::Person }, sink).unwrap();
    wait_for("the person's turn", || calls_of(&script, "turn/start").len() == 4);
    let said = calls_of(&script, "turn/start")[3].1["turn_id"].as_str().unwrap().to_owned();
    notify(&script, "peer/tool/call", tool_call_params(&slug, "c2", &said, Some(&context)));
    wait_for("the second call", || host.calls.lock().unwrap().len() == 2);
    assert_eq!(host.calls.lock().unwrap()[1].0.trigger, TurnTrigger::Person);
    // The system agent never speaks through the app.
    let (sink, rx) = collect();
    let chat2 = broker.open_conversation(spec("@a:x", "card", &ALL)).unwrap();
    chat2.call(ContextOp::TurnFrom { text: "x".into(), trigger: TurnTrigger::SystemAgent }, sink).unwrap();
    assert!(complete(&rx).is_err());
    drop(broker);
}

/// The system agent's `peer/input` starts with the kernel's turn id and NO
/// origin (the kernel labels it `system_agent` and refuses a relabel), on
/// the peer's own session.
#[test]
fn a_peer_input_turn_starts_without_an_origin() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host), None);
    broker.set_account(Some("@a:x"));
    wait_for("ready", || broker.availability() == Availability::Ready);
    let slug = peer_slug(&script);
    let session = format!("_main:api:octosense#peer-{slug}");
    notify(&script, "peer/input", json!({"peer": slug, "session_id": session, "input_id": "i1", "turn_id": "turn-1", "text": "brief"}));
    wait_for("the turn", || position(&script, "turn/start").is_some());
    let start = calls_of(&script, "turn/start")[0].1.clone();
    assert_eq!(start["turn_id"], "turn-1");
    assert_eq!(start["session_id"], session.as_str());
    assert!(start.get("origin").is_none(), "{start}");
}

/// The two lanes run in parallel: a person's message starts in its context
/// while the system agent's input runs on the peer's session, and never
/// waits in the input queue; the inputs keep their own bounded queue; a
/// person's start that meets its own context's previous turn still ending
/// (`turn_in_progress`) is retried with the same turn.
#[test]
fn a_persons_message_runs_while_the_system_agents_input_runs() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host.clone()), None);
    script.lock().unwrap().hold_turns = true;
    broker.set_account(Some("@a:x"));
    wait_for("ready", || broker.availability() == Availability::Ready);
    let slug = peer_slug(&script);
    let session = format!("_main:api:octosense#peer-{slug}");
    let input = |id: &str| json!({"peer": slug, "session_id": session, "input_id": id, "turn_id": format!("turn-{id}"), "text": id});
    notify(&script, "peer/input", input("i1"));
    wait_for("the input's turn", || calls_of(&script, "turn/start").len() == 1);
    // The person writes from the app while the system agent's turn runs:
    // it starts at once, in the person's lane.
    let chat = broker.open_conversation(spec("@a:x", "ui", &ALL)).unwrap();
    let (sink, person_rx) = collect();
    chat.call(ContextOp::TurnFrom { text: "and the weather?".into(), trigger: TurnTrigger::Person }, sink).unwrap();
    wait_for("the person's turn", || calls_of(&script, "turn/start").len() == 2);
    let (_, lane) = person_lane(&script, 0);
    let person_start = calls_of(&script, "turn/start")[1].1.clone();
    assert_eq!(person_start["session_id"], lane.as_str());
    assert_eq!(broker.queued_inputs(), 0, "the person's message is not in the input queue");
    assert_eq!(broker.peer_active_turn().as_deref(), Some("turn-i1"), "the system agent's turn still runs");
    // Inputs queue behind the system agent's own turn only; a full queue
    // refuses `busy`.
    for i in 2..=octosense_app_peers::host_tools::MAX_QUEUED_INPUTS + 1 {
        notify(&script, "peer/input", input(&format!("i{i}")));
    }
    wait_for("full", || broker.queued_inputs() == octosense_app_peers::host_tools::MAX_QUEUED_INPUTS);
    notify(&script, "peer/input", input("extra"));
    wait_for("busy", || calls_of(&script, "peer/input/reject").len() == 1);
    assert_eq!(calls_of(&script, "peer/input/reject")[0].1["reason"], "busy");
    // The person's turn ends while the system agent's still runs.
    let person_turn = person_start["turn_id"].as_str().unwrap().to_owned();
    notify(&script, "turn/completed", json!({"session_id": lane, "turn_id": person_turn}));
    assert_eq!(complete(&person_rx).unwrap()["text"], "Hello there");
    assert_eq!(broker.peer_active_turn().as_deref(), Some("turn-i1"));
    // The next person message meets its context's previous turn still
    // ending: retried, same turn id, same lane.
    script.lock().unwrap().busy_starts = 1;
    let (sink, person_rx) = collect();
    chat.call(ContextOp::TurnFrom { text: "more".into(), trigger: TurnTrigger::Person }, sink).unwrap();
    wait_for("the retried start", || calls_of(&script, "turn/start").len() == 4);
    let starts = calls_of(&script, "turn/start");
    assert_eq!(starts[2].1["turn_id"], starts[3].1["turn_id"], "the same turn after turn_in_progress");
    assert_eq!(starts[3].1["session_id"], lane.as_str());
    notify(&script, "turn/completed", json!({"session_id": lane, "turn_id": starts[3].1["turn_id"]}));
    complete(&person_rx).unwrap();
    // The system agent's turn ends: its next queued input starts, on the
    // peer's session, with no origin.
    notify(&script, "turn/completed", json!({"session_id": session, "turn_id": "turn-i1"}));
    wait_for("the queued input", || calls_of(&script, "turn/start").len() == 5);
    let next = calls_of(&script, "turn/start")[4].1.clone();
    assert_eq!((next["turn_id"].as_str(), next["session_id"].as_str()), (Some("turn-i2"), Some(session.as_str())));
    assert!(next.get("origin").is_none());
    drop(broker);
}

/// The follower of a conversation hears both lanes, each event with its
/// `lane` and speaker: the system agent's turns on the peer's session and
/// the person's in the context. The caller's sink hears only its own turn.
/// History is both transcripts merged by time, each row with its lane.
#[test]
fn a_conversation_follows_both_lanes_and_merges_their_history() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host), None);
    broker.set_account(Some("@a:x"));
    let chat = broker.open_conversation(spec("@a:x", "ui", &ALL)).unwrap();
    let (follow, follow_rx) = collect();
    chat.subscribe(Some(follow));
    let (sink, rx) = collect();
    chat.call(ContextOp::Open, sink).unwrap();
    let opened = complete(&rx).unwrap();
    assert_eq!((opened["conversation"].clone(), opened["shared_history"].clone()), (json!(true), json!(true)));
    let slug = peer_slug(&script);
    let session = format!("_main:api:octosense#peer-{slug}");
    let (_, lane) = person_lane(&script, 0);
    notify(&script, "peer/input", json!({"peer": slug, "session_id": session, "input_id": "i1", "turn_id": "turn-sa", "text": "check the inbox"}));
    notify(&script, "projection/envelope", json!({"session_id": "_main:api:octosense", "topic": format!("peer-{slug}"), "turn_id": "turn-sa", "thread_id": "turn-sa", "seq": 1,
        "payload": {"type": "user_message", "data": {"text": "[from the system agent] check the inbox"}}}));
    let seen = events(&follow_rx, Duration::from_millis(400));
    let sa: Vec<&Value> = seen.iter().filter(|d| d["params"]["turn_id"] == "turn-sa").collect();
    assert!(sa.iter().any(|d| d["method"] == "turn/started"), "{seen:?}");
    assert!(sa.iter().any(|d| d["method"] == "turn/completed"), "{seen:?}");
    assert!(sa.iter().all(|d| d["speaker"]["kind"] == "system_agent" && d["lane"] == "system_agent"), "{sa:?}");
    let user = sa.iter().find(|d| d["method"] == "projection/envelope").unwrap();
    assert_eq!(user["display_text"], "check the inbox");
    // The kernel sends a turn's user message when the turn ends: its words
    // come with its start, from what the broker sent.
    let started = sa.iter().find(|d| d["method"] == "turn/started").unwrap();
    assert_eq!(started["request"], json!({"text": "check the inbox", "speaker": {"kind": "system_agent"}}));
    // The person's own message: the follower sees it too, in the person's
    // lane, with its speaker.
    let (sink, rx) = collect();
    chat.call(ContextOp::Turn { text: "thanks".into() }, sink).unwrap();
    complete(&rx).unwrap();
    let seen = events(&follow_rx, Duration::from_millis(300));
    assert!(
        seen.iter().any(|d| d["method"] == "turn/completed" && d["speaker"]["kind"] == "person" && d["speaker"]["label"] == "Rinx" && d["lane"] == "person" && d["params"]["session_id"] == lane.as_str()),
        "{seen:?}"
    );
    assert!(
        seen.iter().any(|d| d["method"] == "turn/started" && d["lane"] == "person" && d["request"] == json!({"text": "thanks", "speaker": {"kind": "person", "label": "Rinx"}})),
        "{seen:?}"
    );
    // A caller's sink hears only its own turn: nothing of the system agent's.
    let (sink, rx) = collect();
    chat.call(ContextOp::History, sink).unwrap();
    notify(&script, "turn/started", json!({"session_id": session, "turn_id": "turn-other"}));
    complete(&rx).unwrap();
    assert!(events(&rx, Duration::from_millis(200)).is_empty());
    let hydrated: Vec<Value> = calls_of(&script, "session/hydrate").iter().map(|(_, p)| p["session_id"].clone()).collect();
    assert!(hydrated.contains(&json!(lane)) && hydrated.contains(&json!(session)), "both transcripts: {hydrated:?}");
    // Merged by time (fraction digits vary), each row with its lane; the
    // kernel's marker stays and names the speaker.
    {
        let mut s = script.lock().unwrap();
        s.history = vec![
            json!({"role": "user", "content": "[from the person: Rinx] thanks", "persisted_at": "2026-09-29T10:00:01.5Z"}),
            json!({"role": "assistant", "content": "You're welcome.", "persisted_at": "2026-09-29T10:00:03Z"}),
        ];
        s.peer_history = vec![
            json!({"role": "user", "content": "[from the system agent] check the inbox", "persisted_at": "2026-09-29T10:00:01Z"}),
            json!({"role": "assistant", "content": "Done.", "persisted_at": "2026-09-29T10:00:02.25Z"}),
            json!({"role": "user", "content": "unlabelled", "persisted_at": "2026-09-29T10:00:04Z"}),
        ];
    }
    let (sink, rx) = collect();
    chat.call(ContextOp::History, sink).unwrap();
    let rows = complete(&rx).unwrap()["messages"].clone();
    let shown: Vec<(String, String)> = rows.as_array().unwrap().iter().map(|r| (r["lane"].as_str().unwrap().to_owned(), r["content"].as_str().unwrap().to_owned())).collect();
    assert_eq!(
        shown,
        [
            ("system_agent".into(), "[from the system agent] check the inbox".into()),
            ("person".into(), "[from the person: Rinx] thanks".into()),
            ("system_agent".into(), "Done.".into()),
            ("person".into(), "You're welcome.".into()),
            ("system_agent".into(), "unlabelled".into()),
        ]
    );
    assert_eq!((rows[1]["speaker"].clone(), rows[1]["display_text"].clone()), (json!({"kind": "person", "label": "Rinx"}), json!("thanks")));
    assert_eq!(rows[0]["speaker"]["kind"], "system_agent");
    assert!(rows[2].get("speaker").is_none() && rows[4].get("speaker").is_none());
    // Closing the handle ends its follow and closes its context for good.
    chat.close();
    wait_for("the context closed", || calls_of(&script, "peer/context/close").len() == 1);
    assert_eq!(calls_of(&script, "peer/context/close")[0].1["context_id"], calls_of(&script, "peer/context/open")[0].1["context_id"]);
    notify(&script, "turn/started", json!({"session_id": session, "turn_id": "turn-late"}));
    assert!(events(&follow_rx, Duration::from_millis(200)).iter().all(|d| d["params"]["turn_id"] != "turn-late"));
    // A new handle, even for the same instance, opens a new context id.
    let again = broker.open_conversation(spec("@a:x", "ui", &ALL)).unwrap();
    let (sink, rx) = collect();
    again.call(ContextOp::Open, sink).unwrap();
    complete(&rx).unwrap();
    let opens = calls_of(&script, "peer/context/open");
    assert_eq!(opens.len(), 2);
    assert_ne!(opens[0].1["context_id"], opens[1].1["context_id"]);
    drop(broker);
}

/// A stopped turn's user message is never recorded by the kernel (it
/// writes a turn's rows when the turn ends): the conversation's history
/// keeps its request from the broker, in its lane, at the time it started;
/// a recorded one is not repeated.
#[test]
fn a_stopped_turns_request_stays_in_the_conversations_history() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_with(&ALL, Some(host), None);
    {
        let mut s = script.lock().unwrap();
        s.hold_turns = true;
        s.interrupts_end = true;
    }
    broker.set_account(Some("@a:x"));
    let chat = broker.open_conversation(spec("@a:x", "ui", &ALL)).unwrap();
    let (sink, rx) = collect();
    chat.call(ContextOp::TurnFrom { text: "a long digest please".into(), trigger: TurnTrigger::Person }, sink).unwrap();
    wait_for("the person's turn", || calls_of(&script, "turn/start").len() == 1);
    let (sink, stop_rx) = collect();
    chat.call(ContextOp::Interrupt, sink).unwrap();
    assert_eq!(complete(&stop_rx).unwrap()["turns"][0]["lane"], "person");
    assert!(complete(&rx).is_err(), "the stopped turn ends with its error");
    {
        let mut s = script.lock().unwrap();
        s.history = vec![json!({"role": "user", "content": "[from the person: Rinx] earlier", "persisted_at": "2020-01-01T00:00:00Z"})];
        s.peer_history = vec![];
    }
    let (sink, rx) = collect();
    chat.call(ContextOp::History, sink).unwrap();
    let rows = complete(&rx).unwrap()["messages"].as_array().unwrap().clone();
    let shown: Vec<(String, String)> = rows.iter().map(|r| (r["lane"].as_str().unwrap().to_owned(), r["display_text"].as_str().unwrap_or("").to_owned())).collect();
    assert_eq!(shown, [("person".to_owned(), "earlier".to_owned()), ("person".to_owned(), "a long digest please".to_owned())]);
    assert_eq!(rows[1]["speaker"], json!({"kind": "person", "label": "Rinx"}));
    // Once the kernel has the row, it is not added twice.
    script.lock().unwrap().history.push(json!({"role": "user", "content": "[from the person: Rinx] a long digest please", "persisted_at": "2020-01-01T00:00:01Z"}));
    let (sink, rx) = collect();
    chat.call(ContextOp::History, sink).unwrap();
    assert_eq!(complete(&rx).unwrap()["messages"].as_array().unwrap().len(), 2);
    drop(broker);
}

/// A kernel that ignores `share_history` would give the person a plain
/// context the system agent never sees: the conversation is refused.
#[test]
fn a_kernel_without_shared_history_is_refused_for_the_conversation() {
    let (broker, script) = new_broker(&ALL);
    script.lock().unwrap().no_share_history = true;
    broker.set_account(Some("@a:x"));
    let chat = broker.open_conversation(spec("@a:x", "ui", &ALL)).unwrap();
    let (sink, rx) = collect();
    chat.call(ContextOp::Turn { text: "hi".into() }, sink).unwrap();
    assert!(complete(&rx).unwrap_err().contains("share_history"));
    assert!(calls_of(&script, "turn/start").is_empty());
    // A plain request context does not need it.
    let ctx = broker.open_context(spec("@a:x", "mini.news#1", &ALL)).unwrap();
    let (sink, rx) = collect();
    ctx.call(ContextOp::Turn { text: "hi".into() }, sink).unwrap();
    assert_eq!(complete(&rx).unwrap()["text"], "Hello there");
}

/// A caller that opens a request context per client (Rinx's mini apps)
/// still gets its own session: no origin, no shared history, its own
/// transcript.
#[test]
fn a_request_context_for_an_explicit_client_still_works() {
    let (broker, script) = new_broker(&ALL);
    broker.set_account(Some("@a:x"));
    let ctx = broker.open_context(spec("@a:x", "mini.news#1", &ALL)).unwrap();
    let (sink, rx) = collect();
    ctx.call(ContextOp::TurnFrom { text: "hi".into(), trigger: TurnTrigger::Person }, sink).unwrap();
    assert_eq!(complete(&rx).unwrap()["text"], "Hello there");
    let start = calls_of(&script, "turn/start")[0].1.clone();
    assert!(start["session_id"].as_str().unwrap().contains("#peerctx-"));
    assert!(start.get("origin").is_none(), "an origin is refused on a plain context");
    assert_eq!(calls_of(&script, "peer/context/open").len(), 1);
    assert!(calls_of(&script, "peer/context/open")[0].1.get("share_history").is_none(), "a plain context shares nothing");
    let (sink, rx) = collect();
    ctx.subscribe(Some(sink));
    let (sink2, rx2) = collect();
    ctx.call(ContextOp::Open, sink2).unwrap();
    let opened = complete(&rx2).unwrap();
    assert_eq!((opened["conversation"].clone(), opened["shared_history"].clone()), (json!(false), json!(false)));
    assert!(events(&rx, Duration::from_millis(100)).is_empty(), "a request context has no follower");
}


// ---------------------------------------------------------------------------
// Deadlines (ADR 0004 §8): an approval or question nobody answers expires,
// denied or declined, never approved; a turn still running after the grace
// is interrupted so the peer's queue moves on; the person's Stop ends the
// turns running in both lanes.

fn approval_on(session: &str, id: &str, turn: &str) -> Value {
    json!({"session_id": session, "approval_id": id, "turn_id": turn, "tool_name": "write_file", "title": "Write", "body": "notes.md"})
}

/// A running `peer/input` turn (the system agent's) and a second input
/// queued behind it; the peer's session.
fn busy_peer(broker: &Broker, script: &Arc<Mutex<Script>>) -> (String, String) {
    script.lock().unwrap().hold_turns = true;
    broker.set_account(Some("@a:x"));
    wait_for("ready", || broker.availability() == Availability::Ready);
    let slug = peer_slug(script);
    let session = format!("_main:api:octosense#peer-{slug}");
    for id in ["i1", "i2"] {
        notify(script, "peer/input", json!({"peer": slug, "session_id": session, "input_id": id, "turn_id": format!("turn-{id}"), "text": id}));
    }
    wait_for("the first turn and one queued", || calls_of(script, "turn/start").len() == 1 && broker.queued_inputs() == 1);
    (slug, session)
}

#[test]
fn an_approval_nobody_answers_expires_to_deny_and_the_stuck_turn_is_interrupted_after_the_grace() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_timed(&ALL, Some(host.clone()), None, Some((400, 1_500)));
    let (slug, session) = busy_peer(&broker, &script);
    let chat = broker.open_conversation(spec("@a:x", "ui", &ALL)).unwrap();
    let (follow, follow_rx) = collect();
    chat.subscribe(Some(follow));
    notify(&script, "approval/requested", approval_on(&session, "a1", "turn-i1"));
    wait_for("the host", || host.approvals.lock().unwrap().len() == 1);
    assert_eq!(broker.pending_prompts(), 1);
    // The deadline: the app's conversation hears it expired; the host's
    // router expires it (here, the test does what the shell's router does).
    let expired = loop {
        match follow_rx.recv_timeout(Duration::from_secs(5)).expect("prompt/expired") {
            ContextEvent::Data(d) if d["method"] == "prompt/expired" => break d,
            _ => {}
        }
    };
    assert_eq!(expired["params"]["id"], "a1");
    assert_eq!(expired["params"]["reason"], "no answer in 1 s", "the reason says how long");
    let answer = host.approvals.lock().unwrap()[0].1.clone();
    assert!(answer.expire("no answer in 1 s"));
    wait_for("the deny", || position(&script, "approval/respond").is_some());
    // The grace: the turn still runs, so it is interrupted and the queued
    // input starts; nothing is approved and nothing is answered twice.
    wait_for("the interrupt", || position(&script, "turn/interrupt").is_some());
    assert_eq!(calls_of(&script, "turn/interrupt")[0].1["turn_id"], "turn-i1");
    wait_for("the next queued turn", || calls_of(&script, "turn/start").len() == 2);
    assert_eq!(calls_of(&script, "turn/start")[1].1["turn_id"], "turn-i2");
    let responds = calls_of(&script, "approval/respond");
    assert_eq!(responds.len(), 1, "{responds:?}");
    assert_eq!(responds[0].1["decision"], "deny");
    assert!(responds[0].1["client_note"].as_str().unwrap().starts_with("expired: no answer in"), "{}", responds[0].1);
    assert!(calls_of(&script, "approval/respond").iter().all(|(_, p)| p["decision"] != "approve"));
    // N1: a late call of the interrupted turn never runs.
    notify(&script, "peer/tool/call", tool_call_params(&slug, "c-late", "turn-i1", None));
    wait_for("the refusal", || calls_of(&script, "peer/tool/result").iter().any(|(_, p)| p["call_id"] == "c-late"));
    let late = calls_of(&script, "peer/tool/result").into_iter().find(|(_, p)| p["call_id"] == "c-late").unwrap().1;
    assert_eq!(late["error"]["kind"], "turn_interrupted");
    assert!(host.calls.lock().unwrap().is_empty());
    assert_eq!(broker.pending_prompts(), 0);
    drop(broker);
}

/// The host's own deadline (the shell's router counts whole seconds) can
/// deny an approval as expired a moment BEFORE the broker's timer: that is
/// still an expiry, not an answer in time, so the grace runs and the turn
/// still stuck after it is interrupted; the next queued input starts. A
/// deny that is the person's own (no expiry note) is an answer: no
/// interrupt (`an_answered_approval_neither_expires_nor_interrupts`).
#[test]
fn a_host_expiry_just_before_the_brokers_deadline_still_frees_the_stuck_turn() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_timed(&ALL, Some(host.clone()), None, Some((400, 300)));
    let (_slug, session) = busy_peer(&broker, &script);
    notify(&script, "approval/requested", approval_on(&session, "a1", "turn-i1"));
    wait_for("the host", || host.approvals.lock().unwrap().len() == 1);
    // The host expires it at once, ahead of the broker's 400 ms.
    let answer = host.approvals.lock().unwrap()[0].1.clone();
    assert!(answer.respond_with(false, &octosense_app_peers::host_tools::expired_note("no answer in 1 s")));
    assert!(answer.expired());
    wait_for("the interrupt", || position(&script, "turn/interrupt").is_some());
    assert_eq!(calls_of(&script, "turn/interrupt")[0].1["turn_id"], "turn-i1");
    wait_for("the next queued turn", || calls_of(&script, "turn/start").len() == 2);
    assert_eq!(calls_of(&script, "approval/respond").len(), 1, "answered once, by the host");
    assert_eq!(broker.pending_prompts(), 0);
    drop(broker);
}

#[test]
fn a_host_that_never_answers_is_answered_for_with_a_deny_after_the_grace() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_timed(&ALL, Some(host.clone()), None, Some((200, 300)));
    let (_slug, session) = busy_peer(&broker, &script);
    notify(&script, "approval/requested", approval_on(&session, "a1", "turn-i1"));
    wait_for("the host", || host.approvals.lock().unwrap().len() == 1);
    wait_for("the broker's deny", || position(&script, "approval/respond").is_some());
    let respond = calls_of(&script, "approval/respond")[0].1.clone();
    assert_eq!((respond["approval_id"].as_str(), respond["decision"].as_str()), (Some("a1"), Some("deny")));
    assert!(!host.approvals.lock().unwrap()[0].1.respond(true), "the host can no longer approve it");
    wait_for("the interrupt", || position(&script, "turn/interrupt").is_some());
    wait_for("the next queued turn", || calls_of(&script, "turn/start").len() == 2);
    drop(broker);
}

#[test]
fn a_question_nobody_answers_expires_declined_never_with_an_option_chosen() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_timed(&ALL, Some(host.clone()), None, Some((300, 300)));
    script.lock().unwrap().interrupts_end = true;
    let (_slug, session) = busy_peer(&broker, &script);
    notify(&script, "user_question/requested", json!({"session_id": session, "question_id": "q1", "turn_id": "turn-i1", "title": "Which room?", "body": "",
        "questions": [{"header": "Room", "question": "Post where?", "options": [{"label": "#a", "description": ""}, {"label": "#b", "description": ""}]}, {"header": "When", "question": "Now?", "options": [{"label": "yes", "description": ""}, {"label": "no", "description": ""}]}]}));
    wait_for("the host", || host.questions.lock().unwrap().len() == 1);
    // The host never answers: the broker declines it after the grace, one
    // free-text answer per question, then interrupts the turn.
    wait_for("the decline", || position(&script, "user_question/respond").is_some());
    let respond = calls_of(&script, "user_question/respond")[0].1.clone();
    assert_eq!(respond["question_id"], "q1");
    let answers = respond["answers"].as_array().unwrap();
    assert_eq!(answers.len(), 2);
    assert!(answers.iter().all(|a| a.get("selected_labels").is_none() && a["free_text"].as_str().unwrap().contains("expired")), "{respond}");
    assert!(respond["client_note"].as_str().unwrap().contains("no answer in"));
    wait_for("the interrupt", || position(&script, "turn/interrupt").is_some());
    // The kernel's interrupted terminal frees the peer: the next input runs.
    wait_for("the next queued turn", || calls_of(&script, "turn/start").len() == 2);
    wait_for("closed", || host.closed_questions.lock().unwrap().contains(&"q1".to_string()));

    // Asked in a request context of a host that takes no questions: the
    // broker declines it at the deadline itself, and the context hears it.
    let (plain, script) = new_broker_timed(&ALL, None, None, Some((200, 5_000)));
    script.lock().unwrap().hold_turns = true;
    plain.set_account(Some("@a:x"));
    let ctx = plain.open_context(spec("@a:x", "mini#1", &ALL)).unwrap();
    let (sink, rx) = collect();
    ctx.call(ContextOp::Turn { text: "post it".into() }, sink).unwrap();
    wait_for("the turn", || position(&script, "turn/start").is_some());
    let start = calls_of(&script, "turn/start")[0].1.clone();
    notify(&script, "user_question/requested", json!({"session_id": start["session_id"], "question_id": "q2", "turn_id": start["turn_id"], "title": "?", "body": "", "questions": [{"header": "h", "question": "q", "options": []}]}));
    wait_for("the decline", || position(&script, "user_question/respond").is_some());
    assert!(position(&script, "turn/interrupt").is_none(), "the grace has not passed");
    let seen = events(&rx, Duration::from_millis(300));
    assert!(seen.iter().any(|d| d["method"] == "prompt/expired" && d["params"]["id"] == "q2"), "{seen:?}");
    drop(plain);
    drop(broker);
}

#[test]
fn an_answered_approval_neither_expires_nor_interrupts() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_timed(&ALL, Some(host.clone()), None, Some((300, 200)));
    let (_slug, session) = busy_peer(&broker, &script);
    notify(&script, "approval/requested", approval_on(&session, "a1", "turn-i1"));
    wait_for("the host", || host.approvals.lock().unwrap().len() == 1);
    assert!(host.approvals.lock().unwrap()[0].1.respond(true));
    std::thread::sleep(Duration::from_millis(900));
    let responds = calls_of(&script, "approval/respond");
    assert_eq!(responds.len(), 1);
    assert_eq!(responds[0].1["decision"], "approve");
    assert!(position(&script, "turn/interrupt").is_none());
    assert_eq!(broker.pending_prompts(), 0);
    drop(broker);
}

/// Approvals and questions of sessions that are not this app's peer or its
/// contexts (an external client's, octos#2624, G1) are not the shell's:
/// no deadline, no answer, no interrupt.
#[test]
fn prompts_of_an_external_clients_session_never_expire_here() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_timed(&ALL, Some(host.clone()), None, Some((150, 150)));
    let _ = busy_peer(&broker, &script);
    for session in ["_main:api:octosense#system", "_main:api:talk-to-octos#s1"] {
        notify(&script, "approval/requested", approval_on(session, &format!("ext-{session}"), "turn-ext"));
        notify(&script, "user_question/requested", json!({"session_id": session, "question_id": format!("q-{session}"), "turn_id": "turn-ext", "title": "?", "body": "", "questions": []}));
    }
    std::thread::sleep(Duration::from_millis(800));
    assert!(host.approvals.lock().unwrap().is_empty() && host.questions.lock().unwrap().is_empty());
    assert_eq!(broker.pending_prompts(), 0);
    assert!(position(&script, "approval/respond").is_none());
    assert!(position(&script, "user_question/respond").is_none());
    assert!(position(&script, "turn/interrupt").is_none());
    drop(broker);
}

/// The person's Stop on the shared conversation ends whichever turn runs
/// there, the system agent's included; its late calls are refused and the
/// next queued turn starts. The shell's own surfaces reach it by app.
/// Stop on the app's conversation stops BOTH lanes: the person's running
/// turn in its context and the system agent's on the peer's session (the
/// person owns the device); the peer's next queued input then starts, and
/// the stopped turns' late calls are refused. The shell's Stop by app id
/// does the same.
#[test]
fn stop_on_the_conversation_interrupts_both_lanes() {
    let host = Arc::new(RecordingHost::default());
    // Its own app id: the registry is the process's, and tests share one.
    let (broker, script) = new_broker_app("stop-test", &ALL, Some(host.clone()), None, None);
    let (slug, session) = busy_peer(&broker, &script);
    script.lock().unwrap().interrupts_end = true;
    let chat = broker.open_conversation(spec("@a:x", "ui", &ALL)).unwrap();
    let (sink, person_rx) = collect();
    chat.call(ContextOp::TurnFrom { text: "long job".into(), trigger: TurnTrigger::Person }, sink).unwrap();
    wait_for("the person's turn", || calls_of(&script, "turn/start").len() == 2);
    let (_, lane) = person_lane(&script, 0);
    let person_turn = calls_of(&script, "turn/start")[1].1["turn_id"].as_str().unwrap().to_owned();
    let (sink, rx) = collect();
    chat.call(ContextOp::Interrupt, sink).unwrap();
    let stopped = complete(&rx).unwrap();
    assert_eq!(stopped["interrupted"], json!([person_turn, "turn-i1"]));
    assert_eq!(stopped["turns"][0]["lane"], "person");
    assert_eq!(stopped["turns"][0]["speaker"]["kind"], "person");
    assert_eq!(stopped["turns"][1]["lane"], "system_agent");
    assert_eq!(stopped["turns"][1]["speaker"]["kind"], "system_agent");
    let interrupts: Vec<Value> = calls_of(&script, "turn/interrupt").into_iter().map(|(_, p)| p).collect();
    assert!(interrupts.contains(&json!({"session_id": lane, "turn_id": person_turn})), "{interrupts:?}");
    assert!(interrupts.contains(&json!({"session_id": session, "turn_id": "turn-i1"})), "{interrupts:?}");
    assert!(complete(&person_rx).is_err(), "the person's turn ended interrupted");
    wait_for("the next queued input", || calls_of(&script, "turn/start").len() == 3);
    assert_eq!(calls_of(&script, "turn/start")[2].1["turn_id"], "turn-i2");
    notify(&script, "peer/tool/call", tool_call_params(&slug, "c-late", "turn-i1", None));
    wait_for("the refusal", || calls_of(&script, "peer/tool/result").iter().any(|(_, p)| p["call_id"] == "c-late"));
    // The shell's Stop, by app id: both lanes again (the person's lane is
    // idle now, so only the input running).
    let stopped = octosense_app_peers::broker::interrupt_where(|app| app == "stop-test");
    assert_eq!(stopped, ["turn-i2"]);
    wait_for("the third interrupt", || calls_of(&script, "turn/interrupt").len() == 3);
    assert!(octosense_app_peers::broker::interrupt_where(|app| app == "other").is_empty());
    // Nothing running: said so.
    std::thread::sleep(Duration::from_millis(300));
    let (sink, rx) = collect();
    chat.call(ContextOp::Interrupt, sink).unwrap();
    assert!(complete(&rx).unwrap_err().contains("Nothing is running"));
    drop(broker);
}

/// The shell's "Ask <app>" panel stops one lane at a time: its Stop ends
/// the person's own turn and leaves the system agent's running; the system
/// agent's is stopped only on its own control. Late calls of a stopped turn
/// are refused as with any Stop.
#[test]
fn a_lane_stop_leaves_the_other_lane_running() {
    let host = Arc::new(RecordingHost::default());
    let (broker, script) = new_broker_app("lane-stop-test", &ALL, Some(host.clone()), None, None);
    let (slug, session) = busy_peer(&broker, &script);
    script.lock().unwrap().interrupts_end = true;
    let chat = broker.open_conversation(spec("@a:x", "ui", &ALL)).unwrap();
    let (sink, person_rx) = collect();
    chat.call(ContextOp::TurnFrom { text: "long job".into(), trigger: TurnTrigger::Person }, sink).unwrap();
    wait_for("the person's turn", || calls_of(&script, "turn/start").len() == 2);
    let (_, lane) = person_lane(&script, 0);
    let person_turn = calls_of(&script, "turn/start")[1].1["turn_id"].as_str().unwrap().to_owned();
    let mine = |app: &str| app == "lane-stop-test";

    let stopped = octosense_app_peers::broker::interrupt_lane_where(mine, octosense_app_peers::broker::LANE_PERSON);
    assert_eq!(stopped, std::slice::from_ref(&person_turn));
    wait_for("the person's interrupt", || calls_of(&script, "turn/interrupt").len() == 1);
    assert_eq!(calls_of(&script, "turn/interrupt")[0].1, json!({"session_id": lane, "turn_id": person_turn}));
    assert!(complete(&person_rx).is_err(), "the person's turn ended interrupted");
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(calls_of(&script, "turn/interrupt").len(), 1, "the system agent's turn goes on");
    assert_eq!(broker.peer_active_turn().as_deref(), Some("turn-i1"));

    let stopped = octosense_app_peers::broker::interrupt_lane_where(mine, octosense_app_peers::broker::LANE_SYSTEM_AGENT);
    assert_eq!(stopped, ["turn-i1"]);
    wait_for("the system agent's interrupt", || calls_of(&script, "turn/interrupt").len() == 2);
    assert_eq!(calls_of(&script, "turn/interrupt")[1].1, json!({"session_id": session, "turn_id": "turn-i1"}));
    notify(&script, "peer/tool/call", tool_call_params(&slug, "c-late", "turn-i1", None));
    wait_for("the refusal", || calls_of(&script, "peer/tool/result").iter().any(|(_, p)| p["call_id"] == "c-late"));
    assert!(octosense_app_peers::broker::interrupt_lane_where(|app| app == "other", octosense_app_peers::broker::LANE_PERSON).is_empty());
    drop(broker);
}

/// The person's lane keeps #167's deadlines: an approval in the person's
/// turn that nobody answers expires (denied, never approved), the app's
/// conversation hears it, and the stuck turn is interrupted after the
/// grace; the system agent's lane is untouched.
#[test]
fn an_unanswered_approval_in_the_persons_lane_expires_and_its_turn_is_interrupted() {
    let (broker, script) = new_broker_timed(&ALL, None, None, Some((300, 600)));
    script.lock().unwrap().hold_turns = true;
    broker.set_account(Some("@a:x"));
    let chat = broker.open_conversation(spec("@a:x", "ui", &ALL)).unwrap();
    let (follow, follow_rx) = collect();
    chat.subscribe(Some(follow));
    let (sink, person_rx) = collect();
    chat.call(ContextOp::TurnFrom { text: "write it".into(), trigger: TurnTrigger::Person }, sink).unwrap();
    wait_for("the person's turn", || calls_of(&script, "turn/start").len() == 1);
    let (_, lane) = person_lane(&script, 0);
    let turn = calls_of(&script, "turn/start")[0].1["turn_id"].as_str().unwrap().to_owned();
    notify(&script, "approval/requested", approval_on(&lane, "a1", &turn));
    wait_for("tracked", || broker.pending_prompts() == 1);
    let expired = loop {
        match follow_rx.recv_timeout(Duration::from_secs(5)).expect("prompt/expired") {
            ContextEvent::Data(d) if d["method"] == "prompt/expired" => break d,
            _ => {}
        }
    };
    assert_eq!(expired["params"]["id"], "a1");
    wait_for("the deny", || position(&script, "approval/respond").is_some());
    let respond = calls_of(&script, "approval/respond")[0].1.clone();
    assert_eq!((respond["session_id"].as_str(), respond["decision"].as_str()), (Some(lane.as_str()), Some("deny")));
    wait_for("the interrupt", || position(&script, "turn/interrupt").is_some());
    assert_eq!(calls_of(&script, "turn/interrupt")[0].1, json!({"session_id": lane, "turn_id": turn}));
    notify(&script, "turn/error", json!({"session_id": lane, "turn_id": turn, "message": "interrupted"}));
    assert!(complete(&person_rx).is_err());
    assert_eq!(broker.pending_prompts(), 0);
    assert!(broker.peer_active_turn().is_none(), "the system agent's lane was never involved");
    drop(broker);
}
