//! Text out of feed markup: tags stripped, entities decoded, whitespace
//! collapsed, and the few title conventions news feeds use to name the outlet.

const MAX_ENTITY_LEN: usize = 10;
const OUTLET_CHARS: usize = 40;
const TITLE_MATCH_CHARS: usize = 24;

/// Plain text from a fragment of HTML (or text that may hold some): tags
/// removed (block tags leave a space), entities decoded twice (feeds often
/// escape them once more inside CDATA), whitespace collapsed.
pub fn clean(raw: &str) -> String {
    let text = decode_entities(raw.trim());
    let text = strip_tags(&text);
    let text = decode_entities(&text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// At most `max` characters, cut at a word where one is near, with an ellipsis.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    let cut = match cut.rfind(' ') {
        Some(space) if space > cut.len() * 2 / 3 => cut[..space].to_string(),
        _ => cut,
    };
    format!("{}…", cut.trim_end_matches([' ', ',', ';', ':', '.', '-']))
}

const INLINE_TAGS: &[&str] = &[
    "a", "abbr", "b", "big", "cite", "code", "del", "em", "font", "i", "ins", "mark", "q", "s", "small", "span", "strike",
    "strong", "sub", "sup", "tt", "u",
];

const BLOCK_TAGS: &[&str] = &[
    "article", "aside", "audio", "blockquote", "br", "center", "dd", "div", "dl", "dt", "figcaption", "figure", "footer", "h1", "h2", "h3", "h4",
    "h5", "h6", "header", "hr", "iframe", "img", "li", "nav", "ol", "p", "picture", "pre", "section", "source", "table", "tbody", "td", "th",
    "thead", "tr", "ul", "video",
];

fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    // Script and style bodies are not text.
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        let Some(close) = after.find('>') else {
            out.push_str(after);
            return out;
        };
        let tag = &after[1..close];
        let name = tag.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("").to_ascii_lowercase();
        if !tag.starts_with('/') && (name == "script" || name == "style") {
            let end = format!("</{name}");
            let lower = after.to_ascii_lowercase();
            match lower.find(&end) {
                Some(at) => {
                    rest = &after[at..];
                    rest = rest.find('>').map_or("", |gt| &rest[gt + 1..]);
                    out.push(' ');
                    continue;
                }
                None => return out,
            }
        }
        let markup = tag.starts_with('!') || tag.starts_with('?');
        if INLINE_TAGS.contains(&name.as_str()) {
        } else if markup || BLOCK_TAGS.contains(&name.as_str()) {
            out.push(' ');
        } else {
            // Not HTML ("a < b", "<brackets>" in a title): text.
            out.push('<');
            rest = &after[1..];
            continue;
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// The named entities feeds use, and numeric ones.
pub fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos..];
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
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "hellip" => Some('…'),
            "lsquo" => Some('‘'),
            "rsquo" => Some('’'),
            "ldquo" => Some('“'),
            "rdquo" => Some('”'),
            "laquo" => Some('«'),
            "raquo" => Some('»'),
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

fn numeric_entity(entity: &str) -> Option<char> {
    let digits = entity.strip_prefix('#')?;
    let code = match digits.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => digits.parse().ok()?,
    };
    char::from_u32(code).filter(|c| *c != '\0')
}

/// Google News titles end in ` - Outlet`.
pub fn split_google_source(title: &str) -> Option<(String, String)> {
    let (head, tail) = title.rsplit_once(" - ")?;
    let (head, tail) = (head.trim(), tail.trim());
    (!head.is_empty() && !tail.is_empty()).then(|| (head.to_string(), tail.to_string()))
}

/// TechMeme titles end in ` (Outlet)` or ` (Author / Outlet)`.
pub fn trailing_outlet(title: &str) -> Option<(String, String)> {
    let title = title.trim();
    let inner_end = title.strip_suffix(')')?;
    let open = inner_end.rfind('(')?;
    let inner = inner_end[open + 1..].trim();
    let head = inner_end[..open].trim_end();
    if inner.is_empty() || head.is_empty() || inner.chars().count() > OUTLET_CHARS || inner.contains('(') {
        return None;
    }
    let outlet = inner.rsplit('/').next().unwrap_or(inner).trim();
    (!outlet.is_empty()).then(|| (head.to_string(), outlet.to_string()))
}

