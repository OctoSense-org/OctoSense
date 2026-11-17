//! Objects on a slide (SERVICE.md "Objects"): insert, text, format,
//! arrange, pictures, tables and charts. Every method starts with
//! `slide.go {index}` (`safe`, the selection only: it makes the named slide
//! current and clears the selection), so the commands after it act on that
//! slide, and builds every other command's parameters itself from the
//! reviewed values `args` read.
//!
//! The safety review of the engine commands behind them, by their class in
//! `skill/safety.json`:
//!
//! - `insert.picture` and `picture.change` are `file`: their one file
//!   access reads the picture their `path` names (`insert::media_bytes`).
//!   They are the door's reviewed reads (`REVIEWED.file_reads`): the service
//!   first checks the path stays inside the caller's storage, names a file
//!   of at most 64 MiB, and is a picture the engine draws within
//!   [`pictures::MAX_PIXELS`] (`crate::pictures`), then the door checks the
//!   path again through links and hands the engine the file's resolved
//!   path. They never get `data`.
//! - `shape.fill` is `file`, because a non-string `picture` makes it read a
//!   `path`: it is not one of the door's reviewed reads, so the door refuses
//!   it from `deck.run`. The app's `deck.shape {op: "fill"}` runs it with
//!   `color`, `none` or `gradient` (and `transparency`) only, which the
//!   service builds itself; it is one of the service's `UNREAD` commands,
//!   which the door admits for a method only when the call names neither
//!   `picture` nor `path` (`octosense_engine_area::door::Unread`).
//! - Everything else here is `safe` and passes the door: `slide.go`,
//!   `edit.select`, `insert.textBox`, `shape.insert`, `insert.table` (its
//!   rows and columns, and the `data` that fills its cells, which the
//!   engine sets as each cell's text), `insert.chart`, `chart.type`,
//!   `chart.data`, `chart.options`, `text.edit`, `text.select`,
//!   `text.insert`, `text.delete`, `text.exit`, `text.set`, the `format.*`
//!   commands, `shape.setBounds`, `shape.move`, `shape.rotate`,
//!   `shape.flip`, the `arrange.*` commands, `edit.duplicate`,
//!   `edit.delete`, `shape.line`, `shape.effects`, `shape.quickStyle`,
//!   `shape.lock`, `shape.visible`, `shape.altText`, `picture.crop`,
//!   `picture.adjust`, `picture.reset` and the `table.*` commands. The
//!   door's limits apply to them as to `deck.run`: a table's 5,625 cells, a
//!   chart's 10,000 data points, 1,000 line breaks a text, 100,000 points
//!   of position or size, 400 % bullets.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use deckcraft_engine::Clipboard;
use deckcraft_model::{resolve, ColorRef, Fill, MediaId, Presentation, Rgba, Shape, ShapeId, ShapeKind, SlideId};
use serde_json::{json, Value as Json};

use crate::args;
use crate::codes::{fail, invalid, too_large, Code};
use crate::docs::{self, OpenDoc, MAX_BODY};
use crate::gate::{self, Plan};
use crate::pictures;
use crate::reading::{shapes_exist, MAX_SHAPES};
use crate::Ctx;

/// `slide.go` and one insert command (`insert.textBox`, `shape.insert`,
/// `insert.picture`, `insert.table`, `insert.chart`, `edit.paste`), and, for
/// a table or a chart, the one command that styles or titles it
/// (`table.style`, `chart.options`).
const INSERT: usize = 3;
/// The most cells one table insert fills (SERVICE.md decision 2): the rest
/// of a larger table's text goes cell by cell through `deck.set_text`.
pub(crate) const MAX_FILL_CELLS: usize = 1_024;
/// The longest text of one cell a fill sets.
const MAX_CELL_TEXT: usize = 10_000;
/// The most line breaks one text holds: the door's own limit on
/// `text.insert` (each break costs the engine more than the last).
pub(crate) const MAX_LINE_BREAKS: usize = 1_000;
/// The longest text `deck.set_text` sets.
const MAX_SET_TEXT: usize = 100_000;
/// The chart types `insert.chart` and `chart.type` take.
const CHART_TYPES: [&str; 27] = [
    "column", "bar", "line", "pie", "doughnut", "area", "scatter", "stackedColumn", "percentColumn", "stackedBar", "percentBar", "lineMarkers",
    "stackedLine", "stackedArea", "bubble", "radar", "filledRadar", "waterfall", "funnel", "histogram", "pareto", "treemap", "sunburst",
    "boxWhisker", "stock", "surface", "combo",
];

/// The position of slide `id` and, checked, the shapes `ids` on it.
fn on_slide(doc: &OpenDoc, id: u32, ids: &[u32]) -> Result<usize, String> {
    let i = doc.slide_index(id)?;
    shapes_exist(doc, i, ids)?;
    Ok(i)
}

