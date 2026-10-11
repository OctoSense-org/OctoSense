use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Vendor {
    OpenAi,
    MiniMax,
}
pub(super) struct Route {
    pub vendor: Vendor,
    pub base: String,
    pub key: String,
    pub model: String,
    pub binding: String,
}
// A non-reversible binding binds outstanding jobs to the current configured
// credential+route without retaining a credential in the job table.
pub(super) fn binding(candidate: &Candidate) -> String {
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(&candidate.provider).unwrap_or_default());
    hash.update([0]);
    hash.update(candidate.key.as_deref().unwrap_or_default().as_bytes());
    format!("{:x}", hash.finalize())
}
pub(super) fn route(candidates: &[Candidate], kind: &str, class: Class) -> Result<Route, Refusal> {
    for candidate in candidates {
        if candidate.provider.api_type == Some(octosense_llm_config::ApiType::Anthropic) {
            continue;
        }
        let family = candidate.provider.family.to_ascii_lowercase();
        let vendor = match family.as_str() {
            "openai" if kind != "video" => Vendor::OpenAi,
            "minimax" | "minimax-cn" | "minimaxi" if kind != "embeddings" => Vendor::MiniMax,
            // An explicit embedding model is required on a custom compatible
            // route; a generic chat provider (especially DeepSeek) is not one.
            "llama-server" | "openai-compatible" | "lmstudio"
                if kind == "embeddings"
                    && candidate
                        .provider
                        .model
                        .as_deref()
                        .is_some_and(|m| m.starts_with("text-embedding-")) =>
            {
                Vendor::OpenAi
            }
            _ => continue,
        };
        let Some(key) = candidate
            .key
            .as_deref()
            .filter(|s| !s.is_empty() && !s.contains(['\r', '\n']))
        else {
            continue;
        };
        let Some(base) = crate::model::effective_base_url(&candidate.provider) else {
            continue;
        };
        let Ok(mut url) = Url::parse(&base) else {
            continue;
        };
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            continue;
        }
        let path = url.path().trim_end_matches('/');
        // Preserve the configured origin/path, never retarget a proxy's key to
        // a different vendor. An Anthropic-only route is not a media endpoint.
        if path.ends_with("/anthropic") {
            continue;
        }
        let path = path.strip_suffix("/v1").unwrap_or(path).to_string();
        url.set_path(&path);
        let base = url.as_str().trim_end_matches('/').to_string();
        let strong = class == Class::Strong;
        let model = match (vendor, kind) {
            (Vendor::OpenAi, "image") => {
                if strong {
                    "gpt-image-1"
                } else {
                    "gpt-image-1-mini"
                }
            }
            (Vendor::OpenAi, "audio") => "gpt-4o-mini-tts",
            (Vendor::OpenAi, "embeddings") if family != "openai" => {
                candidate.provider.model.as_deref().unwrap()
            }
            (Vendor::OpenAi, "embeddings") => {
                if strong {
                    "text-embedding-3-large"
                } else {
                    "text-embedding-3-small"
                }
            }
            (Vendor::MiniMax, "image") => "image-01",
            (Vendor::MiniMax, "audio") => {
                if strong {
                    "speech-2.8-hd"
                } else {
                    "speech-2.8-turbo"
                }
            }
            (Vendor::MiniMax, "video") => "MiniMax-H3",
            _ => continue,
        };
        return Ok(Route {
            vendor,
            base,
            key: key.into(),
            model: model.into(),
            binding: binding(candidate),
        });
    }
    Err(Refusal::new(Code::NoProvider, format!("No configured provider supports {kind}. Configure a compatible media provider in AI providers.")))
}

