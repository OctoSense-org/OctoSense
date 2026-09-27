//! The broker: one app's scoped assistant service over UI Protocol frames.
//!
//! One [`Broker`] serves one app (one module instance). It holds its own
//! connection to the kernel (through a [`Connector`]), and:
//!
//! - binds the app's peer: `peer/prepare` with the app/account memory
//!   namespace and `resume: true`, owned by the configured originator (the
//!   host's system agent session, or a standalone app's own root session);
//!   the kernel provisions the peer's workspace. A kernel without the
//!   host-owned app peer contract (octos UPCR-2026-034) is refused, never
//!   substituted by an ordinary privileged session;
//! - opens one kernel request context per client instance
//!   (`peer/context/open`), and closes it (`peer/context/close`) when the
//!   instance closes, the account changes or the app releases;
//! - checks the lease on every call — context open, same account and account
//!   generation, app not released, service in the app's grant AND the
//!   instance's — before the request and again before any reply is
//!   delivered, so a late reply never reaches a new account or instance;
//! - routes each notification to the context whose session it names.
//!
//! Nothing here chooses a provider, touches credentials or stops a kernel it
//! does not own.

use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use crate::contract::*;

/// A boxed future, for the object-safe transport traits.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One JSON-RPC frame stream to a kernel.
pub trait Link: Send {
    /// Queue one frame. Never blocks.
    fn send(&mut self, frame: String) -> Result<(), String>;
    /// The next frame, or why the link ended.
    fn recv(&mut self) -> BoxFuture<'_, Result<String, String>>;
}

/// How a broker reaches its kernel.
pub trait Connector: Send + Sync {
    /// Whether a kernel can be reached here at all (checked before starting
    /// anything); the reason when not.
    fn available(&self) -> Result<(), String>;
    /// Open a link, starting an owned runtime if needed.
    fn connect(&self) -> BoxFuture<'static, Result<Box<dyn Link>, String>>;
    /// Whether this connector owns the runtime it starts.
    fn owns_runtime(&self) -> bool;
    /// Stop the runtime; a no-op unless [`Connector::owns_runtime`].
    fn shutdown(&self);
}

/// What a broker is for.
#[derive(Clone, Debug)]
pub struct BrokerConfig {
    pub deployment: Deployment,
    /// The kernel profile (the shared provider profile, `_main` in a shell).
    pub profile_id: String,
    /// The session that owns the app's peer: the host's system agent, or a
    /// standalone app's own root session. Recorded by the kernel as the
    /// peer's originator.
    pub originator: String,
    /// The app's id, one memory-namespace segment (`[a-z0-9][a-z0-9._-]*`).
    pub app_id: String,
    /// What the peer is called (a per-account suffix is added).
    pub app_label: String,
    /// The peer's standing brief.
    pub brief: String,
    /// The app's effective assistant services.
    pub services: BTreeSet<String>,
    /// A configured model lane chosen by host policy for this app's peer.
    pub model_lane: Option<String>,
    pub settings_entry: SettingsEntry,
    /// How long one turn may run.
    pub turn_timeout: Duration,
    /// Where the host keeps each peer's host token (one file per app and
    /// account, mode 0600): the kernel's credential for controlling the peer
    /// it created. `None` keeps tokens in memory, so a peer created by this
    /// process cannot be resumed after a restart.
    pub state_dir: Option<std::path::PathBuf>,
}

impl BrokerConfig {
    /// A config with the defaults a shell uses; set the rest by field.
    pub fn new(
        deployment: Deployment,
        profile_id: impl Into<String>,
        originator: impl Into<String>,
        app_id: impl Into<String>,
        app_label: impl Into<String>,
        services: BTreeSet<String>,
    ) -> Self {
        let app_label = app_label.into();
        Self {
            deployment,
            profile_id: profile_id.into(),
            originator: originator.into(),
            app_id: app_id.into(),
            brief: format!(
                "You are the {app_label} app's assistant. You work for the {app_label} app \
                 inside its own workspace and memory, and you answer to the system agent that \
                 owns you."
            ),
            app_label,
            services,
            model_lane: None,
            settings_entry: match deployment {
                Deployment::Hosted => SettingsEntry::Host,
                Deployment::StandaloneLocal => SettingsEntry::AppLocal,
                Deployment::StandaloneRemote => SettingsEntry::AppRemote,
            },
            turn_timeout: Duration::from_secs(180),
            state_dir: None,
        }
    }
}

/// A stable, non-secret account tag for names and namespaces.
pub fn account_tag(account: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in account.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The memory namespace of `app_id` for `account`.
pub fn app_namespace(app_id: &str, account: &str) -> String {
    format!("app/{app_id}/acct-{}", account_tag(account))
}

/// A kernel context id from a client instance key: `[a-z0-9-]`, at most 64.
fn context_id(nonce: &str, instance: &str) -> String {
    let mut id = String::new();
    for ch in instance.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            id.push(ch);
        } else if !id.ends_with('-') {
            id.push('-');
        }
    }
    let id = id.trim_matches('-');
    let budget = 64 - nonce.len() - 1;
    let id: String = id.chars().take(budget).collect();
    let id = id.trim_end_matches('-');
    if id.is_empty() {
        nonce.to_owned()
    } else {
        format!("{nonce}-{id}")
    }
}

