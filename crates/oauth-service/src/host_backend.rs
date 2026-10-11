//! App-bound backend login within the host-owned auth service. This module
//! never accepts an endpoint or registration from an app request.
use super::*;
use crate::backend::{
    BackendClient, BackendDeclaration, BackendRegistration, BackendRequest, SESSION_SCOPE,
};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registrations {
    schema: u32,
    apps: BTreeMap<String, BackendRegistration>,
}

/// The shell resolves only digest-verified admitted bundle metadata. This hook
/// runs for every use; errors (including withdrawal) never fall back to config.
pub type BackendResolver =
    Arc<dyn Fn(&Path, &str) -> Result<Option<BackendDeclaration>, String> + Send + Sync>;
static RESOLVER: Mutex<Option<BackendResolver>> = Mutex::new(None);
pub fn set_backend_resolver(resolver: BackendResolver) {
    *RESOLVER.lock().unwrap_or_else(|e| e.into_inner()) = Some(resolver);
}

/// Install/update owner calls this when a backend declaration changes or is
/// removed. Existing token handles are durably revoked before the new bundle
/// can reconnect; restoring an older declaration cannot revive old sessions.
pub fn invalidate_backend_registration(root: &Path, app: &str) -> Result<(), String> {
    let operation = operation_lock(root, app);
    let operation_guard = operation.lock().unwrap_or_else(|e| e.into_inner());
    let guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    invalidate_authorizations(root, app);
    let mut store = connections(root)?;
    let previous = store.active(app);
    let mut failure = None;
    for entry in store
        .list(app)
        .into_iter()
        .filter(|entry| entry.provider == Provider::Backend)
    {
        if let Err(error) = store.disconnect(app, &entry.handle) {
            failure.get_or_insert(error);
        }
    }
    let selected = store.active(app);
    drop(guard);
    drop(operation_guard);
    account_changed(
        app,
        previous.as_ref().map(|entry| entry.handle.as_str()),
        selected.as_ref().map(|entry| entry.handle.as_str()),
    );
    failure.map_or(Ok(()), Err)
}

#[cfg(feature = "acceptance-fixtures")]
fn fixtures() -> &'static Mutex<HashMap<(PathBuf, String), Arc<BackendClient>>> {
    static FIXTURES: std::sync::OnceLock<Mutex<HashMap<(PathBuf, String), Arc<BackendClient>>>> =
        std::sync::OnceLock::new();
    FIXTURES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Test executable setup only. Registers an endpoint, never a token/account or
/// replacement vault. The real platform vault remains in use for this journey.
#[cfg(feature = "acceptance-fixtures")]
pub fn register_fixture(root: &Path, client: BackendClient) -> Result<(), String> {
    let root = crate::acceptance_fixtures::validate_root(root)?;
    let key = (root, client.registration().app_id.clone());
    let mut registry = fixtures().lock().unwrap_or_else(|e| e.into_inner());
    if registry.contains_key(&key) {
        return Err("Backend acceptance registration already exists".into());
    }
    registry.insert(key, Arc::new(client));
    Ok(())
}

/// Called while STORE_LOCK is held. Resolving an updated/withdrawn bundle
/// durably revokes earlier handles before its credentials could be used.
pub(super) fn client(root: &Path, app: &str) -> Result<Arc<BackendClient>, String> {
    let result = resolve_client(root, app);
    if RESOLVER.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
        crate::backend_registry::observe(
            root,
            app,
            result.as_ref().ok().map(|client| client.binding()),
            || {
                invalidate_authorizations(root, app);
                let mut store = connections(root)?;
                let mut failure = None;
                for entry in store
                    .list(app)
                    .into_iter()
                    .filter(|entry| entry.provider == Provider::Backend)
                {
                    if let Err(error) = store.disconnect(app, &entry.handle) {
                        failure.get_or_insert(error);
                    }
                }
                failure.map_or(Ok(()), Err)
            },
        )?;
    }
    result
}

/// The shell calls this after an install/catalog change, outside the UI thread.
/// Resolution repeats on every actual request, so this is eager cleanup only.
pub fn revalidate_backend_registration(root: &Path, app: &str) -> Result<(), String> {
    let operation = operation_lock(root, app);
    let operation_guard = operation.lock().unwrap_or_else(|e| e.into_inner());
    let guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let previous = connections(root)?.active(app);
    let result = client(root, app).map(|_| ());
    let selected = connections(root)?.active(app);
    drop(guard);
    drop(operation_guard);
    account_changed(
        app,
        previous.as_ref().map(|entry| entry.handle.as_str()),
        selected.as_ref().map(|entry| entry.handle.as_str()),
    );
    result
}

