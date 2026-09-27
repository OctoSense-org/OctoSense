//! The package seam: what only a package (desktop/ or phone/) knows.
//!
//! The shell names no product feature. A package that has one (the phone's
//! Settings app) wraps the shell's `App` in its own (`#[deref] shell`), runs
//! its work around `App::shell_handle_event`, and installs here, before the
//! shell starts, the two answers the shell needs from it while hosting
//! modules. The desktop installs nothing.
use makepad_app_module::AppModule;
use std::sync::OnceLock;

/// A package's answers to the shell.
pub struct Ext {
    /// Modules the package links beyond the shell's (the phone's Settings).
    pub linked_modules: fn() -> Vec<&'static dyn AppModule>,
    /// Whether `module` is the package's trusted system UI: a singleton
    /// whose root may make privileged host requests (the phone's Settings).
    pub trusted_module: fn(&dyn AppModule) -> bool,
}

static EXT: OnceLock<Ext> = OnceLock::new();

/// Installed once by the package, before the shell's script_mod runs.
pub fn install(ext: Ext) {
    let _ = EXT.set(ext);
}

pub fn linked_modules() -> Vec<&'static dyn AppModule> {
    EXT.get().map(|ext| (ext.linked_modules)()).unwrap_or_default()
}

pub fn trusted_module(module: &dyn AppModule) -> bool {
    EXT.get().is_some_and(|ext| (ext.trusted_module)(module))
}
