//! Markdown in an agent's replies, as the chat panes draw it ([`super::view`]):
//! the blocks and inline styles a chat answer uses. Anything else is shown
//! as written.
//!
//! | Block | Drawn as |
//! | --- | --- |
//! | `# Heading` to `######` | bold; larger for the first two levels |
//! | `- item`, `* item`, `+ item`, `1. item` | a bullet or the number, with a hanging indent, nested by indentation |
//! | `> quote` | dimmed, with a bar |
//! | fenced code (```` ``` ```` or `~~~`) | each line on a tint, spaces kept, wrapped between characters |
//! | `---`, `***`, `___` | a rule |
//! | `\| a \| b \|` (a table row) | as a code line, so the columns stay aligned; the `\|---\|` row is dropped |
//!
//! Inline: `**bold**` and `__bold__`; `*italic*` and `_italic_` (the markers
//! dropped: there is no italic face); `` `code` `` (on a tint);
//! `[text](url)` (the text, in the accent colour); `~~struck~~` (the
//! markers dropped). A marker with no closing twin later on the line is
//! text, and a backslash keeps the next marker as text.
//!
//! Each source line is a line: a chat answer's single line breaks are
//! meant, so they are kept rather than joined into one paragraph.

/// How an inline run is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub code: bool,
    pub link: bool,
}

/// Styled text: runs, adjacent ones of one style merged.
pub type Runs = Vec<(String, Style)>;

/// One source line of an answer.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Para(Runs),
    Heading(u8, Runs),
    /// A list item: its nesting depth, its marker (a bullet or `3.`).
    Item { depth: usize, marker: String, runs: Runs },
    Quote(Runs),
    /// A line of a fenced block, or a table row: shown as written.
    Code(String),
    Rule,
    Blank,
}

/// The blocks of `text`, one per source line (a fence line makes none).
pub fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    // The fence that opened the code block we are in.
    let mut fence: Option<&'static str> = None;
    for raw in text.split('\n') {
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim_start();
        if let Some(f) = fence {
            if trimmed.starts_with(f) {
                fence = None;
            } else {
                out.push(Block::Code(line.to_string()));
            }
            continue;
        }
        if let Some(f) = ["```", "~~~"].into_iter().find(|f| trimmed.starts_with(*f)) {
            fence = Some(f);
            continue;
        }
        if trimmed.is_empty() {
            out.push(Block::Blank);
        } else if is_rule(trimmed) {
            out.push(Block::Rule);
        } else if let Some((level, rest)) = heading(trimmed) {
            out.push(Block::Heading(level, inline(rest)));
        } else if trimmed.starts_with('|') && trimmed.trim_end().len() > 1 && trimmed.trim_end().ends_with('|') {
            if !is_table_rule(trimmed) {
                out.push(Block::Code(trimmed.trim_end().to_string()));
            }
        } else if let Some(rest) = trimmed.strip_prefix('>') {
            out.push(Block::Quote(inline(rest.strip_prefix(' ').unwrap_or(rest))));
        } else if let Some((marker, rest)) = list_item(trimmed) {
            let indent: usize = line.chars().take_while(|c| c.is_whitespace()).map(|c| if c == '\t' { 4 } else { 1 }).sum();
            out.push(Block::Item { depth: (indent / 2).min(4), marker, runs: inline(rest) });
        } else {
            out.push(Block::Para(inline(trimmed)));
        }
    }
    out
}

fn is_rule(line: &str) -> bool {
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    compact.len() >= 3 && ['-', '*', '_'].into_iter().any(|m| compact.chars().all(|c| c == m))
}

fn is_table_rule(line: &str) -> bool {
    line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &line[hashes..];
    let text = rest.strip_prefix(' ').or_else(|| rest.is_empty().then_some(""))?;
    Some((hashes as u8, text.trim_end().trim_end_matches('#').trim_end()))
}

fn list_item(line: &str) -> Option<(String, &str)> {
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return Some(("\u{2022}".to_string(), rest));
        }
    }
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    // Up to three digits: a line that starts with a year is a sentence.
    if (1..=3).contains(&digits) {
        let rest = &line[digits..];
        for close in [". ", ") "] {
            if let Some(after) = rest.strip_prefix(close) {
                return Some((format!("{}.", &line[..digits]), after));
            }
        }
    }
    None
}

