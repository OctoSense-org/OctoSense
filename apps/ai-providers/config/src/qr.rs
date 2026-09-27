//! The provisioning QR.
//!
//! Three payloads are understood:
//!
//! ```text
//! OCTOS1E:<base45(zlib(salt16 ‖ nonce12 ‖ chacha20poly1305(zlib(JSON))))>   — PIN-wrapped
//! OCTOS1:<base45(zlib(JSON))>                                               — plain
//! {"llm_family":…,"llm_model":…,"llm_key":…,"llm_base_url":…,"llm_api_type":…} — legacy a2app
//! ```
//!
//! The first two are octos's own profile-QR format (octos-cli
//! `src/profile_qr.rs`), re-implemented byte-compatibly: the JSON is octos's
//! `ProfileQrPayload` (`v: 1`, `kind: "octos-profile"`, `id`, `llm` = the
//! `LlmProfileConfig` contract verbatim, `secrets` = env var name → key).
//! The PIN key is Argon2id(PIN, salt; m = 64 MiB, t = 3, p = 1) — so
//! **decoding an `OCTOS1E:` code costs ~64 MiB and a noticeable fraction of a
//! second: run [`decode`] off the UI thread.** base45 (RFC 9285) keeps the
//! string in the QR alphanumeric mode.
//!
//! A device accepts LLM configuration only: a payload carrying `endpoint` or
//! `auth_token` (octos server configuration) is refused, and `name`,
//! `memory`, `embedding`, `voice_default` are ignored. Key env var names
//! must be ones a registry family reads or end in `_API_KEY` (a QR must not
//! be able to set `PATH` or `LD_PRELOAD`), and a provider with a custom
//! `base_url` must carry its own key — otherwise a hostile code could point
//! an already-stored key at its own endpoint.

use crate::{json_error, profile, registry, ApiType, Error, Provider, ProviderSet, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};

/// Plain-format prefix.
pub const PREFIX_PLAIN: &str = "OCTOS1:";
/// PIN-encrypted-format prefix.
pub const PREFIX_ENCRYPTED: &str = "OCTOS1E:";
/// The largest input `decode` looks at — a version-40 QR holds 4296
/// alphanumeric characters; anything longer did not come from a QR.
pub const MAX_INPUT_LEN: usize = 8 * 1024;

const PAYLOAD_KIND: &str = "octos-profile";
const PROFILE_ID: &str = "_main";
const BASE45_ALPHABET: &[u8; 45] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const MAX_DECOMPRESSED: u64 = 1024 * 1024;
/// Crockford base32 (no I/L/O/U), as octos's `generate_pin`.
const PIN_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A decoded (or to-be-encoded) provisioning: the provider set and the keys
/// it needs, by env var name. `Debug` redacts the key values.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Provisioning {
    pub set: ProviderSet,
    pub secrets: BTreeMap<String, String>,
}

impl std::fmt::Debug for Provisioning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&String> = self.secrets.keys().collect();
        f.debug_struct("Provisioning").field("set", &self.set).field("secrets", &names).finish()
    }
}

/// Which payload a scanned string is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Encrypted,
    Plain,
    LegacyJson,
}

/// Classify a scanned string without decoding it; `None` for foreign codes.
pub fn format_of(text: &str) -> Option<Format> {
    let t = text.trim();
    if t.starts_with(PREFIX_ENCRYPTED) {
        Some(Format::Encrypted)
    } else if t.starts_with(PREFIX_PLAIN) {
        Some(Format::Plain)
    } else if t.starts_with('{') {
        Some(Format::LegacyJson)
    } else {
        None
    }
}

/// Whether `text` is an `OCTOS1E:` code (ask for the PIN before decoding).
pub fn needs_pin(text: &str) -> bool {
    format_of(text) == Some(Format::Encrypted)
}

