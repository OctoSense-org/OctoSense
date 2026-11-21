//! Open documents (SERVICE.md "Open documents"): `pdf.open` keeps the
//! engine's document, with its edits and undo history, between calls.
//!
//! **Keys.** A document belongs to the caller app, its storage scope (the
//! area's root, as the file system spells it) and its handle. A call from
//! another app or another scope finds no document under that handle and is
//! answered `unknown_doc:`, exactly as for a handle that never existed. A
//! caller has at most [`MAX_OPEN`] documents open, and the service
//! [`MAX_OPEN_ALL`] across callers (`too_many_open:`).
//!
//! **One engine session per document**, made by [`crate::session`]: rooted
//! at the area, with the engine's JavaScript turned off before the file is
//! opened, and checked off. Closing a document drops its session.
//!
//! The engine worker owns this thread-local table and all its sessions. On
//! submission the isolate's owner captures its storage lifetime. An isolate
//! close, tag change, jail change or quota change revokes that lifetime.
//! App Hub's close listener wakes the worker to reclaim expired documents
//! and their cached files. Every call also sweeps the table before use, so
//! even a replacement with the same heap address cannot reach an old handle.
//! Sessions opened with no Makepad storage scope expire after [`IDLE`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, Weak};
use std::time::{Duration, Instant};

use octosense_engine_area::isolate::Isolate;
use octosense_engine_area::Area;
use pdfcraft_automation::{Automation, ToolError};
use serde_json::{json, Value as Json};

use crate::args;
use crate::cache;
use crate::codes::{self, fail, invalid, Code};
use crate::Ctx;

/// The most documents one caller (app and storage scope) keeps open.
pub(crate) const MAX_OPEN: usize = 8;
/// The most documents open across callers.
pub(crate) const MAX_OPEN_ALL: usize = 32;
/// How long a document no isolate holds stays open without a call.
pub(crate) const IDLE: Duration = Duration::from_secs(15 * 60);


/// A render in the cache that shows this document.
pub(crate) struct Rendered {
    /// The document's [`OpenDoc::generation`] it shows.
    pub generation: u64,
    pub width: u64,
    pub height: u64,
    pub bytes: u64,
}

/// One open document.
pub(crate) struct OpenDoc {
    pub app: String,
    pub owner: PathBuf,
    pub handle: String,
    pub engine: Automation,
    /// The engine's id of the document in its session.
    pub id: u64,
    /// The document's own file, relative to the area: what an incremental
    /// save replaces, the file it was opened (or last saved as) from.
    pub file: String,
    pub binding: Isolate,
    pub last_used: Instant,
    /// Bumped by every change: a render of an older generation is stale.
    pub generation: u64,
    /// Each page's displayed size in points, rotation applied.
    pub sizes: Vec<[f64; 2]>,
    /// Renders in the cache, by (page, dpi).
    pub renders: HashMap<(u64, u32), Rendered>,
    /// The comments' dates as the file writes them (`review.rs`), with the
    /// working bytes they were read from: the engine swaps those on every
    /// edit, undo, redo and save, so a version of the document is read once.
    pub dates: Option<(Weak<Vec<u8>>, WrittenDates)>,
}

/// Each comment's and reply's date as the file writes it, by its 1-based page
/// and its 1-based place in the page's `/Annots`.
pub(crate) type WrittenDates = HashMap<(u64, u64), String>;

impl OpenDoc {
    /// `extra` with this document's engine id: the arguments of a command
    /// on it. `extra` is built by the service from reviewed values only.
    pub fn args(&self, extra: Json) -> Json {
        let mut out = match extra {
            Json::Object(map) => Json::Object(map),
            _ => json!({}),
        };
        out["doc"] = json!(self.id);
        out
    }

    /// Run one engine command on this document's session.
    pub fn call(&mut self, tool: &str, extra: Json) -> Result<Json, ToolError> {
        let args = self.args(extra);
        crate::run(&mut self.engine, tool, &args)
    }

    /// The engine's summary of the document: pages, `dirty`, `undo`, `redo`
    /// (`doc_list`, which takes no arguments: the session holds this
    /// document alone).
    pub fn summary(&mut self) -> Result<Json, String> {
        let list = crate::run(&mut self.engine, "doc_list", &json!({})).map_err(|e| codes::refused(&e))?;
        let id = self.id;
        list["documents"]
            .as_array()
            .and_then(|docs| docs.iter().find(|d| d["doc"].as_u64() == Some(id)))
            .cloned()
            .ok_or_else(|| fail(Code::Damaged, "the engine no longer holds this document"))
    }

    /// Read the page sizes again (after a change to the pages).
    pub fn refresh(&mut self) -> Result<(), String> {
        let info = self.call("doc_info", json!({})).map_err(|e| codes::refused(&e))?;
        self.sizes = sizes_of(&info);
        Ok(())
    }

    /// The document changed: its renders are stale.
    pub fn changed(&mut self) {
        self.generation += 1;
    }

