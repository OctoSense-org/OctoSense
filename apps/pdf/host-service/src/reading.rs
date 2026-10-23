//! Reading an open document (SERVICE.md "Reading"): a page rendered at
//! screen resolution into the render cache, find with match rectangles,
//! and the text lines and paragraphs of a page. Every engine command here
//! is `safe`: `page_render`, `text_find`, `text_extract`, `text_lines` and
//! `text_paragraphs` read the open document and write nothing; the one
//! file a call writes is the service's own render ([`cache::store`]).

use std::collections::BTreeMap;

use pdfcraft_automation::Content;
use serde_json::{json, Value as Json};

use crate::args::{self, xywh};
use crate::cache;
use crate::codes::{self, fail, invalid, Code};
use crate::docs::{self, Rendered};
use crate::Ctx;

/// `pdf.page`'s resolutions (SERVICE.md: 24 to 300, default 96).
pub(crate) const DPI_RANGE: (u32, u32) = (24, 300);
pub(crate) const DEFAULT_DPI: u32 = 96;
/// The most pixels one render makes (16 megapixels).
pub(crate) const MAX_PIXELS: u64 = 16_000_000;
/// `pdf.find`'s `limit` (SERVICE.md: at most 500, default 200).
pub(crate) const MAX_FIND: u64 = 500;
pub(crate) const DEFAULT_FIND: u64 = 200;
/// How many matches `total` counts at most: the engine's work for one
/// find stays bounded, however common the query.
pub(crate) const FIND_COUNT: u64 = 10_000;
/// The longest query.
const MAX_QUERY: usize = 1_000;
/// A snippet's length in characters, the `[[` and `]]` around the match
/// and any `…` included.
pub(crate) const SNIPPET: usize = 120;

/// Device pixels that cover `points` at `dpi`, rounded up as the engine's
/// renderer does (`pdfcraft_render::device_pixels`).
pub(crate) fn pixels(points: f64, dpi: u32) -> u64 {
    let px = points * f64::from(dpi) / 72.0 - 0.01;
    if px.is_finite() { px.ceil().max(1.0) as u64 } else { u64::MAX }
}

/// The pixel cap for a page of `size` points at `dpi`.
pub(crate) fn check_pixels(size: [f64; 2], dpi: u32, page: u64) -> Result<(u64, u64), String> {
    let (w, h) = (pixels(size[0], dpi), pixels(size[1], dpi));
    if w.saturating_mul(h) > MAX_PIXELS {
        return Err(fail(Code::TooLarge, format!("page {page} at {dpi} dpi is {w} × {h} pixels, more than {} megapixels: choose a lower dpi", MAX_PIXELS / 1_000_000)));
    }
    Ok((w, h))
}

/// `pdf.page {doc, page, dpi?}`: the page as a PNG in the render cache.
pub(crate) fn page(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.page";
    args::only(a, &["doc", "page", "dpi"], M)?;
    let page = args::positive(a, "page", M)?;
    let dpi = args::dpi(a, "dpi", M, DEFAULT_DPI, DPI_RANGE)?;
    docs::with_doc(a, cx, M, |doc| {
        let size = doc.size(page)?;
        check_pixels(size, dpi, page)?;
        let rel = cache::render_path(&doc.handle, page, dpi);
        if let Some(r) = doc.renders.get(&(page, dpi)) {
            if r.generation == doc.generation && cache::still_there(cx.area, &rel, r.bytes) {
                return Ok(json!({ "path": rel, "width": r.width, "height": r.height, "dpi": dpi }));
            }
        }
        let contents = doc.engine.call("page_render", &doc.args(json!({ "page": page, "dpi": dpi }))).map_err(|e| codes::refused(&e))?;
        let Some(Content::Png { data, width, height }) = contents.into_iter().find(|c| matches!(c, Content::Png { .. })) else {
            return Err(fail(Code::Damaged, format!("page {page} could not be rendered")));
        };
        cache::store(cx.area, &rel, &data)?;
        let (width, height) = (u64::from(width), u64::from(height));
        doc.renders.insert((page, dpi), Rendered { generation: doc.generation, width, height, bytes: data.len() as u64 });
        Ok(json!({ "path": rel, "width": width, "height": height, "dpi": dpi }))
    })
}

