//! The provisioning QR end to end: encode/decode, octos byte-compatibility,
//! capacity, the phone decoder (rqrr), legacy JSON, and applying a scan.
//!
//! Fixtures (`tests/fixtures/`, all keys FAKE):
//! - `qr-a.txt` / `qr-a.png` — "QR-A": OCTOS1E, PIN `7K3M-9QX2`, primary
//!   deepseek/deepseek-chat, fallback zai/glm-4.6, produced by tools/llm-qr.
//! - `octos-*.txt` — produced by octos's own `profile_qr.rs` at the pinned
//!   octos commit (encrypted: PIN `7K3M-9QX2`; server: PIN `483920`).
//! - `legacy-*.json` — the a2app JSON payload.

use octosense_llm_config::qr::{self, Provisioning};
use octosense_llm_config::{apply_to_profile, ApiType, Error, Provider, ProviderSet};
use std::collections::BTreeMap;
use std::path::PathBuf;

const QR_A_PIN: &str = "7K3M-9QX2";

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Deterministic key-shaped filler over `alphabet` — random enough not to
/// flatter the compressor, so the capacity test is honest.
fn fake_key(prefix: &str, len: usize, seed: u32, alphabet: &[u8]) -> String {
    let mut x = seed.wrapping_mul(2654435761).wrapping_add(1);
    let mut s = prefix.to_string();
    while s.len() < len {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        s.push(alphabet[x as usize % alphabet.len()] as char);
    }
    s
}

const HEX: &[u8] = b"0123456789abcdef";
const B62: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Three providers with keys shaped like the real ones: DeepSeek `sk-` + 32
/// hex, Z.ai 32 hex `.` 16 base62, Kimi `sk-` + 48 base62. (An Anthropic key,
/// `sk-ant-api03-` + 95 chars, is the largest common key: in place of Kimi's
/// it pushes the code to version 16.)
fn three_providers() -> Provisioning {
    let fb2 = Provider::new("moonshot", Some("kimi-k2.5".into()));
    Provisioning {
        set: ProviderSet {
            primary: Some(Provider::new("deepseek", Some("deepseek-chat".into()))),
            fallbacks: vec![Provider::new("zai", Some("glm-4.6".into())), fb2],
        },
        secrets: BTreeMap::from([
            ("DEEPSEEK_API_KEY".into(), fake_key("sk-", 35, 1, HEX)),
            ("ZAI_API_KEY".into(), format!("{}.{}", fake_key("", 32, 2, HEX), fake_key("", 16, 3, B62))),
            ("MOONSHOT_API_KEY".into(), fake_key("sk-", 51, 4, B62)),
        ]),
    }
}

fn qr_a_expected() -> Provisioning {
    Provisioning {
        set: ProviderSet {
            primary: Some(Provider::new("deepseek", Some("deepseek-chat".into()))),
            fallbacks: vec![Provider::new("zai", Some("glm-4.6".into()))],
        },
        secrets: BTreeMap::from([
            ("DEEPSEEK_API_KEY".into(), "sk-test-0000000000000000".into()),
            ("ZAI_API_KEY".into(), "zai-test-0000".into()),
        ]),
    }
}

fn assert_secret_free(err: &Error, p: &Provisioning) {
    let shown = format!("{err} / {err:?}");
    for v in p.secrets.values() {
        assert!(!shown.contains(v.as_str()), "error leaks a key: {shown}");
    }
}

#[test]
fn encrypted_round_trip_keeps_keys() {
    let p = three_providers();
    let pin = qr::generate_pin();
    let text = qr::encode_encrypted(&p, &pin).unwrap();
    assert!(text.starts_with("OCTOS1E:") && qr::needs_pin(&text));
    assert_eq!(qr::decode(&text, Some(&pin)).unwrap(), p);
    // The PIN is typed on a phone: case and the hyphen do not matter.
    let sloppy = pin.replace('-', "").to_lowercase();
    assert_eq!(qr::decode(&text, Some(&sloppy)).unwrap(), p);
}

