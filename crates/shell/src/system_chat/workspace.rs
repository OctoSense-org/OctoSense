//! The system agent's workspace: the folder its conversation runs in, which
//! its own file tools read and write, and where the craft engines work for
//! it (ADR 0013, `host_tools::areas`).
//!
//! The kernel decides it and says which when the system conversation opens
//! (`session/open`'s `opened.workspace_root`): the session driver records it
//! here every time it opens the conversation. Before that, in this run, the
//! workspace the core dir saved (Talk to Octos' web client saves the one the
//! kernel confirmed, and the router then opens the conversation there) is
//! the answer; with neither, it is not known yet. Every answer comes from
//! the host's own link to the kernel or the host's own files, never from an
//! agent.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

static CONFIRMED: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The kernel confirmed the system conversation's workspace.
pub fn confirm(root: &Path) {
    *CONFIRMED.lock().unwrap_or_else(|e| e.into_inner()) = Some(root.to_path_buf());
}

/// What `session/open`'s result says the workspace is, when it says.
pub fn of_opened(result: &serde_json::Value) -> Option<PathBuf> {
    result["opened"]["workspace_root"].as_str().filter(|root| !root.is_empty()).map(PathBuf::from).filter(|root| root.is_absolute())
}

/// The system agent's workspace now: the one the kernel last confirmed, else
/// the one the core dir saved; `None` while neither is known.
pub fn current() -> Option<PathBuf> {
    let confirmed = CONFIRMED.lock().unwrap_or_else(|e| e.into_inner()).clone();
    confirmed.or_else(saved)
}

#[cfg(kernel)]
fn saved() -> Option<PathBuf> {
    crate::ai_host::kernel::system_workspace()
}

#[cfg(not(kernel))]
fn saved() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_open_result_names_the_workspace() {
        let opened = serde_json::json!({"opened": {"session_id": "s", "workspace_root": "/data/users/main/workspace"}});
        assert_eq!(of_opened(&opened), Some(PathBuf::from("/data/users/main/workspace")));
        for none in [serde_json::json!({}), serde_json::json!({"opened": {"workspace_root": ""}}), serde_json::json!({"opened": {"workspace_root": "relative"}})] {
            assert_eq!(of_opened(&none), None, "{none}");
        }
    }
}
