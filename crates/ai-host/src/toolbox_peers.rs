//! The system toolbox for app agents (ADR 0002 section 6, ADR 0004 section
//! 12; octos UPCR-2026-035 / octos#2567). Feature `toolbox-peers`.
//!
//! The toolbox is one more owner of host-routed tools in the shell's
//! host-tool relay (`crates/shell/src/host_tools`, octos#2567's shell side).
//! The broker registers and the relay routes; this module adds only what is
//! the toolbox's:
//!
//! - **What an app is offered** ([`ToolboxGrant`]): exactly what it declares
//!   AND the person granted. `research` gives `workflow.run` (read),
//!   `workflow.fork` (act), `toolbox.search` (read) and `toolbox.web_read`
//!   (read); `crawl`, with `max_depth`/`max_pages` above 0 in its scope,
//!   gives `toolbox.deep_crawl` (read). Each is marked with its owning app,
//!   `toolbox` ([`octosense_toolbox::peer::OWNER`]); the relay declares the
//!   toolbox's [`catalog`] once and grants each app its [`ToolboxGrant::tools`].
//!   Octos's generic tools stay the kernel's. The broker registers the
//!   host's explicit `generic_tools` selection separately; toolbox access
//!   never grants arbitrary kernel tools.
//! - **How a call runs** ([`ToolboxExecutor`], the relay's executor for the
//!   `toolbox` owner): the toolbox with the calling app's [`AppContext`] (id,
//!   grants, octos `Scope`, and its host-owned folder), over the octos
//!   research engine (`OctosResearch`), with the templates' model calls going
//!   through the `model` service's host ([`ModelHostClient`] over
//!   `ModelHost::complete`), so they use the person's providers and count
//!   against the app's daily budget in the same ledger as `model.complete`.
//!   The executor checks the grant again: a call the app was not granted is
//!   `not_granted`, whatever reached it.
//!
//! Consent (the #120 first-use sheet) is the relay's: it offers no toolbox
//! tool to an app, and runs none for it, before the person allowed that
//! app's agent.
//!
//! **Results** are written under [`crate::toolbox_folder`]
//! (`<apps root>/.host/toolbox/<app>`), outside the app's jail
//! (`<apps root>/<app>`), where the glance screen's `sys.digest` reads them
//! (OctoSense #87): an app cannot forge a digest.
//!
//! **Where grants come from.** A native module declares `research`/`crawl`
//! among its capabilities, compiled in and reviewed with the shell
//! ([`ToolboxGrant::for_module`]; no manifest scope, so octos's defaults and
//! no crawl limits). A script app declares them in its manifest's
//! `capabilities`, with its scope in the manifest's top-level [`SCOPE_KEY`]
//! object, App Hub #26's shape ([`ToolboxGrant::for_manifest`]).
//! The shell supplies an admitted, digest-checked manifest. Store apps and
//! system apps follow the same toolbox policy. Research/crawl declarations
//! select the shared toolbox tools offered to the agent; the relay still
//! requires consent, actual inter-app grants and the declared resource scope.

use octosense_app_peers::host_tools::{HostToolCall, ToolExecutor, ToolOutcome, ToolReply};
use octosense_llm_service::complete::{self, Class, Code, ModelHost};
use octosense_toolbox::host::{CallContext, HostError, HostFuture};
use octosense_toolbox::peer::{self, PeerToolbox, CRAWL, RESEARCH};
use octosense_toolbox::research::octos::{OctosConfig, OctosResearch};
use octosense_toolbox::research::{ModelClient, ModelRequest};
use octosense_toolbox::{scope, AppContext, Library, Scope};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, Notify};

pub use octosense_toolbox::peer::OWNER;

/// The manifest object that holds the `research`/`crawl` scope (octos's
/// `Scope` fields): App Hub #26's shape (`capabilities: ["research",
/// "crawl"]` plus one top-level `research` object).
pub const SCOPE_KEY: &str = "research";

/// Every toolbox tool, as the relay's catalog declares it (owned by
/// [`OWNER`], `shareable`). Empty when the template library cannot load.
pub fn catalog() -> Vec<Value> {
    Library::builtin().map(|l| peer::catalog(&l)).unwrap_or_default()
}

/// An app's toolbox grant: what it declares AND the person granted.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolboxGrant {
    /// `research` and/or `crawl`.
    pub grants: BTreeSet<String>,
    pub scope: Scope,
    /// Why a declaration was not granted, for the log.
    pub notes: Vec<String>,
}

