//! Where the manifests and the accounts reach the host's storage (ADR 0004
//! §11). Every function here takes the [`Storage`] it acts on, so tests run
//! on scratch homes; the shell passes [`super::host`].
//!
//! - **Manifests.** A native app's `native-apps.json` `storage` block
//!   ([`register_native_specs`], at startup, before any module is created)
//!   and a script app's App Hub `manifest.json` block ([`prepare_script_app`],
//!   at install and at every launch) are parsed with [`StorageSpec`] and
//!   recorded with [`Storage::set_spec`]; [`Storage::open`] then lays out the
//!   jail, the account folders, `common/`, `cache/` and the secrets from it.
//!   A block the host refuses leaves the app on the default (one `device`
//!   folder) and is logged: a reviewed native entry is checked by
//!   `tools/native_apps.py` first, and App Hub refuses a bad script block at
//!   install.
//! - **Quotas.** A native app's jail is measured when it opens and a warning
//!   is logged over its `max_bytes` / `cache_max_bytes` ([`check_quota`]);
//!   a script app's isolate enforces its own.
//! - **Accounts.** [`account_changed`] is the broker's report of an app's
//!   bound account (Rinx's Matrix login and logout, through
//!   `OctosAppService::set_account`); [`mail_account`] is Mail's host service
//!   adding or removing an account for an app; [`app_uninstalled`] is App
//!   Hub's uninstall. They sign in (open the account's folder; the broker
//!   then resumes its peer), sign out (suspend: the broker closes the
//!   request contexts, `crate::host_tools` answers `signed_out` and starts no
//!   turn), remove an account or uninstall (delete the folders, stay
//!   suspended), then erase the agent: [`set_purger`]'s purger asks octos
//!   to `peer/purge` each (app, account) peer the host recorded (octos#2649,
//!   in the background, a busy peer retried) and drops the record; once it
//!   succeeded the storage notes it ([`Storage::mark_erased`],
//!   [`Storage::mark_app_erased`]). The account STAYS suspended until it is
//!   added again (then it gets a new agent, the record being gone); only
//!   [`memory_notice`] stops saying the memory remains. A failed purge
//!   keeps the record for a later one. Signing out keeps the agent.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::{quota_warnings, AppKind, Storage, StorageSpec};

/// The storage app id behind an assistant-service app id: a contained
/// (Card runner) app's peer is `card.<manifest id>`.
pub fn app_of(service_app: &str) -> &str {
    service_app.strip_prefix(crate::ai_host::contained::PEER_PREFIX).unwrap_or(service_app)
}

/// A native entry's `storage` block, as the shell parses it.
pub fn native_spec(app: &crate::native_apps::NativeApp) -> Result<StorageSpec, String> {
    let block: Value = serde_json::from_str(app.storage).map_err(|e| format!("{}: storage: {e}", app.id))?;
    StorageSpec::parse(Some(&block), AppKind::Native).map_err(|e| format!("{}: {e}", app.id))
}

/// Record every native app's declared storage (startup).
pub fn register_native_specs(storage: &Storage) {
    for app in crate::native_apps::APPS {
        match native_spec(app) {
            Ok(spec) => storage.set_spec(app.id, spec),
            Err(e) => makepad_widgets::log!("app storage: {e}; it keeps one device folder"),
        }
    }
}

/// A script app's `manifest.json` under App Hub's data root: an installed
/// app's `<root>/<id>/bundle/`, or the newest unpacked build of a system app
/// (`<root>/.system/<id>/<build>/`). `None` before either exists (a system
/// app the Card runner has not unpacked yet).
pub fn script_manifest(root: &Path, manifest_id: &str) -> Option<Value> {
    super::validate_app_id(manifest_id).ok()?;
    let installed = root.join(manifest_id).join("bundle").join("manifest.json");
    let path = if installed.is_file() {
        installed
    } else {
        let builds = std::fs::read_dir(root.join(".system").join(manifest_id)).ok()?;
        builds
            .flatten()
            .map(|b| b.path().join("manifest.json"))
            .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
            .max()?
            .1
    };
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// A script app is installed or launched: parse its block, record it and
/// lay out its folders. Installed again after an uninstall, its device agent
/// resumes; an app with accounts resumes each as it signs in
/// ([`Storage::installed`]).
pub fn prepare_script_app(storage: &Arc<Storage>, root: &Path, manifest_id: &str) -> Result<StorageSpec, String> {
    // A native app's folders and spec are never a script app's to set.
    crate::apps::check_script_app_id(manifest_id)?;
    let spec = match script_manifest(root, manifest_id) {
        Some(manifest) => StorageSpec::from_manifest(&manifest, AppKind::Script).map_err(|e| format!("{manifest_id}: {e}"))?,
        None => StorageSpec::default(),
    };
    storage.set_spec(manifest_id, spec.clone());
    storage.installed(manifest_id);
    storage.open(manifest_id).map_err(|e| format!("{manifest_id}: {e}"))?;
    Ok(spec)
}

/// Measure `app_id` against its declared ceilings; the warnings, logged.
pub fn check_quota(storage: &Storage, app_id: &str) -> Vec<String> {
    let spec = storage.spec(app_id);
    if spec.max_bytes.is_none() && spec.cache_max_bytes.is_none() {
        return Vec::new();
    }
    let warnings = match storage.usage(app_id) {
        Ok(usage) => quota_warnings(app_id, &spec, usage),
        Err(e) => vec![format!("{app_id}: cannot measure its storage: {e}")],
    };
    for warning in &warnings {
        makepad_widgets::log!("app storage: {warning}");
    }
    warnings
}

/// [`check_quota`] off the UI thread (a module's launch).
pub fn check_quota_later(storage: &'static Arc<Storage>, app_id: &str) {
    if storage.spec(app_id).max_bytes.is_none() && storage.spec(app_id).cache_max_bytes.is_none() {
        return;
    }
    let app_id = app_id.to_owned();
    std::thread::spawn(move || {
        check_quota(storage, &app_id);
    });
}

/// What happened to an account, as [`account_changed`] decided it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// The account's folder is open and its agent may resume.
    SignedIn { app: String, account: Option<String>, folder: PathBuf },
    /// The account's agent is suspended; its data stays.
    SignedOut { app: String, account: Option<String> },
    /// Nothing for storage to do.
    None,
}

