//! The system agent's tool set and the kernel profile's tool ceiling
//! (ADR 0004 §12, plan step 4).
//!
//! Two lists live here, and only here:
//!
//! - [`SYSTEM_AGENT_TOOLS`]: the octos tools the system agent
//!   (`_main:api:octosense#system`) is meant to be offered. With what it is
//!   granted ([`SystemAgentTools`]: toolbox tools, other apps' shareable
//!   tools, and command execution when the person turns it on in Settings)
//!   that is its whole set.
//! - [`profile_ceiling`]: every octos tool ANY `_main` session may be
//!   offered: the system agent's list plus every octos generic tool an app
//!   may declare and be granted ([`APP_GRANTABLE_OCTOS_TOOLS`]). Left out are
//!   only the tools no grant gives: octos's own process tools (`shell`,
//!   `bash`, `exec_command`, `write_stdin`, `check`, `git`), sub-agents and
//!   ad-hoc peers, schedulers, goals and kernel administration
//!   ([`NEVER_OFFERED`]). Command execution, when granted, is a HOST tool
//!   (for example `terminal.run`) with a live approval, never octos's
//!   `shell`.
//!
//! **What is enforced today.** octos (the pinned rev) has no per-session tool
//! list for an ordinary session; the only roster control a host has is the
//! profile's `tool_policy` (allow/deny, deny wins), which octos re-applies to
//! every turn's finished registry (after the per-turn `peer_*`, `spawn` and
//! `send_file` tools are registered) and to kernel wake continuations alike.
//! So before every kernel start ([`enforce`], from `launch::prepare`) the
//! host writes the CEILING as the `_main` profile's policy, replacing any
//! other; a policy change needs a kernel restart in octos, and every start
//! writes it again. Consequences:
//!
//! - **The system agent is bounded by the ceiling, not by its exact list**,
//!   until octos lets the host set a session's tool list (the per-session
//!   host tool list, octos#2567 item 5). No `shell` or other process tool,
//!   no sub-agents, no administration reach it; generic tools an app may be
//!   granted (for example `deep_search`, `browser`, `run_pipeline`) do, when
//!   octos registers them. The real-kernel test that asserts the exact list
//!   is ignored until then.
//! - **App peers are not capped below what they can be granted**: the
//!   ceiling holds every grantable octos tool, and each peer is narrowed to
//!   its grants by its turns' `generic_tools` (octos#2567, plan step 6).
//!   Host-routed tools (app, toolbox, cross-app tools, command execution)
//!   are registered after the policy, so it never strips them.
//! - **Talk to Octos external turns are unaffected**: the ceiling contains
//!   octos's external allowlist ([`EXTERNAL_TURN_TOOLS`]), and octos confines
//!   those turns to it on its own (UPCR-2026-036).

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{json, Value};

/// The octos tools the system agent is meant to be offered (its exact list
/// once octos#2567 item 5 exists; until then it is bounded by
/// [`profile_ceiling`]).
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
/// Not octos's `shell` or any process tool: command execution reaches the
/// system agent only as a host tool the person grants in Settings
/// ([`SystemAgentTools::grant_command_execution`]).
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

/// octos generic tools an app may declare and the person grant (ADR 0004
/// §12: OctoSense hard-codes no exclusions among them), beyond
/// [`SYSTEM_AGENT_TOOLS`]. Each app peer gets only its grants, through its
/// turns' `generic_tools` (plan step 6); a name octos does not register is
/// simply never offered.
pub const APP_GRANTABLE_OCTOS_TOOLS: &[&str] = &[
    // Research and the web.
    "deep_search",
    "deep_research",
    "synthesize_research",
    "deep_crawl",
    "site_crawl",
    "browser",
    // Content generation and pipelines.
    "image_generation",
    "mofa_make",
    "mofa_describe_content_type",
    "run_pipeline",
    "check_background_tasks",
    "read_task_output",
    // Messaging the person.
    "send_file",
    "message",
    "request_user_input",
    // Workspace history and planning.
    "workspace_diff",
    "workspace_log",
    "workspace_show",
    "update_plan",
    "tool_suggest",
    // Memory bookkeeping.
    "record_memory_use",
];

/// octos tools no grant gives, denied outright (octos: deny wins over
/// allow). They are also absent from the ceiling's allowlist, like every
/// plugin and MCP tool.
///
/// - octos's own process tools: `group:runtime` (`shell`, `exec_command`,
///   `write_stdin`, `bash`), `check` (runs build and test commands), `git`
///   (runs git, whose hooks run code). Granted command execution is a host
///   tool with a live approval instead.
/// - Sub-agents and ad-hoc peers: `group:sessions`, `peer_handoff`.
/// - Schedulers and goals, which start work outside any grant: `cron`,
///   `monitor_*`, `goal_*`.
/// - Kernel administration: `group:admin`.
pub const NEVER_OFFERED: &[&str] = &[
    "group:runtime",
    "check",
    "git",
    "group:sessions",
    "peer_handoff",
    "cron",
    "monitor_*",
    "goal_*",
    "group:admin",
];

/// The host tool granted command execution arrives as (the Terminal app's
/// shareable tool, ADR 0004 §10 and §12): each command approved live.
pub const COMMAND_EXECUTION_TOOL: &str = "terminal.run";

