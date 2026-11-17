//! A text draft as lines a document editor can show: what a line's style is,
//! where it is bold or italic, and how to write it back.
//!
//! The draft stays plain text, one paragraph a line: a line starting with `#` is
//! a heading, `-` a bullet, `1.` a numbered item, `>` a quote; `**bold**` and
//! `*italic*` work inside a line. So a line nobody edits goes back byte for byte,
//! and one somebody did edit is written so that reading it gives what was on the
//! page. Tables and code fences stay as typed.
use article_core::document::{Block, BlockKind, Mark};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Body,
    H1,
    H2,
    H3,
    Bullet,
    Number,
    Quote,
    /// A table row or a code fence's line: the line is the text.
    Code,
}

impl Style {
    pub fn id(self) -> &'static str {
        match self {
            Style::Body => "body",
            Style::H1 => "h1",
            Style::H2 => "h2",
            Style::H3 => "h3",
            Style::Bullet => "bullet",
            Style::Number => "number",
            Style::Quote => "quote",
            Style::Code => "code",
        }
    }

    pub fn from_id(id: &str) -> Option<Style> {
        Some(match id {
            "body" => Style::Body,
            "h1" => Style::H1,
            "h2" => Style::H2,
            "h3" => Style::H3,
            "bullet" => Style::Bullet,
            "number" => Style::Number,
            "quote" => Style::Quote,
            "code" => Style::Code,
            _ => return None,
        })
    }

    /// The marker a line gets when it's given this style.
    pub fn marker(self) -> &'static str {
        match self {
            Style::Body | Style::Code => "",
            Style::H1 => "# ",
            Style::H2 => "## ",
            Style::H3 => "### ",
            Style::Bullet => "- ",
            Style::Number => "1. ",
            Style::Quote => "> ",
        }
    }

    pub fn is_list(self) -> bool {
        matches!(self, Style::Bullet | Style::Number)
    }

    pub fn is_heading(self) -> bool {
        matches!(self, Style::H1 | Style::H2 | Style::H3)
    }
}

/// One line of a draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub style: Style,
    /// The line's marker as written, its indent included.
    pub marker: String,
    /// What the page shows.
    pub text: String,
    pub marks: Vec<Mark>,
    /// The line as it is in the draft.
    pub source: String,
}

fn blank(byte: Option<&u8>) -> bool {
    matches!(byte, Some(b' ') | Some(b'\t'))
}

/// A line's style and how long its marker is. A body line has no marker: what
/// leads it, spaces too, is text.
pub fn marker_of(line: &str) -> (Style, usize) {
    let bytes = line.as_bytes();
    let indent = bytes.iter().take_while(|b| **b == b' ' || **b == b'\t').count();
    let rest = &bytes[indent..];
    let with_blanks = |at: usize| at + bytes[at..].iter().take_while(|b| **b == b' ' || **b == b'\t').count();
    let hashes = rest.iter().take_while(|b| **b == b'#').count();
    if (1..=6).contains(&hashes) && blank(rest.get(hashes)) {
        let style = match hashes {
            1 => Style::H1,
            2 => Style::H2,
            _ => Style::H3,
        };
        return (style, with_blanks(indent + hashes));
    }
    if matches!(rest.first(), Some(b'-') | Some(b'*') | Some(b'+')) && blank(rest.get(1)) {
        return (Style::Bullet, with_blanks(indent + 1));
    }
    let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
    if (1..=9).contains(&digits) && matches!(rest.get(digits), Some(b'.') | Some(b')')) && blank(rest.get(digits + 1)) {
        return (Style::Number, with_blanks(indent + digits + 1));
    }
    if rest.first() == Some(&b'>') {
        return (Style::Quote, with_blanks(indent + 1));
    }
    (Style::Body, 0)
}

/// Three or more of one of `-`, `*`, `_`, and nothing else but spaces.
pub fn is_rule(line: &str) -> bool {
    let mut kind = None;
    let mut count = 0;
    for c in line.chars() {
        if c == ' ' || c == '\t' {
            continue;
        }
        if !matches!(c, '-' | '*' | '_') || kind.is_some_and(|k| k != c) {
            return false;
        }
        kind = Some(c);
        count += 1;
    }
    count >= 3
}

pub fn is_table_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// The fence a line opens: its character and how long it is.
pub fn fence_of(line: &str) -> Option<(char, usize)> {
    let t = line.trim_start();
    let c = t.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = t.chars().take_while(|x| *x == c).count();
    (n >= 3).then_some((c, n))
}

fn closes_fence(line: &str, c: char, n: usize) -> bool {
    let t = line.trim();
    t.chars().count() >= n && t.chars().all(|x| x == c)
}

fn literal(line: &str, style: Style) -> Line {
    let bare = line.strip_suffix('\r').unwrap_or(line);
    Line { style, marker: String::new(), text: bare.to_string(), marks: Vec::new(), source: line.to_string() }
}

/// One line, outside any fence.
pub fn parse_line(line: &str) -> Line {
    let bare = line.strip_suffix('\r').unwrap_or(line);
    if is_table_row(bare) || fence_of(bare).is_some() {
        return literal(line, Style::Code);
    }
    if is_rule(bare) {
        return literal(line, Style::Body);
    }
    let (style, len) = marker_of(bare);
    let (marker, rest) = bare.split_at(len);
    let (text, marks) = parse_inline(rest);
    Line { style, marker: marker.to_string(), text, marks, source: line.to_string() }
}

