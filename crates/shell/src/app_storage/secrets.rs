//! The host secrets API (ADR 0004 §11): an app's tokens, keys and passwords,
//! kept by the host and never under `apps/`.
//!
//! - macOS and iOS: the keychain (the `keyring` crate, as Mail's vault),
//!   one item per (profile, app, key); the key names are listed in
//!   `secrets/<app id>/.keychain-index` so uninstalling can delete them.
//! - Elsewhere: one owner-only (0600) file per key in `secrets/<app id>/`,
//!   a 0700 directory. TODO: the platform vault on Windows (Credential
//!   Manager), Linux (Secret Service) and Android (Keystore-wrapped files,
//!   as Mail's vault does).
//!
//! The keychain can prompt (macOS asks again for every rebuilt, unsigned
//! binary) and so hang a run nobody watches. [`select_backend`] therefore
//! picks the file store for the shell's own tests (always; no test touches
//! the login keychain), for `OCTOSENSE_SECRETS=file`, and for headless runs
//! (`CI`, `GITHUB_ACTIONS`, `MAKEPAD_HIDE_WINDOWS`, an SSH session), unless
//! `OCTOSENSE_SECRETS=keychain` asks for the keychain outside tests.

use super::{validate_app_id, StorageError};
use crate::ai_host::app_peers::storage::SecretStore;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The file under `secrets/<app id>/` naming the app's keychain items.
pub const KEYCHAIN_INDEX: &str = ".keychain-index";

/// Keys are file names: `[A-Za-z0-9._-]{1,128}`, not starting with `.`.
pub fn validate_key(key: &str) -> Result<(), StorageError> {
    let ok = !key.is_empty()
        && key.len() <= 128
        && !key.starts_with('.')
        && key.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if ok {
        Ok(())
    } else {
        Err(StorageError::InvalidKey(key.to_owned()))
    }
}

fn io(e: std::io::Error) -> StorageError {
    StorageError::Io(e.to_string())
}

/// Write `bytes` to `path` as an owner-only file, atomically.
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    // `~` is not a key character: a temporary never shadows a key.
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp~");
    let temp = PathBuf::from(temp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&temp).map_err(io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600)).map_err(io)?;
    }
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)?;
    drop(file);
    std::fs::rename(&temp, path).map_err(io)
}

/// One owner-only file per key under `secrets/<app id>/`.
pub struct FileSecrets {
    root: PathBuf,
    dir: PathBuf,
}

impl FileSecrets {
    pub fn new(secrets_root: &Path, app_id: &str) -> Self {
        Self { root: secrets_root.to_path_buf(), dir: secrets_root.join(app_id) }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

impl SecretStore for FileSecrets {
    fn put(&self, key: &str, secret: &[u8]) -> Result<(), StorageError> {
        validate_key(key)?;
        super::ensure_private_dir(&self.root, &self.dir).map_err(io)?;
        write_private(&self.dir.join(key), secret)
    }
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        validate_key(key)?;
        match std::fs::read(self.dir.join(key)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }
    fn remove(&self, key: &str) -> Result<(), StorageError> {
        validate_key(key)?;
        match std::fs::remove_file(self.dir.join(key)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(e)),
            _ => Ok(()),
        }
    }
}

/// Where an app's secrets go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// Owner-only files under `secrets/<app id>/`.
    File,
    /// The platform keychain (macOS, iOS).
    Keychain,
}

/// Environment variables that mark a run nobody can answer a keychain
/// prompt in.
pub const HEADLESS_VARS: &[&str] = &["CI", "GITHUB_ACTIONS", "MAKEPAD_HIDE_WINDOWS", "SSH_CONNECTION", "SSH_TTY"];

/// The backend for a platform with (`vault`) or without a keychain, in or
/// out of the shell's tests, given the environment `var`.
pub fn select_backend(vault: bool, in_test: bool, var: impl Fn(&str) -> Option<String>) -> Backend {
    if !vault || in_test {
        return Backend::File;
    }
    match var("OCTOSENSE_SECRETS").as_deref() {
        Some("file") => return Backend::File,
        Some("keychain") => return Backend::Keychain,
        _ => {}
    }
    if HEADLESS_VARS.iter().any(|name| var(name).is_some_and(|v| !v.is_empty())) {
        return Backend::File;
    }
    Backend::Keychain
}

/// This process's backend.
pub fn backend() -> Backend {
    select_backend(cfg!(any(target_os = "macos", target_os = "ios")), cfg!(test), |name| std::env::var(name).ok())
}