pub(super) enum Request {
    Capabilities,
    Image {
        prompt: String,
        width: u32,
        height: u32,
        seed: Option<u64>,
        class: Class,
    },
    Audio {
        text: String,
        voice: String,
        class: Class,
    },
    Embeddings {
        inputs: Vec<String>,
        single: bool,
        class: Class,
    },
    Video {
        prompt: String,
        duration: u64,
        resolution: String,
        ratio: String,
        class: Class,
    },
    Job {
        id: String,
        cancel: bool,
    },
}
fn object<'a>(
    value: &'a Value,
    allowed: &[&str],
) -> Result<&'a serde_json::Map<String, Value>, Refusal> {
    let fields = value
        .as_object()
        .ok_or_else(|| bad("Arguments must be a JSON object."))?;
    if fields.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(bad("An unsupported argument was supplied."));
    }
    Ok(fields)
}
fn text(value: &Value, field: &str, max: usize) -> Result<String, Refusal> {
    value[field]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= max && !s.contains('\0'))
        .map(str::to_string)
        .ok_or_else(|| {
            bad(&format!(
                "{field} must be nonempty text, at most {max} UTF-8 bytes."
            ))
        })
}
fn class(value: &Value) -> Result<Class, Refusal> {
    if let Some(output) = value.get("output") {
        object(output, &["class"])?;
    }
    if value.get("class").is_some() && value.get("output").and_then(|o| o.get("class")).is_some() {
        return Err(bad("Specify class once, either at the root or in output."));
    }
    match value
        .get("class")
        .or_else(|| value.get("output").and_then(|o| o.get("class")))
    {
        None => Ok(Class::Fast),
        Some(Value::String(s)) if s == "fast" => Ok(Class::Fast),
        Some(Value::String(s)) if s == "strong" => Ok(Class::Strong),
        _ => Err(bad("class must be fast or strong.")),
    }
}
impl Request {
    pub fn parse(method: &str, value: &Value) -> Result<Self, Refusal> {
        match method {
            "capabilities" => {
                object(value, &[])?;
                Ok(Self::Capabilities)
            }
            "video.status" | "video.cancel" => {
                object(value, &["job"])?;
                let id = text(value, "job", 64)?;
                uuid::Uuid::parse_str(&id).map_err(|_| bad("Invalid video job handle."))?;
                Ok(Self::Job {
                    id,
                    cancel: method == "video.cancel",
                })
            }
            "image" => {
                object(value, &["prompt", "size", "n", "seed", "class", "output"])?;
                if value.get("n").is_some_and(|n| n.as_u64() != Some(1)) {
                    return Err(bad(
                        "Only n:1 is supported; submit separate budgeted requests.",
                    ));
                }
                let size = match value.get("size") {
                    None => "1024x1024",
                    Some(Value::String(s)) => s,
                    _ => return Err(bad("size must be a supported widthxheight string.")),
                };
                let (width, height) = match size {
                    "512x512" => (512, 512),
                    "1024x1024" => (1024, 1024),
                    "1536x1024" => (1536, 1024),
                    "1024x1536" => (1024, 1536),
                    _ => return Err(bad("Unsupported image size.")),
                };
                let seed = value
                    .get("seed")
                    .map(|n| {
                        n.as_u64()
                            .filter(|n| *n <= u32::MAX as u64)
                            .ok_or_else(|| bad("seed must be an unsigned 32-bit integer."))
                    })
                    .transpose()?;
                Ok(Self::Image {
                    prompt: text(value, "prompt", 4096)?,
                    width,
                    height,
                    seed,
                    class: class(value)?,
                })
            }
            "audio" => {
                object(value, &["text", "voice", "format", "class", "output"])?;
                if value.get("format").is_some_and(|v| v != "mp3") {
                    return Err(bad("The initial audio service supports mp3 only."));
                }
                let voice = match value.get("voice") {
                    None => "default",
                    Some(Value::String(s)) if matches!(s.as_str(), "default" | "warm-female") => s,
                    _ => return Err(bad("voice must be default or warm-female.")),
                };
                Ok(Self::Audio {
                    text: text(value, "text", 4096)?,
                    voice: voice.into(),
                    class: class(value)?,
                })
            }
            "embeddings" => {
                object(value, &["input", "class", "output"])?;
                let (inputs, single) = match &value["input"] {
                    Value::String(s) => (vec![s.clone()], true),
                    Value::Array(a) if !a.is_empty() && a.len() <= 16 => (
                        a.iter()
                            .map(|v| {
                                v.as_str()
                                    .map(str::to_string)
                                    .ok_or_else(|| bad("Every embedding input must be text."))
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                        false,
                    ),
                    _ => return Err(bad("input must be text or an array of 1–16 texts.")),
                };
                if inputs
                    .iter()
                    .any(|s| s.trim().is_empty() || s.len() > 4096 || s.contains('\0'))
                {
                    return Err(bad(
                        "Each embedding input must be nonempty and at most 4096 UTF-8 bytes.",
                    ));
                }
                Ok(Self::Embeddings {
                    inputs,
                    single,
                    class: class(value)?,
                })
            }
            "video" => {
                // H3 exposes resolution/ratio, not exact pixel dimensions, fps
                // or seed. Refuse unsupported knobs instead of ignoring them.
                object(
                    value,
                    &[
                        "prompt",
                        "duration_s",
                        "resolution",
                        "ratio",
                        "class",
                        "output",
                    ],
                )?;
                let duration = value
                    .get("duration_s")
                    .map(|d| {
                        d.as_u64()
                            .filter(|d| (4..=12).contains(d))
                            .ok_or_else(|| bad("duration_s must be an integer from 4 to 12."))
                    })
                    .transpose()?
                    .unwrap_or(4);
                let resolution = match value.get("resolution") {
                    None => "768P",
                    Some(Value::String(s)) if s == "768P" => s,
                    _ => return Err(bad("Only 768P video is enabled by this host budget.")),
                };
                let ratio = match value.get("ratio") {
                    None => "16:9",
                    Some(Value::String(s)) if matches!(s.as_str(), "16:9" | "9:16" | "1:1") => s,
                    _ => return Err(bad("ratio must be 16:9, 9:16 or 1:1.")),
                };
                Ok(Self::Video {
                    prompt: text(value, "prompt", 4096)?,
                    duration,
                    resolution: resolution.into(),
                    ratio: ratio.into(),
                    class: class(value)?,
                })
            }
            _ => Err(bad("Unknown media method.")),
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Image { .. } => "image",
            Self::Audio { .. } => "audio",
            Self::Embeddings { .. } => "embeddings",
            Self::Video { .. } => "video",
            _ => "",
        }
    }
    pub fn class(&self) -> Class {
        match self {
            Self::Image { class, .. }
            | Self::Audio { class, .. }
            | Self::Embeddings { class, .. }
            | Self::Video { class, .. } => *class,
            _ => Class::Fast,
        }
    }
    pub fn units(&self) -> u64 {
        match self {
            Self::Image { width, height, .. } => *width as u64 * *height as u64,
            Self::Audio { text, .. } => text.chars().count() as u64,
            Self::Embeddings { inputs, .. } => inputs.iter().map(|s| s.len() as u64).sum(),
            Self::Video { duration, .. } => *duration,
            _ => 0,
        }
    }
}

pub(super) fn prepare(route: &Route, request: &Request) -> Result<(&'static str, Value), Refusal> {
    Ok(match request {
        Request::Image {
            prompt,
            width,
            height,
            seed,
            class,
        } => match route.vendor {
            Vendor::MiniMax => {
                let mut body = json!({"model":route.model,"prompt":prompt,"width":width,"height":height,"n":1,"response_format":"base64"});
                if let Some(seed) = seed {
                    body["seed"] = json!(seed);
                }
                ("/v1/image_generation", body)
            }
            Vendor::OpenAi => {
                if seed.is_some() || *width == 512 {
                    return Err(bad("The configured image route does not support seed or 512x512. Use 1024x1024 without seed."));
                }
                (
                    "/v1/images/generations",
                    json!({"model":route.model,"prompt":prompt,"n":1,"size":format!("{width}x{height}"),"quality":if *class == Class::Strong {"high"}else{"low"},"output_format":"png"}),
                )
            }
        },
        Request::Audio { text, voice, .. } => match route.vendor {
            Vendor::OpenAi => (
                "/v1/audio/speech",
                json!({"model":route.model,"input":text,"voice":if voice == "warm-female" {"coral"}else{"alloy"},"response_format":"mp3"}),
            ),
            Vendor::MiniMax => (
                "/v1/t2a_v2",
                json!({"model":route.model,"text":text,"stream":false,"output_format":"hex",
                "voice_setting":{"voice_id":if voice == "warm-female" {"English_Graceful_Lady"}else{"English_expressive_narrator"},"speed":1,"vol":1,"pitch":0},
                "audio_setting":{"sample_rate":32000,"bitrate":128000,"format":"mp3","channel":1}}),
            ),
        },
        Request::Embeddings { inputs, .. } => (
            "/v1/embeddings",
            json!({"model":route.model,"input":inputs,"encoding_format":"float"}),
        ),
        Request::Video {
            prompt,
            duration,
            resolution,
            ratio,
            ..
        } => (
            "/v2/video_generation",
            json!({"model":route.model,"content":[{"type":"text","text":prompt}],"duration":duration,"resolution":resolution,"ratio":ratio}),
        ),
        _ => return Err(bad("No provider submission for this request.")),
    })
}

pub(super) fn audio(bytes: &[u8], duration: Option<f64>) -> Result<Value, Refusal> {
    if bytes.is_empty() || bytes.len() > ASSET_MAX {
        return Err(Refusal::new(
            Code::TooLarge,
            "Generated audio exceeds the host byte limit or is empty.",
        ));
    }
    if !(bytes.starts_with(b"ID3")
        || (bytes.len() > 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0))
    {
        return Err(invalid());
    }
    let mut out = json!({"b64_json":STANDARD.encode(bytes),"format":"mp3","mime":"audio/mpeg","bytes":bytes.len()});
    if let Some(duration) = duration {
        out["duration_s"] = json!(duration);
    }
    Ok(out)
}
pub(super) fn decode(request: &Request, route: &Route, response: Value) -> Result<Value, Refusal> {
    match request {
        Request::Image { width, height, .. } => {
            let encoded = match route.vendor {
                Vendor::OpenAi => &response["data"][0]["b64_json"],
                Vendor::MiniMax => &response["data"]["image_base64"][0],
            };
            let encoded = encoded
                .as_str()
                .filter(|s| s.len() <= ASSET_MAX.div_ceil(3) * 4)
                .ok_or_else(invalid)?;
            let bytes = STANDARD.decode(encoded).map_err(|_| invalid())?;
            if bytes.len() > ASSET_MAX {
                return Err(Refusal::new(
                    Code::TooLarge,
                    "Generated image exceeds the host byte limit.",
                ));
            }
            let format = image::guess_format(&bytes).map_err(|_| invalid())?;
            let (format_name, mime) = match format {
                image::ImageFormat::Png => ("png", "image/png"),
                image::ImageFormat::Jpeg => ("jpg", "image/jpeg"),
                _ => return Err(invalid()),
            };
            let size = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format)
                .into_dimensions()
                .map_err(|_| invalid())?;
            if size != (*width, *height) {
                return Err(invalid());
            }
            Ok(
                json!({"b64_json":STANDARD.encode(bytes),"format":format_name,"mime":mime,"width":width,"height":height}),
            )
        }
        Request::Audio { .. } => {
            let hex = response["data"]["audio"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= ASSET_MAX * 2 && s.len() % 2 == 0)
                .ok_or_else(invalid)?;
            let mut bytes = Vec::with_capacity(hex.len() / 2);
            for pair in hex.as_bytes().chunks_exact(2) {
                fn digit(b: u8) -> Option<u8> {
                    match b {
                        b'0'..=b'9' => Some(b - b'0'),
                        b'a'..=b'f' => Some(b - b'a' + 10),
                        b'A'..=b'F' => Some(b - b'A' + 10),
                        _ => None,
                    }
                }
                bytes.push(
                    digit(pair[0]).ok_or_else(invalid)? * 16
                        + digit(pair[1]).ok_or_else(invalid)?,
                );
            }
            let duration = response["extra_info"]["audio_length"]
                .as_f64()
                .filter(|d| d.is_finite() && *d >= 0.0 && *d <= 600_000.0)
                .map(|ms| ms / 1000.0);
            audio(&bytes, duration)
        }
        Request::Embeddings { inputs, single, .. } => {
            let data = response["data"]
                .as_array()
                .filter(|a| a.len() == inputs.len())
                .ok_or_else(invalid)?;
            let mut vectors = vec![None; inputs.len()];
            let mut dimension = None;
            for item in data {
                let index = item["index"]
                    .as_u64()
                    .filter(|i| *i < inputs.len() as u64)
                    .ok_or_else(invalid)? as usize;
                if vectors[index].is_some() {
                    return Err(invalid());
                }
                let vector = item["embedding"]
                    .as_array()
                    .filter(|v| {
                        !v.is_empty()
                            && v.len() <= 4096
                            && v.iter().all(|x| x.as_f64().is_some_and(f64::is_finite))
                    })
                    .ok_or_else(invalid)?;
                if dimension.is_some_and(|d| d != vector.len()) {
                    return Err(invalid());
                }
                dimension = Some(vector.len());
                vectors[index] = Some(Value::Array(vector.clone()));
            }
            let vectors: Vec<Value> = vectors
                .into_iter()
                .collect::<Option<_>>()
                .ok_or_else(invalid)?;
            if *single {
                Ok(json!({"embedding":vectors[0],"dimensions":dimension}))
            } else {
                Ok(json!({"embeddings":vectors,"dimensions":dimension}))
            }
        }
        _ => Err(invalid()),
    }
}

pub(super) fn schemas() -> Vec<(&'static str, &'static str, Value, Value)> {
    let class = json!({"enum":["fast","strong"]});
    let output = json!({"type":"object","properties":{"class":class},"additionalProperties":false});
    let mut image = json!({"prompt":{"type":"string","minLength":1,"maxLength":4096},"size":{"enum":["512x512","1024x1024","1536x1024","1024x1536"]},"n":{"const":1},"seed":{"type":"integer","minimum":0,"maximum":4294967295u64}});
    let mut audio = json!({"text":{"type":"string","minLength":1,"maxLength":4096},"voice":{"enum":["default","warm-female"]},"format":{"const":"mp3"}});
    let mut embeddings = json!({"input":{"oneOf":[{"type":"string","minLength":1,"maxLength":4096},{"type":"array","minItems":1,"maxItems":16,"items":{"type":"string","minLength":1,"maxLength":4096}}]}});
    let mut video = json!({"prompt":{"type":"string","minLength":1,"maxLength":4096},"duration_s":{"type":"integer","minimum":4,"maximum":12},"resolution":{"const":"768P"},"ratio":{"enum":["16:9","9:16","1:1"]}});
    for args in [&mut image, &mut audio, &mut embeddings, &mut video] {
        args["class"] = class.clone();
        args["output"] = output.clone();
    }
    let object = |properties, required| json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
    let result = json!({"type":"object"});
    let job = object(
        json!({"job":{"type":"string","format":"uuid"}}),
        json!(["job"]),
    );
    vec![
        (
            "model.image",
            "Generate one bounded image with a configured host provider.",
            object(image, json!(["prompt"])),
            result.clone(),
        ),
        (
            "model.audio",
            "Generate bounded MP3 speech; disclose that the voice is AI-generated.",
            object(audio, json!(["text"])),
            result.clone(),
        ),
        (
            "model.embeddings",
            "Embed bounded text using a configured compatible embedding provider.",
            object(embeddings, json!(["input"])),
            result.clone(),
        ),
        (
            "model.video",
            "Submit a budgeted video job once; poll its opaque app-scoped handle.",
            object(video, json!(["prompt"])),
            result.clone(),
        ),
        (
            "model.video.status",
            "Read this app's video job; polling is throttled and does not resubmit.",
            job.clone(),
            result.clone(),
        ),
        (
            "model.video.cancel",
            "Ask the provider to cancel a queued video; running jobs cannot be cancelled.",
            job,
            result.clone(),
        ),
        (
            "model.capabilities",
            "Check configured media routes and limits, not provider entitlements.",
            object(json!({}), json!([])),
            result,
        ),
    ]
}
