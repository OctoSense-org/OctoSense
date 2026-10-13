//! Reading and drawing an open document (SERVICE.md "Reading and drawing",
//! "Present"), and the catalogs the inspector lists.
//!
//! The safety review of the engine commands behind them, all `safe` in
//! `skill/safety.json`: `document.inspect`, `slide.inspect`, `format.state`,
//! `design.themes` and `shape.presets` read the document (or the engine's
//! catalogs) and write nothing; `slide.go`, `edit.select`, `text.edit` (on a
//! shape that already holds text), `text.selectParagraph` and `text.exit`
//! change only the selection. Every one passes the gate (`crate::gate`), and
//! a read never changes the document. The functions they call beside the
//! registry are not commands and touch no file: `Session::hit_test`,
//! deckcraft-render's `render_slide` and `render_layout` (each one bounded
//! raster, written only into the service's own cache, [`cache::store`]),
//! deckcraft-anim's `show_order`, `ShowState::steps` and `Timeline::state`,
//! `slides_to_outline`, and the font database's `families`. Besides the
//! commands' answers, the service reads what the inspector shows straight
//! from the open document's model, read-only (`deckcraft_model`): a box's
//! anchor and autofit (`resolve::body`, with what it inherits), a chart's
//! legend, data labels, gridlines and palette, a table's style options, and
//! the engine's built-in colour schemes with their colours.

use deckcraft_anim::{ShowState, Timeline};
use deckcraft_engine::geom::{Point, Vec2};
use deckcraft_engine::render::{self as render, ParaState, RenderOpts, ShapeState};
use deckcraft_engine::tools::Hit;
use deckcraft_model::resolve::{self, Ctx};
use deckcraft_model::style::Dash;
use deckcraft_model::text::{Align, Anchor, AutoFit, Spacing};
use deckcraft_model::{Fill, Presentation, Shape, ShapeId, ShapeKind, Slide};
use serde_json::{json, Value as Json};

use crate::args;
use crate::cache;
use crate::codes::{fail, invalid, too_large, Code};
use crate::docs::{self, OpenDoc, Rendered};
use crate::gate::{self, HeldCache, Plan};
use crate::Ctx as Call;

/// A drawing's width in pixels (SERVICE.md: 48 to 3840).
pub(crate) const WIDTH: (i64, i64) = (48, 3840);
/// A drawing's longest side, and its most pixels.
pub(crate) const MAX_SIDE: u64 = 4096;
pub(crate) const MAX_PIXELS: u64 = 16_000_000;
/// A layout picture's default width.
pub(crate) const LAYOUT_WIDTH: i64 = 240;
/// The most layout pictures one call draws (a master's layouts).
pub(crate) const MAX_LAYOUTS: usize = 64;
/// The most a `notes` of `deck.slides` shows (the whole notes are
/// `deck.slide`'s): a deck's notes could otherwise pass App Hub's 4 MiB
/// answer.
pub(crate) const NOTES_PREVIEW: usize = 200;

/// The pixels of slide `deck` drawn `width` wide: (scale, width, height), or
/// `too_large:` over the caps.
pub(crate) fn size_for(deck: &Presentation, width: u64, m: &str) -> Result<(f64, u64, u64), String> {
    let (sw, sh) = (deck.slide_size.width.max(1.0), deck.slide_size.height.max(1.0));
    let scale = width as f64 / sw;
    let height = (sh * scale).round().max(1.0) as u64;
    if width.max(height) > MAX_SIDE || width * height > MAX_PIXELS {
        return Err(too_large(format!("{m}: {width} × {height} pixels is past the {MAX_SIDE} pixels a side and {} megapixels of one drawing", MAX_PIXELS / 1_000_000)));
    }
    Ok((scale, width, height))
}

fn width(a: &Json, m: &str, default: i64) -> Result<u64, String> {
    Ok(args::opt_int_in(a, "width", m, WIDTH.0, WIDTH.1)?.unwrap_or(default) as u64)
}

/// A colour of `ctx` as `#RRGGBB`.
fn hex(ctx: &Ctx, color: &deckcraft_model::ColorRef, ph: Option<deckcraft_model::Rgba>) -> String {
    format!("#{}", ctx.color(color, ph).hex())
}

