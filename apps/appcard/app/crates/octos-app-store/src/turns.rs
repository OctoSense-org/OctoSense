//! Active turn map — keyed by `TurnId`. The streaming text buffer lives in
//! `state::Ephemeral`, NOT here, per `03-PROTOCOL-CONTRACT.md` § "Live
//! streaming output": `message/delta` is non-durable.

use chrono::{DateTime, Utc};
// see octos-core ui_protocol.rs:69
use octos_core::ui_protocol::TurnId;
use octos_core::SessionKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnStatus {
    Pending,
    Streaming,
    Completed,
    Errored,
    Interrupted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub id: TurnId,
    pub session_id: SessionKey,
    pub status: TurnStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

impl Turn {
    pub fn started(id: TurnId, session_id: SessionKey, started_at: DateTime<Utc>) -> Self {
        Self {
            id, session_id, started_at,
            status: TurnStatus::Pending, completed_at: None, error: None,
        }
    }

    /// First `message/delta` flips Pending → Streaming. Idempotent.
    pub fn mark_streaming(&mut self) {
        if matches!(self.status, TurnStatus::Pending) { self.status = TurnStatus::Streaming; }
    }

    pub fn mark_completed(&mut self, at: DateTime<Utc>) {
        self.status = TurnStatus::Completed;
        self.completed_at = Some(at);
    }

    /// `turn/error` — `code: "interrupted"` is the documented response to
    /// `turn/interrupt`; render "stopped" vs "failed" off the status.
    pub fn mark_error(&mut self, code: &str, message: String, at: DateTime<Utc>) {
        self.status = if code == "interrupted" { TurnStatus::Interrupted } else { TurnStatus::Errored };
        self.error = Some(message);
        self.completed_at = Some(at);
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.status, TurnStatus::Completed | TurnStatus::Errored | TurnStatus::Interrupted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ts(secs: i64) -> DateTime<Utc> { DateTime::<Utc>::from_timestamp(secs, 0).unwrap() }
    fn tid() -> TurnId { TurnId(Uuid::nil()) }
    fn sk() -> SessionKey { SessionKey("telegram:1".to_owned()) }

    #[test]
    fn turn_lifecycle_streaming_to_completed() {
        let mut t = Turn::started(tid(), sk(), ts(100));
        assert_eq!(t.status, TurnStatus::Pending);
        t.mark_streaming();
        assert_eq!(t.status, TurnStatus::Streaming);
        // Second call is a no-op (idempotent).
        t.mark_streaming();
        assert_eq!(t.status, TurnStatus::Streaming);
        t.mark_completed(ts(200));
        assert_eq!(t.status, TurnStatus::Completed);
        assert_eq!(t.completed_at, Some(ts(200)));
        assert!(t.is_terminal());
    }

    #[test]
    fn turn_error_classifies_interrupted() {
        let mut t = Turn::started(tid(), sk(), ts(0));
        t.mark_error("interrupted", "user stopped".into(), ts(1));
        assert_eq!(t.status, TurnStatus::Interrupted);
        assert_eq!(t.error.as_deref(), Some("user stopped"));
        assert!(t.is_terminal());

        let mut t2 = Turn::started(tid(), sk(), ts(0));
        t2.mark_error("runtime", "boom".into(), ts(1));
        assert_eq!(t2.status, TurnStatus::Errored);
    }

    #[test]
    fn mark_streaming_does_not_revive_terminal_turn() {
        let mut t = Turn::started(tid(), sk(), ts(0));
        t.mark_completed(ts(1));
        t.mark_streaming();
        assert_eq!(t.status, TurnStatus::Completed);
    }
}
