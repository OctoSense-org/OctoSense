//! `Task` and `ToolCall` slices, correlated by id.
//!
//! Per `03-PROTOCOL-CONTRACT.md` § Tool / task / progress events:
//!
//! - `tool/started → tool/progress* → tool/completed` correlate by
//!   `tool_call_id`.
//! - `task/updated → task/output/delta` correlate by `task_id`.
//!
//! The wire types use `tool_call_id: String`; we wrap it locally as a typed
//! [`ToolCallId`] newtype so the reducer signature is loud about which id is
//! which.

use chrono::{DateTime, Utc};
// see octos-core ui_protocol.rs:117 (OutputCursor) and lib.rs:29 (TaskId re-export)
use octos_core::ui_protocol::{OutputCursor, TurnId};
use octos_core::{SessionKey, TaskId};
use serde_json::Value as JsonValue;

/// Local newtype around the protocol's `tool_call_id: String`. Keeps the
/// `HashMap<ToolCallId, ToolCall>` key honest at the type level.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ToolCallId(pub String);

impl ToolCallId {
    pub fn as_str(&self) -> &str { &self.0 }
}

impl From<String> for ToolCallId { fn from(s: String) -> Self { Self(s) } }
impl From<&str> for ToolCallId { fn from(s: &str) -> Self { Self(s.to_owned()) } }

/// One `task/updated` line item. `lifecycle_state` / `runtime_state` are
/// strings so unknown server values pass through (forward-compat per
/// `03-PROTOCOL-CONTRACT.md` § Capability negotiation).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub id: TaskId,
    pub session_id: SessionKey,
    pub lifecycle_state: String,
    pub runtime_state: String,
    pub summary: Option<String>,
    pub last_cursor: Option<OutputCursor>,
    pub last_updated: DateTime<Utc>,
}

impl Task {
    pub fn new(id: TaskId, session_id: SessionKey, last_updated: DateTime<Utc>) -> Self {
        Self {
            id, session_id, last_updated,
            lifecycle_state: String::new(), runtime_state: String::new(),
            summary: None, last_cursor: None,
        }
    }
}

/// One `tool/started` row, mutated through to `tool/completed`.
///
/// `progress_pct` is the latest `tool/progress.progress_pct` notification
/// (octos-core ui_protocol.rs:1328) — `0.0..=1.0` if the tool reports
/// fractional progress, `None` otherwise. Used by the task dock to
/// aggregate "running X/Y" and to render an inline bar (W04 follow-up #4).
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub id: ToolCallId,
    pub session_id: SessionKey,
    pub turn_id: TurnId,
    pub tool_name: String,
    pub arguments: Option<JsonValue>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub success: Option<bool>,
    pub output_preview: Option<String>,
    pub progress_pct: Option<f32>,
}

impl ToolCall {
    pub fn started(
        id: ToolCallId, session_id: SessionKey, turn_id: TurnId,
        tool_name: impl Into<String>, arguments: Option<JsonValue>,
        started_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id, session_id, turn_id, tool_name: tool_name.into(), arguments, started_at,
            completed_at: None, success: None, output_preview: None,
            progress_pct: None,
        }
    }

    pub fn mark_completed(
        &mut self, success: Option<bool>, output_preview: Option<String>,
        completed_at: DateTime<Utc>,
    ) {
        self.success = success;
        self.output_preview = output_preview;
        self.completed_at = Some(completed_at);
    }

    pub fn is_complete(&self) -> bool { self.completed_at.is_some() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ts(secs: i64) -> DateTime<Utc> { DateTime::<Utc>::from_timestamp(secs, 0).unwrap() }
    fn sk() -> SessionKey { SessionKey("t:1".into()) }
    fn turn() -> TurnId { TurnId(Uuid::nil()) }

    #[test]
    fn tool_call_correlation_by_id() {
        // Simulate a small map keyed by ToolCallId. The point of this test is
        // that distinct tool_call_ids do not collide, and that `mark_completed`
        // mutates the right row.
        use std::collections::HashMap;
        let mut tools: HashMap<ToolCallId, ToolCall> = HashMap::new();
        let a = ToolCallId::from("call-a");
        let b = ToolCallId::from("call-b");
        tools.insert(
            a.clone(),
            ToolCall::started(a.clone(), sk(), turn(), "shell", None, ts(0)),
        );
        tools.insert(
            b.clone(),
            ToolCall::started(b.clone(), sk(), turn(), "edit", None, ts(0)),
        );
        // Completing `a` does not touch `b`.
        tools.get_mut(&a).unwrap().mark_completed(Some(true), Some("ok".into()), ts(1));
        assert!(tools.get(&a).unwrap().is_complete());
        assert!(!tools.get(&b).unwrap().is_complete());
        assert_eq!(tools.get(&a).unwrap().success, Some(true));
        assert_eq!(tools.get(&a).unwrap().output_preview.as_deref(), Some("ok"));
    }

    #[test]
    fn task_carries_last_cursor() {
        let mut t = Task::new(TaskId::default(), sk(), ts(0));
        assert!(t.last_cursor.is_none());
        t.last_cursor = Some(OutputCursor { offset: 42 });
        t.runtime_state = "running".into();
        assert_eq!(t.last_cursor.unwrap().offset, 42);
        assert_eq!(t.runtime_state, "running");
    }
}