#[test]
fn wrong_pin_and_missing_pin_are_distinct_and_secret_free() {
    let p = three_providers();
    let text = qr::encode_encrypted(&p, "7K3M-9QX2").unwrap();
    let wrong = qr::decode(&text, Some("7K3M-9QX3")).unwrap_err();
    assert!(matches!(wrong, Error::WrongPin), "{wrong:?}");
    assert_secret_free(&wrong, &p);
    let missing = qr::decode(&text, None).unwrap_err();
    assert!(matches!(missing, Error::PinRequired), "{missing:?}");
    // Debug of a provisioning never prints key values either.
    let dbg = format!("{p:?}");
    for v in p.secrets.values() {
        assert!(!dbg.contains(v.as_str()));
    }
}

#[test]
fn tampered_codes_are_rejected() {
    let p = three_providers();
    let text = qr::encode_encrypted(&p, QR_A_PIN).unwrap();
    let body = &text["OCTOS1E:".len()..];
    // Truncate to a length of 3k + 1 (invalid base45). The cut must not end in
    // a space: space is a base45 digit, `decode` trims the input, and a body
    // trimmed back to 3k is a valid prefix that only fails authentication
    // (WrongPin). The first digit of a triple is a space ~1 time in 45, so
    // with a random salt and nonce a fixed cut made this test flaky.
    let mut cut = (body.len() - 3) / 3 * 3 + 1;
    while body.as_bytes()[cut - 1] == b' ' {
        cut -= 3;
    }

    // Structural damage: a character outside the QR alphanumeric set, a
    // truncated body, a triple beyond 0xFFFF.
    for bad in [
        format!("OCTOS1E:{}a{}", &body[..10], &body[11..]),
        format!("OCTOS1E:{}", &body[..cut]),
        "OCTOS1E:000".to_string(),
        format!("OCTOS1E::::{}", &body[3..]),
        "OCTOS1E:0000".to_string(),
    ] {
        let err = qr::decode(&bad, Some(QR_A_PIN)).unwrap_err();
        assert!(matches!(err, Error::Tampered(_)), "{err:?}");
        assert_secret_free(&err, &p);
    }

    // A valid-alphabet substitution inside the ciphertext fails
    // authentication — indistinguishable from a wrong PIN by design (AEAD).
    let mut chars: Vec<char> = text.chars().collect();
    let i = chars.len() - 10;
    chars[i] = if chars[i] == '0' { '1' } else { '0' };
    let flipped: String = chars.into_iter().collect();
    let err = qr::decode(&flipped, Some(QR_A_PIN)).unwrap_err();
    assert!(matches!(err, Error::WrongPin | Error::Tampered(_)), "{err:?}");
    assert_secret_free(&err, &p);
}

#[test]
fn plain_refuses_keys_unless_allowed() {
    let p = three_providers();
    let err = qr::encode_plain(&p, false).unwrap_err();
    assert!(matches!(err, Error::SecretsNeedPin));
    assert_secret_free(&err, &p);

    let text = qr::encode_plain(&p, true).unwrap();
    assert!(text.starts_with("OCTOS1:") && !qr::needs_pin(&text));
    assert_eq!(qr::decode(&text, None).unwrap(), p);

    let keyless = Provisioning { set: p.set.clone(), secrets: BTreeMap::new() };
    let text = qr::encode_plain(&keyless, false).unwrap();
    assert_eq!(qr::decode(&text, None).unwrap(), keyless);
}

#[test]
fn qr_a_fixture_decodes() {
    let text = fixture("qr-a.txt");
    assert!(qr::needs_pin(&text));
    assert_eq!(qr::decode(&text, Some(QR_A_PIN)).unwrap(), qr_a_expected());
    assert!(matches!(qr::decode(&text, Some("0000-0000")), Err(Error::WrongPin)));
}

#[test]
fn octos_produced_vectors_decode() {
    // Encrypted by octos: extra selection fields (context_window) and
    // payload blocks (name, memory) are ignored, not errors.
    let p = qr::decode(&fixture("octos-encrypted.txt"), Some(QR_A_PIN)).unwrap();
    let mut kimi = Provider::new("moonshot-coding", Some("k3".into()));
    kimi.api_type = Some(ApiType::Anthropic);
    assert_eq!(p.set.primary, Some(Provider::new("deepseek", Some("deepseek-chat".into()))));
    assert_eq!(p.set.fallbacks, vec![kimi]);
    assert_eq!(p.secrets["DEEPSEEK_API_KEY"], "sk-test-octos-vector-0001");
    assert_eq!(p.secrets["KIMI_CODING_API_KEY"], "sk-kimi-test-octos-0002");

    let plain = qr::decode(&fixture("octos-plain.txt"), None).unwrap();
    assert_eq!(plain.set, p.set);
    assert!(plain.secrets.is_empty());

    // A server-profile code (endpoint + auth token) is not accepted here.
    let err = qr::decode(&fixture("octos-server.txt"), Some("483920")).unwrap_err();
    assert!(matches!(err, Error::Unsupported(_)), "{err:?}");
    assert!(!err.to_string().contains("octs_test_token"));
}

