//! Changing an open document and writing it out (SERVICE.md "Pages",
//! "Edit", "History and saving", "Combine, split, export").
//!
//! The safety review of the engine commands behind them:
//!
//! - `page_rotate`, `page_delete`, `page_move`, `page_duplicate`,
//!   `text_edit`, `edit_undo`, `edit_redo` and `doc_list` are `safe`:
//!   undoable edits of the open document, or reads of it.
//! - `page_insert_file` is `file`: it reads the PDF at `path`. The path is
//!   the caller's, contained in its area and at most 128 MiB
//!   ([`crate::checked_input`]), and the engine's root-confined resolver
//!   checks it again at read time. Its `pages` are 1-based numbers, at
//!   most 512.
//! - `page_extract`, `doc_save`, `doc_export_images` and `doc_export_text`
//!   are `file`: they write the paths they are given. The service gives
//!   them only a path in a staging folder inside the area
//!   (`octosense_engine_area::Stage`), and moves the result into place
//!   under the area's rules: contained, within what is left of the
//!   storage, and never over an existing file except the one write
//!   SERVICE.md allows, an incremental save over the document's own file.
//!   `page_extract` gets no `delete`, `separate`, `out_dir` or `open`;
//!   `doc_export_images` gets no format but PNG; `doc_save` gets `full`
//!   as the service decides.

use std::path::{Path, PathBuf};

use octosense_engine_area::{Area, Stage};
use serde_json::{json, Value as Json};

use crate::args;
use crate::cache;
use crate::codes::{self, fail, invalid, Code};
use crate::docs::{self, OpenDoc};
use crate::reading;
use crate::Ctx;

/// `pdf.export images`' resolutions (SERVICE.md: at most 300 dpi) and its
/// default, the engine's.
const EXPORT_DPI: (u32, u32) = (24, 300);
const EXPORT_DEFAULT_DPI: u32 = 150;
/// The longest text an edit types.
const MAX_EDIT: usize = 10_000;

/// Move what the engine wrote in `stage` into place under `area`'s rules,
/// clearing the render cache first when the storage would be too full.
pub(crate) fn commit(area: &Area, stage: &Stage<'_>, moves: &[(PathBuf, PathBuf)]) -> Result<(), String> {
    let added: u64 = moves.iter().map(|(from, _)| cache::regular_len(from)).sum();
    let freed: u64 = if area.may_replace { moves.iter().map(|(_, to)| cache::regular_len(to)).sum() } else { 0 };
    cache::make_room(area, added.saturating_sub(freed));
    stage.commit(moves).map_err(codes::area)
}

/// An area like `area` whose writes never replace a file, whatever the
/// call's surface: where SERVICE.md says "a new file".
fn create_only(area: &Area) -> Area {
    Area::new(area.root.clone(), area.room(), false)
}

/// The file name of `rel` (a path already contained).
fn file_name(rel: &str, m: &str) -> Result<String, String> {
    Path::new(rel).file_name().map(|n| n.to_string_lossy().into_owned()).ok_or_else(|| invalid(format!("{m}: `{rel}` names no file")))
}

/// The pages of one change, after it: the document is new, its renders
/// stale, and its page sizes read again.
fn after_change(doc: &mut OpenDoc) -> Result<Json, String> {
    doc.changed();
    doc.refresh()?;
    Ok(json!({ "pages": doc.sizes.len() }))
}

