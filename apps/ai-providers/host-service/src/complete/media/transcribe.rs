//! Windows app-local WAV to MiniMax speech-to-text bridge.
use super::{wire, Candidate, Class, Code, Context, Refusal, RESPONSE_MAX};
use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;

const MAX_WAV: usize = 1024 * 1024;
const MAX_TEXT: usize = 4096;

fn unavailable() -> Refusal {
    Refusal::new(
        Code::Capability,
        "Speech transcription is unavailable to this app on this host.",
    )
}

fn invalid_audio() -> Refusal {
    Refusal::new(
        Code::BadRequest,
        "The app audio is missing, invalid, or over 1 MiB.",
    )
}

fn provider_error() -> Refusal {
    Refusal::new(
        Code::Provider,
        "The transcription provider failed; the upload was not retried.",
    )
}

pub(super) fn supported() -> bool {
    supported_on(std::env::consts::OS)
}

fn supported_on(platform: &str) -> bool {
    platform == "windows"
}

fn route(candidates: &[Candidate]) -> Result<wire::Route, Refusal> {
    let minimax: Vec<_> = candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.provider.family.to_ascii_lowercase().as_str(),
                "minimax" | "minimax-cn" | "minimaxi"
            )
        })
        .cloned()
        .collect();
    wire::route(&minimax, "audio", Class::Fast).map_err(|_| {
        Refusal::new(
            Code::NoProvider,
            "Configure a supported speech provider in AI providers.",
        )
    })
}

pub(super) fn capability(context: &Context) -> Result<Value, Refusal> {
    if context.call.args.as_object().is_none_or(|o| !o.is_empty()) {
        return Err(Refusal::new(Code::BadRequest, "The capability request takes no arguments."));
    }
    context.check()?;
    let configured = route(&context.providers()?).is_ok();
    Ok(json!({"schema":1,"supported":supported(),"configured":configured && supported()}))
}

fn load_audio(host_dir: &Path, app_id: &str, args: &Value) -> Result<Vec<u8>, Refusal> {
    if app_id.is_empty() || app_id == "." || app_id == ".."
        || app_id.contains(['/', '\\', ':', '\0'])
    {
        return Err(invalid_audio());
    }
    let fields = args.as_object().ok_or_else(invalid_audio)?;
    if fields.len() != 1 {
        return Err(invalid_audio());
    }
    let audio = fields
        .get("audio")
        .and_then(Value::as_object)
        .ok_or_else(invalid_audio)?;
    if audio.len() != 1 {
        return Err(invalid_audio());
    }
    let raw = audio
        .get("app_path")
        .and_then(Value::as_str)
        .ok_or_else(invalid_audio)?;
    let path = raw.strip_prefix('/').unwrap_or(raw);
    if path.is_empty()
        || path.contains(['\\', ':', '?', '#', '\0'])
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
        || !path.to_ascii_lowercase().ends_with(".wav")
    {
        return Err(invalid_audio());
    }
    let root = host_dir
        .parent()
        .ok_or_else(invalid_audio)?
        .join(app_id)
        .canonicalize()
        .map_err(|_| invalid_audio())?;
    let file = root
        .join(path)
        .canonicalize()
        .map_err(|_| invalid_audio())?;
    if !file.starts_with(&root)
        || !file.is_file()
        || file.metadata().map_err(|_| invalid_audio())?.len() > MAX_WAV as u64
    {
        return Err(invalid_audio());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .map_err(|_| invalid_audio())?
        .take(MAX_WAV as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid_audio())?;
    if bytes.len() <= 44
        || bytes.len() > MAX_WAV
        || &bytes[..4] != b"RIFF"
        || &bytes[8..12] != b"WAVE"
        || u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8 != bytes.len()
        || &bytes[12..16] != b"fmt "
        || u32::from_le_bytes(bytes[16..20].try_into().unwrap()) != 16
        || u16::from_le_bytes(bytes[20..22].try_into().unwrap()) != 1
        || u16::from_le_bytes(bytes[22..24].try_into().unwrap()) != 1
        || u32::from_le_bytes(bytes[24..28].try_into().unwrap()) != 16_000
        || u32::from_le_bytes(bytes[28..32].try_into().unwrap()) != 32_000
        || u16::from_le_bytes(bytes[32..34].try_into().unwrap()) != 2
        || u16::from_le_bytes(bytes[34..36].try_into().unwrap()) != 16
        || &bytes[36..40] != b"data"
        || u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize + 44 != bytes.len()
        || (bytes.len() - 44) % 2 != 0
    {
        return Err(invalid_audio());
    }
    Ok(bytes)
}

fn form(wav: &[u8]) -> (String, Vec<u8>) {
    let boundary = format!("octosense-{}", uuid::Uuid::new_v4().simple());
    let mut body = Vec::with_capacity(wav.len() + 512);
    for (name, value) in [
        ("model", "asr-1.0"),
        ("response_format", "json"),
        ("stream", "false"),
    ] {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    }
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"voice.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes());
    body.extend_from_slice(wav);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary, body)
}

pub(super) fn run(context: &Context) -> Result<Value, Refusal> {
    if !supported() {
        return Err(unavailable());
    }
    if !context.call.may_prompt || context.call.from_sheet {
        return Err(unavailable());
    }
    context.check()?;
    let wav = load_audio(&context.call.host_dir, &context.call.app_id, &context.call.args)?;
    let route = route(&context.providers()?)?;
    let seconds = ((wav.len() - 44) as u64 + 31_999) / 32_000;
    super::reserve(context, "transcription", seconds, (context.host.clock)())?;
    context.check()?;
    if !context
        .providers()?
        .iter()
        .any(|candidate| wire::binding(candidate) == route.binding)
    {
        return Err(super::cancelled());
    }
    let (boundary, body) = form(&wav);
    let headers = [
        ("Authorization".into(), format!("Bearer {}", route.key)),
        (
            "Content-Type".into(),
            format!("multipart/form-data; boundary={boundary}"),
        ),
    ];
    let (status, bytes) = context
        .host
        .media
        .transport
        .request_bytes(
            "POST",
            &format!("{}/v1/speech_to_text", route.base),
            &headers,
            &body,
        )
        .map_err(|_| provider_error())?;
    context.check()?;
    if !context
        .providers()?
        .iter()
        .any(|candidate| wire::binding(candidate) == route.binding)
    {
        return Err(super::cancelled());
    }
    if bytes.len() > RESPONSE_MAX {
        return Err(Refusal::new(
            Code::TooLarge,
            "The transcription response is too large.",
        ));
    }
    if status == 429 {
        return Err(Refusal::new(
            Code::Rate,
            "The transcription provider is rate limited.",
        ));
    }
    if !(200..300).contains(&status) {
        return Err(provider_error());
    }
    let answer: Value = serde_json::from_slice(&bytes).map_err(|_| provider_error())?;
    let text = answer
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty() && text.len() <= MAX_TEXT)
        .ok_or_else(provider_error)?;
    let duration = answer
        .get("duration")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && *n > 0.0 && *n <= 35.0)
        .ok_or_else(provider_error)?;
    Ok(json!({"text":text,"duration_seconds":duration}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_windows_is_supported() {
        assert!(supported_on("windows"));
        for platform in ["macos", "android", "linux", "ios"] {
            assert!(!supported_on(platform));
        }
    }
}