// --------------------------------------------------------------------------

type Reply = oneshot::Sender<Result<Value, String>>;

struct TurnWaiter {
    turn_id: String,
    text: String,
    /// The assistant segment `text` belongs to (v2 envelopes).
    segment: String,
    /// Segments whose saved text arrived: later deltas for them are stale.
    persisted: BTreeSet<String>,
    done: Option<oneshot::Sender<Result<String, String>>>,
}

impl TurnWaiter {
    fn new(turn_id: String, done: oneshot::Sender<Result<String, String>>) -> Self {
        Self {
            turn_id,
            text: String::new(),
            segment: String::new(),
            persisted: BTreeSet::new(),
            done: Some(done),
        }
    }

    fn finish(&mut self, result: Result<String, String>) {
        if let Some(done) = self.done.take() {
            let _ = done.send(result);
        }
    }

    /// Apply one v2 envelope payload; `true` when the text changed.
    fn envelope(&mut self, payload: &Value) -> bool {
        let data = &payload["data"];
        match payload["type"].as_str().unwrap_or("") {
            kind @ ("assistant_delta" | "assistant_persisted") => {
                let segment = data["assistant_segment_id"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned();
                // The saved answer can overtake the last live deltas of the
                // same segment; they are already in it.
                if kind == "assistant_delta" && self.persisted.contains(&segment) {
                    return false;
                }
                if self.segment != segment {
                    self.text.clear();
                    self.segment = segment.clone();
                }
                let text = data["text"].as_str().unwrap_or("");
                if kind == "assistant_persisted" {
                    self.persisted.insert(segment);
                    self.text = text.to_owned();
                } else {
                    self.text.push_str(text);
                }
                true
            }
            "turn_terminal" => {
                let text = self.text.clone();
                if data["outcome"] == "completed" {
                    self.finish(Ok(text));
                } else {
                    let message = data["error"]["message"]
                        .as_str()
                        .unwrap_or("The assistant's turn did not complete")
                        .to_owned();
                    self.finish(Err(message));
                }
                false
            }
            _ => false,
        }
    }
}

struct Route {
    generation: u64,
    context: Weak<ContextInner>,
}

#[derive(Clone)]
struct PeerInfo {
    slug: String,
    session: String,
    /// The host token that controls this peer (UPCR-2026-034).
    token: Option<String>,
}

/// `session/open` params for a bound session: the kernel's own workspace
/// for it, so a scoped session resumes after a kernel restart.
fn open_params(session: &str, profile: &str, cwd: Option<&str>) -> Value {
    let mut params = json!({"session_id": session, "profile_id": profile});
    if let Some(cwd) = cwd {
        params["cwd"] = json!(cwd);
    }
    params
}

struct State {
    account: Option<String>,
    generation: u64,
    released: bool,
    link: Option<mpsc::UnboundedSender<String>>,
    link_epoch: u64,
    next_id: u64,
    pending: HashMap<String, Reply>,
    routes: HashMap<String, Route>,
    peer: Option<(u64, PeerInfo)>,
    peer_turn: Option<String>,
    /// Host tokens by memory namespace, when no state dir persists them.
    tokens: HashMap<String, String>,
    contexts: Vec<Weak<ContextInner>>,
    model: Option<ModelInfo>,
    last_error: Option<String>,
}

struct Inner {
    cfg: BrokerConfig,
    connector: Arc<dyn Connector>,
    runtime: Option<tokio::runtime::Runtime>,
    state: Mutex<State>,
    connecting: tokio::sync::Mutex<()>,
    binding: tokio::sync::Mutex<()>,
    nonce: String,
}

/// One app's scoped assistant service. Cheap to clone.
#[derive(Clone)]
pub struct Broker(Arc<Inner>);

impl Broker {
    pub fn new(cfg: BrokerConfig, connector: Arc<dyn Connector>) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("app-peers")
            .enable_all()
            .build()
            .expect("app-peers: tokio runtime");
        let nonce = uuid::Uuid::new_v4().simple().to_string()[..8].to_owned();
        Broker(Arc::new(Inner {
            cfg,
            connector,
            runtime: Some(runtime),
            state: Mutex::new(State {
                account: None,
                generation: 1,
                released: false,
                link: None,
                link_epoch: 0,
                next_id: 0,
                pending: HashMap::new(),
                routes: HashMap::new(),
                peer: None,
                peer_turn: None,
                tokens: HashMap::new(),
                contexts: Vec::new(),
                model: None,
                last_error: None,
            }),
            connecting: tokio::sync::Mutex::new(()),
            binding: tokio::sync::Mutex::new(()),
            nonce,
        }))
    }

    /// The config this broker serves.
    pub fn config(&self) -> &BrokerConfig {
        &self.0.cfg
    }

