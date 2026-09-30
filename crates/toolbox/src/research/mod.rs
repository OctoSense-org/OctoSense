//! `mod.research` v1 on the host side.
//!
//! [`ResearchHost`] implements [`ToolboxHost`] for the `research` module over
//! two pluggable parts: a [`ResearchBackend`] that finds and reads sources
//! (the fixture backend in tests and evaluation; the interim [`live`] adapter;
//! the octos research engine, octos#2568, in `octos` behind the
//! `octos-engine` feature; later metasearch, octos#2582)
//! and a [`ModelClient`] the host supplies for `query` and `digest`.
//!
//! The policy lives here, once, whatever the backend:
//!
//! - every call is checked against the calling app's grant, octos's
//!   `Scope` ([`crate::scope`]): languages, regions, categories, domains,
//!   recency and results per search;
//! - search results get host-assigned ids; `article` reads only ids from this
//!   run's searches, so a template cannot fetch an arbitrary URL;
//! - evidence text stays in the host (capped, hashed); the script sees an
//!   excerpt and the evidence hash;
//! - `digest` takes only articles read in this run, cites them by id, and a
//!   model reply containing a URL or citing anything else is refused;
//! - the digest's summary may only restate its points ([`summary`]): a
//!   summary sentence none of whose key terms (numbers, names, CJK
//!   two-character sequences) appears in a kept point is dropped, and one
//!   carrying a number, name or phrase no point carries is flagged;
//! - relevance, whatever the backend ([`relevance`]): search results whose
//!   headline mentions the topic are listed first; `article` says
//!   `on_topic: false` when a page in the search's language does not mention
//!   the search's terms, and `digest` leaves such articles out; a `digest`
//!   task with a `focus` also asks the model, in the same call, which
//!   articles are not about it, and removes their citations;
//! - provenance (URL, title, source, retrieval time, evidence hash) is kept by
//!   the host and returned with each reply.

#[cfg(feature = "octos-engine")]
pub mod chrome;
#[cfg(feature = "live")]
pub mod live;
#[cfg(feature = "octos-engine")]
pub mod octos;
pub mod relevance;
pub mod summary;

use crate::host::{CallContext, HostError, HostFuture, HostReply, Provenance, Usage};
use crate::json::contains_url;
use crate::library::hex;
use crate::scope;
use crate::ToolboxHost;
use relevance::Topic;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

/// Evidence kept per article, in bytes (cut at a paragraph boundary).
pub const MAX_EVIDENCE_BYTES: usize = 6000;
/// The excerpt a script sees.
pub const MAX_EXCERPT_BYTES: usize = 400;
/// Feeds and API responses one `search` may fetch. Searches are not charged
/// to a run's `max_reads` (see [`crate::modules::MethodKind::Search`]); this
/// caps the fan-out instead.
pub const MAX_SEARCH_FETCHES: u32 = 8;

/// One search as the backend receives it, already within the app's scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    pub topic: String,
    pub language: Option<String>,
    pub region: Option<String>,
    pub limit: u32,
    pub max_age_hours: Option<u32>,
    /// Feeds and API responses the backend may fetch for this search, at
    /// most [`MAX_SEARCH_FETCHES`].
    pub max_fetches: u32,
    /// The metasearch category, when not the backend's default (`news`):
    /// `general`, `it`, `social`, `science`. Already within the grant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// One item a backend found. `via` names the provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundItem {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub via: String,
    /// Whether the backend can read this item. `false` when it knows up front
    /// that reading would fail (the interim adapter cannot resolve Google
    /// News article links without a browser). The host lists readable items
    /// first, and templates skip the rest instead of spending a read on them.
    #[serde(default = "readable_default", skip_serializing_if = "is_readable")]
    pub readable: bool,
    /// The provider's description or snippet, if any. Used only to judge
    /// relevance; never shown to the script.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub snippet: String,
}

fn readable_default() -> bool {
    true
}

fn is_readable(readable: &bool) -> bool {
    *readable
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResults {
    pub items: Vec<FoundItem>,
    pub providers: Vec<String>,
    /// Some provider failed or was skipped.
    pub partial: bool,
    /// Feeds and API responses fetched.
    pub fetches: u32,
    /// What the backend wants the run's diagnostics to say: a provider that
    /// failed or was skipped and why, items dropped as off topic.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// A configured RSS or Atom feed (a publisher's own feed, not a search
/// engine): the live backends keep only its items that mention the topic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub url: String,
    pub name: String,
    pub language: String,
}

/// A page's main text as a backend read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageText {
    pub text: String,
    #[serde(default)]
    pub title: Option<String>,
    /// Where the read ended, after HTTP and browser (JavaScript) redirects:
    /// for a Google News link, the publisher's page. `None` when the backend
    /// cannot tell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_url: Option<String>,
}

/// Finds and reads sources. The fixture backend, the interim live adapter,
/// and later the octos research engine.
pub trait ResearchBackend {
    fn search<'a>(
        &'a self,
        ctx: &'a CallContext,
        query: SearchQuery,
    ) -> HostFuture<'a, Result<SearchResults, HostError>>;

    fn read<'a>(
        &'a self,
        ctx: &'a CallContext,
        item: &'a FoundItem,
    ) -> HostFuture<'a, Result<PageText, HostError>>;

    /// Reads one page of a crawl (`deep_crawl`, the `crawl` capability): its
    /// main text, the URL it was finally read from, and the absolute
    /// `http(s)` links on it. Backends that cannot list links refuse, which
    /// is the default.
    fn read_links<'a>(
        &'a self,
        _ctx: &'a CallContext,
        _url: &'a str,
    ) -> HostFuture<'a, Result<LinkedPage, HostError>> {
        Box::pin(async {
            Err(HostError::Denied(
                "this research backend cannot crawl".into(),
            ))
        })
    }
}