#[test]
fn three_providers_fit_qr_version_15_at_ecc_m() {
    let text = qr::encode_encrypted(&three_providers(), QR_A_PIN).unwrap();
    let v = qr::qr_version(&text).unwrap();
    eprintln!("three providers + keys: {} chars, QR version {v}", text.len());
    assert!(v <= 15, "QR version {v} for {} chars", text.len());
    // qrcode picked alphanumeric mode (base45 is the QR alphanumeric set).
    assert!(text.bytes().all(|b| b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:".contains(&b)));
}

fn rqrr_read(side_w: usize, side_h: usize, luma: impl Fn(usize, usize) -> u8) -> String {
    let mut img = rqrr::PreparedImage::prepare_from_greyscale(side_w, side_h, luma);
    img.detect_grids()
        .into_iter()
        .find_map(|g| g.decode().ok().map(|(_, content)| content))
        .expect("rqrr (the phone decoder) finds the code")
}

#[test]
fn qr_a_png_reads_back_through_rqrr() {
    let bytes = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/qr-a.png")).unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Grayscale);
    let (w, h) = (info.width as usize, info.height as usize);
    let decoded = rqrr_read(w, h, |x, y| buf[y * w + x]);
    assert_eq!(decoded, fixture("qr-a.txt"));
}

#[test]
fn rendered_matrix_reads_back_through_rqrr() {
    let text = qr::encode_encrypted(&three_providers(), QR_A_PIN).unwrap();
    let (width, dark) = qr::render_matrix(&text).unwrap();
    assert_eq!(dark.len(), width * width);
    let (quiet, scale) = (4usize, 6usize);
    let side = (width + 2 * quiet) * scale;
    let decoded = rqrr_read(side, side, |x, y| {
        let (mx, my) = (x / scale, y / scale);
        let inside = (quiet..quiet + width).contains(&mx) && (quiet..quiet + width).contains(&my);
        if inside && dark[(my - quiet) * width + (mx - quiet)] { 0 } else { 255 }
    });
    assert_eq!(decoded, text);

    let svg = qr::render_svg(&text).unwrap();
    assert!(svg.starts_with("<?xml") && svg.contains("<svg"));
}

#[test]
fn legacy_json_fixtures_decode() {
    let kimi = qr::decode(&fixture("legacy-moonshot-coding.json"), None).unwrap();
    assert_eq!(kimi.set.primary, Some(Provider::new("moonshot-coding", Some("k3".into()))));
    assert!(kimi.set.fallbacks.is_empty());
    assert_eq!(kimi.secrets, BTreeMap::from([("KIMI_CODING_API_KEY".into(), "sk-kimi-TEST".into())]));

    let zai = qr::decode(&fixture("legacy-zai.json"), None).unwrap();
    assert_eq!(zai.secrets["ZAI_API_KEY"], "sk-fake-test");

    let custom = qr::decode(&fixture("legacy-custom-endpoint.json"), None).unwrap();
    let primary = custom.set.primary.unwrap();
    assert_eq!(primary.base_url.as_deref(), Some("http://127.0.0.1:30880/v1"));
    assert_eq!(primary.api_type, Some(ApiType::OpenAi));
    assert_eq!(custom.secrets["OPENAI_API_KEY"], "local-test");

    // Aliases are stored under their canonical family.
    let alias = qr::decode(r#"{"llm_family":"kimi-coding","llm_key":"k"}"#, None).unwrap();
    assert_eq!(alias.set.primary.unwrap().family, "moonshot-coding");
}

#[test]
fn legacy_json_is_strict() {
    for bad in [
        r#"{"llm_family":"zai","llm_key":"sk-test","base_url":"https://example.com"}"#,
        r#"{"llm_family":"","llm_key":"sk-test"}"#,
        r#"{"llm_family":"zai","llm_key":"  "}"#,
        r#"{"llm_family":"zai","llm_key":"sk-test","llm_model":""}"#,
        r#"{"llm_family":"openai","llm_key":"sk-test","llm_api_type":"grpc"}"#,
        r#"{"llm_family":"openai","llm_key":"sk-test","llm_base_url":"file:///tmp/model"}"#,
        r#"{"llm_family":"openai","llm_key":"sk-test","llm_base_url":"http://u:p@localhost/v1"}"#,
        r#"{"llm_family":"openai","llm_key":"sk-test","llm_base_url":"http://localhost/v1?key=x"}"#,
        r#"{"llm_family":"zai","llm_key":42}"#,
    ] {
        let err = qr::decode(bad, None).unwrap_err();
        assert!(matches!(err, Error::Invalid(_)), "{bad}: {err:?}");
        assert!(!err.to_string().contains("sk-test"), "{err}");
    }
    let err = qr::decode(r#"{"llm_family":"zai","llm_key":"sk-test","base_url":"https://example.com"}"#, None).unwrap_err();
    assert!(err.to_string().contains("unknown field `base_url`"), "{err}");
}

#[test]
fn hostile_payloads_are_refused() {
    let base = qr_a_expected();

    // A QR must not set arbitrary environment variables.
    let mut env = base.clone();
    env.secrets.insert("LD_PRELOAD".into(), "/tmp/evil.so".into());
    assert!(matches!(qr::encode_plain(&env, true), Err(Error::Invalid(_))));

    // Nor point a key already on the device at its own endpoint.
    let mut exfil = base.clone();
    exfil.set.fallbacks.push(Provider { base_url: Some("https://evil.example/v1".into()), ..Provider::new("openai", None) });
    assert!(matches!(qr::encode_plain(&exfil, true), Err(Error::Invalid(_))));
    // …unless the code carries that endpoint's key itself.
    exfil.secrets.insert("OPENAI_API_KEY".into(), "sk-own".into());
    assert!(qr::encode_plain(&exfil, true).is_ok());

    // Keyless families may use a custom endpoint without a key.
    let local = Provisioning {
        set: ProviderSet { primary: Some(Provider { base_url: Some("http://192.168.1.5:8080/v1".into()), ..Provider::new("local", None) }), fallbacks: vec![] },
        secrets: BTreeMap::new(),
    };
    assert_eq!(qr::decode(&qr::encode_plain(&local, false).unwrap(), None).unwrap(), local);
}

#[test]
fn apply_to_profile_merges_and_summarises_without_keys() {
    let dir = std::env::temp_dir().join(format!("llm-config-apply-{}", std::process::id()));
    let path = octosense_llm_config::profile::profile_path(&dir);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, br#"{"id":"_main","name":"Mine","created_at":"2026-01-01T00:00:00Z","config":{"custom":"keep"}}"#).unwrap();

    let p = qr::decode(&fixture("qr-a.txt"), Some(QR_A_PIN)).unwrap();
    let summary = apply_to_profile(&path, &p).unwrap();
    let shown = format!("{summary} {summary:?}");
    assert_eq!(summary.to_string(), "deepseek/deepseek-chat → zai/glm-4.6 [keys: DEEPSEEK_API_KEY, ZAI_API_KEY]");
    for v in p.secrets.values() {
        assert!(!shown.contains(v.as_str()));
    }

    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(v["name"], "Mine");
    assert_eq!(v["created_at"], "2026-01-01T00:00:00Z");
    assert_eq!(v["enabled"], true);
    assert_eq!(v["config"]["custom"], "keep");
    assert_eq!(v["config"]["llm"]["primary"]["family_id"], "deepseek");
    assert_eq!(v["config"]["llm"]["fallbacks"][0]["model_id"], "glm-4.6");
    assert_eq!(v["config"]["env_vars"]["ZAI_API_KEY"], "zai-test-0000");
    let loaded = octosense_llm_config::profile::load(&path).unwrap();
    assert_eq!(loaded.set, p.set);
    std::fs::remove_dir_all(dir).unwrap();
}
