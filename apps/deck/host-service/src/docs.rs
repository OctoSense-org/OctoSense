//! Open documents (SERVICE.md "Open documents"): `deck.open` and
//! `deck.create` keep the engine's session for a deck, with its edits, undo
//! history, current slide and selection, between calls.
//!
//! **Keys.** A document belongs to the caller app, its storage scope (the
//! area's root, as the file system spells it) and its handle. A call from
//! another app or another scope finds no document under that handle and is
//! answered `unknown_doc:`, exactly as for a handle that never existed. A
//! caller has at most [`MAX_OPEN`] documents open, and the service
//! [`MAX_OPEN_ALL`] across callers (`too_many_open:`). `deck.cover` opens
//! no document.
//!
//! **One engine session per document**, made by [`app_session`]: the
//! engine's AutoCorrect and smart quotes off (text is stored as typed), and
//! its own undo limit lifted, because the service folds each call's undo
//! steps into one and keeps at most [`MAX_UNDO`] ([`crate::gate`]).
//! Closing a document drops its session.
//!
//! **The thread.** Every call reaches this service on the shell's UI
//! thread: App Hub's `services::pump` dispatches an app's `host.request`
//! inline from the card runner's event handler, and the agent relay and
//! components' host calls are pumped from the same place. So the table is a
//! `thread_local`, and the UI thread takes no lock for it (makepad's rule:
//! the UI thread never takes a lock another thread can hold). A call on any
//! other thread finds an empty table of its own: its handles are unknown
//! there, which fails closed and shares nothing.
//!
//! **Release**, as the pdf service's (`apps/pdf/host-service/src/docs.rs`):
//! a document goes on `deck.close`; when the isolate that opened it closes
//! (App Hub's `services::on_isolate_closed` listener, [`isolate_closed`],
//! called from `services::cancel_heap` before the isolate's heap key, an
//! address, can be reused); when its storage scope changes or goes (makepad's
//! `splash_storage::storage_for_heap` for that key, checked by every call's
//! [`sweep`]); and, for a caller with no isolate (a component's host call, a
//! test), after [`IDLE`] without a call. Its renders go with it, and any a
//! released document left behind go when the caller next opens a document
//! ([`cache::prune`]).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use deckcraft_engine::cmd::file as engine_file;
use deckcraft_engine::{DocState, Session};
use deckcraft_model::Presentation;
use octosense_appstore::makepad_widgets::splash_storage::{self, StorageAccess};
use octosense_engine_area::Area;
use serde_json::{json, Value as Json};

use crate::args;
use crate::cache;
use crate::codes::{fail, invalid, too_large, Code};
use crate::gate::{self, HeldCache, Plan};
use crate::guard::Format;
use crate::Ctx;

/// The most documents one caller (app and storage scope) keeps open.
pub(crate) const MAX_OPEN: usize = 8;
/// The most documents open across callers.
pub(crate) const MAX_OPEN_ALL: usize = 32;
/// How long a document no isolate holds stays open without a call.
pub(crate) const IDLE: Duration = Duration::from_secs(15 * 60);
/// The most undo steps a document keeps (SERVICE.md "One undo step per
/// call"; the engine's own default is 500 snapshots of shared slides).
pub(crate) const MAX_UNDO: usize = 100;
/// `deck.create`'s most slides (the door's `OUTLINE_SLIDES` for an outline).
pub(crate) const MAX_CREATE_SLIDES: usize = 200;
/// A new slide's longest title, and its longest body or notes.
pub(crate) const MAX_TITLE: usize = 2_000;
pub(crate) const MAX_BODY: usize = 20_000;

/// What keeps a document open besides `deck.close`.
pub(crate) enum Binding {
    /// Opened from an app's isolate: open while that isolate's storage
    /// scope is.
    Isolate { heap: usize, scope: StorageAccess },
    /// Opened by a caller with no isolate: open until [`IDLE`] passes
    /// without a call.
    Unbound,
}

/// A render in the cache that shows this document: a slide, a slide at a
/// build step, or a layout picture.
pub(crate) struct Rendered {
    /// The storage-relative file.
    pub rel: String,
    /// The revision of what it shows ([`OpenDoc::slide_rev`] for a slide,
    /// [`OpenDoc::deck_rev`] for a layout).
    pub rev: u64,
    pub width: u64,
    pub height: u64,
    pub bytes: u64,
}