/// One page of a crawl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedPage {
    /// Where the page was read from, after redirects.
    pub final_url: String,
    pub page: PageText,
    /// Absolute `http(s)` links, in page order, without fragments.
    pub links: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelTask {
    TranslateQuery,
    Digest,
}

/// One model call. The host owns the prompts; `user` is a JSON document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRequest {
    pub task: ModelTask,
    pub system: String,
    pub user: String,
    /// A cap on the reply's tokens, or `None` for the provider's default.
    /// The host sets none: a reasoning model spends part of any cap on
    /// thinking, and the output schema already bounds the reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// The JSON shape the reply must have.
    pub output_schema: Value,
}

/// The model the host supplies (the person's provider, chosen by the AI
/// host). Tests use [`crate::fixture::FakeModel`].
pub trait ModelClient {
    /// Returns the model's reply text (JSON).
    fn complete<'a>(
        &'a self,
        ctx: &'a CallContext,
        request: ModelRequest,
    ) -> HostFuture<'a, Result<String, HostError>>;
}

#[derive(Default)]
struct RunState {
    items: HashMap<String, FoundItem>,
    /// The searches (terms, language) each item was found under.
    topics: HashMap<String, Vec<(String, Option<String>)>>,
    articles: HashMap<String, ReadArticle>,
    /// Publishers that refused a read in this run (see [`refusal_code`]),
    /// by [`publisher_keys`], with the refusal's reason code.
    refused: HashMap<String, String>,
}

#[derive(Clone)]
struct ReadArticle {
    item: FoundItem,
    evidence: String,
    /// The text mentions the terms of a search that found it.
    on_topic: bool,
}

/// `mod.research` for the runner.
pub struct ResearchHost {
    backend: Arc<dyn ResearchBackend>,
    model: Arc<dyn ModelClient>,
    clock: Arc<dyn Fn() -> String>,
    runs: Mutex<HashMap<String, RunState>>,
}

impl ResearchHost {
    pub fn new(backend: Arc<dyn ResearchBackend>, model: Arc<dyn ModelClient>) -> Self {
        Self {
            backend,
            model,
            clock: Arc::new(|| {
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            }),
            runs: Mutex::new(HashMap::new()),
        }
    }

    /// Replaces the retrieval clock (fixtures use a fixed time).
    pub fn with_clock(mut self, clock: impl Fn() -> String + 'static) -> Self {
        self.clock = Arc::new(clock);
        self
    }

    fn state<T>(&self, run_id: &str, f: impl FnOnce(&mut RunState) -> T) -> T {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        f(runs.entry(run_id.to_owned()).or_default())
    }

    async fn query(&self, ctx: &CallContext, input: Value) -> Result<HostReply, HostError> {
        let query = input["query"].as_str().unwrap_or_default().to_owned();
        let language = input["language"].as_str().unwrap_or("en").to_owned();
        if !scope::allows_language(&ctx.app.scope, &language) {
            return Err(HostError::Denied(format!(
                "language {language} is outside the app's scope"
            )));
        }
        let request = ModelRequest {
            task: ModelTask::TranslateQuery,
            system: format!(
                "Translate the user's news search query into concise search terms in the \
                 language with BCP 47 tag {language}: the topic's key names and subjects only, \
                 as a news article in that language would write them, without words such as \
                 \"news\" or \"latest\". Keep names and numbers. Reply with JSON \
                 {{\"query\": \"...\"}} only. Do not add URLs."
            ),
            user: json!({"query": query, "language": language}).to_string(),
            max_output_tokens: None,
            output_schema: json!({"type": "object", "required": ["query"],
                "properties": {"query": {"type": "string", "minLength": 1, "maxLength": 160}}}),
        };
        let reply = self.model.complete(ctx, request).await?;
        let parsed = parse_model_json(&reply)?;
        let translated = parsed["query"]
            .as_str()
            .map(str::trim)
            .filter(|q| !q.is_empty() && q.chars().count() <= 160 && !q.contains('\n'))
            .ok_or_else(|| HostError::Failed("model output rejected: no usable query".into()))?;
        if contains_url(translated) {
            return Err(HostError::Failed(
                "model output rejected: it contains a URL".into(),
            ));
        }
        Ok(HostReply {
            output: json!({"query": translated, "language": language}),
            provenance: Vec::new(),
            usage: Usage {
                model_calls: 1,
                fetches: 0,
            },
            notes: Vec::new(),
        })
    }