/// `pdf.pages {doc, op, …}`.
pub(crate) fn pages(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.pages";
    let op = args::need_str(a, "op", M)?;
    match op {
        "rotate" => {
            args::only(a, &["doc", "op", "pages", "angle"], M)?;
            let pages = args::pages(a, "pages", M)?;
            let angle = args::opt_int(a, "angle", M)?.filter(|d| matches!(d, 90 | -90 | 180 | -180)).ok_or_else(|| invalid(format!("{M}: `angle` is 90, -90 or 180")))?;
            docs::with_doc(a, cx, M, |doc| {
                doc.call("page_rotate", json!({ "pages": pages, "degrees": angle })).map_err(|e| codes::refused(&e))?;
                after_change(doc)
            })
        }
        "delete" => {
            args::only(a, &["doc", "op", "pages"], M)?;
            let pages = args::pages(a, "pages", M)?;
            docs::with_doc(a, cx, M, |doc| {
                let mut distinct = pages.clone();
                distinct.sort_unstable();
                distinct.dedup();
                if distinct.len() >= doc.sizes.len() && distinct.iter().all(|p| *p as usize <= doc.sizes.len()) {
                    return Err(invalid("a document keeps at least one page: deleting every page is refused"));
                }
                doc.call("page_delete", json!({ "pages": pages })).map_err(|e| codes::refused(&e))?;
                after_change(doc)
            })
        }
        "move" => {
            args::only(a, &["doc", "op", "pages", "to"], M)?;
            let pages = args::pages(a, "pages", M)?;
            let to = args::positive(a, "to", M)?;
            docs::with_doc(a, cx, M, |doc| {
                doc.call("page_move", json!({ "pages": pages, "to": to })).map_err(|e| codes::refused(&e))?;
                after_change(doc)
            })
        }
        "duplicate" => {
            args::only(a, &["doc", "op", "pages"], M)?;
            let pages = args::pages(a, "pages", M)?;
            docs::with_doc(a, cx, M, |doc| {
                doc.call("page_duplicate", json!({ "pages": pages })).map_err(|e| codes::refused(&e))?;
                after_change(doc)
            })
        }
        "insert_file" => {
            args::only(a, &["doc", "op", "path", "at", "pages"], M)?;
            let path = args::need_str(a, "path", M)?;
            let at = args::positive(a, "at", M)?;
            let pages = args::opt_pages(a, "pages", M)?;
            crate::checked_input(cx.area, path, "path", M)?;
            let mut call = json!({ "path": path, "at": at });
            if let Some(pages) = pages {
                call["pages"] = json!(pages);
            }
            docs::with_doc(a, cx, M, |doc| {
                doc.call("page_insert_file", call).map_err(|e| codes::read_or_refused(&e))?;
                after_change(doc)
            })
        }
        "extract" => {
            args::only(a, &["doc", "op", "pages", "out"], M)?;
            let pages = args::pages(a, "pages", M)?;
            let out = args::need_str(a, "out", M)?;
            // A new file in storage, never over one, foreground or not.
            let area = create_only(cx.area);
            let dest = crate::out_path(&area, out, "out", M)?;
            let name = file_name(out, M)?;
            docs::with_doc(a, cx, M, |doc| {
                let stage = area.stage().map_err(codes::area)?;
                let staged = crate::staged(&area, &stage, &name)?;
                doc.call("page_extract", json!({ "pages": pages, "out": staged, "open": false })).map_err(|e| codes::refused(&e))?;
                commit(&area, &stage, &[(stage.path(&name), dest)])?;
                Ok(json!({ "path": out }))
            })
        }
        other => Err(invalid(format!("{M}: `op` is rotate, delete, move, duplicate, insert_file or extract, not {other:?}"))),
    }
}

/// `pdf.edit_text {doc, page, paragraph?, line?, text}`.
pub(crate) fn edit_text(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.edit_text";
    args::only(a, &["doc", "page", "paragraph", "line", "text"], M)?;
    let page = args::positive(a, "page", M)?;
    let text = args::opt_str(a, "text", M, MAX_EDIT)?.ok_or_else(|| invalid(format!("{M} needs `text`")))?;
    let paragraph = args::opt_int(a, "paragraph", M)?;
    let line = args::opt_int(a, "line", M)?;
    let mut call = json!({ "page": page, "text": text });
    match (paragraph, line) {
        (Some(n), None) if n >= 1 => call["paragraph"] = json!(n),
        (None, Some(n)) if n >= 1 => call["line"] = json!(n),
        _ => return Err(invalid(format!("{M}: give exactly one of `paragraph` or `line`, a number from pdf.lines"))),
    }
    docs::with_doc(a, cx, M, |doc| {
        doc.size(page)?;
        doc.call("text_edit", call).map_err(|e| codes::refused(&e))?;
        doc.changed();
        Ok(json!({ "edited": true }))
    })
}

/// `pdf.undo {doc}` and `pdf.redo {doc}`.
pub(crate) fn history(a: &Json, cx: &Ctx, redo: bool) -> Result<Json, String> {
    let m = if redo { "pdf.redo" } else { "pdf.undo" };
    args::only(a, &["doc"], m)?;
    docs::with_doc(a, cx, m, |doc| {
        let out = doc.call(if redo { "edit_redo" } else { "edit_undo" }, json!({})).map_err(|e| codes::refused(&e))?;
        doc.changed();
        doc.refresh()?;
        Ok(docs::history(&out["document"]))
    })
}

