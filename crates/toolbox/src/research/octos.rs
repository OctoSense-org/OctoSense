//! The octos research engine behind [`ResearchBackend`] (feature
//! `octos-engine`).
//!
//! Search is the octos metasearch (`octos_research::metasearch`, octos#2582)
//! and reading is the octos polite reader (`octos_research::reader`,
//! octos#2568). Pages that need JavaScript are rendered by the toolbox's
//! shared headless browser ([`super::chrome`]), which follows octos
//! `deep_crawl`'s discipline. The host's policy ([`super::ResearchHost`]:
//! ids, evidence caps, relevance gate, translation, citations, budgets,
//! provenance) is unchanged: this module only finds and reads.
//!
//! - **One definition of an app's reach.** The app's grant **is**
//!   `octos_research::toolbox::Scope` ([`crate::scope`]), parsed by
//!   `Scope::from_grant`; every search is narrowed by `Scope::search_args`
//!   and every page read checked by `Scope::check_domain`, the same rules
//!   octos applies to its own toolbox tools.
//! - **Search**: the engines of the `news` category (by default Google News
//!   in the language's own edition, GDELT, publisher feeds, Hacker News,
//!   Mastodon; keyed engines when their keys are set), fanned out in
//!   parallel under a deadline, merged, deduplicated and ranked by octos,
//!   which also spaces requests per host, honours `Retry-After`, caches
//!   with ETags and suspends a failing engine with doubling backoff. No
//!   search-results page is scraped. The toolbox's own translation
//!   (`research.query`) is passed as `query_by_lang`, so each language is
//!   searched in its own words without a model call on the octos side.
//! - **Diagnostics**: each item's engines and score are in its provenance
//!   (`via`: `search:metasearch[google_news+gdelt] 0.83`); every engine
//!   that failed, timed out, was suspended or was rate limited, and every
//!   result filtered out, is a note ([`skipped_notes`]; feed entries octos
//!   found not to be about the topic, `query_mismatch`, get their own).
//!   A slow engine is dropped at octos's soft deadline
//!   (`SearchRequest::straggler_grace`, octos's default) and noted as timed
//!   out. `partial` means an engine failed or timed out in this search; an
//!   engine octos had already suspended is reported but does not make every
//!   search partial.
//! - **Read**: SSRF check and DNS pinning on every hop, per-host spacing,
//!   one backoff on 429/503 honouring `Retry-After`, content-type and size
//!   caps, readability extraction, octos's honest User-Agent; robots.txt
//!   only when the operator sets `OCTOS_RESPECT_ROBOTS=1`. When plain HTTP
//!   yields no main text the browser renders the page, and the reader
//!   re-checks where it went. A failed read keeps octos's reason and final
//!   URL ([`read_error`]: `paywall: … (final URL: …)`), so the run's
//!   diagnostics say why.
//! - **Posts are not evidence**: results octos marks `kind: post` (Mastodon
//!   posts) are left out of the search results with a note ([`articles`]).
//! - **`readable`**: a Google News article link is readable only when a
//!   browser is available. Without Chrome (a phone, a server) such results
//!   say `readable: false` and templates skip them.

use super::chrome::{Chrome, ChromeConfig};
use super::{
    same_language, FoundItem, LinkedPage, PageText, ResearchBackend, SearchQuery, SearchResults,
};
use crate::host::{url_host, CallContext, HostError, HostFuture};
use octos_research::metasearch::{
    EngineReport, EngineStatus, MetaItem, Metasearch, ReqwestFetch, SearchRequest,
};
use octos_research::reader::{self, Reader, ReaderConfig};
use octos_research::toolbox::ScopedSearch;
use octos_research::{ItemKind, ReadError, ReadFailure, SkippedUrl};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

/// `OCTOSENSE_TOOLBOX_RENDER=off` turns the browser off
/// ([`OctosConfig::from_env`]).
pub const RENDER_ENV: &str = "OCTOSENSE_TOOLBOX_RENDER";

/// Most merged results one search asks the metasearch for.
pub const MAX_RESULTS: usize = 30;

