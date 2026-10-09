//! Host-only app/account consent and opaque calendar bindings.
use super::model::Calendar;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
// Only pooled workers acquire DISK. UI reads clone a small cached snapshot and
// never wait for fsync. Initial load is capped before allocating or parsing.
static DISK: Mutex<()> = Mutex::new(());
fn cache() -> &'static Mutex<BTreeMap<PathBuf, Store>> {
    static CACHE: OnceLock<Mutex<BTreeMap<PathBuf, Store>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}
const FILE: &str = "device-calendar-consent.json";
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct Grant {
    pub allowed: bool,
    pub revision: u64,
    pub selections: BTreeMap<String, Calendar>,
}
#[derive(Clone, Default, Deserialize, Serialize)]
struct Store {
    grants: BTreeMap<String, BTreeMap<String, Grant>>,
}
const MAX_STORE: usize = 1 << 20;
fn read(root: &Path) -> Result<Store, String> {
    let mut file = match std::fs::File::open(root.join(FILE)) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Store::default()),
        Err(_) => return Err("storage_error: Cannot read calendar consent".into()),
    };
    let mut bytes = Vec::new();
    (&mut file)
        .take((MAX_STORE + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "storage_error: Cannot read calendar consent")?;
    if bytes.len() > MAX_STORE {
        return Err("invalid_store: Calendar consent exceeds its limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "invalid_store: Calendar consent is invalid".into())
}
pub(super) fn load(root: &Path) -> Result<(), String> {
    if cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key(root)
    {
        return Ok(());
    }
    let disk = read(root)?;
    let mut cache = cache().lock().unwrap_or_else(|e| e.into_inner());
    // A worker may have filled/updated the cache while the cold read ran.
    // The global maximum also bounds account metadata across many app roots.
    if !cache.contains_key(root) && cache.len() >= 64 {
        return Err("limit: Too many active calendar app stores; restart the host".into());
    }
    cache.entry(root.into()).or_insert(disk);
    Ok(())
}
fn snapshot(root: &Path) -> Result<Store, String> {
    load(root)?;
    Ok(cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(root)
        .unwrap()
        .clone())
}
pub(super) fn get(root: &Path, app: &str, scope: &str) -> Result<Grant, String> {
    // Called by UI and workers: never wait for a worker-held mutex, and never
    // perform cold disk IO. permission.status warms this cache on the pool.
    let cache = cache()
        .try_lock()
        .map_err(|_| "busy: Calendar consent snapshot is updating; retry")?;
    let store = cache
        .get(root)
        .ok_or("cache_uninitialized: Call permission.status to initialize calendar access")?;
    Ok(store
        .grants
        .get(app)
        .and_then(|accounts| accounts.get(scope))
        .cloned()
        .unwrap_or_default())
}
pub(super) fn change(
    root: &Path,
    app: &str,
    scope: &str,
    expected: Option<u64>,
    f: impl FnOnce(&mut Grant) -> Result<(), String>,
) -> Result<Grant, String> {
    let _disk = DISK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = snapshot(root)?;
    let grant = store
        .grants
        .entry(app.into())
        .or_default()
        .entry(scope.into())
        .or_default();
    if expected.is_some_and(|revision| grant.revision != revision) {
        return Err("stale_consent: Calendar access changed; review again".into());
    }
    f(grant)?;
    grant.revision = grant
        .revision
        .checked_add(1)
        .ok_or("limit: Consent revision exhausted")?;
    let result = grant.clone();
    let bytes = serde_json::to_vec(&store).map_err(|_| "storage_error: Cannot encode consent")?;
    if bytes.len() > 1 << 20 {
        return Err("limit: Too many calendar consent records".into());
    }
    std::fs::create_dir_all(root).map_err(|_| "storage_error: Cannot create host directory")?;
    let temporary = root.join(format!(".device-calendar-{}.tmp", uuid::Uuid::new_v4()));
    let write = || -> Result<(), String> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| "storage_error: Cannot create consent record")?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "storage_error: Cannot write consent")?;
        std::fs::rename(&temporary, root.join(FILE))
            .map_err(|_| "storage_error: Cannot replace consent")?;
        Ok(())
    };
    let outcome = write();
    if outcome.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    outcome?;
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(root.into(), store);
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    static SERIAL: Mutex<()> = Mutex::new(());
    #[test]
    fn ui_cache_reads_never_wait_for_workers_or_read_cold_storage() {
        let _serial = SERIAL.lock().unwrap();
        let root = std::env::temp_dir().join(format!("calendar-unloaded-{}", uuid::Uuid::new_v4()));
        assert!(get(&root, "app", "scope")
            .unwrap_err()
            .starts_with("cache_uninitialized:"));
        assert!(!root.exists());
        let (ready, wait) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let _cache = cache().lock().unwrap();
            ready.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(100));
        });
        wait.recv().unwrap();
        let result = get(&root, "app", "scope");
        worker.join().unwrap();
        assert!(result.unwrap_err().starts_with("busy:"));
    }
    #[test]
    fn oversized_store_is_rejected_before_deserialization() {
        let _serial = SERIAL.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("device-calendar-bounded-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let file = std::fs::File::create(root.join(FILE)).unwrap();
        file.set_len((MAX_STORE + 1) as u64).unwrap();
        assert!(load(&root).unwrap_err().starts_with("invalid_store"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn consent_and_handles_do_not_cross_apps_accounts_or_revocations() {
        let _serial = SERIAL.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "device-calendar-synthetic-{}",
            uuid::Uuid::new_v4()
        ));
        let calendar = Calendar {
            id: "synthetic".into(),
            account: "synthetic-source".into(),
            name: "Test".into(),
            account_name: "Fixture".into(),
            writable: true,
        };
        let granted = change(&root, "app.a", "account.a", Some(0), |g| {
            g.allowed = true;
            g.selections.insert("opaque".into(), calendar);
            Ok(())
        })
        .unwrap();
        assert!(!get(&root, "app.b", "account.a").unwrap().allowed);
        assert!(!get(&root, "app.a", "account.b").unwrap().allowed);
        change(&root, "app.a", "account.a", None, |g| {
            g.allowed = false;
            g.selections.clear();
            Ok(())
        })
        .unwrap();
        assert!(
            change(&root, "app.a", "account.a", Some(granted.revision), |_| Ok(
                ()
            ))
            .is_err()
        );
        assert!(get(&root, "app.a", "account.a")
            .unwrap()
            .selections
            .is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
