//! The app storage contract a shell hands an in-process native module
//! (ADR 0004 §11): the app's jail, its account folders, `common/`, `cache/`
//! and its host-kept secrets.
//!
//! ```text
//! <octosense home>/apps/<app id>/            the app's jail
//!     accounts/<account hash>/               one per account ("device" when the app has none):
//!                                            the account's data = its agent workspace
//!     common/                                data not tied to an account
//!     cache/                                 evictable, not backed up
//! <octosense home>/secrets/<app id>/         host-owned: tokens, keys, passwords
//! ```
//!
//! **Paths come from the host, never hard-coded.** The shell computes every
//! one of them (`octosense_shell::app_storage` is the only source) and hands
//! the module an [`AppStorage`] at creation, exactly like the assistant
//! service ([`crate::injection`]): [`offer`]ed under the module id and
//! instance scope right before `AppModule::create`, [`claim`]ed inside it,
//! [`withdraw`]n right after. A module the shell did not offer storage to
//! gets none.
//!
//! Secrets never live in the jail: a module keeps them through
//! [`AppStorage::secrets`], which the host stores in the platform vault or
//! under `secrets/<app id>/`, never under `apps/`.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// An account id as the host keys it, everywhere: surrounding whitespace
/// trimmed and lowercased (Unicode `to_lowercase`), so `Alice@Example.org `
/// and `alice@example.org` are one account. The account folder's name
/// (`octosense_shell::app_storage::account_hash`) and the agent's memory tag
/// (`broker::account_tag`) both hash this. No other folding: ids reach the
/// host from the app that signed them in.
pub fn normalize_account(account: &str) -> String {
    account.trim().to_lowercase()
}

/// Why a storage call was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageError {
    /// The call does not fit the app's declared `storage.accounts`: an
    /// account's folder for an app without accounts, the device folder for
    /// an app with them, or an empty account id.
    Accounts(String),
    /// The account is signed out: its agent is suspended (never closed) and
    /// gets no workspace until the account signs in again.
    SignedOut,
    /// The app declares `agent_workspace: "none"`: its agent reads no files.
    NoWorkspace,
    /// The startup check found this workspace reaching the host's secrets;
    /// the agent gets no workspace until the next start finds it clean.
    Refused(String),
    /// A secret key outside `[A-Za-z0-9._-]{1,128}` (or starting with `.`).
    InvalidKey(String),
    /// The file system or the vault failed.
    Io(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Accounts(why) => write!(f, "storage accounts: {why}"),
            StorageError::SignedOut => write!(f, "signed_out"),
            StorageError::NoWorkspace => write!(f, "the app's agent has no file workspace"),
            StorageError::Refused(why) => write!(f, "workspace refused: {why}"),
            StorageError::InvalidKey(key) => write!(f, "invalid secret key {key:?}"),
            StorageError::Io(why) => write!(f, "storage: {why}"),
        }
    }
}

impl std::error::Error for StorageError {}

/// The app's host-kept secrets: the platform vault (the keychain on macOS
/// and iOS) or owner-only files under `secrets/<app id>/`. Keys are
/// `[A-Za-z0-9._-]{1,128}`, not starting with `.`.
pub trait SecretStore: Send + Sync {
    fn put(&self, key: &str, secret: &[u8]) -> Result<(), StorageError>;
    /// `Ok(None)`: no secret under `key`.
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;
    /// Removing a missing key is not an error.
    fn remove(&self, key: &str) -> Result<(), StorageError>;
}

/// One app's storage, as the host laid it out. Every directory exists (mode
/// 0700 where the platform has modes) by the time the path is returned.
pub trait AppStorage: Send + Sync {
    fn app_id(&self) -> &str;
    /// `apps/<app id>/`: everything the app may keep on disk is below it.
    fn jail(&self) -> &Path;
    /// `apps/<app id>/common/`: data not tied to an account.
    fn common(&self) -> &Path;
    /// `apps/<app id>/cache/`: evictable, not backed up.
    fn cache(&self) -> &Path;
    /// Whether the app keeps data per account (`storage.accounts`).
    fn has_accounts(&self) -> bool;
    /// The app's folder for `account` (`accounts/<account hash>/`), or its
    /// one `accounts/device/` (`None`, for an app without accounts). The app
    /// may use it while the account is signed out.
    fn account_folder(&self, account: Option<&str>) -> Result<PathBuf, StorageError>;
    /// The account's agent workspace (the `cwd` of its peer): its folder,
    /// unless the app's agent has none, the account is signed out, or the
    /// startup check refused it.
    fn agent_workspace(&self, account: Option<&str>) -> Result<PathBuf, StorageError>;
    /// The app's secrets. Never under `apps/`.
    fn secrets(&self) -> &dyn SecretStore;
}

type Offers = Mutex<HashMap<(String, String), Arc<dyn AppStorage>>>;

fn offers() -> &'static Offers {
    static OFFERS: OnceLock<Offers> = OnceLock::new();
    OFFERS.get_or_init(Default::default)
}

/// Offer `storage` to the instance `scope` of `module` (the shell, just
/// before `create`). A second offer for the same instance replaces the first.
pub fn offer(module: &str, scope: &str, storage: Arc<dyn AppStorage>) {
    offers()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert((module.to_owned(), scope.to_owned()), storage);
}