/// Every octos tool a `_main` session may be offered:
/// [`SYSTEM_AGENT_TOOLS`] ∪ [`APP_GRANTABLE_OCTOS_TOOLS`].
pub fn profile_ceiling() -> BTreeSet<&'static str> {
    SYSTEM_AGENT_TOOLS.iter().chain(APP_GRANTABLE_OCTOS_TOOLS).copied().collect()
}

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
/// granted. Granted tools are host-routed; registering them on the system
/// session needs octos's per-session host tool list (octos#2567 items 5
/// and 6), so today nothing grants any and [`SystemAgentTools::host_tools`]
/// is what the shell will register then.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemAgentTools {
    toolbox: BTreeSet<String>,
    cross_app: BTreeSet<String>,
    command_execution: bool,
}

impl SystemAgentTools {
    /// The set as shipped: no grants, command execution off.
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

    /// The person's Settings switch for the system agent's command
    /// execution (ADR 0004 §12; off by default). On, the system agent gets
    /// the host tool [`COMMAND_EXECUTION_TOOL`], each command approved live
    /// (section 8); never octos's `shell`, which stays denied.
    ///
    /// TODO(ADR 0004 plan steps 4/6): persist the switch in Settings →
    /// Assistant (no Settings plumbing for it exists yet) and register the
    /// host tool on the system session once octos can (octos#2567 item 6).
    pub fn grant_command_execution(&mut self, on: bool) -> &mut Self {
        self.command_execution = on;
        self
    }

    /// Whether the person granted command execution.
    pub fn command_execution(&self) -> bool {
        self.command_execution
    }

    /// The host-routed tools the system agent is granted.
    pub fn host_tools(&self) -> BTreeSet<String> {
        let mut tools: BTreeSet<String> = self.toolbox.union(&self.cross_app).cloned().collect();
        if self.command_execution {
            tools.insert(COMMAND_EXECUTION_TOOL.to_owned());
        }
        tools
    }

    /// Every tool name a system-agent turn is meant to be offered: its octos
    /// tools and its host tools.
    pub fn names(&self) -> BTreeSet<String> {
        SYSTEM_AGENT_TOOLS.iter().map(|t| t.to_string()).chain(self.host_tools()).collect()
    }
}

/// The octos `ToolPolicy` the kernel runs `_main` with: allow
/// [`profile_ceiling`], deny [`NEVER_OFFERED`].
pub fn tool_policy() -> Value {
    json!({
        "allow": profile_ceiling().into_iter().collect::<Vec<_>>(),
        "deny": NEVER_OFFERED,
    })
}

/// Write [`tool_policy`] into `<core_dir>/profiles/_main.json`
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
        Ok(()) => log::info!("octos-core: wrote the tool ceiling to {}", path.display()),
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

    const PROCESS_AND_ADMIN: &[&str] = &[
        "shell", "exec_command", "write_stdin", "bash", "check", "git", "spawn", "spawn_agent",
        "send_input", "resume_agent", "wait_agent", "close_agent", "delegate", "peer_handoff", "cron",
        "monitor_create", "goal_grant", "manage_skills", "configure_tool", "model_check",
    ];

    fn policy_lists() -> (Vec<String>, Vec<String>) {
        let policy = tool_policy();
        let list = |k: &str| policy[k].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect();
        (list("allow"), list("deny"))
    }

    #[test]
    fn no_octos_process_tool_sub_agent_or_admin_tool_is_ever_offered() {
        let (allow, deny) = policy_lists();
        for tool in PROCESS_AND_ADMIN {
            assert!(!allow.iter().any(|a| a == tool), "{tool} is in the ceiling");
            assert!(!SYSTEM_AGENT_TOOLS.contains(tool), "{tool} is in the system agent's list");
        }
        for group in ["group:runtime", "group:sessions", "group:admin"] {
            assert!(deny.iter().any(|d| d == group), "{group} is denied outright");
        }
        assert!(!allow.iter().any(|a| a.contains('*') || a.starts_with("group:")), "the ceiling names tools");
    }

    #[test]
    fn the_ceiling_holds_the_system_agent_every_grantable_tool_and_the_external_allowlist() {
        let ceiling = profile_ceiling();
        for tool in SYSTEM_AGENT_TOOLS.iter().chain(APP_GRANTABLE_OCTOS_TOOLS).chain(EXTERNAL_TURN_TOOLS) {
            assert!(ceiling.contains(tool), "{tool}");
        }
        // octos#2567's peer-safe generic tools, all grantable to an app.
        for tool in [
            "read_file", "list_dir", "glob", "grep", "web_search", "deep_search", "memory_search",
            "memory_load", "recall_memory", "save_memory", "record_memory_use", "mofa_make",
            "mofa_describe_content_type",
        ] {
            assert!(ceiling.contains(tool), "{tool} could not be granted to an app peer");
        }
        assert_eq!(ceiling.len(), SYSTEM_AGENT_TOOLS.len() + APP_GRANTABLE_OCTOS_TOOLS.len(), "no overlap");
    }

    #[test]
    fn grants_join_the_system_agents_set_as_host_tools() {
        let shipped = SystemAgentTools::new();
        assert!(!shipped.command_execution(), "command execution is off by default");
        assert!(shipped.host_tools().is_empty());
        assert_eq!(shipped.names().len(), SYSTEM_AGENT_TOOLS.len());
        let mut granted = SystemAgentTools::new();
        granted
            .grant_toolbox("toolbox.search")
            .grant_cross_app("mail.send")
            .grant_command_execution(true);
        let host: Vec<String> = granted.host_tools().into_iter().collect();
        assert_eq!(host, ["mail.send", COMMAND_EXECUTION_TOOL, "toolbox.search"]);
        assert!(!granted.names().contains("shell"), "never octos's shell");
        assert!(!tool_policy()["allow"].as_array().unwrap().iter().any(|t| t == COMMAND_EXECUTION_TOOL));
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
