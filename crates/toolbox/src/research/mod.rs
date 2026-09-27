//! `mod.research` v1 on the host side.
//!
//! [`ResearchHost`] implements [`ToolboxHost`] for the `research` module over
//! two pluggable parts: a [`ResearchBackend`] that finds and reads sources
//! (the fixture backend in tests and evaluation; the interim [`live`] adapter;
//! later the octos research engine, octos#2568, and metasearch, octos#2576)
//! and a [`ModelClient`] the host supplies for `query` and `digest`.
//!
//! The policy lives here, once, whatever the backend:
//!
//! - every call is checked against the calling app's scope (languages,
//!   regions, allowed and denied domains, recency);
//! - search results get host-assigned ids; `article` reads only ids from this
//!   run's searches, so a template cannot fetch an arbitrary URL;
//! - evidence text stays in the host (capped, hashed); the script sees an
//!   excerpt and the evidence hash;
//! - `digest` takes only articles read in this run, cites them by id, and a
//!   model reply containing a URL or citing anything else is refused;
//! - provenance (URL, title, source, retrieval time, evidence hash) is kept by
//!   the host and returned with each reply.

#[cfg(feature = "live")]
pub mod live;

use crate::host::{CallContext, HostError, HostFuture, HostReply, Provenance, Usage};
use crate::json::contains_url;
use crate::library::hex;
use crate::ToolboxHost;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

/// Evidence kept per article, in bytes (cut at a paragraph boundary).
pub const MAX_EVIDENCE_BYTES: usize = 6000;
/// The excerpt a script sees.
pub const MAX_EXCERPT_BYTES: usize = 400;

/// One search as the backend receives it, already within the app's scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    pub topic: String,
    pub language: Option<String>,
    pub region: Option<String>,
    pub limit: u32,
    pub max_age_hours: Option<u32>,
    /// Pages (feeds, API responses) the backend may fetch for this search.
    pub max_pages: u32,
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
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResults {
    pub items: Vec<FoundItem>,
    pub providers: Vec<String>,
    /// Some provider failed or was skipped.
    pub partial: bool,
    /// Pages fetched.
    pub pages: u32,
}

/// A page's main text as a backend read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageText {
    pub text: String,
    #[serde(default)]
    pub title: Option<String>,
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
    articles: HashMap<String, ReadArticle>,
}

