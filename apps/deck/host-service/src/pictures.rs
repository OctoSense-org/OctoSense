//! The service's own check of a picture `insert.picture` or
//! `picture.change` will read (their reviewed read, `REVIEWED.file_reads`),
//! before the door hands the engine the file's resolved path:
//!
//! - the path stays inside the caller's storage (and the door checks it
//!   again, through links), names a file that is there, and is at most the
//!   door's 64 MiB a call;
//! - the file is a picture the engine draws, by its first bytes: PNG, JPEG,
//!   WebP, GIF, TIFF or BMP. Anything else is refused before the engine
//!   sees it: an SVG (the engine has none), and a sound or a movie, which
//!   the engine's `picture.change` would otherwise embed whatever its type;
//! - the picture is at most [`MAX_PIXELS`] by its own header: the engine
//!   decodes a picture whole into memory on the UI thread, so a small file
//!   naming a huge picture is refused here (`too_large:`), and so is one
//!   whose header doesn't say its size (`invalid:`), which fails closed.

use octosense_engine_area::door::MAX_READ_BYTES;
use octosense_engine_area::Area;

use crate::codes::{fail, invalid, too_large, Code};

/// The most pixels a picture may have: a 48-megapixel phone photo fits.
pub(crate) const MAX_PIXELS: u64 = 50_000_000;

/// The kind of picture `b` starts with.
pub(crate) fn kind(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpeg")
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        Some("gif")
    } else if b.starts_with(b"RIFF") && b.get(8..12) == Some(b"WEBP") {
        Some("webp")
    } else if b.starts_with(b"BM") {
        Some("bmp")
    } else if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
        Some("tiff")
    } else {
        None
    }
}

fn be16(b: &[u8], at: usize) -> Option<u64> {
    Some(u16::from_be_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?) as u64)
}

fn be32(b: &[u8], at: usize) -> Option<u64> {
    Some(u32::from_be_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?) as u64)
}

fn le16(b: &[u8], at: usize) -> Option<u64> {
    Some(u16::from_le_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?) as u64)
}

fn le32(b: &[u8], at: usize) -> Option<u64> {
    Some(u32::from_le_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?) as u64)
}

/// The width and height a picture's header names, if it reads.
pub(crate) fn dimensions(b: &[u8]) -> Option<(u64, u64)> {
    match kind(b)? {
        "png" => Some((be32(b, 16)?, be32(b, 20)?)),
        "gif" => Some((le16(b, 6)?, le16(b, 8)?)),
        "bmp" => match le32(b, 14)? {
            12 => Some((le16(b, 18)?, le16(b, 20)?)),
            _ => {
                let w = le32(b, 18)? as u32 as i32;
                let h = le32(b, 22)? as u32 as i32;
                Some((w.unsigned_abs() as u64, h.unsigned_abs() as u64))
            }
        },
        "webp" => match b.get(12..16)? {
            b"VP8 " => Some((le16(b, 26)? & 0x3FFF, le16(b, 28)? & 0x3FFF)),
            b"VP8L" => {
                let v = le32(b, 21)?;
                Some(((v & 0x3FFF) + 1, ((v >> 14) & 0x3FFF) + 1))
            }
            b"VP8X" => {
                let n = |x: &[u8]| x[0] as u64 | (x[1] as u64) << 8 | (x[2] as u64) << 16;
                Some((n(b.get(24..27)?) + 1, n(b.get(27..30)?) + 1))
            }
            _ => None,
        },
        "jpeg" => {
            // The markers up to the first start-of-frame, each skipped by its
            // own length.
            let mut at = 2usize;
            while at + 4 <= b.len() {
                if b[at] != 0xFF {
                    return None;
                }
                let marker = b[at + 1];
                if marker == 0xFF {
                    at += 1;
                    continue;
                }
                if matches!(marker, 0x01 | 0xD0..=0xD7) {
                    at += 2;
                    continue;
                }
                let len = be16(b, at + 2)? as usize;
                if matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                    return Some((be16(b, at + 7)?, be16(b, at + 5)?));
                }
                at = at.checked_add(2 + len)?;
            }
            None
        }
        "tiff" => {
            let little = b.starts_with(b"II");
            let u16_at = |at: usize| if little { le16(b, at) } else { be16(b, at) };
            let u32_at = |at: usize| if little { le32(b, at) } else { be32(b, at) };
            let ifd = u32_at(4)? as usize;
            let n = u16_at(ifd)? as usize;
            let (mut w, mut h) = (None, None);
            for k in 0..n.min(512) {
                let e = ifd + 2 + 12 * k;
                let value = match u16_at(e + 2)? {
                    3 => u16_at(e + 8)?,
                    4 => u32_at(e + 8)?,
                    _ => continue,
                };
                match u16_at(e)? {
                    256 => w = Some(value),
                    257 => h = Some(value),
                    _ => {}
                }
            }
            Some((w?, h?))
        }
        _ => None,
    }
}

/// Check the picture at `rel` (module doc): its storage-relative path,
/// normalized, for the reviewed read.
pub(crate) fn check(area: &Area, rel: &str, key: &str, m: &str) -> Result<String, String> {
    let path = crate::inside(area, rel, key, m)?;
    let meta = match std::fs::metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(fail(Code::NotFound, format!("{rel} is not in this app's storage"))),
        Err(e) => return Err(invalid(format!("{rel}: {e}"))),
    };
    if !meta.is_file() {
        return Err(invalid(format!("{rel} is not a file")));
    }
    if meta.len() > MAX_READ_BYTES {
        return Err(too_large(format!("{rel} is larger than the {} MB a call reads", MAX_READ_BYTES >> 20)));
    }
    let bytes = std::fs::read(&path).map_err(|e| invalid(format!("{rel}: {e}")))?;
    if kind(&bytes).is_none() {
        return Err(invalid(format!("{rel} is not a picture the deck engine draws (PNG, JPEG, WebP, GIF, TIFF or BMP; it has no SVG)")));
    }
    let (w, h) = dimensions(&bytes).ok_or_else(|| invalid(format!("{rel}: can't read the picture's size")))?;
    if w.saturating_mul(h) > MAX_PIXELS {
        return Err(too_large(format!("{rel} is {w} × {h} pixels, more than the {} megapixels a picture may have", MAX_PIXELS / 1_000_000)));
    }
    Ok(crate::normal(rel))
}
