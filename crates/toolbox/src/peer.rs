//! The toolbox as an app peer's host-registered tools (ADR 0002 section 6;
//! octos UPCR-2026-035, octos#2567).
//!
//! The kernel offers an app's peer only the tools its host registers, and
//! sends every call back to the host. This module is what the host
//! registers and runs for the toolbox, whoever carries the calls
//! (`crates/ai-host` with its `toolbox-peers` feature):
//!
//! - [`tool_decls`]: the `tools.json` entries of an app, **from its grants
//!   alone**. `research` gives `workflow.run` and `workflow.fork` (the
//!   template library, [`crate::api`]), `toolbox.search` and
//!   `toolbox.web_read`; `crawl` gives `toolbox.deep_crawl`, and only while
//!   the scope's crawl limits are not 0 (0 means crawling is not granted).
//!   octos's `deep_research` is never offered: the templates replace it.
//!   `workflow.list` and `workflow.evaluate` are not offered either; the run
//!   tool's description lists the templates. No grant, no tools.
//! - [`PeerToolbox::call`]: one call, checked **again** against the grants
//!   (a call the registration did not offer is refused as `not_granted`,
//!   whatever the kernel sent) and run with the app's [`AppContext`]: its
//!   id, grants, octos `Scope`, budget and folder.
//!
//! Tool names are `<app>.<tool>` for octos (2–4 `.`-separated segments), so
//! the single research tools are `toolbox.search`, `toolbox.web_read` and
//! `toolbox.deep_crawl`; the model sees them as `toolbox_search`, …
//!
//! **Risk**: `read` for everything except `workflow.fork` (`act`: it writes a
//! copy into the app's own toolbox folder). None is `outward` or
//! `destructive`, so the kernel never gates them and `confirm` (`host`, the
//! default: the host runs them, the app has no sheet for them) never comes
//! into play. All are `background`: an app's agent runs them on its own
//! triggers (News M3), with nobody present.
//!
//! **Results** go where the app cannot write: the app's toolbox folder is
//! the host's (`<apps root>/.host/toolbox/<app>`, chosen by the caller), so
//! run results (`toolbox/runs/<template>/<run>.json`, read by the glance
//! screen's `sys.digest`) and research items (`research/*.json`) cannot be
//! forged by the app. Each reply carries the file's path relative to that
//! folder and a compact summary for the agent.

use crate::api::Toolbox;
use crate::host::{url_host, AppContext, CallContext, HostError, Remaining};
use crate::library::{hex, Library};
use crate::manifest::Budget;
use crate::research::{
    cap_evidence, item_id, FoundItem, ModelClient, ResearchBackend, ResearchHost, SearchQuery,
    MAX_EVIDENCE_BYTES, MAX_EXCERPT_BYTES, MAX_SEARCH_FETCHES,
};
use crate::scope;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The capability that grants the research tools and the templates.
pub const RESEARCH: &str = "research";
/// The capability that grants `toolbox.deep_crawl`.
pub const CRAWL: &str = "crawl";

pub const RUN: &str = "workflow.run";
pub const FORK: &str = "workflow.fork";
pub const SEARCH: &str = "toolbox.search";
pub const WEB_READ: &str = "toolbox.web_read";
pub const DEEP_CRAWL: &str = "toolbox.deep_crawl";

/// Results one `toolbox.search` returns when the call asks for no count.
pub const DEFAULT_SEARCH_COUNT: u32 = 10;
/// Page text one `toolbox.web_read` returns to the agent (the whole text is
/// in the saved file).
pub const WEB_READ_TEXT_MAX: usize = 16 * 1024;
/// Links queued per crawl page at most, and the whole frontier.
const CRAWL_LINKS_PER_PAGE: usize = 200;
const CRAWL_FRONTIER_MAX: usize = 2000;
/// How long one single research call may take (a run's own `max_ms` bounds
/// the templates).
const TOOL_MS: u64 = 120_000;

/// Whether the app's scope lets it crawl at all.
pub fn crawl_allowed(app: &AppContext) -> bool {
    app.grants.contains(CRAWL) && app.scope.max_depth > 0 && app.scope.max_pages > 0
}

