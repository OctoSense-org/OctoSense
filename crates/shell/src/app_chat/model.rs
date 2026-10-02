//! An app's conversation as the "Ask <app>" panel draws it: both lanes of
//! the app agent's conversation (ADR 0004 §6), merged, each message with
//! who spoke. Built only from what the conversation handle delivers (its
//! follower's events and its history), on the system chat's model
//! ([`crate::system_chat::model::ChatModel`]), so both surfaces assemble
//! streamed text, tools and approvals the same way. No drawing, no I/O.
//!
//! - **Lanes.** Every event carries its `lane`: `person` (this panel's
//!   sharing context) or `system_agent` (the peer's own session, the
//!   system agent's `peer/input` turns). Both run in parallel: the person
//!   may send while the system agent's turn runs ([`Conversation::running_in`]).
//! - **Speakers.** A user message names who spoke (`speaker`, from the
//!   kernel's origin marker or the broker's record): the person ("You"),
//!   the system agent, or the app itself; the text shown is the text after
//!   the marker (`display_text`). The agent's answers are "<App>'s agent",
//!   and in the system agent's lane "<App>'s agent, to the system agent".
//! - **Approvals** are the shell's: the router draws the sheet (the broker
//!   hands every approval of the peer to it). Nothing here answers one.
//! - **Order.** Each lane has its own ledger (`cursor.seq` counts per
//!   session), so seqs of two lanes do not compare. Within a lane items
//!   keep ledger order; across lanes they go by one clock, the order
//!   events reached the panel (a stream event carries no time of its own;
//!   history rows come merged by `persisted_at`). A turn's request line
//!   stands where the turn STARTED: the kernel sends a turn's user message
//!   only when the turn ends (never for a stopped one), so the broker hands
//!   its words over with `turn/started` (`request`).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::Value;

use crate::system_chat::model::{history_turn, ChatModel, Item, Phase, Role, ToolNames};

/// The person's lane (this panel's context) and the system agent's.
pub const LANE_PERSON: &str = "person";
pub const LANE_SYSTEM_AGENT: &str = "system_agent";

/// One app's conversation, both lanes.
#[derive(Clone, Debug, Default)]
pub struct Conversation {
    pub chat: ChatModel,
    /// The app's display name ("News").
    pub app: String,
    /// The turns running now, in either lane.
    running: BTreeSet<String>,
    /// Each turn's lane, as its events said.
    lanes: HashMap<String, String>,
    /// Turns that ended (their late envelopes do not make them run again).
    finished: Vec<String>,
    /// Each lane's ledger positions (`cursor.seq`) and the place on the
    /// panel's clock each was given.
    places: HashMap<String, BTreeMap<u64, u64>>,
    /// Where each turn started on the panel's clock.
    starts: HashMap<String, u64>,
    /// The panel's clock: one tick per event, [`SPACING`] apart.
    clock: u64,
}

/// Room between two ticks of the clock, for envelopes that arrive after a
/// later one of their own lane.
const SPACING: u64 = 1 << 20;

impl Conversation {
    pub fn new(app: &str) -> Self {
        Conversation { app: app.to_string(), chat: ChatModel::new(), ..Conversation::default() }
    }

    /// Whether a turn runs in either lane.
    pub fn busy(&self) -> bool {
        !self.running.is_empty()
    }

    /// The turn running in `lane` ([`LANE_PERSON`] or
    /// [`LANE_SYSTEM_AGENT`]), if one does. The lanes are independent: the
    /// person may send while the system agent's turn runs.
    pub fn running_in(&self, lane: &str) -> Option<&str> {
        self.running.iter().find(|t| self.lanes.get(*t).map(String::as_str).unwrap_or(LANE_PERSON) == lane).map(String::as_str)
    }

    /// The label of a user message's speaker (`{"kind", "label"?}`).
    pub fn speaker_label(&self, speaker: &Value) -> String {
        match speaker["kind"].as_str().unwrap_or("") {
            "system_agent" => "System agent".to_string(),
            "app" => format!("{} (the app)", self.app),
            // The person, in this panel or in the app's own UI.
            _ => "You".to_string(),
        }
    }

    /// The label of the agent's answers in `lane`.
    pub fn agent_label(&self, lane: &str) -> String {
        if lane == LANE_SYSTEM_AGENT {
            format!("{}'s agent, to the system agent", self.app)
        } else {
            format!("{}'s agent", self.app)
        }
    }

    fn tick(&mut self) -> u64 {
        self.clock += SPACING;
        self.clock
    }

    /// The place of `lane`'s ledger position `seq` on the panel's clock:
    /// now, unless a later position of the same lane already came (then
    /// just before it, after the one before it).
    fn place(&mut self, lane: &str, seq: u64) -> u64 {
        if let Some(at) = self.places.get(lane).and_then(|p| p.get(&seq)) {
            return *at;
        }
        let now = self.tick();
        let placed = self.places.entry(lane.to_string()).or_default();
        let at = match placed.range(seq + 1..).next().map(|(_, at)| *at) {
            None => now,
            Some(next) => {
                let before = placed.range(..seq).next_back().map(|(_, at)| *at).unwrap_or(next.saturating_sub(SPACING));
                before + (next - before) / 2
            }
        };
        placed.insert(seq, at);
        at
    }

