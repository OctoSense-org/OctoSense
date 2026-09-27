//! Sharing one kernel pipe between consumers.
//!
//! `octos serve --stdio` (and `serve_io`) speaks one NDJSON JSON-RPC stream.
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
//! Pure bookkeeping, no I/O, so it is tested frame by frame.

use std::collections::{BTreeSet, HashMap};

use serde_json::Value;

pub(crate) type ConnId = u64;

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
}

impl Router {
    pub(crate) fn attach(&mut self, conn: ConnId) {
        self.conns.insert(conn);
    }

    /// A consumer left: forget its requests and sessions.
    pub(crate) fn detach(&mut self, conn: ConnId) {
        self.conns.remove(&conn);
        self.pending.retain(|_, p| p.conn != conn);
        self.sessions.retain(|_, subscribers| {
            subscribers.remove(&conn);
            !subscribers.is_empty()
        });
    }

    /// A consumer's frame, as the kernel should see it. `None`: not JSON, dropped.
    pub(crate) fn consumer_frame(&mut self, conn: ConnId, text: &str) -> Option<String> {
        let mut frame: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("octos-core: consumer {conn} sent a frame that is not JSON ({e}); dropped");
                return None;
            }
        };
        let Some(obj) = frame.as_object_mut() else {
            return Some(text.to_owned());
        };
        if let Some(session) = obj.get("params").and_then(session_of) {
            self.subscribe(session, conn);
        }
        let method = obj.get("method").and_then(Value::as_str).map(str::to_owned);
        match (method, obj.get("id").cloned()) {
            (Some(method), Some(id)) if !id.is_null() => {
                self.next += 1;
                let kernel_id = format!("k{}", self.next);
                obj.insert("id".into(), Value::String(kernel_id.clone()));
                self.pending.insert(kernel_id, Pending { conn, id, method });
                Some(frame.to_string())
            }
            _ => Some(text.to_owned()),
        }
    }

    /// A kernel frame: who gets it, and as what.
    pub(crate) fn kernel_frame(&mut self, text: &str) -> Vec<(ConnId, String)> {
        let mut frame: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) => {
                log::warn!("octos-core: kernel wrote a line that is not JSON; dropped");
                return Vec::new();
            }
        };
        let Some(obj) = frame.as_object_mut() else {
            return Vec::new();
        };
        let is_response = obj.get("method").is_none() && obj.contains_key("id");
        if is_response {
            let Some(kernel_id) = obj.get("id").and_then(Value::as_str).map(str::to_owned) else {
                log::warn!("octos-core: kernel response without a routable id; dropped");
                return Vec::new();
            };
            let Some(pending) = self.pending.remove(&kernel_id) else {
                // The consumer left, or a restart dropped the request.
                return Vec::new();
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
            obj.insert("id".into(), pending.id);
            return vec![(pending.conn, frame.to_string())];
        }
        // A notification (or a kernel request): by session, else to everyone.
        let targets: Vec<ConnId> = match obj.get("params").and_then(session_of) {
            Some(session) => match self.subscribers(&session) {
                Some(subs) => subs,
                None => self.conns.iter().copied().collect(),
            },
            None => self.conns.iter().copied().collect(),
        };
        targets.into_iter().map(|c| (c, text.to_owned())).collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn requests_get_kernel_ids_and_replies_go_back_with_the_consumers_id() {
        let mut r = Router::default();
        r.attach(1);
        r.attach(2);
        // Both consumers use id "1": the kernel sees two different ids.
        let a = parse(&r.consumer_frame(1, r#"{"jsonrpc":"2.0","id":"1","method":"session/list","params":{}}"#).unwrap());
        let b = parse(&r.consumer_frame(2, r#"{"jsonrpc":"2.0","id":"1","method":"session/list","params":{}}"#).unwrap());
        assert_ne!(a["id"], b["id"]);
        let reply = json!({"jsonrpc":"2.0","id": b["id"], "result": {"sessions": []}}).to_string();
        let out = r.kernel_frame(&reply);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, 2);
        assert_eq!(parse(&out[0].1)["id"], "1");
        // A reply is delivered once.
        assert!(r.kernel_frame(&reply).is_empty());
    }

    #[test]
    fn notifications_follow_the_session_that_was_opened() {
        let mut r = Router::default();
        r.attach(1);
        r.attach(2);
        let open = parse(&r.consumer_frame(1, r#"{"jsonrpc":"2.0","id":"7","method":"session/open","params":{"session_id":"_main:a"}}"#).unwrap());
        r.kernel_frame(&json!({"jsonrpc":"2.0","id": open["id"], "result": {"opened": {"session_id": "_main:a"}}}).to_string());
        let note = r#"{"jsonrpc":"2.0","method":"message/delta","params":{"session_id":"_main:a","text":"hi"}}"#;
        assert_eq!(r.kernel_frame(note), vec![(1, note.to_owned())]);
        // A topic of the session follows it.
        let topic = r#"{"jsonrpc":"2.0","method":"message/delta","params":{"session_id":"_main:a#t","text":"x"}}"#;
        assert_eq!(r.kernel_frame(topic).len(), 1);
        // A session nobody named reaches everyone; so does one without a session.
        let other = r#"{"jsonrpc":"2.0","method":"message/delta","params":{"session_id":"_main:b"}}"#;
        assert_eq!(r.kernel_frame(other).len(), 2);
        assert_eq!(r.kernel_frame(r#"{"jsonrpc":"2.0","method":"server/heartbeat","params":{}}"#).len(), 2);
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
        assert_eq!(r.kernel_frame(note).iter().map(|x| x.0).collect::<Vec<_>>(), vec![1, 2]);
        r.detach(1);
        assert_eq!(r.kernel_frame(note).iter().map(|x| x.0).collect::<Vec<_>>(), vec![2]);
        r.detach(2);
        // Nobody names it any more: everyone left gets it.
        assert_eq!(r.kernel_frame(note).iter().map(|x| x.0).collect::<Vec<_>>(), vec![3]);
    }

    #[test]
    fn replies_for_a_departed_consumer_are_dropped_and_passthrough_is_verbatim() {
        let mut r = Router::default();
        r.attach(1);
        let req = parse(&r.consumer_frame(1, r#"{"jsonrpc":"2.0","id":"1","method":"session/list","params":{}}"#).unwrap());
        r.detach(1);
        assert!(r.kernel_frame(&json!({"jsonrpc":"2.0","id": req["id"], "result": {}}).to_string()).is_empty());
        let n = r#"{"jsonrpc":"2.0","method":"client/note","params":{}}"#;
        assert_eq!(r.consumer_frame(1, n).as_deref(), Some(n));
        assert_eq!(r.consumer_frame(1, "not json"), None);
    }
}
