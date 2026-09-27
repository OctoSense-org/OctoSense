//! A provider QR read out of a picture: a screenshot of the desktop's export
//! sheet, a phone screenshot, a photo of a screen. The shell's
//! [`QrImagePicker`](crate::QrImagePicker) (or a dropped file) hands over the
//! encoded bytes; this finds the code in them with rqrr, the decoder the
//! camera path's scanner is tested against.
//!
//! One pass rarely fits every picture: a code a few hundred pixels wide in a
//! 1080×2280 screenshot, a 4000-pixel photo, a tiny crop. So the image is
//! tried as it is, thresholded, scaled down to a few sizes and scaled up when
//! small, and the first code that reads as an OctoSense provider code wins.
//! Always on a worker: a large photo takes a moment.
use image::imageops::{self, FilterType};
use image::{GrayImage, ImageReader, Limits};
use octosense_llm_config::qr;
use std::io::Cursor;

/// The largest file taken, encoded.
pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
/// The largest picture taken, decoded (a 48 MP phone photo is refused; its
/// 12 MP binned default is not).
pub const MAX_IMAGE_PIXELS: u64 = 40_000_000;

/// The long sides tried when scaling down.
const DOWNSCALED: [u32; 3] = [1600, 1000, 700];
/// Under this long side a picture is also tried scaled up to three times.
const SMALL: u32 = 500;
/// Up to this long side (a phone screenshot) it is tried as it is first,
/// and scaled up 1.5 and 2 times last.
const UPSCALED_MAX: u32 = 2400;

pub const NO_CODE: &str = "No QR code found in that image.";
pub const TOO_LARGE: &str = "That image is too large (at most 20 MB and 40 megapixels).";
pub const NOT_OURS: &str = "The QR code in that image is not an OctoSense provider code.";
pub const NOT_AN_IMAGE: &str = "That file is not a PNG or JPEG image.";

/// The provider code (`OCTOS1E:…`, or an older format [`qr::format_of`]
/// knows) in an encoded PNG or JPEG, or why there is none, as text for the
/// sheet.
pub fn find_code(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(TOO_LARGE.into());
    }
    let luma = decode_luma(bytes)?;
    let mut other_code = false;
    for candidate in candidates(&luma) {
        for text in codes_in(&candidate) {
            if qr::format_of(&text).is_some() {
                return Ok(text.trim().to_string());
            }
            other_code = true;
        }
    }
    Err(if other_code { NOT_OURS } else { NO_CODE }.into())
}

/// The picture as 8-bit grey, refusing one too large before it is decoded.
fn decode_luma(bytes: &[u8]) -> Result<GrayImage, String> {
    let reader = || ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|_| NOT_AN_IMAGE.to_string());
    let probe = reader()?;
    if !matches!(probe.format(), Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)) {
        return Err(NOT_AN_IMAGE.into());
    }
    let (w, h) = probe.into_dimensions().map_err(|_| NOT_AN_IMAGE.to_string())?;
    if w == 0 || h == 0 {
        return Err(NOT_AN_IMAGE.into());
    }
    if u64::from(w) * u64::from(h) > MAX_IMAGE_PIXELS {
        return Err(TOO_LARGE.into());
    }
    let mut decoder = reader()?;
    let mut limits = Limits::default();
    // 40 MP of RGBA16 at most, so the decoder never allocates past it.
    limits.max_alloc = Some(MAX_IMAGE_PIXELS * 8);
    decoder.limits(limits);
    let image = decoder.decode().map_err(|e| match e {
        image::ImageError::Limits(_) => TOO_LARGE.to_string(),
        _ => NOT_AN_IMAGE.to_string(),
    })?;
    Ok(image.into_luma8())
}

/// The versions of the picture to try, cheapest and likeliest first.
fn candidates(original: &GrayImage) -> impl Iterator<Item = GrayImage> + '_ {
    let long = original.width().max(original.height());
    let scaled = move |side: u32, filter: FilterType| {
        let (w, h) = (original.width(), original.height());
        let (nw, nh) = if w >= h { (side, (h as u64 * side as u64 / w as u64).max(1) as u32) } else { ((w as u64 * side as u64 / h as u64).max(1) as u32, side) };
        imageops::resize(original, nw, nh, filter)
    };
    // As it is, unless it is a large photo: then the scaled copies come
    // first and it is tried last.
    let as_is = (long <= UPSCALED_MAX).then(|| original.clone());
    let down = DOWNSCALED.into_iter().filter(move |&side| side < long).map(move |side| scaled(side, FilterType::Triangle));
    // Scaled up smoothly (in halves): a code under three pixels a module
    // reads again, and a fractional factor lands the modules on another
    // pixel grid.
    let halves: &[u32] = if long < SMALL { &[4, 6, 3] } else if long <= UPSCALED_MAX { &[3, 4] } else { &[] };
    let up = halves.iter().map(move |&k| scaled(long * k / 2, FilterType::CatmullRom));
    let large = (long > UPSCALED_MAX).then(|| original.clone());
    as_is
        .into_iter()
        .chain(down)
        .chain(up)
        .chain(large)
        .flat_map(|img| {
            let thresholded = threshold(&img);
            [img, thresholded]
        })
}

/// Black and white at the picture's Otsu threshold: a code on a tinted or
/// unevenly lit background reads more reliably as pure contrast.
fn threshold(img: &GrayImage) -> GrayImage {
    let mut histogram = [0u64; 256];
    for p in img.pixels() {
        histogram[p.0[0] as usize] += 1;
    }
    let total = img.pixels().len() as f64;
    let sum: f64 = histogram.iter().enumerate().map(|(i, &n)| i as f64 * n as f64).sum();
    let (mut weight_back, mut sum_back, mut best, mut cut) = (0.0, 0.0, 0.0, 128u8);
    for (i, &n) in histogram.iter().enumerate() {
        weight_back += n as f64;
        if weight_back == 0.0 {
            continue;
        }
        let weight_fore = total - weight_back;
        if weight_fore == 0.0 {
            break;
        }
        sum_back += i as f64 * n as f64;
        let mean_back = sum_back / weight_back;
        let mean_fore = (sum - sum_back) / weight_fore;
        let between = weight_back * weight_fore * (mean_back - mean_fore).powi(2);
        if between > best {
            best = between;
            cut = i as u8;
        }
    }
    let mut out = img.clone();
    for p in out.pixels_mut() {
        p.0[0] = if p.0[0] <= cut { 0 } else { 255 };
    }
    out
}

/// Every code rqrr reads in `img`.
fn codes_in(img: &GrayImage) -> Vec<String> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| img.as_raw()[y * w + x]);
    prepared.detect_grids().iter().filter_map(|grid| grid.decode().ok().map(|(_, text)| text)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn otsu_splits_two_tones() {
        let img = GrayImage::from_fn(4, 1, |x, _| image::Luma([if x < 2 { 40 } else { 200 }]));
        let out = threshold(&img);
        assert_eq!(out.as_raw(), &[0, 0, 255, 255]);
    }

    #[test]
    fn garbage_is_not_an_image() {
        assert_eq!(find_code(b"not an image").unwrap_err(), NOT_AN_IMAGE);
        assert_eq!(find_code(&vec![0u8; MAX_IMAGE_BYTES + 1]).unwrap_err(), TOO_LARGE);
    }
}
