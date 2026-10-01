//! The `news` host service: News's data service (ADR 0002, milestone M1).
//!
//! Code, not a model, collects the stories: on a timer, while the shell runs
//! and whether or not News is open, from the News app's own feeds (Hacker
//! News, TechMeme, Google News), a short default list of RSS/Atom feeds,
//! feeds imported from OPML, and the person's topics (Google News RSS and
//! GDELT, per language and region). Stories are normalized, deduplicated
//! against a seen-items ledger and kept in the host's own directory
//! (`<host_dir>/news`); the app and, from M2, its agent read them through
//! `host.request`:
//!
//! | method | args | answer |
//! |---|---|---|
//! | `news.list` | `{since?, topic?, lang?, feed?, current?, limit?, offset?}` | `{total, offset, updated, items: [item]}`, newest first; with `feed` and `current: true`, exactly that source's latest fetch in its own order, plus `feed_error` |
//! | `news.read` | `{id, full?}` | `{item, text, full_text}`: `text` is the stored summary, or the article's text when `full` and the shell gave the service a reader |
//! | `news.topics.get` | – | `{topics: [{query, lang, region}]}` |
//! | `news.topics.set` | `{topics: [{query, lang, region}]}` | `{topics}`, as stored (checked, at most 20) |
//! | `news.refresh` | – | the run's report `{at, new, total, clusters, sources}`, or `{busy: true}` |
//! | `news.sources` | – | `{sources: [{id, label, kind, host, lang, topics, last_success, last_error, failures, next_due, items}]}` |
//! | `news.feeds.import` | `{opml}` | `{added: [id], skipped: [{url, reason}]}` |
//!
//! An item is `{id, title, url, source, feed, lang, published, fetched,
//! summary, image?, topics, discussion?, points?, comments?, also?}`; `id` is
//! a hash of the canonical URL, times are Unix seconds.
//!
//! Every request goes to a host the News bundle's manifest declares
//! (`network.hosts`), redirects included, with `User-Agent:
//! OctoSense-News/1.0`, `If-None-Match`/`If-Modified-Since`, a timeout, a
//! 4 MB cap, a pause between requests to one host, a per-source interval, and
//! a growing back-off after a failure: the timer retries a failed source
//! after 30 s, then 1, 2, 4 … minutes (at most an hour), waking for it
//! rather than waiting for its next 15-minute tick, and the person's
//! Refresh (`news.refresh`, which News also sends when it opens) fetches a
//! failed source again at once (at most every 10 s). A first fetch before
//! the network was up is so retried, never stuck until the back-off ends. Full article text is not
//! fetched here: article hosts are arbitrary, so reading one is a separate
//! capability the shell may grant ([`ArticleReader`]).
//!
//! After each run the service calls the shell's hook ([`Options::on_fetch`])
//! with a [`FetchReport`]: how many stories are new and on which topics,
//! the event that will wake News's agent (M3).
use octosense_appstore::services::{HostService, Replier, ServiceCall, ServiceHost};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub mod fetch;
pub mod item;
pub mod parse;
pub mod sources;
pub mod store;
pub mod text;

pub use fetch::{Fetcher, HttpFetcher, Request, Response};
pub use item::{Draft, Item};
pub use sources::{Source, Topic};
pub use store::{Query, Retention};

/// Reads an article's main text, for `news.read {full: true}`. Article hosts
/// are arbitrary, so this is the shell's to grant (a browser-backed reader,
/// under its own policy); the service has none by default.
pub trait ArticleReader: Send + Sync {
    fn read(&self, url: &str) -> Result<String, String>;
}