/// A random transfer PIN, `XXXX-XXXX` in Crockford base32 (40 bits) — the
/// same shape octos generates. Shown BESIDE the QR, never inside it.
pub fn generate_pin() -> String {
    use chacha20poly1305::aead::rand_core::RngCore;
    let mut rng = chacha20poly1305::aead::OsRng;
    let mut pin = String::with_capacity(9);
    for i in 0..8 {
        if i == 4 {
            pin.push('-');
        }
        pin.push(PIN_ALPHABET[(rng.next_u32() % 32) as usize] as char);
    }
    pin
}

/// Encode as `OCTOS1E:` under `pin` (at least 6 characters, as octos).
pub fn encode_encrypted(p: &Provisioning, pin: &str) -> Result<String> {
    let pin = pin.trim();
    if pin.chars().count() < 6 {
        return Err(Error::Invalid("the PIN must be at least 6 characters".into()));
    }
    let json = payload_json(p)?;
    let sealed = encrypt(&compress(&json)?, &normalize_pin(pin).unwrap_or_else(|| pin.to_string()))?;
    Ok(format!("{PREFIX_ENCRYPTED}{}", base45_encode(&sealed)))
}

/// Encode as plain `OCTOS1:`. Refuses a set carrying keys unless
/// `allow_secrets` — anyone who photographs a plain code owns its contents.
pub fn encode_plain(p: &Provisioning, allow_secrets: bool) -> Result<String> {
    if !p.secrets.is_empty() && !allow_secrets {
        return Err(Error::SecretsNeedPin);
    }
    let json = payload_json(p)?;
    Ok(format!("{PREFIX_PLAIN}{}", base45_encode(&compress(&json)?)))
}

/// Decode any of the three payloads. `pin` is required for `OCTOS1E:`; a
/// Crockford PIN is accepted in any case, with or without its hyphen.
///
/// CPU/memory heavy for `OCTOS1E:` (Argon2id, 64 MiB): never call this on the
/// UI thread.
pub fn decode(text: &str, pin: Option<&str>) -> Result<Provisioning> {
    let t = text.trim();
    if t.len() > MAX_INPUT_LEN {
        return Err(Error::Invalid(format!("input longer than {MAX_INPUT_LEN} bytes")));
    }
    let payload = match format_of(t) {
        Some(Format::Encrypted) => {
            let pin = pin.map(str::trim).filter(|p| !p.is_empty()).ok_or(Error::PinRequired)?;
            let sealed = base45_decode(&t[PREFIX_ENCRYPTED.len()..])?;
            let normalized = normalize_pin(pin);
            let compressed = match decrypt(&sealed, normalized.as_deref().unwrap_or(pin)) {
                // A custom octos PIN that merely looks like a Crockford one.
                Err(Error::WrongPin) if normalized.as_deref().is_some_and(|n| n != pin) => decrypt(&sealed, pin)?,
                other => other?,
            };
            parse_payload(&decompress(&compressed)?)?
        }
        Some(Format::Plain) => parse_payload(&decompress(&base45_decode(&t[PREFIX_PLAIN.len()..])?)?)?,
        Some(Format::LegacyJson) => return decode_legacy(t),
        None => return Err(Error::Unsupported("not an OctoSense provisioning code".into())),
    };
    from_payload(payload)
}

/// The QR symbol version `text` encodes to at ECC level M.
pub fn qr_version(text: &str) -> Result<i16> {
    match qr_code(text)?.version() {
        qrcode::Version::Normal(v) | qrcode::Version::Micro(v) => Ok(v),
    }
}

/// An SVG of the QR (ECC level M, 4-module quiet zone).
pub fn render_svg(text: &str) -> Result<String> {
    Ok(qr_code(text)?
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(256, 256)
        .quiet_zone(true)
        .build())
}

/// The QR modules (ECC level M, no quiet zone): `(width, dark)` with `dark`
/// row-major, `width * width` long — for UI toolkits to draw themselves.
pub fn render_matrix(text: &str) -> Result<(usize, Vec<bool>)> {
    let code = qr_code(text)?;
    let dark = code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect();
    Ok((code.width(), dark))
}

