//! Saving and exporting an open document (SERVICE.md "History and saving",
//! "Export"). None of it runs an engine command: the service calls the
//! engine's encoders (`save_bytes` for the native format, `.pptx` and outline
//! text, `pdf_bytes` for a PDF, `render_png` for a picture of a slide), which
//! touch no file and return bytes, and writes every file itself, through a
//! staging folder inside the caller's storage, moved into place all or
//! nothing (`octosense_engine_area::Stage`), within what is left of the
//! storage (the render cache cleared first when it would not fit). A write
//! never replaces a file, except `deck.save` over the deck's own file, from
//! the foreground. The engine's `file.save`, `file.saveAs` and
//! `file.saveTemplate` (`host`) and `file.export` (`file`) never run.

use std::path::{Path, PathBuf};

use deckcraft_engine::cmd::file as engine_file;
use deckcraft_model::Presentation;
use octosense_engine_area::Area;
use serde_json::{json, Value as Json};

use crate::args;
use crate::cache;
use crate::codes::{self, fail, invalid, too_large, Code};
use crate::docs;
use crate::reading;
use crate::Ctx;

/// The largest file the service writes (SERVICE.md "Caps": 64 MiB).
const MAX_WRITE: u64 = crate::MAX_DECK_BYTES;
/// One export call's most pixels to raster: the door's `MAX_RUN_RASTER`.
pub(crate) const MAX_RASTER: u64 = crate::MAX_RUN_RASTER;
/// One raster's most pixels (a PDF page's slide, or one picture).
const MAX_ONE_RASTER: u64 = crate::MAX_RASTER;
/// An image export's most slides, longest side and pixels in all.
pub(crate) const MAX_IMAGES: usize = 64;
const MAX_IMAGE_SIDE: u64 = 4096;
pub(crate) const MAX_IMAGE_PIXELS: u64 = 256_000_000;
/// A PDF's resolutions (SERVICE.md: 72 to 200, default 150).
const DPI: (i64, i64) = (72, 200);
const DEFAULT_DPI: i64 = 150;
/// A handout page's slides.
const PER_PAGE: [i64; 6] = [1, 2, 3, 4, 6, 9];
/// The handout and notes paper the engine lays out (US Letter portrait).
const PAPER: (f64, f64) = (612.0, 792.0);

/// Move `bytes` into place at `dest` (inside `area`, already checked)
/// through a staging folder: all or nothing, within the storage's room,
/// the render cache cleared first when it would not fit, and what still
/// doesn't fit refused before anything is staged. `area`'s `may_replace`
/// decides whether an existing file may be replaced.
fn place(area: &Area, files: &[(PathBuf, &[u8])], m: &str) -> Result<(), String> {
    let total: u64 = files.iter().map(|(_, b)| b.len() as u64).sum();
    if files.iter().any(|(_, b)| b.len() as u64 > MAX_WRITE) {
        return Err(too_large(format!("{m}: the result is larger than the {} MB the service writes", MAX_WRITE >> 20)));
    }
    let freed: u64 = if area.may_replace { files.iter().map(|(d, _)| cache::regular_len(d)).sum() } else { 0 };
    let need = total.saturating_sub(freed);
    cache::make_room(area, need);
    // The commit's own refusal, before anything is staged.
    if let Some(room) = area.room().filter(|room| need > *room) {
        return Err(codes::area(format!("the results are {total} bytes, more than the {room} {}", octosense_engine_area::NO_ROOM)));
    }
    let stage = crate::stage(area)?;
    let mut moves = Vec::with_capacity(files.len());
    for (k, (dest, bytes)) in files.iter().enumerate() {
        let staged = stage.path(format!("{k}"));
        std::fs::write(&staged, bytes).map_err(|e| codes::io(m, &e))?;
        moves.push((staged, dest.clone()));
    }
    stage.commit(&moves).map_err(codes::area)
}

/// An area like `area` whose writes never replace a file, whatever the
/// call's surface: where SERVICE.md says "a new file".
fn create_only(area: &Area) -> Area {
    Area::new(area.root.clone(), area.room(), false)
}

/// Write `bytes` to the new file `dest` (`deck.create`'s deck).
pub(crate) fn write_new(area: &Area, dest: &Path, bytes: &[u8], m: &str) -> Result<(), String> {
    place(&create_only(area), &[(dest.to_path_buf(), bytes)], m)
}

