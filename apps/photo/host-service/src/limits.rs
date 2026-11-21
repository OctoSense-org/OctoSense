//! Bounds on the canvases a photo command can open, create, resize and export.
use super::{Area, ExportOptions, Headless, Json, Path};
use octosense_engine_area::door::{Limit, Measure};
use photocraft_doc::{Document, Size};

const EDGE: u32 = 8192;
const PIXELS: u64 = 16 << 20;
pub(super) const OUTPUT_BYTES: usize = 64 << 20;
const DECODE_BYTES: u64 = 256 << 20;
const MAX_LAYERS: usize = 256;

fn dimensions(w: f64, h: f64) -> Size {
    // The pinned engine rounds floating-point UI inputs to integral pixels.
    Size::new(w.round().max(1.0) as u32, h.round().max(1.0) as u32)
}

fn check(s: Size) -> Result<u64, String> {
    if s.width > EDGE || s.height > EDGE || s.area() > PIXELS {
        return Err(format!("{} × {} pixels exceeds the canvas limit ({EDGE} per edge, {PIXELS} pixels)", s.width, s.height));
    }
    Ok(s.area())
}

fn number(p: &Json, key: &str) -> Option<f64> {
    p.get(key)?.as_f64()
}

fn new_size(p: &Json) -> Size {
    dimensions(number(p, "width").unwrap_or(1920.0), number(p, "height").unwrap_or(1080.0))
}

fn explicit(p: &Json) -> Result<Option<f64>, String> {
    let w = number(p, "width").unwrap_or(1.0);
    let h = number(p, "height").unwrap_or(1.0);
    check(dimensions(w, h)).map(|px| Some(px as f64))
}

fn resize(p: &Json) -> Result<Option<f64>, String> {
    if p["resample"] == "none" {
        return Ok(None);
    }
    explicit(p)
}

fn canvas(p: &Json) -> Result<Option<f64>, String> {
    // Relative deltas can shrink the canvas. Check their effective size at execution.
    if p["relative"] == true {
        return Ok(None);
    }
    explicit(p)
}

const fn limit(id: &'static str, f: fn(&Json) -> Result<Option<f64>, String>) -> Limit {
    Limit { id, what: "pixels", measure: Measure::Custom(f), max: PIXELS as f64, copies: false }
}

pub(super) static COMMANDS: &[Limit] = &[
    limit("image.canvasSize", canvas),
    limit("image.imageSize", resize),
    limit("file.new", |p| check(new_size(p)).map(|px| Some(px as f64))),
    limit("image.crop", |p| {
        if number(p, "x").is_some()
            && number(p, "y").is_some()
            && number(p, "width").is_some_and(|v| v.round() > 0.0)
            && number(p, "height").is_some_and(|v| v.round() > 0.0)
        {
            explicit(p)
        } else {
            Ok(None)
        }
    }),
    Limit {
        id: "channel.merge",
        what: "source channels",
        measure: Measure::Custom(|p| Ok(p["documents"].as_array().map(|a| a.len() as f64))),
        max: 64.0,
        copies: false,
    },
    limit("edit.contentAwareScale", explicit),
];

/// Count the current documents, including masks, cached layer surfaces and extra
/// channels. Tile count includes shared tiles in each surface that uses them.
pub(super) fn session_pixels(h: &Headless) -> Result<u64, String> {
    let mut total = 0;
    let mut layers = 0;
    let mut tile_bytes = 0u64;
    for d in h.session.documents() {
        total += check(d.doc.size)?;
        layers += d.doc.layer_count();
        if layers > MAX_LAYERS {
            return Err(format!("the open documents exceed {MAX_LAYERS} layers"));
        }
        for (_, _, board) in d.doc.artboards() {
            check(board.rect.size())?;
        }
        for surf in surfaces(&d.doc) {
            // All allocated tiles have the same backing size for a pixel format.
            if let Some((_, tile)) = surf.tiles().next() {
                tile_bytes = tile_bytes.saturating_add((surf.tile_count() as u64).saturating_mul(tile.bytes().len() as u64));
                if tile_bytes > DECODE_BYTES {
                    return Err(format!("the open documents exceed {DECODE_BYTES} raster tile bytes"));
                }
            }
        }
    }
    check_total(total)
}

