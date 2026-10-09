//! Host-owned consent for the device host APIs. No tokens or OS grants live here.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

static LOCK: Mutex<()> = Mutex::new(());
type Cache = BTreeMap<(PathBuf, String, String), Grant>;
fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}
fn remember(root: &Path, app: &str, family: &str, grant: Grant) {
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert((root.to_owned(), app.into(), family.into()), grant);
}
pub(super) fn cached(root: &Path, app: &str, family: &str) -> bool {
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(root.to_owned(), app.into(), family.into()))
        .is_some_and(|g| g.allowed)
}
pub(super) fn cached_revision(root: &Path, app: &str, family: &str) -> Option<u64> {
    cache()
        .try_lock()
        .ok()?
        .get(&(root.to_owned(), app.into(), family.into()))
        .filter(|grant| grant.allowed)
        .map(|grant| grant.revision)
}
const FILE: &str = "device-api-consent.json";

#[derive(Clone, Copy, Default, Debug, Deserialize, Serialize, PartialEq)]
pub(super) struct Grant {
    pub allowed: bool,
    pub revision: u64,
}
#[derive(Default, Deserialize, Serialize)]
struct Store {
    #[serde(default)]
    grants: BTreeMap<String, BTreeMap<String, Grant>>,
}
fn read(root: &Path) -> Result<Store, String> {
    match fs::read(root.join(FILE)) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|_| "Device consent store is invalid".into())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
        Err(_) => Err("Cannot read device consent store".into()),
    }
}
pub(super) fn get(root: &Path, app: &str, family: &str) -> Result<Grant, String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let store = match read(root) {
        Ok(store) => store,
        Err(error) => {
            remember(root, app, family, Grant::default());
            return Err(error);
        }
    };
    let grant = store
        .grants
        .get(app)
        .and_then(|g| g.get(family))
        .copied()
        .unwrap_or_default();
    remember(root, app, family, grant);
    Ok(grant)
}
/// A delayed consent dialog cannot undo a revocation or newer consent decision.
pub(super) fn set(
    root: &Path,
    app: &str,
    family: &str,
    allowed: bool,
    expected: Option<u64>,
) -> Result<Grant, String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = read(root)?;
    let grant = store
        .grants
        .entry(app.into())
        .or_default()
        .entry(family.into())
        .or_default();
    if expected.is_some_and(|revision| revision != grant.revision) {
        return Err("Consent changed; request permission again".into());
    }
    *grant = Grant {
        allowed,
        revision: grant
            .revision
            .checked_add(1)
            .ok_or("Consent revision exhausted")?,
    };
    let result = *grant;
    fs::create_dir_all(root).map_err(|_| "Cannot create host consent directory")?;
    let temporary = root.join(format!(".device-consent-{}.tmp", uuid::Uuid::new_v4()));
    let write = || -> Result<(), String> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| "Cannot create consent record")?;
        let bytes = serde_json::to_vec(&store).map_err(|_| "Cannot encode consent record")?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "Cannot save consent record")?;
        fs::rename(&temporary, root.join(FILE)).map_err(|_| "Cannot replace consent record")?;
        Ok(())
    };
    let result_write = write();
    if result_write.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result_write?;
    remember(root, app, family, result);
    makepad_widgets::makepad_platform::SignalToUI::set_ui_signal();
    Ok(result)
}
