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

fn has_crop_rect(p: &Json) -> bool {
    number(p, "x").is_some()
        && number(p, "y").is_some()
        && number(p, "width").is_some_and(|v| v.round() > 0.0)
        && number(p, "height").is_some_and(|v| v.round() > 0.0)
}

const fn limit(id: &'static str, f: fn(&Json) -> Result<Option<f64>, String>) -> Limit {
    Limit { id, what: "pixels", measure: Measure::Custom(f), max: PIXELS as f64, copies: false }
}

pub(super) static COMMANDS: &[Limit] = &[
    limit("image.canvasSize", canvas),
    limit("image.imageSize", resize),
    limit("file.new", |p| check(new_size(p)).map(|px| Some(px as f64))),
    limit("image.crop", |p| if has_crop_rect(p) { explicit(p) } else { Ok(None) }),
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
            count_tiles(&mut tile_bytes, surf, surf.tile_count())?;
        }
    }
    check_total(total)
}

fn count_tiles(total: &mut u64, surface: &photocraft_doc::Surface, count: usize) -> Result<(), String> {
    // All tiles in this surface use the same backing size, including partial edge tiles.
    if let Some((_, tile)) = surface.tiles().next() {
        *total = total.saturating_add((count as u64).saturating_mul(tile.bytes().len() as u64));
        if *total > DECODE_BYTES {
            return Err(format!("the open documents exceed {DECODE_BYTES} raster tile bytes"));
        }
    }
    Ok(())
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
        "image.crop" if has_crop_rect(p) => (dimensions(w.unwrap_or(ow), height.unwrap_or(oh)), false),
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
        check_resize_tiles(h, f64::from(next.width) / ow.max(1.0), f64::from(next.height) / oh.max(1.0))?;
    }
    Ok(())
}

