//! One bounded image from the authenticated calling app's own storage.
use super::{Code, Refusal};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::{ImageFormat, ImageReader};
use serde_json::Value;
use std::{fs::File, io::{Cursor, Read}, path::Path};

const MAX_BYTES: u64 = 10 * 1024 * 1024;

fn invalid() -> Refusal {
    Refusal::new(Code::BadRequest, "The app image is missing, invalid, or over 10 MiB.")
}

pub(super) fn load(app: &str, host_dir: &Path, value: &Value) -> Result<String, Refusal> {
    if !cfg!(target_os = "windows") {
        return Err(Refusal::new(Code::Unavailable, "Image input is unavailable on this host."));
    }
    if app.is_empty() || app.len() > 128 || !app.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric())
        || !app.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        || app.contains("..")
    {
        return Err(invalid());
    }
    let object = value.as_object().ok_or_else(invalid)?;
    if object.len() != 1 {
        return Err(invalid());
    }
    let path = object.get("app_path").and_then(Value::as_str).ok_or_else(invalid)?;
    let path = path.strip_prefix('/').unwrap_or(path);
    if path.is_empty() || path.contains('\\') || path.contains(':')
        || path.split('/').any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(invalid());
    }
    let root = host_dir.parent().ok_or_else(invalid)?.join(app).canonicalize().map_err(|_| invalid())?;
    // This follows the existing canonical-path jail boundary. There is no
    // handle-relative open here, so a concurrent local filesystem mutation
    // between canonicalization and open is not excluded by this check.
    let file = root.join(path).canonicalize().map_err(|_| invalid())?;
    if !file.starts_with(&root) || !file.is_file() {
        return Err(invalid());
    }
    let opened = File::open(file).map_err(|_| invalid())?;
    if opened.metadata().map_err(|_| invalid())?.len() > MAX_BYTES { return Err(invalid()); }
    let mut bytes = Vec::new();
    opened.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|_| invalid())?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_BYTES {
        return Err(invalid());
    }
    let format = image::guess_format(&bytes).map_err(|_| invalid())?;
    let mime = match format {
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Png => "image/png",
        ImageFormat::WebP => "image/webp",
        _ => return Err(invalid()),
    };
    ImageReader::with_format(Cursor::new(&bytes), format).into_dimensions().map_err(|_| invalid())?;
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}
