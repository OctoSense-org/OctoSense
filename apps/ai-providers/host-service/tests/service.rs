//! The `llm` service through App Hub's real dispatch path: a temp core dir,
//! an in-memory vault, the sheet guard, and the QR the export sheet draws
//! read back the way the phone reads it (rqrr).
use octosense_appstore::services::{dispatch, take_replies_for, ServiceCall, ServiceHost};
use octosense_llm_config::{profile, qr, Provider};
use octosense_llm_service::vault::{MemoryVault, Vault};
use octosense_llm_service::{offer_image, register_with, sheets, wants_image, ImageDone, Options, PickError, QrImagePicker, QrScanner, ScanDone};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// One service is registered at a time (the registry is global).
static SERIAL: Mutex<()> = Mutex::new(());
static NEXT: AtomicUsize = AtomicUsize::new(31_000);

const APP: &str = "os.ai-providers";
const DEEPSEEK_KEY: &str = "sk-test-deepseek-0000aaaa1234";
const ZAI_KEY: &str = "sk-test-zai-0000bbbb5678";

#[derive(Default)]
struct Host {
    sheet: Option<Option<String>>,
}

impl ServiceHost for Host {
    fn open_sheet(&mut self, body: String) {
        self.sheet = Some(Some(body));
    }
    fn close_sheet(&mut self) {
        self.sheet = Some(None);
    }
}

impl Host {
    fn body(&self) -> &str {
        self.sheet.as_ref().and_then(|s| s.as_deref()).expect("a sheet is up")
    }
}

struct Rig {
    dir: PathBuf,
    vault: Arc<MemoryVault>,
    changed: Arc<AtomicUsize>,
    host: Host,
    _serial: std::sync::MutexGuard<'static, ()>,
}

impl Rig {
    fn new(tag: &str, scanner: Option<Arc<dyn QrScanner>>) -> Rig {
        Rig::with(tag, |options| match scanner {
            Some(scanner) => options.scanner(scanner),
            None => options,
        })
    }

