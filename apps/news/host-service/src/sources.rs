//! Where stories come from: the News app's own three feeds, a short default
//! list of reputable RSS/Atom feeds, the person's topics (Google News RSS and
//! GDELT per language and region) and feeds imported from OPML. Every source
//! is fetched only from a host the News bundle's manifest declares.
use crate::parse::Style;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The News bundle's manifest: its `network.hosts` is the service's
/// allow-list too, so the app and its service reach the same hosts.
pub const MANIFEST: &str = include_str!("../../bundle/manifest.json");

/// The hosts the News bundle declares.
pub fn manifest_hosts() -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(MANIFEST)
        .ok()
        .and_then(|m| m["network"]["hosts"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|h| h.as_str().map(|h| h.to_ascii_lowercase()))
        .collect()
}

/// A URL's host, lower case.
pub fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url).ok().filter(|u| u.scheme() == "https" || u.scheme() == "http").and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
}

/// One thing the service fetches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// Stable id: `hn`, `techmeme`, `google`, `bbc-world`, `topic-…-google`.
    pub id: String,
    pub label: String,
    /// `hn`, `rss`, `google`, `digest` (RSS with the outlet in the title) or
    /// `gdelt`.
    pub kind: String,
    pub url: String,
    /// ISO 639-1 of what it carries ("" when mixed or unknown).
    #[serde(default)]
    pub lang: String,
    /// Tags every item from it gets: the feed's category or a topic's query.
    #[serde(default)]
    pub topics: Vec<String>,
}

impl Source {
    fn new(id: &str, label: &str, kind: &str, url: &str, lang: &str, topics: &[&str]) -> Source {
        Source {
            id: id.into(),
            label: label.into(),
            kind: kind.into(),
            url: url.into(),
            lang: lang.into(),
            topics: topics.iter().map(|t| t.to_string()).collect(),
        }
    }

    pub fn style(&self) -> Style {
        match self.kind.as_str() {
            "google" => Style::Google,
            "digest" => Style::Digest,
            _ => Style::Plain,
        }
    }

    pub fn host(&self) -> String {
        host_of(&self.url).unwrap_or_default()
    }
}

/// The News app's own tabs, as its script fetched them.
pub fn app_feeds() -> Vec<Source> {
    vec![
        Source::new("hn", "Hacker News", "hn", "https://hn.algolia.com/api/v1/search?tags=front_page&hitsPerPage=30", "en", &["tech"]),
        Source::new("techmeme", "TechMeme", "digest", "https://www.techmeme.com/feed.xml", "en", &["tech"]),
        Source::new("google", "Google News", "google", &google_news_url(None, "en", "US"), "en", &[]),
    ]
}

/// A short list of reputable general and technology feeds, beyond the tabs.
pub fn default_feeds() -> Vec<Source> {
    vec![
        Source::new("bbc-world", "BBC News", "rss", "https://feeds.bbci.co.uk/news/world/rss.xml", "en", &["world"]),
        Source::new("npr", "NPR", "rss", "https://feeds.npr.org/1001/rss.xml", "en", &["world"]),
        Source::new("guardian-world", "The Guardian", "rss", "https://www.theguardian.com/world/rss", "en", &["world"]),
        Source::new("ars", "Ars Technica", "rss", "https://feeds.arstechnica.com/arstechnica/index", "en", &["tech"]),
    ]
}

/// A followed topic: a search (or a Google News section, `topic:BUSINESS`)
/// in one language and region.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Topic {
    pub query: String,
    /// ISO 639-1, lower case: `en`, `de`, `zh`.
    pub lang: String,
    /// ISO 3166-1 alpha-2, upper case: `US`, `DE`, `CN`.
    pub region: String,
}

pub const MAX_TOPICS: usize = 20;

/// Google News's sections, for `topic:<SECTION>`.
const SECTIONS: &[&str] = &["WORLD", "NATION", "BUSINESS", "TECHNOLOGY", "ENTERTAINMENT", "SPORTS", "SCIENCE", "HEALTH"];

impl Topic {
    /// The topic as the person wrote it, checked; or why it cannot be one.
    pub fn validated(query: &str, lang: &str, region: &str) -> Result<Topic, String> {
        let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
        if query.is_empty() || query.chars().count() > 200 {
            return Err("A topic needs a query of 1 to 200 characters.".into());
        }
        if let Some(section) = query.strip_prefix("topic:") {
            if !SECTIONS.contains(&section) {
                return Err(format!("Unknown Google News section {section:?}; one of {}.", SECTIONS.join(", ")));
            }
        }
        let lang = lang.trim().to_ascii_lowercase();
        if !(2..=3).contains(&lang.len()) || !lang.chars().all(|c| c.is_ascii_lowercase()) {
            return Err(format!("{lang:?} is not a language code (en, de, zh…)."));
        }
        let region = region.trim().to_ascii_uppercase();
        if region.len() != 2 || !region.chars().all(|c| c.is_ascii_uppercase()) {
            return Err(format!("{region:?} is not a region code (US, DE, CN…)."));
        }
        Ok(Topic { query, lang, region })
    }

    /// The label items from it carry in `topics`.
    pub fn tag(&self) -> String {
        match self.query.strip_prefix("topic:") {
            Some(section) => section.to_lowercase(),
            None => self.query.clone(),
        }
    }