/// What one fetch run did: the event the shell forwards to News's agent.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct FetchReport {
    /// When the run finished (Unix seconds).
    pub at: i64,
    /// Stories new this run.
    pub new: usize,
    /// Stories kept after the run.
    pub total: usize,
    /// The new stories by topic tag, largest first.
    pub clusters: Vec<Cluster>,
    pub sources: Vec<SourceReport>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Cluster {
    pub topic: String,
    pub count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SourceReport {
    pub id: String,
    /// `ok`, `not_modified`, `skipped` (not due yet) or `error`.
    pub status: &'static str,
    pub new: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

type Hook = Arc<dyn Fn(&FetchReport) + Send + Sync>;
type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// How the service runs. `Options::default()` is what a shell wants.
#[derive(Clone)]
pub struct Options {
    host_dir: Option<PathBuf>,
    fetcher: Arc<dyn Fetcher>,
    reader: Option<Arc<dyn ArticleReader>>,
    on_fetch: Option<Hook>,
    clock: Clock,
    timer: bool,
    interval: Duration,
    source_interval_secs: i64,
    manual_interval_secs: i64,
    retry_secs: i64,
    manual_retry_secs: i64,
    max_backoff_secs: i64,
    host_spacing: Duration,
    gdelt_spacing: Duration,
    gdelt: bool,
    default_feeds: bool,
    hosts: Vec<String>,
    retention: Retention,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            host_dir: None,
            fetcher: Arc::new(HttpFetcher::default()),
            reader: None,
            on_fetch: None,
            clock: Arc::new(|| chrono::Utc::now().timestamp()),
            timer: true,
            interval: Duration::from_secs(900),
            source_interval_secs: 600,
            manual_interval_secs: 120,
            retry_secs: 30,
            manual_retry_secs: 10,
            max_backoff_secs: 3600,
            host_spacing: Duration::from_secs(2),
            // GDELT asks for no more than one request every five seconds.
            gdelt_spacing: Duration::from_secs(6),
            gdelt: true,
            default_feeds: true,
            hosts: sources::manifest_hosts(),
            retention: Retention::default(),
        }
    }
}

impl Options {
    /// The host directory the shell gives the Card runner (`ServiceCall::
    /// host_dir`). With it, fetching starts at registration; without it, at
    /// the first request.
    pub fn host_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.host_dir = Some(dir.into());
        self
    }
    /// Where requests go: the network by default, fixtures in tests.
    pub fn fetcher(mut self, fetcher: Arc<dyn Fetcher>) -> Self {
        self.fetcher = fetcher;
        self
    }
    /// Grant `news.read {full: true}` a way to read article text.
    pub fn reader(mut self, reader: Arc<dyn ArticleReader>) -> Self {
        self.reader = Some(reader);
        self
    }
    /// Called after every fetch run (timer or `news.refresh`), on the fetch
    /// thread, with what was new. The shell forwards it to News's agent.
    pub fn on_fetch(mut self, f: impl Fn(&FetchReport) + Send + Sync + 'static) -> Self {
        self.on_fetch = Some(Arc::new(f));
        self
    }
    /// Unix seconds; tests move time.
    pub fn clock(mut self, f: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        self.clock = Arc::new(f);
        self
    }
    /// Fetch on a timer (default on, every 15 minutes).
    pub fn timer(mut self, on: bool) -> Self {
        self.timer = on;
        self
    }
    pub fn interval(mut self, every: Duration) -> Self {
        self.interval = every;
        self
    }
    /// The least time between two fetches of one source: on the timer, and
    /// on `news.refresh`.
    pub fn source_intervals(mut self, timer_secs: i64, manual_secs: i64) -> Self {
        self.source_interval_secs = timer_secs;
        self.manual_interval_secs = manual_secs;
        self
    }
    /// After a failure: the first retry on the timer (`timer_secs`, then
    /// doubling up to an hour), and the least time between two attempts on
    /// `news.refresh` (`manual_secs`).
    pub fn retries(mut self, timer_secs: i64, manual_secs: i64) -> Self {
        self.retry_secs = timer_secs;
        self.manual_retry_secs = manual_secs;
        self
    }
    /// The pause between two requests to one host, and to GDELT.
    pub fn spacing(mut self, host: Duration, gdelt: Duration) -> Self {
        self.host_spacing = host;
        self.gdelt_spacing = gdelt;
        self
    }
    /// Query GDELT for topics (default on).
    pub fn gdelt(mut self, on: bool) -> Self {
        self.gdelt = on;
        self
    }
    /// Fetch the default RSS/Atom list beyond News's own tabs (default on).
    pub fn default_feeds(mut self, on: bool) -> Self {
        self.default_feeds = on;
        self
    }
    /// The hosts the service may reach; by default the News manifest's
    /// `network.hosts`.
    pub fn hosts(mut self, hosts: Vec<String>) -> Self {
        self.hosts = hosts.into_iter().map(|h| h.to_ascii_lowercase()).collect();
        self
    }
    pub fn retention(mut self, retention: Retention) -> Self {
        self.retention = retention;
        self
    }
}

