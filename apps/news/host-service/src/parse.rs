//! The sources' formats into [`Draft`]s: RSS 2.0, RSS 1.0 (RDF) and Atom
//! feeds, Hacker News through Algolia, GDELT's DOC 2.0 article list, and
//! OPML subscription lists. Pure functions over text, so every one is tested
//! from fixtures.
use crate::item::Draft;
use crate::text;
use roxmltree::{Document, Node, ParsingOptions};
use serde_json::Value;

const SUMMARY_CHARS: usize = 280;
const MEDIA_NS: &str = "http://search.yahoo.com/mrss/";
/// A feed has at most this many items read; the rest is ignored.
const MAX_ITEMS: usize = 200;

/// How a feed names the outlet in its titles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    #[default]
    Plain,
    /// Google News: `Headline - Outlet`.
    Google,
    /// TechMeme: `Headline (Author / Outlet)`, or `Outlet: headline…` as the
    /// summary.
    Digest,
}

fn parse_xml(xml: &str) -> Result<Document<'_>, String> {
    let options = ParsingOptions { allow_dtd: true, nodes_limit: 200_000, ..ParsingOptions::default() };
    Document::parse_with_options(xml.trim_start_matches('\u{feff}'), options).map_err(|e| format!("not a feed: {e}"))
}

fn local<'a>(node: &Node<'a, '_>) -> &'a str {
    node.tag_name().name()
}

fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|c| c.is_element() && local(c) == name && c.tag_name().namespace() != Some(MEDIA_NS))
}

/// All the text under a node (CDATA included), as written.
fn all_text(node: Node) -> String {
    node.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect()
}

fn child_text(node: Node, name: &str) -> Option<String> {
    child(node, name).map(all_text)
}

/// RSS or Atom into drafts, newest-first as the feed orders them. A document
/// that is not a feed is an error; an item without a title or link is skipped.
pub fn feed(xml: &str, style: Style) -> Result<Vec<Draft>, String> {
    let doc = parse_xml(xml)?;
    let root = doc.root_element();
    let is_atom = match local(&root) {
        "feed" => true,
        "rss" | "RDF" => false,
        other => return Err(format!("not a feed: <{other}>")),
    };
    let entry = if is_atom { "entry" } else { "item" };
    Ok(root
        .descendants()
        .filter(|n| n.is_element() && local(n) == entry)
        .take(MAX_ITEMS)
        .filter_map(|n| feed_item(n, is_atom, style))
        .collect())
}

fn feed_item(node: Node, is_atom: bool, style: Style) -> Option<Draft> {
    let mut title = text::clean(&child_text(node, "title")?);
    if title.is_empty() {
        return None;
    }
    let url = if is_atom { atom_link(node) } else { rss_link(node) }?;
    let mut source = if is_atom {
        child(node, "source").and_then(|s| child_text(s, "title")).map(|t| text::clean(&t)).unwrap_or_default()
    } else {
        child_text(node, "source").map(|t| text::clean(&t)).unwrap_or_default()
    };
    match style {
        Style::Google => {
            if let Some((head, tail)) = text::split_google_source(&title) {
                title = head;
                source = tail;
            }
        }
        Style::Digest => {
            if let Some((head, outlet)) = text::trailing_outlet(&title) {
                title = head;
                source = outlet;
            }
        }
        Style::Plain => {}
    }
    let html = ["description", "summary", "encoded", "content"].iter().find_map(|name| child_text(node, name)).unwrap_or_default();
    let mut summary = text::truncate(&text::clean(&html), SUMMARY_CHARS);
    if let Some((outlet, rest)) = text::outlet_prefix(&title, &summary) {
        if source.is_empty() {
            source = outlet;
        }
        summary = rest;
    }
    summary = text::drop_repeated_title(&title, &summary, &source);
    // Google's description is the headline again, then related headlines
    // from other outlets: not a summary of this story.
    if style == Style::Google && summary.to_lowercase().starts_with(&title.to_lowercase()) {
        summary.clear();
    }
    let published = child_text(node, "pubDate")
        .and_then(|d| parse_date(&d))
        .or_else(|| child_text(node, "published").and_then(|d| parse_date(&d)))
        .or_else(|| child_text(node, "updated").and_then(|d| parse_date(&d)))
        .or_else(|| child_text(node, "date").and_then(|d| parse_date(&d)));
    Some(Draft { title, url, source, published, summary, image: image(node, &html), ..Draft::default() })
}

fn rss_link(node: Node) -> Option<String> {
    child_text(node, "link").map(|l| l.trim().to_string()).filter(|l| text::is_http_url(l)).or_else(|| {
        let guid = child(node, "guid")?;
        if guid.attribute("isPermaLink").is_some_and(|v| v.eq_ignore_ascii_case("false")) {
            return None;
        }
        Some(all_text(guid).trim().to_string()).filter(|g| text::is_http_url(g))
    })
}

fn atom_link(node: Node) -> Option<String> {
    node.children()
        .filter(|c| c.is_element() && local(c) == "link")
        .filter(|l| l.attribute("rel").is_none_or(|r| r == "alternate"))
        .find_map(|l| l.attribute("href").map(str::trim).filter(|h| text::is_http_url(h)).map(str::to_string))
}

