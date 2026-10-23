//! One gate for the app's methods (SERVICE.md "One gate", decisions 2 and
//! 3): how a method's engine commands reach an open document.
//!
//! - **Admitted as a whole, before any runs.** A method builds every engine
//!   command and its parameters itself, from reviewed values ([`Plan`]),
//!   and the same door as `deck.run`'s (`crate::door`, from
//!   `skill/safety.json` and `REVIEWED`) admits the whole list with
//!   `Door::admit_method`: the allowlist, the held commands (#448), the
//!   reviewed reads inside the area and every `Limit`. `deck.run`'s 64
//!   commands a call are `deck.run`'s; each method has its own most
//!   commands a call ([`Plan::most`]), with its reason beside its constant.
//!   A refused command refuses the call with nothing run.
//! - **The ceilings after every command.** The door's `RunBudget` ceilings
//!   on the session (slides, shapes, cells, chart points, characters,
//!   media, geometry, and what one call's edits copy) hold after each
//!   command; the session's counts are kept per slide between calls
//!   ([`HeldCache`]), so a check costs what changed, not the whole deck.
//! - **All or nothing.** A command the engine refuses, or one that crosses
//!   a ceiling, puts the document back as it was before the call: its deck,
//!   selection and undo and redo history.
//! - **One undo step per call** (decision 3): the engine makes each command
//!   its own undo step; after a change method runs, the steps it pushed are
//!   folded into one through the engine's public `History`, labelled by the
//!   action, and the history keeps at most [`docs::MAX_UNDO`] steps.
//! - **`rev` and `changed`.** A change, an undo and a redo bump the
//!   document's `rev`; `changed` lists the slides whose content changed,
//!   found by comparing the engine's copy-on-write slide pointers: a new or
//!   replaced slide, every slide after a deck-wide change (a master or its
//!   theme, the slide size, the header and footer), and a slide whose
//!   position changed when it shows its number.
//! - **A read never changes the document**: whatever a read's commands
//!   changed goes back.

use std::collections::HashMap;
use std::sync::Arc;

use deckcraft_engine::{HistoryEntry, Selection, Session};
use deckcraft_model::text::RunKind;
use deckcraft_model::{Master, Presentation, Shape, Slide};
use octosense_engine_area::Area;
use serde_json::{json, Value as Json};

use crate::codes::{self, fail, invalid, too_large, Code};
use crate::docs::{self, OpenDoc};
use crate::{Held, RunBudget};

/// One method's engine work.
pub(crate) struct Plan {
    /// The method, as refusals name it.
    pub method: &'static str,
    /// The most commands this method runs in one call (its reason is beside
    /// the method's constant).
    pub most: usize,
    /// The commands, in order, with the parameters the service built.
    pub cmds: Vec<(&'static str, Json)>,
}

/// What the session holds, as the door's ceilings count it ([`Held`]), kept
/// per slide and per master between calls: a slide the engine did not
/// replace (copy-on-write) is counted once. Each entry holds its slide, so
/// its address cannot be reused while it is kept.
#[derive(Default)]
pub(crate) struct HeldCache {
    slides: HashMap<usize, (Arc<Slide>, Held)>,
    masters: HashMap<usize, (Arc<Master>, Held)>,
}

impl Held {
    /// What one slide holds, as [`Held::of`] counts it.
    fn of_slide(slide: &Slide) -> Held {
        let mut held = Held::default();
        held.add_shapes(&slide.shapes);
        held.most_on_a_slide = held.shapes;
        held.add_text(&slide.notes);
        held
    }

    fn of_master(master: &Master) -> Held {
        let mut held = Held::default();
        held.add_master(master);
        held
    }

