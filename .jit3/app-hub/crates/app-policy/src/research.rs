//! The scope of the `research` and `crawl` capabilities, and how a store
//! says it.
//!
//! The scope itself ([`ResearchScope`], octos's `Scope` field for field) is
//! part of the manifest, so it lives in the app contract
//! (`octosense_app_contract::research`, ADR 0005) and is re-exported here.
//! The plain words a store shows for it are App Hub's own.
pub use octosense_app_contract::research::*;

// ------------------------------------------------------------ plain words

/// What `research` lets the app search, in plain words: "news in English and
/// Chinese, from the last 7 days". Derived from the scope alone.
pub fn search_words(scope: &ResearchScope) -> String {
    let what = if scope.categories.is_empty() {
        "the web".to_string()
    } else {
        let mut seen: Vec<&str> = Vec::new();
        for c in &scope.categories {
            let word = category_word(c);
            if !seen.contains(&word) {
                seen.push(word);
            }
        }
        join_and(&seen)
    };
    let langs = if scope.langs.is_empty() {
        "any language".to_string()
    } else {
        let names: Vec<String> = scope.langs.iter().map(|l| language_name(l)).collect();
        join_and(&names)
    };
    let mut out = format!("{what} in {langs}");
    if !scope.regions.is_empty() {
        let label = if scope.regions.len() == 1 { "region" } else { "regions" };
        out.push_str(&format!(", {label} {}", join_and(&scope.regions)));
    }
    out.push_str(", ");
    out.push_str(&age_words(scope.max_age_days));
    out.push_str(&domain_words(scope));
    out
}

/// What `crawl` lets the app reach, in plain words: "following links up to 2
/// deep and reading up to 50 pages a crawl, on any site".
pub fn crawl_words(scope: &ResearchScope) -> String {
    let pages = if scope.max_pages == 1 { "1 page".to_string() } else { format!("{} pages", scope.max_pages) };
    let mut out = format!("following links up to {} deep and reading up to {pages} a crawl", scope.max_depth);
    if scope.domains_allow.is_empty() && scope.domains_deny.is_empty() {
        out.push_str(", on any site");
    } else {
        out.push_str(&domain_words(scope));
    }
    out
}

fn age_words(days: Option<u32>) -> String {
    match days {
        None => "from any time".to_string(),
        Some(0) => "from today".to_string(),
        Some(1) => "from the last day".to_string(),
        Some(n) => format!("from the last {n} days"),
    }
}

fn domain_words(scope: &ResearchScope) -> String {
    let mut out = String::new();
    if !scope.domains_allow.is_empty() {
        out.push_str(&format!(", only on {}", join_or_and(&scope.domains_allow, "and")));
    }
    if !scope.domains_deny.is_empty() {
        out.push_str(&format!(", never on {}", join_or_and(&scope.domains_deny, "or")));
    }
    out
}

fn category_word(category: &str) -> &'static str {
    match category {
        "news" => "news",
        "science" => "science",
        "it" => "technology",
        "social" => "social media",
        _ => "the web",
    }
}

/// An English name for a language tag: "zh-TW" → "Chinese (TW)". A tag this
/// table does not know is shown as written.
fn language_name(tag: &str) -> String {
    let (primary, rest) = match tag.split_once('-') {
        Some((p, r)) => (p, Some(r)),
        None => (tag, None),
    };
    let name = match primary {
        "en" => "English",
        "zh" => "Chinese",
        "ja" => "Japanese",
        "ko" => "Korean",
        "fr" => "French",
        "de" => "German",
        "es" => "Spanish",
        "pt" => "Portuguese",
        "it" => "Italian",
        "ru" => "Russian",
        "ar" => "Arabic",
        "hi" => "Hindi",
        "nl" => "Dutch",
        "sv" => "Swedish",
        "pl" => "Polish",
        "tr" => "Turkish",
        "uk" => "Ukrainian",
        "vi" => "Vietnamese",
        "th" => "Thai",
        "id" => "Indonesian",
        "ms" => "Malay",
        "he" => "Hebrew",
        "fa" => "Persian",
        "el" => "Greek",
        "cs" => "Czech",
        "da" => "Danish",
        "fi" => "Finnish",
        "no" | "nb" => "Norwegian",
        _ => return tag.to_string(),
    };
    match rest {
        Some(rest) => format!("{name} ({rest})"),
        None => name.to_string(),
    }
}

fn join_and<S: AsRef<str>>(items: &[S]) -> String {
    join_or_and(items, "and")
}

fn join_or_and<S: AsRef<str>>(items: &[S], word: &str) -> String {
    match items {
        [] => String::new(),
        [one] => one.as_ref().to_string(),
        [head @ .., last] => {
            let head: Vec<&str> = head.iter().map(|s| s.as_ref()).collect();
            format!("{} {word} {}", head.join(", "), last.as_ref())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(json: &str) -> Result<ResearchScope, String> {
        let s: ResearchScope = serde_json::from_str(json).map_err(|e| format!("scope: {e}"))?;
        s.validated()
    }

    #[test]
    fn the_words_say_the_scope() {
        let s = scope(r#"{"langs":["en","zh"],"categories":["news"],"max_age_days":7}"#).unwrap();
        assert_eq!(search_words(&s), "news in English and Chinese, from the last 7 days");
        let s = scope("{}").unwrap();
        assert_eq!(search_words(&s), "the web in any language, from any time");
        let s = scope(
            r#"{"langs":["zh-TW"],"regions":["tw"],"categories":["news","it"],"max_age_days":1,
                "domains_allow":["cna.com.tw","udn.com"],"domains_deny":["x.com"]}"#,
        )
        .unwrap();
        assert_eq!(
            search_words(&s),
            "news and technology in Chinese (TW), region TW, from the last day, only on cna.com.tw and udn.com, never on x.com"
        );
        let s = scope(r#"{"max_depth":2,"max_pages":50}"#).unwrap();
        assert_eq!(crawl_words(&s), "following links up to 2 deep and reading up to 50 pages a crawl, on any site");
        let s = scope(r#"{"max_depth":1,"max_pages":1,"domains_allow":["docs.rs"]}"#).unwrap();
        assert_eq!(crawl_words(&s), "following links up to 1 deep and reading up to 1 page a crawl, only on docs.rs");
    }
}
