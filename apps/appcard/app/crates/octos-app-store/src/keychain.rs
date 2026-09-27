//! OS-keychain wrapper for the auth bearer.
//!
//! Backed by the `keyring` crate (macOS Keychain via `security-framework`,
//! Secret Service on Linux, Credential Manager on Windows). Compiled only
//! when the `keychain` feature is enabled — tests in this crate stay
//! hermetic by default.
//!
//! Service name format: `octos-app::<host>::<profile_id>`. Mirrors the
//! W08 design note (§ "Token storage") and lets a future "Accounts" submenu
//! enumerate stored credentials by prefix.
//!
//! # Dev / headless fallback
//!
//! If `OCTOS_APP_TOKEN` is set, [`load_token`] returns it verbatim *without*
//! touching the OS keychain. Documented in `workstreams/W08-auth-tenancy.md`
//! for CI / headless containers; the token is never written to disk in this
//! path.
//!
//! # Threading
//!
//! Callers should invoke these functions off the UI thread
//! (`tokio::task::spawn_blocking`). The first read on macOS may surface the
//! Keychain unlock prompt, which blocks until the user dismisses it.

use crate::auth::{ProfileId, SecretToken, ServerHost};
use std::fmt;

const ENV_TOKEN: &str = "OCTOS_APP_TOKEN";
const SERVICE_PREFIX: &str = "octos-app";

/// Errors from keychain operations.
#[derive(Debug)]
pub enum KeychainError {
    /// Underlying `keyring` failure (platform error, locked keychain, etc).
    Backend(String),
}

impl fmt::Display for KeychainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeychainError::Backend(s) => write!(f, "keychain backend error: {s}"),
        }
    }
}

impl std::error::Error for KeychainError {}

impl From<keyring::Error> for KeychainError {
    fn from(e: keyring::Error) -> Self { KeychainError::Backend(e.to_string()) }
}

fn service_name(host: &ServerHost, profile_id: &ProfileId) -> String {
    format!("{SERVICE_PREFIX}::{host}::{profile_id}")
}

#[cfg(not(target_os = "android"))]
fn entry(host: &ServerHost, profile_id: &ProfileId) -> Result<keyring::Entry, KeychainError> {
    // `user` is informational on macOS (per-login Keychain); we pass the
    // profile id again so multi-account future setups don't collide.
    Ok(keyring::Entry::new(&service_name(host, profile_id), profile_id.as_str())?)
}

// ---------------------------------------------------------------------------
// Android fallback: `keyring` has no Android backend (every call errors at
// runtime), so the bearer lives in a file under `$HOME/.config/octos-app/`.
// HOME points at the app-private files dir (`getFilesDir()`, set at startup
// in `main.rs::handle_startup`), which Android isolates per-app.
// ---------------------------------------------------------------------------
#[cfg(target_os = "android")]
fn token_file(
    host: &ServerHost,
    profile_id: &ProfileId,
) -> Result<std::path::PathBuf, KeychainError> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        KeychainError::Backend("HOME unset (android startup bootstrap missing)".into())
    })?;
    let mut p = std::path::PathBuf::from(home);
    p.push(".config");
    p.push("octos-app");
    std::fs::create_dir_all(&p).map_err(|e| KeychainError::Backend(e.to_string()))?;
    let safe: String = service_name(host, profile_id)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    p.push(format!("{safe}.token"));
    Ok(p)
}

/// Persist a bearer for `(host, profile_id)`. Overwrites any existing entry.
pub fn store_token(
    host: &ServerHost,
    profile_id: &ProfileId,
    token: &SecretToken,
) -> Result<(), KeychainError> {
    #[cfg(not(target_os = "android"))]
    {
        entry(host, profile_id)?.set_password(token.expose())?;
        Ok(())
    }
    #[cfg(target_os = "android")]
    {
        let path = token_file(host, profile_id)?;
        let res =
            std::fs::write(&path, token.expose()).map_err(|e| KeychainError::Backend(e.to_string()));
        log::info!(
            "keychain(android): store {} → {}",
            path.display(),
            if res.is_ok() { "ok" } else { "FAILED" }
        );
        res
    }
}

/// Read a bearer for `(host, profile_id)`. Returns `Ok(None)` for "no entry"
/// (vs. a real backend failure, which is `Err`). Honours the
/// `OCTOS_APP_TOKEN` env-var bypass — when set, the keychain is *not*
/// touched (so headless CI doesn't trip the macOS unlock prompt).
pub fn load_token(
    host: &ServerHost,
    profile_id: &ProfileId,
) -> Result<Option<SecretToken>, KeychainError> {
    if let Ok(t) = std::env::var(ENV_TOKEN) {
        if !t.is_empty() {
            return Ok(Some(SecretToken::from(t)));
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        match entry(host, profile_id)?.get_password() {
            Ok(s) => Ok(Some(SecretToken::from(s))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    #[cfg(target_os = "android")]
    {
        let path = token_file(host, profile_id)?;
        let out = match std::fs::read_to_string(&path) {
            Ok(s) if !s.is_empty() => Ok(Some(SecretToken::from(s))),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(KeychainError::Backend(e.to_string())),
        };
        log::info!(
            "keychain(android): load {} → {}",
            path.display(),
            match &out {
                Ok(Some(_)) => "found",
                Ok(None) => "none",
                Err(_) => "error",
            }
        );
        out
    }
}

/// Remove the stored bearer. A missing entry is not an error — `Logout` is
/// idempotent.
pub fn delete_token(host: &ServerHost, profile_id: &ProfileId) -> Result<(), KeychainError> {
    #[cfg(not(target_os = "android"))]
    {
        match entry(host, profile_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
    #[cfg(target_os = "android")]
    {
        match std::fs::remove_file(token_file(host, profile_id)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(KeychainError::Backend(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_name_format() {
        let host = ServerHost::from("octos.example.com".to_string());
        let pid = ProfileId::from("acme".to_string());
        assert_eq!(service_name(&host, &pid), "octos-app::octos.example.com::acme");
    }

    #[test]
    fn env_var_bypass_returns_token_without_touching_keychain() {
        // SAFETY: tests in this module run sequentially; nothing else mutates env.
        unsafe { std::env::set_var(ENV_TOKEN, "dev-token-from-env"); }
        let host = ServerHost::from("does.not.matter".to_string());
        let pid = ProfileId::from("ignored".to_string());
        let got = load_token(&host, &pid).expect("env path returns Ok");
        assert_eq!(got.as_ref().map(|t| t.expose()), Some("dev-token-from-env"));
        unsafe { std::env::remove_var(ENV_TOKEN); }
    }

    #[test]
    fn keychain_error_display_does_not_leak_token() {
        let e = KeychainError::Backend("entry locked".to_string());
        let s = format!("{e}");
        assert!(s.contains("entry locked") && !s.contains("Bearer"));
    }
}