fn resolve_client(root: &Path, app: &str) -> Result<Arc<BackendClient>, String> {
    #[cfg(feature = "acceptance-fixtures")]
    if let Ok(canonical) = root.canonicalize() {
        if let Some(client) = fixtures()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(canonical, app.to_owned()))
            .cloned()
        {
            return Ok(client);
        }
    }
    let resolver = RESOLVER.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(resolver) = resolver {
        if let Some(declaration) = resolver(root, app)? {
            return BackendClient::new(declaration.into_registration(app)).map(Arc::new);
        }
    }
    let file = std::fs::File::open(root.join("oauth/backends.json"))
        .map_err(|_| Provider::Backend.sign_in_unavailable())?;
    let mut bytes = Vec::new();
    file.take(65_537)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read this app's backend registration")?;
    if bytes.len() > 65_536 {
        return Err("Backend registration exceeds its limit".into());
    }
    let mut registrations: Registrations =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid host backend registration")?;
    if registrations.schema != 1 || registrations.apps.len() > 128 {
        return Err("Invalid host backend registration".into());
    }
    let registration = registrations
        .apps
        .remove(app)
        .filter(|registration| registration.app_id == app)
        .ok_or(Provider::Backend.sign_in_unavailable())?;
    BackendClient::new(registration).map(Arc::new)
}

fn current(
    root: &Path,
    app: &str,
    backend: &BackendClient,
    epoch: u64,
    scope_check: &ScopeCheck,
) -> Result<(), String> {
    if authorization_epoch(root, app) != epoch
        || !scope_check(
            app,
            Provider::Backend,
            &BTreeSet::from([SESSION_SCOPE.into()]),
        )
        || client(root, app)?.binding() != backend.binding()
    {
        return Err("Backend authorization or registration changed; sign in again".into());
    }
    Ok(())
}

pub(super) fn authorize(p: &Pending) -> Result<crate::Connection, String> {
    let backend = p
        .backend
        .as_ref()
        .ok_or(Provider::Backend.sign_in_unavailable())?;
    {
        let _guard = STORE_LOCK.lock().unwrap();
        current(&p.root, &p.app, backend, p.epoch, &p.scope_check)?;
    }
    let code = if p.embedded {
        embedded_code(p, backend)?
    } else {
        browser_code(p, backend)?
    };
    {
        let _guard = STORE_LOCK.lock().unwrap();
        current(&p.root, &p.app, backend, p.epoch, &p.scope_check)?;
        if p.cancelled.load(Ordering::SeqCst) || Instant::now() >= p.deadline {
            return Err("Sign-in cancelled or expired".into());
        }
    }
    let authorized = backend.finish(&p.app, code, unix_now())?;
    let saved = (|| {
        let operation = operation_lock(&p.root, &p.app);
        let operation_guard = operation.lock().unwrap_or_else(|e| e.into_inner());
        let guard = STORE_LOCK.lock().unwrap();
        current(&p.root, &p.app, backend, p.epoch, &p.scope_check)?;
        if p.cancelled.load(Ordering::SeqCst) || Instant::now() >= p.deadline {
            return Err("Sign-in cancelled or expired".into());
        }
        let mut store = connections(&p.root)?;
        let previous = store.active(&p.app);
        let connection = store.connect_backend(
            &p.app,
            &backend.registration().id,
            backend.binding(),
            &authorized.identity.sub,
            &authorized.identity.label,
            &authorized.tokens,
        )?;
        if p.cancelled.load(Ordering::SeqCst) || Instant::now() >= p.deadline {
            store.disconnect(&p.app, &connection.handle)?;
            if let Some(previous) = previous {
                store.select(&p.app, &previous.handle)?;
            }
            return Err("Sign-in cancelled or expired".into());
        }
        drop(guard);
        drop(operation_guard);
        account_changed(
            &p.app,
            previous.as_ref().map(|c| c.handle.as_str()),
            Some(&connection.handle),
        );
        Ok(connection)
    })();
    if saved.is_err() {
        let _ = backend.logout(&p.app, &authorized.tokens);
    }
    saved
}

