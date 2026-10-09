//! The system chat: the person's conversation with the system agent
//! (`_main:api:octosense#system`), inside the shell (ADR 0004 §6, §8,
//! §12; docs/architecture.md, "Agents").
//!
//! | Part | Module |
//! | --- | --- |
//! | the conversation as frames make it: streamed text, tool calls, questions, approvals, history | [`model`] |
//! | the session driver: open, hydrate, turns, interrupt, new conversation, reconnect and resume | [`session`] |
//! | its link: the shell's one kernel through `octosense_ai_host::kernel` | `link` (with a kernel) |
//! | Setup → Assistant → Command execution: the grant, the person's gesture, restart to apply | [`grants`] |
//! | its skills: one per linked craft engine, installed into the kernel's skills dir (ADR 0013) | [`skills`] |
//! | the pane (desktop side panel, phone full screen) | [`view`] |
//! | its prompt: text input, the input method, the pane's keys | [`composer`] |
//! | Markdown in the replies, as the pane draws it | [`markdown`] |
//!
//! **Where it runs.** A thread owns the [`session::Driver`] and its link;
//! the UI thread sends it [`session::Command`]s and draws a snapshot of the
//! model. The chat connects when the pane opens and lets the connection go
//! when it closes with nothing running, so the kernel's idle stop still
//! works.
//!
//! **Approvals.** Every `approval/requested` of a turn this chat started
//! goes to the shell's approval router ([`crate::approvals`]) as the system
//! agent's call, batched per request (the turn; its prompt is the plan),
//! with the id `syschat:<approval id>`; the router's decisions come back
//! through [`crate::approvals::take_system_chat_decisions`] and only then
//! does the chat answer the kernel. The pane never approves anything.
//!
//! The kernel also delivers approvals of OTHER clients' turns on the same
//! session (Talk to Octos): those are external. They go to the router as
//! [`crate::approvals::Caller::External`] on an external connection, which
//! holds and answers nothing (no developer mode, no rule, no sheet); the
//! pane shows them read-only and the chat never answers them.
//!
//! **App agents' questions.** A question an app's agent asks on a turn the
//! system agent started (`peer/input`, ADR 0004 §6) is the system chat's:
//! the chat subscribes to [`crate::questions`] and shows each one in its
//! conversation (ids `routed:<n>`); the person's answer goes back through
//! [`crate::questions::answer`], never through this chat's own session.

pub mod composer;
pub mod grants;
pub mod markdown;
pub mod model;
pub mod session;
pub mod skills;
pub mod view;
pub mod workspace;

#[cfg(kernel)]
pub(crate) mod link;
#[cfg(kernel)]
pub use link::provider_configured;

#[cfg(test)]
mod tests;

use makepad_widgets::*;
use model::{ChatModel, Effect};
use session::{Command, Connector, Driver};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The approval router's ids for this chat's approvals.
pub const HELD_PREFIX: &str = "syschat:";
/// The router's ids for another client's approvals on this session (never
/// held, never answered by the shell).
pub const EXTERNAL_PREFIX: &str = "syschat-external:";
/// The owning app of the system agent's own octos tool approvals, as the
/// sheet names it.
pub const APP: &str = "assistant";

struct Shared {
    model: ChatModel,
    effects: Vec<Effect>,
}

struct Worker {
    tx: mpsc::Sender<Command>,
    thread: std::thread::Thread,
}

struct Chat {
    open: bool,
    draft: composer::Composer,
    /// Bumped on UI-only changes (open, the draft).
    ui_generation: u64,
    shared: Arc<Mutex<Shared>>,
    worker: Option<Worker>,
    /// Scroll back from the newest line, in pixels.
    pub scroll: f64,
    /// App agents' questions routed here ([`crate::questions`]), by id.
    routed: Routed,
    /// What the shell told the system agent about the apps' agents last
    /// ([`crate::agents::system_note`]): said again only when it changed.
    told: Option<String>,
    /// The shell's own lines in the conversation (an agents.ask sheet).
    notes: Vec<model::Item>,
    /// `agents.ask` calls held until the person answered the first-use
    /// sheet and the agent's peer started ([`settle_asks`]).
    asks: Vec<HeldAsk>,
}

