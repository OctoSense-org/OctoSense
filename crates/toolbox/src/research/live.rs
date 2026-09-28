//! **Interim** live backend for `mod.research` (feature `live`).
//!
//! A deliberately small adapter over what exists today, to be replaced by the
//! octos research engine (octos#2568) and metasearch (octos#2576). It follows
//! the toolbox policy of ADR 0002 section 6:
//!
//! - **free structured sources only**: Google News RSS search, the GDELT DOC
//!   API and configured RSS/Atom feeds; no results-page scraping;
//! - **a personal assistant's fetching**: OctoSense agents read on behalf of
//!   one person, so robots.txt is **not** applied by default (for feeds,
//!   person-initiated reads and autonomous research alike). It is an operator
//!   setting ([`LiveConfig::respect_robots`], or `OCTOSENSE_TOOLBOX_ROBOTS=1`
//!   through [`LiveConfig::from_env`]); when off, robots.txt is never fetched;
//! - **polite and safe fetching, always**: an honest User-Agent naming
//!   OctoSense and octos, a minimum interval per host (5 s for GDELT),
//!   backoff on 429/503 honouring `Retry-After`, a timeout, a response size
//!   cap, no cookies or credentials (no paywall or login bypass), and
//!   private, loopback, link-local and cloud-metadata addresses refused on
//!   every fetch and every redirect hop (URL literals and DNS answers);
//! - **plain HTTP reading** with main-text extraction by `dom_smoothie`
//!   (MIT, a Rust port of Mozilla's readability.js). No browser: pages that
//!   need JavaScript (Google News article links among them) fail honestly as
//!   partial results.

use super::{FoundItem, PageText, ResearchBackend, SearchQuery, SearchResults};
use crate::host::{url_host, CallContext, HostError, HostFuture};
use futures_util::future::join_all;
use quick_xml::events::Event;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The User-Agent every request carries: who we are and where to read more.
pub const USER_AGENT: &str = "OctoSense-Toolbox/0.1 (octos research for one person; +https://github.com/OctoSense-org/OctoSense)";
/// The product token matched against robots.txt groups (when enabled).
pub const ROBOTS_TOKEN: &str = "octosense-toolbox";
/// Set to `1` (or `true`) to make [`LiveConfig::from_env`] honour robots.txt.
pub const ROBOTS_ENV: &str = "OCTOSENSE_TOOLBOX_ROBOTS";
/// The longest `Retry-After` the adapter waits for; longer means failure.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);
/// Retries after a 429 or 503.
pub const MAX_RETRIES: u32 = 2;

/// Whether an address is off limits: loopback, private, link-local (cloud
/// metadata endpoints live there), unique-local, shared (CGNAT),
/// unspecified, multicast, broadcast, documentation and benchmarking ranges,
/// and IPv6 forms of any of those.
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => blocked_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return blocked_v4(v4);
            }
            let first = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00 // unique local fc00::/7
                || (first & 0xffc0) == 0xfe80 // link local fe80::/10
                || (first & 0xffc0) == 0xfec0 // site local (deprecated)
                || (first == 0x2001 && v6.segments()[1] == 0x0db8) // documentation
                || (first == 0x0064 && v6.segments()[1] == 0xff9b) // NAT64 to v4
                || v6 == Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254) // AWS IMDS v6
        }
    }
}

fn blocked_v4(ip: Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local() // 169.254.0.0/16, incl. 169.254.169.254
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        || (a == 100 && (64..=127).contains(&b)) // shared address space
        || (a == 198 && (18..=19).contains(&b)) // benchmarking
        || a >= 240 // reserved
        || (a == 192 && b == 0 && ip.octets()[2] == 0) // IETF protocol assignments
}

/// A DNS resolver that drops blocked addresses, so neither the first request
/// nor any redirect can reach an internal host by name (DNS rebinding
/// included: the connection uses exactly these answers).
struct GuardedResolver {
    /// Tests only: also allow loopback (never private or link-local).
    allow_loopback: bool,
}

