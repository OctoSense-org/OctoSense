//! An app's research grant: the scope of its `research` and `crawl`
//! capabilities (ADR 0002 section 6, "The scope is octos's `Scope`").
//!
//! The schema is exactly `octos_research::toolbox::Scope` (octos#2585):
//! `langs`, `regions`, `domains_allow`, `domains_deny`, `max_age_days`,
//! `categories`, `max_results`, and the crawl limits `max_depth` and
//! `max_pages`; unknown fields are refused and empty lists mean no limit.
//!
//! - With the `octos-engine` feature, [`Scope`] **is** octos's type: a grant
//!   is parsed by `Scope::from_grant` and every search is narrowed by
//!   `Scope::search_args` and every read checked by `Scope::check_domain`.
//! - Without it (the fixture and interim live backends, and builds that do
//!   not link octos), [`Scope`] is a thin parser of the same JSON with the
//!   same rules ([`compat`]). A test built with the feature checks that both
//!   accept, refuse and narrow the same grants alike, so the two cannot
//!   drift unnoticed.
//!
//! The scope says what an app may reach, not how much one run may do: the
//! run's budget (`max_reads`, calls, model calls, time) is the template's,
//! narrowed by the app's own budget ([`crate::manifest::Budget`]). The
//! scope's `max_pages` counts pages of one crawl, never articles read.
//!
//! **Grants in the old toolbox shape** (`languages`, `allowed_domains`,
//! `denied_domains`, `recency_hours`, and `max_pages` as articles per run)
//! are refused by [`parse`] with an error that names the new fields. They
//! are not converted: hours become days only by rounding, which would widen
//! what the person granted, and the old `max_pages` means something else
//! now. App Hub pins and the person grants the scope again in the new shape.

#[cfg(any(test, not(feature = "octos-engine")))]
pub mod compat;

#[cfg(not(feature = "octos-engine"))]
pub use compat::Scope;
#[cfg(feature = "octos-engine")]
pub use octos_research::toolbox::Scope;

use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};

/// The metasearch category `mod.research` searches: its backends search
/// news (Google News, GDELT, publisher feeds). A grant whose `categories`
/// leave out `news` refuses every search.
pub const SEARCH_CATEGORY: &str = "news";

/// Fields of the toolbox's scope before it became octos's, with what
/// replaced each.
const OLD_FIELDS: &[(&str, &str)] = &[
    ("languages", "`langs`"),
    ("allowed_domains", "`domains_allow`"),
    ("denied_domains", "`domains_deny`"),
    (
        "recency_hours",
        "`max_age_days` (whole days; rounding hours up would widen the grant, so choose the days)",
    ),
];

/// Parses and validates a grant: octos's `Scope::from_grant` (or the thin
/// parser of the same shape). A grant in the old toolbox shape is refused
/// with the fields to rename.
pub fn parse(grant: &Value) -> Result<Scope, String> {
    if let Some(object) = grant.as_object() {
        let old: Vec<String> = OLD_FIELDS
            .iter()
            .filter(|(field, _)| object.contains_key(*field))
            .map(|(field, new)| format!("`{field}` is now {new}"))
            .collect();
        if !old.is_empty() {
            let pages = if object.contains_key("max_pages") {
                "; `max_pages` is now the crawl limit (pages of one crawl), and articles per run are the app's budget `max_reads`"
            } else {
                ""
            };
            return Err(format!(
                "scope: this grant is in the old toolbox shape, which is no longer accepted; grant it again in octos's scope shape: {}{pages}",
                old.join(", ")
            ));
        }
    }
    Scope::from_grant(grant)
}

/// The scope of an app granted no limits: `{}` parsed, so `max_results` has
/// octos's default (20). Not `Scope::default()`, whose `max_results` is 0.
pub fn unrestricted() -> Scope {
    Scope::from_grant(&json!({})).expect("the empty grant is valid")
}

/// Deserializes a grant through [`parse`], for [`crate::AppContext`].
pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Scope, D::Error> {
    let grant = Value::deserialize(deserializer)?;
    parse(&grant).map_err(serde::de::Error::custom)
}

