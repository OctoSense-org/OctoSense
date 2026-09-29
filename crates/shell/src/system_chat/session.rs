//! The system chat's session driver: one kernel connection, the system
//! agent's session, requests and their replies, reconnect and resume.
//!
//! It speaks OUP over a [`Link`], which is the shell's EXISTING host
//! connection to its one kernel (`octosense_kernel::connect()`: stdio, or
//! the host-token WebSocket while Talk to Octos is on; see `link.rs`). It
//! never starts a second kernel and never holds an external token. Tests
//! drive it with a scripted link.
//!
//! - **Open**: `session/open {session_id: SYSTEM_SESSION, profile_id:
//!   "_main"}`, then `session/hydrate {include: ["messages"]}` for the
//!   history (the `session/messages_page` fallback when hydrate is
//!   refused).
//! - **Send**: `turn/start` with a fresh turn id; **Stop**:
//!   `turn/interrupt`; **New conversation**: the kernel's `/new` (it clears
//!   the conversation without calling the model), then the history again.
//! - **Questions**: `user_question/respond`. **Approvals**: the driver
//!   never decides one: it answers `approval/respond` only with what the
//!   shell's approval router decided ([`Command::Approval`]).
//! - **The kernel restarts** (a provider change, Settings' restart, a
//!   crash): the link ends, the driver reconnects with a back-off and
//!   reopens the same session, so the conversation resumes; a turn that
//!   was running is reported as stopped.

use super::model::{ChatModel, Effect, Phase};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The system agent's conversation (octosense-kernel `SYSTEM_SESSION`).
pub const SYSTEM_SESSION: &str = "_main:api:octosense#system";
pub const SYSTEM_PROFILE: &str = "_main";

/// How a link ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Closed {
    /// The kernel was restarted on purpose (reconnect at once).
    pub restarted: bool,
    pub why: String,
}

/// One frame, nothing yet, or the end.
#[derive(Debug)]
pub enum Recv {
    Frame(String),
    Idle,
    Closed(Closed),
}

/// A connection to the kernel.
pub trait Link: Send {
    fn send(&mut self, frame: String) -> Result<(), Closed>;
    /// The next frame, waiting at most `wait`.
    fn recv(&mut self, wait: Duration) -> Recv;
}

/// Why no link could be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unavailable {
    /// No kernel on this shell at all (no binary, iOS): nothing to retry
    /// until something changes.
    NoKernel(String),
    /// No model provider is configured.
    NoProvider,
    /// It failed to start; retry.
    Failed(String),
}

/// Makes links to the shell's kernel.
pub trait Connector: Send {
    fn connect(&mut self) -> Result<Box<dyn Link>, Unavailable>;
}

/// What the UI asks of the driver.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Connect (the pane opened).
    Open,
    /// Let the connection go when nothing runs (the pane closed).
    Close,
    Send(String),
    Interrupt,
    NewConversation,
    /// Answer the open question with one option or free text.
    Answer { question: String, count: usize, text: String, option: bool },
    /// The approval router's decision for one approval (never the pane's).
    Approval { approval_id: String, approve: bool },
}

