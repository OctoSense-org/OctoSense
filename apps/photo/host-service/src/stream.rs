//! The pinned JPEG and PNG codecs accept Write. Stream their ordinary photo
//! modes directly into the area's private stage, charging each write before it
//! lands. The compositor and codec retain their bounded pixel/row working data.
use std::io::{self, BufWriter, Write};
use std::path::Path;

use octosense_engine_area::Area;
use photocraft_codecs::{ChannelLayout, Format, Image};
use photocraft_doc::{ColorMode, Document, SampleType};

const EXIF: &[u8] = b"Exif\0\0";
const XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

pub(super) fn codec(doc: &Document, dest: &Path, format: Option<&str>) -> Option<Format> {
    let ext = format.map(|s| s.trim_start_matches('.')).or_else(|| dest.extension()?.to_str())?;
    if !matches!(doc.mode, ColorMode::Rgb | ColorMode::Grayscale) {
        return None;
    }
    match photocraft_codecs::from_extension(&ext.to_ascii_lowercase())? {
        Format::Png if matches!(doc.depth, SampleType::U8 | SampleType::U16) => Some(Format::Png),
        // Photocraft selectively trims oversized JPEG XMP. Keep using that
        // export path for documents whose packet needs its special handling.
        Format::Jpeg if doc.depth == SampleType::U8 && doc.metadata.xmp.as_ref().is_none_or(|x| x.len() + XMP.len() <= 65533) => {
            Some(Format::Jpeg)
        }
        _ => None,
    }
}

struct Capped<W> {
    inner: W,
    left: u64,
    exceeded: bool,
}

impl<W: Write> Write for Capped<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.left {
            self.exceeded = true;
            return Err(io::Error::other("encoded output exceeds its byte budget"));
        }
        let n = self.inner.write(bytes)?;
        self.left -= n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

pub(super) fn save(doc: &Document, area: &Area, dest: &Path, format: Format, ceiling: usize) -> Result<(), String> {
    let budget = area.write_limit(dest, ceiling as u64)?;
    // Check the minimum container and metadata work before compositing. ICC
    // and PNG text chunks may be compressed; reserve their uncompressed sizes.
    let header = 64
        + doc.icc_profile.as_ref().map_or(0, |v| v.len() as u64)
        + doc.metadata.exif.as_ref().map_or(0, |v| if format == Format::Jpeg && v.len() + EXIF.len() > 65533 { 0 } else { v.len() as u64 })
        + doc.metadata.xmp.as_ref().map_or(0, |v| v.len() as u64);
    if header > budget {
        area.check(dest, header).map_err(|e| format!("before encoding: {e}"))?;
        return Err(format!("output header exceeds {budget} bytes before encoding"));
    }
    let stage = area.stage()?;
    let staged = stage.path("image");
    let file = std::fs::File::create(&staged).map_err(|e| e.to_string())?;
    let mut out = Capped { inner: BufWriter::new(file), left: budget, exceeded: false };
    let img = photocraft_io::document_to_image(doc, &mut Vec::new()).map_err(|e| e.to_string())?;
    let encoded = if format == Format::Png { png(&img, &mut out) } else { jpeg(&img, &mut out) };
    if out.exceeded {
        area.check(dest, budget + 1)?;
        return Err(format!("encoded output exceeds {budget} bytes"));
    }
    encoded?;
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    stage.commit(&[(staged, dest.to_owned())])
}

