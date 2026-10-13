//! Slides, notes, transitions, the deck's design, and its history
//! (SERVICE.md "Slides", "Design", "History and saving").
//!
//! The safety review of the engine commands behind them, by their class in
//! `skill/safety.json`:
//!
//! - `design.background` is `file`, because a non-string `picture` makes it
//!   read a `path`: the door refuses it from `deck.run`. `deck.design` runs
//!   it with `color`, `gradient` or `reset` and `all` or `index` only, built
//!   by the service; it is one of the service's `UNREAD` commands, which the
//!   door admits for a method only when the call names neither `picture`
//!   nor `path` (`octosense_engine_area::door::Unread`). A background is
//!   never a picture.
//! - Everything else here is `safe` and passes the door with its limits:
//!   `slide.new`, `slide.duplicate`, `slide.delete`, `slide.move`,
//!   `slide.hide`, `slide.layout`, `slide.reset`, `slide.notes`, `slide.go`
//!   and `slide.selectSlides` (the selection only: `slide.hide`,
//!   `slide.layout` and `transition.set` then act on every slide selected,
//!   one command and one undo snapshot for all of them),
//!   `section.add`, `section.rename`, `section.remove` (never with its
//!   `slides: true`, which would delete the section's slides),
//!   `section.move`, `transition.set`, `transition.applyAll`,
//!   `design.theme`, `design.colors`, `design.fonts`, `design.slideSize`
//!   (within the door's 1920 × 1080 points of slide), `design.headerFooter`,
//!   `edit.undo` and `edit.redo`.

use serde_json::{json, Value as Json};

use crate::args;
use crate::codes::{invalid, too_large};
use crate::docs::{self, OpenDoc, MAX_BODY, MAX_TITLE};
use crate::gate::{self, Plan};
use crate::objects::gradient;
use crate::Ctx;

/// The most slides one call names (SERVICE.md "Slides": 500, the session's
/// own ceiling).
pub(crate) const MAX_SLIDES: usize = 500;
/// A slides op's most commands when it takes one a slide: `slide.duplicate`
/// (each copy after its original) and `slide.move`, one for each slide
/// named.
const PER_SLIDE: usize = MAX_SLIDES;
/// `slide.reset` acts on one slide at a time, after the `slide.go` that makes
/// it current: two commands a slide named. (The engine copies the current
/// slide on every edit; making each reset slide current first keeps a call
/// from copying another slide once per command.)
const RESET: usize = 2 * MAX_SLIDES;
/// `slide.selectSlides {ids}` (the selection only), then one command that
/// acts on every slide selected: `slide.hide`, `slide.layout`,
/// `transition.set`.
const SELECTED: usize = 2;
/// One command: `slide.new`, `slide.delete` (all the slides named, by id),
/// or a section's.
const ONE: usize = 1;
/// The longest notes (SERVICE.md "Caps": 20,000 characters).
pub(crate) const MAX_NOTES: usize = 20_000;
/// The longest section name.
const MAX_SECTION_NAME: usize = 256;

/// The positions of the slides `ids` names, in their order, or
/// `unknown_id:`.
fn positions(doc: &OpenDoc, ids: &[u32]) -> Result<Vec<usize>, String> {
    ids.iter().map(|id| doc.slide_index(*id)).collect()
}

/// The `slide.move {from, to}` steps that bring the slides `named` (in
/// their order) together at position `to` of the deck that results, the
/// rest keeping theirs: each named slide follows the one before it, the
/// first follows the slide that ends up before position `to`.
pub(crate) fn moves(order: &[u32], named: &[u32], to: usize) -> Vec<(usize, usize)> {
    let mut cur = order.to_vec();
    let rest: Vec<u32> = order.iter().filter(|id| !named.contains(id)).copied().collect();
    let mut prev = to.min(rest.len()).checked_sub(1).map(|k| rest[k]);
    let mut out = Vec::new();
    for &id in named {
        let Some(from) = cur.iter().position(|x| *x == id) else { continue };
        let target = match prev.and_then(|p| cur.iter().position(|x| *x == p)) {
            None => 0,
            Some(q) => (if q > from { q - 1 } else { q }) + 1,
        };
        if from != target {
            out.push((from, target));
            let moved = cur.remove(from);
            cur.insert(target, moved);
        }
        prev = Some(id);
    }
    out
}

