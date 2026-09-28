//! Host-registered peer tools (octos UPCR-2026-035) against a scripted
//! kernel: the registration rules of OctoSense issue #62 and the routing of
//! `peer/tool/call`.
#![cfg(feature = "peer-tools")]

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use octosense_app_peers::broker::{BoxFuture, Broker, BrokerConfig, Connector, Link};
use octosense_app_peers::peer_tools::{
    Cancel, HostTools, Registration, ToolCall, ToolError, ToolFuture,
};
use octosense_app_peers::*;
use serde_json::{json, Value};
use tokio::sync::mpsc;

const TOKEN: &str = "fixture-host-token";
const CLOSE: &str = "__close__";

#[derive(Default)]
struct Script {
    calls: Vec<(String, Value)>,
    fail_register: bool,
    /// The newest link's kernel → host sender.
    out: Option<mpsc::UnboundedSender<String>>,
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
            match self.from_kernel.recv().await {
                Some(frame) if frame == CLOSE => Err("closed".to_owned()),
                Some(frame) => Ok(frame),
                None => Err("closed".to_owned()),
            }
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
            let (to_kernel, mut kernel_in) = mpsc::unbounded_channel::<String>();
            let (kernel_out, from_kernel) = mpsc::unbounded_channel::<String>();
            script.lock().unwrap().out = Some(kernel_out.clone());
            tokio::spawn(async move {
                while let Some(frame) = kernel_in.recv().await {
                    let frame: Value = serde_json::from_str(&frame).unwrap();
                    let method = frame["method"].as_str().unwrap().to_owned();
                    let params = frame["params"].clone();
                    let id = frame["id"].clone();
                    let fail_register = {
                        let mut s = script.lock().unwrap();
                        s.calls.push((method.clone(), params.clone()));
                        s.fail_register
                    };
                    let reply = |result: Value| {
                        json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
                    };
                    let error = |kind: &str| {
                        json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "refused", "data": {"kind": kind}}})
                            .to_string()
                    };
                    let answer = match method.as_str() {
                        "peer/prepare" => {
                            let mut result = json!({"slug": "news-1", "cwd": "/kernel/ws",
                                "memory_namespace": params["memory_namespace"]});
                            if params.get("host_token").is_none() {
                                result["host_token"] = json!(TOKEN);
                            }
                            reply(result)
                        }
                        "peer/tools/register" if fail_register => error("method_not_found"),
                        "peer/tools/register" if params["host_token"] != TOKEN => {
                            error("peer_host_token_mismatch")
                        }
                        "peer/tools/register" => {
                            reply(json!({"slug": "news-1", "version": 1, "applies": "next_turn"}))
                        }
                        "peer/context/open" => {
                            let session = format!(
                                "_main:api:octosense#peerctx-news-1.{}",
                                params["context_id"].as_str().unwrap()
                            );
                            reply(json!({"session_id": session}))
                        }
                        "turn/start" => {
                            let note = json!({"jsonrpc": "2.0", "method": "turn/completed",
                                "params": {"session_id": params["session_id"], "turn_id": params["turn_id"]}});
                            let _ = kernel_out.send(reply(json!({"accepted": true})));
                            note.to_string()
                        }
                        "session/hydrate" => reply(json!({"messages": []})),
                        "peer/tool/result" => {
                            reply(json!({"call_id": params["call_id"], "accepted": true}))
                        }
                        _ => reply(json!({})),
                    };
                    let _ = kernel_out.send(answer);
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

/// `demo.echo` answers its arguments; `demo.slow` waits for its cancel.
#[derive(Default)]
struct Demo {
    runs: AtomicUsize,
    cancelled: AtomicUsize,
}

struct DemoTools(Arc<Demo>);

impl HostTools for DemoTools {
    fn registration(&self) -> Registration {
        Registration {
            tools: vec![
                json!({"name": "demo.echo", "description": "Echo.", "input_schema": {"type": "object"}, "risk": "read", "background": true}),
                json!({"name": "demo.slow", "description": "Slow.", "input_schema": {"type": "object"}, "risk": "read", "background": true}),
            ],
            generic_tools: Vec::new(),
            call_timeout_ms: Some(300_000),
            max_result_bytes: None,
        }
    }
    fn call(&self, call: ToolCall, cancel: Cancel) -> ToolFuture {
        let demo = self.0.clone();
        demo.runs.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            match call.name.as_str() {
                "demo.echo" => Ok(json!({"echo": call.args, "context": call.context_id})),
                _ => {
                    struct Seen(Arc<Demo>, Cancel);
                    impl Drop for Seen {
                        fn drop(&mut self) {
                            if self.1.is_cancelled() {
                                self.0.cancelled.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                    }
                    let _seen = Seen(demo.clone(), cancel.clone());
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    Err(ToolError::new("late", "should never be answered"))
                }
            }
        })
    }
}

fn broker(tools: Option<Arc<dyn HostTools>>) -> (Broker, Arc<Mutex<Script>>) {
    let script = Arc::new(Mutex::new(Script::default()));
    let services: BTreeSet<String> = OCTOS_SERVICES.iter().map(|s| s.to_string()).collect();
    let mut cfg = BrokerConfig::new(
        Deployment::Hosted,
        "_main",
        "_main:api:octosense#system",
        "news",
        "News",
        services,
    );
    cfg.host_tools = tools;
    (
        Broker::new(cfg, Arc::new(FakeConnector(script.clone()))),
        script,
    )
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

fn position(methods: &[String], method: &str, nth: usize) -> usize {
    methods
        .iter()
        .enumerate()
        .filter(|(_, m)| *m == method)
        .nth(nth)
        .unwrap_or_else(|| panic!("no {method} #{nth} in {methods:?}"))
        .0
}

fn turn(broker: &Broker, text: &str) -> Result<Value, String> {
    let ctx = broker
        .open_context(ContextSpec {
            account: "device".into(),
            instance: "news#1".into(),
            services: OCTOS_SERVICES.iter().map(|s| s.to_string()).collect(),
        })
        .unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = Mutex::new(tx);
    ctx.call(
        ContextOp::Turn { text: text.into() },
        Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }),
    )
    .unwrap();
    loop {
        match rx
            .recv_timeout(Duration::from_secs(10))
            .expect("a completion")
        {
            ContextEvent::Complete(r) => return r,
            ContextEvent::Data(_) => continue,
        }
    }
}

