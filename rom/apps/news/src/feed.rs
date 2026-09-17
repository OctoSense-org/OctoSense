//! RSS 2.0 and Atom into headline rows without an XML crate: items are
//! split on `<item` / `<entry`, fields are the first matching element
//! (CDATA unwrapped, entities decoded, tags stripped, whitespace collapsed).
//! Good enough for the feeds this app reads; a malformed document yields
//! fewer rows, never a panic. Known limitation of the splitter: an `<item`
//! or `<title` inside a CDATA description can confuse it, which degrades to
//! a missing summary or a wrong title for that item, never a panic.

use crate::model::{is_http_url, Headline, SUMMARY_CHARS};

/// Parse a feed body. `split_source` handles Google News titles, which end
/// in ` - Source`.
pub fn parse(xml: &str, split_source: bool) -> Result<Vec<Headline>, String> {
    // A feed that prefixes every Atom element (`<atom:entry>`) reads like
    // a bare one once the prefix is dropped.
    let unprefixed;
    let xml = if !xml.contains("<entry") && xml.contains("<atom:entry") {
        unprefixed = xml.replace("<atom:", "<").replace("</atom:", "</");
        unprefixed.as_str()
    } else {
        xml
    };
    let (open, close, is_atom) = feed_kind(xml).ok_or_else(|| "not an RSS or Atom feed".to_string())?;
    let rows = xml
        .split(open)
        .skip(1)
        .filter_map(|chunk| {
            // `chunk` starts right after `<item` / `<entry`: attributes or `>`.
            let body = &chunk[chunk.find('>')? + 1..];
            let body = body.split_once(close).map_or(body, |(b, _)| b);
            parse_item(body, is_atom, split_source)
        })
        .collect();
    Ok(rows)
}

const RSS: (&str, &str, bool) = ("<item", "</item>", false);
const ATOM: (&str, &str, bool) = ("<entry", "</entry>", true);

/// The item element to split on and whether the feed is Atom: the root
/// element decides (`<feed>`; `<rss>` or `<rdf:RDF>`), else the first item
/// element found.
fn feed_kind(xml: &str) -> Option<(&'static str, &'static str, bool)> {
    match root_element(xml) {
        Some("feed") => Some(ATOM),
        Some("rss" | "rdf:RDF") => Some(RSS),
        _ if has_element(xml, "item") => Some(RSS),
        _ if has_element(xml, "entry") => Some(ATOM),
        _ => None,
    }
}

/// The name of the root element: the first element after the prolog (a
/// byte-order mark, whitespace, `<?...?>`, `<!--...-->`, `<!DOCTYPE ...>`).
/// Looking only there keeps a `<feed>` inside a description from naming
/// the kind.
fn root_element(xml: &str) -> Option<&str> {
    let prolog_space = |c: char| c.is_whitespace() || c == '\u{feff}';
    let mut rest = xml.trim_start_matches(prolog_space);
    loop {
        let after_prolog = if let Some(after) = rest.strip_prefix("<?") {
            after.split_once("?>")?.1
        } else if let Some(after) = rest.strip_prefix("<!--") {
            after.split_once("-->")?.1
        } else if let Some(after) = rest.strip_prefix("<!") {
            // A DOCTYPE with an internal subset (`[...]`) ends at its first
            // inner `>`; what follows is not an element, so the caller falls
            // back to the item heuristic. Feeds do not carry one.
            after.split_once('>')?.1
        } else {
            break;
        };
        rest = after_prolog.trim_start_matches(prolog_space);
    }
    let name = rest.strip_prefix('<')?;
    let end = name.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(name.len());
    Some(&name[..end]).filter(|n| !n.is_empty())
}

/// Whether `xml` opens an element named exactly `name`.
fn has_element(xml: &str, name: &str) -> bool {
    let open = format!("<{name}");
    let mut from = 0;
    while let Some(pos) = xml[from..].find(&open) {
        from += pos + open.len();
        if xml[from..].chars().next().is_some_and(|c| c == '>' || c == '/' || c.is_whitespace()) {
            return true;
        }
    }
    false
}

