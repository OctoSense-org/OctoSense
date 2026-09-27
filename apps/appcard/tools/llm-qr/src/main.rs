//! Encode an LLM provider set into a QR code that OctoSense scans to
//! provision itself — so a user brings their own keys without them ever
//! touching the repo, a keyboard, or the network.
//!
//! Default payload: octos's PIN-wrapped profile QR (`OCTOS1E:…`, see the
//! `octosense-llm-config` crate), carrying a primary provider, optional
//! fallbacks and their keys. The PIN is printed BESIDE the QR, never inside
//! it: a photographed code alone is useless.
//!
//! ```text
//! cargo run --manifest-path tools/llm-qr/Cargo.toml -- \
//!     --family deepseek --model deepseek-chat --prompt-key \
//!     --fallback zai:glm-4.6 --secret ZAI_API_KEY=… --out qr.png --svg qr.svg
//! ```
//!
//! `--plain` emits `OCTOS1:` (refused with keys unless `--allow-plain-secrets`),
//! `--legacy` / `--json` the old a2app JSON (`{"llm_family",…,"llm_key"}`), and
//! `--decode <text> [--pin …]` prints what a code provisions (never its keys).
//! Without `--out`/`--svg` a Unicode QR is printed to the terminal; prefer an
//! image for camera scanning. Every generated code contains keys — treat it
//! like a password.

use octosense_llm_config::qr::{self, Provisioning};
use octosense_llm_config::{registry, ApiType, Provider, ProviderSet};
use qrcode::render::unicode;
use qrcode::{EcLevel, QrCode};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::process::exit;

#[derive(Default)]
struct Args {
    family: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
    api_type: Option<String>,
    key_env: Option<String>,
    key: Option<String>,
    prompt_key: bool,
    fallbacks: Vec<String>,
    secrets: Vec<String>,
    pin: Option<String>,
    plain: bool,
    allow_plain_secrets: bool,
    legacy: bool,
    json: Option<String>,
    decode: Option<String>,
    out: Option<String>,
    svg: Option<String>,
}

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    exit(1);
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        // Accept `--flag value` and `--flag=value`.
        let (flag, inline) = match flag.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (flag, None),
        };
        let mut val = || {
            inline
                .clone()
                .or_else(|| it.next())
                .unwrap_or_else(|| die(&format!("{flag} needs a value")))
        };
        match flag.as_str() {
            "--family" => a.family = Some(val()),
            "--model" => a.model = Some(val()),
            "--base-url" => a.base_url = Some(val()),
            "--api-type" => a.api_type = Some(val()),
            "--key-env" => a.key_env = Some(val()),
            "--key" => a.key = Some(val()),
            "--prompt-key" => a.prompt_key = true,
            "--fallback" => a.fallbacks.push(val()),
            "--secret" => a.secrets.push(val()),
            "--pin" => a.pin = Some(val()),
            "--plain" => a.plain = true,
            "--allow-plain-secrets" => a.allow_plain_secrets = true,
            "--legacy" => a.legacy = true,
            "--json" => a.json = Some(val()),
            "--decode" => a.decode = Some(val()),
            "--out" => a.out = Some(val()),
            "--svg" => a.svg = Some(val()),
            "-h" | "--help" => {
                print_help();
                exit(0);
            }
            other => die(&format!("unknown argument '{other}' (try --help)")),
        }
    }
    a
}

fn print_help() {
    println!(
        "Encode an LLM provider set as a provisioning QR.\n\n\
         Provider set:\n  \
         --family <id>          primary provider family (deepseek, zai, openai, …)\n  \
         --model <id>           primary model (default: the family default)\n  \
         --base-url <url>       custom endpoint for the primary\n  \
         --api-type <t>         openai | anthropic | responses\n  \
         --key-env <NAME>       env var for the primary key (default: registry)\n  \
         --key <key>            the primary provider's key\n  \
         --prompt-key           prompt for the primary key (no shell history)\n  \
         --fallback <fam[:model]>  add a fallback (repeatable)\n  \
         --secret <ENV=value>   add a key by env var name (repeatable)\n\n\
         Format (default: OCTOS1E with a generated PIN):\n  \
         --pin <XXXX-XXXX>      use this PIN instead of generating one\n  \
         --plain                plain OCTOS1 (refused with keys unless --allow-plain-secrets)\n  \
         --legacy               legacy a2app JSON (primary only)\n  \
         --json <json>          encode this legacy JSON payload verbatim\n  \
         --decode <text>        print what a code provisions (with --pin for OCTOS1E)\n\n\
         Output:\n  \
         --out <path.png>       write a PNG\n  \
         --svg <path.svg>       write an SVG\n  \
         -h, --help             show this help"
    );
}

