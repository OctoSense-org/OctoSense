//! One bounded image from the calling Pantry app's own storage.
use super::{Code, Refusal};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::{ImageFormat, ImageReader};
use octosense_llm_config::{ApiType, Provider};
use serde_json::Value;
use std::{io::Cursor, path::Path};

const APP: &str = "pantry-steward";
const MAX_BYTES: u64 = 10 * 1024 * 1024;

pub(super) fn supported(app: &str) -> bool {
    cfg!(target_os = "windows") && app == APP
}

pub(super) fn provider_allowed(provider: &Provider) -> bool {
    if crate::model::effective_api_type(provider) != ApiType::OpenAi {
        return false;
    }
    let family = super::family_id(provider);
    let model = super::effective_model(provider);
    matches!((family, model.as_deref()),
        ("minimax" | "minimax-cn", Some("MiniMax-M3"))
            | ("moonshot", Some("kimi-k2.5")))
}

fn invalid() -> Refusal {
    Refusal::new(Code::BadRequest, "The app image is missing, invalid, or over 10 MiB.")
}

pub(super) fn load(app: &str, host_dir: &Path, value: &Value) -> Result<String, Refusal> {
    if !supported(app) {
        return Err(Refusal::new(Code::Capability, "Image calls are unavailable to this app on this host."));
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
    let root = host_dir.parent().ok_or_else(invalid)?.join(APP).canonicalize().map_err(|_| invalid())?;
    let file = root.join(path).canonicalize().map_err(|_| invalid())?;
    if !file.starts_with(&root) || !file.is_file() || file.metadata().map_err(|_| invalid())?.len() > MAX_BYTES {
        return Err(invalid());
    }
    let bytes = std::fs::read(file).map_err(|_| invalid())?;
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
