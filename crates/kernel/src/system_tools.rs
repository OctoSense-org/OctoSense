//! The system agent's tool set and the kernel profile's tool policy
//! (ADR 0004 §12, plan step 4).
//!
//! - [`SYSTEM_AGENT_TOOLS`]: the octos tools the system agent
//!   (`_main:api:octosense#system`) gets by default. With what the person
//!   grants it ([`SystemAgentTools`]: toolbox tools, other apps' shareable
//!   tools, and command execution behind the Setup → Assistant → Command
//!   execution switch (#132); see [`SystemAgentTools::grant_command_execution`])
//!   that is its whole set: ADR 0004 §12's "exactly its grants".
//! - [`tool_policy`]: the `_main` profile's policy, the ceiling for every
//!   `_main` session. It is every tool any grant can give: OctoSense
//!   hard-codes no exclusions (§12) except ONE, octos's own shell
//!   ([`OCTOS_SHELL`]). §12 delivers command execution only as a host tool
//!   with a live approval (for example `terminal.run`), run by the shell
//!   where it can be approved, audited and shown; octos's `shell` runs
//!   commands inside the kernel with none of that, so no grant gives it.
//!   (If a later reading of §12 makes octos's `shell` grantable too, this is
//!   the one line to change: TODO(ADR 0004 §12).)
//!
//! **What is enforced today, and what is not.** octos (the pinned rev) has no
//! tool list the host can set for one session; the only roster control a
//! host has is the profile's `tool_policy` (allow/deny, deny wins), which
//! octos re-applies to every turn's finished registry (after the per-turn
//! `peer_*`, `spawn` and `send_file` tools) and to kernel wake continuations
//! alike. So before every kernel start ([`enforce`], from `launch::prepare`)
//! the host writes [`tool_policy`] into its OWN profile. It also denies
//! octos's `peer_close` ([`PEER_CLOSE`]): a closed app peer cannot be
//! resumed or replaced, so no agent may close one. Consequently:
//!
//! - **§12's "exactly its grants" is NOT yet enforced for the system
//!   agent.** It is bounded by the grantable ceiling: no octos shell, but
//!   every other tool octos registers for it. octos#2567's host session set
//!   could narrow it with `generic_tools`, but that list narrows every
//!   client's turns on the session, so the shell registers without it; the
//!   exact list waits for a durable host-only list (octos#2605). The
//!   real-kernel exact-list test is kept, ignored until then.
//! - **App peers are narrowed to their grants**: the shell registers each
//!   with `generic_tools`, exactly the kernel tools its manifest declares
//!   and the person granted (`native-apps.json` `agent.generic_tools`, a
//!   script app's `agent.tools`; none when it names none), never octos's
//!   shell; this policy is a second barrier. Host-routed tools (app,
//!   toolbox, cross-app tools, command execution) are registered after the
//!   policy, so it never strips them.
//! - **Talk to Octos external turns are unaffected**: octos confines them to
//!   its external allowlist ([`EXTERNAL_TURN_TOOLS`]), none of which is the
//!   shell (UPCR-2026-036).
//!
//! **Whose profile.** The kernel's core dir is OctoSense's own
//! (`<OctoSense data dir>/octos-home/.octos`, see `dirs`), never the
//! person's standalone octos home, and [`enforce`] replaces only a policy
//! OctoSense wrote ([`POLICY_OWNER`]): it refuses, and warns, on a foreign
//! policy and on the person's own octos home.
//!
//! **Fails closed** (G13): a refused (or unwritable, or not read back)
//! policy starts no kernel (`launch::prepare`); every consumer's connection
//! closes with the reason. App peers do not rely on the policy alone: the
//! shell registers each with exactly the kernel tools its manifest grants
//! (`generic_tools`), which never include octos's shell.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{json, Value};

