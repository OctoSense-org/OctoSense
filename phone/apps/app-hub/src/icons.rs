//! Canonical artwork from installed bundles, shared by shell surfaces.
use std::{io::Read, path::{Component, Path, PathBuf}, sync::atomic::{AtomicU64, Ordering}};

const MAX_ICON_BYTES: u64 = 1024 * 1024;
static GENERATION: AtomicU64 = AtomicU64::new(1);
pub fn generation() -> u64 { GENERATION.load(Ordering::Relaxed) }
pub fn invalidate() { GENERATION.fetch_add(1, Ordering::Relaxed); }

#[derive(Clone, Debug)]
pub enum IconData {
    Svg(String),
    Png(Vec<u8>),
}

fn bounded_read(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() { return None; }
    let mut data = Vec::new();
    file.take(limit + 1).read_to_end(&mut data).ok()?;
    (data.len() as u64 <= limit).then_some(data)
}

/// Resolve only an app's own installed bundle. Displaying artwork never grants
/// launch permission; the shell still performs its separate verified open gate.
pub fn installed_icon_path(root: &Path, id: &str) -> Option<PathBuf> {
    if id.is_empty() || id.len() > 64 || id.starts_with('.') ||
        !id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.') {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let bundle = root.join(id).join("bundle").canonicalize().ok()?;
    if !bundle.starts_with(&root) { return None; }
    let listing = bundle.join("listing.json").canonicalize().ok()?;
    if !listing.starts_with(&bundle) { return None; }
    let json = String::from_utf8(bounded_read(&listing, 64 * 1024)?).ok()?;
    let listing = octosense_app_policy::Listing::parse(&json).ok()?;
    let icon = PathBuf::from(listing.icon?);
    if !icon.components().all(|part| matches!(part, Component::Normal(_))) { return None; }
    let icon = bundle.join(icon).canonicalize().ok()?;
    icon.starts_with(&bundle).then_some(icon)
}

pub fn read_installed_icon(root: &Path, id: &str) -> Option<IconData> {
    let path = installed_icon_path(root, id)?;
    let data = bounded_read(&path, MAX_ICON_BYTES)?;
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "svg" => {
            let svg = String::from_utf8(data).ok()?;
            let doc = makepad_widgets::makepad_draw::svg::parse_svg(&svg);
            let (width, height) = doc.logical_size();
            (width.is_finite() && width > 0.0 && width == height && !doc.root.is_empty())
                .then_some(IconData::Svg(svg))
        }
        "png" => {
            // Check dimensions before the decoder allocates; store icons do
            // not need photographic dimensions or animation atlases.
            if !data.starts_with(b"\x89PNG\r\n\x1a\n") || data.get(12..16)? != b"IHDR" { return None; }
            let width = u32::from_be_bytes(data.get(16..20)?.try_into().ok()?);
            let height = u32::from_be_bytes(data.get(20..24)?.try_into().ok()?);
            if width == 0 || width > 1024 || width != height { return None; }
            Some(IconData::Png(data))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bundle(root: &Path, icon: &str) -> std::path::PathBuf {
        let path = root.join("sample/bundle");
        std::fs::create_dir_all(path.join("assets")).unwrap();
        std::fs::write(path.join("listing.json"), serde_json::json!({
            "schema":1, "description":"A sample", "category":"utilities",
            "platforms":["android"], "age_rating":"all", "icon":icon,
            "publisher":{"name":"Example", "support":"https://example.com",
            "privacy_policy_url":"https://example.com/privacy"}
        }).to_string()).unwrap();
        path
    }
    #[test]
    fn installed_icons_use_the_declared_asset_and_reject_escape_or_non_square_artwork() {
        let root = tempfile::tempdir().unwrap();
        let path = bundle(root.path(), "assets/custom.svg");
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" fill="#12abcd"/></svg>"##;
        std::fs::write(path.join("assets/custom.svg"), svg).unwrap();
        assert!(matches!(read_installed_icon(root.path(), "sample"), Some(IconData::Svg(s)) if s == svg));
        std::fs::write(path.join("assets/custom.svg"), svg.replace("64 64", "64 32")).unwrap();
        assert!(read_installed_icon(root.path(), "sample").is_none());
        bundle(root.path(), "../outside.svg");
        assert!(read_installed_icon(root.path(), "sample").is_none());
        assert!(read_installed_icon(root.path(), "../sample").is_none());
        assert!(read_installed_icon(root.path(), "missing").is_none());
    }

    #[test]
    fn png_icons_are_bounded_before_decoding_and_symlinks_cannot_escape() {
        use makepad_widgets::makepad_zune_png::{makepad_zune_core::{bit_depth::BitDepth,
            colorspace::ColorSpace, options::EncoderOptions}, PngEncoder};
        let root = tempfile::tempdir().unwrap();
        let path = bundle(root.path(), "assets/icon.png");
        let mut png = Vec::new();
        PngEncoder::new(&[255; 4 * 4 * 4], EncoderOptions::new(4, 4, ColorSpace::RGBA, BitDepth::Eight))
            .encode(&mut png).unwrap();
        std::fs::write(path.join("assets/icon.png"), &png).unwrap();
        let Some(IconData::Png(loaded)) = read_installed_icon(root.path(), "sample") else { panic!("valid PNG icon"); };
        assert!(makepad_widgets::image_cache::ImageBuffer::from_png(&loaded).is_ok());
        png[16..20].copy_from_slice(&100_000_u32.to_be_bytes());
        std::fs::write(path.join("assets/icon.png"), &png).unwrap();
        assert!(read_installed_icon(root.path(), "sample").is_none());
        std::fs::write(path.join("assets/icon.png"), vec![0; MAX_ICON_BYTES as usize + 1]).unwrap();
        assert!(read_installed_icon(root.path(), "sample").is_none());
        #[cfg(unix)] {
            let outside = tempfile::tempdir().unwrap();
            std::fs::write(outside.path().join("icon.svg"), crate::APP_ICON_SVG).unwrap();
            bundle(root.path(), "assets/link.svg");
            std::os::unix::fs::symlink(outside.path().join("icon.svg"), path.join("assets/link.svg")).unwrap();
            assert!(read_installed_icon(root.path(), "sample").is_none());
        }
    }
}
