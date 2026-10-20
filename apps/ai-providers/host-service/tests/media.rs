//! Actual host dispatch and protocol adapters with synthetic provider responses.
//! No external requests, credentials or claims of live provider acceptance.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use octosense_appstore::services::{self, dispatch, take_replies_for, ServiceCall, ServiceHost};
use octosense_llm_config::Provider;
use octosense_llm_service::complete::{self, media, Candidate, Providers};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

static SERIAL: Mutex<()> = Mutex::new(());
static NEXT: AtomicUsize = AtomicUsize::new(140_000);
const APP: &str = "org.example.media";
const KEY: &str = "synthetic-fixture-key";
const T0: u64 = 1_790_000_000_000;
struct NoSheets;
impl ServiceHost for NoSheets {
    fn open_sheet(&mut self, _: String) {
        panic!("media must never prompt");
    }
    fn close_sheet(&mut self) {}
}
#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}
impl Gate {
    fn wait(&self) {
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.changed.wait(open).unwrap();
        }
    }
    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }
}
#[derive(Clone)]
struct Seen {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Value,
    bytes: Option<Vec<u8>>,
}
#[derive(Default)]
struct Fake {
    answers: Mutex<VecDeque<(u16, Vec<u8>)>>,
    seen: Mutex<Vec<Seen>>,
    gate: Mutex<Option<Arc<Gate>>>,
}
impl Fake {
    fn json(&self, status: u16, value: Value) {
        self.answers
            .lock()
            .unwrap()
            .push_back((status, value.to_string().into_bytes()));
    }
    fn bytes(&self, bytes: Vec<u8>) {
        self.answers.lock().unwrap().push_back((200, bytes));
    }
    fn count(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
    fn wait_calls(&self, count: usize) {
        let until = Instant::now() + Duration::from_secs(5);
        while self.count() < count {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
impl media::Transport for Fake {
    fn request(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&str>,
    ) -> Result<(u16, Vec<u8>), String> {
        self.seen.lock().unwrap().push(Seen {
            method: method.into(),
            url: url.into(),
            headers: headers.to_vec(),
            body: body
                .map(|b| serde_json::from_str(b).unwrap())
                .unwrap_or(Value::Null),
            bytes: None,
        });
        let gate = self.gate.lock().unwrap().clone();
        if let Some(gate) = gate {
            gate.wait();
        }
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| "synthetic no response".into())
    }

    fn request_bytes(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &[u8],
    ) -> Result<(u16, Vec<u8>), String> {
        self.seen.lock().unwrap().push(Seen {
            method: method.into(),
            url: url.into(),
            headers: headers.to_vec(),
            body: Value::Null,
            bytes: Some(body.to_vec()),
        });
        if let Some(gate) = self.gate.lock().unwrap().clone() { gate.wait(); }
        self.answers.lock().unwrap().pop_front()
            .ok_or_else(|| "synthetic no response".into())
    }
}
impl complete::Transport for Fake {
    fn post(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<(u16, Vec<u8>), String> {
        media::Transport::request(self, "POST", url, headers, Some(body))
    }
}
struct Config(Mutex<Vec<Candidate>>);
impl Providers for Config {
    fn candidates(&self) -> Result<Vec<Candidate>, String> {
        Ok(self.0.lock().unwrap().clone())
    }
}
struct Rig {
    root: PathBuf,
    fake: Arc<Fake>,
    config: Arc<Config>,
    allowed: Arc<AtomicBool>,
    scope: Arc<AtomicUsize>,
    now: Arc<AtomicU64>,
    _serial: std::sync::MutexGuard<'static, ()>,
}
fn candidate(family: &str) -> Candidate {
    Candidate {
        provider: Provider::new(family, None),
        key: Some(KEY.into()),
    }
}
impl Rig {
    fn new(family: &str) -> Self {
        Self::limits(family, media::Limits::default())
    }
    fn limits(family: &str, limits: media::Limits) -> Self {
        Self::call_limit(family, limits, 1000)
    }
    fn call_limit(family: &str, limits: media::Limits, calls_per_day: u32) -> Self {
        let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("model-media-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join(".host")).unwrap();
        let fake = Arc::new(Fake::default());
        let config = Arc::new(Config(Mutex::new(vec![candidate(family)])));
        let allowed = Arc::new(AtomicBool::new(true));
        let scope = Arc::new(AtomicUsize::new(1));
        let now = Arc::new(AtomicU64::new(T0));
        let grant = allowed.clone();
        let clock = now.clone();
        let account = scope.clone();
        let mut options = complete::Options::default()
            .providers(config.clone())
            .transport(fake.clone())
            .grants(move |_, _| grant.load(Ordering::SeqCst))
            .limits(complete::ledger::Limits {
                per_minute: 100,
                calls_per_day,
                tokens_per_day: 100_000,
            })
            .clock(move || clock.load(Ordering::SeqCst));
        options.media_transport = Some(fake.clone());
        options.media_limits = Some(limits);
        options.scope = Some(Arc::new(move |_, _| {
            Some(format!("account-{}", account.load(Ordering::SeqCst)))
        }));
        complete::register_with(options);
        Self {
            root,
            fake,
            config,
            allowed,
            scope,
            now,
            _serial: serial,
        }
    }
    fn submit(&self, app: &str, method: &str, args: Value) -> usize {
        self.submit_with_foreground(app, method, args, false)
    }
    fn submit_with_foreground(&self, app: &str, method: &str, args: Value, foreground: bool) -> usize {
        let heap = NEXT.fetch_add(1, Ordering::Relaxed);
        dispatch(
            ServiceCall {
                app_id: app.into(),
                service: format!("model.{method}"),
                args,
                from_sheet: false,
                may_prompt: foreground,
                host_dir: self.root.join(".host"),
            },
            heap,
            1,
            &mut NoSheets,
        );
        heap
    }
    fn answer(&self, heap: usize) -> Result<Value, String> {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some((_, _, r)) = take_replies_for(&[heap]).pop() {
                return r.map(|s| serde_json::from_str(&s).unwrap());
            }
            assert!(Instant::now() < until, "no service reply");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn call(&self, method: &str, args: Value) -> Result<Value, String> {
        self.answer(self.submit(APP, method, args))
    }
    fn call_front(&self, app: &str, method: &str, args: Value) -> Result<Value, String> {
        self.answer(self.submit_with_foreground(app, method, args, true))
    }
    fn start_video(&self) -> String {
        self.fake.json(200, json!({"task_id":"task_123"}));
        self.call("video", json!({"prompt":"Synthetic paper kite"}))
            .unwrap()["job"]
            .as_str()
            .unwrap()
            .into()
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        if let Some(g) = self.fake.gate.lock().unwrap().as_ref() {
            g.release();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn embeddings() -> Value {
    json!({"data":[{"index":0,"embedding":[0.25,-0.5,1.0]}],"model":"private-model","usage":{"total_tokens":3}})
}
fn png() -> String {
    let mut bytes = Vec::new();
    {
        let mut e = png::Encoder::new(&mut bytes, 512, 512);
        e.set_color(png::ColorType::Grayscale);
        e.set_depth(png::BitDepth::Eight);
        let mut w = e.write_header().unwrap();
        w.write_image_data(&vec![120; 512 * 512]).unwrap();
    }
    STANDARD.encode(bytes)
}

#[cfg(target_os = "windows")]
fn app_wav(rig: &Rig, app: &str, name: &str, bytes: &[u8]) {
    let dir = rig.root.join(app).join("agent");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), bytes).unwrap();
}

#[cfg(target_os = "windows")]
fn short_wav() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&38u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&16_000u32.to_le_bytes());
    bytes.extend_from_slice(&32_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&0i16.to_le_bytes());
    bytes
}

#[cfg(target_os = "windows")]
#[test]
fn pantry_transcribe_is_admitted_and_capability_hides_provider_details() {
    let rig = Rig::new("moonshot");
    assert_eq!(octosense_appstore::host_api::available_versions().get("model.transcribe"), Some(&1));
    assert_eq!(octosense_appstore::host_api::available_versions().get("model.transcribe_capability"), Some(&1));
    assert_eq!(rig.answer(rig.submit("pantry-steward", "transcribe_capability", json!({}))).unwrap(),
        json!({"schema":1,"supported":true,"configured":false}));
    assert_eq!(rig.answer(rig.submit(APP, "transcribe_capability", json!({}))).unwrap(),
        json!({"schema":1,"supported":true,"configured":false}));
    assert!(rig.answer(rig.submit("pantry-steward", "transcribe_capability", json!({"provider":"minimax"})))
        .unwrap_err().starts_with("bad_request:"));
    *rig.config.0.lock().unwrap() = vec![candidate("moonshot"), candidate("minimax-cn")];
    assert_eq!(rig.answer(rig.submit("pantry-steward", "transcribe_capability", json!({}))).unwrap(),
        json!({"schema":1,"supported":true,"configured":true}));
    assert_eq!(rig.fake.count(), 0);
}

#[cfg(target_os = "windows")]
#[test]
fn pantry_transcribe_uses_only_bounded_wav_and_fixed_minimax_multipart() {
    let rig = Rig::new("moonshot");
    *rig.config.0.lock().unwrap() = vec![candidate("moonshot"), candidate("minimax-cn"), candidate("minimax")];
    let wav = short_wav();
    app_wav(&rig, "pantry-steward", "voice.wav", &wav);
    app_wav(&rig, APP, "voice.wav", &wav);
    rig.fake.json(200, json!({"text":"  今天吃豆腐  ","duration":0.25,"trace_id":"secret-trace"}));
    let request = json!({"audio":{"app_path":"/agent/voice.wav"}});
    let background_error = rig.answer(rig.submit("pantry-steward", "transcribe", request.clone())).unwrap_err();
    assert!(background_error.contains("unavailable to agents/background surfaces"),
        "background must be refused: {background_error}");
    rig.fake.json(200, json!({"text":"今天吃豆腐","duration":0.25}));
    assert_eq!(rig.call_front(APP, "transcribe", request.clone()).unwrap()["text"], "今天吃豆腐");
    let result = rig.call_front("pantry-steward", "transcribe", request).unwrap();
    assert_eq!(result, json!({"text":"今天吃豆腐","duration_seconds":0.25}));
    let seen = rig.fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    let call = &seen[1];
    assert_eq!(call.method, "POST");
    assert_eq!(call.url, "https://api.minimaxi.com/v1/speech_to_text");
    assert!(call.headers.iter().any(|(key,value)| key == "Authorization" && value == &format!("Bearer {KEY}")));
    let content_type = call.headers.iter().find(|(key,_)| key == "Content-Type").unwrap().1.as_str();
    assert!(content_type.starts_with("multipart/form-data; boundary=octosense-"));
    let body = call.bytes.as_ref().unwrap();
    assert!(body.windows(wav.len()).any(|chunk| chunk == wav));
    let form = String::from_utf8_lossy(body);
    for field in ["model", "file", "response_format", "stream"] {
        assert!(form.contains(&format!("name=\"{field}\"")));
    }
    assert_eq!(form.matches("Content-Disposition: form-data; name=\"").count(), 4);
    assert!(form.contains("asr-1.0") && form.contains("\r\njson\r\n") && form.contains("\r\nfalse\r\n"));
    for forbidden in ["timestamp_level", "language", "speaker", "subtitle"] {
        assert!(!form.contains(forbidden));
    }
    assert!(!result.to_string().contains(KEY));
}

#[cfg(target_os = "windows")]
#[test]
fn pantry_transcribe_rejects_paths_and_malformed_audio_before_upload() {
    let rig = Rig::new("minimax");
    app_wav(&rig, "pantry-steward", "valid.wav", &short_wav());
    app_wav(&rig, "pantry-steward", "empty.wav", b"");
    app_wav(&rig, "pantry-steward", "broken.wav", b"RIFFbad");
    app_wav(&rig, "pantry-steward", "oversize.wav", &vec![0; 1024 * 1024 + 1]);
    for path in ["../valid.wav", "C:/valid.wav", "agent\\valid.wav", "https://host/valid.wav",
        "data:audio/wav;base64,AAAA", "//agent/valid.wav", "/agent/empty.wav",
        "/agent/broken.wav", "/agent/oversize.wav", "/agent/missing.wav"] {
        let err = rig.call_front("pantry-steward", "transcribe", json!({"audio":{"app_path":path}})).unwrap_err();
        assert!(err.starts_with("bad_request:"), "{path}: {err}");
    }
    for args in [json!({}), json!({"audio":"/agent/valid.wav"}),
        json!({"audio":{"app_path":"/agent/valid.wav","url":"https://host"}}),
        json!({"audio":{"app_path":"/agent/valid.wav"},"model":"other"})] {
        assert!(rig.call_front("pantry-steward", "transcribe", args).unwrap_err().starts_with("bad_request:"));
    }
    assert_eq!(rig.fake.count(), 0);
}

#[cfg(target_os = "windows")]
#[test]
fn pantry_transcribe_charges_call_budget_and_never_retries_or_leaks_provider_errors() {
    let rig = Rig::call_limit("minimax", media::Limits::default(), 0);
    app_wav(&rig, "pantry-steward", "voice.wav", &short_wav());
    assert!(rig.call_front("pantry-steward", "transcribe", json!({"audio":{"app_path":"/agent/voice.wav"}}))
        .unwrap_err().starts_with("budget:"));
    assert_eq!(rig.fake.count(), 0);
    drop(rig);

    let rig = Rig::new("minimax");
    app_wav(&rig, "pantry-steward", "voice.wav", &short_wav());
    rig.fake.json(403, json!({"error":format!("{KEY} https://private.example/trace") }));
    let err = rig.call_front("pantry-steward", "transcribe", json!({"audio":{"app_path":"/agent/voice.wav"}})).unwrap_err();
    assert!(err.starts_with("provider:"));
    assert!(!err.contains(KEY) && !err.contains("private.example"));
    assert_eq!(rig.fake.count(), 1);
}

#[cfg(target_os = "windows")]
#[test]
fn transcription_seconds_are_reserved_before_upload() {
    let mut limits = media::Limits::default();
    limits.transcription_seconds_per_day = 0;
    let rig = Rig::limits("minimax", limits);
    app_wav(&rig, "pantry-steward", "voice.wav", &short_wav());
    let err = rig.call_front("pantry-steward", "transcribe",
        json!({"audio":{"app_path":"/agent/voice.wav"}})).unwrap_err();
    assert!(err.starts_with("budget:"), "{err}");
    assert_eq!(rig.fake.count(), 0);
}

#[cfg(target_os = "windows")]
#[test]
fn minimaxi_alias_is_a_configured_transcription_route() {
    let rig = Rig::new("minimaxi");
    assert_eq!(rig.call("transcribe_capability", json!({})).unwrap()["configured"], true);
}

#[cfg(target_os = "windows")]
#[test]
fn pantry_transcribe_rejects_empty_or_invalid_provider_text_without_leaking_response() {
    let rig = Rig::new("minimax");
    app_wav(&rig, "pantry-steward", "voice.wav", &short_wav());
    let args = json!({"audio":{"app_path":"/agent/voice.wav"}});
    for reply in [
        json!({"text":"  ","duration":0.25,"trace_id":KEY}),
        json!({"text":"hello","duration":0,"trace_id":KEY}),
        json!({"text":"hello","trace_id":KEY}),
        json!({"text":"x".repeat(4097),"duration":0.25,"trace_id":KEY}),
    ] {
        rig.fake.json(200, reply);
        let error = rig.call_front("pantry-steward", "transcribe", args.clone()).unwrap_err();
        assert!(error.starts_with("provider:"), "{error}");
        assert!(!error.contains(KEY));
    }
    assert_eq!(rig.fake.count(), 4);
}

#[cfg(target_os = "windows")]
#[test]
fn pantry_transcribe_drops_a_late_result_after_provider_change() {
    let rig = Rig::new("minimax");
    app_wav(&rig, "pantry-steward", "voice.wav", &short_wav());
    let gate = Arc::new(Gate::default());
    *rig.fake.gate.lock().unwrap() = Some(gate.clone());
    rig.fake.json(200, json!({"text":"迟到回复","duration":0.25}));
    let heap = rig.submit_with_foreground("pantry-steward", "transcribe",
        json!({"audio":{"app_path":"/agent/voice.wav"}}), true);
    rig.fake.wait_calls(1);
    *rig.config.0.lock().unwrap() = vec![candidate("moonshot")];
    gate.release();
    assert!(rig.answer(heap).unwrap_err().starts_with("capability:"));
    assert_eq!(rig.fake.count(), 1);
}

#[cfg(target_os = "windows")]
#[test]
fn pantry_transcribe_requires_a_current_app_grant_at_start_and_delivery() {
    let rig = Rig::new("minimax");
    app_wav(&rig, "pantry-steward", "voice.wav", &short_wav());
    let args = json!({"audio":{"app_path":"/agent/voice.wav"}});
    rig.allowed.store(false, Ordering::SeqCst);
    assert!(rig.answer(rig.submit("pantry-steward", "transcribe_capability", json!({})))
        .unwrap_err().starts_with("capability:"));
    assert!(rig.call_front("pantry-steward", "transcribe", args.clone())
        .unwrap_err().starts_with("capability:"));
    assert_eq!(rig.fake.count(), 0);

    rig.allowed.store(true, Ordering::SeqCst);
    let gate = Arc::new(Gate::default());
    *rig.fake.gate.lock().unwrap() = Some(gate.clone());
    rig.fake.json(200, json!({"text":"late","duration":0.25}));
    let heap = rig.submit_with_foreground("pantry-steward", "transcribe", args, true);
    rig.fake.wait_calls(1);
    rig.allowed.store(false, Ordering::SeqCst);
    gate.release();
    assert!(rig.answer(heap).unwrap_err().starts_with("capability:"));
    assert_eq!(rig.fake.count(), 1);
}

#[test]
fn embeddings_use_the_real_adapter_hide_provider_and_reorder_batch_results() {
    let rig = Rig::new("openai");
    rig.fake.json(
        200,
        json!({"data":[{"index":1,"embedding":[2.0,3.0]},{"index":0,"embedding":[0.0,1.0]}]}),
    );
    let reply = rig
        .call(
            "embeddings",
            json!({"input":["synthetic one","synthetic two"],"output":{"class":"strong"}}),
        )
        .unwrap();
    assert_eq!(reply["embeddings"], json!([[0.0, 1.0], [2.0, 3.0]]));
    assert_eq!(reply["meta"]["budget"]["embeddings"]["used"], 26);
    let seen = rig.fake.seen.lock().unwrap();
    let call = &seen[0];
    assert_eq!(call.method, "POST");
    assert_eq!(call.url, "https://api.openai.com/v1/embeddings");
    assert_eq!(call.body["model"], "text-embedding-3-large");
    assert_eq!(call.body["encoding_format"], "float");
    assert!(call
        .headers
        .iter()
        .any(|(k, v)| k == "Authorization" && v == &format!("Bearer {KEY}")));
    for private in [KEY, "openai", "text-embedding"] {
        assert!(!reply.to_string().contains(private));
    }
}
#[test]
fn deepseek_does_not_claim_embedding_or_media_support() {
    let rig = Rig::new("deepseek");
    let capabilities = rig.call("capabilities", json!({})).unwrap();
    assert_eq!(
        capabilities["configured"],
        json!({"image":false,"audio":false,"video":false,"embeddings":false})
    );
    assert!(rig
        .call("embeddings", json!({"input":"synthetic"}))
        .unwrap_err()
        .starts_with("no_provider:"));
    assert_eq!(rig.fake.count(), 0);
}
#[test]
fn minimax_image_returns_valid_bounded_image_data() {
    let rig = Rig::new("minimax");
    rig.fake.json(
        200,
        json!({"base_resp":{"status_code":0},"data":{"image_base64":[png()]}}),
    );
    let result = rig
        .call(
            "image",
            json!({"prompt":"Synthetic square","size":"512x512","seed":42}),
        )
        .unwrap();
    assert_eq!(result["width"], 512);
    assert_eq!(result["mime"], "image/png");
    assert!(STANDARD
        .decode(result["b64_json"].as_str().unwrap())
        .unwrap()
        .starts_with(b"\x89PNG"));
    let seen = rig.fake.seen.lock().unwrap();
    assert_eq!(seen[0].body["model"], "image-01");
    assert_eq!(seen[0].body["seed"], 42);
    assert!(seen[0].url.ends_with("/v1/image_generation"));
}
#[test]
fn audio_normalizes_minimax_hex_and_openai_binary_without_fake_duration() {
    {
        let rig = Rig::new("minimax");
        rig.fake.json(200,json!({"base_resp":{"status_code":0},"data":{"audio":"4944330000"},"extra_info":{"audio_length":1250}}));
        let r = rig
            .call("audio", json!({"text":"Synthetic voice"}))
            .unwrap();
        assert_eq!(r["duration_s"], 1.25);
        assert_eq!(
            STANDARD.decode(r["b64_json"].as_str().unwrap()).unwrap(),
            b"ID3\0\0"
        );
    }
    {
        let rig = Rig::new("openai");
        rig.fake.bytes(b"ID3\0\0".to_vec());
        let r = rig
            .call(
                "audio",
                json!({"text":"Synthetic voice","voice":"warm-female"}),
            )
            .unwrap();
        assert!(r.get("duration_s").is_none());
        assert_eq!(rig.fake.seen.lock().unwrap()[0].body["voice"], "coral");
    }
}
#[test]
fn malformed_inputs_and_unsupported_provider_knobs_never_submit() {
    let rig = Rig::new("openai");
    for (method, args) in [
        (
            "audio",
            json!({"text":"x","endpoint":"https://evil.example"}),
        ),
        ("audio", json!({"text":"x","format":"wav"})),
        ("embeddings", json!({"input":[]})),
        ("embeddings", json!({"input":["x",2]})),
        ("image", json!({"prompt":"x","n":2})),
        ("image", json!({"prompt":"x","seed":1})),
        ("image", json!({"prompt":"x","size":"512x512"})),
        ("video", json!({"prompt":"x","fps":24})),
        (
            "embeddings",
            json!({"input":"x","class":"fast","output":{"class":"fast"}}),
        ),
    ] {
        assert!(
            rig.call(method, args)
                .unwrap_err()
                .starts_with("bad_request:"),
            "{method}"
        );
    }
    assert_eq!(rig.fake.count(), 0);
}
#[test]
fn provider_errors_are_sanitized_and_uncertain_submissions_are_not_retried() {
    let rig = Rig::new("minimax");
    rig.fake
        .json(403, json!({"error":format!("private route and {KEY}")}));
    let err = rig
        .call("video", json!({"prompt":"synthetic"}))
        .unwrap_err();
    assert!(err.starts_with("provider:"));
    assert!(!err.contains(KEY));
    assert_eq!(rig.fake.count(), 1);
    let budget = rig.call("budget", json!({})).unwrap();
    assert_eq!(budget["media"]["video"]["used"], 4);
}
#[test]
fn malformed_and_oversized_provider_results_fail_closed() {
    let rig = Rig::new("openai");
    for response in [
        json!({"data":[{"index":0,"embedding":["NaN"]}]}),
        json!({"data":[{"index":0,"embedding":[]}]}),
        json!({"data":[{"index":1,"embedding":[0.0]}]}),
        json!({"data":[{"index":0,"embedding":vec![0.0;4097]}]}),
    ] {
        rig.fake.json(200, response);
        assert!(rig
            .call("embeddings", json!({"input":"synthetic"}))
            .unwrap_err()
            .starts_with("invalid_output:"));
    }
    rig.fake.bytes(vec![b'x'; media::RESPONSE_MAX + 1]);
    assert!(rig
        .call("embeddings", json!({"input":"synthetic"}))
        .unwrap_err()
        .starts_with("too_large:"));
    rig.fake.bytes(b"<html>not mp3</html>".to_vec());
    assert!(rig.call("audio", json!({"text":"synthetic"})).is_err());
}
#[test]
fn quotas_persist_and_are_per_modality_per_app_and_fail_closed_on_corruption() {
    let mut limits = media::Limits::default();
    limits.embedding_bytes_per_day = 3;
    let rig = Rig::limits("openai", limits);
    rig.fake.json(200, embeddings());
    rig.call("embeddings", json!({"input":"abc"})).unwrap();
    assert!(rig
        .call("embeddings", json!({"input":"d"}))
        .unwrap_err()
        .starts_with("budget:"));
    assert_eq!(rig.fake.count(), 1);
    rig.fake.json(200, embeddings());
    assert!(rig
        .answer(rig.submit("org.example.other", "embeddings", json!({"input":"abc"})))
        .is_ok());
    let ledger = rig.root.join(".host/model/media-ledger.json");
    let stored: Value = serde_json::from_slice(&std::fs::read(&ledger).unwrap()).unwrap();
    assert_eq!(stored["apps"][APP]["embeddings"], 3);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&ledger).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    std::fs::write(ledger, "broken").unwrap();
    assert!(rig
        .call("audio", json!({"text":"x"}))
        .unwrap_err()
        .starts_with("budget:"));
    assert_eq!(rig.fake.count(), 2);
}
#[test]
fn video_jobs_poll_without_resubmission_and_bind_app_account_and_provider() {
    let rig = Rig::new("minimax");
    let job = rig.start_video();
    assert!(rig
        .answer(rig.submit("org.example.other", "video.status", json!({"job":job})))
        .is_err());
    rig.scope.store(2, Ordering::SeqCst);
    assert!(rig.call("video.status", json!({"job":job})).is_err());
    rig.scope.store(1, Ordering::SeqCst);
    rig.fake
        .json(200, json!({"task":{"id":"task_123","status":"running"}}));
    assert_eq!(
        rig.call("video.status", json!({"job":job})).unwrap()["status"],
        "running"
    );
    assert_eq!(
        rig.call("video.status", json!({"job":job})).unwrap()["status"],
        "running"
    );
    assert_eq!(rig.fake.count(), 2);
    rig.now.fetch_add(5000, Ordering::SeqCst);
    rig.fake.json(200,json!({"task":{"id":"task_123","status":"succeeded","duration":4,"resolution":"768P","content":{"url":"https://cdn.example.com/synthetic.mp4"}}}));
    let done = rig.call("video.status", json!({"job":job})).unwrap();
    assert_eq!(done["result"]["duration_s"], 4);
    assert!(!done.to_string().contains("task_123"));
    assert_eq!(
        rig.fake
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.method == "POST")
            .count(),
        1
    );
    rig.config.0.lock().unwrap()[0].key = Some("rotated-test-key".into());
    assert!(rig
        .call("video.status", json!({"job":job}))
        .unwrap_err()
        .starts_with("capability:"));
}
#[test]
fn video_cancel_reports_remote_truth_and_keeps_running_job_after_refusal() {
    let rig = Rig::new("minimax");
    let job = rig.start_video();
    rig.fake
        .json(400, json!({"error":{"message":"running cannot cancel"}}));
    assert!(rig
        .call("video.cancel", json!({"job":job}))
        .unwrap_err()
        .starts_with("provider:"));
    rig.now.fetch_add(5000, Ordering::SeqCst);
    rig.fake.json(
        200,
        json!({"task_id":"task_123","action":"cancelled","status":"cancelled"}),
    );
    assert_eq!(
        rig.call("video.cancel", json!({"job":job})).unwrap()["status"],
        "cancelled"
    );
    let seen = rig.fake.seen.lock().unwrap();
    assert_eq!(seen[1].method, "DELETE");
    assert!(seen[1].url.ends_with("/v2/video_generation/task_123"));
}
#[test]
fn video_rejects_active_local_urls_and_mismatched_remote_jobs() {
    let rig = Rig::new("minimax");
    let job = rig.start_video();
    for url in [
        "javascript:alert(1)",
        "https://127.0.0.1/movie.mp4",
        "https://test.local/movie.mp4",
        "https://user:pass@cdn.example.com/x",
    ] {
        rig.now.fetch_add(5000, Ordering::SeqCst);
        rig.fake.json(200,json!({"task":{"id":"task_123","status":"succeeded","duration":4,"resolution":"768P","content":{"url":url}}}));
        assert!(rig
            .call("video.status", json!({"job":job}))
            .unwrap_err()
            .starts_with("invalid_output:"));
    }
    rig.now.fetch_add(86_400_000, Ordering::SeqCst);
    assert!(rig
        .call("video.status", json!({"job":job}))
        .unwrap_err()
        .starts_with("bad_request:"));
}
#[test]
fn withdrawal_and_account_switch_block_inflight_delivery() {
    for change in ["grant", "account", "provider"] {
        let rig = Rig::new("openai");
        let gate = Arc::new(Gate::default());
        *rig.fake.gate.lock().unwrap() = Some(gate.clone());
        rig.fake.json(200, embeddings());
        let heap = rig.submit(
            APP,
            "embeddings",
            json!({"input":"synthetic private fixture"}),
        );
        rig.fake.wait_calls(1);
        match change {
            "grant" => rig.allowed.store(false, Ordering::SeqCst),
            "account" => rig.scope.store(2, Ordering::SeqCst),
            _ => rig.config.0.lock().unwrap().clear(),
        }
        gate.release();
        assert!(rig.answer(heap).unwrap_err().starts_with("capability:"));
    }
}
#[test]
fn cancelling_an_isolate_drops_reply_and_worker_limit_refuses_overload() {
    let rig = Rig::new("openai");
    let gate = Arc::new(Gate::default());
    *rig.fake.gate.lock().unwrap() = Some(gate.clone());
    for _ in 0..4 {
        rig.fake.json(200, embeddings());
    }
    let a = rig.submit(APP, "embeddings", json!({"input":"a"}));
    let b = rig.submit(APP, "embeddings", json!({"input":"b"}));
    rig.fake.wait_calls(2);
    assert!(rig
        .call("embeddings", json!({"input":"c"}))
        .unwrap_err()
        .starts_with("rate:"));
    let c = rig.submit("org.example.second", "embeddings", json!({"input":"c"}));
    let d = rig.submit("org.example.second", "embeddings", json!({"input":"d"}));
    rig.fake.wait_calls(4);
    assert!(rig
        .answer(rig.submit("org.example.third", "embeddings", json!({"input":"e"})))
        .unwrap_err()
        .starts_with("rate:"));
    services::cancel_heap(a);
    gate.release();
    for heap in [b, c, d] {
        rig.answer(heap).unwrap();
    }
    assert!(take_replies_for(&[a]).is_empty());
}
#[test]
fn discovery_reports_actual_versioned_methods_and_no_secret_or_account_operation() {
    let _rig = Rig::new("openai");
    let methods = octosense_appstore::host_api::methods();
    for method in [
        "model.image",
        "model.audio",
        "model.embeddings",
        "model.video",
        "model.video.status",
        "model.video.cancel",
        "model.capabilities",
    ] {
        let descriptor = methods.iter().find(|d| d.name == method).unwrap();
        assert_eq!(descriptor.version, 1);
        assert_eq!(descriptor.capability, "model");
        assert_eq!(descriptor.agent_access, services::AgentAccess::Allowed);
        assert_eq!(descriptor.input_schema["additionalProperties"], false);
    }
}

#[test]
fn existing_complete_cannot_retry_or_deliver_after_admission_is_revoked() {
    let rig = Rig::new("deepseek");
    let gate = Arc::new(Gate::default());
    *rig.fake.gate.lock().unwrap() = Some(gate.clone());
    rig.fake.json(
        200,
        json!({"choices":[{"message":{"content":"not valid JSON"}}]}),
    );
    let heap = rig.submit(
        APP,
        "complete",
        json!({"task":"Synthetic classification","input":"fixture","schema":{"type":"object"}}),
    );
    rig.fake.wait_calls(1);
    rig.allowed.store(false, Ordering::SeqCst);
    gate.release();
    assert!(rig.answer(heap).unwrap_err().starts_with("capability:"));
    assert_eq!(rig.fake.count(), 1);
}

#[test]
fn quota_is_enforced_after_service_restart_and_resets_on_the_next_utc_day() {
    let mut limits = media::Limits::default();
    limits.embedding_bytes_per_day = 3;
    let rig = Rig::limits("openai", limits);
    rig.fake.json(200, embeddings());
    rig.call("embeddings", json!({"input":"abc"})).unwrap();
    let now = rig.now.clone();
    let mut options = complete::Options::default()
        .providers(rig.config.clone())
        .grants(|_, _| true)
        .clock(move || now.load(Ordering::SeqCst));
    options.media_transport = Some(rig.fake.clone());
    options.media_limits = Some(limits);
    complete::register_with(options);
    assert!(rig
        .call("embeddings", json!({"input":"abc"}))
        .unwrap_err()
        .starts_with("budget:"));
    assert_eq!(rig.fake.count(), 1);
    rig.now.fetch_add(86_400_000, Ordering::SeqCst);
    rig.fake.json(200, embeddings());
    assert!(rig.call("embeddings", json!({"input":"abc"})).is_ok());
}

#[test]
fn actual_http_transport_refuses_redirects_and_oversized_streams() {
    use media::Transport;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    for oversized in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            connection
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = vec![0; 4096];
            let read = connection.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..read]).starts_with("GET /fixture HTTP/1.1"));
            if oversized {
                let bytes = vec![b'x'; media::RESPONSE_MAX + 1];
                write!(
                    connection,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                )
                .unwrap();
                let _ = connection.write_all(&bytes);
            } else {
                write!(connection,"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/must-not-follow\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            }
        });
        // Direct transport fixture only: production route selection requires HTTPS.
        let result = media::Http.request("GET", &format!("http://{address}/fixture"), &[], None);
        if oversized {
            assert!(result.unwrap_err().contains("byte limit"));
        } else {
            assert_eq!(result.unwrap().0, 302);
        }
        server.join().unwrap();
    }
}