/// `deck.save {doc, path?}`: the native format to the deck's own file, or
/// to a new file, which the deck is from then on.
pub(crate) fn save(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.save";
    args::only(a, &["doc", "path"], M)?;
    let to = args::opt_str(a, "path", M, 512)?;
    docs::with_doc(a, cx, M, |doc| {
        let (area, dest, rel) = match to {
            None => {
                let file = doc.file.clone().ok_or_else(|| invalid(format!("{M}: this deck has no .deckcraft file yet (it came from a .pptx or an outline): save it with a `path`")))?;
                // The one write that replaces a file: the deck's own, from
                // the foreground.
                if !cx.area.may_replace {
                    return Err(invalid(format!("{M}: saving over the deck's own file needs the app in the foreground")));
                }
                let dest = crate::inside(cx.area, &file, "path", M)?;
                (Area::new(cx.area.root.clone(), cx.area.room(), true), dest, file)
            }
            Some(out) => {
                if !out.to_ascii_lowercase().ends_with(".deckcraft") {
                    return Err(invalid(format!("{M}: `path` is a new .deckcraft file (.pptx goes through deck.export)")));
                }
                let area = create_only(cx.area);
                let dest = crate::new_file(&area, out, "path", M)?;
                (area, dest, crate::normal(out))
            }
        };
        let bytes = engine_file::save_bytes(&doc.st()?.doc, "deckcraft").map_err(|e| fail(Code::Damaged, format!("{M}: {e}")))?;
        place(&area, &[(dest, &bytes)], M)?;
        let st = doc.st_mut()?;
        st.saved_doc = st.doc.clone();
        doc.file = Some(rel.clone());
        Ok(json!({ "path": rel, "bytes": bytes.len(), "rev": doc.rev }))
    })
}

/// The slides an export names (ids, in deck order), or every slide; hidden
/// ones only with `include_hidden`.
fn export_slides(deck: &Presentation, doc: &docs::OpenDoc, a: &Json, m: &str, include_hidden: bool) -> Result<Vec<usize>, String> {
    let mut at: Vec<usize> = match args::opt_ids(a, "slides", m, crate::slides::MAX_SLIDES)? {
        Some(ids) => ids.iter().map(|id| doc.slide_index(*id)).collect::<Result<_, _>>()?,
        None => (0..deck.slides.len()).collect(),
    };
    at.sort_unstable();
    at.retain(|i| include_hidden || !deck.slides[*i].hidden);
    if at.is_empty() {
        return Err(invalid(format!("{m}: no slide to export (hidden slides need `include_hidden`)")));
    }
    Ok(at)
}