/// The service's state, shared by the host service, the timer and the shell.
struct Core {
    options: Options,
    dir: Mutex<Option<PathBuf>>,
    data: Mutex<Option<store::Data>>,
    running: Mutex<()>,
    timer_started: AtomicBool,
    stop: Arc<AtomicBool>,
}

/// A handle on the news service: what the host service answers with, and
/// what a shell (or, from M2, the kernel's tool routing) can call directly.
#[derive(Clone)]
pub struct News {
    core: Arc<Core>,
}

/// Offer the service to the Card runner with the defaults: the network, the
/// manifest's hosts, a 15-minute timer that starts at the first request.
pub fn register() -> News {
    register_with(Options::default())
}

pub fn register_with(options: Options) -> News {
    let news = News::new(options);
    octosense_appstore::services::register_host_service(Box::new(NewsService { news: news.clone() }));
    news
}

/// Which apps may call the service: system apps (News).
fn may_call(app_id: &str) -> bool {
    app_id.starts_with("os.")
}

impl News {
    /// The service without registering it (tests, or a shell that routes
    /// calls itself).
    pub fn new(options: Options) -> News {
        let host_dir = options.host_dir.clone();
        let news = News {
            core: Arc::new(Core {
                options,
                dir: Mutex::new(None),
                data: Mutex::new(None),
                running: Mutex::new(()),
                timer_started: AtomicBool::new(false),
                stop: Arc::new(AtomicBool::new(false)),
            }),
        };
        if let Some(dir) = host_dir {
            news.attach(&dir);
        }
        news
    }