#[derive(Debug, Clone)]
pub struct OctosConfig {
    /// The metasearch category searched (`news` for the research templates).
    pub category: String,
    /// Only these engines, if set (engine ids).
    pub engines: Option<Vec<String>>,
    /// The whole fan-out's deadline.
    pub search_deadline: Duration,
    /// Operator setting: honour robots.txt. Off by default; when off,
    /// robots.txt is never fetched.
    pub respect_robots: bool,
    /// Minimum time between two page reads from one host.
    pub host_interval: Duration,
    /// One page request's timeout.
    pub timeout: Duration,
    /// The browser for pages that need JavaScript, or `None` for plain HTTP
    /// only.
    pub render: Option<ChromeConfig>,
}

impl Default for OctosConfig {
    fn default() -> Self {
        Self {
            category: "news".into(),
            engines: None,
            search_deadline: Duration::from_secs(15),
            respect_robots: false,
            host_interval: Duration::from_secs(1),
            timeout: Duration::from_secs(15),
            render: Some(ChromeConfig::default()),
        }
    }
}

impl OctosConfig {
    /// The defaults with the operator's settings: `OCTOS_RESPECT_ROBOTS` and
    /// `OCTOSENSE_TOOLBOX_RENDER=off`. Engine keys and settings are read by
    /// the metasearch itself (`OCTOS_METASEARCH_*`, each engine's key
    /// variable).
    pub fn from_env() -> Self {
        let env = |k: &str| std::env::var(k).ok();
        let render_off = env(RENDER_ENV).is_some_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "0" | "off" | "false" | "no"
            )
        });
        Self {
            respect_robots: octos_research::respect_robots(env),
            render: if render_off {
                None
            } else {
                Some(ChromeConfig::default())
            },
            ..Self::default()
        }
    }
}

/// A toolbox search as octos toolbox `search` arguments. The topic is
/// already in the search's language (the host's `research.query`
/// translated it), so it is also that language's `query_by_lang` entry.
/// `count` is the results the app gets, already within the grant's
/// `max_results` (the host narrowed it); the engines' candidate pool is
/// [`candidate_pool`].
pub fn search_args(query: &SearchQuery, category: &str) -> Value {
    let mut args = json!({
        "query": query.topic,
        "category": category,
        "count": query.limit,
    });
    if let Some(language) = query.language.as_deref().filter(|l| !l.is_empty()) {
        args["lang"] = json!([language]);
        args["query_by_lang"] = json!({ language: query.topic });
    }
    if let Some(region) = query.region.as_deref().filter(|r| !r.is_empty()) {
        args["region"] = json!(region);
    }
    if let Some(hours) = query.max_age_hours {
        args["since"] = json!(format!("{hours}h"));
    }
    args
}

/// Merged results asked of the metasearch for a search whose app gets
/// `count`: three times as many, at most [`MAX_RESULTS`], so the host's
/// relevance, readability and domain filters still leave `count`. The
/// grant's `max_results` bounds what the app gets, not this pool.
pub fn candidate_pool(count: usize) -> usize {
    (count * 3).min(MAX_RESULTS).max(count)
}

/// The octos research engine as a toolbox backend.
pub struct OctosResearch {
    config: OctosConfig,
    metasearch: Metasearch,
    reader: Reader,
    chrome: Option<Arc<Chrome>>,
}

impl OctosResearch {
    /// The metasearch shares the process's politeness state (host slots,
    /// cache, engine health); the browser is the process's shared one
    /// ([`Chrome::shared`]), or none when rendering is off or no Chrome is
    /// installed.
    pub fn new(config: OctosConfig) -> Self {
        let metasearch = Metasearch::from_env(Arc::new(ReqwestFetch::new()), &BTreeMap::new());
        let chrome = config.render.clone().and_then(Chrome::shared);
        let renderer: Option<reader::Renderer> = chrome.clone().map(|chrome| {
            Arc::new(move |url: String| {
                let chrome = chrome.clone();
                Box::pin(async move { chrome.render(&url).await }) as reader::RenderFuture
            }) as reader::Renderer
        });
        let reader = Reader::new(ReaderConfig {
            host_interval: config.host_interval,
            timeout: config.timeout,
            respect_robots: config.respect_robots,
            renderer,
            // `deep_crawl` lists a page's links from its HTML
            // ([`ResearchBackend::read_links`]).
            keep_html: true,
            ..ReaderConfig::default()
        });
        Self {
            config,
            metasearch,
            reader,
            chrome,
        }
    }

