//! "Ask <app>": the person's lane of an app agent's conversation, drawn by
//! the shell (ADR 0004 §4, §6) for every app that has an agent
//! ([`crate::agents`]), whether or not the app draws a chat of its own.
//!
//! | Part | Where |
//! | --- | --- |
//! | both lanes merged, with speakers | [`model::Conversation`] |
//! | the pane (desktop column beside the system chat; phone: a full-screen sheet) | the system chat's pane ([`crate::system_chat::view`], `app_panel: true`) |
//! | the handle | `agents::conversation`: a sharing context on the app's one peer (`open_conversation`) |
//!
//! - **Open** (the bar's "Ask <app>", Shift+F8, Setup › Assistant): the
//!   app's agent must be allowed. Undecided, the first-use sheet shows and
//!   the panel waits for the person; off, it says so. Allowed: the panel
//!   opens a sharing context, follows both lanes ([`OctosContext::subscribe`])
//!   and loads their merged history.
//! - **Send**: a person turn in this context (`TurnTrigger::Person`), in
//!   parallel with the system agent's lane. With a question of the app's
//!   agent open, the text answers it instead.
//! - **Stop**: the person's own running turn (the person's lane). The
//!   system agent's turn goes on; the panel stops it only on its own,
//!   explicit control ("Stop the system agent's task", [`stop_system_agent`]).
//!   Send stays while only the system agent's lane runs: the lanes are
//!   independent.
//! - **Questions** of the app's conversation (G11: turns the person or the
//!   app started) are shown and answered here; the system agent's go to the
//!   system chat. **Approvals** are the shell's sheets, as everywhere.
//! - **Close** hides the panel. The sharing context stays open with its
//!   follower, so the conversation keeps up while hidden and a reopen shows
//!   the person's own rows again: a new context would start an empty
//!   person's lane (the broker gives every handle a new kernel context).
//!   A reopen loads both lanes' merged history again. The context closes
//!   when the panel opens for another app, when the agent is turned off, or
//!   when the app's peer goes (then a reopen starts a new one).

pub mod model;

#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex};

use makepad_widgets::*;

use crate::ai_host::app_peers::{ContextEvent, ContextOp, EventSink, OctosContext, TurnTrigger};
use crate::apps::AgentApp;
use crate::system_chat::model::{ChatModel, Item, Phase};
use model::Conversation;

/// The panel's client instance on the app's peer (the same on every open).
pub const INSTANCE: &str = "shell-ask";

/// The id prefix of the app's questions in the panel.
pub const ROUTED_PREFIX: &str = "routed:";

/// Where the panel stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Nothing open.
    Idle,
    /// The first-use sheet is up: the person decides.
    NeedsConsent,
    /// Turned off: only the person turns it on (Settings).
    Off,
    Connecting,
    Ready,
    Failed(String),
}

struct Shared {
    conversation: Conversation,
    context: Option<Arc<dyn OctosContext>>,
    status: Status,
    generation: u64,
    /// The context this state belongs to (a reopen starts a new one; late
    /// events of an old one are dropped).
    epoch: u64,
}

struct Panel {
    app: Option<AgentApp>,
    open: bool,
    /// The keyboard goes here (else to the system chat).
    focused: bool,
    draft: crate::system_chat::composer::Composer,
    scroll: f64,
    ui_generation: u64,
    shared: Arc<Mutex<Shared>>,
}

static PANEL: Mutex<Option<Panel>> = Mutex::new(None);
static EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn with<R>(f: impl FnOnce(&mut Panel) -> R) -> R {
    let mut guard = PANEL.lock().unwrap_or_else(|e| e.into_inner());
    let panel = guard.get_or_insert_with(|| Panel {
        app: None,
        open: false,
        focused: false,
        draft: Default::default(),
        scroll: 0.0,
        ui_generation: 0,
        shared: Arc::new(Mutex::new(Shared { conversation: Conversation::new(""), context: None, status: Status::Idle, generation: 0, epoch: 0 })),
    });
    f(panel)
}

fn shared() -> Arc<Mutex<Shared>> {
    with(|p| p.shared.clone())
}