#[derive(Clone)]
struct ReadArticle {
    item: FoundItem,
    evidence: String,
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
        if !ctx.app.scope.allows_language(&language) {
            return Err(HostError::Denied(format!(
                "language {language} is outside the app's scope"
            )));
        }
        let request = ModelRequest {
            task: ModelTask::TranslateQuery,
            system: format!(
                "Translate the user's news search query into concise search terms in the \
                 language with BCP 47 tag {language}. Keep names and numbers. Reply with JSON \
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
            .filter(|q| !q.is_empty() && q.len() <= 160 && !q.contains('\n'))
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
                pages: 0,
            },
        })
    }

    async fn search(&self, ctx: &CallContext, input: Value) -> Result<HostReply, HostError> {
        let scope = &ctx.app.scope;
        let language = input["language"].as_str().map(str::to_owned);
        let region = input["region"].as_str().map(str::to_owned);
        if let Some(l) = &language {
            if !scope.allows_language(l) {
                return Err(HostError::Denied(format!(
                    "language {l} is outside the app's scope"
                )));
            }
        }
        if let Some(r) = &region {
            if !scope.allows_region(r) {
                return Err(HostError::Denied(format!(
                    "region {r} is outside the app's scope"
                )));
            }
        }
        let limit = input["limit"].as_u64().unwrap_or(5).clamp(1, 10) as u32;
        let requested_age = input["max_age_hours"].as_u64().map(|h| h as u32);
        let max_age_hours = match (requested_age, scope.recency_hours) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let query = SearchQuery {
            topic: input["topic"].as_str().unwrap_or_default().to_owned(),
            language,
            region,
            limit,
            max_age_hours,
            max_pages: ctx.remaining.pages.max(1),
        };
        let results = self.backend.search(ctx, query).await?;
        let queried_at = (self.clock)();
        let mut seen_urls = BTreeSet::new();
        let mut seen_titles = BTreeSet::new();
        let mut items = Vec::new();
        let mut provenance = Vec::new();

        for item in results.items {
            if !item.url.starts_with("https://") && !item.url.starts_with("http://") {
                continue;
            }
            if !scope.allows_url(&item.url) {
                continue;
            }
            if !item.language.is_empty() && !scope.allows_language(&item.language) {
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
                via: format!("search:{}", item.via),
            });
            items.push(json!({
                "id": id,
                "title": clip(&item.title, 400),
                "url": item.url,
                "source": clip(&item.source, 200),
                "language": clip(&item.language, 16),
                "published_at": clip(&item.published_at, 40),
            }));
            self.state(&ctx.run_id, |s| s.items.insert(id, item));
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
                pages: results.pages.max(1),
            },
        })
    }

    async fn article(&self, ctx: &CallContext, input: Value) -> Result<HostReply, HostError> {
        let id = input["id"].as_str().unwrap_or_default().to_owned();
        let Some(item) = self.state(&ctx.run_id, |s| s.items.get(&id).cloned()) else {
            return Err(HostError::Denied(format!(
                "{id} is not a result of this run's searches"
            )));
        };
        if !ctx.app.scope.allows_url(&item.url) {
            return Err(HostError::Denied("outside the app's scope".into()));
        }
        let page = self.backend.read(ctx, &item).await?;
        let (evidence, truncated) = cap_evidence(&page.text, MAX_EVIDENCE_BYTES);
        if evidence.trim().is_empty() {
            return Err(HostError::Failed("no main text".into()));
        }
        let hash = hex(&Sha256::digest(evidence.as_bytes()));
        let retrieved_at = (self.clock)();
        let title = if item.title.is_empty() {
            page.title.clone().unwrap_or_default()
        } else {
            item.title.clone()
        };
        let output = json!({
            "id": id,
            "title": clip(&title, 400),
            "url": item.url,
            "source": clip(&item.source, 200),
            "language": clip(&item.language, 16),
            "published_at": clip(&item.published_at, 40),
            "excerpt": clip(&evidence, MAX_EXCERPT_BYTES),
            "chars": evidence.chars().count(),
            "truncated": truncated,
            "evidence_sha256": hash,
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
            via: "article".into(),
        }];
        self.state(&ctx.run_id, |s| {
            s.articles.insert(id, ReadArticle { item, evidence })
        });
        Ok(HostReply {
            output,
            provenance,
            usage: Usage {
                model_calls: 0,
                pages: 1,
            },
        })
    }

    async fn digest(&self, ctx: &CallContext, input: Value) -> Result<HostReply, HostError> {
        let task = input["task"].as_str().unwrap_or("digest").to_owned();
        let language = input["language"].as_str().unwrap_or("en").to_owned();
        if !ctx.app.scope.allows_language(&language) {
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
        for id in &ids {
            let Some(article) = self.state(&ctx.run_id, |s| s.articles.get(id).cloned()) else {
                return Err(HostError::Denied(format!("{id} was not read in this run")));
            };
            articles.push(json!({
                "id": id,
                "title": article.item.title,
                "source": article.item.source,
                "language": article.item.language,
                "published_at": article.item.published_at,
                "text": article.evidence,
            }));
        }
        let request = ModelRequest {
            task: ModelTask::Digest,
            system: digest_prompt(&task, &language),
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
                    }}}
            }}),
        };
        let reply = self.model.complete(ctx, request).await?;
        let parsed = parse_model_json(&reply)?;
        let output = validate_digest(&parsed, &task, &language, &ids)?;
        Ok(HostReply {
            output,
            provenance: Vec::new(),
            usage: Usage {
                model_calls: 1,
                pages: 0,
            },
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

fn digest_prompt(task: &str, language: &str) -> String {
    let what = match task {
        "brief" => "a short briefing across the topics, one or two points per topic",
        "plan" => "a practical plan for the person based on what the sources say, with caveats where the sources are uncertain",
        "compare" => "a comparison of the subjects named in the focus; label each point with the subject it is about, or \"both\"",
        _ => "a digest of the news: the key facts, one point per development",
    };
    format!(
        "You write {what}. Write in the language with BCP 47 tag {language}, translating the \
         sources as needed. Use only facts stated in the articles you are given. Every point \
         cites the ids of the articles it rests on in `citations`. Never write URLs or invent \
         sources. Reply with JSON only: {{\"summary\": string, \"points\": [{{\"text\": string, \
         \"citations\": [article id], \"label\": optional string}}]}}."
    )
}

fn parse_model_json(reply: &str) -> Result<Value, HostError> {
    let trimmed = reply.trim();
    let body = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|s| s.strip_suffix("```"))
        .unwrap_or(trimmed);
    serde_json::from_str(body.trim())
        .map_err(|e| HostError::Failed(format!("model output rejected: not JSON ({e})")))
}

fn validate_digest(
    parsed: &Value,
    task: &str,
    language: &str,
    ids: &[String],
) -> Result<Value, HostError> {
    let reject = |why: &str| HostError::Failed(format!("model output rejected: {why}"));
    let summary = parsed["summary"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 1200)
        .ok_or_else(|| reject("no summary"))?;
    let points = parsed["points"]
        .as_array()
        .filter(|p| !p.is_empty() && p.len() <= 12)
        .ok_or_else(|| reject("points must be 1–12"))?;
    let known: BTreeSet<&str> = ids.iter().map(String::as_str).collect();
    let mut out_points = Vec::new();
    for point in points {
        let text = point["text"]
            .as_str()
            .map(str::trim)
            .filter(|t| !t.is_empty() && t.len() <= 400)
            .ok_or_else(|| reject("a point has no text"))?;
        let citations: Vec<&str> = point["citations"]
            .as_array()
            .map(|c| c.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if citations.is_empty() || citations.len() > 8 {
            return Err(reject("every point cites 1–8 articles"));
        }
        if let Some(bad) = citations.iter().find(|c| !known.contains(*c)) {
            return Err(reject(&format!("it cites {bad}, which it was not given")));
        }
        let mut record = json!({"text": text, "citations": citations});
        if let Some(label) = point["label"].as_str().filter(|l| !l.is_empty()) {
            if label.len() > 40 {
                return Err(reject("a label is longer than 40 bytes"));
            }
            record["label"] = json!(label);
        }
        out_points.push(record);
    }
    let output =
        json!({"task": task, "language": language, "summary": summary, "points": out_points});
    if crate::json::strings(&output).into_iter().any(contains_url) {
        return Err(reject("it contains a URL"));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(validate_digest(&ok, "digest", "en", &ids).is_ok());
        let foreign = json!({"summary": "S", "points": [{"text": "T", "citations": ["s9"]}]});
        assert!(validate_digest(&foreign, "digest", "en", &ids).is_err());
        let url = json!({"summary": "see https://example.org", "points": [{"text": "T", "citations": ["s1"]}]});
        assert!(validate_digest(&url, "digest", "en", &ids).is_err());
    }
}
