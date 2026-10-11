//! Comments, form fields and Fill & Sign (SERVICE.md "Comments" and "Fill &
//! Sign"), each an undoable edit of the open document.
//!
//! The safety review of the engine commands behind them:
//!
//! - `comment_add` is `file` in `skill/safety.json`: its `attachment` type
//!   reads a file named by `path` into the document. The service builds
//!   its arguments itself, from the five types SERVICE.md names
//!   (highlight, underline, strikeout, note, textbox) and from `page`,
//!   `quads` (made from the caller's `rects`), `at`, `rect`, `color`,
//!   `contents` and `author` only; `pdf.comment` refuses any other key
//!   (`path`, `file`, `stamp`, `find`…) before any engine work. No
//!   parameter that names a file ever reaches the command, so it runs as a
//!   plain document edit.
//! - `form_fill` is `code`: with JavaScript on it runs the fields'
//!   keystroke, validate, format and calculate scripts. Every session has
//!   JavaScript off ([`crate::session`]), so the engine fills through
//!   `NoScripts` (the value goes in as given, no script result) and runs
//!   no XFA script; Acrobat's built-in AF formats and calculations are
//!   evaluated natively by the engine, with no interpreter. The proof is a
//!   test document whose calculate script, rejecting validate script,
//!   format and keystroke scripts, document script and open action would
//!   each leave a mark, and leave none through the service
//!   (`doc_tests::a_forms_own_scripts_never_run`).
//! - `comment_list`, `comment_edit`, `comment_delete`, `comment_reply`,
//!   `comment_set_status`, `form_fields` and `fill_sign_add` are `safe`.
//!
//! **Dates.** `comment_list` lists a comment's `/M` through pdfcraft-render's
//! `pretty_date`, which keeps the file's wall clock and drops the zone and
//! the seconds: the engine stamps its own marks in UTC
//! (`D:20261011003256Z`), so a mark made at 17:32 in California on 10 Oct
//! listed as 00:32 on 11 Oct. SERVICE.md's `date` is this device's local
//! time, so [`comments`] reads each date as the file writes it from the
//! engine's own comment reader, `pdfcraft_annot::summaries` (the reader
//! the engine's listing comes from), over a `pdfcraft_cos` document opened
//! on the open document's working bytes (its unsaved edits included), and
//! takes it to local time ([`crate::dates`]). That route only reads: the
//! bytes are the engine's own copy in memory (no file is opened, no path
//! named, nothing written), the object reader runs no script and changes
//! nothing, and the session it comes from is borrowed, not called. One pass
//! over the page tree and the comments, the cost of the engine's own
//! listing (the engine's per-comment `comment_props` walks the page tree for
//! each comment). A date is used only where it is the one the engine
//! listed for that page and place; otherwise, or if the read fails, the
//! listed wall clock stands, taken as written.

use std::collections::{BTreeSet, HashMap};
use std::panic::{catch_unwind, AssertUnwindSafe};

use serde_json::{json, Value as Json};

use crate::args::{self, xywh};
use crate::codes::{self, invalid};
use crate::dates;
use crate::docs::{self, OpenDoc};
use crate::Ctx;

/// The comment types `pdf.comment add` makes (SERVICE.md).
pub(crate) const COMMENT_TYPES: [&str; 5] = ["highlight", "underline", "strikeout", "note", "textbox"];
/// Review statuses (SERVICE.md).
const STATUSES: [&str; 5] = ["accepted", "rejected", "cancelled", "completed", "none"];
/// Fill & Sign marks (SERVICE.md).
const MARKS: [&str; 5] = ["text", "date", "initials", "check", "cross"];
/// A text box made from a single point: its size in points.
const TEXTBOX_FROM_POINT: [f64; 2] = [200.0, 40.0];
/// The longest comment, reply or typed text.
const MAX_TEXT: usize = 10_000;
/// The longest author name and colour.
const MAX_NAME: usize = 256;
/// The most fields one fill sets.
const MAX_FILL: usize = 1_000;

/// A comment's id: its name in the file, or, for a comment the file left
/// unnamed, `@<page>-<index>` (its place on the page, as the engine counts
/// it now).
fn comment_id(c: &Json) -> Json {
    match c["id"].as_str() {
        Some(id) => json!(id),
        None => match (c["page"].as_u64(), c["index"].as_u64()) {
            (Some(page), Some(index)) => json!(format!("@{page}-{index}")),
            _ => Json::Null,
        },
    }
}