/// An app's assistant service bound another account (the broker, through
/// `app_peers::storage::account_changed`). An app without accounts
/// (`storage.accounts: false`) acts for the device, which never signs out.
pub fn account_changed(storage: &Arc<Storage>, service_app: &str, previous: Option<&str>, current: Option<&str>) -> Change {
    let app = app_of(service_app);
    if !storage.spec(app).accounts {
        return Change::None;
    }
    match current {
        Some(account) => {
            storage.sign_in(app, Some(account));
            match storage.open(app).and_then(|s| s.account_folder(Some(account))) {
                Ok(folder) => Change::SignedIn { app: app.to_owned(), account: Some(account.to_owned()), folder },
                Err(e) => {
                    makepad_widgets::log!("app storage: {app}: cannot open the account's folder: {e}");
                    Change::None
                }
            }
        }
        None => match previous {
            Some(account) => {
                storage.sign_out(app, Some(account));
                Change::SignedOut { app: app.to_owned(), account: Some(account.to_owned()) }
            }
            None => Change::None,
        },
    }
}

/// Mail's host service gave an app an account, or took it away.
#[cfg(any(feature = "app-hub", native_mobile))]
pub fn mail_account(storage: &Arc<Storage>, event: &octosense_mail_service::AccountEvent) -> Change {
    use octosense_mail_service::AccountEvent;
    match event {
        AccountEvent::Added { app_id, account } => account_changed(storage, app_id, None, Some(account)),
        AccountEvent::Removed { app_id, account } => {
            if !storage.spec(app_id).accounts {
                return Change::None;
            }
            if let Err(e) = storage.remove_account(app_id, Some(account)) {
                makepad_widgets::log!("app storage: {app_id}: cannot remove the account's folder: {e}");
            }
            erase_agents(storage, app_of(app_id), Some(account));
            Change::SignedOut { app: app_id.clone(), account: Some(account.clone()) }
        }
    }
}

/// App Hub uninstalled `manifest_id` (its jail is gone or going): delete
/// what the host keeps for it and keep its agents suspended. Only when the
/// jail itself is gone: an update replaces `bundle/` alone, and a system
/// app (`os.*`) ships with the build and is never uninstalled; nor is a
/// native app, whose folders an event naming its id must never delete.
pub fn app_uninstalled(storage: &Arc<Storage>, root: &Path, manifest_id: &str) -> bool {
    if manifest_id.starts_with("os.")
        || super::validate_app_id(manifest_id).is_err()
        || crate::apps::check_script_app_id(manifest_id).is_err()
        || root.join(manifest_id).exists()
    {
        return false;
    }
    if let Err(e) = storage.uninstall(manifest_id) {
        makepad_widgets::log!("app storage: {manifest_id}: uninstall left something behind: {e}");
    }
    erase_agents(storage, manifest_id, None);
    true
}

/// What the host asks octos to erase: the peers of a storage app id under
/// each id its agent may have (a native app's own id, a script app's
/// `card.<manifest id>`), for one account or (`None`) every account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PurgeRequest {
    pub app: String,
    pub service_apps: Vec<String>,
    /// The labels a broker of the app may have named its peers with (the
    /// name of a peer recorded before records carried it): the shell's
    /// display label and the app id (a contained app's broker is labelled
    /// with its manifest id).
    pub labels: Vec<String>,
    pub account: Option<String>,
}

/// Erases agents (`peer/purge`) and says, once done, whether every one is
/// gone. The shell installs [`kernel_purger`]; tests install their own.
pub type Purger = Arc<dyn Fn(PurgeRequest, Box<dyn FnOnce(bool) + Send>) + Send + Sync>;

fn purger() -> &'static Mutex<Option<Purger>> {
    static PURGER: Mutex<Option<Purger>> = Mutex::new(None);
    &PURGER
}