fn canvas_pixels(h: &Headless) -> u64 {
    h.session.documents().iter().map(|d| d.doc.size.area()).sum()
}

fn check_total(total: u64) -> Result<u64, String> {
    if total > PIXELS {
        return Err(format!("the open documents total {total} pixels (limit {PIXELS})"));
    }
    Ok(total)
}

/// Resolve dimensions against the current active document, including any preceding
/// command in the same call. A new tab adds its canvas to the existing session.
pub(super) fn before(h: &Headless, id: &str, p: &Json) -> Result<(), String> {
    let active = h.session.active();
    let old = active.map(|s| s.doc.size).unwrap_or_default();
    let (ow, oh) = (f64::from(old.width), f64::from(old.height));
    let (w, height) = (number(p, "width"), number(p, "height"));
    if id == "channel.merge" {
        if let Some(sources) = p["documents"].as_array() {
            let requested: u64 =
                sources.iter().filter_map(|i| i.as_u64().and_then(|i| h.session.documents().get(i as usize))).map(|s| s.doc.size.area()).sum();
            check_total(requested)?;
        }
        return Ok(());
    }
    if id == "channel.split" {
        if let Some(st) = active {
            let planes = photocraft_engine::channel_cmds::color_count(&st.doc) + st.doc.channels.len();
            let closes_original = p["closeOriginal"].as_bool().unwrap_or(true);
            let added = planes.saturating_sub(usize::from(closes_original));
            check_total(canvas_pixels(h).saturating_add(old.area().saturating_mul(added as u64)))?;
        }
        return Ok(());
    }
    let (next, adds) = match id {
        "file.new" => (new_size(p), true),
        "image.duplicate" => (old, true),
        "channel.duplicate" if p["document"] == "new" => (old, true),
        "image.calculations" if p["result"] == "newDocument" => (old, true),
        "file.newFromClipboard" | "edit.paste" | "edit.pasteSpecial.pasteInPlace" if id == "file.newFromClipboard" || active.is_none() => {
            let Some(clip) = &h.session.clipboard else { return Ok(()) };
            (clip.bounds.size(), true)
        }
        "image.canvasSize" if p["relative"] == true => (dimensions(ow + w.unwrap_or(0.0), oh + height.unwrap_or(0.0)), false),
        "image.canvasSize" => (dimensions(w.unwrap_or(ow), height.unwrap_or(oh)), false),
        "image.imageSize" if p["resample"] != "none" => {
            let w = w.unwrap_or_else(|| height.unwrap_or(oh) * ow / oh.max(1.0));
            (dimensions(w, height.unwrap_or_else(|| w * oh / ow.max(1.0))), false)
        }
        "file.automate.fitImage" if p["resample"] != "none" => {
            let k = (w.unwrap_or(ow).max(1.0) / ow.max(1.0)).min(height.unwrap_or(oh).max(1.0) / oh.max(1.0));
            if k > 1.0 && p["dontEnlarge"] == true {
                return Ok(());
            }
            (dimensions(ow * k, oh * k), false)
        }
        "image.rotation.arbitrary" => {
            let deg = f64::from(number(p, "angle").unwrap_or(0.0) as f32);
            (photocraft_engine::mode_cmds::rotated_size(old, deg), false)
        }
        "image.crop" if number(p, "x").is_some() && number(p, "y").is_some() && w.is_some() && height.is_some() => {
            (dimensions(w.unwrap_or(ow), height.unwrap_or(oh)), false)
        }
        "image.revealAll" => {
            let Some(st) = active else { return Ok(()) };
            let all = st
                .doc
                .walk()
                .into_iter()
                .filter_map(|(_, _, l)| l.surface())
                .fold(st.doc.bounds(), |bounds, surf| bounds.union(&surf.content_bounds()));
            (all.size(), false)
        }
        "edit.contentAwareScale" => {
            let Some(st) = active else { return Ok(()) };
            let Some(surf) = st.active_layer.and_then(|id| st.doc.layer(id)).and_then(|l| l.surface()) else { return Ok(()) };
            let region = st.doc.selection.as_ref().map_or_else(|| surf.content_bounds(), |s| s.content_bounds().intersect(&st.doc.bounds()));
            check(region.size())?;
            check(dimensions(
                w.unwrap_or_else(|| f64::from(region.width()) * number(p, "scaleX").unwrap_or(100.0) / 100.0),
                height.unwrap_or_else(|| f64::from(region.height()) * number(p, "scaleY").unwrap_or(100.0) / 100.0),
            ))?;
            return Ok(());
        }
        _ => return Ok(()),
    };
    let existing = canvas_pixels(h) - if adds { 0 } else { old.area() };
    check_total(existing + check(next)?)?;
    if next != old && matches!(id, "image.imageSize" | "file.automate.fitImage") {
        if let Some(st) = active {
            let sx = f64::from(next.width) / ow.max(1.0);
            let sy = f64::from(next.height) / oh.max(1.0);
            for surface in surfaces(&st.doc) {
                let b = surface.content_bounds();
                if b.is_empty() {
                    continue;
                }
                // Photocraft scales the bounds of each layer, including off-canvas
                // pixels, by rounding the start down and the end up.
                check(dimensions(
                    (f64::from(b.x1) * sx).ceil() - (f64::from(b.x0) * sx).floor(),
                    (f64::from(b.y1) * sy).ceil() - (f64::from(b.y0) * sy).floor(),
                ))?;
            }
        }
    }
    Ok(())
}