#[derive(Clone, Debug)]
enum Pending {
    Open,
    History { fallback: bool },
    Turn { turn: String },
    NewConversation,
    Other(&'static str),
}

/// The driver. [`Driver::step`] does one round: it is what the chat's
/// thread loops on, and what tests call.
pub struct Driver {
    connector: Box<dyn Connector>,
    link: Option<Box<dyn Link>>,
    pub model: ChatModel,
    /// Approvals to hand to the router (drained by the shell).
    pub effects: Vec<Effect>,
    pending: HashMap<String, Pending>,
    next_id: u64,
    /// The pane wants a connection.
    wanted: bool,
    retry_at: Option<Instant>,
    backoff: Duration,
    /// Commands that wait for the session to open.
    queued: Vec<Command>,
    opened: bool,
}

impl Driver {
    pub fn new(connector: Box<dyn Connector>) -> Self {
        Driver {
            connector,
            link: None,
            model: ChatModel::new(),
            effects: Vec::new(),
            pending: HashMap::new(),
            next_id: 0,
            wanted: false,
            retry_at: None,
            backoff: Duration::from_millis(500),
            queued: Vec::new(),
            opened: false,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.link.is_some()
    }

    fn request(&mut self, method: &str, params: Value, pending: Pending) -> bool {
        self.next_id += 1;
        let id = format!("syschat-{}", self.next_id);
        let frame = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
        let Some(link) = self.link.as_mut() else { return false };
        match link.send(frame) {
            Ok(()) => {
                self.pending.insert(id, pending);
                true
            }
            Err(closed) => {
                self.lost(closed);
                false
            }
        }
    }

    /// One command from the UI.
    pub fn command(&mut self, cmd: Command) {
        match cmd {
            Command::Open => {
                self.wanted = true;
                if self.link.is_none() {
                    self.retry_at = None;
                    self.try_connect();
                }
            }
            Command::Close => {
                self.wanted = false;
                if self.model.phase().running_turn().is_none() {
                    self.drop_link();
                    self.model.set_phase(Phase::Idle);
                }
            }
            other if !self.opened => {
                // Not open yet: connect, and do it once the session is open.
                self.wanted = true;
                self.queued.push(other);
                if self.link.is_none() {
                    self.try_connect();
                }
            }
            Command::Send(text) => {
                let text = text.trim().to_string();
                if text.is_empty() {
                    return;
                }
                if let Some(turn) = self.model.phase().running_turn() {
                    let _ = turn;
                    self.model.notice("Wait for the current answer, or stop it first.");
                    return;
                }
                let turn = new_turn_id();
                self.model.start_turn(&turn, &text);
                let params = json!({"session_id": SYSTEM_SESSION, "turn_id": turn, "input": [{"kind": "text", "text": text}]});
                self.request("turn/start", params, Pending::Turn { turn });
            }
            Command::Interrupt => {
                if let Some(turn) = self.model.phase().running_turn().map(str::to_string) {
                    self.request("turn/interrupt", json!({"session_id": SYSTEM_SESSION, "turn_id": turn}), Pending::Other("turn/interrupt"));
                    self.model.end_turn(&turn, Some("Stopped."));
                }
            }
            Command::NewConversation => {
                if self.model.phase().running_turn().is_some() {
                    self.model.notice("Stop the current answer before starting a new conversation.");
                    return;
                }
                self.model.clear();
                let turn = new_turn_id();
                let params = json!({"session_id": SYSTEM_SESSION, "turn_id": turn, "input": [{"kind": "text", "text": "/new"}]});
                self.request("turn/start", params, Pending::NewConversation);
            }
            Command::Answer { question, count, text, option } => {
                let one = if option { json!({"selected_labels": [text]}) } else { json!({"free_text": text}) };
                let answers: Vec<Value> = (0..count.max(1)).map(|_| one.clone()).collect();
                self.model.question_answered(&question, &text);
                self.request("user_question/respond", json!({"session_id": SYSTEM_SESSION, "question_id": question, "answers": answers}), Pending::Other("user_question/respond"));
            }
            Command::Approval { approval_id, approve } => {
                self.model.approval_decided(&approval_id, approve);
                let decision = if approve { "approve" } else { "deny" };
                self.request(
                    "approval/respond",
                    json!({"session_id": SYSTEM_SESSION, "approval_id": approval_id, "decision": decision, "client_note": "decided by the OctoSense shell's approval router"}),
                    Pending::Other("approval/respond"),
                );
            }
        }
    }

    fn drop_link(&mut self) {
        self.link = None;
        self.pending.clear();
        self.opened = false;
    }

    fn try_connect(&mut self) {
        if !self.wanted {
            return;
        }
        self.model.set_phase(Phase::Connecting);
        match self.connector.connect() {
            Ok(link) => {
                self.link = Some(link);
                self.backoff = Duration::from_millis(500);
                self.retry_at = None;
                self.request("session/open", json!({"session_id": SYSTEM_SESSION, "profile_id": SYSTEM_PROFILE}), Pending::Open);
            }
            Err(Unavailable::NoKernel(why)) => {
                self.model.set_phase(Phase::NoKernel(why));
                // Something may change (a binary installed): look again later.
                self.retry_at = Some(Instant::now() + Duration::from_secs(5));
            }
            Err(Unavailable::NoProvider) => {
                self.model.set_phase(Phase::NoProvider);
                self.retry_at = Some(Instant::now() + Duration::from_secs(3));
            }
            Err(Unavailable::Failed(why)) => self.schedule_retry(why),
        }
    }

    fn schedule_retry(&mut self, why: String) {
        self.model.set_phase(Phase::Reconnecting(why));
        self.retry_at = Some(Instant::now() + self.backoff);
        self.backoff = (self.backoff * 2).min(Duration::from_secs(10));
    }

    /// The link ended: report a running turn as stopped, then reconnect
    /// (at once after a deliberate restart) and resume the session.
    fn lost(&mut self, closed: Closed) {
        self.drop_link();
        if let Some(turn) = self.model.phase().running_turn().map(str::to_string) {
            self.model.end_turn(&turn, Some("The assistant restarted; that answer stopped. Ask again to continue."));
        }
        let why = if closed.restarted { "The assistant is restarting".to_string() } else { format!("The assistant stopped: {}", closed.why) };
        if closed.restarted {
            self.backoff = Duration::from_millis(200);
        }
        if self.wanted {
            self.schedule_retry(why);
        } else {
            self.model.set_phase(Phase::Idle);
        }
    }

    /// One round: retry a connection that is due, then take frames for up
    /// to `wait`.
    pub fn step(&mut self, wait: Duration) {
        if self.link.is_none() {
            if self.wanted && self.retry_at.is_some_and(|t| Instant::now() >= t) {
                self.retry_at = None;
                self.try_connect();
            }
            if self.link.is_none() {
                if !wait.is_zero() {
                    std::thread::sleep(wait.min(Duration::from_millis(100)));
                }
                return;
            }
        }
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let Some(link) = self.link.as_mut() else { return };
            match link.recv(left) {
                Recv::Frame(frame) => self.frame(&frame),
                Recv::Idle => return,
                Recv::Closed(closed) => {
                    self.lost(closed);
                    return;
                }
            }
            if Instant::now() >= deadline {
                return;
            }
        }
    }