/// `deck.export {doc, kind, …}`: new files only.
pub(crate) fn export(a: &Json, cx: &Ctx) -> Result<Json, String> {
    const M: &str = "deck.export";
    let kind = args::need_str(a, "kind", M)?.to_string();
    match kind.as_str() {
        "pptx" | "outline" => args::only(a, &["doc", "kind", "out"], M)?,
        "pdf" => args::only(a, &["doc", "kind", "out", "layout", "per_page", "slides", "include_hidden", "dpi"], M)?,
        "images" => args::only(a, &["doc", "kind", "out_dir", "width", "slides"], M)?,
        other => return Err(invalid(format!("{M}: `kind` is pptx, pdf, images or outline, not {other:?}"))),
    }
    let area = create_only(cx.area);
    docs::with_doc(a, cx, M, |doc| {
        let deck = doc.deck()?;
        match kind.as_str() {
            "pptx" | "outline" => {
                let out = args::need_str(a, "out", M)?;
                let lower = out.to_ascii_lowercase();
                let ok = if kind == "pptx" { lower.ends_with(".pptx") } else { lower.ends_with(".txt") || lower.ends_with(".md") };
                if !ok {
                    return Err(invalid(format!("{M}: `out` is a new {} file", if kind == "pptx" { ".pptx" } else { ".txt or .md" })));
                }
                let dest = crate::new_file(&area, out, "out", M)?;
                let bytes = engine_file::save_bytes(&deck, &kind).map_err(|e| fail(Code::Damaged, format!("{M}: {e}")))?;
                place(&area, &[(dest, &bytes)], M)?;
                Ok(json!({ "path": crate::normal(out), "bytes": bytes.len() }))
            }
            "pdf" => {
                let out = args::need_str(a, "out", M)?;
                if !out.to_ascii_lowercase().ends_with(".pdf") {
                    return Err(invalid(format!("{M}: `out` is a new .pdf file")));
                }
                let dest = crate::new_file(&area, out, "out", M)?;
                let layout = args::opt_choice(a, "layout", M, &["slides", "notes", "handouts"])?.unwrap_or("slides");
                let per_page = args::opt_int(a, "per_page", M)?;
                if per_page.is_some() && layout != "handouts" {
                    return Err(invalid(format!("{M}: `per_page` goes with handouts")));
                }
                let per_page = per_page.unwrap_or(6);
                if !PER_PAGE.contains(&per_page) {
                    return Err(invalid(format!("{M}: `per_page` is 1, 2, 3, 4, 6 or 9")));
                }
                let dpi = args::opt_int_in(a, "dpi", M, DPI.0, DPI.1)?.unwrap_or(DEFAULT_DPI);
                let include_hidden = args::opt_bool(a, "include_hidden", M)?.unwrap_or(false);
                let slides = export_slides(&deck, doc, a, M, include_hidden)?;
                // Every cap before anything is drawn: each slide is rastered
                // at `dpi` into its box on the page.
                let (sw, sh) = (deck.slide_size.width.max(1.0), deck.slide_size.height.max(1.0));
                let box_w = match layout {
                    "slides" => sw,
                    "notes" => PAPER.0 - 144.0,
                    _ => handout_box_width(per_page as u8, sw / sh),
                };
                let px = |pt: f64| (pt / 72.0 * dpi as f64).ceil().max(1.0) as u64;
                let one = px(box_w) * px(box_w * sh / sw);
                if one > MAX_ONE_RASTER {
                    return Err(too_large(format!("{M}: a {sw} × {sh} point slide at {dpi} dpi is a {one}-pixel raster, more than the {MAX_ONE_RASTER} one may have: export at a lower dpi")));
                }
                let all = one.saturating_mul(slides.len() as u64);
                if all > MAX_RASTER {
                    return Err(too_large(format!(
                        "{M}: {} slides at {dpi} dpi are {:.1} megapixels to raster, more than the {} one export rasters: export fewer slides, or at a lower dpi",
                        slides.len(),
                        all as f64 / 1e6,
                        MAX_RASTER / 1_000_000
                    )));
                }
                let mut opts = json!({ "layout": layout, "dpi": dpi, "slides": slides, "includeHidden": include_hidden });
                if layout == "handouts" {
                    opts["perPage"] = json!(per_page);
                }
                let bytes = engine_file::pdf_bytes(&deck, &opts).map_err(|e| fail(Code::Damaged, format!("{M}: {e}")))?;
                place(&area, &[(dest, &bytes)], M)?;
                let pages = if layout == "handouts" { slides.len().div_ceil(per_page as usize) } else { slides.len() };
                Ok(json!({ "path": crate::normal(out), "bytes": bytes.len(), "pages": pages }))
            }
            _ => {
                let out_dir = args::need_str(a, "out_dir", M)?;
                let dir = crate::inside(&area, out_dir, "out_dir", M)?;
                let width = args::opt_int_in(a, "width", M, reading::WIDTH.0, MAX_IMAGE_SIDE as i64)?.unwrap_or(1920) as u64;
                let slides = export_slides(&deck, doc, a, M, true)?;
                if slides.len() > MAX_IMAGES {
                    return Err(too_large(format!("{M}: {} slides, more than the {MAX_IMAGES} pictures one export writes", slides.len())));
                }
                let (sw, sh) = (deck.slide_size.width.max(1.0), deck.slide_size.height.max(1.0));
                let scale = width as f64 / sw;
                let height = (sh * scale).round().max(1.0) as u64;
                if height > MAX_IMAGE_SIDE {
                    return Err(too_large(format!("{M}: a picture {width} pixels wide is {height} high, more than {MAX_IMAGE_SIDE}: export them narrower")));
                }
                let all = (width * height).saturating_mul(slides.len() as u64);
                if all > MAX_IMAGE_PIXELS {
                    return Err(too_large(format!(
                        "{M}: {} pictures of {width} × {height} are {:.1} megapixels, more than the {} one export draws",
                        slides.len(),
                        all as f64 / 1e6,
                        MAX_IMAGE_PIXELS / 1_000_000
                    )));
                }
                let names: Vec<String> = slides.iter().map(|i| format!("Slide {}.png", i + 1)).collect();
                for name in &names {
                    area.check(&dir.join(name), 0).map_err(codes::area)?;
                }
                let pngs: Vec<Vec<u8>> = slides.iter().map(|i| engine_file::render_png(&deck, *i, scale, false).0).collect();
                let files: Vec<(PathBuf, &[u8])> = names.iter().zip(&pngs).map(|(n, b)| (dir.join(n), b.as_slice())).collect();
                place(&area, &files, M)?;
                let base = crate::normal(out_dir);
                Ok(json!({ "paths": names.iter().map(|n| format!("{base}/{n}")).collect::<Vec<_>>() }))
            }
        }
    })
}

/// The width of a slide's box on a handout page of `per_page` slides
/// (deckcraft-pdf's `handout_boxes`, on US Letter).
fn handout_box_width(per_page: u8, aspect: f64) -> f64 {
    let (pw, ph) = PAPER;
    let (mx, top, bottom) = (54.0, 54.0, 54.0);
    let (cols, rows, notes) = match per_page {
        0 | 1 => (1.0, 1.0, false),
        2 => (1.0, 2.0, false),
        3 => (1.0, 3.0, true),
        4 => (2.0, 2.0, false),
        6 => (2.0, 3.0, false),
        _ => (3.0, 3.0, false),
    };
    let gap = 18.0;
    let aw = if notes { (pw - 2.0 * mx) * 0.5 } else { pw - 2.0 * mx };
    let ah = ph - top - bottom - 18.0;
    let cw = (aw - gap * (cols - 1.0)) / cols;
    let ch = (ah - gap * (rows - 1.0)) / rows;
    if cw / ch > aspect {
        ch * aspect
    } else {
        cw
    }
}
