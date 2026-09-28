//! App storage (ADR 0004 §11): the host's one source of every app's paths.
//!
//! ```text
//! <octosense home>/apps/<app id>/            the app's jail (App Hub's jail root; a native app's sandbox root)
//!     accounts/<account hash>/               one per account ("device" when the app has none):
//!                                            the account's data = that account's agent workspace
//!     common/                                app data not tied to an account
//!     cache/                                 evictable, not backed up
//! <octosense home>/secrets/<app id>/         host-owned: tokens, keys, passwords, encryption stores
//! ```
//!
//! - [`Layout`] computes the paths: the apps root is `$OCTOSENSE_APP_DATA`
//!   when set (App Hub honours the same variable), else `<home>/apps`; the
//!   secrets root is always `<home>/secrets`, and the two never overlap. The
//!   home is the platform's app data directory where there is one (a phone),
//!   else the OctoSense home (`octosense::paths::home`).
//! - [`account_hash`] names an account's folder (below).
//! - [`Storage`] creates the directories (0700, refusing symlinked
//!   components), holds each app's declared [`StorageSpec`], the signed-out
//!   accounts and what the startup check refused, and builds the
//!   [`AppStorage`] handle a native module is offered at creation
//!   (`module_host.rs`, through `octosense_app_peers::storage`).
//! - [`check`] is the startup check: no agent workspace may contain or link
//!   to the host's secrets.
//! - [`secrets`] is the host secrets API behind [`AppStorage::secrets`].
//!
//! **Signing out** (ADR §11) suspends the account's agent and never closes
//! it. The shell has no account system of its own yet: a module binds its
//! account through its assistant service (`OctosAppService::set_account`,
//! whose broker revokes the account's contexts and never calls
//! `peer_close`). [`Storage::sign_out`] / [`Storage::sign_in`] are the seam
//! the shell's account events will call (plan step 6: answering
//! `peer/tool/call` with `signed_out`, starting no turn); today they make
//! [`AppStorage::agent_workspace`] answer [`StorageError::SignedOut`] while
//! the account's folder and data stay. [`Storage::remove_account`] and
//! [`Storage::uninstall`] delete the folders and suspend the same way.

pub mod check;
pub mod secrets;
pub mod spec;

pub use crate::ai_host::app_peers::storage::{AppStorage, SecretStore, StorageError};
pub use spec::{AgentWorkspace, AppKind, StorageSpec};

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use sha2::{Digest, Sha256};

/// The folder of an app that declares no accounts (`storage.accounts: false`).
pub const DEVICE: &str = "device";
/// Hex digits of an account hash: 128 bits of SHA-256.
pub const ACCOUNT_HASH_LEN: usize = 32;
/// Domain separation for [`account_hash`]; bump the version to re-key.
const ACCOUNT_HASH_DOMAIN: &[u8] = b"octosense.account.v1\0";

/// An account id as it is hashed: surrounding whitespace trimmed and
/// lowercased (Unicode `to_lowercase`), so `Alice@Example.org ` and
/// `alice@example.org` are one account. No other folding (no Unicode
/// normalization): ids reach the host from the app that signed them in.
pub fn normalize_account(account: &str) -> String {
    account.trim().to_lowercase()
}

/// The name of an account's folder: the first [`ACCOUNT_HASH_LEN`] lowercase
/// hex digits of `SHA-256("octosense.account.v1\0" ‖ normalize_account(id))`.
///
/// **A compatibility contract.** The hash IS the name of every app's
/// per-account folder, i.e. that account's data and its agent's workspace.
/// Changing the domain string (the salt), [`normalize_account`] or the
/// truncation ([`ACCOUNT_HASH_LEN`]) renames every account folder: all
/// existing per-account data and agent workspaces are orphaned (still on
/// disk, no longer found). A change needs a migration that renames the
/// folders, shipped with it; `tests.rs` pins a value to catch it.
///
/// Stable (same id, same folder, on every device and build), non-reversible
/// (the folder name does not reveal the id; an unsalted hash still lets
/// someone who guesses the id confirm it), and never [`DEVICE`] (not hex).
/// It is the same for every app: the app id is already in the path.
pub fn account_hash(account: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ACCOUNT_HASH_DOMAIN);
    hasher.update(normalize_account(account).as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(ACCOUNT_HASH_LEN);
    for byte in digest.iter().take(ACCOUNT_HASH_LEN / 2) {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// An app id usable as one path component: `[A-Za-z0-9._-]{1,128}`,
/// starting with a letter or digit (App Hub keeps `.host`, `.system` and
/// `catalog.json` beside the jails), no `..`.
pub fn validate_app_id(id: &str) -> Result<(), String> {
    let ok_len = !id.is_empty() && id.len() <= 128;
    let ok_chars = id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    let ok_first = id.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric());
    if ok_len && ok_chars && ok_first && !id.contains("..") && id != "catalog.json" {
        Ok(())
    } else {
        Err(format!("invalid app id {id:?}"))
    }
}

/// Lexically normalized absolute path (`.` and `..` resolved, no file
/// system access).
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// `path` with symlinks resolved as far as it exists, the rest appended.
fn resolved(path: &Path) -> PathBuf {
    let path = lexical(path);
    let mut existing = path.as_path();
    let mut rest = Vec::new();
    loop {
        if let Ok(real) = std::fs::canonicalize(existing) {
            let mut out = real;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                existing = parent;
            }
            _ => return path,
        }
    }
}

fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

/// Where apps' storage lives. The only place the shell derives these paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    apps_root: PathBuf,
    secrets_root: PathBuf,
}

impl Layout {
    /// `<home>/apps` and `<home>/secrets`.
    pub fn new(home: &Path) -> Result<Self, String> {
        Self::with_roots(home.join("apps"), home.join("secrets"))
    }

    /// Explicit roots: both absolute, and neither inside the other (also
    /// after resolving symlinks), so a secret is never under `apps/`.
    pub fn with_roots(apps_root: PathBuf, secrets_root: PathBuf) -> Result<Self, String> {
        if !apps_root.is_absolute() || !secrets_root.is_absolute() {
            return Err(format!("storage roots must be absolute: {apps_root:?}, {secrets_root:?}"));
        }
        let (apps_root, secrets_root) = (lexical(&apps_root), lexical(&secrets_root));
        if overlaps(&apps_root, &secrets_root) || overlaps(&resolved(&apps_root), &resolved(&secrets_root)) {
            return Err(format!("the secrets root {secrets_root:?} overlaps the apps root {apps_root:?}"));
        }
        Ok(Self { apps_root, secrets_root })
    }

    /// This platform's layout: `data_dir` is the platform's app data
    /// directory (`cx.get_data_dir()`), else the OctoSense home.
    pub fn platform(data_dir: Option<PathBuf>) -> Result<Self, String> {
        let home = data_dir.unwrap_or_else(crate::octosense::paths::home);
        let apps_root = std::env::var_os("OCTOSENSE_APP_DATA")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("apps"));
        let apps_root = if apps_root.is_absolute() {
            apps_root
        } else {
            std::env::current_dir().unwrap_or_default().join(apps_root)
        };
        Self::with_roots(apps_root, home.join("secrets"))
    }

    pub fn apps_root(&self) -> &Path {
        &self.apps_root
    }

    pub fn secrets_root(&self) -> &Path {
        &self.secrets_root
    }

    /// The paths of `app_id` (nothing is created).
    pub fn app(&self, app_id: &str) -> Result<AppPaths, String> {
        validate_app_id(app_id)?;
        let jail = self.apps_root.join(app_id);
        Ok(AppPaths {
            app_id: app_id.to_owned(),
            accounts: jail.join("accounts"),
            common: jail.join("common"),
            cache: jail.join("cache"),
            secrets: self.secrets_root.join(app_id),
            jail,
        })
    }
}

/// One app's paths, as [`Layout::app`] computes them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppPaths {
    pub app_id: String,
    pub jail: PathBuf,
    pub accounts: PathBuf,
    pub common: PathBuf,
    pub cache: PathBuf,
    pub secrets: PathBuf,
}

impl AppPaths {
    /// `accounts/<account hash>/`, or `accounts/device/` for `None`.
    pub fn account(&self, account: Option<&str>) -> PathBuf {
        match account {
            Some(id) => self.accounts.join(account_hash(id)),
            None => self.accounts.join(DEVICE),
        }
    }
}

/// Create `dir` below `root` (both created as needed) as owner-only
/// directories (0700 on Unix; TODO: an owner-only ACL on Windows). Every
/// component from `root` down must be a real directory: a symlink there
/// would move the jail (or a secret) somewhere else, so it is refused.
pub fn ensure_private_dir(root: &Path, dir: &Path) -> io::Result<()> {
    let rel = dir.strip_prefix(root).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, format!("{dir:?} is outside {root:?}"))
    })?;
    if !root.is_dir() {
        std::fs::create_dir_all(root)?;
    }
    let mut at = root.to_path_buf();
    for part in rel.components() {
        let Component::Normal(part) = part else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{dir:?} is not a plain path")));
        };
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{at:?} is a symlink")));
            }
            Ok(meta) if !meta.is_dir() => {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{at:?} is not a directory")));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let mut builder = std::fs::DirBuilder::new();
                #[cfg(unix)]
                std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
                builder.create(&at)?;
            }
            Err(e) => return Err(e),
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