    async fn search(&self, ctx: &CallContext, input: Value) -> Result<HostReply, HostError> {
        let scope = &ctx.app.scope;
        let topic = input["topic"].as_str().unwrap_or_default().to_owned();
        let language = input["language"].as_str().map(str::to_owned);
        let region = input["region"].as_str().map(str::to_owned);
        let limit = input["limit"].as_u64().unwrap_or(5).clamp(1, 10) as u32;
        let requested_age = input["max_age_hours"].as_u64().map(|h| h as u32);
        let category = input["category"]
            .as_str()
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_owned);
        // octos's `Scope::search_args` (or the thin parser's same rules):
        // a language, region or category outside the grant is refused,
        // recency and the result count are clamped.
        let narrowed = scope::narrow_search_in(
            scope,
            category.as_deref().unwrap_or(scope::SEARCH_CATEGORY),
            &topic,
            language.as_deref(),
            region.as_deref(),
            requested_age,
            limit,
        )
        .map_err(HostError::Denied)?;
        let limit = narrowed.limit;
        let query = SearchQuery {
            topic,
            language,
            region,
            limit,
            max_age_hours: narrowed.max_age_hours,
            max_fetches: MAX_SEARCH_FETCHES,
            category,
        };
        let topic = Topic::new(&query.topic);
        let mut results = self.backend.search(ctx, query.clone()).await?;
        // Readable items first; then articles before listing pages (a
        // site's topic or tag page is not a report); then by how many of
        // the topic's terms the headline or snippet names (a stable sort
        // keeps the backend's order otherwise), so `limit` fills with items
        // worth a read.
        results.items.sort_by_key(|item| {
            let named = topic.coverage(&format!("{} {}", item.title, item.snippet));
            (!item.readable, is_listing(&item.url), std::cmp::Reverse(named))
        });
        let queried_at = (self.clock)();
        let mut seen_urls = BTreeSet::new();
        let mut seen_titles = BTreeSet::new();
        let mut items = Vec::new();
        let mut provenance = Vec::new();

