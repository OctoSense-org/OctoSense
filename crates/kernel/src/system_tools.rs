//! The system agent's tool set (ADR 0004 §12, plan step 4).
//!
//! The system agent (`_main:api:octosense#system`) gets an explicit list of
//! tools, not octos's full default set: no command execution, no file writes
//! outside its workspace, no delegation to unconfined sub-agents, no kernel
//! administration. This module is the ONE definition of that list, and the
//! kernel enforces it.
//!
//! **How it is enforced.** octos (the pinned rev) has no per-session or
//! per-turn tool roster for an ordinary session; the only roster control a
//! host has is the profile's `tool_policy` (allow/deny, deny wins), which
//! octos re-applies to every turn's FINISHED registry, after the per-turn
//! `peer_*`, `spawn` and `send_file` tools are registered, and to kernel
//! continuation turns alike. So before every kernel start ([`enforce`], from
//! `launch::prepare`) the host writes [`tool_policy`] into
//! `<core_dir>/profiles/_main.json`, replacing whatever policy is there. A
//! `tool_policy` change needs a kernel restart in octos, and every start
//! re-writes it, so a stale or edited profile never runs without it.
//!
//! **What else it bounds.** The policy is the profile's, so it is the
//! ceiling for every session of `_main`, not only the system agent: app
//! peers (which ADR 0004 §12 also denies command execution; octos#2567's
//! host tools are added after the policy, so it never strips them), AppCard
//! and Rinx sessions, and peers the system agent reaches. Talk to Octos
//! external turns are unaffected: [`SYSTEM_AGENT_TOOLS`] contains every tool
//! of octos's external allowlist ([`EXTERNAL_TURN_TOOLS`]), and octos
//! confines those turns further on its own (UPCR-2026-036).
//!
//! **Granted tools.** Toolbox tools (OctoSense#108) and cross-app tools
//! (ADR 0004 §7) the system agent is granted are host-routed tools. They
//! join the set through [`SystemAgentTools::grant_toolbox`] and
//! [`SystemAgentTools::grant_cross_app`]; nothing grants any yet, and octos
//! has no way yet to register host-routed tools on the system session
//! (octos#2567 registers them per app peer only).

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{json, Value};

/// The kernel tools the system agent is offered, and nothing else.
///
/// - **Supervision** of app peers (ADR 0004 §6): `peer_send_input` briefs
///   and asks, `peer_gather` / `peer_list` read the blackboard,
///   `peer_respond` answers a peer's question (never its approvals, which
///   octos refuses), `peer_close` retires one. Not `peer_handoff`: app peers
///   are prepared by the host (`peer/prepare`), and an ad-hoc peer would be
///   an agent outside every grant.
/// - **Its workspace**, fenced by octos to the session's working directory:
///   read, search and edit files there.
/// - **The person**: `ask_user_question`, media viewing.
/// - **Memory**: recall and search, and saving to its own namespace.
/// - **The web**: octos's builtin `web_search` / `web_fetch`, which Talk to
///   Octos clients also keep, until the toolbox (#108) grants the system
///   agent `toolbox.search` / `toolbox.web_read`.
///
/// Absent on purpose: `shell`, `exec_command`, `write_stdin`, `bash` and
/// every other code or command execution (`group:runtime`); `spawn`,
/// `delegate` and the sub-agent tools (`group:sessions`); skill, tool and
/// model administration (`group:admin`); `browser`, `deep_search`,
/// `deep_crawl`, pipelines; plugin and MCP tools (they are not named here,
/// so the allowlist drops them whatever they are called).
pub const SYSTEM_AGENT_TOOLS: &[&str] = &[
    // Supervision.
    "peer_send_input",
    "peer_gather",
    "peer_list",
    "peer_respond",
    "peer_close",
    // Its workspace (octos fences these to the session's working directory).
    "read_file",
    "write_file",
    "edit_file",
    "diff_edit",
    "apply_patch",
    "glob",
    "grep",
    "list_dir",
    "code_structure",
    "check_workspace_contract",
    // The person.
    "ask_user_question",
    "view_image",
    "view_video",
    // Memory.
    "recall",
    "recall_memory",
    "memory_search",
    "memory_load",
    "save_memory",
    "memory_note",
    // The web (until the toolbox grants replace them, #108).
    "web_search",
    "web_fetch",
    // Tool discovery over this same set.
    "tool_search",
];

/// Denied whatever the allowlist says (octos: deny always wins): command
/// execution, sub-agents and administration, as octos's own groups.
pub const SYSTEM_AGENT_DENIED: &[&str] = &["group:runtime", "group:sessions", "group:admin"];

/// octos's allowlist for a Talk to Octos external turn (octos
/// `crates/octos-cli/src/api/host_managed.rs`, `EXTERNAL_TURN_TOOLS`, at the
/// pinned rev). Kept here to prove the system agent's set never narrows it.
pub const EXTERNAL_TURN_TOOLS: &[&str] = &[
    "read_file",
    "write_file",
    "edit_file",
    "diff_edit",
    "apply_patch",
    "glob",
    "grep",
    "list_dir",
    "code_structure",
    "check_workspace_contract",
    "web_search",
    "web_fetch",
    "ask_user_question",
    "recall",
    "recall_memory",
    "memory_search",
    "memory_load",
    "view_image",
    "view_video",
    "tool_search",
];

/// The system agent's tool set: [`SYSTEM_AGENT_TOOLS`] plus what it is
/// granted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemAgentTools {
    toolbox: BTreeSet<String>,
    cross_app: BTreeSet<String>,
}