/// The octos tools the system agent gets by default (not yet enforced as
/// exact: see the module docs).
///
/// - **Supervision** of app peers (ADR 0004 §6): `peer_send_input` briefs
///   and asks, `peer_gather` / `peer_list` read the blackboard,
///   `peer_respond` answers a peer's question (never its approvals, which
///   octos refuses). Not `peer_handoff`: app peers are prepared by the host
///   (`peer/prepare`). Not `peer_close`: octos cannot resume a closed peer
///   or make a new one for its (app, account), so closing one would erase
///   an app's agent for good; [`tool_policy`] denies it too.
/// - **Its workspace**, fenced by octos to the session's working directory:
///   read, search and edit files there.
/// - **The person**: `ask_user_question`, media viewing.
/// - **Memory**: recall and search, and saving to its own namespace.
/// - **The web**: octos's builtin `web_search` / `web_fetch`, which Talk to
///   Octos clients also keep, until the toolbox (#108) grants the system
///   agent `toolbox.search` / `toolbox.web_read`.
///
/// Anything else it may have is by grant ([`SystemAgentTools`]); command
/// execution only as a host tool the person turns on (Setup › Assistant ›
/// Command execution; the shell's system chat registers it on the session).
pub const SYSTEM_AGENT_TOOLS: &[&str] = &[
    // Supervision.
    "peer_send_input",
    "peer_gather",
    "peer_list",
    "peer_respond",
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

/// octos's own shell: `group:runtime`, the `shell` tool and its aliases
/// (`bash`, `exec_command` and its PTY input `write_stdin`). The one tool
/// OctoSense never offers: command execution is granted as a host tool
/// ([`COMMAND_EXECUTION_TOOL`]), each command approved live.
pub const OCTOS_SHELL: &str = "group:runtime";

/// Marks a `tool_policy` OctoSense wrote (`"owner"`, a field octos ignores).
pub const POLICY_OWNER: &str = "octosense";

/// The host tool granted command execution arrives as (the Terminal app's
/// shareable tool, ADR 0004 §10 and §12): each command approved live.
pub const COMMAND_EXECUTION_TOOL: &str = "terminal.run";

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
/// granted. Granted tools are host-routed: the shell's system chat registers
/// the ones it is granted on the system session (octos#2567's host session
/// target; today command execution's [`COMMAND_EXECUTION_TOOL`]). Narrowing
/// the session to this set waits for a durable host-only list (octos#2605).
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
    /// (section 8); never octos's shell.
    ///
    /// The shell persists the switch (Setup → Assistant → Command
    /// execution, `crates/shell/src/system_chat/grants.rs`) and hands the
    /// set to [`set_grants`]; a kernel start takes it ([`grants_at_start`]).
    /// While it is on, the shell's system chat registers the host tool on
    /// the system session (`crates/shell/src/system_chat/session.rs`).
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

// ---- the grants a kernel starts with ---------------------------------------

/// The person's current grants for the system agent (the shell sets them
/// from Settings), and the grants the running kernel generation started
/// with: a change applies from the next kernel start.
static GRANTS: std::sync::Mutex<Option<SystemAgentTools>> = std::sync::Mutex::new(None);
static AT_START: std::sync::Mutex<Option<SystemAgentTools>> = std::sync::Mutex::new(None);

fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The shell hands over the person's grants (at startup, and whenever a
/// Settings switch changes). Only the shell calls this, from its Settings
/// handler, which requires the person's gesture to turn a grant on.
pub fn set_grants(tools: SystemAgentTools) {
    *lock(&GRANTS) = Some(tools);
}

/// The person's current grants ([`SystemAgentTools::new`] until the shell
/// sets them).
pub fn grants() -> SystemAgentTools {
    lock(&GRANTS).clone().unwrap_or_default()
}

/// Called before every kernel start (`launch::prepare`): the new
/// generation runs with the grants of this moment.
pub(crate) fn take_grants_for_start() {
    *lock(&AT_START) = Some(grants());
}

/// The grants the last kernel start took; `None` before any start. The
/// shell compares it with [`grants`] to say "restart the assistant to
/// apply" (see `crate::system_agent_tools_in_effect`).
pub fn grants_at_start() -> Option<SystemAgentTools> {
    lock(&AT_START).clone()
}

/// The octos `ToolPolicy` the kernel runs `_main` with: everything a grant
/// can give (an empty allowlist is octos's "allow all") except octos's
/// shell, marked as OctoSense's.
pub fn tool_policy() -> Value {
    json!({ "allow": [], "deny": [OCTOS_SHELL, PEER_CLOSE], "owner": POLICY_OWNER })
}

/// octos's tool that retires a peer for good (ADR 0004 gap 8: a closed peer
/// cannot be resumed or replaced for its (app, account)). No agent gets it;
/// only the host decides a peer's life (it never closes one today).
pub const PEER_CLOSE: &str = "peer_close";

/// What [`enforce`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Enforced {
    /// The policy was written (or already current).
    Written,
    /// No profile yet: no provider, no turns, nothing to do.
    NoProfile,
    /// Refused: why (a foreign policy, the person's own octos home, an
    /// unreadable profile). Logged as a warning.
    Refused(String),
}