/// `pdf.find {doc, query, limit?}`.
pub(crate) fn find(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.find";
    args::only(a, &["doc", "query", "limit"], M)?;
    let query = args::need_str(a, "query", M)?;
    if query.chars().count() > MAX_QUERY {
        return Err(fail(Code::TooLarge, format!("{M}: `query` is at most {MAX_QUERY} characters")));
    }
    if query.trim().is_empty() {
        return Err(invalid(format!("{M}: `query` has no words")));
    }
    let limit = match args::opt_int(a, "limit", M)? {
        None => DEFAULT_FIND,
        Some(n) if (1..=MAX_FIND as i64).contains(&n) => n as u64,
        Some(_) => return Err(invalid(format!("{M}: `limit` is 1 to {MAX_FIND}"))),
    };
    docs::with_doc(a, cx, M, |doc| {
        let found = doc.call("text_find", json!({ "query": query, "limit": FIND_COUNT })).map_err(|e| codes::refused(&e))?;
        let all = found["matches"].as_array().cloned().unwrap_or_default();
        let shown: Vec<&Json> = all.iter().take(limit as usize).collect();
        // The page text, to cut each match's snippet from (the engine's
        // find has just extracted it: these read its cache).
        let mut pages: Vec<u64> = shown.iter().filter_map(|m| m["page"].as_u64()).collect();
        pages.dedup();
        let mut texts: BTreeMap<u64, Vec<(usize, usize)>> = BTreeMap::new();
        let mut flat: BTreeMap<u64, Vec<char>> = BTreeMap::new();
        if !pages.is_empty() {
            let extracted = doc.call("text_extract", json!({ "pages": pages })).map_err(|e| codes::refused(&e))?;
            for p in extracted["pages"].as_array().into_iter().flatten() {
                if let (Some(n), Some(text)) = (p["page"].as_u64(), p["text"].as_str()) {
                    let (chars, hits) = occurrences(text, query);
                    flat.insert(n, chars);
                    texts.insert(n, hits);
                }
            }
        }
        let mut seen: BTreeMap<u64, usize> = BTreeMap::new();
        let matches: Vec<Json> = shown
            .iter()
            .map(|m| {
                let page = m["page"].as_u64().unwrap_or(0);
                let k = seen.entry(page).or_insert(0);
                let hit = texts.get(&page).and_then(|hits| hits.get(*k)).copied();
                *k += 1;
                let snippet = match (hit, flat.get(&page)) {
                    (Some((start, end)), Some(chars)) => snippet(chars, start, end),
                    // The page text did not show the match where the engine
                    // found it: the match alone.
                    _ => marked(&collapse(m["text"].as_str().unwrap_or(query)).chars().collect::<Vec<_>>()),
                };
                json!({
                    "page": page,
                    "rects": m["rects"].as_array().map(|r| r.iter().map(xywh).collect::<Vec<_>>()).unwrap_or_default(),
                    "snippet": snippet,
                })
            })
            .collect();
        Ok(json!({ "total": all.len(), "matches": matches }))
    })
}

/// `text` with every run of whitespace one space, trimmed.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The page text with whitespace collapsed, as characters, and where
/// `needle` occurs in it, as the engine's find matches: case-insensitive,
/// whitespace-normalised, left to right without overlaps. The k-th
/// occurrence is the engine's k-th match on the page.
pub(crate) fn occurrences(text: &str, needle: &str) -> (Vec<char>, Vec<(usize, usize)>) {
    let chars: Vec<char> = collapse(text).chars().collect();
    // Lower-cased, each lower character mapped back to its character.
    let mut lower: Vec<(char, usize)> = Vec::with_capacity(chars.len());
    for (i, c) in chars.iter().enumerate() {
        for l in c.to_lowercase() {
            lower.push((l, i));
        }
    }
    let needle: Vec<char> = collapse(&needle.to_lowercase()).chars().collect();
    let mut hits = Vec::new();
    if needle.is_empty() {
        return (chars, hits);
    }
    let mut i = 0;
    while i + needle.len() <= lower.len() {
        if lower[i..i + needle.len()].iter().map(|c| c.0).eq(needle.iter().copied()) {
            hits.push((lower[i].1, lower[i + needle.len() - 1].1 + 1));
            i += needle.len();
        } else {
            i += 1;
        }
    }
    (chars, hits)
}

/// The match `chars[start..end]` marked `[[…]]` in the text around it, at
/// most [`SNIPPET`] characters with `…` where the text was cut.
pub(crate) fn snippet(chars: &[char], start: usize, end: usize) -> String {
    let inner = SNIPPET - 4;
    if end - start >= inner {
        return marked(&chars[start..end]);
    }
    // Context on both sides, the rest of one side's share to the other.
    let room = inner - (end - start);
    let mut before = (room / 2).min(start);
    let mut after = (room - before).min(chars.len() - end);
    before = (room - after).min(start);
    // A cut costs an ellipsis.
    let cut_before = start - before > 0;
    let cut_after = end + after < chars.len();
    let spare = room - before - after;
    let need = usize::from(cut_before) + usize::from(cut_after);
    // Trim the longer side first, so a side shown whole stays whole.
    let mut short = need.saturating_sub(spare);
    while short > 0 && (before > 0 || after > 0) {
        if after >= before {
            after -= 1;
        } else {
            before -= 1;
        }
        short -= 1;
    }
    let cut_before = start - before > 0;
    let cut_after = end + after < chars.len();
    let mut out = String::new();
    if cut_before {
        out.push('…');
    }
    out.extend(chars[start - before..start].iter());
    out.push_str("[[");
    out.extend(chars[start..end].iter());
    out.push_str("]]");
    out.extend(chars[end..end + after].iter());
    if cut_after {
        out.push('…');
    }
    out
}

/// A match alone, marked, cut with `…` to fit [`SNIPPET`].
fn marked(text: &[char]) -> String {
    let budget = SNIPPET - 4;
    let mut out = String::from("[[");
    if text.len() > budget {
        out.extend(text[..budget - 1].iter());
        out.push('…');
    } else {
        out.extend(text.iter());
    }
    out.push_str("]]");
    out
}

/// `pdf.lines {doc, page}`.
pub(crate) fn lines(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.lines";
    args::only(a, &["doc", "page"], M)?;
    let page = args::positive(a, "page", M)?;
    docs::with_doc(a, cx, M, |doc| {
        doc.size(page)?;
        let lines = doc.call("text_lines", json!({ "page": page })).map_err(|e| codes::refused(&e))?;
        let paragraphs = doc.call("text_paragraphs", json!({ "page": page })).map_err(|e| codes::refused(&e))?;
        let lines: Vec<Json> = lines["lines"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|l| json!({ "n": l["line"], "text": l["text"], "box": xywh(&l["rect"]), "font": l["font"], "size": l["size"] }))
            .collect();
        let paragraphs: Vec<Json> = paragraphs["paragraphs"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| json!({ "n": p["paragraph"], "text": p["text"], "box": xywh(&p["rect"]), "lines": p["lines"], "font": p["font"], "size": p["size"] }))
            .collect();
        Ok(json!({ "lines": lines, "paragraphs": paragraphs }))
    })
}
