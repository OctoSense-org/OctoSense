//! The two-lane scenario end to end (ADR 0004 §6, §8): the system agent and
//! the person work with ONE app's agent at the same time, against a REAL
//! octos kernel (the pinned rev), a scripted model
//! (`crates/app-peers/tests/fixtures/mock_agent_llm.py`, the `SCN_*`
//! rules), and the shell's own pieces: the app-peers broker with the shell
//! as its tool host ([`super::ShellToolHost`]), the relay ([`super::pump`]),
//! the approval router and its sheet model, the question request model
//! ([`crate::questions`]) and the approval surface's press handler
//! ([`crate::approvals::view::press`], what a tap on the drawn sheet runs).
//!
//! **The app.** A test-only system app with the id `os.news` (App Hub's
//! capability list is closed, so the fixture reuses News's `news` family),
//! registered from a pack built here, admitted by App Hub's own loader. Its
//! `tools.json` has one read tool (`news.lookup`) and one that reaches
//! outside the device (`news.share`: `risk: destructive`, `confirm: host`;
//! octos gates destructive and outward tools alike, and App Hub's
//! `outward` flag came later); both run on a
//! recording `news` host service through the real script-app executor
//! (`script_apps::HostServiceExecutor`), wrapped to count the calls it
//! runs. Its agent keeps octos's `ask_user_question`, from its manifest's
//! `agent.tools` (App Hub's one kernel tool for a contained app), through the
//! script-app path (G3) into the peer's `generic_tools`; its person-lane
//! question is shown and answered in the shell's "Ask News" panel.
//!
//! **The lanes.** The system agent's lane is a real `peer/input`: a turn on
//! the system session makes the model call `peer_send_input`, the kernel
//! delivers `peer/input` to the broker, which starts the peer's turn. The
//! person's lane is the app's conversation (`open_conversation`, a
//! `share_history` request context) with a `TurnFrom { trigger: Person }`.
//!
//! **The tests.** The main scenario, then four variants: (a) nobody answers,
//! (b) the person's Stop, (c) a glance card's own action, (d) a Talk to
//! Octos client. Each says what it proves.
//!
//! **Found by it** (the fixes are in the same change): a turn stopped from
//! the app's own conversation left its approval on the shell's sheet until
//! the deadline (the broker now withdraws it,
//! `ToolHost::host_tool_approval_closed`); and the shell's router, counting
//! whole seconds, could deny an approval as expired just before the
//! broker's own deadline, which then took it for an answer and never freed
//! the stuck turn (an expiry deny is now marked as one,
//! `ApprovalAnswer::expired`). Fixed in octos (octos#2644): a lane's rows
//! were shared with the other lane only once its turn ended; a running
//! turn's request and status are now shown too
//! (`crates/app-peers/tests/real_kernel.rs`,
//! `a_running_turns_request_is_shown_to_the_other_lane`, and 3c below).
//!
//! What is not the real thing, and why:
//! - the person's taps go through the approval surface's press handler, not
//!   a pointer event on a drawn window (there is no window in a test);
//! - the interactive glance card (variant c) runs in a real Splash isolate
//!   under the app's resolved policy and its request leaves through App
//!   Hub's Card runner pump, but the tap is a typed key on a card widget
//!   whose handler makes the request (no layout, so no button to hit);
//! - time: the prompt deadline and the grace are seconds, not minutes.
//!
//! Runs when `OCTOS_SCENARIO_TEST_KERNEL` names an `octos` binary built at
//! the pinned revision (`python3 tools/kernel-artifact.py --host`); says so
//! and passes without one. The tests share the process's shell state
//! (approvals, relay, questions), so they run one at a time and apart from
//! the rest of the shell's tests:
//!
//! ```sh
//! OCTOS_SCENARIO_TEST_KERNEL=<octos> cargo test --locked --features mobile-apps -p octosense-shell --lib host_tools::scenario_tests -- --test-threads=1 --nocapture   # from phone/
//! ```

use std::collections::BTreeSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::ai_host::app_peers::broker::{Broker, BrokerConfig, ToolHostHandle};
use crate::ai_host::app_peers::connectors::CoreConnector;
use crate::ai_host::app_peers::host_tools::{CallOrigin, HostToolCall, ToolExecutor, ToolHost, ToolReply};
use crate::ai_host::app_peers::{ContextEvent, ContextOp, ContextSpec, Deployment, OctosAppService, OctosContext, TurnTrigger, OCTOS_SERVICES};
use crate::ai_host::kernel::{Core, Options};
use crate::approvals::sheet::{Answer, Place};
use crate::approvals::view::{press, Hit};
use crate::approvals::{Caller, RequestId, Trigger};
use crate::questions::{Conversation, Origin, State};
use octosense_appstore::services::{HostService, Replier, ServiceCall, ServiceHost};

/// The fixture app, its peer (a script app's is `card.<id>`) and label.
const APP: &str = "os.news";
const PEER_APP: &str = "card.os.news";
const LABEL: &str = "News";
const SYSTEM: &str = "_main:api:octosense#system";
/// The person's conversation handle's client.
const CHAT: &str = "news-chat";

/// What the scripted model passes `news_share` (`SCN_SHARE_ARGS`).
fn share_args() -> Value {
    json!({"story_id": "s-42", "to": "team@example.org", "note": "Quarterly numbers"})
}

/// One scenario at a time: the approval router, the relay and the question
/// model are the process's.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn kernel() -> Option<PathBuf> {
    let program = std::env::var_os("OCTOS_SCENARIO_TEST_KERNEL").map(PathBuf::from);
    if program.is_none() {
        eprintln!("OCTOS_SCENARIO_TEST_KERNEL is not set: skipping the two-lane scenario");
    }
    program
}

