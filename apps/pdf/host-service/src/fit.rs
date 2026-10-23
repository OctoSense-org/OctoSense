//! What an edit would do, before PDF Tools makes it (`pdf.edit_check`).
//!
//! Real-world PDFs draw text in fonts the engine can't always type new text
//! in. An embedded subset holds only the glyphs the document uses, and so do
//! a browser's Type 3 fonts; a designed title may come in pieces, one font
//! each. Text the paragraph's font can't show, the engine sets in a stand-in
//! (Helvetica, Times or Courier), with other widths; text no stand-in shows
//! (Chinese, Japanese or Korean, without the fallback face this build leaves
//! out) it refuses. An edited one-line paragraph grows to the right up to the
//! page's margin, and a longer one grows down, over whatever is there. So
//! PDF Tools asks first, and says what will happen before Apply.
//!
//! `pdf.edit_check` takes `pdf.edit_text`'s arguments and answers
//! `{keeps_font, font, box, covers, off_page}`, or the refusal
//! `pdf.edit_text` would give, in the same words ([`codes::edit_refused`]):
//!
//! - `keeps_font`: the new text is drawn in the paragraph's (or line's) own
//!   font. `font` names the font it would be drawn in: the stand-in, or the
//!   paragraph's own name (`""` for an unnamed one, as `pdf.lines` says).
//! - `box`: where the edited text would be, `[x, y, w, h]` in points from the
//!   top left of the displayed page; `null` when no text would show.
//! - `covers`: how many other lines of text on the page the edited text would
//!   run into that the paragraph doesn't touch now.
//! - `off_page`: part of the edited text would fall outside the page.
//!
//! **How.** The engine's own editor makes the edit (pdfcraft-edit's
//! `rewrite_block` for a paragraph and `replace_line` for a line, what the
//! engine's `text_edit` runs), on a private copy: a `pdfcraft_cos` document
//! opened on the open document's working bytes (its unsaved edits included),
//! as `review.rs` reads comment dates, and dropped after. The edit's lines are
//! the page's lines after it that weren't on the page before, or that were
//! the paragraph's own; every other line is compared with them.
//!
//! **Review.** No engine command runs and nothing is written: the copy lives
//! in memory, no file is opened or named, and the editor runs no script (a
//! fallback font it would make goes into the copy only). The session is
//! borrowed, never called. The arguments and the page are checked as
//! `pdf.edit_text` checks them, the text is at most 10,000 characters, and
//! the document's read-only state and its security settings refuse as the
//! engine would. A panic in the editor is this call's `damaged:` error and
//! leaves the open document as it was. The cost is the engine's own on the
//! UI thread (#399): opening the working bytes (the object table; objects are
//! read as they are needed) and reading the page's text before and after.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

use pdfcraft_edit::{Align, BlockStyle, TextLine};
use serde_json::{json, Value as Json};

use crate::args::xywh;
use crate::change::{EditCall, Target};
use crate::codes::{self, fail, Code};
use crate::docs::{self, OpenDoc};
use crate::Ctx;

/// How far (points) the edited text may cross another line's box, or the
/// page's edge, before it counts: less is rounding.
const SLACK: f64 = 0.5;

/// `pdf.edit_check {doc, page, paragraph?, line?, text, align?, color?}`.
pub(crate) fn edit_check(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.edit_check";
    let edit = EditCall::read(a, M)?;
    docs::with_doc(a, cx, M, |doc| {
        doc.size(edit.page)?;
        check(doc, &edit)
    })
}

/// A line's identity across the edit: its text, its box to 1/100 pt and its
/// font's resource name.
type Key = (String, [i64; 4], String);

fn key(l: &TextLine) -> Key {
    (l.text.clone(), l.rect.map(|v| (v * 100.0).round() as i64), l.font.clone())
}