        for item in results.items {
            if !item.url.starts_with("https://") && !item.url.starts_with("http://") {
                continue;
            }
            if scope.check_domain(&item.url).is_err() {
                continue;
            }
            if !item.language.is_empty() && !scope::allows_language(scope, &item.language) {
                continue;
            }
            let title_key = item.title.trim().to_lowercase();
            if !seen_urls.insert(item.url.clone())
                || (!title_key.is_empty() && !seen_titles.insert(title_key))
            {
                continue;
            }
            if items.len() as u32 >= limit {
                break;
            }
            let id = item_id(&item.url);
            provenance.push(Provenance {
                id: id.clone(),
                url: item.url.clone(),
                title: item.title.clone(),
                source: item.source.clone(),
                language: item.language.clone(),
                published_at: item.published_at.clone(),
                retrieved_at: queried_at.clone(),
                evidence_sha256: None,
                resolved_url: None,
                publisher_host: None,
                via: format!("search:{}", item.via),
            });
            items.push(json!({
                "id": id,
                "title": clip(&item.title, 400),
                "url": item.url,
                "source": clip(&item.source, 200),
                "language": clip(&item.language, 16),
                "published_at": clip(&item.published_at, 40),
                "readable": item.readable,
            }));
            self.state(&ctx.run_id, |s| {
                let topics = s.topics.entry(id.clone()).or_default();
                let searched = (query.topic.clone(), query.language.clone());
                if !topics.contains(&searched) {
                    topics.push(searched);
                }
                s.items.insert(id, item);
            });
        }
        let mut providers = results.providers;
        providers.truncate(8);
        Ok(HostReply {
            output: json!({
                "items": items,
                "source": {"partial": results.partial, "providers": providers, "queried_at": queried_at},
            }),
            provenance,
            usage: Usage {
                model_calls: 0,
                fetches: results.fetches,
            },
            notes: narrowed.notes.into_iter().chain(results.notes).collect(),
        })
    }

    async fn article(&self, ctx: &CallContext, input: Value) -> Result<HostReply, HostError> {
        let id = input["id"].as_str().unwrap_or_default().to_owned();
        let Some(item) = self.state(&ctx.run_id, |s| s.items.get(&id).cloned()) else {
            return Err(HostError::Denied(format!(
                "{id} is not a result of this run's searches"
            )));
        };
        if let Err(why) = ctx.app.scope.check_domain(&item.url) {
            return Err(HostError::Denied(why));
        }
        // A publisher that refused an earlier read in this run (a bot
        // challenge or HTTP 403) is not asked again: the answer would be the
        // same, and a browser render costs seconds.
        let keys = publisher_keys(&item);
        let refused = self.state(&ctx.run_id, |s| {
            keys.iter()
                .find_map(|k| s.refused.get(k).map(|code| (k.clone(), code.clone())))
        });
        if let Some((key, code)) = refused {
            return Err(HostError::Failed(format!(
                "host_skipped: {key} refused an earlier read in this run ({code}); not read"
            )));
        }
        let page = match self.backend.read(ctx, &item).await {
            Ok(page) => page,
            Err(err) => {
                if let HostError::Failed(message) = &err {
                    if let Some(code) = refusal_code(message) {
                        let mut keys = keys;
                        keys.extend(
                            final_url(message)
                                .and_then(host_of)
                                .map(|h| format!("host {h}")),
                        );
                        self.state(&ctx.run_id, |s| {
                            for key in keys {
                                s.refused.entry(key).or_insert_with(|| code.to_owned());
                            }
                        });
                    }
                }
                return Err(err);
            }
        };
        let (evidence, truncated) = cap_evidence(&page.text, MAX_EVIDENCE_BYTES);
        if evidence.trim().is_empty() {
            return Err(HostError::Failed("no main text".into()));
        }
        // The relevance gate: the page must mention the terms of a search
        // that found it (the whole page, not only the evidence kept). Only
        // searches in the page's language judge it: a page in another
        // language cannot be expected to use the search's words, and is left
        // to the digest's model check.
        let topics: Vec<Topic> = self
            .state(&ctx.run_id, |s| s.topics.get(&id).cloned())
            .unwrap_or_default()
            .iter()
            .filter(|(_, language)| {
                language
                    .as_deref()
                    .is_none_or(|l| item.language.is_empty() || same_language(l, &item.language))
            })
            .map(|(terms, _)| Topic::new(terms))
            .collect();
        let on_topic = Topic::mentioned_in_any(
            &topics,
            &format!(
                "{}\n{}\n{}",
                item.title,
                page.title.as_deref().unwrap_or_default(),
                page.text
            ),
        );
        let mut notes = Vec::new();
        if !on_topic {
            let terms: Vec<String> = topics.iter().flat_map(Topic::terms).collect();
            notes.push(format!(
                "{id}: off topic, the page does not mention {}; not digested",
                terms.join(" + ")
            ));
        }
        let hash = hex(&Sha256::digest(evidence.as_bytes()));
        let retrieved_at = (self.clock)();
        let title = if item.title.is_empty() {
            page.title.clone().unwrap_or_default()
        } else {
            item.title.clone()
        };
        let (resolved_url, publisher_host) = resolve(&item.url, page.final_url.as_deref());
        let output = json!({
            "id": id,
            "title": clip(&title, 400),
            "url": item.url,
            "resolved_url": resolved_url,
            "publisher_host": publisher_host,
            "source": clip(&item.source, 200),
            "language": clip(&item.language, 16),
            "published_at": clip(&item.published_at, 40),
            "excerpt": clip(&evidence, MAX_EXCERPT_BYTES),
            "chars": evidence.chars().count(),
            "truncated": truncated,
            "evidence_sha256": hash,
            "on_topic": on_topic,
        });
        let provenance = vec![Provenance {
            id: id.clone(),
            url: item.url.clone(),
            title,
            source: item.source.clone(),
            language: item.language.clone(),
            published_at: item.published_at.clone(),
            retrieved_at,
            evidence_sha256: Some(hash),
            resolved_url,
            publisher_host,
            via: "article".into(),
        }];
        self.state(&ctx.run_id, |s| {
            s.articles.insert(
                id,
                ReadArticle {
                    item,
                    evidence,
                    on_topic,
                },
            )
        });
        Ok(HostReply {
            output,
            provenance,
            usage: Usage {
                model_calls: 0,
                fetches: 1,
            },
            notes,
        })
    }

    async fn digest(&self, ctx: &CallContext, input: Value) -> Result<HostReply, HostError> {
        let task = input["task"].as_str().unwrap_or("digest").to_owned();
        let language = input["language"].as_str().unwrap_or("en").to_owned();
        if !scope::allows_language(&ctx.app.scope, &language) {
            return Err(HostError::Denied(format!(
                "language {language} is outside the app's scope"
            )));
        }
        let focus = input["focus"].as_str().unwrap_or_default().to_owned();
        if contains_url(&focus) {
            return Err(HostError::Denied("focus may not contain a URL".into()));
        }
        let ids: Vec<String> = input["article_ids"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let mut articles = Vec::new();
        let mut notes = Vec::new();
        let mut given = Vec::new();
        for id in &ids {
            let Some(article) = self.state(&ctx.run_id, |s| s.articles.get(id).cloned()) else {
                return Err(HostError::Denied(format!("{id} was not read in this run")));
            };
            // The relevance gate holds whatever the template passes.
            if !article.on_topic {
                notes.push(format!("digest: {id} left out: off topic"));
                continue;
            }
            given.push(id.clone());
            articles.push(json!({
                "id": id,
                "title": article.item.title,
                "source": article.item.source,
                "language": article.item.language,
                "published_at": article.item.published_at,
                "text": article.evidence,
            }));
        }
        if given.is_empty() {
            return Err(HostError::Failed(
                "no article given is about the topic".into(),
            ));
        }
        // The model checks relevance for news digests with a focus (the
        // other tasks use the focus to name their subjects).
        let focused = task == "digest" && !focus.trim().is_empty();
        let request = ModelRequest {
            task: ModelTask::Digest,
            system: digest_prompt(&task, &language, focused),
            user: json!({"task": task, "language": language, "focus": focus, "articles": articles})
                .to_string(),
            max_output_tokens: None,
            output_schema: json!({"type": "object", "required": ["summary", "points"],
            "properties": {
                "summary": {"type": "string", "minLength": 1, "maxLength": 1200},
                "points": {"type": "array", "minItems": 1, "maxItems": 12, "items": {
                    "type": "object", "required": ["text", "citations"],
                    "properties": {
                        "text": {"type": "string", "minLength": 1, "maxLength": 400},
                        "citations": {"type": "array", "minItems": 1, "maxItems": 8, "items": {"type": "string"}},
                        "label": {"type": "string", "maxLength": 40}
                    }}},
                "off_topic": {"type": "array", "maxItems": 8, "items": {"type": "string"}}
            }}),
        };
        let reply = self.model.complete(ctx, request).await?;
        let parsed = parse_model_json(&reply)?;
        // The model's own relevance check, folded into this one call: the
        // articles it marks as not about the focus are left out.
        let mut off_topic: Vec<String> = Vec::new();
        if focused {
            for id in parsed["off_topic"].as_array().into_iter().flatten() {
                if let Some(id) = id.as_str().filter(|id| given.iter().any(|g| g == id)) {
                    if !off_topic.iter().any(|o| o == id) {
                        off_topic.push(id.to_owned());
                    }
                }
            }
        }
        if !off_topic.is_empty() {
            notes.push(format!(
                "digest: the model marked {} of {} articles off topic: {}",
                off_topic.len(),
                given.len(),
                off_topic.join(", ")
            ));
        }
        if off_topic.len() == given.len() {
            return Err(HostError::Failed(
                "the model found no article about the topic".into(),
            ));
        }
        let (mut output, point_notes) =
            validate_digest(&parsed, &task, &language, &given, &off_topic)?;
        notes.extend(point_notes);
        output["off_topic"] = json!(off_topic);
        Ok(HostReply {
            output,
            provenance: Vec::new(),
            usage: Usage {
                model_calls: 1,
                fetches: 0,
            },
            notes,
        })
    }
}