/// Take the storage offered to this instance (the module, inside `create`).
/// `None`: the host offered none.
pub fn claim(module: &str, scope: &str) -> Option<Arc<dyn AppStorage>> {
    offers()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&(module.to_owned(), scope.to_owned()))
}

/// Drop an unclaimed offer (the shell, right after `create`). Returns whether
/// one was left.
pub fn withdraw(module: &str, scope: &str) -> bool {
    offers()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&(module.to_owned(), scope.to_owned()))
        .is_some()
}

/// Told when an app's bound account changes: `(app id, previous, current)`,
/// the app id as its assistant service knows it (a contained app's is its
/// peer id, `card.<manifest id>`). The shell's account lifecycle (ADR 0004
/// §11): `Some(a)` → `None` signs `a` out (its agent is suspended, never
/// closed), `→ Some(b)` signs `b` in (its folder opened, its agent
/// resumed). A switch `Some(a)` → `Some(b)` signs `b` in and leaves `a` as it
/// was: an app that ends a session says so with `None` first.
pub type AccountObserver = Arc<dyn Fn(&str, Option<&str>, Option<&str>) + Send + Sync>;

fn account_observer() -> &'static Mutex<Option<AccountObserver>> {
    static OBSERVER: OnceLock<Mutex<Option<AccountObserver>>> = OnceLock::new();
    OBSERVER.get_or_init(Default::default)
}

/// Install (or with `None` remove) the process's account observer (the
/// shell, once at startup). A standalone app installs none.
pub fn observe_accounts(observer: Option<AccountObserver>) {
    *account_observer().lock().unwrap_or_else(|e| e.into_inner()) = observer;
}

/// An app's assistant service bound a new account (`set_account`), before
/// it revokes the previous account's contexts or prepares the new peer, so
/// the host's suspension state is current when the broker asks for it.
/// Called outside every lock; a no-op without an observer or a change.
pub fn account_changed(app_id: &str, previous: Option<&str>, current: Option<&str>) {
    if previous == current {
        return;
    }
    let observer = account_observer().lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(observer) = observer {
        observer(app_id, previous, current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoSecrets;
    impl SecretStore for NoSecrets {
        fn put(&self, _: &str, _: &[u8]) -> Result<(), StorageError> {
            Err(StorageError::Io("none".into()))
        }
        fn get(&self, _: &str) -> Result<Option<Vec<u8>>, StorageError> {
            Ok(None)
        }
        fn remove(&self, _: &str) -> Result<(), StorageError> {
            Ok(())
        }
    }
    struct Fixed(PathBuf);
    impl AppStorage for Fixed {
        fn app_id(&self) -> &str {
            "probe"
        }
        fn jail(&self) -> &Path {
            &self.0
        }
        fn common(&self) -> &Path {
            &self.0
        }
        fn cache(&self) -> &Path {
            &self.0
        }
        fn has_accounts(&self) -> bool {
            false
        }
        fn account_folder(&self, _: Option<&str>) -> Result<PathBuf, StorageError> {
            Ok(self.0.clone())
        }
        fn agent_workspace(&self, _: Option<&str>) -> Result<PathBuf, StorageError> {
            Err(StorageError::NoWorkspace)
        }
        fn secrets(&self) -> &dyn SecretStore {
            &NoSecrets
        }
    }

    #[test]
    fn storage_reaches_only_its_instance_once() {
        offer("probe", "i1g1", Arc::new(Fixed("/x".into())));
        assert!(claim("probe", "i1g2").is_none(), "another instance gets nothing");
        assert!(claim("other", "i1g1").is_none(), "another module gets nothing");
        assert_eq!(claim("probe", "i1g1").unwrap().jail(), Path::new("/x"));
        assert!(claim("probe", "i1g1").is_none(), "claimed once");
        offer("probe", "i2g1", Arc::new(Fixed("/y".into())));
        assert!(withdraw("probe", "i2g1"));
        assert!(!withdraw("probe", "i2g1"));
    }

    #[test]
    fn account_changes_reach_the_observer_once_per_change() {
        let seen: Arc<Mutex<Vec<String>>> = Arc::default();
        let log = seen.clone();
        observe_accounts(Some(Arc::new(move |app: &str, from: Option<&str>, to: Option<&str>| {
            if app == "observer-probe" {
                log.lock().unwrap().push(format!("{from:?}->{to:?}"));
            }
        })));
        account_changed("observer-probe", None, Some("a"));
        account_changed("observer-probe", Some("a"), Some("a"));
        account_changed("observer-probe", Some("a"), None);
        observe_accounts(None);
        account_changed("observer-probe", None, Some("b"));
        assert_eq!(*seen.lock().unwrap(), vec!["None->Some(\"a\")", "Some(\"a\")->None"]);
    }

    #[test]
    fn should_normalize_an_account_by_trimming_and_lowercasing_it() {
        assert_eq!(normalize_account("  Alice@Example.ORG\n"), "alice@example.org");
    }

    #[test]
    fn signed_out_reads_as_the_wire_answer() {
        assert_eq!(StorageError::SignedOut.to_string(), "signed_out");
    }
}