/// The click builds of slide `i`: (its timeline, how many clicks, whether
/// its first step plays by itself as the slide appears). A show without
/// animation has none.
fn builds(deck: &Presentation, i: usize) -> (Timeline, usize, bool) {
    if deck.show.without_animation {
        return (Timeline::default(), 0, false);
    }
    let tl = ShowState::timeline(deck, i);
    let auto = tl.step_is_auto(0);
    let clicks = ShowState::steps(deck, i).saturating_sub(usize::from(auto));
    (tl, clicks, auto)
}

/// What a slide shows after `step` click builds, each at its end state
/// (SERVICE.md "What Present shows"): the renderer's state of every shape.
fn show_state(tl: &Timeline, auto: bool, step: usize) -> impl Fn(ShapeId) -> ShapeState + '_ {
    // The main step whose end shows, and the moment past its end (a build
    // of no duration ends at once).
    let main = (step + usize::from(auto)).checked_sub(1);
    move |id: ShapeId| {
        let at = |para: Option<usize>| match main {
            None => tl.state(id, para, 0, 0.0),
            Some(k) => tl.state(id, para, k, tl.step_duration(k) + 1.0),
        };
        let a = at(None);
        ShapeState {
            visible: a.visible,
            opacity: a.opacity,
            offset: Vec2::new(a.offset_x, a.offset_y),
            scale: (a.scale_x, a.scale_y),
            rotate: a.rotate,
            clip: a.clip,
            tint: a.tint,
            paras: tl
                .paragraph_targets(id)
                .into_iter()
                .map(|p| {
                    let a = at(Some(p));
                    (p, ParaState { visible: a.visible, opacity: a.opacity, offset: Vec2::new(a.offset_x, a.offset_y) })
                })
                .collect(),
        }
    }
}

fn transition_json(slide: &Slide) -> Json {
    match &slide.transition {
        Some(t) => json!({ "kind": t.kind, "option": t.option, "duration": t.duration_ms, "advance_after": t.advance_after_ms.map(|ms| ms as f64 / 1000.0), "on_click": t.advance_on_click }),
        None => Json::Null,
    }
}

/// The section a slide is in, by name.
fn section_of(deck: &Presentation, slide: &Slide) -> Json {
    deck.section_of(slide.id).and_then(|i| deck.sections.get(i)).map_or(Json::Null, |s| json!(s.name))
}

/// `document.inspect {}`: one `safe` read.
const SLIDES: usize = 1;

/// `deck.slides {doc}`.
pub(crate) fn slides(a: &Json, cx: &Call) -> Result<Json, String> {
    const M: &str = "deck.slides";
    args::only(a, &["doc"], M)?;
    docs::with_doc(a, cx, M, |doc| {
        let inspected = gate::read(doc, cx.area, Plan { method: M, most: SLIDES, cmds: vec![("document.inspect", json!({}))] })?.remove(0);
        let deck = doc.deck()?;
        let list: Vec<Json> = deck
            .slides
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let row = &inspected["slides"][i];
                let notes = row["notes"].as_str().unwrap_or("");
                let notes = if notes.chars().count() > NOTES_PREVIEW { format!("{}…", notes.chars().take(NOTES_PREVIEW).collect::<String>()) } else { notes.to_string() };
                json!({
                    "id": s.id.0,
                    "index": i,
                    "title": row["title"],
                    "layout": s.layout.0,
                    "hidden": s.hidden,
                    "section": section_of(&deck, s),
                    "notes": notes,
                    "transition": row["transition"],
                    "builds": builds(&deck, i).1,
                    "comments": s.comments.len(),
                })
            })
            .collect();
        Ok(json!({ "slides": list, "sections": docs::sections(&deck), "size": inspected["slideSize"] }))
    })
}

/// `slide.inspect {index}`: one `safe` read.
const SLIDE: usize = 1;