fn lock(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

fn changed(s: &mut Shared) {
    s.generation += 1;
    SignalToUI::set_ui_signal();
}

pub fn is_open() -> bool {
    with(|p| p.open)
}

/// The app the panel is for.
pub fn app() -> Option<AgentApp> {
    with(|p| p.app.clone())
}

/// Whether the keyboard is the panel's.
pub fn is_focused() -> bool {
    with(|p| p.open && p.focused)
}

pub fn focus(on: bool) {
    with(|p| {
        if p.focused != on {
            p.focused = on;
            p.ui_generation += 1;
        }
    });
}

/// Open "Ask <app>" for the app with an agent `name` means (its id, its
/// launcher id or its name). False when it has none.
pub fn open(name: &str) -> bool {
    let Some(app) = crate::agents::find(name) else { return false };
    open_app(app);
    true
}

pub fn open_app(app: AgentApp) {
    let same = with(|p| p.app.as_ref().is_some_and(|a| a.id == app.id));
    let live = same && lock(&shared()).context.as_ref().is_some_and(|c| c.is_open());
    with(|p| {
        p.open = true;
        p.focused = true;
        p.ui_generation += 1;
    });
    if live {
        // Reopened: both lanes' history again (what ran while it was
        // hidden, and anything a stale view lost); the follower stays.
        reload_history();
        return;
    }
    close_context();
    let epoch = EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    {
        let shared = shared();
        let mut s = lock(&shared);
        s.conversation = Conversation::new(&app.name);
        s.epoch = epoch;
        s.status = Status::Idle;
        changed(&mut s);
    }
    with(|p| {
        p.app = Some(app.clone());
        p.draft.clear();
        p.scroll = 0.0;
    });
    begin(&app);
}

/// Ask for consent if needed, else connect.
fn begin(app: &AgentApp) {
    let status = match crate::agents::ask(app) {
        crate::agents::Access::Allowed => {
            connect(app);
            return;
        }
        crate::agents::Access::NotAsked => Status::NeedsConsent,
        crate::agents::Access::Off => Status::Off,
    };
    let shared = shared();
    let mut s = lock(&shared);
    s.status = status;
    changed(&mut s);
}

/// Open the sharing context (off the UI thread: the peer may be prepared
/// first), follow both lanes, and load the history.
fn connect(app: &AgentApp) {
    let shared = shared();
    let epoch = {
        let mut s = lock(&shared);
        s.status = Status::Connecting;
        changed(&mut s);
        s.epoch
    };
    let app = app.clone();
    let _ = std::thread::Builder::new().name("ask-app".into()).spawn(move || {
        // One instance for every open: the broker gives each handle a new
        // kernel context, and its history keeps the person's rows of the
        // earlier ones of the same instance.
        let instance = INSTANCE.to_string();
        let context = match crate::agents::conversation(&app, &instance) {
            Ok(context) => context,
            Err(e) => {
                let mut s = lock(&shared);
                if s.epoch == epoch {
                    s.status = Status::Failed(e);
                    changed(&mut s);
                }
                return;
            }
        };
        let follower = shared.clone();
        context.subscribe(Some(Arc::new(move |event| {
            if let ContextEvent::Data(data) = event {
                let mut s = lock(&follower);
                if s.epoch == epoch {
                    s.conversation.apply(&data);
                    changed(&mut s);
                }
            }
        })));
        {
            let mut s = lock(&shared);
            if s.epoch != epoch {
                context.close();
                return;
            }
            s.context = Some(context.clone());
        }
        load_history(&shared, &context, epoch);
    });
}

/// Load both lanes' merged history (`octos.session.history`) into the
/// conversation; the panel is ready once it came.
fn load_history(shared: &Arc<Mutex<Shared>>, context: &Arc<dyn OctosContext>, epoch: u64) {
    let history = shared.clone();
    let loaded: EventSink = Arc::new(move |event| {
        if let ContextEvent::Complete(result) = event {
            let mut s = lock(&history);
            if s.epoch != epoch {
                return;
            }
            match result {
                Ok(value) => s.conversation.load_history(&value["messages"]),
                Err(e) => s.conversation.notice(format!("Could not load the conversation: {e}")),
            }
            s.status = Status::Ready;
            changed(&mut s);
        }
    });
    if let Err(e) = context.call(ContextOp::History, loaded) {
        let mut s = lock(shared);
        s.status = Status::Failed(e);
        changed(&mut s);
    }
}

/// The panel reopened on its live context: its history again.
fn reload_history() {
    let shared = shared();
    let (context, epoch) = {
        let s = lock(&shared);
        (s.context.clone(), s.epoch)
    };
    if let Some(context) = context {
        load_history(&shared, &context, epoch);
    }
}

fn close_context() {
    let context = lock(&shared()).context.take();
    if let Some(context) = context {
        context.subscribe(None);
        context.close();
    }
}

/// Hide the panel. Its context stays open (see the module): a reopen shows
/// the same conversation, the person's rows included.
pub fn close() {
    with(|p| {
        p.open = false;
        p.focused = false;
        p.ui_generation += 1;
    });
    let shared = shared();
    let mut s = lock(&shared);
    if !s.context.as_ref().is_some_and(|c| c.is_open()) {
        s.context = None;
        s.status = Status::Idle;
    }
    changed(&mut s);
}

/// On the UI thread, with the approvals tick and every signal: the person
/// decided on the first-use sheet; the agent was turned off.
pub fn pump() {
    let Some(app) = with(|p| p.open.then(|| p.app.clone()).flatten()) else { return };
    let status = lock(&shared()).status.clone();
    let access = crate::agents::access(&app.id);
    match (status, access) {
        (Status::NeedsConsent | Status::Off, crate::agents::Access::Allowed) => connect(&app),
        (Status::NeedsConsent, crate::agents::Access::Off) => {
            let shared = shared();
            let mut s = lock(&shared);
            s.status = Status::Off;
            changed(&mut s);
        }
        (Status::Ready | Status::Connecting, crate::agents::Access::Off | crate::agents::Access::NotAsked) => {
            close_context();
            let shared = shared();
            let mut s = lock(&shared);
            s.status = Status::Off;
            s.conversation.notice(format!("{}'s assistant was turned off.", app.name));
            changed(&mut s);
        }
        _ => {}
    }
}

pub fn send_draft() {
    let text = with(|p| {
        p.ui_generation += 1;
        p.scroll = 0.0;
        p.draft.take()
    });
    send(&text);
}

/// The person's message: a turn in the person's lane (or the answer to the
/// app agent's open question).
pub fn send(text: &str) {
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }
    if let Some((question, _)) = snapshot().open_question().map(|(id, n)| (id.to_string(), n)) {
        answer(&question, &text, false);
        return;
    }
    let shared = shared();
    let (context, epoch) = {
        let s = lock(&shared);
        (s.context.clone(), s.epoch)
    };
    let Some(context) = context else {
        let mut s = lock(&shared);
        s.conversation.notice("Not connected yet.");
        changed(&mut s);
        return;
    };
    let failed = shared.clone();
    // Once the turn started, its own end says how it ended (the follower's
    // `turn_terminal`: "turn interrupted by client" after a Stop); the
    // send's error is the same news. Only an error before any event of the
    // turn (it never started) is the send's to tell.
    let started = std::sync::atomic::AtomicBool::new(false);
    let sink: EventSink = Arc::new(move |event| match event {
        ContextEvent::Data(_) => started.store(true, std::sync::atomic::Ordering::Relaxed),
        ContextEvent::Complete(Err(e)) if !started.load(std::sync::atomic::Ordering::Relaxed) => {
            let mut s = lock(&failed);
            if s.epoch == epoch {
                s.conversation.notice(e);
                changed(&mut s);
            }
        }
        ContextEvent::Complete(_) => {}
    });
    if let Err(e) = context.call(ContextOp::TurnFrom { text, trigger: TurnTrigger::Person }, sink) {
        let mut s = lock(&shared);
        s.conversation.notice(e);
        changed(&mut s);
    }
}