fn io_err(e: impl std::fmt::Display) -> StorageError {
    StorageError::Io(e.to_string())
}

#[derive(Default)]
struct State {
    specs: HashMap<String, StorageSpec>,
    /// (app id, account folder name) whose agent is suspended.
    signed_out: HashSet<(String, String)>,
    /// (app id, account folder name) the startup check refused, and why.
    refused: HashMap<(String, String), String>,
}

/// The host's app storage: the layout plus what it enforces.
pub struct Storage {
    layout: Layout,
    state: Mutex<State>,
    /// Test hook: always use owner-only files for secrets.
    file_secrets: bool,
}

impl Storage {
    pub fn new(layout: Layout) -> Arc<Self> {
        Arc::new(Self { layout, state: Mutex::default(), file_secrets: false })
    }

    /// Secrets in owner-only files even where a vault exists (tests,
    /// headless runs).
    pub fn with_file_secrets(layout: Layout) -> Arc<Self> {
        Arc::new(Self { layout, state: Mutex::default(), file_secrets: true })
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Record `app_id`'s declared storage block (from its manifest). An app
    /// without one gets [`StorageSpec::default`].
    pub fn set_spec(&self, app_id: &str, spec: StorageSpec) {
        self.state().specs.insert(app_id.to_owned(), spec);
    }

    pub fn spec(&self, app_id: &str) -> StorageSpec {
        self.state().specs.get(app_id).cloned().unwrap_or_default()
    }

    /// Lay out `app_id`'s jail (`accounts/`, `common/`, `cache/`) and its
    /// secrets directory, and return the handle a module is offered.
    pub fn open(self: &Arc<Self>, app_id: &str) -> Result<Arc<dyn AppStorage>, StorageError> {
        let paths = self.layout.app(app_id).map_err(StorageError::Io)?;
        let apps = self.layout.apps_root();
        for dir in [&paths.jail, &paths.accounts, &paths.common, &paths.cache] {
            ensure_private_dir(apps, dir).map_err(io_err)?;
        }
        ensure_private_dir(self.layout.secrets_root(), &paths.secrets).map_err(io_err)?;
        let secrets: Arc<dyn SecretStore> = if self.file_secrets {
            Arc::new(secrets::FileSecrets::new(self.layout.secrets_root(), app_id))
        } else {
            secrets::platform(self.layout.secrets_root(), app_id)
        };
        Ok(Arc::new(HostAppStorage { storage: self.clone(), spec: self.spec(app_id), paths, secrets }))
    }

    fn key(app_id: &str, account: Option<&str>) -> (String, String) {
        (app_id.to_owned(), account.map(account_hash).unwrap_or_else(|| DEVICE.to_owned()))
    }

    /// The account signed out: its agent is suspended (no workspace, see
    /// the module docs); its data stays.
    pub fn sign_out(&self, app_id: &str, account: Option<&str>) {
        self.state().signed_out.insert(Self::key(app_id, account));
    }

    /// The account signed in again: the same agent resumes.
    pub fn sign_in(&self, app_id: &str, account: Option<&str>) {
        self.state().signed_out.remove(&Self::key(app_id, account));
    }

    pub fn is_signed_out(&self, app_id: &str, account: Option<&str>) -> bool {
        self.state().signed_out.contains(&Self::key(app_id, account))
    }

    /// Removing an account: its folder goes and its agent is suspended
    /// (octos keeps the peer's memory until it has a `peer/purge`).
    pub fn remove_account(&self, app_id: &str, account: Option<&str>) -> Result<(), StorageError> {
        let paths = self.layout.app(app_id).map_err(StorageError::Io)?;
        self.sign_out(app_id, account);
        remove_tree(&paths.account(account))
    }

    /// Uninstalling: `apps/<app id>/` and `secrets/<app id>/` go (vault
    /// items too) and every account of the app stays suspended.
    pub fn uninstall(&self, app_id: &str) -> Result<(), StorageError> {
        let paths = self.layout.app(app_id).map_err(StorageError::Io)?;
        if let Ok(entries) = std::fs::read_dir(&paths.accounts) {
            let mut state = self.state();
            for entry in entries.flatten() {
                state.signed_out.insert((app_id.to_owned(), entry.file_name().to_string_lossy().into_owned()));
            }
        }
        secrets::purge(self.layout.secrets_root(), app_id);
        remove_tree(&paths.jail)?;
        remove_tree(&paths.secrets)
    }

    /// Run the startup check and refuse every workspace it flags.
    pub fn startup_check(&self) -> check::Report {
        let report = check::check_workspaces(&self.layout);
        let mut state = self.state();
        state.refused.clear();
        for v in &report.violations {
            state.refused.insert((v.app_id.clone(), v.account.clone()), v.reason.clone());
        }
        report
    }

    /// Why the startup check refused this account's workspace, if it did
    /// (a jail-level finding refuses every account of the app).
    pub fn refused(&self, app_id: &str, account: Option<&str>) -> Option<String> {
        let state = self.state();
        state
            .refused
            .get(&Self::key(app_id, account))
            .or_else(|| state.refused.get(&(app_id.to_owned(), check::EVERY_ACCOUNT.to_owned())))
            .cloned()
    }
}

/// Remove a directory tree without following a symlink at its top.
fn remove_tree(path: &Path) -> Result<(), StorageError> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => std::fs::remove_file(path).map_err(io_err),
        Ok(_) => std::fs::remove_dir_all(path).map_err(io_err),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(e)),
    }
}

