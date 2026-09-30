//! The broker: one app's scoped assistant service over UI Protocol frames.
//!
//! One [`Broker`] serves one app (one module instance). It holds its own
//! connection to the kernel (through a [`Connector`]), and:
//!
//! - binds the app's peer: `peer/prepare` with the app/account memory
//!   namespace and `resume: true`, owned by the configured originator (the
//!   host's system agent session, or a standalone app's own root session);
//!   the kernel provisions the peer's workspace. A kernel without the
//!   host-owned app peer contract (octos UPCR-2026-034) is refused, never
//!   substituted by an ordinary privileged session;
//! - opens one kernel request context per client instance
//!   (`peer/context/open`), and closes it (`peer/context/close`) when the
//!   instance closes, the account changes or the app releases;
//! - hands out handles on the app's conversation
//!   ([`OctosAppService::open_conversation`], ADR 0004 §6, 2026-09-29): the
//!   person's lane, a request context opened with `share_history` (octos
//!   UPCR-2026-034, "Parallel person context with shared history"), which
//!   runs IN PARALLEL with the peer's own session (the system agent's
//!   lane). Each handle gets a new context id. A person's message runs in
//!   its context, started on this link (the one that registered the peer's
//!   tools) with `origin` (`person`, or `app` when the app started the
//!   run), waiting only for its own context's previous turn; the kernel
//!   shows each lane's turns the other's recent rows read-only. The
//!   follower hears both lanes (its context's events and the peer
//!   session's), each event with its `lane` and speaker; history is both
//!   transcripts merged by time;
//! - keeps ONE queue per peer for the system agent's `peer/input`s on the
//!   peer's session, one turn at a time (the kernel admits one per session
//!   and queues none; `turn_in_progress` is retried), bounded: a full queue
//!   refuses an input `busy`;
//! - checks the lease on every call — context open, same account and account
//!   generation, app not released, service in the app's grant AND the
//!   instance's — before the request and again before any reply is
//!   delivered, so a late reply never reaches a new account or instance;
//! - routes each notification to the context whose session it names;
//! - is the peer's **tool host** (octos UPCR-2026-035): right after every
//!   `peer/prepare` (and so after every reconnect, which prepares again) it
//!   registers the app's tools on its own link, the one that drives the
//!   peer's turns (`peer/tools/register`, with the host's exact
//!   `generic_tools` for the app's agent when it sets them). A peer whose registration fails runs no
//!   turn. It takes every `peer/tool/call` on that link, stamps the account,
//!   the calling context's client and the caller, executes each occurrence
//!   at most once, refuses calls of turns it interrupted, and hands the call
//!   to the host ([`crate::host_tools::ToolHost`]); `peer/tool/cancel` and a
//!   closed link end calls before they run. `peer/input` (the system agent's
//!   input) starts the peer's turn on the same link with the kernel's turn
//!   id, once per input, queued while the peer is busy; `host_tool`
//!   approvals go to the host, never to the app, and are withdrawn there
//!   when their turn ends before an answer;
//! - routes the agent's questions (`user_question/requested`, octos's
//!   `ask_user_question`) on the peer's session and its contexts to the
//!   host with the turn's origin (a context's, the peer's own, or a
//!   `peer/input` turn's), answers them only as the host says
//!   (`user_question/respond` on this link), tells the app's context only
//!   that the host took it, closes them when their turn ends, and refuses
//!   an app's attempt to answer an approval or question the host holds;
//! - gives every approval and question on the peer's session and its
//!   contexts a deadline ([`BrokerConfig::prompt_deadline`], 10 min,
//!   `OCTOSENSE_PROMPT_DEADLINE_SECS`): what the host holds its router or
//!   request model expires (denied or declined, never approved), what the
//!   app holds the broker answers so; the app hears `prompt/expired`. If
//!   the host has not answered after [`BrokerConfig::expiry_grace`] (30 s)
//!   the broker denies or declines it itself, and a turn still running then
//!   is interrupted (N1 applies) so the peer's next queued turn starts;
//! - on the person's Stop (`ContextOp::Interrupt` on a conversation,
//!   [`interrupt_where`] for the shell's own surfaces) stops BOTH lanes:
//!   the person's running turn and the system agent's (the person owns the
//!   device). A plain request context's Stop stops its own turn only.
//!
//! Nothing here chooses a provider, touches credentials or stops a kernel it
//! does not own.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use crate::contract::*;
use crate::host_tools::{self, AgentQuestion, ApprovalAnswer, CallOrigin, HostToolApproval, HostToolCall, InputRefusal, PeerInput, QuestionAnswer, ToolHost, ToolOutcome, ToolReply};

/// A boxed future, for the object-safe transport traits.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One JSON-RPC frame stream to a kernel.
pub trait Link: Send {
    /// Queue one frame. Never blocks.
    fn send(&mut self, frame: String) -> Result<(), String>;
    /// The next frame, or why the link ended.
    fn recv(&mut self) -> BoxFuture<'_, Result<String, String>>;
}

/// How a broker reaches its kernel.
pub trait Connector: Send + Sync {
    /// Whether a kernel can be reached here at all (checked before starting
    /// anything); the reason when not.
    fn available(&self) -> Result<(), String>;
    /// Open a link, starting an owned runtime if needed.
    fn connect(&self) -> BoxFuture<'static, Result<Box<dyn Link>, String>>;
    /// Whether this connector owns the runtime it starts.
    fn owns_runtime(&self) -> bool;
    /// Stop the runtime; a no-op unless [`Connector::owns_runtime`].
    fn shutdown(&self);
}

/// What a broker is for.
#[derive(Clone, Debug)]
pub struct BrokerConfig {
    pub deployment: Deployment,
    /// The kernel profile (the shared provider profile, `_main` in a shell).
    pub profile_id: String,
    /// The session that owns the app's peer: the host's system agent, or a
    /// standalone app's own root session. Recorded by the kernel as the
    /// peer's originator.
    pub originator: String,
    /// The app's id, one memory-namespace segment (`[a-z0-9][a-z0-9._-]*`).
    pub app_id: String,
    /// What the peer is called (a per-account suffix is added).
    pub app_label: String,
    /// The peer's standing brief.
    pub brief: String,
    /// The app's effective assistant services.
    pub services: BTreeSet<String>,
    /// A configured model lane chosen by host policy for this app's peer.
    pub model_lane: Option<String>,
    pub settings_entry: SettingsEntry,
    /// How long one turn may run.
    pub turn_timeout: Duration,
    /// How long an approval or question on the peer's session or a context
    /// waits for its answer before it expires (denied or declined, never
    /// approved): [`host_tools::prompt_deadline`], 10 min by default.
    pub prompt_deadline: Duration,
    /// After an expiry, how long the turn may still run before the broker
    /// interrupts it and the peer's next queued turn starts.
    pub expiry_grace: Duration,
    /// Where the host keeps each peer's host token (one file per app and
    /// account, mode 0600): the kernel's credential for controlling the peer
    /// it created. `None` keeps tokens in memory, so a peer created by this
    /// process cannot be resumed after a restart.
    pub state_dir: Option<std::path::PathBuf>,
    /// The peer's tool host (UPCR-2026-035); `None`: the process's
    /// ([`host_tools::host`]).
    pub tool_host: Option<ToolHostHandle>,
}

/// A tool host for one broker (tests, a standalone app's own).
#[derive(Clone)]
pub struct ToolHostHandle(pub Arc<dyn ToolHost>);

impl std::fmt::Debug for ToolHostHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ToolHostHandle")
    }
}

impl BrokerConfig {
    /// A config with the defaults a shell uses; set the rest by field.
    pub fn new(
        deployment: Deployment,
        profile_id: impl Into<String>,
        originator: impl Into<String>,
        app_id: impl Into<String>,
        app_label: impl Into<String>,
        services: BTreeSet<String>,
    ) -> Self {
        let app_label = app_label.into();
        Self {
            deployment,
            profile_id: profile_id.into(),
            originator: originator.into(),
            app_id: app_id.into(),
            brief: format!(
                "You are the {app_label} app's assistant. You work for the {app_label} app \
                 inside its own workspace and memory, and you answer to the system agent that \
                 owns you."
            ),
            app_label,
            services,
            model_lane: None,
            settings_entry: match deployment {
                Deployment::Hosted => SettingsEntry::Host,
                Deployment::StandaloneLocal => SettingsEntry::AppLocal,
                Deployment::StandaloneRemote => SettingsEntry::AppRemote,
            },
            turn_timeout: Duration::from_secs(180),
            prompt_deadline: host_tools::prompt_deadline(),
            expiry_grace: host_tools::EXPIRY_GRACE,
            state_dir: None,
            tool_host: None,
        }
    }
}

/// A stable, non-secret account tag for names and namespaces.
pub fn account_tag(account: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in account.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The memory namespace of `app_id` for `account`.
pub fn app_namespace(app_id: &str, account: &str) -> String {
    format!("app/{app_id}/acct-{}", account_tag(account))
}

/// A kernel context id from a client instance key: `[a-z0-9-]`, at most 64.
fn context_id(nonce: &str, instance: &str) -> String {
    let mut id = String::new();
    for ch in instance.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            id.push(ch);
        } else if !id.ends_with('-') {
            id.push('-');
        }
    }
    let id = id.trim_matches('-');
    let budget = 64 - nonce.len() - 1;
    let id: String = id.chars().take(budget).collect();
    let id = id.trim_end_matches('-');
    if id.is_empty() {
        nonce.to_owned()
    } else {
        format!("{nonce}-{id}")
    }
}

// --------------------------------------------------------------------------

type Reply = oneshot::Sender<Result<Value, String>>;

struct TurnWaiter {
    turn_id: String,
    text: String,
    /// The assistant segment `text` belongs to (v2 envelopes).
    segment: String,
    /// Segments whose saved text arrived: later deltas for them are stale.
    persisted: BTreeSet<String>,
    done: Option<oneshot::Sender<Result<String, String>>>,
}

impl TurnWaiter {
    fn new(turn_id: String, done: oneshot::Sender<Result<String, String>>) -> Self {
        Self {
            turn_id,
            text: String::new(),
            segment: String::new(),
            persisted: BTreeSet::new(),
            done: Some(done),
        }
    }

    fn finish(&mut self, result: Result<String, String>) {
        if let Some(done) = self.done.take() {
            let _ = done.send(result);
        }
    }

    /// Apply one v2 envelope payload; `true` when the text changed.
    fn envelope(&mut self, payload: &Value) -> bool {
        let data = &payload["data"];
        match payload["type"].as_str().unwrap_or("") {
            kind @ ("assistant_delta" | "assistant_persisted") => {
                let segment = data["assistant_segment_id"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned();
                // The saved answer can overtake the last live deltas of the
                // same segment; they are already in it.
                if kind == "assistant_delta" && self.persisted.contains(&segment) {
                    return false;
                }
                if self.segment != segment {
                    self.text.clear();
                    self.segment = segment.clone();
                }
                let text = data["text"].as_str().unwrap_or("");
                if kind == "assistant_persisted" {
                    self.persisted.insert(segment);
                    self.text = text.to_owned();
                } else {
                    self.text.push_str(text);
                }
                true
            }
            "turn_terminal" => {
                let text = self.text.clone();
                if data["outcome"] == "completed" {
                    self.finish(Ok(text));
                } else {
                    let message = data["error"]["message"]
                        .as_str()
                        .unwrap_or("The assistant's turn did not complete")
                        .to_owned();
                    self.finish(Err(message));
                }
                false
            }
            _ => false,
        }
    }
}

struct Route {
    generation: u64,
    context: Weak<ContextInner>,
}

/// A `peer/tool/call` the host is working on.
struct InFlight {
    reply: ToolReply,
    occurrence: String,
    turn_id: String,
}

/// One occurrence `(session, turn, tool call, args digest)`: executed at
/// most once; a repeat gets the first result.
enum Occurrence {
    /// The call ids waiting on it (the first is the one the host runs).
    Running(Vec<String>),
    Done(ToolOutcome),
}

/// How many finished occurrences, interrupted turns and inputs are kept.
const REMEMBERED: usize = 512;

/// Who answers a pending approval or question.
enum PromptAnswer {
    /// The host took the approval: its answer (the router's decision).
    HostApproval(ApprovalAnswer),
    /// The host took the question: its answer, and how many it takes.
    HostQuestion(QuestionAnswer, usize),
    /// Left to the app's context (a host that takes none): the broker
    /// answers it at the deadline.
    AppApproval,
    AppQuestion(usize),
}

impl PromptAnswer {
    /// Answered in time. An expiry the host sent (its router's deadline
    /// can land a moment before this broker's) is not an answer: the grace
    /// still runs and a turn still stuck after it is interrupted.
    fn answered(&self) -> bool {
        match self {
            PromptAnswer::HostApproval(a) => a.is_sent() && !a.expired(),
            PromptAnswer::HostQuestion(a, _) => a.is_sent() && !a.expired(),
            PromptAnswer::AppApproval | PromptAnswer::AppQuestion(_) => false,
        }
    }
}

/// An approval or question waiting on the peer's session or one of its
/// contexts, with its deadline running (ADR 0004 §8).
struct Prompt {
    session: String,
    turn: String,
    answer: PromptAnswer,
    /// The context it was raised in (`None`: the peer's own session).
    context: Option<Weak<ContextInner>>,
    /// The link it came on: only there can it be answered.
    link: mpsc::UnboundedSender<String>,
}

/// Every live broker, so the shell can stop an app agent's running turn
/// from its own surfaces ([`interrupt_where`]).
static BROKERS: Mutex<Vec<Weak<Inner>>> = Mutex::new(Vec::new());

/// Stop the running turns of both lanes of every live broker whose app id
/// `matches`: the system agent's on the peer's session and the person's in
/// every open conversation (the Stop on a shell surface: the person owns
/// the device, so the system agent's turns stop too). The turns
/// interrupted.
pub fn interrupt_where(matches: impl Fn(&str) -> bool) -> Vec<String> {
    let brokers: Vec<Arc<Inner>> = {
        let mut all = BROKERS.lock().unwrap_or_else(|e| e.into_inner());
        all.retain(|b| b.strong_count() > 0);
        all.iter().filter_map(Weak::upgrade).collect()
    };
    brokers.into_iter().filter(|b| matches(&b.cfg.app_id)).flat_map(|b| Broker(b).interrupt_running()).collect()
}

/// Register the tools again on every live, prepared peer whose app id
/// `matches` (what the host offers changed: developer mode's `dev.run` and
/// grants came or went, ADR 0004 §13). Each registration replaces the peer's
/// set on the link it was prepared on; a failure is logged. The brokers
/// asked.
pub fn reregister_tools_where(matches: impl Fn(&str) -> bool) -> usize {
    let brokers: Vec<Arc<Inner>> = {
        let mut all = BROKERS.lock().unwrap_or_else(|e| e.into_inner());
        all.retain(|b| b.strong_count() > 0);
        all.iter().filter_map(Weak::upgrade).collect()
    };
    let mut asked = 0;
    for inner in brokers.into_iter().filter(|b| matches(&b.cfg.app_id)) {
        let target = {
            let st = inner.lock();
            match (&st.peer, &st.account, st.released) {
                (Some((_, peer)), Some(account), false) => peer.token.clone().map(|t| (peer.slug.clone(), t, account.clone())),
                _ => None,
            }
        };
        let Some((slug, token, account)) = target else { continue };
        asked += 1;
        let task = inner.clone();
        inner.rt().spawn(async move {
            if let Err(e) = task.register_tools(&slug, &token, &account).await {
                eprintln!("app-peers: {}: registering its tools again: {e}", task.cfg.app_id);
            }
        });
    }
    asked
}

/// The live (not released) broker of the app whose id is `app_id`, if one
/// runs: a shell surface opens the app's conversation on the same peer the
/// app uses (the shell's "Ask <app>" panel).
pub fn live(app_id: &str) -> Option<Broker> {
    let mut all = BROKERS.lock().unwrap_or_else(|e| e.into_inner());
    all.retain(|b| b.strong_count() > 0);
    all.iter().rev().filter_map(Weak::upgrade).find(|b| b.cfg.app_id == app_id && !b.lock().released).map(Broker)
}

/// Whether a kernel refusal is `turn_in_progress` (the session runs another
/// turn: the kernel queues nothing).
fn turn_in_progress(error: &str) -> bool {
    error.contains("(turn_in_progress)")
}

impl State {
    /// Record what started `turn` (a context turn this broker started).
    fn note_trigger(&mut self, turn: &str, trigger: TurnTrigger) {
        if self.turn_triggers.insert(turn.to_owned(), trigger).is_none() {
            self.trigger_order.push_back(turn.to_owned());
            while self.trigger_order.len() > REMEMBERED {
                if let Some(old) = self.trigger_order.pop_front() {
                    self.turn_triggers.remove(&old);
                    self.speakers.remove(&old);
                }
            }
        }
    }