    fn key(&self) -> String {
        let digest = Sha256::digest(format!("{}\n{}\n{}", self.query.to_lowercase(), self.lang, self.region).as_bytes());
        digest.iter().take(4).map(|b| format!("{b:02x}")).collect()
    }

    /// Its sources: Google News always, GDELT for searches when enabled.
    pub fn sources(&self, gdelt: bool) -> Vec<Source> {
        let key = self.key();
        let tag = self.tag();
        let label = format!("{} ({}-{})", tag, self.lang, self.region);
        let section = self.query.strip_prefix("topic:");
        let mut out = vec![Source {
            id: format!("topic-{key}-google"),
            label: format!("Google News: {label}"),
            kind: "google".into(),
            url: match section {
                Some(section) => google_news_section_url(section, &self.lang, &self.region),
                None => google_news_url(Some(&self.query), &self.lang, &self.region),
            },
            lang: self.lang.clone(),
            topics: vec![tag.clone()],
        }];
        if gdelt && section.is_none() {
            out.push(Source {
                id: format!("topic-{key}-gdelt"),
                label: format!("GDELT: {label}"),
                kind: "gdelt".into(),
                url: gdelt_url(&self.query, &self.lang, "1d"),
                lang: self.lang.clone(),
                topics: vec![tag],
            });
        }
        out
    }
}

fn google_params(lang: &str, region: &str) -> String {
    format!("hl={lang}-{region}&gl={region}&ceid={region}:{lang}")
}

/// Google News RSS: top stories, or a search.
pub fn google_news_url(query: Option<&str>, lang: &str, region: &str) -> String {
    match query {
        None => format!("https://news.google.com/rss?{}", google_params(lang, region)),
        Some(q) => {
            let q: String = url::form_urlencoded::byte_serialize(q.as_bytes()).collect();
            format!("https://news.google.com/rss/search?q={q}&{}", google_params(lang, region))
        }
    }
}

pub fn google_news_section_url(section: &str, lang: &str, region: &str) -> String {
    format!("https://news.google.com/rss/headlines/section/topic/{section}?{}", google_params(lang, region))
}

/// GDELT DOC 2.0's article list for a query in one source language, newest
/// first, over `timespan` (`1d`, `12h`, …).
pub fn gdelt_url(query: &str, lang: &str, timespan: &str) -> String {
    let mut q = query.to_string();
    if let Some(name) = gdelt_language(lang) {
        q.push_str(&format!(" sourcelang:{name}"));
    }
    let q: String = url::form_urlencoded::byte_serialize(q.as_bytes()).collect();
    format!("https://api.gdeltproject.org/api/v2/doc/doc?query={q}&mode=ArtList&format=json&maxrecords=75&sort=DateDesc&timespan={timespan}")
}

/// ISO 639-1 and GDELT's language names.
const LANGUAGES: &[(&str, &str)] = &[
    ("en", "english"),
    ("de", "german"),
    ("fr", "french"),
    ("es", "spanish"),
    ("it", "italian"),
    ("pt", "portuguese"),
    ("nl", "dutch"),
    ("ru", "russian"),
    ("uk", "ukrainian"),
    ("pl", "polish"),
    ("tr", "turkish"),
    ("ar", "arabic"),
    ("he", "hebrew"),
    ("fa", "persian"),
    ("hi", "hindi"),
    ("zh", "chinese"),
    ("ja", "japanese"),
    ("ko", "korean"),
    ("id", "indonesian"),
    ("vi", "vietnamese"),
    ("th", "thai"),
    ("sv", "swedish"),
];

fn gdelt_language(iso: &str) -> Option<&'static str> {
    LANGUAGES.iter().find(|(code, _)| *code == iso).map(|(_, name)| *name)
}

/// GDELT names an article's language in English ("Chinese"); the ISO code.
pub fn iso_from_gdelt_language(name: &str) -> Option<&'static str> {
    let name = name.trim().to_ascii_lowercase();
    LANGUAGES.iter().find(|(_, n)| *n == name).map(|(code, _)| *code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_source_is_on_a_declared_host() {
        let hosts = manifest_hosts();
        let topic = Topic::validated("climate", "de", "de").unwrap();
        for s in app_feeds().into_iter().chain(default_feeds()).chain(topic.sources(true)) {
            assert!(hosts.contains(&s.host()), "{} ({}) is not in the manifest's network.hosts", s.id, s.host());
        }
    }

    #[test]
    fn topic_urls() {
        let t = Topic::validated("  electric   cars ", "DE", "de").unwrap();
        assert_eq!(t, Topic { query: "electric cars".into(), lang: "de".into(), region: "DE".into() });
        let s = t.sources(true);
        assert_eq!(s[0].url, "https://news.google.com/rss/search?q=electric+cars&hl=de-DE&gl=DE&ceid=DE:de");
        assert!(s[1].url.starts_with("https://api.gdeltproject.org/api/v2/doc/doc?query=electric+cars+sourcelang%3Agerman&mode=ArtList&format=json"));
        assert!(s[1].url.ends_with("&timespan=1d"));
        let section = Topic::validated("topic:BUSINESS", "en", "GB").unwrap();
        assert_eq!(section.sources(true).len(), 1, "a section has no GDELT query");
        assert!(Topic::validated("", "en", "US").is_err());
        assert!(Topic::validated("x", "english", "US").is_err());
        assert!(Topic::validated("topic:NOPE", "en", "US").is_err());
    }
}