#[test]
fn actual_http_transport_sends_binary_body_without_following_redirect() {
    use media::Transport;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut connection, _) = listener.accept().unwrap();
        connection.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        while !request.windows(4).any(|part| part == b"\r\n\r\n") {
            let count = connection.read(&mut buffer).unwrap();
            assert!(count > 0);
            request.extend_from_slice(&buffer[..count]);
        }
        let body_start = request.windows(4).position(|part| part == b"\r\n\r\n").unwrap() + 4;
        while request.len() - body_start < 5 {
            let count = connection.read(&mut buffer).unwrap();
            assert!(count > 0);
            request.extend_from_slice(&buffer[..count]);
        }
        assert!(String::from_utf8_lossy(&request[..body_start]).starts_with("POST /fixture HTTP/1.1"));
        assert_eq!(&request[body_start..], b"a\0b\xffc");
        write!(connection,"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/must-not-follow\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let result = media::Http.request_bytes("POST", &format!("http://{address}/fixture"),
        &[("Content-Type".into(), "application/octet-stream".into())], b"a\0b\xffc");
    assert_eq!(result.unwrap().0, 302);
    server.join().unwrap();
}

#[test]
fn an_anthropic_only_route_does_not_reuse_its_key_for_media() {
    let rig = Rig::new("minimax");
    rig.config.0.lock().unwrap()[0].provider.api_type =
        Some(octosense_llm_config::ApiType::Anthropic);
    assert_eq!(
        rig.call("capabilities", json!({})).unwrap()["configured"]["image"],
        false
    );
    assert!(rig
        .call("image", json!({"prompt":"synthetic"}))
        .unwrap_err()
        .starts_with("no_provider:"));
    assert_eq!(rig.fake.count(), 0);
}