    /// `part` added to `self`, in [`Held::of`]'s order (its first problem
    /// stays the first).
    fn absorb(&mut self, part: &Held) {
        self.shapes += part.shapes;
        self.most_on_a_slide = self.most_on_a_slide.max(part.most_on_a_slide);
        self.cells += part.cells;
        self.points += part.points;
        self.chars += part.chars;
        if self.problem.is_none() {
            self.problem.clone_from(&part.problem);
        }
    }
}

impl HeldCache {
    /// What `s` holds now: [`Held::of`], counting only what changed since
    /// the last call.
    pub(crate) fn held(&mut self, s: &Session) -> Held {
        let mut total = Held::default();
        let mut slides = HashMap::new();
        let mut masters = HashMap::new();
        for st in s.documents() {
            let doc = &st.doc;
            total.slides += doc.slides.len();
            total.media += doc.media.iter().map(|m| m.data.len() as u64).sum::<u64>();
            for slide in &doc.slides {
                let key = Arc::as_ptr(slide) as usize;
                let entry = self.slides.remove(&key).or_else(|| slides.remove(&key)).unwrap_or_else(|| (slide.clone(), Held::of_slide(slide)));
                total.absorb(&entry.1);
                slides.insert(key, entry);
            }
            for master in doc.masters.iter().chain(&doc.notes_master).chain(&doc.handout_master) {
                let key = Arc::as_ptr(master) as usize;
                let entry = self.masters.remove(&key).or_else(|| masters.remove(&key)).unwrap_or_else(|| (master.clone(), Held::of_master(master)));
                total.absorb(&entry.1);
                masters.insert(key, entry);
            }
        }
        self.slides = slides;
        self.masters = masters;
        total
    }
}

/// Admit `plan` as a whole, then run it on `session` in order, with the
/// ceilings checked after every command (module doc). The results, in
/// order.
pub(crate) fn exec(session: &mut Session, area: &Area, plan: &Plan, held: &mut HeldCache) -> Result<Vec<Json>, String> {
    let door = crate::door().map_err(|e| fail(Code::Damaged, e))?;
    let admitted = door.admit_method(plan.method, &plan.cmds, plan.most, area, crate::UNREAD).map_err(|r| codes::refusal(&r))?;
    let named = |e: String| too_large(e.replacen("deck.run", plan.method, 1));
    let now = held.held(session);
    if let Some(why) = now.over() {
        return Err(too_large(format!("{}: the deck holds {why}, so the service runs no commands on it", plan.method)));
    }
    let mut budget = RunBudget::fresh(session);
    let mut results = Vec::with_capacity(admitted.len());
    for (id, params) in admitted {
        budget.before(session, &id, &params).map_err(named)?;
        let result = session.execute(&id, &params).map_err(|e| codes::engine(plan.method, &id, &e))?;
        budget.after_held(held.held(session), session, &id, &result).map_err(named)?;
        results.push(result);
    }
    Ok(results)
}

/// The document as it was before a call.
struct Before {
    doc: Arc<Presentation>,
    selection: Selection,
    undo: usize,
    redo: Vec<HistoryEntry>,
}

impl Before {
    fn take(doc: &OpenDoc) -> Result<Before, String> {
        let st = doc.st()?;
        Ok(Before { doc: st.doc.clone(), selection: st.selection.clone(), undo: st.history.undo.len(), redo: st.history.redo.clone() })
    }