/// One open document.
pub(crate) struct OpenDoc {
    pub app: String,
    pub owner: PathBuf,
    pub handle: String,
    pub session: Session,
    /// The deck's own native file, relative to the area: what `deck.save`
    /// without a path writes. `None` for a deck opened from a `.pptx` or an
    /// outline, whose first save names a path.
    pub file: Option<String>,
    pub binding: Binding,
    pub last_used: Instant,
    /// Bumped by every change, undo and redo.
    pub rev: u64,
    /// The revision at which each slide last changed (absent: as opened).
    pub slide_revs: HashMap<u32, u64>,
    /// The revision of the last change to the whole deck (its theme, slide
    /// size, footer), which the layout pictures show.
    pub deck_rev: u64,
    /// Renders in the cache, by what they show (slide, mode and width).
    pub renders: HashMap<String, Rendered>,
    /// What the session holds, as the door's ceilings count it, kept per
    /// slide between calls ([`gate`]).
    pub held: HeldCache,
}

impl OpenDoc {
    /// The engine's document: a session here always holds exactly one.
    pub fn st(&self) -> Result<&DocState, String> {
        self.session.doc().map_err(|_| fail(Code::Damaged, "the engine no longer holds this document"))
    }

    pub fn st_mut(&mut self) -> Result<&mut DocState, String> {
        self.session.doc_mut().map_err(|_| fail(Code::Damaged, "the engine no longer holds this document"))
    }

    pub fn deck(&self) -> Result<Arc<Presentation>, String> {
        Ok(self.st()?.doc.clone())
    }

    /// Slide `id`'s position, or `unknown_id:`.
    pub fn slide_index(&self, id: u32) -> Result<usize, String> {
        let st = self.st()?;
        st.doc.slide_index(deckcraft_model::SlideId(id)).ok_or_else(|| unknown_slide(id))
    }

    /// The revision a render of slide `id` shows when it is current.
    pub fn slide_rev(&self, id: u32) -> u64 {
        self.slide_revs.get(&id).copied().unwrap_or(0)
    }

    /// `{edited, can_undo, can_redo}`.
    pub fn history(&self) -> Result<Json, String> {
        let st = self.st()?;
        Ok(json!({ "edited": st.is_dirty(), "can_undo": !st.history.undo.is_empty(), "can_redo": !st.history.redo.is_empty() }))
    }

    /// SERVICE.md's common answer of a change: `{rev, changed, edited,
    /// can_undo, can_redo}`.
    pub fn common(&self, changed: &[u32]) -> Result<Json, String> {
        let mut out = self.history()?;
        out["rev"] = json!(self.rev);
        out["changed"] = json!(changed);
        Ok(out)
    }

    /// The common answer with `extra`'s fields added.
    pub fn answer(&self, changed: &[u32], extra: Json) -> Result<Json, String> {
        let mut out = self.common(changed)?;
        if let Json::Object(map) = extra {
            for (k, v) in map {
                out[k] = v;
            }
        }
        Ok(out)
    }

    fn alive(&self, now: Instant) -> bool {
        match &self.binding {
            Binding::Isolate { heap, scope } => splash_storage::storage_for_heap(*heap, &self.app).is_some_and(|now| now.same_scope(scope)),
            Binding::Unbound => now.saturating_duration_since(self.last_used) < IDLE,
        }
    }

    fn bound_to(&self, closed: &[usize]) -> bool {
        matches!(&self.binding, Binding::Isolate { heap, .. } if closed.contains(heap))
    }

    /// The document goes: its engine session, and its renders in the
    /// service's cache.
    fn release(self) {
        cache::forget(&Area::new(self.owner.clone(), None, false), &self.handle);
    }
}

pub(crate) fn unknown_slide(id: u32) -> String {
    fail(Code::UnknownId, format!("the deck has no slide {id} (an undo may have removed it)"))
}

/// A session for an app's document (module doc): text is stored as typed.
pub(crate) fn app_session() -> Session {
    let mut s = Session::new();
    s.prefs.autocorrect = false;
    s.prefs.smart_quotes = false;
    s
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
    /// The open documents of this thread: the UI thread's (module doc).
    static DOCS: RefCell<Docs> = RefCell::new(Docs::default());
    /// Heap keys of isolates that closed while a call held [`DOCS`]: the
    /// next [`sweep`] releases their documents. A `Cell`, which a listener
    /// can always fill.
    static CLOSED: Cell<Vec<usize>> = const { Cell::new(Vec::new()) };
}