impl SystemAgentTools {
    /// The set as shipped: no grants.
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant a toolbox tool (`toolbox.search`, …; ADR 0002 §6, #108).
    pub fn grant_toolbox(&mut self, tool: impl Into<String>) -> &mut Self {
        self.toolbox.insert(tool.into());
        self
    }

    /// Grant another app's shareable tool (`mail.send`, …; ADR 0004 §7).
    pub fn grant_cross_app(&mut self, tool: impl Into<String>) -> &mut Self {
        self.cross_app.insert(tool.into());
        self
    }

    /// Every tool name a system-agent turn may be offered.
    pub fn names(&self) -> BTreeSet<String> {
        SYSTEM_AGENT_TOOLS
            .iter()
            .map(|t| t.to_string())
            .chain(self.toolbox.iter().cloned())
            .chain(self.cross_app.iter().cloned())
            .collect()
    }

    /// The octos `ToolPolicy` that enforces the set.
    pub fn tool_policy(&self) -> Value {
        json!({
            "allow": self.names().into_iter().collect::<Vec<_>>(),
            "deny": SYSTEM_AGENT_DENIED,
        })
    }
}

/// The policy the kernel runs with (no grants yet).
pub fn tool_policy() -> Value {
    SystemAgentTools::new().tool_policy()
}

/// Write the system agent's policy into `<core_dir>/profiles/_main.json`
/// (`config.tool_policy`), keeping every other key. No profile yet means no
/// provider and no turns: nothing to do. An unreadable profile is left for
/// the kernel to report (it cannot run turns from it either).
pub fn enforce(core_dir: &Path) {
    let path = crate::dirs::profile_path(core_dir);
    let Ok(bytes) = std::fs::read(&path) else { return };
    let Ok(mut root) = serde_json::from_slice::<Value>(&bytes) else {
        log::warn!("octos-core: {} is not JSON; tool policy NOT written", path.display());
        return;
    };
    let Some(obj) = root.as_object_mut() else {
        log::warn!("octos-core: {} is not a JSON object; tool policy NOT written", path.display());
        return;
    };
    let config = obj.entry("config").or_insert_with(|| json!({}));
    let Some(config) = config.as_object_mut() else {
        log::warn!("octos-core: {} `config` is not an object; tool policy NOT written", path.display());
        return;
    };
    let policy = tool_policy();
    if config.get("tool_policy") == Some(&policy) {
        return;
    }
    config.insert("tool_policy".into(), policy);
    let result = serde_json::to_vec_pretty(&root)
        .map_err(|e| e.to_string())
        .and_then(|body| {
            let dir = path.parent().unwrap_or(core_dir);
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("_main.json");
            crate::network::write_private(dir, name, &body).map_err(|e| e.to_string())
        });
    match result {
        Ok(()) => log::info!("octos-core: wrote the system agent's tool policy to {}", path.display()),
        Err(e) => log::warn!("octos-core: could not write the tool policy to {}: {e}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("octos-systools-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("profiles")).unwrap();
        dir
    }

    #[test]
    fn no_command_execution_and_no_unconfined_agents() {
        for denied in [
            "shell", "exec_command", "write_stdin", "bash", "spawn", "spawn_agent", "delegate",
            "peer_handoff", "browser", "deep_search", "deep_crawl", "run_pipeline", "manage_skills",
            "configure_tool", "model_check",
        ] {
            assert!(!SYSTEM_AGENT_TOOLS.contains(&denied), "{denied}");
        }
        let policy = tool_policy();
        let deny: Vec<&str> = policy["deny"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert!(deny.contains(&"group:runtime"), "command execution is denied outright");
    }

    #[test]
    fn talk_to_octos_clients_keep_their_tools() {
        for tool in EXTERNAL_TURN_TOOLS {
            assert!(SYSTEM_AGENT_TOOLS.contains(tool), "{tool} would be taken from external clients");
        }
    }

    #[test]
    fn the_list_has_no_duplicates_and_grants_join_it() {
        let names = SystemAgentTools::new().names();
        assert_eq!(names.len(), SYSTEM_AGENT_TOOLS.len());
        let mut granted = SystemAgentTools::new();
        granted.grant_toolbox("toolbox.search").grant_cross_app("mail.send");
        let policy = granted.tool_policy();
        let allow: Vec<&str> = policy["allow"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert!(allow.contains(&"toolbox.search") && allow.contains(&"mail.send"));
        assert_eq!(allow.len(), SYSTEM_AGENT_TOOLS.len() + 2);
    }

    #[test]
    fn enforce_writes_the_policy_and_keeps_the_rest() {
        let dir = tmp("keep");
        let path = dir.join("profiles/_main.json");
        std::fs::write(
            &path,
            r#"{"id":"_main","config":{"llm":{"primary":{"family_id":"x"}},"tool_policy":{"allow":["shell"]}}}"#,
        )
        .unwrap();
        enforce(&dir);
        let v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(v["id"], "_main");
        assert_eq!(v["config"]["llm"]["primary"]["family_id"], "x");
        assert_eq!(v["config"]["tool_policy"], tool_policy(), "a widened policy is replaced");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn enforce_without_a_profile_or_with_a_broken_one_leaves_it() {
        let dir = tmp("none");
        enforce(&dir);
        assert!(!dir.join("profiles/_main.json").exists(), "no profile is invented");
        std::fs::write(dir.join("profiles/_main.json"), "{not json").unwrap();
        enforce(&dir);
        assert_eq!(std::fs::read_to_string(dir.join("profiles/_main.json")).unwrap(), "{not json");
        let _ = std::fs::remove_dir_all(dir);
    }
}