/// The handle one app's module holds.
struct HostAppStorage {
    storage: Arc<Storage>,
    spec: StorageSpec,
    paths: AppPaths,
    secrets: Arc<dyn SecretStore>,
}

impl AppStorage for HostAppStorage {
    fn app_id(&self) -> &str {
        &self.paths.app_id
    }
    fn jail(&self) -> &Path {
        &self.paths.jail
    }
    fn common(&self) -> &Path {
        &self.paths.common
    }
    fn cache(&self) -> &Path {
        &self.paths.cache
    }
    fn has_accounts(&self) -> bool {
        self.spec.accounts
    }
    fn account_folder(&self, account: Option<&str>) -> Result<PathBuf, StorageError> {
        match (self.spec.accounts, account) {
            (true, None) => return Err(StorageError::Accounts("the app keeps data per account; name one".into())),
            (false, Some(_)) => return Err(StorageError::Accounts("the app declares no accounts; use the device folder".into())),
            (true, Some(id)) if normalize_account(id).is_empty() => {
                return Err(StorageError::Accounts("empty account id".into()));
            }
            _ => {}
        }
        let dir = self.paths.account(account);
        ensure_private_dir(self.storage.layout.apps_root(), &dir).map_err(io_err)?;
        Ok(dir)
    }
    fn agent_workspace(&self, account: Option<&str>) -> Result<PathBuf, StorageError> {
        if self.spec.agent_workspace == AgentWorkspace::None {
            return Err(StorageError::NoWorkspace);
        }
        if self.storage.is_signed_out(&self.paths.app_id, account) {
            return Err(StorageError::SignedOut);
        }
        if let Some(why) = self.storage.refused(&self.paths.app_id, account) {
            return Err(StorageError::Refused(why));
        }
        self.account_folder(account)
    }
    fn secrets(&self) -> &dyn SecretStore {
        &*self.secrets
    }
}

static HOST: OnceLock<Arc<Storage>> = OnceLock::new();

/// Set up the host's storage once at startup (`handle_startup`): compute
/// the layout, run the startup check and log what it refused. Later calls
/// return the first result.
pub fn init(data_dir: Option<PathBuf>) -> Option<&'static Arc<Storage>> {
    if let Some(host) = HOST.get() {
        return Some(host);
    }
    let layout = match Layout::platform(data_dir) {
        Ok(layout) => layout,
        Err(e) => {
            makepad_widgets::log!("app storage: {e}; no app storage this run");
            return None;
        }
    };
    // The secrets backend (keychain or files) is chosen per run by
    // `secrets::select_backend`: never the keychain headless or in tests.
    let storage = Storage::new(layout);
    let report = storage.startup_check();
    report.log();
    Some(HOST.get_or_init(|| storage))
}

/// The host's storage, once [`init`] ran (never in unit tests that do not
/// set it up, so they write nothing under a real home).
pub fn host() -> Option<&'static Arc<Storage>> {
    HOST.get()
}

/// Offer `module`'s storage to the instance `scope` for its `create`, when
/// it declares the `storage` capability and the host storage is set up.
/// Returns whether an offer was made; the caller withdraws it afterwards.
pub fn offer(module_id: &str, capabilities: &[&str], scope: &str) -> bool {
    let Some(host) = host() else { return false };
    if !capabilities.contains(&"storage") {
        return false;
    }
    match host.open(module_id) {
        Ok(storage) => {
            crate::ai_host::app_peers::storage::offer(module_id, scope, storage);
            true
        }
        Err(e) => {
            makepad_widgets::log!("app storage: {module_id}: {e}");
            false
        }
    }
}

/// Drop what the module did not claim.
pub fn withdraw(module_id: &str, scope: &str) {
    crate::ai_host::app_peers::storage::withdraw(module_id, scope);
}

#[cfg(test)]
pub(crate) mod tests;