/// A whole draft: always at least one line, and its sources joined by newlines
/// are the draft again.
pub fn parse_draft(text: &str) -> Vec<Line> {
    let mut fence: Option<(char, usize)> = None;
    text.split('\n')
        .map(|line| {
            let bare = line.strip_suffix('\r').unwrap_or(line);
            if let Some((c, n)) = fence {
                if closes_fence(bare, c, n) {
                    fence = None;
                }
                return literal(line, Style::Code);
            }
            if let Some(open) = fence_of(bare) {
                fence = Some(open);
                return literal(line, Style::Code);
            }
            parse_line(line)
        })
        .collect()
}

struct Delim {
    ch: char,
    count: usize,
    orig: usize,
    open: bool,
    close: bool,
    active: bool,
}

struct Node {
    text: String,
    strong: bool,
    em: bool,
    delim: Option<Delim>,
}

fn is_punct(c: char) -> bool {
    !c.is_alphanumeric() && !c.is_whitespace()
}

/// The emphasis in a line's text, as CommonMark reads it; a backslash before
/// punctuation hides itself, and a code span stays as typed, its backticks too.
/// Everything else stays literal.
pub fn parse_inline(src: &str) -> (String, Vec<Mark>) {
    let chars: Vec<char> = src.chars().collect();
    let mut nodes: Vec<Node> = Vec::new();
    let mut run = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && chars.get(i + 1).is_some_and(|n| n.is_ascii_punctuation()) {
            run.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == '`' {
            let n = chars[i..].iter().take_while(|x| **x == '`').count();
            let mut j = i + n;
            let mut closing = None;
            while j < chars.len() {
                if chars[j] == '`' {
                    let m = chars[j..].iter().take_while(|x| **x == '`').count();
                    if m == n {
                        closing = Some(j);
                        break;
                    }
                    j += m;
                } else {
                    j += 1;
                }
            }
            let end = closing.map_or(i + n, |j| j + n);
            run.extend(&chars[i..end]);
            i = end;
            continue;
        }
        if c == '*' || c == '_' {
            let n = chars[i..].iter().take_while(|x| **x == c).count();
            let before = if i == 0 { ' ' } else { chars[i - 1] };
            let after = chars.get(i + n).copied().unwrap_or(' ');
            let left = !after.is_whitespace() && (!is_punct(after) || before.is_whitespace() || is_punct(before));
            let right = !before.is_whitespace() && (!is_punct(before) || after.is_whitespace() || is_punct(after));
            let (open, close) = if c == '*' {
                (left, right)
            } else {
                (left && (!right || is_punct(before)), right && (!left || is_punct(after)))
            };
            if !run.is_empty() {
                nodes.push(Node { text: std::mem::take(&mut run), strong: false, em: false, delim: None });
            }
            nodes.push(Node {
                text: String::new(),
                strong: false,
                em: false,
                delim: Some(Delim { ch: c, count: n, orig: n, open, close, active: true }),
            });
            i += n;
            continue;
        }
        run.push(c);
        i += 1;
    }
    if !run.is_empty() {
        nodes.push(Node { text: run, strong: false, em: false, delim: None });
    }
    let mut c = 0;
    while c < nodes.len() {
        let (ch, closer_orig, closer_open) = match &nodes[c].delim {
            Some(d) if d.active && d.close && d.count > 0 => (d.ch, d.orig, d.open),
            _ => {
                c += 1;
                continue;
            }
        };
        let mut found = None;
        let mut o = c;
        while o > 0 {
            o -= 1;
            let Some(d) = &nodes[o].delim else { continue };
            if !d.active || !d.open || d.ch != ch || d.count == 0 {
                continue;
            }
            let odd = (d.close || closer_open)
                && (d.orig + closer_orig) % 3 == 0
                && !(d.orig % 3 == 0 && closer_orig % 3 == 0);
            if !odd {
                found = Some(o);
                break;
            }
        }
        let Some(o) = found else {
            if !closer_open {
                nodes[c].delim.as_mut().unwrap().active = false;
            }
            c += 1;
            continue;
        };
        let opener_count = nodes[o].delim.as_ref().unwrap().count;
        let closer_count = nodes[c].delim.as_ref().unwrap().count;
        let used = if opener_count >= 2 && closer_count >= 2 { 2 } else { 1 };
        for node in &mut nodes[o + 1..c] {
            if used == 2 {
                node.strong = true;
            } else {
                node.em = true;
            }
            if let Some(d) = &mut node.delim {
                d.active = false;
            }
        }
        let opener = nodes[o].delim.as_mut().unwrap();
        opener.count -= used;
        if opener.count == 0 {
            opener.active = false;
        }
        let closer = nodes[c].delim.as_mut().unwrap();
        closer.count -= used;
        if closer.count == 0 {
            closer.active = false;
            c += 1;
        }
    }
    let mut text = String::new();
    let mut marks = Vec::new();
    for node in &nodes {
        let start = text.len();
        match &node.delim {
            Some(d) => text.extend(std::iter::repeat(d.ch).take(d.count)),
            None => text.push_str(&node.text),
        }
        if text.len() > start && (node.strong || node.em) {
            push_mark(&mut marks, start, text.len(), node.strong, node.em);
        }
    }
    (text, marks)
}

