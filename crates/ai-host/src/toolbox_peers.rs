//! The system toolbox offered to app agents (ADR 0002 section 6, News M5;
//! octos UPCR-2026-035 / octos#2567). Feature `toolbox-peers`.
//!
//! When the broker sets up an app's peer, it registers the toolbox tools the
//! app's grants allow ([`octosense_toolbox::peer::tool_decls`]): `research`
//! gives `workflow.run`, `workflow.fork`, `toolbox.search` and
//! `toolbox.web_read`; `crawl` (with crawl limits in the scope) gives
//! `toolbox.deep_crawl`; no grant gives the empty set, which the broker still
//! registers. octos's `deep_research` is never offered.
//!
//! Each `peer/tool/call` runs [`ToolboxTools`]: the toolbox with the app's
//! [`AppContext`] (id, grants, octos `Scope`, budget, and its host-owned
//! folder), over the octos research engine (`OctosResearch`), with the
//! templates' model calls going through the `model` service's host
//! ([`ModelHostClient`] over `ModelHost::complete`), so they use the
//! person's providers and count against the app's daily budget in the same
//! ledger as `model.complete`.
//!
//! **Results** are written under [`crate::toolbox_folder`]
//! (`<apps root>/.host/toolbox/<app>`), outside the app's jail
//! (`<apps root>/<app>`), where the glance screen's `sys.digest` reads them
//! (OctoSense #87): an app cannot forge a digest.
//!
//! **Where grants come from** ([`grant_from_manifest`],
//! [`grant_from_capabilities`]): the app's manifest declares `research`
//! and/or `crawl` among its `capabilities`, with its scope, in octos's
//! `Scope` shape, under the manifest's [`SCOPE_KEY`] object (absent: no
//! narrowing beyond octos's defaults, and no crawl limits, so no crawling).
//!
//! **TEMPORARY, until the shells' App Hub pin includes App Hub #26 (which
//! admits the `research` and `crawl` capabilities: checks, pins and lets the
//! person grant them) and the host reads its verified `AppPolicy::research`:**
//! only system apps
//! (`os.*`, shipped inside the shell and reviewed with it) get what they
//! declare. Any other app's declaration is ignored, so a store app cannot
//! grant itself research by writing it into its manifest. Remove
//! [`system_app_only`] when App Hub verifies the grant, and read the verified
//! grant instead.

use crate::toolbox_folder;
use octosense_app_peers::peer_tools::{Cancel, HostTools, Registration, ToolCall, ToolError, ToolFuture};
use octosense_llm_service::complete::{self, Class, Code, ModelHost};
use octosense_toolbox::host::{CallContext, HostError, HostFuture};
use octosense_toolbox::peer::{self, PeerToolbox, CRAWL, RESEARCH};
use octosense_toolbox::research::octos::{OctosConfig, OctosResearch};
use octosense_toolbox::research::{ModelClient, ModelRequest};
use octosense_toolbox::{scope, AppContext, Library, Scope};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

/// The manifest object that holds the `research`/`crawl` scope (octos's
/// `Scope` fields): App Hub #26's shape (`capabilities: ["research",
/// "crawl"]` plus one top-level `research` object).
pub const SCOPE_KEY: &str = "research";

/// How long the kernel waits for one toolbox call: the most octos allows
/// (a template's own `max_ms` is at most 300 s too).
pub const CALL_TIMEOUT_MS: u64 = 300_000;
/// The largest result the kernel accepts from the toolbox (octos's ceiling).
pub const MAX_RESULT_BYTES: u64 = 1024 * 1024;

/// An app's toolbox grant.
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
        Self {
            grants: BTreeSet::new(),
            scope: scope::unrestricted(),
            notes: note.into_iter().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.grants.is_empty()
    }
}

/// TEMPORARY (see the module docs): whether `app_id` may have what it
/// declares without App Hub's check. System apps only.
pub fn system_app_only(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

/// The grant an app's manifest declares: `research`/`crawl` in its
/// `capabilities`, the scope under [`SCOPE_KEY`]. A manifest for another id,
/// a scope octos refuses, or (for now) an app that is not a system app gets
/// nothing.
pub fn grant_from_manifest(app_id: &str, manifest: &Value) -> ToolboxGrant {
    if manifest.get("id").and_then(Value::as_str) != Some(app_id) {
        return ToolboxGrant::none(Some(format!("{app_id}: the manifest names another app")));
    }
    let declared = manifest["capabilities"]
        .as_array()
        .map(|c| c.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    grant(app_id, declared, manifest.get(SCOPE_KEY))
}

/// The grant of a native module from its declared capabilities: it has no
/// manifest scope, so octos's defaults and no crawl limits.
pub fn grant_from_capabilities<'a>(app_id: &str, capabilities: impl IntoIterator<Item = &'a str>) -> ToolboxGrant {
    grant(app_id, capabilities, None)
}

fn grant<'a>(app_id: &str, declared: impl IntoIterator<Item = &'a str>, scope: Option<&Value>) -> ToolboxGrant {
    let declared: BTreeSet<String> = declared
        .into_iter()
        .filter(|c| *c == RESEARCH || *c == CRAWL)
        .map(str::to_owned)
        .collect();
    if declared.is_empty() {
        return ToolboxGrant::none(None);
    }
    if !system_app_only(app_id) {
        return ToolboxGrant::none(Some(format!(
            "{app_id} declares {declared:?}, but App Hub does not verify those capabilities yet: until it does only system apps (os.*) get them"
        )));
    }
    match scope::parse(scope.unwrap_or(&json!({}))) {
        Ok(scope) => ToolboxGrant { grants: declared, scope, notes: Vec::new() },
        Err(why) => ToolboxGrant::none(Some(format!("{app_id}: its research scope is refused ({why})"))),
    }
}

