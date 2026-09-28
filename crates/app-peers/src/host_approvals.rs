//! The host's say over approvals octos raises in an app agent's context.
//!
//! Normally the broker hands octos's `approval/requested` to the app, which
//! shows it and answers with [`crate::ContextOp::Approval`]. A shell may
//! install an override ([`set_override`]): OctoSense's developer mode (ADR
//! 0004 §13) answers every approval of the apps it covers itself. When the
//! override says yes, the broker answers `approve` at once and the app gets
//! the event renamed [`ANSWERED_BY_HOST`], so it can show what happened but
//! never asks the person. Without an override nothing changes.

use serde_json::{json, Value};
use std::sync::OnceLock;

/// octos's notification.
pub const APPROVAL_REQUESTED: &str = "approval/requested";
/// What the app sees instead when the host answered.
pub const ANSWERED_BY_HOST: &str = "approval/answered_by_host";

/// `(app id, tool name, session, the event's params) -> answer it approved
/// here`. The session is the one the approval was raised on; the host answers
/// only on sessions no Talk to Octos client can reach (ADR 0003). The host
/// logs what it answers.
pub type ApprovalOverride = fn(&str, &str, &str, &Value) -> bool;

static OVERRIDE: OnceLock<ApprovalOverride> = OnceLock::new();

/// Install the host's override, once per process (later calls are ignored).
pub fn set_override(hook: ApprovalOverride) {
    let _ = OVERRIDE.set(hook);
}

/// For an `approval/requested` event of `app_id`'s context that the host
/// answers: the `approval/respond` params to send (on `session`).
pub fn auto_answer(app_id: &str, method: &str, session: &str, params: &Value) -> Option<Value> {
    answer_with(OVERRIDE.get().copied(), app_id, method, session, params)
}

fn answer_with(hook: Option<ApprovalOverride>, app_id: &str, method: &str, session: &str, params: &Value) -> Option<Value> {
    if method != APPROVAL_REQUESTED {
        return None;
    }
    let approval_id = params.get("approval_id").and_then(Value::as_str)?;
    let tool = params.get("tool_name").and_then(Value::as_str).unwrap_or("");
    let hook = hook?;
    hook(app_id, tool, session, params).then(|| {
        json!({"session_id": session, "approval_id": approval_id, "decision": "approve"})
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev_app_only(app: &str, _tool: &str, session: &str, _params: &Value) -> bool {
        app == "dev.app" && session.contains("#peerctx-")
    }

    #[test]
    fn the_host_answers_only_what_its_override_covers() {
        let event = json!({"session_id": "_main:api:octosense#peerctx-1", "approval_id": "a7", "turn_id": "t", "tool_name": "shell", "title": "Run", "body": "rm -rf build"});
        let session = "_main:api:octosense#peerctx-1";
        assert_eq!(
            answer_with(Some(dev_app_only), "dev.app", APPROVAL_REQUESTED, session, &event),
            Some(json!({"session_id": session, "approval_id": "a7", "decision": "approve"}))
        );
        assert_eq!(answer_with(Some(dev_app_only), "other.app", APPROVAL_REQUESTED, session, &event), None);
        // The hook sees the session: one a Talk to Octos client can reach
        // (the system conversation) keeps the normal path.
        let system = "_main:api:octosense#system";
        assert_eq!(answer_with(Some(dev_app_only), "dev.app", APPROVAL_REQUESTED, system, &event), None);
        assert_eq!(answer_with(None, "dev.app", APPROVAL_REQUESTED, session, &event), None, "no override: the app asks");
        assert_eq!(answer_with(Some(dev_app_only), "dev.app", "message/delta", session, &event), None);
        assert_eq!(answer_with(Some(dev_app_only), "dev.app", APPROVAL_REQUESTED, session, &json!({})), None, "no id, nothing to answer");
    }
}