/// A shape as `deck.slide` lists it: `slide.inspect`'s summary, with its
/// paragraphs, outline, lock, alt text and picture from the model.
fn shape_json(ctx: Option<&Ctx>, summary: &Json, sh: &Shape) -> Json {
    let mut out = json!({
        "id": sh.id.0,
        "name": sh.name,
        "kind": summary["kind"],
        "box": summary["box"],
        "rotation": summary["rotation"],
    });
    for (from, to) in [("placeholder", "placeholder"), ("preset", "preset"), ("text", "text"), ("fill", "fill"), ("table", "table"), ("chart", "chart"), ("altText", "alt_text"), ("media", "media")] {
        if !summary[from].is_null() {
            out[to] = summary[from].clone();
        }
    }
    if let Some(t) = sh.text.as_ref().filter(|t| !t.paragraphs.is_empty()) {
        out["paragraphs"] = json!(t.paragraphs.iter().map(|p| json!({ "text": p.text(), "level": p.level })).collect::<Vec<_>>());
    }
    if let Some(ctx) = ctx {
        if !matches!(sh.kind, ShapeKind::Group { .. } | ShapeKind::Table(_) | ShapeKind::Chart(_)) {
            let (line, ph) = resolve::line(ctx, sh);
            let color = match &line.fill {
                Some(Fill::Solid { color }) => json!(hex(ctx, color, ph)),
                Some(Fill::None) | None => json!("none"),
                Some(_) => json!("other"),
            };
            let dash = match &line.dash {
                Some(Dash::Preset { name }) => json!(name),
                _ => json!("solid"),
            };
            out["line"] = json!({ "color": color, "width": line.width, "dash": dash });
        }
    }
    if sh.hidden {
        out["hidden"] = json!(true);
    }
    if sh.locked {
        out["locked"] = json!(true);
    }
    // What `slide.inspect` leaves out of a chart and a table, from the model,
    // in the keys `deck.chart` and `deck.table {op: "options"}` take.
    match &sh.kind {
        ShapeKind::Chart(c) if out["chart"].is_object() => {
            out["chart"]["legend"] = json!(c.legend.as_deref().unwrap_or("none"));
            out["chart"]["data_labels"] = json!(c.data_labels);
            out["chart"]["gridlines"] = json!(c.gridlines);
            out["chart"]["palette"] = json!(c.palette);
        }
        ShapeKind::Table(t) if out["table"].is_object() => {
            out["table"]["options"] = json!({
                "header_row": t.first_row,
                "total_row": t.last_row,
                "banded_rows": t.band_row,
                "first_column": t.first_col,
                "last_column": t.last_col,
                "banded_columns": t.band_col,
            });
        }
        _ => {}
    }
    if let ShapeKind::Picture { fill } = &sh.kind {
        out["picture"] = json!({
            "crop": fill.crop,
            "brightness": fill.adjust.brightness,
            "contrast": fill.adjust.contrast,
            "saturation": fill.adjust.saturation.unwrap_or(1.0),
            "grayscale": fill.adjust.grayscale,
            "transparency": 1.0 - fill.alpha.unwrap_or(1.0),
            "decorative": sh.decorative,
        });
    }
    out
}

/// `deck.slide {doc, slide}`.
pub(crate) fn slide(a: &Json, cx: &Call) -> Result<Json, String> {
    const M: &str = "deck.slide";
    args::only(a, &["doc", "slide"], M)?;
    let id = args::id(a, "slide", M)?;
    docs::with_doc(a, cx, M, |doc| {
        let i = doc.slide_index(id)?;
        let inspected = gate::read(doc, cx.area, Plan { method: M, most: SLIDE, cmds: vec![("slide.inspect", json!({ "index": i }))] })?.remove(0);
        let deck = doc.deck()?;
        let s = &deck.slides[i];
        let ctx = Ctx::for_slide(&deck, s);
        let summaries = inspected["shapes"].as_array().cloned().unwrap_or_default();
        let shapes: Vec<Json> = summaries.iter().zip(&s.shapes).map(|(summary, sh)| shape_json(ctx.as_ref(), summary, sh)).collect();
        Ok(json!({
            "id": id,
            "index": i,
            "layout": s.layout.0,
            "background": inspected["background"],
            "hidden": s.hidden,
            "notes": inspected["notes"],
            "transition": transition_json(s),
            "builds": builds(&deck, i).1,
            "shapes": shapes,
        }))
    })
}

/// `slide.go` and `edit.select`: the slide, and the shapes whose handles
/// may hit.
const HIT: usize = 2;
/// The most shapes a call names (SERVICE.md "Objects": 500).
pub(crate) const MAX_SHAPES: usize = 500;