/// An `agents.ask` call the system chat holds: the app, the call's reply,
/// and when it came.
struct HeldAsk {
    app: crate::apps::AgentApp,
    reply: crate::ai_host::app_peers::host_tools::ToolReply,
    since: std::time::Instant,
}

/// How long a held `agents.ask` waits for the person before it answers
/// that they have not (the system agent tells them; the sheet stays up).
/// The kernel holds it longer (`confirm: app`, agents.rs `declarations`).
const ASK_WAIT: Duration = Duration::from_secs(600);

/// The id prefix of a routed question in the conversation.
pub const ROUTED_PREFIX: &str = "routed:";
/// How many routed questions the chat keeps: the oldest settled ones go
/// first, and an open one is never dropped (the person must still see it).
pub const ROUTED_KEPT: usize = 32;

/// Keep at most `max` routed questions, dropping the oldest settled ones;
/// open questions stay whatever their number.
pub(crate) fn trim_routed(routed: &mut Vec<(u64, model::Item)>, max: usize) {
    while routed.len() > max {
        let Some(i) = routed.iter().position(|(_, item)| !open_question(item)) else { break };
        routed.remove(i);
    }
}

/// How many OPEN routed questions the chat shows at once; past it a new one
/// waits (said visibly) and comes in when one settles.
pub const ROUTED_OPEN_MAX: usize = 16;

fn open_question(item: &model::Item) -> bool {
    matches!(item, model::Item::Question { answered: None, .. })
}

/// The app agents' questions routed to the system chat: bounded, and never
/// dropping an open one silently.
#[derive(Debug, Default)]
pub(crate) struct Routed {
    /// In the conversation, oldest first.
    pub(crate) shown: Vec<(u64, model::Item)>,
    /// Open questions past [`ROUTED_OPEN_MAX`], oldest first.
    pub(crate) waiting: Vec<(u64, model::Item)>,
}

impl Routed {
    /// A routed question asked or changed.
    pub(crate) fn place(&mut self, id: u64, item: model::Item) {
        if let Some(slot) = self.shown.iter_mut().find(|(n, _)| *n == id) {
            slot.1 = item;
        } else if let Some(i) = self.waiting.iter().position(|(n, _)| *n == id) {
            if open_question(&item) {
                self.waiting[i].1 = item;
            } else {
                // Settled while it waited (expired, closed): shown as such.
                self.waiting.remove(i);
                self.shown.push((id, item));
            }
        } else if open_question(&item) && self.open_count() >= ROUTED_OPEN_MAX {
            self.waiting.push((id, item));
        } else {
            self.shown.push((id, item));
        }
        while self.open_count() < ROUTED_OPEN_MAX && !self.waiting.is_empty() {
            let next = self.waiting.remove(0);
            self.shown.push(next);
        }
        trim_routed(&mut self.shown, ROUTED_KEPT);
    }
    fn open_count(&self) -> usize {
        self.shown.iter().filter(|(_, i)| open_question(i)).count()
    }
    /// What the conversation shows: the questions, and a line saying how
    /// many more wait.
    pub(crate) fn items(&self) -> Vec<model::Item> {
        let mut out: Vec<model::Item> = self.shown.iter().map(|(_, i)| i.clone()).collect();
        if !self.waiting.is_empty() {
            let n = self.waiting.len();
            out.push(model::Item::Notice(format!(
                "{n} more app question{} waiting: answer one above to see the next (unanswered ones expire).",
                if n == 1 { "" } else { "s" }
            )));
        }
        out
    }
}

/// The system chat as a consumer of [`crate::questions`]: the questions of
/// `peer/input` turns (the system agent's requests to app agents).
struct RoutedQuestions;