    /// The current account generation (bumped by every account change).
    pub fn generation(&self) -> u64 {
        self.0.lock().generation
    }

    /// The bound peer's slug and session, once bound.
    pub fn peer(&self) -> Option<(String, String)> {
        self.0
            .lock()
            .peer
            .as_ref()
            .map(|(_, p)| (p.slug.clone(), p.session.clone()))
    }

    /// The peer's running turn (driven by its owner), as this broker saw it
    /// start. `release` interrupts it.
    pub fn peer_active_turn(&self) -> Option<String> {
        self.0.lock().peer_turn.clone()
    }

    /// Bind (create or resume) the app's peer for the current account now,
    /// without a model inference. A host calls this at launch; otherwise the
    /// first request does it.
    pub fn bind(&self) -> Result<(), String> {
        let inner = self.0.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.0.rt().spawn(async move {
            let result = inner.ensure_peer().await.map(|_| ());
            let _ = tx.send(result);
        });
        rx.recv_timeout(Duration::from_secs(60))
            .map_err(|_| "binding the app peer timed out".to_owned())?
    }

    /// Send a raw request on this broker's link. For the HOST only (its own
    /// system-agent operations and tests); never handed to an app.
    pub fn host_request(&self, method: &str, params: Value) -> Result<Value, String> {
        let inner = self.0.clone();
        let method = method.to_owned();
        let (tx, rx) = std::sync::mpsc::channel();
        self.0.rt().spawn(async move {
            let _ = tx.send(inner.request(&method, params).await);
        });
        rx.recv_timeout(Duration::from_secs(60))
            .map_err(|_| "host request timed out".to_owned())?
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // A broker may be dropped inside another runtime (a test's, a
        // shell's): never block there.
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl Inner {
    fn rt(&self) -> &tokio::runtime::Runtime {
        self.runtime
            .as_ref()
            .expect("app-peers: runtime present until drop")
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn token_path(&self, namespace: &str) -> Option<std::path::PathBuf> {
        let dir = self.cfg.state_dir.as_ref()?;
        Some(dir.join(format!("{}.token", namespace.replace('/', "_"))))
    }

    fn load_token(&self, namespace: &str) -> Option<String> {
        if let Some(token) = self.lock().tokens.get(namespace) {
            return Some(token.clone());
        }
        let text = std::fs::read_to_string(self.token_path(namespace)?).ok()?;
        let token = text.trim().to_owned();
        (!token.is_empty()).then_some(token)
    }

    fn save_token(&self, namespace: &str, token: &str) -> Result<(), String> {
        self.lock()
            .tokens
            .insert(namespace.to_owned(), token.to_owned());
        let Some(path) = self.token_path(namespace) else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("token.tmp");
        std::fs::write(&tmp, token).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }

    fn fail(&self, error: &str) {
        self.lock().last_error = Some(error.to_owned());
    }

    async fn ensure_link(self: &Arc<Self>) -> Result<mpsc::UnboundedSender<String>, String> {
        if let Some(link) = self.lock().link.clone() {
            return Ok(link);
        }
        let _guard = self.connecting.lock().await;
        if let Some(link) = self.lock().link.clone() {
            return Ok(link);
        }
        self.connector.available()?;
        let mut link = match self.connector.connect().await {
            Ok(link) => link,
            Err(err) => {
                self.fail(&err);
                return Err(err);
            }
        };
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let epoch = {
            let mut st = self.lock();
            st.link_epoch += 1;
            st.link = Some(tx.clone());
            st.last_error = None;
            st.link_epoch
        };
        let weak = Arc::downgrade(self);
        self.rt().spawn(async move {
            let why = loop {
                tokio::select! {
                    outbound = rx.recv() => match outbound {
                        Some(frame) => {
                            if let Err(err) = link.send(frame) {
                                break err;
                            }
                        }
                        None => break "link released".to_owned(),
                    },
                    inbound = link.recv() => match inbound {
                        Ok(frame) => match weak.upgrade() {
                            Some(inner) => inner.inbound(&frame),
                            None => break "broker dropped".to_owned(),
                        },
                        Err(err) => break err,
                    },
                }
            };
            if let Some(inner) = weak.upgrade() {
                inner.link_closed(epoch, &why);
            }
        });
        Ok(tx)
    }

    /// The link ended: fail everything waiting on it; sessions and the peer
    /// are opened again on the next request.
    fn link_closed(&self, epoch: u64, why: &str) {
        let (pending, waiters) = {
            let mut st = self.lock();
            if st.link_epoch != epoch {
                return;
            }
            st.link = None;
            st.peer = None;
            st.peer_turn = None;
            st.last_error = Some(why.to_owned());
            let pending: Vec<Reply> = st.pending.drain().map(|(_, r)| r).collect();
            let contexts: Vec<Arc<ContextInner>> =
                st.contexts.iter().filter_map(Weak::upgrade).collect();
            (pending, contexts)
        };
        let message = format!("The assistant connection ended ({why}); try again");
        for reply in pending {
            let _ = reply.send(Err(message.clone()));
        }
        for context in waiters {
            context
                .bound
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            if let Some(waiter) = context
                .turn
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
            {
                if let Some(done) = waiter.done {
                    let _ = done.send(Err(message.clone()));
                }
            }
        }
    }

    fn inbound(&self, frame: &str) {
        if std::env::var_os("APP_PEERS_TRACE").is_some() {
            eprintln!("app-peers <- {}", frame);
        }
        let Ok(value) = serde_json::from_str::<Value>(frame) else {
            return;
        };
        let has_result = value.get("result").is_some() || value.get("error").is_some();
        if let (Some(id), true) = (value.get("id"), has_result) {
            let key = id
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| id.to_string());
            let reply = self.lock().pending.remove(&key);
            if let Some(reply) = reply {
                let result = match value.get("error") {
                    Some(error) if !error.is_null() => Err(rpc_error_text(error)),
                    _ => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = reply.send(result);
            }
            return;
        }
        let Some(method) = value.get("method").and_then(Value::as_str) else {
            return;
        };
        let params = value.get("params").cloned().unwrap_or(Value::Null);
        let Some(session_id) = params.get("session_id").and_then(Value::as_str) else {
            return;
        };
        // v2 projection envelopes name the BASE session and carry the topic
        // separately.
        let session = match params.get("topic").and_then(Value::as_str) {
            Some(topic) if !session_id.contains('#') => format!("{session_id}#{topic}"),
            _ => session_id.to_owned(),
        };
        let session = session.as_str();
        let (route, peer_session, generation) = {
            let st = self.lock();
            (
                st.routes
                    .get(session)
                    .map(|r| (r.generation, r.context.clone())),
                st.peer.as_ref().map(|(_, p)| p.session.clone()),
                st.generation,
            )
        };
        if peer_session.as_deref() == Some(session) {
            // Track the peer's own (system-agent driven) turn so a release
            // can stop it.
            let turn = params
                .get("turn_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let terminal =
                method == "projection/envelope" && params["payload"]["type"] == "turn_terminal";
            let mut st = self.lock();
            let ended = terminal || matches!(method, "turn/completed" | "turn/error");
            if method == "turn/started" {
                st.peer_turn = turn;
            } else if ended && st.peer_turn == turn {
                st.peer_turn = None;
            }
            return;
        }
        let Some((route_generation, context)) = route else {
            return;
        };
        // Stale-reply dropping: a context of an older account generation
        // never receives anything again.
        if route_generation != generation {
            return;
        }
        if let Some(context) = context.upgrade() {
            context.notification(method, &params);
        }
    }

    async fn request(self: &Arc<Self>, method: &str, params: Value) -> Result<Value, String> {
        let link = self.ensure_link().await?;
        let (tx, rx) = oneshot::channel();
        let id = {
            let mut st = self.lock();
            st.next_id += 1;
            let id = format!("app-peers-{}", st.next_id);
            st.pending.insert(id.clone(), tx);
            id
        };
        let frame = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if std::env::var_os("APP_PEERS_TRACE").is_some() {
            eprintln!("app-peers -> {frame}");
        }
        if link.send(frame.to_string()).is_err() {
            self.lock().pending.remove(&id);
            return Err("The assistant connection ended; try again".into());
        }
        match tokio::time::timeout(Duration::from_secs(60), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("The assistant connection ended; try again".into()),
            Err(_) => {
                self.lock().pending.remove(&id);
                Err(format!("{method} timed out"))
            }
        }
    }

    /// Create or resume the app's peer for the current account.
    async fn ensure_peer(self: &Arc<Self>) -> Result<(u64, PeerInfo), String> {
        let _guard = self.binding.lock().await;
        let (account, generation) = {
            let st = self.lock();
            if st.released {
                return Err("The app was closed".into());
            }
            if let Some((generation, peer)) = &st.peer {
                if *generation == st.generation {
                    return Ok((*generation, peer.clone()));
                }
            }
            (st.account.clone(), st.generation)
        };
        if self.cfg.services.is_empty() {
            return Err("This app has no assistant access".into());
        }
        let account = account.ok_or("Sign in before using the assistant")?;
        let tag = account_tag(&account);
        let namespace = app_namespace(&self.cfg.app_id, &account);
        let name = format!("{} {}", self.cfg.app_label, &tag[..8]);
        // The owner session is live before its peer exists, so the kernel
        // can wake it when the peer asks a question.
        self.request(
            "session/open",
            json!({"session_id": self.cfg.originator, "profile_id": self.cfg.profile_id}),
        )
        .await?;
        let mut params = json!({
            "profile_id": self.cfg.profile_id,
            "session_id": self.cfg.originator,
            "names": [name],
            "brief": self.cfg.brief,
            "memory_namespace": namespace,
            "resume": true,
        });
        let known_token = self.load_token(&namespace);
        if let Some(token) = &known_token {
            params["host_token"] = json!(token);
        }
        if let Some(lane) = &self.cfg.model_lane {
            params["model"] = json!(lane);
        }
        let result = match self.request("peer/prepare", params).await {
            Ok(result) => result,
            Err(err) => {
                self.fail(&err);
                return Err(err);
            }
        };
        // A kernel that ignored the host binding staged an ORDINARY peer:
        // refuse it rather than run the app with the profile's memory.
        if result.get("memory_namespace").and_then(Value::as_str) != Some(namespace.as_str()) {
            let err = "This assistant kernel does not support host-owned app peers \
                       (octos UPCR-2026-034); update it"
                .to_owned();
            self.fail(&err);
            return Err(err);
        }
        let slug = result["slug"]
            .as_str()
            .ok_or("peer/prepare returned no slug")?
            .to_owned();
        // A new peer's credential arrives once; keep it before anything else.
        let token = match result["host_token"].as_str() {
            Some(token) => {
                if let Err(err) = self.save_token(&namespace, token) {
                    let err = format!("could not keep the assistant's peer credential: {err}");
                    self.fail(&err);
                    return Err(err);
                }
                Some(token.to_owned())
            }
            None => known_token,
        };
        let session = format!(
            "{}#peer-{slug}",
            self.cfg
                .originator
                .split('#')
                .next()
                .unwrap_or(&self.cfg.originator)
        );
        let model = model_info(&result["model"]);
        // Open the peer session so the system agent can address it.
        self.request(
            "session/open",
            open_params(&session, &self.cfg.profile_id, result["cwd"].as_str()),
        )
        .await?;
        let peer = PeerInfo {
            slug,
            session: session.clone(),
            token,
        };
        let mut st = self.lock();
        if st.generation != generation || st.released {
            return Err("The account changed; try again".into());
        }
        st.peer = Some((generation, peer.clone()));
        st.model = model;
        Ok((generation, peer))
    }

    fn close_context_on_kernel(
        self: &Arc<Self>,
        peer: String,
        context_id: String,
        turn: Option<String>,
        session: String,
        token: Option<String>,
    ) {
        let inner = self.clone();
        self.rt().spawn(async move {
            if let Some(turn) = turn {
                let _ = inner
                    .request(
                        "turn/interrupt",
                        json!({"session_id": session, "turn_id": turn}),
                    )
                    .await;
            }
            let _ = inner
                .request(
                    "peer/context/close",
                    json!({
                        "profile_id": inner.cfg.profile_id,
                        "session_id": inner.cfg.originator,
                        "peer": peer,
                        "context_id": context_id,
                        "host_token": token,
                    }),
                )
                .await;
        });
    }

    /// Revoke every context (account change, release): they refuse calls,
    /// drop late events and are closed on the kernel.
    fn revoke_contexts(self: &Arc<Self>) {
        let contexts: Vec<Arc<ContextInner>> = {
            let mut st = self.lock();
            let contexts = st.contexts.drain(..).filter_map(|c| c.upgrade()).collect();
            st.routes.clear();
            contexts
        };
        for context in contexts {
            context.revoke(self);
        }
    }
}

fn rpc_error_text(error: &Value) -> String {
    let message = error["message"].as_str().unwrap_or("request failed");
    match error["data"]["kind"].as_str() {
        Some(kind) => format!("{message} ({kind})"),
        None => message.to_owned(),
    }
}

fn model_info(value: &Value) -> Option<ModelInfo> {
    let lane = value.get("lane")?.as_str()?.to_owned();
    Some(ModelInfo {
        lane,
        provider: value
            .get("provider")
            .and_then(Value::as_str)
            .map(str::to_owned),
        model: value
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

// --------------------------------------------------------------------------

struct Bound {
    session: String,
    peer_slug: String,
    context_id: String,
    token: Option<String>,
}

struct ContextInner {
    broker: Weak<Inner>,
    account: String,
    generation: u64,
    services: BTreeSet<String>,
    context_id: String,
    open: AtomicBool,
    bound: Mutex<Option<Bound>>,
    turn: Mutex<Option<TurnWaiter>>,
    sink: Mutex<Option<EventSink>>,
    calls: AtomicU64,
    /// (thread, seq) of v2 envelopes already applied: the kernel may deliver
    /// one twice (replay, shared consumers).
    seen: Mutex<BTreeSet<(String, u64)>>,
}

impl ContextInner {
    /// The lease check, before a request and again before any delivery.
    fn check(&self, inner: &Inner, service: Option<&str>) -> Result<(), String> {
        if !self.open.load(Ordering::Acquire) {
            return Err("This app's assistant access was closed".into());
        }
        let st = inner.lock();
        if st.released {
            return Err("The app was closed".into());
        }
        if st.generation != self.generation || st.account.as_deref() != Some(self.account.as_str())
        {
            return Err("The account changed; reopen this app".into());
        }
        if let Some(service) = service {
            if !inner.cfg.services.contains(service) || !self.services.contains(service) {
                return Err(format!("This app was not granted {service}"));
            }
        }
        Ok(())
    }

    fn notification(&self, method: &str, params: &Value) {
        let Some(inner) = self.broker.upgrade() else {
            return;
        };
        if self.check(&inner, None).is_err() {
            return;
        }
        if method == "projection/envelope" {
            if let (Some(thread), Some(seq)) =
                (params["thread_id"].as_str(), params["seq"].as_u64())
            {
                let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
                if !seen.insert((thread.to_owned(), seq)) {
                    return;
                }
                if seen.len() > 4096 {
                    seen.clear();
                }
            }
        }
        let turn_id = params.get("turn_id").and_then(Value::as_str);
        let mut text_so_far = None;
        {
            let mut turn = self.turn.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(waiter) = turn
                .as_mut()
                .filter(|w| Some(w.turn_id.as_str()) == turn_id)
            {
                match method {
                    "projection/envelope" => {
                        if waiter.envelope(&params["payload"]) {
                            text_so_far = Some(waiter.text.clone());
                        }
                    }
                    "message/delta" => {
                        waiter.text.push_str(params["text"].as_str().unwrap_or(""));
                        text_so_far = Some(waiter.text.clone());
                    }
                    "turn/completed" => {
                        let text = waiter.text.clone();
                        waiter.finish(Ok(text));
                    }
                    "turn/error" => {
                        let message = params["message"]
                            .as_str()
                            .or_else(|| params["code"].as_str())
                            .unwrap_or("The assistant's turn failed")
                            .to_owned();
                        waiter.finish(Err(message));
                    }
                    _ => {}
                }
            }
        }
        let sink = self.sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(sink) = sink {
            let mut data = json!({"method": method, "params": params});
            if let Some(text) = text_so_far {
                data["text"] = json!(text);
            }
            sink(ContextEvent::Data(data));
        }
    }

    fn revoke(&self, inner: &Arc<Inner>) {
        if !self.open.swap(false, Ordering::AcqRel) {
            return;
        }
        let turn = self
            .turn
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .map(|w| w.turn_id);
        self.sink.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(bound) = self.bound.lock().unwrap_or_else(|e| e.into_inner()).take() {
            inner.lock().routes.remove(&bound.session);
            inner.close_context_on_kernel(
                bound.peer_slug,
                bound.context_id,
                turn,
                bound.session,
                bound.token,
            );
        }
    }

    async fn ensure_bound(self: &Arc<Self>, inner: &Arc<Inner>) -> Result<String, String> {
        if let Some(bound) = self
            .bound
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            return Ok(bound.session.clone());
        }
        let (generation, peer) = inner.ensure_peer().await?;
        if generation != self.generation {
            return Err("The account changed; reopen this app".into());
        }
        let result = inner
            .request(
                "peer/context/open",
                json!({
                    "profile_id": inner.cfg.profile_id,
                    "session_id": inner.cfg.originator,
                    "peer": peer.slug,
                    "context_id": self.context_id,
                    "host_token": peer.token,
                }),
            )
            .await?;
        let session = result["session_id"]
            .as_str()
            .ok_or("peer/context/open returned no session")?
            .to_owned();
        inner
            .request(
                "session/open",
                open_params(&session, &inner.cfg.profile_id, result["cwd"].as_str()),
            )
            .await?;
        self.check(inner, None)?;
        inner.lock().routes.insert(
            session.clone(),
            Route {
                generation: self.generation,
                context: Arc::downgrade(self),
            },
        );
        *self.bound.lock().unwrap_or_else(|e| e.into_inner()) = Some(Bound {
            session: session.clone(),
            peer_slug: peer.slug,
            context_id: self.context_id.clone(),
            token: peer.token,
        });
        Ok(session)
    }

    async fn run(self: &Arc<Self>, inner: &Arc<Inner>, op: ContextOp) -> Result<Value, String> {
        let session = self.ensure_bound(inner).await?;
        self.check(inner, Some(op.service()))?;
        let result = match op {
            ContextOp::Open => Ok(json!({
                "open": true,
                "model": inner.lock().model.as_ref().map(|m| json!({
                    "lane": m.lane, "provider": m.provider, "model": m.model,
                })),
            })),
            ContextOp::History => {
                inner
                    .request(
                        "session/hydrate",
                        json!({"session_id": session, "include": ["messages"]}),
                    )
                    .await
            }
            ContextOp::Turn { text } => {
                let text = text.trim().to_owned();
                if text.is_empty() || text.len() > 32 * 1024 {
                    return Err("Provide text (at most 32 KiB)".into());
                }
                let turn_id = uuid::Uuid::new_v4().to_string();
                let (done_tx, done_rx) = oneshot::channel();
                {
                    let mut turn = self.turn.lock().unwrap_or_else(|e| e.into_inner());
                    if turn.is_some() {
                        return Err("This app already has an assistant turn running".into());
                    }
                    *turn = Some(TurnWaiter::new(turn_id.clone(), done_tx));
                }
                let started = inner
                    .request(
                        "turn/start",
                        json!({
                            "session_id": session,
                            "turn_id": turn_id,
                            "input": [{"kind": "text", "text": text}],
                        }),
                    )
                    .await;
                let outcome = match started {
                    Err(err) => Err(err),
                    Ok(_) => match tokio::time::timeout(inner.cfg.turn_timeout, done_rx).await {
                        Ok(Ok(result)) => result,
                        Ok(Err(_)) => Err("The assistant's turn was cancelled".into()),
                        Err(_) => {
                            let _ = inner
                                .request(
                                    "turn/interrupt",
                                    json!({"session_id": session, "turn_id": turn_id}),
                                )
                                .await;
                            Err("The assistant's turn timed out".into())
                        }
                    },
                };
                {
                    let mut turn = self.turn.lock().unwrap_or_else(|e| e.into_inner());
                    if turn.as_ref().is_some_and(|w| w.turn_id == turn_id) {
                        turn.take();
                    }
                }
                let streamed = outcome?;
                // The persisted answer is authoritative; the terminal can
                // overtake the transcript lane, so look for THIS turn's
                // answer (its thread) a few times before using the stream.
                let mut saved = None;
                for attempt in 0..10 {
                    let history = inner
                        .request(
                            "session/hydrate",
                            json!({"session_id": session, "include": ["messages"]}),
                        )
                        .await
                        .unwrap_or(Value::Null);
                    saved = history["messages"]
                        .as_array()
                        .and_then(|rows| {
                            rows.iter().rev().find(|m| {
                                m["role"] == "assistant"
                                    && (m["turn_id"] == turn_id.as_str()
                                        || m["thread_id"] == turn_id.as_str())
                            })
                        })
                        .and_then(|m| m["content"].as_str())
                        .map(str::to_owned);
                    if saved.is_some() || attempt == 9 {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                Ok(json!({"turn_id": turn_id, "text": saved.unwrap_or(streamed)}))
            }
            ContextOp::Interrupt => {
                let turn = self
                    .turn
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .map(|w| w.turn_id.clone())
                    .ok_or("No assistant turn is running")?;
                inner
                    .request(
                        "turn/interrupt",
                        json!({"session_id": session, "turn_id": turn}),
                    )
                    .await
            }
            ContextOp::Approval { id, approve } => {
                inner
                    .request(
                        "approval/respond",
                        json!({
                            "session_id": session,
                            "approval_id": id,
                            "decision": if approve { "approve" } else { "deny" },
                        }),
                    )
                    .await
            }
        };
        // Drop a reply that outlived its lease.
        self.check(inner, None)?;
        result
    }
}

/// A request context handed to the app.
pub struct BrokerContext(Arc<ContextInner>);

impl OctosContext for BrokerContext {
    fn call(&self, op: ContextOp, sink: EventSink) -> Result<(), String> {
        let inner = self
            .0
            .broker
            .upgrade()
            .ok_or("The assistant service is gone")?;
        self.0.check(&inner, Some(op.service()))?;
        if matches!(op, ContextOp::Turn { .. })
            && self
                .0
                .turn
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
        {
            return Err("This app already has an assistant turn running".into());
        }
        *self.0.sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(sink.clone());
        self.0.calls.fetch_add(1, Ordering::Relaxed);
        let context = self.0.clone();
        inner.rt().spawn({
            let inner = inner.clone();
            async move {
                let result = context.run(&inner, op).await;
                // Stale replies are dropped, never delivered to a new
                // account or instance.
                if context.check(&inner, None).is_ok() {
                    sink(ContextEvent::Complete(result));
                }
            }
        });
        Ok(())
    }

    fn close(&self) {
        if let Some(inner) = self.0.broker.upgrade() {
            self.0.revoke(&inner);
        } else {
            self.0.open.store(false, Ordering::Release);
        }
    }

    fn is_open(&self) -> bool {
        self.0.open.load(Ordering::Acquire)
    }
}

impl OctosAppService for Broker {
    fn deployment(&self) -> Deployment {
        self.0.cfg.deployment
    }

    fn availability(&self) -> Availability {
        if self.0.cfg.services.is_empty() {
            return Availability::Unavailable("This app has no assistant access".into());
        }
        if let Err(reason) = self.0.connector.available() {
            return Availability::Unavailable(reason);
        }
        let st = self.0.lock();
        if st.released {
            return Availability::Unavailable("The app was closed".into());
        }
        if st.account.is_none() {
            return Availability::Unavailable("Sign in to use the assistant".into());
        }
        if st.peer.as_ref().is_some_and(|(g, _)| *g == st.generation) && st.link.is_some() {
            return Availability::Ready;
        }
        match &st.last_error {
            Some(err) => Availability::Failed(err.clone()),
            None => Availability::Idle,
        }
    }

    fn services(&self) -> BTreeSet<String> {
        self.0.cfg.services.clone()
    }

    fn model(&self) -> Option<ModelInfo> {
        self.0.lock().model.clone()
    }

    fn settings_entry(&self) -> SettingsEntry {
        self.0.cfg.settings_entry
    }

    fn set_account(&self, account: Option<&str>) {
        let changed = {
            let mut st = self.0.lock();
            if st.account.as_deref() == account {
                false
            } else {
                st.account = account.map(str::to_owned);
                st.generation += 1;
                st.peer = None;
                st.model = None;
                true
            }
        };
        if changed {
            self.0.revoke_contexts();
            // Create or resume the new account's peer now (no inference), so
            // the system agent can address it from launch on.
            if account.is_some()
                && !self.0.cfg.services.is_empty()
                && self.0.connector.available().is_ok()
            {
                let inner = self.0.clone();
                self.0.rt().spawn(async move {
                    let _ = inner.ensure_peer().await;
                });
            }
        }
    }

    fn open_context(&self, spec: ContextSpec) -> Result<Arc<dyn OctosContext>, String> {
        let st = self.0.lock();
        if st.released {
            return Err("The app was closed".into());
        }
        if st.account.as_deref() != Some(spec.account.as_str()) {
            return Err("The account changed; reopen this app".into());
        }
        let services: BTreeSet<String> = spec
            .services
            .intersection(&self.0.cfg.services)
            .cloned()
            .collect();
        if services.is_empty() {
            return Err("This app was not granted the assistant".into());
        }
        let context = Arc::new(ContextInner {
            broker: Arc::downgrade(&self.0),
            account: spec.account,
            generation: st.generation,
            services,
            context_id: context_id(&self.0.nonce, &spec.instance),
            open: AtomicBool::new(true),
            bound: Mutex::new(None),
            turn: Mutex::new(None),
            sink: Mutex::new(None),
            calls: AtomicU64::new(0),
            seen: Mutex::new(BTreeSet::new()),
        });
        drop(st);
        let mut st = self.0.lock();
        st.contexts.retain(|c| c.strong_count() > 0);
        st.contexts.push(Arc::downgrade(&context));
        Ok(Arc::new(BrokerContext(context)))
    }

    fn release(&self) {
        let peer_turn = {
            let mut st = self.0.lock();
            if st.released {
                return;
            }
            st.released = true;
            let turn = st.peer_turn.take();
            turn.zip(st.peer.as_ref().map(|(_, p)| p.session.clone()))
        };
        self.0.revoke_contexts();
        // Conservative background policy (ADR 0007 open question): closing
        // the app stops its peer's running turn too. The peer and its state
        // stay for the next launch; nothing else is stopped.
        let inner = self.0.clone();
        self.0.rt().spawn(async move {
            if let Some((turn, session)) = peer_turn {
                let _ = inner
                    .request(
                        "turn/interrupt",
                        json!({"session_id": session, "turn_id": turn}),
                    )
                    .await;
            }
            // Let the context closes above reach the kernel, then let go of
            // the link: the shared kernel keeps serving other consumers.
            tokio::time::sleep(Duration::from_millis(500)).await;
            inner.lock().link.take();
        });
    }

    fn shutdown(&self) {
        self.release();
        if self.0.connector.owns_runtime() {
            // Give the release a moment to reach the kernel first.
            std::thread::sleep(Duration::from_millis(600));
            self.0.connector.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_ids_are_kernel_safe_and_nonce_scoped() {
        assert_eq!(
            context_id("ab12cd34", "dev.example.app#7"),
            "ab12cd34-dev-example-app-7"
        );
        assert_eq!(context_id("ab12cd34", "///"), "ab12cd34");
        let long = context_id("ab12cd34", &"x".repeat(200));
        assert!(long.len() <= 64, "{}", long.len());
        assert!(long
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));
    }

    #[test]
    fn a_saved_answer_wins_over_late_deltas_of_its_segment() {
        let (tx, mut rx) = oneshot::channel();
        let mut waiter = TurnWaiter::new("t".into(), tx);
        let env = |kind: &str, segment: &str, text: &str| json!({"type": kind, "data": {"assistant_segment_id": segment, "text": text}});
        waiter.envelope(&env("assistant_delta", "a", "Hel"));
        waiter.envelope(&env("assistant_persisted", "a", "Hello there"));
        assert!(!waiter.envelope(&env("assistant_delta", "a", "lo there")));
        assert_eq!(waiter.text, "Hello there");
        waiter.envelope(&env("assistant_delta", "b", "Next"));
        assert_eq!(waiter.text, "Next");
        waiter.envelope(&json!({"type": "turn_terminal", "data": {"outcome": "completed"}}));
        assert_eq!(rx.try_recv().unwrap().unwrap(), "Next");
    }

    #[test]
    fn namespaces_scope_app_and_account_without_revealing_the_account() {
        let a = app_namespace("rinx", "@alice:example.org");
        let b = app_namespace("rinx", "@bob:example.org");
        assert_ne!(a, b);
        assert!(a.starts_with("app/rinx/acct-"));
        assert!(!a.contains("alice"));
        assert_eq!(a, app_namespace("rinx", "@alice:example.org"));
    }
}
