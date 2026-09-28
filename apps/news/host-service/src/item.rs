//! A story as the service keeps it, and what makes two stories the same one.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// One story, whatever feed it came from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Item {
    /// A stable hash of the canonical URL ([`item_id`]).
    pub id: String,
    pub title: String,
    pub url: String,
    /// The outlet: "Reuters", "Hacker News", "bbc.co.uk".
    pub source: String,
    /// The source (feed) id that first brought it: `hn`, `google`, `topic-…`.
    pub feed: String,
    /// ISO 639-1, as the feed or topic says; "" when unknown.
    pub lang: String,
    /// Unix seconds; 0 when the feed gives no date.
    pub published: i64,
    /// Unix seconds, when the service first stored it.
    pub fetched: i64,
    /// Plain text, at most a few hundred characters; may be empty.
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Topics it matched (a followed topic's query, or the feed's category).
    #[serde(default)]
    pub topics: Vec<String>,
    /// Where people discuss it (Hacker News's thread).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discussion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comments: Option<u64>,
    /// Other feeds that carried the same story (by URL or a near-identical
    /// title), after the first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<String>,
}

impl Item {
    /// When it happened, as far as the service knows.
    pub fn when(&self) -> i64 {
        if self.published > 0 {
            self.published
        } else {
            self.fetched
        }
    }
}

/// A story as a parser reads it, before the service names and files it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draft {
    pub title: String,
    pub url: String,
    pub source: String,
    pub published: Option<i64>,
    pub summary: String,
    pub image: Option<String>,
    pub discussion: Option<String>,
    pub points: Option<u64>,
    pub comments: Option<u64>,
    /// The article's language where the source says (GDELT does).
    pub lang: Option<String>,
}

/// Query parameters that only track the reader.
const TRACKING: &[&str] =
    &["fbclid", "gclid", "dclid", "msclkid", "mc_cid", "mc_eid", "ocid", "cmpid", "smid", "ref", "ref_src", "igshid", "guccounter", "_ga", "cid", "sr_share", "taid"];

/// The URL two feeds would agree on for one article: https, no `www.`, no
/// tracking parameters, the rest sorted, no trailing slash, no tracking
/// fragment. Unparseable input comes back trimmed.
pub fn canonical_url(raw: &str) -> String {
    let raw = raw.trim();
    let Ok(mut url) = url::Url::parse(raw) else { return raw.to_string() };
    if url.scheme() == "http" {
        let _ = url.set_scheme("https");
    }
    if let Some(host) = url.host_str().map(|h| h.to_ascii_lowercase()) {
        if let Some(bare) = host.strip_prefix("www.") {
            let _ = url.set_host(Some(bare));
        }
    }
    let _ = url.set_port(None);
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| {
            let k = k.to_ascii_lowercase();
            !k.starts_with("utm_") && !k.starts_with("at_") && !TRACKING.contains(&k.as_str())
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    pairs.sort();
    if pairs.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    // A fragment is kept when it names the story (TechMeme's `#a260916p1`),
    // dropped when it tracks (`#xtor=RSS-1`, text fragments).
    if let Some(fragment) = url.fragment() {
        if fragment.is_empty() || fragment.contains('=') || fragment.starts_with(":~:") || fragment.starts_with("xtor") {
            url.set_fragment(None);
        }
    }
    let path = url.path().to_string();
    if path.len() > 1 && path.ends_with('/') {
        url.set_path(path.trim_end_matches('/'));
    }
    url.to_string()
}

/// The stable id of the story at `url`: 16 hex digits of the SHA-256 of its
/// canonical URL.
pub fn item_id(url: &str) -> String {
    let digest = Sha256::digest(canonical_url(url).as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// A title reduced for comparison: lower case, letters and digits only, and
/// without a trailing ` - Outlet` / ` | Outlet`.
pub fn title_key(title: &str) -> String {
    let title = title.rsplit_once(" | ").map_or(title, |(head, _)| head);
    let lower = title.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    for c in lower.chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
    out.trim().to_string()
}

/// The words of a title key.
pub fn title_words(key: &str) -> HashSet<&str> {
    key.split(' ').filter(|w| !w.is_empty()).collect()
}

/// Two titles for the same story: the same key, or (for titles of five words
/// or more) at least 80% of their words shared.
pub fn near_duplicate(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (wa, wb) = (title_words(a), title_words(b));
    if wa.len() < 5 || wb.len() < 5 {
        return false;
    }
    let shared = wa.intersection(&wb).count() as f64;
    let union = wa.union(&wb).count() as f64;
    shared / union >= 0.8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_urls_agree() {
        let a = canonical_url("http://www.Example.com/story/?utm_source=rss&b=2&a=1#xtor=RSS-3");
        let b = canonical_url("https://example.com/story?a=1&b=2&fbclid=zz");
        assert_eq!(a, b);
        assert_eq!(a, "https://example.com/story?a=1&b=2");
        assert_eq!(item_id("https://example.com/story?a=1&b=2&utm_medium=x"), item_id(&a));
        assert_ne!(canonical_url("https://www.techmeme.com/260916/p1#a260916p1"), canonical_url("https://www.techmeme.com/260916/p2#a260916p2"));
        assert_eq!(item_id("x").len(), 16);
    }

    #[test]
    fn near_duplicate_titles() {
        let a = title_key("Apple unveils the new iPhone 18 at its September event");
        let b = title_key("Apple unveils new iPhone 18 at its September event | The Verge");
        assert!(near_duplicate(&a, &b));
        assert!(!near_duplicate(&title_key("Rates hold"), &title_key("Rates rise")));
        assert!(!near_duplicate(&title_key("Apple unveils the new iPhone 18"), &title_key("Google unveils the new Pixel 12")));
    }
}