fn parse_api_type(s: &str) -> Result<ApiType, String> {
    ApiType::parse(s)
        .ok_or_else(|| format!("--api-type must be openai, anthropic or responses (got '{s}')"))
}

fn provisioning(a: &Args) -> Result<Provisioning, String> {
    let family = a.family.as_deref().ok_or("--family is required")?;
    let mut primary = Provider::new(family, a.model.clone());
    primary.base_url = a.base_url.clone();
    primary.api_type = a.api_type.as_deref().map(parse_api_type).transpose()?;
    if let Some(env) = &a.key_env {
        primary.key_env = env.clone();
    }
    let mut secrets = BTreeMap::new();
    if let Some(key) = &a.key {
        secrets.insert(primary.key_env.clone(), key.clone());
    }
    let mut fallbacks = Vec::new();
    for fb in &a.fallbacks {
        let (fam, model) = match fb.split_once(':') {
            Some((f, m)) => (f, Some(m.to_string())),
            None => (fb.as_str(), None),
        };
        fallbacks.push(Provider::new(fam, model));
    }
    for s in &a.secrets {
        let (name, value) = s.split_once('=').ok_or("--secret wants ENV=value")?;
        secrets.insert(name.to_string(), value.to_string());
    }
    let set = ProviderSet {
        primary: Some(primary),
        fallbacks,
    };
    for p in set.iter() {
        if registry::lookup(&p.family).is_none() {
            eprintln!(
                "note: '{}' is not an octos registry family; its key env is {}",
                p.family, p.key_env
            );
        } else if registry::lookup(&p.family).is_some_and(|f| f.key_required)
            && !secrets.contains_key(&p.key_env)
        {
            eprintln!(
                "note: no key for {} ({}) — the device must already have it",
                p.family, p.key_env
            );
        }
    }
    Ok(Provisioning { set, secrets })
}

/// The legacy a2app JSON (compact — QR capacity is limited).
fn legacy_payload(a: &Args) -> Result<String, String> {
    let value = if let Some(json) = &a.json {
        serde_json::from_str::<Value>(json).map_err(|e| format!("--json is not valid JSON: {e}"))?
    } else {
        let (Some(family), Some(key)) = (&a.family, &a.key) else {
            return Err("--family and --key are required (or pass --json)".into());
        };
        let mut m = Map::new();
        m.insert("llm_family".into(), Value::from(family.clone()));
        if let Some(model) = &a.model {
            m.insert("llm_model".into(), Value::from(model.clone()));
        }
        m.insert("llm_key".into(), Value::from(key.clone()));
        if let Some(url) = &a.base_url {
            m.insert("llm_base_url".into(), Value::from(url.clone()));
        }
        if let Some(t) = &a.api_type {
            m.insert("llm_api_type".into(), Value::from(t.clone()));
        }
        Value::Object(m)
    };
    let payload = serde_json::to_string(&value).map_err(|e| format!("serialize payload: {e}"))?;
    // Validate with exactly the parser the device uses.
    qr::decode(&payload, None).map_err(|e| e.to_string())?;
    Ok(payload)
}

/// Encode per the flags; returns `(payload, pin)`.
fn encode(a: &Args) -> Result<(String, Option<String>), String> {
    if a.legacy || a.json.is_some() {
        return Ok((legacy_payload(a)?, None));
    }
    let p = provisioning(a)?;
    if a.plain {
        let text = qr::encode_plain(&p, a.allow_plain_secrets).map_err(|e| e.to_string())?;
        return Ok((text, None));
    }
    let pin = a.pin.clone().unwrap_or_else(qr::generate_pin);
    let text = qr::encode_encrypted(&p, &pin).map_err(|e| e.to_string())?;
    Ok((text, Some(pin)))
}