fn surfaces(doc: &Document) -> impl Iterator<Item = &photocraft_doc::Surface> {
    doc.walk()
        .into_iter()
        .flat_map(|(_, _, l)| {
            [l.surface(), l.fill_cache.as_ref().map(|f| &f.surface), l.mask.as_ref().map(|m| &m.surface)].into_iter().flatten()
        })
        .chain(doc.channels.iter().chain(doc.quick_mask.as_ref()).map(|c| &c.surface))
        .chain(doc.selection.as_ref())
}

fn msg(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The pinned AuthorizedWorkspace reads entire files. Open the same relative
/// names through a directory capability and enforce the byte cap on the open
/// file itself, including a file that grows during the read.
pub(super) fn read(area: &Area, path: &str, max: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let invalid = |part: &str| {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$")
            || stem
                .strip_prefix("COM")
                .or_else(|| stem.strip_prefix("LPT"))
                .is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"));
        part.is_empty() || part.ends_with(['.', ' ']) || device
    };
    if path.contains(['\\', ':']) || path.split('/').any(invalid) {
        return Err(format!("automation path rejected: `{path}`"));
    }
    let root = cap_std::fs::Dir::open_ambient_dir(&area.root, cap_std::ambient_authority()).map_err(msg)?;
    let file = root.open(path).map_err(|e| format!("read `{path}`: {e}"))?;
    let meta = file.metadata().map_err(msg)?;
    if !meta.is_file() {
        return Err(format!("expected a regular file: `{path}`"));
    }
    let too_big = || format!("input exceeds {max} bytes");
    if meta.len() > max {
        return Err(too_big());
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(meta.len() as usize).map_err(msg)?;
    file.take(max + 1).read_to_end(&mut bytes).map_err(msg)?;
    if bytes.len() as u64 > max {
        return Err(too_big());
    }
    Ok(bytes)
}

pub(super) enum DecodeError {
    Limit(String),
    Unreadable(String),
}

impl DecodeError {
    fn unreadable(e: impl std::fmt::Display) -> Self {
        Self::Unreadable(e.to_string())
    }

    fn native(e: photocraft_format::FormatError) -> Self {
        match e {
            photocraft_format::FormatError::LimitExceeded(_) => Self::Limit(e.to_string()),
            _ => Self::unreadable(e),
        }
    }

    fn message(self) -> String {
        match self {
            Self::Limit(e) | Self::Unreadable(e) => e,
        }
    }
}

/// Inspect the same bytes the engine will import. The ordinary raster headers
/// need no pixel decoding. For formats without a matching header reader the
/// pinned codec first validates its chosen image under its own decode limits.
pub(super) fn decode(name: &str, bytes: &[u8]) -> Result<Document, DecodeError> {
    use photocraft_codecs as codecs;
    if Path::new(name).extension().is_some_and(|e| e.eq_ignore_ascii_case("pcraft")) || photocraft_format::is_pcraft(bytes) {
        let opts = photocraft_format::LoadOptions {
            max_manifest_bytes: 8 << 20,
            max_blob_bytes: OUTPUT_BYTES,
            max_total_bytes: DECODE_BYTES,
            ..Default::default()
        };
        // Check the manifest before loading any tile or embedded blob. The ZIP
        // reader bounds inflation as well as the declared uncompressed length.
        let archive = photocraft_format::zip::ZipReader::new(bytes).map_err(DecodeError::native)?;
        let manifest = archive.read_by_name("manifest.json", opts.max_manifest_bytes).map_err(DecodeError::native)?;
        #[derive(serde::Deserialize)]
        struct Header {
            document: Canvas,
        }
        #[derive(serde::Deserialize)]
        struct Canvas {
            size: Size,
        }
        check(serde_json::from_slice::<Header>(&manifest).map_err(DecodeError::unreadable)?.document.size).map_err(DecodeError::Limit)?;
        return photocraft_format::load_from_bytes_with(bytes, &opts).map_err(DecodeError::native);
    }
    if photocraft_io::is_psd(bytes) {
        let header = bytes.get(14..22).ok_or_else(|| DecodeError::unreadable("incomplete PSD/PSB dimensions"))?;
        let side = |i| u32::from_be_bytes([header[i], header[i + 1], header[i + 2], header[i + 3]]);
        check(Size::new(side(4), side(0))).map_err(DecodeError::Limit)?;
    } else if photocraft_io::raw::is_raw(bytes) {
        let mut opts = photocraft_raw::DevelopOptions::default();
        opts.limits = photocraft_raw::Limits { max_width: EDGE, max_height: EDGE, max_pixels: PIXELS, max_alloc: DECODE_BYTES };
        if let Some(preview) = photocraft_raw::embedded_preview(bytes) {
            check(Size::new(preview.width, preview.height)).map_err(DecodeError::Limit)?;
        }
        return photocraft_io::raw::import_raw_with(name, bytes, &opts).map(|r| r.document).map_err(DecodeError::unreadable);
    } else {
        let format = codecs::detect(bytes);
        match format {
            Some(codecs::Format::Tiff) => {
                let info = codecs::tiff_info(bytes).map_err(DecodeError::unreadable)?;
                let page =
                    info.default_page().and_then(|i| info.pages.get(i)).ok_or_else(|| DecodeError::unreadable("TIFF has no image page"))?;
                check(Size::new(page.width, page.height)).map_err(DecodeError::Limit)?;
            }
            Some(codecs::Format::Png | codecs::Format::Jpeg | codecs::Format::Gif | codecs::Format::Qoi) => {
                if let Ok(s) = imagesize::blob_size(bytes) {
                    check(dimensions(s.width as f64, s.height as f64)).map_err(DecodeError::Limit)?;
                } else {
                    probe_codec(bytes).map_err(DecodeError::Limit)?;
                }
            }
            Some(_) => probe_codec(bytes).map_err(DecodeError::Limit)?,
            None => (),
        }
    }
    photocraft_automation::files::open_bytes(name, bytes).map(|r| r.document).map_err(DecodeError::unreadable)
}

fn probe_codec(bytes: &[u8]) -> Result<(), String> {
    let limits = photocraft_codecs::Limits { max_width: EDGE, max_height: EDGE, max_pixels: PIXELS, max_alloc: DECODE_BYTES };
    photocraft_codecs::decode_with(bytes, &photocraft_codecs::DecodeOptions { limits, ..Default::default() }).map(|_| ()).map_err(msg)
}

pub(super) fn open(h: &mut Headless, path: &str, bytes: &[u8]) -> Result<(), String> {
    let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
    let mut doc = decode(name, bytes).map_err(DecodeError::message)?;
    check(doc.size)?;
    // Preserve Headless::open's template naming and tab path semantics.
    let path = if let Some(name) = photocraft_engine::file_cmds::template_name(&h.session, path) {
        doc.name = name;
        None
    } else {
        Some(path.to_owned())
    };
    h.session.add_document(doc, path);
    Ok(())
}

pub(super) fn save(h: &Headless, area: &Area, dest: &Path, format: Option<&str>, max: usize) -> Result<(), String> {
    let active = h.session.active().ok_or("no document open")?;
    let (bytes, _) =
        photocraft_automation::files::save_bytes(&active.doc, &super::leaf(dest), format, &ExportOptions::default()).map_err(msg)?;
    if bytes.len() > max {
        return Err(format!("encoded output exceeds {max} bytes"));
    }
    area.write(dest, &bytes)
}
