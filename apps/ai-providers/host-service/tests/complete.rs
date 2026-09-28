//! The `model` service through App Hub's real dispatch path, with a fake
//! provider: no network. The live check is `examples/model_complete_live.rs`.
use octosense_appstore::services::{dispatch, take_replies_for, ServiceCall, ServiceHost};
use octosense_llm_config::Provider;
use octosense_llm_service::complete::{self, ledger::Limits, Candidate, Code, Providers, Request, Transport, OUTPUT_MAX};
use octosense_llm_service::model::ProfileStore;
use octosense_llm_service::vault::MemoryVault;
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// One service is registered at a time (the registry is global).
static SERIAL: Mutex<()> = Mutex::new(());
static NEXT: AtomicUsize = AtomicUsize::new(41_000);

const APP: &str = "com.example.notes";
const KEY: &str = "sk-test-deepseek-0000aaaa1234";

struct NoSheets;
impl ServiceHost for NoSheets {
    fn open_sheet(&mut self, _body: String) {}
    fn close_sheet(&mut self) {}
}

/// One request the fake saw: URL, headers, body.
type Seen = (String, Vec<(String, String)>, Value);

/// What the fake provider answers, in order; every request it saw.
#[derive(Default)]
struct Fake {
    answers: Mutex<VecDeque<Result<(u16, String), String>>>,
    seen: Mutex<Vec<Seen>>,
}

impl Fake {
    fn answer(&self, a: Result<(u16, String), String>) -> &Self {
        self.answers.lock().unwrap().push_back(a);
        self
    }
    /// An OpenAI chat-completions answer carrying `content`.
    fn says(&self, content: &str) -> &Self {
        let body = json!({"choices": [{"message": {"content": content}}], "usage": {"prompt_tokens": 100, "completion_tokens": 20}});
        self.answer(Ok((200, body.to_string())))
    }
    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

impl Transport for Fake {
    fn post(&self, url: &str, headers: &[(String, String)], body: &str) -> Result<(u16, Vec<u8>), String> {
        self.seen.lock().unwrap().push((url.to_string(), headers.to_vec(), serde_json::from_str(body).unwrap()));
        match self.answers.lock().unwrap().pop_front() {
            Some(Ok((status, body))) => Ok((status, body.into_bytes())),
            Some(Err(e)) => Err(e),
            None => Err("connection refused".into()),
        }
    }
}

struct Fixed(Vec<Candidate>);
impl Providers for Fixed {
    fn candidates(&self) -> Result<Vec<Candidate>, String> {
        Ok(self.0.clone())
    }
}

fn deepseek(model: &str) -> Candidate {
    Candidate { provider: Provider::new("deepseek", Some(model.into())), key: Some(KEY.into()) }
}

fn anthropic(model: &str) -> Candidate {
    Candidate { provider: Provider::new("anthropic", Some(model.into())), key: Some("sk-ant-test-0000".into()) }
}

struct Rig {
    dir: PathBuf,
    fake: Arc<Fake>,
    now: Arc<AtomicU64>,
    host: Arc<complete::ModelHost>,
    _serial: std::sync::MutexGuard<'static, ()>,
}

const T0: u64 = 1_790_000_000_000;

impl Rig {
    fn new(tag: &str, providers: Vec<Candidate>) -> Rig {
        Rig::with(tag, providers, |o| o)
    }

