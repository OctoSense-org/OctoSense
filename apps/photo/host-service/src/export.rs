//! Admission before photocraft's Vec-returning encoders render, compress or
//! assemble an export. Charge uncompressed data and container overhead: a
//! highly compressible file can therefore need less space than its reservation.
use std::{io, path::Path, sync::Arc};

use photocraft_codecs::Format;
use photocraft_doc::{Document, Effects, LayerContent, Rect, Size, SmartObject, SmartSource, Surface};
use serde::Serialize;

struct Reservation {
    bytes: u64,
    limit: u64,
}

impl Reservation {
    fn add(&mut self, bytes: u64) -> Result<(), String> {
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > self.limit {
            Err(format!("output reservation exceeds the {} bytes left before encoding", self.limit))
        } else {
            Ok(())
        }
    }

    // The native manifest is pretty-printed. Charge serialized fields without
    // allocating their JSON, including indentation from the enclosing layer
    // groups. PSD regenerates descriptors and UTF-16 strings from these fields.
    fn fields(&mut self, value: &impl Serialize, depth: usize) -> Result<(), String> {
        struct Counter<'a> {
            budget: &'a mut Reservation,
            indent: u64,
        }
        impl io::Write for Counter<'_> {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                let lines = buf.iter().filter(|&&c| c == b'\n').count() as u64;
                self.budget.add(4 * buf.len() as u64 + lines * self.indent).map_err(io::Error::other)?;
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        serde_json::to_writer_pretty(Counter { budget: self, indent: 48 + depth as u64 * 8 }, value).map_err(|e| e.to_string())
    }

    fn blob(&mut self, len: usize) -> Result<(), String> {
        // Zstd's raw blocks, content hash, manifest reference and both ZIP headers.
        self.add(512 + compressed(len as u64))
    }

    fn optional_blob(&mut self, bytes: &Option<Arc<Vec<u8>>>) -> Result<(), String> {
        if let Some(bytes) = bytes {
            self.blob(bytes.len())?;
        }
        Ok(())
    }

    fn effects(&mut self, fx: &Effects, depth: usize) -> Result<(), String> {
        self.fields(&fx.items, depth)?;
        self.optional_blob(&fx.psd_raw)
    }

    fn native_surface(&mut self, s: &Surface, depth: usize) -> Result<(), String> {
        let tiles = s.tile_count() as u64;
        // Charge every tile reference, even when the bundle writer deduplicates.
        let tile_size = s.tiles().next().map_or(0, |(_, t)| t.bytes().len() as u64);
        self.add(1024 + 128 * depth as u64 + tiles.saturating_mul(compressed(tile_size) + 1024 + 64 * depth as u64))
    }

    fn planes(&mut self, size: Size, channels: u64, bytes_per_sample: usize) -> Result<(), String> {
        // PSD/PSB writes PackBits or ZIP-predicted channel rows (or stores
        // them raw), plus row length tables. Include PSB's wider records.
        let row = u64::from(size.width) * bytes_per_sample as u64;
        let bytes = compressed(row).saturating_add(16).saturating_mul(u64::from(size.height)).saturating_mul(channels);
        self.add(bytes)
    }
}

fn compressed(bytes: u64) -> u64 {
    // Covers stored/deflated blocks, PackBits' literal run overhead and
    // Zstd's framing even on data that does not compress.
    bytes.saturating_add(bytes.div_ceil(8)).saturating_add(32)
}

pub(super) fn reservation(doc: &Document, dest: &Path, format: Option<&str>, max: usize) -> Result<u64, String> {
    let ext = format
        .map(|f| f.trim_start_matches('.'))
        .or_else(|| dest.extension()?.to_str())
        .ok_or_else(|| format!("cannot tell the format of `{}`; pass a format", super::leaf(dest)))?
        .to_ascii_lowercase();
    let mut b = Reservation { bytes: 0, limit: max as u64 };
    match ext.as_str() {
        "pcraft" => layered(&mut b, doc, true, 0)?,
        "psd" | "psb" => layered(&mut b, doc, false, 0)?,
        _ => flat(&mut b, doc, photocraft_codecs::from_extension(&ext).ok_or_else(|| format!("unknown image format `{ext}`"))?)?,
    }
    Ok(b.bytes)
}

