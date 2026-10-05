//! Sharing one kernel frame stream between native consumers.
//!
//! The WebSocket adapter (or private stdio/embedded mode) presents NDJSON JSON-RPC.
//! Each consumer speaks the same protocol as if it owned that stream; the
//! router keeps them apart:
//!
//! - a consumer's **request** gets a kernel-unique id (`k<n>`) on the way in;
//!   the **response** gets the consumer's own id back and goes only to it;
//! - a **notification** goes to the consumers that named its `session_id`
//!   (in any request's params, or as the session a `session/open` returned);
//!   one for a session nobody named goes to every consumer, which is what a
//!   single consumer always saw;
//! - frames without an id to rewrite (a consumer's notification, its
//!   response to a kernel request) pass through untouched.
//!
//! Only the system workspace is persisted: Web opens it with an explicit cwd,
//! so native clients must send the same cwd when resuming after a restart.
//!
//! A **scoped** consumer (an app that is itself an octos client, through its
//! kernel port; ADR 0003) is held to its [`Scope`]: each of its requests is
//! checked first, and one the scope refuses is answered here, never sent;
//! before its first frame about a session the scope may have the router
//! prepare that session (a host request, such as the session's exact tool
//! list), and its frames about the session wait until the kernel confirmed;
//! it hears only about sessions it named in requests its scope allowed (no
//! broadcast of anyone else's), and only the frames its scope lets through,
//! while one its scope keeps from it goes to the unscoped consumers (a
//! host-only frame about its session); its replies pass the scope's filter;
//! and it may answer only the kernel requests it was sent.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use serde_json::{json, Value};

pub(crate) type ConnId = u64;

/// What a scoped consumer may do. The kernel sees the shell's authority on
/// every frame, so this is the whole boundary: refuse by default.
pub trait Scope: Send + Sync {
    /// A request (or notification) from the consumer: `Ok` forwards it,
    /// `params` possibly rewritten; `Err` refuses it with that message.
    fn request(&self, method: &str, params: &mut Value) -> Result<(), String>;
    /// Whether a notification or kernel request the router would deliver to
    /// the consumer reaches it: `session` is the one it is about (a session
    /// the consumer named), `None` for one about no session.
    fn notification(&self, method: &str, session: Option<&str>) -> bool;
    /// The result of the consumer's `method`, filtered in place.
    fn result(&self, method: &str, result: &mut Value);
    /// The params of a notification or kernel request that
    /// [`Scope::notification`] lets through, as the consumer gets them:
    /// `Some` when the scope rewrote them, `None` to pass the frame as it is.
    fn rewrite(&self, _method: &str, _params: &Value) -> Option<Value> {
        None
    }
    /// A host request (`{"method", "params"}`) to make before the consumer's
    /// first frame about `session` in this kernel generation; its frames
    /// about the session wait for the answer, and a failed one refuses them.
    fn prepare(&self, _session: &str) -> Option<Value> {
        None
    }
}

/// The JSON-RPC error a refused request is answered with.
pub const SCOPE_DENIED: i64 = -32003;

/// What becomes of a consumer's frame.
#[derive(Debug, PartialEq)]
pub(crate) enum Routed {
    /// Write it to the kernel.
    Kernel(String),
    /// Answer the consumer with it; the kernel never sees the request.
    Answer(String),
    /// Neither: not JSON, or refused without an id to answer.
    Drop,
    /// Kept until its session is prepared ([`Scope::prepare`]).
    Held,
}

/// Where a kernel frame's consequences go.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Delivery {
    /// Frames for consumers.
    pub(crate) consumers: Vec<(ConnId, String)>,
    /// Frames for the kernel: held frames whose session is now prepared.
    pub(crate) kernel: Vec<String>,
}

struct Pending {
    conn: ConnId,
    id: Value,
    method: String,
}

