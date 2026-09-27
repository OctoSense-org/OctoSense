//! **Interim** live backend for `mod.research` (feature `live`).
//!
//! A deliberately small adapter over what exists today, to be replaced by the
//! octos research engine (octos#2568) and metasearch (octos#2576). It follows
//! the toolbox policy of ADR 0002 section 6:
//!
//! - **free structured sources only**: Google News RSS search, the GDELT DOC
//!   API and configured RSS/Atom feeds; no results-page scraping;
//! - **polite fetching**: an honest User-Agent naming OctoSense, robots.txt
//!   honoured for every host (unreachable robots.txt means "do not fetch"),
//!   a minimum interval per host, redirects followed by hand (each hop
//!   checked), a response size cap and a timeout;
//! - **plain HTTP reading** with main-text extraction by `dom_smoothie`
//!   (MIT, a Rust port of Mozilla's readability.js). No browser: pages that
//!   need JavaScript (Google News article links among them) fail honestly as
//!   partial results.

use super::{FoundItem, PageText, ResearchBackend, SearchQuery, SearchResults};
use crate::host::{url_host, CallContext, HostError, HostFuture};
use futures_util::future::join_all;
use quick_xml::events::Event;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The User-Agent every request carries: who we are and where to read more.
pub const USER_AGENT: &str =
    "OctoSense-Toolbox/0.1 (research workflows; +https://github.com/OctoSense-org/OctoSense)";
/// The product token matched against robots.txt groups.
pub const ROBOTS_TOKEN: &str = "octosense-toolbox";

/// A configured RSS or Atom feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub url: String,
    pub name: String,
    pub language: String,
}

#[derive(Debug, Clone)]
pub struct LiveConfig {
    /// Google News RSS search. Off by default: news.google.com's robots.txt
    /// disallows `/rss` for every agent but a few, and this adapter honours it
    /// (turning it on only makes each search report it as refused).
    pub google_news: bool,
    pub gdelt: bool,
    pub feeds: Vec<Feed>,
    /// Minimum time between two requests to one host.
    pub min_interval: Duration,
    /// Longer intervals for hosts that publish one (GDELT asks for 5 s).
    pub host_intervals: Vec<(String, Duration)>,
    pub timeout: Duration,
    pub max_response_bytes: usize,
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            google_news: false,
            gdelt: true,
            feeds: Vec::new(),
            min_interval: Duration::from_secs(1),
            host_intervals: vec![("api.gdeltproject.org".into(), Duration::from_secs(5))],
            timeout: Duration::from_secs(15),
            max_response_bytes: 2 * 1024 * 1024,
        }
    }
}

/// robots.txt rules: (allow, path pattern).
type Rules = Vec<(bool, String)>;

/// Parsed robots.txt rules for our group: (allow, path prefix).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Robots {
    rules: Rules,
    deny_all: bool,
}

impl Robots {
    /// Parses robots.txt (RFC 9309): the group naming our token, else `*`.
    pub fn parse(text: &str, token: &str) -> Robots {
        let mut groups: Vec<(Vec<String>, Rules)> = Vec::new();
        let mut in_agents = false;
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim().to_owned();
            match key.as_str() {
                "user-agent" => {
                    if !in_agents {
                        groups.push((Vec::new(), Vec::new()));
                    }
                    in_agents = true;
                    if let Some(group) = groups.last_mut() {
                        group.0.push(value.to_ascii_lowercase());
                    }
                }
                "allow" | "disallow" => {
                    in_agents = false;
                    if let Some(group) = groups.last_mut() {
                        if !value.is_empty() {
                            group.1.push((key == "allow", value));
                        }
                    }
                }
                _ => in_agents = false,
            }
        }
        let token = token.to_ascii_lowercase();
        let pick = |pred: &dyn Fn(&str) -> bool| {
            let rules: Rules = groups
                .iter()
                .filter(|g| g.0.iter().any(|a| pred(a)))
                .flat_map(|g| g.1.clone())
                .collect();
            let named = groups.iter().any(|g| g.0.iter().any(|a| pred(a)));
            named.then_some(rules)
        };
        let rules = pick(&|a: &str| a != "*" && token.starts_with(a))
            .or_else(|| pick(&|a: &str| a == "*"))
            .unwrap_or_default();
        Robots {
            rules,
            deny_all: false,
        }
    }

    fn deny_all() -> Robots {
        Robots {
            rules: Vec::new(),
            deny_all: true,
        }
    }

    /// Longest matching rule wins; on a tie, allow. `*` and `$` are honoured.
    pub fn allows(&self, path: &str) -> bool {
        if self.deny_all {
            return false;
        }
        let mut best: Option<(usize, bool)> = None;
        for (allow, pattern) in &self.rules {
            if robots_match(pattern, path) {
                let len = pattern.len();
                best = match best {
                    Some((l, a)) if l > len || (l == len && a) => Some((l, a)),
                    _ => Some((len, *allow)),
                };
            }
        }
        best.is_none_or(|(_, allow)| allow)
    }
}

