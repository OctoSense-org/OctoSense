//! Capability negotiation.
//!
//! Server advertises supported features via `UiProtocolCapabilities`
//! (octos-core ui_protocol.rs:369). Two known v1 flags get typed booleans;
//! unknown future flags stay in `raw` per the contract's forward-compat rule.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// see octos-core ui_protocol.rs:29
pub use octos_core::ui_protocol::UI_PROTOCOL_FEATURE_APPROVAL_TYPED_V1 as APPROVAL_TYPED_V1;
// see octos-core ui_protocol.rs:32
pub use octos_core::ui_protocol::UI_PROTOCOL_FEATURE_PANE_SNAPSHOTS_V1 as PANE_SNAPSHOTS_V1;
// see octos-core ui_protocol.rs:36
pub use octos_core::ui_protocol::UI_PROTOCOL_FEATURE_SESSION_WORKSPACE_CWD_V1 as SESSION_WORKSPACE_CWD_V1;
// see octos-core ui_protocol.rs:214 — gates live `context/normalization` +
// `context/compaction` events (the server withholds them unless requested).
pub use octos_core::ui_protocol::UI_PROTOCOL_FEATURE_CONTEXT_LIFECYCLE_V1 as CONTEXT_LIFECYCLE_V1;
// see octos-core ui_protocol.rs:117 — gates `session/hydrate` (chat-history
// reload for session resume).
pub use octos_core::ui_protocol::UI_PROTOCOL_FEATURE_SESSION_HYDRATE_V1 as SESSION_HYDRATE_V1;
// see octos-core ui_protocol.rs:195 — strict opt-in gate for the aux
// REST-over-WS methods (`session/list`, `session/delete`, …). Without it the
// server rejects `session/list` with method_not_supported.
pub use octos_core::ui_protocol::UI_PROTOCOL_FEATURE_AUXILIARY_REST_TO_WS_V1 as AUXILIARY_REST_TO_WS_V1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub typed_approvals: bool,
    pub pane_snapshots: bool,
    pub session_workspace_cwd: bool,
    /// Live context lifecycle events (`context/normalization` per turn +
    /// `context/compaction`). Drives the context-usage chip and the
    /// compaction toast — the server only streams these when requested.
    pub context_lifecycle: bool,
    /// `session/hydrate` — authoritative chat-history reload used when
    /// resuming a session from the sidebar.
    pub session_hydrate: bool,
    /// Aux REST-over-WS methods (`session/list` etc.) — strict opt-in; the
    /// sidebar session list stays empty without it.
    pub auxiliary_rest_to_ws: bool,
    /// Everything the server advertised, including unknown features.
    #[serde(default)]
    pub raw: BTreeMap<String, Value>,
}

impl Capabilities {
    /// Default desired capabilities — request every client-supported flag.
    pub fn requested() -> Self {
        Self {
            typed_approvals: true,
            pane_snapshots: true,
            session_workspace_cwd: true,
            context_lifecycle: true,
            session_hydrate: true,
            auxiliary_rest_to_ws: true,
            raw: BTreeMap::new(),
        }
    }

    /// Feature names to request during the WS handshake.
    pub fn requested_features(&self) -> Vec<String> {
        let mut features = Vec::new();
        if self.typed_approvals {
            features.push(APPROVAL_TYPED_V1.to_owned());
        }
        if self.pane_snapshots {
            features.push(PANE_SNAPSHOTS_V1.to_owned());
        }
        if self.session_workspace_cwd {
            features.push(SESSION_WORKSPACE_CWD_V1.to_owned());
        }
        if self.context_lifecycle {
            features.push(CONTEXT_LIFECYCLE_V1.to_owned());
        }
        if self.session_hydrate {
            features.push(SESSION_HYDRATE_V1.to_owned());
        }
        if self.auxiliary_rest_to_ws {
            features.push(AUXILIARY_REST_TO_WS_V1.to_owned());
        }
        for (feature, enabled) in &self.raw {
            if enabled.as_bool() == Some(true) && !features.iter().any(|f| f == feature) {
                features.push(feature.clone());
            }
        }
        features
    }