struct Model(std::process::Child, u16);
impl Drop for Model {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_model(log: &Path) -> Model {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app-peers/tests/fixtures/mock_agent_llm.py");
    let mut child = std::process::Command::new("python3")
        .arg(script)
        .env("MOCK_LLM_TOOLS_LOG", log)
        .env("MOCK_SCN_STUCK_SECS", "60")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("python3 for the scripted model");
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    Model(child, line.trim().parse().expect("model port"))
}

fn write_profile(core_dir: &Path, port: u16) {
    let dir = core_dir.join("profiles");
    std::fs::create_dir_all(&dir).unwrap();
    let profile = json!({
        "id": "_main", "name": "Main", "enabled": true,
        "created_at": "2026-09-29T00:00:00Z", "updated_at": "2026-09-29T00:00:00Z",
        "config": {"llm": {"primary": {"family_id": "local", "model_id": "mock-model",
            "route": {"base_url": format!("http://127.0.0.1:{port}/v1"), "api_type": "openai"}}}}
    });
    std::fs::write(dir.join("_main.json"), serde_json::to_vec_pretty(&profile).unwrap()).unwrap();
}

// ------------------------------------------------------------ the app

/// The fixture's `tools.json`: a read tool and an outward one the host
/// confirms, both on the app's host service.
fn tools_json() -> Value {
    json!({"schema": 1, "tools": [
        {"name": "news.lookup", "description": "Look up one collected story by its id.",
         "input_schema": {"type": "object", "properties": {"id": {"type": "string", "maxLength": 64}}, "required": ["id"], "additionalProperties": false},
         "output_schema": {"type": "object", "properties": {"id": {"type": "string"}, "title": {"type": "string"}}, "required": ["id"]},
         "risk": "read", "implemented_by": "host-service"},
        {"name": "news.share", "description": "Share a story with someone outside the device, with a note. It reaches past the app, so the person approves each call.",
         "input_schema": {"type": "object", "properties": {
             "story_id": {"type": "string", "maxLength": 64}, "to": {"type": "string", "maxLength": 200}, "note": {"type": "string", "maxLength": 500}},
             "required": ["story_id", "to"], "additionalProperties": false},
         "output_schema": {"type": "object", "properties": {"shared": {"type": "boolean"}, "id": {"type": "string"}}, "required": ["shared"]},
         "risk": "destructive", "confirm": "host", "implemented_by": "host-service"}
    ]})
}

/// Register the fixture as a system app, packed from a bundle made here
/// (the manifest carries the bundle's digest, as App Hub packs it).
fn register_app(dir: &Path) {
    let bundle = dir.join("fixture-bundle");
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::write(bundle.join("main.splash"), "View{}").unwrap();
    std::fs::write(bundle.join("tools.json"), serde_json::to_vec_pretty(&tools_json()).unwrap()).unwrap();
    let digest = octosense_app_policy::digest_dir(&bundle).unwrap();
    let manifest = json!({
        "schema": 1, "id": APP, "version": "1", "name": LABEL,
        "integrity": {"bundle_blake3": digest},
        "capabilities": ["storage", "glance", "news"],
        "agent": {"profile": "read-only", "tools": ["ask_user_question"], "model": {"needs": ["tool_calling"]}}
    });
    std::fs::write(bundle.join("manifest.json"), serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    let pack = serde_json::to_string(&octosense_app_hub::pack::pack_dir(&bundle).unwrap()).unwrap();
    octosense_appstore::system::register_system_app(octosense_appstore::system::SystemApp { id: APP, name: LABEL, pack: Box::leak(pack.into_boxed_str()), assets: &[] });
}

/// One call the `news` host service answered.
#[derive(Clone, Debug)]
struct Served {
    app_id: String,
    service: String,
    args: Value,
    from_sheet: bool,
}

/// The fixture's `news` host service: records every call, answers it.
struct RecordingNews(Arc<Mutex<Vec<Served>>>);

impl HostService for RecordingNews {
    fn family(&self) -> &'static str {
        "news"
    }
    fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
        self.0.lock().unwrap().push(Served { app_id: call.app_id.clone(), service: call.service.clone(), args: call.args.clone(), from_sheet: call.from_sheet });
        let n = self.0.lock().unwrap().len();
        reply.send(match call.method() {
            "share" => Ok(json!({"shared": true, "id": format!("share-{n}")})),
            "lookup" => Ok(json!({"id": call.args["id"], "title": "Quarterly numbers"})),
            other => Err(format!("no method {other}")),
        });
    }
}

/// The app's executor (the real script-app one), counting what it runs.
struct Counting {
    inner: Arc<dyn ToolExecutor>,
    ran: Arc<Mutex<Vec<HostToolCall>>>,
}

impl ToolExecutor for Counting {
    fn execute(&self, call: HostToolCall, reply: ToolReply) {
        self.ran.lock().unwrap().push(call.clone());
        self.inner.execute(call, reply);
    }
    fn cancel(&self, call_id: &str) {
        self.inner.cancel(call_id);
    }
}

// ------------------------------------------------------------ the rig

struct Scenario {
    broker: Broker,
    core: Core,
    _model: Model,
    dir: PathBuf,
    core_dir: PathBuf,
    offered_log: PathBuf,
    slug: String,
    peer_session: String,
    /// What the app's host service answered (agent calls and card calls).
    served: Arc<Mutex<Vec<Served>>>,
    /// What the relay handed the app's executor (agent calls only).
    executed: Arc<Mutex<Vec<HostToolCall>>>,
    _one: MutexGuard<'static, ()>,
}

/// The prompt deadline and grace a variant runs with.
#[derive(Clone, Copy)]
struct Deadlines {
    prompt: Duration,
    grace: Duration,
}