/// App Hub's `services::on_isolate_closed` listener ([`crate::register`]):
/// the isolate whose heap key is `heap` closed, so every document bound to
/// it goes now, with its renders, before a new isolate can get the key. If a
/// call holds the table (a listener reached from inside one), the key waits
/// for the next [`sweep`].
pub(crate) fn isolate_closed(heap: usize) {
    let released = DOCS.with(|docs| {
        let mut docs = docs.try_borrow_mut().ok()?;
        let (gone, kept): (Vec<OpenDoc>, Vec<OpenDoc>) = std::mem::take(&mut docs.open).into_iter().partition(|doc| doc.bound_to(&[heap]));
        docs.open = kept;
        Some(gone)
    });
    match released {
        // The engine sessions drop and the renders go outside the table.
        Some(gone) => gone.into_iter().for_each(OpenDoc::release),
        None => CLOSED.with(|closed| {
            let mut keys = closed.take();
            keys.push(heap);
            closed.set(keys);
        }),
    }
}

/// Whose documents a call reaches: its area's root as the file system
/// spells it (two spellings of one folder are one scope).
pub(crate) fn owner(area: &Area) -> PathBuf {
    area.root.canonicalize().unwrap_or_else(|_| area.root.clone())
}

/// A new handle: lowercase hex, unique in this process, and unlike any of
/// an earlier run (it names the document's render folder, which may still
/// be there).
fn mint() -> String {
    static SEED: OnceLock<u32> = OnceLock::new();
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let seed = *SEED.get_or_init(|| {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        (nanos as u32) ^ (nanos >> 32) as u32 ^ std::process::id().rotate_left(16)
    });
    format!("{seed:08x}{:x}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// The handle a call names: what `deck.open` or `deck.create` returned.
/// Anything else names no document.
fn handle(args: &Json, m: &str) -> Result<String, String> {
    match args.get("doc") {
        None | Some(Json::Null) => Err(invalid(format!("{m} needs `doc`, the handle deck.open returned"))),
        Some(Json::String(h)) if !h.is_empty() && h.len() <= 40 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) => Ok(h.clone()),
        Some(_) => Err(unknown()),
    }
}

fn unknown() -> String {
    fail(Code::UnknownDoc, "no deck of this app is open under that handle: open it again")
}

/// Release the documents of isolates that closed while a call held the
/// table, and those whose caller's scope went away (module doc).
pub(crate) fn sweep() {
    sweep_at(Instant::now());
}

pub(crate) fn sweep_at(now: Instant) {
    let closed = CLOSED.with(Cell::take);
    let gone: Vec<OpenDoc> = DOCS.with(|docs| {
        let mut docs = docs.borrow_mut();
        let (gone, kept): (Vec<OpenDoc>, Vec<OpenDoc>) = std::mem::take(&mut docs.open).into_iter().partition(|doc| doc.bound_to(&closed) || !doc.alive(now));
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

/// Run `f` on the document `args.doc` names, if this caller has it open. A
/// panic in the engine closes the document rather than leave it half
/// changed.
pub(crate) fn with_doc<T>(args: &Json, cx: &Ctx, m: &str, f: impl FnOnce(&mut OpenDoc) -> Result<T, String>) -> Result<T, String> {
    let handle = handle(args, m)?;
    let owner = owner(cx.area);
    let (result, dropped) = DOCS.with(|docs| {
        let mut docs = docs.borrow_mut();
        let Some(i) = docs.find(cx.app, &owner, &handle) else { return (Err(unknown()), None) };
        docs.open[i].last_used = Instant::now();
        match catch_unwind(AssertUnwindSafe(|| f(&mut docs.open[i]))) {
            Ok(result) => (result, None),
            Err(_) => {
                let doc = docs.open.remove(i);
                (Err(fail(Code::Damaged, "the deck engine failed on this document, so it was closed: open it again")), Some(doc))
            }
        }
    });
    if let Some(doc) = dropped {
        doc.release();
    }
    result
}

/// What keeps a document this call opens open (module doc).
fn bind(cx: &Ctx) -> Binding {
    match splash_storage::storage_for_heap(cx.isolate, cx.app) {
        Some(scope) => Binding::Isolate { heap: cx.isolate, scope },
        None => Binding::Unbound,
    }
}

/// Refuse a new document when the caller, or the service, holds the most.
fn room_for_one(cx: &Ctx, owner: &Path) -> Result<(), String> {
    DOCS.with(|docs| {
        let docs = docs.borrow();
        if docs.open.iter().filter(|d| d.app == cx.app && d.owner == owner).count() >= MAX_OPEN {
            return Err(fail(Code::TooManyOpen, format!("{MAX_OPEN} decks are open already: close one first")));
        }
        if docs.open.len() >= MAX_OPEN_ALL {
            return Err(fail(Code::TooManyOpen, "the deck engine holds as many decks as it can: close one first"));
        }
        Ok(())
    })
}

/// Add `doc` to the table, and clear the renders of documents no longer
/// open in its storage.
fn admit(doc: OpenDoc, area: &Area) {
    let (app, owner) = (doc.app.clone(), doc.owner.clone());
    let open: Vec<String> = DOCS.with(|docs| {
        let mut docs = docs.borrow_mut();
        docs.open.push(doc);
        docs.open.iter().filter(|d| d.app == app && d.owner == owner).map(|d| d.handle.clone()).collect()
    });
    cache::prune(area, &open);
}

/// A fresh open document around `session` (which holds its deck).
fn new_doc(cx: &Ctx, owner: PathBuf, session: Session, file: Option<String>) -> Result<OpenDoc, String> {
    let mut doc = OpenDoc {
        app: cx.app.to_string(),
        owner,
        handle: mint(),
        session,
        file,
        binding: bind(cx),
        last_used: Instant::now(),
        rev: 0,
        slide_revs: HashMap::new(),
        deck_rev: 0,
        renders: HashMap::new(),
        held: HeldCache::default(),
    };
    // The service keeps the undo steps itself (crate::gate).
    doc.st_mut()?.history.limit = usize::MAX;
    Ok(doc)
}

/// `document.inspect {}`: `safe`, a read of the open document (the one
/// command `deck.open`, `deck.create`'s answer and `deck.state` run).
const INSPECT: usize = 1;

/// `deck.open {path}` and `deck.create`'s answer: the deck as the app shows
/// it on opening.
fn opened(doc: &mut OpenDoc, cx: &Ctx, m: &'static str, path: &str) -> Result<Json, String> {
    let inspected = gate::read(doc, cx.area, Plan { method: m, most: INSPECT, cmds: vec![("document.inspect", json!({}))] })?.remove(0);
    let deck = doc.deck()?;
    let stem = Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let title = Some(deck.props.title.trim()).filter(|t| !t.is_empty()).map(str::to_owned).unwrap_or(stem);
    let layouts: Vec<Json> = deck
        .masters
        .first()
        .map(|m| m.layouts.iter().map(|l| json!({ "id": l.id.0, "name": l.name, "kind": l.kind.xml() })).collect())
        .unwrap_or_default();
    let design = design_state(&deck);
    let mut out = doc.common(&[])?;
    for (k, v) in [
        ("doc", json!(doc.handle)),
        ("path", json!(path)),
        ("title", json!(title)),
        ("size", inspected["slideSize"].clone()),
        ("slides", json!(deck.slides.len())),
        ("theme", inspected["theme"].clone()),
        ("color_scheme", design["color_scheme"].clone()),
        ("font_scheme", design["font_scheme"].clone()),
        ("footer", design["footer"].clone()),
        ("layouts", json!(layouts)),
        ("sections", sections(&deck)),
        ("missing_fonts", json!(missing_fonts(&deck))),
    ] {
        out[k] = v;
    }
    if let Some(map) = out.as_object_mut() {
        map.remove("changed");
    }
    Ok(out)
}

/// The deck's sections: each one's name, the position of its first slide
/// and how many slides it holds.
pub(crate) fn sections(deck: &Presentation) -> Json {
    let mut at = 0usize;
    let list: Vec<Json> = deck
        .sections
        .iter()
        .map(|s| {
            let first = s.slides.iter().filter_map(|id| deck.slide_index(*id)).min().unwrap_or(at);
            let count = s.slides.iter().filter(|id| deck.slide_index(**id).is_some()).count();
            at = first + count;
            json!({ "name": s.name, "first": first, "count": count })
        })
        .collect();
    json!(list)
}

/// The font families the deck names (its themes' and its text runs') that
/// the engine draws with a substitute: they are not installed and not
/// built in.
pub(crate) fn missing_fonts(deck: &Presentation) -> Vec<String> {
    let mut named: Vec<String> = Vec::new();
    let mut add = |f: &str| {
        let f = f.trim();
        if !f.is_empty() && !f.starts_with('+') && !named.iter().any(|n| n.eq_ignore_ascii_case(f)) {
            named.push(f.to_string());
        }
    };
    for m in &deck.masters {
        add(&m.theme.fonts.major.latin);
        add(&m.theme.fonts.minor.latin);
    }
    let mut runs = |shapes: &[deckcraft_model::Shape]| {
        deckcraft_model::walk(shapes, &mut |sh, _| {
            for p in sh.text.iter().flat_map(|t| &t.paragraphs) {
                for r in &p.runs {
                    if let Some(f) = &r.props.font {
                        add(f);
                    }
                }
            }
        });
    };
    for slide in &deck.slides {
        runs(&slide.shapes);
    }
    let fonts = deckcraft_fonts::FontDb::global();
    named.retain(|f| !fonts.has_family(f));
    named.sort_by_key(|f| f.to_lowercase());
    named
}

/// `deck.open {path}`.
pub(crate) fn open(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.open";
    args::only(a, &["path"], M)?;
    let path = args::need_str(a, "path", M)?;
    let owner = owner(cx.area);
    room_for_one(cx, &owner)?;
    let (rel, deck, format) = crate::read_guarded(cx.area, path, "path", M)?;
    let mut session = app_session();
    engine_file::add_opened(&mut session, &rel, None, deck);
    if let Some(why) = crate::Held::of(&session).over() {
        return Err(crate::too_large_deck(why));
    }
    let file = (format == Format::Native).then(|| rel.clone());
    let mut doc = new_doc(cx, owner, session, file)?;
    let answer = opened(&mut doc, cx, M, &rel)?;
    admit(doc, cx.area);
    Ok(answer)
}

/// `deck.create`'s most commands: the `file.new`, then a `slide.new` and its
/// `slide.notes` for each of at most [`MAX_CREATE_SLIDES`] slides.
const CREATE: usize = 1 + 2 * MAX_CREATE_SLIDES;

/// The engine's name of a theme a call names (its own spelling, any case).
pub(crate) fn theme_name(name: &str, m: &str) -> Result<String, String> {
    let themes = deckcraft_model::theme::builtin_themes();
    themes
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(name) || (name.eq_ignore_ascii_case("default") && t.name == "DeckCraft"))
        .map(|t| t.name.clone())
        .ok_or_else(|| invalid(format!("{m}: `theme` is one of {}", themes.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", "))))
}

/// A layout a call names: an id from `deck.open`, or a name or kind the
/// engine resolves (`slide.new`, `slide.layout`).
pub(crate) fn layout_value(v: &Json, m: &str) -> Result<Json, String> {
    match v {
        Json::String(s) if !s.is_empty() && s.chars().count() <= 128 => Ok(json!(s)),
        v => match args::whole(v).and_then(|n| u32::try_from(n).ok()) {
            Some(id) => Ok(json!(id)),
            None => Err(invalid(format!("{m}: `layout` is a layout id from deck.open, or a layout name"))),
        },
    }
}

/// `deck.create {out, theme?, size?, slides?, outline?}`.
pub(crate) fn create(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.create";
    args::only(a, &["out", "theme", "size", "slides", "outline"], M)?;
    let out = args::need_str(a, "out", M)?;
    if !out.to_ascii_lowercase().ends_with(".deckcraft") {
        return Err(invalid(format!("{M}: `out` is a new .deckcraft file")));
    }
    // A new file in storage, never over one, foreground or not.
    let dest = crate::new_file(cx.area, out, "out", M)?;
    let theme = match args::opt_str(a, "theme", M, 64)? {
        Some(t) => theme_name(t, M)?,
        None => "DeckCraft".into(),
    };
    let [w, h] = args::opt_size(a, "size", M)?.unwrap_or([960.0, 540.0]);
    let mut cmds: Vec<(&'static str, Json)> = vec![("file.new", json!({ "theme": theme, "size": [w, h], "blank": true }))];
    match (args::present(a, "slides"), args::opt_str(a, "outline", M, 200_000)?) {
        (Some(_), Some(_)) => return Err(invalid(format!("{M} takes `slides` or `outline`, not both"))),
        (Some(slides), None) => {
            let slides = slides.as_array().ok_or_else(|| invalid(format!("{M}: `slides` is a list of {{layout?, title?, body?, notes?}}")))?;
            if slides.is_empty() {
                return Err(invalid(format!("{M}: `slides` names at least one slide")));
            }
            if slides.len() > MAX_CREATE_SLIDES {
                return Err(too_large(format!("{M}: `slides` is at most {MAX_CREATE_SLIDES} slides")));
            }
            for (i, slide) in slides.iter().enumerate() {
                let sm = "deck.create slides";
                args::only(slide, &["layout", "title", "body", "notes"], sm)?;
                let mut params = json!({ "at": i });
                if let Some(layout) = args::present(slide, "layout") {
                    params["layout"] = layout_value(layout, sm)?;
                }
                if let Some(title) = args::opt_str(slide, "title", sm, MAX_TITLE)? {
                    params["title"] = json!(title);
                }
                if let Some(body) = args::opt_str(slide, "body", sm, MAX_BODY)? {
                    params["body"] = json!(body);
                }
                cmds.push(("slide.new", params));
                if let Some(notes) = args::opt_str(slide, "notes", sm, MAX_BODY)? {
                    cmds.push(("slide.notes", json!({ "text": notes, "index": i })));
                }
            }
        }
        (None, Some(outline)) => cmds.push(("slide.fromOutline", json!({ "text": outline }))),
        (None, None) => cmds.push(("slide.new", json!({ "at": 0, "layout": "title" }))),
    }
    let owner = owner(cx.area);
    room_for_one(cx, &owner)?;
    let mut session = app_session();
    gate::exec(&mut session, cx.area, &Plan { method: M, most: CREATE, cmds }, &mut HeldCache::default())?;
    let st = session.doc_mut().map_err(|_| fail(Code::Damaged, "the engine made no deck"))?;
    if st.doc.slides.is_empty() {
        return Err(invalid(format!("{M}: the outline has no slide (a slide is an unindented line)")));
    }
    // Written there as made: the deck's own file from now on.
    let rel = crate::normal(out);
    crate::writing::write_new(cx.area, &dest, &engine_file::save_bytes(&st.doc, "deckcraft").map_err(|e| fail(Code::Damaged, format!("{M}: {e}")))?, M)?;
    st.saved_doc = st.doc.clone();
    st.history.undo.clear();
    st.history.redo.clear();
    let mut doc = new_doc(cx, owner, session, Some(rel.clone()))?;
    let answer = opened(&mut doc, cx, M, &rel)?;
    admit(doc, cx.area);
    Ok(answer)
}

/// `deck.close {doc, discard?}`.
pub(crate) fn close(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.close";
    args::only(a, &["doc", "discard"], M)?;
    let discard = args::opt_bool(a, "discard", M)?.unwrap_or(false);
    let handle = handle(a, M)?;
    let owner = owner(cx.area);
    let doc = DOCS.with(|docs| {
        let mut docs = docs.borrow_mut();
        let i = docs.find(cx.app, &owner, &handle).ok_or_else(unknown)?;
        let edited = docs.open[i].st().map(DocState::is_dirty).unwrap_or(false);
        if edited && !discard {
            return Err(fail(Code::Unsaved, "the deck has changes that are not saved: save it first, or close it with discard: true"));
        }
        Ok(docs.open.remove(i))
    })?;
    doc.release();
    Ok(json!({ "closed": true }))
}

/// `deck.state {doc}`.
pub(crate) fn state(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.state";
    args::only(a, &["doc"], M)?;
    with_doc(a, cx, M, |doc| {
        let inspected = gate::read(doc, cx.area, Plan { method: M, most: INSPECT, cmds: vec![("document.inspect", json!({}))] })?.remove(0);
        let mut out = doc.history()?;
        out["rev"] = json!(doc.rev);
        out["undo"] = inspected["undo"].clone();
        out["redo"] = inspected["redo"].clone();
        out["slides"] = json!(inspected["slides"].as_array().map_or(0, Vec::len));
        let deck = doc.deck()?;
        let design = design_state(&deck);
        out["theme"] = json!(deck.masters.first().map(|m| m.theme.name.clone()));
        for key in ["color_scheme", "font_scheme", "footer"] {
            out[key] = design[key].clone();
        }
        Ok(out)
    })
}

/// The deck's design as the inspector shows it, read from the model: the
/// first master's colour and font scheme names (the master
/// `document.inspect` reports the theme of), and the deck's header and
/// footer settings in the keys `deck.design {footer}` takes.
pub(crate) fn design_state(deck: &Presentation) -> Json {
    let master = deck.masters.first();
    let hf = &deck.header_footer;
    json!({
        "color_scheme": master.map(|m| m.theme.colors.name.clone()),
        "font_scheme": master.map(|m| m.theme.fonts.name.clone()),
        "footer": {
            "slide_numbers": hf.slide_number,
            "footer": hf.footer,
            "footer_text": hf.footer_text,
            "date": hf.date,
            "hide_on_title": hf.hide_on_title,
        },
    })
}
