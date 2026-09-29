//! The system chat's model: the conversation as the pane draws it, built
//! only from the kernel's UI Protocol frames (OUP `octos-ui/v1alpha1`) and
//! the person's own actions. No drawing and no I/O here, so it is tested
//! frame by frame.
//!
//! - **Streamed text**: `message/delta` (and a v2 `projection/envelope`'s
//!   `assistant_delta` / `assistant_persisted`) grow the turn's assistant
//!   message.
//! - **Tool calls** with their status: `tool/started` → running,
//!   `tool/progress` → its message, `tool/completed` → done or failed.
//! - **Questions**: `user_question/requested` (the system agent's
//!   `ask_user_question`), answered from the pane.
//! - **Approvals** are NOT answered here: `approval/requested` becomes an
//!   [`Effect::Approval`] for the shell's approval router (ADR 0004 §8); the
//!   pane only notes that the sheet is waiting and what was decided. Only a
//!   turn this chat started ([`ChatModel::start_turn`]) is the system
//!   agent's on the person's behalf; an approval of any other turn on the
//!   session (an external client's, e.g. Talk to Octos) is marked
//!   [`ApprovalAsk::external`] and shown read-only: that client answers it.
//! - **History**: `session/hydrate`'s rows replace the transcript.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolStatus {
    Running,
    Done,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalState {
    /// On the shell's sheet, waiting for the person.
    Waiting,
    Approved,
    Denied,
    /// The kernel withdrew it (the turn ended).
    Cancelled,
    /// Another client's turn asked it: shown read-only, that client
    /// answers it.
    External,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Message {
        role: Role,
        text: String,
        turn: Option<String>,
    },
    Tool {
        call_id: String,
        name: String,
        status: ToolStatus,
        detail: String,
        turn: Option<String>,
    },
    Question {
        id: String,
        turn: String,
        title: String,
        body: String,
        /// The first question's options (labels).
        options: Vec<String>,
        /// How many questions the kernel asked (each is answered).
        count: usize,
        answered: Option<String>,
    },
    /// A tool approval the shell's router holds; the sheet shows the call.
    Approval {
        id: String,
        tool: String,
        title: String,
        state: ApprovalState,
    },
    /// Something the person should know (an error, a restart).
    Notice(String),
}

/// Where the conversation stands.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Phase {
    /// Not connected (the pane is closed, or has not connected yet).
    #[default]
    Idle,
    Connecting,
    /// No kernel on this shell (no binary, iOS, a build without one).
    NoKernel(String),
    /// No model provider is configured: nothing to talk to.
    NoProvider,
    /// Open and waiting for the person.
    Ready,
    /// A turn is running (the pane's own, or another client's on the same
    /// system conversation).
    Running { turn: String },
    /// The kernel went away; the chat reconnects and resumes the session.
    Reconnecting(String),
}

impl Phase {
    pub fn running_turn(&self) -> Option<&str> {
        match self {
            Phase::Running { turn } => Some(turn),
            _ => None,
        }
    }
}

/// An approval the chat hands to the approval router.
#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalAsk {
    pub approval_id: String,
    pub turn: String,
    pub tool: String,
    pub title: String,
    pub body: String,
    /// The exact arguments as the kernel gave them (typed details when
    /// present, else the title and body).
    pub args: Value,
    /// The person's words that started the turn: the batch's plan.
    pub plan: String,
    /// A host-routed tool's approval (`approval_kind: "host_tool"`,
    /// UPCR-2026-035): the app that owns the tool. `tool` and `args` are
    /// then the declared name and the exact arguments.
    pub app: Option<String>,
    /// The same call ran before and its outcome is unknown.
    pub outcome_unknown: bool,
    /// The turn is not one this chat started (an external client's turn on
    /// the same session): never the shell's to answer.
    pub external: bool,
}

/// What the driver must do after a frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Approval(ApprovalAsk),
    /// The approval's outcome is known; the router's pending request, if
    /// any, is moot.
    ApprovalGone(String),
    /// A `peer/tool/call` of the system agent's (a host tool registered on
    /// this session, UPCR-2026-035): for the shell's relay, answered once
    /// through `reply` on this link.
    ToolCall { call: crate::ai_host::app_peers::host_tools::HostToolCall, reply: crate::ai_host::app_peers::host_tools::ToolReply },
    /// The kernel cancelled a call, or the link that carried it ended.
    ToolCancel(String),
}