fn write_outputs(payload: &str, a: &Args) -> Result<(), String> {
    if let Some(path) = &a.out {
        let png = qr::render_png(payload, 8).map_err(|e| e.to_string())?;
        std::fs::write(path, png).map_err(|e| format!("write {path}: {e}"))?;
        println!("Wrote {path}");
    }
    if let Some(path) = &a.svg {
        let svg = qr::render_svg(payload).map_err(|e| e.to_string())?;
        std::fs::write(path, svg).map_err(|e| format!("write {path}: {e}"))?;
        println!("Wrote {path}");
    }
    if a.out.is_none() && a.svg.is_none() {
        // Dense1x2 packs two module rows per line; swapping the colors inverts
        // it for a typical dark terminal. Some fonts distort this rendering,
        // which is why camera provisioning should prefer an image.
        let code = QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::M)
            .map_err(|e| format!("could not encode QR (payload too long?): {e}"))?;
        let art = code
            .render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Light)
            .light_color(unicode::Dense1x2::Dark)
            .quiet_zone(true)
            .build();
        println!("{art}");
    }
    Ok(())
}

fn main() {
    let mut a = parse_args();
    if let Some(text) = &a.decode {
        match qr::decode(text, a.pin.as_deref()) {
            Ok(p) => {
                for (i, prov) in p.set.iter().enumerate() {
                    let role = if i == 0 && p.set.primary.is_some() {
                        "primary "
                    } else {
                        "fallback"
                    };
                    println!("{role} {} (key env {})", prov.label(), prov.key_env);
                }
                println!(
                    "keys for: {}",
                    p.secrets.keys().cloned().collect::<Vec<_>>().join(", ")
                );
            }
            Err(e) => die(&e.to_string()),
        }
        return;
    }
    if a.prompt_key {
        if a.key.is_some() || a.json.is_some() {
            die("--prompt-key cannot be combined with --key or --json");
        }
        a.key = Some(
            rpassword::prompt_password("API key: ")
                .unwrap_or_else(|e| die(&format!("could not read API key: {e}"))),
        );
    }
    let (payload, pin) = encode(&a).unwrap_or_else(|e| die(&e));
    match &pin {
        Some(pin) => {
            println!("{payload}\n");
            println!("PIN: {pin}   (tell it separately — never print it next to the code)");
            match qr::qr_version(&payload) {
                Ok(v) => println!("QR version {v} (ECC M), {} chars", payload.len()),
                Err(e) => die(&e.to_string()),
            }
        }
        // A plain or legacy payload with keys IS the keys: do not echo it.
        None if payload.starts_with('{') || !a.secrets.is_empty() || a.key.is_some() => {
            println!("QR payload generated (contains keys; not printed).")
        }
        None => println!("{payload}"),
    }
    write_outputs(&payload, &a).unwrap_or_else(|e| die(&e));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(f: impl FnOnce(&mut Args)) -> Args {
        let mut a = Args::default();
        f(&mut a);
        a
    }

    #[test]
    fn legacy_payload_contains_only_llm_data() {
        let payload = legacy_payload(&args(|a| {
            a.family = Some("zai".into());
            a.model = Some("glm-5.2".into());
            a.key = Some("sk-test".into());
        }))
        .unwrap();
        let value: Value = serde_json::from_str(&payload).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), 3);
        assert!(object.contains_key("llm_family"));
        assert!(object.contains_key("llm_model"));
        assert!(object.contains_key("llm_key"));
    }

    #[test]
    fn legacy_json_payload_rejects_server_url() {
        let result = legacy_payload(&args(|a| {
            a.json = Some(
                r#"{"llm_family":"zai","llm_key":"sk-test","base_url":"https://example.com"}"#
                    .into(),
            );
        }));
        assert!(result.unwrap_err().contains("unknown field `base_url`"));
    }

    #[test]
    fn encrypted_payload_round_trips_with_fallback_and_keys() {
        let a = args(|a| {
            a.family = Some("deepseek".into());
            a.model = Some("deepseek-chat".into());
            a.key = Some("sk-test-0000000000000000".into());
            a.fallbacks = vec!["zai:glm-4.6".into()];
            a.secrets = vec!["ZAI_API_KEY=zai-test-0000".into()];
            a.pin = Some("7K3M-9QX2".into());
        });
        let (text, pin) = encode(&a).unwrap();
        assert!(text.starts_with("OCTOS1E:"));
        let p = qr::decode(&text, pin.as_deref()).unwrap();
        assert_eq!(p.set.fallbacks[0].label(), "zai/glm-4.6");
        assert_eq!(p.secrets["DEEPSEEK_API_KEY"], "sk-test-0000000000000000");
        assert_eq!(p.secrets["ZAI_API_KEY"], "zai-test-0000");
    }

    #[test]
    fn plain_refuses_keys_by_default() {
        let a = args(|a| {
            a.family = Some("deepseek".into());
            a.key = Some("sk-test".into());
            a.plain = true;
        });
        assert!(encode(&a).is_err());
    }
}