impl ToolboxGrant {
    /// Nothing granted.
    pub fn none(note: Option<String>) -> Self {
        Self { grants: BTreeSet::new(), scope: scope::unrestricted(), notes: note.into_iter().collect() }
    }

    /// The toolbox capabilities in both `declared` and `granted`, with the
    /// scope (a scope octos refuses grants nothing).
    pub fn new<'a, 'b>(
        app_id: &str,
        declared: impl IntoIterator<Item = &'a str>,
        granted: impl IntoIterator<Item = &'b str>,
        scope: Option<&Value>,
    ) -> Self {
        let granted: BTreeSet<&str> = granted.into_iter().collect();
        let grants: BTreeSet<String> = declared
            .into_iter()
            .filter(|c| (*c == RESEARCH || *c == CRAWL) && granted.contains(c))
            .map(str::to_owned)
            .collect();
        if grants.is_empty() {
            return Self::none(None);
        }
        match scope::parse(scope.unwrap_or(&json!({}))) {
            Ok(scope) => Self { grants, scope, notes: Vec::new() },
            Err(why) => Self::none(Some(format!("{app_id}: its research scope is refused ({why})"))),
        }
    }

    /// A native module's grant from its declared capabilities: reviewed
    /// with the shell (`native-apps.json`), so what it declares is granted;
    /// no manifest scope, so octos's defaults and no crawl limits.
    pub fn for_module(app_id: &str, capabilities: &[&str]) -> Self {
        Self::new(app_id, capabilities.iter().copied(), capabilities.iter().copied(), None)
    }

    /// A script app's grant from its manifest: `research`/`crawl` in its
    /// `capabilities`, the scope under [`SCOPE_KEY`]. A manifest for another
    /// id or a scope octos refuses gets nothing. The caller supplies an
    /// admitted manifest; the relay separately checks consent and tool sharing.
    pub fn for_manifest(app_id: &str, manifest: &Value) -> Self {
        if manifest.get("id").and_then(Value::as_str) != Some(app_id) {
            return Self::none(Some(format!("{app_id}: the manifest names another app")));
        }
        let declared: Vec<&str> = manifest["capabilities"]
            .as_array()
            .map(|c| c.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        Self::new(app_id, declared.iter().copied(), declared.iter().copied(), manifest.get(SCOPE_KEY))
    }

    pub fn is_empty(&self) -> bool {
        self.grants.is_empty()
    }

    /// The toolbox's [`AppContext`] for `app_id`: these grants and scope, and
    /// its host-owned folder under `apps_root`. `None` for an id that is not
    /// one path segment.
    pub fn app_context(&self, app_id: &str, apps_root: &Path) -> Option<AppContext> {
        Some(self.context(app_id, crate::toolbox_folder(apps_root, app_id)?))
    }

    fn context(&self, app_id: &str, folder: PathBuf) -> AppContext {
        let mut app = AppContext::new(app_id, folder).with_scope(self.scope.clone());
        for g in &self.grants {
            app = app.grant(g.clone());
        }
        app
    }

    /// The toolbox tool names this grant offers.
    pub fn tools(&self) -> BTreeSet<&'static str> {
        peer::tool_names(&self.context("", PathBuf::new()))
    }
}

/// The toolbox's `ModelClient` over the `model` service's host: the
/// person's providers, the app's daily budget, one ledger
/// (`<apps root>/.host/model/ledger.json`).
pub struct ModelHostClient {
    host: Option<Arc<ModelHost>>,
    /// `<apps root>/.host`, where the ledger lives.
    host_dir: PathBuf,
}

impl ModelHostClient {
    /// Over the host the shell registered (`complete::host()`, looked up at
    /// each call: the `llm` service registers it at start).
    pub fn registered(apps_root: &Path) -> Self {
        Self { host: None, host_dir: apps_root.join(".host") }
    }

    /// Over this host (tests).
    pub fn new(host: Arc<ModelHost>, apps_root: &Path) -> Self {
        Self { host: Some(host), host_dir: apps_root.join(".host") }
    }

    /// The toolbox's request as a host caller's `model.complete`: the
    /// template's prompt as the host-only `system`, its user document as the
    /// input, its output schema, and URLs allowed (the toolbox's own
    /// `validate_digest` checks the reply after).
    pub fn request(request: &ModelRequest) -> complete::Request {
        complete::Request {
            // Both tasks are bounded by their schema; the fast class is what
            // the templates were validated on (deepseek-v4-flash).
            class: Class::Fast,
            task: String::new(),
            input: Value::String(request.user.clone()),
            schema: request.output_schema.clone(),
            allow_urls: true,
            system: Some(request.system.clone()),
        }
    }
}