    /// Use `<host_dir>/news`, and start the timer, if not already.
    pub fn attach(&self, host_dir: &Path) {
        {
            let mut dir = self.core.dir.lock().unwrap();
            if dir.is_none() {
                *dir = Some(store::Store::at(host_dir).dir);
            }
        }
        if self.core.options.timer && !self.core.timer_started.swap(true, Ordering::SeqCst) {
            let news = self.clone();
            let stop = self.core.stop.clone();
            let every = self.core.options.interval;
            std::thread::Builder::new()
                .name("news-fetch".into())
                .spawn(move || loop {
                    let _ = news.refresh_with(false);
                    // Wake for the next source due (a failed one's retry
                    // comes before the next tick).
                    let until = Instant::now() + news.next_wake(every);
                    while Instant::now() < until {
                        if stop.load(Ordering::SeqCst) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(500).min(every));
                    }
                })
                .ok();
        }
    }

    /// Stop the timer (at shutdown; tests).
    pub fn stop(&self) {
        self.core.stop.store(true, Ordering::SeqCst);
    }

    fn store(&self) -> Result<store::Store, String> {
        let dir = self.core.dir.lock().unwrap().clone().ok_or("The news service has no folder yet.")?;
        Ok(store::Store { dir })
    }

    /// Run `f` over the data, loaded from disk on first use.
    fn with_data<T>(&self, f: impl FnOnce(&mut store::Data, &store::Store) -> T) -> Result<T, String> {
        let store = self.store()?;
        let mut data = self.core.data.lock().unwrap();
        let data = data.get_or_insert_with(|| store.load());
        Ok(f(data, &store))
    }

    fn now(&self) -> i64 {
        (self.core.options.clock)()
    }

    /// Every source the service fetches now: News's tabs, the default feeds,
    /// imported feeds and the topics' searches.
    pub fn sources(&self) -> Result<Vec<Source>, String> {
        let options = &self.core.options;
        self.with_data(|data, _| {
            let mut out = sources::app_feeds();
            if options.default_feeds {
                out.extend(sources::default_feeds());
            }
            for feed in &data.feeds {
                if !out.iter().any(|s| s.id == feed.id) {
                    out.push(feed.clone());
                }
            }
            for topic in &data.topics {
                for source in topic.sources(options.gdelt) {
                    if !out.iter().any(|s| s.id == source.id) {
                        out.push(source);
                    }
                }
            }
            out
        })
    }

    fn allowed(&self, url: &str) -> Option<String> {
        sources::host_of(url).filter(|h| self.core.options.hosts.contains(h))
    }

    /// `news.list`.
    pub fn list(&self, query: &Query) -> Result<Value, String> {
        self.with_data(|data, _| {
            let (total, items) = data.query(query);
            let mut out = json!({"total": total, "offset": query.offset, "updated": data.updated, "items": items});
            if let (Some(feed), true) = (&query.feed, query.current) {
                out["feed_error"] = json!(data.state.get(feed).and_then(|s| s.last_error.clone()));
            }
            out
        })
    }

    /// A stored story.
    pub fn item(&self, id: &str) -> Result<Item, String> {
        self.with_data(|data, _| data.item(id).cloned())?.ok_or_else(|| "There is no such story.".to_string())
    }

    /// `news.read`: the stored story, with its summary as text, or the
    /// article's text when `full` and a reader was granted.
    pub fn read(&self, id: &str, full: bool) -> Result<Value, String> {
        let item = self.item(id)?;
        if full {
            if let Some(reader) = &self.core.options.reader {
                let text = reader.read(&item.url)?;
                return Ok(json!({"item": item, "text": text, "full_text": true}));
            }
        }
        Ok(json!({"item": item, "text": item.summary, "full_text": false}))
    }

    pub fn topics(&self) -> Result<Vec<Topic>, String> {
        self.with_data(|data, _| data.topics.clone())
    }

    /// `news.topics.set`: replace the followed topics. Their sources are
    /// fetched on the next run.
    pub fn set_topics(&self, topics: &[Value]) -> Result<Vec<Topic>, String> {
        if topics.len() > sources::MAX_TOPICS {
            return Err(format!("At most {} topics.", sources::MAX_TOPICS));
        }
        let mut checked: Vec<Topic> = Vec::new();
        for t in topics {
            let s = |k: &str| t[k].as_str().unwrap_or("");
            let topic = Topic::validated(s("query"), if s("lang").is_empty() { "en" } else { s("lang") }, if s("region").is_empty() { "US" } else { s("region") })?;
            if !checked.contains(&topic) {
                checked.push(topic);
            }
        }
        self.with_data(|data, store| {
            store.save_topics(&checked)?;
            data.topics = checked.clone();
            Ok(checked)
        })?
    }

    /// `news.feeds.import`: add the feeds an OPML file lists whose hosts the
    /// News manifest declares; the rest are reported, not fetched.
    pub fn import_opml(&self, opml: &str) -> Result<Value, String> {
        let feeds = parse::opml(opml)?;
        let known = self.sources()?;
        let mut added = Vec::new();
        let mut skipped = Vec::new();
        let mut new_feeds = Vec::new();
        for feed in feeds {
            if self.allowed(&feed.url).is_none() {
                skipped.push(json!({"url": feed.url, "reason": "its host is not in the News manifest's network.hosts"}));
                continue;
            }
            if known.iter().chain(new_feeds.iter()).any(|s: &Source| item::canonical_url(&s.url) == item::canonical_url(&feed.url)) {
                skipped.push(json!({"url": feed.url, "reason": "already followed"}));
                continue;
            }
            let id = format!("feed-{}", &item::item_id(&feed.url)[..8]);
            added.push(json!(id));
            new_feeds.push(Source { id, label: feed.title, kind: "rss".into(), url: feed.url, lang: String::new(), topics: feed.topics });
        }
        self.with_data(|data, store| {
            let mut all = data.feeds.clone();
            all.extend(new_feeds);
            store.save_feeds(&all)?;
            data.feeds = all;
            Ok::<(), String>(())
        })??;
        Ok(json!({"added": added, "skipped": skipped}))
    }

    /// `news.sources`.
    pub fn source_status(&self) -> Result<Value, String> {
        let sources = self.sources()?;
        self.with_data(|data, _| {
            let rows: Vec<Value> = sources
                .iter()
                .map(|s| {
                    let st = data.state.get(&s.id).cloned().unwrap_or_default();
                    json!({"id": s.id, "label": s.label, "kind": s.kind, "host": s.host(), "lang": s.lang, "topics": s.topics,
                        "last_success": st.last_success, "last_error": st.last_error, "failures": st.failures,
                        "next_due": st.next_due, "items": st.current.len()})
                })
                .collect();
            json!({"sources": rows})
        })
    }

    /// How long the timer sleeps: until the next source is due, at least
    /// 15 s and at most `every`.
    fn next_wake(&self, every: Duration) -> Duration {
        let now = self.now();
        let next = self.with_data(|data, _| data.state.values().map(|st| st.next_due).min()).ok().flatten();
        match next {
            Some(due) => Duration::from_secs((due - now).clamp(15, every.as_secs().max(15) as i64) as u64),
            None => every,
        }
    }

    /// The timer's run: every source whose time (its interval, or a failed
    /// one's back-off) has come.
    pub fn refresh_due(&self) -> Result<Option<FetchReport>, String> {
        self.refresh_with(false)
    }

    /// Fetch what is due now, as `news.refresh` does (a manual run fetches a
    /// source again after [`Options::source_intervals`]' manual interval,
    /// and a failed one after [`Options::retries`]' manual retry, whatever
    /// its back-off).
    /// `Err` only when there is no folder yet; a run already going answers
    /// `Ok(None)`.
    pub fn refresh(&self) -> Result<Option<FetchReport>, String> {
        self.refresh_with(true)
    }

    fn refresh_with(&self, manual: bool) -> Result<Option<FetchReport>, String> {
        let Ok(_running) = self.core.running.try_lock() else { return Ok(None) };
        let options = &self.core.options;
        let sources = self.sources()?;
        let state: HashMap<String, store::SourceState> = self.with_data(|data, _| data.state.clone())?;
        let mut last_request: HashMap<String, Instant> = HashMap::new();
        let mut fetched: Vec<(Source, Result<Option<fetch::Response>, String>, i64)> = Vec::new();
        let mut reports = Vec::new();
        for source in sources {
            let st = state.get(&source.id).cloned().unwrap_or_default();
            let now = self.now();
            let due = if manual {
                let least = if st.failures == 0 { options.manual_interval_secs } else { options.manual_retry_secs };
                now - st.last_attempt >= least
            } else {
                now >= st.next_due
            };
            if !due {
                reports.push(SourceReport { id: source.id.clone(), status: "skipped", new: 0, error: None });
                continue;
            }
            let result = self.fetch_source(&source, &st, &mut last_request);
            fetched.push((source, result, self.now()));
        }
        let report = self.with_data(|data, store| {
            let mut new_ids: Vec<String> = Vec::new();
            for (source, result, at) in fetched {
                let st = data.state.entry(source.id.clone()).or_default();
                st.last_attempt = at;
                let parsed = result.and_then(|response| match response {
                    None => Ok(None),
                    Some(response) => {
                        let drafts = match source.kind.as_str() {
                            "hn" => parse::hn(&response.body),
                            "gdelt" => parse::gdelt(&response.body),
                            _ => parse::feed(&response.body, source.style()),
                        }?;
                        Ok(Some((response, drafts)))
                    }
                });
                match parsed {
                    Ok(parsed) => {
                        st.failures = 0;
                        st.last_error = None;
                        st.last_success = at;
                        st.next_due = at + options.source_interval_secs;
                        match parsed {
                            None => reports.push(SourceReport { id: source.id.clone(), status: "not_modified", new: 0, error: None }),
                            Some((response, drafts)) => {
                                st.etag = response.etag;
                                st.last_modified = response.last_modified;
                                let new = data.ingest(&source, drafts, at, &options.retention);
                                reports.push(SourceReport { id: source.id.clone(), status: "ok", new: new.len(), error: None });
                                new_ids.extend(new);
                            }
                        }
                    }
                    Err(error) => {
                        st.failures = st.failures.saturating_add(1);
                        let backoff = failure_backoff(options, st.failures);
                        st.next_due = at + backoff;
                        st.last_error = Some(error.clone());
                        reports.push(SourceReport { id: source.id.clone(), status: "error", new: 0, error: Some(error) });
                    }
                }
            }
            let clusters = clusters(data, &new_ids);
            let now = self.now();
            data.prune(now, &options.retention);
            data.updated = now;
            let saved = store.save(data);
            (saved, now, data.items.len(), new_ids.len(), clusters)
        })?;
        let (saved, at, total, new, clusters) = report;
        saved?;
        let report = FetchReport { at, new, total, clusters, sources: reports };
        if report.sources.iter().any(|s| s.status != "skipped") {
            if let Some(hook) = &options.on_fetch {
                hook(&report);
            }
        }
        Ok(Some(report))
    }

    /// One source: its declared host only, paced per host, conditional on
    /// its last answer, following a redirect only to a declared host.
    /// `Ok(None)` is "not modified".
    fn fetch_source(&self, source: &Source, st: &store::SourceState, last_request: &mut HashMap<String, Instant>) -> Result<Option<fetch::Response>, String> {
        let options = &self.core.options;
        let mut request = fetch::Request { url: source.url.clone(), etag: st.etag.clone(), last_modified: st.last_modified.clone() };
        for _ in 0..4 {
            let host = self.allowed(&request.url).ok_or_else(|| format!("{} is not a host the News manifest declares", sources::host_of(&request.url).unwrap_or_default()))?;
            let spacing = if host == "api.gdeltproject.org" { options.gdelt_spacing } else { options.host_spacing };
            if let Some(last) = last_request.get(&host) {
                let wait = spacing.saturating_sub(last.elapsed());
                if !wait.is_zero() {
                    std::thread::sleep(wait);
                }
            }
            last_request.insert(host, Instant::now());
            let response = options.fetcher.get(&request)?;
            match response.status {
                200 => return Ok(Some(response)),
                304 => return Ok(None),
                301 | 302 | 303 | 307 | 308 => {
                    let location = response.location.ok_or("a redirect without a location")?;
                    let next = url::Url::parse(&request.url).and_then(|base| base.join(&location)).map_err(|e| format!("a bad redirect: {e}"))?;
                    request = fetch::Request { url: next.to_string(), etag: None, last_modified: None };
                }
                429 => return Err("the source asked the service to slow down (429)".into()),
                status => return Err(format!("the source answered {status}")),
            }
        }
        Err("too many redirects".into())
    }
}

