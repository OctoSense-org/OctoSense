//! Shared `auth` service. All protocol work runs off the Makepad UI thread.
use crate::{
    authorize::{exchange_google, identity, DevicePoll, GithubDeviceAttempt},
    oauth::{GoogleAttempt, Tokens, AUTH_LIFETIME},
    providers::{ClientRegistration, Provider},
    transport::HttpsTransport,
    Connections, CredentialStore,
};
use octosense_appstore::services::{self, HostService, Replier, ServiceCall, ServiceHost};
use serde::Deserialize;
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Client {
    pub client_id: String,
    pub client_secret: Option<String>,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct Clients {
    pub github: Option<Client>,
    pub google: Option<Client>,
}
pub(crate) fn clients(root: &Path) -> Result<Clients, String> {
    let bytes = std::fs::read(root.join("oauth/clients.json")).map_err(|_| {
        "OAuth is not configured. Add provider registrations in the host's oauth/clients.json"
    })?;
    if bytes.len() > 16384 {
        return Err("OAuth host configuration exceeds its limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid OAuth host configuration".into())
}
pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

struct Pending {
    app: String,
    root: PathBuf,
    provider: Provider,
    scopes: BTreeSet<String>,
    original: Replier,
    cancelled: AtomicBool,
    started: AtomicBool,
    deadline: Instant,
    status: Mutex<Value>,
    epoch: u64,
    scope_check: ScopeCheck,
}
struct AuthService {
    scope_check: ScopeCheck,
    pending: HashMap<String, Arc<Pending>>,
}

pub fn register(scope_check: ScopeCheck) {
    services::register_host_service(Box::new(AuthService {
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
    fn family(&self) -> &'static str {
        "auth"
    }
    fn call(&mut self, call: ServiceCall, reply: Replier, host: &mut dyn ServiceHost) {
        self.cleanup();
        if call.method().starts_with("sheet.") && !call.from_sheet {
            reply.send(Err("Authentication controls belong to the host".into()));
            return;
        }
        match call.method() {
            "active" => reply.send(Ok(json!(active_connection(&call.host_dir, &call.app_id)))),
            "select" => {
                std::thread::spawn(move || {
                    let result = (|| {
                        let guard = STORE_LOCK.lock().unwrap();
                        let mut store = connections(&call.host_dir)?;
                        let previous = store.active(&call.app_id);
                        let handle = call.args["connection"]
                            .as_str()
                            .ok_or("Choose a connected account")?;
                        let selected = store.select(&call.app_id, handle)?;
                        invalidate_authorizations(&call.host_dir, &call.app_id);
                        drop(guard);
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
                    let guard = STORE_LOCK.lock().unwrap();
                    let result = (|| {
                        let handle = call.args["connection"]
                            .as_str()
                            .ok_or("Choose a connected account")?;
                        let mut store = connections(&call.host_dir)?;
                        let previous = store.active(&call.app_id);
                        let revoked = store.disconnect(&call.app_id, handle);
                        invalidate_authorizations(&call.host_dir, &call.app_id);
                        let current = store.active(&call.app_id);
                        drop(guard);
                        account_changed(
                            &call.app_id,
                            previous.as_ref().map(|c| c.handle.as_str()),
                            current.as_ref().map(|c| c.handle.as_str()),
                        );
                        revoked?;
                        Ok(json!({"disconnected":true}))
                    })();
                    reply.send(result);
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
                        .map_err(|_| "Choose GitHub or Google")?;
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
                    Ok((
                        provider,
                        scopes,
                        authorization_epoch(&call.host_dir, &call.app_id),
                    ))
                })();
                let (provider, scopes, epoch) = match parsed {
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
                let ticket = Uuid::new_v4().to_string();
                let pending = Arc::new(Pending {
                    app: call.app_id,
                    root: call.host_dir,
                    provider,
                    scopes,
                    original: reply,
                    cancelled: AtomicBool::new(false),
                    started: AtomicBool::new(false),
                    deadline: Instant::now() + AUTH_LIFETIME,
                    status: Mutex::new(json!({"phase":"ready"})),
                    epoch,
                    scope_check: self.scope_check.clone(),
                });
                host.open_sheet(consent_sheet(&ticket, &pending));
                self.pending.insert(ticket, pending);
            }
            "sheet.start" => match self.pending(&call) {
                Err(e) => reply.send(Err(e)),
                Ok(pending) => {
                    if !pending.started.swap(true, Ordering::SeqCst) {
                        *pending.status.lock().unwrap() = json!({"phase":"starting"});
                        std::thread::spawn(move || {
                            let result = authorize(&pending);
                            if pending.cancelled.load(Ordering::SeqCst) {
                                return;
                            }
                            match result {
                                Ok(connection) => {
                                    *pending.status.lock().unwrap() = json!({"phase":"connected"});
                                    pending.original.clone().send(Ok(json!(connection)));
                                }
                                Err(error) => {
                                    *pending.status.lock().unwrap() =
                                        json!({"phase":"error","message":error});
                                    // Settle the app request even if the user leaves the
                                    // error sheet visible. A second reply is discarded.
                                    pending.original.clone().send(Err(error));
                                }
                            }
                        });
                    }
                    reply.send(Ok(json!({"started":true})));
                }
            },
            "sheet.status" => match self.pending(&call) {
                Ok(p) => {
                    let status = p.status.lock().unwrap().clone();
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

fn authorize(p: &Pending) -> Result<crate::Connection, String> {
    let settings = clients(&p.root)?;
    let client = match p.provider {
        Provider::Github => settings.github,
        Provider::Google => settings.google,
    }
    .ok_or("This provider is not configured in OctoSense")?;
    let registration = ClientRegistration {
        client_id: client.client_id,
    };
    let transport = HttpsTransport::new()?;
    let scopes: Vec<String> = p.scopes.iter().cloned().collect();
    let tokens: Tokens = match p.provider {
        Provider::Github => {
            let mut attempt = GithubDeviceAttempt::begin(
                &p.app,
                &registration,
                &scopes,
                &transport,
                Instant::now(),
            )?;
            *p.status.lock().unwrap() =
                json!({"phase":"browser","url":attempt.verification_uri,"code":attempt.user_code});
            loop {
                if p.cancelled.load(Ordering::SeqCst) || Instant::now() >= p.deadline {
                    attempt.cancel();
                    return Err("Sign-in cancelled or expired".into());
                }
                match attempt.poll(&p.app, &transport, Instant::now(), unix_now())? {
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
                &transport,
                unix_now(),
            )?
        }
    };
    let (subject, label) = identity(p.provider, &tokens, &transport)?;
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
    account_changed(
        &p.app,
        previous.as_ref().map(|c| c.handle.as_str()),
        Some(&connection.handle),
    );
    Ok(connection)
}

fn consent_sheet(ticket: &str, p: &Pending) -> String {
    // All variable text enters as a JSON string literal, never executable Splash.
    let ticket = json!(ticket).to_string();
    let title = json!(format!(
        "Connect {}",
        match p.provider {
            Provider::Github => "GitHub",
            Provider::Google => "Google",
        }
    ))
    .to_string();
    let description = json!(format!(
        "{} requests:\n{}\n\nCredentials stay with OctoSense.",
        p.app,
        p.scopes
            .iter()
            .map(|scope| crate::providers::scope_words(scope))
            .collect::<Vec<_>>()
            .join("\n")
    ))
    .to_string();
    format!(
        r#"
let ticket = {ticket}
let watching = false
let browser_url = ""
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
fn begin() {{
    ui.oauth_status.set_text("Preparing secure sign-in…")
    host.request("auth.sheet.start", {{ticket: ticket}}, fn(r) {{
        if r.is_ok {{ watching = true poll() }} else {{ ui.oauth_status.set_text(r.error) }}
    }})
}}
SolidView {{width: Fill height: Fill flow: Down padding: 20 spacing: 16 draw_bg.color: #fff
    Label {{width: Fill text: {title} draw_text.color: #222 draw_text.text_style.font_size: 22}}
    ScrollYView {{width: Fill height: Fill
        Label {{width: Fill text: {description} draw_text.color: #444 draw_text.text_style.font_size: 13}}
    }}
    oauth_code := Label {{width: Fill draw_text.color: #222 draw_text.text_style.font_size: 22}}
    oauth_browser := View {{width: Fill height: Fit on_render: || {{
        if browser_url != "" {{
            LinkLabel {{width: Fill text: "Open secure sign-in in your browser" url: browser_url}}
        }}
    }}}}
    oauth_status := Label {{width: Fill text: "Continue to authorize this connection." draw_text.color: #444}}
    Button {{width: Fill height: 48 text: "Continue" on_click: || begin()}}
    ButtonFlat {{width: Fill height: 44 text: "Cancel" on_click: || {{watching = false host.request("auth.sheet.cancel", {{ticket: ticket}}, fn(r) {{}})}}}}
}}
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