/// Resolve theme colours and fonts to their source values before pasting an
/// ordinary object into a deck with a different theme.
fn freeze_shape_look(ctx: &resolve::Ctx<'_>, shape: &Shape) -> Result<Shape, String> {
    let unsupported = || fail(Code::Unsupported, "This item needs matching deck designs. Change the destination design to match, then paste again.");
    if matches!(shape.kind, ShapeKind::Group { .. } | ShapeKind::Table(_) | ShapeKind::Chart(_) | ShapeKind::Opaque { .. }) {
        return Err(unsupported());
    }
    fn freeze(value: &mut Json, ctx: &resolve::Ctx<'_>, ph: Option<Rgba>) -> Result<(), String> {
        let damaged = || fail(Code::Damaged, "the copied item's appearance couldn't be read");
        match value {
            Json::Array(items) => for item in items { freeze(item, ctx, ph)?; },
            Json::Object(fields) => {
                if fields.get("base").and_then(Json::as_object).and_then(|base| base.get("kind")).and_then(Json::as_str)
                    .is_some_and(|kind| matches!(kind, "rgb" | "scheme" | "preset" | "system"))
                {
                    let color: ColorRef = serde_json::from_value(Json::Object(fields.clone())).map_err(|_| damaged())?;
                    *value = serde_json::to_value(ColorRef::rgb(ctx.color(&color, ph))).map_err(|_| damaged())?;
                    return Ok(());
                }
                for (key, item) in fields {
                    if matches!(key.as_str(), "font" | "fontEa" | "fontCs" | "fontSym" | "bulletFont") {
                        if let Some(name) = item.as_str().filter(|name| name.starts_with("+mj-") || name.starts_with("+mn-")) {
                            let name = ctx.master.theme.font(name);
                            if name.is_empty() { return Err(fail(Code::Unsupported, "This item's font can't be copied to another design yet.")); }
                            *item = Json::String(name);
                        }
                    }
                    freeze(item, ctx, ph)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn resolved_run(ctx: &resolve::Ctx<'_>, shape: &Shape, para: &deckcraft_model::Paragraph, run: &deckcraft_model::RunProps) -> deckcraft_model::RunProps {
        let mut props = resolve::run(ctx, shape, para, run);
        if props.fill.is_none() { props.fill = Some(Fill::solid(ColorRef::rgb(resolve::text_color(ctx, &props)))); }
        props.font = Some(resolve::font_family(ctx, &props));
        props
    }

    let mut out = shape.clone();
    out.xfrm = Some(resolve::xfrm(ctx, shape));
    let (fill, fill_ph) = resolve::fill(ctx, shape);
    let (line, line_ph) = resolve::line(ctx, shape);
    let (effects, effects_ph) = resolve::effects(ctx, shape);
    if matches!(fill, Some(Fill::Group | Fill::Background)) || matches!(line.fill, Some(Fill::Group | Fill::Background))
        || effects.as_ref().is_some_and(|effects| effects.raw3d.is_some())
    {
        return Err(unsupported());
    }
    out.fill = fill;
    out.line = Some(line);
    out.effects = effects;
    if let Some(text) = out.text.as_mut() {
        text.body = resolve::body(ctx, shape);
        for para in &mut text.paragraphs {
            let original = para.clone();
            para.props = resolve::para(ctx, shape, &original);
            for run in &mut para.runs { run.props = resolved_run(ctx, shape, &original, &run.props); }
            para.end_props = resolved_run(ctx, shape, &original, &original.end_props);
        }
    }
    out.style = None;
    let mut value = serde_json::to_value(out).map_err(|_| fail(Code::Damaged, "the copied item's appearance couldn't be read"))?;
    for (field, ph) in [("fill", fill_ph), ("line", line_ph), ("effects", effects_ph)] {
        if let Some(value) = value.get_mut(field) { freeze(value, ctx, ph)?; }
    }
    freeze(&mut value, ctx, None)?;
    serde_json::from_value(value).map_err(|_| fail(Code::Damaged, "the copied item's appearance couldn't be read"))
}

/// Copy between decks, remapping media when IDs collide and refusing
/// document-relative references the engine cannot yet migrate.
fn cross_deck_clipboard(
    source: &Presentation,
    source_slide: u32,
    target: &Presentation,
    target_slide: u32,
    clipboard: &Clipboard,
    offset: f64,
) -> Result<(Clipboard, u32), String> {
    let unsupported = || fail(Code::Unsupported, "This item can't be pasted into another deck yet. Copy it again in its original deck to paste it there.");
    let different_design = || fail(Code::Unsupported, "The decks need the same design. Match their designs, then paste again.");
    if clipboard.shapes.is_empty() == clipboard.slides.is_empty() {
        return Err(unsupported());
    }
    let from = source.slide(SlideId(source_slide)).ok_or_else(|| crate::docs::unknown_slide(source_slide))?;
    let to = target.slide(SlideId(target_slide)).ok_or_else(|| crate::docs::unknown_slide(target_slide))?;
    if source.embedded_fonts != target.embedded_fonts {
        return Err(different_design());
    }
    if !clipboard.slides.is_empty() && (source.slide_size != target.slide_size || source.header_footer != target.header_footer
        || source.default_text_style != target.default_text_style)
    {
        return Err(different_design());
    }
    let mut freeze_looks = false;
    let mut designs = vec![json!(&source.default_text_style)];
    if !clipboard.shapes.is_empty() {
        let (source_master, _) = source.master_for(from).ok_or_else(unsupported)?;
        let (target_master, _) = target.master_for(to).ok_or_else(unsupported)?;
        if source_master.theme != target_master.theme || source_master.color_map != target_master.color_map {
            freeze_looks = true;
        }
        freeze_looks |= source.default_text_style != target.default_text_style;
        designs.push(json!(source_master));
    }
    let mut unsafe_item = false;
    deckcraft_model::walk(&clipboard.shapes, &mut |shape, _| {
        if shape.raw_ext.is_some() || matches!(shape.kind, ShapeKind::Opaque { .. }) {
            unsafe_item = true;
        }
    });
    for slide in &clipboard.slides {
        let (source_master, source_layout) = source.layout(slide.layout).ok_or_else(unsupported)?;
        let (target_master, target_layout) = target.layout(slide.layout).ok_or_else(unsupported)?;
        if slide.raw_ext.is_some() { return Err(unsupported()); }
        if source_layout != target_layout || source_master.theme != target_master.theme || source_master.color_map != target_master.color_map
            || source_master.shapes != target_master.shapes || source_master.background != target_master.background
            || source_master.title_style != target_master.title_style || source_master.body_style != target_master.body_style
            || source_master.other_style != target_master.other_style || source_master.raw_ext != target_master.raw_ext
        {
            return Err(different_design());
        }
        designs.push(json!(source_master));
        deckcraft_model::walk(&slide.shapes, &mut |shape, _| {
            if shape.raw_ext.is_some() || matches!(shape.kind, ShapeKind::Opaque { .. }) {
                unsafe_item = true;
            }
        });
    }
    fn inspect_refs(value: &Json, media: &mut BTreeSet<u32>) -> bool {
        match value {
            Json::Array(items) => items.iter().any(|item| inspect_refs(item, media)),
            Json::Object(fields) => fields.iter().any(|(key, value)| {
                if matches!(key.as_str(), "media" | "poster" | "preview" | "sound") {
                    if let Some(id) = value.as_u64() {
                        if let Ok(id) = u32::try_from(id) { media.insert(id); } else { return true; }
                    }
                }
                (key == "kind" && value.as_str() == Some("slide"))
                    || (matches!(key.as_str(), "rawExt" | "rawExtra") && !value.is_null())
                    || inspect_refs(value, media)
            }),
            _ => false,
        }
    }
    fn map_media(value: &mut Json, ids: &BTreeMap<u32, u32>) {
        match value {
            Json::Array(items) => items.iter_mut().for_each(|item| map_media(item, ids)),
            Json::Object(fields) => {
                for (key, value) in fields {
                    if matches!(key.as_str(), "media" | "poster" | "preview" | "sound") {
                        if let Some(new) = value.as_u64().and_then(|id| u32::try_from(id).ok()).and_then(|id| ids.get(&id)) {
                            *value = json!(new);
                        }
                    }
                    map_media(value, ids);
                }
            }
            _ => {}
        }
    }
    let mut design_media = BTreeSet::new();
    if designs.iter().any(|design| inspect_refs(design, &mut design_media)) { return Err(unsupported()); }
    for (_, id) in &source.embedded_fonts { design_media.insert(id.0); }
    for id in design_media {
        let original = source.media(MediaId(id)).ok_or_else(unsupported)?;
        if target.media(MediaId(id)) != Some(original) {
            if !clipboard.shapes.is_empty() && !source.embedded_fonts.iter().any(|(_, font)| font.0 == id) {
                freeze_looks = true;
            } else {
                return Err(different_design());
            }
        }
    }
    let mut copied = clipboard.clone();
    if freeze_looks {
        let context = resolve::Ctx::for_slide(source, from).ok_or_else(unsupported)?;
        copied.shapes = clipboard.shapes.iter().map(|shape| freeze_shape_look(&context, shape)).collect::<Result<_, _>>()?;
    }
    let mut value = json!({"shapes": &copied.shapes, "slides": &copied.slides});
    let mut references = BTreeSet::new();
    if unsafe_item || inspect_refs(&value, &mut references) {
        return Err(unsupported());
    }
    let mut fixed = target.clone();
    fixed.fix_next_id();
    let mut next_id = fixed.next_id;
    let mut ids = BTreeMap::new();
    let mut media = Vec::new();
    for id in references {
        let item = clipboard.media.iter().find(|item| item.id == MediaId(id))
            .or_else(|| source.media(MediaId(id))).ok_or_else(unsupported)?;
        let mapped = if target.media(item.id) == Some(item) {
            id
        } else {
            next_id = next_id.checked_add(1).ok_or_else(|| too_large("this deck has no room for another item"))?;
            next_id
        };
        let mut item = item.clone();
        item.id = MediaId(mapped);
        ids.insert(id, mapped);
        media.push(item);
    }
    map_media(&mut value, &ids);
    copied.shapes = serde_json::from_value(value["shapes"].take()).map_err(|_| fail(Code::Damaged, "the copied item couldn't be read"))?;
    copied.slides = serde_json::from_value(value["slides"].take()).map_err(|_| fail(Code::Damaged, "the copied item couldn't be read"))?;
    copied.media = media;
    // The engine tests whether an original ID occurs on this slide to decide
    // whether to offset a paste. IDs are only unique within a deck.
    let mut unused = u32::MAX;
    while to.shape(ShapeId(unused)).is_some() {
        unused = unused.checked_sub(1).ok_or_else(unsupported)?;
    }
    for shape in &mut copied.shapes {
        shape.id = ShapeId(unused);
        if let Some(rect) = shape.xfrm.as_mut() {
            rect.x += offset;
            rect.y += offset;
        }
    }
    copied.pastes = 0;
    Ok((copied, next_id))
}

/// The engine assigns new IDs when it pastes. Reconnect references to the
/// pasted objects rather than leaving them pointing at IDs in the source deck.
fn repair_pasted_references(
    doc: &mut OpenDoc,
    original: &Clipboard,
    target_slide: u32,
    before_slides: &[u32],
    results: &[Json],
    same_origin: bool,
) -> Result<(), String> {
    let damaged = || fail(Code::Damaged, "the copied item couldn't be pasted");
    fn pair_shapes(source: &Shape, pasted: &Shape, ids: &mut BTreeMap<u32, u32>) -> Result<(), String> {
        if source.children().len() != pasted.children().len() || ids.insert(source.id.0, pasted.id.0).is_some() {
            return Err(fail(Code::Damaged, "the copied item couldn't be pasted"));
        }
        for (source, pasted) in source.children().iter().zip(pasted.children()) {
            pair_shapes(source, pasted, ids)?;
        }
        Ok(())
    }
    fn fix_shapes(shapes: &mut [Shape], ids: &BTreeMap<u32, u32>, existing: &BTreeSet<u32>) {
        for shape in shapes {
            if let ShapeKind::Connector { start, end } = &mut shape.kind {
                for endpoint in [start, end] {
                    if let Some((old, port)) = *endpoint {
                        *endpoint = ids.get(&old.0).map(|id| (ShapeId(*id), port)).or_else(|| existing.contains(&old.0).then_some((old, port)));
                    }
                }
            }
            if let Some(children) = shape.children_mut() { fix_shapes(children, ids, existing); }
        }
    }
    fn fix_comments(comments: &mut [deckcraft_model::Comment], ids: &BTreeMap<u32, u32>) -> Result<(), String> {
        for comment in comments {
            if let Some(old) = comment.shape {
                comment.shape = Some(ShapeId(*ids.get(&old.0).ok_or_else(|| fail(Code::Unsupported, "This item can't be pasted into another deck yet. Copy it again in its original deck to paste it there."))?));
            }
            fix_comments(&mut comment.replies, ids)?;
        }
        Ok(())
    }

    if !original.shapes.is_empty() {
        let made = results.get(1).and_then(|result| result.get("ids")).and_then(Json::as_array).ok_or_else(damaged)?;
        if made.len() != original.shapes.len() { return Err(damaged()); }
        let ids: Vec<u32> = made.iter().map(|id| id.as_u64().and_then(|id| u32::try_from(id).ok()).ok_or_else(damaged)).collect::<Result<_, _>>()?;
        let deck = doc.deck()?;
        let slide = deck.slide(SlideId(target_slide)).ok_or_else(damaged)?;
        let mut mapping = BTreeMap::new();
        let mut existing = BTreeSet::new();
        if same_origin { deckcraft_model::walk(&slide.shapes, &mut |shape, _| { existing.insert(shape.id.0); }); }
        for (source, id) in original.shapes.iter().zip(&ids) {
            pair_shapes(source, slide.shape(ShapeId(*id)).ok_or_else(damaged)?, &mut mapping)?;
        }
        let state = doc.st_mut()?;
        let slide = Arc::make_mut(&mut state.doc).slide_mut(SlideId(target_slide)).ok_or_else(damaged)?;
        for id in ids {
            let shape = slide.shape_mut(ShapeId(id)).ok_or_else(damaged)?;
            fix_shapes(std::slice::from_mut(shape), &mapping, &existing);
        }
    } else if !original.slides.is_empty() {
        let deck = doc.deck()?;
        let made: Vec<u32> = deck.slides.iter().map(|slide| slide.id.0).filter(|id| !before_slides.contains(id)).collect();
        if made.len() != original.slides.len() { return Err(damaged()); }
        let mut mappings = Vec::with_capacity(made.len());
        for (source, id) in original.slides.iter().zip(&made) {
            let pasted = deck.slide(SlideId(*id)).ok_or_else(damaged)?;
            if source.shapes.len() != pasted.shapes.len() { return Err(damaged()); }
            let mut mapping = BTreeMap::new();
            for (source, pasted) in source.shapes.iter().zip(&pasted.shapes) { pair_shapes(source, pasted, &mut mapping)?; }
            mappings.push(mapping);
        }
        let state = doc.st_mut()?;
        let deck = Arc::make_mut(&mut state.doc);
        for (id, mapping) in made.into_iter().zip(mappings) {
            let slide = deck.slide_mut(SlideId(id)).ok_or_else(damaged)?;
            fix_shapes(&mut slide.shapes, &mapping, &BTreeSet::new());
            for animation in &mut slide.animations {
                animation.shape = ShapeId(*mapping.get(&animation.shape.0).ok_or_else(damaged)?);
                if let Some(trigger) = animation.trigger {
                    animation.trigger = Some(ShapeId(*mapping.get(&trigger.0).ok_or_else(damaged)?));
                }
            }
            fix_comments(&mut slide.comments, &mapping)?;
        }
    }
    Ok(())
}

/// A shape of slide `i`, cloned.
fn shape_of(doc: &OpenDoc, i: usize, id: u32) -> Result<Shape, String> {
    shapes_exist(doc, i, &[id])?;
    Ok(doc.deck()?.slides[i].shape(ShapeId(id)).cloned().unwrap_or_default())
}

/// The line breaks in `text`.
fn line_breaks(text: &str) -> usize {
    text.matches(['\n', '\r', '\u{b}']).count()
}

fn check_breaks(text: &str, m: &str) -> Result<(), String> {
    let n = line_breaks(text);
    if n > MAX_LINE_BREAKS {
        return Err(too_large(format!("{m}: the text has {n} line breaks, more than the {MAX_LINE_BREAKS} a text may have")));
    }
    Ok(())
}

/// A box centred on the slide.
fn centred(doc: &OpenDoc, w: f64, h: f64) -> Result<[f64; 4], String> {
    let size = doc.deck()?.slide_size;
    Ok([(size.width - w) / 2.0, (size.height - h) / 2.0, w, h])
}

/// A chart's data as `insert.chart` and `chart.data` take it, from the
/// call's `categories` and `series`.
fn chart_data(a: &Json, m: &str, needed: bool) -> Result<Option<(Json, Json)>, String> {
    let (cats, series) = (args::present(a, "categories"), args::present(a, "series"));
    if cats.is_none() && series.is_none() && !needed {
        return Ok(None);
    }
    let cats = cats.and_then(Json::as_array).ok_or_else(|| invalid(format!("{m} needs `categories`, a list of labels")))?;
    let categories: Vec<String> = cats
        .iter()
        .map(|c| match c {
            Json::String(s) if s.chars().count() <= 256 => Ok(s.clone()),
            Json::Number(n) => Ok(n.to_string()),
            _ => Err(invalid(format!("{m}: `categories` holds labels of at most 256 characters"))),
        })
        .collect::<Result<_, _>>()?;
    let series = series.and_then(Json::as_array).ok_or_else(|| invalid(format!("{m} needs `series`, a list of {{name, values}}")))?;
    if series.is_empty() {
        return Err(invalid(format!("{m}: `series` holds at least one series")));
    }
    let series: Vec<Json> = series
        .iter()
        .map(|s| {
            args::only(s, &["name", "values"], m)?;
            let name = args::opt_str(s, "name", m, 256)?.unwrap_or("Series");
            let values = args::present(s, "values").and_then(Json::as_array).ok_or_else(|| invalid(format!("{m}: each series has `values`, a list of numbers")))?;
            let values: Vec<Json> = values
                .iter()
                .map(|v| match v {
                    Json::Null => Ok(Json::Null),
                    v => v.as_f64().filter(|f| f.is_finite()).map(|f| json!(f)).ok_or_else(|| invalid(format!("{m}: a series' values are numbers (or null)"))),
                })
                .collect::<Result<_, _>>()?;
            Ok(json!({ "name": name, "values": values }))
        })
        .collect::<Result<_, String>>()?;
    Ok(Some((json!(categories), json!(series))))
}

/// `deck.insert {doc, slide, kind, …}`.
pub(crate) fn insert(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.insert";
    let kind = args::need_str(a, "kind", M)?;
    match kind {
        "text" => {
            args::only(a, &["doc", "slide", "kind", "rect", "text"], M)?;
            let slide = args::id(a, "slide", M)?;
            let rect = args::opt_rect(a, "rect", M)?;
            let text = args::opt_str(a, "text", M, MAX_BODY)?.unwrap_or("");
            check_breaks(text, M)?;
            docs::with_doc(a, cx, M, |doc| {
                let i = on_slide(doc, slide, &[])?;
                let rect = match rect {
                    Some(r) => r,
                    None => centred(doc, 216.0, 36.0)?,
                };
                let cmds = vec![("slide.go", json!({ "index": i })), ("insert.textBox", json!({ "rect": rect, "text": text }))];
                inserted(doc, cx, cmds, "Insert text box", |_, _| Ok(json!({})))
            })
        }
        "shape" => {
            args::only(a, &["doc", "slide", "kind", "preset", "rect", "text"], M)?;
            let slide = args::id(a, "slide", M)?;
            let preset = args::need_str(a, "preset", M)?;
            let info = deckcraft_engine::geom::preset::info(preset).ok_or_else(|| invalid(format!("{M}: `preset` is a name from deck.shapes, not {preset:?}")))?;
            let rect = args::opt_rect(a, "rect", M)?;
            let text = args::opt_str(a, "text", M, MAX_BODY)?;
            if let Some(t) = text {
                check_breaks(t, M)?;
            }
            docs::with_doc(a, cx, M, |doc| {
                let i = on_slide(doc, slide, &[])?;
                // The service centres a default box: a line is as wide and
                // has no height.
                let rect = match rect {
                    Some(r) => r,
                    None if deckcraft_engine::geom::preset::is_line_like(info.name) => centred(doc, 144.0, 0.0)?,
                    None => centred(doc, 144.0, 144.0)?,
                };
                let mut params = json!({ "preset": info.name, "rect": rect });
                if let Some(t) = text {
                    params["text"] = json!(t);
                }
                let cmds = vec![("slide.go", json!({ "index": i })), ("shape.insert", params)];
                inserted(doc, cx, cmds, "Insert shape", |_, _| Ok(json!({})))
            })
        }
        "picture" => {
            args::only(a, &["doc", "slide", "kind", "path", "rect"], M)?;
            let slide = args::id(a, "slide", M)?;
            let path = args::need_str(a, "path", M)?;
            let rect = args::opt_rect(a, "rect", M)?;
            let rel = pictures::check(cx.area, path, "path", M)?;
            docs::with_doc(a, cx, M, |doc| {
                let i = on_slide(doc, slide, &[])?;
                let mut params = json!({ "path": rel });
                if let Some(r) = rect {
                    params["rect"] = json!(r);
                }
                let cmds = vec![("slide.go", json!({ "index": i })), ("insert.picture", params)];
                inserted(doc, cx, cmds, "Insert picture", |doc, id| {
                    let deck = doc.deck()?;
                    let x = deck.slides[i].shape(ShapeId(id)).and_then(|s| s.xfrm).unwrap_or_default();
                    Ok(json!({ "size": [round(x.w), round(x.h)] }))
                })
            })
        }
        "table" => {
            args::only(a, &["doc", "slide", "kind", "rows", "cols", "rect", "cells", "style"], M)?;
            let slide = args::id(a, "slide", M)?;
            let rows = args::opt_int_in(a, "rows", M, 1, 75)?.ok_or_else(|| invalid(format!("{M} needs `rows`, 1 to 75")))?;
            let cols = args::opt_int_in(a, "cols", M, 1, 75)?.ok_or_else(|| invalid(format!("{M} needs `cols`, 1 to 75")))?;
            let rect = args::opt_rect(a, "rect", M)?;
            let style = table_style(a, M)?;
            let data = match args::present(a, "cells") {
                Some(cells) => Some(fill_cells(cells, rows as usize, cols as usize, M)?),
                None => None,
            };
            docs::with_doc(a, cx, M, |doc| {
                let i = on_slide(doc, slide, &[])?;
                let mut params = json!({ "rows": rows, "cols": cols });
                if let Some(r) = rect {
                    params["rect"] = json!(r);
                }
                if let Some(d) = &data {
                    params["data"] = d.clone();
                }
                let mut cmds = vec![("slide.go", json!({ "index": i })), ("insert.table", params)];
                // The new table is the selection: the style goes on it.
                if let Some(s) = &style {
                    cmds.push(("table.style", json!({ "style": s })));
                }
                inserted(doc, cx, cmds, "Insert table", |_, _| Ok(json!({})))
            })
        }
        "chart" => {
            args::only(a, &["doc", "slide", "kind", "type", "categories", "series", "rect", "title"], M)?;
            let slide = args::id(a, "slide", M)?;
            let chart = args::need_choice(a, "type", M, &CHART_TYPES)?;
            let (categories, series) = chart_data(a, M, true)?.unwrap_or_default();
            let rect = args::opt_rect(a, "rect", M)?;
            let title = args::opt_str(a, "title", M, 256)?;
            docs::with_doc(a, cx, M, |doc| {
                let i = on_slide(doc, slide, &[])?;
                let mut params = json!({ "type": chart, "categories": categories, "series": series });
                if let Some(r) = rect {
                    params["rect"] = json!(r);
                }
                let mut cmds = vec![("slide.go", json!({ "index": i })), ("insert.chart", params)];
                // The new chart is the selection: the title goes on it.
                if let Some(t) = title {
                    cmds.push(("chart.options", json!({ "title": t })));
                }
                inserted(doc, cx, cmds, "Insert chart", |_, _| Ok(json!({})))
            })
        }
        // What Cut or Copy kept (`deck.shape`, `deck.slides_op`): shapes
        // onto the slide, or slides after it. A clipboard token names this
        // caller's retained copy, even after its source deck was closed.
        "paste" => {
            args::only(a, &["doc", "slide", "kind", "clipboard"], M)?;
            let slide = args::id(a, "slide", M)?;
            let token = args::opt_int_in(a, "clipboard", M, 1, i64::MAX)?.map(|id| id as u64);
            let snapshot = token.map(|id| docs::copied(cx, id)).transpose()?;
            docs::with_doc(a, cx, M, |doc| {
                let i = on_slide(doc, slide, &[])?;
                let before: Vec<u32> = doc.deck()?.slides.iter().map(|s| s.id.0).collect();
                let cmds = vec![("slide.go", json!({ "index": i })), ("edit.paste", json!({}))];
                let same_origin = snapshot.as_ref().is_some_and(|copy| copy.origin == doc.handle);
                let transfer = match &snapshot {
                    Some(copy) if same_origin => Some((copy.clipboard.clone(), None)),
                    Some(copy) => {
                        let target = doc.deck()?;
                        let repeats = docs::copied_on(cx, copy.token, &doc.handle, slide)?;
                        let offset = doc.session.prefs.duplicate_offset * f64::from(repeats);
                        let (clipboard, next_id) = cross_deck_clipboard(&copy.source, copy.slide, &target, slide, &copy.clipboard, offset)?;
                        Some((clipboard, Some(next_id)))
                    }
                    None => None,
                };
                let (old_clipboard, reserve) = match transfer {
                    Some((clipboard, reserve)) => (Some(std::mem::replace(&mut doc.session.clipboard, clipboard)), reserve),
                    None => (None, None),
                };
                let result = gate::change_with_hooks(
                    doc, cx.area, Plan { method: M, most: INSERT, cmds }, "Paste",
                    |doc| {
                        if let Some(next_id) = reserve {
                            let state = doc.st_mut()?;
                            if next_id > state.doc.next_id {
                                Arc::make_mut(&mut state.doc).next_id = next_id;
                            }
                        }
                        Ok(())
                    },
                    |doc, results| {
                        if let Some(copy) = &snapshot {
                            repair_pasted_references(doc, &copy.clipboard, slide, &before, results, same_origin)?;
                        }
                        Ok(())
                    },
                );
                if let Some(clipboard) = old_clipboard {
                    if same_origin && result.is_ok() {
                        if let Some(token) = token { docs::copied_pastes(cx, token, doc.session.clipboard.pastes); }
                    } else {
                        if result.is_ok() && snapshot.as_ref().is_some_and(|copy| !copy.clipboard.shapes.is_empty()) {
                            if let Some(token) = token { docs::copied_on_advance(cx, token, &doc.handle, slide); }
                        }
                        doc.session.clipboard = clipboard;
                    }
                }
                let done = result?;
                let result = done.results.get(1).cloned().unwrap_or_default();
                let extra = if result.get("slides").is_some() {
                    let made: Vec<u32> = doc.deck()?.slides.iter().map(|s| s.id.0).filter(|id| !before.contains(id)).collect();
                    json!({ "ids": [], "slides": made })
                } else {
                    json!({ "ids": result.get("ids").cloned().unwrap_or_else(|| json!([])), "slides": [] })
                };
                doc.answer(&done.changed, extra)
            })
        }
        other => Err(invalid(format!("{M}: `kind` is text, shape, picture, table, chart or paste, not {other:?}"))),
    }
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Run an insert's `cmds` (the insert second): the common answer with the
/// new shape's `id` and what `extra` adds.
fn inserted(doc: &mut OpenDoc, cx: &Ctx, cmds: Vec<(&'static str, Json)>, label: &str, extra: impl FnOnce(&OpenDoc, u32) -> Result<Json, String>) -> Result<Json, String> {
    let done = gate::change(doc, cx.area, Plan { method: "deck.insert", most: INSERT, cmds }, label)?;
    let id = done.results.get(1).and_then(|r| r["id"].as_u64()).map(|n| n as u32).ok_or_else(|| invalid("deck.insert: the engine made no shape"))?;
    let mut more = extra(doc, id)?;
    more["id"] = json!(id);
    doc.answer(&done.changed, more)
}

/// A table style name, as the engine names them (`medium2-accent1`).
fn table_style(a: &Json, m: &str) -> Result<Option<String>, String> {
    match args::opt_str(a, "style", m, 64)? {
        Some(s) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') => Ok(Some(s.to_string())),
        Some(_) => Err(invalid(format!("{m}: `style` is a table style name such as medium2-accent1"))),
        None => Ok(None),
    }
}

/// A fill's cells as `insert.table`'s `data`: rows of text, within the table,
/// at most [`MAX_FILL_CELLS`] cells.
fn fill_cells(cells: &Json, rows: usize, cols: usize, m: &str) -> Result<Json, String> {
    let wrong = || invalid(format!("{m}: `cells` is rows of cell text, within the table"));
    let list = cells.as_array().ok_or_else(wrong)?;
    if list.len() > rows {
        return Err(wrong());
    }
    let mut n = 0usize;
    let mut data = Vec::with_capacity(list.len());
    for row in list {
        let row = row.as_array().filter(|r| r.len() <= cols).ok_or_else(wrong)?;
        let mut out = Vec::with_capacity(row.len());
        for cell in row {
            let text = match cell {
                Json::Null => String::new(),
                Json::String(s) => s.clone(),
                Json::Number(n) => n.to_string(),
                _ => return Err(wrong()),
            };
            if text.chars().count() > MAX_CELL_TEXT {
                return Err(too_large(format!("{m}: a cell's text is at most {MAX_CELL_TEXT} characters")));
            }
            check_breaks(&text, m)?;
            n += 1;
            out.push(json!(text));
        }
        data.push(json!(out));
    }
    if n > MAX_FILL_CELLS {
        return Err(too_large(format!("{m}: `cells` fills {n} cells, more than the {MAX_FILL_CELLS} one call fills: set the rest with deck.set_text")));
    }
    Ok(json!(data))
}

/// `slide.go`, `text.edit`, `text.select`, `text.insert`, `text.select` and
/// `text.delete` (the replaced span's first character, last), `text.exit`.
const SET_TEXT: usize = 7;

/// A flat character offset in `body`'s text (`\n` between paragraphs) as
/// the engine's (paragraph, character).
fn position(lens: &[usize], mut offset: usize) -> [usize; 2] {
    for (p, len) in lens.iter().enumerate() {
        if offset <= *len {
            return [p, offset];
        }
        offset -= len + 1;
    }
    let last = lens.len().saturating_sub(1);
    [last, lens.last().copied().unwrap_or(0)]
}

/// `deck.set_text {doc, slide, id, text, cell?}` (SERVICE.md "deck.set_text
/// keeps formatting"): the service compares the box's text with `text` and
/// replaces only the span that changed, so the rest of the box keeps its
/// runs, and the new text takes the formatting of the span's first
/// character. A box with no text yet gets its text with `text.set`, unless
/// a paragraph would start with a tab, which `text.set` takes for an
/// outline level: the text stays exactly as typed either way.
pub(crate) fn set_text(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.set_text";
    args::only(a, &["doc", "slide", "id", "text", "cell"], M)?;
    let slide = args::id(a, "slide", M)?;
    let id = args::id(a, "id", M)?;
    let text = args::need_text(a, "text", M, MAX_SET_TEXT)?.replace("\r\n", "\n").replace('\r', "\n");
    let cell = args::opt_cell(a, "cell", M)?;
    check_breaks(&text, M)?;
    docs::with_doc(a, cx, M, |doc| {
        let i = doc.slide_index(slide)?;
        let sh = shape_of(doc, i, id)?;
        let body = match (&sh.kind, cell) {
            (ShapeKind::Table(t), Some([r, c])) => t.cell(r, c).map(|x| x.text.clone()).ok_or_else(|| invalid(format!("{M}: table {id} has no cell [{r}, {c}]")))?,
            (ShapeKind::Table(_), None) => return Err(invalid(format!("{M}: say which `cell` of table {id}"))),
            (ShapeKind::Shape | ShapeKind::Connector { .. }, None) => sh.text.clone().unwrap_or_default(),
            (_, Some(_)) => return Err(invalid(format!("{M}: shape {id} is not a table"))),
            _ => return Err(invalid(format!("{M}: shape {id} doesn't hold text"))),
        };
        let old: Vec<char> = body.text().chars().collect();
        let new: Vec<char> = text.chars().collect();
        if old == new {
            return doc.answer(&[], json!({ "id": id }));
        }
        let mut cmds = vec![("slide.go", json!({ "index": i }))];
        let tabbed = text.split('\n').any(|p| p.starts_with('\t'));
        if old.is_empty() && !tabbed {
            let mut params = json!({ "id": id, "text": text });
            if let Some(c) = cell {
                params["cell"] = json!(c);
            }
            cmds.push(("text.set", params));
        } else {
            let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
            let room = old.len().min(new.len()) - prefix;
            let suffix = old.iter().rev().zip(new.iter().rev()).take(room).take_while(|(a, b)| a == b).count();
            let (start, end) = (prefix, old.len() - suffix);
            let replacement: String = new[prefix..new.len() - suffix].iter().collect();
            let lens: Vec<usize> = body.paragraphs.iter().map(|p| p.char_len()).collect();
            let (s, e) = (position(&lens, start), position(&lens, end));
            let mut edit = json!({ "id": id, "at": s });
            if let Some(c) = cell {
                edit["cell"] = json!(c);
            }
            let first_char_in_paragraph = start < end && s[1] < lens.get(s[0]).copied().unwrap_or(0);
            if first_char_in_paragraph && !replacement.is_empty() {
                // Replace all of the span but its first character, then that
                // character: the new text is typed after it, in its format.
                let after_first = [s[0], s[1] + 1];
                edit["at"] = json!(after_first);
                cmds.push(("text.edit", edit));
                cmds.push(("text.select", json!({ "anchor": after_first, "caret": e })));
                cmds.push(("text.insert", json!({ "text": replacement })));
                cmds.push(("text.select", json!({ "anchor": s, "caret": after_first })));
                cmds.push(("text.delete", json!({})));
            } else {
                cmds.push(("text.edit", edit));
                cmds.push(("text.select", json!({ "anchor": s, "caret": e })));
                if replacement.is_empty() {
                    cmds.push(("text.delete", json!({})));
                } else {
                    cmds.push(("text.insert", json!({ "text": replacement })));
                }
            }
            cmds.push(("text.exit", json!({})));
        }
        let done = gate::change(doc, cx.area, Plan { method: M, most: SET_TEXT, cmds }, "Edit text")?;
        doc.answer(&done.changed, json!({ "id": id }))
    })
}

/// `slide.go`, `text.edit` and `text.select` (or `edit.select`), the 17
/// `format.*` commands a call may set, up to 8 more `format.indent` or
/// `format.outdent` for `level`, and `text.exit`.
const FORMAT: usize = 3 + 17 + 8 + 1;
/// The alignments, the anchors and the autofit modes `deck.format` takes.
const ALIGNS: [&str; 4] = ["left", "center", "right", "justify"];
const ANCHORS: [&str; 3] = ["top", "middle", "bottom"];
const AUTOFITS: [&str; 3] = ["none", "shrink", "resize"];

/// `deck.format {doc, slide, ids, paragraphs?, …}`: run, paragraph and box
/// formatting of whole boxes, or of one box's paragraphs `[first, last]`.
/// `level` moves the paragraphs that many outline levels in (`format.indent`)
/// or, negative, out (`format.outdent`).
pub(crate) fn format(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.format";
    let keys = [
        "doc", "slide", "ids", "paragraphs", "font", "size", "bold", "italic", "underline", "strike", "color", "highlight", "align", "bullets", "numbering", "level",
        "line_spacing", "space_before", "space_after", "anchor", "autofit", "columns", "margins",
    ];
    args::only(a, &keys, M)?;
    let slide = args::id(a, "slide", M)?;
    let ids = args::ids(a, "ids", M, MAX_SHAPES)?;
    let paragraphs = match args::present(a, "paragraphs") {
        Some(v) => Some(args::cell_of(v).filter(|[f, l]| f <= l).ok_or_else(|| invalid(format!("{M}: `paragraphs` is [first, last], from 0")))?),
        None => None,
    };
    if paragraphs.is_some() && ids.len() != 1 {
        return Err(invalid(format!("{M}: `paragraphs` formats one box: give one id")));
    }
    let mut fmt: Vec<(&'static str, Json)> = Vec::new();
    if let Some(f) = args::opt_str(a, "font", M, 128)? {
        if f.trim().is_empty() {
            return Err(invalid(format!("{M}: `font` names a family")));
        }
        fmt.push(("format.font", json!({ "family": f })));
    }
    if let Some(s) = args::opt_num(a, "size", M, 1.0, 4000.0)? {
        fmt.push(("format.size", json!({ "size": s })));
    }
    for (key, id) in [("bold", "format.bold"), ("italic", "format.italic"), ("underline", "format.underline"), ("strike", "format.strikethrough")] {
        if let Some(on) = args::opt_bool(a, key, M)? {
            fmt.push((id, json!({ "on": on })));
        }
    }
    if let Some(c) = args::opt_color(a, "color", M)? {
        fmt.push(("format.color", json!({ "color": c })));
    }
    match args::present(a, "highlight") {
        Some(Json::String(s)) if s == "none" => fmt.push(("format.highlight", json!({}))),
        Some(_) => fmt.push(("format.highlight", json!({ "color": args::opt_color(a, "highlight", M)? }))),
        None => {}
    }
    if let Some(al) = args::opt_choice(a, "align", M, &ALIGNS)? {
        fmt.push(("format.align", json!({ "align": al })));
    }
    if let Some(on) = args::opt_bool(a, "bullets", M)? {
        fmt.push(("format.bullets", json!({ "on": on })));
    }
    if let Some(on) = args::opt_bool(a, "numbering", M)? {
        fmt.push(("format.numbering", json!({ "on": on })));
    }
    if let Some(level) = args::opt_int_in(a, "level", M, -8, 8)? {
        let id = if level > 0 { "format.indent" } else { "format.outdent" };
        for _ in 0..level.unsigned_abs() {
            fmt.push((id, json!({})));
        }
    }
    if let Some(lines) = args::opt_num(a, "line_spacing", M, 0.5, 9.99)? {
        fmt.push(("format.lineSpacing", json!({ "lines": lines })));
    }
    let (before, after) = (args::opt_num(a, "space_before", M, 0.0, 1584.0)?, args::opt_num(a, "space_after", M, 0.0, 1584.0)?);
    if before.is_some() || after.is_some() {
        let mut p = json!({});
        if let Some(v) = before {
            p["spaceBefore"] = json!(v);
        }
        if let Some(v) = after {
            p["spaceAfter"] = json!(v);
        }
        fmt.push(("format.paragraph", p));
    }
    if let Some(an) = args::opt_choice(a, "anchor", M, &ANCHORS)? {
        fmt.push(("format.anchor", json!({ "anchor": an })));
    }
    if let Some(mode) = args::opt_choice(a, "autofit", M, &AUTOFITS)? {
        fmt.push(("format.autofit", json!({ "mode": mode })));
    }
    if let Some(n) = args::opt_int_in(a, "columns", M, 1, 16)? {
        fmt.push(("format.columns", json!({ "count": n })));
    }
    if let Some(m) = args::present(a, "margins") {
        args::only(m, &["left", "top", "right", "bottom"], "deck.format margins")?;
        let mut p = json!({});
        for side in ["left", "top", "right", "bottom"] {
            if let Some(v) = args::opt_num(m, side, "deck.format margins", 0.0, 1584.0)? {
                p[side] = json!(v);
            }
        }
        fmt.push(("format.margins", p));
    }
    if fmt.is_empty() {
        return Err(invalid(format!("{M}: name at least one formatting to change")));
    }
    docs::with_doc(a, cx, M, |doc| {
        let i = on_slide(doc, slide, &ids)?;
        let mut cmds = vec![("slide.go", json!({ "index": i }))];
        match paragraphs {
            Some([first, last]) => {
                let sh = shape_of(doc, i, ids[0])?;
                let lens: Vec<usize> = match (&sh.kind, &sh.text) {
                    (ShapeKind::Shape | ShapeKind::Connector { .. }, Some(t)) => t.paragraphs.iter().map(|p| p.char_len()).collect(),
                    _ => Vec::new(),
                };
                if last >= lens.len() {
                    return Err(invalid(format!("{M}: shape {} has {} paragraphs, so `paragraphs` ends below {}", ids[0], lens.len(), lens.len())));
                }
                cmds.push(("text.edit", json!({ "id": ids[0], "at": [first, 0] })));
                cmds.push(("text.select", json!({ "anchor": [first, 0], "caret": [last, lens[last]] })));
                cmds.extend(fmt);
                cmds.push(("text.exit", json!({})));
            }
            None => {
                cmds.push(("edit.select", json!({ "ids": ids })));
                cmds.extend(fmt);
            }
        }
        let done = gate::change(doc, cx.area, Plan { method: M, most: FORMAT, cmds }, "Format text")?;
        doc.answer(&done.changed, json!({}))
    })
}

/// `slide.go`, for Cut and Copy `edit.select` (they act on the selection),
/// and the op's one command.
const SHAPE: usize = 3;

/// `deck.shape {doc, slide, ids, op, …}`.
pub(crate) fn shape(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.shape";
    let op = args::need_str(a, "op", M)?;
    let slide = args::id(a, "slide", M)?;
    let ids = args::ids(a, "ids", M, MAX_SHAPES)?;
    let only = |keys: &[&str]| {
        let mut all = vec!["doc", "slide", "ids", "op"];
        all.extend_from_slice(keys);
        args::only(a, &all, M)
    };
    let (id, mut params, label): (&'static str, Json, &str) = match op {
        "bounds" => {
            only(&["x", "y", "w", "h"])?;
            if ids.len() != 1 {
                return Err(invalid(format!("{M}: `bounds` sets one shape's box: give one id")));
            }
            let n = |k: &str| args::need_num(a, k, M, -1.0e9, 1.0e9);
            let (w, h) = (n("w")?, n("h")?);
            if w < 0.0 || h < 0.0 {
                return Err(invalid(format!("{M}: `w` and `h` are 0 or more")));
            }
            ("shape.setBounds", json!({ "x": n("x")?, "y": n("y")?, "w": w, "h": h }), "Move and size")
        }
        "move" => {
            only(&["dx", "dy"])?;
            let dx = args::opt_num(a, "dx", M, -1.0e9, 1.0e9)?.unwrap_or(0.0);
            let dy = args::opt_num(a, "dy", M, -1.0e9, 1.0e9)?.unwrap_or(0.0);
            ("shape.move", json!({ "dx": dx, "dy": dy }), "Move")
        }
        "rotate" => {
            only(&["deg"])?;
            ("shape.rotate", json!({ "deg": args::need_num(a, "deg", M, -36_000.0, 36_000.0)? }), "Rotate")
        }
        "flip" => {
            only(&["axis"])?;
            ("shape.flip", json!({ "axis": args::need_choice(a, "axis", M, &["horizontal", "vertical"])? }), "Flip")
        }
        "order" => {
            only(&["to"])?;
            let id = match args::need_choice(a, "to", M, &["front", "back", "forward", "backward"])? {
                "front" => "arrange.bringToFront",
                "back" => "arrange.sendToBack",
                "forward" => "arrange.bringForward",
                _ => "arrange.sendBackward",
            };
            (id, json!({}), "Arrange")
        }
        "align" => {
            only(&["edge", "to"])?;
            let mut p = json!({ "edge": args::need_choice(a, "edge", M, &["left", "center", "right", "top", "middle", "bottom"])? });
            if let Some(to) = args::opt_choice(a, "to", M, &["slide", "selection"])? {
                p["to"] = json!(to);
            }
            ("arrange.align", p, "Align")
        }
        "distribute" => {
            only(&["dir"])?;
            ("arrange.distribute", json!({ "dir": args::need_choice(a, "dir", M, &["horizontal", "vertical"])? }), "Distribute")
        }
        "group" => {
            only(&[])?;
            if ids.len() < 2 {
                return Err(invalid(format!("{M}: `group` takes two or more shapes")));
            }
            ("arrange.group", json!({}), "Group")
        }
        "ungroup" => {
            only(&[])?;
            ("arrange.ungroup", json!({}), "Ungroup")
        }
        "duplicate" => {
            only(&[])?;
            ("edit.duplicate", json!({}), "Duplicate")
        }
        "delete" => {
            only(&[])?;
            ("edit.delete", json!({}), "Delete")
        }
        // The deck's own clipboard (its engine session's): `deck.insert
        // {kind: "paste"}` puts the shapes back, on any of its slides.
        "copy" => {
            only(&[])?;
            ("edit.copy", json!({}), "Copy")
        }
        "cut" => {
            only(&[])?;
            ("edit.cut", json!({}), "Cut")
        }
        "fill" => {
            only(&["color", "gradient", "none", "transparency"])?;
            let mut p = json!({});
            let color = args::opt_color(a, "color", M)?;
            let none = args::opt_bool(a, "none", M)?.unwrap_or(false);
            let gradient = match args::present(a, "gradient") {
                Some(g) => Some(gradient(g, M)?),
                None => None,
            };
            match (color, gradient, none) {
                (Some(c), None, false) => p["color"] = json!(c),
                (None, Some(g), false) => p["gradient"] = g,
                (None, None, true) => p["none"] = json!(true),
                (None, None, false) => {}
                _ => return Err(invalid(format!("{M}: a fill is one of `color`, `gradient` or `none`"))),
            }
            if let Some(t) = args::opt_num(a, "transparency", M, 0.0, 1.0)? {
                p["transparency"] = json!(t);
            }
            if p.as_object().is_some_and(|o| o.is_empty()) {
                return Err(invalid(format!("{M}: give `color`, `gradient`, `none` or `transparency`")));
            }
            ("shape.fill", p, "Fill")
        }
        "line" => {
            only(&["color", "width", "dash", "none"])?;
            let mut p = json!({});
            if let Some(c) = args::opt_color(a, "color", M)? {
                p["color"] = json!(c);
            }
            if let Some(w) = args::opt_num(a, "width", M, 0.0, 1584.0)? {
                p["width"] = json!(w);
            }
            if let Some(d) = args::opt_choice(a, "dash", M, &["solid", "dot", "dash", "lgDash", "dashDot", "sysDot", "sysDash"])? {
                p["dash"] = json!(d);
            }
            if args::opt_bool(a, "none", M)? == Some(true) {
                p["none"] = json!(true);
            }
            if p.as_object().is_some_and(|o| o.is_empty()) {
                return Err(invalid(format!("{M}: give `color`, `width`, `dash` or `none`")));
            }
            ("shape.line", p, "Outline")
        }
        "effects" => {
            only(&["shadow", "glow", "soft_edges", "reflection"])?;
            let mut p = json!({});
            if let Some(s) = args::opt_choice(a, "shadow", M, &["none", "outer", "inner", "perspective"])? {
                p["shadow"] = json!(s);
            }
            match args::present(a, "glow") {
                Some(Json::String(s)) if s == "none" => p["glow"] = json!("none"),
                Some(_) => p["glow"] = json!(args::need_num(a, "glow", M, 0.0, 150.0)?),
                None => {}
            }
            if let Some(v) = args::opt_num(a, "soft_edges", M, 0.0, 100.0)? {
                p["softEdges"] = json!(v);
            }
            if let Some(r) = args::opt_choice(a, "reflection", M, &["none", "tight", "half", "full"])? {
                p["reflection"] = json!(r);
            }
            if p.as_object().is_some_and(|o| o.is_empty()) {
                return Err(invalid(format!("{M}: give `shadow`, `glow`, `soft_edges` or `reflection`")));
            }
            ("shape.effects", p, "Effects")
        }
        "style" => {
            only(&["index"])?;
            ("shape.quickStyle", json!({ "index": args::opt_int_in(a, "index", M, 0, 41)?.ok_or_else(|| invalid(format!("{M} needs `index`, 0 to 41")))? }), "Quick style")
        }
        "lock" => {
            only(&["locked"])?;
            ("shape.lock", json!({ "locked": args::need_bool(a, "locked", M)? }), "Lock")
        }
        "hide" => {
            only(&["hidden"])?;
            ("shape.visible", json!({ "visible": !args::need_bool(a, "hidden", M)? }), "Show or hide")
        }
        other => {
            return Err(invalid(format!(
                "{M}: `op` is bounds, move, rotate, flip, order, align, distribute, group, ungroup, duplicate, delete, copy, cut, fill, line, effects, style, lock or hide, not {other:?}"
            )))
        }
    };
    params["ids"] = json!(ids);
    docs::with_doc(a, cx, M, |doc| {
        let i = on_slide(doc, slide, &ids)?;
        let copy_source = if matches!(id, "edit.copy" | "edit.cut") { Some(doc.deck()?) } else { None };
        // Group and Duplicate also turn placeholders into ordinary shapes.
        // Keep each selected placeholder's effective look before the engine
        // removes its link to the layout, without changing the original.
        let looks = if matches!(id, "arrange.group" | "edit.duplicate") {
            let deck = doc.deck()?;
            let ctx = resolve::Ctx::for_slide(&deck, &deck.slides[i]);
            deck.slides[i].shapes.iter().filter(|shape| ids.contains(&shape.id.0)).map(|shape| {
                (shape.id.0, ctx.as_ref().filter(|_| shape.ph.is_some()).map(|ctx| with_its_look(ctx, shape)))
            }).collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut cmds = vec![("slide.go", json!({ "index": i }))];
        if matches!(id, "edit.copy" | "edit.cut") {
            cmds.push(("edit.select", json!({ "ids": ids })));
        }
        cmds.push((id, params));
        let plan = Plan { method: M, most: SHAPE, cmds };
        let done = if looks.iter().any(|(_, look)| look.is_some()) {
            gate::change_with_checked_finish(doc, cx.area, plan, label, |doc, results| {
                let target_ids: Vec<u32> = if id == "edit.duplicate" {
                    results.last().and_then(|result| result["ids"].as_array()).ok_or_else(|| invalid("deck.shape: the duplicated items could not be read"))?
                        .iter().map(|id| id.as_u64().and_then(|id| u32::try_from(id).ok()).ok_or_else(|| invalid("deck.shape: the duplicated items could not be read")))
                        .collect::<Result<_, _>>()?
                } else {
                    looks.iter().map(|(id, _)| *id).collect()
                };
                if target_ids.len() != looks.len() {
                    return Err(invalid("deck.shape: the edited items could not be read"));
                }
                let state = doc.st_mut()?;
                let deck = Arc::make_mut(&mut state.doc);
                let slide = Arc::make_mut(deck.slides.get_mut(i).ok_or_else(|| invalid("deck.shape: the edited slide could not be read"))?);
                for (target, (_, look)) in target_ids.into_iter().zip(looks) {
                    let Some(look) = look else { continue };
                    let shape = slide.shape_mut(ShapeId(target)).ok_or_else(|| invalid("deck.shape: the edited item could not be read"))?;
                    shape.fill = look.fill;
                    shape.line = look.line;
                    shape.effects = look.effects;
                    shape.style = look.style;
                    shape.text = look.text;
                }
                Ok(())
            })?
        } else {
            gate::change(doc, cx.area, plan, label)?
        };
        if matches!(id, "edit.copy" | "edit.cut") {
            keep_look(doc, i)?;
        }
        let result = done.results.last().cloned().unwrap_or_default();
        let out_ids = match id {
            "arrange.group" => json!([result["id"]]),
            "arrange.ungroup" | "edit.duplicate" => result["ids"].clone(),
            "edit.delete" | "edit.cut" => json!([]),
            _ => json!(ids),
        };
        let mut answer = doc.answer(&done.changed, json!({ "ids": out_ids }))?;
        if let Some(source) = copy_source {
            answer["clipboard"] = json!(docs::remember_copy(cx, doc, source, slide));
        }
        Ok(answer)
    })
}

/// A placeholder that goes to the clipboard pastes as a plain shape (the engine
/// cuts its tie to the layout), so it takes along the look the layout and the
/// master gave it on slide `slide`: its box, fill, outline, text body,
/// paragraph and run styles. Keynote's copy of a title keeps the title's font,
/// size and alignment the same way.
fn keep_look(doc: &mut OpenDoc, slide: usize) -> Result<(), String> {
    let deck = doc.deck()?;
    let Some(ctx) = deck.slides.get(slide).and_then(|s| resolve::Ctx::for_slide(&deck, s)) else {
        return Ok(());
    };
    let shapes: Vec<Shape> = doc.session.clipboard.shapes.iter().map(|sh| with_its_look(&ctx, sh)).collect();
    doc.session.clipboard.shapes = shapes;
    Ok(())
}

fn with_its_look(ctx: &resolve::Ctx, sh: &Shape) -> Shape {
    let mut out = sh.clone();
    if sh.ph.is_none() {
        return out;
    }
    out.xfrm = Some(resolve::xfrm(ctx, sh));
    // A fill or effects the theme gives by the shape's own style stay with that style.
    if let (None, (Some(fill), None)) = (&sh.fill, resolve::fill(ctx, sh)) {
        out.fill = Some(fill);
    }
    if let (None, (Some(effects), None)) = (&sh.effects, resolve::effects(ctx, sh)) {
        out.effects = Some(effects);
    }
    let (layout, master) = resolve::parents(ctx, sh);
    // A style on a parent supplies theme fill, line and effects through a
    // placeholder colour. Once pasted the engine removes `ph`, so retain the
    // style reference as well as the directly inherited properties. Keeping
    // the reference lets the pasted shape follow later theme changes.
    if out.style.is_none() {
        if let Some(style) = layout.and_then(|parent| parent.style.as_ref()) {
            out.style = Some(style.clone());
        } else if let Some(style) = master.and_then(|parent| parent.style.as_ref()) {
            let mut style = style.clone();
            // Text resolution uses a shape's or layout's font reference, not
            // the master placeholder's. Do not introduce a new text colour.
            style.font_ref = ("none".into(), None);
            out.style = Some(style);
        }
    }
    if sh.line.is_none() && [layout, master].into_iter().flatten().any(|p| p.line.is_some()) {
        out.line = Some(resolve::line(ctx, sh).0);
    }
    if let Some(text) = out.text.as_mut() {
        text.body = resolve::body(ctx, sh);
        for p in &mut text.paragraphs {
            let para = p.clone();
            p.props = resolve::para(ctx, sh, &para);
            for r in &mut p.runs {
                r.props = resolve::run(ctx, sh, &para, &r.props);
            }
            p.end_props = resolve::run(ctx, sh, &para, &para.end_props);
        }
    }
    out
}

/// A gradient as `shape.fill` and `design.background` take it: 2 to 10
/// stops `[position 0–1, colour]`, an angle, and linear or radial.
pub(crate) fn gradient(g: &Json, m: &str) -> Result<Json, String> {
    args::only(g, &["stops", "angle", "kind"], m)?;
    let wrong = || invalid(format!("{m}: a gradient's `stops` are 2 to 10 [position 0 to 1, colour]"));
    let stops = g.get("stops").and_then(Json::as_array).filter(|s| (2..=10).contains(&s.len())).ok_or_else(wrong)?;
    let stops: Vec<Json> = stops
        .iter()
        .map(|s| {
            let pair = s.as_array().filter(|p| p.len() == 2).ok_or_else(wrong)?;
            let pos = pair[0].as_f64().filter(|p| (0.0..=1.0).contains(p)).ok_or_else(wrong)?;
            let color = args::color_value(&pair[1]).ok_or_else(wrong)?;
            Ok(json!([pos, color]))
        })
        .collect::<Result<_, String>>()?;
    let mut out = json!({ "stops": stops });
    if let Some(angle) = args::opt_num(g, "angle", m, 0.0, 360.0)? {
        out["angle"] = json!(angle);
    }
    if let Some(kind) = args::opt_choice(g, "kind", m, &["linear", "radial"])? {
        out["kind"] = json!(kind);
    }
    Ok(out)
}

/// `slide.go` and the op's one command.
const PICTURE: usize = 2;

/// `deck.picture {doc, slide, id, op, …}`.
pub(crate) fn picture(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.picture";
    let op = args::need_str(a, "op", M)?;
    let slide = args::id(a, "slide", M)?;
    let id = args::id(a, "id", M)?;
    let only = |keys: &[&str]| {
        let mut all = vec!["doc", "slide", "id", "op"];
        all.extend_from_slice(keys);
        args::only(a, &all, M)
    };
    let (cmd, mut params, label): (&'static str, Json, &str) = match op {
        "replace" => {
            only(&["path"])?;
            let rel = pictures::check(cx.area, args::need_str(a, "path", M)?, "path", M)?;
            ("picture.change", json!({ "path": rel }), "Replace picture")
        }
        "crop" => {
            only(&["left", "top", "right", "bottom"])?;
            let mut p = json!({});
            for side in ["left", "top", "right", "bottom"] {
                if let Some(v) = args::opt_num(a, side, M, 0.0, 0.95)? {
                    p[side] = json!(v);
                }
            }
            ("picture.crop", p, "Crop picture")
        }
        "adjust" => {
            only(&["brightness", "contrast", "saturation", "grayscale", "transparency"])?;
            let mut p = json!({});
            for (key, lo, hi) in [("brightness", -1.0, 1.0), ("contrast", -1.0, 1.0), ("saturation", 0.0, 4.0), ("transparency", 0.0, 1.0)] {
                if let Some(v) = args::opt_num(a, key, M, lo, hi)? {
                    p[key] = json!(v);
                }
            }
            if let Some(g) = args::opt_bool(a, "grayscale", M)? {
                p["grayscale"] = json!(g);
            }
            if p.as_object().is_some_and(|o| o.is_empty()) {
                return Err(invalid(format!("{M}: give `brightness`, `contrast`, `saturation`, `grayscale` or `transparency`")));
            }
            ("picture.adjust", p, "Adjust picture")
        }
        "reset" => {
            only(&[])?;
            ("picture.reset", json!({}), "Reset picture")
        }
        "alt_text" => {
            only(&["text", "decorative"])?;
            let mut p = json!({ "text": args::need_text(a, "text", M, 2_000)? });
            if let Some(d) = args::opt_bool(a, "decorative", M)? {
                p["decorative"] = json!(d);
            }
            ("shape.altText", p, "Alt text")
        }
        other => return Err(invalid(format!("{M}: `op` is replace, crop, adjust, reset or alt_text, not {other:?}"))),
    };
    params["ids"] = json!([id]);
    docs::with_doc(a, cx, M, |doc| {
        let i = doc.slide_index(slide)?;
        if !matches!(shape_of(doc, i, id)?.kind, ShapeKind::Picture { .. }) {
            return Err(invalid(format!("{M}: shape {id} is not a picture")));
        }
        let cmds = vec![("slide.go", json!({ "index": i })), (cmd, params)];
        let done = gate::change(doc, cx.area, Plan { method: M, most: PICTURE, cmds }, label)?;
        doc.answer(&done.changed, json!({}))
    })
}

/// `slide.go` and the op's one command.
const TABLE: usize = 2;

/// `deck.table {doc, slide, id, op, …}`: the table's `{rows, cols}` after.
pub(crate) fn table(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.table";
    let op = args::need_str(a, "op", M)?.to_string();
    let slide = args::id(a, "slide", M)?;
    let id = args::id(a, "id", M)?;
    let only = |keys: &[&str]| {
        let mut all = vec!["doc", "slide", "id", "op"];
        all.extend_from_slice(keys);
        args::only(a, &all, M)
    };
    match op.as_str() {
        "insert_row" => only(&["row", "below"])?,
        "insert_col" => only(&["col", "right"])?,
        "delete_row" | "row_height" => only(&["row", "height"])?,
        "delete_col" | "col_width" => only(&["col", "width"])?,
        "merge" => only(&["from", "to"])?,
        "split" => only(&["cell"])?,
        "style" => only(&["style"])?,
        "options" => only(&["header_row", "total_row", "banded_rows", "first_column", "last_column", "banded_columns"])?,
        "fill" => only(&["cells", "color"])?,
        other => {
            return Err(invalid(format!(
                "{M}: `op` is insert_row, insert_col, delete_row, delete_col, merge, split, style, options, fill, col_width or row_height, not {other:?}"
            )))
        }
    }
    docs::with_doc(a, cx, M, |doc| {
        let i = doc.slide_index(slide)?;
        let ShapeKind::Table(t) = shape_of(doc, i, id)?.kind else { return Err(invalid(format!("{M}: shape {id} is not a table"))) };
        let (rows, cols) = (t.n_rows(), t.n_cols());
        let row = |key: &str| -> Result<usize, String> {
            let r = args::need_index(a, key, M)?;
            if r >= rows {
                return Err(invalid(format!("{M}: table {id} has {rows} rows, so `{key}` is below {rows}")));
            }
            Ok(r)
        };
        let col = |key: &str| -> Result<usize, String> {
            let c = args::need_index(a, key, M)?;
            if c >= cols {
                return Err(invalid(format!("{M}: table {id} has {cols} columns, so `{key}` is below {cols}")));
            }
            Ok(c)
        };
        let cell = |key: &str| -> Result<[usize; 2], String> {
            let c = args::opt_cell(a, key, M)?.ok_or_else(|| invalid(format!("{M} needs `{key}`, [row, column]")))?;
            if c[0] >= rows || c[1] >= cols {
                return Err(invalid(format!("{M}: table {id} is {rows} × {cols}, so `{key}` is within it")));
            }
            Ok(c)
        };
        let (cmd, params, label): (&'static str, Json, &str) = match op.as_str() {
            "insert_row" => {
                let below = args::opt_bool(a, "below", M)?.unwrap_or(true);
                (if below { "table.insertRowBelow" } else { "table.insertRowAbove" }, json!({ "row": row("row")? }), "Insert row")
            }
            "insert_col" => {
                let right = args::opt_bool(a, "right", M)?.unwrap_or(true);
                (if right { "table.insertColumnRight" } else { "table.insertColumnLeft" }, json!({ "col": col("col")? }), "Insert column")
            }
            "delete_row" => ("table.deleteRow", json!({ "row": row("row")? }), "Delete row"),
            "delete_col" => ("table.deleteColumn", json!({ "col": col("col")? }), "Delete column"),
            "merge" => ("table.merge", json!({ "from": cell("from")?, "to": cell("to")? }), "Merge cells"),
            "split" => ("table.split", json!({ "cell": cell("cell")? }), "Split cell"),
            "style" => ("table.style", json!({ "style": table_style(a, M)?.ok_or_else(|| invalid(format!("{M} needs `style`")))? }), "Table style"),
            "options" => {
                let mut p = json!({});
                for (key, engine) in [
                    ("header_row", "headerRow"),
                    ("total_row", "totalRow"),
                    ("banded_rows", "bandedRows"),
                    ("first_column", "firstColumn"),
                    ("last_column", "lastColumn"),
                    ("banded_columns", "bandedColumns"),
                ] {
                    if let Some(v) = args::opt_bool(a, key, M)? {
                        p[engine] = json!(v);
                    }
                }
                if p.as_object().is_some_and(|o| o.is_empty()) {
                    return Err(invalid(format!("{M}: give at least one table option")));
                }
                ("table.options", p, "Table options")
            }
            "fill" => {
                let list = args::present(a, "cells").and_then(Json::as_array).ok_or_else(|| invalid(format!("{M} needs `cells`, a list of [row, column]")))?;
                if list.is_empty() {
                    return Err(invalid(format!("{M}: `cells` names at least one cell")));
                }
                let mut cells = Vec::with_capacity(list.len());
                for c in list {
                    let [r, k] = args::cell_of(c).ok_or_else(|| invalid(format!("{M}: `cells` holds [row, column]")))?;
                    if r >= rows || k >= cols {
                        return Err(invalid(format!("{M}: table {id} is {rows} × {cols}, so every cell is within it")));
                    }
                    cells.push(json!([r, k]));
                }
                let mut p = json!({ "cells": cells });
                match args::present(a, "color") {
                    Some(Json::String(s)) if s == "none" => p["none"] = json!(true),
                    _ => p["color"] = json!(args::opt_color(a, "color", M)?.ok_or_else(|| invalid(format!("{M} needs `color`, or \"none\"")))?),
                }
                ("table.cellFill", p, "Cell fill")
            }
            "col_width" => ("table.columnWidth", json!({ "col": col("col")?, "width": args::need_num(a, "width", M, 4.0, 4000.0)? }), "Column width"),
            _ => ("table.rowHeight", json!({ "row": row("row")?, "height": args::need_num(a, "height", M, 4.0, 4000.0)? }), "Row height"),
        };
        let mut params = params;
        params["id"] = json!(id);
        let cmds = vec![("slide.go", json!({ "index": i })), (cmd, params)];
        let done = gate::change(doc, cx.area, Plan { method: M, most: TABLE, cmds }, label)?;
        let ShapeKind::Table(t) = shape_of(doc, i, id)?.kind else { return Err(invalid(format!("{M}: shape {id} is not a table"))) };
        doc.answer(&done.changed, json!({ "rows": t.n_rows(), "cols": t.n_cols() }))
    })
}

/// `slide.go`, then `chart.type`, `chart.data` and `chart.options`.
const CHART: usize = 4;

/// `deck.chart {doc, slide, id, type?, categories?, series?, title?, legend?,
/// data_labels?, gridlines?, palette?}`. An empty `title` removes it, and
/// `legend: "none"` hides the legend.
pub(crate) fn chart(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.chart";
    args::only(a, &["doc", "slide", "id", "type", "categories", "series", "title", "legend", "data_labels", "gridlines", "palette"], M)?;
    let slide = args::id(a, "slide", M)?;
    let id = args::id(a, "id", M)?;
    let mut cmds: Vec<(&'static str, Json)> = Vec::new();
    if let Some(t) = args::opt_choice(a, "type", M, &CHART_TYPES)? {
        cmds.push(("chart.type", json!({ "type": t, "id": id })));
    }
    if let Some((categories, series)) = chart_data(a, M, false)? {
        cmds.push(("chart.data", json!({ "categories": categories, "series": series, "id": id })));
    }
    let mut options = json!({ "id": id });
    if let Some(t) = args::opt_str(a, "title", M, 256)? {
        options["title"] = if t.is_empty() { Json::Null } else { json!(t) };
    }
    if let Some(l) = args::opt_choice(a, "legend", M, &["b", "t", "l", "r", "none"])? {
        options["legend"] = if l == "none" { Json::Null } else { json!(l) };
    }
    if let Some(v) = args::opt_bool(a, "data_labels", M)? {
        options["dataLabels"] = json!(v);
    }
    if let Some(v) = args::opt_bool(a, "gridlines", M)? {
        options["gridlines"] = json!(v);
    }
    if let Some(p) = args::opt_str(a, "palette", M, 64)? {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            return Err(invalid(format!("{M}: `palette` is a palette name")));
        }
        options["palette"] = json!(p);
    }
    if options.as_object().is_some_and(|o| o.len() > 1) {
        cmds.push(("chart.options", options));
    }
    if cmds.is_empty() {
        return Err(invalid(format!("{M}: name at least one thing to change")));
    }
    docs::with_doc(a, cx, M, |doc| {
        let i = doc.slide_index(slide)?;
        if !matches!(shape_of(doc, i, id)?.kind, ShapeKind::Chart(_)) {
            return Err(invalid(format!("{M}: shape {id} is not a chart")));
        }
        let mut all = vec![("slide.go", json!({ "index": i }))];
        all.extend(cmds);
        let done = gate::change(doc, cx.area, Plan { method: M, most: CHART, cmds: all }, "Edit chart")?;
        doc.answer(&done.changed, json!({}))
    })
}