    /// Value for `X-Octos-Ui-Features`.
    pub fn handshake_header_value(&self) -> Option<String> {
        let features = self.requested_features();
        if features.is_empty() {
            None
        } else {
            Some(features.join(", "))
        }
    }

    /// Build from the server's `supported_features` list (see octos-core
    /// ui_protocol.rs:374).
    pub fn from_supported_features<I, S>(features: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut caps = Self::default();
        for feature in features {
            let name = feature.as_ref();
            match name {
                APPROVAL_TYPED_V1 => caps.typed_approvals = true,
                PANE_SNAPSHOTS_V1 => caps.pane_snapshots = true,
                SESSION_WORKSPACE_CWD_V1 => caps.session_workspace_cwd = true,
                CONTEXT_LIFECYCLE_V1 => caps.context_lifecycle = true,
                SESSION_HYDRATE_V1 => caps.session_hydrate = true,
                AUXILIARY_REST_TO_WS_V1 => caps.auxiliary_rest_to_ws = true,
                _ => {}
            }
            caps.raw.insert(name.to_owned(), Value::Bool(true));
        }
        caps
    }

    /// Parse capabilities from a `SessionOpenedResult`-shaped JSON value.
    /// Accepts either `{ capabilities.supported_features: [..] }` (matches
    /// `UiProtocolCapabilities::supported_features`, octos-core ui_protocol.rs:374)
    /// or a `{ capabilities: { feature_name: bool } }` map. Forward-compat
    /// per protocol contract § Capability negotiation.
    pub fn parse(json: &Value) -> Self {
        let node = json
            .pointer("/capabilities")
            .or_else(|| json.pointer("/result/capabilities"))
            .or_else(|| json.pointer("/opened/capabilities"))
            .unwrap_or(json);
        if let Some(arr) = node.get("supported_features").and_then(|v| v.as_array()) {
            return Self::from_supported_features(arr.iter().filter_map(|v| v.as_str()));
        }
        let mut out = Self::default();
        if let Some(map) = node.as_object() {
            for (k, v) in map {
                let on = v.as_bool().unwrap_or(false);
                if on {
                    match k.as_str() {
                        APPROVAL_TYPED_V1 => out.typed_approvals = true,
                        PANE_SNAPSHOTS_V1 => out.pane_snapshots = true,
                        SESSION_WORKSPACE_CWD_V1 => out.session_workspace_cwd = true,
                        CONTEXT_LIFECYCLE_V1 => out.context_lifecycle = true,
                        SESSION_HYDRATE_V1 => out.session_hydrate = true,
                        AUXILIARY_REST_TO_WS_V1 => out.auxiliary_rest_to_ws = true,
                        _ => {}
                    }
                }
                out.raw.insert(k.clone(), v.clone());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_from_supported_features_array() {
        let v = json!({
            "capabilities": {
                "supported_features": [
                    "approval.typed.v1",
                    "pane.snapshots.v1",
                    "session.workspace_cwd.v1",
                    "future.v1"
                ]
            }
        });
        let caps = Capabilities::parse(&v);
        assert!(caps.typed_approvals);
        assert!(caps.pane_snapshots);
        assert!(caps.session_workspace_cwd);
        assert!(caps.raw.contains_key("future.v1"));
    }

    #[test]
    fn parse_from_bool_map() {
        let v = json!({
            "capabilities": {
                "approval.typed.v1": true,
                "pane.snapshots.v1": false,
                "session.workspace_cwd.v1": true
            }
        });
        let caps = Capabilities::parse(&v);
        assert!(caps.typed_approvals);
        assert!(!caps.pane_snapshots);
        assert!(caps.session_workspace_cwd);
    }

    #[test]
    fn requested_features_format_handshake_header() {
        let mut caps = Capabilities::requested();
        caps.raw.insert("future.v1".into(), Value::Bool(true));
        assert_eq!(
            caps.handshake_header_value().as_deref(),
            Some("approval.typed.v1, pane.snapshots.v1, session.workspace_cwd.v1, context.lifecycle.v1, state.session_hydrate.v1, auxiliary.rest_to_ws.v1, future.v1")
        );
    }
}