#[test]
fn admission_and_ledger_work_are_off_ui_and_bounded_for_every_model_method() {
    let rig = Rig::new("openai");
    let gate = Arc::new(Gate::default());
    let admission_gate = gate.clone();
    let entered = Arc::new(AtomicUsize::new(0));
    let checked = entered.clone();
    let ui_thread = std::thread::current().id();
    let account = rig.scope.clone();
    let mut options = complete::Options::default()
        .providers(rig.config.clone())
        .transport(rig.fake.clone())
        .grants(move |_, _| {
            assert_ne!(
                std::thread::current().id(),
                ui_thread,
                "signed-bundle admission must never block the dispatch/UI thread"
            );
            checked.fetch_add(1, Ordering::SeqCst);
            admission_gate.wait();
            true
        });
    options.media_transport = Some(rig.fake.clone());
    options.scope = Some(Arc::new(move |_, _| {
        Some(format!("account-{}", account.load(Ordering::SeqCst)))
    }));
    complete::register_with(options);
    let mut heaps = Vec::new();
    for (index, method) in ["complete", "budget", "capabilities", "embeddings"]
        .iter()
        .enumerate()
    {
        heaps.push(rig.submit(&format!("org.example.worker{index}"), method, json!({})));
    }
    let until = Instant::now() + Duration::from_secs(5);
    while entered.load(Ordering::SeqCst) < 4 {
        assert!(Instant::now() < until, "admission workers did not start");
        std::thread::sleep(Duration::from_millis(2));
    }
    // Four admission reads are blocked, yet dispatch can refuse overload at
    // once, and no caller's ledger has been opened or created before admission.
    assert!(rig
        .call("budget", json!({}))
        .unwrap_err()
        .starts_with("rate:"));
    assert!(!rig.root.join(".host/model").exists());
    services::cancel_heap(heaps[0]);
    rig.scope.store(2, Ordering::SeqCst);
    gate.release();
    for heap in heaps.into_iter().skip(1) {
        assert!(rig.answer(heap).unwrap_err().starts_with("capability:"));
    }
    assert_eq!(rig.fake.count(), 0);
    assert!(!rig.root.join(".host/model").exists());
}

