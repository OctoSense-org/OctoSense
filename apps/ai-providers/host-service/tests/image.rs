//! A provider QR read out of pictures the way people take them: the fixture
//! PNG itself, a JPEG of it, a phone screenshot with the code small in it, a
//! large photo, a small crop; and the pictures that must be refused.
use image::{DynamicImage, GrayImage, ImageFormat, Luma, Rgb, RgbImage};
use octosense_llm_service::image_qr::{find_code, NOT_AN_IMAGE, NOT_OURS, NO_CODE, TOO_LARGE};
use std::io::Cursor;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../config/tests/fixtures").join(name)
}

fn qr_a_png() -> Vec<u8> {
    std::fs::read(fixture("qr-a.png")).unwrap()
}

fn qr_a_text() -> String {
    std::fs::read_to_string(fixture("qr-a.txt")).unwrap().trim().to_string()
}

fn qr_a() -> GrayImage {
    image::load_from_memory(&qr_a_png()).unwrap().into_luma8()
}

fn encode(img: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, format).unwrap();
    out.into_inner()
}

fn jpeg(img: &DynamicImage, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality).encode_image(&img.to_rgb8()).unwrap();
    out
}

/// `code` scaled to `side` pixels and pasted at (x, y) on a `w`×`h` picture
/// of a phone screen: grey, with a darker status bar and some UI blocks.
fn screenshot(w: u32, h: u32, side: u32, x: u32, y: u32) -> DynamicImage {
    let mut screen = RgbImage::from_pixel(w, h, Rgb([0xd8, 0xd8, 0xdc]));
    for py in 0..90.min(h) {
        for px in 0..w {
            screen.put_pixel(px, py, Rgb([0x30, 0x30, 0x36]));
        }
    }
    for block in 0..6u32 {
        let top = 200 + block * 140;
        for py in top..(top + 90).min(h) {
            for px in 60..(w - 60) {
                screen.put_pixel(px, py, Rgb([0xff, 0xff, 0xff]));
            }
        }
    }
    let code = image::imageops::resize(&qr_a(), side, side, image::imageops::FilterType::Triangle);
    for (cx, cy, p) in code.enumerate_pixels() {
        let v = p.0[0];
        screen.put_pixel(x + cx, y + cy, Rgb([v, v, v]));
    }
    DynamicImage::ImageRgb8(screen)
}

#[test]
fn the_fixture_png_reads() {
    assert_eq!(find_code(&qr_a_png()).unwrap(), qr_a_text());
}

#[test]
fn a_jpeg_of_it_reads() {
    let img = DynamicImage::ImageLuma8(qr_a());
    assert_eq!(find_code(&jpeg(&img, 80)).unwrap(), qr_a_text());
    assert_eq!(find_code(&jpeg(&img, 40)).unwrap(), qr_a_text());
}

#[test]
fn a_phone_screenshot_with_the_code_small_in_it_reads() {
    let shot = screenshot(1080, 2280, 300, 390, 1500);
    assert_eq!(find_code(&encode(&shot, ImageFormat::Png)).unwrap(), qr_a_text());
    assert_eq!(find_code(&jpeg(&shot, 80)).unwrap(), qr_a_text());
}

#[test]
fn a_large_photo_reads_through_a_scaled_copy() {
    // 4032x3024, the code about a fifth of the frame, tinted and soft.
    let mut photo = RgbImage::from_pixel(4032, 3024, Rgb([0x9a, 0x8e, 0x80]));
    let code = image::imageops::resize(&qr_a(), 900, 900, image::imageops::FilterType::CatmullRom);
    for (cx, cy, p) in code.enumerate_pixels() {
        let v = p.0[0] as u32;
        photo.put_pixel(1500 + cx, 1000 + cy, Rgb([(40 + v * 190 / 255) as u8, (38 + v * 185 / 255) as u8, (36 + v * 170 / 255) as u8]));
    }
    let photo = DynamicImage::ImageRgb8(photo);
    assert_eq!(find_code(&jpeg(&photo, 85)).unwrap(), qr_a_text());
}

#[test]
fn a_small_crop_reads_scaled_up() {
    // Three pixels a module, blurred by the scale down.
    let small = image::imageops::resize(&qr_a(), 207, 207, image::imageops::FilterType::Triangle);
    assert_eq!(find_code(&encode(&DynamicImage::ImageLuma8(small), ImageFormat::Png)).unwrap(), qr_a_text());
}

#[test]
fn a_picture_without_a_code_is_a_clear_error() {
    let blank = screenshot(1080, 2280, 1, 0, 0);
    assert_eq!(find_code(&encode(&blank, ImageFormat::Png)).unwrap_err(), NO_CODE);
    assert_eq!(find_code(b"GIF89a....").unwrap_err(), NOT_AN_IMAGE);
}

#[test]
fn another_qr_code_is_not_ours() {
    // A plain URL code, drawn the way the export sheet draws one.
    let code = qrcode::QrCode::new(b"https://example.com/").unwrap();
    let width = code.width();
    let colors = code.to_colors();
    let (scale, quiet) = (8u32, 4u32);
    let side = (width as u32 + 2 * quiet) * scale;
    let img = GrayImage::from_fn(side, side, |x, y| {
        let (mx, my) = ((x / scale) as i64 - quiet as i64, (y / scale) as i64 - quiet as i64);
        let inside = mx >= 0 && my >= 0 && (mx as usize) < width && (my as usize) < width;
        Luma([if inside && colors[my as usize * width + mx as usize] == qrcode::Color::Dark { 0 } else { 255 }])
    });
    assert_eq!(find_code(&encode(&DynamicImage::ImageLuma8(img), ImageFormat::Png)).unwrap_err(), NOT_OURS);
}

#[test]
fn oversized_pictures_are_refused_before_decoding() {
    // 8000x6000 = 48 MP: refused from the header, though the file is small.
    let big = GrayImage::from_pixel(8000, 6000, Luma([200]));
    let bytes = encode(&DynamicImage::ImageLuma8(big), ImageFormat::Png);
    assert!(bytes.len() < 20 * 1024 * 1024);
    assert_eq!(find_code(&bytes).unwrap_err(), TOO_LARGE);
}