/// Every id of `ids` names a shape on slide `i` (groups' members too), or
/// `unknown_id:`.
pub(crate) fn shapes_exist(doc: &OpenDoc, i: usize, ids: &[u32]) -> Result<(), String> {
    let deck = doc.deck()?;
    let slide = &deck.slides[i];
    match ids.iter().find(|id| slide.shape(ShapeId(**id)).is_none()) {
        Some(id) => Err(fail(Code::UnknownId, format!("slide {} has no shape {id} (an undo may have removed it)", slide.id.0))),
        None => Ok(()),
    }
}

/// `deck.hit {doc, slide, at, tol?, selected?}`. An empty `selected` is no
/// selection, as a missing one is.
pub(crate) fn hit(a: &Json, cx: &Call) -> Result<Json, String> {
    const M: &str = "deck.hit";
    args::only(a, &["doc", "slide", "at", "tol", "selected"], M)?;
    let id = args::id(a, "slide", M)?;
    let at = args::opt_point(a, "at", M)?.ok_or_else(|| invalid(format!("{M} needs `at`, [x, y] in points")))?;
    let tol = args::opt_num(a, "tol", M, 0.0, 100.0)?.unwrap_or(4.0);
    let selected = match args::present(a, "selected") {
        Some(Json::Array(list)) if list.is_empty() => None,
        _ => args::opt_ids(a, "selected", M, MAX_SHAPES)?,
    };
    docs::with_doc(a, cx, M, |doc| {
        let i = doc.slide_index(id)?;
        let mut cmds = vec![("slide.go", json!({ "index": i }))];
        if let Some(ids) = &selected {
            shapes_exist(doc, i, ids)?;
            cmds.push(("edit.select", json!({ "ids": ids })));
        }
        gate::read(doc, cx.area, Plan { method: M, most: HIT, cmds })?;
        let hit = doc.session.hit_test(Point::new(at[0], at[1]), tol.max(0.01));
        Ok(json!({ "hit": hit.map(|h| match h {
            Hit::Shape { id, text } => json!({ "kind": if text { "text" } else { "shape" }, "id": id.0 }),
            Hit::Resize { id, handle } => json!({ "kind": "resize", "id": id.0, "handle": handle }),
            Hit::Rotate { id } => json!({ "kind": "rotate", "id": id.0 }),
            Hit::Adjust { id, index } => json!({ "kind": "adjust", "id": id.0, "index": index }),
            Hit::LineEnd { id, end } => json!({ "kind": "line_end", "id": id.0, "end": end }),
        }) }))
    })
}

/// `deck.view {doc, slide, width, edit?, editing?, step?}`.
pub(crate) fn view(a: &Json, cx: &Call) -> Result<Json, String> {
    const M: &str = "deck.view";
    args::only(a, &["doc", "slide", "width", "edit", "editing", "step"], M)?;
    let id = args::id(a, "slide", M)?;
    let w = args::opt_int_in(a, "width", M, WIDTH.0, WIDTH.1)?.ok_or_else(|| invalid(format!("{M} needs `width`, {} to {} pixels", WIDTH.0, WIDTH.1)))? as u64;
    let edit = args::opt_bool(a, "edit", M)?.unwrap_or(false);
    let editing = match args::present(a, "editing") {
        Some(_) => Some(args::id(a, "editing", M)?),
        None => None,
    };
    let step = args::opt_int_in(a, "step", M, 0, 100_000)?.map(|s| s as usize);
    if step.is_some() && (edit || editing.is_some()) {
        return Err(invalid(format!("{M}: `step` draws the show view, without `edit` or `editing`")));
    }
    docs::with_doc(a, cx, M, |doc| {
        let i = doc.slide_index(id)?;
        if let Some(e) = editing {
            shapes_exist(doc, i, &[e])?;
        }
        let deck = doc.deck()?;
        let (scale, width, _) = size_for(&deck, w, M)?;
        let (tl, clicks, auto) = builds(&deck, i);
        if let Some(k) = step.filter(|k| *k > clicks) {
            return Err(invalid(format!("{M}: slide {id} has {clicks} builds, so `step` is 0 to {clicks}, not {k}")));
        }
        let mode = match (edit, editing, step) {
            (_, _, Some(k)) => format!("b{k}"),
            (true, Some(e), None) => format!("et{e}"),
            (false, Some(e), None) => format!("st{e}"),
            (true, None, None) => "e".into(),
            (false, None, None) => "s".into(),
        };
        let key = format!("{id}-{mode}@{width}");
        let rev = doc.slide_rev(id);
        if let Some(r) = doc.renders.get(&key).filter(|r| r.rev == rev && cache::still_there(cx.area, &r.rel, r.bytes)) {
            return Ok(json!({ "path": r.rel, "width": r.width, "height": r.height, "rev": doc.rev }));
        }
        let state = step.filter(|_| !deck.show.without_animation).map(|k| show_state(&tl, auto, k));
        let opts = RenderOpts {
            scale,
            edit,
            skip_text: editing.map(ShapeId),
            state: state.as_ref().map(|f| f as &dyn Fn(ShapeId) -> ShapeState),
            ..Default::default()
        };
        let img = render::render_slide(&deck, i, &opts);
        let png = img.to_png();
        let file = format!("{key}-r{rev}.png");
        let rel = cache::render_path(&doc.handle, &file);
        cache::store(cx.area, &rel, &png, M)?;
        // An older revision of the same slide, size and mode goes.
        cache::drop_older(cx.area, &doc.handle, &format!("{key}-r"), &file);
        let (drawn_w, drawn_h) = (u64::from(img.width), u64::from(img.height));
        doc.renders.insert(key, Rendered { rel: rel.clone(), rev, width: drawn_w, height: drawn_h, bytes: png.len() as u64 });
        Ok(json!({ "path": rel, "width": drawn_w, "height": drawn_h, "rev": doc.rev }))
    })
}