    /// Whether pages that need JavaScript can be read.
    pub fn renders(&self) -> bool {
        self.chrome.is_some()
    }

    /// The browser, if any (tests and diagnostics).
    pub fn chrome(&self) -> Option<&Arc<Chrome>> {
        self.chrome.as_ref()
    }
}

/// Whether a link reaches its article only through JavaScript.
pub fn needs_browser(url: &str) -> bool {
    url_host(url).as_deref() == Some("news.google.com")
}

/// The language a found item is tagged with: the search's own tag when the
/// engine's is the same language (`zh-CN` for a `zh` search stays `zh`, so
/// the app's scope and the host's gate see the tag they asked for), else
/// the engine's, else nothing.
pub fn item_language(found: Option<&str>, searched: Option<&str>) -> String {
    match (found.filter(|l| !l.trim().is_empty()), searched) {
        (Some(found), Some(searched)) if same_language(found, searched) => searched.to_owned(),
        (Some(found), _) => found.to_owned(),
        (None, _) => String::new(),
    }
}

/// A merged metasearch item as a [`FoundItem`]. `via` names the engines
/// that found it and its score, and so reaches the item's provenance.
pub fn found_item(item: &MetaItem, searched: Option<&str>, renders: bool) -> FoundItem {
    let source = Some(item.source.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            octos_research::urls::domain_of(item.source_url.as_deref().unwrap_or(&item.url))
        })
        .unwrap_or_default();
    FoundItem {
        url: item.url.clone(),
        title: item.title.clone(),
        source,
        language: item_language(item.lang.as_deref(), searched),
        published_at: item.published.clone().unwrap_or_default(),
        via: format!(
            "{}[{}] {:.2}",
            octos_research::metasearch::PROVIDER_ID,
            item.engines.join("+"),
            item.score
        ),
        readable: renders || !needs_browser(&item.url),
        snippet: item.snippet.clone(),
    }
}

/// One note per engine call that did not answer: failed, timed out,
/// suspended by octos's backoff, rate limited, refused by robots.txt, or
/// challenged by a bot check (a CAPTCHA page; a miss like an error, and the
/// note carries the page for the person to open).
pub fn engine_notes(reports: &[EngineReport]) -> Vec<String> {
    reports
        .iter()
        .filter_map(|r| {
            let what = match r.status {
                EngineStatus::Ok | EngineStatus::Empty => return None,
                EngineStatus::Error => "failed",
                EngineStatus::Timeout => "timed out",
                EngineStatus::Suspended => "suspended after earlier errors (octos backoff)",
                EngineStatus::RateLimited => "skipped: its host's rate limit",
                EngineStatus::Robots => "skipped: robots.txt",
                // A miss for this engine, like an error: octos suspends it and the
                // others answer (the challenge is never worked around).
                EngineStatus::Challenge => "challenged (bot check); skipped",
            };
            let lang = r
                .lang
                .as_deref()
                .map(|l| format!(" ({l})"))
                .unwrap_or_default();
            let error = r
                .error
                .as_deref()
                .map(|e| format!(": {e}"))
                .unwrap_or_default();
            let page = r
                .challenge_url
                .as_deref()
                .map(|u| format!(" ({u})"))
                .unwrap_or_default();
            Some(format!("engine {}{lang} {what}{error}{page}", r.engine))
        })
        .collect()
}