#[derive(Clone, Debug, Default)]
pub struct ChatModel {
    pub items: Vec<Item>,
    pub phase: Phase,
    /// Bumped on every visible change.
    pub generation: u64,
    /// The prompt of each turn this model saw start (for approval batches).
    prompts: Vec<(String, String)>,
    /// Every turn id this chat started itself (its own `turn/start`), kept
    /// across a new conversation: only these turns' approvals are routed.
    own_turns: std::collections::HashSet<String>,
    /// v2 envelope segments whose saved text arrived.
    persisted: Vec<String>,
    segment: String,
}

impl ChatModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn set_phase(&mut self, phase: Phase) {
        if self.phase != phase {
            self.phase = phase;
            self.changed();
        }
    }

    fn changed(&mut self) {
        self.generation += 1;
    }

    pub fn notice(&mut self, text: impl Into<String>) {
        self.items.push(Item::Notice(text.into()));
        self.changed();
    }

    /// The person sent `text` as turn `turn`.
    pub fn start_turn(&mut self, turn: &str, text: &str) {
        self.items.push(Item::Message { role: Role::User, text: text.to_string(), turn: Some(turn.to_string()) });
        self.prompts.push((turn.to_string(), text.to_string()));
        self.own_turns.insert(turn.to_string());
        self.phase = Phase::Running { turn: turn.to_string() };
        self.changed();
    }

    /// A turn this chat started without a visible prompt (`/new`).
    pub fn own_turn(&mut self, turn: &str) {
        self.own_turns.insert(turn.to_string());
    }

    /// Whether this chat started `turn` itself.
    pub fn is_own_turn(&self, turn: &str) -> bool {
        self.own_turns.contains(turn)
    }

    /// The prompt that started `turn`, if this model saw it.
    pub fn prompt_of(&self, turn: &str) -> Option<&str> {
        self.prompts.iter().rev().find(|(t, _)| t == turn).map(|(_, p)| p.as_str())
    }

    /// The turn ended (completed, failed, interrupted or lost): back to
    /// ready, and any tool still running is marked failed.
    pub fn end_turn(&mut self, turn: &str, error: Option<&str>) {
        for item in &mut self.items {
            if let Item::Tool { status, turn: Some(t), .. } = item {
                if t == turn && *status == ToolStatus::Running {
                    *status = ToolStatus::Failed;
                }
            }
            if let Item::Approval { state, .. } = item {
                if matches!(*state, ApprovalState::Waiting | ApprovalState::External) {
                    *state = ApprovalState::Cancelled;
                }
            }
        }
        if let Some(error) = error {
            self.items.push(Item::Notice(error.to_string()));
        }
        if self.phase.running_turn() == Some(turn) || matches!(self.phase, Phase::Running { .. }) {
            self.phase = Phase::Ready;
        }
        self.changed();
    }

    /// The assistant's text for `turn` grows by `delta`.
    fn append_assistant(&mut self, turn: &str, delta: &str) {
        if let Some(Item::Message { role: Role::Assistant, text, turn: Some(t) }) = self.items.last_mut() {
            if t == turn {
                text.push_str(delta);
                self.changed();
                return;
            }
        }
        self.items.push(Item::Message { role: Role::Assistant, text: delta.to_string(), turn: Some(turn.to_string()) });
        self.changed();
    }

    /// The saved text of the turn's last assistant message replaces what
    /// streamed.
    fn set_assistant(&mut self, turn: &str, full: &str) {
        if let Some(Item::Message { role: Role::Assistant, text, turn: Some(t) }) = self.items.last_mut() {
            if t == turn {
                *text = full.to_string();
                self.changed();
                return;
            }
        }
        self.append_assistant(turn, full);
    }

    fn tool_mut(&mut self, call_id: &str) -> Option<&mut Item> {
        self.items.iter_mut().rev().find(|i| matches!(i, Item::Tool { call_id: c, .. } if c == call_id))
    }

    /// One notification for the system session. Returns what the driver
    /// must do next.
    pub fn apply(&mut self, method: &str, params: &Value) -> Vec<Effect> {
        let s = |k: &str| params.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let turn = s("turn_id");
        let mut effects = Vec::new();
        match method {
            "turn/started" => {
                // Another client's turn on the same conversation (Talk to
                // Octos), or the kernel confirming ours.
                self.phase = Phase::Running { turn };
                self.changed();
            }
            "message/delta" => self.append_assistant(&turn, &s("text")),
            "tool/started" => {
                let detail = params.get("arguments").map(brief_args).unwrap_or_default();
                self.items.push(Item::Tool { call_id: s("tool_call_id"), name: s("tool_name"), status: ToolStatus::Running, detail, turn: Some(turn) });
                self.changed();
            }
            "tool/progress" => {
                let message = s("message");
                if let Some(Item::Tool { detail, .. }) = self.tool_mut(&s("tool_call_id")) {
                    if !message.is_empty() {
                        *detail = message;
                    }
                }
                self.changed();
            }
            "tool/completed" => {
                let ok = params.get("success").and_then(Value::as_bool).unwrap_or(true);
                let preview = s("output_preview");
                let call_id = s("tool_call_id");
                let name = s("tool_name");
                match self.tool_mut(&call_id) {
                    Some(Item::Tool { status, detail, .. }) => {
                        *status = if ok { ToolStatus::Done } else { ToolStatus::Failed };
                        if !ok && !preview.is_empty() {
                            *detail = preview;
                        }
                    }
                    _ => self.items.push(Item::Tool {
                        call_id,
                        name,
                        status: if ok { ToolStatus::Done } else { ToolStatus::Failed },
                        detail: String::new(),
                        turn: Some(turn),
                    }),
                }
                self.changed();
            }
            "user_question/requested" => {
                let questions = params.get("questions").and_then(Value::as_array).cloned().unwrap_or_default();
                let first = questions.first();
                let options = first
                    .and_then(|q| q.get("options"))
                    .and_then(Value::as_array)
                    .map(|o| o.iter().filter_map(|o| o.get("label").and_then(Value::as_str)).map(str::to_string).collect())
                    .unwrap_or_default();
                let body = first.and_then(|q| q.get("question")).and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| s("body"));
                self.items.push(Item::Question { id: s("question_id"), turn, title: s("title"), body, options, count: questions.len().max(1), answered: None });
                self.changed();
            }
            "approval/requested" => {
                let id = s("approval_id");
                let mut tool = s("tool_name");
                let title = s("title");
                let body = s("body");
                let mut args = match params.get("typed_details") {
                    Some(details) if !details.is_null() => details.clone(),
                    _ => serde_json::json!({ "title": title, "body": body }),
                };
                // A host-routed tool's approval names its owning app, the
                // declared tool and the exact arguments.
                let mut app = None;
                let mut outcome_unknown = false;
                if s("approval_kind") == "host_tool" {
                    let host = &params["typed_details"]["host_tool"];
                    if let (Some(owner), Some(name)) = (host["app"].as_str(), host["tool"].as_str()) {
                        app = Some(owner.to_string());
                        tool = name.to_string();
                        args = host["args"].clone();
                        outcome_unknown = host["outcome_unknown_before"] == true;
                    }
                }
                let external = !self.is_own_turn(&turn);
                let plan = match self.prompt_of(&turn) {
                    Some(p) if !external => p.to_string(),
                    _ if external => "An outside client's request".to_string(),
                    _ => "The system agent's request".to_string(),
                };
                let state = if external { ApprovalState::External } else { ApprovalState::Waiting };
                self.items.push(Item::Approval { id: id.clone(), tool: tool.clone(), title: title.clone(), state });
                self.changed();
                effects.push(Effect::Approval(ApprovalAsk { approval_id: id, turn, tool, title, body, args, plan, app, outcome_unknown, external }));
            }
            "approval/decided" => {
                let approved = params.get("decision").and_then(Value::as_str) == Some("approve");
                let id = s("approval_id");
                self.set_approval(&id, if approved { ApprovalState::Approved } else { ApprovalState::Denied });
                effects.push(Effect::ApprovalGone(id));
            }
            "approval/cancelled" => {
                let id = s("approval_id");
                self.set_approval(&id, ApprovalState::Cancelled);
                effects.push(Effect::ApprovalGone(id));
            }
            "turn/completed" => self.end_turn(&turn, None),
            "turn/error" => {
                let message = params.get("message").and_then(Value::as_str).filter(|m| !m.is_empty()).map(str::to_string).unwrap_or_else(|| s("code"));
                let message = if message.is_empty() { "The assistant's turn failed.".to_string() } else { message };
                self.end_turn(&turn, Some(&message));
            }
            "projection/envelope" => self.envelope(&turn, &params["payload"]),
            _ => {}
        }
        effects
    }

    /// A v2 projection envelope (a connection that negotiated
    /// `projection.envelope.v2`; the stdio default does not).
    fn envelope(&mut self, turn: &str, payload: &Value) {
        let data = &payload["data"];
        match payload["type"].as_str().unwrap_or("") {
            kind @ ("assistant_delta" | "assistant_persisted") => {
                let segment = data["assistant_segment_id"].as_str().unwrap_or("").to_string();
                if kind == "assistant_delta" && self.persisted.contains(&segment) {
                    return;
                }
                let text = data["text"].as_str().unwrap_or("");
                if self.segment != segment {
                    self.segment = segment.clone();
                    self.items.push(Item::Message { role: Role::Assistant, text: String::new(), turn: Some(turn.to_string()) });
                }
                if kind == "assistant_persisted" {
                    self.persisted.push(segment);
                    self.set_assistant(turn, text);
                } else {
                    self.append_assistant(turn, text);
                }
            }
            "turn_terminal" => {
                if data["outcome"] == "completed" {
                    self.end_turn(turn, None);
                } else {
                    let message = data["error"]["message"].as_str().unwrap_or("The assistant's turn did not complete.").to_string();
                    self.end_turn(turn, Some(&message));
                }
            }
            _ => {}
        }
    }

    fn set_approval(&mut self, id: &str, to: ApprovalState) {
        for item in &mut self.items {
            if let Item::Approval { id: i, state, .. } = item {
                if i == id && matches!(*state, ApprovalState::Waiting | ApprovalState::External) {
                    *state = to.clone();
                }
            }
        }
        self.changed();
    }

    /// An approval of another client's turn (shown read-only).
    pub fn is_external_approval(&self, id: &str) -> bool {
        self.items.iter().any(|i| matches!(i, Item::Approval { id: i, state: ApprovalState::External, .. } if i == id))
    }

    /// The router decided (the person, a rule or developer mode).
    pub fn approval_decided(&mut self, id: &str, approved: bool) {
        self.set_approval(id, if approved { ApprovalState::Approved } else { ApprovalState::Denied });
    }

    /// The person answered a question from the pane.
    pub fn question_answered(&mut self, id: &str, answer: &str) {
        for item in &mut self.items {
            if let Item::Question { id: i, answered, .. } = item {
                if i == id {
                    *answered = Some(answer.to_string());
                }
            }
        }
        self.changed();
    }

    /// The question waiting for the person, if any.
    pub fn open_question(&self) -> Option<(&str, usize)> {
        self.items.iter().rev().find_map(|i| match i {
            Item::Question { id, answered: None, count, .. } => Some((id.as_str(), *count)),
            _ => None,
        })
    }

    /// The transcript from `session/hydrate` (`result.messages`), replacing
    /// what the pane showed. Rows are `{role, content, turn_id?}`.
    pub fn load_history(&mut self, messages: &Value) {
        let mut items = Vec::new();
        for row in messages.as_array().into_iter().flatten() {
            let text = row.get("content").and_then(Value::as_str).unwrap_or("").to_string();
            let turn = row.get("turn_id").and_then(Value::as_str).map(str::to_string);
            match row.get("role").and_then(Value::as_str).unwrap_or("") {
                "user" => items.push(Item::Message { role: Role::User, text, turn }),
                "assistant" if !text.trim().is_empty() => items.push(Item::Message { role: Role::Assistant, text, turn }),
                "tool" => items.push(Item::Tool {
                    call_id: row.get("tool_call_id").and_then(Value::as_str).unwrap_or("").to_string(),
                    name: row.get("name").or_else(|| row.get("tool_name")).and_then(Value::as_str).unwrap_or("tool").to_string(),
                    status: ToolStatus::Done,
                    detail: String::new(),
                    turn,
                }),
                _ => {}
            }
        }
        // Keep what history cannot know: notices and a question or approval
        // still open.
        for item in std::mem::take(&mut self.items) {
            let keep = match &item {
                Item::Question { answered: None, .. } => true,
                Item::Approval { state: ApprovalState::Waiting | ApprovalState::External, .. } => true,
                _ => false,
            };
            if keep {
                items.push(item);
            }
        }
        self.items = items;
        self.changed();
    }

    /// New conversation: the transcript is cleared at once.
    pub fn clear(&mut self) {
        self.items.clear();
        self.prompts.clear();
        self.changed();
    }
}

/// One short line for a tool call's arguments ("path: notes.md").
pub fn brief_args(args: &Value) -> String {
    match args {
        Value::Object(map) => map
            .iter()
            .take(3)
            .map(|(k, v)| {
                let v = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let v: String = v.chars().take(60).collect();
                format!("{k}: {v}")
            })
            .collect::<Vec<_>>()
            .join(" \u{00b7} "),
        Value::Null => String::new(),
        other => other.to_string().chars().take(80).collect(),
    }
}