impl ModelClient for ModelHostClient {
    fn complete<'a>(&'a self, ctx: &'a CallContext, request: ModelRequest) -> HostFuture<'a, Result<String, HostError>> {
        Box::pin(async move {
            let Some(host) = self.host.clone().or_else(complete::host) else {
                return Err(HostError::Denied("no_provider: the model service is not running in this shell".into()));
            };
            host.attach(&self.host_dir);
            let app = ctx.app.app_id.clone();
            let request = Self::request(&request);
            let answer = tokio::task::spawn_blocking(move || host.complete(&app, request))
                .await
                .map_err(|e| HostError::Failed(format!("model call: {e}")))?;
            match answer {
                Ok(done) => Ok(done.output.to_string()),
                Err(refusal) => Err(match refusal.code {
                    Code::Budget | Code::Rate | Code::Capability | Code::NoProvider => HostError::Denied(refusal.to_string()),
                    _ => HostError::Failed(refusal.to_string()),
                }),
            }
        })
    }
}

/// One call's cancel: fired by the relay's cancel, awaited by the worker.
#[derive(Default)]
struct Cancel {
    fired: AtomicBool,
    notify: Notify,
}

impl Cancel {
    fn fire(&self) {
        self.fired.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }
    fn is_fired(&self) -> bool {
        self.fired.load(Ordering::Acquire)
    }
    async fn fired(&self) {
        loop {
            let notified = self.notify.notified();
            if self.is_fired() {
                return;
            }
            notified.await;
        }
    }
}

/// Builds an app's toolbox, on its worker thread.
pub type MakeToolbox = Arc<dyn Fn() -> Result<PeerToolbox, String> + Send + Sync>;

struct Job {
    call: HostToolCall,
    reply: ToolReply,
    cancel: Arc<Cancel>,
}

struct AppEntry {
    grant: ToolboxGrant,
    /// The app's worker, started at its first call.
    jobs: Option<mpsc::UnboundedSender<Job>>,
}

struct Inner {
    apps_root: PathBuf,
    make: MakeToolbox,
    apps: Mutex<HashMap<String, AppEntry>>,
    running: Mutex<HashMap<String, Arc<Cancel>>>,
}

/// The relay's executor for the `toolbox` owner: runs each call with the
/// calling app's grant. The toolbox's futures are not `Send` (the template
/// VM stays on its thread), so each app's toolbox lives on a worker thread
/// of its own; calls reach it over a channel, and a cancel stops a call
/// The octos research engine as the shell's toolbox backend. No Chrome on
/// a phone: its WebView renders instead (the shell runs a
/// `webview_render::WebViewRenderHost`).
fn research_backend() -> Arc<OctosResearch> {
    #[cfg(target_os = "android")]
    return Arc::new(OctosResearch::with_renderer(
        OctosConfig::from_env(),
        crate::webview_render::renderer(),
    ));
    #[cfg(not(target_os = "android"))]
    Arc::new(OctosResearch::new(OctosConfig::from_env()))
}

