//! Shared `auth` service. All protocol work runs off the Makepad UI thread.
use crate::{
    authorize::{exchange_google, identity, DevicePoll, GithubDeviceAttempt},
    oauth::{GoogleAttempt, Tokens, AUTH_LIFETIME},
    providers::{ClientRegistration, Provider},
    Connections, CredentialStore,
};
use octosense_appstore::services::{self, HostService, Replier, ServiceCall, ServiceHost};
use serde_json::{json, Value};
use std::{
    collections::{BTreeSet, HashMap},
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

#[path = "host_backend.rs"]
pub(crate) mod backend_host;
#[cfg(feature = "acceptance-fixtures")]
pub use backend_host::register_fixture as register_backend_fixture;
pub use backend_host::{
    invalidate_backend_registration, revalidate_backend_registration, set_backend_resolver,
    BackendResolver,
};

pub type ScopeCheck = Arc<dyn Fn(&str, Provider, &BTreeSet<String>) -> bool + Send + Sync>;
pub type AccountChanged = Arc<dyn Fn(&str, Option<&str>, Option<&str>) + Send + Sync>;
static ACCOUNT_CHANGED: Mutex<Option<AccountChanged>> = Mutex::new(None);
#[derive(Default)]
struct AuthorizationEpochs(HashMap<(PathBuf, String), u64>);
impl AuthorizationEpochs {
    fn get(&self, root: &Path, app: &str) -> u64 {
        *self.0.get(&(root.into(), app.into())).unwrap_or(&0)
    }
    fn invalidate(&mut self, root: &Path, app: &str) {
        let value = self.0.entry((root.into(), app.into())).or_default();
        *value = value.saturating_add(1);
    }
}
static AUTHORIZATION_EPOCHS: std::sync::OnceLock<Mutex<AuthorizationEpochs>> =
    std::sync::OnceLock::new();
fn authorization_epochs() -> &'static Mutex<AuthorizationEpochs> {
    AUTHORIZATION_EPOCHS.get_or_init(|| Mutex::new(AuthorizationEpochs::default()))
}
fn authorization_epoch(root: &Path, app: &str) -> u64 {
    authorization_epochs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(root, app)
}
/// Called under STORE_LOCK so uninstall and final credential admission cannot
/// cross. A later reinstall does not revive a browser request from the old app.
fn invalidate_authorizations(root: &Path, app: &str) {
    authorization_epochs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .invalidate(root, app);
}
pub fn on_account_changed(hook: AccountChanged) {
    *ACCOUNT_CHANGED.lock().unwrap_or_else(|e| e.into_inner()) = Some(hook);
}
fn account_changed(app: &str, previous: Option<&str>, current: Option<&str>) {
    let hook = ACCOUNT_CHANGED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if previous != current {
        if let Some(hook) = hook {
            hook(app, previous, current);
        }
    }
}
pub fn active_connection(root: &Path, app: &str) -> Option<crate::Connection> {
    connections(root).ok()?.active(app)
}

/// Uninstall revokes every connection before a reinstalled bundle can discover
/// it. A vault deletion failure cannot restore an already revoked handle.
pub fn forget_app(root: &Path, app: &str) -> Result<(), String> {
    let operation = operation_lock(root, app);
    let operation_guard = operation.lock().unwrap_or_else(|e| e.into_inner());
    let guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    invalidate_authorizations(root, app);
    let mut store = connections(root)?;
    let previous = store.active(app);
    let mut failure = None;
    for connection in store.list(app) {
        if let Err(error) = store.disconnect(app, &connection.handle) {
            failure.get_or_insert(error);
        }
    }
    // disconnect may report a vault failure after durable revocation. Check
    // the actual remaining connection metadata before erasing private data.
    if store.list(app).is_empty() {
        for result in [
            crate::inbox::purge_app(root, app),
            crate::inbox_events::purge_app(root, app),
            crate::calendar_cache::purge_app(root, app),
        ] {
            if let Err(error) = result {
                failure.get_or_insert(error);
            }
        }
    }
    let current = store.active(app);
    drop(guard);
    drop(operation_guard);
    account_changed(
        app,
        previous.as_ref().map(|c| c.handle.as_str()),
        current.as_ref().map(|c| c.handle.as_str()),
    );
    match failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Mail's existing platform vault is reused without sharing Mail credentials.
struct Vault {
    inner: Arc<dyn octosense_mail_service::vault::Vault>,
    place: octosense_mail_service::vault::Place,
}
impl CredentialStore for Vault {
    fn put(&self, key: &str, value: &str) -> Result<(), String> {
        self.inner.put(&self.place, key, value)
    }
    fn get(&self, key: &str) -> Result<String, String> {
        self.inner.get(&self.place, key)
    }
    fn remove(&self, key: &str) -> Result<(), String> {
        self.inner.remove(&self.place, key);
        Ok(())
    }
}
pub(crate) fn connections(root: &Path) -> Result<Connections, String> {
    #[cfg(feature = "acceptance-fixtures")]
    if let Some(fixture) = crate::acceptance_fixtures::for_root(root) {
        return Connections::open(&root.join("oauth"), fixture.vault);
    }
    let root = root.join("oauth");
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
    if std::env::var("OCTOSENSE_MAIL_VAULT").is_ok_and(|v| v == "file") {
        return Err("OAuth requires the platform credential store; disable the Mail development file-vault override".into());
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "android",
        target_os = "windows",
        target_os = "linux"
    )))]
    return Err("OAuth has no secure credential adapter on this platform".into());
    // Unlike the legacy Mail fallback, Windows/Linux OAuth credentials use
    // their OS credential service. Failure is explicit, never plaintext.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        return Connections::open(&root, Arc::new(DesktopVault::new(&root)?));
    }
    #[allow(unreachable_code)]
    let vault = Vault {
        inner: octosense_mail_service::vault::platform(),
        place: octosense_mail_service::vault::Place::legacy(&root.join("credentials")),
    };
    Connections::open(&root, Arc::new(vault))
}