fn png(img: &Image, out: &mut impl Write) -> Result<(), String> {
    let mut info = png::Info::with_size(img.width(), img.height());
    info.color_type = match img.layout() {
        ChannelLayout::Gray => png::ColorType::Grayscale,
        ChannelLayout::GrayA => png::ColorType::GrayscaleAlpha,
        ChannelLayout::Rgb => png::ColorType::Rgb,
        ChannelLayout::Rgba => png::ColorType::Rgba,
        _ => return Err("unsupported PNG color layout".into()),
    };
    let wide = img.sample_type() == photocraft_codecs::SampleType::U16;
    info.bit_depth = if wide { png::BitDepth::Sixteen } else { png::BitDepth::Eight };
    info.icc_profile = img.icc.as_deref().map(Into::into);
    info.exif_metadata = img.meta.exif.as_deref().map(photocraft_codecs::upright_exif);
    if let Some((x, y)) = img.meta.dpi.filter(|(x, y)| *x > 0.0 && *y > 0.0) {
        info.pixel_dims =
            Some(png::PixelDimensions { xppu: (x / 0.0254).round() as u32, yppu: (y / 0.0254).round() as u32, unit: png::Unit::Meter });
    }
    let mut enc = png::Encoder::with_info(out, info).map_err(msg)?;
    enc.set_compression(png::Compression::Balanced);
    if let Some(xmp) = &img.meta.xmp {
        enc.add_itxt_chunk("XML:com.adobe.xmp".into(), photocraft_codecs::upright_xmp(xmp).into_owned()).map_err(msg)?;
    }
    let mut writer = enc.write_header().map_err(msg)?;
    let mut stream = writer.stream_writer().map_err(msg)?;
    if wide && cfg!(target_endian = "little") {
        // PNG stores 16-bit samples big endian. Swap one row at a time.
        let row = img.width() as usize * img.layout().channels() * 2;
        let mut swapped = vec![0; row];
        for src in img.data().chunks_exact(row) {
            for (dst, pair) in swapped.chunks_exact_mut(2).zip(src.chunks_exact(2)) {
                dst.copy_from_slice(&[pair[1], pair[0]]);
            }
            stream.write_all(&swapped).map_err(msg)?;
        }
    } else {
        stream.write_all(img.data()).map_err(msg)?;
    }
    stream.finish().map_err(msg)?;
    writer.finish().map_err(msg)
}

fn jpeg(img: &Image, out: &mut impl Write) -> Result<(), String> {
    let opts = photocraft_codecs::EncodeOptions::default();
    let mut enc = jpeg_encoder::Encoder::new(out, opts.jpeg_quality);
    let gray = matches!(img.layout(), ChannelLayout::Gray | ChannelLayout::GrayA);
    enc.set_sampling_factor(if gray || !opts.jpeg_chroma_subsampling {
        jpeg_encoder::SamplingFactor::R_4_4_4
    } else {
        jpeg_encoder::SamplingFactor::R_4_2_0
    });
    if let Some((x, y)) = img.meta.dpi.filter(|(x, y)| *x >= 1.0 && *y >= 1.0) {
        enc.set_density(jpeg_encoder::Density::Inch { x: x.round() as u16, y: y.round() as u16 });
    }
    if let Some(v) = img.meta.exif.as_deref().filter(|v| v.len() + EXIF.len() <= 65533) {
        enc.add_app_segment(1, &[EXIF, &photocraft_codecs::upright_exif(v)].concat()).map_err(msg)?;
    }
    if let Some(v) = &img.meta.xmp {
        enc.add_app_segment(1, &[XMP, photocraft_codecs::upright_xmp(v).as_bytes()].concat()).map_err(msg)?;
    }
    if let Some(v) = &img.icc {
        enc.add_icc_profile(v).map_err(msg)?;
    }
    let pixels = if img.layout().has_alpha() {
        // Photocraft's flat JPEG export composites unassociated alpha over white.
        let mut matted = Vec::with_capacity(img.pixel_count() * img.layout().color_channels());
        for px in img.data().chunks_exact(img.layout().channels()) {
            let (&a, colors) = px.split_last().unwrap();
            let alpha = f32::from(a) / 255.0;
            matted.extend(colors.iter().map(|&v| ((f32::from(v) / 255.0 * alpha + (1.0 - alpha)) * 255.0).round() as u8));
        }
        std::borrow::Cow::Owned(matted)
    } else {
        std::borrow::Cow::Borrowed(img.data())
    };
    let color = if gray { jpeg_encoder::ColorType::Luma } else { jpeg_encoder::ColorType::Rgb };
    enc.encode(&pixels, img.width() as u16, img.height() as u16, color).map_err(msg)
}

fn msg(e: impl std::fmt::Display) -> String {
    e.to_string()
}