/// An 8-bit greyscale PNG of the QR, `scale` pixels per module, 4-module
/// quiet zone.
#[cfg(feature = "png")]
pub fn render_png(text: &str, scale: usize) -> Result<Vec<u8>> {
    let (width, dark) = render_matrix(text)?;
    let scale = scale.max(1);
    let side = (width + 8) * scale;
    let mut pixels = vec![255u8; side * side];
    for y in 0..width {
        for x in 0..width {
            if dark[y * width + x] {
                for dy in 0..scale {
                    let row = ((y + 4) * scale + dy) * side;
                    let col = (x + 4) * scale;
                    pixels[row + col..row + col + scale].fill(0);
                }
            }
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, side as u32, side as u32);
        enc.set_color(png::ColorType::Grayscale);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| Error::Invalid(format!("png: {e}")))?;
        w.write_image_data(&pixels).map_err(|e| Error::Invalid(format!("png: {e}")))?;
    }
    Ok(out)
}

fn qr_code(text: &str) -> Result<qrcode::QrCode> {
    qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M)
        .map_err(|e| Error::Invalid(format!("QR encode: {e}")))
}

// ---------------------------------------------------------------------------
// Payload (octos `ProfileQrPayload`, same field order)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Payload {
    v: u32,
    kind: String,
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    llm: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    memory: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    embedding: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    voice_default: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    secrets: BTreeMap<String, String>,
}

fn payload_json(p: &Provisioning) -> Result<Vec<u8>> {
    let p = validate(p.clone())?;
    let mut llm = serde_json::Map::new();
    if let Some(primary) = &p.set.primary {
        llm.insert("primary".into(), profile::selection_value(primary));
    }
    llm.insert(
        "fallbacks".into(),
        Value::Array(p.set.fallbacks.iter().map(profile::selection_value).collect()),
    );
    let payload = Payload {
        v: 1,
        kind: PAYLOAD_KIND.into(),
        id: PROFILE_ID.into(),
        name: None,
        endpoint: None,
        auth_token: None,
        llm: if p.set.is_empty() { None } else { Some(Value::Object(llm)) },
        memory: None,
        embedding: None,
        voice_default: None,
        secrets: p.secrets,
    };
    serde_json::to_vec(&payload).map_err(|e| Error::Invalid(json_error(&e)))
}

fn parse_payload(json: &[u8]) -> Result<Payload> {
    serde_json::from_slice(json).map_err(|e| Error::Invalid(format!("payload: {}", json_error(&e))))
}

fn from_payload(p: Payload) -> Result<Provisioning> {
    if p.kind != PAYLOAD_KIND {
        return Err(Error::Unsupported(format!("payload kind `{}`", p.kind)));
    }
    if p.v != 1 {
        return Err(Error::Unsupported(format!("payload version {}", p.v)));
    }
    if p.endpoint.is_some() || p.auth_token.is_some() {
        return Err(Error::Unsupported(
            "server configuration (endpoint / auth token) is not accepted here — this code only sets LLM providers".into(),
        ));
    }
    let mut set = ProviderSet::default();
    if let Some(llm) = &p.llm {
        if !llm.is_object() {
            return Err(Error::Invalid("`llm` must be an object".into()));
        }
        if let Some(primary) = llm.get("primary").filter(|v| !v.is_null()) {
            set.primary = Some(
                profile::provider_from_selection(primary)
                    .ok_or_else(|| Error::Invalid("primary selection has no family_id".into()))?,
            );
        }
        if let Some(fallbacks) = llm.get("fallbacks").filter(|v| !v.is_null()) {
            let arr = fallbacks.as_array().ok_or_else(|| Error::Invalid("`fallbacks` must be a list".into()))?;
            for fb in arr {
                set.fallbacks.push(
                    profile::provider_from_selection(fb)
                        .ok_or_else(|| Error::Invalid("fallback selection has no family_id".into()))?,
                );
            }
        }
    }
    validate(Provisioning { set, secrets: p.secrets })
}