impl crate::questions::Consumer for RoutedQuestions {
    fn changed(&mut self, request: &crate::questions::Request) {
        if request.conversation != crate::questions::Conversation::SystemChat {
            return;
        }
        let answered = match &request.state {
            crate::questions::State::Open => None,
            crate::questions::State::Answered(text) => Some(text.clone()),
            crate::questions::State::Closed => Some("(no longer asked)".to_string()),
            crate::questions::State::Expired(reason) => Some(format!("Expired: {reason}")),
        };
        let item = model::Item::Question {
            id: format!("{ROUTED_PREFIX}{}", request.id),
            turn: request.turn_id.clone(),
            title: if request.title.is_empty() { request.asked_by() } else { format!("{}: {}", request.asked_by(), request.title) },
            body: request.text().to_string(),
            options: request.options(),
            count: request.answer_count(),
            answered,
        };
        with(|c| {
            c.routed.place(request.id, item);
            c.ui_generation += 1;
        });
    }
}

static CHAT: Mutex<Option<Chat>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Chat) -> R) -> R {
    let mut guard = CHAT.lock().unwrap_or_else(|e| e.into_inner());
    let chat = guard.get_or_insert_with(|| Chat {
        open: false,
        draft: composer::Composer::default(),
        ui_generation: 0,
        shared: Arc::new(Mutex::new(Shared { model: ChatModel::new(), effects: Vec::new() })),
        worker: None,
        scroll: 0.0,
        routed: Routed::default(),
        told: None,
        notes: Vec::new(),
        asks: Vec::new(),
    });
    f(chat)
}

fn connector() -> Box<dyn Connector> {
    #[cfg(kernel)]
    return Box::new(link::KernelConnector);
    #[cfg(not(kernel))]
    Box::new(NoKernel)
}

#[cfg(not(kernel))]
struct NoKernel;

#[cfg(not(kernel))]
impl Connector for NoKernel {
    fn connect(&mut self) -> Result<Box<dyn session::Link>, session::Unavailable> {
        Err(session::Unavailable::NoKernel("this build has no assistant (feature `octos-core`)".into()))
    }
}

/// The chat's thread: steps the driver, publishes the model when it
/// changed, and wakes the UI.
fn spawn(shared: Arc<Mutex<Shared>>) -> Worker {
    let (tx, rx) = mpsc::channel::<Command>();
    let handle = std::thread::Builder::new()
        .name("system-chat".into())
        .spawn(move || {
            let mut driver = Driver::new(connector());
            driver.set_waker(std::thread::current());
            let mut seen = u64::MAX;
            loop {
                loop {
                    match rx.try_recv() {
                        Ok(cmd) => driver.command(cmd),
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                }
                driver.step(Duration::from_millis(if driver.is_connected() { 100 } else { 250 }));
                if driver.model.generation != seen || !driver.effects.is_empty() {
                    seen = driver.model.generation;
                    let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                    s.model = driver.model.clone();
                    s.effects.append(&mut driver.effects);
                    drop(s);
                    SignalToUI::set_ui_signal();
                }
            }
        })
        .expect("system chat thread");
    Worker { tx, thread: handle.thread().clone() }
}

fn command(cmd: Command) {
    with(|c| {
        if c.worker.is_none() {
            c.worker = Some(spawn(c.shared.clone()));
        }
        let w = c.worker.as_ref().unwrap();
        let _ = w.tx.send(cmd);
        w.thread.unpark();
    });
}

// ------------------------------------------------------------ the pane

/// At startup: the command-execution grant of this home, the system
/// agent's skills, and the app agents' questions this chat shows.
pub fn init(home: &std::path::Path) {
    grants::init(home);
    skills::init();
    subscribe_questions();
}

/// Show the system agent's routed questions here (once).
pub fn subscribe_questions() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| crate::questions::subscribe(Box::new(RoutedQuestions)));
}

pub fn is_open() -> bool {
    with(|c| c.open)
}

pub fn open() {
    with(|c| {
        c.open = true;
        c.ui_generation += 1;
    });
    command(Command::Open);
}

pub fn close() {
    with(|c| {
        c.open = false;
        c.ui_generation += 1;
    });
    command(Command::Close);
}

pub fn toggle() {
    if is_open() {
        close()
    } else {
        open()
    }
}