/// The toolbox's [`AppContext`] for an app: its grants and scope, and its
/// host-owned folder under `apps_root`. `None` for an id that is not one
/// path segment.
pub fn app_context(app_id: &str, grant: &ToolboxGrant, apps_root: &Path) -> Option<AppContext> {
    let folder = toolbox_folder(apps_root, app_id)?;
    let mut app = AppContext::new(app_id, folder).with_scope(grant.scope.clone());
    for g in &grant.grants {
        app = app.grant(g.clone());
    }
    Some(app)
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

type Job = (ToolCall, Cancel, oneshot::Sender<Result<Value, ToolError>>);

/// One app's toolbox tools for its peer. The toolbox's futures are not
/// `Send` (the template VM stays on its thread), so each app's toolbox lives
/// on a worker thread of its own; the broker's calls reach it over a
/// channel, and a call's cancel stops it there.
pub struct ToolboxTools {
    app_id: String,
    registration: Registration,
    jobs: mpsc::UnboundedSender<Job>,
}

impl ToolboxTools {
    /// The tools of `app`, run by the toolbox `make` builds on the worker.
    pub fn new(app: AppContext, make: impl FnOnce() -> Result<PeerToolbox, String> + Send + 'static) -> Self {
        let registration = Registration {
            tools: Library::builtin().map(|l| peer::tool_decls(&app, &l)).unwrap_or_default(),
            generic_tools: Vec::new(),
            call_timeout_ms: Some(CALL_TIMEOUT_MS),
            max_result_bytes: Some(MAX_RESULT_BYTES),
        };
        let app_id = app.app_id.clone();
        let (jobs, rx) = mpsc::unbounded_channel();
        let spawned = std::thread::Builder::new()
            .name(format!("toolbox-{app_id}"))
            .spawn(move || worker(app, make, rx));
        if let Err(e) = spawned {
            makepad_widgets::log!("toolbox: no worker for {app_id}: {e}");
        }
        Self { app_id, registration, jobs }
    }

    /// The shell's toolbox for `app_id`: the octos research engine and the
    /// person's providers through the `model` service. `None` without a
    /// grant (the broker then registers the empty set).
    pub fn shell(app_id: &str, grant: &ToolboxGrant, apps_root: &Path) -> Option<Self> {
        for note in &grant.notes {
            makepad_widgets::log!("toolbox: {note}");
        }
        if grant.is_empty() {
            return None;
        }
        let app = app_context(app_id, grant, apps_root)?;
        let root = apps_root.to_path_buf();
        Some(Self::new(app, move || {
            let library = Library::builtin().map_err(|e| e.to_string())?;
            let backend = Arc::new(OctosResearch::new(OctosConfig::from_env()));
            Ok(PeerToolbox::new(library, backend, Arc::new(ModelHostClient::registered(&root))))
        }))
    }

    pub fn app_id(&self) -> &str {
        &self.app_id
    }
}

fn worker(
    app: AppContext,
    make: impl FnOnce() -> Result<PeerToolbox, String>,
    mut jobs: mpsc::UnboundedReceiver<Job>,
) {
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
        while let Some((call, cancel, reply)) = jobs.recv().await {
            let toolbox = toolbox.clone();
            let app = app.clone();
            tokio::task::spawn_local(async move {
                if cancel.is_cancelled() {
                    return;
                }
                let result = match &toolbox {
                    Err(why) => Err(ToolError::new("unavailable", why.clone())),
                    Ok(toolbox) => {
                        tokio::select! {
                            result = toolbox.call(&app, &call.name, call.args) => {
                                result.map_err(|e| ToolError::new(&e.kind, e.message))
                            }
                            // Never finish a call the kernel stopped waiting for.
                            _ = cancel.cancelled() => return,
                        }
                    }
                };
                let _ = reply.send(result);
            });
        }
    });
}

impl HostTools for ToolboxTools {
    fn registration(&self) -> Registration {
        self.registration.clone()
    }

    fn call(&self, call: ToolCall, cancel: Cancel) -> ToolFuture {
        let (tx, rx) = oneshot::channel();
        let sent = self.jobs.send((call, cancel, tx));
        Box::pin(async move {
            if sent.is_err() {
                return Err(ToolError::new("unavailable", "the toolbox is not running"));
            }
            rx.await
                .unwrap_or_else(|_| Err(ToolError::new("cancelled", "the toolbox call was stopped")))
        })
    }
}

/// The tools a contained app's peer gets from its manifest (OctoSense #106
/// launches those peers): `None` without a grant or before the host knows
/// its apps root.
pub fn tools_for_manifest(app_id: &str, manifest: &Value) -> Option<Arc<dyn HostTools>> {
    let root = octosense_appstore::data_root_if_set()?;
    ToolboxTools::shell(app_id, &grant_from_manifest(app_id, manifest), &root).map(|t| Arc::new(t) as Arc<dyn HostTools>)
}

/// The tools a native module's peer gets from its declared capabilities.
pub fn tools_for_module<'a>(app_id: &str, capabilities: impl IntoIterator<Item = &'a str>) -> Option<Arc<dyn HostTools>> {
    let grant = grant_from_capabilities(app_id, capabilities);
    if grant.is_empty() && grant.notes.is_empty() {
        return None;
    }
    let Some(root) = octosense_appstore::data_root_if_set() else {
        makepad_widgets::log!("toolbox: {app_id} is granted research, but the apps root is not known yet; no tools");
        return None;
    };
    ToolboxTools::shell(app_id, &grant, &root).map(|t| Arc::new(t) as Arc<dyn HostTools>)
}
