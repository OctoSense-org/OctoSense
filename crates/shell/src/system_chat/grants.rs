//! Setup → Assistant → Command execution: the person's switch for the
//! system agent's command execution (ADR 0004 §12; off by default).
//!
//! - **Only the person turns it on.** [`set`] needs a [`CommandGesture`],
//!   which only the Settings row makes, and only from the confirmation the
//!   person typed ([`CONFIRM_PHRASE`], shown with the risk: [`RISK`]), the
//!   pattern of developer mode's `PersonGesture` (#118). No agent, app, bus
//!   call or host service can make one (a test scans the sources). Turning
//!   it OFF is always allowed.
//! - **What it grants**: the host tool `terminal.run`
//!   (`octosense_kernel::system_tools::COMMAND_EXECUTION_TOOL`), never
//!   octos's own shell. Each command goes through the approval router as a
//!   command (`auto_approvable: false`: no standing rule answers it) with a
//!   live sheet showing the exact command ([`request_command`]); developer
//!   mode still answers it (§13).
//! - **When it applies**: the kernel takes the grants when it starts
//!   (`octosense_kernel::system_tools::set_grants`, taken at the next
//!   start), so a change says "restart the assistant to apply" and Settings
//!   offers the restart.
//! - **Persisted** per OctoSense home in [`GRANTS_FILE`], owner-only.
//!
//! **Registered on the system session** (octos#2567's host session target):
//! while the switch is on, the system chat registers `terminal.run` on its
//! own connection (`peer/tools/register` without `peer`, `generic_tools`
//! omitted), and withdraws it (an empty set) the moment the switch goes off
//! ([`host_tools`], `session.rs`). The kernel gates each call (destructive):
//! its `host_tool` approval reaches the router as a command; the approved
//! call reaches the shell's relay (`crate::host_tools`), which types it into
//! the Terminal the person sees.

use crate::approvals::{self, Caller, RequestContext, Route, ToolSpec, Trigger};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Relative to the OctoSense home.
pub const GRANTS_FILE: &str = "assistant/system-agent.json";
/// What the person types in Settings to turn command execution on.
pub const CONFIRM_PHRASE: &str = "let the assistant run commands";
/// The confirmation text: what the switch risks.
pub const RISK: &str = "The assistant could run any command on this computer as you: read any file (your secrets too), reach any website, and change or delete anything. You approve each command on a sheet that shows it exactly; nothing runs without that.";
/// The host tool command execution is granted as.
pub const COMMAND_TOOL: &str = "terminal.run";
/// The app that owns the command tool (the Terminal).
pub const COMMAND_APP: &str = "terminal";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grants {
    #[serde(default)]
    pub command_execution: bool,
}

/// Proof that the person typed the confirmation into Settings. Only
/// [`CommandGesture::settings_phrase`] makes one, and only the shell's
/// Settings row calls it.
pub struct CommandGesture(());

impl CommandGesture {
    /// The person typed [`CONFIRM_PHRASE`] (case and spacing ignored).
    pub(crate) fn settings_phrase(typed: &str) -> Option<CommandGesture> {
        let typed = typed.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        (typed == CONFIRM_PHRASE).then_some(CommandGesture(()))
    }
}

pub struct GrantStore {
    path: Option<PathBuf>,
    grants: Grants,
}

impl GrantStore {
    pub fn in_home(home: &Path) -> GrantStore {
        let path = home.join(GRANTS_FILE);
        let grants = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        GrantStore { path: Some(path), grants }
    }
    pub fn memory() -> GrantStore {
        GrantStore { path: None, grants: Grants::default() }
    }
    pub fn grants(&self) -> &Grants {
        &self.grants
    }
    /// Turn command execution on (with the person's gesture) or off.
    pub fn set_command_execution(&mut self, on: bool, gesture: Option<CommandGesture>) -> Result<(), String> {
        if on && gesture.is_none() {
            return Err(format!("To let the assistant run commands, type \u{201c}{CONFIRM_PHRASE}\u{201d} in Setup \u{203a} Assistant \u{203a} Command execution, then choose Allow."));
        }
        self.grants.command_execution = on;
        if let Some(path) = &self.path {
            let body = serde_json::to_vec_pretty(&self.grants).map_err(|e| e.to_string())?;
            approvals::write_private(path, &body).map_err(|e| format!("could not save {}: {e}", path.display()))?;
        }
        Ok(())
    }
    /// The system agent's tool set these grants give.
    pub fn tools(&self) -> SystemTools {
        SystemTools::from(&self.grants)
    }
}