/// The store for this platform and run (see [`select_backend`]).
pub fn platform(secrets_root: &Path, app_id: &str) -> Arc<dyn SecretStore> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    if backend() == Backend::Keychain {
        return Arc::new(keychain::Keychain::new(secrets_root, app_id));
    }
    Arc::new(FileSecrets::new(secrets_root, app_id))
}

/// Delete every secret of `app_id`: its keychain items (those the index
/// names), only when this run uses the keychain. The caller removes
/// `secrets/<app id>/` itself. `false`: keychain items are indexed that this
/// run could not delete (no keychain here, or a headless run); the caller
/// keeps the index.
pub fn purge(secrets_root: &Path, app_id: &str) -> bool {
    if validate_app_id(app_id).is_err() {
        return true;
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    if backend() == Backend::Keychain {
        keychain::Keychain::new(secrets_root, app_id).purge();
        return true;
    }
    !secrets_root.join(app_id).join(KEYCHAIN_INDEX).exists()
}

/// A run with the keychain deletes the items an earlier headless uninstall
/// could not: `secrets/<app id>/` holding nothing but the index, of an app
/// no longer installed (`uninstalled`: it has no jail; a reinstalled app's
/// items are its own again).
pub fn purge_leftovers(secrets_root: &Path, uninstalled: impl Fn(&str) -> bool) {
    if backend() != Backend::Keychain {
        return;
    }
    let Ok(entries) = std::fs::read_dir(secrets_root) else { return };
    for entry in entries.flatten() {
        let app_id = entry.file_name().to_string_lossy().into_owned();
        if validate_app_id(&app_id).is_err() || !uninstalled(&app_id) {
            continue;
        }
        let only_index = std::fs::read_dir(entry.path())
            .map(|files| files.flatten().all(|f| f.file_name() == KEYCHAIN_INDEX))
            .unwrap_or(false);
        if only_index && purge(secrets_root, &app_id) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub mod keychain {
    use super::*;
    use sha2::{Digest, Sha256};

    const INDEX: &str = super::KEYCHAIN_INDEX;

    pub struct Keychain {
        files: FileSecrets,
        service: String,
    }

    impl Keychain {
        /// The item's service names the profile (its secrets root) too, so
        /// two OctoSense homes on one machine keep separate items.
        pub fn new(secrets_root: &Path, app_id: &str) -> Self {
            let root = std::fs::canonicalize(secrets_root).unwrap_or_else(|_| secrets_root.to_path_buf());
            let digest = Sha256::digest(root.to_string_lossy().as_bytes());
            let profile: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
            Self { files: FileSecrets::new(secrets_root, app_id), service: format!("OctoSense app {app_id} {profile}") }
        }

        fn entry(&self, key: &str) -> Result<keyring::Entry, StorageError> {
            keyring::Entry::new(&self.service, key).map_err(|e| StorageError::Io(format!("the keychain is unavailable: {e}")))
        }

        fn index(&self) -> Vec<String> {
            std::fs::read_to_string(self.files.dir().join(INDEX))
                .unwrap_or_default()
                .lines()
                .filter(|k| validate_key(k).is_ok())
                .map(str::to_owned)
                .collect()
        }

        fn write_index(&self, keys: &[String]) -> Result<(), StorageError> {
            super::super::ensure_private_dir(&self.files.root, self.files.dir()).map_err(io)?;
            write_private(&self.files.dir().join(INDEX), keys.join("\n").as_bytes())
        }

        pub fn purge(&self) {
            for key in self.index() {
                if let Ok(entry) = self.entry(&key) {
                    let _ = entry.delete_credential();
                }
            }
        }
    }

    impl SecretStore for Keychain {
        fn put(&self, key: &str, secret: &[u8]) -> Result<(), StorageError> {
            validate_key(key)?;
            let mut keys = self.index();
            if !keys.iter().any(|k| k == key) {
                keys.push(key.to_owned());
                self.write_index(&keys)?;
            }
            self.entry(key)?
                .set_secret(secret)
                .map_err(|e| StorageError::Io(format!("cannot store the secret in the keychain: {e}")))
        }
        fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
            validate_key(key)?;
            match self.entry(key)?.get_secret() {
                Ok(secret) => Ok(Some(secret)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(StorageError::Io(format!("cannot read the secret from the keychain: {e}"))),
            }
        }
        fn remove(&self, key: &str) -> Result<(), StorageError> {
            validate_key(key)?;
            match self.entry(key)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => return Err(StorageError::Io(format!("cannot delete the secret: {e}"))),
            }
            let keys: Vec<String> = self.index().into_iter().filter(|k| k != key).collect();
            self.write_index(&keys)
        }
    }
}