fn push_mark(marks: &mut Vec<Mark>, start: usize, end: usize, bold: bool, italic: bool) {
    if let Some(last) = marks.last_mut() {
        if last.end == start && last.bold == bold && last.italic == italic && last.link.is_none() {
            last.end = end;
            return;
        }
    }
    marks.push(Mark { start, end, bold, italic, link: None });
}

/// Whether `pos` is bold and italic under `marks`.
pub fn flags_at(marks: &[Mark], pos: usize) -> (bool, bool) {
    let mut flags = (false, false);
    for m in marks {
        if pos >= m.start && pos < m.end {
            flags.0 |= m.bold;
            flags.1 |= m.italic;
        }
    }
    flags
}

/// The same ink on the page: a space being bold or not shows nothing.
pub fn same_ink(text: &str, a: &[Mark], b: &[Mark]) -> bool {
    text.char_indices().all(|(i, c)| c.is_whitespace() || flags_at(a, i) == flags_at(b, i))
}

/// The part of `marks` over `range`, counted from the range's start.
pub fn slice_marks(marks: &[Mark], range: std::ops::Range<usize>) -> Vec<Mark> {
    marks
        .iter()
        .filter_map(|m| {
            let start = m.start.max(range.start);
            let end = m.end.min(range.end);
            (start < end).then(|| Mark { start: start - range.start, end: end - range.start, ..m.clone() })
        })
        .collect()
}