/// Send the draft (or answer the open question with it).
pub fn send_draft() {
    let text = with(|c| {
        c.ui_generation += 1;
        c.scroll = 0.0;
        c.draft.take()
    });
    send(&text);
}

/// Send `text` as the person's next message; with a question open, it
/// answers the question instead.
pub fn send(text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let question = snapshot().open_question().map(|(id, n)| (id.to_string(), n));
    match question {
        Some((question, count)) => answer(&question, count, text, false),
        None => {
            // The apps' agents, when they changed since the system agent
            // was last told (it cannot see an agent that is not allowed).
            let note = crate::agents::system_note();
            let fresh = with(|c| {
                let fresh = note.is_some() && c.told != note;
                if fresh {
                    c.told = note.clone();
                }
                fresh
            });
            let note = if fresh { note.unwrap_or_default() } else { String::new() };
            // Allowed agents get their peer before the system agent looks.
            crate::agents::prepare_allowed();
            command(Command::SendNoted { text: text.to_string(), note });
        }
    }
}

pub fn answer_option(question: &str, count: usize, label: &str) {
    answer(question, count, label, true);
}

/// The person answered a question from the pane: the chat's own on its
/// session; an app agent's routed one through [`crate::questions`].
fn answer(question: &str, count: usize, text: &str, option: bool) {
    if let Some(id) = question.strip_prefix(ROUTED_PREFIX).and_then(|n| n.parse::<u64>().ok()) {
        use crate::ai_host::app_peers::host_tools::QuestionReply;
        let reply = if option { QuestionReply::option(text) } else { QuestionReply::text(text) };
        if let Err(e) = crate::questions::answer(id, &[reply], &crate::questions::PersonAnswer::from_shell_surface()) {
            log!("system chat: question {id}: {e}");
        }
        return;
    }
    command(Command::Answer { question: question.to_string(), count, text: text.to_string(), option });
}

pub fn interrupt() {
    command(Command::Interrupt);
}

/// The system agent's granted host tools changed (Setup's switch): the
/// connected chat registers the new set on its session at once (the set
/// lives with the connection; a chat that is not connected registers when
/// it connects).
pub fn sync_host_tools() {
    let running = with(|c| c.worker.is_some());
    if running {
        command(Command::SyncTools);
    }
}

pub fn new_conversation() {
    with(|c| {
        c.told = None;
        c.notes.clear();
    });
    command(Command::NewConversation);
}

/// The model as the pane draws it: the conversation, then the app agents'
/// questions routed here.
pub fn snapshot() -> ChatModel {
    with(|c| {
        let mut model = c.shared.lock().unwrap_or_else(|e| e.into_inner()).model.clone();
        model.items.extend(c.routed.items());
        model.items.extend(c.notes.iter().cloned());
        model
    })
}

pub fn draft() -> String {
    with(|c| c.draft.text().to_string())
}

/// The prompt's editor state, for the input method.
pub fn draft_state() -> makepad_widgets::makepad_platform::event::FullTextState {
    with(|c| c.draft.state())
}

/// The prompt as it stands (its text, caret and selection), for drawing.
pub fn composer() -> composer::Composer {
    with(|c| c.draft.clone())
}

/// Edit the prompt (a click placing the caret, a drag selecting).
pub fn edit_draft(f: impl FnOnce(&mut composer::Composer)) {
    with(|c| {
        f(&mut c.draft);
        c.ui_generation += 1;
    });
}

/// Copy or cut the prompt's selection (`None` without one).
pub fn copy_draft(cut: bool) -> Option<String> {
    with(|c| {
        let text = c.draft.copy(cut);
        if cut && text.is_some() {
            c.ui_generation += 1;
        }
        text
    })
}

pub fn scroll() -> f64 {
    with(|c| c.scroll)
}

pub fn scroll_by(dy: f64, max: f64) {
    with(|c| {
        c.scroll = (c.scroll + dy).clamp(0.0, max.max(0.0));
        c.ui_generation += 1;
    });
}

/// One number for "redraw".
pub fn generation() -> u64 {
    with(|c| c.ui_generation + c.shared.lock().unwrap_or_else(|e| e.into_inner()).model.generation)
}

