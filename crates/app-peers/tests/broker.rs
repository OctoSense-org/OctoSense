//! The broker against a scripted kernel speaking the UPCR-2026-034 subset.
#![cfg(feature = "broker")]

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use octosense_app_peers::broker::{BoxFuture, Broker, BrokerConfig, Connector, Link};
use octosense_app_peers::*;
use serde_json::{json, Value};
use tokio::sync::mpsc;

/// What the scripted kernel saw and how it behaves.
#[derive(Default)]
struct Script {
    calls: Vec<(String, Value)>,
    /// peer/prepare ignores the host binding (a pre-UPCR kernel).
    legacy: bool,
    /// turn/start never completes on its own.
    hold_turns: bool,
    connects: usize,
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

impl Connector for FakeConnector {
    fn available(&self) -> Result<(), String> {
        Ok(())
    }
    fn connect(&self) -> BoxFuture<'static, Result<Box<dyn Link>, String>> {
        let script = self.0.clone();
        Box::pin(async move {
            script.lock().unwrap().connects += 1;
            let (to_kernel, mut kernel_in) = mpsc::unbounded_channel::<String>();
            let (kernel_out, from_kernel) = mpsc::unbounded_channel::<String>();
            tokio::spawn(async move {
                while let Some(frame) = kernel_in.recv().await {
                    let frame: Value = serde_json::from_str(&frame).unwrap();
                    let method = frame["method"].as_str().unwrap().to_owned();
                    let params = frame["params"].clone();
                    let id = frame["id"].clone();
                    let (legacy, hold) = {
                        let mut s = script.lock().unwrap();
                        s.calls.push((method.clone(), params.clone()));
                        (s.legacy, s.hold_turns)
                    };
                    let reply = |result: Value| {
                        json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
                    };
                    match method.as_str() {
                        "peer/prepare" => {
                            let name = params["names"][0]
                                .as_str()
                                .unwrap()
                                .to_lowercase()
                                .replace(' ', "-");
                            let mut result = json!({"slug": name, "cwd": "/kernel/ws", "model": {"lane": "primary"}});
                            if !legacy {
                                result["memory_namespace"] = params["memory_namespace"].clone();
                                result["resumed"] = json!(false);
                            }
                            let _ = kernel_out.send(reply(result));
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
                            let _ = kernel_out
                                .send(reply(json!({"session_id": session, "created": true})));
                        }
                        "turn/start" => {
                            let session = params["session_id"].clone();
                            let turn = params["turn_id"].clone();
                            let _ = kernel_out.send(reply(json!({"accepted": true})));
                            let note = |method: &str, extra: Value| {
                                let mut p = json!({"session_id": session, "turn_id": turn});
                                for (k, v) in extra.as_object().unwrap() {
                                    p[k] = v.clone();
                                }
                                json!({"jsonrpc": "2.0", "method": method, "params": p}).to_string()
                            };
                            let _ =
                                kernel_out.send(note("message/delta", json!({"text": "Hello "})));
                            let _ =
                                kernel_out.send(note("message/delta", json!({"text": "there"})));
                            if !hold {
                                let _ = kernel_out.send(note("turn/completed", json!({})));
                            }
                        }
                        "session/hydrate" => {
                            let _ = kernel_out.send(reply(json!({"messages": []})));
                        }
                        _ => {
                            let _ = kernel_out.send(reply(json!({})));
                        }
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
    let script = Arc::new(Mutex::new(Script::default()));
    let cfg = BrokerConfig::new(
        Deployment::Hosted,
        "_main",
        "_main:api:octosense#system",
        "rinx",
        "Rinx",
        services.iter().map(|s| s.to_string()).collect(),
    );
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