fn robots_match(pattern: &str, path: &str) -> bool {
    let (pattern, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let parts: Vec<&str> = pattern.split('*').collect();
    let last = parts.len() - 1;
    let mut at = 0;
    for (i, part) in parts.iter().enumerate() {
        if anchored && i == last && i > 0 {
            return path[at..].ends_with(part);
        }
        if i == 0 {
            if !path.starts_with(part) {
                return false;
            }
            at = part.len();
        } else if let Some(found) = path[at..].find(part) {
            at += found + part.len();
        } else {
            return false;
        }
    }
    !anchored || at == path.len()
}

/// The interim live backend.
pub struct InterimResearch {
    client: reqwest::Client,
    config: LiveConfig,
    robots: Mutex<HashMap<String, Arc<Robots>>>,
    next_slot: Mutex<HashMap<String, Instant>>,
}

fn failed(message: impl Into<String>) -> HostError {
    HostError::Failed(message.into())
}

impl InterimResearch {
    pub fn new(config: LiveConfig) -> Result<Self, HostError> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(config.timeout)
            .build()
            .map_err(|e| failed(e.to_string()))?;
        Ok(Self {
            client,
            config,
            robots: Mutex::new(HashMap::new()),
            next_slot: Mutex::new(HashMap::new()),
        })
    }

    /// Waits for this host's next polite slot.
    async fn pace(&self, host: &str) {
        let wait = {
            let mut slots = self.next_slot.lock().unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            let slot = slots.get(host).copied().unwrap_or(now).max(now);
            let interval = self
                .config
                .host_intervals
                .iter()
                .find(|(h, _)| h == host)
                .map_or(self.config.min_interval, |(_, i)| *i);
            slots.insert(host.to_owned(), slot + interval);
            slot - now
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    async fn raw_get(
        &self,
        url: &str,
    ) -> Result<(u16, Option<String>, String, Vec<u8>), HostError> {
        let host = url_host(url).ok_or_else(|| failed(format!("not an http(s) URL: {url}")))?;
        self.pace(&host).await;
        let mut response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| failed(format!("{host}: {e}")))?;
        let status = response.status().as_u16();
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| failed(e.to_string()))? {
            if body.len() + chunk.len() > self.config.max_response_bytes {
                return Err(failed(format!("{host}: response larger than the cap")));
            }
            body.extend_from_slice(&chunk);
        }
        Ok((status, location, content_type, body))
    }

    async fn robots_for(&self, url: &str) -> Arc<Robots> {
        let Some(host) = url_host(url) else {
            return Arc::new(Robots::deny_all());
        };
        if let Some(cached) = self
            .robots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&host)
        {
            return cached.clone();
        }
        let scheme = if url.starts_with("http://") {
            "http"
        } else {
            "https"
        };
        let robots = match self.raw_get(&format!("{scheme}://{host}/robots.txt")).await {
            Ok((200..=299, _, _, body)) => {
                Robots::parse(&String::from_utf8_lossy(&body), ROBOTS_TOKEN)
            }
            // RFC 9309: 4xx means no restrictions; anything else, stay out.
            Ok((400..=499, _, _, _)) => Robots::default(),
            _ => Robots::deny_all(),
        };
        let robots = Arc::new(robots);
        self.robots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(host, robots.clone());
        robots
    }

    /// GET with robots.txt, pacing and at most three redirects, each hop
    /// checked. Returns (final URL, content type, body).
    pub async fn get(&self, url: &str) -> Result<(String, String, Vec<u8>), HostError> {
        let mut current = url.to_owned();
        for _ in 0..4 {
            let path = path_of(&current);
            if !self.robots_for(&current).await.allows(&path) {
                return Err(HostError::Denied(format!("robots.txt disallows {current}")));
            }
            let (status, location, content_type, body) = self.raw_get(&current).await?;
            match status {
                200..=299 => return Ok((current, content_type, body)),
                301 | 302 | 303 | 307 | 308 => {
                    let next = location.ok_or_else(|| failed("redirect without Location"))?;
                    current = url::Url::parse(&current)
                        .and_then(|base| base.join(&next))
                        .map_err(|e| failed(e.to_string()))?
                        .to_string();
                }
                _ => return Err(failed(format!("HTTP {status} from {current}"))),
            }
        }
        Err(failed("too many redirects"))
    }

    async fn provider(
        &self,
        name: &str,
        url: String,
        feed: Option<&Feed>,
    ) -> Result<Vec<FoundItem>, HostError> {
        let (_, _, body) = self.get(&url).await?;
        if name == "gdelt" {
            return parse_gdelt(&body);
        }
        let mut items = parse_feed(&body)?;
        for item in &mut items {
            item.via = name.to_owned();
            if let Some(feed) = feed {
                if item.source.is_empty() {
                    item.source = feed.name.clone();
                }
                if item.language.is_empty() {
                    item.language = feed.language.clone();
                }
            }
        }
        Ok(items)
    }
}