/// Whether a language (a search's, a translation's, a digest's or an
/// item's) is within the grant's `langs`, comparing primary subtags.
pub fn allows_language(scope: &Scope, tag: &str) -> bool {
    #[cfg(feature = "octos-engine")]
    return octos_research::lang::matches_any(tag, &scope.langs);
    #[cfg(not(feature = "octos-engine"))]
    return compat::matches_any(tag, &scope.langs);
}

/// A `research.search` narrowed to the grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NarrowedSearch {
    /// The recency the search may ask for: the requested one, capped by the
    /// grant's `max_age_days`; the grant's when none was requested.
    pub max_age_hours: Option<u32>,
    /// Results the app may get, capped by the grant's `max_results`.
    pub limit: u32,
    /// What was narrowed, for the run's diagnostics.
    pub notes: Vec<String>,
}

/// Narrows one search to the grant, or refuses it with the reason: a
/// language, region or category outside the grant is refused; recency and
/// the result count are clamped. With the feature this is
/// `Scope::search_args`.
pub fn narrow_search(
    scope: &Scope,
    topic: &str,
    language: Option<&str>,
    region: Option<&str>,
    max_age_hours: Option<u32>,
    limit: u32,
) -> Result<NarrowedSearch, String> {
    #[cfg(feature = "octos-engine")]
    {
        let now = chrono::Utc::now();
        let mut args = json!({"query": topic, "category": SEARCH_CATEGORY, "count": limit});
        if let Some(language) = language.filter(|l| !l.is_empty()) {
            args["lang"] = json!([language]);
        }
        if let Some(region) = region.filter(|r| !r.is_empty()) {
            args["region"] = json!(region);
        }
        if let Some(hours) = max_age_hours {
            args["since"] = json!(format!("{hours}h"));
        }
        let scoped = scope.search_args(&args, now)?;
        Ok(NarrowedSearch {
            max_age_hours: scoped.since.map(|since| {
                let seconds = (now - since.cutoff).num_seconds().max(0) as u64;
                seconds.div_ceil(3600) as u32
            }),
            limit: scoped.count as u32,
            notes: scoped.notes,
        })
    }
    #[cfg(not(feature = "octos-engine"))]
    compat::narrow_search(scope, topic, language, region, max_age_hours, limit)
}

/// A `deep_crawl` narrowed to the grant (the `crawl` capability's limits).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NarrowedCrawl {
    pub url: String,
    /// Link hops from `url`, at least 1, at most the grant's `max_depth`.
    pub max_depth: u32,
    /// Pages read, at least 1, at most the grant's `max_pages`.
    pub max_pages: u32,
    /// Only URLs whose path starts with this, if set.
    pub path_prefix: Option<String>,
}