/// A summary that starts `Outlet: <the title>…`: the outlet, and what the
/// summary says beyond the title.
pub fn outlet_prefix(title: &str, summary: &str) -> Option<(String, String)> {
    let (outlet, rest) = summary.split_once(':')?;
    let outlet = outlet.trim();
    let rest = rest.trim();
    if !looks_like_name(outlet) {
        return None;
    }
    let title = title.trim().trim_end_matches('…').trim();
    let head: String = title.chars().take(TITLE_MATCH_CHARS).collect();
    if head.is_empty() || !rest.to_lowercase().starts_with(&head.to_lowercase()) {
        return None;
    }
    let remainder = strip_prefix_ci(rest, title).unwrap_or("").trim();
    let remainder = remainder.trim_start_matches(['—', '-', '·', ':', ',']).trim();
    Some((outlet.to_string(), remainder.to_string()))
}

/// The summary without a leading copy of the title (Google News repeats the
/// headline and the outlet as its description).
pub fn drop_repeated_title(title: &str, summary: &str, source: &str) -> String {
    let Some(rest) = strip_prefix_ci(summary, title) else { return summary.to_string() };
    let rest = rest.trim().trim_start_matches(['—', '-', '·', ':', ',']).trim();
    if rest.is_empty() || rest.eq_ignore_ascii_case(source) {
        String::new()
    } else {
        summary.to_string()
    }
}

fn looks_like_name(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() || text.chars().count() > OUTLET_CHARS || text.contains(['.', '!', '?']) {
        return false;
    }
    words.iter().all(|word| {
        matches!(*word, "of" | "the" | "and" | "de" | "for" | "&") || word.chars().next().is_some_and(|c| c.is_uppercase() || c.is_ascii_digit())
    })
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let mut rest = text;
    for want in prefix.chars() {
        let got = rest.chars().next()?;
        if !got.to_lowercase().eq(want.to_lowercase()) {
            return None;
        }
        rest = &rest[got.len_utf8()..];
    }
    Some(rest)
}

/// The first `<img src>` in a fragment of HTML that is not a tracking pixel.
pub fn first_image(html: &str) -> Option<String> {
    let html = decode_entities(html);
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find("<img") {
        let at = from + pos + 4;
        let tag_end = lower[at..].find('>')?;
        let attrs = &html[at..at + tag_end];
        from = at + tag_end;
        let tiny = ["width", "height"].iter().any(|key| {
            attribute(attrs, key).and_then(|v| v.trim().trim_end_matches("px").trim().parse::<f64>().ok()).is_some_and(|px| px < 50.0)
        });
        if tiny {
            continue;
        }
        if let Some(src) = attribute(attrs, "src").filter(|s| is_http_url(s)) {
            return Some(src);
        }
    }
    None
}

fn attribute(attrs: &str, name: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let key = format!("{name}=");
    let mut from = 0;
    while let Some(pos) = lower[from..].find(&key) {
        let at = from + pos;
        from = at + key.len();
        if at > 0 && !lower[..at].ends_with(char::is_whitespace) {
            continue;
        }
        let rest = &attrs[at + key.len()..];
        let value = match rest.chars().next()? {
            quote @ ('"' | '\'') => {
                let value = &rest[1..];
                &value[..value.find(quote)?]
            }
            _ => rest.split(char::is_whitespace).next().unwrap_or(""),
        };
        return Some(value.to_string());
    }
    None
}

pub fn is_http_url(s: &str) -> bool {
    ["http://", "https://"].iter().any(|scheme| s.get(..scheme.len()).is_some_and(|head| head.eq_ignore_ascii_case(scheme)))
}

/// Images load over https only.
pub fn https_image(url: String) -> String {
    match url.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None => url,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_markup() {
        assert_eq!(clean("<p>A <em>short</em> summary.</p>"), "A short summary.");
        assert_eq!(clean("a &lt; b &amp;&amp; c"), "a < b && c");
        assert_eq!(clean("x<script>alert(1)</script>y"), "x y");
        assert_eq!(clean("caf&#233; &#x2014; ok"), "café — ok");
    }

    #[test]
    fn truncates_at_a_word() {
        let t = truncate("one two three four five six seven", 20);
        assert!(t.ends_with('…') && t.chars().count() <= 21, "{t}");
    }

    #[test]
    fn outlet_conventions() {
        assert_eq!(split_google_source("A - b - Reuters"), Some(("A - b".into(), "Reuters".into())));
        assert_eq!(trailing_outlet("Big news (Jane Doe / Bloomberg)"), Some(("Big news".into(), "Bloomberg".into())));
        assert_eq!(outlet_prefix("Acme launches Widget 2", "Example: Acme launches Widget 2 — a device."), Some(("Example".into(), "a device.".into())));
        assert_eq!(drop_repeated_title("Markets rally", "Markets rally Reuters", "Reuters"), "");
    }
}