/// Stop: the person's own running turn, only. The system agent's lane
/// goes on (its own control: [`stop_system_agent`]). The turn's open
/// questions and held approvals end with it (the broker closes them).
pub fn stop() {
    let Some(app) = app() else { return };
    let stopped = crate::host_tools::interrupt_agent_lane(&app.id, model::LANE_PERSON);
    let shared = shared();
    let mut s = lock(&shared);
    s.conversation.notice(if stopped.is_empty() { "Nothing of yours was running.".to_string() } else { "Stopped.".to_string() });
    changed(&mut s);
}

/// "Stop the system agent's task": the system agent's running turn in this
/// app's conversation, only, and only on the person's explicit gesture on
/// that turn's own row (never the Send button's place).
pub fn stop_system_agent() {
    let Some(app) = app() else { return };
    let stopped = crate::host_tools::interrupt_agent_lane(&app.id, model::LANE_SYSTEM_AGENT);
    let shared = shared();
    let mut s = lock(&shared);
    s.conversation.notice(if stopped.is_empty() { "The system agent had nothing running here.".to_string() } else { "Stopped the system agent's task.".to_string() });
    changed(&mut s);
}

/// The person's lane runs a turn (Stop shows instead of Send).
pub fn person_running() -> bool {
    let shared = shared();
    let s = lock(&shared);
    s.status == Status::Ready && s.conversation.running_in(model::LANE_PERSON).is_some()
}

/// The system agent's lane runs a turn (its row offers to stop it).
pub fn system_agent_running() -> bool {
    let shared = shared();
    let s = lock(&shared);
    s.status == Status::Ready && s.conversation.running_in(model::LANE_SYSTEM_AGENT).is_some()
}

pub fn answer_option(question: &str, _count: usize, label: &str) {
    answer(question, label, true);
}

fn answer(question: &str, text: &str, option: bool) {
    let Some(id) = question.strip_prefix(ROUTED_PREFIX).and_then(|n| n.parse::<u64>().ok()) else { return };
    use crate::ai_host::app_peers::host_tools::QuestionReply;
    let reply = if option { QuestionReply::option(text) } else { QuestionReply::text(text) };
    if let Err(e) = crate::questions::answer(id, &[reply], &crate::questions::PersonAnswer::from_shell_surface()) {
        log!("ask: question {id}: {e}");
    }
}