/// On-device check (the shell's `toolbox-research:<topic>` test action): a
/// full research run as an app agent's call would make it, for a module
/// granted `research` and `crawl`: `workflow.run` of `topic-brief` (English,
/// and Chinese translated), then `toolbox.deep_crawl`, over the octos engine
/// and the person's providers through the `model` service. Logs
/// `[toolbox-research]` lines; the full results are in the app's toolbox
/// folder.
pub fn research_test(apps_root: &Path, topic: String) {
    let root = apps_root.to_path_buf();
    let _ = std::thread::Builder::new().name("toolbox-research".into()).spawn(move || {
        // Let the shell bring the `model` service up first.
        std::thread::sleep(std::time::Duration::from_secs(20));
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => return makepad_widgets::log!("[toolbox-research] no runtime: {e}"),
        };
        runtime.block_on(async move {
            let library = match Library::builtin() {
                Ok(l) => l,
                Err(e) => return makepad_widgets::log!("[toolbox-research] no library: {e}"),
            };
            let toolbox = PeerToolbox::new(library, research_backend(), Arc::new(ModelHostClient::registered(&root)));
            let id = "os.research-check";
            // Crawl limits in the scope: `toolbox.deep_crawl` is offered only
            // with them.
            let scope = json!({"max_depth": 1, "max_pages": 6});
            let grant = ToolboxGrant::new(id, [RESEARCH, CRAWL], [RESEARCH, CRAWL], Some(&scope));
            let Some(app) = grant.app_context(id, &root) else {
                return makepad_widgets::log!("[toolbox-research] no app context");
            };
            let started = std::time::Instant::now();
            let params = json!({
                "topic": topic,
                "language": "en",
                "languages": [{"language": "en", "translate": false}, {"language": "zh", "translate": true}],
                "per_language": 3,
                "read_top": 6,
                "max_age_hours": 168
            });
            makepad_widgets::log!("[toolbox-research] workflow.run topic-brief {}", params);
            match toolbox.call(&app, peer::RUN, json!({"id": "topic-brief", "params": params})).await {
                Ok(r) => {
                    makepad_widgets::log!(
                        "[toolbox-research] run {}s status={} reasons={} sources={} stats={} result={}",
                        started.elapsed().as_secs(),
                        r["status"],
                        r["status_reasons"],
                        r["sources"].as_array().map_or(0, |s| s.len()),
                        r["stats"],
                        r["result"]
                    );
                    for s in r["sources"].as_array().into_iter().flatten() {
                        makepad_widgets::log!("[toolbox-research] source {} | {}", s["source"], s["url"]);
                    }
                    // Why steps failed (model errors among them), from the
                    // result file the agent would not see.
                    // The path is relative to the app's folder.
                    if let Some(path) = r["result"].as_str() {
                        let text = std::fs::read_to_string(app.folder.join(path)).unwrap_or_default();
                        let full: Value = serde_json::from_str(&text).unwrap_or_default();
                        for d in full["diagnostics"].as_array().into_iter().flatten().take(12) {
                            makepad_widgets::log!("[toolbox-research] diagnostic {}", d);
                        }
                    }
                    let brief = r["data"].to_string();
                    let brief: String = brief.chars().take(1500).collect();
                    makepad_widgets::log!("[toolbox-research] data {}", brief);
                }
                Err(e) => makepad_widgets::log!("[toolbox-research] run failed: {} {}", e.kind, e.message),
            }
            let started = std::time::Instant::now();
            let crawl = json!({"url": "https://www.reuters.com/technology/", "max_depth": 1, "max_pages": 6});
            match toolbox.call(&app, peer::DEEP_CRAWL, crawl).await {
                Ok(r) => {
                    let pages = r["pages"].as_array().map_or(0, |p| p.len());
                    makepad_widgets::log!(
                        "[toolbox-research] deep_crawl {}s pages={} failures={}",
                        started.elapsed().as_secs(),
                        pages,
                        r["failures"]
                    );
                    for p in r["pages"].as_array().into_iter().flatten() {
                        makepad_widgets::log!("[toolbox-research] page {} | {}", p["url"], p["title"]);
                    }
                }
                Err(e) => makepad_widgets::log!("[toolbox-research] deep_crawl failed: {} {}", e.kind, e.message),
            }
            makepad_widgets::log!("[toolbox-research] done");
        });
    });
}

/// there. A cancelled call is never answered.
#[derive(Clone)]
pub struct ToolboxExecutor(Arc<Inner>);

impl ToolboxExecutor {
    /// Toolboxes built by `make`, with results under `apps_root`.
    pub fn new(apps_root: &Path, make: MakeToolbox) -> Self {
        Self(Arc::new(Inner {
            apps_root: apps_root.to_path_buf(),
            make,
            apps: Mutex::new(HashMap::new()),
            running: Mutex::new(HashMap::new()),
        }))
    }

    /// The shell's: the octos research engine, and the person's providers
    /// through the `model` service.
    pub fn shell(apps_root: &Path) -> Self {
        let root = apps_root.to_path_buf();
        Self::new(
            apps_root,
            Arc::new(move || {
                let library = Library::builtin().map_err(|e| e.to_string())?;
                let backend = research_backend();
                Ok(PeerToolbox::new(library, backend, Arc::new(ModelHostClient::registered(&root))))
            }),
        )
    }