/// The edit `edit` on a copy of `doc` (module doc).
fn check(doc: &OpenDoc, edit: &EditCall) -> Result<Json, String> {
    let refused = |words: &str| codes::edit_refused(words, edit.text);
    let page = (edit.page - 1) as usize;
    let engine = doc.engine.session().docs().iter().find(|d| d.id.0 == doc.id).ok_or_else(|| fail(Code::Damaged, "the engine no longer holds this document"))?;
    if let Some(reason) = &engine.read_only_reason {
        return Err(refused(&format!("this document can't be edited: {reason}")));
    }
    let view = engine.info.pages.get(page).cloned().ok_or_else(|| refused(&format!("page {} is out of range", edit.page)))?;
    let mut copy = pdfcraft_cos::Document::open_with_password(engine.bytes.clone(), engine.password.as_deref())
        .map_err(|e| refused(&format!("this document can't be edited: {e}")))?;
    if copy.permissions().is_some_and(|p| !p.modify()) {
        return Err(refused("the document's security settings don't allow changes to the document"));
    }
    let before = pdfcraft_edit::text_lines(&copy, page).map_err(|e| refused(&e.to_string()))?;
    // The paragraph's (or line's) own lines, and its font as pdf.lines names it.
    let (own, font) = match edit.target {
        Target::Paragraph(n) => {
            let blocks = pdfcraft_edit::text_blocks(&copy, page).map_err(|e| refused(&e.to_string()))?;
            let b = blocks.get((n - 1) as usize).ok_or_else(|| refused(&format!("paragraph {n} is out of range: page {} has {} paragraphs", edit.page, blocks.len())))?;
            (b.lines.clone(), b.base_font.clone())
        }
        Target::Line(n) => {
            let l = before.get((n - 1) as usize).ok_or_else(|| refused(&format!("line {n} is out of range: page {} has {} lines", edit.page, before.len())))?;
            (vec![(n - 1) as usize], l.base_font.clone())
        }
    };
    let style = BlockStyle { align: edit.align.map(align), color: edit.color.as_deref().map(rgb), ..BlockStyle::default() };
    let made = catch_unwind(AssertUnwindSafe(|| match edit.target {
        Target::Paragraph(n) => pdfcraft_edit::rewrite_block(&mut copy, page, (n - 1) as usize, Some(edit.text), &style),
        Target::Line(n) => pdfcraft_edit::replace_line(&mut copy, page, (n - 1) as usize, edit.text),
    }))
    .map_err(|_| fail(Code::Damaged, "the PDF engine failed unexpectedly on this edit; the document was not changed"))?
    .map_err(|e| refused(&e.to_string()))?;
    let after = pdfcraft_edit::text_lines(&copy, page).map_err(|e| refused(&e.to_string()))?;

    // Each of the page's other lines found again is still there; what is
    // left is the edited text.
    let mut others: HashMap<Key, usize> = HashMap::new();
    for (i, l) in before.iter().enumerate() {
        if !own.contains(&i) {
            *others.entry(key(l)).or_default() += 1;
        }
    }
    let (mut edited, mut kept) = (Vec::new(), Vec::new());
    for l in &after {
        match others.get_mut(&key(l)) {
            Some(n) if *n > 0 => {
                *n -= 1;
                kept.push(l.rect);
            }
            _ => edited.push(l.rect),
        }
    }
    let was: Vec<[f64; 4]> = own.iter().filter_map(|i| before.get(*i)).map(|l| l.rect).collect();
    let covers = kept.iter().filter(|k| edited.iter().any(|e| runs_into(**k, *e)) && !was.iter().any(|w| runs_into(**k, *w))).count();
    let crop = view.crop.map(f64::from);
    let off_page = edited.iter().any(|r| r[0] < crop[0] - SLACK || r[1] < crop[1] - SLACK || r[2] > crop[2] + SLACK || r[3] > crop[3] + SLACK);
    let whole = edited.iter().copied().reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]);
    let shown = match whole {
        // Into the displayed page's points, as pdf.lines gives boxes.
        Some(b) => {
            let (p, q) = (view.user_to_view(b[0] as f32, b[1] as f32), view.user_to_view(b[2] as f32, b[3] as f32));
            xywh(&json!([f64::from(p[0]), f64::from(p[1]), f64::from(q[0]), f64::from(q[1])]))
        }
        None => Json::Null,
    };
    Ok(json!({
        "keeps_font": made.substituted.is_none(),
        "font": made.substituted.unwrap_or(font),
        "box": shown,
        "covers": covers,
        "off_page": off_page,
    }))
}

/// Whether two line boxes overlap enough to show: more than [`SLACK`]
/// across, and by more than a quarter of the smaller line's height (lines
/// of a tightly set paragraph touch by less).
fn runs_into(a: [f64; 4], b: [f64; 4]) -> bool {
    let across = a[2].min(b[2]) - a[0].max(b[0]);
    let down = a[3].min(b[3]) - a[1].max(b[1]);
    let tall = (a[3] - a[1]).min(b[3] - b[1]).max(0.01);
    across > SLACK && down > tall * 0.25
}

/// An alignment as [`crate::change`] checked it.
fn align(name: &str) -> Align {
    match name {
        "center" => Align::Center,
        "right" => Align::Right,
        "justify" => Align::Justify,
        _ => Align::Left,
    }
}

/// `#rrggbb` (as [`crate::change`] wrote it) as RGB from 0 to 1, as the
/// engine reads a colour.
fn rgb(hex: &str) -> [f64; 3] {
    let byte = |i: usize| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).map_or(0.0, |b| f64::from(b) / 255.0);
    [byte(1), byte(3), byte(5)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_run_into_each_other_only_when_it_shows() {
        let line = [10.0, 100.0, 110.0, 112.0];
        assert!(runs_into(line, [100.0, 101.0, 150.0, 113.0]), "the same baseline, ten points over");
        assert!(!runs_into(line, [110.0, 100.0, 150.0, 112.0]), "touching ends");
        assert!(!runs_into(line, [110.3, 100.0, 150.0, 112.0]), "within rounding");
        assert!(!runs_into(line, [10.0, 89.0, 110.0, 101.0]), "a tightly set line below touches by a point");
        assert!(runs_into(line, [10.0, 94.0, 110.0, 106.0]), "a line half under it");
        assert!(!runs_into(line, [10.0, 60.0, 110.0, 72.0]), "far below");
    }

    #[test]
    fn colours_and_alignments_reach_the_editor_as_checked() {
        assert_eq!(rgb("#c0392b"), [192.0 / 255.0, 57.0 / 255.0, 43.0 / 255.0]);
        assert_eq!(rgb("#000000"), [0.0, 0.0, 0.0]);
        assert_eq!(align("center"), Align::Center);
        assert_eq!(align("justify"), Align::Justify);
        assert_eq!(align("left"), Align::Left);
    }
}