// ---------------------------------------------------------------------------
// Legacy a2app JSON
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyConfig {
    llm_family: String,
    llm_model: Option<String>,
    llm_key: String,
    llm_base_url: Option<String>,
    llm_api_type: Option<String>,
}

fn decode_legacy(text: &str) -> Result<Provisioning> {
    let c: LegacyConfig = serde_json::from_str(text)
        .map_err(|e| Error::Invalid(format!("LLM config: {}", json_error(&e))))?;
    if c.llm_family.trim().is_empty() {
        return Err(Error::Invalid("llm_family must not be empty".into()));
    }
    if c.llm_key.trim().is_empty() {
        return Err(Error::Invalid("llm_key must not be empty".into()));
    }
    let api_type = match c.llm_api_type.as_deref() {
        None => None,
        Some(t) => Some(ApiType::parse(t).ok_or_else(|| {
            Error::Invalid("llm_api_type must be openai, anthropic or responses".into())
        })?),
    };
    let mut primary = Provider::new(c.llm_family.trim(), c.llm_model);
    primary.base_url = c.llm_base_url;
    primary.api_type = api_type;
    let secrets = BTreeMap::from([(primary.key_env.clone(), c.llm_key.trim().to_string())]);
    validate(Provisioning { set: ProviderSet { primary: Some(primary), fallbacks: Vec::new() }, secrets })
}

// ---------------------------------------------------------------------------
// Validation (shared by every format and by the encoders)
// ---------------------------------------------------------------------------

/// A key env var a QR may set: a legal name that a registry family reads or
/// that ends in `_API_KEY`.
fn acceptable_key_env(name: &str) -> bool {
    crate::is_env_name(name) && (registry::is_known_key_env(name) || name.ends_with("_API_KEY"))
}

fn validate_base_url(s: &str) -> Result<String> {
    let bad = || Error::Invalid("base_url must be an http(s) endpoint without credentials, query or fragment".into());
    let u = url::Url::parse(s.trim()).map_err(|_| bad())?;
    if !matches!(u.scheme(), "http" | "https")
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
    {
        return Err(bad());
    }
    Ok(s.trim().to_string())
}

fn validate(mut p: Provisioning) -> Result<Provisioning> {
    let mut providers: Vec<&mut Provider> = p.set.primary.iter_mut().chain(p.set.fallbacks.iter_mut()).collect();
    for prov in providers.iter_mut() {
        let family = prov.family.trim();
        if family.is_empty() {
            return Err(Error::Invalid("family must not be empty".into()));
        }
        // Canonical ids: `z.ai` is stored as `zai`, `kimi-coding` as `moonshot-coding`.
        prov.family = match registry::lookup(family) {
            Some(f) => f.id.to_string(),
            None if family.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)) => {
                family.to_ascii_lowercase()
            }
            None => return Err(Error::Invalid("family must be [a-z0-9._-]".into())),
        };
        if let Some(m) = &prov.model {
            if m.trim().is_empty() {
                return Err(Error::Invalid(format!("{}: model must not be empty", prov.family)));
            }
            prov.model = Some(m.trim().to_string());
        }
        if let Some(u) = &prov.base_url {
            prov.base_url = Some(validate_base_url(u)?);
        }
        if !acceptable_key_env(&prov.key_env) {
            return Err(Error::Invalid(format!(
                "{}: key env `{}` must be a provider key variable (…_API_KEY)",
                prov.family, prov.key_env
            )));
        }
    }
    for (name, value) in &p.secrets {
        if !acceptable_key_env(name) {
            return Err(Error::Invalid(format!("key `{name}` must be a provider key variable (…_API_KEY)")));
        }
        if value.trim().is_empty() {
            return Err(Error::Invalid(format!("key `{name}` is empty")));
        }
    }
    for prov in p.set.iter() {
        let keyless = registry::lookup(&prov.family).is_some_and(|f| !f.key_required);
        if prov.base_url.is_some() && !keyless && !p.secrets.contains_key(&prov.key_env) {
            return Err(Error::Invalid(format!(
                "{}: a custom base_url must come with its own key ({})",
                prov.family, prov.key_env
            )));
        }
    }
    Ok(p)
}