fn path_of(url: &str) -> String {
    url::Url::parse(url)
        .map(|u| {
            let mut p = u.path().to_owned();
            if let Some(q) = u.query() {
                p.push('?');
                p.push_str(q);
            }
            p
        })
        .unwrap_or_else(|_| "/".into())
}

fn encode(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

fn gdelt_language(code: &str) -> Option<&'static str> {
    Some(
        match code.split('-').next()?.to_ascii_lowercase().as_str() {
            "en" => "english",
            "es" => "spanish",
            "fr" => "french",
            "de" => "german",
            "it" => "italian",
            "pt" => "portuguese",
            "zh" => "chinese",
            "ja" => "japanese",
            "ko" => "korean",
            "ru" => "russian",
            "ar" => "arabic",
            _ => return None,
        },
    )
}

/// RFC 2822, RFC 3339 or GDELT's `YYYYMMDDTHHMMSSZ`, as RFC 3339 UTC.
pub fn normalize_date(text: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let text = text.trim();
    chrono::DateTime::parse_from_rfc2822(text)
        .or_else(|_| chrono::DateTime::parse_from_rfc3339(text))
        .map(|d| d.with_timezone(&chrono::Utc))
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(text, "%Y%m%dT%H%M%SZ")
                .ok()
                .map(|d| d.and_utc())
        })
}

/// Items of an RSS 2.0 or Atom document.
pub fn parse_feed(bytes: &[u8]) -> Result<Vec<FoundItem>, HostError> {
    let mut reader = quick_xml::Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut items = Vec::new();
    let mut current: Option<FoundItem> = None;
    let mut field = String::new();
    let mut feed_language = String::new();
    let mut buf = Vec::new();
    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| failed(format!("feed: {e}")))?;
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_ascii_lowercase();
                if name == "item" || name == "entry" {
                    current = Some(FoundItem {
                        url: String::new(),
                        title: String::new(),
                        source: String::new(),
                        language: feed_language.clone(),
                        published_at: String::new(),
                        via: String::new(),
                    });
                } else if name == "link" {
                    if let Some(item) = current.as_mut() {
                        let mut href = None;
                        let mut rel = None;
                        for attr in e.attributes().flatten() {
                            let key = attr.key.local_name();
                            let value = attr
                                .unescape_value()
                                .map(|v| v.into_owned())
                                .unwrap_or_default();
                            match key.as_ref() {
                                b"href" => href = Some(value),
                                b"rel" => rel = Some(value),
                                _ => {}
                            }
                        }
                        if let Some(href) = href {
                            if rel.as_deref().is_none_or(|r| r == "alternate")
                                && item.url.is_empty()
                            {
                                item.url = href;
                            }
                        }
                    }
                }
                field = name;
            }
            Event::Text(t) => {
                let text = t.unescape().map(|v| v.into_owned()).unwrap_or_default();
                match current.as_mut() {
                    Some(item) => match field.as_str() {
                        "title" if item.title.is_empty() => item.title = text,
                        "link" if item.url.is_empty() => item.url = text,
                        "pubdate" | "published" | "updated" | "date"
                            if item.published_at.is_empty() =>
                        {
                            item.published_at = normalize_date(&text)
                                .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
                                .unwrap_or_default();
                        }
                        "source" if item.source.is_empty() => item.source = text,
                        _ => {}
                    },
                    None if field == "language" => feed_language = text,
                    None => {}
                }
            }
            Event::CData(t) => {
                if let (Some(item), "title") = (current.as_mut(), field.as_str()) {
                    if item.title.is_empty() {
                        item.title = String::from_utf8_lossy(&t).into_owned();
                    }
                }
            }
            Event::End(e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_ascii_lowercase();
                if name == "item" || name == "entry" {
                    if let Some(item) = current.take() {
                        if !item.url.is_empty() {
                            items.push(item);
                        }
                    }
                }
                field.clear();
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(items)
}