/// One line with every engine call: `engines: google_news (zh) 10, gdelt
/// (zh) suspended, publisher_feeds (zh) 0`.
fn engines_line(reports: &[EngineReport]) -> Option<String> {
    (!reports.is_empty()).then(|| {
        let calls: Vec<String> = reports
            .iter()
            .map(|r| {
                let lang = r
                    .lang
                    .as_deref()
                    .map(|l| format!(" ({l})"))
                    .unwrap_or_default();
                let outcome = match r.status {
                    EngineStatus::Ok | EngineStatus::Empty => r.hits.to_string(),
                    EngineStatus::Error => "failed".into(),
                    EngineStatus::Timeout => "timed out".into(),
                    EngineStatus::Suspended => "suspended".into(),
                    EngineStatus::RateLimited => "rate limited".into(),
                    EngineStatus::Robots => "robots".into(),
                    EngineStatus::Challenge => "challenged".into(),
                };
                let cached = if r.cached { " cached" } else { "" };
                format!("{}{lang} {outcome}{cached}", r.engine)
            })
            .collect();
        format!("engines: {}", calls.join(", "))
    })
}

/// The engines and score behind each of the first results, by host id
/// (the id `research.search` gives the script): `ranked: s1a2b3c4d5e6f
/// google_news+gdelt 0.83, …`. The article's provenance keeps only the
/// read, so this is where a person sees why a source was chosen.
pub fn ranking_note(items: &[FoundItem]) -> Option<String> {
    const SHOWN: usize = 10;
    (!items.is_empty()).then(|| {
        let ranked: Vec<String> = items
            .iter()
            .take(SHOWN)
            .map(|i| {
                let via = i
                    .via
                    .strip_prefix(octos_research::metasearch::PROVIDER_ID)
                    .unwrap_or(&i.via)
                    .trim_start_matches('[')
                    .replacen("] ", " ", 1);
                format!("{} {via}", super::item_id(&i.url))
            })
            .collect();
        let more = items.len().saturating_sub(SHOWN);
        let more = if more > 0 {
            format!(" (+{more} more)")
        } else {
            String::new()
        };
        format!("ranked: {}{more}", ranked.join(", "))
    })
}

/// The metasearch request for a scoped search. The soft deadline for slow
/// engines is octos's default ([`SearchRequest::new`]).
pub fn search_request(
    scoped: &ScopedSearch,
    config: &OctosConfig,
    now: chrono::DateTime<chrono::Utc>,
) -> SearchRequest {
    let mut request = SearchRequest::new(&scoped.query, &scoped.category);
    request.query_by_lang = scoped.query_by_lang.clone();
    request.langs = scoped.langs.clone();
    request.region = scoped.region.clone();
    request.since = scoped.since.clone();
    request.count = candidate_pool(scoped.count);
    request.limit = request.count;
    request.filters = scoped.filters.clone();
    request.engines = config.engines.clone();
    request.deadline = config.search_deadline;
    request.now = now;
    request
}

/// Notes for the results octos skipped: entries of a listing engine
/// (publisher feeds) that were not about the topic (`query_mismatch`) in one
/// note, every other reason (filters, dates, domains) counted in another.
pub fn skipped_notes(skipped: &[SkippedUrl]) -> Vec<String> {
    let mut reasons: BTreeMap<&str, usize> = BTreeMap::new();
    let mut mismatched = 0;
    for s in skipped {
        if s.reason == "query_mismatch" {
            mismatched += 1;
        } else {
            *reasons.entry(s.reason.as_str()).or_default() += 1;
        }
    }
    let mut notes = Vec::new();
    if mismatched > 0 {
        notes.push(format!(
            "{mismatched} feed entries left out: not about the topic (query_mismatch)"
        ));
    }
    if !reasons.is_empty() {
        let parts: Vec<String> = reasons.iter().map(|(r, n)| format!("{r}: {n}")).collect();
        notes.push(format!("results filtered out ({})", parts.join(", ")));
    }
    notes
}

/// The merged items that are articles, and a note counting the posts left
/// out. Posts (Mastodon posts, discussion threads without a linked article;
/// octos#2590 marks them `kind: post`) are signal about what people say, not
/// evidence, so they are never offered for reading or citation.
pub fn articles(items: &[MetaItem]) -> (Vec<&MetaItem>, Option<String>) {
    let (articles, posts): (Vec<&MetaItem>, Vec<&MetaItem>) =
        items.iter().partition(|i| i.kind == ItemKind::Article);
    let note = (!posts.is_empty()).then(|| {
        format!(
            "{} results are social posts, not articles; not used as evidence",
            posts.len()
        )
    });
    (articles, note)
}