/// A comment's `date` (SERVICE.md: this device's local time,
/// `YYYY-MM-DDTHH:MM`). `listed` is what the engine's `comment_list` gives,
/// the file's wall clock `YYYY-MM-DD HH:MM` without its zone (module doc);
/// `written` is the date as the file writes it, `/M` with its zone, when
/// the service could read it. A written date that is the listed one goes to
/// local time from its zone ([`dates::local`]); without one, the listed wall
/// clock is taken as written. Any other text stays as the file has it.
pub(crate) fn date(listed: &Json, written: Option<&str>, offset_at: impl Fn(i64) -> i64) -> Json {
    let Some(listed) = listed.as_str() else { return Json::Null };
    if let Some(local) = written.filter(|w| pretty_date(w) == listed).and_then(|w| dates::local(w, &offset_at)) {
        return json!(local);
    }
    let b = listed.as_bytes();
    let digit = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    if b.len() == 16 && (b[4], b[7], b[10], b[13]) == (b'-', b'-', b' ', b':') && [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15].into_iter().all(digit) {
        return json!(format!("{}T{}", &listed[..10], &listed[11..]));
    }
    // A date the listing passed through unshortened (fewer than twelve
    // digits): read as a PDF date.
    match dates::local(listed, &offset_at) {
        Some(local) => json!(local),
        None => json!(listed),
    }
}

/// What the engine's listing makes of a written date: pdfcraft-render's
/// `pretty_date` (`D:20261001123000Z` → `2026-10-01 12:30`; anything with
/// fewer than twelve digits after `D:` unchanged), so a written date can be
/// matched with the one listed for the same comment.
fn pretty_date(written: &str) -> String {
    let d = written.trim_start_matches("D:");
    let b = d.as_bytes();
    if b.len() >= 12 && b[..12].iter().all(u8::is_ascii_digit) {
        format!("{}-{}-{} {}:{}", &d[0..4], &d[4..6], &d[6..8], &d[8..10], &d[10..12])
    } else {
        written.to_string()
    }
}

/// Every comment's and reply's date as the file writes it (`/M`), by its
/// 1-based page and its 1-based place in the page's `/Annots` (as the
/// engine's `comment_list` gives `page` and `index`): read with the
/// engine's own comment reader over the open document's working bytes, the
/// reviewed read-only route of the module doc. Empty if the document cannot
/// be read so (the engine holds no editor for it either, or a reader
/// fails); a reader's panic stays here.
fn written_dates(doc: &OpenDoc) -> HashMap<(u64, u64), String> {
    let id = doc.id;
    let Some(working) = doc.engine.session().docs().iter().find(|d| d.id.0 == id) else { return HashMap::new() };
    let (bytes, password) = (working.bytes.clone(), working.password.clone());
    catch_unwind(AssertUnwindSafe(|| {
        let Ok(cos) = pdfcraft_cos::Document::open_with_password(bytes, password.as_deref()) else { return HashMap::new() };
        pdfcraft_annot::summaries(&cos).into_iter().filter_map(|s| Some(((s.page as u64 + 1, s.index as u64 + 1), s.modified?))).collect()
    }))
    .unwrap_or_default()
}

/// The engine's annotation subtype as SERVICE.md's comment type.
fn kind(subtype: &Json) -> Json {
    let t = subtype.as_str().unwrap_or("");
    json!(match t {
        "Highlight" => "highlight",
        "Underline" => "underline",
        "StrikeOut" => "strikeout",
        "Squiggly" => "squiggly",
        "Text" => "note",
        "FreeText" => "textbox",
        "Square" => "rectangle",
        "Circle" => "oval",
        "PolyLine" => "polyline",
        "FileAttachment" => "attachment",
        other => return json!(other.to_ascii_lowercase()),
    })
}

