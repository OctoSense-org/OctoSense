//! The host secrets API (ADR 0004 §11): an app's tokens, keys and passwords,
//! kept by the host and never under `apps/`.
//!
//! - macOS and iOS: the keychain (the `keyring` crate, as Mail's vault),
//!   one item per (profile, app, key); the key names are listed in
//!   `secrets/<app id>/.keychain-index` so uninstalling can delete them.
//! - Elsewhere, and with `OCTOSENSE_SECRETS=file` (unsigned development
//!   builds, where macOS asks again for every rebuilt binary): one
//!   owner-only (0600) file per key in `secrets/<app id>/`, a 0700
//!   directory. TODO: the platform vault on Windows (Credential Manager),
//!   Linux (Secret Service) and Android (Keystore-wrapped files, as Mail's
//!   vault does).

use super::{validate_app_id, StorageError};
use crate::ai_host::app_peers::storage::SecretStore;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

/// The store for this platform (see the module docs).
pub fn platform(secrets_root: &Path, app_id: &str) -> Arc<dyn SecretStore> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    if !std::env::var("OCTOSENSE_SECRETS").is_ok_and(|v| v == "file") {
        return Arc::new(keychain::Keychain::new(secrets_root, app_id));
    }
    Arc::new(FileSecrets::new(secrets_root, app_id))
}

/// Delete every secret of `app_id`: its vault items (those the index
/// names). The caller removes `secrets/<app id>/` itself.
pub fn purge(secrets_root: &Path, app_id: &str) {
    if validate_app_id(app_id).is_err() {
        return;
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    keychain::Keychain::new(secrets_root, app_id).purge();
    let _ = secrets_root;
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub mod keychain {
    use super::*;
    use sha2::{Digest, Sha256};

    const INDEX: &str = ".keychain-index";

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