/// `deck.layouts {doc, width?}`: a picture of each layout of the first
/// master, in the render cache.
pub(crate) fn layouts(a: &Json, cx: &Call) -> Result<Json, String> {
    const M: &str = "deck.layouts";
    args::only(a, &["doc", "width"], M)?;
    let w = width(a, M, LAYOUT_WIDTH)?;
    docs::with_doc(a, cx, M, |doc| {
        let deck = doc.deck()?;
        let (scale, width, height) = size_for(&deck, w, M)?;
        let Some(master) = deck.masters.first() else { return Ok(json!({ "layouts": [] })) };
        let n = master.layouts.len().min(MAX_LAYOUTS);
        if (n as u64) * width * height > MAX_PIXELS {
            return Err(too_large(format!("{M}: {n} layouts at {width} pixels are more than the {} megapixels one call draws: draw them smaller", MAX_PIXELS / 1_000_000)));
        }
        let rev = doc.deck_rev;
        let mut out = Vec::with_capacity(n);
        for (li, layout) in master.layouts.iter().take(n).enumerate() {
            let key = format!("layout{}@{width}", layout.id.0);
            let current = doc.renders.get(&key).filter(|r| r.rev == rev && cache::still_there(cx.area, &r.rel, r.bytes)).map(|r| r.rel.clone());
            let rel = match current {
                Some(rel) => rel,
                None => {
                    let img = render::render_layout(&deck, 0, Some(li), &RenderOpts { scale, edit: true, ..Default::default() });
                    let png = img.to_png();
                    let file = format!("{key}-r{rev}.png");
                    let rel = cache::render_path(&doc.handle, &file);
                    cache::store(cx.area, &rel, &png, M)?;
                    cache::drop_older(cx.area, &doc.handle, &format!("{key}-r"), &file);
                    doc.renders.insert(key, Rendered { rel: rel.clone(), rev, width: u64::from(img.width), height: u64::from(img.height), bytes: png.len() as u64 });
                    rel
                }
            };
            out.push(json!({ "id": layout.id.0, "name": layout.name, "kind": layout.kind.xml(), "path": rel }));
        }
        Ok(json!({ "layouts": out }))
    })
}

/// A command on the engine's catalogs, on a session of its own: one
/// `safe` read through the gate.
fn catalog(cx: &Call, m: &'static str, id: &'static str) -> Result<Json, String> {
    let mut s = docs::app_session();
    Ok(gate::exec(&mut s, cx.area, &Plan { method: m, most: 1, cmds: vec![(id, json!({}))] }, &mut HeldCache::default())?.remove(0))
}

/// `deck.themes {}`.
pub(crate) fn themes(a: &Json, cx: &Call) -> Result<Json, String> {
    const M: &str = "deck.themes";
    args::only(a, &[], M)?;
    let got = catalog(cx, M, "design.themes")?;
    let themes: Vec<Json> = got["themes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| {
            let name = t["name"].as_str().unwrap_or("");
            json!({ "name": name, "label": if name == "DeckCraft" { "Default" } else { name }, "colors": t["colors"], "major": t["major"], "minor": t["minor"] })
        })
        .collect();
    Ok(json!({ "themes": themes, "color_schemes": color_schemes(), "font_schemes": got["fontSchemes"] }))
}