fn flat(b: &mut Reservation, doc: &Document, format: Format) -> Result<(), String> {
    let fmt = doc.pixel_format();
    let (w, h) = (u64::from(doc.size.width), u64::from(doc.size.height));
    let px = doc.size.area();
    let channels = fmt.channels().max(4) as u64;
    let raw = px * channels * fmt.sample.bytes() as u64;
    let image = match format {
        Format::Png => 4096 + compressed(raw + h) + 16 * raw.div_ceil(8192),
        // Up to 16 Huffman bits plus 16 magnitude bits per coefficient and
        // one stuffed byte after each 0xff. Use full 16x16 MCU rounding.
        Format::Jpeg => 4096 + w.next_multiple_of(16) * h.next_multiple_of(16) * channels * 8,
        Format::Qoi => 64 + px * 5,
        Format::Pnm => 256 + raw,
        Format::Bmp | Format::Tga => 2048 + h * 8 + px * 5,
        Format::Tiff => 8192 + raw * 2 + h * 64,
        Format::OpenExr => 8192 + px * channels * 4 * 2 + h * 128,
        Format::Hdr => 1024 + h * 32 + px * 8,
        Format::Gif => 4096 + px * 3 + h * 8,
        Format::WebP | Format::Ico | Format::Avif => 8192 + px * 16 + h * 64,
        Format::Heif => return Err("HEIF encoding is unavailable".into()),
    };
    b.add(image)?;
    // Flat exports may carry an ICC profile, EXIF and XMP; include framing,
    // compression headroom and any orientation normalization.
    for n in [doc.icc_profile.as_ref().map(|v| v.len()), doc.metadata.exif.as_ref().map(|v| v.len()), doc.metadata.xmp.as_ref().map(|v| v.len())]
        .into_iter()
        .flatten()
    {
        b.add(256 + 2 * n as u64)?;
    }
    Ok(())
}