/// The model as the pane draws it: the conversation, then the app agent's
/// open questions (G11: the app's conversation).
pub fn snapshot() -> ChatModel {
    let (app, shared) = with(|p| (p.app.clone(), p.shared.clone()));
    let s = lock(&shared);
    let mut model = s.conversation.chat.clone();
    let phase = match &s.status {
        Status::Ready => model.phase().clone(),
        Status::Failed(why) => Phase::NoKernel(why.clone()),
        Status::Connecting | Status::Idle => Phase::Connecting,
        Status::NeedsConsent | Status::Off => Phase::Idle,
    };
    model.phase = phase;
    drop(s);
    if let Some(app) = app {
        for request in crate::questions::open(&crate::questions::Conversation::App(app.id.clone())) {
            model.items.push(Item::Question {
                id: format!("{ROUTED_PREFIX}{}", request.id),
                turn: request.turn_id.clone(),
                title: if request.title.is_empty() { request.asked_by() } else { format!("{}: {}", request.asked_by(), request.title) },
                body: request.text().to_string(),
                options: request.options(),
                count: request.answer_count(),
                answered: None,
            });
        }
    }
    model
}

/// Tests that drive the kernel themselves (the two-lane scenario): the
/// panel is open for `app` without a conversation of its own, so it shows
/// the app's questions and answers them as the person would here.
#[cfg(test)]
pub(crate) fn show_for_tests(app: AgentApp) {
    with(|p| {
        p.app = Some(app);
        p.open = true;
        p.focused = true;
        p.ui_generation += 1;
    });
}

pub fn status() -> Status {
    lock(&shared()).status.clone()
}

/// The header's status line.
pub fn status_text() -> String {
    let name = app().map(|a| a.name).unwrap_or_default();
    match status() {
        Status::Idle | Status::Connecting => format!("Connecting to {name}'s agent\u{2026}"),
        Status::NeedsConsent => format!("{name}'s assistant is not allowed yet: allow it on the sheet."),
        Status::Off => format!("{name}'s assistant is off. Turn it on in Setup \u{203a} Assistant \u{203a} Approvals."),
        Status::Failed(why) => format!("{name}'s assistant is not available: {why}"),
        Status::Ready => {
            if snapshot().phase().running_turn().is_some() {
                "Working\u{2026} (you and the system agent share this conversation)".to_string()
            } else {
                format!("{name}'s agent \u{00b7} ready")
            }
        }
    }
}

pub fn draft() -> String {
    with(|p| p.draft.text().to_string())
}

/// The prompt's editor state, for the input method.
pub fn draft_state() -> makepad_widgets::makepad_platform::event::FullTextState {
    with(|p| p.draft.state())
}

pub fn scroll() -> f64 {
    with(|p| p.scroll)
}

pub fn scroll_by(dy: f64, max: f64) {
    with(|p| {
        p.scroll = (p.scroll + dy).clamp(0.0, max.max(0.0));
        p.ui_generation += 1;
    });
}

/// One number for "redraw".
pub fn generation() -> u64 {
    let shared = shared();
    let g = lock(&shared).generation;
    with(|p| p.ui_generation) + g + crate::questions::generation()
}

/// The keyboard, while the panel is open and focused. True when it was the
/// panel's. Characters are not typed here: they arrive as text input
/// ([`text_input`]), as they do for makepad's `TextInput`.
pub fn key(e: &KeyEvent) -> bool {
    if !is_focused() {
        return false;
    }
    use crate::system_chat::composer::Key;
    match crate::system_chat::composer::key(e) {
        Key::Close => close(),
        Key::Send => send_draft(),
        Key::Backspace => with(|p| {
            if p.draft.backspace() {
                p.ui_generation += 1;
            }
        }),
        Key::Stop => stop(),
        Key::Swallow => {}
        // Function keys (F8: the system chat) go on to the shell.
        Key::New | Key::Pass => return false,
    }
    true
}

/// Text input while focused: typed characters, a paste, the phone's input
/// method (see [`crate::system_chat::composer`]).
pub fn text_input(event: &makepad_widgets::makepad_platform::event::TextInputEvent) -> bool {
    if !is_focused() {
        return false;
    }
    let submit = with(|p| {
        if p.draft.text_input(event) {
            p.ui_generation += 1;
        }
        p.draft.take_submit()
    });
    if submit {
        send_draft();
    }
    true
}

/// `--test-action ask:<app>` (open the panel) and `ask-send:<text>` (send
/// in the open panel), for hidden-window runs.
pub fn test_action(name: &str) -> bool {
    if let Some(app) = name.strip_prefix("ask:") {
        return open(app);
    }
    if let Some(text) = name.strip_prefix("ask-send:") {
        send(text);
        return true;
    }
    false
}