/// A failed read as a host error. The message is octos's reason as
/// `<code>: <detail> (final URL: …)` (`bot_challenge`, `paywall`,
/// `consent_page`, `stub_page`, `http_403`, `redirect_unresolved`, `no_main_text`, …),
/// so the run's diagnostics say why a page could not be read and where the
/// read ended. SSRF, scope and robots.txt refusals are denials; every other
/// reason is a failure.
pub fn read_error(err: ReadError) -> HostError {
    let message = err.to_string();
    match err.reason {
        ReadFailure::Blocked | ReadFailure::Robots | ReadFailure::RobotsUnreachable => {
            HostError::Denied(message)
        }
        _ => HostError::Failed(message),
    }
}

impl ResearchBackend for OctosResearch {
    fn search<'a>(
        &'a self,
        ctx: &'a CallContext,
        query: SearchQuery,
    ) -> HostFuture<'a, Result<SearchResults, HostError>> {
        Box::pin(async move {
            let now = chrono::Utc::now();
            let scope = &ctx.app.scope;
            let scoped = scope
                .search_args(
                    &search_args(
                        &query,
                        query.category.as_deref().unwrap_or(&self.config.category),
                    ),
                    now,
                )
                .map_err(HostError::Denied)?;
            let request = search_request(&scoped, &self.config, now);
            let response = self.metasearch.search(&request).await;

            let mut notes = scoped.notes.clone();
            notes.extend(engines_line(&response.engines));
            notes.extend(engine_notes(&response.engines));
            notes.extend(response.note.clone());
            notes.extend(skipped_notes(&response.skipped));
            let failed_now = response
                .engines
                .iter()
                .filter(|r| {
                    matches!(
                        r.status,
                        EngineStatus::Error | EngineStatus::Timeout | EngineStatus::Challenge
                    )
                })
                .count();
            let answered = response
                .engines
                .iter()
                .filter(|r| matches!(r.status, EngineStatus::Ok | EngineStatus::Empty))
                .count();
            if response.items.is_empty() && answered == 0 && !response.engines.is_empty() {
                return Err(HostError::Failed(format!(
                    "no search engine answered: {}",
                    engine_notes(&response.engines).join("; ")
                )));
            }
            let renders = self.renders();
            let (articles, posts) = articles(&response.items);
            notes.extend(posts);
            let mut items: Vec<FoundItem> = articles
                .iter()
                .map(|item| found_item(item, query.language.as_deref(), renders))
                .collect();
            let unreadable = items.iter().filter(|i| !i.readable).count();
            if unreadable > 0 {
                notes.push(format!(
                    "{unreadable} results are Google News links, which need a browser to read (none available)"
                ));
            }
            // Readable first, keeping octos's ranking otherwise.
            items.sort_by_key(|i| !i.readable);
            notes.extend(ranking_note(&items));
            let providers: Vec<String> = articles
                .iter()
                .flat_map(|i| i.engines.iter().cloned())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let fetches = response
                .engines
                .iter()
                .filter(|r| {
                    !r.cached
                        && matches!(
                            r.status,
                            EngineStatus::Ok
                                | EngineStatus::Empty
                                | EngineStatus::Error
                                | EngineStatus::Timeout
                                | EngineStatus::Challenge
                        )
                })
                .count() as u32;
            Ok(SearchResults {
                items,
                providers,
                partial: failed_now > 0,
                fetches,
                notes,
            })
        })
    }

    fn read<'a>(
        &'a self,
        ctx: &'a CallContext,
        item: &'a FoundItem,
    ) -> HostFuture<'a, Result<PageText, HostError>> {
        Box::pin(async move {
            let scope = &ctx.app.scope;
            scope.check_domain(&item.url).map_err(HostError::Denied)?;
            if !self.renders() && needs_browser(&item.url) {
                return Err(HostError::Failed(
                    "Google News article links need a browser to resolve; none available".into(),
                ));
            }
            let page = self.reader.read(&item.url).await.map_err(read_error)?;
            // A Google News link is checked again where it ended up: the
            // publisher.
            scope
                .check_domain(&page.final_url)
                .map_err(HostError::Denied)?;
            Ok(PageText {
                text: page.text,
                title: page.meta.title.filter(|t| !t.trim().is_empty()),
                final_url: Some(page.final_url),
            })
        })
    }

    fn read_links<'a>(
        &'a self,
        ctx: &'a CallContext,
        url: &'a str,
    ) -> HostFuture<'a, Result<LinkedPage, HostError>> {
        Box::pin(async move {
            let scope = &ctx.app.scope;
            scope.check_domain(url).map_err(HostError::Denied)?;
            let page = self.reader.read(url).await.map_err(read_error)?;
            scope
                .check_domain(&page.final_url)
                .map_err(HostError::Denied)?;
            let links = extract_links(&page.html, &page.final_url);
            Ok(LinkedPage {
                page: PageText {
                    text: page.text,
                    title: page.meta.title.filter(|t| !t.trim().is_empty()),
                    final_url: Some(page.final_url.clone()),
                },
                final_url: page.final_url,
                links,
            })
        })
    }
}