    /// Who speaks in `turn` of the app's conversation, as this host started
    /// it: the origin it sent (a person-lane turn), or the system agent for
    /// a `peer/input` turn.
    fn speaker_of(&self, turn: &str) -> Option<Speaker> {
        if self.input_turns.iter().any(|t| t == turn) {
            return Some(Speaker { kind: host_tools::TurnOrigin::SystemAgent, label: None });
        }
        self.speakers.get(turn).cloned()
    }

    /// Whether a turn is running on the peer or waiting for it.
    fn peer_busy(&self) -> bool {
        self.peer_turn.is_some() || !self.queue.is_empty()
    }

    /// What started `turn`: a `peer/input` turn is the system agent's; a
    /// context turn is what its starter said; anything else is unknown.
    fn trigger_of(&self, turn: &str) -> TurnTrigger {
        if self.input_turns.iter().any(|t| t == turn) {
            return TurnTrigger::SystemAgent;
        }
        self.turn_triggers.get(turn).cloned().unwrap_or_default()
    }
}

/// The words that started one turn of the app's conversation, as this
/// broker sent them: which session (lane), who spoke and when it started.
#[derive(Clone, Debug)]
struct Request {
    turn: String,
    session: String,
    lane: &'static str,
    text: String,
    speaker: Speaker,
    /// RFC 3339 (UTC), the host's clock at the start.
    at: String,
}

impl Request {
    fn to_json(&self) -> Value {
        json!({"text": self.text, "speaker": self.speaker.to_json()})
    }
}

impl State {
    fn note_request(&mut self, request: Request) {
        if request.text.trim().is_empty() || self.requests.iter().any(|r| r.turn == request.turn) {
            return;
        }
        self.requests.push_back(request);
        while self.requests.len() > REMEMBERED {
            self.requests.pop_front();
        }
    }

    fn request_of(&self, turn: &str) -> Option<&Request> {
        self.requests.iter().find(|r| r.turn == turn)
    }
}

/// Now as RFC 3339 in UTC with microseconds, like the kernel's
/// `persisted_at` (so [`time_key`] orders both).
fn rfc3339_now() -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970-01-01 to a civil date (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:06}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        now.subsec_micros()
    )
}

fn remember(list: &mut VecDeque<String>, item: String) {
    if !list.contains(&item) {
        list.push_back(item);
        while list.len() > REMEMBERED {
            list.pop_front();
        }
    }
}

#[derive(Clone)]
struct PeerInfo {
    slug: String,
    session: String,
    /// The host token that controls this peer (UPCR-2026-034).
    token: Option<String>,
}

/// `session/open` params for a bound session: the kernel's own workspace
/// for it, so a scoped session resumes after a kernel restart.
fn open_params(session: &str, profile: &str, cwd: Option<&str>) -> Value {
    let mut params = json!({"session_id": session, "profile_id": profile});
    if let Some(cwd) = cwd {
        params["cwd"] = json!(cwd);
    }
    params
}

struct State {
    account: Option<String>,
    generation: u64,
    released: bool,
    link: Option<mpsc::UnboundedSender<String>>,
    link_epoch: u64,
    next_id: u64,
    pending: HashMap<String, Reply>,
    routes: HashMap<String, Route>,
    peer: Option<(u64, PeerInfo)>,
    peer_turn: Option<String>,
    /// Host tokens by memory namespace, when no state dir persists them.
    tokens: HashMap<String, String>,
    contexts: Vec<Weak<ContextInner>>,
    model: Option<ModelInfo>,
    last_error: Option<String>,
    /// In-flight `peer/tool/call`s by call id.
    calls: HashMap<String, InFlight>,
    occurrences: HashMap<String, Occurrence>,
    occurrence_order: VecDeque<String>,
    /// Turns this broker interrupted or saw end interrupted: their late
    /// calls are refused (octos#2567 follow-up N1).
    interrupted: VecDeque<String>,
    /// `peer/input` ids taken, and the turns started for them.
    inputs_seen: VecDeque<String>,
    input_turns: VecDeque<String>,
    /// What started each context turn this broker started (G2), and their
    /// order, so the oldest is forgotten first.
    turn_triggers: HashMap<String, TurnTrigger>,
    trigger_order: VecDeque<String>,
    /// Who speaks in each person-lane turn this broker started (the origin
    /// it sent), by turn id.
    speakers: HashMap<String, Speaker>,
    /// The words that started each turn of the app's conversation this
    /// broker started (a person's message, the system agent's input), in
    /// start order. The kernel records a turn's user message only when the
    /// turn ends (and never for an interrupted one), so a follower is told
    /// them when the turn starts, and history keeps a stopped turn's.
    requests: VecDeque<Request>,
    /// The system agent's inputs waiting for the peer's running turn, in
    /// order (one queue per peer; the person's lane has its own session).
    queue: VecDeque<PeerInput>,
    /// Conversations opened: each gets a new kernel context id.
    conversations: u64,
    /// Workspaces new peers were created with, when no state dir keeps them.
    cwds: HashMap<String, String>,
    /// Approval and question ids the host took: only the host answers them.
    host_held: VecDeque<String>,
    /// Questions the host holds, by id: the turn that asked.
    questions: HashMap<String, String>,
    /// Approvals and questions waiting for an answer, by id, each with its
    /// deadline running.
    prompts: HashMap<String, Prompt>,
}

struct Inner {
    cfg: BrokerConfig,
    connector: Arc<dyn Connector>,
    runtime: Option<tokio::runtime::Runtime>,
    state: Mutex<State>,
    connecting: tokio::sync::Mutex<()>,
    binding: tokio::sync::Mutex<()>,
    nonce: String,
}

/// One app's scoped assistant service. Cheap to clone.
#[derive(Clone)]
pub struct Broker(Arc<Inner>);

