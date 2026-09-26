//! `ApprovalsSlice` — pending/decided approvals with idempotency dedup.
//!
//! Detailed payload types (typed details, render hints, decision/scope
//! enums) come straight from `octos-core` so we never re-encode them. The
//! slice itself just tracks lifecycle: server-pending → client-sent →
//! server-acked / failed.
//!
//! See `03-PROTOCOL-CONTRACT.md` § Approval / diff preview — `approval/respond`
//! is **idempotent**, so re-sending the same decision is safe; the slice
//! reflects that by treating a duplicate `Decided` as a no-op.

// see octos-core ui_protocol.rs:85 (ApprovalId), :566 (ApprovalDecision),
// :1480 (ApprovalRequestedEvent)
use octos_core::ui_protocol::{ApprovalDecision, ApprovalId, ApprovalRequestedEvent};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalState {
    /// Server asked, we have not yet sent a response.
    Awaiting,
    /// `approval/respond` is in flight.
    PendingResponse { decision: ApprovalDecision },
    /// Server acknowledged (`accepted: true`).
    Decided { decision: ApprovalDecision },
    /// Either the response was rejected, or a transport error surfaced
    /// before ack.
    Failed(String),
}

#[derive(Clone, Debug, Default)]
pub struct ApprovalsSlice {
    pub by_id: HashMap<ApprovalId, ApprovalRequestedEvent>,
    pub state: HashMap<ApprovalId, ApprovalState>,
    /// Display order for the approvals dock — oldest pending first.
    pub pending_order: Vec<ApprovalId>,
}

impl ApprovalsSlice {
    pub fn new() -> Self { Self::default() }

    /// Server asked. Idempotent: re-receiving the same id (e.g. via cursor
    /// replay) keeps the existing state; we must not double-render.
    pub fn requested(&mut self, ev: ApprovalRequestedEvent) {
        let id = ev.approval_id.clone();
        if !self.by_id.contains_key(&id) {
            self.pending_order.push(id.clone());
            self.state.insert(id.clone(), ApprovalState::Awaiting);
        }
        self.by_id.insert(id, ev);
    }

    /// Local — we just dispatched `approval/respond`.
    pub fn pending_response(&mut self, id: &ApprovalId, decision: ApprovalDecision) {
        if self.by_id.contains_key(id) {
            self.state.insert(id.clone(), ApprovalState::PendingResponse { decision });
        }
    }

    /// Server acked. Removes from `pending_order` but keeps the payload in
    /// `by_id` for history rendering.
    pub fn decided(&mut self, id: &ApprovalId, decision: ApprovalDecision) {
        if self.by_id.contains_key(id) {
            self.state.insert(id.clone(), ApprovalState::Decided { decision });
            self.pending_order.retain(|x| x != id);
        }
    }

    pub fn failed(&mut self, id: &ApprovalId, error: impl Into<String>) {
        if self.by_id.contains_key(id) {
            self.state.insert(id.clone(), ApprovalState::Failed(error.into()));
        }
    }

    /// Server cancelled the pending approval before a client decision. Keep
    /// the payload for history rendering, but remove it from the actionable
    /// queue.
    pub fn cancelled(&mut self, id: &ApprovalId, reason: impl Into<String>) {
        if self.by_id.contains_key(id) {
            self.state
                .insert(id.clone(), ApprovalState::Failed(format!("cancelled: {}", reason.into())));
            self.pending_order.retain(|x| x != id);
        }
    }

    pub fn pending_count(&self) -> usize { self.pending_order.len() }
    pub fn state_for(&self, id: &ApprovalId) -> Option<&ApprovalState> { self.state.get(id) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octos_core::SessionKey;
    use octos_core::ui_protocol::TurnId;
    use uuid::Uuid;

    fn ev(id: &ApprovalId) -> ApprovalRequestedEvent {
        ApprovalRequestedEvent::generic(
            SessionKey("t:1".into()),
            id.clone(),
            TurnId(Uuid::nil()),
            "shell",
            "Run",
            "ls -la",
        )
    }

    #[test]
    fn requested_then_decided_lifecycle() {
        let mut s = ApprovalsSlice::new();
        let id = ApprovalId(Uuid::from_u128(1));
        s.requested(ev(&id));
        assert_eq!(s.pending_count(), 1);
        assert_eq!(s.state_for(&id), Some(&ApprovalState::Awaiting));
        s.pending_response(&id, ApprovalDecision::Approve);
        assert!(matches!(
            s.state_for(&id),
            Some(ApprovalState::PendingResponse { decision: ApprovalDecision::Approve })
        ));
        s.decided(&id, ApprovalDecision::Approve);
        assert!(matches!(
            s.state_for(&id),
            Some(ApprovalState::Decided { decision: ApprovalDecision::Approve })
        ));
        assert_eq!(s.pending_count(), 0);
    }

    #[test]
    fn duplicate_request_is_idempotent() {
        let mut s = ApprovalsSlice::new();
        let id = ApprovalId(Uuid::from_u128(1));
        s.requested(ev(&id));
        s.requested(ev(&id));
        assert_eq!(s.pending_count(), 1);
        assert_eq!(s.pending_order, vec![id.clone()]);
    }

    #[test]
    fn failed_keeps_in_pending_order() {
        let mut s = ApprovalsSlice::new();
        let id = ApprovalId(Uuid::from_u128(1));
        s.requested(ev(&id));
        s.failed(&id, "transport timeout");
        // Still pending until server says otherwise.
        assert_eq!(s.pending_count(), 1);
        assert!(matches!(s.state_for(&id), Some(ApprovalState::Failed(_))));
    }

    /// W05 follow-up #2: the `-32011 APPROVAL_NOT_PENDING` retry collapse
    /// path replays `decided` after the first decision; a duplicate
    /// `requested` event (e.g. from cursor replay on reconnect) must not
    /// flip the already-Decided card back to Awaiting.
    #[test]
    fn requested_after_decided_is_idempotent() {
        let mut s = ApprovalsSlice::new();
        let id = ApprovalId(Uuid::from_u128(1));
        s.requested(ev(&id));
        s.decided(&id, ApprovalDecision::Approve);
        assert_eq!(s.pending_count(), 0);
        assert!(matches!(
            s.state_for(&id),
            Some(ApprovalState::Decided { decision: ApprovalDecision::Approve })
        ));
        // A second `requested` (cursor replay / double-click race) must
        // not clobber the Decided state nor reinsert into pending_order.
        s.requested(ev(&id));
        assert_eq!(s.pending_count(), 0);
        assert!(matches!(
            s.state_for(&id),
            Some(ApprovalState::Decided { decision: ApprovalDecision::Approve })
        ));
    }

    #[test]
    fn cancelled_removes_pending_but_keeps_history() {
        let mut s = ApprovalsSlice::new();
        let id = ApprovalId(Uuid::from_u128(2));
        s.requested(ev(&id));
        s.cancelled(&id, "turn_interrupted");
        assert_eq!(s.pending_count(), 0);
        assert!(matches!(s.state_for(&id), Some(ApprovalState::Failed(msg)) if msg == "cancelled: turn_interrupted"));
        assert!(s.by_id.contains_key(&id));
    }
}