impl Scenario {
    fn start(tag: &str, deadlines: Option<Deadlines>, external: bool) -> Option<Scenario> {
        let program = kernel()?;
        let one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("octosense-two-lane-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let offered_log = dir.join("offered.jsonl");
        let model = start_model(&offered_log);
        let core_dir = dir.join("octos-home/.octos");
        write_profile(&core_dir, model.1);
        if external {
            // The web client's origin, as Setup › Talk to Octos saves it.
            std::fs::write(core_dir.join("web-client-origin.txt"), WEB).unwrap();
        }
        let core = Core::new(Options::default().core_dir(&core_dir).program(&program));
        if external {
            // Talk to Octos on before anything connects: turning it on
            // restarts the kernel as the host-managed server.
            core.set_external_access(true).expect("Talk to Octos on");
        }

        // The shell's approvals, fresh for this scenario, with the relay's
        // decisions going back to the relay (what `host_tools::init` does).
        crate::approvals::init(&dir.join("home"));
        crate::approvals::set_relay(Box::new(super::DecisionRelay));
        crate::approvals::with(|a| {
            // The person allowed News's agent (the first-use sheet).
            a.consent.set(&crate::approvals::rules::ApprovalGesture::sheet_tap(), APP, true, crate::approvals::now());
            if let Some(d) = deadlines {
                a.router.sheet_expiry_s = d.prompt.as_secs();
            }
        });
        if let Some(d) = deadlines {
            crate::questions::set_deadline_for_tests(Some(d.prompt.as_secs()));
        } else {
            crate::questions::set_deadline_for_tests(None);
        }

        // App Hub: the fixture as a system app, its recording host service,
        // and its agent block loaded the way the shell loads a script app's.
        octosense_appstore::set_data_root(dir.join("apps"));
        register_app(&dir);
        let served: Arc<Mutex<Vec<Served>>> = Arc::default();
        octosense_appstore::services::register_host_service(Box::new(RecordingNews(served.clone())));
        super::script_apps::load(APP).expect("the fixture's agent block is admitted");
        let root = dir.join("apps");
        let system = octosense_appstore::system::system_app(APP).unwrap();
        let bundle = octosense_appstore::system::prepare(&root, &system).expect("the fixture unpacks").0;
        let loaded = super::script_apps::from_bundle(&bundle).unwrap();
        // Its agent keeps `ask_user_question` from its manifest: App Hub
        // admits that one kernel tool for a contained app, and the
        // script-app path hands it to the peer's `generic_tools`.
        assert_eq!(loaded.generic, ["ask_user_question"]);
        assert_eq!(super::with_relay(|r| r.catalog.generic(APP, false)), ["ask_user_question"]);
        let executed: Arc<Mutex<Vec<HostToolCall>>> = Arc::default();
        let inner = super::script_apps::HostServiceExecutor { app: APP.into(), tools: loaded.host_service_tools, families: loaded.families, host_dir: root.join(".host") };
        super::set_executor(APP, Some(Arc::new(Counting { inner: Arc::new(inner), ran: executed.clone() })));

        // News's peer, with the shell as its tool host.
        let services: BTreeSet<String> = OCTOS_SERVICES.iter().map(|s| s.to_string()).collect();
        let mut cfg = BrokerConfig::new(Deployment::Hosted, "_main", SYSTEM, PEER_APP, LABEL, services);
        cfg.state_dir = Some(dir.join("host-state"));
        cfg.tool_host = Some(ToolHostHandle(Arc::new(super::ShellToolHost) as Arc<dyn ToolHost>));
        if let Some(d) = deadlines {
            cfg.prompt_deadline = d.prompt;
            cfg.expiry_grace = d.grace;
        }
        let broker = Broker::new(cfg, Arc::new(CoreConnector::shared(core.clone())));
        broker.set_account(Some(crate::ai_host::contained::ACCOUNT));
        broker.bind().expect("News's peer is bound and its tools registered");
        let (slug, peer_session) = broker.peer().expect("News's peer");
        Some(Scenario { broker, core, _model: model, dir, core_dir, offered_log, slug, peer_session, served, executed, _one: one })
    }

    /// Run the shell's UI-thread work (the relay, the router's and the
    /// question model's once-a-second tick) until `f` says done.
    fn until<T>(&self, what: &str, secs: u64, mut f: impl FnMut() -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(secs);
        let mut tick = Instant::now();
        loop {
            super::pump();
            if tick.elapsed() >= Duration::from_millis(500) {
                crate::approvals::tick();
                tick = Instant::now();
            }
            if let Some(v) = f() {
                return v;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(40));
        }
    }

    /// A turn of the system agent (its session is busy while the kernel
    /// ends its last turn or wakes it: retried).
    fn system_turn(&self, text: String) {
        let mut started = Err(String::new());
        for _ in 0..120 {
            started = self.broker.host_request("turn/start", json!({"session_id": SYSTEM, "turn_id": uuid_like(), "input": [{"kind": "text", "text": text}]}));
            if started.is_ok() {
                return;
            }
            super::pump();
            std::thread::sleep(Duration::from_millis(250));
        }
        panic!("the system agent's turn did not start: {started:?}");
    }

    fn transcript(&self, session: &str) -> String {
        self.broker.host_request("session/hydrate", json!({"session_id": session, "include": ["messages"]})).unwrap_or(Value::Null).to_string()
    }

    /// Every model request, in order.
    fn requests(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.offered_log).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }

    fn peer_dir(&self) -> PathBuf {
        self.core_dir.join("profiles/_main/data/peers").join(&self.slug)
    }

    /// The shell's pending approval for `news.share`, once the router has it.
    fn share_approval(&self) -> Option<(u64, RequestId)> {
        crate::approvals::with(|a| a.router.sheets().iter().find_map(|s| s.open_lines().find(|l| l.tool == "news.share").map(|l| (s.id, l.request.clone())))).flatten()
    }

    /// The open question of the app's conversation.
    fn open_question(&self) -> Option<crate::questions::Request> {
        crate::questions::open(&Conversation::App(APP.into())).into_iter().next()
    }

    /// Start the person's message in the app's conversation.
    fn person_says(&self, chat: &Arc<dyn OctosContext>, text: &str) -> mpsc::Receiver<Result<Value, String>> {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        chat.call(
            ContextOp::TurnFrom { text: text.into(), trigger: TurnTrigger::Person },
            Arc::new(move |e| {
                if let ContextEvent::Complete(r) = e {
                    let _ = tx.lock().unwrap().send(r);
                }
            }),
        )
        .expect("the person's message starts at once: no turn_in_progress, no queue");
        rx
    }

    fn conversation(&self) -> Arc<dyn OctosContext> {
        self.broker.open_conversation(ContextSpec { account: crate::ai_host::contained::ACCOUNT.into(), instance: CHAT.into(), services: OCTOS_SERVICES.iter().map(|s| s.to_string()).collect() }).expect("the app's conversation")
    }

    /// Both lanes running: the system agent's `peer/input` turn parked on
    /// the `news.share` approval, the person's turn parked on its question.
    /// Returns (the sheet, the approval, the peer's turn, the question).
    fn both_lanes_waiting(&self, chat: &Arc<dyn OctosContext>) -> (u64, RequestId, String, crate::questions::Request, mpsc::Receiver<Result<Value, String>>) {
        self.system_turn(format!("SCN_DELEGATE:{}", self.slug));
        let (sheet, approval) = self.until("the news.share approval on the shell's sheet", 120, || self.share_approval());
        let peer_turn = self.broker.peer_active_turn().expect("the system agent's peer/input turn runs");
        let person = self.person_says(chat, "SCN_ASK");
        let question = self.until("the person's lane asks its question", 120, || self.open_question());
        (sheet, approval, peer_turn, question, person)
    }
}