/// One `<item>` / `<entry>` body as a row; None without a title or an http
/// link.
fn parse_item(body: &str, is_atom: bool, split_source: bool) -> Option<Headline> {
    let mut title = clean_text(element_text(body, "title")?);
    if title.is_empty() {
        return None;
    }
    let link = if is_atom { atom_link(body) } else { rss_link(body) }?;
    let mut source = if is_atom {
        String::new()
    } else {
        element_text(body, "source").map(clean_text).unwrap_or_default()
    };
    if split_source {
        if let Some((head, tail)) = split_google_source(&title) {
            title = head;
            source = tail;
        }
    }
    let summary = element_text(body, "description")
        .or_else(|| element_text(body, "summary"))
        .or_else(|| element_text(body, "content"))
        .or_else(|| element_text(body, "content:encoded"))
        .map(|d| truncate(&clean_text(d), SUMMARY_CHARS))
        .unwrap_or_default();
    let published = element_text(body, "pubDate")
        .and_then(|d| parse_rfc822(d.trim()))
        .or_else(|| element_text(body, "published").and_then(|d| parse_rfc3339(d.trim())))
        .or_else(|| element_text(body, "updated").and_then(|d| parse_rfc3339(d.trim())))
        .or_else(|| element_text(body, "dc:date").and_then(|d| parse_rfc3339(d.trim())));
    // The model stamps `source_id` when the rows land: one reader serves
    // every feed.
    Some(Headline { title, link, source, source_id: String::new(), published, points: None, comments: None, summary, discussion: None })
}

/// Google News titles end in ` - Source`; the last dash is the separator.
fn split_google_source(title: &str) -> Option<(String, String)> {
    let (head, tail) = title.rsplit_once(" - ")?;
    let (head, tail) = (head.trim(), tail.trim());
    (!head.is_empty() && !tail.is_empty()).then(|| (head.to_string(), tail.to_string()))
}

/// The first `<tag ...>...</tag>` in `xml` as its attribute text and raw
/// inner text. A tag name must end at `>`, whitespace or `/`, so `title`
/// never matches `titleX`. A self-closing element has empty inner text.
fn element<'a>(xml: &'a str, tag: &str) -> Option<(&'a str, &'a str)> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut from = 0;
    while let Some(pos) = xml[from..].find(&open) {
        let at = from + pos;
        let after = &xml[at + open.len()..];
        let next = after.chars().next()?;
        if next != '>' && next != '/' && !next.is_whitespace() {
            from = at + 1;
            continue;
        }
        let tag_end = after.find('>')?;
        let attrs = after[..tag_end].trim_end();
        if let Some(attrs) = attrs.strip_suffix('/') {
            return Some((attrs, ""));
        }
        let content = &after[tag_end + 1..];
        let end = content.find(&close)?;
        return Some((attrs, &content[..end]));
    }
    None
}

/// The raw inner text of the first `<tag>` in `xml`.
fn element_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    element(xml, tag).map(|(_, text)| text)
}

/// An RSS item's `<link>` when it is an http(s) URL, else its `<guid>` when
/// that is one and not marked `isPermaLink="false"`.
fn rss_link(body: &str) -> Option<String> {
    element_text(body, "link")
        .map(clean_text)
        .filter(|l| is_http_url(l))
        .or_else(|| permalink_guid(body))
}

fn permalink_guid(body: &str) -> Option<String> {
    let (attrs, text) = element(body, "guid")?;
    if attribute(attrs, "isPermaLink").is_some_and(|v| v.eq_ignore_ascii_case("false")) {
        return None;
    }
    Some(clean_text(text)).filter(|g| is_http_url(g))
}

/// Atom links are `<link href="..." rel="..."/>`; the article is the one
/// without `rel` or with `rel="alternate"`.
fn atom_link(xml: &str) -> Option<String> {
    let mut from = 0;
    while let Some(pos) = xml[from..].find("<link") {
        let at = from + pos;
        let after = &xml[at + 5..];
        let tag_end = after.find('>')?;
        let attrs = &after[..tag_end];
        // Past the whole tag: restarting inside it would rescan to its `>`
        // once per `<link` in a wall of them.
        from = at + 5 + tag_end + 1;
        let rel = attribute(attrs, "rel");
        if rel.as_deref().is_some_and(|r| r != "alternate") {
            continue;
        }
        if let Some(href) = attribute(attrs, "href") {
            let href = decode_entities(&href);
            if is_http_url(&href) {
                return Some(href);
            }
        }
    }
    None
}

