//! Developer mode's say, asked before anything else (ADR 0004 §13):
//! developer mode approves every call of the apps it covers,
//! `auto_approvable: false` included, with no sheet, no rule and no app's
//! own sheet.
//!
//! The router asks through [`DevModeHooks`], so tests use [`FixedDevMode`].
//! [`ShellDevMode`] is the adapter to the shell's developer mode
//! (`crate::dev_mode`, OctoSense#118): `answers_approval` (as
//! `ApprovalKind::HostConfirm`), `overrides_app_confirm`, `approves_command`
//! and `grants_all`, and every automatic approval it gives is also written
//! to developer mode's own audit (`dev_mode::audit_auto_approval`).

use super::types::Connection;

/// The three hooks of #118 the router needs, plus the consent one.
pub trait DevModeHooks: Send {
    /// Every approval kind but the two below (`confirm: host` sheets).
    fn answers_approval(&self, owning_app: &str, auto_approvable: bool, connection: Connection) -> bool;
    /// The `confirm: app` hand-off: skip the owning app's own sheet.
    fn overrides_app_confirm(&self, owning_app: &str) -> bool;
    /// Command execution (`terminal.run`, `dev.run`, granted commands).
    fn approves_command(&self, owning_app: &str, connection: Connection) -> bool;
    /// Every grant, and no first-use consent prompt (§13).
    fn grants_all(&self, app: &str) -> bool;
    /// Developer mode answered one: its own audit records it too.
    fn audit_auto_approval(&self, _owning_app: &str, _tool: &str, _args: &str, _caller: &str, _kind: DevKind) {}
}

/// Which of developer mode's approval kinds the router answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevKind {
    HostConfirm,
    AppConfirm,
    Command,
}

fn connection(c: Connection) -> crate::dev_mode::Connection {
    match c {
        Connection::Host => crate::dev_mode::Connection::Host,
        Connection::External => crate::dev_mode::Connection::External,
    }
}

/// The shell's developer mode (`crate::dev_mode`).
#[derive(Clone, Copy, Debug, Default)]
pub struct ShellDevMode;

impl DevModeHooks for ShellDevMode {
    fn answers_approval(&self, owning_app: &str, auto_approvable: bool, c: Connection) -> bool {
        crate::dev_mode::answers_approval(owning_app, crate::dev_mode::ApprovalKind::HostConfirm, auto_approvable, connection(c))
    }
    fn overrides_app_confirm(&self, owning_app: &str) -> bool {
        crate::dev_mode::overrides_app_confirm(owning_app)
    }
    fn approves_command(&self, owning_app: &str, c: Connection) -> bool {
        crate::dev_mode::approves_command(owning_app, connection(c))
    }
    fn grants_all(&self, app: &str) -> bool {
        crate::dev_mode::grants_all(app)
    }
    fn audit_auto_approval(&self, owning_app: &str, tool: &str, args: &str, caller: &str, kind: DevKind) {
        let kind = match kind {
            DevKind::HostConfirm => crate::dev_mode::ApprovalKind::HostConfirm,
            DevKind::AppConfirm => crate::dev_mode::ApprovalKind::AppConfirm,
            DevKind::Command => crate::dev_mode::ApprovalKind::Command,
        };
        crate::dev_mode::audit_auto_approval(owning_app, tool, args, caller, kind);
    }
}

/// The test double: on for the listed apps (or all), off otherwise. Like
/// #118, never for an external connection.
#[derive(Clone, Debug, Default)]
pub struct FixedDevMode {
    pub all: bool,
    pub apps: Vec<String>,
}

impl FixedDevMode {
    pub fn off() -> FixedDevMode {
        FixedDevMode::default()
    }
    pub fn all() -> FixedDevMode {
        FixedDevMode { all: true, apps: Vec::new() }
    }
    fn covers(&self, app: &str) -> bool {
        self.all || self.apps.iter().any(|a| a == app)
    }
}

impl DevModeHooks for FixedDevMode {
    fn answers_approval(&self, owning_app: &str, _auto_approvable: bool, connection: Connection) -> bool {
        connection == Connection::Host && self.covers(owning_app)
    }
    fn overrides_app_confirm(&self, owning_app: &str) -> bool {
        self.covers(owning_app)
    }
    fn approves_command(&self, owning_app: &str, connection: Connection) -> bool {
        connection == Connection::Host && self.covers(owning_app)
    }
    fn grants_all(&self, app: &str) -> bool {
        self.covers(app)
    }
}