    /// Where `turn` started on the panel's clock (its first event, if its
    /// start was not seen).
    fn start_of(&mut self, turn: &str) -> u64 {
        if let Some(at) = self.starts.get(turn) {
            return *at;
        }
        let at = self.tick();
        self.starts.insert(turn.to_string(), at);
        at
    }

    /// The turn's request line (who asked, and what), where the turn
    /// started; once.
    fn request_line(&mut self, turn: &str, text: String, speaker: &Value) {
        let known = self.chat.items.iter().any(|i| matches!(i, Item::Message { role: Role::User, turn: Some(t), .. } if t == turn));
        if known || text.trim().is_empty() {
            return;
        }
        let at = self.start_of(turn);
        let speaker = self.speaker_label(speaker);
        let item = Item::Message { role: Role::User, text, turn: Some(turn.to_string()), segment: None, seq: Some(at), speaker: Some(speaker) };
        self.chat.insert_ordered(item, Some(at));
    }

    /// One event of the conversation's follower: `{method, params, lane,
    /// speaker?, display_text?, request?}`.
    pub fn apply(&mut self, data: &Value) {
        let method = data["method"].as_str().unwrap_or("");
        let lane = data["lane"].as_str().unwrap_or(LANE_PERSON).to_string();
        let mut params = data["params"].clone();
        let turn = params["turn_id"].as_str().unwrap_or("").to_string();
        if !turn.is_empty() {
            self.lanes.entry(turn.clone()).or_insert_with(|| lane.clone());
            self.start_of(&turn);
        }
        // One clock for both lanes: the model below orders by this place.
        if method == "projection/envelope" {
            if let Some(seq) = params["cursor"]["seq"].as_u64().or_else(|| params["seq"].as_u64()) {
                let at = self.place(&lane, seq);
                params["cursor"] = serde_json::json!({"seq": at});
            }
        }
        let kind = params["payload"]["type"].as_str().unwrap_or("").to_string();
        match (method, kind.as_str()) {
            ("projection/envelope", "user_message") => {
                // Who asked: shown where the turn started, not where the
                // kernel recorded it (at its end).
                let text = data["display_text"]
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| params["payload"]["data"]["text"].as_str().map(|t| strip_marker(t).to_string()))
                    .unwrap_or_default();
                self.request_line(&turn, text, &data["speaker"]);
                if !self.ended(&turn) {
                    self.running.insert(turn.clone());
                }
            }
            ("turn/started", _) => {
                if let Some(text) = data["request"]["text"].as_str() {
                    let speaker = if data["request"]["speaker"].is_object() { &data["request"]["speaker"] } else { &data["speaker"] };
                    self.request_line(&turn, text.to_string(), speaker);
                }
                if !self.ended(&turn) {
                    self.running.insert(turn.clone());
                }
            }
            _ => {}
        }
        let terminal = matches!(method, "turn/completed" | "turn/error") || kind == "turn_terminal";
        // The approvals the model notes are the router's; its effects are
        // never acted on here.
        let _ = self.chat.apply(method, &params);
        if terminal && !turn.is_empty() {
            self.running.remove(&turn);
            self.finished.push(turn.clone());
            if self.finished.len() > 256 {
                self.finished.remove(0);
            }
        }
        self.label_answers();
        self.settle_phase();
    }

    fn ended(&self, turn: &str) -> bool {
        self.finished.iter().any(|t| t == turn)
    }

    /// Name the agent's answers by their turn's lane.
    fn label_answers(&mut self) {
        let lanes = &self.lanes;
        let person = format!("{}'s agent", self.app);
        let system = format!("{}'s agent, to the system agent", self.app);
        for item in &mut self.chat.items {
            if let Item::Message { role: Role::Assistant, turn: Some(t), speaker: speaker @ None, .. } = item {
                *speaker = Some(if lanes.get(t).is_some_and(|l| l == LANE_SYSTEM_AGENT) { system.clone() } else { person.clone() });
            }
        }
    }

    /// Busy while either lane runs; otherwise ready (the model's own phase
    /// follows one turn at a time).
    fn settle_phase(&mut self) {
        let phase = match self.running.iter().next() {
            Some(turn) => Phase::Running { turn: turn.clone() },
            None => Phase::Ready,
        };
        self.chat.set_phase(phase);
    }

    /// The conversation's history (both transcripts merged by time; rows
    /// `{role, content, lane, speaker?, display_text?, thread_id|turn_id?}`)
    /// replaces what the panel showed. A tool row keeps the name the panel
    /// showed for it ([`ToolNames`]: history has none), and what history
    /// cannot know stays where it stood ([`keep_in_place`]).
    pub fn load_history(&mut self, rows: &Value) {
        let rows = rows.as_array().map(Vec::as_slice).unwrap_or(&[]);
        let mut tools = ToolNames::new(&self.chat.items, rows);
        let mut items = Vec::new();
        for row in rows {
            let lane = row["lane"].as_str().unwrap_or(LANE_PERSON);
            let turn = history_turn(row);
            if let Some(t) = &turn {
                self.lanes.entry(t.clone()).or_insert_with(|| lane.to_string());
            }
            let content = row["content"].as_str().unwrap_or("");
            match row["role"].as_str().unwrap_or("") {
                "user" => {
                    let text = row["display_text"].as_str().unwrap_or_else(|| strip_marker(content)).to_string();
                    let speaker = if row["speaker"].is_object() {
                        self.speaker_label(&row["speaker"])
                    } else if lane == LANE_SYSTEM_AGENT {
                        "System agent".to_string()
                    } else {
                        "You".to_string()
                    };
                    items.push(Item::Message { role: Role::User, text, turn, segment: None, seq: None, speaker: Some(speaker) });
                }
                "assistant" if !content.trim().is_empty() => {
                    items.push(Item::Message { role: Role::Assistant, text: content.to_string(), turn, segment: None, seq: None, speaker: Some(self.agent_label(lane)) });
                }
                "tool" => items.push(Item::Tool {
                    call_id: row["tool_call_id"].as_str().unwrap_or("").to_string(),
                    name: tools.name(row, turn.as_deref()),
                    status: crate::system_chat::model::ToolStatus::Done,
                    detail: String::new(),
                    turn,
                    seq: None,
                }),
                _ => {}
            }
        }
        // What history cannot know stays: notices, and anything still open.
        let running = &self.running;
        let shown = std::mem::take(&mut self.chat.items);
        self.chat.items = keep_in_place(items, shown, |item| {
            matches!(item, Item::Question { answered: None, .. } | Item::Notice(_)) || matches!(item, Item::Message { turn: Some(t), .. } if running.contains(t))
        });
        self.settle_phase();
    }

    pub fn notice(&mut self, text: impl Into<String>) {
        self.chat.notice(text);
    }
}