impl Broker {
    pub fn new(cfg: BrokerConfig, connector: Arc<dyn Connector>) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("app-peers")
            .enable_all()
            .build()
            .expect("app-peers: tokio runtime");
        let nonce = uuid::Uuid::new_v4().simple().to_string()[..8].to_owned();
        let broker = Broker(Arc::new(Inner {
            cfg,
            connector,
            runtime: Some(runtime),
            state: Mutex::new(State {
                account: None,
                generation: 1,
                released: false,
                link: None,
                link_epoch: 0,
                next_id: 0,
                pending: HashMap::new(),
                routes: HashMap::new(),
                peer: None,
                peer_turn: None,
                tokens: HashMap::new(),
                contexts: Vec::new(),
                model: None,
                last_error: None,
                calls: HashMap::new(),
                occurrences: HashMap::new(),
                occurrence_order: VecDeque::new(),
                interrupted: VecDeque::new(),
                inputs_seen: VecDeque::new(),
                input_turns: VecDeque::new(),
                turn_triggers: HashMap::new(),
                trigger_order: VecDeque::new(),
                speakers: HashMap::new(),
                requests: VecDeque::new(),
                queue: VecDeque::new(),
                conversations: 0,
                cwds: HashMap::new(),
                host_held: VecDeque::new(),
                questions: HashMap::new(),
                prompts: HashMap::new(),
            }),
            connecting: tokio::sync::Mutex::new(()),
            binding: tokio::sync::Mutex::new(()),
            nonce,
        }));
        {
            let mut all = BROKERS.lock().unwrap_or_else(|e| e.into_inner());
            all.retain(|b| b.strong_count() > 0);
            all.push(Arc::downgrade(&broker.0));
        }
        broker
    }

    /// The config this broker serves.
    pub fn config(&self) -> &BrokerConfig {
        &self.0.cfg
    }

    /// The current account generation (bumped by every account change).
    pub fn generation(&self) -> u64 {
        self.0.lock().generation
    }

    /// The account the broker acts for now (`None`: signed out).
    pub fn account(&self) -> Option<String> {
        self.0.lock().account.clone()
    }

    /// The bound peer's slug and session, once bound.
    pub fn peer(&self) -> Option<(String, String)> {
        self.0
            .lock()
            .peer
            .as_ref()
            .map(|(_, p)| (p.slug.clone(), p.session.clone()))
    }

    /// The peer's running turn (driven by its owner), as this broker saw it
    /// start. `release` interrupts it.
    pub fn peer_active_turn(&self) -> Option<String> {
        self.0.lock().peer_turn.clone()
    }

    /// Bind (create or resume) the app's peer for the current account now,
    /// without a model inference. A host calls this at launch; otherwise the
    /// first request does it.
    pub fn bind(&self) -> Result<(), String> {
        let inner = self.0.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.0.rt().spawn(async move {
            let result = inner.ensure_peer().await.map(|_| ());
            let _ = tx.send(result);
        });
        rx.recv_timeout(Duration::from_secs(60))
            .map_err(|_| "binding the app peer timed out".to_owned())?
    }

    /// Stop the turns running in both lanes, whoever started them: the
    /// system agent's on the peer's session and the person's (or the app's)
    /// in every open conversation. The Stop of a shell surface. Their late
    /// calls are refused (N1) and the peer's next queued input starts. The
    /// turns stopped (none when nothing runs).
    pub fn interrupt_running(&self) -> Vec<String> {
        let running = self.0.running_turns();
        for (session, turn, _) in &running {
            let inner = self.0.clone();
            let (session, turn) = (session.clone(), turn.clone());
            self.0.rt().spawn(async move {
                if let Err(e) = inner.interrupt_turn(&session, &turn).await {
                    eprintln!("app-peers: {}: stopping turn {turn}: {e}", inner.cfg.app_id);
                }
            });
        }
        running.into_iter().map(|(_, turn, _)| turn).collect()
    }

    /// Approvals and questions still waiting for an answer.
    pub fn pending_prompts(&self) -> usize {
        self.0.lock().prompts.len()
    }

    /// Tool calls the host has not answered yet.
    pub fn calls_in_flight(&self) -> usize {
        self.0.lock().calls.len()
    }

    /// `peer/input`s waiting for the peer's running turn (the person's
    /// messages never wait for it: they run in their own lane).
    pub fn queued_inputs(&self) -> usize {
        self.0.lock().queue.len()
    }

    /// A request context (`conversation: false`) or a conversation: the
    /// person's lane, a request context that shares history with the
    /// peer's session.
    fn open_handle(&self, spec: ContextSpec, conversation: bool) -> Result<Arc<dyn OctosContext>, String> {
        let mut st = self.0.lock();
        if st.released {
            return Err("The app was closed".into());
        }
        if st.account.as_deref() != Some(spec.account.as_str()) {
            return Err("The account changed; reopen this app".into());
        }
        let services: BTreeSet<String> = spec
            .services
            .intersection(&self.0.cfg.services)
            .cloned()
            .collect();
        if services.is_empty() {
            return Err("This app was not granted the assistant".into());
        }
        // A conversation's kernel context is new for every handle: a closed
        // context id is never reopened, and the same instance may open again.
        let kernel_instance = if conversation {
            st.conversations += 1;
            format!("p{}-{}", st.conversations, spec.instance)
        } else {
            spec.instance.clone()
        };
        let context = Arc::new(ContextInner {
            broker: Arc::downgrade(&self.0),
            account: spec.account,
            generation: st.generation,
            services,
            context_id: context_id(&self.0.nonce, &kernel_instance),
            instance: spec.instance.clone(),
            open: AtomicBool::new(true),
            conversation,
            bound: Mutex::new(None),
            turn: Mutex::new(None),
            sink: Mutex::new(None),
            subscriber: Mutex::new(None),
            calls: AtomicU64::new(0),
            seen: Mutex::new(BTreeSet::new()),
        });
        st.contexts.retain(|c| c.strong_count() > 0);
        st.contexts.push(Arc::downgrade(&context));
        Ok(Arc::new(BrokerContext(context)))
    }

    /// Send a raw request on this broker's link. For the HOST only (its own
    /// system-agent operations and tests); never handed to an app.
    pub fn host_request(&self, method: &str, params: Value) -> Result<Value, String> {
        let inner = self.0.clone();
        let method = method.to_owned();
        let (tx, rx) = std::sync::mpsc::channel();
        self.0.rt().spawn(async move {
            let _ = tx.send(inner.request(&method, params).await);
        });
        rx.recv_timeout(Duration::from_secs(60))
            .map_err(|_| "host request timed out".to_owned())?
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // A broker may be dropped inside another runtime (a test's, a
        // shell's): never block there.
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl Inner {
    fn rt(&self) -> &tokio::runtime::Runtime {
        self.runtime
            .as_ref()
            .expect("app-peers: runtime present until drop")
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn token_path(&self, namespace: &str) -> Option<std::path::PathBuf> {
        let dir = self.cfg.state_dir.as_ref()?;
        Some(dir.join(format!("{}.token", namespace.replace('/', "_"))))
    }

    fn load_token(&self, namespace: &str) -> Option<String> {
        if let Some(token) = self.lock().tokens.get(namespace) {
            return Some(token.clone());
        }
        let text = std::fs::read_to_string(self.token_path(namespace)?).ok()?;
        let token = text.trim().to_owned();
        (!token.is_empty()).then_some(token)
    }

    fn save_token(&self, namespace: &str, token: &str) -> Result<(), String> {
        self.lock()
            .tokens
            .insert(namespace.to_owned(), token.to_owned());
        let Some(path) = self.token_path(namespace) else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("token.tmp");
        std::fs::write(&tmp, token).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }

    fn tool_host(&self) -> Arc<dyn ToolHost> {
        match &self.cfg.tool_host {
            Some(handle) => handle.0.clone(),
            None => host_tools::host(),
        }
    }

    /// The workspace a peer was created with (`<namespace>.cwd` beside its
    /// token): a resume must name the same one. None for a peer the kernel
    /// provisioned.
    fn load_cwd(&self, namespace: &str) -> Option<String> {
        if let Some(cwd) = self.lock().cwds.get(namespace) {
            return Some(cwd.clone());
        }
        let path = self.token_path(namespace)?.with_extension("cwd");
        let text = std::fs::read_to_string(path).ok()?;
        let cwd = text.trim().to_owned();
        (!cwd.is_empty()).then_some(cwd)
    }

    fn save_cwd(&self, namespace: &str, cwd: &str) -> Result<(), String> {
        self.lock().cwds.insert(namespace.to_owned(), cwd.to_owned());
        let Some(path) = self.token_path(namespace).map(|p| p.with_extension("cwd")) else {
            return Ok(());
        };
        let tmp = path.with_extension("cwd.tmp");
        std::fs::write(&tmp, cwd).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }

    fn fail(&self, error: &str) {
        self.lock().last_error = Some(error.to_owned());
    }

    async fn ensure_link(self: &Arc<Self>) -> Result<mpsc::UnboundedSender<String>, String> {
        if let Some(link) = self.lock().link.clone() {
            return Ok(link);
        }
        let _guard = self.connecting.lock().await;
        if let Some(link) = self.lock().link.clone() {
            return Ok(link);
        }
        self.connector.available()?;
        let mut link = match self.connector.connect().await {
            Ok(link) => link,
            Err(err) => {
                self.fail(&err);
                return Err(err);
            }
        };
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let epoch = {
            let mut st = self.lock();
            st.link_epoch += 1;
            st.link = Some(tx.clone());
            st.last_error = None;
            st.link_epoch
        };
        let weak = Arc::downgrade(self);
        self.rt().spawn(async move {
            let why = loop {
                tokio::select! {
                    outbound = rx.recv() => match outbound {
                        Some(frame) => {
                            if let Err(err) = link.send(frame) {
                                break err;
                            }
                        }
                        None => break "link released".to_owned(),
                    },
                    inbound = link.recv() => match inbound {
                        Ok(frame) => match weak.upgrade() {
                            Some(inner) => inner.inbound(&frame),
                            None => break "broker dropped".to_owned(),
                        },
                        Err(err) => break err,
                    },
                }
            };
            if let Some(inner) = weak.upgrade() {
                inner.link_closed(epoch, &why);
            }
        });
        Ok(tx)
    }

    /// The link ended: fail everything waiting on it; sessions and the peer
    /// are opened again (and the app's tools registered again) on the next
    /// request, or by the rebind this schedules. Tool calls that came on the
    /// link end now: the kernel fails them, and the host never runs them.
    fn link_closed(self: &Arc<Self>, epoch: u64, why: &str) {
        let (pending, waiters, calls, rebind) = {
            let mut st = self.lock();
            if st.link_epoch != epoch {
                return;
            }
            st.link = None;
            let had_peer = st.peer.take().is_some();
            st.peer_turn = None;
            st.last_error = Some(why.to_owned());
            let pending: Vec<Reply> = st.pending.drain().map(|(_, r)| r).collect();
            let contexts: Vec<Arc<ContextInner>> =
                st.contexts.iter().filter_map(Weak::upgrade).collect();
            let calls: Vec<(String, ToolReply)> = st.calls.drain().map(|(id, f)| (id, f.reply)).collect();
            st.occurrences.retain(|_, o| matches!(o, Occurrence::Done(_)));
            // Queued inputs are the kernel's to fail with the connection.
            st.queue.clear();
            // What waited on the link can no longer be answered there.
            st.prompts.clear();
            let rebind = (had_peer && !st.released && st.account.is_some()).then_some(st.generation);
            (pending, contexts, calls, rebind)
        };
        let host = self.tool_host();
        for (call_id, reply) in calls {
            if reply.cancel() {
                host.tool_cancel(&self.cfg.app_id, &call_id, "disconnected");
            }
        }
        if let Some(generation) = rebind {
            self.schedule_rebind(generation);
        }
        let message = format!("The assistant connection ended ({why}); try again");
        for reply in pending {
            let _ = reply.send(Err(message.clone()));
        }
        for context in waiters {
            context
                .bound
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            if let Some(waiter) = context
                .turn
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
            {
                if let Some(done) = waiter.done {
                    let _ = done.send(Err(message.clone()));
                }
            }
        }
    }

    /// The peer was bound when its link ended: bind it again on a new link
    /// (prepare, then register), so the system agent's `peer/input` reaches
    /// the app again without waiting for the app's next request.
    fn schedule_rebind(self: &Arc<Self>, generation: u64) {
        let weak = Arc::downgrade(self);
        self.rt().spawn(async move {
            let mut wait = Duration::from_millis(500);
            for _ in 0..6 {
                tokio::time::sleep(wait).await;
                let Some(inner) = weak.upgrade() else { return };
                {
                    let st = inner.lock();
                    if st.released || st.generation != generation || st.account.is_none() || st.peer.is_some() {
                        return;
                    }
                }
                if inner.ensure_peer().await.is_ok() {
                    return;
                }
                wait = (wait * 2).min(Duration::from_secs(10));
            }
        });
    }

    fn inbound(self: &Arc<Self>, frame: &str) {
        if std::env::var_os("APP_PEERS_TRACE").is_some() {
            eprintln!("app-peers <- {}", frame);
        }
        let Ok(value) = serde_json::from_str::<Value>(frame) else {
            return;
        };
        let has_result = value.get("result").is_some() || value.get("error").is_some();
        if let (Some(id), true) = (value.get("id"), has_result) {
            let key = id
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| id.to_string());
            let reply = self.lock().pending.remove(&key);
            if let Some(reply) = reply {
                let result = match value.get("error") {
                    Some(error) if !error.is_null() => Err(rpc_error_text(error)),
                    _ => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = reply.send(result);
            }
            return;
        }
        let Some(method) = value.get("method").and_then(Value::as_str) else {
            return;
        };
        let params = value.get("params").cloned().unwrap_or(Value::Null);
        match method {
            host_tools::TOOL_CALL => return self.on_tool_call(&params),
            host_tools::TOOL_CANCEL => return self.on_tool_cancel(&params),
            host_tools::PEER_INPUT => return self.on_peer_input(&params),
            _ => {}
        }
        let Some(session_id) = params.get("session_id").and_then(Value::as_str) else {
            return;
        };
        // v2 projection envelopes name the BASE session and carry the topic
        // separately.
        let session = match params.get("topic").and_then(Value::as_str) {
            Some(topic) if !session_id.contains('#') => format!("{session_id}#{topic}"),
            _ => session_id.to_owned(),
        };
        let session = session.as_str();
        let (route, peer_session, generation) = {
            let st = self.lock();
            (
                st.routes
                    .get(session)
                    .map(|r| (r.generation, r.context.clone())),
                st.peer.as_ref().map(|(_, p)| p.session.clone()),
                st.generation,
            )
        };
        let ours = peer_session.as_deref() == Some(session) || route.is_some();
        if ours {
            self.note_terminal(method, &params);
            if let Some(turn) = turn_ended(method, &params) {
                self.close_questions(turn);
                self.close_approvals(turn);
                // Its approvals and questions end with it: no deadline.
                self.lock().prompts.retain(|_, p| p.turn != turn);
            }
        }
        // An agent's question is the person's, asked by the shell in the
        // right conversation: never the app context's to answer.
        if ours && method == host_tools::USER_QUESTION_REQUESTED {
            if let Some(question) = AgentQuestion::parse(&params, session) {
                let context = route.as_ref().and_then(|(g, c)| (*g == generation).then(|| c.upgrade()).flatten());
                if route.is_some() && context.is_none() {
                    return;
                }
                if self.on_user_question(question, context.as_ref()) {
                    if let Some(context) = context {
                        context.notification(host_tools::QUESTION_HANDLED_BY_HOST, &params);
                    } else if peer_session.as_deref() == Some(session) {
                        self.to_conversations(host_tools::QUESTION_HANDLED_BY_HOST, &params, session);
                    }
                    return;
                }
            }
        }
        // Every approval on the peer's session or one of its contexts is the
        // host's to draw (ADR 0004 §8): a host-routed tool's (the owning
        // app's sheet, the shell's router) and octos's own (the shell's
        // router, as the app agent's call). The app's context only hears
        // that the host has it. A host that takes none leaves octos's own
        // approvals to the context, as before.
        if ours && method == "approval/requested" {
            let context = route.as_ref().and_then(|(g, c)| (*g == generation).then(|| c.upgrade()).flatten());
            let approval = HostToolApproval::parse(&params, session)
                .or_else(|| HostToolApproval::parse_octos(&params, session, &self.cfg.app_id))
                .map(|mut a| {
                    if let Some(c) = &context {
                        a.context_id.get_or_insert_with(|| c.context_id.clone());
                        a.client = Some(c.instance.clone());
                    }
                    a
                });
            if let Some(approval) = approval {
                let id = approval.approval_id.clone();
                if self.on_host_approval(approval, context.as_ref()) {
                    remember(&mut self.lock().host_held, id);
                    if let Some(context) = context {
                        context.notification(host_tools::HANDLED_BY_HOST, &params);
                    } else if peer_session.as_deref() == Some(session) {
                        self.to_conversations(host_tools::HANDLED_BY_HOST, &params, session);
                    }
                    return;
                }
            }
        }
        if peer_session.as_deref() == Some(session) {
            // Track the peer's own (system-agent driven) turn so a release
            // can stop it.
            let turn = params
                .get("turn_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let ended = turn_ended(method, &params).is_some();
            let mut st = self.lock();
            let mut next = false;
            if method == "turn/started" {
                // A turn this host holds the peer for (one starting, or
                // retrying after `turn_in_progress`) keeps it.
                if st.peer_turn.is_none() {
                    st.peer_turn = turn;
                }
            } else if ended && st.peer_turn == turn {
                st.peer_turn = None;
                next = true;
            }
            drop(st);
            if next {
                self.next_turn();
            }
            // An approval or question left to the app still expires.
            self.track_unheld(method, &params, session, None);
            // The whole shared conversation reaches the app's
            // conversations: the person's turns from every surface, the
            // app's and the system agent's.
            self.to_conversations(method, &params, session);
            return;
        }
        let Some((route_generation, context)) = route else {
            return;
        };
        // Stale-reply dropping: a context of an older account generation
        // never receives anything again.
        if route_generation != generation {
            return;
        }
        if let Some(context) = context.upgrade() {
            self.track_unheld(method, &params, session, Some(&context));
            context.notification(method, &params);
        }
    }

    async fn request(self: &Arc<Self>, method: &str, params: Value) -> Result<Value, String> {
        let link = self.ensure_link().await?;
        let (tx, rx) = oneshot::channel();
        let id = {
            let mut st = self.lock();
            st.next_id += 1;
            let id = format!("app-peers-{}", st.next_id);
            st.pending.insert(id.clone(), tx);
            id
        };
        let frame = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if std::env::var_os("APP_PEERS_TRACE").is_some() {
            eprintln!("app-peers -> {frame}");
        }
        if link.send(frame.to_string()).is_err() {
            self.lock().pending.remove(&id);
            return Err("The assistant connection ended; try again".into());
        }
        match tokio::time::timeout(Duration::from_secs(60), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("The assistant connection ended; try again".into()),
            Err(_) => {
                self.lock().pending.remove(&id);
                Err(format!("{method} timed out"))
            }
        }
    }

    /// Create or resume the app's peer for the current account, and
    /// register its tools on this link (UPCR-2026-035's host obligation).
    async fn ensure_peer(self: &Arc<Self>) -> Result<(u64, PeerInfo), String> {
        let _guard = self.binding.lock().await;
        let (account, generation) = {
            let st = self.lock();
            if st.released {
                return Err("The app was closed".into());
            }
            if let Some((generation, peer)) = &st.peer {
                if *generation == st.generation {
                    return Ok((*generation, peer.clone()));
                }
            }
            (st.account.clone(), st.generation)
        };
        if self.cfg.services.is_empty() {
            return Err("This app has no assistant access".into());
        }
        let account = account.ok_or("Sign in before using the assistant")?;
        let host = self.tool_host();
        if host.suspended(&self.cfg.app_id, &account) {
            let err = "This account is signed out; its assistant is paused".to_owned();
            self.fail(&err);
            return Err(err);
        }
        let tag = account_tag(&account);
        let namespace = app_namespace(&self.cfg.app_id, &account);
        let name = format!("{} {}", self.cfg.app_label, &tag[..8]);
        // The owner session is live before its peer exists, so the kernel
        // can wake it when the peer asks a question.
        self.request(
            "session/open",
            json!({"session_id": self.cfg.originator, "profile_id": self.cfg.profile_id}),
        )
        .await?;
        let mut params = json!({
            "profile_id": self.cfg.profile_id,
            "session_id": self.cfg.originator,
            "names": [name],
            "brief": self.cfg.brief,
            "memory_namespace": namespace,
            "resume": true,
        });
        let known_token = self.load_token(&namespace);
        // The agent's workspace is the account's folder (ADR 0004 §11) for
        // a peer created now; a resume names the workspace the peer was
        // created with (the kernel refuses any other), so peers created
        // before keep the one the kernel provisioned.
        let mut chosen_cwd = None;
        if let Some(token) = &known_token {
            params["host_token"] = json!(token);
            if let Some(cwd) = self.load_cwd(&namespace) {
                params["cwd"] = json!(cwd);
            }
        } else if let Some(cwd) = host.agent_workspace(&self.cfg.app_id, &account) {
            let cwd = cwd.to_string_lossy().into_owned();
            params["cwd"] = json!(cwd);
            chosen_cwd = Some(cwd);
        }
        if let Some(lane) = &self.cfg.model_lane {
            params["model"] = json!(lane);
        }
        let result = match self.request("peer/prepare", params).await {
            Ok(result) => result,
            Err(err) => {
                self.fail(&err);
                return Err(err);
            }
        };
        // A kernel that ignored the host binding staged an ORDINARY peer:
        // refuse it rather than run the app with the profile's memory.
        if result.get("memory_namespace").and_then(Value::as_str) != Some(namespace.as_str()) {
            let err = "This assistant kernel does not support host-owned app peers \
                       (octos UPCR-2026-034); update it"
                .to_owned();
            self.fail(&err);
            return Err(err);
        }
        let slug = result["slug"]
            .as_str()
            .ok_or("peer/prepare returned no slug")?
            .to_owned();
        // A new peer's credential arrives once; keep it before anything else.
        let token = match result["host_token"].as_str() {
            Some(token) => {
                if let Err(err) = self.save_token(&namespace, token) {
                    let err = format!("could not keep the assistant's peer credential: {err}");
                    self.fail(&err);
                    return Err(err);
                }
                if chosen_cwd.is_some() {
                    let cwd = result["cwd"].as_str().map(str::to_owned).or(chosen_cwd);
                    if let Some(cwd) = cwd {
                        if let Err(err) = self.save_cwd(&namespace, &cwd) {
                            eprintln!("app-peers: could not record the peer's workspace ({err}); it resumes only in this run");
                        }
                    }
                }
                Some(token.to_owned())
            }
            None => known_token,
        };
        let session = format!(
            "{}#peer-{slug}",
            self.cfg
                .originator
                .split('#')
                .next()
                .unwrap_or(&self.cfg.originator)
        );
        let model = model_info(&result["model"]);
        // Open the peer session so the system agent can address it.
        self.request(
            "session/open",
            open_params(&session, &self.cfg.profile_id, result["cwd"].as_str()),
        )
        .await?;
        let epoch = self.lock().link_epoch;
        // Register the app's tools on THIS link before any turn: the kernel
        // gives app memory, app context and app tools only to turns of the
        // connection that registered. A peer that could not register runs
        // no turn (never a memory-less one).
        let Some(host_token) = token.clone() else {
            let err = "The assistant returned no credential for the app's peer; it cannot take the app's tools".to_owned();
            self.fail(&err);
            return Err(err);
        };
        if let Err(err) = self.register_tools(&slug, &host_token, &account).await {
            self.fail(&err);
            return Err(err);
        }
        let peer = PeerInfo {
            slug,
            session: session.clone(),
            token,
        };
        let mut st = self.lock();
        if st.generation != generation || st.released {
            return Err("The account changed; try again".into());
        }
        if st.link_epoch != epoch || st.link.is_none() {
            return Err("The assistant connection changed; try again".into());
        }
        st.peer = Some((generation, peer.clone()));
        st.model = model;
        Ok((generation, peer))
    }

    /// `peer/tools/register` on this link: the app's granted tools plus the
    /// cross-app tools granted to it, and exactly the kernel tools the host
    /// grants its agent (`generic_tools`; omitted only when the host sets
    /// none, and then the peer keeps its kernel roster).
    async fn register_tools(self: &Arc<Self>, slug: &str, token: &str, account: &str) -> Result<Value, String> {
        let host = self.tool_host();
        let tools = host
            .declarations(&self.cfg.app_id, account)
            .map_err(|e| format!("The app's tools could not be declared ({e}); the assistant runs nothing for it"))?;
        let mut params = json!({
            "profile_id": self.cfg.profile_id,
            "session_id": self.cfg.originator,
            "peer": slug,
            "host_token": token,
            "tools": tools,
        });
        if let Some(generic) = host.generic_tools(&self.cfg.app_id, account) {
            params["generic_tools"] = json!(generic);
        }
        self.request(host_tools::REGISTER, params)
            .await
            .map_err(|e| format!("The assistant did not take the app's tools ({e}); it runs no turn without them"))
    }

    /// Send a request on `link` (the connection a call came on), answer
    /// logged, never retried on another connection.
    fn fire(self: &Arc<Self>, link: &mpsc::UnboundedSender<String>, method: &str, params: Value) {
        let (tx, rx) = oneshot::channel();
        let id = {
            let mut st = self.lock();
            st.next_id += 1;
            let id = format!("app-peers-{}", st.next_id);
            st.pending.insert(id.clone(), tx);
            id
        };
        let frame = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if std::env::var_os("APP_PEERS_TRACE").is_some() {
            eprintln!("app-peers -> {frame}");
        }
        if link.send(frame.to_string()).is_err() {
            self.lock().pending.remove(&id);
            return;
        }
        let inner = self.clone();
        let method = method.to_owned();
        self.rt().spawn(async move {
            match tokio::time::timeout(Duration::from_secs(60), rx).await {
                Ok(Ok(Err(e))) => eprintln!("app-peers: {method} was refused: {e}"),
                Err(_) => {
                    inner.lock().pending.remove(&id);
                }
                _ => {}
            }
        });
    }

    /// Record a turn as interrupted (N1): its late calls are refused, and
    /// its in-flight calls end now.
    fn note_interrupted(&self, turn: &str) {
        let calls: Vec<(String, ToolReply)> = {
            let mut st = self.lock();
            remember(&mut st.interrupted, turn.to_owned());
            let ids: Vec<String> = st.calls.iter().filter(|(_, f)| f.turn_id == turn).map(|(id, _)| id.clone()).collect();
            ids.into_iter().filter_map(|id| st.calls.remove(&id).map(|f| (id, f.reply))).collect()
        };
        let host = self.tool_host();
        for (call_id, reply) in calls {
            if reply.cancel() {
                host.tool_cancel(&self.cfg.app_id, &call_id, "cancelled");
            }
        }
    }

    /// A turn of ours ended interrupted (the kernel's terminal says so).
    fn note_terminal(&self, method: &str, params: &Value) {
        let Some(turn) = params.get("turn_id").and_then(Value::as_str) else { return };
        let interrupted = match method {
            "projection/envelope" => {
                params["payload"]["type"] == "turn_terminal"
                    && matches!(params["payload"]["data"]["outcome"].as_str(), Some("interrupted" | "cancelled"))
            }
            "turn/error" => params["message"].as_str().or(params["code"].as_str()).is_some_and(|m| m.contains("interrupt")),
            "turn/interrupted" => true,
            _ => false,
        };
        if interrupted {
            self.note_interrupted(turn);
        }
    }

    /// The answer handle of one call: `peer/tool/result` on the link it came
    /// on, with the peer's credential; a final answer settles its occurrence.
    fn reply_for(self: &Arc<Self>, call: &HostToolCall, link: mpsc::UnboundedSender<String>, token: Option<String>) -> ToolReply {
        let weak = Arc::downgrade(self);
        let base = json!({
            "profile_id": self.cfg.profile_id,
            "session_id": self.cfg.originator,
            "peer": call.peer,
            "host_token": token,
        });
        let occurrence = call.occurrence();
        let call_id = call.call_id.clone();
        ToolReply::new(call.call_id.clone(), move |fields: Value| {
            let Some(inner) = weak.upgrade() else { return };
            let mut params = base.clone();
            for (k, v) in fields.as_object().into_iter().flatten() {
                params[k] = v.clone();
            }
            let final_answer = fields.get("status").is_none();
            inner.fire(&link, host_tools::TOOL_RESULT, params);
            if final_answer {
                let outcome = if fields["ok"] == true {
                    ToolOutcome::Ok(fields.get("data").cloned().unwrap_or(Value::Null))
                } else {
                    ToolOutcome::Error {
                        kind: fields["error"]["kind"].as_str().unwrap_or("error").to_owned(),
                        message: fields["error"]["message"].as_str().unwrap_or("").to_owned(),
                    }
                };
                inner.settle(&call_id, &occurrence, outcome);
            }
        })
    }

    /// A call was answered: its occurrence is done, and every repeat of it
    /// waiting gets the same answer.
    fn settle(&self, call_id: &str, occurrence: &str, outcome: ToolOutcome) {
        let others: Vec<ToolReply> = {
            let mut st = self.lock();
            st.calls.remove(call_id);
            let waiting = match st.occurrences.get(occurrence) {
                Some(Occurrence::Running(ids)) => ids.clone(),
                _ => Vec::new(),
            };
            if !st.occurrences.contains_key(occurrence) || !waiting.is_empty() {
                st.occurrence_order.push_back(occurrence.to_owned());
            }
            st.occurrences.insert(occurrence.to_owned(), Occurrence::Done(outcome.clone()));
            while st.occurrence_order.len() > REMEMBERED {
                if let Some(old) = st.occurrence_order.pop_front() {
                    if matches!(st.occurrences.get(&old), Some(Occurrence::Done(_))) {
                        st.occurrences.remove(&old);
                    }
                }
            }
            waiting.iter().filter(|id| id.as_str() != call_id).filter_map(|id| st.calls.remove(id).map(|f| f.reply)).collect()
        };
        for reply in others {
            reply.finish(outcome.clone());
        }
    }

    /// `peer/tool/call`: stamp it, check it, run each occurrence once.
    fn on_tool_call(self: &Arc<Self>, params: &Value) {
        let mut call = match HostToolCall::parse(params) {
            Ok(call) => call,
            Err(e) => return eprintln!("app-peers: {} dropped: {e}", host_tools::TOOL_CALL),
        };
        let host = self.tool_host();
        let (link, peer, account) = {
            let st = self.lock();
            let peer = st.peer.as_ref().filter(|(g, _)| *g == st.generation).map(|(_, p)| p.clone());
            (st.link.clone(), peer, st.account.clone())
        };
        // The shell's consumers share one kernel connection, and a
        // notification reaches every consumer that named its session (every
        // broker names the system session): only this app's peer's calls are
        // this broker's to answer. Any other is left to its own host.
        let (Some(link), Some(peer)) = (link, peer) else { return };
        if call.peer.as_deref() != Some(peer.slug.as_str()) {
            return;
        }
        // Answered on the connection the call came on (the kernel refuses a
        // result from any other).
        let reply = self.reply_for(&call, link, peer.token.clone());
        let refuse = |kind: &str, message: &str| {
            reply.finish(ToolOutcome::error(kind, message));
        };
        let Some(account) = account else {
            return refuse("signed_out", "the account is signed out");
        };
        if host.suspended(&self.cfg.app_id, &account) {
            return refuse("signed_out", "the account is signed out");
        }
        call.calling_app = self.cfg.app_id.clone();
        call.account = Some(account);
        let occurrence = call.occurrence();
        let repeat = {
            let mut st = self.lock();
            // N1: a call can still arrive just after an interrupt.
            if st.interrupted.contains(&call.turn_id) {
                drop(st);
                return refuse("turn_interrupted", "the turn was interrupted");
            }
            match &call.context_id {
                Some(context_id) => {
                    let owner = st
                        .contexts
                        .iter()
                        .filter_map(Weak::upgrade)
                        .find(|c| &c.context_id == context_id && c.open.load(Ordering::Acquire) && c.generation == st.generation);
                    match owner {
                        Some(context) => {
                            call.client = Some(context.instance.clone());
                            call.origin = CallOrigin::Context;
                            call.trigger = st.trigger_of(&call.turn_id);
                        }
                        None => {
                            drop(st);
                            return refuse("unknown_context", "a request context this app did not open, or one already closed");
                        }
                    }
                }
                None => {
                    call.origin = if st.input_turns.contains(&call.turn_id) { CallOrigin::PeerInput } else { CallOrigin::PeerOwn };
                    call.trigger = st.trigger_of(&call.turn_id);
                }
            }
            let repeat = match st.occurrences.get_mut(&occurrence) {
                Some(Occurrence::Done(outcome)) => Some(Some(outcome.clone())),
                Some(Occurrence::Running(ids)) => {
                    ids.push(call.call_id.clone());
                    Some(None)
                }
                None => {
                    st.occurrences.insert(occurrence.clone(), Occurrence::Running(vec![call.call_id.clone()]));
                    None
                }
            };
            if !matches!(repeat, Some(Some(_))) {
                st.calls.insert(call.call_id.clone(), InFlight { reply: reply.clone(), occurrence, turn_id: call.turn_id.clone() });
            }
            repeat
        };
        match repeat {
            // Executed once already: the same answer, nothing runs again.
            Some(Some(outcome)) => {
                reply.finish(outcome);
            }
            // Running: this repeat gets the first one's answer.
            Some(None) => {}
            None => host.tool_call(call, reply),
        }
    }

    /// `peer/tool/cancel`: nothing of the call runs or answers afterwards.
    fn on_tool_cancel(&self, params: &Value) {
        let Some(call_id) = params.get("call_id").and_then(Value::as_str) else { return };
        let reason = params.get("reason").and_then(Value::as_str).unwrap_or("cancelled");
        let reply = {
            let mut st = self.lock();
            let flight = st.calls.remove(call_id);
            if let Some(f) = &flight {
                if let Some(Occurrence::Running(ids)) = st.occurrences.get_mut(&f.occurrence) {
                    ids.retain(|id| id != call_id);
                    if ids.is_empty() {
                        st.occurrences.remove(&f.occurrence);
                    }
                }
            }
            flight.map(|f| f.reply)
        };
        if reply.is_some_and(|r| r.cancel()) {
            self.tool_host().tool_cancel(&self.cfg.app_id, call_id, reason);
        }
    }

    /// `peer/input`: the system agent's input to this app's peer. Started as
    /// the peer's turn on this link with the kernel's turn id (a host-driven
    /// turn: the app's tools, memory and approvals), once per input id,
    /// after the running turn when the peer is busy; never for a signed-out
    /// or suspended account (ADR 0004 §11).
    fn on_peer_input(self: &Arc<Self>, params: &Value) {
        let Some(input) = PeerInput::parse(params) else {
            return eprintln!("app-peers: {} dropped: malformed", host_tools::PEER_INPUT);
        };
        let (peer, account, busy, seen) = {
            let mut st = self.lock();
            let peer = st.peer.as_ref().filter(|(g, _)| *g == st.generation).map(|(_, p)| p.clone());
            let seen = st.inputs_seen.contains(&input.input_id);
            if !seen {
                remember(&mut st.inputs_seen, input.input_id.clone());
            }
            (peer, st.account.clone(), st.peer_busy(), seen)
        };
        if seen {
            return;
        }
        let Some(peer) = peer.filter(|p| p.slug == input.peer && p.session == input.session_id) else {
            return eprintln!("app-peers: {}: input {} is not for this app's peer", self.cfg.app_id, input.input_id);
        };
        // A refusal the host already knows is said to the kernel before any
        // turn starts (octos#2621), so the system agent learns why.
        let host = self.tool_host();
        let Some(account) = account.filter(|a| !host.suspended(&self.cfg.app_id, a)) else {
            return self.reject_input(&peer, &input, InputRefusal::SignedOut);
        };
        if let Err(why) = host.admit_input(&self.cfg.app_id, &account, &input) {
            return self.reject_input(&peer, &input, why);
        }
        if busy {
            let full = self.lock().queue.len() >= host_tools::MAX_QUEUED_INPUTS;
            if full {
                return self.reject_input(&peer, &input, InputRefusal::Busy);
            }
            // Queued: it starts later, with its own turn id.
            self.lock().queue.push_back(input);
            // The running turn may have ended meanwhile.
            self.next_turn();
        } else {
            self.start_input(input);
        }
    }

    /// `peer/input/reject` on this link (the one the input came on), with
    /// the peer's credential, before any `turn/start` with its turn id.
    fn reject_input(self: &Arc<Self>, peer: &PeerInfo, input: &PeerInput, why: InputRefusal) {
        eprintln!("app-peers: {}: input {} refused: {why}", self.cfg.app_id, input.input_id);
        let Some(link) = self.lock().link.clone() else { return };
        let mut params = json!({
            "profile_id": self.cfg.profile_id,
            "session_id": self.cfg.originator,
            "peer": peer.slug,
            "host_token": peer.token,
            "input_id": input.input_id,
        });
        for (k, v) in why.fields().as_object().into_iter().flatten() {
            params[k] = v.clone();
        }
        self.fire(&link, host_tools::PEER_INPUT_REJECT, params);
    }

    /// Start the system agent's input as the peer's turn: the kernel's turn
    /// id and NO origin (the kernel labels a `peer/input` turn
    /// `system_agent` itself and refuses any other label on it).
    fn start_input(self: &Arc<Self>, input: PeerInput) {
        {
            let mut st = self.lock();
            st.peer_turn = Some(input.turn_id.clone());
            remember(&mut st.input_turns, input.turn_id.clone());
            st.note_request(Request {
                turn: input.turn_id.clone(),
                session: input.session_id.clone(),
                lane: LANE_SYSTEM_AGENT,
                text: input.text.clone(),
                speaker: Speaker { kind: host_tools::TurnOrigin::SystemAgent, label: None },
                at: rfc3339_now(),
            });
        }
        let inner = self.clone();
        self.rt().spawn(async move {
            let params = json!({
                "session_id": input.session_id,
                "turn_id": input.turn_id,
                "input": [{"kind": "text", "text": input.text}],
            });
            let turn = input.turn_id.clone();
            let still = move |inner: &Inner| inner.lock().peer_turn.as_deref() == Some(turn.as_str());
            if let Err(e) = inner.start_turn_retrying(params, still).await {
                eprintln!("app-peers: {}: the system agent's input {} did not start: {e}", inner.cfg.app_id, input.input_id);
                inner.peer_turn_ended(&input.turn_id);
            }
        });
    }

    /// `turn/start`, retried while the kernel still runs another turn on
    /// that session (`turn_in_progress`: a turn whose end this host has not
    /// seen yet), until the turn timeout, and only while `still` says the
    /// turn is still wanted.
    async fn start_turn_retrying(self: &Arc<Self>, params: Value, still: impl Fn(&Inner) -> bool) -> Result<Value, String> {
        let until = tokio::time::Instant::now() + self.cfg.turn_timeout;
        let mut wait = Duration::from_millis(200);
        loop {
            match self.request("turn/start", params.clone()).await {
                Err(e) if turn_in_progress(&e) && tokio::time::Instant::now() + wait < until => {
                    tokio::time::sleep(wait).await;
                    wait = (wait * 2).min(Duration::from_secs(2));
                    if !still(self) {
                        return Err("The message was withdrawn".into());
                    }
                }
                other => return other,
            }
        }
    }

    /// The turns running now in both lanes: `(session, turn, lane)`, the
    /// peer's own (the system agent's lane) first, then each open
    /// conversation's (the person's lane).
    fn running_turns(&self) -> Vec<(String, String, &'static str)> {
        let st = self.lock();
        let mut running = Vec::new();
        if let (Some(turn), Some((_, peer))) = (&st.peer_turn, &st.peer) {
            running.push((peer.session.clone(), turn.clone(), LANE_SYSTEM_AGENT));
        }
        let conversations: Vec<Arc<ContextInner>> = st
            .contexts
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|c| c.conversation && c.generation == st.generation && c.open.load(Ordering::Acquire))
            .collect();
        drop(st);
        for conversation in conversations {
            let turn = conversation.turn.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|w| w.turn_id.clone());
            let session = conversation.bound.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|b| b.session.clone());
            if let (Some(turn), Some(session)) = (turn, session) {
                running.push((session, turn, LANE_PERSON));
            }
        }
        running
    }

    /// `turn` no longer holds the peer (it ended, failed to start or was
    /// dropped): the next queued turn starts.
    fn peer_turn_ended(self: &Arc<Self>, turn: &str) {
        let ended = {
            let mut st = self.lock();
            let ours = st.peer_turn.as_deref() == Some(turn);
            if ours {
                st.peer_turn = None;
            }
            ours
        };
        if ended {
            self.next_turn();
        }
    }

    /// The peer is free: the next queued input, if any, takes it.
    fn next_turn(self: &Arc<Self>) {
        let next = {
            let mut st = self.lock();
            if st.peer_turn.is_some() {
                return;
            }
            st.queue.pop_front()
        };
        if let Some(input) = next {
            self.start_input(input);
        }
    }

    /// A `host_tool` approval: the host draws it and answers on this link.
    fn on_host_approval(self: &Arc<Self>, mut approval: HostToolApproval, context: Option<&Arc<ContextInner>>) -> bool {
        let (link, account) = {
            let st = self.lock();
            approval.trigger = st.trigger_of(&approval.turn_id);
            (st.link.clone(), st.account.clone())
        };
        let Some(link) = link else { return false };
        let weak = Arc::downgrade(self);
        let session = approval.session_id.clone();
        let approval_id = approval.approval_id.clone();
        let turn = approval.turn_id.clone();
        let respond_link = link.clone();
        let answer = ApprovalAnswer::with_note(move |approve, note| {
            if let Some(inner) = weak.upgrade() {
                inner.fire(&respond_link, "approval/respond", approval_respond(&session, &approval_id, approve, note));
            }
        });
        let session = approval.session_id.clone();
        let id = approval.approval_id.clone();
        let taken = self.tool_host().host_tool_approval(&self.cfg.app_id, account.as_deref(), approval, answer.clone());
        if taken {
            self.track_prompt(id, Prompt { session, turn, answer: PromptAnswer::HostApproval(answer), context: context.map(Arc::downgrade), link });
        }
        taken
    }

    /// An agent's question: stamped with its origin and handed to the host,
    /// which answers on this link. False when the host did not take it.
    fn on_user_question(self: &Arc<Self>, mut question: AgentQuestion, context: Option<&Arc<ContextInner>>) -> bool {
        let (link, account) = {
            let st = self.lock();
            question.origin = match context {
                Some(_) => CallOrigin::Context,
                None if st.input_turns.contains(&question.turn_id) => CallOrigin::PeerInput,
                None => CallOrigin::PeerOwn,
            };
            // Until octos reports the turn's origin, derive it from what
            // this host knows: a turn of the shared conversation it started
            // says who speaks (the origin it sent, the system agent for a
            // `peer/input`); otherwise what started each turn it started
            // (G2): a turn its starter said the person or the app started
            // is theirs; an unsaid context turn is the person's, any other
            // the app's.
            if !question.origin_reported {
                question.turn_origin = match (st.speaker_of(&question.turn_id), st.trigger_of(&question.turn_id), question.origin) {
                    (Some(speaker), _, _) => speaker.kind,
                    (None, TurnTrigger::SystemAgent, _) | (None, _, CallOrigin::PeerInput) => host_tools::TurnOrigin::SystemAgent,
                    (None, TurnTrigger::Person, _) => host_tools::TurnOrigin::Person,
                    (None, TurnTrigger::App | TurnTrigger::Incoming { .. }, _) => host_tools::TurnOrigin::App,
                    (None, TurnTrigger::Unknown, CallOrigin::Context) => host_tools::TurnOrigin::Person,
                    (None, TurnTrigger::Unknown, _) => host_tools::TurnOrigin::App,
                };
            }
            (st.link.clone(), st.account.clone())
        };
        if let Some(context) = context {
            question.context_id = Some(context.context_id.clone());
            question.client = Some(context.instance.clone());
        }
        let Some(link) = link else { return false };
        let weak = Arc::downgrade(self);
        let session = question.session_id.clone();
        let question_id = question.question_id.clone();
        let turn = question.turn_id.clone();
        let respond_link = link.clone();
        let answer = QuestionAnswer::with_note(move |answers, note| {
            if let Some(inner) = weak.upgrade() {
                inner.lock().questions.remove(&question_id);
                inner.fire(&respond_link, host_tools::USER_QUESTION_RESPOND, question_respond(&session, &question_id, answers, note));
            }
        });
        let id = question.question_id.clone();
        let session = question.session_id.clone();
        let count = question.answer_count();
        let taken = self.tool_host().user_question(&self.cfg.app_id, account.as_deref(), question, answer.clone());
        if taken {
            {
                let mut st = self.lock();
                remember(&mut st.host_held, id.clone());
                st.questions.insert(id.clone(), turn.clone());
            }
            self.track_prompt(id, Prompt { session, turn, answer: PromptAnswer::HostQuestion(answer, count), context: context.map(Arc::downgrade), link });
        }
        taken
    }

    /// An approval or question the host did not take, about to reach the
    /// app's context or conversation: its deadline runs here.
    fn track_unheld(self: &Arc<Self>, method: &str, params: &Value, session: &str, context: Option<&Arc<ContextInner>>) {
        let (id, answer) = match method {
            "approval/requested" => (params.get("approval_id").and_then(Value::as_str), PromptAnswer::AppApproval),
            host_tools::USER_QUESTION_REQUESTED => {
                let count = params.get("questions").and_then(Value::as_array).map_or(1, |q| q.len().max(1));
                (params.get("question_id").and_then(Value::as_str), PromptAnswer::AppQuestion(count))
            }
            _ => return,
        };
        let Some(id) = id.filter(|id| !id.is_empty()) else { return };
        let turn = params.get("turn_id").and_then(Value::as_str).unwrap_or("").to_owned();
        let link = {
            let st = self.lock();
            if st.host_held.iter().any(|h| h == id) || st.prompts.contains_key(id) {
                return;
            }
            st.link.clone()
        };
        let Some(link) = link else { return };
        self.track_prompt(id.to_owned(), Prompt { session: session.to_owned(), turn, answer, context: context.map(Arc::downgrade), link });
    }

    /// Start `id`'s deadline: at it, an unanswered request expires; after
    /// the grace, its turn is interrupted if it still runs.
    fn track_prompt(self: &Arc<Self>, id: String, prompt: Prompt) {
        self.lock().prompts.insert(id.clone(), prompt);
        let (deadline, grace) = (self.cfg.prompt_deadline, self.cfg.expiry_grace);
        let weak = Arc::downgrade(self);
        self.rt().spawn(async move {
            tokio::time::sleep(deadline).await;
            let Some(inner) = weak.upgrade() else { return };
            if !inner.prompt_expired(&id) {
                return;
            }
            drop(inner);
            tokio::time::sleep(grace).await;
            if let Some(inner) = weak.upgrade() {
                inner.expiry_grace_over(&id).await;
            }
        });
    }

    /// `id`'s deadline passed. Answered in time (or its turn ended): true
    /// only when it is expiring now. What the host holds its own router or
    /// request model expires at the same deadline (audit, the sheet);
    /// what the app holds the broker denies or declines here. Never an
    /// approval.
    fn prompt_expired(self: &Arc<Self>, id: &str) -> bool {
        let reason = host_tools::expiry_reason(self.cfg.prompt_deadline);
        let (session, turn, context, app_answer) = {
            let mut st = self.lock();
            let Some(prompt) = st.prompts.get(id) else { return false };
            if prompt.answer.answered() {
                st.prompts.remove(id);
                return false;
            }
            let app_answer = match &prompt.answer {
                PromptAnswer::AppApproval => Some((prompt.link.clone(), approval_respond(&prompt.session, id, false, &host_tools::expired_note(&reason)), "approval/respond")),
                PromptAnswer::AppQuestion(count) => {
                    let answers: Vec<Value> = (0..*count).map(|_| json!({"free_text": host_tools::expired_question_text(&reason)})).collect();
                    Some((prompt.link.clone(), question_respond(&prompt.session, id, Value::Array(answers), &host_tools::expired_note(&reason)), host_tools::USER_QUESTION_RESPOND))
                }
                _ => None,
            };
            (prompt.session.clone(), prompt.turn.clone(), prompt.context.clone(), app_answer)
        };
        eprintln!("app-peers: {}: {id} on turn {turn} expired ({reason})", self.cfg.app_id);
        if let Some((link, params, method)) = app_answer {
            self.fire(&link, method, params);
        }
        // The app hears it expired (its sheet or card says so).
        let params = json!({"session_id": session, "turn_id": turn, "id": id, "reason": reason});
        match context.and_then(|c| c.upgrade()) {
            Some(context) => context.deliver(host_tools::PROMPT_EXPIRED, &params),
            None => self.to_conversations(host_tools::PROMPT_EXPIRED, &params, &session),
        }
        true
    }

    /// The grace after `id` expired is over: a host that has not answered
    /// is answered for (deny or decline), and a turn that still runs is
    /// interrupted, so the peer's next queued turn starts.
    async fn expiry_grace_over(self: &Arc<Self>, id: &str) {
        let Some(prompt) = self.lock().prompts.remove(id) else { return };
        let reason = host_tools::expiry_reason(self.cfg.prompt_deadline);
        let late = match &prompt.answer {
            PromptAnswer::HostApproval(answer) => answer.expire(&reason),
            PromptAnswer::HostQuestion(answer, count) => {
                let late = answer.expire(*count, &reason);
                if late {
                    // The host's request model withdraws it.
                    self.tool_host().user_question_closed(&self.cfg.app_id, id);
                }
                late
            }
            _ => false,
        };
        if late {
            eprintln!("app-peers: {}: the host did not answer the expired {id}; denied or declined here", self.cfg.app_id);
        }
        eprintln!("app-peers: {}: turn {} still runs {}s after {id} expired; interrupting it", self.cfg.app_id, prompt.turn, self.cfg.expiry_grace.as_secs());
        if let Err(e) = self.interrupt_turn(&prompt.session, &prompt.turn).await {
            eprintln!("app-peers: {}: interrupting turn {}: {e}", self.cfg.app_id, prompt.turn);
        }
    }

    /// Interrupt `turn` on `session` (N1: its late calls are refused, its
    /// calls in flight end). On the peer's session it no longer holds the
    /// peer once the kernel took the interrupt: the next queued turn
    /// starts (retried while the kernel still ends this one).
    async fn interrupt_turn(self: &Arc<Self>, session: &str, turn: &str) -> Result<Value, String> {
        self.note_interrupted(turn);
        let result = self.request("turn/interrupt", json!({"session_id": session, "turn_id": turn})).await;
        let peer_session = self.lock().peer.as_ref().map(|(_, p)| p.session.clone());
        if result.is_ok() && peer_session.as_deref() == Some(session) {
            self.peer_turn_ended(turn);
        }
        result
    }

    /// `turn` ended: the `host_tool` approvals of it the host holds and has
    /// not answered are withdrawn there (the kernel dropped them with the
    /// turn), so no sheet waits on a request nothing will run.
    fn close_approvals(&self, turn: &str) {
        let closed: Vec<String> = {
            let st = self.lock();
            st.prompts
                .iter()
                .filter(|(_, p)| p.turn == turn && matches!(&p.answer, PromptAnswer::HostApproval(answer) if !answer.is_sent()))
                .map(|(id, _)| id.clone())
                .collect()
        };
        if closed.is_empty() {
            return;
        }
        let host = self.tool_host();
        for id in closed {
            host.host_tool_approval_closed(&self.cfg.app_id, &id);
        }
    }

    /// `turn` ended: its unanswered questions can no longer be answered.
    fn close_questions(&self, turn: &str) {
        let closed: Vec<String> = {
            let mut st = self.lock();
            let ids: Vec<String> = st.questions.iter().filter(|(_, t)| t.as_str() == turn).map(|(q, _)| q.clone()).collect();
            for id in &ids {
                st.questions.remove(id);
            }
            ids
        };
        if closed.is_empty() {
            return;
        }
        let host = self.tool_host();
        for id in closed {
            host.user_question_closed(&self.cfg.app_id, &id);
        }
    }

    /// Deliver one event of the peer's session to every open conversation
    /// of the current account (octos's own approval answered by the host
    /// once here, not by each of them).
    fn to_conversations(self: &Arc<Self>, method: &str, params: &Value, session: &str) {
        let conversations: Vec<Arc<ContextInner>> = {
            let st = self.lock();
            st.contexts
                .iter()
                .filter_map(Weak::upgrade)
                .filter(|c| c.conversation && c.generation == st.generation)
                .collect()
        };
        if conversations.is_empty() {
            return;
        }
        let mut method = method;
        if let Some(respond) = crate::host_approvals::auto_answer(&self.cfg.app_id, method, session, params) {
            if let Some(id) = respond["approval_id"].as_str() {
                self.lock().prompts.remove(id);
            }
            let broker = self.clone();
            self.rt().spawn(async move {
                if let Err(e) = broker.request("approval/respond", respond).await {
                    eprintln!("app-peers: the host's approval answer failed: {e}");
                }
            });
            method = crate::host_approvals::ANSWERED_BY_HOST;
        }
        for conversation in conversations {
            conversation.deliver(method, params);
        }
    }

    fn close_context_on_kernel(
        self: &Arc<Self>,
        peer: String,
        context_id: String,
        turn: Option<String>,
        session: String,
        token: Option<String>,
    ) {
        let inner = self.clone();
        if let Some(turn) = &turn {
            self.note_interrupted(turn);
        }
        self.rt().spawn(async move {
            if let Some(turn) = turn {
                let _ = inner
                    .request(
                        "turn/interrupt",
                        json!({"session_id": session, "turn_id": turn}),
                    )
                    .await;
            }
            let _ = inner
                .request(
                    "peer/context/close",
                    json!({
                        "profile_id": inner.cfg.profile_id,
                        "session_id": inner.cfg.originator,
                        "peer": peer,
                        "context_id": context_id,
                        "host_token": token,
                    }),
                )
                .await;
        });
    }

    /// Revoke every context (account change, release): they refuse calls,
    /// drop late events and are closed on the kernel.
    fn revoke_contexts(self: &Arc<Self>) {
        let contexts: Vec<Arc<ContextInner>> = {
            let mut st = self.lock();
            let contexts = st.contexts.drain(..).filter_map(|c| c.upgrade()).collect();
            st.routes.clear();
            contexts
        };
        for context in contexts {
            context.revoke(self);
        }
    }
}