/// `7k3m 9qx2`, `7K3M9QX2`, `7K3M-9QXZ` with I/L/O → `7K3M-9QX2` shape;
/// `None` when `pin` is not an 8-character Crockford PIN.
fn normalize_pin(pin: &str) -> Option<String> {
    let chars: Vec<u8> = pin
        .bytes()
        .filter(|b| !matches!(b, b'-' | b' '))
        .map(|b| match b.to_ascii_uppercase() {
            b'O' => b'0',
            b'I' | b'L' => b'1',
            c => c,
        })
        .collect();
    if chars.len() != 8 || !chars.iter().all(|c| PIN_ALPHABET.contains(c)) {
        return None;
    }
    let s = String::from_utf8(chars).ok()?;
    Some(format!("{}-{}", &s[..4], &s[4..]))
}

// ---------------------------------------------------------------------------
// base45 (RFC 9285), zlib, PIN wrap — byte-compatible with octos
// ---------------------------------------------------------------------------

fn base45_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 3 / 2 + 3);
    for chunk in data.chunks(2) {
        if let [a, b] = chunk {
            let n = u32::from(*a) * 256 + u32::from(*b);
            out.push(BASE45_ALPHABET[(n % 45) as usize] as char);
            out.push(BASE45_ALPHABET[(n / 45 % 45) as usize] as char);
            out.push(BASE45_ALPHABET[(n / 45 / 45) as usize] as char);
        } else {
            let n = u32::from(chunk[0]);
            out.push(BASE45_ALPHABET[(n % 45) as usize] as char);
            out.push(BASE45_ALPHABET[(n / 45) as usize] as char);
        }
    }
    out
}

fn base45_decode(s: &str) -> Result<Vec<u8>> {
    let value = |c: u8| {
        BASE45_ALPHABET
            .iter()
            .position(|&a| a == c)
            .map(|v| v as u32)
            .ok_or(Error::Tampered("invalid character"))
    };
    let bytes = s.as_bytes();
    if bytes.len() % 3 == 1 {
        return Err(Error::Tampered("invalid length"));
    }
    let mut out = Vec::with_capacity(bytes.len() / 3 * 2 + 1);
    for chunk in bytes.chunks(3) {
        match chunk {
            [a, b, c] => {
                let n = value(*a)? + value(*b)? * 45 + value(*c)? * 45 * 45;
                if n > 0xFFFF {
                    return Err(Error::Tampered("encoding out of range"));
                }
                out.push((n / 256) as u8);
                out.push((n % 256) as u8);
            }
            [a, b] => {
                let n = value(*a)? + value(*b)? * 45;
                if n > 0xFF {
                    return Err(Error::Tampered("encoding out of range"));
                }
                out.push(n as u8);
            }
            _ => unreachable!("length checked above"),
        }
    }
    Ok(out)
}

fn compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    enc.write_all(data)?;
    Ok(enc.finish()?)
}

fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    // Read one byte past the cap so an oversized stream is rejected, not
    // truncated into something that might still parse.
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .take(MAX_DECOMPRESSED + 1)
        .read_to_end(&mut out)
        .map_err(|_| Error::Tampered("compression"))?;
    if out.len() as u64 > MAX_DECOMPRESSED {
        return Err(Error::Tampered("oversized payload"));
    }
    Ok(out)
}