/// `b`'s marks moved along by `by`, after `a`'s.
pub fn join_marks(a: &[Mark], b: &[Mark], by: usize) -> Vec<Mark> {
    let mut out = a.to_vec();
    for m in b {
        let (start, end) = (m.start + by, m.end + by);
        if let Some(last) = out.last_mut() {
            if last.end == start && last.bold == m.bold && last.italic == m.italic && last.link == m.link {
                last.end = end;
                continue;
            }
        }
        out.push(Mark { start, end, ..m.clone() });
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flag {
    Bold,
    Italic,
}

/// Runs of one emphasis. A mark can't start or end on a space, so spaces at a
/// mark's edge go outside it.
fn runs_of(text: &str, marks: &[Mark]) -> Vec<(String, bool, bool)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut flags: Vec<(bool, bool)> = chars.iter().map(|(i, _)| flags_at(marks, *i)).collect();
    for which in [0, 1] {
        let get = |f: &(bool, bool)| if which == 0 { f.0 } else { f.1 };
        let mut last = false;
        let mut lefts = vec![false; chars.len()];
        for k in 0..chars.len() {
            if !chars[k].1.is_whitespace() {
                last = get(&flags[k]);
            }
            lefts[k] = last;
        }
        let mut next = false;
        for k in (0..chars.len()).rev() {
            if !chars[k].1.is_whitespace() {
                next = get(&flags[k]);
            } else {
                let on = lefts[k] && next;
                if which == 0 {
                    flags[k].0 = on;
                } else {
                    flags[k].1 = on;
                }
            }
        }
    }
    let mut runs: Vec<(String, bool, bool)> = Vec::new();
    for (k, (_, c)) in chars.iter().enumerate() {
        match runs.last_mut() {
            Some(run) if run.1 == flags[k].0 && run.2 == flags[k].1 => run.0.push(*c),
            _ => runs.push((c.to_string(), flags[k].0, flags[k].1)),
        }
    }
    runs
}

/// Backslashes for what would be read as something else. Level 0 adds none;
/// level 1 escapes emphasis; level 2 code spans too.
fn escaped(text: &str, level: u8) -> String {
    if level == 0 {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 4);
    for (k, c) in chars.iter().enumerate() {
        let hides = *c == '\\' && chars.get(k + 1).map_or(true, |n| n.is_ascii_punctuation() || *n == '\\');
        if *c == '*' || *c == '_' || hides || (level >= 2 && *c == '`') {
            out.push('\\');
        }
        out.push(*c);
    }
    out
}

fn delimited(runs: &[(String, bool, bool)], bold: &str, italic: &str, level: u8) -> String {
    let mut out = String::new();
    let mut stack: Vec<Flag> = Vec::new();
    let delim = |f: Flag| if f == Flag::Bold { bold } else { italic };
    for (text, b, i) in runs {
        let wanted = |f: Flag| if f == Flag::Bold { *b } else { *i };
        let mut reopen = Vec::new();
        while stack.iter().any(|f| !wanted(*f)) {
            let top = stack.pop().unwrap();
            out.push_str(delim(top));
            if wanted(top) {
                reopen.push(top);
            }
        }
        for f in reopen.into_iter().rev() {
            stack.push(f);
            out.push_str(delim(f));
        }
        if *b && !stack.contains(&Flag::Bold) {
            stack.push(Flag::Bold);
            out.push_str(bold);
        }
        if *i && !stack.contains(&Flag::Italic) {
            stack.push(Flag::Italic);
            out.push_str(italic);
        }
        out.push_str(&escaped(text, level));
    }
    while let Some(top) = stack.pop() {
        out.push_str(delim(top));
    }
    out
}

fn reads_as(candidate: &str, text: &str, marks: &[Mark]) -> bool {
    let (read, read_marks) = parse_inline(candidate);
    read == text && same_ink(text, &read_marks, marks)
}

/// A line's text with its emphasis, so that `parse_inline` reads it back. What
/// Markdown can't say is simplified: a mark that runs up against another loses
/// out, and at worst the text is kept and the marks are not.
pub fn emit_inline(text: &str, marks: &[Mark]) -> String {
    let attempt = |runs: &[(String, bool, bool)], want: &[Mark]| -> Option<String> {
        for level in 0..3u8 {
            for (bold, italic) in [("**", "*"), ("__", "_"), ("**", "_"), ("__", "*")] {
                let candidate = delimited(runs, bold, italic, level);
                if reads_as(&candidate, text, want) {
                    return Some(candidate);
                }
            }
        }
        None
    };
    let runs = runs_of(text, marks);
    if let Some(done) = attempt(&runs, marks) {
        return done;
    }
    let mut simple = runs.clone();
    for k in 1..simple.len() {
        let (b, i) = (simple[k - 1].1, simple[k - 1].2);
        if (b && !simple[k].1 && simple[k].2) || (i && !simple[k].2 && simple[k].1) {
            simple[k - 1].1 = false;
            simple[k - 1].2 = false;
        }
    }
    let mut at = 0;
    let mut want = Vec::new();
    for (t, b, i) in &simple {
        if *b || *i {
            push_mark(&mut want, at, at + t.len(), *b, *i);
        }
        at += t.len();
    }
    if let Some(done) = attempt(&simple, &want) {
        return done;
    }
    escaped(text, 2)
}

/// A body line can't start with what would make it something else.
fn guard_body(inline: &str, text: &str) -> String {
    if is_rule(text) {
        return inline.to_string();
    }
    let reads_as_body = |s: &str| {
        let line = parse_line(s);
        line.style == Style::Body && line.text == text
    };
    if reads_as_body(inline) {
        return inline.to_string();
    }
    let lead = inline.len() - inline.trim_start_matches([' ', '\t']).len();
    let rest = &inline[lead..];
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let at = lead + digits;
    let guarded = format!("{}\\{}", &inline[..at], &inline[at..]);
    if reads_as_body(&guarded) {
        guarded
    } else {
        inline.to_string()
    }
}

/// A line as the draft keeps it.
pub fn emit_line(style: Style, marker: &str, text: &str, marks: &[Mark]) -> String {
    if style == Style::Code {
        return text.to_string();
    }
    let inline = emit_inline(text, marks);
    if style == Style::Body {
        return guard_body(&inline, text);
    }
    format!("{marker}{inline}")
}

/// What typing just made of a line: the marker that makes it a list, a heading or
/// a quote, once the caret is right after that marker.
pub fn typed_marker(text: &str, caret: usize) -> Option<(Style, usize)> {
    if is_rule(text) {
        return None;
    }
    let (style, len) = marker_of(text);
    let ended = text[..len].ends_with([' ', '\t']);
    (style != Style::Body && ended && caret == len).then_some((style, len))
}

/// A line after `live_emphasis`.
#[derive(Debug, PartialEq, Eq)]
pub struct LiveEmphasis {
    pub text: String,
    pub marks: Vec<Mark>,
    pub caret: usize,
}

fn run_before(text: &str, end: usize, ch: char) -> usize {
    text[..end].chars().rev().take_while(|c| *c == ch).count()
}

/// Typing the closing `**` of `**word**` makes the word bold and takes the stars
/// away. `2*3*4` stays as typed: an opening mark starts a word.
pub fn live_emphasis(text: &str, marks: &[Mark], caret: usize) -> Option<LiveEmphasis> {
    let ch = text[..caret].chars().next_back().filter(|c| *c == '*' || *c == '_')?;
    let n = run_before(text, caret, ch);
    if !(1..=3).contains(&n) {
        return None;
    }
    if text[caret..].chars().next().is_some_and(char::is_alphanumeric) {
        return None;
    }
    let closer = caret - n;
    if closer == 0 {
        return None;
    }
    // The nearest run of this mark before the closing one.
    let before = &text[..closer];
    let at = before.rfind(ch)?;
    let opener_end = at + ch.len_utf8();
    if run_before(text, opener_end, ch) != n {
        return None;
    }
    let opener = opener_end - n;
    let inner = &text[opener_end..closer];
    if inner.is_empty() || inner.starts_with(char::is_whitespace) || inner.ends_with(char::is_whitespace) {
        return None;
    }
    if text[..opener].chars().next_back().is_some_and(char::is_alphanumeric) {
        return None;
    }
    if text[..opener].matches('`').count() % 2 == 1 {
        return None;
    }
    let mut block = Block::new(BlockKind::Paragraph, text);
    block.marks = marks.to_vec();
    block.replace_text(format!("{}{}", &text[..closer], &text[caret..]));
    block.replace_text(format!("{}{}{}", &text[..opener], inner, &text[caret..]));
    let range = opener..opener + inner.len();
    let bold = (n >= 2).then_some(true);
    let italic = (n != 2).then_some(true);
    block.format(range.clone(), bold, italic, None).ok()?;
    Some(LiveEmphasis { text: block.text, marks: block.marks, caret: range.end })
}

/// A list line's depth: Writer's documents have two levels.
pub fn list_level(marker: &str) -> usize {
    let indent = marker.bytes().take_while(|b| *b == b' ' || *b == b'\t').count();
    usize::from(indent >= 2 || marker.starts_with('\t'))
}

/// A numbered marker's number and what follows the digits.
pub fn number_of(marker: &str) -> Option<(u64, String)> {
    let t = marker.trim_start();
    let digits = t.bytes().take_while(u8::is_ascii_digit).count();
    let n = t[..digits].parse().ok()?;
    Some((n, t[digits..].to_string()))
}

/// JSON's way to write a string.
pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The word at a byte of a line: a right click's. None off a word, and at the line's end,
/// where a click may be anywhere past it.
pub fn word_around(text: &str, byte: usize) -> Option<std::ops::Range<usize>> {
    if byte >= text.len() || !text.is_char_boundary(byte) {
        return None;
    }
    let in_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '\u{2019}';
    let (before, after) = text.split_at(byte);
    let start = before.char_indices().rev().take_while(|(_, c)| in_word(*c)).last().map_or(byte, |(at, _)| at);
    let end = byte + after.char_indices().take_while(|(_, c)| in_word(*c)).last().map_or(0, |(at, c)| at + c.len_utf8());
    (start < end).then_some(start..end)
}

/// One place Find found: the block, and the bytes the words take in it.
pub type Found = (usize, usize, usize);

/// Every place `words` are in the blocks, case aside, one after another and never
/// overlapping, as a Mac's Find counts them. A match stays inside one block.
pub fn find_all<'a>(blocks: impl IntoIterator<Item = &'a str>, words: &str) -> Vec<Found> {
    let wanted: String = words.chars().flat_map(char::to_lowercase).collect();
    let mut found = Vec::new();
    if wanted.is_empty() {
        return found;
    }
    for (index, text) in blocks.into_iter().enumerate() {
        // The text lowered, and for each of its bytes the byte its character starts at.
        let mut lowered = String::with_capacity(text.len());
        let mut from = Vec::with_capacity(text.len());
        for (at, c) in text.char_indices() {
            for low in c.to_lowercase() {
                lowered.push(low);
                from.resize(lowered.len(), at);
            }
        }
        let mut start = 0;
        while let Some(hit) = lowered[start..].find(&wanted) {
            let (begin, end) = (start + hit, start + hit + wanted.len());
            let last = from[end - 1];
            found.push((index, from[begin], last + text[last..].chars().next().map_or(0, char::len_utf8)));
            start = end;
        }
    }
    found
}

