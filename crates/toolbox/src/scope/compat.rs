//! The thin parser of octos's scope shape, for builds without the
//! `octos-engine` feature: the same JSON, field names, units, defaults and
//! validation as `octos_research::toolbox::Scope::from_grant`, and the same
//! domain and language rules, with only what `mod.research` needs. With the
//! feature it is compiled for tests only, to check it still agrees with
//! octos (`the_thin_parser_matches_octos`).

use super::{NarrowedSearch, SEARCH_CATEGORY};
use crate::host::url_host;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// octos's metasearch categories.
const CATEGORIES: &[&str] = &["general", "news", "science", "it", "social"];

/// An app's research/crawl grant, in octos's shape. Empty lists mean no
/// restriction.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    /// BCP-47 languages the app may search in.
    #[serde(default)]
    pub langs: Vec<String>,
    /// ISO 3166-1 alpha-2 regions.
    #[serde(default)]
    pub regions: Vec<String>,
    #[serde(default)]
    pub domains_allow: Vec<String>,
    #[serde(default)]
    pub domains_deny: Vec<String>,
    /// Oldest material the app may ask for, in days back from now.
    #[serde(default)]
    pub max_age_days: Option<u32>,
    /// Metasearch categories (`news`, `general`, `science`, `it`, `social`).
    #[serde(default)]
    pub categories: Vec<String>,
    /// Most results per `search` call.
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    /// `deep_crawl` limits (the `crawl` capability); 0 = crawling not
    /// granted. `mod.research` does not crawl.
    #[serde(default)]
    pub max_depth: u32,
    #[serde(default)]
    pub max_pages: u32,
}

fn default_max_results() -> usize {
    20
}

impl Scope {
    /// Parse and validate a grant.
    pub fn from_grant(grant: &Value) -> Result<Self, String> {
        let mut s: Scope =
            serde_json::from_value(grant.clone()).map_err(|e| format!("scope: {e}"))?;
        let mut langs = Vec::new();
        for l in &s.langs {
            langs.push(normalize(l).ok_or_else(|| format!("scope: bad language {l:?}"))?);
        }
        s.langs = langs;
        s.regions = s
            .regions
            .iter()
            .map(|r| r.trim().to_ascii_uppercase())
            .collect();
        if let Some(r) = s.regions.iter().find(|r| r.len() != 2) {
            return Err(format!("scope: bad region {r:?}"));
        }
        if let Some(c) = s
            .categories
            .iter()
            .find(|c| !CATEGORIES.contains(&c.as_str()))
        {
            return Err(format!("scope: unknown category {c:?}"));
        }
        if s.max_results == 0 {
            return Err("scope: max_results must be > 0".into());
        }
        Ok(s)
    }

    /// Whether `url` may be read under this scope's domain lists.
    pub fn check_domain(&self, url: &str) -> Result<(), String> {
        let why = match url_host(url.trim()) {
            None => Some("invalid_url"),
            Some(host) => {
                let host = host.strip_prefix("www.").unwrap_or(&host);
                if self.domains_deny.iter().any(|p| domain_matches(host, p)) {
                    Some("domain_deny")
                } else if !self.domains_allow.is_empty()
                    && !self.domains_allow.iter().any(|p| domain_matches(host, p))
                {
                    Some("domain_allow")
                } else {
                    None
                }
            }
        };
        match why {
            None => Ok(()),
            Some(why) => Err(format!(
                "{url} is outside this app's research grant ({why})"
            )),
        }
    }
}

/// octos's `urls::domain_matches`: the host is the pattern or a subdomain
/// of it, ignoring `www.`, `*.` and trailing dots.
fn domain_matches(host: &str, pattern: &str) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let p = pattern.trim().to_ascii_lowercase();
    let p = p
        .trim_start_matches("*.")
        .trim_start_matches('.')
        .trim_end_matches('.');
    let p = p.strip_prefix("www.").unwrap_or(p);
    if p.is_empty() {
        return false;
    }
    host == p || host.ends_with(&format!(".{p}"))
}