    /// Put the document back as it was.
    fn restore(self, doc: &mut OpenDoc) {
        if let Ok(st) = doc.st_mut() {
            st.doc = self.doc;
            st.selection = self.selection;
            st.history.undo.truncate(self.undo);
            st.history.redo = self.redo;
            st.interaction = None;
        }
    }
}

/// What a change method did.
pub(crate) struct Change {
    pub results: Vec<Json>,
    /// The slides whose content changed.
    pub changed: Vec<u32>,
}

/// Run a change method's `plan` on `doc` (module doc): all or nothing, one
/// undo step labelled `label`.
pub(crate) fn change(doc: &mut OpenDoc, area: &Area, plan: Plan, label: &str) -> Result<Change, String> {
    let before = Before::take(doc)?;
    let results = match exec(&mut doc.session, area, &plan, &mut doc.held) {
        Ok(results) => results,
        Err(e) => {
            before.restore(doc);
            return Err(e);
        }
    };
    let st = doc.st_mut()?;
    if Arc::ptr_eq(&st.doc, &before.doc) || same_content(&before.doc, &st.doc) {
        // Nothing changed (the engine's copies aside): the deck as it was, no
        // undo step, no new revision.
        let selection = st.selection.clone();
        before.restore(doc);
        doc.st_mut()?.selection = selection;
        return Ok(Change { results, changed: Vec::new() });
    }
    st.history.undo.truncate(before.undo);
    st.history.undo.push(HistoryEntry { label: label.to_string(), doc: before.doc.clone(), selection: before.selection.clone() });
    st.history.redo.clear();
    let excess = st.history.undo.len().saturating_sub(docs::MAX_UNDO);
    st.history.undo.drain(..excess);
    let after = st.doc.clone();
    let changed = advance(doc, &before.doc, &after);
    Ok(Change { results, changed })
}

/// Run a read method's `plan` on `doc`: the results, with the document as
/// it was.
pub(crate) fn read(doc: &mut OpenDoc, area: &Area, plan: Plan) -> Result<Vec<Json>, String> {
    let before = Before::take(doc)?;
    let results = exec(&mut doc.session, area, &plan, &mut doc.held);
    let same = doc.st().is_ok_and(|st| Arc::ptr_eq(&st.doc, &before.doc));
    if !same || results.is_err() {
        before.restore(doc);
    }
    results
}

/// `edit.undo {}` or `edit.redo {}` (one folded step): the slides whose
/// content changed.
pub(crate) fn step(doc: &mut OpenDoc, area: &Area, redo: bool) -> Result<Vec<u32>, String> {
    let (m, id) = if redo { ("deck.redo", "edit.redo") } else { ("deck.undo", "edit.undo") };
    {
        let st = doc.st()?;
        let empty = if redo { st.history.redo.is_empty() } else { st.history.undo.is_empty() };
        if empty {
            return Err(invalid(format!("{m}: nothing to {}", if redo { "redo" } else { "undo" })));
        }
    }
    let before = Before::take(doc)?;
    // `edit.undo` and `edit.redo` are `safe`: one command, the history's own.
    if let Err(e) = exec(&mut doc.session, area, &Plan { method: m, most: 1, cmds: vec![(id, json!({}))] }, &mut doc.held) {
        before.restore(doc);
        return Err(e);
    }
    let after = doc.st()?.doc.clone();
    Ok(advance(doc, &before.doc, &after))
}

/// A new revision of `doc`, from `before` to `after`: the slides whose
/// content changed get it, and the deck does when the change was deck-wide.
fn advance(doc: &mut OpenDoc, before: &Presentation, after: &Presentation) -> Vec<u32> {
    let (changed, deck_wide) = changed_slides(before, after);
    doc.rev += 1;
    for id in &changed {
        doc.slide_revs.insert(*id, doc.rev);
    }
    if deck_wide {
        doc.deck_rev = doc.rev;
    }
    changed
}

/// The slides of `after` whose content changed since `before`, and whether
/// the change was deck-wide (module doc). A slide or master whose pointer
/// changed is compared by content too: the engine copies the current slide
/// on every edit (`Session::edit` reroutes its connectors through
/// `Arc::make_mut`), changed or not.
pub(crate) fn changed_slides(before: &Presentation, after: &Presentation) -> (Vec<u32>, bool) {
    let same_master = |a: &Arc<Master>, b: &Arc<Master>| Arc::ptr_eq(a, b) || **a == **b;
    let deck_wide = before.masters.len() != after.masters.len()
        || before.masters.iter().zip(&after.masters).any(|(a, b)| !same_master(a, b))
        || before.slide_size != after.slide_size
        || before.header_footer != after.header_footer
        || before.first_slide_number != after.first_slide_number
        || before.default_text_style != after.default_text_style;
    if deck_wide {
        return (after.slides.iter().map(|s| s.id.0).collect(), true);
    }
    let old: HashMap<u32, (&Arc<Slide>, usize)> = before.slides.iter().enumerate().map(|(i, s)| (s.id.0, (s, i))).collect();
    let numbered_by_masters = || after.masters.iter().any(|m| has_number(m.shapes.iter().filter(|s| s.ph.is_none())) || m.layouts.iter().any(|l| has_number(l.shapes.iter().filter(|s| s.ph.is_none()))));
    let mut by_masters: Option<bool> = None;
    let mut out = Vec::new();
    for (i, s) in after.slides.iter().enumerate() {
        let id = s.id.0;
        match old.get(&id) {
            Some((prev, at)) if Arc::ptr_eq(prev, s) || ***prev == **s => {
                if *at != i && (has_number(s.shapes.iter()) || *by_masters.get_or_insert_with(numbered_by_masters)) {
                    out.push(id);
                }
            }
            _ => out.push(id),
        }
    }
    (out, false)
}

/// Whether `after` holds what `before` held: every slide and master the same
/// (by pointer, else by content), and the rest of the deck equal. Its next
/// free id is left out: the engine takes ids without using them.
fn same_content(before: &Presentation, after: &Presentation) -> bool {
    let slides = before.slides.len() == after.slides.len() && before.slides.iter().zip(&after.slides).all(|(a, b)| Arc::ptr_eq(a, b) || **a == **b);
    let masters = before.masters.len() == after.masters.len() && before.masters.iter().zip(&after.masters).all(|(a, b)| Arc::ptr_eq(a, b) || **a == **b);
    slides
        && masters
        && before.notes_master == after.notes_master
        && before.handout_master == after.handout_master
        && before.slide_size == after.slide_size
        && before.notes_size == after.notes_size
        && before.sections == after.sections
        && before.custom_shows == after.custom_shows
        && before.show == after.show
        && before.header_footer == after.header_footer
        && before.props == after.props
        && before.default_text_style == after.default_text_style
        && before.media == after.media
        && before.first_slide_number == after.first_slide_number
        && before.embedded_fonts == after.embedded_fonts
        && before.raw_ext == after.raw_ext
}

/// Whether any of `shapes` (groups too) draws a slide-number field.
fn has_number<'a>(shapes: impl Iterator<Item = &'a Shape>) -> bool {
    let mut found = false;
    for shape in shapes {
        deckcraft_model::walk(std::slice::from_ref(shape), &mut |sh, _| {
            if !found {
                found = sh.text.iter().flat_map(|t| &t.paragraphs).flat_map(|p| &p.runs).any(|r| matches!(&r.kind, RunKind::Field { field } if field == "slidenum"));
            }
        });
        if found {
            return true;
        }
    }
    false
}