    /// Page `page`'s displayed size, or why there is no such page.
    pub fn size(&self, page: u64) -> Result<[f64; 2], String> {
        let n = self.sizes.len();
        usize::try_from(page).ok().and_then(|p| p.checked_sub(1)).and_then(|p| self.sizes.get(p)).copied().ok_or_else(|| {
            invalid(format!("page {page} is out of range: the document has {n} page{}", if n == 1 { "" } else { "s" }))
        })
    }

    fn alive(&self, now: Instant) -> bool {
        if self.binding.is_bound() {
            self.binding.is_live()
        } else {
            now.saturating_duration_since(self.last_used) < IDLE
        }
    }

    /// The document goes: its engine session, and its renders in the
    /// service's cache.
    fn release(self) {
        cache::forget(&Area::new(self.owner.clone(), None, false), &self.handle);
    }
}

/// Each page's displayed size from `doc_info`.
pub(crate) fn sizes_of(info: &Json) -> Vec<[f64; 2]> {
    let round = |v: &Json| (v.as_f64().unwrap_or(0.0) * 100.0).round() / 100.0;
    info["pages"].as_array().map(|pages| pages.iter().map(|p| [round(&p["width"]), round(&p["height"])]).collect()).unwrap_or_default()
}

#[derive(Default)]
struct Docs {
    open: Vec<OpenDoc>,
}

impl Docs {
    fn find(&self, app: &str, owner: &Path, handle: &str) -> Option<usize> {
        self.open.iter().position(|d| d.app == app && d.owner == owner && d.handle == handle)
    }
}

thread_local! {
    /// The open documents of this thread: the engine worker's (module doc).
    static DOCS: RefCell<Docs> = RefCell::new(Docs::default());
}


/// Whose documents a call reaches: its area's root as the file system
/// spells it (two spellings of one folder are one scope).
pub(crate) fn owner(area: &Area) -> PathBuf {
    area.root.canonicalize().unwrap_or_else(|_| area.root.clone())
}