fn attribute(attrs: &str, name: &str) -> Option<String> {
    let key = format!("{name}=");
    let mut from = 0;
    while let Some(pos) = attrs[from..].find(&key) {
        let at = from + pos;
        let before_ok = at == 0 || attrs[..at].ends_with(|c: char| c.is_whitespace());
        let rest = &attrs[at + key.len()..];
        from = at + key.len();
        if !before_ok {
            continue;
        }
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            continue;
        }
        let value = &rest[1..];
        let end = value.find(quote)?;
        return Some(value[..end].to_string());
    }
    None
}

/// CDATA markers dropped (they can sit anywhere in a description), entities
/// decoded, HTML tags stripped, entities decoded again (feeds double-encode
/// HTML in descriptions), whitespace collapsed.
pub(crate) fn clean_text(raw: &str) -> String {
    let text = raw.replace("<![CDATA[", "").replace("]]>", "");
    let text = decode_entities(text.trim());
    let text = strip_tags(&text);
    let text = decode_entities(&text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What an angle-bracketed run turns into once stripped.
enum TagKind {
    /// Wraps words inside a sentence: dropped, so `<a>Example</a>:` keeps
    /// its colon attached.
    Inline,
    /// Separates words or lines: a space, so `line<br/>break` never fuses.
    Block,
    /// Not an HTML tag at all: a title may well say `<3` or `x < y`.
    Text,
}

const INLINE_TAGS: &[&str] = &[
    "a", "abbr", "b", "big", "cite", "code", "del", "em", "font", "i", "ins", "mark", "q", "s", "small", "span",
    "strike", "strong", "sub", "sup", "tt", "u",
];

const BLOCK_TAGS: &[&str] = &[
    "article", "aside", "audio", "blockquote", "br", "center", "dd", "div", "dl", "dt", "figcaption", "figure",
    "footer", "h1", "h2", "h3", "h4", "h5", "h6", "header", "hr", "iframe", "img", "li", "nav", "ol", "p", "picture",
    "pre", "section", "source", "table", "tbody", "td", "th", "thead", "tr", "ul", "video",
];

fn tag_kind(tag: &str) -> TagKind {
    // Comments and processing instructions are markup too.
    if tag.starts_with('!') || tag.starts_with('?') {
        return TagKind::Block;
    }
    let name = tag
        .trim_start_matches('/')
        .split(|c: char| c.is_whitespace() || c == '/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if INLINE_TAGS.contains(&name.as_str()) {
        TagKind::Inline
    } else if BLOCK_TAGS.contains(&name.as_str()) {
        TagKind::Block
    } else {
        TagKind::Text
    }
}

/// Remove known HTML tags (see `TagKind`); an unclosed `<` is text.
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        let Some(close) = after.find('>') else {
            rest = after;
            break;
        };
        match tag_kind(&after[1..close]) {
            TagKind::Inline => {}
            TagKind::Block => out.push(' '),
            TagKind::Text => out.push_str(&after[..=close]),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// The scan for an entity's `;` looks at `&` and the ten bytes after it,
/// so an entity is at most nine characters between `&` and `;`; the
/// longest wanted, `&#x10FFFF;`, has eight.
const MAX_ENTITY_LEN: usize = 10;

/// The named entities feeds actually use, plus numeric ones. A numeric
/// entity for a control character (other than tab, newline and carriage
/// return) stays as written, so no feed can smuggle one into a label.
pub(crate) fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos..];
        // Only look as far as an entity can reach, so a wall of `&` stays linear.
        let Some(end) = after.bytes().take(MAX_ENTITY_LEN + 1).position(|b| b == b';') else {
            out.push('&');
            rest = &after[1..];
            continue;
        };
        let entity = &after[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "hellip" => Some('…'),
            "lsquo" => Some('‘'),
            "rsquo" => Some('’'),
            "ldquo" => Some('“'),
            "rdquo" => Some('”'),
            _ => numeric_entity(entity),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `#65` or `#x41`, digits only (no sign), and never a control character
/// other than tab, newline or carriage return.
fn numeric_entity(entity: &str) -> Option<char> {
    let number = entity.strip_prefix('#')?;
    let code = match number.strip_prefix(['x', 'X']) {
        Some(hex) if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) => u32::from_str_radix(hex, 16).ok()?,
        Some(_) => return None,
        None if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) => number.parse().ok()?,
        None => return None,
    };
    char::from_u32(code).filter(|c| !c.is_control() || matches!(c, '\n' | '\t' | '\r'))
}

/// At most `max` characters: cut to `max - 1`, a trailing space dropped, an
/// ellipsis appended.
pub(crate) fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

/// `Wed, 16 Sep 2026 08:15:01 -0400`, the weekday and seconds optional,
/// the zone `GMT`/`UT`/`UTC`/`Z`, `±HHMM`, or a US abbreviation.
pub(crate) fn parse_rfc822(text: &str) -> Option<i64> {
    let mut parts: Vec<&str> = text.split_whitespace().collect();
    if parts.first().is_some_and(|p| p.ends_with(',')) {
        parts.remove(0);
    }
    if parts.len() < 4 {
        return None;
    }
    let day: i64 = parts[0].parse().ok()?;
    let month_name = parts[1].to_lowercase();
    let month = MONTHS.iter().position(|m| month_name.starts_with(m))? as i64 + 1;
    let year: i64 = parts[2].parse().ok()?;
    let year = if year < 100 { year + 2000 } else { year };
    let clock = hms(parts[3])?;
    let offset = parts.get(4).map(|z| zone_offset(z)).unwrap_or(0);
    unix_seconds(year, month, day, clock, offset)
}

/// `2026-09-16T08:30:00+02:00`, `...Z`, fractional seconds allowed, a
/// missing zone read as UTC.
pub(crate) fn parse_rfc3339(text: &str) -> Option<i64> {
    let (date, time) = text.split_once(['T', 't', ' '])?;
    let mut d = date.split('-');
    let year: i64 = d.next()?.parse().ok()?;
    let month: i64 = d.next()?.parse().ok()?;
    let day: i64 = d.next()?.parse().ok()?;
    let zone_at = time.find(['Z', 'z', '+']).or_else(|| time.rfind('-')).unwrap_or(time.len());
    let (clock, zone) = time.split_at(zone_at);
    let clock = hms(clock.split('.').next()?)?;
    unix_seconds(year, month, day, clock, zone_offset(zone))
}

/// Unix seconds for a civil date and clock in a zone `offset` seconds east
/// of UTC; None when a date field is out of range. A day past its month's
/// end is let through, as a feed's own clock would have it.
fn unix_seconds(year: i64, month: i64, day: i64, (h, m, s): (i64, i64, i64), offset: i64) -> Option<i64> {
    if !(1..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + h * 3600 + m * 60 + s - offset)
}

/// `HH:MM` or `HH:MM:SS`, each field in range (a leap second allowed).
fn hms(text: &str) -> Option<(i64, i64, i64)> {
    let mut p = text.split(':');
    let h: i64 = p.next()?.parse().ok()?;
    let m: i64 = p.next()?.parse().ok()?;
    let s: i64 = match p.next() {
        Some(s) => s.parse().ok()?,
        None => 0,
    };
    ((0..=23).contains(&h) && (0..=59).contains(&m) && (0..=60).contains(&s)).then_some((h, m, s))
}

/// Seconds east of UTC for a zone token. Zone names other than the UTC
/// spellings and the US abbreviations (CET, BST, AEST, ...) are read as
/// UTC: a time off by a few hours beats no time at all.
fn zone_offset(zone: &str) -> i64 {
    match zone.to_uppercase().as_str() {
        "GMT" | "UT" | "UTC" | "Z" | "" => 0,
        "EST" => -5 * 3600,
        "EDT" => -4 * 3600,
        "CST" => -6 * 3600,
        "CDT" => -5 * 3600,
        "MST" => -7 * 3600,
        "MDT" => -6 * 3600,
        "PST" => -8 * 3600,
        "PDT" => -7 * 3600,
        z => {
            let sign = if z.starts_with('-') { -1 } else { 1 };
            let digits: String = z.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() < 4 {
                return 0;
            }
            let hh: i64 = digits[..2].parse().unwrap_or(0);
            let mm: i64 = digits[2..4].parse().unwrap_or(0);
            sign * (hh * 3600 + mm * 60)
        }
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
pub(crate) fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    const TECHMEME: &str = include_str!("../tests/fixtures/techmeme.xml");
    const GOOGLE: &str = include_str!("../tests/fixtures/google.xml");
    const ATOM: &str = include_str!("../tests/fixtures/atom.xml");

    #[test]
    fn rss_items_become_rows_with_clean_titles_and_summaries() {
        let rows = parse(TECHMEME, false).unwrap();
        assert_eq!(rows.len(), 2, "an item without a title or without a link is skipped");
        assert_eq!(rows[0].title, "Acme & Co launches \"Widget 2\" for $99");
        assert_eq!(rows[0].link, "https://www.techmeme.com/260916/p1#a260916p1");
        assert_eq!(rows[0].summary, "Example: Acme launches Widget 2 — a $99 device.");
        assert_eq!(rows[0].source, "");
        // 08:15:01 -0400 is 12:15:01 UTC on 2026-09-16.
        assert_eq!(rows[0].published, Some(1_789_560_901));
        assert_eq!(rows[1].title, "Second story with <brackets>");
        assert_eq!(rows[1].published, Some(1_789_542_000));
    }

    #[test]
    fn google_titles_split_their_trailing_source() {
        let rows = parse(GOOGLE, true).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "Markets rally as rates hold");
        assert_eq!(rows[0].source, "Reuters");
        assert_eq!(rows[0].summary, "Markets rally as rates hold Reuters");
        assert_eq!(rows[1].title, "A title with - a dash", "only the LAST dash separates the source");
        assert_eq!(rows[1].source, "The Verge");
        let unsplit = parse(GOOGLE, false).unwrap();
        assert_eq!(unsplit[0].title, "Markets rally as rates hold - Reuters");
        assert_eq!(unsplit[0].source, "Reuters", "the <source> element still fills the field");
    }

    #[test]
    fn atom_entries_use_the_alternate_link_and_summary_or_content() {
        let rows = parse(ATOM, false).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "First & foremost");
        assert_eq!(rows[0].link, "https://blog.example/posts/first", "rel=self is skipped");
        assert_eq!(rows[0].summary, "A short summary.");
        // 08:30 +02:00 is 06:30 UTC.
        assert_eq!(rows[0].published, Some(1_789_540_200));
        assert_eq!(rows[1].link, "https://blog.example/posts/second");
        assert_eq!(rows[1].summary, "Full content here.");
        assert_eq!(rows[1].published, Some(1_789_502_400));
    }

    #[test]
    fn the_root_element_names_the_kind_so_an_empty_feed_is_not_an_error() {
        assert!(parse(r#"<rss version="2.0"><channel><title>Empty</title></channel></rss>"#, false).unwrap().is_empty());
        assert!(parse(r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Empty</title></feed>"#, false).unwrap().is_empty());
        assert!(parse(r#"<rdf:RDF xmlns="http://purl.org/rss/1.0/"><channel/></rdf:RDF>"#, false).unwrap().is_empty());
    }

    #[test]
    fn a_prefixed_atom_feed_parses_like_a_bare_one() {
        let xml = r#"<?xml version="1.0"?><atom:feed xmlns:atom="http://www.w3.org/2005/Atom"><atom:title>Prefixed</atom:title><atom:entry><atom:title>Only entry</atom:title><atom:link rel="self" href="https://p.example/only.atom"/><atom:link href="https://p.example/only"/><atom:updated>2026-09-16T09:00:00Z</atom:updated></atom:entry></atom:feed>"#;
        let rows = parse(xml, false).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Only entry");
        assert_eq!(rows[0].link, "https://p.example/only");
        assert_eq!(rows[0].published, Some(1_789_549_200));
    }

    #[test]
    fn an_rss_item_without_a_link_falls_back_to_a_permalink_guid() {
        let xml = r#"<rss version="2.0"><channel>
<item><title>Guid only</title><guid isPermaLink="true">https://g.example/post</guid></item>
<item><title>Not a permalink</title><guid isPermaLink="false">https://g.example/id</guid></item>
<item><title>Bare guid</title><guid>https://g.example/bare</guid></item>
<item><title>Empty link</title><link></link><guid>https://g.example/empty-link</guid></item>
<item><title>Link wins</title><link>https://g.example/link</link><guid>https://g.example/guid</guid></item>
<item><title>Opaque guid</title><guid>tag:g.example,2026:1</guid></item>
<item><title>Relative link</title><link>/post/1</link><guid>https://g.example/relative</guid></item>
<item><title>Relative link only</title><link>/post/2</link></item>
</channel></rss>"#;
        let links: Vec<String> = parse(xml, false).unwrap().into_iter().map(|h| h.link).collect();
        assert_eq!(
            links,
            ["https://g.example/post", "https://g.example/bare", "https://g.example/empty-link", "https://g.example/link", "https://g.example/relative"],
            "a non-http link falls through to the guid"
        );
    }

    #[test]
    fn an_item_with_only_content_encoded_uses_it_as_the_summary() {
        let xml = r#"<rss version="2.0"><channel><item><title>Encoded</title><link>https://e.example/1</link><content:encoded><![CDATA[<p>Full <b>body</b> here.</p>]]></content:encoded></item></channel></rss>"#;
        let rows = parse(xml, false).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].summary, "Full body here.");
    }

    #[test]
    fn the_root_is_the_first_element_after_the_prolog_not_a_tag_inside_a_description() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- generated -->
<!DOCTYPE rss>
<rss version="2.0"><channel>
<item><title>About feeds</title><link>https://r.example/feeds</link><description><![CDATA[Use a <feed> element for Atom.]]></description></item>
</channel></rss>"#;
        let rows = parse(xml, false).unwrap();
        assert_eq!(rows.len(), 1, "an RSS feed mentioning <feed> is still RSS");
        assert_eq!(rows[0].summary, "Use a <feed> element for Atom.");
        assert_eq!(root_element(xml), Some("rss"));
        assert_eq!(root_element("\u{feff}  <?xml version=\"1.0\"?> <feed xmlns=\"x\">"), Some("feed"));
        assert_eq!(root_element("<rdf:RDF xmlns=\"x\"/>"), Some("rdf:RDF"));
        assert_eq!(root_element("<!-- unterminated"), None);
        assert_eq!(root_element("plain text"), None);
    }

    #[test]
    fn not_a_feed_is_an_error_and_garbage_never_panics() {
        assert!(parse("<html><body>nope</body></html>", false).is_err());
        assert!(parse("", false).is_err());
        for junk in ["<item>", "<item><title>", "<entry><title>x</title><link href=", "<item><title>t</title><link>", "<rss><item><title><![CDATA[unterminated"] {
            let _ = parse(junk, true);
        }
        // Absurd fields are out of range, never an overflow.
        assert_eq!(parse_rfc822("16 Sep 9999999999999999 12:15:01 GMT"), None);
        assert_eq!(parse_rfc822("9223372036854775807 Sep 2026 12:15:01 GMT"), None);
        assert_eq!(parse_rfc3339("2026-09-16T9223372036854775807:00:00Z"), None);
        assert_eq!(parse_rfc3339("2026-99-99T99:99:99Z"), None);
    }

    #[test]
    fn text_cleaning_decodes_entities_strips_tags_and_collapses_space() {
        assert_eq!(clean_text("<![CDATA[ a  &amp;   b ]]>"), "a & b");
        assert_eq!(clean_text("&lt;b&gt;bold&lt;/b&gt; &#39;q&#39; &#x41;&nbsp;x"), "bold 'q' A x");
        assert_eq!(clean_text("line<br/>break"), "line break");
        assert_eq!(clean_text("before <![CDATA[middle]]> after"), "before middle after");
        assert_eq!(clean_text("before <![CDATA[<b>bold</b> x]]> after"), "before bold x after");
        assert_eq!(truncate("héllo wörld", 5), "héll…");
        assert_eq!(truncate("hello world", 7), "hello…");
        assert_eq!(truncate("short", 10), "short");
    }

    #[test]
    fn numeric_entities_decode_printable_characters_only() {
        assert_eq!(decode_entities("&#65;&#x42;"), "AB");
        assert_eq!(decode_entities("a&#10;b&#9;c&#13;d"), "a\nb\tc\rd");
        assert_eq!(clean_text("a&#13;\nb"), "a b", "a carriage return is whitespace, collapsed away");
        assert_eq!(decode_entities("a&#0;b&#1;c&#127;d"), "a&#0;b&#1;c&#127;d");
        // The scan for `;` reaches nine characters past `&` and no further.
        assert_eq!(decode_entities("&#00000065;"), "A");
        assert_eq!(decode_entities("&#000000065;"), "&#000000065;");
        assert_eq!(decode_entities("&#+65;"), "&#+65;");
        assert_eq!(decode_entities("&#x+41;"), "&#x+41;");
    }

    #[test]
    fn a_wall_of_ampersands_cleans_in_linear_time() {
        // Each `&` once scanned to the next `;` anywhere in the text: quadratic.
        let wall = "&".repeat(200 * 1024);
        assert_eq!(clean_text(&wall), wall);
    }

    #[test]
    fn a_wall_of_link_tags_parses_in_linear_time() {
        // Each `<link` once rescanned to the closing `>` of the tag after it: quadratic.
        let wall = "<link ".repeat(50 * 1024);
        let xml = format!("<feed><entry><title>Wall</title>{wall}></entry></feed>");
        assert!(parse(&xml, false).unwrap().is_empty(), "no href, no row");
    }

    #[test]
    fn only_http_and_https_links_make_rows() {
        let rss = r#"<rss><channel><item><title>Odd scheme</title><link>httpx://x.example/1</link></item><item><title>Odd guid</title><guid>httpx://x.example/2</guid></item><item><title>Upper</title><link>HTTPS://X.EXAMPLE/3</link></item></channel></rss>"#;
        let links: Vec<String> = parse(rss, false).unwrap().into_iter().map(|h| h.link).collect();
        assert_eq!(links, ["HTTPS://X.EXAMPLE/3"], "httpx is not http; the scheme is case-insensitive");
        let atom = r#"<feed><entry><title>Odd href</title><link href="httpx://x.example/4"/></entry></feed>"#;
        assert!(parse(atom, false).unwrap().is_empty());
    }

    #[test]
    fn dates_parse_rfc822_and_rfc3339_with_zones() {
        assert_eq!(parse_rfc822("Wed, 16 Sep 2026 12:15:01 GMT"), Some(1_789_560_901));
        assert_eq!(parse_rfc822("16 Sep 2026 12:15:01 +0000"), Some(1_789_560_901));
        assert_eq!(parse_rfc822("Wed, 16 Sep 2026 08:15:01 EDT"), Some(1_789_560_901));
        assert_eq!(parse_rfc822("Wed, 16 Sep 2026 12:15 GMT"), Some(1_789_560_900));
        assert_eq!(parse_rfc822("garbage"), None);
        assert_eq!(parse_rfc3339("2026-09-16T12:15:01Z"), Some(1_789_560_901));
        assert_eq!(parse_rfc3339("2026-09-16T14:15:01.250+02:00"), Some(1_789_560_901));
        assert_eq!(parse_rfc3339("2026-09-16T05:15:01-07:00"), Some(1_789_560_901));
        assert_eq!(parse_rfc3339("2026-09-16T12:15:01"), Some(1_789_560_901), "a missing zone is UTC");
        assert_eq!(parse_rfc3339("2026-09-16"), None);
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    }
}