fn check_resize_tiles(h: &Headless, sx: f64, sy: f64) -> Result<(), String> {
    let mut total = 0;
    for (i, doc) in h.session.documents().iter().enumerate() {
        for surface in surfaces(&doc.doc) {
            let count = if Some(i) == h.session.active_index() {
                let b = surface.content_bounds();
                if b.is_empty() {
                    continue;
                }
                // The engine writes the scaled rectangle before pruning empty tiles.
                // Off-canvas offsets can make a layer cover an extra edge tile.
                let (x0, y0) = ((f64::from(b.x0) * sx).floor(), (f64::from(b.y0) * sy).floor());
                let (x1, y1) = ((f64::from(b.x1) * sx).ceil(), (f64::from(b.y1) * sy).ceil());
                check(dimensions(x1 - x0, y1 - y0))?;
                photocraft_doc::Rect::new(x0 as i32, y0 as i32, x1 as i32, y1 as i32).tiles().count()
            } else {
                surface.tile_count()
            };
            count_tiles(&mut total, surface, count)?;
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

#[cfg(test)]
mod resize_tests {
    use super::*;
    use photocraft_doc::{ColorMode, Layer, SampleType};
    use serde_json::json;

    fn small_stack(layers: usize) -> Document {
        let mut doc = Document::new("Stack", Size::new(16, 8), ColorMode::Rgb, SampleType::U16);
        for i in 0..layers {
            let mut layer = Layer::raster(i.to_string(), doc.pixel_format());
            layer.surface_mut().unwrap().fill_rect(doc.bounds(), &[1.0; 4]);
            doc.layers.push(layer);
        }
        doc
    }

    #[test]
    fn resize_counts_target_tiles_across_layers_and_other_open_documents() {
        // Only the tiny source tiles are allocated. Ask preflight what the new
        // bounds would cost, without running either resampling command.
        let mut h = Headless::new();
        h.session.add_document(small_stack(4), None);
        let target = json!({"width": 4096, "height": 2048});
        for command in ["image.imageSize", "file.automate.fitImage"] {
            before(&h, command, &target).unwrap(); // exactly 256 MiB
        }
        h.session.add_document(small_stack(1), None);
        h.session.set_active(0);
        for command in ["image.imageSize", "file.automate.fitImage"] {
            let err = before(&h, command, &target).unwrap_err();
            assert!(err.contains("raster tile bytes"), "{err}");
            before(&h, command, &json!({"width": 2048, "height": 1024})).unwrap();
        }
    }

    #[test]
    fn a_nonpositive_crop_extent_uses_the_selection() {
        let mut h = Headless::new();
        h.session.add_document(small_stack(1), None);
        h.command_run("select.all", json!({})).unwrap();
        let params = json!({"x": 0, "y": 0, "width": 0, "height": EDGE + 1});
        before(&h, "image.crop", &params).unwrap();
        let result = h.command_run("image.crop", params).unwrap();
        assert_eq!((result["width"].as_u64(), result["height"].as_u64()), (Some(16), Some(8)));
    }

    #[test]
    fn layered_photoshop_import_checks_layer_counts_before_decoding_pixels() {
        let mut doc = Document::new("Empty layers", Size::new(2, 2), ColorMode::Rgb, SampleType::U8);
        doc.layers = (0..=MAX_LAYERS).map(|i| Layer::raster(i.to_string(), doc.pixel_format())).collect();
        for format in ["psd", "psb"] {
            let bytes = photocraft_io::export(&doc, format, &ExportOptions::default()).unwrap().bytes;
            assert!(matches!(decode(format, &bytes), Err(DecodeError::Limit(e)) if e.contains("layers")), "{format}");
        }
    }

    #[test]
    fn photoshop_layer_bounds_and_masks_share_one_tile_budget() {
        use photocraft_psd::{LayerSpec, MaskSpec, PixelData, PsdBuilder, Rect, Version};
        let tile_area = (photocraft_geom::TILE_SIZE as u64).pow(2);
        for version in [Version::Psd, Version::Psb] {
            let mut builder = PsdBuilder::new(2, 2).version(version);
            builder.push_layer(LayerSpec::new("Base", 0, 0, 2, 2, PixelData::Rgba8(vec![255; 16])));
            let mut top = LayerSpec::new("Offset", -1, 0, 2, 2, PixelData::Rgba8(vec![255; 16]));
            top.mask = Some(MaskSpec { rect: Rect::from_xywh(0, 0, 1, 1), data: vec![128], default_color: 255, disabled: false });
            builder.push_layer(top);
            let file = builder.build().unwrap();
            // Three 4-byte RGBA tiles (the offset layer straddles zero) and a 1-byte mask tile.
            let exact = tile_area * 13;
            check_psd(&file, exact).unwrap();
            assert!(check_psd(&file, exact - 1).unwrap_err().contains("raster tile bytes"));
            let bytes = file.to_bytes().unwrap();
            let imported = decode("normal.psd", &bytes).unwrap_or_else(|e| panic!("{}", e.message()));
            assert_eq!(imported.layers.len(), 2);
            assert!(imported.layers[1].mask.is_some());
        }
    }
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
        // Use Photocraft's own records (including PSB and 16/32-bit layer
        // sections) before its importer creates channel buffers and surfaces.
        let file = photocraft_psd::PsdFile::from_bytes(bytes).map_err(DecodeError::unreadable)?;
        check_psd(&file, DECODE_BYTES).map_err(DecodeError::Limit)?;
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

fn check_psd(file: &photocraft_psd::PsdFile, limit: u64) -> Result<(), String> {
    use photocraft_psd::layer::{CHANNEL_REAL_USER_MASK, CHANNEL_USER_MASK};
    use photocraft_psd::{ColorMode, Rect, SectionType};

    let h = &file.header;
    let sample = u64::from(h.depth.div_ceil(8));
    let colors: u64 = match h.color_mode {
        ColorMode::Grayscale | ColorMode::Bitmap | ColorMode::Duotone => 1,
        ColorMode::Cmyk => 4,
        _ => 3,
    };
    let layered = h.depth != 1 && matches!(h.color_mode, ColorMode::Grayscale | ColorMode::Rgb | ColorMode::Cmyk | ColorMode::Lab | ColorMode::Unknown(_));
    let extra = u64::from(h.channels).saturating_sub(colors + u64::from(file.merged_has_alpha()));
    if !layered || file.layers().is_empty() || extra > 0 {
        let merged = h.row_bytes() as u64 * u64::from(h.height) * u64::from(h.channels);
        if merged > limit {
            return Err(format!("the Photoshop composite exceeds {limit} decoded bytes"));
        }
    }

    let mut bytes = 0u64;
    let mut charge = |r: Rect, pixel_bytes: u64| -> Result<(), String> {
        if r.is_empty() || pixel_bytes == 0 {
            return Ok(());
        }
        check(Size::new(r.width() as u32, r.height() as u32))?;
        let bounds = photocraft_doc::Rect::new(r.left, r.top, r.right, r.bottom);
        let tile_area = (photocraft_geom::TILE_SIZE as u64).pow(2);
        bytes += bounds.tiles().count() as u64 * tile_area * pixel_bytes;
        if bytes > limit { Err(format!("the Photoshop document exceeds {limit} raster tile bytes")) } else { Ok(()) }
    };
    let canvas = Rect::from_xywh(0, 0, h.width, h.height);
    if !layered {
        // Ink channels import as separate surfaces. Bitmap, indexed and
        // duotone import as a single 8-bit background.
        return charge(canvas, if h.color_mode == ColorMode::Multichannel && h.depth != 1 { u64::from(h.channels) * sample } else { colors + 1 });
    }
    let mut layers = 0;
    for layer in file.layers() {
        let kind = layer.section_type();
        if kind == SectionType::BoundingDivider {
            continue;
        }
        layers += 1;
        if layers > MAX_LAYERS {
            return Err(format!("the Photoshop document exceeds {MAX_LAYERS} layers"));
        }
        if !kind.is_folder() {
            charge(layer.rect, (colors + 1) * sample)?;
        }
        // The importer prefers the real user mask when supplied.
        if let Some(mask) = layer.layer_mask() {
            if let Some(real) = mask.real.filter(|_| layer.channel(CHANNEL_REAL_USER_MASK).is_some()) {
                charge(real.rect, sample)?;
            } else if layer.channel(CHANNEL_USER_MASK).is_some() {
                charge(mask.rect, sample)?;
            }
        }
    }
    if layers == 0 {
        charge(canvas, (colors + 1) * sample)?;
    }
    charge(canvas, extra * sample)
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
    if let Some(codec) = super::stream::codec(&active.doc, dest, format) {
        return super::stream::save(&active.doc, area, dest, codec, max);
    }
    let budget = area.write_limit(dest, max as u64)?;
    let reserve = super::export::reservation(&active.doc, dest, format, budget as usize)?;
    area.check(dest, reserve).map_err(|e| format!("before encoding: {e}"))?;
    let (bytes, _) =
        photocraft_automation::files::save_bytes(&active.doc, &super::leaf(dest), format, &ExportOptions::default()).map_err(msg)?;
    if bytes.len() > max {
        return Err(format!("encoded output exceeds {max} bytes"));
    }
    area.write(dest, &bytes)
}