/// The system agent's tools as the kernel crate sees them.
#[cfg(kernel)]
pub type SystemTools = octosense_ai_host::kernel::system_tools::SystemAgentTools;

#[cfg(kernel)]
impl From<&Grants> for SystemTools {
    fn from(g: &Grants) -> Self {
        let mut tools = SystemTools::new();
        tools.grant_command_execution(g.command_execution);
        tools
    }
}

/// Without a kernel: the host tools the grants would give.
#[cfg(not(kernel))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemTools {
    command_execution: bool,
}

#[cfg(not(kernel))]
impl SystemTools {
    pub fn command_execution(&self) -> bool {
        self.command_execution
    }
    pub fn host_tools(&self) -> std::collections::BTreeSet<String> {
        self.command_execution.then(|| COMMAND_TOOL.to_string()).into_iter().collect()
    }
}

#[cfg(not(kernel))]
impl From<&Grants> for SystemTools {
    fn from(g: &Grants) -> Self {
        SystemTools { command_execution: g.command_execution }
    }
}

static STORE: Mutex<Option<GrantStore>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut GrantStore) -> R) -> R {
    let mut guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(GrantStore::memory))
}

/// At startup: this home's grants, handed to the kernel for its starts.
pub fn init(home: &Path) {
    let store = GrantStore::in_home(home);
    publish(&store);
    *STORE.lock().unwrap_or_else(|e| e.into_inner()) = Some(store);
}

fn publish(store: &GrantStore) {
    #[cfg(kernel)]
    octosense_ai_host::kernel::system_tools::set_grants(store.tools());
    #[cfg(not(kernel))]
    let _ = store;
}

pub fn command_execution() -> bool {
    with(|s| s.grants.command_execution)
}

/// The host tools the system agent may call now: what the system chat
/// registers on its session (`peer/tools/register` without `peer`,
/// UPCR-2026-035) and the relay checks every call against.
pub fn host_tools() -> std::collections::BTreeSet<String> {
    command_execution().then(|| COMMAND_TOOL.to_string()).into_iter().collect()
}

/// Settings' switch. On needs the person's gesture; off never does.
pub fn set_command_execution(on: bool, gesture: Option<CommandGesture>) -> Result<(), String> {
    with(|s| -> Result<(), String> {
        s.set_command_execution(on, gesture)?;
        publish(s);
        Ok(())
    })?;
    // Registered (or withdrawn) on the system session now.
    super::sync_host_tools();
    Ok(())
}

/// Where the switch stands against the running kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applied {
    /// The running kernel has it.
    Yes,
    /// The running kernel started with the other value: restart to apply.
    NeedsRestart,
    /// No kernel runs: the next start takes it.
    NextStart,
}

pub fn applied() -> Applied {
    #[cfg(kernel)]
    {
        match octosense_ai_host::kernel::system_agent_tools_in_effect() {
            None => Applied::NextStart,
            Some(t) if t.command_execution() == command_execution() => Applied::Yes,
            Some(_) => Applied::NeedsRestart,
        }
    }
    #[cfg(not(kernel))]
    Applied::NextStart
}

/// Whether the system agent may run commands right now: the kernel that
/// runs its turns started with the grant.
pub fn command_execution_in_effect() -> bool {
    #[cfg(kernel)]
    return octosense_ai_host::kernel::system_agent_tools_in_effect().is_some_and(|t| t.command_execution());
    #[cfg(not(kernel))]
    false
}

/// Restart the assistant so a changed switch applies. True when a kernel
/// was running (its consumers reconnect; the chat resumes its session).
pub fn restart_assistant() -> bool {
    #[cfg(kernel)]
    return octosense_ai_host::kernel::restart();
    #[cfg(not(kernel))]
    false
}

/// One `terminal.run` call of the system agent's (`call_id`: the kernel's
/// id for it). Refused unless the kernel runs with the grant; otherwise
/// the approval router takes it as a command: developer mode may answer
/// it, no standing rule can, and the person sees the exact command.
pub fn request_command(granted: bool, call_id: &str, command: &str, cwd: Option<&str>) -> Route {
    if !granted {
        return Route::Refused("The assistant may not run commands. Setup \u{203a} Assistant \u{203a} Command execution turns it on.".into());
    }
    let mut args = serde_json::json!({ "command": command });
    if let Some(cwd) = cwd {
        args["cwd"] = serde_json::json!(cwd);
    }
    let context = RequestContext { call_id: format!("{}{call_id}", super::HELD_PREFIX), trigger: Trigger::Person, ..RequestContext::default() };
    approvals::approval_requested(COMMAND_APP, ToolSpec::host(COMMAND_TOOL).command(), args, Caller::SystemAgent, context)
}