    /// Set `app_id`'s grant (a module's id, or a contained app's id without
    /// its peer's `card.` prefix). Returns the tool names it offers. A
    /// changed grant starts a new worker at the next call.
    pub fn set_grant(&self, app_id: &str, grant: ToolboxGrant) -> BTreeSet<&'static str> {
        let tools = grant.tools();
        let mut apps = self.0.apps.lock().unwrap_or_else(|e| e.into_inner());
        match apps.get_mut(app_id) {
            Some(entry) if entry.grant == grant => {}
            _ => {
                apps.insert(app_id.to_owned(), AppEntry { grant, jobs: None });
            }
        }
        tools
    }

    /// The tool names `app_id` is granted (empty for an app never granted).
    pub fn tools(&self, app_id: &str) -> BTreeSet<&'static str> {
        self.0.apps.lock().unwrap_or_else(|e| e.into_inner()).get(app_id).map(|e| e.grant.tools()).unwrap_or_default()
    }

    /// Calls running now.
    pub fn running(&self) -> usize {
        self.0.running.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    fn worker_for(&self, app_id: &str) -> Result<mpsc::UnboundedSender<Job>, ToolOutcome> {
        let mut apps = self.0.apps.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = apps.get_mut(app_id) else {
            return Err(ToolOutcome::error("not_granted", format!("{app_id} has no toolbox grant")));
        };
        if let Some(jobs) = entry.jobs.as_ref().filter(|j| !j.is_closed()) {
            return Ok(jobs.clone());
        }
        let app = entry
            .grant
            .app_context(app_id, &self.0.apps_root)
            .ok_or_else(|| ToolOutcome::error("not_granted", format!("{app_id} cannot have a toolbox folder")))?;
        let (jobs, rx) = mpsc::unbounded_channel();
        let make = self.0.make.clone();
        let inner = Arc::downgrade(&self.0);
        std::thread::Builder::new()
            .name(format!("toolbox-{app_id}"))
            .spawn(move || worker(app, make, rx, inner))
            .map_err(|e| ToolOutcome::error("unavailable", format!("no toolbox worker: {e}")))?;
        entry.jobs = Some(jobs.clone());
        Ok(jobs)
    }
}

impl ToolExecutor for ToolboxExecutor {
    fn execute(&self, call: HostToolCall, reply: ToolReply) {
        if !reply.is_open() {
            return;
        }
        // The relay authorized the call; the toolbox checks its own grant.
        let app = app_of_peer(&call.calling_app).to_owned();
        if !self.tools(&app).contains(call.name.as_str()) {
            reply.finish(ToolOutcome::error("not_granted", format!("{} is not among {app}'s toolbox tools", call.name)));
            return;
        }
        let jobs = match self.worker_for(&app) {
            Ok(jobs) => jobs,
            Err(outcome) => {
                reply.finish(outcome);
                return;
            }
        };
        let cancel = Arc::new(Cancel::default());
        self.0.running.lock().unwrap_or_else(|e| e.into_inner()).insert(call.call_id.clone(), cancel.clone());
        let call_id = call.call_id.clone();
        if jobs.send(Job { call, reply: reply.clone(), cancel }).is_err() {
            self.0.running.lock().unwrap_or_else(|e| e.into_inner()).remove(&call_id);
            reply.finish(ToolOutcome::error("unavailable", "the toolbox is not running"));
        }
    }

    fn cancel(&self, call_id: &str) {
        if let Some(cancel) = self.0.running.lock().unwrap_or_else(|e| e.into_inner()).remove(call_id) {
            cancel.fire();
        }
    }
}

/// A contained app's peer is `card.<app id>`; its grant is the app's.
fn app_of_peer(app: &str) -> &str {
    app.strip_prefix(crate::contained::PEER_PREFIX).unwrap_or(app)
}

fn worker(app: AppContext, make: MakeToolbox, mut jobs: mpsc::UnboundedReceiver<Job>, inner: std::sync::Weak<Inner>) {
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            makepad_widgets::log!("toolbox: no runtime for {}: {e}", app.app_id);
            return;
        }
    };
    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, async move {
        let toolbox = make().map(Rc::new);
        if let Err(why) = &toolbox {
            makepad_widgets::log!("toolbox: {} has no toolbox: {why}", app.app_id);
        }
        let app = Rc::new(app);
        while let Some(Job { call, reply, cancel }) = jobs.recv().await {
            let toolbox = toolbox.clone();
            let app = app.clone();
            let inner = inner.clone();
            tokio::task::spawn_local(async move {
                let outcome = if cancel.is_fired() {
                    None
                } else {
                    match &toolbox {
                        Err(why) => Some(ToolOutcome::error("unavailable", why.clone())),
                        Ok(toolbox) => tokio::select! {
                            result = toolbox.call(&app, &call.name, call.args) => Some(match result {
                                Ok(data) => ToolOutcome::Ok(data),
                                Err(e) => ToolOutcome::error(&e.kind, e.message),
                            }),
                            // Never finish a call the kernel stopped waiting for.
                            _ = cancel.fired() => None,
                        },
                    }
                };
                if let Some(inner) = inner.upgrade() {
                    inner.running.lock().unwrap_or_else(|e| e.into_inner()).remove(&call.call_id);
                }
                if let Some(outcome) = outcome {
                    // False (nothing sent) when the call was cancelled meanwhile.
                    reply.finish(outcome);
                }
            });
        }
    });
}