    fn with(tag: &str, extra: impl FnOnce(Options) -> Options) -> Rig {
        let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("llm-service-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let vault = Arc::new(MemoryVault::default());
        let changed = Arc::new(AtomicUsize::new(0));
        let counter = changed.clone();
        let options = Options::default().core_dir(&dir).vault(vault.clone()).on_changed(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        register_with(extra(options));
        Rig { dir, vault, changed, host: Host::default(), _serial: serial }
    }

    fn send(&mut self, app: &str, service: &str, args: Value, from_sheet: bool) -> usize {
        let heap = NEXT.fetch_add(1, Ordering::Relaxed);
        let call = ServiceCall { app_id: app.into(), service: service.into(), args, from_sheet, host_dir: self.dir.clone() };
        dispatch(call, heap, 1, &mut self.host);
        heap
    }

    fn ask(&mut self, service: &str, args: Value) -> Result<Value, String> {
        let heap = self.send(APP, service, args, false);
        wait(heap)
    }

    fn sheet(&mut self, service: &str, args: Value) -> Result<Value, String> {
        let heap = self.send(APP, service, args, true);
        wait(heap)
    }

    fn profile_text(&self) -> String {
        std::fs::read_to_string(profile::profile_path(&self.dir)).unwrap_or_default()
    }

    /// Add a provider on the sheet, as a person would.
    fn add(&mut self, form: Value) -> Value {
        let waiting = self.send(APP, "llm.add_provider", Value::Null, false);
        assert!(self.host.body().contains("is_password: true"));
        self.sheet("llm.sheet.submit", form).unwrap();
        wait(waiting).unwrap()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn wait(heap: usize) -> Result<Value, String> {
    for _ in 0..2000 {
        if let Some((_, _, result)) = take_replies_for(&[heap]).pop() {
            return result.map(|s| serde_json::from_str(&s).unwrap());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("no answer on {heap}");
}

fn still_waiting(heap: usize) -> bool {
    std::thread::sleep(std::time::Duration::from_millis(50));
    take_replies_for(&[heap]).is_empty()
}

fn all(providers: &Value) -> Vec<Value> {
    providers["primary"].as_object().map(|_| providers["primary"].clone()).into_iter().chain(providers["fallbacks"].as_array().cloned().unwrap_or_default()).collect()
}

fn two_providers(rig: &mut Rig) {
    rig.add(json!({"family": "deepseek", "model": "deepseek-chat", "base_url": "", "api_type": "default", "key": DEEPSEEK_KEY}));
    rig.add(json!({"family": "zai", "model": "", "base_url": "", "api_type": "default", "key": ZAI_KEY}));
}

#[test]
fn keys_are_typed_only_on_the_sheet_and_never_come_back() {
    let mut rig = Rig::new("edit", None);

    // The app cannot hand the service a key itself.
    let refused = rig.ask("llm.sheet.submit", json!({"family": "deepseek", "key": DEEPSEEK_KEY})).unwrap_err();
    assert!(refused.contains("for the host's sheet"), "{refused}");
    // A store app is not served at all.
    let heap = rig.send("com.example.app", "llm.providers", Value::Null, false);
    assert!(wait(heap).unwrap_err().contains("OctoSense's own apps"));

    let families = rig.ask("llm.families", Value::Null).unwrap();
    assert!(families.as_array().unwrap().iter().any(|f| f["id"] == "deepseek" && f["key_required"] == true));

    // add_provider waits on the sheet; a missing key keeps it waiting.
    let waiting = rig.send(APP, "llm.add_provider", Value::Null, false);
    let err = rig.sheet("llm.sheet.submit", json!({"family": "deepseek", "model": "", "base_url": "", "api_type": "default", "key": ""})).unwrap_err();
    assert!(err.contains("API key"), "{err}");
    assert!(still_waiting(waiting), "the app is still waiting");
    rig.sheet("llm.sheet.submit", json!({"family": "deepseek", "model": "deepseek-chat", "base_url": "", "api_type": "default", "key": DEEPSEEK_KEY}))
        .unwrap();
    let added = wait(waiting).unwrap();
    assert_eq!(added["label"], "DeepSeek · deepseek-chat");
    rig.add(json!({"family": "zai", "model": "", "base_url": "", "api_type": "default", "key": ZAI_KEY}));
    rig.add(json!({"family": "ollama", "model": "", "base_url": "", "api_type": "default", "key": ""}));
    assert_eq!(rig.changed.load(Ordering::SeqCst), 3, "every save tells the shell");

    let providers = rig.ask("llm.providers", Value::Null).unwrap();
    let text = providers.to_string();
    assert!(!text.contains(DEEPSEEK_KEY) && !text.contains("0000aaaa"), "no key in {text}");
    assert_eq!(providers["primary"]["key"], "set ••••1234");
    assert_eq!(providers["fallbacks"][0]["key"], "set ••••5678");
    assert_eq!(providers["fallbacks"][1]["key"], "not needed");
    assert_eq!(providers["fallbacks"][0]["model"], "glm-5-turbo");
    assert_eq!(providers["fallbacks"][0]["custom_model"], false);
    assert_eq!(providers["scanner"], false);
    // Keys are in the vault behind octos's marker, not in the profile.
    assert!(!rig.profile_text().contains("sk-test"), "the profile holds markers");
    assert!(rig.profile_text().contains("\"keychain:\""));
    assert_eq!(rig.vault.get("DEEPSEEK_API_KEY").unwrap().as_deref(), Some(DEEPSEEK_KEY));

    // Plain edits.
    let zai = providers["fallbacks"][0]["id"].as_str().unwrap().to_string();
    let renamed = rig.ask("llm.set_model", json!({"id": zai, "model": "glm-4.6"})).unwrap();
    let zai = renamed["id"].as_str().unwrap().to_string();
    rig.ask("llm.set_primary", json!({"id": zai})).unwrap();
    let providers = rig.ask("llm.providers", Value::Null).unwrap();
    assert_eq!(providers["primary"]["model"], "glm-4.6");
    assert_eq!(providers["primary"]["custom_model"], true);
    let ollama = providers["fallbacks"][1]["id"].as_str().unwrap().to_string();
    rig.ask("llm.move", json!({"id": ollama, "to": 1})).unwrap();
    let order: Vec<Value> = all(&rig.ask("llm.providers", Value::Null).unwrap()).iter().map(|p| p["family"].clone()).collect();
    assert_eq!(order, ["zai", "ollama", "deepseek"]);

    // Edit on the sheet: an empty key keeps the saved one.
    let deepseek = all(&rig.ask("llm.providers", Value::Null).unwrap())[2]["id"].as_str().unwrap().to_string();
    let waiting = rig.send(APP, "llm.edit_provider", json!({"id": deepseek}), false);
    assert!(rig.host.body().contains("let family = \"deepseek\""));
    rig.sheet("llm.sheet.submit", json!({"family": "deepseek", "model": "deepseek-reasoner", "base_url": "", "api_type": "openai", "key": ""}))
        .unwrap();
    let edited = wait(waiting).unwrap();
    let providers = all(&rig.ask("llm.providers", Value::Null).unwrap());
    assert_eq!(providers[2]["id"], edited["id"]);
    assert_eq!(providers[2]["api_type"], "openai");
    assert_eq!(providers[2]["key"], "set ••••1234");

    // Cancel answers the app; remove drops the key env var from the profile.
    let waiting = rig.send(APP, "llm.add_provider", Value::Null, false);
    rig.sheet("llm.sheet.cancel", json!({})).unwrap();
    assert_eq!(wait(waiting).unwrap_err(), "Cancelled.");
    assert_eq!(rig.host.sheet, Some(None));
    rig.ask("llm.remove", json!({"id": edited["id"]})).unwrap();
    assert!(!rig.profile_text().contains("DEEPSEEK_API_KEY"));
    assert_eq!(all(&rig.ask("llm.providers", Value::Null).unwrap()).len(), 2);
}

/// The QR rows of an export sheet, back into modules.
fn matrix_of(body: &str) -> (usize, Vec<Vec<bool>>) {
    let start = body.find("// qr-begin ").expect("a QR on the sheet");
    let px: usize = body[start + 12..].split_whitespace().next().unwrap().parse().unwrap();
    let end = body.find("// qr-end").unwrap();
    let mut rows = Vec::new();
    for line in body[start..end].lines().map(str::trim).filter(|l| l.starts_with("QrRow{")) {
        let height: usize = line["QrRow{height: ".len()..].split(' ').next().unwrap().trim_end_matches('}').parse().unwrap();
        let mut row = Vec::new();
        for span in line.split(' ').collect::<Vec<_>>().windows(2) {
            let dark = match span[0] {
                "Dark{width:" => true,
                "Light{width:" => false,
                _ => continue,
            };
            let width: usize = span[1].trim_end_matches('}').parse().unwrap();
            row.extend(std::iter::repeat_n(dark, width / px));
        }
        for _ in 0..height / px {
            rows.push(row.clone());
        }
    }
    let size = rows.len();
    for row in &mut rows {
        row.resize(size, false);
    }
    (size, rows)
}

/// What the phone reads off the screen: the matrix as a PNG, decoded by rqrr.
fn scan_png(size: usize, rows: &[Vec<bool>]) -> String {
    let (scale, quiet) = (6, 4);
    let side = (size + 2 * quiet) * scale;
    let mut pixels = vec![255u8; side * side];
    for (y, row) in rows.iter().enumerate() {
        for (x, &dark) in row.iter().enumerate() {
            if dark {
                for dy in 0..scale {
                    let at = ((y + quiet) * scale + dy) * side + (x + quiet) * scale;
                    pixels[at..at + scale].fill(0);
                }
            }
        }
    }
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, side as u32, side as u32);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let (w, h) = (info.width as usize, info.height as usize);
    let mut image = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| buf[y * w + x]);
    let grids = image.detect_grids();
    assert_eq!(grids.len(), 1, "one QR on the sheet");
    grids[0].decode().unwrap().1
}

fn pin_of(body: &str) -> String {
    let at = body.find("pin := Label{").unwrap();
    let rest = &body[at..];
    let start = rest.find("text: \"").unwrap() + 7;
    rest[start..start + rest[start..].find('"').unwrap()].to_string()
}

/// Export, and read the code and PIN off the sheet the person sees.
fn export(rig: &mut Rig, args: Value) -> (usize, String, String) {
    let waiting = rig.send(APP, "llm.export_qr", args, false);
    assert!(rig.host.body().contains("llm.sheet.qr"), "a waiting sheet first");
    // Not ready yet: the swap is refused.
    let mut ready = false;
    for _ in 0..400 {
        if rig.sheet("llm.sheet.qr", json!({})).unwrap()["ready"] == true {
            ready = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(ready, "the code was sealed");
    assert!(rig.host.body().contains("llm.sheet.qr"), "ready does not swap the sheet by itself");
    rig.sheet("llm.sheet.show", json!({})).unwrap();
    let body = rig.host.body().to_string();
    // Shown once: nothing is left to show again.
    assert!(rig.sheet("llm.sheet.show", json!({})).is_err());
    assert!(body.contains("// qr-begin"), "the QR sheet replaced the waiting one");
    let (size, rows) = matrix_of(&body);
    (waiting, scan_png(size, &rows), pin_of(&body))
}

#[test]
fn the_export_sheet_draws_a_qr_that_decodes_to_the_saved_set_with_its_keys() {
    let mut rig = Rig::new("export", None);
    two_providers(&mut rig);
    let (waiting, code, pin) = export(&mut rig, Value::Null);
    assert!(code.starts_with("OCTOS1E:"), "{}", &code[..12]);
    assert!(still_waiting(waiting), "the app hears only when the sheet closes");

    let decoded = qr::decode(&code, Some(&pin)).unwrap();
    let saved = profile::load(&profile::profile_path(&rig.dir)).unwrap().set;
    assert_eq!(decoded.set, saved);
    assert_eq!(decoded.secrets["DEEPSEEK_API_KEY"], DEEPSEEK_KEY);
    assert_eq!(decoded.secrets["ZAI_API_KEY"], ZAI_KEY);

    rig.sheet("llm.sheet.cancel", json!({})).unwrap();
    let closed = wait(waiting).unwrap();
    assert!(!closed.to_string().contains(&pin), "the app never gets the PIN");
    assert_eq!(closed, json!({}));

    // A chosen subset.
    let zai = rig.ask("llm.providers", Value::Null).unwrap()["fallbacks"][0]["id"].clone();
    let (waiting, code, pin) = export(&mut rig, json!({"ids": [zai]}));
    let decoded = qr::decode(&code, Some(&pin)).unwrap();
    assert_eq!(decoded.set.primary.unwrap().family, "zai");
    assert!(decoded.set.fallbacks.is_empty());
    assert_eq!(decoded.secrets.keys().collect::<Vec<_>>(), ["ZAI_API_KEY"]);
    rig.sheet("llm.sheet.cancel", json!({})).unwrap();
    wait(waiting).unwrap();
}

fn sample_code() -> (String, String) {
    let mut deepseek = Provider::new("deepseek", Some("deepseek-chat".into()));
    deepseek.api_type = None;
    let set = octosense_llm_config::ProviderSet { primary: Some(deepseek), fallbacks: vec![Provider::new("zai", Some("glm-4.6".into()))] };
    let secrets = [("DEEPSEEK_API_KEY".to_string(), DEEPSEEK_KEY.to_string()), ("ZAI_API_KEY".to_string(), ZAI_KEY.to_string())].into();
    let pin = "7K3M-9QX2".to_string();
    (qr::encode_encrypted(&qr::Provisioning { set, secrets }, &pin).unwrap(), pin)
}

fn assert_imported(rig: &mut Rig, answer: &Value) {
    assert_eq!(answer["applied"], json!(["DeepSeek · deepseek-chat", "Z.ai · glm-4.6"]));
    let providers = rig.ask("llm.providers", Value::Null).unwrap();
    assert!(!providers.to_string().contains("sk-test"));
    assert_eq!(providers["primary"]["key"], "set ••••1234");
    assert_eq!(providers["fallbacks"][0]["key"], "set ••••5678");
    assert!(!rig.profile_text().contains("sk-test"), "keys go to the vault on import too");
    assert_eq!(rig.vault.get("ZAI_API_KEY").unwrap().as_deref(), Some(ZAI_KEY));
    assert!(rig.changed.load(Ordering::SeqCst) >= 1);
}

#[test]
fn a_pasted_code_imports_after_the_right_pin() {
    let mut rig = Rig::new("paste", None);
    let (code, pin) = sample_code();
    let waiting = rig.send(APP, "llm.import_qr", Value::Null, false);
    assert!(rig.host.body().contains("let can_scan = false"));
    assert!(rig.sheet("llm.sheet.scan", json!({})).unwrap_err().contains("no camera scanner"));
    let wrong = rig.sheet("llm.sheet.import", json!({"text": code, "pin": "0000-0000"})).unwrap_err();
    assert!(wrong.contains("wrong PIN"), "{wrong}");
    assert!(!wrong.contains("sk-test"));
    let missing = rig.sheet("llm.sheet.import", json!({"text": code, "pin": ""})).unwrap_err();
    assert!(missing.contains("PIN"), "{missing}");
    assert!(still_waiting(waiting), "a wrong PIN leaves the app waiting");
    let answer = rig.sheet("llm.sheet.import", json!({"text": format!("  {code}\n"), "pin": pin.to_lowercase()})).unwrap();
    assert_eq!(answer["message"], "Saved DeepSeek · deepseek-chat as primary. Added Z.ai · glm-4.6 as a fallback.");
    let told = wait(waiting).unwrap();
    assert_eq!(told["applied"], answer["applied"]);
    assert_eq!(told["message"], answer["message"]);
    assert_eq!(told["added"], answer["applied"], "into an empty list everything is added");
    assert_imported(&mut rig, &answer);
}

/// A camera that "reads" whatever it was given.
struct FakeScanner(Mutex<Vec<Result<String, String>>>);

impl QrScanner for FakeScanner {
    fn scan(&self, done: ScanDone) {
        let next = self.0.lock().unwrap().remove(0);
        std::thread::spawn(move || done(next));
    }
}

#[test]
fn a_scanned_code_imports_after_the_pin() {
    let (code, pin) = sample_code();
    let scanner = Arc::new(FakeScanner(Mutex::new(vec![Err("cancelled".into()), Ok("https://example.com".into()), Ok(code)])));
    let mut rig = Rig::new("scan", Some(scanner));
    assert_eq!(rig.ask("llm.providers", Value::Null).unwrap()["scanner"], true);
    let waiting = rig.send(APP, "llm.import_qr", Value::Null, false);
    assert!(rig.host.body().contains("let can_scan = true"));
    assert_eq!(rig.sheet("llm.sheet.scan", json!({})).unwrap_err(), "Scan cancelled.");
    assert!(rig.sheet("llm.sheet.scan", json!({})).unwrap_err().contains("not an OctoSense"));
    assert_eq!(rig.sheet("llm.sheet.scan", json!({})).unwrap(), json!({"needs_pin": true}));
    assert!(rig.sheet("llm.sheet.import", json!({"text": "", "pin": "WRONG-PIN1"})).unwrap_err().contains("wrong PIN"));
    let answer = rig.sheet("llm.sheet.import", json!({"text": "", "pin": pin})).unwrap();
    assert_eq!(wait(waiting).unwrap()["applied"], answer["applied"]);
    assert_imported(&mut rig, &answer);
}

/// A provider endpoint on localhost: 200 for the right bearer key, else a
/// 401 that quotes the key back the way providers do.
fn fake_provider(key: &'static str) -> (u16, std::thread::JoinHandle<Vec<String>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = stream.read(&mut buf).unwrap();
                request.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&request).to_string();
                if let Some(head_end) = text.find("\r\n\r\n") {
                    let length = text[..head_end]
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()))
                        .unwrap_or(0);
                    if request.len() >= head_end + 4 + length {
                        break;
                    }
                }
            }
            let text = String::from_utf8_lossy(&request).to_string();
            let ok = text.contains(&format!("Bearer {key}"));
            let (status, body) = if ok {
                ("200 OK", r#"{"choices":[]}"#.to_string())
            } else {
                ("401 Unauthorized", format!(r#"{{"error":{{"message":"Incorrect API key provided: sk-****{}"}}}}"#, &key[key.len() - 4..]))
            };
            let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            seen.push(text);
        }
        seen
    });
    (port, server)
}

#[test]
fn test_sends_one_tiny_request_and_never_echoes_the_key() {
    let mut rig = Rig::new("probe", None);
    let key: &'static str = "sk-test-local-0000cccc9999";
    let (port, server) = fake_provider(key);
    let base = format!("http://127.0.0.1:{port}/v1");
    let good = rig.add(json!({"family": "openai", "model": "gpt-test", "base_url": base, "api_type": "default", "key": key}));
    let result = rig.ask("llm.test", json!({"id": good["id"]})).unwrap();
    assert_eq!(result["ok"], true, "{result}");
    assert!(result["ms"].as_u64().is_some());

    // The same endpoint under a wrong key.
    let bad = rig.add(json!({"family": "openai", "model": "gpt-other", "base_url": base, "api_type": "default", "key": "sk-test-wrong-0000dddd9999"}));
    let result = rig.ask("llm.test", json!({"id": bad["id"]})).unwrap();
    assert_eq!(result["ok"], false);
    let error = result["error"].as_str().unwrap();
    assert!(error.starts_with("HTTP 401"), "{error}");
    assert!(!error.contains("9999"), "{error}");

    let seen = server.join().unwrap();
    assert!(seen[0].starts_with("POST /v1/chat/completions"));
    assert!(seen[0].contains("\"max_tokens\":1"));
    assert!(rig.ask("llm.test", json!({"id": "nope"})).unwrap_err().contains("no such provider"));
}

#[test]
fn the_export_sheet_expires_and_forgets_the_code() {
    let mut rig = Rig::with("expire", |o| o.qr_lifetime_secs(3));
    two_providers(&mut rig);
    let (waiting, _code, _pin) = export(&mut rig, Value::Null);
    let body = rig.host.body().to_string();
    // The sheet counts the lifetime it was given down, and closes itself.
    assert!(body.contains("let left = 3"), "the sheet counts from the lifetime");
    assert!(body.contains("Expires in 0:03"));
    assert!(body.contains("if left <= 0 { close() return }"));
    // Nobody closes it: the service does, just after the lifetime, and the
    // app hears so; the code and the PIN are gone from the service.
    let started = std::time::Instant::now();
    let closed = loop {
        if let Some((_, _, result)) = take_replies_for(&[waiting]).pop() {
            break result.map(|s| serde_json::from_str::<Value>(&s).unwrap()).unwrap();
        }
        assert!(started.elapsed().as_secs() < 15, "the export never expired");
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert_eq!(closed, json!({"expired": true}));
    assert!(started.elapsed().as_secs() >= 3, "not before its lifetime");
    assert!(rig.sheet("llm.sheet.qr", json!({})).unwrap_err().contains("No code"));
    assert!(rig.sheet("llm.sheet.show", json!({})).unwrap_err().contains("No code"));
    // A new export starts afresh, with a new PIN.
    let (waiting, _, pin) = export(&mut rig, Value::Null);
    assert_eq!(pin.len(), 9);
    rig.sheet("llm.sheet.cancel", json!({})).unwrap();
    assert_eq!(wait(waiting).unwrap(), json!({}));
}

fn qr_a_code() -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../config/tests/fixtures/qr-a.txt")).unwrap()
}
const QR_A_DEEPSEEK: &str = "sk-test-0000000000000000";
const QR_A_ZAI: &str = "zai-test-0000";

/// Import `code` on the sheet: what the app hears.
fn import(rig: &mut Rig, code: &str, pin: &str) -> Value {
    let waiting = rig.send(APP, "llm.import_qr", Value::Null, false);
    assert!(!rig.host.body().contains("Replace"), "no confirm step");
    let sheet = rig.sheet("llm.sheet.import", json!({"text": code, "pin": pin})).unwrap();
    let told = wait(waiting).unwrap();
    assert_eq!(sheet["message"], told["message"]);
    assert!(!told.to_string().contains("test-0000") && !told.to_string().contains("sk-test"), "no key in the answer");
    told
}

fn saved(rig: &Rig) -> Vec<Provider> {
    profile::load(&profile::profile_path(&rig.dir)).unwrap().set.iter().cloned().collect()
}

fn route(family: &str, model: &str, key_env: &str) -> Provider {
    let mut p = Provider::new(family, Some(model.into()));
    p.key_env = key_env.into();
    p
}

/// The phone's case: a real primary, then QR-A (DeepSeek deepseek-chat and
/// Z.ai glm-4.6 with other keys) added as fallbacks.
const REAL_KEY: &str = "sk-test-real-primary-fcb0";

fn real_primary(rig: &mut Rig) {
    rig.add(json!({"family": "deepseek", "model": "deepseek-v4-flash", "base_url": "", "api_type": "default", "key": REAL_KEY}));
}

#[test]
fn an_import_appends_fallbacks_and_never_overwrites_a_saved_key() {
    let mut rig = Rig::new("append", None);
    real_primary(&mut rig);
    let changes = rig.changed.load(Ordering::SeqCst);
    let told = import(&mut rig, &qr_a_code(), QR_A_PIN);
    assert_eq!(told["message"], "Added DeepSeek · deepseek-chat, Z.ai · glm-4.6 as fallbacks.");
    assert_eq!(told["added"], json!(["DeepSeek · deepseek-chat", "Z.ai · glm-4.6"]));
    assert_eq!(told["updated"], json!([]));
    assert_eq!(rig.changed.load(Ordering::SeqCst), changes + 1);

    // The primary is unchanged; the code's DeepSeek has a slot of its own.
    assert_eq!(
        saved(&rig),
        [
            route("deepseek", "deepseek-v4-flash", "DEEPSEEK_API_KEY"),
            route("deepseek", "deepseek-chat", "DEEPSEEK_2_API_KEY"),
            route("zai", "glm-4.6", "ZAI_API_KEY"),
        ]
    );
    assert_eq!(rig.vault.get("DEEPSEEK_API_KEY").unwrap().as_deref(), Some(REAL_KEY), "the saved key is untouched");
    assert_eq!(rig.vault.get("DEEPSEEK_2_API_KEY").unwrap().as_deref(), Some(QR_A_DEEPSEEK));
    assert_eq!(rig.vault.get("ZAI_API_KEY").unwrap().as_deref(), Some(QR_A_ZAI));
    let json: Value = serde_json::from_str(&rig.profile_text()).unwrap();
    let llm = &json["config"]["llm"];
    assert_eq!(llm["fallbacks"][0]["route"]["api_key_env"], "DEEPSEEK_2_API_KEY", "octos reads the slot from the route");
    assert!(llm["primary"]["route"].get("api_key_env").is_none());
    assert_eq!(json["config"]["env_vars"]["DEEPSEEK_2_API_KEY"], "keychain:");
    assert!(!rig.profile_text().contains("test-0000") && !rig.profile_text().contains(REAL_KEY));
    let providers = rig.ask("llm.providers", Value::Null).unwrap();
    let keys: Vec<Value> = all(&providers).iter().map(|p| p["key"].clone()).collect();
    assert_eq!(keys, [json!("set ••••fcb0"), json!("set ••••0000"), json!("set ••••0000")]);

    // The same code again changes nothing.
    let (before, changes) = (rig.profile_text(), rig.changed.load(Ordering::SeqCst));
    let again = import(&mut rig, &qr_a_code(), QR_A_PIN);
    assert_eq!(rig.changed.load(Ordering::SeqCst), changes, "no kernel restart for nothing");
    assert_eq!(again["message"], "Already saved: DeepSeek · deepseek-chat, Z.ai · glm-4.6.");
    assert_eq!((again["added"].clone(), again["updated"].clone()), (json!([]), json!([])));
    assert_eq!(rig.profile_text(), before);
    assert_eq!(saved(&rig).len(), 3);

    // The phone QR of the merged set carries both DeepSeek keys.
    let (waiting, code, pin) = export(&mut rig, Value::Null);
    let decoded = qr::decode(&code, Some(&pin)).unwrap();
    assert_eq!(decoded.set.iter().cloned().collect::<Vec<_>>(), saved(&rig));
    assert_eq!(decoded.secrets["DEEPSEEK_API_KEY"], REAL_KEY);
    assert_eq!(decoded.secrets["DEEPSEEK_2_API_KEY"], QR_A_DEEPSEEK);
    assert_eq!(decoded.secrets["ZAI_API_KEY"], QR_A_ZAI);
    rig.sheet("llm.sheet.cancel", json!({})).unwrap();
    wait(waiting).unwrap();
    // Removing the added ones takes their slot out of the profile.
    let providers = rig.ask("llm.providers", Value::Null).unwrap();
    for p in &all(&providers)[1..] {
        rig.ask("llm.remove", json!({"id": p["id"]})).unwrap();
    }
    assert_eq!(saved(&rig), [route("deepseek", "deepseek-v4-flash", "DEEPSEEK_API_KEY")]);
    let json: Value = serde_json::from_str(&rig.profile_text()).unwrap();
    assert!(json["config"]["env_vars"].get("DEEPSEEK_2_API_KEY").is_none());
    assert!(json["config"]["env_vars"].get("ZAI_API_KEY").is_none());
    assert_eq!(rig.vault.get("DEEPSEEK_API_KEY").unwrap().as_deref(), Some(REAL_KEY));
    drop(rig);

    // The merged set's QR imports into an empty list as the same set, with
    // the same slots.
    let mut other = Rig::new("append-other", None);
    let told = import(&mut other, &code, &pin);
    assert_eq!(told["message"], "Saved DeepSeek · deepseek-v4-flash as primary. Added DeepSeek · deepseek-chat, Z.ai · glm-4.6 as fallbacks.");
    assert_eq!(saved(&other), decoded.set.iter().cloned().collect::<Vec<_>>());
    assert_eq!(other.vault.get("DEEPSEEK_API_KEY").unwrap().as_deref(), Some(REAL_KEY));
    assert_eq!(other.vault.get("DEEPSEEK_2_API_KEY").unwrap().as_deref(), Some(QR_A_DEEPSEEK));
}

#[test]
fn a_provider_the_code_names_again_keeps_its_place_and_takes_the_code_key() {
    let mut rig = Rig::new("update", None);
    two_providers(&mut rig);
    let told = import(&mut rig, &qr_a_code(), QR_A_PIN);
    assert_eq!(told["message"], "Added Z.ai · glm-4.6 as a fallback. Updated the key for DeepSeek · deepseek-chat.");
    assert_eq!(told["updated"], json!(["DeepSeek · deepseek-chat"]));
    assert_eq!(
        saved(&rig),
        [
            route("deepseek", "deepseek-chat", "DEEPSEEK_API_KEY"),
            Provider::new("zai", None),
            route("zai", "glm-4.6", "ZAI_2_API_KEY"),
        ]
    );
    assert_eq!(rig.vault.get("DEEPSEEK_API_KEY").unwrap().as_deref(), Some(QR_A_DEEPSEEK), "updated in place");
    assert_eq!(rig.vault.get("ZAI_API_KEY").unwrap().as_deref(), Some(ZAI_KEY), "the saved Z.ai key is untouched");
    assert_eq!(rig.vault.get("ZAI_2_API_KEY").unwrap().as_deref(), Some(QR_A_ZAI));
}

#[test]
fn the_same_key_shares_its_slot() {
    let mut rig = Rig::new("share", None);
    real_primary(&mut rig);
    let set = octosense_llm_config::ProviderSet { primary: Some(Provider::new("deepseek", Some("deepseek-chat".into()))), fallbacks: vec![] };
    let secrets = [("DEEPSEEK_API_KEY".to_string(), REAL_KEY.to_string())].into();
    let code = qr::encode_encrypted(&qr::Provisioning { set, secrets }, QR_A_PIN).unwrap();
    let told = import(&mut rig, &code, QR_A_PIN);
    assert_eq!(told["message"], "Added DeepSeek · deepseek-chat as a fallback.");
    assert_eq!(saved(&rig), [route("deepseek", "deepseek-v4-flash", "DEEPSEEK_API_KEY"), route("deepseek", "deepseek-chat", "DEEPSEEK_API_KEY")]);
    assert_eq!(rig.vault.get("DEEPSEEK_2_API_KEY").unwrap(), None);
}

#[test]
fn a_legacy_code_is_appended_too() {
    let mut rig = Rig::new("legacy", None);
    real_primary(&mut rig);
    let legacy = r#"{"llm_family":"deepseek","llm_model":"deepseek-reasoner","llm_key":"sk-test-legacy-00001111"}"#;
    let told = import(&mut rig, legacy, "");
    assert_eq!(told["message"], "Added DeepSeek · deepseek-reasoner as a fallback.");
    assert_eq!(
        saved(&rig),
        [route("deepseek", "deepseek-v4-flash", "DEEPSEEK_API_KEY"), route("deepseek", "deepseek-reasoner", "DEEPSEEK_2_API_KEY")]
    );
    assert_eq!(rig.vault.get("DEEPSEEK_API_KEY").unwrap().as_deref(), Some(REAL_KEY));
    assert_eq!(rig.vault.get("DEEPSEEK_2_API_KEY").unwrap().as_deref(), Some("sk-test-legacy-00001111"));
    // The app cannot import for the sheet, and there is no Replace any more.
    assert!(rig.ask("llm.sheet.import", json!({"text": legacy})).unwrap_err().contains("for the host's sheet"));
    let waiting = rig.send(APP, "llm.import_qr", Value::Null, false);
    assert!(rig.sheet("llm.sheet.replace", json!({})).unwrap_err().contains("no method"));
    rig.sheet("llm.sheet.cancel", json!({})).unwrap();
    assert_eq!(wait(waiting).unwrap_err(), "Cancelled.");
}

#[test]
fn no_sheet_arms_a_timer_that_outlives_it() {
    // A swapped sheet keeps its isolate, and with it any repeating timer the
    // old program armed; Splash also calls a `fn tick()` once a second.
    let size = 21;
    let modules = vec![false; size * size];
    for body in [
        sheets::edit(None),
        sheets::export_waiting(),
        sheets::export(size, &modules, "ABCD-EFGH", &["x".into()], 300),
        sheets::import(true, true, true),
    ] {
        assert!(!body.contains("start_interval"), "{body}");
        assert!(!body.contains("fn tick"), "{body}");
    }
    assert!(sheets::export_waiting().contains("host.request(\"llm.sheet.show\", {}, nil)"));
}

#[test]
fn the_sheets_draw_what_the_tests_read() {
    // The QR views round-trip through the parser above for an odd size.
    let size = 21;
    let modules: Vec<bool> = (0..size * size).map(|i| (i * 7 + i / size) % 3 == 0).collect();
    let body = sheets::export(size, &modules, "ABCD-EFGH", &["DeepSeek · deepseek-chat".into()], 300);
    let (read_size, rows) = matrix_of(&body);
    assert_eq!(read_size, size);
    assert_eq!(rows.concat(), modules);
    assert_eq!(pin_of(&body), "ABCD-EFGH");
}

/// QR-A (config/tests/fixtures): OCTOS1E, PIN 7K3M-9QX2, deepseek then zai.
fn qr_a_png() -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../config/tests/fixtures/qr-a.png")).unwrap()
}
const QR_A_PIN: &str = "7K3M-9QX2";

fn qr_a_set() -> octosense_llm_config::ProviderSet {
    octosense_llm_config::ProviderSet {
        primary: Some(Provider::new("deepseek", Some("deepseek-chat".into()))),
        fallbacks: vec![Provider::new("zai", Some("glm-4.6".into()))],
    }
}

/// A file dialog that "chooses" whatever it was given, answering on another
/// thread as a real one does.
struct FakePicker(Mutex<Vec<Result<Vec<u8>, PickError>>>);

impl QrImagePicker for FakePicker {
    fn pick(&self, done: ImageDone) {
        let next = self.0.lock().unwrap().remove(0);
        std::thread::spawn(move || done(next));
    }
}

fn assert_qr_a_imported(rig: &mut Rig, answer: &Value) {
    assert_eq!(answer["applied"], json!(["DeepSeek · deepseek-chat", "Z.ai · glm-4.6"]));
    let saved = profile::load(&profile::profile_path(&rig.dir)).unwrap().set;
    assert_eq!(saved, qr_a_set());
    assert_eq!(rig.vault.get("DEEPSEEK_API_KEY").unwrap().as_deref(), Some("sk-test-0000000000000000"));
    assert_eq!(rig.vault.get("ZAI_API_KEY").unwrap().as_deref(), Some("zai-test-0000"));
    let providers = rig.ask("llm.providers", Value::Null).unwrap();
    assert!(!providers.to_string().contains("sk-test"));
    assert!(!rig.profile_text().contains("sk-test"));
}

#[test]
fn a_picked_image_imports_after_the_pin() {
    let big = {
        let img = image::GrayImage::from_pixel(8000, 6000, image::Luma([200]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    };
    let blank = {
        let img = image::GrayImage::from_pixel(640, 480, image::Luma([180]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    };
    let picker = Arc::new(FakePicker(Mutex::new(vec![
        Err(PickError::Cancelled),
        Err(PickError::Failed("permission denied".into())),
        Ok(blank),
        Ok(big),
        Ok(qr_a_png()),
    ])));
    let mut rig = Rig::with("pick", |o| o.image_picker(picker));
    let providers = rig.ask("llm.providers", Value::Null).unwrap();
    assert_eq!((providers["image_picker"].clone(), providers["scanner"].clone()), (json!(true), json!(false)));
    let waiting = rig.send(APP, "llm.import_qr", Value::Null, false);
    assert!(rig.host.body().contains("let can_pick = true"));
    assert!(rig.host.body().contains("Choose image"));
    // The app cannot pick for the sheet.
    assert!(rig.ask("llm.sheet.pick", json!({})).unwrap_err().contains("for the host's sheet"));

    assert_eq!(rig.sheet("llm.sheet.pick", json!({})).unwrap(), json!({"needs_pin": false, "cancelled": true, "error": null}));
    assert!(rig.sheet("llm.sheet.pick", json!({})).unwrap()["error"].as_str().unwrap().contains("permission denied"));
    assert_eq!(rig.sheet("llm.sheet.pick", json!({})).unwrap()["error"], "No QR code found in that image.");
    assert!(rig.sheet("llm.sheet.pick", json!({})).unwrap()["error"].as_str().unwrap().contains("too large"));
    assert!(rig.sheet("llm.sheet.import", json!({"text": "", "pin": QR_A_PIN})).unwrap_err().contains("Scan or paste"));
    assert!(still_waiting(waiting), "failed picks leave the sheet up");

    assert_eq!(rig.sheet("llm.sheet.pick", json!({})).unwrap(), json!({"needs_pin": true, "cancelled": false, "error": null}));
    let wrong = rig.sheet("llm.sheet.import", json!({"text": "", "pin": "0000-0000"})).unwrap_err();
    assert!(wrong.contains("wrong PIN"), "{wrong}");
    assert!(still_waiting(waiting));
    let answer = rig.sheet("llm.sheet.import", json!({"text": "", "pin": QR_A_PIN})).unwrap();
    assert_eq!(wait(waiting).unwrap()["applied"], answer["applied"]);
    assert_qr_a_imported(&mut rig, &answer);
}

#[test]
fn without_a_picker_the_sheet_offers_none() {
    let mut rig = Rig::new("nopick", None);
    assert_eq!(rig.ask("llm.providers", Value::Null).unwrap()["image_picker"], false);
    let waiting = rig.send(APP, "llm.import_qr", Value::Null, false);
    assert!(!rig.host.body().contains("Choose image"));
    assert!(rig.sheet("llm.sheet.pick", json!({})).unwrap_err().contains("cannot choose an image"));
    assert!(rig.sheet("llm.sheet.image", json!({})).unwrap_err().contains("No image can be dropped"));
    assert!(!offer_image(qr_a_png()), "no sheet waits for a drop");
    rig.sheet("llm.sheet.cancel", json!({})).unwrap();
    wait(waiting).unwrap_err();
}

#[test]
fn a_dropped_image_imports_after_the_pin() {
    let mut rig = Rig::with("drop", |o| o.image_drops(true));
    assert!(!wants_image() && !offer_image(qr_a_png()), "nothing waits before the sheet");
    let waiting = rig.send(APP, "llm.import_qr", Value::Null, false);
    assert!(rig.host.body().contains("let can_drop = true"));
    assert!(rig.host.body().contains("drop a screenshot"));
    // The sheet waits for a drop; the answer comes with the dropped image.
    let armed = rig.send(APP, "llm.sheet.image", json!({}), true);
    assert!(still_waiting(armed));
    assert!(wants_image());
    assert!(offer_image(b"not an image".to_vec()));
    assert_eq!(wait(armed).unwrap()["error"], "That file is not a PNG or JPEG image.");
    assert!(!wants_image(), "the sheet has to ask again");
    let armed = rig.send(APP, "llm.sheet.image", json!({}), true);
    assert!(still_waiting(armed));
    assert!(offer_image(qr_a_png()));
    assert_eq!(wait(armed).unwrap(), json!({"needs_pin": true, "cancelled": false, "error": null}));
    let rearmed = rig.send(APP, "llm.sheet.image", json!({}), true);
    assert!(rig.sheet("llm.sheet.import", json!({"text": "", "pin": "0000-0000"})).unwrap_err().contains("wrong PIN"));
    let answer = rig.sheet("llm.sheet.import", json!({"text": "", "pin": QR_A_PIN})).unwrap();
    assert_eq!(wait(waiting).unwrap()["applied"], answer["applied"]);
    assert_qr_a_imported(&mut rig, &answer);
    // The import ended the wait: a later drop is not the service's.
    assert!(!wants_image() && !offer_image(qr_a_png()));
    let _ = rearmed;
}