fn derive_key(pin: &str, salt: &[u8]) -> Result<[u8; 32]> {
    use argon2::{Algorithm, Argon2, Params, Version};
    // octos's parameters exactly: 64 MiB, t = 3, p = 1, 32-byte output.
    let params = Params::new(64 * 1024, 3, 1, Some(32)).map_err(|e| Error::Invalid(format!("argon2: {e}")))?;
    let mut key = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pin.as_bytes(), salt, &mut key)
        .map_err(|e| Error::Invalid(format!("argon2: {e}")))?;
    Ok(key)
}

fn encrypt(plaintext: &[u8], pin: &str) -> Result<Vec<u8>> {
    use chacha20poly1305::aead::rand_core::RngCore;
    use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
    use chacha20poly1305::{AeadCore, ChaCha20Poly1305};
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let key = derive_key(pin, &salt)?;
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = ChaCha20Poly1305::new((&key).into())
        .encrypt(&nonce, plaintext)
        .map_err(|_| Error::Invalid("encryption failed".into()))?;
    let mut out = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn decrypt(body: &[u8], pin: &str) -> Result<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::ChaCha20Poly1305;
    if body.len() < SALT_LEN + NONCE_LEN + TAG_LEN {
        return Err(Error::Tampered("truncated"));
    }
    let (salt, rest) = body.split_at(SALT_LEN);
    let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
    let key = derive_key(pin, salt)?;
    ChaCha20Poly1305::new((&key).into())
        .decrypt(nonce.into(), ciphertext)
        .map_err(|_| Error::WrongPin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base45_round_trips_and_rejects_garbage() {
        for len in [0usize, 1, 2, 3, 63, 64, 400] {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 % 256) as u8).collect();
            assert_eq!(base45_decode(&base45_encode(&data)).unwrap(), data, "len {len}");
        }
        // RFC 9285 examples.
        assert_eq!(base45_encode(b"AB"), "BB8");
        assert_eq!(base45_encode(b"Hello!!"), "%69 VD92EX0");
        assert!(matches!(base45_decode("ab#"), Err(Error::Tampered(_))));
        assert!(matches!(base45_decode("A"), Err(Error::Tampered(_))));
        assert!(matches!(base45_decode(":::"), Err(Error::Tampered(_))));
    }

    #[test]
    fn decompression_bomb_is_rejected() {
        let bomb = compress(&vec![0u8; 2 * 1024 * 1024]).unwrap();
        assert!(matches!(decompress(&bomb), Err(Error::Tampered(_))));
    }

    #[test]
    fn pins_normalise() {
        assert_eq!(normalize_pin("7k3m 9qx2").as_deref(), Some("7K3M-9QX2"));
        assert_eq!(normalize_pin("7K3M9QX2").as_deref(), Some("7K3M-9QX2"));
        assert_eq!(normalize_pin("7K3M-9QXO").as_deref(), Some("7K3M-9QX0"));
        assert_eq!(normalize_pin("483920"), None);
        for _ in 0..20 {
            let pin = generate_pin();
            assert_eq!(normalize_pin(&pin).as_deref(), Some(pin.as_str()));
        }
    }

    #[test]
    fn formats_are_classified() {
        assert!(needs_pin(" OCTOS1E:ABC"));
        assert!(!needs_pin("OCTOS1:ABC"));
        assert_eq!(format_of("{\"llm_family\":\"zai\"}"), Some(Format::LegacyJson));
        assert_eq!(format_of("WIFI:T:WPA;S:x;;"), None);
        assert!(matches!(decode("WIFI:T:WPA;S:x;;", None), Err(Error::Unsupported(_))));
        assert!(matches!(decode("OCTOS1E:AAA", None), Err(Error::PinRequired)));
        let long = format!("OCTOS1:{}", "A".repeat(MAX_INPUT_LEN));
        assert!(matches!(decode(&long, None), Err(Error::Invalid(_))));
    }
}