/// Which of `found` Find chooses from the selection `start..end` (block and byte):
/// `step` 0 the first at or after its start, so a match stays while its words are
/// typed; 1 the first at or after its end; -1 the last before its start. Past either
/// end it goes round to the other, as a Mac's Find does.
pub fn pick_found(found: &[Found], start: (usize, usize), end: (usize, usize), step: i32) -> Option<usize> {
    if found.is_empty() {
        return None;
    }
    let at = |f: &Found| (f.0, f.1);
    let chosen = match step {
        0 => found.iter().position(|f| at(f) >= start),
        s if s > 0 => found.iter().position(|f| at(f) >= end),
        _ => found.iter().rposition(|f| at(f) < start),
    };
    Some(chosen.unwrap_or(if step < 0 { found.len() - 1 } else { 0 }))
}

/// What a Mac's Transformations do to letters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Upper,
    Lower,
    /// A capital to each word and the rest small, a word running to a space, as a Mac's
    /// Capitalize has it.
    Capitalize,
}

impl Case {
    pub fn from_id(id: &str) -> Option<Case> {
        match id {
            "upper" => Some(Case::Upper),
            "lower" => Some(Case::Lower),
            "capitalize" => Some(Case::Capitalize),
            _ => None,
        }
    }
}

/// `text` with the letters of `range` in `case`, and where its characters went: `(old, new)`
/// byte offsets for each character of the range and for its end, since a letter can change
/// length (\u{130} lowered is two characters). `word_start` says whether the range's first
/// character starts a word.
pub fn change_case(text: &str, range: std::ops::Range<usize>, case: Case, word_start: bool) -> (String, Vec<(usize, usize)>) {
    let mut out = String::with_capacity(text.len() + 8);
    out.push_str(&text[..range.start]);
    let mut moved = Vec::new();
    let mut starts = word_start;
    for (at, c) in text[range.clone()].char_indices() {
        moved.push((range.start + at, out.len()));
        let capital = match case {
            Case::Upper => true,
            Case::Lower => false,
            Case::Capitalize => starts,
        };
        if capital {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
        starts = c.is_whitespace();
    }
    moved.push((range.end, out.len()));
    out.push_str(&text[range.end..]);
    (out, moved)
}

/// Where byte `at` of a line is after `change_case`, from the moves it said.
pub fn moved_to(moved: &[(usize, usize)], at: usize) -> usize {
    let (Some(first), Some(last)) = (moved.first(), moved.last()) else { return at };
    if at <= first.0 {
        return at;
    }
    if at >= last.0 {
        return at - last.0 + last.1;
    }
    moved.iter().take_while(|(old, _)| *old <= at).last().map_or(at, |(_, new)| *new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_counts_each_place_once_case_aside_and_goes_round() {
        let blocks = ["The cat sat.", "", "CAT and Cat"];
        let found = find_all(blocks, "cat");
        assert_eq!(found, [(0, 4, 7), (2, 0, 3), (2, 8, 11)]);
        assert!(find_all(blocks, "").is_empty());
        assert!(find_all(blocks, "dog").is_empty());
        assert_eq!(find_all(["aaaa"], "aa"), [(0, 0, 2), (0, 2, 4)], "never overlapping");
        // The bytes are the text's own, whatever lowering does to it.
        assert_eq!(find_all(["\u{dc}n\u{ef}code \u{fc}n\u{ef}"], "\u{dc}N\u{cf}"), [(0, 0, 5), (0, 10, 15)]);
        assert_eq!(find_all(["\u{130}stanbul"], "i\u{307}s"), [(0, 0, 3)], "a letter that lowers to two");
        // Typing keeps the match at the selection's start; next and back move on and go round.
        assert_eq!(pick_found(&found, (2, 0), (2, 0), 0), Some(1));
        assert_eq!(pick_found(&found, (2, 0), (2, 3), 0), Some(1));
        assert_eq!(pick_found(&found, (2, 0), (2, 3), 1), Some(2));
        assert_eq!(pick_found(&found, (2, 8), (2, 11), 1), Some(0), "past the last, the first");
        assert_eq!(pick_found(&found, (0, 4), (0, 7), -1), Some(2), "before the first, the last");
        assert_eq!(pick_found(&found, (2, 8), (2, 11), -1), Some(1));
        assert_eq!(pick_found(&found, (1, 0), (1, 0), 1), Some(1), "from a caret, the next after it");
        assert_eq!(pick_found(&[], (0, 0), (0, 0), 1), None);
    }

    fn bold(start: usize, end: usize) -> Mark {
        Mark { start, end, bold: true, italic: false, link: None }
    }
    fn italic(start: usize, end: usize) -> Mark {
        Mark { start, end, bold: false, italic: true, link: None }
    }
    fn both(start: usize, end: usize) -> Mark {
        Mark { start, end, bold: true, italic: true, link: None }
    }

    #[test]
    fn markers_say_a_lines_style() {
        for (line, style, text) in [
            ("# One", Style::H1, "One"),
            ("## Two", Style::H2, "Two"),
            ("### Three", Style::H3, "Three"),
            ("###### Six", Style::H3, "Six"),
            ("- Item", Style::Bullet, "Item"),
            ("* Item", Style::Bullet, "Item"),
            ("+ Item", Style::Bullet, "Item"),
            ("   - Nested", Style::Bullet, "Nested"),
            ("1. First", Style::Number, "First"),
            ("12) Twelfth", Style::Number, "Twelfth"),
            ("> Quoted", Style::Quote, "Quoted"),
            (">Quoted", Style::Quote, "Quoted"),
            ("Plain", Style::Body, "Plain"),
            ("  Two spaces", Style::Body, "  Two spaces"),
            ("#hashtag", Style::Body, "#hashtag"),
            ("-dash", Style::Body, "-dash"),
            ("1.5 metres", Style::Body, "1.5 metres"),
            ("", Style::Body, ""),
            ("---", Style::Body, "---"),
            ("* * *", Style::Body, "* * *"),
            ("| a | b |", Style::Code, "| a | b |"),
            ("```rust", Style::Code, "```rust"),
        ] {
            let parsed = parse_line(line);
            assert_eq!((parsed.style, parsed.text.as_str()), (style, text), "{line}");
        }
    }

    #[test]
    fn a_draft_nobody_edits_comes_back_byte_for_byte() {
        let draft = "# Title\r\n\n  - odd   spacing\n1) x\n* y\n**b** and _i_\n```\n# not a heading\n- nor a list\n```\n| a |\n|---|\n\n\\*literal\\*\ntrailing  ";
        let joined = parse_draft(draft).iter().map(|l| l.source.clone()).collect::<Vec<_>>().join("\n");
        assert_eq!(joined, draft);
        assert_eq!(parse_draft("").len(), 1);
    }

    #[test]
    fn a_fence_keeps_its_lines_as_typed() {
        let lines = parse_draft("Before\n```\n# not a heading\n**not bold**\n```\n# Heading");
        assert_eq!(lines[0].style, Style::Body);
        assert!(lines[1..5].iter().all(|l| l.style == Style::Code && l.marks.is_empty()));
        assert_eq!(lines[2].text, "# not a heading");
        assert_eq!(lines[3].text, "**not bold**");
        assert_eq!(lines[5].style, Style::H1);
    }

    #[test]
    fn emphasis_reads_as_commonmark_does() {
        assert_eq!(parse_inline("**a**"), ("a".into(), vec![bold(0, 1)]));
        assert_eq!(parse_inline("*a*"), ("a".into(), vec![italic(0, 1)]));
        assert_eq!(parse_inline("_a_"), ("a".into(), vec![italic(0, 1)]));
        assert_eq!(parse_inline("__a__"), ("a".into(), vec![bold(0, 1)]));
        assert_eq!(parse_inline("***a***"), ("a".into(), vec![both(0, 1)]));
        assert_eq!(
            parse_inline("**a *b* c**"),
            ("a b c".into(), vec![bold(0, 2), both(2, 3), bold(3, 5)])
        );
        assert_eq!(parse_inline("snake_case_name"), ("snake_case_name".into(), vec![]));
        assert_eq!(parse_inline("2 * 3 * 4"), ("2 * 3 * 4".into(), vec![]));
        assert_eq!(parse_inline("2*3*4"), ("234".into(), vec![italic(1, 2)]));
        assert_eq!(parse_inline("\\*not\\*"), ("*not*".into(), vec![]));
        assert_eq!(parse_inline("a\\b"), ("a\\b".into(), vec![]));
        assert_eq!(parse_inline("`*code*` *x*"), ("`*code*` x".into(), vec![italic(9, 10)]));
        assert_eq!(parse_inline("**unclosed"), ("**unclosed".into(), vec![]));
        assert_eq!(parse_inline("**a*"), ("*a".into(), vec![italic(1, 2)]));
        assert_eq!(parse_inline("foo**bar**baz"), ("foobarbaz".into(), vec![bold(3, 6)]));
        assert_eq!(parse_inline("中文**粗体**中文"), ("中文粗体中文".into(), vec![bold(6, 12)]));
    }

    #[test]
    fn what_is_on_the_page_is_what_is_written_back() {
        for (text, marks, want) in [
            ("plain", vec![], "plain"),
            ("bold", vec![bold(0, 4)], "**bold**"),
            ("it", vec![italic(0, 2)], "*it*"),
            ("xy", vec![both(0, 2)], "***xy***"),
            ("foobar", vec![bold(0, 3)], "**foo**bar"),
            ("a b c", vec![bold(0, 2), both(2, 3), bold(3, 5)], "**a *b* c**"),
            ("bold ", vec![bold(0, 5)], "**bold** "),
            (" bold", vec![bold(0, 5)], " **bold**"),
            ("2*3*4", vec![], "2\\*3\\*4"),
            ("snake_case_name", vec![], "snake_case_name"),
            ("C:\\*", vec![], "C:\\\\\\*"),
        ] {
            let written = emit_inline(text, &marks);
            assert_eq!(written, want, "{text}");
            let (read, read_marks) = parse_inline(&written);
            assert_eq!(read, text);
            assert!(same_ink(text, &read_marks, &marks), "{text}: {read_marks:?}");
        }
    }

    #[test]
    fn marks_that_cross_stay_on_the_page_or_go_cleanly() {
        // Italic ending where bold goes on: no stars are left in the text.
        let marks = vec![italic(0, 1), both(1, 2), bold(2, 3)];
        let written = emit_inline("abc", &marks);
        let (read, read_marks) = parse_inline(&written);
        assert_eq!(read, "abc", "{written}");
        assert!(same_ink("abc", &read_marks, &marks), "{written}: {read_marks:?}");
        // Marks Markdown can't say never turn into literal stars.
        let written = emit_inline("\"a\"b", &[bold(0, 3)]);
        assert_eq!(parse_inline(&written).0, "\"a\"b", "{written}");
    }

    #[test]
    fn a_body_line_that_looks_like_something_else_says_it_isnt() {
        for text in ["# not a heading", "- not a bullet", "1. not a number", "> not a quote", "| not a row", "```"] {
            let written = emit_line(Style::Body, "", text, &[]);
            let read = parse_line(&written);
            assert_eq!((read.style, read.text.as_str()), (Style::Body, text), "{written}");
        }
        assert_eq!(emit_line(Style::Body, "", "---", &[]), "---");
        assert_eq!(emit_line(Style::H2, "## ", "Next", &[bold(0, 4)]), "## **Next**");
        assert_eq!(emit_line(Style::Bullet, "   - ", "x", &[]), "   - x");
        assert_eq!(emit_line(Style::Code, "", "| *a* |", &[]), "| *a* |");
    }

    #[test]
    fn typing_a_marker_makes_the_style() {
        assert_eq!(typed_marker("# ", 2), Some((Style::H1, 2)));
        assert_eq!(typed_marker("- ", 2), Some((Style::Bullet, 2)));
        assert_eq!(typed_marker("1. ", 3), Some((Style::Number, 3)));
        assert_eq!(typed_marker("> ", 2), Some((Style::Quote, 2)));
        assert_eq!(typed_marker(">", 1), None);
        assert_eq!(typed_marker("#", 1), None);
        // Text that came to look like a marker some other way stays text.
        assert_eq!(typed_marker("- x", 3), None);
        assert_eq!(typed_marker("- x", 0), None);
    }

    #[test]
    fn typing_the_closing_mark_makes_the_emphasis() {
        let live = live_emphasis("a **bold**", &[], 10).unwrap();
        assert_eq!(live, LiveEmphasis { text: "a bold".into(), marks: vec![bold(2, 6)], caret: 6 });
        let live = live_emphasis("*it*", &[], 4).unwrap();
        assert_eq!(live, LiveEmphasis { text: "it".into(), marks: vec![italic(0, 2)], caret: 2 });
        let live = live_emphasis("_it_", &[], 4).unwrap();
        assert_eq!(live.text, "it");
        let live = live_emphasis("***xy***", &[], 8).unwrap();
        assert_eq!(live, LiveEmphasis { text: "xy".into(), marks: vec![both(0, 2)], caret: 2 });
        let live = live_emphasis("**中文** after", &[], 10).unwrap();
        assert_eq!(live, LiveEmphasis { text: "中文 after".into(), marks: vec![bold(0, 6)], caret: 6 });
        // Not yet, and not ever.
        assert_eq!(live_emphasis("**bold*", &[], 7), None);
        assert_eq!(live_emphasis("2*3*", &[], 4), None);
        assert_eq!(live_emphasis("snake_case_", &[], 11), None);
        assert_eq!(live_emphasis("* a*", &[], 4), None);
        assert_eq!(live_emphasis("*a *", &[], 4), None);
        assert_eq!(live_emphasis("**", &[], 2), None);
        assert_eq!(live_emphasis("`*a*", &[], 4), None);
        assert_eq!(live_emphasis("*a*b", &[], 3), None);
    }

    #[test]
    fn slices_and_joins_keep_marks_where_they_were() {
        let marks = vec![bold(2, 6)];
        assert_eq!(slice_marks(&marks, 0..4), vec![bold(2, 4)]);
        assert_eq!(slice_marks(&marks, 4..8), vec![bold(0, 2)]);
        assert_eq!(slice_marks(&marks, 6..8), vec![]);
        assert_eq!(join_marks(&[bold(0, 2)], &[bold(0, 2)], 2), vec![bold(0, 4)]);
        assert_eq!(join_marks(&[], &[italic(1, 2)], 5), vec![italic(6, 7)]);
    }

    #[test]
    fn numbers_and_levels_come_from_the_marker() {
        assert_eq!(number_of("12. "), Some((12, ". ".into())));
        assert_eq!(number_of("   3) "), Some((3, ") ".into())));
        assert_eq!(number_of("- "), None);
        assert_eq!(list_level("- "), 0);
        assert_eq!(list_level("   - "), 1);
        assert_eq!(list_level("\t- "), 1);
    }

    #[test]
    fn json_strings_escape_what_json_needs() {
        assert_eq!(json_string("a\"b\\c\nd\u{1}"), "\"a\\\"b\\\\c\\nd\\u0001\"");
    }

    #[test]
    fn transformations_change_the_letters_and_say_where_they_went() {
        let (text, moved) = change_case("say hello world", 4..15, Case::Upper, true);
        assert_eq!(text, "say HELLO WORLD");
        assert_eq!((moved_to(&moved, 4), moved_to(&moved, 9), moved_to(&moved, 15)), (4, 9, 15));
        assert_eq!(change_case("MIXED Case here", 0..15, Case::Lower, true).0, "mixed case here");
        assert_eq!(change_case("hELLO wORLD (and) more", 0..22, Case::Capitalize, true).0, "Hello World (and) More");
        assert_eq!(change_case("one\ttwo", 0..7, Case::Capitalize, true).0, "One\tTwo", "a tab ends a word too");
        // From inside a word, its next letter isn't a word's first.
        assert_eq!(change_case("pARTly", 1..6, Case::Capitalize, false).0, "partly");
        // Only the range changes.
        assert_eq!(change_case("keep THIS keep", 5..9, Case::Lower, false).0, "keep this keep");
        // A letter that lowers to two characters: what's after it moves along.
        let (text, moved) = change_case("\u{130} ok", 0..2, Case::Lower, true);
        assert_eq!(text, "i\u{307} ok");
        assert_eq!((moved_to(&moved, 0), moved_to(&moved, 2), moved_to(&moved, 3)), (0, 3, 4));
        // Letters that grow and shrink inside the range.
        let (text, moved) = change_case("\u{fb00}x", 0..4, Case::Upper, true);
        assert_eq!(text, "FFX");
        assert_eq!((moved_to(&moved, 3), moved_to(&moved, 4)), (2, 3));
        assert_eq!(Case::from_id("capitalize"), Some(Case::Capitalize));
        assert_eq!(Case::from_id("title"), None);
    }

    #[test]
    fn a_right_click_finds_its_word() {
        let line = "Dear neighbours, it\u{2019}s two";
        assert_eq!(word_around(line, 0), Some(0..4));
        assert_eq!(word_around(line, 2), Some(0..4));
        // Just after a word, as a click on its last letter's right half lands.
        assert_eq!(word_around(line, 4), Some(0..4));
        assert_eq!(word_around(line, 7), Some(5..15));
        assert_eq!(word_around(line, 16), None);
        assert_eq!(word_around(line, 18), Some(17..23));
        assert_eq!(word_around(line, line.len()), None);
        assert_eq!(word_around("", 0), None);
        assert_eq!(word_around("a  b", 2), None);
        assert_eq!(word_around("\u{4f60}\u{597d} ok", 3), Some(0..6));
    }
}