    fn frame(&mut self, text: &str) {
        let Ok(frame) = serde_json::from_str::<Value>(text) else { return };
        if let Some(id) = frame.get("id").filter(|_| frame.get("result").is_some() || frame.get("error").is_some()) {
            let key = id.as_str().map(str::to_string).unwrap_or_else(|| id.to_string());
            if let Some(pending) = self.pending.remove(&key) {
                self.reply(pending, frame.get("result"), frame.get("error").filter(|e| !e.is_null()));
            }
            return;
        }
        let Some(method) = frame.get("method").and_then(Value::as_str) else { return };
        let params = frame.get("params").cloned().unwrap_or(Value::Null);
        // Only the system conversation (the router sends only the sessions
        // this connection opened; be strict anyway).
        let session = match (params.get("session_id").and_then(Value::as_str), params.get("topic").and_then(Value::as_str)) {
            (Some(id), Some(topic)) if !id.contains('#') => format!("{id}#{topic}"),
            (Some(id), _) => id.to_string(),
            _ => String::new(),
        };
        if session != SYSTEM_SESSION {
            return;
        }
        let effects = self.model.apply(method, &params);
        self.effects.extend(effects);
    }

    fn reply(&mut self, pending: Pending, result: Option<&Value>, error: Option<&Value>) {
        let error_text = error.map(|e| e.get("message").and_then(Value::as_str).unwrap_or("error").to_string());
        match pending {
            Pending::Open => match error_text {
                Some(why) => {
                    let lower = why.to_lowercase();
                    if lower.contains("provider") || lower.contains("profile") || lower.contains("llm") || lower.contains("model") {
                        self.model.set_phase(Phase::NoProvider);
                    } else {
                        self.model.notice(format!("Could not open the conversation: {why}"));
                        self.model.set_phase(Phase::Ready);
                    }
                    self.drop_link();
                    self.retry_at = Some(Instant::now() + Duration::from_secs(3));
                }
                None => {
                    self.opened = true;
                    self.model.set_phase(Phase::Ready);
                    self.load_history();
                    for cmd in std::mem::take(&mut self.queued) {
                        self.command(cmd);
                    }
                }
            },
            Pending::History { fallback } => match (error_text, result) {
                (None, Some(result)) => {
                    let rows = if fallback { &result["messages"] } else { &result["messages"] };
                    self.model.load_history(rows);
                }
                (Some(_), _) if !fallback => {
                    self.request("session/messages_page", json!({"session_id": SYSTEM_SESSION, "limit": 200}), Pending::History { fallback: true });
                }
                (Some(why), _) => self.model.notice(format!("Could not load the conversation: {why}")),
                _ => {}
            },
            Pending::Turn { turn } => {
                if let Some(why) = error_text {
                    self.model.end_turn(&turn, Some(&format!("The assistant could not start: {why}")));
                }
            }
            Pending::NewConversation => {
                if let Some(why) = error_text {
                    self.model.notice(format!("Could not start a new conversation: {why}"));
                }
                self.load_history();
            }
            Pending::Other(what) => {
                if let Some(why) = error_text {
                    self.model.notice(format!("{what}: {why}"));
                }
            }
        }
    }

    fn load_history(&mut self) {
        self.request("session/hydrate", json!({"session_id": SYSTEM_SESSION, "include": ["messages"]}), Pending::History { fallback: false });
    }
}

/// A random version-4 UUID (octos turn ids are UUIDs), from the standard
/// library's randomly seeded hasher, the clock and a counter.
pub fn new_turn_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut words = [0u64; 2];
    for (i, word) in words.iter_mut().enumerate() {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(nanos);
        h.write_u64(n);
        h.write_usize(i);
        *word = h.finish();
    }
    let mut b = [0u8; 16];
    b[..8].copy_from_slice(&words[0].to_le_bytes());
    b[8..].copy_from_slice(&words[1].to_le_bytes());
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}