#[test]
fn video_submission_expiring_during_provider_request_fails_without_poisoning_jobs() {
    let rig = Rig::new("minimax");
    let gate = Arc::new(Gate::default());
    *rig.fake.gate.lock().unwrap() = Some(gate.clone());
    rig.fake.json(200, json!({"task_id":"task_fixture"}));
    rig.fake.json(200, json!({"task_id":"task_fixture"}));
    let first = rig.submit(APP, "video", json!({"prompt":"Synthetic first kite"}));
    rig.fake.wait_calls(1);
    rig.now.fetch_add(86_400_000, Ordering::SeqCst);
    // The second submission prunes the first job while its POST is in flight.
    let second = rig.submit(APP, "video", json!({"prompt":"Synthetic second kite"}));
    rig.fake.wait_calls(2);
    gate.release();
    assert!(rig.answer(first).unwrap_err().starts_with("capability:"));
    let current = rig.answer(second).unwrap();
    assert_eq!(current["status"], "queued");
    assert_eq!(rig.fake.count(), 2);
    rig.fake
        .json(200, json!({"task":{"id":"task_fixture","status":"queued"}}));
    // A panic in the first worker would poison the shared job table, making
    // this later operation fail instead of returning the surviving job.
    assert_eq!(
        rig.call("video.status", json!({"job":current["job"]}))
            .unwrap()["status"],
        "queued"
    );
}