fn layered(b: &mut Reservation, doc: &Document, native: bool, nesting: usize) -> Result<(), String> {
    let fmt = doc.pixel_format();
    let channels = fmt.channels() as u64;
    let sample = fmt.sample.bytes();
    b.add(16 << 10)?;
    if native {
        // Native archives include both previews. Neither grows a small canvas.
        for side in [256u64, 1024] {
            let px = doc.size.area().min(side * side);
            b.add(1024 + compressed(px * 4 + side))?;
        }
    } else {
        b.planes(doc.size, channels + doc.channels.len() as u64 + u64::from(doc.quick_mask.is_some()), sample)?;
    }
    b.fields(&(&doc.name, &doc.guides, &doc.work_path, &doc.clipping_path, &doc.color_table, &doc.duotone, &doc.variables, &doc.timeline), 0)?;
    b.fields(&(&doc.measurement, &doc.notes, &doc.text_styles, &doc.slices, &doc.metadata.xmp), 0)?;
    b.optional_blob(&doc.icc_profile)?;
    b.optional_blob(&doc.metadata.exif)?;
    for (_, name, bytes) in &doc.metadata.psd_resources {
        b.fields(name, 0)?;
        b.blob(bytes.len())?;
    }
    for (_, _, bytes) in &doc.metadata.psd_global_blocks {
        b.blob(bytes.len())?;
    }
    for path in &doc.paths {
        b.add(1024)?;
        b.fields(&(&path.name, &path.path), 0)?;
        b.optional_blob(&path.psd_raw)?;
    }
    for comp in doc.layer_comps.iter().chain(doc.last_document_state.as_ref()) {
        b.add(2048)?;
        b.fields(&(&comp.name, &comp.comment), 0)?;
        for state in &comp.states {
            b.add(512)?;
            if let Some(a) = &state.appearance {
                b.effects(&a.effects, 0)?;
            }
        }
    }
    for ch in doc.channels.iter().chain(doc.quick_mask.as_ref()) {
        b.add(1024)?;
        b.fields(&ch.name, 0)?;
        if native {
            b.native_surface(&ch.surface, 0)?;
        }
    }
    if native {
        if let Some(s) = &doc.selection {
            b.native_surface(s, 0)?;
        }
    }
    for p in &doc.patterns {
        b.fields(&(&p.id, &p.name), 0)?;
        if native {
            b.native_surface(&p.surface, 0)?;
        } else {
            let f = p.surface.format();
            b.planes(Size::new(p.width, p.height), channels.max(f.channels() as u64), sample.max(f.sample.bytes()))?;
        }
    }
    for (_, depth, l) in doc.walk() {
        b.add(8192 + 1024 * depth as u64)?;
        b.fields(&(&l.name, &l.vector_mask), depth)?;
        b.effects(&l.effects, depth)?;
        for (_, bytes) in &l.psd_blocks {
            b.blob(bytes.len())?;
        }
        let charge_surface = |b: &mut Reservation, s: &Surface, channels| {
            if native {
                b.native_surface(s, depth)
            } else {
                b.planes(s.content_bounds().size(), channels, sample)
            }
        };
        if let Some(s) = l.surface() {
            charge_surface(b, s, channels)?;
        }
        if let Some(mask) = &l.mask {
            charge_surface(b, &mask.surface, 1)?;
        }
        if let Some(c) = &l.fill_cache {
            b.fields(&c.fill, depth)?;
            charge_surface(b, &c.surface, channels)?;
        }
        if let Some(v) = &l.video {
            b.fields(&v.source, depth)?;
            if native {
                for frame in &v.frames {
                    b.native_surface(frame, depth)?;
                }
            }
        }
        match &l.content {
            LayerContent::Raster(_) => (),
            LayerContent::Group(g) => b.fields(&g.artboard, depth)?,
            LayerContent::Adjustment(a) => b.fields(a, depth)?,
            LayerContent::Fill(f) => {
                b.fields(f, depth)?;
                if !native {
                    b.planes(doc.size, channels, sample)?;
                }
            }
            LayerContent::Text(t) => {
                b.fields(&(&t.text, &t.font_family, &t.runs, &t.paragraphs, &t.warp), depth)?;
                b.optional_blob(&t.psd_raw)?;
            }
            LayerContent::Shape(s) => {
                b.fields(&(&s.fill, &s.path, &s.stroke, &s.live), depth)?;
                b.optional_blob(&s.psd_raw)?;
            }
            LayerContent::Smart(s) => {
                b.fields(&(&s.smart_filters, &s.warp), depth)?;
                b.optional_blob(&s.psd_raw)?;
                if let Some(mask) = &s.filter_mask {
                    charge_surface(b, &mask.surface, 1)?;
                }
                match &s.source {
                    SmartSource::Linked { path } => {
                        b.fields(path, depth)?;
                        if !native && !s.smart_filters.is_empty() {
                            if let Some(f) = photocraft_io::linked::find_linked_file(&doc.metadata, path) {
                                smart_source(b, doc, s, &f.file_name, &f.bytes, nesting, false)?;
                            }
                        }
                    }
                    SmartSource::Embedded { file_name, bytes } => {
                        b.fields(file_name, depth)?;
                        b.blob(bytes.len())?;
                        if !native {
                            smart_source(b, doc, s, file_name, bytes, nesting, true)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// PSD embeds a native source as PSB and writes each smart-filter cache over
/// the union of its positioned source, cached layer bounds and the canvas.
fn smart_source(
    b: &mut Reservation,
    doc: &Document,
    smart: &SmartObject,
    name: &str,
    bytes: &[u8],
    nesting: usize,
    embedded: bool,
) -> Result<(), String> {
    let converts = embedded && nesting < super::MAX_LINK_DEPTH && photocraft_format::is_pcraft(bytes);
    if !converts && smart.smart_filters.is_empty() {
        return Ok(());
    }
    let inner = match super::limits::decode(name, bytes) {
        Ok(d) => d,
        Err(super::limits::DecodeError::Limit(e)) => return Err(e),
        Err(super::limits::DecodeError::Unreadable(_)) => return Ok(()),
    };
    if converts {
        layered(b, &inner, false, nesting + 1)?;
    }
    if smart.smart_filters.is_empty() {
        return Ok(());
    }
    let mut bounds = smart.cache.as_ref().map_or(doc.bounds(), |c| c.content_bounds().union(&doc.bounds()));
    let warp = smart.warp.as_ref().filter(|w| !w.is_identity());
    // The engine uses four corners for a homography and a four-pixel mesh
    // capped at 512 cells per axis for a warp. Count the same vertices, with
    // the rasterizer's one-pixel border; no mesh or pixel buffer is needed.
    let cells = |n: u32| if warp.is_some() { n.div_ceil(4).clamp(1, 512) } else { 1 };
    let (nx, ny) = (cells(inner.size.width), cells(inner.size.height));
    let [a, bx, c, d, tx, ty] = smart.transform.m;
    let m = smart.perspective.unwrap_or([a, c, tx, bx, d, ty, 0.0, 0.0, 1.0]);
    for j in 0..=ny {
        for i in 0..=nx {
            let x = f64::from(inner.size.width) * f64::from(i) / f64::from(nx);
            let y = f64::from(inner.size.height) * f64::from(j) / f64::from(ny);
            let (x, y) = warp.map_or((x, y), |w| w.map(x, y));
            let z = m[6] * x + m[7] * y + m[8];
            let (x, y) = ((m[0] * x + m[1] * y + m[2]) / z, (m[3] * x + m[4] * y + m[5]) / z);
            if !x.is_finite() || !y.is_finite() {
                return Err("cannot bound the smart filter's placement before encoding".into());
            }
            bounds = bounds.union(&Rect::new(
                (x.floor() as i32).saturating_sub(1),
                (y.floor() as i32).saturating_sub(1),
                (x.ceil() as i32).saturating_add(1),
                (y.ceil() as i32).saturating_add(1),
            ));
        }
    }
    let fmt = doc.pixel_format();
    b.planes(bounds.size(), fmt.channels() as u64 + 1, fmt.sample.bytes())
}