/// The inline runs of one line.
pub fn inline(text: &str) -> Runs {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Runs = Vec::new();
    let mut style = Style::default();
    // The marker that opened bold (`*` or `_`), italic, and whether a strike is open.
    let mut bold: Option<char> = None;
    let mut italic: Option<char> = None;
    let mut struck = false;
    let mut i = 0;
    let push = |out: &mut Runs, text: &str, style: Style| {
        if text.is_empty() {
            return;
        }
        match out.last_mut() {
            Some((last, s)) if *s == style => last.push_str(text),
            _ => out.push((text.to_string(), style)),
        }
    };
    let alnum = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric());
    let space = |c: Option<&char>| c.map_or(true, |c| c.is_whitespace());
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1);
        let prev = if i > 0 { chars.get(i - 1) } else { None };
        // A backslash keeps the next marker as text.
        if c == '\\' && next.is_some_and(|n| "\\`*_[]()~#>|-+.!".contains(*n)) {
            push(&mut out, &next.unwrap().to_string(), style);
            i += 2;
            continue;
        }
        // `code`: nothing inside is a marker.
        if c == '`' {
            if let Some(end) = find(&chars, i + 1, &['`']) {
                let code: String = chars[i + 1..end].iter().collect();
                push(&mut out, &code, Style { code: true, ..style });
                i = end + 1;
                continue;
            }
        }
        // [text](url): the text, as a link.
        if c == '[' {
            if let Some(close) = find(&chars, i + 1, &[']']) {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(end) = find(&chars, close + 2, &[')']) {
                        let label: String = chars[i + 1..close].iter().collect();
                        push(&mut out, &label, Style { link: true, ..style });
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        // **bold** and __bold__.
        if (c == '*' || c == '_') && next == Some(&c) {
            if bold == Some(c) && !space(prev) {
                bold = None;
                style.bold = false;
                i += 2;
                continue;
            }
            if bold.is_none() && !space(chars.get(i + 2)) && (c == '*' || !alnum(prev)) && find(&chars, i + 2, &[c, c]).is_some() {
                bold = Some(c);
                style.bold = true;
                i += 2;
                continue;
            }
        }
        // *italic* and _italic_: the markers go.
        if c == '*' || c == '_' {
            if italic == Some(c) && !space(prev) && (c == '*' || !alnum(next)) {
                italic = None;
                i += 1;
                continue;
            }
            if italic.is_none() && !space(next) && (c == '*' || !alnum(prev)) && find_single(&chars, i + 1, c).is_some() {
                italic = Some(c);
                i += 1;
                continue;
            }
        }
        // ~~struck~~: the markers go.
        if c == '~' && next == Some(&'~') && (struck || find(&chars, i + 2, &['~', '~']).is_some()) {
            struck = !struck;
            i += 2;
            continue;
        }
        push(&mut out, &c.to_string(), style);
        i += 1;
    }
    out
}

/// Where `pattern` next starts at or after `from`.
fn find(chars: &[char], from: usize, pattern: &[char]) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i..].starts_with(pattern))
}

/// Where a single `marker` (not doubled) next closes an italic run after
/// `from`: preceded by a non-space.
fn find_single(chars: &[char], from: usize, marker: char) -> Option<usize> {
    (from..chars.len()).find(|&i| {
        chars[i] == marker
            && chars.get(i + 1) != Some(&marker)
            && (i == 0 || chars[i - 1] != marker)
            && i > from
            && !chars[i - 1].is_whitespace()
            && (marker == '*' || !chars.get(i + 1).is_some_and(|c| c.is_alphanumeric()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(runs: &Runs) -> String {
        runs.iter().map(|(t, _)| t.as_str()).collect()
    }

    const B: Style = Style { bold: true, code: false, link: false };
    const C: Style = Style { bold: false, code: true, link: false };
    const L: Style = Style { bold: false, code: false, link: true };
    const N: Style = Style { bold: false, code: false, link: false };

    #[test]
    fn inline_markers_style_their_text_and_go() {
        assert_eq!(inline("a **bold** b"), vec![("a ".into(), N), ("bold".into(), B), (" b".into(), N)]);
        assert_eq!(inline("use `mail.notify` now"), vec![("use ".into(), N), ("mail.notify".into(), C), (" now".into(), N)]);
        assert_eq!(inline("see [the docs](https://x.example)"), vec![("see ".into(), N), ("the docs".into(), L)]);
        assert_eq!(plain(&inline("an *italic* and _this_ and ~~gone~~")), "an italic and this and gone");
        assert_eq!(inline("**粗体**文字"), vec![("粗体".into(), B), ("文字".into(), N)]);
    }

    #[test]
    fn markers_without_their_twin_and_inside_words_are_text() {
        assert_eq!(plain(&inline("2 * 3 = 6")), "2 * 3 = 6");
        assert_eq!(plain(&inline("a **dangling marker")), "a **dangling marker");
        assert_eq!(plain(&inline("file_name_here.rs")), "file_name_here.rs");
        assert_eq!(plain(&inline(r"a \*literal\* star")), "a *literal* star");
        assert_eq!(plain(&inline("`unclosed code")), "`unclosed code");
    }

    #[test]
    fn a_reply_becomes_its_blocks() {
        let reply = "# Title\nSome **text**.\n\n- one\n  - nested\n2. two\n> quoted\n---\n```rust\nlet x  = 1;\n```\n| a | b |\n|---|---|\n| 1 | 2 |";
        let b = blocks(reply);
        assert_eq!(b[0], Block::Heading(1, vec![("Title".into(), N)]));
        assert_eq!(plain(match &b[1] { Block::Para(r) => r, _ => panic!("{:?}", b[1]) }), "Some text.");
        assert_eq!(b[2], Block::Blank);
        assert!(matches!(&b[3], Block::Item { depth: 0, marker, .. } if marker == "\u{2022}"));
        assert!(matches!(&b[4], Block::Item { depth: 1, .. }));
        assert!(matches!(&b[5], Block::Item { marker, .. } if marker == "2."));
        assert_eq!(b[6], Block::Quote(vec![("quoted".into(), N)]));
        assert_eq!(b[7], Block::Rule);
        assert_eq!(b[8], Block::Code("let x  = 1;".into()), "a code line keeps its spaces");
        assert_eq!(b[9], Block::Code("| a | b |".into()));
        assert_eq!(b[10], Block::Code("| 1 | 2 |".into()), "the table's rule row is dropped");
        assert_eq!(b.len(), 11);
    }

    #[test]
    fn hashes_and_bullets_need_their_space() {
        assert_eq!(blocks("#hashtag"), vec![Block::Para(vec![("#hashtag".into(), N)])]);
        assert_eq!(blocks("*emphasis*"), vec![Block::Para(vec![("emphasis".into(), N)])]);
        assert_eq!(blocks("2024. was a year"), vec![Block::Para(vec![("2024. was a year".into(), N)])], "a year is not a list");
    }
}