/// The lanes of an app agent's conversation (ADR 0004 §6): the peer's own
/// session, which the system agent drives, and the person's sharing
/// context.
pub const LANE_SYSTEM_AGENT: &str = "system_agent";
pub const LANE_PERSON: &str = "person";

/// The session a notification names (v2 envelopes name the base session
/// and carry the topic separately).
fn session_of(params: &Value) -> String {
    match (params["session_id"].as_str(), params["topic"].as_str()) {
        (Some(id), Some(topic)) if !id.contains('#') => format!("{id}#{topic}"),
        (Some(id), _) => id.to_owned(),
        _ => String::new(),
    }
}

/// The turn a notification ends, if it ends one.
fn turn_ended<'a>(method: &str, params: &'a Value) -> Option<&'a str> {
    let ended = match method {
        "projection/envelope" => params["payload"]["type"] == "turn_terminal",
        "turn/completed" | "turn/error" | "turn/interrupted" => true,
        _ => false,
    };
    if ended {
        params.get("turn_id").and_then(Value::as_str)
    } else {
        None
    }
}

/// The turn's persisted answer, else what streamed: the terminal can
/// overtake the transcript lane, so look for THIS turn's answer (its
/// thread) a few times.
async fn saved_answer(inner: &Arc<Inner>, session: &str, turn_id: &str, streamed: String) -> String {
    let mut saved = None;
    for attempt in 0..10 {
        let history = inner
            .request("session/hydrate", json!({"session_id": session, "include": ["messages"]}))
            .await
            .unwrap_or(Value::Null);
        saved = history["messages"]
            .as_array()
            .and_then(|rows| rows.iter().rev().find(|m| m["role"] == "assistant" && (m["turn_id"] == turn_id || m["thread_id"] == turn_id)))
            .and_then(|m| m["content"].as_str())
            .map(str::to_owned);
        if saved.is_some() || attempt == 9 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    saved.unwrap_or(streamed)
}

/// A transcript as a surface shows it: each user message the kernel marked
/// with its speaker gets `speaker` and `display_text` (the text after the
/// marker); `content` keeps the marker.
fn speakers_in_history(mut history: Value) -> Value {
    for row in history["messages"].as_array_mut().into_iter().flatten() {
        if row["role"] != "user" {
            continue;
        }
        let Some((speaker, text)) = row["content"].as_str().and_then(split_origin_marker).map(|(s, t)| (s, t.to_owned())) else { continue };
        row["speaker"] = speaker.to_json();
        row["display_text"] = json!(text);
    }
    history
}

/// A `persisted_at` (RFC 3339, UTC) as a key that sorts by time whatever
/// its fraction digits: `YYYY-MM-DDTHH:MM:SS` then nine fraction digits.
fn time_key(row: &Value) -> Option<String> {
    let at = row["persisted_at"].as_str()?;
    let at = at.strip_suffix('Z').unwrap_or(at);
    let (seconds, fraction) = at.split_once('.').unwrap_or((at, ""));
    let digits: String = fraction.chars().take_while(char::is_ascii_digit).take(9).collect();
    Some(format!("{seconds}.{digits:0<9}"))
}

/// The app's conversation as one history (ADR 0004 §6): the person's lane
/// (its sharing context's transcript) and the system agent's lane (the
/// peer's), each row with its `lane` and, for a marked user message, its
/// `speaker` and `display_text`, merged by `persisted_at` (a row without one
/// keeps its place in its own lane's order). A turn this broker started
/// whose user message the kernel never recorded (it was stopped) gets its
/// request as a user row (`requests`: this conversation's lanes), at the
/// time it started.
fn merged_history(person: Value, system_agent: Value, requests: &[Request]) -> Value {
    let mut out = speakers_in_history(person);
    let lane_rows = |history: Value, lane: &str| -> Vec<Value> {
        let mut rows = match speakers_in_history(history)["messages"].take() {
            Value::Array(rows) => rows,
            _ => Vec::new(),
        };
        for row in &mut rows {
            row["lane"] = json!(lane);
        }
        rows
    };
    let mut rows = lane_rows(json!({"messages": out["messages"].take()}), LANE_PERSON);
    rows.extend(lane_rows(system_agent, LANE_SYSTEM_AGENT));
    for request in requests {
        let recorded = rows.iter().any(|row| {
            row["role"] == "user"
                && row["lane"] == request.lane
                && (row["turn_id"] == request.turn.as_str()
                    || row["thread_id"] == request.turn.as_str()
                    || row["display_text"].as_str().or_else(|| row["content"].as_str()).is_some_and(|t| t.trim() == request.text.trim()))
        });
        if !recorded {
            rows.push(json!({
                "role": "user", "content": request.text, "display_text": request.text,
                "speaker": request.speaker.to_json(), "lane": request.lane,
                "turn_id": request.turn, "persisted_at": request.at, "unrecorded": true,
            }));
        }
    }
    // A stable sort: equal or missing times keep their lane order.
    let mut last = String::new();
    let mut keyed: Vec<(String, Value)> = Vec::with_capacity(rows.len());
    for row in rows {
        if let Some(key) = time_key(&row) {
            last = key;
        }
        keyed.push((last.clone(), row));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    out["messages"] = Value::Array(keyed.into_iter().map(|(_, row)| row).collect());
    out
}

/// `approval/respond` params: the decision and why (`client_note`).
fn approval_respond(session: &str, approval_id: &str, approve: bool, note: &str) -> Value {
    let note = if note.is_empty() { "decided by the host (UPCR-2026-035 host_tool)" } else { note };
    json!({
        "session_id": session,
        "approval_id": approval_id,
        "decision": if approve { "approve" } else { "deny" },
        "client_note": note,
    })
}

/// `user_question/respond` params, with a note when the host gave one.
fn question_respond(session: &str, question_id: &str, answers: Value, note: &str) -> Value {
    let mut params = json!({"session_id": session, "question_id": question_id, "answers": answers});
    if !note.is_empty() {
        params["client_note"] = json!(note);
    }
    params
}

fn rpc_error_text(error: &Value) -> String {
    let message = error["message"].as_str().unwrap_or("request failed");
    match error["data"]["kind"].as_str() {
        Some(kind) => format!("{message} ({kind})"),
        None => message.to_owned(),
    }
}

fn model_info(value: &Value) -> Option<ModelInfo> {
    let lane = value.get("lane")?.as_str()?.to_owned();
    Some(ModelInfo {
        lane,
        provider: value
            .get("provider")
            .and_then(Value::as_str)
            .map(str::to_owned),
        model: value
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

// --------------------------------------------------------------------------

struct Bound {
    session: String,
    peer_slug: String,
    context_id: String,
    token: Option<String>,
}

struct ContextInner {
    broker: Weak<Inner>,
    account: String,
    generation: u64,
    services: BTreeSet<String>,
    context_id: String,
    /// The client instance key the app gave (a Rinx mini app): stamped on
    /// tool calls from this context as their `client`.
    instance: String,
    open: AtomicBool,
    /// The app's conversation (ADR 0004 §6): the person's lane, a request
    /// context opened with `share_history` that also follows the system
    /// agent's lane (the peer's session). `false`: a plain request context.
    conversation: bool,
    bound: Mutex<Option<Bound>>,
    turn: Mutex<Option<TurnWaiter>>,
    sink: Mutex<Option<EventSink>>,
    /// A conversation's follower: every event of both lanes.
    subscriber: Mutex<Option<EventSink>>,
    calls: AtomicU64,
    /// (thread, seq) of v2 envelopes already applied: the kernel may deliver
    /// one twice (replay, shared consumers).
    seen: Mutex<BTreeSet<(String, u64)>>,
}

impl ContextInner {
    /// The lease check, before a request and again before any delivery.
    fn check(&self, inner: &Inner, service: Option<&str>) -> Result<(), String> {
        if !self.open.load(Ordering::Acquire) {
            return Err("This app's assistant access was closed".into());
        }
        let st = inner.lock();
        if st.released {
            return Err("The app was closed".into());
        }
        if st.generation != self.generation || st.account.as_deref() != Some(self.account.as_str())
        {
            return Err("The account changed; reopen this app".into());
        }
        if let Some(service) = service {
            if !inner.cfg.services.contains(service) || !self.services.contains(service) {
                return Err(format!("This app was not granted {service}"));
            }
        }
        Ok(())
    }

    /// An event of this context's own session (the host's answer to
    /// octos's approval first, in developer mode).
    fn notification(&self, method: &str, params: &Value) {
        let Some(inner) = self.broker.upgrade() else {
            return;
        };
        // The host may answer octos's approval itself (developer mode): then
        // the app hears that it was answered, and is never asked.
        let session = session_of(params);
        let mut method = method;
        if let Some(respond) = crate::host_approvals::auto_answer(&inner.cfg.app_id, method, &session, params) {
            if let Some(id) = respond["approval_id"].as_str() {
                inner.lock().prompts.remove(id);
            }
            let broker = inner.clone();
            inner.rt().spawn(async move {
                if let Err(e) = broker.request("approval/respond", respond).await {
                    eprintln!("app-peers: the host's approval answer failed: {e}");
                }
            });
            method = crate::host_approvals::ANSWERED_BY_HOST;
        }
        self.deliver(method, params);
    }

    /// The kernel session this handle is bound to, once bound.
    fn session(&self) -> Option<String> {
        self.bound.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|b| b.session.clone())
    }

    /// Hand one event to this handle: its running turn, the caller's sink
    /// and (a conversation's) its follower.
    fn deliver(&self, method: &str, params: &Value) {
        let Some(inner) = self.broker.upgrade() else {
            return;
        };
        if self.check(&inner, None).is_err() {
            return;
        }
        if method == "projection/envelope" {
            if let (Some(thread), Some(seq)) =
                (params["thread_id"].as_str(), params["seq"].as_u64())
            {
                let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
                if !seen.insert((thread.to_owned(), seq)) {
                    return;
                }
                if seen.len() > 4096 {
                    seen.clear();
                }
            }
        }
        let turn_id = params.get("turn_id").and_then(Value::as_str);
        let mut text_so_far = None;
        let own_turn: bool;
        {
            let mut turn = self.turn.lock().unwrap_or_else(|e| e.into_inner());
            own_turn = turn.as_ref().is_some_and(|w| Some(w.turn_id.as_str()) == turn_id);
            if let Some(waiter) = turn
                .as_mut()
                .filter(|w| Some(w.turn_id.as_str()) == turn_id)
            {
                match method {
                    "projection/envelope" => {
                        if waiter.envelope(&params["payload"]) {
                            text_so_far = Some(waiter.text.clone());
                        }
                    }
                    "message/delta" => {
                        waiter.text.push_str(params["text"].as_str().unwrap_or(""));
                        text_so_far = Some(waiter.text.clone());
                    }
                    "turn/completed" => {
                        let text = waiter.text.clone();
                        waiter.finish(Ok(text));
                    }
                    "turn/error" => {
                        let message = params["message"]
                            .as_str()
                            .or_else(|| params["code"].as_str())
                            .unwrap_or("The assistant's turn failed")
                            .to_owned();
                        waiter.finish(Err(message));
                    }
                    _ => {}
                }
            }
        }
        let mut data = json!({"method": method, "params": params});
        if let Some(text) = text_so_far {
            data["text"] = json!(text);
        }
        if self.conversation {
            // Which lane: this context's own session is the person's; the
            // peer's session is the system agent's.
            let lane = if self.session().is_some_and(|own| own == session_of(params)) { LANE_PERSON } else { LANE_SYSTEM_AGENT };
            data["lane"] = json!(lane);
            // Who speaks, from this host's own record of the turn, else
            // from the kernel's marker on the user message.
            let mut speaker = turn_id.and_then(|t| inner.lock().speaker_of(t));
            let user_text = (method == "projection/envelope" && params["payload"]["type"] == "user_message")
                .then(|| params["payload"]["data"]["text"].as_str())
                .flatten();
            if let Some((marked, text)) = user_text.and_then(split_origin_marker) {
                data["display_text"] = json!(text);
                speaker.get_or_insert(marked);
            }
            if let Some(speaker) = speaker {
                data["speaker"] = speaker.to_json();
            }
            // The turn's words, when it starts: the kernel sends its user
            // message only when the turn ends (never for a stopped one).
            if method == "turn/started" {
                if let Some(request) = turn_id.and_then(|t| inner.lock().request_of(t).map(Request::to_json)) {
                    data["request"] = request;
                }
            }
        }
        // A conversation's caller hears only its own turn; a request
        // context's session is its caller's alone.
        if !self.conversation || own_turn {
            let sink = self.sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(sink) = sink {
                sink(ContextEvent::Data(data.clone()));
            }
        }
        let subscriber = self.subscriber.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(subscriber) = subscriber {
            subscriber(ContextEvent::Data(data));
        }
    }

    fn revoke(&self, inner: &Arc<Inner>) {
        if !self.open.swap(false, Ordering::AcqRel) {
            return;
        }
        let turn = self
            .turn
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .map(|w| w.turn_id);
        self.sink.lock().unwrap_or_else(|e| e.into_inner()).take();
        self.subscriber.lock().unwrap_or_else(|e| e.into_inner()).take();
        // A conversation is a context too: its person's turn stops and its
        // context closes for good (the next handle opens a new one). The
        // system agent's lane goes on.
        if let Some(bound) = self.bound.lock().unwrap_or_else(|e| e.into_inner()).take() {
            inner.lock().routes.remove(&bound.session);
            inner.close_context_on_kernel(
                bound.peer_slug,
                bound.context_id,
                turn,
                bound.session,
                bound.token,
            );
        }
    }

    async fn ensure_bound(self: &Arc<Self>, inner: &Arc<Inner>) -> Result<String, String> {
        if let Some(bound) = self
            .bound
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            return Ok(bound.session.clone());
        }
        let (generation, peer) = inner.ensure_peer().await?;
        if generation != self.generation {
            return Err("The account changed; reopen this app".into());
        }
        let mut open = json!({
            "profile_id": inner.cfg.profile_id,
            "session_id": inner.cfg.originator,
            "peer": peer.slug,
            "context_id": self.context_id,
            "host_token": peer.token,
        });
        if self.conversation {
            // The person's lane: runs in parallel with the peer's session,
            // each shown the other's recent turns (the kernel's defaults).
            open["share_history"] = json!({});
        }
        let result = inner.request("peer/context/open", open).await?;
        // A kernel that ignored `share_history` opened a plain context: the
        // person would talk without the system agent's side, and the system
        // agent would never see the person's. Refuse it.
        if self.conversation && !result["share_history"].is_object() {
            return Err("This assistant kernel cannot share the app's conversation with its system agent \
                        (octos UPCR-2026-034 share_history); update it"
                .into());
        }
        let session = result["session_id"]
            .as_str()
            .ok_or("peer/context/open returned no session")?
            .to_owned();
        inner
            .request(
                "session/open",
                open_params(&session, &inner.cfg.profile_id, result["cwd"].as_str()),
            )
            .await?;
        self.check(inner, None)?;
        inner.lock().routes.insert(
            session.clone(),
            Route {
                generation: self.generation,
                context: Arc::downgrade(self),
            },
        );
        *self.bound.lock().unwrap_or_else(|e| e.into_inner()) = Some(Bound {
            session: session.clone(),
            peer_slug: peer.slug,
            context_id: self.context_id.clone(),
            token: peer.token,
        });
        Ok(session)
    }

    /// A person's (or the app's) message in the app's conversation: a
    /// `turn/start` in this handle's sharing context (the person's lane),
    /// on the link that registered the peer's tools, with who is speaking
    /// (`origin`). It never waits for the system agent's lane; only for this
    /// context's previous turn still ending (`turn_in_progress`, retried).
    /// G2's trigger stays the authority for approval rules; the origin only
    /// says who speaks.
    async fn conversation_turn(self: &Arc<Self>, inner: &Arc<Inner>, session: &str, text: String, trigger: TurnTrigger) -> Result<Value, String> {
        let text = text.trim().to_owned();
        if text.is_empty() || text.len() > 32 * 1024 {
            return Err("Provide text (at most 32 KiB)".into());
        }
        let kind = match trigger {
            // The system agent speaks only through its own `peer/input`.
            TurnTrigger::SystemAgent => return Err("Only the system agent's own input speaks for it".into()),
            // The app itself started the run (its schedule, content that
            // arrived): the app speaks.
            TurnTrigger::App | TurnTrigger::Incoming { .. } => host_tools::TurnOrigin::App,
            // The person in the app's UI or its cards.
            TurnTrigger::Person | TurnTrigger::Unknown => host_tools::TurnOrigin::Person,
        };
        let label = inner.cfg.app_label.trim();
        let speaker = Speaker { kind, label: (!label.is_empty()).then(|| label.to_owned()) };
        let turn_id = uuid::Uuid::new_v4().to_string();
        let (done_tx, done_rx) = oneshot::channel();
        {
            let mut turn = self.turn.lock().unwrap_or_else(|e| e.into_inner());
            if turn.is_some() {
                return Err("This app already has an assistant turn running".into());
            }
            *turn = Some(TurnWaiter::new(turn_id.clone(), done_tx));
        }
        {
            let mut st = inner.lock();
            st.note_trigger(&turn_id, trigger);
            st.speakers.insert(turn_id.clone(), speaker.clone());
            st.note_request(Request {
                turn: turn_id.clone(),
                session: session.to_owned(),
                lane: LANE_PERSON,
                text: text.clone(),
                speaker: speaker.clone(),
                at: rfc3339_now(),
            });
        }
        let params = json!({
            "session_id": session,
            "turn_id": turn_id,
            "input": [{"kind": "text", "text": text}],
            "origin": speaker.to_json(),
        });
        let me = Arc::downgrade(self);
        let wanted = turn_id.clone();
        let still = move |_: &Inner| {
            me.upgrade().is_some_and(|c| c.turn.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|w| w.turn_id == wanted))
        };
        let outcome = match inner.start_turn_retrying(params, still).await {
            Err(err) => Err(err),
            Ok(_) => match tokio::time::timeout(inner.cfg.turn_timeout, done_rx).await {
                Ok(Ok(result)) => result,
                Ok(Err(_)) => Err("The assistant's turn was cancelled".into()),
                Err(_) => {
                    inner.note_interrupted(&turn_id);
                    let _ = inner.request("turn/interrupt", json!({"session_id": session, "turn_id": turn_id})).await;
                    Err("The assistant's turn timed out".into())
                }
            },
        };
        {
            let mut turn = self.turn.lock().unwrap_or_else(|e| e.into_inner());
            if turn.as_ref().is_some_and(|w| w.turn_id == turn_id) {
                turn.take();
            }
        }
        let streamed = outcome?;
        let text = saved_answer(inner, session, &turn_id, streamed).await;
        Ok(json!({"turn_id": turn_id, "text": text, "speaker": speaker.to_json(), "lane": LANE_PERSON}))
    }

    /// Stop on the app's conversation: BOTH lanes' running turns, the
    /// person's (this handle's) and the system agent's on the peer's
    /// session (the person owns the device). Each stopped turn with its
    /// lane and speaker.
    async fn stop_both_lanes(self: &Arc<Self>, inner: &Arc<Inner>, session: &str) -> Result<Value, String> {
        let mut running = Vec::new();
        let own = self.turn.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|w| w.turn_id.clone());
        if let Some(own) = own {
            running.push((session.to_owned(), own, LANE_PERSON));
        }
        {
            let st = inner.lock();
            if let (Some(turn), Some((_, peer))) = (&st.peer_turn, &st.peer) {
                running.push((peer.session.clone(), turn.clone(), LANE_SYSTEM_AGENT));
            }
        }
        if running.is_empty() {
            return Err("Nothing is running in this conversation".into());
        }
        let mut stopped = Vec::new();
        for (session, turn, lane) in running {
            let speaker = inner.lock().speaker_of(&turn).map(|s| s.to_json());
            inner.interrupt_turn(&session, &turn).await?;
            stopped.push(json!({"turn_id": turn, "lane": lane, "speaker": speaker}));
        }
        let turns: Vec<Value> = stopped.iter().map(|t| t["turn_id"].clone()).collect();
        Ok(json!({"interrupted": turns, "turns": stopped}))
    }

    async fn run(self: &Arc<Self>, inner: &Arc<Inner>, op: ContextOp) -> Result<Value, String> {
        let session = self.ensure_bound(inner).await?;
        self.check(inner, Some(op.service()))?;
        // One turn path; what started it is recorded per turn id below.
        let (op, trigger) = match op.turn() {
            Some((text, trigger)) => (ContextOp::Turn { text: text.to_owned() }, trigger),
            None => (op, TurnTrigger::Unknown),
        };
        let result = match op {
            ContextOp::Open => Ok(json!({
                "open": true,
                "conversation": self.conversation,
                "shared_history": self.conversation,
                "model": inner.lock().model.as_ref().map(|m| json!({
                    "lane": m.lane, "provider": m.provider, "model": m.model,
                })),
            })),
            ContextOp::History => {
                let history = inner
                    .request(
                        "session/hydrate",
                        json!({"session_id": session, "include": ["messages"]}),
                    )
                    .await;
                if self.conversation {
                    // Both lanes: the person's context and the peer's session.
                    let peer_session = inner.lock().peer.as_ref().map(|(_, p)| p.session.clone());
                    let system_agent = match peer_session.clone() {
                        Some(peer) => inner.request("session/hydrate", json!({"session_id": peer, "include": ["messages"]})).await?,
                        None => json!({"messages": []}),
                    };
                    let requests: Vec<Request> = inner.lock().requests.iter().filter(|r| r.session == session || peer_session.as_deref() == Some(r.session.as_str())).cloned().collect();
                    history.map(|person| merged_history(person, system_agent, &requests))
                } else {
                    history
                }
            }
            ContextOp::Turn { text } if self.conversation => self.conversation_turn(inner, &session, text, trigger).await,
            // Stop, on the app's conversation: both lanes (the person owns
            // the device, so the system agent's turn stops too).
            ContextOp::Interrupt if self.conversation => self.stop_both_lanes(inner, &session).await,
            ContextOp::Turn { text } => {
                let text = text.trim().to_owned();
                if text.is_empty() || text.len() > 32 * 1024 {
                    return Err("Provide text (at most 32 KiB)".into());
                }
                let turn_id = uuid::Uuid::new_v4().to_string();
                inner.lock().note_trigger(&turn_id, trigger);
                let (done_tx, done_rx) = oneshot::channel();
                {
                    let mut turn = self.turn.lock().unwrap_or_else(|e| e.into_inner());
                    if turn.is_some() {
                        return Err("This app already has an assistant turn running".into());
                    }
                    *turn = Some(TurnWaiter::new(turn_id.clone(), done_tx));
                }
                let started = inner
                    .request(
                        "turn/start",
                        json!({
                            "session_id": session,
                            "turn_id": turn_id,
                            "input": [{"kind": "text", "text": text}],
                        }),
                    )
                    .await;
                let outcome = match started {
                    Err(err) => Err(err),
                    Ok(_) => match tokio::time::timeout(inner.cfg.turn_timeout, done_rx).await {
                        Ok(Ok(result)) => result,
                        Ok(Err(_)) => Err("The assistant's turn was cancelled".into()),
                        Err(_) => {
                            inner.note_interrupted(&turn_id);
                            let _ = inner
                                .request(
                                    "turn/interrupt",
                                    json!({"session_id": session, "turn_id": turn_id}),
                                )
                                .await;
                            Err("The assistant's turn timed out".into())
                        }
                    },
                };
                {
                    let mut turn = self.turn.lock().unwrap_or_else(|e| e.into_inner());
                    if turn.as_ref().is_some_and(|w| w.turn_id == turn_id) {
                        turn.take();
                    }
                }
                let streamed = outcome?;
                let text = saved_answer(inner, &session, &turn_id, streamed).await;
                Ok(json!({"turn_id": turn_id, "text": text}))
            }
            ContextOp::TurnFrom { .. } => Err("a turn reached the broker unnormalized".into()),
            ContextOp::Interrupt => {
                let turn = self
                    .turn
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .map(|w| w.turn_id.clone())
                    .ok_or("No assistant turn is running")?;
                inner.note_interrupted(&turn);
                inner
                    .request(
                        "turn/interrupt",
                        json!({"session_id": session, "turn_id": turn}),
                    )
                    .await
            }
            ContextOp::Approval { id, approve } => {
                // What the host holds (a `host_tool` approval, an agent's
                // question) only the host answers, on the person's word.
                {
                    let mut st = inner.lock();
                    if st.host_held.contains(&id) {
                        return Err("The person answers this in OctoSense, not the app".into());
                    }
                    // The app answered it: its deadline stops.
                    st.prompts.remove(&id);
                }
                inner
                    .request(
                        "approval/respond",
                        json!({
                            "session_id": session,
                            "approval_id": id,
                            "decision": if approve { "approve" } else { "deny" },
                        }),
                    )
                    .await
            }
        };
        // Drop a reply that outlived its lease.
        self.check(inner, None)?;
        result
    }
}

/// A request context handed to the app.
pub struct BrokerContext(Arc<ContextInner>);

impl OctosContext for BrokerContext {
    fn call(&self, op: ContextOp, sink: EventSink) -> Result<(), String> {
        let inner = self
            .0
            .broker
            .upgrade()
            .ok_or("The assistant service is gone")?;
        self.0.check(&inner, Some(op.service()))?;
        if op.turn().is_some()
            && self
                .0
                .turn
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
        {
            return Err("This app already has an assistant turn running".into());
        }
        *self.0.sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(sink.clone());
        self.0.calls.fetch_add(1, Ordering::Relaxed);
        let context = self.0.clone();
        inner.rt().spawn({
            let inner = inner.clone();
            async move {
                let result = context.run(&inner, op).await;
                // Stale replies are dropped, never delivered to a new
                // account or instance.
                if context.check(&inner, None).is_ok() {
                    sink(ContextEvent::Complete(result));
                }
            }
        });
        Ok(())
    }

    fn close(&self) {
        if let Some(inner) = self.0.broker.upgrade() {
            self.0.revoke(&inner);
        } else {
            self.0.open.store(false, Ordering::Release);
        }
    }

    fn is_open(&self) -> bool {
        self.0.open.load(Ordering::Acquire)
    }

    fn subscribe(&self, sink: Option<EventSink>) {
        if self.0.conversation && self.0.open.load(Ordering::Acquire) {
            *self.0.subscriber.lock().unwrap_or_else(|e| e.into_inner()) = sink;
        }
    }
}

impl OctosAppService for Broker {
    fn deployment(&self) -> Deployment {
        self.0.cfg.deployment
    }

    fn availability(&self) -> Availability {
        if self.0.cfg.services.is_empty() {
            return Availability::Unavailable("This app has no assistant access".into());
        }
        if let Err(reason) = self.0.connector.available() {
            return Availability::Unavailable(reason);
        }
        let st = self.0.lock();
        if st.released {
            return Availability::Unavailable("The app was closed".into());
        }
        if st.account.is_none() {
            return Availability::Unavailable("Sign in to use the assistant".into());
        }
        if st.peer.as_ref().is_some_and(|(g, _)| *g == st.generation) && st.link.is_some() {
            return Availability::Ready;
        }
        match &st.last_error {
            Some(err) => Availability::Failed(err.clone()),
            None => Availability::Idle,
        }
    }

    fn services(&self) -> BTreeSet<String> {
        self.0.cfg.services.clone()
    }

    fn prepare(&self) -> Result<(), String> {
        self.bind()
    }

    fn peer_slug(&self) -> Option<String> {
        self.peer().map(|(slug, _)| slug)
    }

    fn model(&self) -> Option<ModelInfo> {
        self.0.lock().model.clone()
    }

    fn settings_entry(&self) -> SettingsEntry {
        self.0.cfg.settings_entry
    }

    fn set_account(&self, account: Option<&str>) {
        let changed = {
            let mut st = self.0.lock();
            if st.account.as_deref() == account {
                None
            } else {
                let previous = std::mem::replace(&mut st.account, account.map(str::to_owned));
                st.generation += 1;
                st.peer = None;
                st.model = None;
                Some(previous)
            }
        };
        if let Some(previous) = changed {
            // The host's account lifecycle (ADR 0004 §11) first: a sign-out
            // suspends before the contexts go, a sign-in resumes before the
            // peer is prepared below.
            crate::storage::account_changed(&self.0.cfg.app_id, previous.as_deref(), account);
            self.0.revoke_contexts();
            // Create or resume the new account's peer now (no inference), so
            // the system agent can address it from launch on.
            if account.is_some()
                && !self.0.cfg.services.is_empty()
                && self.0.connector.available().is_ok()
            {
                let inner = self.0.clone();
                self.0.rt().spawn(async move {
                    let _ = inner.ensure_peer().await;
                });
            }
        }
    }

    fn open_context(&self, spec: ContextSpec) -> Result<Arc<dyn OctosContext>, String> {
        self.open_handle(spec, false)
    }

    fn open_conversation(&self, spec: ContextSpec) -> Result<Arc<dyn OctosContext>, String> {
        self.open_handle(spec, true)
    }

    fn release(&self) {
        let peer_turn = {
            let mut st = self.0.lock();
            if st.released {
                return;
            }
            st.released = true;
            let turn = st.peer_turn.take();
            turn.zip(st.peer.as_ref().map(|(_, p)| p.session.clone()))
        };
        self.0.revoke_contexts();
        // Conservative background policy (ADR 0007 open question): closing
        // the app stops its peer's running turn too. The peer and its state
        // stay for the next launch; nothing else is stopped.
        if let Some((turn, _)) = &peer_turn {
            self.0.note_interrupted(turn);
        }
        let inner = self.0.clone();
        self.0.rt().spawn(async move {
            if let Some((turn, session)) = peer_turn {
                let _ = inner
                    .request(
                        "turn/interrupt",
                        json!({"session_id": session, "turn_id": turn}),
                    )
                    .await;
            }
            // Let the context closes above reach the kernel, then let go of
            // the link: the shared kernel keeps serving other consumers.
            tokio::time::sleep(Duration::from_millis(500)).await;
            inner.lock().link.take();
        });
    }

    fn set_tool_executor(&self, executor: Option<Arc<dyn host_tools::ToolExecutor>>) {
        self.0.tool_host().set_executor(&self.0.cfg.app_id, executor);
    }

    fn set_confirm_sheet(&self, sheet: Option<Arc<dyn host_tools::ConfirmSheet>>) {
        self.0.tool_host().set_confirm_sheet(&self.0.cfg.app_id, sheet);
    }

    fn shutdown(&self) {
        self.release();
        if self.0.connector.owns_runtime() {
            // Give the release a moment to reach the kernel first.
            std::thread::sleep(Duration::from_millis(600));
            self.0.connector.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_ids_are_kernel_safe_and_nonce_scoped() {
        assert_eq!(
            context_id("ab12cd34", "dev.example.app#7"),
            "ab12cd34-dev-example-app-7"
        );
        assert_eq!(context_id("ab12cd34", "///"), "ab12cd34");
        let long = context_id("ab12cd34", &"x".repeat(200));
        assert!(long.len() <= 64, "{}", long.len());
        assert!(long
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));
    }

    #[test]
    fn a_saved_answer_wins_over_late_deltas_of_its_segment() {
        let (tx, mut rx) = oneshot::channel();
        let mut waiter = TurnWaiter::new("t".into(), tx);
        let env = |kind: &str, segment: &str, text: &str| json!({"type": kind, "data": {"assistant_segment_id": segment, "text": text}});
        waiter.envelope(&env("assistant_delta", "a", "Hel"));
        waiter.envelope(&env("assistant_persisted", "a", "Hello there"));
        assert!(!waiter.envelope(&env("assistant_delta", "a", "lo there")));
        assert_eq!(waiter.text, "Hello there");
        waiter.envelope(&env("assistant_delta", "b", "Next"));
        assert_eq!(waiter.text, "Next");
        waiter.envelope(&json!({"type": "turn_terminal", "data": {"outcome": "completed"}}));
        assert_eq!(rx.try_recv().unwrap().unwrap(), "Next");
    }

    #[test]
    fn namespaces_scope_app_and_account_without_revealing_the_account() {
        let a = app_namespace("rinx", "@alice:example.org");
        let b = app_namespace("rinx", "@bob:example.org");
        assert_ne!(a, b);
        assert!(a.starts_with("app/rinx/acct-"));
        assert!(!a.contains("alice"));
        assert_eq!(a, app_namespace("rinx", "@alice:example.org"));
    }
}