fn allowed(ip: IpAddr, allow_loopback: bool) -> bool {
    !is_blocked_ip(ip) || (allow_loopback && ip.is_loopback())
}

impl reqwest::dns::Resolve for GuardedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        let allow_loopback = self.allow_loopback;
        Box::pin(async move {
            let answers: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|a| allowed(a.ip(), allow_loopback))
                .collect();
            if answers.is_empty() {
                return Err(format!("{host} resolves only to blocked addresses").into());
            }
            let addrs: reqwest::dns::Addrs = Box::new(answers.into_iter());
            Ok(addrs)
        })
    }
}

/// A configured RSS or Atom feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub url: String,
    pub name: String,
    pub language: String,
}

#[derive(Debug, Clone)]
pub struct LiveConfig {
    /// Google News RSS search (a default free source).
    pub google_news: bool,
    pub gdelt: bool,
    pub feeds: Vec<Feed>,
    /// Operator setting: honour robots.txt. Off by default; when off,
    /// robots.txt is never fetched.
    pub respect_robots: bool,
    /// Minimum time between two requests to one host.
    pub min_interval: Duration,
    /// Longer intervals for hosts that publish one (GDELT asks for 5 s).
    pub host_intervals: Vec<(String, Duration)>,
    pub timeout: Duration,
    pub max_response_bytes: usize,
    /// **Tests only**: allow loopback addresses so a test can serve pages
    /// from 127.0.0.1. Private, link-local and metadata addresses stay
    /// blocked. Never set in production; there is no environment switch.
    #[doc(hidden)]
    pub allow_loopback_for_tests: bool,
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            google_news: true,
            gdelt: true,
            feeds: Vec::new(),
            respect_robots: false,
            min_interval: Duration::from_secs(1),
            host_intervals: vec![("api.gdeltproject.org".into(), Duration::from_secs(5))],
            timeout: Duration::from_secs(15),
            max_response_bytes: 2 * 1024 * 1024,
            allow_loopback_for_tests: false,
        }
    }
}