/// On the UI thread, after a signal or the approvals tick: hand the new
/// approvals to the router, and the router's decisions to the kernel.
pub fn pump() {
    let effects = with(|c| std::mem::take(&mut c.shared.lock().unwrap_or_else(|e| e.into_inner()).effects));
    for effect in effects {
        match effect {
            Effect::Approval(ask) => {
                let route = route_approval(&ask);
                match route {
                    crate::approvals::Route::Refused(why) => log!("system chat: approval {} refused: {why}", ask.approval_id),
                    crate::approvals::Route::LeftToClient(_) => log!("system chat: approval {} belongs to another client's turn; left to it", ask.approval_id),
                    _ => {}
                }
            }
            Effect::ApprovalGone(_) => {}
            // `agents.ask`: the first-use sheet is the person's to answer.
            // The call waits for the answer and the agent's peer, so the
            // system agent goes on with the request in the same turn.
            Effect::ToolCall { call, reply } if call.name == crate::agents::ASK_TOOL => match crate::agents::ask_app(&call.args) {
                Err(outcome) => {
                    reply.finish(outcome);
                }
                Ok(app) => {
                    if crate::agents::begin_ask(&app) {
                        let name = app.name.clone();
                        with(|c| {
                            c.notes.push(model::Item::Notice(format!("The system agent asks to use {name}'s assistant: allow or deny it on the sheet.")));
                            c.ui_generation += 1;
                        });
                    }
                    match crate::agents::ask_settled(&app) {
                        Some(outcome) => {
                            reply.finish(outcome);
                        }
                        None => {
                            reply.acknowledge();
                            with(|c| c.asks.push(HeldAsk { app, reply, since: std::time::Instant::now() }));
                        }
                    }
                }
            },
            // `agents.list`: the shell's own answer.
            Effect::ToolCall { call, reply } if crate::agents::is_agents_tool(&call.name) => {
                reply.finish(crate::agents::call(&call.name, &call.args));
            }
            Effect::ToolCall { call, reply } => crate::host_tools::system_call(call, reply),
            Effect::ToolCancel(call_id) => crate::host_tools::system_cancel(&call_id),
        }
    }
    settle_asks();
    for (id, decision, _reason) in crate::approvals::take_system_chat_decisions() {
        if let Some(approval_id) = id.0.strip_prefix(HELD_PREFIX) {
            command(Command::Approval { approval_id: approval_id.to_string(), approve: decision.approved() });
        }
    }
}

/// Answer the held `agents.ask` calls that can be: the person answered and
/// the peer started (or could not), or the wait ran out. A cancelled call
/// (the turn was stopped) is dropped.
fn settle_asks() {
    let asks = with(|c| std::mem::take(&mut c.asks));
    let mut held = Vec::new();
    for ask in asks {
        if !ask.reply.is_open() {
            continue;
        }
        if let Some(outcome) = crate::agents::ask_settled(&ask.app) {
            log!("system chat: agents.ask for {} answered: {}", ask.app.id, crate::agents::access(&ask.app.id).as_str());
            ask.reply.finish(outcome);
        } else if ask.since.elapsed() >= ASK_WAIT {
            log!("system chat: agents.ask for {} still waits for the person; answered so", ask.app.id);
            ask.reply.finish(crate::agents::ask_pending(&ask.app));
        } else {
            held.push(ask);
        }
    }
    if !held.is_empty() {
        with(|c| c.asks.extend(held));
    }
}

/// One of the conversation's approvals, to the router: the system agent's
/// call, batched per request (its turn; the prompt is the plan).
pub fn route_approval(ask: &model::ApprovalAsk) -> crate::approvals::Route {
    let (app, tool, args, caller, context) = approval_request(ask);
    crate::approvals::approval_requested(&app, tool, args, caller, context)
}