/// Install (or with `None` remove) the process's purger.
pub fn set_purger(purger_fn: Option<Purger>) {
    *purger().lock().unwrap_or_else(|e| e.into_inner()) = purger_fn;
}

/// After the storage lifecycle deleted an account's folder or an app's
/// jail: erase the agent, and once it is gone forget its suspension.
fn erase_agents(storage: &Arc<Storage>, app: &str, account: Option<&str>) {
    let Some(purge) = purger().lock().unwrap_or_else(|e| e.into_inner()).clone() else { return };
    let mut service_apps = vec![app.to_owned()];
    let contained = format!("{}{app}", crate::ai_host::contained::PEER_PREFIX);
    if !service_apps.contains(&contained) {
        service_apps.push(contained);
    }
    let mut labels = vec![crate::approvals::sheet::app_label(app)];
    if !labels.iter().any(|l| l == app) {
        labels.push(app.to_owned());
    }
    let request = PurgeRequest { app: app.to_owned(), service_apps, labels, account: account.map(str::to_owned) };
    let storage = storage.clone();
    let (app, account) = (request.app.clone(), request.account.clone());
    // `done` may run on the purge's background thread: `Storage` locks its
    // own state, so marking it there is safe.
    purge(request, Box::new(move |erased| {
        if !erased {
            makepad_widgets::log!("app storage: {app}: its agent could not be erased; it stays suspended and its record is kept");
            return;
        }
        match account {
            Some(account) => storage.mark_erased(&app, Some(&account)),
            None => storage.mark_app_erased(&app),
        }
    }));
}

/// The shell's purger: octos `peer/purge` on the shell's kernel for every
/// peer the host recorded (`crate::ai_host::app_peers::purge`), off the UI
/// thread.
#[cfg(kernel)]
pub fn kernel_purger() -> Purger {
    use crate::ai_host::app_peers::{connectors::CoreConnector, hosted, purge};
    Arc::new(|request: PurgeRequest, done: Box<dyn FnOnce(bool) + Send>| {
        let Some(core_dir) = crate::ai_host::kernel::core_dir() else { return done(false) };
        let host = purge::PurgeHost::new(hosted::SHARED_PROFILE, hosted::system_session(), hosted::host_state_dir(&core_dir));
        purge::purge_in_background(Arc::new(CoreConnector::shell()), host, request.service_apps, request.labels, request.account, move |purged| {
            if !purged.erased.is_empty() {
                makepad_widgets::log!("app storage: erased the agents of {:?}", purged.erased);
            }
            done(purged.ok());
        });
    })
}

/// Settings' line for an app whose agent has suspended accounts: a
/// signed-out account's agent keeps its memory, and a removed one's until
/// it is erased.
pub fn memory_notice(storage: &Storage, app_id: &str) -> Option<String> {
    let n = storage.suspended_accounts(app_id);
    (n > 0).then(|| {
        let who = if n == 1 { "1 account is".to_owned() } else { format!("{n} accounts are") };
        format!("{who} signed out or removed; its agent's memory remains until the account is removed and its agent erased")
    })
}

/// Connect the account sources to the host's storage (startup, once the
/// host storage is set up): the brokers' account changes and Mail's.
pub fn install(storage: &'static Arc<Storage>) {
    register_native_specs(storage);
    #[cfg(kernel)]
    set_purger(Some(kernel_purger()));
    crate::ai_host::app_peers::storage::observe_accounts(Some(Arc::new(move |app: &str, previous: Option<&str>, current: Option<&str>| {
        account_changed(storage, app, previous, current);
    })));
    #[cfg(any(feature = "app-hub", native_mobile))]
    {
        octosense_mail_service::on_account_event(Some(Arc::new(move |event| {
            mail_account(storage, &event);
        })));
        mail_secrets_at_startup(storage);
    }
}

/// Mail's passwords in the host's secrets (ADR 0004 §11), not under
/// `apps/.host/mail/`: name the folder for the service, and move every
/// password an older build left there now (an old copy a failed delete
/// left behind goes too, so a failure is retried at the next start).
#[cfg(any(feature = "app-hub", native_mobile))]
pub fn mail_secrets_at_startup(storage: &Storage) {
    let dir = mail_secrets_dir(storage.layout());
    if let Err(e) = super::ensure_private_dir(storage.layout().secrets_root(), &dir) {
        makepad_widgets::log!("app storage: Mail's secrets stay in its service folder: {}: {e}", dir.display());
        return;
    }
    let mail_dir = storage.layout().apps_root().join(".host").join("mail");
    let moved = octosense_mail_service::vault::migrate_all(&octosense_mail_service::vault::Place::resolve(&mail_dir, Some(&dir)));
    if moved > 0 {
        makepad_widgets::log!("app storage: moved {moved} of Mail's passwords into {}", dir.display());
    }
    octosense_mail_service::set_secrets_dir(Some(dir));
}

/// Where Mail's host service keeps passwords: `secrets/os.mail/`.
pub fn mail_secrets_dir(layout: &super::Layout) -> std::path::PathBuf {
    layout.secrets_root().join("os.mail")
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