fn embedded_code(
    p: &Pending,
    backend: &BackendClient,
) -> Result<crate::backend::BackendCode, String> {
    let mut attempt = backend.begin_webview(&p.app, Instant::now())?;
    *p.status.lock().unwrap() = json!({
        "phase":"webview", "url":attempt.authorization_url().as_str(),
        "callback":crate::backend::WEBVIEW_CALLBACK_URL,
    });
    // Only the worker owns this receiver. The UI uses a bounded try_send.
    let receiver = p
        .callback_rx
        .lock()
        .map_err(|_| "Authentication receiver stopped")?;
    loop {
        if p.cancelled.load(Ordering::SeqCst) || Instant::now() >= p.deadline {
            attempt.cancel();
            return Err("Sign-in cancelled or expired".into());
        }
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(url) => return attempt.consume_callback(&p.app, &url, Instant::now()),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => (),
            Err(_) => return Err("Authentication view closed".into()),
        }
    }
}

fn browser_code(
    p: &Pending,
    backend: &BackendClient,
) -> Result<crate::backend::BackendCode, String> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .map_err(|_| "Cannot start backend callback listener")?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "Cannot configure backend callback listener")?;
    let port = listener
        .local_addr()
        .map_err(|_| "Cannot read callback address")?
        .port();
    let redirect = url::Url::parse(&format!("http://127.0.0.1:{port}/oauth/callback")).unwrap();
    let mut attempt = backend.begin(&p.app, redirect, Instant::now())?;
    *p.status.lock().unwrap() =
        json!({"phase":"browser","url":attempt.authorization_url().as_str(),"code":""});
    loop {
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
                    Ok(count) => count,
                    Err(_) => continue,
                };
                let request = String::from_utf8_lossy(&bytes[..count]);
                let mut words = request.lines().next().unwrap_or("").split_whitespace();
                if words.next() != Some("GET") {
                    continue;
                }
                let path = words.next().unwrap_or("");
                if !path.starts_with("/oauth/callback?") {
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    continue;
                }
                match attempt.consume_callback(
                    &p.app,
                    &format!("http://127.0.0.1:{port}{path}"),
                    Instant::now(),
                ) {
                    Ok(code) => {
                        let body = "Authorization received. Return to OctoSense.";
                        let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
                        return Ok(code);
                    }
                    Err(error) => {
                        let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        if attempt.is_finished() {
                            return Err(error);
                        }
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(_) => return Err("Backend callback listener stopped".into()),
        }
    }
}

// A rotating refresh token must have exactly one in-flight profile/refresh
// request per connection. Reject overlapping requests instead of queuing them
// across account changes, and always release the lease on every error path.
struct ProfileLease((PathBuf, String, String));
fn profile_leases() -> &'static Mutex<std::collections::HashSet<(PathBuf, String, String)>> {
    static LEASES: std::sync::OnceLock<
        Mutex<std::collections::HashSet<(PathBuf, String, String)>>,
    > = std::sync::OnceLock::new();
    LEASES.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}