/// `pdf.save {doc, path?}`: an incremental save to the document's own
/// file, or a full save to a new file, which the document then is.
pub(crate) fn save(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.save";
    args::only(a, &["doc", "path"], M)?;
    let to = args::opt_str(a, "path", M, 512)?;
    docs::with_doc(a, cx, M, |doc| {
        let was_edited = doc.summary()?["dirty"] == true;
        let (area, dest, rel, full) = match to {
            None => {
                // The one write that replaces a file: the document's own,
                // which pdf.open opened for editing, from the foreground.
                if !cx.area.may_replace {
                    return Err(invalid("saving over the document's own file needs the app in the foreground"));
                }
                let dest = crate::contained(cx.area, &doc.file, "path", M)?;
                (Area::new(cx.area.root.clone(), cx.area.room(), true), dest, doc.file.clone(), false)
            }
            Some(out) => {
                // A full save to a new file: never over an existing one.
                let area = create_only(cx.area);
                let dest = crate::out_path(&area, out, "path", M)?;
                let rel: PathBuf = Path::new(out).components().collect();
                (area, dest, rel.to_string_lossy().into_owned(), true)
            }
        };
        let name = file_name(&rel, M)?;
        let stage = area.stage().map_err(codes::area)?;
        let staged = crate::staged(&area, &stage, &name)?;
        let saved = doc.call("doc_save", json!({ "path": staged, "full": full })).map_err(|e| codes::refused(&e))?;
        if let Err(e) = commit(&area, &stage, &[(stage.path(&name), dest)]) {
            // The engine counts the document saved now, and it is not. An
            // undo and a redo leave its content as it was and mark it edited
            // again, so closing it still asks to save.
            doc.changed();
            if was_edited && doc.call("edit_undo", json!({})).is_ok() && doc.call("edit_redo", json!({})).is_err() {
                return Err(format!("{e} (and the document's last change was undone: redo it)"));
            }
            return Err(e);
        }
        // The engine reads the document's file from the staging folder it
        // saved to: the service names its file itself from here on.
        doc.file = rel.clone();
        Ok(json!({ "path": rel, "bytes": saved["bytes"] }))
    })
}

/// `pdf.export {doc, kind: "images", out_dir, dpi?, pages?}` or
/// `{doc, kind: "text", out, pages?}`: new files only.
pub(crate) fn export(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.export";
    let kind = args::need_str(a, "kind", M)?;
    let area = create_only(cx.area);
    match kind {
        "images" => {
            args::only(a, &["doc", "kind", "out_dir", "dpi", "pages"], M)?;
            let out_dir = args::need_str(a, "out_dir", M)?;
            let dpi = args::dpi(a, "dpi", M, EXPORT_DEFAULT_DPI, EXPORT_DPI)?;
            let pages = args::opt_pages(a, "pages", M)?;
            let dir = crate::contained(&area, out_dir, "out_dir", M)?;
            docs::with_doc(a, cx, M, |doc| {
                let pages = all_pages(doc, pages, M)?;
                for &p in &pages {
                    reading::check_pixels(doc.size(p)?, dpi, p)?;
                }
                let stage = area.stage().map_err(codes::area)?;
                let staging = crate::staged(&area, &stage, "")?;
                doc.call("doc_export_images", json!({ "folder": staging, "dpi": dpi, "pages": pages, "format": "png" })).map_err(|e| codes::refused(&e))?;
                let moves: Vec<(PathBuf, PathBuf)> = stage.files().into_iter().map(|rel| (stage.path(&rel), dir.join(&rel))).collect();
                commit(&area, &stage, &moves)?;
                let paths: Vec<String> = stage_names(&moves).iter().map(|n| Path::new(out_dir).join(n).to_string_lossy().into_owned()).collect();
                Ok(json!({ "paths": paths }))
            })
        }
        "text" => {
            args::only(a, &["doc", "kind", "out", "pages"], M)?;
            let out = args::need_str(a, "out", M)?;
            let pages = args::opt_pages(a, "pages", M)?;
            let dest = crate::out_path(&area, out, "out", M)?;
            let name = file_name(out, M)?;
            docs::with_doc(a, cx, M, |doc| {
                let pages = all_pages(doc, pages, M)?;
                let stage = area.stage().map_err(codes::area)?;
                let staged = crate::staged(&area, &stage, &name)?;
                doc.call("doc_export_text", json!({ "path": staged, "pages": pages })).map_err(|e| codes::refused(&e))?;
                commit(&area, &stage, &[(stage.path(&name), dest)])?;
                Ok(json!({ "paths": [out] }))
            })
        }
        other => Err(invalid(format!("{M}: `kind` is images or text, not {other:?}"))),
    }
}

/// The pages an export names, or every page of a document of at most
/// [`args::MAX_PAGES`].
fn all_pages(doc: &OpenDoc, pages: Option<Vec<u64>>, m: &str) -> Result<Vec<u64>, String> {
    match pages {
        Some(pages) => Ok(pages),
        None if doc.sizes.len() <= args::MAX_PAGES => Ok((1..=doc.sizes.len() as u64).collect()),
        None => Err(fail(Code::TooLarge, format!("{m}: the document has {} pages; pass `pages` (at most {} per call)", doc.sizes.len(), args::MAX_PAGES))),
    }
}

/// The file names the moves land on, in order.
fn stage_names(moves: &[(PathBuf, PathBuf)]) -> Vec<String> {
    moves.iter().filter_map(|(_, to)| to.file_name().map(|n| n.to_string_lossy().into_owned())).collect()
}