/// Narrows one `deep_crawl` call (`{url, max_depth?, max_pages?,
/// path_prefix?}`) to the grant, or refuses it: crawling not granted
/// (`max_depth` or `max_pages` 0), no `url`, or a site outside the domain
/// lists. Depth defaults to 1 and pages to 10, both clamped to the grant.
/// With the feature this is `Scope::crawl_args`.
pub fn narrow_crawl(scope: &Scope, args: &Value) -> Result<NarrowedCrawl, String> {
    #[cfg(feature = "octos-engine")]
    let narrowed = scope.crawl_args(args)?;
    #[cfg(not(feature = "octos-engine"))]
    let narrowed = compat::crawl_args(scope, args)?;
    let number = |key: &str| narrowed.get(key).and_then(Value::as_u64).unwrap_or(1) as u32;
    Ok(NarrowedCrawl {
        url: narrowed["url"].as_str().unwrap_or_default().to_owned(),
        max_depth: number("max_depth"),
        max_pages: number("max_pages"),
        path_prefix: narrowed
            .get("path_prefix")
            .and_then(Value::as_str)
            .filter(|p| !p.is_empty())
            .map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grant_in_the_old_shape_is_refused_with_the_new_fields() {
        let error = parse(&json!({
            "languages": ["en"], "recency_hours": 48, "max_pages": 3
        }))
        .unwrap_err();
        assert!(error.contains("old toolbox shape"), "{error}");
        assert!(error.contains("`languages` is now `langs`"), "{error}");
        assert!(
            error.contains("`recency_hours` is now `max_age_days`"),
            "{error}"
        );
        assert!(error.contains("`max_reads`"), "{error}");
        // Any other unknown field is refused by octos's parser itself.
        assert!(parse(&json!({"depth": 2})).is_err());
    }

    #[test]
    fn the_new_shape_parses_and_normalizes() {
        let scope = parse(&json!({
            "langs": ["ZH-tw", "en"], "regions": ["us"],
            "domains_deny": ["example.com"], "max_age_days": 2
        }))
        .unwrap();
        assert_eq!(scope.langs, ["zh-TW", "en"]);
        assert_eq!(scope.regions, ["US"]);
        assert_eq!(scope.max_results, 20);
        assert_eq!((scope.max_depth, scope.max_pages), (0, 0));
        assert!(parse(&json!({"regions": ["USA"]})).is_err());
        assert!(parse(&json!({"categories": ["videos"]})).is_err());
        assert!(parse(&json!({"max_results": 0})).is_err());
        assert_eq!(unrestricted().max_results, 20);
    }

    #[test]
    fn searches_are_narrowed_to_the_grant() {
        let scope = parse(&json!({
            "langs": ["zh"], "regions": ["TW"], "max_age_days": 2, "max_results": 3
        }))
        .unwrap();
        let narrowed =
            narrow_search(&scope, "台风", Some("zh-TW"), Some("tw"), Some(72), 5).unwrap();
        assert_eq!(narrowed.max_age_hours, Some(48));
        assert_eq!(narrowed.limit, 3);
        assert_eq!(narrowed.notes.len(), 2, "{:?}", narrowed.notes);
        // No recency asked: the grant's.
        let narrowed = narrow_search(&scope, "台风", None, None, None, 2).unwrap();
        assert_eq!((narrowed.max_age_hours, narrowed.limit), (Some(48), 2));
        assert!(narrowed.notes.is_empty());
        let fr = narrow_search(&scope, "typhon", Some("fr"), None, None, 2).unwrap_err();
        assert!(fr.contains("not in this app's research grant"), "{fr}");
        assert!(narrow_search(&scope, "x", None, Some("JP"), None, 2).is_err());
        let not_news = parse(&json!({"categories": ["science"]})).unwrap();
        assert!(narrow_search(&not_news, "x", None, None, None, 2).is_err());
        assert!(allows_language(&scope, "zh-CN"));
        assert!(!allows_language(&scope, "en"));
        assert!(allows_language(&unrestricted(), "en"));
    }

    #[test]
    fn crawls_need_the_crawl_limits_and_stay_within_them() {
        let none = parse(&json!({})).unwrap();
        let error = narrow_crawl(&none, &json!({"url": "https://example.org/"})).unwrap_err();
        assert!(
            error.contains("crawling is not in this app's grant"),
            "{error}"
        );
        let scope =
            parse(&json!({"max_depth": 2, "max_pages": 5, "domains_deny": ["bad.example.org"]}))
                .unwrap();
        let narrowed = narrow_crawl(
            &scope,
            &json!({"url": "https://example.org/", "max_depth": 9, "max_pages": 90, "path_prefix": "/a"}),
        )
        .unwrap();
        assert_eq!((narrowed.max_depth, narrowed.max_pages), (2, 5));
        assert_eq!(narrowed.path_prefix.as_deref(), Some("/a"));
        let defaults = narrow_crawl(&scope, &json!({"url": "https://example.org/"})).unwrap();
        assert_eq!((defaults.max_depth, defaults.max_pages), (1, 5));
        assert!(narrow_crawl(&scope, &json!({"url": "https://bad.example.org/"})).is_err());
        assert!(narrow_crawl(&scope, &json!({})).is_err());
    }

    #[test]
    fn reads_are_checked_against_the_domains() {
        let scope = parse(&json!({
            "domains_allow": ["example.org"], "domains_deny": ["bad.example.org"]
        }))
        .unwrap();
        assert!(scope.check_domain("https://www.news.example.org/x").is_ok());
        let error = scope.check_domain("https://bad.example.org/x").unwrap_err();
        assert!(
            error.contains("outside this app's research grant"),
            "{error}"
        );
        assert!(scope.check_domain("https://example.com/x").is_err());
        assert!(scope.check_domain("https://notexample.org/x").is_err());
        assert!(scope.check_domain("not a url").is_err());
    }

    /// The thin parser and octos's `Scope` accept, refuse and narrow the
    /// same grants alike.
    #[cfg(feature = "octos-engine")]
    #[test]
    fn the_thin_parser_matches_octos() {
        let grants = [
            json!({}),
            json!({"langs": ["ZH-hant", "en-gb", "pt_BR"], "regions": [" us ", "gb"]}),
            json!({"domains_allow": ["*.example.org"], "domains_deny": ["www.bad.example.org."]}),
            json!({"max_age_days": 3, "categories": ["news", "it"], "max_results": 7}),
            json!({"max_depth": 2, "max_pages": 40}),
            json!({"langs": ["english"]}),
            json!({"langs": ["e"]}),
            json!({"regions": ["USA"]}),
            json!({"categories": ["video"]}),
            json!({"max_results": 0}),
            json!({"depth": 1}),
            json!({"langs": "en"}),
        ];
        let urls = [
            "https://example.org/a",
            "https://www.news.example.org/a",
            "https://bad.example.org/a",
            "https://example.com/a",
            "http://user@Example.ORG:8080/a",
            "not a url",
        ];
        type Search<'a> = (Option<&'a str>, Option<&'a str>, Option<u32>, u32);
        let searches: [Search; 6] = [
            (None, None, None, 5),
            (Some("zh-TW"), Some("us"), Some(24), 10),
            (Some("en"), None, Some(720), 1),
            (Some("fr"), Some("JP"), None, 3),
            (Some("bad tag!"), None, None, 3),
            (Some("pt"), Some("GB"), Some(100), 8),
        ];
        for grant in &grants {
            let octos = octos_research::toolbox::Scope::from_grant(grant);
            let thin = compat::Scope::from_grant(grant);
            assert_eq!(octos.is_ok(), thin.is_ok(), "{grant}: {octos:?} / {thin:?}");
            let (Ok(octos), Ok(thin)) = (octos, thin) else {
                continue;
            };
            assert_eq!(
                serde_json::to_value(&octos).unwrap(),
                serde_json::to_value(&thin).unwrap(),
                "{grant}"
            );
            for url in urls {
                assert_eq!(
                    octos.check_domain(url),
                    thin.check_domain(url),
                    "{grant} {url}"
                );
            }
            for tag in ["en", "EN-us", "zh-Hant", "pt", "fr"] {
                assert_eq!(
                    octos_research::lang::matches_any(tag, &octos.langs),
                    compat::matches_any(tag, &thin.langs),
                    "{grant} {tag}"
                );
            }
            for args in [
                json!({"url": "https://example.org/a"}),
                json!({"url": "https://example.org/a", "max_depth": 9, "max_pages": 900, "path_prefix": "/news"}),
                json!({"url": "https://bad.example.org/a", "max_depth": 1}),
                json!({"max_depth": 1}),
            ] {
                assert_eq!(
                    octos.crawl_args(&args),
                    compat::crawl_args(&thin, &args),
                    "{grant} {args}"
                );
            }
            for (language, region, hours, limit) in searches {
                let a = narrow_search(&octos, "topic", language, region, hours, limit);
                let b = compat::narrow_search(&thin, "topic", language, region, hours, limit);
                assert_eq!(a, b, "{grant} {language:?} {region:?} {hours:?} {limit}");
            }
        }
    }
}