/// The absolute `http(s)` links of `<a href>`s in `html`, resolved against
/// `base`, without fragments, deduplicated in page order. A plain scan, not
/// a parser: it only has to find links, never to render.
pub fn extract_links(html: &str, base: &str) -> Vec<String> {
    let Ok(base) = url::Url::parse(base) else {
        return Vec::new();
    };
    let lower = html.to_ascii_lowercase();
    let mut links = Vec::new();
    let mut seen = BTreeSet::new();
    let mut from = 0;
    while let Some(at) = lower[from..].find("href") {
        let start = from + at + 4;
        from = start;
        let rest = &html[start..];
        let trimmed = rest.trim_start();
        let Some(after_eq) = trimmed.strip_prefix('=') else {
            continue;
        };
        let after_eq = after_eq.trim_start();
        let value = match after_eq.chars().next() {
            Some(q @ ('"' | '\'')) => match after_eq[1..].find(q) {
                Some(end) => &after_eq[1..1 + end],
                None => continue,
            },
            Some(_) => {
                let end = after_eq
                    .find(|c: char| c.is_whitespace() || c == '>')
                    .unwrap_or(after_eq.len());
                &after_eq[..end]
            }
            None => continue,
        };
        let value = value.trim().replace("&amp;", "&");
        if value.is_empty() || value.starts_with('#') {
            continue;
        }
        let Ok(mut link) = base.join(&value) else {
            continue;
        };
        if !matches!(link.scheme(), "http" | "https") {
            continue;
        }
        link.set_fragment(None);
        let link = link.to_string();
        if seen.insert(link.clone()) {
            links.push(link);
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(url: &str, lang: Option<&str>) -> MetaItem {
        MetaItem {
            url: url.into(),
            title: "Typhoon makes landfall".into(),
            source: "Example News".into(),
            lang: lang.map(str::to_owned),
            published: Some("2026-09-27T10:00:00Z".into()),
            snippet: String::new(),
            engines: vec!["google_news".into(), "gdelt".into()],
            score: 0.834,
            category: "news".into(),
            source_url: None,
            kind: ItemKind::Article,
        }
    }

    #[test]
    fn links_are_absolute_http_without_fragments_or_repeats() {
        let html = r##"<a href="/a">A</a> <A HREF='b?x=1&amp;y=2#top'>B</A>
            <a href=https://other.example/c>C</a> <a href="#frag">F</a>
            <a href="mailto:x@example.org">M</a> <a href="/a#again">A2</a>"##;
        assert_eq!(
            extract_links(html, "https://example.org/dir/page"),
            [
                "https://example.org/a",
                "https://example.org/dir/b?x=1&y=2",
                "https://other.example/c",
            ]
        );
        assert!(extract_links(html, "not a url").is_empty());
    }

    #[test]
    fn google_news_links_are_readable_only_with_a_browser() {
        let gn = "https://news.google.com/rss/articles/CBMiXYZ?oc=5";
        assert!(!found_item(&item(gn, Some("en")), Some("en"), false).readable);
        assert!(found_item(&item(gn, Some("en")), Some("en"), true).readable);
        let direct = "https://www.bbc.co.uk/news/articles/x";
        assert!(found_item(&item(direct, None), Some("en"), false).readable);
    }

    #[test]
    fn items_carry_their_engines_score_and_searched_language() {
        let found = found_item(
            &item("https://x.example/a", Some("zh-CN")),
            Some("zh"),
            false,
        );
        assert_eq!(found.language, "zh");
        assert_eq!(found.source, "Example News");
        assert_eq!(found.via, "metasearch[google_news+gdelt] 0.83");
        assert_eq!(
            ranking_note(std::slice::from_ref(&found)).unwrap(),
            format!(
                "ranked: {} google_news+gdelt 0.83",
                super::super::item_id(&found.url)
            )
        );
        assert_eq!(item_language(Some("en"), Some("zh")), "en");
        assert_eq!(item_language(None, Some("zh")), "");
    }

    #[test]
    fn a_source_without_a_name_is_its_publisher_domain() {
        let mut i = item("https://news.google.com/rss/articles/CBMi", None);
        i.source.clear();
        i.source_url = Some("https://www.reuters.com".into());
        assert_eq!(found_item(&i, None, true).source, "reuters.com");
    }

    #[test]
    fn the_app_grant_is_the_octos_scope() {
        let grant: crate::AppContext = serde_json::from_value(json!({
            "app_id": "os.news", "grants": ["research"], "folder": "/tmp",
            "scope": {"langs": ["en", "zh"], "domains_deny": ["example.com"], "max_age_days": 2}
        }))
        .unwrap();
        let scope = &grant.scope;
        assert_eq!((scope.max_depth, scope.max_pages), (0, 0));
        assert!(scope
            .check_domain("https://news.example.com/a")
            .unwrap_err()
            .contains("outside"));
        let query = SearchQuery {
            topic: "台风".into(),
            language: Some("zh".into()),
            region: None,
            limit: 5,
            max_age_hours: Some(72),
            max_fetches: 8,
            category: None,
        };
        let now = chrono::Utc::now();
        let scoped = scope
            .search_args(&search_args(&query, "news"), now)
            .unwrap();
        assert_eq!(scoped.langs, vec!["zh".to_string()]);
        assert_eq!(
            scoped.query_by_lang.get("zh").map(String::as_str),
            Some("台风")
        );
        assert_eq!(scoped.count, 5);
        assert_eq!(candidate_pool(scoped.count), 15);
        assert_eq!(candidate_pool(20), MAX_RESULTS);
        // The grant's 2 days narrow the search's 72 hours.
        assert!(scoped.since.unwrap().cutoff >= now - chrono::Duration::days(2));
        // A language outside the grant is refused by octos's narrowing.
        let fr = SearchQuery {
            language: Some("fr".into()),
            ..query
        };
        assert!(scope.search_args(&search_args(&fr, "news"), now).is_err());
    }

    #[test]
    fn engines_that_did_not_answer_become_diagnostics() {
        let report = |engine: &str, status, error: Option<&str>| EngineReport {
            engine: engine.into(),
            lang: Some("zh".into()),
            status,
            hits: 0,
            elapsed_ms: 5,
            error: error.map(str::to_owned),
            cached: false,
            challenge_url: None,
            in_browser: false,
        };
        let reports = vec![
            report("google_news", EngineStatus::Ok, None),
            report("gdelt", EngineStatus::Suspended, None),
            report("mastodon", EngineStatus::Error, Some("HTTP 500")),
            EngineReport {
                challenge_url: Some("https://www.bing.com/turing/captcha".into()),
                ..report(
                    "bing",
                    EngineStatus::Challenge,
                    Some("challenge (captcha): open the page to continue"),
                )
            },
        ];
        assert_eq!(
            engine_notes(&reports),
            vec![
                "engine gdelt (zh) suspended after earlier errors (octos backoff)".to_string(),
                "engine mastodon (zh) failed: HTTP 500".to_string(),
                "engine bing (zh) challenged (bot check); skipped: challenge (captcha): \
                 open the page to continue (https://www.bing.com/turing/captcha)"
                    .to_string(),
            ]
        );
        assert_eq!(
            engines_line(&reports).unwrap(),
            "engines: google_news (zh) 0, gdelt (zh) suspended, mastodon (zh) failed, \
             bing (zh) challenged"
        );
    }

    #[test]
    fn posts_are_left_out_of_the_results_with_a_note() {
        let mut post = item("https://mastodon.social/@a/1", Some("en"));
        post.kind = ItemKind::Post;
        let article = item("https://x.example/a", Some("en"));
        let all = [post, article];
        let (kept, note) = articles(&all);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].url, "https://x.example/a");
        assert_eq!(
            note.as_deref(),
            Some("1 results are social posts, not articles; not used as evidence")
        );
        assert!(articles(&all[1..]).1.is_none());
    }

    #[test]
    fn read_failures_keep_octos_reason_and_final_url() {
        let wall = ReadError::new(ReadFailure::Paywall, "subscriber-only")
            .at("https://publisher.example/story");
        match read_error(wall) {
            HostError::Failed(m) => assert_eq!(
                m,
                "paywall: subscriber-only (final URL: https://publisher.example/story)"
            ),
            other => panic!("{other:?}"),
        }
        match read_error(ReadError::new(ReadFailure::Http(403), "")) {
            HostError::Failed(m) => assert_eq!(m, "http_403"),
            other => panic!("{other:?}"),
        }
        for denied in [
            ReadFailure::Blocked,
            ReadFailure::Robots,
            ReadFailure::RobotsUnreachable,
        ] {
            assert!(matches!(
                read_error(ReadError::new(denied, "")),
                HostError::Denied(_)
            ));
        }
    }

    #[test]
    fn stub_pages_keep_their_reason() {
        let stub = ReadError::new(ReadFailure::StubPage, "only a video caption")
            .at("https://publisher.example/video/1");
        match read_error(stub) {
            HostError::Failed(m) => assert_eq!(
                m,
                "stub_page: only a video caption (final URL: https://publisher.example/video/1)"
            ),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn feed_entries_off_the_topic_get_their_own_note() {
        let skip = |url: &str, reason: &str| SkippedUrl::new(url, reason);
        let skipped = [
            skip("https://feed.example/terrorist-act", "query_mismatch"),
            skip("https://feed.example/ai-in-schools", "query_mismatch"),
            skip("https://old.example/a", "older_than_since"),
        ];
        assert_eq!(
            skipped_notes(&skipped),
            vec![
                "2 feed entries left out: not about the topic (query_mismatch)".to_string(),
                "results filtered out (older_than_since: 1)".to_string(),
            ]
        );
        assert!(skipped_notes(&[]).is_empty());
    }

    #[test]
    fn slow_engines_get_octos_soft_deadline() {
        let grant: crate::AppContext = serde_json::from_value(json!({
            "app_id": "os.news", "grants": ["research"], "folder": "/tmp",
            "scope": {"langs": ["en"]}
        }))
        .unwrap();
        let query = SearchQuery {
            topic: "EU AI Act".into(),
            language: Some("en".into()),
            region: None,
            limit: 5,
            max_age_hours: None,
            max_fetches: 8,
            category: None,
        };
        let now = chrono::Utc::now();
        let scoped = grant
            .scope
            .search_args(&search_args(&query, "news"), now)
            .unwrap();
        let config = OctosConfig::default();
        let request = search_request(&scoped, &config, now);
        assert_eq!(
            request.straggler_grace,
            Some(octos_research::metasearch::DEFAULT_STRAGGLER_GRACE)
        );
        assert_eq!(request.deadline, config.search_deadline);
        assert_eq!(request.count, candidate_pool(5));
        assert_eq!(request.langs, vec!["en".to_string()]);
    }

    #[test]
    fn robots_stay_off_and_rendering_is_on_by_default() {
        let config = OctosConfig::default();
        assert!(!config.respect_robots);
        assert!(config.render.is_some());
        assert_eq!(config.category, "news");
        assert!(octos_research::USER_AGENT.contains("octos-research"));
    }
}