#[derive(Default)]
pub(crate) struct Router {
    next: u64,
    pending: HashMap<String, Pending>,
    sessions: HashMap<String, BTreeSet<ConnId>>,
    conns: BTreeSet<ConnId>,
    scopes: HashMap<ConnId, Arc<dyn Scope>>,
    /// Kernel requests delivered to scoped consumers, by (consumer, id):
    /// the only responses such a consumer may send.
    asked: HashSet<(ConnId, String)>,
    /// Sessions being prepared for a scoped consumer, by the kernel id of
    /// the host request: (consumer, session).
    preparing: HashMap<String, (ConnId, String)>,
    /// A consumer's frames about a session being prepared, in order.
    held: HashMap<(ConnId, String), Vec<String>>,
    /// Sessions prepared for a consumer in this kernel generation.
    prepared: HashSet<(ConnId, String)>,
    system_workspace_file: Option<std::path::PathBuf>,
}

impl Router {
    pub(crate) fn new(core_dir: &std::path::Path) -> Self {
        Self { system_workspace_file: Some(core_dir.join(crate::network::SYSTEM_WORKSPACE_FILE)), ..Self::default() }
    }
    pub(crate) fn attach(&mut self, conn: ConnId) {
        self.conns.insert(conn);
    }

    /// A consumer held to `scope`.
    pub(crate) fn attach_scoped(&mut self, conn: ConnId, scope: Arc<dyn Scope>) {
        self.conns.insert(conn);
        self.scopes.insert(conn, scope);
    }

    /// A consumer left: forget its requests and sessions.
    pub(crate) fn detach(&mut self, conn: ConnId) {
        self.conns.remove(&conn);
        self.scopes.remove(&conn);
        self.asked.retain(|(c, _)| *c != conn);
        self.preparing.retain(|_, (c, _)| *c != conn);
        self.held.retain(|(c, _), _| *c != conn);
        self.prepared.retain(|(c, _)| *c != conn);
        self.pending.retain(|_, p| p.conn != conn);
        self.sessions.retain(|_, subscribers| {
            subscribers.remove(&conn);
            !subscribers.is_empty()
        });
    }