/// The new stories of a run by topic tag, largest first.
fn clusters(data: &store::Data, new_ids: &[String]) -> Vec<Cluster> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for id in new_ids {
        for topic in data.item(id).map(|i| i.topics.as_slice()).unwrap_or_default() {
            *counts.entry(topic.clone()).or_default() += 1;
        }
    }
    let mut clusters: Vec<Cluster> = counts.into_iter().map(|(topic, count)| Cluster { topic, count }).collect();
    clusters.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.topic.cmp(&b.topic)));
    clusters
}

/// `news.list`'s arguments.
pub fn query_from(args: &Value) -> Query {
    let text = |k: &str| args[k].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let number = |k: &str| args[k].as_f64().filter(|n| n.is_finite() && *n >= 0.0).map(|n| n as i64);
    Query {
        since: number("since"),
        topic: text("topic"),
        lang: text("lang"),
        feed: text("feed"),
        current: args["current"].as_bool().unwrap_or(false),
        limit: number("limit").map_or(50, |n| n as usize),
        offset: number("offset").map_or(0, |n| n as usize),
    }
}

/// How long a source waits after its `failures`-th failure in a row:
/// [`Options::retries`]' first retry, doubling, at most the maximum.
fn failure_backoff(options: &Options, failures: u32) -> i64 {
    let doublings = failures.saturating_sub(1).min(16);
    options.retry_secs.max(1).saturating_mul(1i64 << doublings).min(options.max_backoff_secs)
}