/// `pdf.comments {doc}`.
pub(crate) fn comments(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.comments";
    args::only(a, &["doc"], M)?;
    docs::with_doc(a, cx, M, |doc| {
        let list = doc.call("comment_list", json!({})).map_err(|e| codes::refused(&e))?;
        // Each date with its zone, from the file (module doc).
        let written = written_dates(doc);
        let when = |c: &Json| {
            let at = c["page"].as_u64().zip(c["index"].as_u64());
            date(&c["modified"], at.and_then(|at| written.get(&at)).map(String::as_str), dates::offset_at)
        };
        let comments: Vec<Json> = list["comments"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                // The engine lists a comment's bounding rectangle (a marked
                // passage's lines as one).
                let rect = xywh(&c["rect"]);
                json!({
                    "id": comment_id(c),
                    "page": c["page"],
                    "type": kind(&c["type"]),
                    "author": c["author"],
                    "text": c["contents"],
                    "date": when(c),
                    "color": c["color"],
                    "status": c["status"].as_str().map(str::to_ascii_lowercase).unwrap_or_else(|| "none".into()),
                    "rects": if rect.is_null() { json!([]) } else { json!([rect]) },
                    "replies": c["replies"].as_array().into_iter().flatten().map(|r| json!({
                        "id": comment_id(r),
                        "author": r["author"],
                        "text": r["contents"],
                        "date": when(r),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        Ok(json!({ "comments": comments }))
    })
}

/// Every comment and reply in the document, as the engine lists them.
fn every(doc: &mut OpenDoc) -> Result<Vec<Json>, String> {
    let list = doc.call("comment_list", json!({})).map_err(|e| codes::refused(&e))?;
    let mut out = Vec::new();
    for c in list["comments"].as_array().into_iter().flatten() {
        out.push(c.clone());
        out.extend(c["replies"].as_array().into_iter().flatten().cloned());
    }
    Ok(out)
}

/// Every comment's and reply's id.
fn ids(doc: &mut OpenDoc) -> Result<Vec<String>, String> {
    Ok(every(doc)?.iter().filter_map(|c| comment_id(c).as_str().map(str::to_owned)).collect())
}

/// The engine's target for comment `id`: its name, or the page and index
/// an `@<page>-<index>` id stands for.
fn target(doc: &mut OpenDoc, id: &str) -> Result<Json, String> {
    let place = id.strip_prefix('@').and_then(|r| r.split_once('-')).and_then(|(p, i)| Some((p.parse::<u64>().ok()?, i.parse::<u64>().ok()?)));
    if let Some((page, index)) = place.filter(|(p, i)| *p >= 1 && *i >= 1) {
        // A name in the file that happens to read like one is still a name.
        if !every(doc)?.iter().any(|c| c["id"].as_str() == Some(id)) {
            return Ok(json!({ "page": page, "index": index }));
        }
    }
    Ok(json!({ "id": id }))
}

/// `pdf.comment {doc, op, …}`.
pub(crate) fn comment(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.comment";
    let op = args::need_str(a, "op", M)?;
    match op {
        "add" => add(a, cx),
        "edit" => edit(a, cx),
        "delete" => delete(a, cx),
        "reply" => reply(a, cx),
        "status" => status(a, cx),
        other => Err(invalid(format!("{M}: `op` is add, edit, delete, reply or status, not {other:?}"))),
    }
}

/// `pdf.comment {op: "add", page, type, rects?, at?, color?, text?, author?}`.
fn add(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.comment add";
    // The reviewed parameters, and nothing else (the module doc).
    args::only(a, &["doc", "op", "page", "type", "rects", "at", "color", "text", "author"], M)?;
    let page = args::positive(a, "page", M)?;
    let kind = args::need_str(a, "type", M)?;
    if !COMMENT_TYPES.contains(&kind) {
        return Err(invalid(format!("{M}: `type` is {}, not {kind:?}", COMMENT_TYPES.join(", "))));
    }
    let rects = args::opt_rects(a, "rects", M)?;
    let at = args::opt_point(a, "at", M)?;
    let color = args::opt_str(a, "color", M, MAX_NAME)?;
    let text = args::opt_str(a, "text", M, MAX_TEXT)?;
    let author = args::opt_str(a, "author", M, MAX_NAME)?;
    let mut call = json!({ "page": page, "type": kind });
    match kind {
        "highlight" | "underline" | "strikeout" => {
            let rects = rects.ok_or_else(|| invalid(format!("{M}: a {kind} needs `rects`, the text it marks")))?;
            // A rectangle on the displayed page as a quad: its top-left,
            // top-right, bottom-left and bottom-right corners (the order
            // the engine's own `find` makes them in).
            let quads: Vec<[f64; 8]> = rects.iter().map(|&[x, y, w, h]| [x, y, x + w, y, x, y + h, x + w, y + h]).collect();
            call["quads"] = json!(quads);
        }
        "note" => {
            let at = at.ok_or_else(|| invalid(format!("{M}: a note needs `at`, where its icon goes")))?;
            call["at"] = json!(at);
        }
        _ => {
            let [x, y, w, h] = match (rects.as_deref(), at) {
                (Some([one]), _) => *one,
                (Some(_), _) => return Err(invalid(format!("{M}: a textbox takes one rectangle"))),
                (None, Some([x, y])) => [x, y, TEXTBOX_FROM_POINT[0], TEXTBOX_FROM_POINT[1]],
                (None, None) => return Err(invalid(format!("{M}: a textbox needs `rects` (one) or `at`"))),
            };
            call["rect"] = json!([x, y, x + w, y + h]);
        }
    }
    if let Some(color) = color {
        call["color"] = json!(color);
    }
    if let Some(text) = text {
        call["contents"] = json!(text);
    }
    if let Some(author) = author {
        call["author"] = json!(author);
    }
    docs::with_doc(a, cx, M, |doc| {
        let out = doc.call("comment_add", call).map_err(|e| codes::refused(&e))?;
        doc.changed();
        Ok(json!({ "id": comment_id(&out["comment"]) }))
    })
}

/// `pdf.comment {op: "edit", id, text?, color?}`.
fn edit(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.comment edit";
    args::only(a, &["doc", "op", "id", "text", "color"], M)?;
    let id = args::need_str(a, "id", M)?;
    let text = args::opt_str(a, "text", M, MAX_TEXT)?;
    let color = args::opt_str(a, "color", M, MAX_NAME)?;
    if text.is_none() && color.is_none() {
        return Err(invalid(format!("{M}: give `text`, `color` or both")));
    }
    docs::with_doc(a, cx, M, |doc| {
        let mut call = target(doc, id)?;
        if let Some(text) = text {
            call["contents"] = json!(text);
        }
        if let Some(color) = color {
            call["color"] = json!(color);
        }
        doc.call("comment_edit", call).map_err(|e| codes::refused(&e))?;
        doc.changed();
        Ok(json!({ "id": id }))
    })
}

/// `pdf.comment {op: "delete", id}`.
fn delete(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.comment delete";
    args::only(a, &["doc", "op", "id"], M)?;
    let id = args::need_str(a, "id", M)?;
    docs::with_doc(a, cx, M, |doc| {
        let call = target(doc, id)?;
        doc.call("comment_delete", call).map_err(|e| codes::refused(&e))?;
        doc.changed();
        Ok(json!({ "deleted": true }))
    })
}

/// `pdf.comment {op: "reply", id, text, author?}`: the reply's id.
fn reply(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.comment reply";
    args::only(a, &["doc", "op", "id", "text", "author"], M)?;
    let id = args::need_str(a, "id", M)?;
    let text = args::opt_str(a, "text", M, MAX_TEXT)?.filter(|t| !t.trim().is_empty()).ok_or_else(|| invalid(format!("{M} needs `text`")))?;
    let author = args::opt_str(a, "author", M, MAX_NAME)?;
    docs::with_doc(a, cx, M, |doc| {
        let before: BTreeSet<String> = ids(doc)?.into_iter().collect();
        let mut call = target(doc, id)?;
        call["text"] = json!(text);
        if let Some(author) = author {
            call["author"] = json!(author);
        }
        doc.call("comment_reply", call).map_err(|e| codes::refused(&e))?;
        doc.changed();
        let reply = ids(doc)?.into_iter().find(|n| !before.contains(n));
        Ok(json!({ "id": reply }))
    })
}

/// `pdf.comment {op: "status", id, status}`.
fn status(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.comment status";
    args::only(a, &["doc", "op", "id", "status"], M)?;
    let id = args::need_str(a, "id", M)?;
    let status = args::need_str(a, "status", M)?;
    if !STATUSES.contains(&status) {
        return Err(invalid(format!("{M}: `status` is {}, not {status:?}", STATUSES.join(", "))));
    }
    docs::with_doc(a, cx, M, |doc| {
        let mut call = target(doc, id)?;
        call["status"] = json!(status);
        doc.call("comment_set_status", call).map_err(|e| codes::refused(&e))?;
        doc.changed();
        Ok(json!({ "id": id }))
    })
}

/// `pdf.fields {doc}`.
pub(crate) fn fields(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.fields";
    args::only(a, &["doc"], M)?;
    docs::with_doc(a, cx, M, |doc| {
        let list = doc.call("form_fields", json!({})).map_err(|e| codes::refused(&e))?;
        let fields: Vec<Json> = list["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| {
                // Every field's options as {value, label}: a radio group's
                // are its buttons' export values.
                let options: Vec<Json> = f["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|o| match o.as_str() {
                        Some(v) => json!({ "value": v, "label": v }),
                        None => json!({ "value": o["value"], "label": o["label"] }),
                    })
                    .collect();
                json!({
                    "name": f["name"],
                    "type": f["type"],
                    "value": f["value"],
                    "required": f["required"] == true,
                    "read_only": f["read_only"] == true,
                    "page": f["page"],
                    "rect": xywh(&f["rect"]),
                    "options": options,
                })
            })
            .collect();
        Ok(json!({ "fields": fields }))
    })
}

/// `pdf.fill {doc, values}`: one undo step, no script runs (module doc). A
/// read-only field refuses the whole call before the engine sees it, so
/// nothing of it is filled.
pub(crate) fn fill(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.fill";
    args::only(a, &["doc", "values"], M)?;
    let values = a.get("values").and_then(Json::as_object).filter(|v| !v.is_empty()).ok_or_else(|| invalid(format!("{M} needs `values`, field names to values")))?;
    if values.len() > MAX_FILL {
        return Err(codes::fail(codes::Code::TooLarge, format!("{M} sets at most {MAX_FILL} fields at once")));
    }
    for (name, v) in values {
        let fits = match v {
            Json::String(s) => s.chars().count() <= MAX_TEXT,
            Json::Bool(_) | Json::Number(_) | Json::Null => true,
            Json::Array(items) => items.len() <= MAX_FILL && items.iter().all(|i| i.as_str().is_some_and(|s| s.chars().count() <= MAX_TEXT)),
            Json::Object(_) => false,
        };
        if !fits {
            return Err(invalid(format!("{M}: the value for {name:?} is a string, true or false, or a list of strings")));
        }
    }
    docs::with_doc(a, cx, M, |doc| {
        let listed = doc.call("form_fields", json!({})).map_err(|e| codes::refused(&e))?;
        let locked: Vec<String> = values
            .keys()
            .filter(|name| listed["fields"].as_array().into_iter().flatten().any(|f| f["name"] == name.as_str() && f["read_only"] == true))
            .map(|name| format!("{name:?}"))
            .collect();
        if !locked.is_empty() {
            let verb = if locked.len() == 1 { "is" } else { "are" };
            return Err(invalid(format!("{M}: {} {verb} read-only, so nothing was filled", locked.join(", "))));
        }
        let out = doc.call("form_fill", json!({ "values": values })).map_err(|e| codes::refused(&e))?;
        doc.changed();
        Ok(json!({ "filled": out["filled"].as_u64().unwrap_or(values.len() as u64) }))
    })
}

/// `pdf.fill_sign {doc, page, kind, at, text?}`.
pub(crate) fn fill_sign(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.fill_sign";
    args::only(a, &["doc", "page", "kind", "at", "text"], M)?;
    let page = args::positive(a, "page", M)?;
    let kind = args::need_str(a, "kind", M)?;
    if !MARKS.contains(&kind) {
        return Err(invalid(format!("{M}: `kind` is {}, not {kind:?}", MARKS.join(", "))));
    }
    let at = args::opt_point(a, "at", M)?.ok_or_else(|| invalid(format!("{M} needs `at`, [x, y] in points")))?;
    let text = args::opt_str(a, "text", M, MAX_NAME)?;
    let mut call = json!({ "page": page, "type": kind, "at": at });
    if matches!(kind, "text" | "initials") {
        let text = text.filter(|t| !t.trim().is_empty()).ok_or_else(|| invalid(format!("{M}: {kind} needs `text`")))?;
        call["text"] = json!(text);
    }
    docs::with_doc(a, cx, M, |doc| {
        doc.call("fill_sign_add", call).map_err(|e| codes::refused(&e))?;
        doc.changed();
        Ok(json!({ "added": true }))
    })
}