/// Write [`tool_policy`] into `<core_dir>/profiles/_main.json`
/// (`config.tool_policy`), keeping every other key. It replaces only a
/// policy OctoSense wrote (owner [`POLICY_OWNER`]) or none, and never
/// touches the person's own octos home (`$HOME/octos-home/.octos`).
pub fn enforce(core_dir: &Path) -> Enforced {
    // `$HOME/octos-home/.octos`, never `$OCTOS_APP_CORE_DIR` (which names
    // OctoSense's own core dir when set).
    let outcome = enforce_unless_shared(core_dir, crate::dirs::persons_octos_home().as_deref());
    if let Enforced::Refused(why) = &outcome {
        log::warn!("octos-core: tool policy NOT written: {why}");
    }
    outcome
}

pub(crate) fn enforce_unless_shared(core_dir: &Path, shared: Option<&Path>) -> Enforced {
    if shared.is_some_and(|shared| same_dir(shared, core_dir)) {
        return Enforced::Refused(format!(
            "{} is the person's own octos home, not OctoSense's",
            core_dir.display()
        ));
    }
    let path = crate::dirs::profile_path(core_dir);
    let Ok(bytes) = std::fs::read(&path) else { return Enforced::NoProfile };
    let refused = |what: &str| Enforced::Refused(format!("{} {what}", path.display()));
    let Ok(mut root) = serde_json::from_slice::<Value>(&bytes) else { return refused("is not JSON") };
    let Some(obj) = root.as_object_mut() else { return refused("is not a JSON object") };
    let config = obj.entry("config").or_insert_with(|| json!({}));
    let Some(config) = config.as_object_mut() else { return refused("has a `config` that is not an object") };
    let policy = tool_policy();
    match config.get("tool_policy") {
        Some(current) if *current == policy => return Enforced::Written,
        None | Some(Value::Null) => {}
        Some(current) if current.get("owner").and_then(Value::as_str) == Some(POLICY_OWNER) => {}
        Some(_) => return refused("has a tool policy OctoSense did not write; leaving it"),
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
        Ok(()) if reads_back(&path) => {
            log::info!("octos-core: wrote OctoSense's tool policy to {}", path.display());
            Enforced::Written
        }
        Ok(()) => refused("does not read back with OctoSense's tool policy after writing it"),
        Err(e) => Enforced::Refused(format!("could not write {}: {e}", path.display())),
    }
}

/// Whether the profile at `path` carries [`tool_policy`] now.
fn reads_back(path: &Path) -> bool {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .is_some_and(|root| root["config"]["tool_policy"] == tool_policy())
}