/// The engine's built-in colour schemes, each with its twelve colours in the
/// engine's order (dk1, lt1, dk2, lt2, accent1 to accent6, hlink, folHlink),
/// as `design.themes` gives a theme's: the list `design.colors {name}`
/// chooses from (`deckcraft_model::theme::builtin_color_schemes`, which
/// `design.themes` names only).
fn color_schemes() -> Json {
    json!(deckcraft_model::theme::builtin_color_schemes()
        .iter()
        .map(|c| json!({ "name": c.name, "colors": c.colors.iter().map(|x| format!("#{}", x.hex())).collect::<Vec<_>>() }))
        .collect::<Vec<_>>())
}

/// A box's vertical anchor as `deck.format` names it (`top`, `middle`,
/// `bottom`), or as the model names the two it doesn't set.
fn anchor_name(a: Anchor) -> &'static str {
    match a {
        Anchor::Top => "top",
        Anchor::Middle => "middle",
        Anchor::Bottom => "bottom",
        Anchor::Justified => "justified",
        Anchor::Distributed => "distributed",
    }
}

/// A box's autofit as `deck.format` names it.
fn autofit_name(a: &AutoFit) -> &'static str {
    match a {
        AutoFit::None => "none",
        AutoFit::Shrink { .. } => "shrink",
        AutoFit::Shape => "resize",
    }
}

/// `deck.fonts {}`: what the engine can draw on this device.
pub(crate) fn fonts(a: &Json, _cx: &Call) -> Result<Json, String> {
    args::only(a, &[], "deck.fonts")?;
    Ok(json!({ "families": deckcraft_fonts::FontDb::global().families() }))
}

/// `deck.shapes {}`.
pub(crate) fn shapes(a: &Json, cx: &Call) -> Result<Json, String> {
    const M: &str = "deck.shapes";
    args::only(a, &[], M)?;
    Ok(json!({ "presets": catalog(cx, M, "shape.presets")? }))
}

/// `deck.text {doc}`.
pub(crate) fn text(a: &Json, cx: &Call) -> Result<Json, String> {
    const M: &str = "deck.text";
    args::only(a, &["doc"], M)?;
    docs::with_doc(a, cx, M, |doc| {
        let deck = doc.deck()?;
        Ok(json!({ "outline": deckcraft_format::slides_to_outline(&deck) }))
    })
}

/// `slide.go`, then `edit.select` or `text.edit` and
/// `text.selectParagraph`, `format.state` and `text.exit`.
const FORMAT_STATE: usize = 5;

/// An alignment as SERVICE.md names it.
pub(crate) fn align_name(a: Align) -> &'static str {
    match a {
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
        Align::Justify => "justify",
        Align::Distributed => "distributed",
    }
}