/// Articles from a GDELT DOC API `mode=artlist&format=json` response.
pub fn parse_gdelt(bytes: &[u8]) -> Result<Vec<FoundItem>, HostError> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| failed(format!("gdelt: {e}")))?;
    Ok(value["articles"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|article| {
                    Some(FoundItem {
                        url: article["url"].as_str()?.to_owned(),
                        title: article["title"].as_str().unwrap_or_default().to_owned(),
                        source: article["domain"].as_str().unwrap_or_default().to_owned(),
                        language: String::new(),
                        published_at: article["seendate"]
                            .as_str()
                            .and_then(normalize_date)
                            .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
                            .unwrap_or_default(),
                        via: "gdelt".into(),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

impl ResearchBackend for InterimResearch {
    fn search<'a>(
        &'a self,
        _ctx: &'a CallContext,
        query: SearchQuery,
    ) -> HostFuture<'a, Result<SearchResults, HostError>> {
        Box::pin(async move {
            let hours = query.max_age_hours.unwrap_or(72);
            let language = query.language.clone().unwrap_or_else(|| "en".into());
            let region = query
                .region
                .clone()
                .unwrap_or_else(|| "US".into())
                .to_ascii_uppercase();
            let mut calls: Vec<(String, String, Option<&Feed>)> = Vec::new();
            if self.config.gdelt {
                let mut q = query.topic.clone();
                if let Some(name) = gdelt_language(&language) {
                    q.push_str(&format!(" sourcelang:{name}"));
                }
                calls.push((
                    "gdelt".into(),
                    format!(
                        "https://api.gdeltproject.org/api/v2/doc/doc?query={}&mode=artlist&format=json&maxrecords={}&timespan={hours}h&sort=datedesc",
                        encode(&q),
                        (query.limit * 2).min(50)
                    ),
                    None,
                ));
            }
            if self.config.google_news {
                calls.push((
                    "google-news-rss".into(),
                    format!(
                        "https://news.google.com/rss/search?q={}&hl={}&gl={region}&ceid={region}:{}",
                        encode(&format!("{} when:{hours}h", query.topic)),
                        encode(&language),
                        encode(language.split('-').next().unwrap_or("en"))
                    ),
                    None,
                ));
            }
            for feed in &self.config.feeds {
                if feed.language.is_empty() || feed.language.eq_ignore_ascii_case(&language) {
                    calls.push((format!("feed:{}", feed.name), feed.url.clone(), Some(feed)));
                }
            }
            let mut partial = calls.len() as u32 > query.max_pages;
            calls.truncate(query.max_pages as usize);
            let pages = calls.len() as u32;
            let results = join_all(
                calls
                    .iter()
                    .map(|(name, url, feed)| self.provider(name, url.clone(), *feed)),
            )
            .await;
            let terms: Vec<String> = query
                .topic
                .split_whitespace()
                .filter(|w| w.chars().count() >= 2)
                .map(str::to_lowercase)
                .collect();
            let cutoff = chrono::Utc::now() - chrono::Duration::hours(i64::from(hours));
            let mut providers = Vec::new();
            let mut errors = Vec::new();
            let mut lists = Vec::new();
            for ((name, _, feed), result) in calls.iter().zip(results) {
                match result {
                    Ok(items) => {
                        providers.push(name.clone());
                        let items: Vec<FoundItem> = items
                            .into_iter()
                            .filter(|i| normalize_date(&i.published_at).is_none_or(|d| d >= cutoff))
                            .filter(|i| {
                                // Configured feeds are not search engines: keep
                                // only items that mention the query.
                                feed.is_none() || {
                                    let title = i.title.to_lowercase();
                                    terms.iter().any(|t| title.contains(t))
                                }
                            })
                            .collect();
                        lists.push(items);
                    }
                    Err(e) => {
                        partial = true;
                        errors.push(format!("{name}: {e}"));
                    }
                }
            }
            if providers.is_empty() && pages > 0 {
                return Err(failed(format!(
                    "every provider failed ({})",
                    errors.join("; ")
                )));
            }
            // Interleave providers so one source does not fill the list.
            let mut items = Vec::new();
            let longest = lists.iter().map(Vec::len).max().unwrap_or(0);
            for round in 0..longest {
                for list in &lists {
                    if let Some(item) = list.get(round) {
                        items.push(item.clone());
                    }
                }
            }
            items.truncate((query.limit * 3) as usize);
            Ok(SearchResults {
                items,
                providers,
                partial,
                pages,
            })
        })
    }

    fn read<'a>(
        &'a self,
        _ctx: &'a CallContext,
        item: &'a FoundItem,
    ) -> HostFuture<'a, Result<PageText, HostError>> {
        Box::pin(async move {
            if url_host(&item.url).as_deref() == Some("news.google.com") {
                return Err(failed(
                    "Google News article links need a browser to resolve; not read",
                ));
            }
            let (final_url, content_type, body) = self.get(&item.url).await?;
            if !content_type.is_empty() && !content_type.contains("html") {
                return Err(failed(format!("not an HTML page ({content_type})")));
            }
            let html = String::from_utf8_lossy(&body).into_owned();
            let mut readability =
                dom_smoothie::Readability::new(html, Some(final_url.as_str()), None)
                    .map_err(|e| failed(format!("extraction: {e}")))?;
            let article = readability
                .parse()
                .map_err(|e| failed(format!("no main text: {e}")))?;
            let text = article.text_content.to_string();
            let text = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            Ok(PageText {
                text,
                title: Some(article.title.to_string()).filter(|t| !t.is_empty()),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robots_groups_and_longest_match() {
        let robots = Robots::parse(
            "User-agent: *\nDisallow: /private\nAllow: /private/ok\n\nUser-agent: OctoSense-Toolbox\nDisallow: /no-bots\n",
            ROBOTS_TOKEN,
        );
        // Our own group applies, not `*`.
        assert!(!robots.allows("/no-bots/x"));
        assert!(robots.allows("/private/x"));
        let star = Robots::parse(
            "User-agent: *\nDisallow: /private\nAllow: /private/ok\nDisallow: /*.pdf$\n",
            ROBOTS_TOKEN,
        );
        assert!(!star.allows("/private/x"));
        assert!(star.allows("/private/ok/x"));
        assert!(!star.allows("/files/a.pdf"));
        assert!(star.allows("/files/a.pdf?x"));
        assert!(star.allows("/"));
        assert!(!Robots::deny_all().allows("/"));
    }

    #[test]
    fn feeds_parse() {
        let rss = br#"<?xml version="1.0"?><rss><channel><language>en</language>
            <item><title>One &amp; two</title><link>https://a.example/1</link>
            <pubDate>Sat, 19 Sep 2026 10:00:00 GMT</pubDate><source url="https://a.example">A News</source></item>
            </channel></rss>"#;
        let items = parse_feed(rss).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "One & two");
        assert_eq!(items[0].url, "https://a.example/1");
        assert_eq!(items[0].published_at, "2026-09-19T10:00:00Z");
        assert_eq!(items[0].source, "A News");
        assert_eq!(items[0].language, "en");
        let atom = br#"<feed xmlns="http://www.w3.org/2005/Atom"><entry><title>T</title>
            <link rel="alternate" href="https://b.example/2"/><updated>2026-09-19T10:00:00Z</updated></entry></feed>"#;
        let items = parse_feed(atom).unwrap();
        assert_eq!(items[0].url, "https://b.example/2");
        let gdelt = br#"{"articles":[{"url":"https://c.example/3","title":"G","domain":"c.example","seendate":"20260919T101500Z"}]}"#;
        let items = parse_gdelt(gdelt).unwrap();
        assert_eq!(items[0].published_at, "2026-09-19T10:15:00Z");
    }
}