impl ToolboxHost for ResearchHost {
    fn call<'a>(
        &'a self,
        ctx: CallContext,
        module: &'a str,
        method: &'a str,
        input: Value,
    ) -> HostFuture<'a, Result<HostReply, HostError>> {
        Box::pin(async move {
            if module != "research" {
                return Err(HostError::Denied(format!("no host module mod.{module}")));
            }
            if !ctx.app.grants.contains("research") {
                return Err(HostError::Denied("research is not granted".into()));
            }
            match method {
                "query" => self.query(&ctx, input).await,
                "search" => self.search(&ctx, input).await,
                "article" => self.article(&ctx, input).await,
                "digest" => self.digest(&ctx, input).await,
                _ => Err(HostError::Denied(format!("no method research.{method}"))),
            }
        })
    }

    fn finish_run(&self, run_id: &str) {
        self.runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(run_id);
    }
}

/// Whether two BCP 47 tags share their primary language (`zh`, `zh-CN`).
pub fn same_language(a: &str, b: &str) -> bool {
    let primary = |t: &str| {
        t.split(['-', '_'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    };
    primary(a) == primary(b)
}

/// The host-assigned id of a found item: stable for a URL, so recorded
/// fixtures and both sides of an evaluation see the same ids.
pub fn item_id(url: &str) -> String {
    format!("s{}", &hex(&Sha256::digest(url.as_bytes()))[..12])
}

fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// Caps evidence at `max` bytes, cutting at the last paragraph (or line,
/// or character) boundary that fits.
pub fn cap_evidence(text: &str, max: usize) -> (String, bool) {
    let text = text.trim();
    if text.len() <= max {
        return (text.to_owned(), false);
    }
    let head = clip(text, max);
    let cut = head
        .rfind("\n\n")
        .or_else(|| head.rfind('\n'))
        .filter(|&at| at > max / 2)
        .unwrap_or(head.len());
    (head[..cut].trim_end().to_owned(), true)
}

fn digest_prompt(task: &str, language: &str, focused: bool) -> String {
    let what = match task {
        "brief" => "a short briefing across the topics, one or two points per topic",
        "plan" => "a practical plan for the person based on what the sources say, with caveats where the sources are uncertain",
        "compare" => "a comparison of the subjects named in the focus; label each point with the subject it is about, or \"both\"",
        _ => "a digest of the news: the key facts, one point per development",
    };
    let relevance = if focused {
        "The `focus` is what the person asked about. First check each article: if it is not \
         about the focus, put its id in `off_topic` and use nothing from it. "
    } else {
        ""
    };
    let shape = if focused {
        ", \"off_topic\": [article id]"
    } else {
        ""
    };
    format!(
        "{relevance}You write {what}. Write in the language with BCP 47 tag {language}, translating the \
         sources as needed. Use only facts stated in the articles you are given. Every point \
         cites the ids of the articles it rests on in `citations`. The summary only restates \
         your points: every statement in it must be in a point, and it adds no fact, number, \
         date, name, cause, forecast, quote or conclusion of its own; the host removes summary \
         sentences that no point backs. Never write URLs or invent sources. Keep within these limits, or the host drops what exceeds them: a summary of \
         at most 1000 characters; at most {MAX_POINTS} points; each point one or two sentences \
         of at most 300 characters, citing 1 to {MAX_CITATIONS} articles; a label of at most \
         30 characters. Reply with JSON only: {{\"summary\": string, \"points\": [{{\"text\": \
         string, \"citations\": [article id], \"label\": optional string}}]{shape}}}."
    )
}

/// Parses a model reply as JSON. A reply wrapped in a code fence, or with a
/// line of prose around the object, is accepted: the object between the first
/// `{` and the last `}` is tried when the whole reply is not JSON. What it
/// contains is validated afterwards either way.
fn parse_model_json(reply: &str) -> Result<Value, HostError> {
    let trimmed = reply.trim();
    let body = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|s| s.strip_suffix("```"))
        .unwrap_or(trimmed)
        .trim();
    serde_json::from_str(body).or_else(|e| {
        match (body.find('{'), body.rfind('}')) {
            (Some(start), Some(end)) if start < end => {
                serde_json::from_str(&body[start..=end]).ok()
            }
            _ => None,
        }
        .ok_or_else(|| HostError::Failed(format!("model output rejected: not JSON ({e})")))
    })
}

/// The most points a digest keeps.
const MAX_POINTS: usize = 12;
/// The most citations one point may carry.
const MAX_CITATIONS: usize = 8;

/// Checks a model's digest.
///
/// The summary is checked first: a digest without a valid summary, or whose
/// summary carries a URL, is refused. Each point is then checked on its own.
/// One with no text or too much, no citation or too many, a citation to an
/// article it was not given, or a URL is dropped, with a note, and the rest
/// are kept. A label that is too long or carries a URL is dropped from its
/// point. A digest with no valid point left is refused. A point citing only
/// articles marked off topic is removed too, but is not counted as dropped.
/// The summary is then
/// checked against the kept points ([`summary::check_summary`]): sentences no
/// point backs are dropped, sentences carrying a number, name or phrase no
/// point carries are flagged, and a summary with no sentence left is
/// refused. The output counts what was dropped (`dropped_points`,
/// `summary_check`), so a template can tell a complete digest from one the
/// host had to cut.
///
/// The security properties hold for every point kept: it cites only the ids
/// in `ids` (articles read in this run), and no string in the output carries
/// a URL. Notes never quote the model's text.
fn validate_digest(
    parsed: &Value,
    task: &str,
    language: &str,
    ids: &[String],
    off_topic: &[String],
) -> Result<(Value, Vec<String>), HostError> {
    let reject = |why: &str| HostError::Failed(format!("model output rejected: {why}"));
    let summary = parsed["summary"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.chars().count() <= 1200)
        .ok_or_else(|| reject("no summary"))?;
    if contains_url(summary) {
        return Err(reject("the summary contains a URL"));
    }
    let points = parsed["points"]
        .as_array()
        .filter(|p| !p.is_empty())
        .ok_or_else(|| reject("no points"))?;
    let known: BTreeSet<&str> = ids.iter().map(String::as_str).collect();
    let excluded: BTreeSet<&str> = off_topic.iter().map(String::as_str).collect();
    let mut notes = Vec::new();
    let mut kept = Vec::new();
    // Points about articles marked off topic are removed by design; only
    // the invalid ones count as dropped.
    let mut off_topic_points = 0;
    for (index, point) in points.iter().enumerate() {
        let n = index + 1;
        if kept.len() == MAX_POINTS {
            notes.push(format!(
                "digest point {n} dropped: more than {MAX_POINTS} points"
            ));
            continue;
        }
        match check_point(point, &known, &excluded) {
            Ok((record, label_note)) => {
                if let Some(why) = label_note {
                    notes.push(format!("digest point {n}: label dropped: {why}"));
                }
                kept.push(record);
            }
            Err(why) => {
                if why == OFF_TOPIC_ONLY {
                    off_topic_points += 1;
                }
                notes.push(format!("digest point {n} dropped: {why}"))
            }
        }
    }
    if kept.is_empty() {
        return Err(reject(&format!("no valid point ({})", notes.join("; "))));
    }
    let dropped_points = points.len() - kept.len() - off_topic_points;
    // The summary may only restate the kept points.
    let texts: Vec<&str> = kept.iter().filter_map(|p| p["text"].as_str()).collect();
    let check = summary::check_summary(summary, &texts);
    for n in &check.dropped {
        notes.push(format!(
            "digest summary: sentence {n} of {} dropped: no point backs it",
            check.sentences
        ));
    }
    for n in &check.flagged {
        notes.push(format!(
            "digest summary: sentence {n} of {} kept but flagged: it carries a number, name or phrase no point carries",
            check.sentences
        ));
    }
    if check.summary.is_empty() {
        return Err(reject("no summary sentence restates a point"));
    }
    let output = json!({
        "task": task,
        "language": language,
        "summary": check.summary,
        "points": kept,
        "dropped_points": dropped_points,
        "summary_check": {
            "sentences": check.sentences,
            "dropped": check.dropped.len(),
            "flagged": check.flagged.len(),
        },
    });
    // Each part was checked above; this guards the assembled whole.
    if crate::json::strings(&output).into_iter().any(contains_url) {
        return Err(reject("it contains a URL"));
    }
    Ok((output, notes))
}

const OFF_TOPIC_ONLY: &str = "it cites only articles marked off topic";

/// One digest point: the record to keep and, if its label was dropped, why;
/// or why the point is dropped.
fn check_point(
    point: &Value,
    known: &BTreeSet<&str>,
    excluded: &BTreeSet<&str>,
) -> Result<(Value, Option<&'static str>), &'static str> {
    let text = point["text"]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or("it has no text")?;
    if text.chars().count() > 400 {
        return Err("its text is longer than 400 characters");
    }
    if contains_url(text) {
        return Err("its text contains a URL");
    }
    let raw = point["citations"].as_array().ok_or("it cites nothing")?;
    let mut citations: Vec<&str> = Vec::new();
    let mut cites_off_topic = false;
    for citation in raw {
        let id = citation.as_str().ok_or("a citation is not an article id")?;
        if !known.contains(id) {
            return Err("it cites an article it was not given");
        }
        if excluded.contains(id) {
            // A citation of an article marked off topic is removed.
            cites_off_topic = true;
        } else if !citations.contains(&id) {
            citations.push(id);
        }
    }
    if citations.is_empty() && cites_off_topic {
        return Err(OFF_TOPIC_ONLY);
    }
    if citations.is_empty() {
        return Err("it cites nothing");
    }
    if citations.len() > MAX_CITATIONS {
        return Err("it cites more than 8 articles");
    }
    let mut record = json!({"text": text, "citations": citations});
    let mut label_note = None;
    if let Some(label) = point["label"]
        .as_str()
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        if label.chars().count() > 40 {
            label_note = Some("longer than 40 characters");
        } else if contains_url(label) {
            label_note = Some("it contains a URL");
        } else {
            record["label"] = json!(label);
        }
    }
    Ok((record, label_note))
}

/// Hosts whose article links stand for many publishers (the publisher is
/// known only after the read resolves the link), so they never identify one.
const AGGREGATOR_HOSTS: &[&str] = &["news.google.com"];

/// Read failures that say the publisher refuses this reader, as octos's
/// reason codes (octos#2590): the same publisher would refuse the next read
/// too. Other failures (no main text, a timeout) may be the page's alone.
fn refusal_code(message: &str) -> Option<&'static str> {
    ["bot_challenge", "http_403"]
        .into_iter()
        .find(|code| message.starts_with(code))
}