    /// A consumer's frame: as the kernel should see it, or answered here.
    pub(crate) fn consumer_frame(&mut self, conn: ConnId, text: &str) -> Routed {
        let mut frame: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("octos-core: consumer {conn} sent a frame that is not JSON ({e}); dropped");
                return Routed::Drop;
            }
        };
        if let Some(scope) = self.scopes.get(&conn).cloned() {
            if let Some(refused) = self.scoped_frame(conn, scope.as_ref(), &mut frame) {
                return refused;
            }
            if let Some(waiting) = self.prepare(conn, scope.as_ref(), &frame) {
                return waiting;
            }
        }
        let Some(obj) = frame.as_object_mut() else {
            return Routed::Kernel(text.to_owned());
        };
        if obj.get("method").and_then(Value::as_str) == Some("session/open") {
            if let Some(params) = obj.get_mut("params").and_then(Value::as_object_mut) {
                if params.get("session_id").and_then(Value::as_str) == Some(crate::SYSTEM_SESSION)
                    && !params.contains_key("cwd")
                {
                    if let Some(workspace) = self.system_workspace_file.as_ref()
                        .and_then(|path| std::fs::read_to_string(path).ok())
                        .filter(|path| std::path::Path::new(path).is_absolute())
                    {
                        params.insert("cwd".into(), workspace.into());
                    }
                }
            }
        }
        if let Some(session) = obj.get("params").and_then(session_of) {
            self.subscribe(session, conn);
        }
        let method = obj.get("method").and_then(Value::as_str).map(str::to_owned);
        let scoped = self.scopes.contains_key(&conn);
        match (method, obj.get("id").cloned()) {
            (Some(method), Some(id)) if !id.is_null() => {
                self.next += 1;
                let kernel_id = format!("k{}", self.next);
                obj.insert("id".into(), Value::String(kernel_id.clone()));
                self.pending.insert(kernel_id, Pending { conn, id, method });
                Routed::Kernel(frame.to_string())
            }
            // A scope may have rewritten a notification's params.
            (Some(_), _) if scoped => Routed::Kernel(frame.to_string()),
            _ => Routed::Kernel(text.to_owned()),
        }
    }

    /// A scoped consumer's frame checked: `None` lets it go on (its params
    /// as the scope left them), else what becomes of it instead.
    fn scoped_frame(&mut self, conn: ConnId, scope: &dyn Scope, frame: &mut Value) -> Option<Routed> {
        let Some(obj) = frame.as_object_mut() else {
            return Some(Routed::Drop);
        };
        let id = obj.get("id").filter(|id| !id.is_null()).cloned();
        let Some(method) = obj.get("method").and_then(Value::as_str).map(str::to_owned) else {
            // A response: only to a kernel request this consumer was sent.
            let asked = id.as_ref().map(|id| (conn, id_key(id)));
            return match asked {
                Some(key) if self.asked.remove(&key) => None,
                _ => Some(Routed::Drop),
            };
        };
        let params = obj.entry("params").or_insert_with(|| json!({}));
        match scope.request(&method, params) {
            Ok(()) => None,
            Err(message) => {
                Some(match id {
                    Some(id) => Routed::Answer(
                        json!({"jsonrpc": "2.0", "id": id, "error": {"code": SCOPE_DENIED, "message": message, "data": {"kind": "scope_denied", "method": method}}})
                            .to_string(),
                    ),
                    None => Routed::Drop,
                })
            }
        }
    }

    /// A scoped consumer's frame about a session not yet prepared: the
    /// host request to make first (the frame waits), or `Held` while one is
    /// out; `None` lets it go on.
    fn prepare(&mut self, conn: ConnId, scope: &dyn Scope, frame: &Value) -> Option<Routed> {
        frame.get("method")?;
        let session = frame.get("params").and_then(session_of)?;
        let key = (conn, session);
        if self.prepared.contains(&key) {
            return None;
        }
        if let Some(waiting) = self.held.get_mut(&key) {
            waiting.push(frame.to_string());
            return Some(Routed::Held);
        }
        let Some(mut host) = scope.prepare(&key.1) else {
            self.prepared.insert(key);
            return None;
        };
        self.next += 1;
        let kernel_id = format!("p{}", self.next);
        if let Some(obj) = host.as_object_mut() {
            obj.insert("jsonrpc".into(), "2.0".into());
            obj.insert("id".into(), Value::String(kernel_id.clone()));
        }
        self.held.insert(key.clone(), vec![frame.to_string()]);
        self.preparing.insert(kernel_id, key);
        Some(Routed::Kernel(host.to_string()))
    }

    /// The kernel answered a session's preparation: release its frames, or
    /// refuse them.
    fn prepared_or_refused(&mut self, key: (ConnId, String), answer: &Value) -> Delivery {
        let mut out = Delivery::default();
        let held = self.held.remove(&key).unwrap_or_default();
        let (conn, session) = key;
        if let Some(error) = answer.get("error") {
            let message = format!(
                "the shell could not prepare session {session}: {}",
                error.get("message").and_then(Value::as_str).unwrap_or("refused")
            );
            log::warn!("octos-core: consumer {conn}: {message}");
            for text in held {
                let Ok(frame) = serde_json::from_str::<Value>(&text) else { continue };
                if let Some(id) = frame.get("id").filter(|id| !id.is_null()) {
                    let method = frame.get("method").cloned().unwrap_or(Value::Null);
                    out.consumers.push((
                        conn,
                        json!({"jsonrpc": "2.0", "id": id, "error": {"code": SCOPE_DENIED, "message": message, "data": {"kind": "scope_denied", "method": method}}})
                            .to_string(),
                    ));
                }
            }
            return out;
        }
        self.prepared.insert((conn, session));
        for text in held {
            match self.consumer_frame(conn, &text) {
                Routed::Kernel(frame) => out.kernel.push(frame),
                Routed::Answer(frame) => out.consumers.push((conn, frame)),
                Routed::Drop | Routed::Held => {}
            }
        }
        out
    }

    /// A kernel frame: who gets it, and as what.
    pub(crate) fn kernel_frame(&mut self, text: &str) -> Delivery {
        let mut frame: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) => {
                log::warn!("octos-core: kernel wrote a line that is not JSON; dropped");
                return Delivery::default();
            }
        };
        let Some(obj) = frame.as_object_mut() else {
            return Delivery::default();
        };
        let is_response = obj.get("method").is_none() && obj.contains_key("id");
        if is_response {
            let Some(kernel_id) = obj.get("id").and_then(Value::as_str).map(str::to_owned) else {
                log::warn!("octos-core: kernel response without a routable id; dropped");
                return Delivery::default();
            };
            if let Some(key) = self.preparing.remove(&kernel_id) {
                return self.prepared_or_refused(key, &frame);
            }
            let Some(pending) = self.pending.remove(&kernel_id) else {
                // The consumer left, or a restart dropped the request.
                return Delivery::default();
            };
            if pending.method == "session/open" {
                if let Some(session) = obj
                    .get("result")
                    .and_then(|r| r.get("opened"))
                    .and_then(|o| o.get("session_id"))
                    .and_then(Value::as_str)
                {
                    self.subscribe(session.to_owned(), pending.conn);
                }
            }
            if let Some(scope) = self.scopes.get(&pending.conn) {
                if let Some(result) = obj.get_mut("result") {
                    scope.result(&pending.method, result);
                }
            }
            obj.insert("id".into(), pending.id);
            return Delivery { consumers: vec![(pending.conn, frame.to_string())], kernel: Vec::new() };
        }
        // A notification (or a kernel request): to the consumers that named
        // its session, as far as their scopes let it through. When none
        // takes it (nobody named the session, or only scoped consumers whose
        // scope keeps it from them: a host-only frame about their session),
        // to the unscoped consumers; a scoped consumer gets a frame about no
        // session only if its scope lets it through.
        let method = obj.get("method").and_then(Value::as_str).unwrap_or_default().to_owned();
        let session = obj.get("params").and_then(session_of);
        let lets_through = |conn: &ConnId| {
            self.scopes.get(conn).is_none_or(|scope| scope.notification(&method, session.as_deref()))
        };
        let named = session.as_deref().and_then(|s| self.subscribers(s)).unwrap_or_default();
        let mut targets: Vec<ConnId> = named.into_iter().filter(|c| lets_through(c)).collect();
        if targets.is_empty() {
            targets = self
                .conns
                .iter()
                .copied()
                .filter(|c| !self.scopes.contains_key(c) || (session.is_none() && lets_through(c)))
                .collect();
        }
        let request_id = obj.get("id").filter(|id| !id.is_null()).map(id_key);
        let mut out = Delivery::default();
        for conn in targets {
            let mut line = text.to_owned();
            let Some(scope) = self.scopes.get(&conn) else {
                out.consumers.push((conn, line));
                continue;
            };
            if let Some(params) = frame.get("params").and_then(|p| scope.rewrite(&method, p)) {
                let mut rewritten = frame.clone();
                rewritten["params"] = params;
                line = rewritten.to_string();
            }
            if let Some(id) = &request_id {
                self.asked.insert((conn, id.clone()));
            }
            out.consumers.push((conn, line));
        }
        out
    }

    fn subscribe(&mut self, session: String, conn: ConnId) {
        self.sessions.entry(session).or_default().insert(conn);
    }

    /// Consumers of `session`, or of the session it is a topic of
    /// (`<session>#<topic>`).
    fn subscribers(&self, session: &str) -> Option<Vec<ConnId>> {
        if let Some(subs) = self.sessions.get(session) {
            return Some(subs.iter().copied().collect());
        }
        let (base, _) = session.split_once('#')?;
        self.sessions.get(base).map(|s| s.iter().copied().collect())
    }
}