pub(crate) fn provider_transport(
    root: &Path,
) -> Result<Arc<dyn crate::transport::Transport>, String> {
    #[cfg(feature = "acceptance-fixtures")]
    if let Some(fixture) = crate::acceptance_fixtures::for_root(root) {
        return Ok(fixture.transport);
    }
    let _ = root;
    Ok(Arc::new(crate::transport::HttpsTransport::new()?))
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
struct DesktopVault {
    service: String,
}
#[cfg(any(target_os = "windows", target_os = "linux"))]
impl DesktopVault {
    fn new(root: &Path) -> Result<Self, String> {
        use sha2::{Digest, Sha256};
        std::fs::create_dir_all(root).map_err(|_| "Cannot create OAuth profile directory")?;
        let path = root
            .canonicalize()
            .map_err(|_| "Cannot resolve OAuth profile directory")?;
        let digest = Sha256::digest(path.to_string_lossy().as_bytes());
        Ok(Self {
            service: format!("OctoSense OAuth {:x}", digest),
        })
    }
    fn entry(&self, key: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(&self.service, key)
            .map_err(|_| "The OS credential service is unavailable".to_owned())
    }
}
#[cfg(any(target_os = "windows", target_os = "linux"))]
impl CredentialStore for DesktopVault {
    fn put(&self, key: &str, value: &str) -> Result<(), String> {
        self.entry(key)?
            .set_password(value)
            .map_err(|_| "Unlock the OS credential store to connect this account".into())
    }
    fn get(&self, key: &str) -> Result<String, String> {
        self.entry(key)?
            .get_password()
            .map_err(|_| "Unlock the OS credential store or connect the account again".into())
    }
    fn remove(&self, key: &str) -> Result<(), String> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => {
                Err("Connection revoked, but its old OS credential could not be removed".into())
            }
        }
    }
}
pub(crate) static STORE_LOCK: Mutex<()> = Mutex::new(());