/// What a transcript row is, to find it again in a reloaded history: its
/// kind and its turn (history rows carry the turn, [`history_turn`]).
fn row_key(item: &Item) -> Option<(&'static str, &str)> {
    match item {
        Item::Message { role: Role::User, turn: Some(t), .. } => Some(("user", t)),
        Item::Message { role: Role::Assistant, turn: Some(t), .. } => Some(("assistant", t)),
        Item::Tool { turn: Some(t), .. } => Some(("tool", t)),
        _ => None,
    }
}

/// `history`, with the items of `shown` (the transcript it replaces) that
/// history cannot know (`kept`: notices, an open question, a running turn's
/// rows) put back where they stood: right after the row they followed in
/// `shown`, found again in `history` by its kind and turn (the same
/// occurrence of that pair, or the last one there is). An item kept from
/// before any row comes first; one whose rows history has none of comes
/// last. Appending them all after the history moved "Stopped." and the
/// other notices below every newer row on each reload.
fn keep_in_place(history: Vec<Item>, shown: Vec<Item>, kept: impl Fn(&Item) -> bool) -> Vec<Item> {
    let keys: Vec<Option<(&'static str, String)>> = history.iter().map(|i| row_key(i).map(|(k, t)| (k, t.to_string()))).collect();
    let find = |key: &(&'static str, String), nth: usize| -> Option<usize> {
        let at: Vec<usize> = keys.iter().enumerate().filter(|(_, k)| k.as_ref() == Some(key)).map(|(i, _)| i).collect();
        at.get(nth).or(at.last()).copied()
    };
    let mut after: Vec<Vec<Item>> = (0..history.len()).map(|_| Vec::new()).collect();
    let (mut first, mut last) = (Vec::new(), Vec::new());
    // The rows `shown` had so far: kind, turn, and which occurrence.
    let mut before: Vec<((&'static str, String), usize)> = Vec::new();
    let mut seen: HashMap<(&'static str, String), usize> = HashMap::new();
    for item in shown {
        if kept(&item) {
            match before.iter().rev().find_map(|(key, nth)| find(key, *nth)) {
                Some(row) => after[row].push(item),
                None if before.is_empty() => first.push(item),
                None => last.push(item),
            }
        } else if let Some((kind, turn)) = row_key(&item) {
            let key = (kind, turn.to_string());
            let nth = seen.entry(key.clone()).or_default();
            before.push((key, *nth));
            *nth += 1;
        }
    }
    let mut items = first;
    for (row, placed) in history.into_iter().zip(after) {
        items.push(row);
        items.extend(placed);
    }
    items.extend(last);
    items
}

/// The text after the kernel's origin marker, if it carries one.
fn strip_marker(text: &str) -> &str {
    crate::ai_host::app_peers::split_origin_marker(text).map(|(_, rest)| rest).unwrap_or(text)
}
