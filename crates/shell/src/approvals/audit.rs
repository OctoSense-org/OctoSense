//! The approvals audit (ADR 0004 §8 "after the fact"): every decision, and
//! above all every automatic one, as one JSON line in [`AUDIT_FILE`] under
//! the home: append-only, owner-only (0600). Each entry names the rule,
//! the owning app, the tool, a digest of the exact arguments (never the
//! arguments), the caller, the time and the result.
//!
//! Beside it, [`CALLS_FILE`]: every tool call the relay handles
//! ([`CallAudit`]), approved or not, whoever made it, `dev.run` included.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Relative to the OctoSense home.
pub const AUDIT_FILE: &str = "logs/approvals-audit.jsonl";
/// Every tool call the shell's relay handles (ADR 0004 §8, §12, §13),
/// relative to the OctoSense home: one line when a call arrives and one
/// when it ends. Append-only, owner-only (0600).
pub const CALLS_FILE: &str = "logs/tool-calls.jsonl";

/// One line of [`CALLS_FILE`]: who called which owning app's tool, a digest
/// of the exact arguments (never the arguments), and what became of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallAudit {
    /// Unix seconds.
    pub ts: u64,
    pub call_id: String,
    /// `own_agent[/client]`, `app/<id>`, `system_agent` ([`super::Caller::as_audit`]).
    pub caller: String,
    /// The owning app.
    pub owner: String,
    pub tool: String,
    pub args_digest: String,
    /// `call` (received) or `done` (answered, refused or cancelled).
    pub phase: String,
    /// `received`; then `ok`, `error:<kind>` or `cancelled`.
    pub outcome: String,
}

/// Append one line to `home`'s [`CALLS_FILE`] (created owner-only).
pub fn append_call(home: &Path, entry: &CallAudit) -> std::io::Result<()> {
    let line = format!("{}\n", serde_json::to_string(entry).unwrap_or_default());
    append_private(&home.join(CALLS_FILE), line.as_bytes())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Unix seconds.
    pub ts: u64,
    pub id: String,
    pub app: String,
    pub tool: String,
    pub args_digest: String,
    pub caller: String,
    pub trigger: String,
    /// Who answered: `developer_mode`, `rule`, `person`, `app_sheet`,
    /// `timeout`, `refused`.
    pub by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    /// `approved` or `denied`.
    pub result: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

impl Entry {
    pub fn automatic(&self) -> bool {
        self.by == "developer_mode" || self.by == "rule"
    }
}

#[derive(Debug, Default)]
pub struct AuditLog {
    path: Option<PathBuf>,
    /// The session's entries, newest last (what Settings shows without
    /// re-reading the file; a memory log keeps only these).
    recent: Vec<Entry>,
}

const RECENT: usize = 200;

impl AuditLog {
    pub fn memory() -> AuditLog {
        AuditLog::default()
    }
    /// The home's log; earlier sessions' last entries are read back.
    pub fn in_home(home: &Path) -> AuditLog {
        let path = home.join(AUDIT_FILE);
        let recent = std::fs::read_to_string(&path)
            .map(|s| s.lines().filter_map(|l| serde_json::from_str::<Entry>(l).ok()).collect::<Vec<_>>())
            .unwrap_or_default();
        let skip = recent.len().saturating_sub(RECENT);
        AuditLog { path: Some(path), recent: recent.into_iter().skip(skip).collect() }
    }
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    /// The home this log lives in (`None` for a memory log).
    pub fn home(&self) -> Option<PathBuf> {
        self.path.as_deref()?.parent()?.parent().map(Path::to_path_buf)
    }

    pub fn append(&mut self, entry: Entry) {
        if let Some(path) = &self.path {
            let line = format!("{}\n", serde_json::to_string(&entry).unwrap_or_default());
            if let Err(e) = append_private(path, line.as_bytes()) {
                eprintln!("approvals: could not write the audit log: {e}");
            }
        }
        self.recent.push(entry);
        if self.recent.len() > RECENT {
            self.recent.remove(0);
        }
    }

    /// The newest automatic approvals first.
    pub fn recent_automatic(&self, n: usize) -> Vec<Entry> {
        self.recent.iter().rev().filter(|e| e.automatic()).take(n).cloned().collect()
    }
    pub fn all(&self) -> &[Entry] {
        &self.recent
    }
}

fn append_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        super::create_private_dir(dir)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(bytes)
}