/// The tool names an app with these grants is offered.
pub fn tool_names(app: &AppContext) -> BTreeSet<&'static str> {
    let mut names = BTreeSet::new();
    if app.grants.contains(RESEARCH) {
        names.extend([RUN, FORK, SEARCH, WEB_READ]);
    }
    if crawl_allowed(app) {
        names.insert(DEEP_CRAWL);
    }
    names
}

/// The `tools.json` entries (octos `peer/tools/register` `tools`) of an app
/// with these grants, in a fixed order. Empty without a grant.
pub fn tool_decls(app: &AppContext, library: &Library) -> Vec<Value> {
    let names = tool_names(app);
    let mut decls = Vec::new();
    let decl = |name: &str, risk: &str, description: String, input_schema: Value| {
        json!({
            "name": name,
            "description": description,
            "input_schema": input_schema,
            "risk": risk,
            "background": true,
            "outward": false,
            "confirm": "host",
        })
    };
    let workflow = crate::api::tool_descriptors();
    let schema_of = |name: &str| {
        workflow
            .as_array()
            .and_then(|tools| tools.iter().find(|t| t["name"] == name))
            .map(|t| t["input_schema"].clone())
            .unwrap_or_else(|| json!({"type": "object"}))
    };
    if names.contains(RUN) {
        let mut listed = String::new();
        for t in library.templates() {
            let line = format!("{} ({}); ", t.manifest.id, t.manifest.title);
            if listed.len() + line.len() > 1200 {
                break;
            }
            listed.push_str(&line);
        }
        decls.push(decl(
            RUN,
            "read",
            format!(
                "Run a toolbox workflow template with parameters (each template's `params` schema is in its manifest). \
                 The result, with its sources, is saved in this app's toolbox folder and summarized here. \
                 Templates: {}or a fork of one made with workflow.fork.",
                listed
            ),
            schema_of(RUN),
        ));
    }
    if names.contains(FORK) {
        decls.push(decl(
            FORK,
            "act",
            "Copy a library template into this app's toolbox folder to change it; the copy keeps lineage to its parent and never gains more than it had.".into(),
            schema_of(FORK),
        ));
    }
    if names.contains(SEARCH) {
        decls.push(decl(
            SEARCH,
            "read",
            "Search free news sources (Google News, GDELT, publisher feeds) within this app's granted languages, regions, domains and recency; returns dated items with their sources, saved in this app's toolbox folder.".into(),
            json!({
                "type": "object", "required": ["query"], "additionalProperties": false,
                "properties": {
                    "query": {"type": "string", "minLength": 1, "maxLength": 300},
                    "lang": {"type": "string", "description": "BCP-47 language to search in (must be granted to the app)"},
                    "region": {"type": "string", "description": "ISO 3166-1 alpha-2 region"},
                    "max_age_days": {"type": "integer", "minimum": 1, "description": "only material this recent"},
                    "count": {"type": "integer", "minimum": 1, "maximum": 30}
                }
            }),
        ));
    }
    if names.contains(WEB_READ) {
        decls.push(decl(
            WEB_READ,
            "read",
            "Read one page (rendered by a real browser when needed) inside this app's granted domains and return its main text; the page is saved in this app's toolbox folder.".into(),
            json!({
                "type": "object", "required": ["url"], "additionalProperties": false,
                "properties": {"url": {"type": "string", "minLength": 1, "maxLength": 2048}}
            }),
        ));
    }
    if names.contains(DEEP_CRAWL) {
        decls.push(decl(
            DEEP_CRAWL,
            "read",
            format!(
                "Crawl one site within this app's limits (same site, at most {} link hops and {} pages, optionally under a path prefix); the pages are saved in this app's toolbox folder.",
                app.scope.max_depth, app.scope.max_pages
            ),
            json!({
                "type": "object", "required": ["url"], "additionalProperties": false,
                "properties": {
                    "url": {"type": "string", "minLength": 1, "maxLength": 2048},
                    "max_depth": {"type": "integer", "minimum": 1},
                    "max_pages": {"type": "integer", "minimum": 1},
                    "path_prefix": {"type": "string"}
                }
            }),
        ));
    }
    decls
}

/// A refused or failed call: `kind` is `[a-z0-9_]{1,32}` (octos passes it to
/// the model as `host:<kind>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerError {
    pub kind: String,
    pub message: String,
}