/// Serialize one app's provider work and local cache/draft edits, without
/// holding the process-wide metadata lock while a provider is slow. Account
/// mutations take this lock before STORE_LOCK, in that order everywhere.
pub(crate) fn operation_lock(root: &Path, app: &str) -> Arc<Mutex<()>> {
    type Locks = HashMap<(PathBuf, String), std::sync::Weak<Mutex<()>>>;
    static LOCKS: std::sync::OnceLock<Mutex<Locks>> = std::sync::OnceLock::new();
    let mut locks = LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    locks.retain(|_, lock| lock.strong_count() > 0);
    let key = (
        root.canonicalize().unwrap_or_else(|_| root.into()),
        app.to_owned(),
    );
    if let Some(lock) = locks.get(&key).and_then(std::sync::Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(key, Arc::downgrade(&lock));
    lock
}

pub(crate) fn with_provider_api<T>(
    root: &Path,
    app: &str,
    run: impl FnOnce(&mut crate::api::Api<'_>) -> Result<T, String>,
) -> Result<T, String> {
    let operation = operation_lock(root, app);
    let _operation = operation.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = {
        let _metadata = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        connections(root)?
    };
    let config = clients(root)?;
    let client = config.google.as_ref().map(|c| ClientRegistration {
        client_id: c.client_id.clone(),
    });
    let transport = provider_transport(root)?;
    run(&mut crate::api::Api {
        connections: &mut store,
        transport: transport.as_ref(),
        google_client: client.as_ref(),
        google_client_secret: config
            .google
            .as_ref()
            .and_then(|c| c.client_secret.as_deref()),
        now: unix_now(),
    })
}

// Authorization and both connector refresh paths resolve the same identity.
pub(crate) use crate::registration::clients;
pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

struct Pending {
    /// The sheet's ticket: the key of the code it may show (`sign_in_code`).
    ticket: String,
    app: String,
    root: PathBuf,
    provider: Provider,
    scopes: BTreeSet<String>,
    original: Replier,
    cancelled: Arc<AtomicBool>,
    started: AtomicBool,
    deadline: Instant,
    status: Mutex<Value>,
    epoch: u64,
    scope_check: ScopeCheck,
    backend: Option<Arc<crate::backend::BackendClient>>,
    embedded: bool,
    callback_claimed: AtomicBool,
    callback_tx: std::sync::mpsc::SyncSender<String>,
    callback_rx: Mutex<std::sync::mpsc::Receiver<String>>,
}
struct AuthService {
    backend_reviews: crate::host_api::BackendReviews,
    scope_check: ScopeCheck,
    pending: HashMap<String, Arc<Pending>>,
}

pub fn register(scope_check: ScopeCheck) {
    services::register_host_service(Box::new(AuthService {
        backend_reviews: Default::default(),
        scope_check,
        pending: HashMap::new(),
    }));
}

impl AuthService {
    fn pending(&self, call: &ServiceCall) -> Result<Arc<Pending>, String> {
        let ticket = call.args["ticket"]
            .as_str()
            .ok_or("Missing authorization request")?;
        self.pending
            .get(ticket)
            .filter(|p| {
                p.app == call.app_id
                    && p.root == call.host_dir
                    && !p.cancelled.load(Ordering::SeqCst)
                    && Instant::now() < p.deadline
            })
            .cloned()
            .ok_or("Authorization expired".into())
    }
    fn cleanup(&mut self) {
        self.pending.retain(|_, p| {
            if Instant::now() >= p.deadline || p.cancelled.load(Ordering::SeqCst) {
                p.cancelled.store(true, Ordering::SeqCst);
                p.original
                    .clone()
                    .send(Err("Authorization expired or was cancelled".into()));
                false
            } else {
                true
            }
        });
    }
}
impl HostService for AuthService {
    fn api_methods(&self) -> Vec<services::HostApiMethod> {
        crate::host_catalog::auth()
    }
    fn family(&self) -> &'static str {
        "auth"
    }
    fn call(&mut self, call: ServiceCall, reply: Replier, host: &mut dyn ServiceHost) {
        self.cleanup();
        if call.method().starts_with("sheet.") && !call.from_sheet {
            reply.send(Err("Authentication controls belong to the host".into()));
            return;
        }
        if call.method() == "backend.request" || call.method().starts_with("backend.sheet.") {
            self.backend_reviews
                .call(call, reply, host, self.scope_check.clone());
            return;
        }
        match call.method() {
            "active" => reply.send(Ok(json!(active_connection(&call.host_dir, &call.app_id)))),
            "select" => {
                std::thread::spawn(move || {
                    let result = (|| {
                        let operation = operation_lock(&call.host_dir, &call.app_id);
                        let operation_guard = operation.lock().unwrap_or_else(|e| e.into_inner());
                        let guard = STORE_LOCK.lock().unwrap();
                        let mut store = connections(&call.host_dir)?;
                        let previous = store.active(&call.app_id);
                        let handle = call.args["connection"]
                            .as_str()
                            .ok_or("Choose a connected account")?;
                        let selected = store.select(&call.app_id, handle)?;
                        invalidate_authorizations(&call.host_dir, &call.app_id);
                        drop(guard);
                        drop(operation_guard);
                        account_changed(
                            &call.app_id,
                            previous.as_ref().map(|c| c.handle.as_str()),
                            Some(&selected.handle),
                        );
                        Ok(json!(selected))
                    })();
                    reply.send(result);
                });
            }
            "accounts" => {
                std::thread::spawn(move || {
                    let _guard = STORE_LOCK.lock().unwrap();
                    reply.send(
                        connections(&call.host_dir).map(|store| json!(store.list(&call.app_id))),
                    );
                });
            }
            "disconnect" => {
                std::thread::spawn(move || {
                    let operation = operation_lock(&call.host_dir, &call.app_id);
                    let operation_guard = operation.lock().unwrap_or_else(|e| e.into_inner());
                    let guard = STORE_LOCK.lock().unwrap();
                    let result = (|| {
                        let handle = call.args["connection"]
                            .as_str()
                            .ok_or("Choose a connected account")?;
                        let store = connections(&call.host_dir)?;
                        let previous = store.active(&call.app_id);
                        let remote = backend_host::logout_material(
                            &call.host_dir,
                            &call.app_id,
                            handle,
                            &store,
                        );
                        // Resolving logout may have revoked every backend handle
                        // after a declaration change. Never persist an older
                        // metadata snapshot over that durable revocation.
                        let mut store = connections(&call.host_dir)?;
                        let already_revoked = remote.is_some()
                            && !store
                                .list(&call.app_id)
                                .iter()
                                .any(|entry| entry.handle == handle);
                        let revoked = if already_revoked {
                            Ok(())
                        } else {
                            store.disconnect(&call.app_id, handle)
                        };
                        invalidate_authorizations(&call.host_dir, &call.app_id);
                        let current = store.active(&call.app_id);
                        drop(guard);
                        drop(operation_guard);
                        account_changed(
                            &call.app_id,
                            previous.as_ref().map(|c| c.handle.as_str()),
                            current.as_ref().map(|c| c.handle.as_str()),
                        );
                        revoked?;
                        let mut result = json!({"disconnected":true});
                        if let Some(remote) = remote {
                            result["remote_logout"] = backend_host::logout(&call.app_id, remote);
                        }
                        Ok(result)
                    })();
                    reply.send(result);
                });
            }
            "backend.me" => {
                let scope_check = self.scope_check.clone();
                std::thread::spawn(move || {
                    reply.send(backend_host::me(&call, &scope_check));
                });
            }
            "connect" => {
                if !call.may_prompt {
                    reply.send(Err("Open the app to connect an account".into()));
                    return;
                }
                let parsed = (|| {
                    let _guard = STORE_LOCK.try_lock().map_err(|_| {
                        "Another account operation is busy; retry sign-in when it finishes"
                    })?;
                    let provider: Provider = serde_json::from_value(call.args["provider"].clone())
                        .map_err(|_| "Choose GitHub, Google or this app's backend")?;
                    let requested: Vec<String> =
                        serde_json::from_value(call.args["scopes"].clone())
                            .map_err(|_| "Specify OAuth scopes")?;
                    let scopes = provider.validate_scopes(&requested)?;
                    if !(self.scope_check)(&call.app_id, provider, &scopes) {
                        return Err("Requested scopes exceed this app's granted services".into());
                    }
                    #[cfg(target_os = "android")]
                    if provider == Provider::Google {
                        return Err("Google authorization needs the Android host adapter; desktop login is not supported on this device".into());
                    }
                    let embedded = presentation(provider, call.args.get("presentation"))?;
                    #[cfg(any(target_os = "android", target_os = "ios"))]
                    if provider == Provider::Backend && !embedded {
                        return Err(
                            "Backend browser authorization needs a native mobile callback adapter"
                                .into(),
                        );
                    }
                    let backend = if provider == Provider::Backend {
                        Some(backend_host::client(&call.host_dir, &call.app_id)?)
                    } else {
                        None
                    };
                    Ok((
                        provider,
                        scopes,
                        authorization_epoch(&call.host_dir, &call.app_id),
                        backend,
                        embedded,
                    ))
                })();
                let (provider, scopes, epoch, backend, embedded) = match parsed {
                    Ok(v) => v,
                    Err(e) => {
                        reply.send(Err(e));
                        return;
                    }
                };
                for pending in self
                    .pending
                    .values()
                    .filter(|p| p.app == call.app_id && p.root == call.host_dir)
                {
                    pending.cancelled.store(true, Ordering::SeqCst);
                    pending
                        .original
                        .clone()
                        .send(Err("A new sign-in replaced this request".into()));
                }
                self.cleanup();
                // Bound worker lifetimes independently of consumed UI tickets.
                if self.pending.len() >= 32 {
                    reply.send(Err(
                        "Too many sign-in requests are open; close one and retry".into(),
                    ));
                    return;
                }
                let ticket = Uuid::new_v4().to_string();
                let (callback_tx, callback_rx) = std::sync::mpsc::sync_channel(1);
                let pending = Arc::new(Pending {
                    ticket: ticket.clone(),
                    app: call.app_id,
                    root: call.host_dir,
                    provider,
                    scopes,
                    original: reply,
                    cancelled: Arc::new(AtomicBool::new(false)),
                    started: AtomicBool::new(false),
                    deadline: Instant::now() + AUTH_LIFETIME,
                    status: Mutex::new(json!({"phase":"ready"})),
                    epoch,
                    scope_check: self.scope_check.clone(),
                    backend,
                    embedded,
                    callback_claimed: AtomicBool::new(false),
                    callback_tx,
                    callback_rx: Mutex::new(callback_rx),
                });
                if !makepad_widgets::web_reader::register_auth_lifetime(&ticket, &pending.cancelled)
                {
                    pending
                        .original
                        .clone()
                        .send(Err("Cannot open another sign-in right now".into()));
                    return;
                }
                let worker = pending.clone();
                if std::thread::Builder::new()
                    .name("oauth-sign-in".into())
                    .spawn(move || complete_pending(worker))
                    .is_err()
                {
                    pending.cancelled.store(true, Ordering::SeqCst);
                    pending
                        .original
                        .clone()
                        .send(Err("Cannot start sign-in right now".into()));
                    return;
                }
                host.open_sheet(consent_sheet(&ticket, &pending));
                self.pending.insert(ticket, pending);
            }
            "sheet.start" => match self.pending(&call) {
                Err(e) => reply.send(Err(e)),
                Ok(pending) => {
                    if !pending.started.load(Ordering::SeqCst) {
                        *pending.status.lock().unwrap() = json!({"phase":"starting"});
                        pending.started.store(true, Ordering::SeqCst);
                    }
                    reply.send(Ok(json!({"started":true})));
                }
            },
            "sheet.callback" => {
                let result = (|| {
                    let p = self.pending(&call)?;
                    if !p.embedded || !p.started.load(Ordering::SeqCst) {
                        return Err("No embedded sign-in is waiting".into());
                    }
                    let url = call.args["url"]
                        .as_str()
                        .filter(|s| s.len() <= 8192)
                        .ok_or("Invalid authentication callback")?;
                    if p.callback_claimed.swap(true, Ordering::SeqCst) {
                        return Err("Authentication callback already received".into());
                    }
                    p.callback_tx
                        .try_send(url.to_owned())
                        .map_err(|_| "Authentication callback is unavailable")?;
                    Ok(json!({"received":true}))
                })();
                reply.send(result);
            }
            "sheet.status" => match self.pending(&call) {
                Ok(p) => {
                    let mut status = p.status.lock().unwrap().clone();
                    if status["phase"] == "browser" {
                        // The sheet's countdown: the request's deadline, not
                        // the provider's longer code lifetime.
                        let left = p
                            .deadline
                            .saturating_duration_since(Instant::now())
                            .as_secs();
                        status["remaining"] = json!(format!("{}:{:02}", left / 60, left % 60));
                    }
                    // Only the current ticket's own visible sheet may close
                    // itself. A late worker must not dismiss a newer review.
                    if status["phase"] == "connected" {
                        host.close_sheet();
                        p.cancelled.store(true, Ordering::SeqCst);
                    }
                    reply.send(Ok(status));
                }
                Err(e) => reply.send(Err(e)),
            },
            "sheet.cancel" => {
                if let Ok(p) = self.pending(&call) {
                    crate::sign_in_code::retire(&p.ticket);
                    p.cancelled.store(true, Ordering::SeqCst);
                    p.original.clone().send(Err("Sign-in cancelled".into()));
                }
                host.close_sheet();
                reply.send(Ok(json!({"cancelled":true})));
                self.cleanup();
            }
            _ => reply.send(Err("Unknown authentication operation".into())),
        }
    }
}

/// A worker exists before Continue so an abandoned sheet settles the original
/// request as well. The UI only flips atomics; it never waits for this worker.
fn wait_for_consent(
    started: &AtomicBool,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(), String> {
    loop {
        if cancelled.load(Ordering::SeqCst) || Instant::now() >= deadline {
            return Err("Sign-in cancelled or expired".into());
        }
        if started.load(Ordering::SeqCst) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn complete_pending(pending: Arc<Pending>) {
    let result = wait_for_consent(&pending.started, &pending.cancelled, pending.deadline)
        .and_then(|()| authorize(&pending));
    // authorize checks cancellation around the store commit and rolls back a
    // cancelled write. Once it returns Ok the connection is committed; a later
    // sheet close must not relabel that durable success as a failed sign-in.
    crate::sign_in_code::retire(&pending.ticket);
    match result {
        Ok(connection) => {
            *pending.status.lock().unwrap() = json!({"phase":"connected"});
            pending.original.clone().send(Ok(json!(connection)));
        }
        Err(error) => {
            *pending.status.lock().unwrap() =
                json!({"phase":"error","message":sheet_failure(pending.provider, &error)});
            // Replier discards duplicates/stale isolates. A live replaced app
            // must still receive cancellation even if no native event arrives.
            pending.original.clone().send(Err(error));
        }
    }
}

fn authorize(p: &Pending) -> Result<crate::Connection, String> {
    if p.provider == Provider::Backend {
        return backend_host::authorize(p);
    }
    let settings = clients(&p.root)?;
    let client = match p.provider {
        Provider::Github => settings.github,
        Provider::Google => settings.google,
        Provider::Backend => return Err("Backend registration is unavailable".into()),
    }
    .ok_or(p.provider.sign_in_unavailable())?;
    let registration = ClientRegistration {
        client_id: client.client_id,
    };
    // The provider's transport: HTTPS, or a marked acceptance profile's
    // synthetic provider (acceptance-fixtures builds only).
    let transport = provider_transport(&p.root)?;
    let transport = transport.as_ref();
    let scopes: Vec<String> = p.scopes.iter().cloned().collect();
    let tokens: Tokens = match p.provider {
        Provider::Backend => return Err("Backend registration is unavailable".into()),
        Provider::Github => {
            let mut attempt = GithubDeviceAttempt::begin(
                &p.app,
                &registration,
                &scopes,
                transport,
                Instant::now(),
            )?;
            *p.status.lock().unwrap() =
                json!({"phase":"browser","url":attempt.verification_uri,"code":attempt.user_code});
            crate::sign_in_code::publish(&p.ticket, &attempt.user_code, &attempt.verification_uri);
            loop {
                if p.cancelled.load(Ordering::SeqCst) || Instant::now() >= p.deadline {
                    attempt.cancel();
                    return Err("Sign-in cancelled or expired".into());
                }
                match attempt.poll(&p.app, transport, Instant::now(), unix_now())? {
                    DevicePoll::Wait(wait) => {
                        std::thread::sleep(wait.min(Duration::from_millis(250)))
                    }
                    DevicePoll::Authorized(tokens) => break tokens,
                    DevicePoll::Denied => return Err("GitHub authorization was declined".into()),
                    DevicePoll::Expired => {
                        return Err("GitHub authorization expired; start again".into())
                    }
                }
            }
        }
        Provider::Google => {
            let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                .map_err(|_| "Cannot start OAuth callback listener")?;
            listener
                .set_nonblocking(true)
                .map_err(|_| "Cannot configure callback listener")?;
            let port = listener
                .local_addr()
                .map_err(|_| "Cannot read callback address")?
                .port();
            let mut attempt =
                GoogleAttempt::desktop(&p.app, &registration, &scopes, port, Instant::now())?;
            *p.status.lock().unwrap() =
                json!({"phase":"browser","url":attempt.authorization_url().as_str(),"code":""});
            crate::sign_in_code::publish(&p.ticket, "", attempt.authorization_url().as_str());
            let code = loop {
                if p.cancelled.load(Ordering::SeqCst) || Instant::now() >= p.deadline {
                    attempt.cancel();
                    return Err("Sign-in cancelled or expired".into());
                }
                match listener.accept() {
                    Ok((mut stream, peer)) => {
                        if !peer.ip().is_loopback() {
                            continue;
                        }
                        stream.set_read_timeout(Some(Duration::from_secs(1))).ok();
                        let mut bytes = [0; 8192];
                        let count = match stream.read(&mut bytes) {
                            Ok(n) => n,
                            Err(_) => continue,
                        };
                        let request = String::from_utf8_lossy(&bytes[..count]);
                        let mut words = request.lines().next().unwrap_or("").split_whitespace();
                        if words.next() != Some("GET") {
                            continue;
                        }
                        let path = words.next().unwrap_or("");
                        if !path.starts_with("/oauth/callback?") {
                            let _=stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                            continue;
                        }
                        match attempt.consume_callback(
                            &p.app,
                            &format!("http://127.0.0.1:{port}{path}"),
                            Instant::now(),
                        ) {
                            Ok(code) => {
                                let body = "Authorization received. Return to OctoSense.";
                                let _=write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
                                break code;
                            }
                            Err(error) => {
                                let _=stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                                if attempt.is_finished() {
                                    return Err(error);
                                }
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(100))
                    }
                    Err(_) => return Err("OAuth callback listener stopped".into()),
                }
            };
            exchange_google(
                &registration,
                client.client_secret.as_deref(),
                code,
                &p.scopes,
                transport,
                unix_now(),
            )?
        }
    };
    let (subject, label) = identity(p.provider, &tokens, transport)?;
    let operation = operation_lock(&p.root, &p.app);
    let operation_guard = operation.lock().unwrap_or_else(|e| e.into_inner());
    let _guard = STORE_LOCK.lock().unwrap();
    if p.cancelled.load(Ordering::SeqCst)
        || Instant::now() >= p.deadline
        || authorization_epoch(&p.root, &p.app) != p.epoch
        || !(p.scope_check)(&p.app, p.provider, &p.scopes)
    {
        return Err("Authorization cancelled".into());
    }
    let mut store = connections(&p.root)?;
    let previous = store.active(&p.app);
    let connection = store.connect(&p.app, p.provider, &subject, &label, tokens)?;
    if p.cancelled.load(Ordering::SeqCst) || Instant::now() >= p.deadline {
        store.disconnect(&p.app, &connection.handle)?;
        if let Some(previous) = previous {
            store.select(&p.app, &previous.handle)?;
        }
        return Err("Authorization cancelled".into());
    }
    drop(_guard);
    drop(operation_guard);
    account_changed(
        &p.app,
        previous.as_ref().map(|c| c.handle.as_str()),
        Some(&connection.handle),
    );
    Ok(connection)
}

fn presentation(provider: Provider, value: Option<&Value>) -> Result<bool, String> {
    let supported = cfg!(any(target_os = "macos", target_os = "android"));
    match value.and_then(Value::as_str) {
        None if value.is_some() => Err("Invalid sign-in presentation".into()),
        None => Ok(provider == Provider::Backend && supported),
        Some("browser") => Ok(false),
        Some("webview") if provider != Provider::Backend => {
            Err("Provider sign-in uses its supported browser flow".into())
        }
        Some("webview") if !supported => {
            Err("Embedded backend sign-in is unavailable on this platform".into())
        }
        Some("webview") => Ok(true),
        _ => Err("Choose browser or webview sign-in".into()),
    }
}

fn consent_sheet(ticket: &str, p: &Pending) -> String {
    if p.embedded {
        return embedded_sheet(ticket, p);
    }
    if p.provider != Provider::Backend {
        return provider_sheet(ticket, p.provider, &p.app, &p.scopes);
    }
    // An app backend's browser sign-in.
    // All variable text enters as a JSON string literal, never executable Splash.
    let ticket = json!(ticket).to_string();
    let title = json!(format!(
        "Connect {}",
        match p.provider {
            Provider::Github => "GitHub",
            Provider::Google => "Google",
            Provider::Backend => "app backend",
        }
    ))
    .to_string();
    let backend_origin = p
        .backend
        .as_ref()
        .map(|client| {
            let origin = url::Url::parse(&client.registration().authorization_url)
                .map(|url| url.origin().ascii_serialization())
                .unwrap_or_default();
            format!("\n\nBackend: {}\n{}", client.registration().id, origin)
        })
        .unwrap_or_default();
    let description = json!(format!(
        "{} requests:\n{}{}\n\nCredentials stay with OctoSense.",
        p.app,
        p.scopes
            .iter()
            .map(|scope| crate::providers::scope_words(scope))
            .collect::<Vec<_>>()
            .join("\n"),
        backend_origin
    ))
    .to_string();
    format!(
        r#"
let ticket = {ticket}
let watching = false
let browser_url = ""
fn host_dismiss() {{
    watching = false
    host.request("auth.sheet.cancel", {{ticket: ticket}}, fn(r) {{}})
}}
fn poll() {{
    if !watching {{ return }}
    host.request("auth.sheet.status", {{ticket: ticket}}, fn(r) {{
        if r.is_ok {{
            if r.data.phase == "browser" {{
                if browser_url != r.data.url {{
                    browser_url = r.data.url
                    ui.oauth_browser.render()
                }}
                ui.oauth_code.set_text(r.data.code)
                ui.oauth_status.set_text("Complete authorization in the browser, then return here.")
            }}
            if r.data.phase == "error" {{ ui.oauth_status.set_text(r.data.message) watching = false }}
        }} else {{ watching = false }}
        if watching {{ start_timeout(0.5, || poll()) }}
    }})
}}
fn bind_lifetime() {{ return ui.oauth_lifetime.bind_auth_lifetime(ticket) }}
fn begin() {{
    if !bind_lifetime() {{ host_dismiss() return }}
    ui.oauth_status.set_text("Preparing secure sign-in…")
    host.request("auth.sheet.start", {{ticket: ticket}}, fn(r) {{
        if r.is_ok {{ watching = true poll() }} else {{ ui.oauth_status.set_text(r.error) }}
    }})
}}
let content = SolidView {{width: Fill height: Fill flow: Down padding: 20 spacing: 16 draw_bg.color: #fff
    Label {{width: Fill text: {title} draw_text.color: #222 draw_text.text_style.font_size: 22}}
    ScrollYView {{width: Fill height: Fill
        Label {{width: Fill text: {description} draw_text.color: #444 draw_text.text_style.font_size: 13}}
    }}
    oauth_lifetime := WebReader {{width: 0 height: 0 visible: false}}
    oauth_code := Label {{width: Fill draw_text.color: #222 draw_text.text_style.font_size: 22}}
    oauth_browser := View {{width: Fill height: Fit on_render: || {{
        if browser_url != "" {{
            LinkLabel {{width: Fill text: "Open secure sign-in in your browser" url: browser_url}}
        }}
    }}}}
    oauth_status := Label {{width: Fill text: "Continue to authorize this connection." draw_text.color: #444}}
    Button {{width: Fill height: 48 text: "Continue" on_click: || begin()}}
    ButtonFlat {{width: Fill height: 44 text: "Cancel" on_click: || host_dismiss()}}
}}
start_timeout(0.0, || bind_lifetime())
content
"#
    )
}

/// The installed app's display name, from its admitted manifest: the shell
/// registers the lookup. A sign-in sheet shows it above the app's id.
pub type AppNames = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;
static APP_NAMES: Mutex<Option<AppNames>> = Mutex::new(None);

/// Register the lookup [`AppNames`] describes.
pub fn set_app_names(names: AppNames) {
    *APP_NAMES.lock().unwrap_or_else(|e| e.into_inner()) = Some(names);
}

fn app_name(app: &str) -> Option<String> {
    let names = APP_NAMES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()?;
    names(app).filter(|name| !name.trim().is_empty() && name.len() <= 80)
}

/// The sheet's words for a provider sign-in that ended without a connection.
/// The app still receives the original error.
fn sheet_failure(provider: Provider, error: &str) -> String {
    let name = match provider {
        Provider::Github => "GitHub",
        Provider::Google => "Google",
        Provider::Backend => return error.to_string(),
    };
    if error.contains("declined") {
        format!("You declined on {name}, so nothing was connected. Connect again from the app if you change your mind.")
    } else if error.contains("authorization expired") {
        "The code expired before it was approved. Close this and connect again for a new one."
            .into()
    } else if error == "Sign-in cancelled or expired" {
        "This sign-in timed out. Close this and connect again to start over.".into()
    } else if error.contains("connection failed") || error.contains("check the network") {
        format!("Couldn't reach {name}. Check your connection, then connect again.")
    } else {
        error.to_string()
    }
}

/// GitHub's (device code) and Google's (browser) sign-in sheet: who asks and
/// for what, one action to go on, then the code with Copy and Open, a visible
/// wait, and failures in plain words.
fn provider_sheet(
    ticket: &str,
    provider: Provider,
    app_id: &str,
    scopes: &BTreeSet<String>,
) -> String {
    let (name, site, has_code) = match provider {
        Provider::Github => ("GitHub", "github.com", true),
        _ => ("Google", "accounts.google.com", false),
    };
    let app = app_name(app_id).unwrap_or_else(|| app_id.to_string());
    let lit = |text: String| json!(text).to_string();
    let ticket = lit(ticket.to_string());
    let title = lit(format!("Connect {name}"));
    let subtitle = lit(format!("{app} wants to use your {name} account."));
    let app_id = lit(app_id.to_string());
    let rows: String = scopes
        .iter()
        .map(|scope| {
            format!(
                r#"View {{width: Fill height: Fit flow: Right spacing: 10 align: Align{{y: 0.0}}
                    RoundedView {{width: 7 height: 7 margin: Inset{{top: 10}} draw_bg +: {{color: #x1a7f37 border_radius: 3.5}}}}
                    Label {{width: Fill text: {} draw_text.wrap: Words draw_text.color: #x172336 draw_text.text_style.font_size: 13}}
                }}
                "#,
                lit(crate::providers::scope_words(scope).to_string())
            )
        })
        .collect();
    let trust = lit(format!(
        "You approve this on {site}{}. OctoSense keeps the connection; {app} never sees your password or token. You can disconnect it at any time.",
        if has_code { " with a one-time code" } else { "" }
    ));
    let preparing = lit(if has_code {
        "Getting a one-time code from GitHub…".to_string()
    } else {
        "Opening Google sign-in…".to_string()
    });
    let step_title = lit(if has_code {
        "Enter this code on GitHub".to_string()
    } else {
        "Continue in your browser".to_string()
    });
    let step_help = lit(if has_code {
        "Open GitHub copies the code. Paste it on github.com/login/device, then choose Authorize."
            .to_string()
    } else {
        "Sign in to Google, allow access, then come back here.".to_string()
    });
    let waiting = lit(format!("Waiting for you to approve on {name}"));
    let minutes = AUTH_LIFETIME.as_secs() / 60;
    let (expiry, expires_in) = if has_code {
        (
            format!("The code works for {minutes} minutes."),
            "Code expires in ",
        )
    } else {
        (
            format!("This sign-in stays open for {minutes} minutes."),
            "This sign-in expires in ",
        )
    };
    let expiry = lit(expiry);
    let expires_in = lit(expires_in.to_string());
    let open_label = lit(format!("Open {name}"));
    let continue_label = lit(format!("Continue to {name}"));
    format!(
        r#"
let ticket = {ticket}
let watching = false
let phase = "intro"
let dots = 0
fn host_dismiss() {{
    watching = false
    host.request("auth.sheet.cancel", {{ticket: ticket}}, fn(r) {{}})
}}
fn show(next) {{
    phase = next
    ui.intro.set_visible(next == "intro")
    ui.preparing.set_visible(next == "starting")
    ui.step.set_visible(next == "code")
    ui.failed.set_visible(next == "error")
    ui.oauth_continue.set_visible(next == "intro")
    if next == "error" {{ ui.oauth_cancel.set_text("Close") }}
}}
fn tick() {{
    if dots >= 3 {{ dots = 0 }} else {{ dots = dots + 1 }}
    let trail = ""
    for i in dots {{ trail = trail + "." }}
    ui.waiting.set_text({waiting} + trail)
}}
fn poll() {{
    if !watching {{ return }}
    host.request("auth.sheet.status", {{ticket: ticket}}, fn(r) {{
        if r.is_ok {{
            if r.data.phase == "browser" && phase != "code" {{
                if ui.device_code.bind(ticket, {open_label}) {{ show("code") }}
            }}
            if r.data.phase == "browser" && r.data.remaining != nil {{
                ui.expiry.set_text({expires_in} + r.data.remaining)
            }}
            if r.data.phase == "error" {{
                ui.failure.set_text(r.data.message)
                show("error")
                watching = false
            }}
        }} else {{ watching = false }}
        if watching {{
            if phase == "code" {{ tick() }}
            start_timeout(0.5, || poll())
        }}
    }})
}}
fn bind_lifetime() {{ return ui.oauth_lifetime.bind_auth_lifetime(ticket) }}
fn begin() {{
    if !bind_lifetime() {{ host_dismiss() return }}
    show("starting")
    host.request("auth.sheet.start", {{ticket: ticket}}, fn(r) {{
        if r.is_ok {{ watching = true poll() }} else {{ ui.failure.set_text(r.error) show("error") }}
    }})
}}
let content = SolidView {{width: Fill height: Fill flow: Down align: Align{{x: 0.5}} padding: Inset{{left: 24 right: 24 top: 28 bottom: 20}} draw_bg.color: #fff
    // A readable column on a wide window; the full width on a phone.
    View {{width: Fill max_width: 560 height: Fill flow: Down
        oauth_title := Label {{width: Fill text: {title} draw_text.color: #x172336 draw_text.text_style: theme.font_bold{{font_size: 22}}}}
        View {{width: Fill height: 6}}
        oauth_subtitle := Label {{width: Fill text: {subtitle} draw_text.wrap: Words draw_text.color: #x3b4a5e draw_text.text_style.font_size: 14}}
        Label {{width: Fill text: {app_id} draw_text.color: #x8a96a8 draw_text.text_style.font_size: 11}}
        View {{width: Fill height: 20}}
        ScrollYView {{width: Fill height: Fill flow: Down spacing: 16
            intro := View {{width: Fill height: Fit flow: Down spacing: 12
                Label {{width: Fill text: "It will be able to:" draw_text.color: #x172336 draw_text.text_style: theme.font_bold{{font_size: 14}}}}
                RoundedView {{width: Fill height: Fit flow: Down padding: 14 spacing: 10 draw_bg +: {{color: #xf3f6f9 border_radius: 10.0}}
                    {rows}
                }}
                Label {{width: Fill text: {trust} draw_text.wrap: Words draw_text.color: #x526071 draw_text.text_style.font_size: 13}}
            }}
            preparing := View {{visible: false width: Fill height: Fit
                Label {{width: Fill text: {preparing} draw_text.color: #x3b4a5e draw_text.text_style.font_size: 14}}
            }}
            step := View {{visible: false width: Fill height: Fit flow: Down spacing: 14
                Label {{width: Fill text: {step_title} draw_text.color: #x172336 draw_text.text_style: theme.font_bold{{font_size: 16}}}}
                device_code := SignInCode {{}}
                Label {{width: Fill text: {step_help} draw_text.wrap: Words draw_text.color: #x526071 draw_text.text_style.font_size: 13}}
                waiting := Label {{width: Fill text: {waiting} draw_text.color: #x172336 draw_text.text_style.font_size: 13}}
                expiry := Label {{width: Fill text: {expiry} draw_text.color: #x8a96a8 draw_text.text_style.font_size: 11}}
            }}
            failed := View {{visible: false width: Fill height: Fit flow: Down spacing: 8
                Label {{width: Fill text: "Not connected" draw_text.color: #xb42318 draw_text.text_style: theme.font_bold{{font_size: 16}}}}
                failure := Label {{width: Fill text: "" draw_text.wrap: Words draw_text.color: #x172336 draw_text.text_style.font_size: 13}}
            }}
        }}
        oauth_lifetime := WebReader {{width: 0 height: 0 visible: false}}
        View {{width: Fill height: 12}}
        oauth_continue := Button {{width: Fill height: 48 text: {continue_label} on_click: || begin()
            draw_bg +: {{color: #x1f2937 color_hover: #x111827 color_down: #x0b1220 color_focus: #x1f2937 border_size: 0.0 border_radius: 8.0}}
            draw_text +: {{color: #xffffff color_hover: #xffffff color_down: #xffffff color_focus: #xffffff text_style: theme.font_bold{{font_size: 13}}}}
        }}
        View {{width: Fill height: 8}}
        oauth_cancel := ButtonFlat {{width: Fill height: 44 text: "Cancel" on_click: || host_dismiss()
            draw_text +: {{color: #x3b4a5e color_hover: #x172336 color_down: #x172336 text_style: theme.font_regular{{font_size: 13}}}}
        }}
    }}
}}
start_timeout(0.0, || bind_lifetime())
content
"#
    )
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn removing_or_selecting_an_account_invalidates_only_its_authorization_scope() {
        let mut epochs = AuthorizationEpochs::default();
        let root = Path::new("fixture-profile");
        let old = epochs.get(root, "sample.one");
        epochs.invalidate(root, "sample.one");
        assert_ne!(
            epochs.get(root, "sample.one"),
            old,
            "a reinstalled app must not accept the old browser callback"
        );
        assert_eq!(epochs.get(root, "sample.two"), 0);
        assert_eq!(epochs.get(Path::new("other-profile"), "sample.one"), 0);
        let fresh = epochs.get(root, "sample.one");
        epochs.invalidate(root, "sample.one");
        assert_ne!(
            epochs.get(root, "sample.one"),
            fresh,
            "an account switch retires the next attempt too"
        );
    }
}

/// The contained app never receives this sheet's authorization URL or callback.
fn embedded_sheet(ticket: &str, p: &Pending) -> String {
    let ticket = json!(ticket).to_string();
    let backend = p
        .backend
        .as_ref()
        .expect("Backend presentation is validated");
    let origin = url::Url::parse(&backend.registration().authorization_url)
        .map(|u| u.origin().ascii_serialization())
        .unwrap_or_default();
    let app = json!(&p.app).to_string();
    let origin = json!(origin).to_string();
    format!(
        r#"
let ticket = {ticket}
let watching = false
let opened_url = ""
fn host_dismiss() {{
    watching = false
    ui.oauth_webview.close()
    host.request("auth.sheet.cancel", {{ticket: ticket}}, fn(r) {{}})
}}
fn changed() {{
    let state = ui.oauth_webview.auth_status()
    if state == "loading" {{ ui.oauth_status.set_text("Loading sign-in…") }}
    if state == "loaded" {{ ui.oauth_status.set_text("Sign in or create an account on this app’s website.") }}
    if state == "error" {{ ui.oauth_status.set_text("Could not load sign-in. Check your connection and retry.") }}
    if state == "blocked" {{ ui.oauth_status.set_text("This page tried to leave the app’s login website. Use the browser login option for another provider.") }}
    if state == "cancelled" {{ host_dismiss() }}
    if state == "callback" {{
        let callback = ui.oauth_webview.auth_callback()
        if callback == "" {{ return }}
        ui.oauth_status.set_text("Completing sign-in…")
        host.request("auth.sheet.callback", {{ticket: ticket, url: callback}}, fn(r) {{
            if !r.is_ok {{ ui.oauth_status.set_text(r.error) }}
        }})
    }}
}}
fn poll() {{
    if !watching {{ return }}
    host.request("auth.sheet.status", {{ticket: ticket}}, fn(r) {{
        if r.is_ok {{
            if r.data.phase == "webview" && opened_url != r.data.url {{
                opened_url = r.data.url
                ui.oauth_intro.set_visible(false)
                ui.oauth_continue.set_visible(false)
                ui.oauth_toolbar.set_visible(true)
                ui.oauth_webview.set_visible(true)
                if !ui.oauth_webview.open_auth(r.data.url, r.data.callback) {{
                    ui.oauth_status.set_text("Embedded sign-in is unavailable on this platform.")
                    host_dismiss()
                }}
            }}
            if r.data.phase == "error" {{
                ui.oauth_webview.close()
                ui.oauth_status.set_text(r.data.message)
                watching = false
            }}
        }} else {{
            ui.oauth_webview.close()
            watching = false
        }}
        if watching {{ start_timeout(0.25, || poll()) }}
    }})
}}
fn bind_lifetime() {{ return ui.oauth_webview.bind_auth_lifetime(ticket) }}
fn begin() {{
    if !bind_lifetime() {{ host_dismiss() return }}
    ui.oauth_status.set_text("Preparing sign-in…")
    host.request("auth.sheet.start", {{ticket: ticket}}, fn(r) {{
        if r.is_ok {{ watching = true poll() }} else {{ ui.oauth_status.set_text(r.error) }}
    }})
}}
let content = SolidView {{width: Fill height: Fill flow: Down padding: 16 spacing: 10 draw_bg.color: #f8fafc
    Label {{width: Fill text: "Sign in to this app" draw_text.color: #172033 draw_text.text_style.font_size: 22}}
    Label {{width: Fill text: {app} draw_text.color: #43536c draw_text.text_style.font_size: 12}}
    Label {{width: Fill text: {origin} draw_text.color: #172033 draw_text.text_style.font_size: 13}}
    oauth_intro := View {{width: Fill height: Fill flow: Down spacing: 12
        Label {{width: Fill text: "Continue to this app’s website to sign in or create an account. This app can then read its declared backend data. Changes require your review. OctoSense keeps credentials in its secure store." draw_text.color: #43536c}}
    }}
    oauth_webview := WebReader {{width: Fill height: Fill visible: false on_auth: || changed()}}
    oauth_status := Label {{width: Fill text: "Continue to authorize this connection." draw_text.color: #43536c draw_text.text_style.font_size: 12}}
    oauth_toolbar := View {{width: Fill height: Fit flow: Right spacing: 8 visible: false
        Button {{width: Fill height: 44 text: "Back" on_click: || ui.oauth_webview.auth_back()}}
        Button {{width: Fill height: 44 text: "Retry" on_click: || ui.oauth_webview.auth_retry()}}
    }}
    oauth_continue := Button {{width: Fill height: 48 text: "Continue" on_click: || begin()}}
    ButtonFlat {{width: Fill height: 44 text: "Cancel" on_click: || host_dismiss()}}
}}
start_timeout(0.0, || bind_lifetime())
content
"#
    )
}

#[cfg(test)]
#[path = "host_lifetime_tests.rs"]
mod host_lifetime_tests;

#[cfg(test)]
#[path = "host_operation_tests.rs"]
mod host_operation_tests;

#[cfg(test)]
#[path = "host_sheet_tests.rs"]
mod host_sheet_tests;