/// The page a failed read ended on, from octos's `(final URL: …)` suffix.
fn final_url(message: &str) -> Option<&str> {
    let start = message.rfind("(final URL: ")? + "(final URL: ".len();
    message[start..].split(')').next()
}

/// A URL's host, lowercased and without `www.`.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?.split(':').next()?;
    let host = host.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_owned();
    (!host.is_empty()).then_some(host)
}

/// Where an item's read ended (`resolved_url`) and its publisher's host.
/// A read that ended on an aggregator (still a Google News link) is not
/// resolved; the publisher host then comes from the item's own link unless
/// that is an aggregator's too.
fn resolve(url: &str, final_url: Option<&str>) -> (Option<String>, Option<String>) {
    let not_aggregator =
        |u: &&str| host_of(u).is_some_and(|h| !AGGREGATOR_HOSTS.contains(&h.as_str()));
    let resolved = final_url
        .map(str::trim)
        .filter(|u| u.starts_with("http://") || u.starts_with("https://"))
        .filter(not_aggregator)
        .filter(|u| u.len() <= 2048)
        .map(str::to_owned);
    let host = resolved
        .as_deref()
        .or(Some(url).filter(not_aggregator))
        .and_then(host_of);
    (resolved, host)
}

/// What identifies an item's publisher before it is read: its source name,
/// and its link's host unless that is an aggregator's.
fn publisher_keys(item: &FoundItem) -> Vec<String> {
    let mut keys = Vec::new();
    let source = item.source.trim();
    if !source.is_empty() {
        keys.push(format!("source \"{source}\""));
    }
    if let Some(host) = host_of(&item.url).filter(|h| !AGGREGATOR_HOSTS.contains(&h.as_str())) {
        keys.push(format!("host {host}"));
    }
    keys
}

