//! The peer link's frames, as the shell reads and writes them: the same
//! JSON as Makepad's `makepad_ai_services::peer` (the app's side), inside a
//! studio `Custom` frame under the `"octos_peer"` key. The shell parses
//! with serde_json and depends on no Makepad type for it, so the link works
//! with any Makepad revision that carries the client; the fixtures in
//! `tests.rs` are the frames that client writes.

use serde_json::{json, Map, Value};

/// The envelope key (Makepad's `PEER_KEY`).
pub const PEER_KEY: &str = "octos_peer";
/// What an app may ask, by exact name (Makepad's `PEER_METHODS`).
pub const PEER_METHODS: [&str; 5] = [
    "octos.session.open",
    "octos.session.history",
    "octos.turn.start",
    "octos.turn.interrupt",
    "octos.context.close",
];
/// Bytes of one frame; larger frames are dropped unread.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
/// Longest call id, context id or client label.
pub const MAX_ID_BYTES: usize = 128;

/// How the app answers a tool call.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Ok(Value),
    Error(String),
    AwaitingConfirmation,
}

/// App → shell.
#[derive(Clone, Debug, PartialEq)]
pub enum Up {
    Request { req_id: u64, method: String, args: Map<String, Value> },
    ToolResult { call_id: String, outcome: Outcome },
}

/// How much a tool can break (the wire's `risk`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Risk {
    Read,
    Act,
    Destructive,
}

impl Risk {
    pub fn as_str(self) -> &'static str {
        match self {
            Risk::Read => "read",
            Risk::Act => "act",
            Risk::Destructive => "destructive",
        }
    }
}

/// A tool call as the app gets it: identity fields stamped by the shell.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCallDown {
    pub call_id: String,
    pub name: String,
    pub args: Value,
    pub risk: Risk,
    pub confirm_required: bool,
    pub timeout_ms: u64,
    pub account: Option<String>,
    pub context_id: Option<String>,
    pub client: Option<String>,
    /// `own_agent`, `app:<id>` or `system_agent`.
    pub caller: String,
}

/// Shell → app.
#[derive(Clone, Debug, PartialEq)]
pub enum Down {
    Reply { req_id: u64, result: Result<Value, String> },
    Event { req_id: u64, event: Value },
    ToolCall(ToolCallDown),
    ToolCancel { call_id: String },
    ContextClosed { context: String, reason: String },
}

pub fn id_ok(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_BYTES && !id.chars().any(char::is_control)
}

/// Whether a studio `Custom` frame is the peer link's (then the shell
/// never hands it to the AI bus or the WM, parsed or not).
pub fn is_peer_frame(frame: &str) -> bool {
    frame.trim_start().starts_with("{\"octos_peer\"")
}

impl Up {
    /// `None` for anything not a well-formed up frame: not ours, over the
    /// cap, an unknown method, args that are not an object, a bad id.
    pub fn parse(frame: &str) -> Option<Up> {
        if frame.len() > MAX_FRAME_BYTES || !is_peer_frame(frame) {
            return None;
        }
        let value: Value = serde_json::from_str(frame).ok()?;
        let outer = value.as_object()?;
        if outer.len() != 1 {
            return None;
        }
        let v = outer.get(PEER_KEY)?.as_object()?;
        match v.get("up")?.as_str()? {
            "request" => {
                let req_id = v.get("req_id")?.as_u64()?;
                let method = v.get("method")?.as_str()?;
                if !PEER_METHODS.contains(&method) {
                    return None;
                }
                let args = match v.get("args") {
                    None => Map::new(),
                    Some(Value::Object(m)) => m.clone(),
                    Some(_) => return None,
                };
                Some(Up::Request { req_id, method: method.to_string(), args })
            }
            "tool_result" => {
                let call_id = v.get("call_id")?.as_str().filter(|c| id_ok(c))?.to_string();
                let outcome = if v.get("awaiting_confirmation").and_then(Value::as_bool) == Some(true) {
                    Outcome::AwaitingConfirmation
                } else if v.get("ok")?.as_bool()? {
                    Outcome::Ok(v.get("data").cloned().unwrap_or(Value::Null))
                } else {
                    Outcome::Error(v.get("error").and_then(Value::as_str).unwrap_or("failed").to_string())
                };
                Some(Up::ToolResult { call_id, outcome })
            }
            _ => None,
        }
    }
}

impl Down {
    pub fn to_json(&self) -> String {
        let inner = match self {
            Down::Reply { req_id, result } => match result {
                Ok(data) => json!({"down": "reply", "req_id": req_id, "ok": true, "data": data}),
                Err(error) => json!({"down": "reply", "req_id": req_id, "ok": false, "error": error}),
            },
            Down::Event { req_id, event } => json!({"down": "event", "req_id": req_id, "event": event}),
            Down::ToolCall(c) => json!({
                "down": "tool_call",
                "call_id": c.call_id,
                "name": c.name,
                "args": c.args,
                "risk": c.risk.as_str(),
                "confirm_required": c.confirm_required,
                "timeout_ms": c.timeout_ms,
                "account": c.account,
                "context_id": c.context_id,
                "client": c.client,
                "caller": c.caller,
            }),
            Down::ToolCancel { call_id } => json!({"down": "tool_cancel", "call_id": call_id}),
            Down::ContextClosed { context, reason } => json!({"down": "context_closed", "context": context, "reason": reason}),
        };
        json!({ PEER_KEY: inner }).to_string()
    }

    /// For tests and the recording link: the frame read back.
    pub fn parse(frame: &str) -> Option<Down> {
        let value: Value = serde_json::from_str(frame).ok()?;
        let v = value.get(PEER_KEY)?.as_object()?;
        let opt = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        Some(match v.get("down")?.as_str()? {
            "reply" => Down::Reply {
                req_id: v.get("req_id")?.as_u64()?,
                result: if v.get("ok")?.as_bool()? { Ok(v.get("data").cloned().unwrap_or(Value::Null)) } else { Err(opt("error").unwrap_or_default()) },
            },
            "event" => Down::Event { req_id: v.get("req_id")?.as_u64()?, event: v.get("event")?.clone() },
            "tool_call" => Down::ToolCall(ToolCallDown {
                call_id: opt("call_id")?,
                name: opt("name")?,
                args: v.get("args")?.clone(),
                risk: match v.get("risk")?.as_str()? {
                    "read" => Risk::Read,
                    "act" => Risk::Act,
                    _ => Risk::Destructive,
                },
                confirm_required: v.get("confirm_required")?.as_bool()?,
                timeout_ms: v.get("timeout_ms")?.as_u64()?,
                account: opt("account"),
                context_id: opt("context_id"),
                client: opt("client"),
                caller: opt("caller")?,
            }),
            "tool_cancel" => Down::ToolCancel { call_id: opt("call_id")? },
            "context_closed" => Down::ContextClosed { context: opt("context")?, reason: opt("reason").unwrap_or_default() },
            _ => return None,
        })
    }
}