/// What [`route_approval`] hands the router for `ask`: the owning app, the
/// tool, the exact arguments, the caller and the context.
///
/// - A turn this chat started: the system agent's call, triggered by the
///   person, batched per turn, on the host connection.
/// - Any other turn on the session (an external client's): an
///   [`Caller::External`](crate::approvals::Caller::External) call on an
///   external connection, trigger unknown, never batched; the router
///   answers nothing for it.
pub fn approval_request(ask: &model::ApprovalAsk) -> (String, crate::approvals::ToolSpec, serde_json::Value, crate::approvals::Caller, crate::approvals::RequestContext) {
    use crate::approvals::{Batch, Caller, Connection, RequestContext, ToolSpec, Trigger};
    // Command execution (`terminal.run`, the host tool Setup's switch grants
    // the system agent) is a command: `auto_approvable: false`, no standing
    // rule answers it, developer mode may (ADR 0004 §12, §13).
    let tool = if ask.tool == grants::COMMAND_TOOL { ToolSpec::host(&ask.tool).command() } else { ToolSpec::host(&ask.tool) };
    let app = if ask.tool == grants::COMMAND_TOOL { grants::COMMAND_APP } else { ask.app.as_deref().unwrap_or(APP) }.to_string();
    if ask.external {
        let context = RequestContext {
            call_id: format!("{EXTERNAL_PREFIX}{}", ask.approval_id),
            trigger: Trigger::Unknown,
            connection: Connection::External,
            outcome_unknown: ask.outcome_unknown,
            ..RequestContext::default()
        };
        return (app, tool, ask.args.clone(), Caller::External { client: None }, context);
    }
    let context = RequestContext {
        call_id: format!("{HELD_PREFIX}{}", ask.approval_id),
        trigger: Trigger::Person,
        batch: Some(Batch { id: format!("{HELD_PREFIX}{}", ask.turn), plan: ask.plan.clone() }),
        outcome_unknown: ask.outcome_unknown,
        ..RequestContext::default()
    };
    (app, tool, ask.args.clone(), Caller::SystemAgent, context)
}

/// The keyboard while the pane is open. True when it was the pane's.
/// Characters are not typed here: they arrive as text input
/// ([`text_input`]), as they do for makepad's `TextInput`.
pub fn key(e: &KeyEvent) -> bool {
    if !is_open() {
        return false;
    }
    match composer::key(e) {
        composer::Key::Close => close(),
        composer::Key::Send => send_draft(),
        composer::Key::Backspace => with(|c| {
            if c.draft.backspace() {
                c.ui_generation += 1;
            }
        }),
        composer::Key::NewLine => with(|c| {
            c.draft.newline();
            c.ui_generation += 1;
        }),
        composer::Key::Delete => with(|c| {
            if c.draft.delete_forward() {
                c.ui_generation += 1;
            }
        }),
        composer::Key::Move(m) => {
            let layout = view::prompt_layout(composer::Pane::System);
            with(|c| {
                c.draft.motion(m, &layout);
                c.ui_generation += 1;
            });
        }
        composer::Key::SelectAll => with(|c| {
            c.draft.select_all();
            c.ui_generation += 1;
        }),
        composer::Key::New => new_conversation(),
        composer::Key::Stop => interrupt(),
        composer::Key::Swallow => {}
        // Other keys stay the pane's too, unless they are shortcuts or
        // function keys (the shell's: F8, F9).
        composer::Key::Pass => return composer::keeps_unused(e),
    }
    true
}

/// Text input while the pane is open: typed characters, a paste, the
/// phone's input method (see [`composer`]).
pub fn text_input(event: &makepad_widgets::makepad_platform::event::TextInputEvent) -> bool {
    if !is_open() {
        return false;
    }
    let submit = with(|c| {
        if c.draft.text_input(event) {
            c.ui_generation += 1;
        }
        c.draft.take_submit()
    });
    if submit {
        send_draft();
    }
    true
}

/// `--test-action system-chat` (open the pane) and
/// `--test-action system-chat-send:<text>` (open it and send a prompt),
/// for hidden-window runs.
pub fn test_action(name: &str) -> bool {
    if name == "system-chat" {
        open();
        return true;
    }
    if let Some(text) = name.strip_prefix("system-chat-send:") {
        open();
        send(text);
        return true;
    }
    false
}

pub fn script_mod(vm: &mut ScriptVm) {
    view::script_mod(vm);
}