/// `deck.format_state {doc, slide, id, paragraph?}`: the formatting of a box
/// (its first run and paragraph), or of one of its paragraphs.
pub(crate) fn format_state(a: &Json, cx: &Call) -> Result<Json, String> {
    const M: &str = "deck.format_state";
    args::only(a, &["doc", "slide", "id", "paragraph"], M)?;
    let id = args::id(a, "slide", M)?;
    let shape = args::id(a, "id", M)?;
    let paragraph = args::opt_index(a, "paragraph", M)?;
    docs::with_doc(a, cx, M, |doc| {
        let i = doc.slide_index(id)?;
        shapes_exist(doc, i, &[shape])?;
        let deck = doc.deck()?;
        let sh = deck.slides[i].shape(ShapeId(shape)).cloned().unwrap_or_default();
        let mut cmds = vec![("slide.go", json!({ "index": i }))];
        let para = match paragraph {
            Some(p) => {
                let n = sh.text.as_ref().map_or(0, |t| t.paragraphs.len());
                if p >= n {
                    return Err(invalid(format!("{M}: shape {shape} has {n} paragraphs, so `paragraph` is below {n}")));
                }
                if !matches!(sh.kind, ShapeKind::Shape | ShapeKind::Connector { .. }) {
                    return Err(invalid(format!("{M}: shape {shape} holds no paragraphs of its own")));
                }
                cmds.push(("text.edit", json!({ "id": shape, "at": [p, 0] })));
                cmds.push(("text.selectParagraph", json!({ "at": [p, 0] })));
                p
            }
            None => {
                cmds.push(("edit.select", json!({ "ids": [shape] })));
                0
            }
        };
        cmds.push(("format.state", json!({})));
        if paragraph.is_some() {
            cmds.push(("text.exit", json!({})));
        }
        let got = gate::read(doc, cx.area, Plan { method: M, most: FORMAT_STATE, cmds })?;
        let state = &got[got.len() - if paragraph.is_some() { 2 } else { 1 }];
        // The box's anchor and autofit, as it draws them: its own, else what
        // its layout and master placeholders give, else the engine's
        // defaults (`resolve::body`).
        let body = Ctx::for_slide(&deck, &deck.slides[i]).map(|ctx| resolve::body(&ctx, &sh));
        let anchor = body.as_ref().and_then(|b| b.anchor).map(anchor_name);
        let autofit = body.as_ref().and_then(|b| b.autofit.as_ref()).map(autofit_name);
        // The colour of the same run format.state reads, resolved.
        let color = Ctx::for_slide(&deck, &deck.slides[i]).map(|ctx| {
            let body = sh.text.clone().unwrap_or_default();
            let p = body.paragraphs.get(para).cloned().unwrap_or_default();
            let run = match paragraph {
                Some(n) => deckcraft_model::edit::props_at(&body, (n, 1)),
                None => p.runs.first().map(|r| r.props.clone()).unwrap_or_default(),
            };
            format!("#{}", resolve::text_color(&ctx, &resolve::run(&ctx, &sh, &p, &run)).hex())
        });
        let align = state["align"].as_str().and_then(Align::from_xml).map(align_name);
        let spacing: Option<Spacing> = serde_json::from_value(state["lineSpacing"].clone()).ok();
        Ok(json!({
            "font": state["font"],
            "size": state["size"],
            "bold": state["bold"],
            "italic": state["italic"],
            "underline": state["underline"],
            "strike": state["strike"],
            "color": color,
            "align": align,
            "bullets": state["bullets"],
            "numbering": state["numbering"],
            "line_spacing": match spacing {
                Some(Spacing::Pct(lines)) => json!(lines),
                Some(Spacing::Pts(pt)) => json!({ "pt": pt }),
                None => Json::Null,
            },
            "anchor": anchor,
            "autofit": autofit,
        }))
    })
}

/// `deck.show {doc, from?}`: the slides the show visits, from `from` (or its
/// first slide), each with its click builds, and whether it loops. With
/// `loop`, the order runs on through the show's slides before `from`, so
/// the app can start again at `order[0]`.
pub(crate) fn show(a: &Json, cx: &Call) -> Result<Json, String> {
    const M: &str = "deck.show";
    args::only(a, &["doc", "from"], M)?;
    let from = match args::present(a, "from") {
        Some(_) => Some(args::id(a, "from", M)?),
        None => None,
    };
    docs::with_doc(a, cx, M, |doc| {
        let deck = doc.deck()?;
        let order = deckcraft_anim::show_order(&deck);
        let looping = deck.show.loop_until_esc || deck.show.show_type == "kiosk";
        let seq: Vec<usize> = match from {
            None => order.clone(),
            Some(id) => {
                let i = doc.slide_index(id)?;
                let mut seq = vec![i];
                match order.iter().position(|j| *j == i) {
                    Some(p) => {
                        seq.extend(&order[p + 1..]);
                        if looping {
                            seq.extend(&order[..p]);
                        }
                    }
                    None => {
                        seq.extend(order.iter().filter(|j| **j > i));
                        if looping {
                            seq.extend(order.iter().filter(|j| **j < i));
                        }
                    }
                }
                seq
            }
        };
        let entries: Vec<Json> = seq
            .iter()
            .map(|&i| {
                let s = &deck.slides[i];
                let mut e = json!({ "slide": s.id.0, "steps": builds(&deck, i).1 });
                if let Some(after) = deck.show.use_timings.then_some(()).and(s.transition.as_ref()).and_then(|t| t.advance_after_ms) {
                    e["advance_after"] = json!(after as f64 / 1000.0);
                }
                e
            })
            .collect();
        Ok(json!({ "order": entries, "loop": looping }))
    })
}