fn inject(script: &Arc<Mutex<Script>>, method: &str, params: Value) {
    let out = script.lock().unwrap().out.clone().expect("a link");
    out.send(json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string())
        .unwrap();
}

fn tool_call(call_id: &str, tool_call_id: &str, name: &str, args: Value, timeout_ms: u64) -> Value {
    json!({"peer": "news-1", "session_id": "_main:api:octosense#peer-news-1", "context_id": null,
           "turn_id": "turn-1", "call_id": call_id, "tool_call_id": tool_call_id, "args_digest": "d1",
           "name": name, "args": args, "risk": "read", "confirm_required": false,
           "timeout_ms": timeout_ms, "tools_version": 1})
}

/// The `peer/tool/result`s sent so far, by call id.
fn results(script: &Arc<Mutex<Script>>) -> Vec<Value> {
    script
        .lock()
        .unwrap()
        .calls
        .iter()
        .filter(|(m, _)| m == "peer/tool/result")
        .map(|(_, p)| p.clone())
        .collect()
}

fn wait_for_result(script: &Arc<Mutex<Script>>, call_id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(r) = results(script)
            .into_iter()
            .find(|r| r["call_id"] == call_id)
        {
            return r;
        }
        assert!(Instant::now() < deadline, "no result for {call_id}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn bound(tools: Option<Arc<dyn HostTools>>) -> (Broker, Arc<Mutex<Script>>) {
    let (broker, script) = broker(tools);
    broker.set_account(Some("device"));
    broker.bind().unwrap();
    (broker, script)
}

#[test]
fn every_prepare_registers_before_any_turn_and_again_after_a_reconnect() {
    let (broker, script) = broker(None);
    broker.set_account(Some("device"));
    turn(&broker, "hello").unwrap();
    let seen = methods(&script);
    let prepare = position(&seen, "peer/prepare", 0);
    let register = position(&seen, "peer/tools/register", 0);
    assert_eq!(register, prepare + 1, "right after prepare: {seen:?}");
    assert!(register < position(&seen, "peer/context/open", 0));
    assert!(register < position(&seen, "turn/start", 0));
    // No tools: the empty set, with the peer's token, on the same link.
    let (_, params) = script.lock().unwrap().calls[register].clone();
    assert_eq!(params["tools"], json!([]));
    assert_eq!(params["generic_tools"], json!([]));
    assert_eq!(params["peer"], "news-1");
    assert_eq!(params["host_token"], TOKEN);
    assert_eq!(params["session_id"], "_main:api:octosense#system");

    // The link drops: the next turn prepares and registers again first.
    let out = script.lock().unwrap().out.clone().unwrap();
    out.send(CLOSE.into()).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let before = methods(&script).len();
    let result = turn(&broker, "again");
    let result = match result {
        Ok(r) => r,
        // The context was bound to the old link: a fresh one is opened.
        Err(_) => turn(&broker, "again").unwrap(),
    };
    assert!(result["turn_id"].is_string());
    let seen = methods(&script);
    let after: Vec<String> = seen[before..].to_vec();
    let prepare = position(&after, "peer/prepare", 0);
    let register = position(&after, "peer/tools/register", 0);
    assert_eq!(register, prepare + 1, "{after:?}");
    assert!(register < position(&after, "turn/start", 0), "{after:?}");
}

#[test]
fn a_failed_registration_starts_no_turn_and_says_so() {
    let (broker, script) = broker(None);
    script.lock().unwrap().fail_register = true;
    broker.set_account(Some("device"));
    let error = turn(&broker, "hello").unwrap_err();
    assert!(error.contains("could not take this app's tools"), "{error}");
    let seen = methods(&script);
    assert!(seen.contains(&"peer/tools/register".to_owned()));
    assert!(!seen.contains(&"turn/start".to_owned()), "{seen:?}");
    assert!(!seen.contains(&"peer/context/open".to_owned()), "{seen:?}");
    match broker.availability() {
        Availability::Failed(why) => assert!(why.contains("tools"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(broker.bind().is_err());
}

#[test]
fn the_apps_tools_are_registered_and_a_call_is_answered_by_the_host() {
    let demo = Arc::new(Demo::default());
    let (_broker, script) = bound(Some(Arc::new(DemoTools(demo.clone()))));
    let register = script
        .lock()
        .unwrap()
        .calls
        .iter()
        .find(|(m, _)| m == "peer/tools/register")
        .unwrap()
        .1
        .clone();
    let names: Vec<&str> = register["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["demo.echo", "demo.slow"]);
    assert_eq!(register["call_timeout_ms"], 300_000);

    inject(
        &script,
        "peer/tool/call",
        tool_call("c1", "tc1", "demo.echo", json!({"q": 1}), 5000),
    );
    let result = wait_for_result(&script, "c1");
    assert_eq!(result["ok"], true);
    assert_eq!(result["data"]["echo"], json!({"q": 1}));
    assert_eq!(result["peer"], "news-1");
    assert_eq!(result["host_token"], TOKEN);
    assert_eq!(result["session_id"], "_main:api:octosense#system");

    // A repeat of the same occurrence is answered with the first result
    // and never run again; a new tool-call id is a new occurrence.
    inject(
        &script,
        "peer/tool/call",
        tool_call("c2", "tc1", "demo.echo", json!({"q": 1}), 5000),
    );
    assert_eq!(
        wait_for_result(&script, "c2")["data"]["echo"],
        json!({"q": 1})
    );
    assert_eq!(demo.runs.load(Ordering::SeqCst), 1);
    inject(
        &script,
        "peer/tool/call",
        tool_call("c3", "tc2", "demo.echo", json!({"q": 2}), 5000),
    );
    assert_eq!(
        wait_for_result(&script, "c3")["data"]["echo"],
        json!({"q": 2})
    );
    assert_eq!(demo.runs.load(Ordering::SeqCst), 2);
}

#[test]
fn a_tool_that_was_not_registered_is_refused_without_running() {
    let demo = Arc::new(Demo::default());
    let (_broker, script) = bound(Some(Arc::new(DemoTools(demo.clone()))));
    inject(
        &script,
        "peer/tool/call",
        tool_call(
            "c1",
            "tc1",
            "toolbox.deep_crawl",
            json!({"url": "https://example.org/"}),
            5000,
        ),
    );
    let result = wait_for_result(&script, "c1");
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"]["kind"], "not_registered");
    assert_eq!(demo.runs.load(Ordering::SeqCst), 0);
    // A call for another peer is not this broker's to answer.
    let mut other = tool_call("c2", "tc2", "demo.echo", json!({}), 5000);
    other["peer"] = json!("mail-1");
    inject(&script, "peer/tool/call", other);
    std::thread::sleep(Duration::from_millis(200));
    assert!(results(&script).iter().all(|r| r["call_id"] != "c2"));
    assert_eq!(demo.runs.load(Ordering::SeqCst), 0);
}

#[test]
fn a_cancelled_or_timed_out_call_is_stopped_and_never_answered() {
    let demo = Arc::new(Demo::default());
    let (_broker, script) = bound(Some(Arc::new(DemoTools(demo.clone()))));
    inject(
        &script,
        "peer/tool/call",
        tool_call("c1", "tc1", "demo.slow", json!({}), 60_000),
    );
    std::thread::sleep(Duration::from_millis(100));
    inject(
        &script,
        "peer/tool/cancel",
        json!({"call_id": "c1", "reason": "cancelled"}),
    );
    // Past its deadline: the kernel no longer waits.
    inject(
        &script,
        "peer/tool/call",
        tool_call("c2", "tc2", "demo.slow", json!({}), 150),
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while demo.cancelled.load(Ordering::SeqCst) < 2 {
        assert!(Instant::now() < deadline, "both calls stop");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(200));
    assert!(results(&script).is_empty(), "{:?}", results(&script));
    assert_eq!(demo.runs.load(Ordering::SeqCst), 2);
}

#[test]
fn releasing_the_app_stops_its_calls() {
    let demo = Arc::new(Demo::default());
    let (broker, script) = bound(Some(Arc::new(DemoTools(demo.clone()))));
    inject(
        &script,
        "peer/tool/call",
        tool_call("c1", "tc1", "demo.slow", json!({}), 60_000),
    );
    std::thread::sleep(Duration::from_millis(100));
    broker.release();
    let deadline = Instant::now() + Duration::from_secs(3);
    while demo.cancelled.load(Ordering::SeqCst) < 1 {
        assert!(Instant::now() < deadline, "the call stops");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(results(&script).is_empty());
}