impl PeerError {
    pub fn new(kind: &str, message: impl Into<String>) -> Self {
        Self {
            kind: kind.to_owned(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for PeerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

fn host_error(e: HostError) -> PeerError {
    match e {
        HostError::Denied(m) => PeerError::new("denied", m),
        HostError::Failed(m) => PeerError::new("failed", m),
        HostError::TimedOut(m) => PeerError::new("timeout", m),
    }
}

/// The toolbox one host runs for app peers: the template library over
/// `mod.research`, and the same research backend for the single tools.
pub struct PeerToolbox {
    toolbox: Toolbox<ResearchHost>,
    backend: Arc<dyn ResearchBackend>,
    clock: Arc<dyn Fn() -> chrono::DateTime<chrono::Utc>>,
}

impl PeerToolbox {
    /// The toolbox over `backend` (the octos research engine in a shell) with
    /// `model` for the templates' model calls (the person's providers).
    pub fn new(
        library: Library,
        backend: Arc<dyn ResearchBackend>,
        model: Arc<dyn ModelClient>,
    ) -> Self {
        let host = ResearchHost::new(backend.clone(), model);
        Self::from_host(library, host, backend)
    }

    /// The toolbox over an already built `mod.research` host (fixtures set
    /// its clock); `backend` must be the one `host` uses.
    pub fn from_host(
        library: Library,
        host: ResearchHost,
        backend: Arc<dyn ResearchBackend>,
    ) -> Self {
        Self {
            toolbox: Toolbox::new(library, host),
            backend,
            clock: Arc::new(chrono::Utc::now),
        }
    }

    /// Replaces the clock of saved research items (tests).
    pub fn with_clock(
        mut self,
        clock: impl Fn() -> chrono::DateTime<chrono::Utc> + 'static,
    ) -> Self {
        self.clock = Arc::new(clock);
        self
    }

    pub fn library(&self) -> &Library {
        self.toolbox.library()
    }

    /// The app's `tools.json` entries ([`tool_decls`]).
    pub fn decls(&self, app: &AppContext) -> Vec<Value> {
        tool_decls(app, self.library())
    }

    /// Runs one peer tool call for `app`. The call is checked against the
    /// app's grants again: a tool it was not offered is `not_granted`.
    pub async fn call(
        &self,
        app: &AppContext,
        name: &str,
        args: Value,
    ) -> Result<Value, PeerError> {
        if !tool_names(app).contains(name) {
            return Err(PeerError::new(
                "not_granted",
                format!("{name} is not among this app's toolbox tools"),
            ));
        }
        if !args.is_object() {
            return Err(PeerError::new("bad_request", "arguments must be an object"));
        }
        match name {
            RUN | FORK => self.workflow(app, name, args).await,
            SEARCH => self.search(app, &args).await,
            WEB_READ => self.web_read(app, &args).await,
            DEEP_CRAWL => self.deep_crawl(app, &args).await,
            _ => Err(PeerError::new(
                "not_granted",
                format!("no toolbox tool {name}"),
            )),
        }
    }

    async fn workflow(
        &self,
        app: &AppContext,
        name: &str,
        args: Value,
    ) -> Result<Value, PeerError> {
        let reply = self
            .toolbox
            .handle_json(app, json!({"tool": name, "arguments": args}))
            .await;
        if let Some(error) = reply.get("error") {
            let kind = error["kind"].as_str().unwrap_or("failed");
            return Err(PeerError::new(
                kind,
                error["message"]
                    .as_str()
                    .unwrap_or("the toolbox refused the call"),
            ));
        }
        if name == FORK {
            return Ok(json!({
                "id": reply["template"]["id"],
                "version": reply["template"]["version"],
                "lineage": reply["template"]["lineage"],
                "path": reply["path"],
            }));
        }
        // The agent gets the data and where the full result is; the trace,
        // diagnostics and evidence stay in the file.
        let sources: Vec<Value> = reply["provenance"]
            .as_array()
            .map(|p| {
                p.iter()
                    .map(|s| {
                        json!({"id": s["id"], "url": s["url"], "title": s["title"],
                               "source": s["source"], "published_at": s["published_at"]})
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(json!({
            "run_id": reply["run_id"],
            "template": reply["template"],
            "status": reply["status"],
            "status_reasons": reply["status_reasons"],
            "data": reply["data"],
            "sources": sources,
            "stats": reply["stats"],
            "result": reply["result_path"],
        }))
    }

    fn context(&self, app: &AppContext, tool: &str, reads: u32) -> CallContext {
        CallContext {
            app: Arc::new(app.clone()),
            run_id: format!("{tool}-{}", (self.clock)().format("%Y%m%dT%H%M%S%3fZ")),
            template_id: tool.to_owned(),
            template_digest: String::new(),
            budget: Budget {
                max_calls: reads.max(1),
                max_model_calls: 0,
                max_reads: reads,
                max_ms: TOOL_MS,
                max_concurrency: 1,
            },
            remaining: Remaining {
                calls: reads.max(1),
                model_calls: 0,
                reads,
                ms: TOOL_MS,
            },
            call_index: 0,
        }
    }

    async fn search(&self, app: &AppContext, args: &Value) -> Result<Value, PeerError> {
        let query = args["query"].as_str().unwrap_or("").trim();
        let lang = args["lang"].as_str();
        let region = args["region"].as_str();
        let max_age_hours = args["max_age_days"]
            .as_u64()
            .map(|d| d.min(3650) as u32 * 24);
        let count = args["count"]
            .as_u64()
            .map_or(DEFAULT_SEARCH_COUNT, |c| c.clamp(1, 30) as u32);
        let narrowed = scope::narrow_search(&app.scope, query, lang, region, max_age_hours, count)
            .map_err(|e| PeerError::new("denied", e))?;
        let ctx = self.context(app, SEARCH, 0);
        let found = self
            .backend
            .search(
                &ctx,
                SearchQuery {
                    topic: query.to_owned(),
                    language: lang.map(str::to_owned).filter(|l| !l.is_empty()),
                    region: region.map(str::to_owned).filter(|r| !r.is_empty()),
                    limit: narrowed.limit,
                    max_age_hours: narrowed.max_age_hours,
                    max_fetches: MAX_SEARCH_FETCHES,
                },
            )
            .await
            .map_err(host_error)?;
        let mut notes = narrowed.notes;
        notes.extend(found.notes);
        let items: Vec<Value> = found
            .items
            .iter()
            .filter(|i| app.scope.check_domain(&i.url).is_ok())
            .take(narrowed.limit as usize)
            .enumerate()
            .map(|(n, i)| {
                json!({"n": n + 1, "id": item_id(&i.url), "title": i.title, "url": i.url,
                       "source": i.source, "language": i.language, "published_at": i.published_at,
                       "readable": i.readable, "via": i.via})
            })
            .collect();
        let now = self.stamp();
        let doc = json!({
            "kind": "search", "app_id": app.app_id, "query": query, "retrieved_at": now,
            "items": items, "providers": found.providers, "partial": found.partial, "notes": notes,
        });
        let file = save(&app.folder, "search", query, &doc)?;
        let mut reply = doc;
        reply["file"] = json!(file);
        Ok(reply)
    }

    async fn web_read(&self, app: &AppContext, args: &Value) -> Result<Value, PeerError> {
        let url = args["url"].as_str().unwrap_or("").trim();
        if url_host(url).is_none() {
            return Err(PeerError::new("bad_request", "url must be an http(s) URL"));
        }
        app.scope
            .check_domain(url)
            .map_err(|e| PeerError::new("denied", e))?;
        let ctx = self.context(app, WEB_READ, 1);
        let item = FoundItem {
            url: url.to_owned(),
            title: String::new(),
            source: url_host(url).unwrap_or_default(),
            language: String::new(),
            published_at: String::new(),
            via: "web_read".into(),
            readable: true,
            snippet: String::new(),
        };
        let page = self.backend.read(&ctx, &item).await.map_err(host_error)?;
        let sha = hex(&Sha256::digest(page.text.as_bytes()));
        let (text, truncated) = cap_evidence(&page.text, WEB_READ_TEXT_MAX);
        let now = self.stamp();
        let doc = json!({
            "kind": "read", "app_id": app.app_id, "url": url, "id": item_id(url),
            "title": page.title, "retrieved_at": now, "evidence_sha256": sha, "text": page.text,
        });
        let file = save(&app.folder, "read", url, &doc)?;
        Ok(json!({
            "url": url, "id": item_id(url), "title": page.title, "retrieved_at": now,
            "evidence_sha256": sha, "text": text, "truncated": truncated, "file": file,
        }))
    }

    async fn deep_crawl(&self, app: &AppContext, args: &Value) -> Result<Value, PeerError> {
        let narrowed =
            scope::narrow_crawl(&app.scope, args).map_err(|e| PeerError::new("denied", e))?;
        let site = url_host(&narrowed.url)
            .ok_or_else(|| PeerError::new("bad_request", "url must be an http(s) URL"))?;
        let site = site.strip_prefix("www.").unwrap_or(&site).to_owned();
        let same_site = |link: &str| {
            url_host(link).is_some_and(|h| h.strip_prefix("www.").unwrap_or(&h) == site)
        };
        let under_prefix = |link: &str| match &narrowed.path_prefix {
            None => true,
            Some(prefix) => url_path(link).is_some_and(|p| p.starts_with(prefix.as_str())),
        };
        let ctx = self.context(app, DEEP_CRAWL, narrowed.max_pages);
        let mut frontier = VecDeque::from([(narrowed.url.clone(), 0u32)]);
        let mut seen = BTreeSet::from([narrowed.url.clone()]);
        let mut pages = Vec::new();
        let mut failures = Vec::new();
        let mut attempts = 0;
        while let Some((url, depth)) = frontier.pop_front() {
            if attempts >= narrowed.max_pages {
                break;
            }
            attempts += 1;
            match self.backend.read_links(&ctx, &url).await {
                Ok(linked) => {
                    let (evidence, _) = cap_evidence(&linked.page.text, MAX_EVIDENCE_BYTES);
                    let (excerpt, _) = cap_evidence(&linked.page.text, MAX_EXCERPT_BYTES);
                    pages.push(json!({
                        "n": pages.len() + 1, "url": linked.final_url, "id": item_id(&linked.final_url),
                        "title": linked.page.title, "depth": depth, "excerpt": excerpt, "text": evidence,
                        "evidence_sha256": hex(&Sha256::digest(linked.page.text.as_bytes())),
                    }));
                    if depth < narrowed.max_depth {
                        for link in linked.links.into_iter().take(CRAWL_LINKS_PER_PAGE) {
                            if frontier.len() >= CRAWL_FRONTIER_MAX {
                                break;
                            }
                            if same_site(&link)
                                && under_prefix(&link)
                                && app.scope.check_domain(&link).is_ok()
                                && seen.insert(link.clone())
                            {
                                frontier.push_back((link, depth + 1));
                            }
                        }
                    }
                }
                Err(e) if pages.is_empty() && frontier.is_empty() => return Err(host_error(e)),
                Err(e) => failures.push(json!({"url": url, "error": e.to_string()})),
            }
        }
        let now = self.stamp();
        let doc = json!({
            "kind": "crawl", "app_id": app.app_id, "url": narrowed.url, "retrieved_at": now,
            "max_depth": narrowed.max_depth, "max_pages": narrowed.max_pages,
            "path_prefix": narrowed.path_prefix, "pages": pages, "failures": failures,
        });
        let file = save(&app.folder, "crawl", &narrowed.url, &doc)?;
        let listed: Vec<Value> = pages
            .iter()
            .map(|p| json!({"n": p["n"], "url": p["url"], "title": p["title"], "depth": p["depth"], "excerpt": p["excerpt"]}))
            .collect();
        Ok(json!({
            "url": narrowed.url, "retrieved_at": now, "pages": listed,
            "failures": doc["failures"], "file": file,
        }))
    }

    fn stamp(&self) -> String {
        (self.clock)().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }
}

/// Writes `doc` to `<folder>/research/<kind>-<slug>-<hash>.json` and returns
/// the path relative to the folder.
fn save(folder: &Path, kind: &str, subject: &str, doc: &Value) -> Result<String, PeerError> {
    let dir = folder.join("research");
    std::fs::create_dir_all(&dir)
        .map_err(|e| PeerError::new("io", format!("toolbox folder: {e}")))?;
    let body = serde_json::to_vec_pretty(doc).map_err(|e| PeerError::new("io", e.to_string()))?;
    let hash = &hex(&Sha256::digest(&body))[..12];
    let name = format!("{kind}-{}-{hash}.json", slug(subject));
    let path: PathBuf = dir.join(&name);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &body)
        .and_then(|_| std::fs::rename(&tmp, &path))
        .map_err(|e| PeerError::new("io", format!("write {name}: {e}")))?;
    Ok(format!("research/{name}"))
}

/// The path of an `http(s)` URL (`/` when it has none).
fn url_path(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let rest = rest.split(['?', '#']).next()?;
    Some(rest.find('/').map_or("/", |at| &rest[at..]))
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 40 {
            break;
        }
    }
    let out = out.trim_matches('-');
    if out.is_empty() {
        "item".into()
    } else {
        out.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(grants: &[&str], scope: Value) -> AppContext {
        let mut app =
            AppContext::new("os.news", "/nonexistent").with_scope(scope::parse(&scope).unwrap());
        for g in grants {
            app = app.grant(*g);
        }
        app
    }

    fn names(app: &AppContext) -> Vec<String> {
        tool_decls(app, &Library::builtin().unwrap())
            .iter()
            .map(|d| d["name"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn registration_follows_the_grants() {
        assert!(names(&app(&[], json!({}))).is_empty());
        assert_eq!(
            names(&app(&["research"], json!({}))),
            [RUN, FORK, SEARCH, WEB_READ]
        );
        // `crawl` without crawl limits is no crawl.
        assert_eq!(names(&app(&["crawl"], json!({}))), Vec::<String>::new());
        assert_eq!(
            names(&app(&["crawl"], json!({"max_depth": 2, "max_pages": 10}))),
            [DEEP_CRAWL]
        );
        assert_eq!(
            names(&app(
                &["research", "crawl"],
                json!({"max_depth": 2, "max_pages": 10})
            )),
            [RUN, FORK, SEARCH, WEB_READ, DEEP_CRAWL]
        );
        // The limits alone grant nothing.
        assert!(names(&app(&[], json!({"max_depth": 2, "max_pages": 10}))).is_empty());
    }

    #[test]
    fn declarations_are_octos_tools_json_entries() {
        let decls = tool_decls(
            &app(
                &["research", "crawl"],
                json!({"max_depth": 2, "max_pages": 10}),
            ),
            &Library::builtin().unwrap(),
        );
        let allowed: BTreeSet<&str> = [
            "name",
            "description",
            "input_schema",
            "risk",
            "background",
            "outward",
            "confirm",
        ]
        .into();
        for d in &decls {
            let name = d["name"].as_str().unwrap();
            // `<app>.<tool>`: 2–4 segments of [a-z][a-z0-9_]{0,31}.
            let segments: Vec<&str> = name.split('.').collect();
            assert!((2..=4).contains(&segments.len()), "{name}");
            assert!(segments.iter().all(|s| s.len() <= 32
                && s.starts_with(|c: char| c.is_ascii_lowercase())
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')));
            assert!(
                d.as_object()
                    .unwrap()
                    .keys()
                    .all(|k| allowed.contains(k.as_str())),
                "{d}"
            );
            assert!(d["description"].as_str().unwrap().len() <= 2048, "{name}");
            assert_eq!(d["input_schema"]["type"], "object");
            let risk = if name == FORK { "act" } else { "read" };
            assert_eq!(d["risk"], risk, "{name}");
            assert_eq!(d["outward"], false);
            assert_eq!(d["background"], true);
            assert_eq!(d["confirm"], "host");
        }
        assert!(!decls
            .iter()
            .any(|d| d["name"].as_str().unwrap().contains("deep_research")));
        // The run tool lists the templates.
        assert!(decls[0]["description"]
            .as_str()
            .unwrap()
            .contains("news-digest"));
    }

    #[test]
    fn saved_names_are_slugged_and_relative() {
        assert_eq!(slug("https://Example.org/a b?c"), "https-example-org-a-b-c");
        assert_eq!(slug("台风"), "item");
        assert_eq!(url_path("https://example.org/a/b?c#d"), Some("/a/b"));
        assert_eq!(url_path("https://example.org"), Some("/"));
        assert_eq!(url_path("ftp://example.org/a"), None);
    }
}