fn same_dir(a: &Path, b: &Path) -> bool {
    a == b || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
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

    fn read(path: &Path) -> Value {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    /// ADR 0004 gap 8: octos cannot resume a closed peer or make a new one
    /// for its (app, account), so no agent may close one: not in the system
    /// agent's defaults, and denied by the profile whatever else it has.
    #[test]
    fn no_agent_may_close_an_app_peer() {
        assert!(!SYSTEM_AGENT_TOOLS.contains(&"peer_close"));
        assert!(tool_policy()["deny"].as_array().unwrap().contains(&json!("peer_close")));
        assert!(!SystemAgentTools::new().names().iter().any(|t| t == "peer_close"));
    }

    #[test]
    fn only_octos_shell_is_excluded_and_the_system_agent_default_has_none_of_it() {
        let policy = tool_policy();
        assert_eq!(policy["allow"], json!([]), "everything a grant can give");
        assert_eq!(policy["deny"], json!(["group:runtime", "peer_close"]), "octos's shell and closing a peer, and nothing else");
        for shell in ["shell", "bash", "exec_command", "write_stdin"] {
            assert!(!SYSTEM_AGENT_TOOLS.contains(&shell));
            assert!(!EXTERNAL_TURN_TOOLS.contains(&shell), "external clients lose nothing");
        }
        assert!(!SYSTEM_AGENT_TOOLS.contains(&"peer_handoff"));
    }

    #[test]
    fn grants_join_the_system_agents_set_as_host_tools() {
        let shipped = SystemAgentTools::new();
        assert!(!shipped.command_execution(), "command execution is off by default");
        assert!(shipped.host_tools().is_empty());
        assert_eq!(shipped.names().len(), SYSTEM_AGENT_TOOLS.len(), "no duplicates");
        let mut granted = SystemAgentTools::new();
        granted
            .grant_toolbox("toolbox.search")
            .grant_cross_app("mail.send")
            .grant_command_execution(true);
        let host: Vec<String> = granted.host_tools().into_iter().collect();
        assert_eq!(host, ["mail.send", COMMAND_EXECUTION_TOOL, "toolbox.search"]);
        assert!(!granted.names().contains("shell"), "never octos's shell");
    }

    #[test]
    fn a_start_takes_the_grants_of_that_moment() {
        let mut on = SystemAgentTools::new();
        on.grant_command_execution(true);
        set_grants(on.clone());
        take_grants_for_start();
        assert_eq!(grants_at_start(), Some(on));
        // A later change waits for the next start.
        set_grants(SystemAgentTools::new());
        assert!(grants_at_start().unwrap().command_execution());
        take_grants_for_start();
        assert!(!grants_at_start().unwrap().command_execution());
    }

    #[test]
    fn enforce_writes_ours_keeps_the_rest_and_replaces_only_our_own() {
        let dir = tmp("keep");
        let path = dir.join("profiles/_main.json");
        std::fs::write(&path, r#"{"id":"_main","config":{"llm":{"primary":{"family_id":"x"}}}}"#).unwrap();
        assert_eq!(enforce_unless_shared(&dir, None), Enforced::Written);
        let v = read(&path);
        assert_eq!(v["id"], "_main");
        assert_eq!(v["config"]["llm"]["primary"]["family_id"], "x");
        assert_eq!(v["config"]["tool_policy"], tool_policy());
        // An older policy of ours is replaced.
        std::fs::write(&path, r#"{"config":{"tool_policy":{"allow":["read_file"],"owner":"octosense"}}}"#).unwrap();
        assert_eq!(enforce_unless_shared(&dir, None), Enforced::Written);
        assert_eq!(read(&path)["config"]["tool_policy"], tool_policy());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn enforce_refuses_a_foreign_policy_and_the_persons_own_octos_home() {
        let dir = tmp("foreign");
        let path = dir.join("profiles/_main.json");
        let foreign = r#"{"config":{"tool_policy":{"allow":["*"]}}}"#;
        std::fs::write(&path, foreign).unwrap();
        assert!(matches!(enforce_unless_shared(&dir, None), Enforced::Refused(_)));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), foreign, "untouched");
        // The person's own octos home: never written, policy or not.
        let own = r#"{"config":{"llm":{}}}"#;
        std::fs::write(&path, own).unwrap();
        assert!(matches!(enforce_unless_shared(&dir, Some(&dir)), Enforced::Refused(_)));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), own, "untouched");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn enforce_without_a_profile_or_with_a_broken_one_leaves_it() {
        let dir = tmp("none");
        assert_eq!(enforce_unless_shared(&dir, None), Enforced::NoProfile);
        assert!(!dir.join("profiles/_main.json").exists(), "no profile is invented");
        std::fs::write(dir.join("profiles/_main.json"), "{not json").unwrap();
        assert!(matches!(enforce_unless_shared(&dir, None), Enforced::Refused(_)));
        assert_eq!(std::fs::read_to_string(dir.join("profiles/_main.json")).unwrap(), "{not json");
        let _ = std::fs::remove_dir_all(dir);
    }
}
