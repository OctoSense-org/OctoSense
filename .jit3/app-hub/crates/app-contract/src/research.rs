//! The scope of the `research` and `crawl` capabilities.
//!
//! An app that asks for `research` (search through the system toolbox) or
//! `crawl` (follow links through a site) declares what it may reach in the
//! manifest's top-level `research` object. That object's schema is exactly
//! octos's `octos_research::toolbox::Scope` (octos `crates/octos-research/
//! src/toolbox.rs`), which is the single source of truth for an app's
//! research permission: the same field names, units, defaults and rules, so
//! the scope App Hub checks and pins is the grant the host hands octos
//! unchanged. OctoSense's toolbox (`crates/toolbox/src/scope.rs`) parses the
//! same JSON.
//!
//! This crate links no octos code (see `Cargo.toml`), so the struct and
//! [`ResearchScope::validated`] mirror octos's `Scope` and
//! `Scope::from_grant` field for field and rule for rule. Change them only
//! together with octos.
//!
//! - `langs`: BCP-47 languages the app may search in.
//! - `regions`: ISO 3166-1 alpha-2 regions.
//! - `domains_allow`, `domains_deny`: domain patterns (`example.com` also
//!   covers its subdomains).
//! - `max_age_days`: the oldest material, in days back from now.
//! - `categories`: metasearch categories, of [`RESEARCH_CATEGORIES`].
//! - `max_results`: most results per search (default 20, never 0).
//! - `max_depth`, `max_pages`: the `crawl` limits; 0 means crawling is not
//!   granted.
//!
//! Empty lists mean no restriction. Unknown fields are refused.
//!
//! The scope is part of the manifest, so it is part of the contract; how a
//! store puts it into words is App Hub's own (`octosense-app-policy`).
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// octos's metasearch categories (`metasearch::manifest::CATEGORIES`).
pub const RESEARCH_CATEGORIES: &[&str] = &["general", "news", "science", "it", "social"];

/// The field names, in octos's order.
pub const SCOPE_FIELDS: &[&str] = &[
    "langs",
    "regions",
    "domains_allow",
    "domains_deny",
    "max_age_days",
    "categories",
    "max_results",
    "max_depth",
    "max_pages",
];

/// Fields of the toolbox's scope before it became octos's, with what
/// replaced each. A manifest that uses them is refused with this advice; it
/// is not converted, because hours become days only by rounding, which would
/// widen the grant, and the old `max_pages` meant articles per run.
const OLD_FIELDS: &[(&str, &str)] = &[
    ("languages", "`langs`"),
    ("allowed_domains", "`domains_allow`"),
    ("denied_domains", "`domains_deny`"),
    ("recency_hours", "`max_age_days` (whole days; choose them, since rounding hours up would widen the grant)"),
];

/// An app's research and crawl scope: octos's `Scope`, field for field.
///
/// Serialising skips fields that hold their default (except `max_results`),
/// so a pinned manifest stays close to what the publisher wrote; octos reads
/// the result back as the same scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ResearchScope {
    /// BCP-47 languages the app may search in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub langs: Vec<String>,
    /// ISO 3166-1 alpha-2 regions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domains_allow: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domains_deny: Vec<String>,
    /// Oldest material the app may ask for, in days back from now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_days: Option<u32>,
    /// Metasearch categories (`news`, `general`, `science`, `it`, `social`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    /// Most results per search call.
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    /// Crawl limits (the `crawl` capability); 0 = crawling not granted.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub max_depth: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub max_pages: u32,
}