impl Drop for Scenario {
    fn drop(&mut self) {
        self.broker.release();
        self.core.shutdown_within(Duration::from_secs(5));
        crate::questions::set_deadline_for_tests(None);
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

fn uuid_like() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let hex = format!("{nanos:032x}");
    format!("{}-{}-4{}-8{}-{}", &hex[0..8], &hex[8..12], &hex[13..16], &hex[17..20], &hex[20..32])
}

fn wait_complete(s: &Scenario, rx: &mpsc::Receiver<Result<Value, String>>, what: &str) -> Result<Value, String> {
    s.until(what, 120, || rx.try_recv().ok())
}

/// A blackboard round's frontmatter, checked with exactly the rules of
/// octos's `peer_gather` receipt parser at the pin (`gathered_peer_result`,
/// `crates/octos-cli/src/api/ui_protocol_transport.rs`): `slug`, a known
/// `outcome`, `updated_unix`, a positive `turn`, then only `turn_id:`,
/// `origin:` and `context:` lines. The real parser runs too: the system
/// agent's `peer_gather` below leaves its receipt only for a round it
/// parsed.
fn round_header(text: &str, slug: &str) -> Result<Vec<(String, String)>, String> {
    let (header, _) = text.split_once("\n---\n\n").ok_or("no frontmatter")?;
    let mut lines = header.lines();
    if lines.next() != Some("---") || lines.next() != Some(format!("slug: {slug}").as_str()) {
        return Err(format!("not this peer's round: {header}"));
    }
    let outcome = lines.next().ok_or("no outcome")?;
    if !matches!(outcome, "outcome: completed" | "outcome: errored" | "outcome: interrupted" | "outcome: rate_limited") {
        return Err(format!("outcome: {outcome}"));
    }
    lines.next().and_then(|l| l.strip_prefix("updated_unix: ")).and_then(|v| v.parse::<u64>().ok()).ok_or("updated_unix")?;
    let turn = lines.next().and_then(|l| l.strip_prefix("turn: ")).and_then(|v| v.parse::<u32>().ok()).ok_or("turn")?;
    if turn == 0 {
        return Err("turn 0".into());
    }
    let mut fields = vec![("turn".to_string(), turn.to_string())];
    for line in lines {
        let (key, value) = line.split_once(": ").ok_or_else(|| format!("line {line:?}"))?;
        if !matches!(key, "turn_id" | "origin" | "context") {
            return Err(format!("unknown key {key}"));
        }
        fields.push((key.to_string(), value.to_string()));
    }
    Ok(fields)
}

fn field<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// The receipt octos keeps once the system agent's `peer_gather` read a
/// peer's current round: `(round, digest)`.
fn gather_receipt(s: &Scenario) -> Option<(u64, String)> {
    let peers = s.core_dir.join("profiles/_main/data/peers");
    let file = std::fs::read_dir(&peers).ok()?.flatten().find(|e| e.file_name().to_string_lossy().starts_with(".consumed-"))?;
    let record: Value = serde_json::from_str(&std::fs::read_to_string(file.path()).ok()?).ok()?;
    let receipt = &record["results"][&s.slug];
    Some((receipt["round"].as_u64()?, receipt["digest"].as_str()?.to_string()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// The system agent gathers the peer's blackboard; the kernel's own parser
/// must accept the current round for the receipt to name it.
fn gather_and_check(s: &Scenario, expect_origin: &str) -> String {
    let result = std::fs::read_to_string(s.peer_dir().join("result.md")).expect("the peer's current round");
    let fields = round_header(&result, &s.slug).unwrap_or_else(|e| panic!("{e}: {result}"));
    assert_eq!(field(&fields, "origin"), Some(expect_origin), "{result}");
    let round: u64 = field(&fields, "turn").unwrap().parse().unwrap();
    s.system_turn(format!("SCN_GATHER:{}", s.slug));
    let (got, digest) = s.until("octos's peer_gather receipt for the current round", 120, || gather_receipt(s).filter(|(r, _)| *r == round));
    assert_eq!(got, round);
    assert_eq!(digest, sha256_hex(result.as_bytes()), "the receipt is for these exact bytes");
    let text = s.until("the system agent read the round", 60, || Some(s.transcript(SYSTEM)).filter(|t| t.contains(&format!("origin: {expect_origin}"))));
    assert!(text.contains("GATHERED"), "{text}");
    result
}

// ------------------------------------------------------------ the scenario

/// The main scenario: the system agent hands News's agent a task whose
/// outward tool waits for approval; meanwhile the person talks to the same
/// agent in the app's conversation and is asked a question. Both lanes run
/// at once, see each other's recent rows, and finish when the person
/// answers and approves; the tool runs exactly once.
#[test]
fn the_system_agent_and_the_person_work_with_one_app_agent_at_once() {
    let Some(s) = Scenario::start("main", None, false) else { return };

    // 1. The system agent's peer/input turn calls news.share: an approval.
    s.system_turn(format!("SCN_DELEGATE:{}", s.slug));
    let (sheet, approval) = s.until("the news.share approval on the shell's sheet", 120, || s.share_approval());
    let peer_turn = s.broker.peer_active_turn().expect("the system agent's peer/input turn runs");
    let request = crate::approvals::with(|a| a.router.pending_request(&approval).cloned()).flatten().expect("the router holds it");
    assert_eq!((request.app.as_str(), request.tool.name.as_str()), (APP, "news.share"));
    assert_eq!(request.args, share_args(), "the exact arguments");
    assert_eq!(request.caller, Caller::OwnAgent { client: None }, "News's own agent calls its own tool");
    assert_eq!(request.context.trigger, Trigger::SystemAgent, "a turn the system agent started (peer/input)");
    assert_eq!(request.context.connection, crate::approvals::Connection::Host);
    let shown = crate::approvals::with(|a| a.router.sheets().iter().find(|x| x.id == sheet).cloned()).flatten().unwrap();
    assert_eq!(shown.place, Place::AppConversation { app: APP.into() }, "shown in News's conversation");
    let line = shown.line(&approval).unwrap();
    assert_eq!((line.heading(), line.caller.as_str()), ("News \u{00b7} news.share".to_string(), "News's agent"));
    assert!(line.args.iter().any(|a| a.contains("team@example.org")), "the sheet shows the arguments: {:?}", line.args);
    assert!(s.executed.lock().unwrap().is_empty() && s.served.lock().unwrap().is_empty(), "nothing ran before the approval");

    // 2. Meanwhile the person opens News's conversation and asks.
    let chat = s.conversation();
    let person = s.person_says(&chat, "SCN_ASK");
    let question = s.until("the person's lane asks its question", 120, || s.open_question());

    // 3a. Both lanes run at once: the approval still waits on the peer's
    // own turn, the person's turn asked in its own lane, nothing queued.
    assert_eq!(s.broker.peer_active_turn().as_deref(), Some(peer_turn.as_str()), "the system agent's turn is still running");
    assert_ne!(question.turn_id, peer_turn, "the person's turn is its own");
    assert!(crate::approvals::with(|a| a.router.is_pending(&approval)).unwrap(), "the approval is still pending");
    assert_eq!(s.broker.queued_inputs(), 0, "the person's message did not queue behind the peer's turn");
    assert!(person.try_recv().is_err(), "the person's turn is running");

    // 3c. The question is the person's, in News's conversation.
    assert_eq!(question.origin, Origin::Person);
    assert_eq!(question.conversation, Conversation::App(APP.into()));
    assert_eq!(question.client.as_deref(), Some(CHAT));
    assert!(question.context_id.is_some(), "asked in the person's context");
    assert_eq!(question.text(), "Who should see the summary?");
    assert_eq!(question.options(), ["Team", "Everyone"]);
    assert!(crate::questions::open(&Conversation::SystemChat).iter().all(|q| q.app != APP), "not in the system chat");

    // The system agent's turn is still running (parked on the approval), so
    // its rows are not in its transcript yet; octos shows the person's lane
    // the running turn instead (octos#2644): its request and what it waits
    // on (the tool's name only). 3b below checks the sharing both ways on
    // ended turns.
    let requests = s.requests();
    let asked = requests.iter().find(|r| r["own"].as_str().is_some_and(|u| u.contains("SCN_ASK"))).expect("the person's model request");
    let running = asked["shared"].to_string();
    assert!(running.contains("lane=\\\"system_agent\\\"") && running.contains("[from the system agent] SCN_TASK"), "{running}");
    assert!(running.contains("[turn status] still running, waiting for approval: news_share"), "{running}");
    assert!(!running.contains("Quarterly numbers"), "no tool arguments: {running}");

    // 4. The person answers in the shell's "Ask News" panel (G11: the
    // question of the app's conversation is shown there, not in the system
    // chat): their turn ends while the system agent's is still parked on
    // its approval.
    crate::app_chat::show_for_tests(crate::apps::AgentApp { id: APP.into(), name: LABEL.into(), octos: Vec::new(), manifest: json!({}), native: false });
    let panel = crate::app_chat::snapshot();
    let routed = panel
        .items
        .iter()
        .find_map(|i| match i {
            crate::system_chat::model::Item::Question { id, body, options, answered: None, .. } if body == "Who should see the summary?" => Some((id.clone(), options.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the panel shows the question: {:?}", panel.items));
    assert_eq!(routed, (format!("{}{}", crate::app_chat::ROUTED_PREFIX, question.id), vec!["Team".to_string(), "Everyone".to_string()]));
    crate::app_chat::answer_option(&routed.0, 1, "Team");
    crate::app_chat::close();
    let said = wait_complete(&s, &person, "the person's turn").expect("the person's turn completed");
    assert!(said["text"].as_str().unwrap_or("").starts_with("SCN AUDIENCE") && said["text"].to_string().contains("Team"), "{said}");
    assert_eq!(said["lane"], "person");
    assert_eq!(crate::questions::get(question.id).unwrap().state, State::Answered("Team".into()));
    assert!(crate::approvals::with(|a| a.router.is_pending(&approval)).unwrap(), "the system agent's lane still waits");

    // The person's round is on the blackboard, and octos's peer_gather
    // accepts it (the system agent's turn runs while its peer's waits).
    let person_round = gather_and_check(&s, "person");
    assert!(person_round.contains("SCN AUDIENCE"), "{person_round}");
    let person_fields = round_header(&person_round, &s.slug).unwrap();
    assert_eq!(field(&person_fields, "context"), question.context_id.as_deref(), "the round names the person's context");

    // 5. The person approves on the shell's sheet: the tool runs once.
    press(Hit::Answer { sheet, request: approval.clone(), answer: Answer::Once });
    let published = s.until("the system agent's lane finished", 120, || Some(s.transcript(&s.peer_session)).filter(|t| t.contains("SCN PUBLISHED")));
    assert!(published.contains("share-1"), "the host service's answer reached the peer's turn: {published}");
    s.until("the peer is free", 60, || s.broker.peer_active_turn().is_none().then_some(()));
    let entry = crate::approvals::with(|a| a.router.audit.all().iter().rev().find(|e| e.id == approval.0).cloned()).flatten().expect("audited");
    assert_eq!((entry.by.as_str(), entry.result.as_str(), entry.caller.as_str(), entry.trigger.as_str()), ("person", "approved", "own_agent", "system_agent"));

    // 3d/6. Exactly one run, with the exact arguments, from the peer/input turn.
    let ran = s.executed.lock().unwrap().clone();
    assert_eq!(ran.len(), 1, "{ran:?}");
    assert_eq!((ran[0].name.as_str(), ran[0].app.as_str()), ("news.share", APP));
    assert_eq!(ran[0].args, share_args());
    assert_eq!(ran[0].origin, CallOrigin::PeerInput);
    assert_eq!(ran[0].trigger, TurnTrigger::SystemAgent);
    let served = s.served.lock().unwrap().clone();
    assert_eq!(served.len(), 1, "{served:?}");
    assert_eq!((served[0].app_id.as_str(), served[0].service.as_str(), served[0].from_sheet), (APP, "news.share", false));
    assert_eq!(served[0].args, share_args());

    // The system agent's round, parsed by octos's own gather too.
    let system_round = gather_and_check(&s, "system_agent");
    assert!(system_round.contains("SCN PUBLISHED"), "{system_round}");

    // 3b. The person's next message is shown the system agent's lane:
    // its request and the tool's outcome, read-only, not the person's own.
    let person = s.person_says(&chat, "SHOW_SHARED");
    let seen = wait_complete(&s, &person, "the person's second turn").expect("completed");
    let seen = seen["text"].as_str().unwrap_or("").to_string();
    assert!(seen.contains("[from the system agent] SCN_TASK") && seen.contains("[the app agent] SCN PUBLISHED"), "{seen}");
    assert!(!seen.contains("SCN_ASK"), "only the other lane: {seen}");
    let requests = s.requests();
    let shown = requests.iter().rev().find(|r| r["own"].as_str().is_some_and(|u| u.contains("[from the person: News] SHOW_SHARED"))).expect("the person's model request");
    let shared = shown["shared"].to_string();
    assert!(shared.contains("lane=\\\"system_agent\\\"") && shared.contains("SCN_TASK"), "{shared}");

    // ...and the other way: the system agent's next turn on the peer is
    // shown the person's rows (and runs as the peer's next input).
    s.system_turn(format!("TELL_PEER_SHOW:{}", s.slug));
    let shown_to_agent = s.until("the peer's SHOW_SHARED turn", 120, || Some(s.transcript(&s.peer_session)).filter(|t| t.contains("SHARED ")));
    assert!(shown_to_agent.contains("[from the person: News] SCN_ASK"), "{shown_to_agent}");
    let requests = s.requests();
    let show = requests.iter().find(|r| r["own"].as_str().is_some_and(|u| u.contains("[from the system agent] SHOW_SHARED"))).expect("the peer's model request");
    let shared = show["shared"].to_string();
    assert!(shared.contains("lane=\\\"person\\\"") && shared.contains("SCN_ASK") && shared.contains("SCN AUDIENCE"), "{shared}");

    // 3e. Every round on the blackboard names its origin: the person's two
    // turns and the system agent's two, in the order they ended.
    let rounds_now = || -> Vec<(u32, Vec<(String, String)>)> {
        let mut rounds: Vec<(u32, Vec<(String, String)>)> = std::fs::read_dir(s.peer_dir())
            .unwrap()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let n: u32 = name.strip_prefix("result-")?.strip_suffix(".md")?.parse().ok()?;
                let text = std::fs::read_to_string(e.path()).unwrap();
                Some((n, round_header(&text, &s.slug).unwrap_or_else(|e| panic!("result-{n}.md: {e}: {text}"))))
            })
            .collect();
        rounds.sort_by_key(|(n, _)| *n);
        rounds
    };
    let rounds = s.until("four rounds on the blackboard", 60, || Some(rounds_now()).filter(|r| r.len() >= 4));
    let origins: Vec<&str> = rounds.iter().filter_map(|(_, f)| field(f, "origin")).collect();
    assert_eq!(origins, ["person", "system_agent", "person", "system_agent"], "{rounds:?}");
    assert!(rounds.iter().filter(|(_, f)| field(f, "origin") == Some("person")).all(|(_, f)| field(f, "context").is_some()), "a person's round names its context: {rounds:?}");

    // 3f. octos.session.history: both lanes merged by time, with speakers.
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    chat.call(ContextOp::History, Arc::new(move |e| {
        if let ContextEvent::Complete(r) = e {
            let _ = tx.lock().unwrap().send(r);
        }
    }))
    .unwrap();
    let history = wait_complete(&s, &rx, "the conversation's history").expect("history");
    let rows = history["messages"].as_array().cloned().unwrap_or_default();
    let at = |pred: &dyn Fn(&Value) -> bool| rows.iter().position(pred).unwrap_or_else(|| panic!("{rows:#?}"));
    let task = at(&|m| m["role"] == "user" && m["display_text"] == "SCN_TASK");
    let ask = at(&|m| m["role"] == "user" && m["display_text"] == "SCN_ASK");
    let audience = at(&|m| m["role"] == "assistant" && m["content"].as_str().is_some_and(|c| c.starts_with("SCN AUDIENCE")));
    let publish = at(&|m| m["role"] == "assistant" && m["content"].as_str().is_some_and(|c| c.starts_with("SCN PUBLISHED")));
    assert_eq!((rows[task]["lane"].clone(), rows[task]["speaker"].clone()), (json!("system_agent"), json!({"kind": "system_agent"})));
    assert_eq!((rows[ask]["lane"].clone(), rows[ask]["speaker"].clone()), (json!("person"), json!({"kind": "person", "label": LABEL})));
    assert_eq!((rows[audience]["lane"].clone(), rows[publish]["lane"].clone()), (json!("person"), json!("system_agent")));
    assert!(task < ask && ask < audience && audience < publish, "merged by time: task {task}, ask {ask}, audience {audience}, publish {publish}");
    assert!(rows.iter().all(|m| !m["content"].as_str().unwrap_or("").contains("<shared_history")), "the shared block is in neither transcript");
}

/// (a) Nobody answers: at a short deadline the shell's router denies the
/// approval with its reason and keeps the expired record; after the grace
/// the broker interrupts the peer's turn, still running (the scripted model
/// takes a minute to answer the refusal), and the system agent's next
/// queued input runs. Nothing ran. Whichever deadline fires first, the
/// router's (whole seconds) or the broker's, the grace frees the turn.
#[test]
fn an_unanswered_approval_expires_and_the_next_input_runs() {
    let deadlines = Deadlines { prompt: Duration::from_secs(4), grace: Duration::from_secs(5) };
    let Some(s) = Scenario::start("expiry", Some(deadlines), false) else { return };
    s.system_turn(format!("SCN_DELEGATE_STUCK:{}", s.slug));
    let (_, approval) = s.until("the news.share approval on the shell's sheet", 120, || s.share_approval());
    let stuck = s.broker.peer_active_turn().expect("the peer's turn runs");
    // The system agent's next input waits for the peer.
    s.system_turn(format!("TELL_PEER_AGAIN:{}", s.slug));
    s.until("the second input queued", 60, || (s.broker.queued_inputs() == 1).then_some(()));

    // The deadline: denied (never approved), with why, and kept visible.
    let expired = s.until("the approval expired", 60, || crate::approvals::with(|a| a.router.expired().iter().find(|e| e.id == approval).cloned()).flatten());
    assert_eq!(expired.reason, "no answer in 4 s");
    assert_eq!(expired.status(), "Expired: no answer in 4 s");
    assert_eq!((expired.heading.as_str(), expired.caller.as_str()), ("News \u{00b7} news.share", "News's agent"));
    let entry = crate::approvals::with(|a| a.router.audit.all().iter().rev().find(|e| e.id == approval.0).cloned()).flatten().expect("audited");
    assert_eq!((entry.by.as_str(), entry.result.as_str(), entry.reason.as_str()), ("expired", "denied", "expired: no answer in 4 s"));
    assert!(!crate::approvals::with(|a| a.router.is_pending(&approval)).unwrap());
    assert!(s.share_approval().is_none(), "withdrawn from the sheet");
    // The model heard the refusal and is still working (it sleeps).
    assert_eq!(s.broker.peer_active_turn().as_deref(), Some(stuck.as_str()), "the turn still runs after the denial");

    // The grace: the broker interrupts it, and the queued input runs.
    let second = s.until("the queued input ran", 90, || Some(s.transcript(&s.peer_session)).filter(|t| t.contains("SECOND_INPUT") && t.contains("ECHO:")));
    assert!(second.contains("ECHO: [from the system agent] SECOND_INPUT") || second.contains("ECHO: SECOND_INPUT"), "{second}");
    assert!(!second.contains("SCN STUCK DONE"), "the stuck turn was interrupted, not finished: {second}");
    let state = s.broker.host_request("turn/state/get", json!({"session_id": s.peer_session, "turn_id": stuck})).map(|r| r["state"].clone()).unwrap_or(Value::Null);
    assert_ne!(state, "running", "{state}");
    assert!(s.executed.lock().unwrap().is_empty() && s.served.lock().unwrap().is_empty(), "nothing ran");
    // Still visible until the person dismisses it.
    assert!(crate::approvals::with(|a| a.router.expired().iter().any(|e| e.id == approval)).unwrap());
    press(Hit::DismissExpired(approval.clone()));
    assert!(!crate::approvals::with(|a| a.router.expired().iter().any(|e| e.id == approval)).unwrap());
}

/// (b) The person's Stop on the shell's surface for News's conversation
/// stops BOTH running lanes, whoever started them: the approval is denied,
/// the question declined, and the turns it stopped are both listed.
#[test]
fn the_persons_stop_interrupts_both_lanes() {
    let Some(s) = Scenario::start("stop", None, false) else { return };
    let chat = s.conversation();
    let (_, approval, peer_turn, question, person) = s.both_lanes_waiting(&chat);

    // The Stop button (`Hit::StopAgent`) runs exactly this.
    let stopped = crate::approvals::stop_agent(APP);
    assert!(stopped.contains(&peer_turn), "the system agent's lane: {stopped:?}");
    assert!(stopped.contains(&question.turn_id), "the person's lane: {stopped:?}");
    assert_eq!(stopped.len(), 2, "{stopped:?}");
    let entry = crate::approvals::with(|a| a.router.audit.all().iter().rev().find(|e| e.id == approval.0).cloned()).flatten().expect("audited");
    assert_eq!((entry.by.as_str(), entry.result.as_str(), entry.reason.as_str()), ("person", "denied", "the person stopped the agent's turn"));
    assert_eq!(crate::questions::get(question.id).unwrap().state, State::Answered("(stopped)".into()));
    let ended = wait_complete(&s, &person, "the person's turn ended");
    eprintln!("[stop] the person's turn ended with {ended:?}");
    s.until("the peer's turn ended", 60, || s.broker.peer_active_turn().is_none().then_some(()));
    for turn in [&peer_turn] {
        let state = s.broker.host_request("turn/state/get", json!({"session_id": s.peer_session, "turn_id": turn})).map(|r| r["state"].clone()).unwrap_or(Value::Null);
        assert_ne!(state, "running", "{turn}: {state}");
    }
    assert!(s.executed.lock().unwrap().is_empty() && s.served.lock().unwrap().is_empty(), "nothing ran");

    // The app's own Stop (the conversation's `octos.turn.interrupt`) says
    // the same, per lane.
    let (_, approval, peer_turn, question, person) = s.both_lanes_waiting(&chat);
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    chat.call(ContextOp::Interrupt, Arc::new(move |e| {
        if let ContextEvent::Complete(r) = e {
            let _ = tx.lock().unwrap().send(r);
        }
    }))
    .unwrap();
    let reply = wait_complete(&s, &rx, "the conversation's Stop").expect("stopped");
    let turns = reply["turns"].as_array().cloned().unwrap_or_default();
    let lane_of = |turn: &str| turns.iter().find(|t| t["turn_id"] == turn).map(|t| t["lane"].clone());
    assert_eq!(lane_of(&peer_turn), Some(json!("system_agent")), "{reply}");
    assert_eq!(lane_of(&question.turn_id), Some(json!("person")), "{reply}");
    let _ = wait_complete(&s, &person, "the person's turn ended");
    s.until("the peer's turn ended", 60, || s.broker.peer_active_turn().is_none().then_some(()));
    // What the stopped turns asked is not left for the person to answer:
    // the question is closed, and the approval withdrawn from the sheet
    // (before this PR the sheet kept asking until its deadline).
    s.until("the question is withdrawn", 30, || (crate::questions::get(question.id).unwrap().state != State::Open).then_some(()));
    s.until("the approval is withdrawn", 30, || (!crate::approvals::with(|a| a.router.is_pending(&approval)).unwrap()).then_some(()));
    assert!(s.share_approval().is_none(), "no sheet asks for it");
    let entry = crate::approvals::with(|a| a.router.audit.all().iter().rev().find(|e| e.id == approval.0).cloned()).flatten().expect("audited");
    assert_eq!((entry.by.as_str(), entry.result.as_str()), ("withdrawn", "denied"));
    assert!(s.executed.lock().unwrap().is_empty() && s.served.lock().unwrap().is_empty(), "nothing ran");
}

/// (c) An interactive glance card of News, running under News's resolved
/// policy, calls News's host service through the Card runner's path: the
/// app's own action, not its agent's tool call, so no approval, no relay,
/// no kernel.
#[test]
fn a_glance_cards_action_is_the_apps_own_not_an_agent_tool_call() {
    let Some(s) = Scenario::start("card", None, false) else { return };
    use makepad_widgets::{Cx, Event, TextInputEvent};
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        crate::glance_card::script_mod(vm);
    });
    makepad_widgets::widget_async::register_splash_isolate_mod(|vm| {
        card_probe::script_mod(vm);
    });
    let mut tiles = crate::glance_card::GlanceTiles::default();
    tiles.open(&mut cx, "os.news/share", APP, true, &"probe := NewsShareProbe{}".into());
    // The person acts on the card; its handler makes the request.
    tiles.handle_event(&mut cx, &Event::TextInput(TextInputEvent { input: "s".into(), ..Default::default() }));
    for _ in 0..50 {
        tiles.handle_event(&mut cx, &Event::Signal);
        super::pump();
        if !s.served.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let served = s.served.lock().unwrap().clone();
    assert_eq!(served.len(), 1, "the card's request reached News's host service: {served:?}");
    assert_eq!((served[0].app_id.as_str(), served[0].service.as_str(), served[0].from_sheet), (APP, "news.share", false), "as News itself");
    assert_eq!(served[0].args, share_args());
    // Not an agent tool call: nothing reached the executor, the router, the
    // audit or the peer.
    std::thread::sleep(Duration::from_secs(1));
    super::pump();
    assert!(s.executed.lock().unwrap().is_empty(), "the relay ran nothing");
    assert_eq!(crate::approvals::with(|a| (a.router.pending(), a.router.sheets().len(), a.router.audit.all().len())).unwrap(), (0, 0, 0), "no approval raised");
    assert_eq!((s.broker.pending_prompts(), s.broker.calls_in_flight()), (0, 0));
    assert!(!s.transcript(&s.peer_session).contains("news_share"), "the agent made no call");
    assert!(s.requests().is_empty(), "no model was asked");
}

/// A card widget whose handler asks News's host service, from the card's
/// own isolate, as a card's tap handler would (see the module docs).
mod card_probe {
    use makepad_widgets::*;

    script_mod! {
        use mod.prelude.widgets.*
        mod.widgets.NewsShareProbe = set_type_default() do #(NewsShareProbe::register_widget(vm)) {}
        mod.prelude.widgets.NewsShareProbe = mod.widgets.NewsShareProbe
    }

    #[derive(Script, ScriptHook, Widget)]
    pub struct NewsShareProbe {
        #[deref]
        view: View,
    }

    impl Widget for NewsShareProbe {
        fn handle_event(&mut self, cx: &mut Cx, event: &Event, _: &mut Scope) {
            if let Event::TextInput(_) = event {
                cx.with_vm(|vm| {
                    script_eval!(vm, { mod.host.request("news.share", {story_id: "s-42" to: "team@example.org" note: "Quarterly numbers"}, nil) });
                });
            }
        }
        fn draw_walk(&mut self, _: &mut Cx2d, _: &mut Scope, _: Walk) -> DrawStep {
            DrawStep::done()
        }
    }
}

// ------------------------------------------------------------ outside

const WEB: &str = "http://localhost:4173";

/// (d) A Talk to Octos client (the external connection, as a browser
/// makes it) cannot open News's peer, read its pending approval, or answer
/// the approval or the question, with their live ids; it is sent neither.
/// The person then answers and approves on the shell as usual.
#[test]
fn an_external_client_cannot_see_or_answer_the_app_peers_prompts() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};
    let Some(s) = Scenario::start("external", None, true) else { return };
    let access = s.core.client_access().expect("the external endpoint");
    let chat = s.conversation();
    let (sheet, approval, _peer_turn, question, person) = s.both_lanes_waiting(&chat);
    let approval_id = approval.0.strip_prefix(super::APPROVAL_PREFIX).expect("a host_tool approval").to_string();

    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let (seen, answers) = rt.block_on(async {
        let mut req = format!("{}?ui_feature=state.session_hydrate.v1", access.endpoint()).into_client_request().unwrap();
        req.headers_mut().insert("Sec-WebSocket-Protocol", format!("octos-ui, octos.bearer.{}", access.token).parse().unwrap());
        req.headers_mut().insert("Origin", WEB.parse().unwrap());
        let (mut ws, _) = tokio_tungstenite::connect_async(req).await.expect("the external client connects");
        let mut seen: Vec<Value> = Vec::new();
        let call = |id: &'static str, method: &str, params: Value| (id, json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string());
        let mut answers = Vec::new();
        let requests = [
            call("open-system", "session/open", json!({"session_id": SYSTEM, "profile_id": "_main"})),
            call("open-peer", "session/open", json!({"session_id": s.peer_session, "profile_id": "_main"})),
            call("hydrate-peer", "session/hydrate", json!({"session_id": s.peer_session, "include": ["messages", "pending_approvals"]})),
            call("approve", "approval/respond", json!({"session_id": s.peer_session, "approval_id": approval_id, "decision": "approve"})),
            call("answer", "user_question/respond", json!({"session_id": question.session_id, "question_id": question.question_id, "answers": [{"selected_labels": ["Everyone"]}]})),
            call("hydrate-ctx", "session/hydrate", json!({"session_id": question.session_id, "include": ["messages", "pending_approvals"]})),
        ];
        for (id, frame) in requests {
            ws.send(Message::Text(frame)).await.unwrap();
            let reply = tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    match ws.next().await.expect("the external connection closed").expect("a frame") {
                        Message::Text(text) => {
                            let frame: Value = serde_json::from_str(&text).unwrap();
                            if frame["id"] == id {
                                return frame;
                            }
                            seen.push(frame);
                        }
                        Message::Ping(bytes) => ws.send(Message::Pong(bytes)).await.unwrap(),
                        _ => {}
                    }
                }
            })
            .await
            .expect("the kernel answers the external client");
            answers.push((id, reply));
        }
        // Whatever else arrives for a while: never the peer's prompts.
        let _ = tokio::time::timeout(Duration::from_secs(3), async {
            while let Some(Ok(message)) = ws.next().await {
                if let Message::Text(text) = message {
                    seen.push(serde_json::from_str(&text).unwrap_or(Value::Null));
                }
            }
        })
        .await;
        (seen, answers)
    });
    let reply = |id: &str| answers.iter().find(|(i, _)| *i == id).map(|(_, r)| r.clone()).unwrap();
    eprintln!("[external] {answers:#?}");
    assert!(reply("open-system").get("error").is_none(), "the external client talks to the system agent: {}", reply("open-system"));
    assert_eq!(reply("open-peer")["error"]["data"]["kind"], "host_owned_peer_session_denied", "{}", reply("open-peer"));
    for (id, kind) in [
        // Not the peer's transcript or its pending approvals...
        ("hydrate-peer", "host_owned_peer_session_denied"),
        // ...nor the person's lane, where the question was asked...
        ("hydrate-ctx", "host_owned_peer_session_denied"),
        // ...and neither prompt can be answered, by its live id.
        ("approve", "host_owned_peer_answer_denied"),
        ("answer", "host_owned_peer_answer_denied"),
    ] {
        assert_eq!(reply(id)["error"]["data"]["kind"], kind, "{id}: {}", reply(id));
    }
    let leaked = seen.iter().filter(|f| {
        let text = f.to_string();
        text.contains(&approval_id) || text.contains(&question.question_id) || f["method"] == "approval/requested" || f["method"] == "user_question/requested"
    });
    assert_eq!(leaked.count(), 0, "the external client was sent none of the peer's prompts: {seen:#?}");

    // Nothing it did counted: both still wait for the person.
    super::pump();
    assert!(crate::approvals::with(|a| a.router.is_pending(&approval)).unwrap(), "still pending");
    assert_eq!(crate::questions::get(question.id).unwrap().state, State::Open);
    assert!(s.executed.lock().unwrap().is_empty());
    press(Hit::QuestionOption { id: question.id, label: Some("Team".into()) });
    press(Hit::Answer { sheet, request: approval, answer: Answer::Once });
    let said = wait_complete(&s, &person, "the person's turn").expect("completed");
    assert!(said["text"].to_string().contains("Team"), "the person's answer, not the outside one: {said}");
    s.until("the system agent's lane finished", 120, || Some(s.transcript(&s.peer_session)).filter(|t| t.contains("SCN PUBLISHED")));
    assert_eq!(s.executed.lock().unwrap().len(), 1);
}