fn session_of(params: &Value) -> Option<String> {
    params.get("session_id").and_then(Value::as_str).map(str::to_owned)
}

/// What a refusal the router answered says, for the log: the method and the
/// scope's reason.
pub(crate) fn refusal_note(frame: &str) -> String {
    let v: Value = serde_json::from_str(frame).unwrap_or_default();
    let error = &v["error"];
    let method = error["data"]["method"].as_str().unwrap_or("?");
    format!("refused {method}: {}", error["message"].as_str().unwrap_or("?"))
}

/// A JSON-RPC id as a key (a string id or a number's text).
fn id_key(id: &Value) -> String {
    id.as_str().map(str::to_owned).unwrap_or_else(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    /// What the consumers get of a kernel frame.
    fn to(r: &mut Router, text: &str) -> Vec<(ConnId, String)> {
        r.kernel_frame(text).consumers
    }

    /// The frame the kernel gets.
    fn kernel(routed: Routed) -> String {
        match routed {
            Routed::Kernel(frame) => frame,
            other => panic!("expected a frame for the kernel, got {other:?}"),
        }
    }

    #[test]
    fn requests_get_kernel_ids_and_replies_go_back_with_the_consumers_id() {
        let mut r = Router::default();
        r.attach(1);
        r.attach(2);
        // Both consumers use id "1": the kernel sees two different ids.
        let a = parse(&kernel(r.consumer_frame(1, r#"{"jsonrpc":"2.0","id":"1","method":"session/list","params":{}}"#)));
        let b = parse(&kernel(r.consumer_frame(2, r#"{"jsonrpc":"2.0","id":"1","method":"session/list","params":{}}"#)));
        assert_ne!(a["id"], b["id"]);
        let reply = json!({"jsonrpc":"2.0","id": b["id"], "result": {"sessions": []}}).to_string();
        let out = to(&mut r, &reply);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, 2);
        assert_eq!(parse(&out[0].1)["id"], "1");
        // A reply is delivered once.
        assert!(to(&mut r, &reply).is_empty());
    }

    #[test]
    fn notifications_follow_the_session_that_was_opened() {
        let mut r = Router::default();
        r.attach(1);
        r.attach(2);
        let open = parse(&kernel(r.consumer_frame(1, r#"{"jsonrpc":"2.0","id":"7","method":"session/open","params":{"session_id":"_main:a"}}"#)));
        to(&mut r, &json!({"jsonrpc":"2.0","id": open["id"], "result": {"opened": {"session_id": "_main:a"}}}).to_string());
        let note = r#"{"jsonrpc":"2.0","method":"message/delta","params":{"session_id":"_main:a","text":"hi"}}"#;
        assert_eq!(to(&mut r, note), vec![(1, note.to_owned())]);
        // A topic of the session follows it.
        let topic = r#"{"jsonrpc":"2.0","method":"message/delta","params":{"session_id":"_main:a#t","text":"x"}}"#;
        assert_eq!(to(&mut r, topic).len(), 1);
        // A session nobody named reaches everyone; so does one without a session.
        let other = r#"{"jsonrpc":"2.0","method":"message/delta","params":{"session_id":"_main:b"}}"#;
        assert_eq!(to(&mut r, other).len(), 2);
        assert_eq!(to(&mut r, r#"{"jsonrpc":"2.0","method":"server/heartbeat","params":{}}"#).len(), 2);
    }

    #[test]
    fn a_session_named_by_two_consumers_reaches_both_and_detach_forgets() {
        let mut r = Router::default();
        r.attach(1);
        r.attach(2);
        r.attach(3);
        for c in [1, 2] {
            r.consumer_frame(c, r#"{"jsonrpc":"2.0","id":"1","method":"turn/start","params":{"session_id":"s"}}"#);
        }
        let note = r#"{"jsonrpc":"2.0","method":"turn/started","params":{"session_id":"s"}}"#;
        assert_eq!(to(&mut r, note).iter().map(|x| x.0).collect::<Vec<_>>(), vec![1, 2]);
        r.detach(1);
        assert_eq!(to(&mut r, note).iter().map(|x| x.0).collect::<Vec<_>>(), vec![2]);
        r.detach(2);
        // Nobody names it any more: everyone left gets it.
        assert_eq!(to(&mut r, note).iter().map(|x| x.0).collect::<Vec<_>>(), vec![3]);
    }

    #[test]
    fn replies_for_a_departed_consumer_are_dropped_and_passthrough_is_verbatim() {
        let mut r = Router::default();
        r.attach(1);
        let req = parse(&kernel(r.consumer_frame(1, r#"{"jsonrpc":"2.0","id":"1","method":"session/list","params":{}}"#)));
        r.detach(1);
        assert!(to(&mut r, &json!({"jsonrpc":"2.0","id": req["id"], "result": {}}).to_string()).is_empty());
        let n = r#"{"jsonrpc":"2.0","method":"client/note","params":{}}"#;
        assert_eq!(r.consumer_frame(1, n), Routed::Kernel(n.to_owned()));
        assert_eq!(r.consumer_frame(1, "not json"), Routed::Drop);
    }

    /// A scope for the tests: `session/*` and `turn/*`, on `code-…` sessions
    /// only; nothing `peer/…`; about no session, `server/heartbeat` only;
    /// `session/list` answers its own sessions (octos names them `id`).
    struct CodeOnly;

    impl Scope for CodeOnly {
        fn request(&self, method: &str, params: &mut Value) -> Result<(), String> {
            if !(method.starts_with("session/") || method.starts_with("turn/")) {
                return Err(format!("{method} is not allowed"));
            }
            match params.get("session_id").and_then(Value::as_str) {
                Some(s) if !s.starts_with("code-") => Err(format!("session {s} is not this app's")),
                _ => Ok(()),
            }
        }
        fn notification(&self, method: &str, session: Option<&str>) -> bool {
            !method.starts_with("peer/") && (session.is_some() || method == "server/heartbeat")
        }
        fn result(&self, method: &str, result: &mut Value) {
            if method == "session/list" {
                if let Some(list) = result.get_mut("sessions").and_then(Value::as_array_mut) {
                    list.retain(|s| s["id"].as_str().is_some_and(|id| id.starts_with("code-")));
                }
            }
        }
        fn rewrite(&self, _method: &str, params: &Value) -> Option<Value> {
            let methods = params.pointer("/capabilities/supported_methods")?.as_array()?;
            let mut params = params.clone();
            params["capabilities"]["supported_methods"] = methods
                .iter()
                .filter(|m| m.as_str().is_some_and(|m| m.starts_with("session/") || m.starts_with("turn/")))
                .cloned()
                .collect();
            Some(params)
        }
    }

    #[test]
    fn a_scoped_consumer_gets_a_notification_as_its_scope_rewrites_it() {
        let mut r = Router::default();
        r.attach(1);
        r.attach_scoped(2, Arc::new(CodeOnly));
        r.consumer_frame(1, r#"{"jsonrpc":"2.0","id":"1","method":"turn/start","params":{"session_id":"code-a"}}"#);
        let open = parse(&kernel(r.consumer_frame(2, r#"{"jsonrpc":"2.0","id":"1","method":"session/open","params":{"session_id":"code-a"}}"#)));
        to(&mut r, &json!({"jsonrpc":"2.0","id": open["id"], "result": {"opened": {"session_id": "code-a"}}}).to_string());
        // octos sends the opened session again as a `session/open`
        // notification, with the whole capability list.
        let opened = r#"{"jsonrpc":"2.0","method":"session/open","params":{"session_id":"code-a","capabilities":{"supported_methods":["turn/start","server/shutdown"]}}}"#;
        let out = to(&mut r, opened);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], (1, opened.to_owned()), "an unscoped consumer gets the frame verbatim");
        assert_eq!(out[1].0, 2);
        assert_eq!(parse(&out[1].1)["params"]["capabilities"]["supported_methods"], json!(["turn/start"]));
        // A frame the scope leaves alone passes verbatim.
        let delta = r#"{"jsonrpc":"2.0","method":"message/delta","params":{"session_id":"code-a","text":"hi"}}"#;
        assert_eq!(to(&mut r, delta), vec![(1, delta.to_owned()), (2, delta.to_owned())]);
    }

    /// [`CodeOnly`] that prepares each session with its exact tool list.
    struct Prepared;

    impl Scope for Prepared {
        fn request(&self, method: &str, params: &mut Value) -> Result<(), String> {
            CodeOnly.request(method, params)
        }
        fn notification(&self, method: &str, session: Option<&str>) -> bool {
            CodeOnly.notification(method, session)
        }
        fn result(&self, method: &str, result: &mut Value) {
            CodeOnly.result(method, result)
        }
        fn rewrite(&self, method: &str, params: &Value) -> Option<Value> {
            CodeOnly.rewrite(method, params)
        }
        fn prepare(&self, session: &str) -> Option<Value> {
            Some(json!({"method": "session/tool_list/set", "params": {"session_id": session, "generic_tools": ["read_file"]}}))
        }
    }

    #[test]
    fn a_scoped_consumers_session_is_prepared_before_its_frames_reach_the_kernel() {
        let mut r = Router::default();
        r.attach_scoped(1, Arc::new(Prepared));
        let open = r#"{"jsonrpc":"2.0","id":"1","method":"session/open","params":{"session_id":"code-a"}}"#;
        let host = parse(&kernel(r.consumer_frame(1, open)));
        assert_eq!(host["method"], "session/tool_list/set", "the preparation goes first");
        assert_eq!(host["params"]["session_id"], "code-a");
        let turn = r#"{"jsonrpc":"2.0","id":"2","method":"turn/start","params":{"session_id":"code-a"}}"#;
        assert_eq!(r.consumer_frame(1, turn), Routed::Held, "the session's frames wait");
        assert!(r.pending.is_empty(), "nothing went to the kernel yet");
        let released = r.kernel_frame(&json!({"jsonrpc":"2.0","id": host["id"], "result": {"version": 1}}).to_string());
        assert!(released.consumers.is_empty(), "the host's answer is nobody's");
        let methods: Vec<Value> = released.kernel.iter().map(|f| parse(f)["method"].clone()).collect();
        assert_eq!(methods, [json!("session/open"), json!("turn/start")], "released in order");
        assert_eq!(r.pending.len(), 2);
        // Prepared: the next frame goes straight on.
        let again = r#"{"jsonrpc":"2.0","id":"3","method":"turn/interrupt","params":{"session_id":"code-a"}}"#;
        assert_eq!(parse(&kernel(r.consumer_frame(1, again)))["method"], "turn/interrupt");
    }

    #[test]
    fn a_session_whose_preparation_failed_refuses_its_held_requests_and_tries_again() {
        let mut r = Router::default();
        r.attach_scoped(1, Arc::new(Prepared));
        let open = r#"{"jsonrpc":"2.0","id":"1","method":"session/open","params":{"session_id":"code-a"}}"#;
        let host = parse(&kernel(r.consumer_frame(1, open)));
        assert_eq!(r.consumer_frame(1, r#"{"jsonrpc":"2.0","method":"turn/steer","params":{"session_id":"code-a"}}"#), Routed::Held);
        let refused = r.kernel_frame(&json!({"jsonrpc":"2.0","id": host["id"], "error": {"code": -32602, "message": "no"}}).to_string());
        assert!(refused.kernel.is_empty(), "nothing held reaches the kernel");
        assert_eq!(refused.consumers.len(), 1, "the request is answered; the notification is dropped");
        let answer = parse(&refused.consumers[0].1);
        assert_eq!((answer["id"].clone(), answer["error"]["code"].clone()), (json!("1"), json!(SCOPE_DENIED)));
        // Not prepared: the next frame prepares it again.
        assert_eq!(parse(&kernel(r.consumer_frame(1, open)))["method"], "session/tool_list/set");
    }

    #[test]
    fn a_host_only_frame_about_a_scoped_consumers_session_goes_to_the_shell() {
        let mut r = Router::default();
        r.attach(1);
        r.attach_scoped(2, Arc::new(CodeOnly));
        r.consumer_frame(2, r#"{"jsonrpc":"2.0","id":"1","method":"turn/start","params":{"session_id":"code-a"}}"#);
        let call = r#"{"jsonrpc":"2.0","id":"h1","method":"peer/tool/call","params":{"session_id":"code-a"}}"#;
        assert_eq!(to(&mut r, call).iter().map(|x| x.0).collect::<Vec<_>>(), vec![1], "the shell's, not the app's");
        assert_eq!(r.consumer_frame(2, r#"{"jsonrpc":"2.0","id":"h1","result":{}}"#), Routed::Drop, "nor may the app answer it");
    }

    #[test]
    fn a_scoped_consumers_refused_request_is_answered_here_and_never_reaches_the_kernel() {
        let mut r = Router::default();
        r.attach_scoped(1, Arc::new(CodeOnly));
        match r.consumer_frame(1, r#"{"jsonrpc":"2.0","id":"9","method":"server/shutdown","params":{}}"#) {
            Routed::Answer(frame) => {
                let v = parse(&frame);
                assert_eq!(v["id"], "9", "answered with the consumer's own id");
                assert_eq!(v["error"]["code"], SCOPE_DENIED);
                assert_eq!(v["error"]["data"]["kind"], "scope_denied");
            }
            other => panic!("{other:?}"),
        }
        let other = r#"{"jsonrpc":"2.0","id":"10","method":"turn/start","params":{"session_id":"_main:x"}}"#;
        assert!(matches!(r.consumer_frame(1, other), Routed::Answer(_)), "a session that is not its own");
        // A refused notification has no id to answer: dropped.
        assert_eq!(r.consumer_frame(1, r#"{"jsonrpc":"2.0","method":"peer/control","params":{}}"#), Routed::Drop);
        let ok = parse(&kernel(r.consumer_frame(1, r#"{"jsonrpc":"2.0","id":"11","method":"turn/start","params":{"session_id":"code-a"}}"#)));
        assert_ne!(ok["id"], "11", "an allowed request goes on with a kernel id");
        assert_eq!(r.pending.len(), 1, "refusals leave nothing waiting");
        // It named only its own session.
        assert_eq!(r.sessions.keys().collect::<Vec<_>>(), ["code-a"]);
    }

    #[test]
    fn a_scoped_consumer_hears_only_its_own_sessions_and_what_its_scope_lets_through() {
        let mut r = Router::default();
        r.attach(1);
        r.attach_scoped(2, Arc::new(CodeOnly));
        r.consumer_frame(2, r#"{"jsonrpc":"2.0","id":"1","method":"turn/start","params":{"session_id":"code-a"}}"#);
        let own = r#"{"jsonrpc":"2.0","method":"message/delta","params":{"session_id":"code-a"}}"#;
        assert_eq!(to(&mut r, own).iter().map(|x| x.0).collect::<Vec<_>>(), vec![2]);
        // A session nobody named reaches the unscoped consumers only.
        let other = r#"{"jsonrpc":"2.0","method":"message/delta","params":{"session_id":"_main:b"}}"#;
        assert_eq!(to(&mut r, other).iter().map(|x| x.0).collect::<Vec<_>>(), vec![1]);
        // About no session: as its scope says.
        assert_eq!(to(&mut r, r#"{"jsonrpc":"2.0","method":"server/heartbeat","params":{}}"#).len(), 2);
        let staged = r#"{"jsonrpc":"2.0","method":"peer/staged","params":{}}"#;
        assert_eq!(to(&mut r, staged).iter().map(|x| x.0).collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn a_scoped_consumers_replies_pass_its_filter_and_it_answers_only_what_it_was_asked() {
        let mut r = Router::default();
        r.attach_scoped(1, Arc::new(CodeOnly));
        let list = parse(&kernel(r.consumer_frame(1, r#"{"jsonrpc":"2.0","id":"1","method":"session/list","params":{}}"#)));
        let reply = json!({"jsonrpc":"2.0","id": list["id"], "result": {"sessions": [{"id":"code-a"},{"id":"_main:sys"}]}});
        let out = to(&mut r, &reply.to_string());
        assert_eq!(parse(&out[0].1)["result"]["sessions"], json!([{"id":"code-a"}]));
        // A response to a kernel request it was never sent: dropped.
        assert_eq!(r.consumer_frame(1, r#"{"jsonrpc":"2.0","id":"q1","result":{}}"#), Routed::Drop);
        // One it was sent, about its own session: forwarded, once.
        r.consumer_frame(1, r#"{"jsonrpc":"2.0","id":"2","method":"session/open","params":{"session_id":"code-a"}}"#);
        let ask = r#"{"jsonrpc":"2.0","id":"q2","method":"user_question/ask","params":{"session_id":"code-a"}}"#;
        assert_eq!(to(&mut r, ask).len(), 1);
        let answer = r#"{"jsonrpc":"2.0","id":"q2","result":{"answer":"yes"}}"#;
        assert_eq!(r.consumer_frame(1, answer), Routed::Kernel(answer.to_owned()));
        assert_eq!(r.consumer_frame(1, answer), Routed::Drop, "once");
        r.detach(1);
        assert!(r.asked.is_empty() && r.scopes.is_empty());
    }
}