    fn with(tag: &str, providers: Vec<Candidate>, extra: impl FnOnce(complete::Options) -> complete::Options) -> Rig {
        let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("model-service-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".host")).unwrap();
        let fake = Arc::new(Fake::default());
        let now = Arc::new(AtomicU64::new(T0));
        let clock = now.clone();
        let options = complete::Options::default()
            .providers(Arc::new(Fixed(providers)))
            .transport(fake.clone())
            .grants(|app: &str, _: &Path| app == APP)
            .clock(move || clock.load(Ordering::SeqCst));
        let host = complete::register_with(extra(options));
        Rig { dir, fake, now, host, _serial: serial }
    }

    fn call(&self, app: &str, method: &str, args: Value) -> Result<Value, String> {
        let heap = NEXT.fetch_add(1, Ordering::Relaxed);
        let call = ServiceCall { app_id: app.into(), service: format!("model.{method}"), args, from_sheet: false, host_dir: self.dir.join(".host") };
        dispatch(call, heap, 1, &mut NoSheets);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some((_, _, result)) = take_replies_for(&[heap]).pop() {
                return result.map(|text| serde_json::from_str(&text).unwrap());
            }
            assert!(Instant::now() < deadline, "no answer to model.{method}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn complete(&self, args: Value) -> Result<Value, String> {
        self.call(APP, "complete", args)
    }
}

fn args() -> Value {
    json!({
        "task": "Give the note a short title and up to three tags.",
        "input": {"note": "Buy oat milk and call the plumber on Monday."},
        "schema": {
            "type": "object", "additionalProperties": false, "required": ["title", "tags"],
            "properties": {
                "title": {"type": "string", "maxLength": 40},
                "tags": {"type": "array", "maxItems": 3, "items": {"type": "string"}}
            }
        }
    })
}

const GOOD: &str = r#"{"title":"Errands","tags":["shopping","home"]}"#;

#[test]
fn a_valid_reply_passes_with_class_usage_and_budget_and_nothing_about_the_provider() {
    let rig = Rig::new("pass", vec![deepseek("deepseek-v4-flash")]);
    rig.fake.says(&format!("```json\n{GOOD}\n```"));
    let r = rig.complete(args()).unwrap();
    assert_eq!(r["output"], json!({"title": "Errands", "tags": ["shopping", "home"]}));
    let meta = &r["meta"];
    assert_eq!((meta["class"].as_str(), meta["requested"].as_str(), meta["attempts"].as_u64()), (Some("fast"), Some("fast"), Some(1)));
    assert_eq!(meta["usage"], json!({"input_tokens": 100, "output_tokens": 20, "estimated": false}));
    assert_eq!(meta["budget"]["tokens_today"], 120);
    assert_eq!(meta["budget"]["calls_today"], 1);
    assert_eq!(meta["budget"]["tokens_per_day"], Limits::default().tokens_per_day);
    // The app never sees the provider, the model id or the key.
    let text = r.to_string();
    for secret in ["deepseek", "v4-flash", KEY, "api.deepseek.com"] {
        assert!(!text.contains(secret), "{secret} leaked: {text}");
    }
    // The request carried the task, the schema and the input, and the key
    // only in its header.
    let (url, headers, body) = rig.fake.seen().pop().unwrap();
    assert!(url.ends_with("/chat/completions"), "{url}");
    assert!(headers.iter().any(|(k, v)| k == "Authorization" && v.ends_with(KEY)));
    assert!(!body.to_string().contains(KEY));
    let system = body["messages"][0]["content"].as_str().unwrap();
    assert!(system.contains("short title") && system.contains("\"maxItems\":3") && system.contains("Do not include any URL"));
    assert!(body["messages"][1]["content"].as_str().unwrap().contains("oat milk"));
    // The ledger is on disk under the host dir.
    assert!(rig.dir.join(".host/model/ledger.json").is_file());
}

#[test]
fn no_output_token_cap_is_sent() {
    let rig = Rig::new("nocap", vec![deepseek("deepseek-v4-flash")]);
    rig.fake.says(GOOD);
    rig.complete(args()).unwrap();
    let (_, _, body) = rig.fake.seen().pop().unwrap();
    let obj = body.as_object().unwrap();
    for cap in ["max_tokens", "max_completion_tokens", "max_output_tokens"] {
        assert!(!obj.contains_key(cap), "{cap} sent: {body}");
    }
    // Anthropic's API requires max_tokens: the model's own catalog maximum,
    // not a cap of the host's.
    let rig2 = { drop(rig); Rig::new("nocap-anthropic", vec![anthropic("claude-haiku-4-5-20251001")]) };
    rig2.fake.answer(Ok((200, json!({"content": [{"type": "text", "text": GOOD}], "usage": {"input_tokens": 5, "output_tokens": 5}}).to_string())));
    rig2.complete(args()).unwrap();
    let (url, _, body) = rig2.fake.seen().pop().unwrap();
    assert!(url.ends_with("/v1/messages"));
    assert_eq!(body["max_tokens"], 64000);
}

#[test]
fn a_reply_failing_the_schema_is_retried_once_then_refused() {
    let rig = Rig::new("schema-fail", vec![deepseek("deepseek-v4-flash")]);
    rig.fake.says(r#"{"title":"Errands"}"#).says(r#"{"title":"Errands","tags":"home"}"#);
    let err = rig.complete(args()).unwrap_err();
    assert!(err.starts_with("invalid_output: "), "{err}");
    assert!(err.contains("schema"), "{err}");
    let seen = rig.fake.seen();
    assert_eq!(seen.len(), 2, "one retry, no more");
    // The retry says why.
    assert!(seen[1].2["messages"][1]["content"].as_str().unwrap().contains("missing \\\"tags\\\"") || seen[1].2["messages"][1]["content"].as_str().unwrap().contains("missing \"tags\""));
    // Both attempts are charged.
    assert_eq!(rig.host.budget(APP).tokens_today, 240);
}

#[test]
fn invalid_json_then_a_good_reply_passes_on_the_retry() {
    let rig = Rig::new("retry", vec![deepseek("deepseek-v4-flash")]);
    rig.fake.says("Sure! Here is the JSON you asked for.").says(GOOD);
    let r = rig.complete(args()).unwrap();
    assert_eq!(r["meta"]["attempts"], 2);
    assert_eq!(r["meta"]["usage"]["input_tokens"], 200);
    assert!(rig.fake.seen()[1].2["messages"][1]["content"].as_str().unwrap().contains("not valid JSON"));
}

#[test]
fn a_url_in_the_reply_is_refused_unless_the_app_allows_urls() {
    let rig = Rig::new("url", vec![deepseek("deepseek-v4-flash")]);
    let with_url = r#"{"title":"See https://evil.example","tags":[]}"#;
    rig.fake.says(with_url).says(r#"{"title":"www.evil.example","tags":[]}"#);
    let err = rig.complete(args()).unwrap_err();
    assert!(err.starts_with("invalid_output: ") && err.contains("URL"), "{err}");
    rig.fake.says(with_url);
    let mut a = args();
    a["allow_urls"] = json!(true);
    let r = rig.complete(a).unwrap();
    assert_eq!(r["output"]["title"], "See https://evil.example");
    // Without the URL rule, the prompt does not forbid them either.
    let (_, _, body) = rig.fake.seen().pop().unwrap();
    assert!(!body["messages"][0]["content"].as_str().unwrap().contains("Do not include any URL"));
}

#[test]
fn an_oversized_reply_is_refused() {
    let rig = Rig::new("size", vec![deepseek("deepseek-v4-flash")]);
    let huge = json!({"title": "x", "tags": ["y".repeat(OUTPUT_MAX)]}).to_string();
    rig.fake.says(&huge).says(&huge);
    let err = rig.complete(args()).unwrap_err();
    assert!(err.starts_with("too_large: "), "{err}");
    // Request caps too: the input and the schema.
    let mut a = args();
    a["input"] = json!("z".repeat(complete::INPUT_MAX + 1));
    assert!(rig.complete(a).unwrap_err().starts_with("bad_request: input is"));
    let mut a = args();
    a["schema"] = json!({"type": "string", "description": "d".repeat(complete::schema::MAX_SCHEMA_BYTES)});
    assert!(rig.complete(a).unwrap_err().starts_with("bad_request: schema:"));
}

#[test]
fn a_missing_or_unsupported_schema_is_a_bad_request() {
    let rig = Rig::new("bad", vec![deepseek("deepseek-v4-flash")]);
    let mut a = args();
    a.as_object_mut().unwrap().remove("schema");
    assert_eq!(rig.complete(a).unwrap_err(), "bad_request: schema is required");
    let mut a = args();
    a["schema"] = json!({"type": "string", "pattern": "^x"});
    assert!(rig.complete(a).unwrap_err().contains("pattern"));
    let mut a = args();
    a["class"] = json!("huge");
    assert!(rig.complete(a).unwrap_err().starts_with("bad_request: class"));
    // An app cannot set the host's system prompt, or a token cap.
    for extra in ["system", "max_tokens", "model", "provider"] {
        let mut a = args();
        a[extra] = json!("x");
        assert!(rig.complete(a).unwrap_err().starts_with("bad_request:"), "{extra}");
    }
    assert!(rig.fake.seen().is_empty(), "nothing reached the provider");
}

#[test]
fn the_budget_runs_out_and_says_so() {
    // Room for two calls (120 tokens each, as the fake reports) but not
    // for a third call's estimate.
    let estimate = Request::from_args(&args()).unwrap().estimate();
    let budget = 200 + estimate;
    let rig = Rig::with("budget", vec![deepseek("deepseek-v4-flash")], |o| o.limits(Limits { per_minute: 100, calls_per_day: 100, tokens_per_day: budget }));
    rig.fake.says(GOOD).says(GOOD);
    rig.complete(args()).unwrap();
    rig.complete(args()).unwrap();
    let err = rig.complete(args()).unwrap_err();
    assert!(err.starts_with("budget: ") && err.contains(&format!("240 of its {budget} tokens")), "{err}");
    assert_eq!(rig.fake.seen().len(), 2);
    assert_eq!(rig.call(APP, "budget", json!({})).unwrap()["tokens_left"], budget - 240);
    // The next UTC day starts again.
    rig.now.fetch_add(24 * 3600 * 1000, Ordering::SeqCst);
    rig.fake.says(GOOD);
    assert!(rig.complete(args()).is_ok());
}

#[test]
fn daily_calls_run_out_too() {
    let rig = Rig::with("calls", vec![deepseek("deepseek-v4-flash")], |o| o.limits(Limits { per_minute: 100, calls_per_day: 1, tokens_per_day: 1_000_000 }));
    rig.fake.says(GOOD);
    rig.complete(args()).unwrap();
    let err = rig.complete(args()).unwrap_err();
    assert!(err.starts_with("budget: ") && err.contains("1 model calls for today"), "{err}");
}

#[test]
fn the_rate_limit_holds_per_app() {
    let rig = Rig::with("rate", vec![deepseek("deepseek-v4-flash")], |o| o.limits(Limits { per_minute: 2, calls_per_day: 100, tokens_per_day: 1_000_000 }));
    rig.fake.says(GOOD).says(GOOD).says(GOOD);
    rig.complete(args()).unwrap();
    rig.complete(args()).unwrap();
    let err = rig.complete(args()).unwrap_err();
    assert!(err.starts_with("rate: ") && err.contains("2 model calls a minute"), "{err}");
    rig.now.fetch_add(61_000, Ordering::SeqCst);
    assert!(rig.complete(args()).is_ok());
}

#[test]
fn no_provider_is_named_as_such() {
    let rig = Rig::new("none", Vec::new());
    let err = rig.complete(args()).unwrap_err();
    assert!(err.starts_with("no_provider: "), "{err}");
    // A refused call costs nothing.
    assert_eq!(rig.host.budget(APP).calls_today, 0);
}

#[test]
fn an_app_without_the_capability_is_refused_before_anything_else() {
    let rig = Rig::new("cap", vec![deepseek("deepseek-v4-flash")]);
    for method in ["complete", "budget"] {
        let err = rig.call("com.example.other", method, args()).unwrap_err();
        assert!(err.starts_with("capability: "), "{err}");
    }
    assert!(rig.fake.seen().is_empty());
}

/// The Card runner's isolate gate (Makepad's `splash_policy`, which App Hub
/// feeds each app's resolved policy) lets `model.*` out only for an app whose
/// policy lists `model`: `llm` or a neighbouring name is not enough, and
/// `model` grants nothing else. Behind it, the service's default grant reads
/// the same manifest, so an app the gate admits is served and one it would
/// refuse is refused again here.
#[test]
fn the_card_runner_gate_and_the_service_agree_on_the_model_capability() {
    use octosense_appstore::makepad_widgets::splash_policy::{service_allowed, set_policy_for_heap};
    let without = NEXT.fetch_add(2, Ordering::Relaxed);
    let with = without + 1;
    set_policy_for_heap(without, vec!["storage".into(), "llm".into(), "models".into()], Vec::new(), None);
    for method in ["model.complete", "model.budget"] {
        assert!(service_allowed(without, method).is_err(), "{method}");
    }
    set_policy_for_heap(with, vec!["model".into()], Vec::new(), None);
    for method in ["model.complete", "model.budget"] {
        assert!(service_allowed(with, method).is_ok(), "{method}");
    }
    assert!(service_allowed(with, "llm.list").is_err(), "model grants nothing else");

    // The service's own check, the default one, over manifests where App Hub
    // puts them: the granted app is served, the other refused.
    let rig = Rig::with("gate", vec![deepseek("deepseek-v4-flash")], |o| o.grants(complete::manifest_grants));
    let write = |id: &str, caps: &[&str]| {
        let path = rig.dir.join(id).join("bundle/manifest.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, json!({"id": id, "capabilities": caps}).to_string()).unwrap();
    };
    write(APP, &["storage", "model"]);
    write("com.example.other", &["storage", "llm"]);
    assert!(rig.call(APP, "budget", json!({})).is_ok());
    let err = rig.call("com.example.other", "budget", json!({})).unwrap_err();
    assert!(err.starts_with("capability: "), "{err}");
    assert!(rig.fake.seen().is_empty());
}

#[test]
fn the_default_grant_reads_the_apps_own_manifest() {
    let root = std::env::temp_dir().join(format!("model-grants-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let host_dir = root.join(".host");
    let write = |path: PathBuf, caps: &[&str], id: &str| {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, json!({"id": id, "capabilities": caps}).to_string()).unwrap();
    };
    write(root.join("com.a/bundle/manifest.json"), &["model"], "com.a");
    write(root.join("com.b/bundle/manifest.json"), &["storage", "llm"], "com.b");
    write(root.join(".system/os.notes/0123456789abcdef/manifest.json"), &["model"], "os.notes");
    // A manifest that names another id grants nothing.
    write(root.join("com.c/bundle/manifest.json"), &["model"], "com.a");
    assert!(complete::manifest_grants("com.a", &host_dir));
    assert!(!complete::manifest_grants("com.b", &host_dir));
    assert!(complete::manifest_grants("os.notes", &host_dir));
    assert!(!complete::manifest_grants("com.c", &host_dir));
    assert!(!complete::manifest_grants("../com.a", &host_dir));
    assert!(!complete::manifest_grants("com.missing", &host_dir));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_failing_provider_falls_back_in_the_persons_order_and_classes_come_first() {
    // The person's order: a strong model, then two fast ones.
    let rig = Rig::new("fallback", vec![deepseek("deepseek-v4-pro"), deepseek("deepseek-v4-flash"), anthropic("claude-haiku-4-5-20251001")]);
    rig.fake.answer(Ok((503, r#"{"error":{"message":"overloaded"}}"#.into()))).answer(Err("connection refused".into()));
    rig.fake.says(GOOD);
    // "fast": the two fast models first (in the person's order), then the strong one.
    let r = rig.complete(args()).unwrap();
    assert_eq!(r["meta"]["class"], "strong");
    let urls: Vec<String> = rig.fake.seen().iter().map(|s| s.0.clone()).collect();
    let models: Vec<String> = rig.fake.seen().iter().map(|s| s.2["model"].as_str().unwrap().to_string()).collect();
    assert_eq!(models, ["deepseek-v4-flash", "claude-haiku-4-5-20251001", "deepseek-v4-pro"], "{urls:?}");
    // Every provider down: the error says so without naming one.
    rig.fake.answer(Ok((401, r#"{"error":{"message":"bad key"}}"#.into()))).answer(Err("dns".into())).answer(Err("dns".into()));
    let err = rig.complete(args()).unwrap_err();
    assert!(err.starts_with("provider: "), "{err}");
    for name in ["deepseek", "anthropic", "claude", KEY] {
        assert!(!err.contains(name), "{err}");
    }
}

#[test]
fn a_strong_request_prefers_a_strong_model() {
    let rig = Rig::new("strong", vec![deepseek("deepseek-v4-flash"), deepseek("deepseek-v4-pro")]);
    rig.fake.says(GOOD);
    let mut a = args();
    a["class"] = json!("strong");
    let r = rig.complete(a).unwrap();
    assert_eq!((r["meta"]["class"].as_str(), r["meta"]["requested"].as_str()), (Some("strong"), Some("strong")));
    assert_eq!(rig.fake.seen()[0].2["model"], "deepseek-v4-pro");
}

#[test]
fn the_profile_is_the_provider_source_and_a_keyless_provider_is_skipped() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("model-profile-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let vault = Arc::new(MemoryVault::default());
    let path = octosense_llm_config::profile::profile_path(&dir);
    let mut store = ProfileStore::open(path.clone(), vault.clone()).unwrap();
    let with_key = Provider::new("deepseek", Some("deepseek-v4-flash".into()));
    let mut keys = BTreeMap::new();
    keys.insert(with_key.key_env.clone(), KEY.to_string());
    store.save(vec![Provider::new("anthropic", None), with_key], &keys).unwrap();
    let providers = complete::ProfileProviders { path, vault };
    let got = providers.candidates().unwrap();
    assert_eq!(got.len(), 1, "the provider with no key is left out");
    assert_eq!((got[0].provider.family.as_str(), got[0].key.as_deref()), ("deepseek", Some(KEY)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_host_caller_shares_the_same_ledger() {
    // The toolbox's ModelClient adapter: its own system prompt and a user
    // document, through the same host (and so the same budget).
    let rig = Rig::new("adapter", vec![deepseek("deepseek-v4-flash")]);
    rig.fake.says(GOOD);
    let host = complete::host().expect("registered");
    let request = Request {
        class: complete::Class::Fast,
        task: String::new(),
        input: json!("{\"query\":\"x\"}"),
        schema: args()["schema"].clone(),
        allow_urls: true,
        system: Some("You translate a research query.".into()),
    };
    let done = host.complete(APP, request).unwrap();
    assert_eq!(done.output["title"], "Errands");
    assert_eq!(rig.host.budget(APP).calls_today, 1);
    let (_, _, body) = rig.fake.seen().pop().unwrap();
    assert!(body["messages"][0]["content"].as_str().unwrap().starts_with("You translate a research query."));
    assert_eq!(body["messages"][1]["content"], "{\"query\":\"x\"}");
    assert_eq!(Code::Budget.as_str(), "budget");
}