/// A new handle: unique in this process, and unlike any of an earlier run
/// (it names the document's render folder, which may still be there).
fn mint() -> String {
    static SEED: OnceLock<u32> = OnceLock::new();
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let seed = *SEED.get_or_init(|| {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        (nanos as u32) ^ (nanos >> 32) as u32 ^ std::process::id().rotate_left(16)
    });
    format!("{seed:08x}{:x}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// The handle a call names: what `pdf.open` returned. Anything else names
/// no document.
fn handle(args: &Json, m: &str) -> Result<String, String> {
    match args.get("doc") {
        None | Some(Json::Null) => Err(invalid(format!("{m} needs `doc`, the handle pdf.open returned"))),
        Some(Json::String(h)) if !h.is_empty() && h.len() <= 40 && h.bytes().all(|b| b.is_ascii_digit() || b.is_ascii_lowercase()) => Ok(h.clone()),
        Some(_) => Err(unknown()),
    }
}

fn unknown() -> String {
    fail(Code::UnknownDoc, "no document of this app is open under that handle: open it again")
}

/// Release the documents of isolates that closed while a call held the
/// table, and those whose caller's scope went away (module doc).
pub(crate) fn sweep() {
    sweep_at(Instant::now());
}

pub(crate) fn sweep_at(now: Instant) {
    let gone: Vec<OpenDoc> = DOCS.with(|docs| {
        let mut docs = docs.borrow_mut();
        let (gone, kept): (Vec<OpenDoc>, Vec<OpenDoc>) = std::mem::take(&mut docs.open).into_iter().partition(|doc| !doc.alive(now));
        docs.open = kept;
        gone
    });
    gone.into_iter().for_each(OpenDoc::release);
}

/// How many documents are open on this thread (tests).
#[cfg(test)]
pub(crate) fn open_count() -> usize {
    DOCS.with(|docs| docs.borrow().open.len())
}

/// Run `f` on the open document whose handle is `handle`, if there is one,
/// to look into the engine's copy of it (tests).
#[cfg(test)]
pub(crate) fn with_open<T>(handle: &str, f: impl FnOnce(&OpenDoc) -> T) -> Option<T> {
    DOCS.with(|docs| docs.borrow().open.iter().find(|d| d.handle == handle).map(f))
}

/// Run `f` while a call holds the table, as a listener reached from inside
/// one would find it (tests).
#[cfg(test)]
pub(crate) fn while_busy(f: impl FnOnce()) {
    DOCS.with(|docs| {
        let _held = docs.borrow_mut();
        f();
    });
}

/// Run `f` on the document `args.doc` names, if this caller has it open.
/// A panic in the engine closes the document rather than leave it half
/// changed.
pub(crate) fn with_doc<T>(args: &Json, cx: &Ctx, m: &str, f: impl FnOnce(&mut OpenDoc) -> Result<T, String>) -> Result<T, String> {
    let handle = handle(args, m)?;
    let owner = owner(cx.area);
    DOCS.with(|docs| {
        let mut docs = docs.borrow_mut();
        let i = docs.find(cx.app, &owner, &handle).ok_or_else(unknown)?;
        docs.open[i].last_used = Instant::now();
        match catch_unwind(AssertUnwindSafe(|| f(&mut docs.open[i]))) {
            Ok(result) => result,
            Err(_) => {
                docs.open.remove(i);
                Err(fail(Code::Damaged, "the PDF engine failed on this document, so it was closed: open it again"))
            }
        }
    })
}


/// `pdf.open {path}`.
pub(crate) fn open(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.open";
    args::only(a, &["path"], M)?;
    let path = args::need_str(a, "path", M)?;
    crate::checked_input(cx.area, path, "path", M)?;
    let owner = owner(cx.area);
    DOCS.with(|docs| {
        let docs = docs.borrow();
        if docs.open.iter().filter(|d| d.app == cx.app && d.owner == owner).count() >= MAX_OPEN {
            return Err(fail(Code::TooManyOpen, format!("{MAX_OPEN} documents are open already: close one first")));
        }
        if docs.open.len() >= MAX_OPEN_ALL {
            return Err(fail(Code::TooManyOpen, "the PDF engine holds as many documents as it can: close one first"));
        }
        Ok(())
    })?;
    let mut engine = crate::session(cx.area)?;
    let opened = crate::run(&mut engine, "doc_open", &json!({ "path": path })).map_err(|e| codes::unreadable(&e))?;
    let id = opened["doc"].as_u64().ok_or_else(|| fail(Code::Damaged, "the engine opened no document"))?;
    let info = crate::run(&mut engine, "doc_info", &json!({ "doc": id })).map_err(|e| codes::unreadable(&e))?;
    let file: PathBuf = Path::new(path).components().collect();
    let file = file.to_string_lossy().into_owned();
    let sizes = sizes_of(&info);
    let stem = Path::new(&file).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let title = info["title"].as_str().map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned).unwrap_or(stem);
    let comments = info["annotations"].as_array().map_or(0, |all| all.iter().filter(|c| c["type"] != "Popup" && c["in_reply_to"].is_null()).count());
    let handle = mint();
    let answer = json!({
        "doc": handle,
        "path": file,
        "pages": sizes.len(),
        "sizes": sizes,
        "title": title,
        "outline": info["outline"].as_array().cloned().unwrap_or_default(),
        "fields": info["fields"].as_array().map_or(0, Vec::len),
        "comments": comments,
        "can_edit": info["document"]["editable"].as_bool().unwrap_or(false),
    });
    let doc = OpenDoc {
        app: cx.app.to_string(),
        owner: owner.clone(),
        handle,
        engine,
        id,
        file,
        binding: cx.isolate.clone(),
        last_used: Instant::now(),
        generation: 0,
        sizes,
        renders: HashMap::new(),
        dates: None,
    };
    let open: Vec<String> = DOCS.with(|docs| {
        let mut docs = docs.borrow_mut();
        docs.open.push(doc);
        docs.open.iter().filter(|d| d.app == cx.app && d.owner == owner).map(|d| d.handle.clone()).collect()
    });
    // Renders of documents no longer open in this storage are stale.
    cache::prune(cx.area, &open);
    Ok(answer)
}

/// `pdf.close {doc, discard?}`.
pub(crate) fn close(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.close";
    args::only(a, &["doc", "discard"], M)?;
    let discard = args::opt_bool(a, "discard", M)?.unwrap_or(false);
    let handle = handle(a, M)?;
    let owner = owner(cx.area);
    DOCS.with(|docs| {
        let mut docs = docs.borrow_mut();
        let i = docs.find(cx.app, &owner, &handle).ok_or_else(unknown)?;
        let edited = docs.open[i].summary().map(|s| s["dirty"] == true).unwrap_or(false);
        if edited && !discard {
            return Err(fail(Code::Unsaved, "the document has changes that are not saved: save it first, or close it with discard: true"));
        }
        docs.open.remove(i);
        Ok(())
    })?;
    cache::forget(cx.area, &handle);
    Ok(json!({ "closed": true }))
}

/// `{edited, can_undo, can_redo}` from the engine's summary, whose `undo`
/// and `redo` name the edit they would undo or redo (`null`: none).
pub(crate) fn history(summary: &Json) -> Json {
    let some = |v: &Json| !v.is_null() && *v != json!(false);
    json!({ "edited": summary["dirty"] == true, "can_undo": some(&summary["undo"]), "can_redo": some(&summary["redo"]) })
}

/// `pdf.state {doc}`.
pub(crate) fn state(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "pdf.state";
    args::only(a, &["doc"], M)?;
    with_doc(a, cx, M, |doc| {
        let summary = doc.summary()?;
        let mut out = history(&summary);
        out["pages"] = json!(summary["pages"].as_u64().unwrap_or(doc.sizes.len() as u64));
        Ok(out)
    })
}