/// The host service over a [`News`].
pub struct NewsService {
    news: News,
}

/// A worker thread for anything that may touch the network, so the UI
/// thread never waits.
fn work(f: impl FnOnce() + Send + 'static) {
    std::thread::spawn(f);
}

impl HostService for NewsService {
    fn family(&self) -> &'static str {
        "news"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        if !may_call(&call.app_id) {
            reply.send(Err("The news service serves system apps only.".into()));
            return;
        }
        let news = self.news.clone();
        news.attach(&call.host_dir);
        let method = call.method().to_string();
        let args = call.args;
        match method.as_str() {
            "list" => reply.send(news.list(&query_from(&args))),
            "read" => {
                let id = args["id"].as_str().unwrap_or("").to_string();
                let full = args["full"].as_bool().unwrap_or(false);
                if full && news.core.options.reader.is_some() {
                    work(move || reply.send(news.read(&id, true)));
                } else {
                    reply.send(news.read(&id, false));
                }
            }
            "topics.get" => reply.send(news.topics().map(|topics| json!({"topics": topics}))),
            "topics.set" => {
                let topics = args["topics"].as_array().cloned().unwrap_or_default();
                reply.send(news.set_topics(&topics).map(|topics| json!({"topics": topics})));
            }
            "refresh" => work(move || {
                reply.send(news.refresh().map(|report| match report {
                    Some(report) => json!(report),
                    None => json!({"busy": true}),
                }))
            }),
            "sources" => reply.send(news.source_status()),
            "feeds.import" => reply.send(news.import_opml(args["opml"].as_str().unwrap_or(""))),
            other => reply.send(Err(format!("news has no method {other:?}"))),
        }
    }
}