impl LiveConfig {
    /// The defaults, with the operator's robots.txt setting from
    /// `OCTOSENSE_TOOLBOX_ROBOTS` (`1` or `true` turns it on).
    pub fn from_env() -> Self {
        let respect_robots = std::env::var(ROBOTS_ENV)
            .map(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false);
        Self {
            respect_robots,
            ..Self::default()
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
        // No cookie store, no credentials, no proxy (the resolver must see
        // the real host), redirects by hand.
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .dns_resolver(Arc::new(GuardedResolver {
                allow_loopback: config.allow_loopback_for_tests,
            }))
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

    /// Pushes this host's next slot to at least `delay` from now (backoff).
    fn defer(&self, host: &str, delay: Duration) {
        let mut slots = self.next_slot.lock().unwrap_or_else(|e| e.into_inner());
        let at = Instant::now() + delay;
        let slot = slots.entry(host.to_owned()).or_insert(at);
        *slot = (*slot).max(at);
    }

    /// Refuses a URL whose host is an IP literal in a blocked range. Names
    /// are checked by the resolver on every connection.
    fn check_target(&self, url: &str) -> Result<String, HostError> {
        let parsed = url::Url::parse(url).map_err(|e| HostError::Denied(format!("{url}: {e}")))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(HostError::Denied(format!("not an http(s) URL: {url}")));
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(HostError::Denied(
                "URLs with credentials are not fetched".into(),
            ));
        }
        let ip = match parsed.host() {
            Some(url::Host::Ipv4(v4)) => Some(IpAddr::V4(v4)),
            Some(url::Host::Ipv6(v6)) => Some(IpAddr::V6(v6)),
            Some(url::Host::Domain(d)) => {
                let d = d.trim_end_matches('.').to_ascii_lowercase();
                if (d == "localhost" || d.ends_with(".localhost"))
                    && !self.config.allow_loopback_for_tests
                {
                    return Err(HostError::Denied(format!("{d} is a local host")));
                }
                None
            }
            None => return Err(HostError::Denied(format!("no host in {url}"))),
        };
        if let Some(ip) = ip {
            if !allowed(ip, self.config.allow_loopback_for_tests) {
                return Err(HostError::Denied(format!(
                    "{ip} is a private, local or metadata address"
                )));
            }
        }
        url_host(url).ok_or_else(|| HostError::Denied(format!("no host in {url}")))
    }

    /// One GET. Returns (status, Location or, for 429/503, Retry-After,
    /// content type, body).
    async fn raw_get(
        &self,
        url: &str,
    ) -> Result<(u16, Option<String>, String, Vec<u8>), HostError> {
        let host = self.check_target(url)?;
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
        if matches!(status, 429 | 503) {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            // The body is not needed; the caller backs off.
            return Ok((status, retry_after, content_type, Vec::new()));
        }
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
        // Per origin: scheme, host and port.
        let Some(robots_url) = url::Url::parse(url)
            .ok()
            .filter(|u| u.has_host())
            .and_then(|u| u.join("/robots.txt").ok())
        else {
            return Arc::new(Robots::deny_all());
        };
        let host = robots_url.origin().ascii_serialization();
        if let Some(cached) = self
            .robots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&host)
        {
            return cached.clone();
        }
        let robots = match self.raw_get(robots_url.as_str()).await {
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

    /// GET with pacing, backoff on 429/503, robots.txt when the operator
    /// turned it on, and at most three redirects, each hop checked again.
    /// Returns (final URL, content type, body).
    pub async fn get(&self, url: &str) -> Result<(String, String, Vec<u8>), HostError> {
        let mut current = url.to_owned();
        let mut retries = 0;
        let mut hops = 0;
        loop {
            if self.config.respect_robots {
                let path = path_of(&current);
                if !self.robots_for(&current).await.allows(&path) {
                    return Err(HostError::Denied(format!("robots.txt disallows {current}")));
                }
            }
            let (status, location, content_type, body) = self.raw_get(&current).await?;
            match status {
                200..=299 => return Ok((current, content_type, body)),
                429 | 503 => {
                    let wait = location
                        .as_deref()
                        .and_then(parse_retry_after)
                        .unwrap_or(Duration::from_secs(2 << retries));
                    if retries >= MAX_RETRIES || wait > MAX_RETRY_AFTER {
                        return Err(failed(format!(
                            "HTTP {status} from {current}; retry after {}s",
                            wait.as_secs()
                        )));
                    }
                    retries += 1;
                    if let Some(host) = url_host(&current) {
                        self.defer(&host, wait);
                    }
                    continue;
                }
                301 | 302 | 303 | 307 | 308 if hops < 3 => {
                    hops += 1;
                    let next = location.ok_or_else(|| failed("redirect without Location"))?;
                    // The next hop is checked by `raw_get` like the first.
                    current = url::Url::parse(&current)
                        .and_then(|base| base.join(&next))
                        .map_err(|e| failed(e.to_string()))?
                        .to_string();
                }
                301 | 302 | 303 | 307 | 308 => return Err(failed("too many redirects")),
                // 401/402/403 and the like: no login or paywall workaround.
                _ => return Err(failed(format!("HTTP {status} from {current}"))),
            }
        }
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

/// `Retry-After` as seconds or an HTTP date.
pub fn parse_retry_after(value: &str) -> Option<Duration> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let at = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let delta = at.with_timezone(&chrono::Utc) - chrono::Utc::now();
    Some(delta.to_std().unwrap_or(Duration::ZERO))
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

/// Links this adapter cannot read without a browser: Google News article
/// links resolve to the publisher only through JavaScript. Search marks them
/// `readable: false` so templates skip them, and `read` refuses them.
pub fn needs_browser(url: &str) -> bool {
    url_host(url).as_deref() == Some("news.google.com")
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
                        readable: true,
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
                        readable: true,
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
                        // GDELT's `sourcelang:` and Google News's `hl` select
                        // the query's language, so their items are in it.
                        let tagged = match name.as_str() {
                            "gdelt" => gdelt_language(&language).is_some(),
                            "google-news-rss" => true,
                            _ => false,
                        };
                        let items: Vec<FoundItem> = items
                            .into_iter()
                            .map(|mut i| {
                                i.readable = !needs_browser(&i.url);
                                if tagged && i.language.is_empty() {
                                    i.language = language.clone();
                                }
                                i
                            })
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
            // Items this adapter can read first, so the cut keeps them.
            items.sort_by_key(|i| !i.readable);
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
            if needs_browser(&item.url) {
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
    fn google_news_links_need_a_browser() {
        assert!(needs_browser(
            "https://news.google.com/rss/articles/CBMiXYZ?oc=5"
        ));
        assert!(!needs_browser("https://www.bbc.co.uk/news/articles/x"));
        assert!(!needs_browser("https://news.google.com.example.org/x"));
    }

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

    #[test]
    fn blocked_addresses() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.100.100.200",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd00:ec2::254",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
        ] {
            assert!(is_blocked_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["93.184.216.34", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(!is_blocked_ip(ip.parse().unwrap()), "{ip}");
        }
        assert_eq!(parse_retry_after("7"), Some(Duration::from_secs(7)));
        assert_eq!(
            parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT"),
            Some(Duration::ZERO)
        );
        assert_eq!(parse_retry_after("soon"), None);
    }

    #[test]
    fn robots_is_an_operator_setting_off_by_default() {
        let config = LiveConfig::default();
        assert!(!config.respect_robots);
        assert!(config.google_news && config.gdelt);
        std::env::set_var(ROBOTS_ENV, "1");
        assert!(LiveConfig::from_env().respect_robots);
        std::env::set_var(ROBOTS_ENV, "0");
        assert!(!LiveConfig::from_env().respect_robots);
        std::env::remove_var(ROBOTS_ENV);
        assert!(!LiveConfig::from_env().respect_robots);
    }

    /// A tiny HTTP/1.1 server on 127.0.0.1 that records request paths and
    /// replays scripted responses per path (the last one repeats).
    struct Server {
        base: String,
        paths: Arc<Mutex<Vec<String>>>,
    }

    type Reply = (u16, Vec<(&'static str, String)>, String);

    async fn serve(routes: Vec<(&'static str, Vec<Reply>)>) -> Server {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let paths = Arc::new(Mutex::new(Vec::new()));
        let routes: Arc<Mutex<HashMap<String, Vec<Reply>>>> = Arc::new(Mutex::new(
            routes.into_iter().map(|(p, r)| (p.to_owned(), r)).collect(),
        ));
        let seen = paths.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let mut request = Vec::new();
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    if stream.readable().await.is_err() {
                        break;
                    }
                    let mut buf = [0u8; 4096];
                    match stream.try_read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                        Err(_) => break,
                    }
                }
                let text = String::from_utf8_lossy(&request);
                let path = text.split_whitespace().nth(1).unwrap_or("/").to_owned();
                seen.lock().unwrap().push(path.clone());
                let (status, headers, body) = {
                    let mut routes = routes.lock().unwrap();
                    match routes.get_mut(&path) {
                        Some(replies) if replies.len() > 1 => replies.remove(0),
                        Some(replies) => replies[0].clone(),
                        None => (404, Vec::new(), String::new()),
                    }
                };
                let mut response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n",
                    body.len()
                );
                for (k, v) in headers {
                    response.push_str(&format!("{k}: {v}\r\n"));
                }
                response.push_str("\r\n");
                response.push_str(&body);
                let bytes = response.into_bytes();
                let mut written = 0;
                while written < bytes.len() {
                    if stream.writable().await.is_err() {
                        break;
                    }
                    match stream.try_write(&bytes[written..]) {
                        Ok(n) => written += n,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                        Err(_) => break,
                    }
                }
            }
        });
        Server { base, paths }
    }

    fn html() -> Reply {
        (
            200,
            vec![("content-type", "text/html".into())],
            "<html><body><p>Hello</p></body></html>".into(),
        )
    }

    fn local(respect_robots: bool) -> InterimResearch {
        InterimResearch::new(LiveConfig {
            respect_robots,
            min_interval: Duration::ZERO,
            allow_loopback_for_tests: true,
            ..LiveConfig::default()
        })
        .unwrap()
    }

    #[tokio::test]
    async fn by_default_robots_txt_is_never_requested() {
        let robots = (200, Vec::new(), "User-agent: *\nDisallow: /\n".to_owned());
        let server = serve(vec![("/robots.txt", vec![robots]), ("/a", vec![html()])]).await;
        let (_, content_type, body) = local(false)
            .get(&format!("{}/a", server.base))
            .await
            .unwrap();
        assert_eq!(content_type, "text/html");
        assert!(String::from_utf8_lossy(&body).contains("Hello"));
        assert_eq!(*server.paths.lock().unwrap(), vec!["/a".to_owned()]);
    }

    #[tokio::test]
    async fn with_the_setting_on_robots_txt_is_honoured_and_cached() {
        let robots = (
            200,
            Vec::new(),
            "User-agent: *\nDisallow: /private\n".to_owned(),
        );
        let server = serve(vec![
            ("/robots.txt", vec![robots]),
            ("/private/a", vec![html()]),
            ("/public", vec![html()]),
        ])
        .await;
        let adapter = local(true);
        let err = adapter
            .get(&format!("{}/private/a", server.base))
            .await
            .unwrap_err();
        assert!(matches!(err, HostError::Denied(_)), "{err}");
        adapter
            .get(&format!("{}/public", server.base))
            .await
            .unwrap();
        assert_eq!(
            *server.paths.lock().unwrap(),
            vec!["/robots.txt".to_owned(), "/public".to_owned()]
        );
    }

    #[tokio::test]
    async fn internal_addresses_are_refused_on_every_hop() {
        // Without the test switch even loopback is refused.
        let strict = InterimResearch::new(LiveConfig::default()).unwrap();
        for url in [
            "http://127.0.0.1:9/x",
            "http://localhost/x",
            "http://10.0.0.1/x",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]/x",
            "http://[fd00:ec2::254]/x",
            "http://user:secret@93.184.216.34/x",
            "file:///etc/passwd",
        ] {
            let err = strict.get(url).await.unwrap_err();
            assert!(matches!(err, HostError::Denied(_)), "{url}: {err}");
        }
        // A redirect to a metadata or private address is refused too.
        let server = serve(vec![
            (
                "/to-metadata",
                vec![(
                    302,
                    vec![(
                        "location",
                        "http://169.254.169.254/latest/meta-data/".into(),
                    )],
                    String::new(),
                )],
            ),
            (
                "/to-private",
                vec![(
                    301,
                    vec![("location", "http://192.168.1.1/admin".into())],
                    String::new(),
                )],
            ),
        ])
        .await;
        let adapter = local(false);
        for path in ["/to-metadata", "/to-private"] {
            let err = adapter
                .get(&format!("{}{path}", server.base))
                .await
                .unwrap_err();
            assert!(matches!(err, HostError::Denied(_)), "{path}: {err}");
        }
        assert_eq!(server.paths.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn backs_off_on_429_and_503_with_retry_after() {
        let server = serve(vec![
            (
                "/slow",
                vec![
                    (429, vec![("retry-after", "1".into())], String::new()),
                    html(),
                ],
            ),
            (
                "/busy",
                vec![(503, vec![("retry-after", "120".into())], String::new())],
            ),
        ])
        .await;
        let adapter = local(false);
        let started = Instant::now();
        adapter.get(&format!("{}/slow", server.base)).await.unwrap();
        assert!(started.elapsed() >= Duration::from_millis(950));
        // Retry-After beyond the cap fails at once rather than waiting.
        let started = Instant::now();
        let err = adapter
            .get(&format!("{}/busy", server.base))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("503"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(
            *server.paths.lock().unwrap(),
            vec!["/slow".to_owned(), "/slow".to_owned(), "/busy".to_owned()]
        );
    }
}
