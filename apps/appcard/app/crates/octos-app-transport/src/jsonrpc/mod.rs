//! JSON-RPC 2.0 framing.
//!
//! `RpcEnvelope` is the union over the four frame shapes the wire can carry.
//! JSON-RPC 2.0 doesn't tag frames — the shape *is* the discriminator, so we
//! use serde-untagged. Each variant wraps the shape octos-core already
//! defines (request: ui_protocol.rs:122, response: :147, notification: :169,
//! error response: :237).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use octos_core::ui_protocol::{
    JSON_RPC_VERSION, RpcError, RpcErrorResponse, RpcNotification, RpcRequest, RpcResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::oneshot;

/// One JSON-RPC frame. Variant order matters for serde-untagged: ErrorResponse
/// must come first because it shares `id` with Response but has `error`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcEnvelope {
    ErrorResponse(RpcErrorResponse),
    Response(RpcResponse<Value>),
    Request(RpcRequest<Value>),
    Notification(RpcNotification<Value>),
}

impl RpcEnvelope {
    /// JSON-RPC id (request / response / error response). Notifications: `None`.
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Request(r) => Some(&r.id),
            Self::Response(r) => Some(&r.id),
            Self::ErrorResponse(r) => r.id.as_deref(),
            Self::Notification(_) => None,
        }
    }

    /// Parse a single JSON text frame.
    pub fn parse(text: &str) -> Result<Self, JsonRpcError> {
        serde_json::from_str(text).map_err(JsonRpcError::Decode)
    }
}

/// Errors raised at the JSON-RPC layer that aren't already a server-emitted
/// `RpcError`.
#[derive(Debug)]
pub enum JsonRpcError {
    Decode(serde_json::Error),
    UnknownId(String),
    Server(RpcError),
    Cancelled,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "json decode: {e}"),
            Self::UnknownId(id) => write!(f, "unknown rpc id: {id}"),
            Self::Server(e) => write!(f, "server error {}: {}", e.code, e.message),
            Self::Cancelled => f.write_str("rpc cancelled (transport shutting down)"),
        }
    }
}

impl std::error::Error for JsonRpcError {}

/// Stringified u64 — protocol-stable per-connection uniqueness.
pub type JsonRpcId = String;

/// Pending-request registry. The connection task owns one of these and
/// records oneshot senders by id; on response the senders fire with the raw
/// `result` value or a server `RpcError`. Typed decode is the caller's job.
#[derive(Default)]
pub struct RpcRegistry {
    pending: Mutex<HashMap<JsonRpcId, oneshot::Sender<Result<Value, RpcError>>>>,
    counter: AtomicU64,
}

impl std::fmt::Debug for RpcRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.pending.lock().map(|p| p.len()).unwrap_or(0);
        f.debug_struct("RpcRegistry").field("pending_len", &len).finish()
    }
}

impl RpcRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a fresh id; monotonic over the registry's lifetime.
    pub fn next_id(&self) -> JsonRpcId {
        self.counter.fetch_add(1, Ordering::Relaxed).to_string()
    }

    pub fn register(&self, id: JsonRpcId, sender: oneshot::Sender<Result<Value, RpcError>>) {
        if let Ok(mut p) = self.pending.lock() {
            p.insert(id, sender);
        }
    }

    pub fn complete(&self, id: &str, outcome: Result<Value, RpcError>) {
        if let Ok(mut p) = self.pending.lock() {
            if let Some(tx) = p.remove(id) {
                let _ = tx.send(outcome);
            }
        }
    }

    pub fn cancel_all(&self) {
        if let Ok(mut p) = self.pending.lock() {
            p.clear();
        }
    }

    pub fn len(&self) -> usize {
        self.pending.lock().map(|p| p.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Serialize a JSON-RPC 2.0 request frame.
pub fn serialize_request(
    id: &str,
    method: &str,
    params: &Value,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&serde_json::json!({
        "jsonrpc": JSON_RPC_VERSION,
        "id": id,
        "method": method,
        "params": params,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_id_is_monotonic() {
        let reg = RpcRegistry::new();
        assert_eq!(reg.next_id(), "0");
        assert_eq!(reg.next_id(), "1");
    }

    #[test]
    fn parse_response_envelope() {
        let raw = r#"{"jsonrpc":"2.0","id":"7","result":{"opened":{"session_id":"cli:demo"}}}"#;
        match RpcEnvelope::parse(raw).unwrap() {
            RpcEnvelope::Response(r) => assert_eq!(r.id, "7"),
            other => panic!("expected Response, got {other:?}"),
        }
    }

    #[test]
    fn parse_notification_envelope() {
        let raw = r#"{"jsonrpc":"2.0","method":"tool/started","params":{"foo":1}}"#;
        match RpcEnvelope::parse(raw).unwrap() {
            RpcEnvelope::Notification(n) => assert_eq!(n.method, "tool/started"),
            other => panic!("expected Notification, got {other:?}"),
        }
    }

    #[test]
    fn parse_error_envelope() {
        let raw = r#"{"jsonrpc":"2.0","id":"3","error":{"code":-32602,"message":"bad params"}}"#;
        match RpcEnvelope::parse(raw).unwrap() {
            RpcEnvelope::ErrorResponse(e) => assert_eq!(e.error.code, -32602),
            other => panic!("expected ErrorResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn registry_round_trip_ok() {
        let reg = RpcRegistry::new();
        let id = reg.next_id();
        let (tx, rx) = oneshot::channel();
        reg.register(id.clone(), tx);
        reg.complete(&id, Ok(Value::Bool(true)));
        assert_eq!(rx.await.unwrap().unwrap(), Value::Bool(true));
        assert!(reg.is_empty());
    }
}
