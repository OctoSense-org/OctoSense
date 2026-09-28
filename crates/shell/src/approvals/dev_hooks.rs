//! Developer mode's say, asked before anything else (ADR 0004 §13):
//! developer mode approves every call of the apps it covers,
//! `auto_approvable: false` included, with no sheet, no rule and no app's
//! own sheet.
//!
//! The router asks through [`DevModeHooks`], so tests use [`FixedDevMode`].
//! [`ShellDevMode`] is the adapter to the shell's developer mode.
//!
//! ADAPTER STUB (until OctoSense#118 lands): the shell has no `dev_mode`
//! module yet, so [`ShellDevMode`] answers "no" to every question. Once
//! #118 is merged it forwards to `crate::dev_mode::{answers_approval,
//! overrides_app_confirm, approves_command, grants_all}`; nothing else in
//! this module changes.

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
}

/// The shell's developer mode. STUB until #118: see the module docs.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShellDevMode;

impl DevModeHooks for ShellDevMode {
    fn answers_approval(&self, _owning_app: &str, _auto_approvable: bool, _connection: Connection) -> bool {
        false
    }
    fn overrides_app_confirm(&self, _owning_app: &str) -> bool {
        false
    }
    fn approves_command(&self, _owning_app: &str, _connection: Connection) -> bool {
        false
    }
    fn grants_all(&self, _app: &str) -> bool {
        false
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