fn image(node: Node, html: &str) -> Option<String> {
    let media = |name: &str, typed: bool| {
        node.descendants().filter(|n| n.is_element() && n.tag_name().namespace() == Some(MEDIA_NS) && local(n) == name).find_map(|n| {
            let medium = n.attribute("medium");
            let kind = n.attribute("type");
            let ok = !typed || medium == Some("image") || kind.is_some_and(|t| t.starts_with("image/")) || (medium.is_none() && kind.is_none());
            n.attribute("url").filter(|u| ok && text::is_http_url(u)).map(str::to_string)
        })
    };
    let enclosure = || {
        node.children()
            .filter(|n| n.is_element() && local(n) == "enclosure")
            .find(|n| n.attribute("type").is_some_and(|t| t.starts_with("image/")))
            .and_then(|n| n.attribute("url"))
            .filter(|u| text::is_http_url(u))
            .map(str::to_string)
    };
    media("content", true).or_else(|| media("thumbnail", false)).or_else(enclosure).or_else(|| text::first_image(html)).map(text::https_image)
}

/// RFC 2822 (RSS), RFC 3339 (Atom, Dublin Core) or GDELT's `20260927T101500Z`.
pub fn parse_date(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    if let Ok(t) = chrono::DateTime::parse_from_rfc2822(raw) {
        return Some(t.timestamp());
    }
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(t.timestamp());
    }
    // Zone names chrono's RFC 2822 parser does not take.
    for (zone, offset) in [(" UTC", " +0000"), (" Z", " +0000"), (" EDT", " -0400"), (" PDT", " -0700")] {
        if let Some(head) = raw.strip_suffix(zone) {
            if let Ok(t) = chrono::DateTime::parse_from_rfc2822(&format!("{head}{offset}")) {
                return Some(t.timestamp());
            }
        }
    }
    if let Ok(t) = chrono::NaiveDateTime::parse_from_str(raw, "%Y%m%dT%H%M%SZ") {
        return Some(t.and_utc().timestamp());
    }
    chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0)).map(|t| t.and_utc().timestamp())
}

/// Hacker News through Algolia's search API (`hits`): the story's own link,
/// and the thread as its discussion.
pub fn hn(json: &str) -> Result<Vec<Draft>, String> {
    let data: Value = serde_json::from_str(json).map_err(|e| format!("not Hacker News JSON: {e}"))?;
    let hits = data["hits"].as_array().ok_or("not Hacker News JSON: no hits")?;
    Ok(hits
        .iter()
        .take(MAX_ITEMS)
        .filter_map(|hit| {
            let title = text::clean(hit["title"].as_str()?);
            let object_id = match &hit["objectID"] {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => return None,
            };
            if title.is_empty() || object_id.is_empty() {
                return None;
            }
            let discussion = format!("https://news.ycombinator.com/item?id={object_id}");
            let url = hit["url"].as_str().filter(|u| text::is_http_url(u)).map_or(discussion.clone(), str::to_string);
            Some(Draft {
                title,
                url,
                source: "Hacker News".into(),
                published: hit["created_at_i"].as_i64(),
                discussion: Some(discussion),
                points: hit["points"].as_u64(),
                comments: hit["num_comments"].as_u64(),
                ..Draft::default()
            })
        })
        .collect())
}

/// GDELT DOC 2.0 `mode=ArtList&format=json`: `{articles: [{url, title,
/// seendate, socialimage, domain, language}]}`. An empty answer (no
/// matches) is `{}`, which is no articles, not an error.
pub fn gdelt(json: &str) -> Result<Vec<Draft>, String> {
    let json = json.trim();
    if json.is_empty() {
        return Ok(Vec::new());
    }
    let data: Value = serde_json::from_str(json).map_err(|_| {
        // GDELT answers a bad query with a plain-text sentence.
        format!("GDELT: {}", text::truncate(json, 120))
    })?;
    let Some(articles) = data["articles"].as_array() else { return Ok(Vec::new()) };
    Ok(articles
        .iter()
        .take(MAX_ITEMS)
        .filter_map(|a| {
            let title = text::clean(a["title"].as_str()?);
            let url = a["url"].as_str().filter(|u| text::is_http_url(u))?.to_string();
            if title.is_empty() {
                return None;
            }
            let domain = a["domain"].as_str().unwrap_or("").trim();
            Some(Draft {
                title,
                url,
                source: domain.strip_prefix("www.").unwrap_or(domain).to_string(),
                published: a["seendate"].as_str().and_then(parse_date),
                image: a["socialimage"].as_str().filter(|u| text::is_http_url(u)).map(|u| text::https_image(u.to_string())),
                lang: a["language"].as_str().and_then(crate::sources::iso_from_gdelt_language).map(str::to_string),
                ..Draft::default()
            })
        })
        .collect())
}

/// One subscription in an OPML file.
#[derive(Clone, Debug, PartialEq)]
pub struct OpmlFeed {
    pub url: String,
    pub title: String,
    /// The enclosing outlines' names, lower case: the category a feed was
    /// filed under.
    pub topics: Vec<String>,
}

/// The feeds an OPML subscription list names (`<outline xmlUrl=…>`), with
/// the folders they sit in as topics.
pub fn opml(xml: &str) -> Result<Vec<OpmlFeed>, String> {
    let doc = parse_xml(xml).map_err(|_| "not an OPML file".to_string())?;
    if local(&doc.root_element()) != "opml" {
        return Err("not an OPML file".into());
    }
    let mut out = Vec::new();
    for node in doc.descendants().filter(|n| n.is_element() && local(n) == "outline") {
        let Some(url) = node.attribute("xmlUrl").map(str::trim).filter(|u| text::is_http_url(u)) else { continue };
        let title = node.attribute("title").or_else(|| node.attribute("text")).unwrap_or(url).trim().to_string();
        let topics = node
            .ancestors()
            .skip(1)
            .filter(|a| a.is_element() && local(a) == "outline")
            .filter_map(|a| a.attribute("text").or_else(|| a.attribute("title")))
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        out.push(OpmlFeed { url: url.to_string(), title, topics });
    }
    Ok(out)
}