fn default_max_results() -> usize {
    20
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl Default for ResearchScope {
    /// The empty grant `{}`: no restriction, 20 results a search, no crawl.
    fn default() -> Self {
        ResearchScope {
            langs: Vec::new(),
            regions: Vec::new(),
            domains_allow: Vec::new(),
            domains_deny: Vec::new(),
            max_age_days: None,
            categories: Vec::new(),
            max_results: default_max_results(),
            max_depth: 0,
            max_pages: 0,
        }
    }
}

impl ResearchScope {
    /// Validates and normalises the scope with octos's `Scope::from_grant`
    /// rules: every language a BCP-47 tag (normalised, `zh_cn` → `zh-CN`),
    /// every region two letters after trimming and upper-casing, every
    /// category one of [`RESEARCH_CATEGORIES`], and `max_results` above 0.
    ///
    /// One rule is App Hub's own and stricter: a domain pattern must be a bare
    /// domain (`example.com`, `.example.com` or `*.example.com`). octos
    /// compares patterns as strings, so `https://example.com/` would match no
    /// site: in `domains_deny` it would deny nothing the person was told it
    /// denies.
    pub fn validated(&self) -> Result<ResearchScope, String> {
        let mut s = self.clone();
        let mut langs = Vec::new();
        for l in &s.langs {
            langs.push(normalize_lang(l).ok_or_else(|| format!("scope: bad language {l:?}"))?);
        }
        s.langs = langs;
        s.regions = s.regions.iter().map(|r| r.trim().to_ascii_uppercase()).collect();
        if let Some(r) = s.regions.iter().find(|r| r.len() != 2) {
            return Err(format!("scope: bad region {r:?}"));
        }
        if let Some(c) = s.categories.iter().find(|c| !RESEARCH_CATEGORIES.contains(&c.as_str())) {
            return Err(format!("scope: unknown category {c:?}"));
        }
        if s.max_results == 0 {
            return Err("scope: max_results must be > 0".into());
        }
        for d in s.domains_allow.iter().chain(&s.domains_deny) {
            check_domain_pattern(d)?;
        }
        Ok(s)
    }

    /// Whether the scope grants crawling at all.
    pub fn crawls(&self) -> bool {
        self.max_depth > 0 && self.max_pages > 0
    }
}

/// octos's `lang::normalize`: a primary subtag of two or three letters,
/// lower-cased; a two-letter subtag upper-cased (a region), a four-letter one
/// title-cased (a script), anything else lower-cased. `None` when it is not a
/// language tag.
pub fn normalize_lang(tag: &str) -> Option<String> {
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

/// A bare domain, optionally with a leading `.` or `*.`: no scheme, path,
/// port, credentials or inner wildcard.
fn check_domain_pattern(pattern: &str) -> Result<(), String> {
    let bare = pattern.trim().trim_start_matches("*.").trim_start_matches('.');
    let well_formed = !bare.is_empty()
        && bare.len() <= 253
        && bare.contains('.')
        && !bare.starts_with('.')
        && !bare.ends_with('.')
        && !bare.contains("..")
        && bare.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    if !well_formed {
        return Err(format!(
            "scope: domain {pattern:?} must be a bare domain such as example.com (no scheme, path, port or inner wildcard)"
        ));
    }
    Ok(())
}

/// The advice for a `research` object in the toolbox's old shape, if it is
/// one: which fields to rename. `None` for anything else.
pub(crate) fn old_shape_advice(research: &Value) -> Option<String> {
    let object = research.as_object()?;
    let old: Vec<String> = OLD_FIELDS
        .iter()
        .filter(|(field, _)| object.contains_key(*field))
        .map(|(field, new)| format!("`{field}` is now {new}"))
        .collect();
    if old.is_empty() {
        return None;
    }
    let pages = if object.contains_key("max_pages") {
        "; `max_pages` is now the crawl limit (pages of one crawl), not articles per run"
    } else {
        ""
    };
    Some(format!(
        "manifest research scope is in the old toolbox shape, which is no longer accepted; write it in octos's scope shape: {}{pages}",
        old.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(json: &str) -> Result<ResearchScope, String> {
        let s: ResearchScope = serde_json::from_str(json).map_err(|e| format!("scope: {e}"))?;
        s.validated()
    }

    #[test]
    fn the_empty_grant_is_octos_default() {
        let s = scope("{}").unwrap();
        assert_eq!(s, ResearchScope::default());
        assert_eq!(s.max_results, 20);
        assert!(!s.crawls());
        assert_eq!(serde_json::to_string(&s).unwrap(), r#"{"max_results":20}"#);
    }

    #[test]
    fn the_rules_are_octos_from_grant() {
        let s = scope(r#"{"langs":["EN","zh_cn","zh-hant"],"regions":[" us ","cn"]}"#).unwrap();
        assert_eq!(s.langs, ["en", "zh-CN", "zh-Hant"]);
        assert_eq!(s.regions, ["US", "CN"]);
        assert!(scope(r#"{"langs":["english"]}"#).unwrap_err().contains("bad language"));
        assert!(scope(r#"{"langs":["1"]}"#).unwrap_err().contains("bad language"));
        assert!(scope(r#"{"regions":["USA"]}"#).unwrap_err().contains("bad region"));
        assert!(scope(r#"{"categories":["video"]}"#).unwrap_err().contains("unknown category"));
        assert!(scope(r#"{"categories":["News"]}"#).unwrap_err().contains("unknown category"));
        assert!(scope(r#"{"max_results":0}"#).unwrap_err().contains("max_results must be > 0"));
        assert!(scope(r#"{"recency":"7d"}"#).unwrap_err().contains("unknown field"));
        for c in RESEARCH_CATEGORIES {
            scope(&format!(r#"{{"categories":["{c}"]}}"#)).unwrap();
        }
    }

    #[test]
    fn a_domain_pattern_is_a_bare_domain() {
        scope(r#"{"domains_allow":["bbc.co.uk",".reuters.com","*.example.org"],"domains_deny":["www.x.com"]}"#).unwrap();
        for bad in ["https://x.com", "x.com/news", "x.com:443", "a.*.com", "localhost", "", "x..com"] {
            let err = scope(&format!(r#"{{"domains_deny":["{bad}"]}}"#)).unwrap_err();
            assert!(err.contains("bare domain"), "{bad}: {err}");
        }
    }

    #[test]
    fn the_old_toolbox_shape_gets_advice() {
        let advice = old_shape_advice(&serde_json::json!({"languages":["en"],"recency_hours":24,"max_pages":5})).unwrap();
        assert!(advice.contains("`languages` is now `langs`"), "{advice}");
        assert!(advice.contains("`recency_hours` is now `max_age_days`"), "{advice}");
        assert!(advice.contains("`max_pages` is now the crawl limit"), "{advice}");
        assert!(old_shape_advice(&serde_json::json!({"langs":["en"]})).is_none());
    }
}