impl ProfileLease {
    fn acquire(root: &Path, app: &str, handle: &str) -> Result<Self, String> {
        let key = (
            root.canonicalize()
                .map_err(|_| "Backend host directory is unavailable")?,
            app.to_owned(),
            handle.to_owned(),
        );
        if !profile_leases()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.clone())
        {
            return Err("Backend account request is already in progress; try again shortly".into());
        }
        Ok(Self(key))
    }
}
impl Drop for ProfileLease {
    fn drop(&mut self) {
        profile_leases()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

pub(super) fn me(call: &ServiceCall, scope_check: &ScopeCheck) -> Result<Value, String> {
    let handle = call.args["connection"]
        .as_str()
        .ok_or("Choose a backend account")?;
    with_session(
        &call.host_dir,
        &call.app_id,
        handle,
        scope_check,
        None,
        |backend, tokens, subject| {
            let identity = backend.me(&call.app_id, tokens)?;
            if identity.sub != subject {
                return Err("Backend account identity changed; sign in again".into());
            }
            Ok(
                json!({"connection":handle,"backend_id":backend.registration().id,"identity":identity}),
            )
        },
    )
}

/// Immutable request retained only in host Rust while the native review is open.
#[derive(Clone)]
pub(crate) struct PreparedRequest {
    pub app: String,
    pub root: PathBuf,
    pub request: BackendRequest,
    pub account: String,
    pub origin: String,
    pub method: String,
    pub path: String,
    pub binding: String,
    epoch: u64,
    scope_check: ScopeCheck,
}
impl PreparedRequest {
    pub fn mutates(&self) -> bool {
        self.method != "GET"
    }
    pub fn execute(&self) -> Result<Value, String> {
        with_session(
            &self.root,
            &self.app,
            &self.request.connection,
            &self.scope_check,
            Some((&self.binding, self.epoch)),
            |backend, tokens, _| backend.request(&self.app, tokens, &self.request),
        )
    }
}
pub(crate) fn prepare_request(
    call: &ServiceCall,
    scope_check: ScopeCheck,
) -> Result<PreparedRequest, String> {
    let request: BackendRequest =
        serde_json::from_value(call.args.clone()).map_err(|_| "Invalid backend request")?;
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let backend = client(&call.host_dir, &call.app_id)?;
    let epoch = authorization_epoch(&call.host_dir, &call.app_id);
    current(&call.host_dir, &call.app_id, &backend, epoch, &scope_check)?;
    backend.validate_request(&request)?;
    let operation = backend.operation(&request.operation)?;
    let store = connections(&call.host_dir)?;
    store.backend_tokens(
        &call.app_id,
        &request.connection,
        &backend.registration().id,
        backend.binding(),
    )?;
    let account = store
        .authorized(
            &call.app_id,
            &request.connection,
            Provider::Backend,
            SESSION_SCOPE,
        )?
        .label
        .clone();
    Ok(PreparedRequest {
        app: call.app_id.clone(),
        root: call.host_dir.clone(),
        request,
        account,
        origin: url::Url::parse(&backend.registration().authorization_url)
            .unwrap()
            .origin()
            .ascii_serialization(),
        method: operation.method.clone(),
        path: operation.path.clone(),
        binding: backend.binding().into(),
        epoch,
        scope_check,
    })
}

fn with_session(
    root: &Path,
    app: &str,
    handle: &str,
    scope_check: &ScopeCheck,
    expected: Option<(&str, u64)>,
    run: impl FnOnce(&BackendClient, &Tokens, &str) -> Result<Value, String>,
) -> Result<Value, String> {
    let operation = operation_lock(root, app);
    let _operation = operation.lock().unwrap_or_else(|e| e.into_inner());
    let _lease = ProfileLease::acquire(root, app, handle)?;
    let (backend, epoch, subject, mut tokens) = {
        let _guard = STORE_LOCK.lock().unwrap();
        let backend = client(root, app)?;
        let epoch = authorization_epoch(root, app);
        current(root, app, &backend, epoch, scope_check)?;
        if expected.is_some_and(|(binding, original_epoch)| {
            binding != backend.binding() || original_epoch != epoch
        }) {
            return Err("Backend authorization or registration changed; review again".into());
        }
        let store = connections(root)?;
        let tokens =
            store.backend_tokens(app, handle, &backend.registration().id, backend.binding())?;
        let subject = store
            .authorized(app, handle, Provider::Backend, SESSION_SCOPE)?
            .subject
            .clone();
        (backend, epoch, subject, tokens)
    };
    let refreshed = tokens
        .expires_at
        .is_some_and(|at| at <= unix_now().saturating_add(60));
    if refreshed {
        tokens = backend.refresh(app, &tokens, unix_now())?;
    }
    // A backend may rotate its refresh token immediately. Persist it before
    // /me: a transient profile failure must not strand the next login attempt
    // with the already-consumed old token. Binding/epoch checks still prevent
    // a revoked or switched account from accepting refreshed credentials.
    {
        let _guard = STORE_LOCK.lock().unwrap();
        current(root, app, &backend, epoch, scope_check)?;
        let mut store = connections(root)?;
        store.backend_tokens(app, handle, &backend.registration().id, backend.binding())?;
        if refreshed {
            store.replace_backend_tokens(
                app,
                handle,
                &backend.registration().id,
                backend.binding(),
                tokens,
            )?;
            tokens =
                store.backend_tokens(app, handle, &backend.registration().id, backend.binding())?;
        }
    }
    let result = run(&backend, &tokens, &subject)?;
    let _guard = STORE_LOCK.lock().unwrap();
    current(root, app, &backend, epoch, scope_check)?;
    let store = connections(root)?;
    store.backend_tokens(app, handle, &backend.registration().id, backend.binding())?;
    Ok(result)
}

pub(super) type Logout = Option<(Arc<BackendClient>, Tokens)>;
pub(super) fn logout_material(
    root: &Path,
    app: &str,
    handle: &str,
    store: &Connections,
) -> Option<Logout> {
    let entry = store
        .list(app)
        .into_iter()
        .find(|entry| entry.handle == handle)?;
    if entry.provider != Provider::Backend {
        return None;
    }
    Some((|| {
        let backend = client(root, app).ok()?;
        let tokens = store
            .backend_logout_tokens(app, handle, &backend.registration().id, backend.binding())
            .ok()?;
        Some((backend, tokens))
    })())
}
pub(super) fn logout(app: &str, material: Logout) -> Value {
    match material {
        Some((backend, tokens)) => {
            json!({"attempted":true,"succeeded":backend.logout(app, &tokens).is_ok()})
        }
        None => json!({"attempted":false,"succeeded":false}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registration(app: &str) -> BackendRegistration {
        BackendRegistration {
            id: "synthetic-backend".into(),
            app_id: app.into(),
            client_id: "synthetic-native".into(),
            authorization_url: "https://backend.example/authorize".into(),
            token_url: "https://backend.example/token".into(),
            me_url: "https://backend.example/me".into(),
            logout_url: "https://backend.example/logout".into(),
            scopes: BTreeSet::from([SESSION_SCOPE.into()]),
            operations: Default::default(),
        }
    }
    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("backend-host-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("oauth")).unwrap();
        root
    }
    fn write(root: &Path, value: Value) {
        std::fs::write(
            root.join("oauth/backends.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn host_backend_registration_is_exact_app_bound_and_fail_closed() {
        let root = root();
        assert!(client(&root, "app.one").is_err());
        write(
            &root,
            json!({"schema":1,"apps":{"app.one":registration("app.one")}}),
        );
        let original = client(&root, "app.one").unwrap();
        assert_eq!(original.registration().app_id, "app.one");
        assert!(client(&root, "app.two").is_err());
        write(
            &root,
            json!({"schema":1,"apps":{"app.one":registration("app.two")}}),
        );
        assert!(client(&root, "app.one").is_err());
        write(
            &root,
            json!({"schema":2,"apps":{"app.one":registration("app.one")}}),
        );
        assert!(client(&root, "app.one").is_err());
        write(
            &root,
            json!({"schema":1,"apps":{"app.one":registration("app.one")},"extra":true}),
        );
        assert!(client(&root, "app.one").is_err());
        std::fs::write(root.join("oauth/backends.json"), vec![b' '; 65_537]).unwrap();
        assert!(client(&root, "app.one").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn production_config_rejects_loopback_and_binding_changes_revoke_inflight_access() {
        let root = root();
        write(
            &root,
            json!({"schema":1,"apps":{"app.one":registration("app.one")}}),
        );
        let original = client(&root, "app.one").unwrap();
        let allow: ScopeCheck = Arc::new(|_, _, _| true);
        let epoch = authorization_epoch(&root, "app.one");
        current(&root, "app.one", &original, epoch, &allow).unwrap();
        let mut changed = registration("app.one");
        changed.client_id = "changed-native-client".into();
        write(&root, json!({"schema":1,"apps":{"app.one":changed}}));
        assert!(current(&root, "app.one", &original, epoch, &allow).is_err());
        let mut local = registration("app.one");
        for endpoint in [
            &mut local.authorization_url,
            &mut local.token_url,
            &mut local.me_url,
            &mut local.logout_url,
        ] {
            *endpoint = endpoint.replace("https://backend.example", "http://127.0.0.1:54321");
        }
        write(&root, json!({"schema":1,"apps":{"app.one":local}}));
        assert!(client(&root, "app.one").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn profile_lease_rejects_concurrent_refresh_and_releases_after_error_scope() {
        let root = root();
        let lease = ProfileLease::acquire(&root, "app.one", "account-one").unwrap();
        assert!(ProfileLease::acquire(&root, "app.one", "account-one").is_err());
        let other = ProfileLease::acquire(&root, "app.one", "account-two").unwrap();
        drop(lease);
        let retry = ProfileLease::acquire(&root, "app.one", "account-one").unwrap();
        drop((other, retry));
        std::fs::remove_dir_all(root).unwrap();
    }
}