/// `deck.slides_op {doc, op, …}`.
pub(crate) fn slides_op(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.slides_op";
    let op = args::need_str(a, "op", M)?.to_string();
    let only = |keys: &[&str]| {
        let mut all = vec!["doc", "op"];
        all.extend_from_slice(keys);
        args::only(a, &all, M)
    };
    match op.as_str() {
        "new" => only(&["at", "layout", "title", "body"])?,
        "duplicate" | "delete" | "reset" => only(&["slides"])?,
        "move" => only(&["slides", "to"])?,
        "hide" => only(&["slides", "hidden"])?,
        "layout" => only(&["slides", "layout"])?,
        "section_add" => only(&["at", "name"])?,
        "section_rename" => only(&["section", "name"])?,
        "section_remove" => only(&["section"])?,
        "section_move" => only(&["section", "to"])?,
        other => {
            return Err(invalid(format!(
                "{M}: `op` is new, duplicate, delete, move, hide, layout, reset, section_add, section_rename, section_remove or section_move, not {other:?}"
            )))
        }
    }
    let slides = |doc: &OpenDoc| -> Result<(Vec<u32>, Vec<usize>), String> {
        let ids = args::ids(a, "slides", M, MAX_SLIDES)?;
        let at = positions(doc, &ids)?;
        Ok((ids, at))
    };
    docs::with_doc(a, cx, M, |doc| {
        let deck = doc.deck()?;
        let n = deck.slides.len();
        let (cmds, most, label): (Vec<(&'static str, Json)>, usize, &str) = match op.as_str() {
            "new" => {
                let at = args::opt_index(a, "at", M)?.unwrap_or(n);
                if at > n {
                    return Err(invalid(format!("{M}: `at` is a position from 0 to {n}")));
                }
                let mut p = json!({ "at": at });
                if let Some(layout) = args::present(a, "layout") {
                    p["layout"] = docs::layout_value(layout, M)?;
                }
                if let Some(t) = args::opt_str(a, "title", M, MAX_TITLE)? {
                    p["title"] = json!(t);
                }
                if let Some(b) = args::opt_str(a, "body", M, MAX_BODY)? {
                    p["body"] = json!(b);
                }
                (vec![("slide.new", p)], ONE, "New slide")
            }
            "duplicate" => {
                let (_, mut at) = slides(doc)?;
                at.sort_unstable();
                // Each copy follows its original: the k-th original has k
                // copies before it by the time it is duplicated.
                (at.iter().enumerate().map(|(k, i)| ("slide.duplicate", json!({ "index": i + k }))).collect(), PER_SLIDE, "Duplicate slides")
            }
            "delete" => {
                let (ids, _) = slides(doc)?;
                if ids.len() >= n {
                    return Err(invalid(format!("{M}: a deck keeps at least one slide: deleting every slide is refused")));
                }
                (vec![("slide.delete", json!({ "ids": ids }))], ONE, "Delete slides")
            }
            "move" => {
                let (ids, _) = slides(doc)?;
                let to = args::need_index(a, "to", M)?;
                if to > n {
                    return Err(invalid(format!("{M}: `to` is a position from 0 to {n}")));
                }
                let order: Vec<u32> = deck.slides.iter().map(|s| s.id.0).collect();
                (moves(&order, &ids, to).into_iter().map(|(from, to)| ("slide.move", json!({ "from": from, "to": to }))).collect(), PER_SLIDE, "Move slides")
            }
            "hide" => {
                let (ids, _) = slides(doc)?;
                let hidden = args::need_bool(a, "hidden", M)?;
                let label = if hidden { "Hide slides" } else { "Show slides" };
                (vec![("slide.selectSlides", json!({ "ids": ids })), ("slide.hide", json!({ "hidden": hidden }))], SELECTED, label)
            }
            "layout" => {
                let (ids, _) = slides(doc)?;
                let layout = docs::layout_value(args::present(a, "layout").ok_or_else(|| invalid(format!("{M} needs `layout`")))?, M)?;
                (vec![("slide.selectSlides", json!({ "ids": ids })), ("slide.layout", json!({ "layout": layout }))], SELECTED, "Change layout")
            }
            "reset" => {
                let (_, at) = slides(doc)?;
                let cmds = at.iter().flat_map(|i| [("slide.go", json!({ "index": i })), ("slide.reset", json!({ "index": i }))]).collect();
                (cmds, RESET, "Reset slides")
            }
            "section_add" => {
                let at = args::need_index(a, "at", M)?;
                if at >= n {
                    return Err(invalid(format!("{M}: `at` is the position of the section's first slide, below {n}")));
                }
                let name = args::opt_str(a, "name", M, MAX_SECTION_NAME)?.unwrap_or("Untitled Section");
                (vec![("section.add", json!({ "name": name, "at": at }))], ONE, "Add section")
            }
            _ => {
                let section = args::need_index(a, "section", M)?;
                let count = deck.sections.len();
                if section >= count {
                    return Err(invalid(format!("{M}: the deck has {count} sections, so `section` is below {count}")));
                }
                match op.as_str() {
                    "section_rename" => {
                        let name = args::need_text(a, "name", M, MAX_SECTION_NAME)?;
                        (vec![("section.rename", json!({ "index": section, "name": name }))], ONE, "Rename section")
                    }
                    // The section goes; its slides stay, in the section
                    // before it.
                    "section_remove" => (vec![("section.remove", json!({ "index": section }))], ONE, "Remove section"),
                    _ => {
                        let to = args::need_index(a, "to", M)?;
                        if to >= count {
                            return Err(invalid(format!("{M}: `to` is a section position below {count}")));
                        }
                        (vec![("section.move", json!({ "index": section, "to": to }))], ONE, "Move section")
                    }
                }
            }
        };
        let done = gate::change(doc, cx.area, Plan { method: M, most, cmds }, label)?;
        let deck = doc.deck()?;
        let extra = match op.as_str() {
            "new" => json!({ "slide": done.results.first().and_then(|r| r["id"].as_u64()) }),
            "duplicate" => json!({ "slides": done.results.iter().filter_map(|r| r["ids"][0].as_u64()).collect::<Vec<_>>() }),
            o if o.starts_with("section_") => json!({ "sections": docs::sections(&deck) }),
            _ => json!({}),
        };
        doc.answer(&done.changed, extra)
    })
}

/// `slide.notes {text, index}`: one command.
const NOTES: usize = 1;

/// `deck.notes {doc, slide, text}`.
pub(crate) fn notes(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.notes";
    args::only(a, &["doc", "slide", "text"], M)?;
    let slide = args::id(a, "slide", M)?;
    let text = args::need_text(a, "text", M, MAX_NOTES)?;
    docs::with_doc(a, cx, M, |doc| {
        let i = doc.slide_index(slide)?;
        let done = gate::change(doc, cx.area, Plan { method: M, most: NOTES, cmds: vec![("slide.notes", json!({ "text": text, "index": i }))] }, "Edit notes")?;
        doc.answer(&done.changed, json!({}))
    })
}

/// For the slides named, `slide.selectSlides` and one `transition.set` on
/// them all; for every slide, `slide.go`, `transition.set` and
/// `transition.applyAll`.
const TRANSITION: usize = 3;

/// `deck.transition {doc, slides | all: true, kind, option?, duration?}`:
/// `kind` and `option` from the engine's transition catalog, `duration` in
/// milliseconds (at most the engine's 59,000).
pub(crate) fn transition(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.transition";
    args::only(a, &["doc", "slides", "all", "kind", "option", "duration"], M)?;
    let kind = args::need_str(a, "kind", M)?;
    let info = deckcraft_model::anim::TRANSITIONS
        .iter()
        .find(|t| t.0.eq_ignore_ascii_case(kind))
        .ok_or_else(|| invalid(format!("{M}: `kind` is a transition of the engine's catalog (none, fade, push, wipe, split, reveal, cover, morph…), not {kind:?}")))?;
    let option = args::opt_str(a, "option", M, 64)?;
    if let Some(o) = option.filter(|o| !info.4.contains(o)) {
        return Err(invalid(format!("{M}: {}'s `option` is one of {}, not {o:?}", info.0, info.4.join(", "))));
    }
    let duration = args::opt_int_in(a, "duration", M, 0, 59_000)?;
    let all = args::opt_bool(a, "all", M)?.unwrap_or(false);
    let ids = args::opt_ids(a, "slides", M, MAX_SLIDES)?;
    let mut params = json!({ "kind": info.0 });
    if let Some(o) = option {
        params["option"] = json!(o);
    }
    if let Some(d) = duration {
        params["duration"] = json!(d);
    }
    docs::with_doc(a, cx, M, |doc| {
        let cmds: Vec<(&'static str, Json)> = match (all, ids) {
            (true, None) => {
                let mut p = params.clone();
                p["index"] = json!(0);
                vec![("slide.go", json!({ "index": 0 })), ("transition.set", p), ("transition.applyAll", json!({}))]
            }
            (false, Some(ids)) => {
                positions(doc, &ids)?;
                vec![("slide.selectSlides", json!({ "ids": ids })), ("transition.set", params.clone())]
            }
            _ => return Err(invalid(format!("{M}: give `slides`, or `all: true`"))),
        };
        let done = gate::change(doc, cx.area, Plan { method: M, most: TRANSITION, cmds }, "Change transition")?;
        doc.answer(&done.changed, json!({}))
    })
}

/// The most masters a theme, colour or font change reaches in one call.
const MAX_MASTERS: usize = 16;
/// `deck.design`'s most commands: for each master its slides use, a
/// `slide.go` and `design.theme`, `design.colors` and `design.fonts`; one
/// `design.slideSize`; for each slide named a `slide.go` and its
/// `design.background` (or one `slide.go` and one for all); one
/// `design.headerFooter`.
const DESIGN: usize = 4 * MAX_MASTERS + 1 + 2 * MAX_SLIDES + 1;

/// `deck.design {doc, theme?, colors?, fonts?, size?, scale?, background?,
/// footer?}`: the theme, colours and fonts of every master the slides use;
/// `background` is `{color | gradient | reset: true, slides | all: true}`,
/// never a picture; `footer` is `{slide_numbers?, footer?, footer_text?,
/// date?, hide_on_title?}` on every slide.
pub(crate) fn design(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.design";
    args::only(a, &["doc", "theme", "colors", "fonts", "size", "scale", "background", "footer"], M)?;
    let theme = match args::opt_str(a, "theme", M, 64)? {
        Some(t) => Some(docs::theme_name(t, M)?),
        None => None,
    };
    let named = |key: &str, names: Vec<String>| -> Result<Option<String>, String> {
        match args::opt_str(a, key, M, 64)? {
            Some(n) => names.iter().find(|x| x.eq_ignore_ascii_case(n)).cloned().map(Some).ok_or_else(|| invalid(format!("{M}: `{key}` is one of {}", names.join(", ")))),
            None => Ok(None),
        }
    };
    let colors = named("colors", deckcraft_model::theme::builtin_color_schemes().into_iter().map(|c| c.name).collect())?;
    let fonts = named("fonts", deckcraft_model::theme::builtin_font_schemes().into_iter().map(|f| f.name).collect())?;
    let size = args::opt_size(a, "size", M)?;
    let scale = match args::opt_choice(a, "scale", M, &["fit", "maximize"])? {
        Some("maximize") => "maximize",
        Some(_) => "ensureFit",
        None => "ensureFit",
    };
    if args::present(a, "scale").is_some() && size.is_none() {
        return Err(invalid(format!("{M}: `scale` goes with `size`")));
    }
    // The background: what, and where.
    let background = match args::present(a, "background") {
        Some(bg) => {
            let bm = "deck.design background";
            args::only(bg, &["color", "gradient", "reset", "slides", "all"], bm)?;
            let mut p = json!({});
            match (args::opt_color(bg, "color", bm)?, args::present(bg, "gradient"), args::opt_bool(bg, "reset", bm)?.unwrap_or(false)) {
                (Some(c), None, false) => p["color"] = json!(c),
                (None, Some(g), false) => p["gradient"] = gradient(g, bm)?,
                (None, None, true) => p["reset"] = json!(true),
                _ => return Err(invalid(format!("{bm} is one of `color`, `gradient` or `reset: true`"))),
            }
            let all = args::opt_bool(bg, "all", bm)?.unwrap_or(false);
            let ids = args::opt_ids(bg, "slides", bm, MAX_SLIDES)?;
            match (all, ids) {
                (true, None) => Some((p, None)),
                (false, Some(ids)) => Some((p, Some(ids))),
                _ => return Err(invalid(format!("{bm} applies to `slides`, or to `all: true`"))),
            }
        }
        None => None,
    };
    let footer = match args::present(a, "footer") {
        Some(f) => {
            let fm = "deck.design footer";
            args::only(f, &["slide_numbers", "footer", "footer_text", "date", "hide_on_title"], fm)?;
            let mut p = json!({ "all": true });
            for (key, engine) in [("slide_numbers", "slideNumber"), ("footer", "footer"), ("date", "date"), ("hide_on_title", "hideOnTitle")] {
                if let Some(v) = args::opt_bool(f, key, fm)? {
                    p[engine] = json!(v);
                }
            }
            if let Some(t) = args::opt_str(f, "footer_text", fm, 512)? {
                p["footerText"] = json!(t);
            }
            if p.as_object().is_some_and(|o| o.len() == 1) {
                return Err(invalid(format!("{fm}: give at least one of its settings")));
            }
            Some(p)
        }
        None => None,
    };
    if theme.is_none() && colors.is_none() && fonts.is_none() && size.is_none() && background.is_none() && footer.is_none() {
        return Err(invalid(format!("{M}: name at least one thing to change")));
    }
    docs::with_doc(a, cx, M, |doc| {
        let deck = doc.deck()?;
        let mut cmds: Vec<(&'static str, Json)> = Vec::new();
        let mut labels: Vec<&str> = Vec::new();
        if theme.is_some() || colors.is_some() || fonts.is_some() {
            // Every master the slides use, by the first slide using it (the
            // engine changes the current slide's master).
            let mut firsts: Vec<(usize, usize)> = Vec::new();
            for (i, s) in deck.slides.iter().enumerate() {
                let master = deck.masters.iter().position(|m| m.layout(s.layout).is_some()).unwrap_or(0);
                if !firsts.iter().any(|(m, _)| *m == master) {
                    firsts.push((master, i));
                }
            }
            if firsts.len() > MAX_MASTERS {
                return Err(too_large(format!("{M}: the slides use {} masters, more than the {MAX_MASTERS} one call changes", firsts.len())));
            }
            let targets: Vec<Option<usize>> = if firsts.is_empty() { vec![None] } else { firsts.iter().map(|(_, i)| Some(*i)).collect() };
            for at in targets {
                if let Some(i) = at {
                    cmds.push(("slide.go", json!({ "index": i })));
                }
                if let Some(t) = &theme {
                    cmds.push(("design.theme", json!({ "name": t })));
                }
                if let Some(c) = &colors {
                    cmds.push(("design.colors", json!({ "name": c })));
                }
                if let Some(f) = &fonts {
                    cmds.push(("design.fonts", json!({ "name": f })));
                }
            }
            labels.push(if theme.is_some() { "Change theme" } else { "Change theme variant" });
        }
        if let Some([w, h]) = size {
            cmds.push(("design.slideSize", json!({ "w": w, "h": h, "scale": scale })));
            labels.push("Change slide size");
        }
        if let Some((p, ids)) = &background {
            match ids {
                None => {
                    let mut p = p.clone();
                    p["all"] = json!(true);
                    if !deck.slides.is_empty() {
                        cmds.push(("slide.go", json!({ "index": 0 })));
                    }
                    cmds.push(("design.background", p));
                }
                Some(ids) => {
                    for i in positions(doc, ids)? {
                        let mut p = p.clone();
                        p["index"] = json!(i);
                        cmds.push(("slide.go", json!({ "index": i })));
                        cmds.push(("design.background", p));
                    }
                }
            }
            labels.push("Change background");
        }
        if let Some(p) = &footer {
            cmds.push(("design.headerFooter", p.clone()));
            labels.push("Header and footer");
        }
        let label = if labels.len() == 1 { labels[0] } else { "Change design" };
        let done = gate::change(doc, cx.area, Plan { method: M, most: DESIGN, cmds }, label)?;
        let size = doc.deck()?.slide_size;
        doc.answer(&done.changed, json!({ "size": [size.width, size.height] }))
    })
}

/// `deck.undo {doc}` and `deck.redo {doc}`: one folded step, and the labels
/// of the next undo and redo.
pub(crate) fn history(a: &Json, cx: &Ctx, redo: bool) -> Result<Json, String> {
    let m = if redo { "deck.redo" } else { "deck.undo" };
    args::only(a, &["doc"], m)?;
    docs::with_doc(a, cx, m, |doc| {
        let changed = gate::step(doc, cx.area, redo)?;
        let st = doc.st()?;
        let (undo, redo) = (st.history.undo.last().map(|e| e.label.clone()), st.history.redo.last().map(|e| e.label.clone()));
        doc.answer(&changed, json!({ "undo": undo, "redo": redo }))
    })
}