/// octos's `lang::normalize`: a BCP-47 tag in canonical case, or `None`.
fn normalize(tag: &str) -> Option<String> {
    let mut parts = tag
        .trim()
        .split(['-', '_'])
        .filter(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()));
    let primary = parts.next()?.to_ascii_lowercase();
    if !(2..=3).contains(&primary.len()) || !primary.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let mut out = primary;
    for sub in parts {
        out.push('-');
        match sub.len() {
            2 => out.push_str(&sub.to_ascii_uppercase()),
            4 => {
                let mut chars = sub.chars();
                if let Some(first) = chars.next() {
                    out.push(first.to_ascii_uppercase());
                    out.push_str(&chars.as_str().to_ascii_lowercase());
                }
            }
            _ => out.push_str(&sub.to_ascii_lowercase()),
        }
    }
    Some(out)
}

fn primary(tag: &str) -> String {
    tag.trim()
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// octos's `lang::matches_any`: primary subtags compared; an empty list
/// matches everything.
pub fn matches_any(lang: &str, wanted: &[String]) -> bool {
    wanted.is_empty() || {
        let p = primary(lang);
        wanted.iter().any(|w| primary(w) == p)
    }
}

/// `Scope::search_args` for one `mod.research` search: the same refusals,
/// clamps and notes, in the same order.
pub fn narrow_search(
    scope: &Scope,
    topic: &str,
    language: Option<&str>,
    region: Option<&str>,
    max_age_hours: Option<u32>,
    limit: u32,
) -> Result<NarrowedSearch, String> {
    if topic.trim().is_empty() {
        return Err("query is required".into());
    }
    let mut notes = Vec::new();
    if let Some(l) = language.map(str::trim).filter(|l| !l.is_empty()) {
        let tag = normalize(l).ok_or_else(|| format!("invalid language tag {l:?}"))?;
        if !matches_any(&tag, &scope.langs) {
            return Err(format!(
                "language {tag} is not in this app's research grant"
            ));
        }
    }
    if let Some(r) = region.map(str::trim).filter(|r| !r.is_empty()) {
        let r = r.to_ascii_uppercase();
        if !scope.regions.is_empty() && !scope.regions.contains(&r) {
            return Err(format!("region {r} is not in this app's research grant"));
        }
    }
    let mut max_age_hours = max_age_hours;
    if let Some(days) = scope.max_age_days {
        let floor = days.saturating_mul(24);
        if max_age_hours.is_none_or(|h| h > floor) {
            if max_age_hours.is_some() {
                notes.push(format!("since clamped to this app's {days}-day limit"));
            }
            max_age_hours = Some(floor);
        }
    }
    if !scope.categories.is_empty() && !scope.categories.iter().any(|c| c == SEARCH_CATEGORY) {
        return Err(format!(
            "category {SEARCH_CATEGORY} is not in this app's research grant"
        ));
    }
    let asked = limit.max(1) as usize;
    let count = asked.min(scope.max_results);
    if count < asked {
        notes.push(format!("count clamped to this app's limit of {count}"));
    }
    Ok(NarrowedSearch {
        max_age_hours,
        limit: count as u32,
        notes,
    })
}

/// `Scope::crawl_args`: a `deep_crawl` call narrowed to the crawl limits.
pub fn crawl_args(scope: &Scope, args: &Value) -> Result<Value, String> {
    if scope.max_pages == 0 || scope.max_depth == 0 {
        return Err("crawling is not in this app's grant".into());
    }
    let url = args
        .get("url")
        .and_then(Value::as_str)
        .ok_or("url is required")?;
    scope.check_domain(url)?;
    let depth = args
        .get("max_depth")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .clamp(1, scope.max_depth as u64);
    let pages = args
        .get("max_pages")
        .and_then(Value::as_u64)
        .unwrap_or(10)
        .clamp(1, scope.max_pages as u64);
    let mut out = json!({"url": url, "max_depth": depth, "max_pages": pages});
    if let Some(p) = args.get("path_prefix").and_then(Value::as_str) {
        out["path_prefix"] = json!(p);
    }
    Ok(out)
}