/// A site's topic, tag, category or section page, or its home page: a
/// list of reports rather than one.
fn is_listing(url: &str) -> bool {
    const LISTING: &[&str] = &["topic", "topics", "tag", "tags", "category", "categories", "section", "sections", "hub"];
    let Some((_, rest)) = url.split_once("://") else {
        return false;
    };
    let path = rest.find('/').map_or("", |at| &rest[at..]);
    let path = path.split(['?', '#']).next().unwrap_or("");
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segments.is_empty() || segments.iter().any(|s| LISTING.contains(&s.to_ascii_lowercase().as_str()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn listing_pages_are_told_from_reports() {
        for listing in [
            "https://hbr.org/topic/subject/ai-and-machine-learning",
            "https://www.example.com/",
            "https://news.example.org/Tags/phones",
            "https://www.example.com/category/tech/",
        ] {
            assert!(is_listing(listing), "{listing}");
        }
        for report in [
            "https://www.upi.com/Top_News/World-News/2026/09/29/kakao-ai-safety-institute/4251790722100",
            "https://example.com/2026/09/29/topical-review",
            "https://github.com/TencentCloudBase/skills",
        ] {
            assert!(!is_listing(report), "{report}");
        }
    }

    use super::*;

    #[test]
    fn google_news_links_resolve_only_off_the_aggregator() {
        let gn = "https://news.google.com/rss/articles/CBMi?oc=5";
        assert_eq!(resolve(gn, None), (None, None));
        assert_eq!(resolve(gn, Some(gn)), (None, None));
        assert_eq!(
            resolve(gn, Some("https://www.startupfortune.com/dhh/")),
            (
                Some("https://www.startupfortune.com/dhh/".into()),
                Some("startupfortune.com".into())
            )
        );
        assert_eq!(
            resolve("https://bbc.co.uk/a", None),
            (None, Some("bbc.co.uk".into()))
        );
    }

    #[test]
    fn refusals_mark_the_publisher_and_the_host_the_read_ended_on() {
        let bot = "bot_challenge: a DataDome challenge (not bypassed) (final URL: https://www.nytimes.com/a/b.html?x=1)";
        assert_eq!(refusal_code(bot), Some("bot_challenge"));
        assert_eq!(
            refusal_code("http_403: stated by the page"),
            Some("http_403")
        );
        assert_eq!(refusal_code("no_main_text: rendered"), None);
        assert_eq!(refusal_code("HTTP 403"), None);
        assert_eq!(
            final_url(bot).and_then(host_of).as_deref(),
            Some("nytimes.com")
        );
        assert_eq!(final_url("bot_challenge: blocked by a bot challenge"), None);
        assert_eq!(
            host_of("https://user@News.CN:443/x").as_deref(),
            Some("news.cn")
        );
        let item = |url: &str, source: &str| FoundItem {
            url: url.into(),
            title: String::new(),
            source: source.into(),
            language: String::new(),
            published_at: String::new(),
            via: String::new(),
            readable: true,
            snippet: String::new(),
        };
        assert_eq!(
            publisher_keys(&item(
                "https://news.google.com/rss/articles/x",
                "The New York Times"
            )),
            ["source \"The New York Times\""]
        );
        assert_eq!(
            publisher_keys(&item("https://www.nytimes.com/a", " ")),
            ["host nytimes.com"]
        );
    }

    #[test]
    fn evidence_is_capped_at_paragraphs() {
        let text = format!("{}\n\n{}", "a".repeat(4000), "b".repeat(4000));
        let (capped, truncated) = cap_evidence(&text, 6000);
        assert!(truncated);
        assert_eq!(capped, "a".repeat(4000));
        let (whole, truncated) = cap_evidence("short", 6000);
        assert_eq!((whole.as_str(), truncated), ("short", false));
    }

    #[test]
    fn digests_cite_only_given_articles_and_carry_no_urls() {
        let ids = vec!["s1".to_owned()];
        let ok = json!({"summary": "S", "points": [{"text": "T", "citations": ["s1"]}]});
        let (digest, notes) = validate_digest(&ok, "digest", "en", &ids, &[]).unwrap();
        assert_eq!(digest["points"].as_array().unwrap().len(), 1);
        assert!(notes.is_empty());
        // A digest whose only point cites a foreign article has nothing left.
        let foreign = json!({"summary": "S", "points": [{"text": "T", "citations": ["s9"]}]});
        let error = validate_digest(&foreign, "digest", "en", &ids, &[]).unwrap_err();
        assert!(error.to_string().contains("no valid point"), "{error}");
        // A URL in the summary refuses the whole digest.
        let url = json!({"summary": "see https://example.org", "points": [{"text": "T", "citations": ["s1"]}]});
        assert!(validate_digest(&url, "digest", "en", &ids, &[]).is_err());
        let blank = json!({"summary": " ", "points": [{"text": "T", "citations": ["s1"]}]});
        assert!(validate_digest(&blank, "digest", "en", &ids, &[]).is_err());
    }

    #[test]
    fn invalid_points_are_dropped_and_valid_ones_kept() {
        let ids = vec!["s1".to_owned(), "s2".to_owned()];
        let long = "x".repeat(401);
        let reply = json!({"summary": "S", "points": [
            {"text": "Kept.", "citations": ["s1", "s1", "s2"]},
            {"text": "", "citations": ["s1"]},
            {"citations": ["s1"]},
            {"text": "Invented.", "citations": ["s1", "s9"]},
            {"text": "More at https://model.invalid/story", "citations": ["s1"]},
            {"text": "More at www.model.invalid", "citations": ["s2"]},
            {"text": "No sources.", "citations": []},
            {"text": "Bad citation.", "citations": [3]},
            {"text": long, "citations": ["s1"]},
            {"text": "Kept, label dropped.", "citations": ["s2"], "label": "see https://x.invalid"},
            {"text": "Kept with label.", "citations": ["s2"], "label": "Label"}
        ]});
        let (digest, notes) = validate_digest(&reply, "digest", "en", &ids, &[]).unwrap();
        let points = digest["points"].as_array().unwrap();
        assert_eq!(points.len(), 3, "{digest}");
        assert_eq!(points[0]["citations"], json!(["s1", "s2"]));
        assert!(points[1].get("label").is_none());
        assert_eq!(points[2]["label"], "Label");
        assert_eq!(
            notes.iter().filter(|n| n.contains("dropped:")).count(),
            9,
            "{notes:?}"
        );
        assert!(notes
            .iter()
            .any(|n| n.starts_with("digest point 2 dropped: it has no text")));
        assert!(notes
            .iter()
            .any(|n| n.starts_with("digest point 4 dropped: it cites an article")));
        assert!(notes
            .iter()
            .any(|n| n.starts_with("digest point 5 dropped: its text contains a URL")));
        assert!(notes
            .iter()
            .any(|n| n.starts_with("digest point 10: label dropped")));
        // Notes never quote the model.
        assert!(!notes
            .iter()
            .any(|n| n.contains("model.invalid") || n.contains("s9")));
    }

    #[test]
    fn citations_of_off_topic_articles_are_removed() {
        let ids = vec!["s1".to_owned(), "s2".to_owned()];
        let reply = json!({"summary": "S", "points": [
            {"text": "Both.", "citations": ["s1", "s2"]},
            {"text": "Off topic only.", "citations": ["s2"]}
        ]});
        let (digest, notes) =
            validate_digest(&reply, "digest", "en", &ids, &["s2".to_owned()]).unwrap();
        assert_eq!(
            digest["points"],
            json!([{"text": "Both.", "citations": ["s1"]}])
        );
        assert_eq!(
            notes,
            vec!["digest point 2 dropped: it cites only articles marked off topic"]
        );
    }

    #[test]
    fn a_digest_keeps_at_most_twelve_points() {
        let ids = vec!["s1".to_owned()];
        let points: Vec<Value> = (0..14)
            .map(|i| json!({"text": format!("Point {i}."), "citations": ["s1"]}))
            .collect();
        let reply = json!({"summary": "S", "points": points});
        let (digest, notes) = validate_digest(&reply, "digest", "en", &ids, &[]).unwrap();
        assert_eq!(digest["points"].as_array().unwrap().len(), 12);
        assert_eq!(notes.len(), 2);
    }

    #[test]
    fn model_json_may_be_fenced_or_wrapped_in_prose() {
        assert_eq!(parse_model_json("{\"a\": 1}").unwrap(), json!({"a": 1}));
        assert_eq!(
            parse_model_json("```json\n{\"a\": 1}\n```").unwrap(),
            json!({"a": 1})
        );
        assert_eq!(
            parse_model_json("Here is the digest:\n{\"a\": {\"b\": 2}}\nDone.").unwrap(),
            json!({"a": {"b": 2}})
        );
        assert!(parse_model_json("no json here").is_err());
        assert!(parse_model_json("{\"a\": ").is_err());
    }
}
