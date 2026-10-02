//! Agents' questions to the person (ADR 0004 §6): the shell's request
//! model for octos's `ask_user_question` on app peers, and where each
//! question is asked. A routing layer, not a view.
//!
//! | In | Here | Out |
//! | --- | --- | --- |
//! | a broker's `user_question/requested` on an app peer's session or context ([`crate::host_tools::ShellToolHost`]) | [`Questions::requested`]: one [`Request`] with its owning app, peer, account, context, turn, the questions and options, who started the turn ([`Origin`]) and so which [`Conversation`] it belongs to | every [`Consumer`] hears it ([`subscribe`]) |
//! | the turn ended (the broker saw its terminal) | [`Questions::closed`] | the consumers hear it closed |
//! | the person answered on a shell surface | [`answer`], with a [`PersonAnswer`] | `user_question/respond` on the broker's link, once |
//! | nobody answered within the prompt deadline (10 min; `OCTOSENSE_PROMPT_DEADLINE_SECS`) | [`Questions::tick`] | declined (free text saying it expired, never an option), [`State::Expired`], kept visible |
//! | the person pressed Stop on the app's conversation | [`Questions::stop_agent`] | declined, the consumers hear it answered |
//!
//! **Which conversation: the turn's origin, not the session.** An app
//! agent's conversation has two lanes that run in parallel: the person's
//! (a sharing request context: the app's UI, its cards, the app) and the
//! system agent's (the peer session, `peer/input`); plain request contexts
//! are the app's too. A question from a turn the person or the
//! app started is the app's: [`Conversation::App`]. A question from a turn
//! the system agent started (`peer/input`) is the system agent's to relay,
//! so it goes to the system chat: [`Conversation::SystemChat`]. The
//! request keeps the turn's [`Origin`]: the one octos reports once it
//! carries `origin` on the event ([`Request::origin_reported`]), until then
//! the broker's derivation (the turns it started for `peer/input` are the
//! system agent's; a context's the person's; any other the app's). There
//! is no `host.ask`: octos keeps `ask_user_question` on host-driven turns
//! when the peer's `generic_tools` lists it.
//!
//! **Consumers.** The system chat subscribes and shows its questions in
//! its conversation (`crate::system_chat`). An app's are shown in its "Ask
//! <app>" panel (`crate::app_chat`) while that is open, and otherwise on
//! the shell's app-conversation surface (the approvals overlay's card,
//! `crate::approvals::view`): one surface at a time. Notification and
//! glance cards can subscribe the same way; none of them owns a question,
//! and none renders anything here.
//!
//! **Only the person answers, only through the shell.** The one answer
//! path is [`answer`], which needs a [`PersonAnswer`]: made only by the
//! shell's own surfaces from the person's tap or typing
//! ([`PersonAnswer::from_shell_surface`]; a test scans the sources). The
//! answer handle ([`QuestionAnswer`]) is the broker's, made for the host
//! connection the question came on; nothing an app reaches (its context,
//! a host service, an executor, the peer link, `host.request`) carries one
//! or can name a question: the broker refuses an app's attempt to answer
//! an id the host holds, and the peer link and the `octos` host service
//! have no method for it.

#[cfg(test)]
mod tests;

use std::sync::Mutex;

use crate::ai_host::app_peers::host_tools::{AgentQuestion, QuestionAnswer, QuestionItem, QuestionReply, TurnOrigin};

/// Who started the turn that asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The person, in the app (its UI, its cards).
    Person,
    /// The app itself (a trigger, its own run).
    App,
    /// The system agent (`peer/input`): its request to the app's agent.
    SystemAgent,
}

impl From<TurnOrigin> for Origin {
    fn from(o: TurnOrigin) -> Origin {
        match o {
            TurnOrigin::Person => Origin::Person,
            TurnOrigin::App => Origin::App,
            TurnOrigin::SystemAgent => Origin::SystemAgent,
        }
    }
}

/// Where the question is asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conversation {
    /// The owning app's conversation.
    App(String),
    /// The system chat (the system agent's conversation).
    SystemChat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Open,
    /// Answered by the person (a summary of the answer).
    Answered(String),
    /// Its turn ended unanswered.
    Closed,
    /// Nobody answered within the deadline: declined, with why ("no answer
    /// in 10 min"). Shown as expired, not removed.
    Expired(String),
}

/// One agent question, as every consumer sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The shell's id for it.
    pub id: u64,
    /// The owning app (`rinx`, `os.news`).
    pub app: String,
    /// The broker's app id (a script app's peer is `card.<app id>`).
    pub peer_app: String,
    pub account: Option<String>,
    /// The asking request context, and its client (a Rinx mini app).
    pub context_id: Option<String>,
    pub client: Option<String>,
    pub session_id: String,
    pub turn_id: String,
    /// The kernel's question id.
    pub question_id: String,
    pub title: String,
    pub body: String,
    pub items: Vec<QuestionItem>,
    /// Who started the turn that asked; the conversation follows from it.
    pub origin: Origin,
    /// Whether octos reported the origin (else the broker derived it).
    pub origin_reported: bool,
    pub conversation: Conversation,
    pub state: State,
    /// When it was asked (Unix seconds): its deadline counts from here.
    pub asked: u64,
}

impl Request {
    /// How many answers the kernel takes (one per question).
    pub fn answer_count(&self) -> usize {
        self.items.len().max(1)
    }
    /// "Rinx's agent asks", for a surface's heading.
    pub fn asked_by(&self) -> String {
        let app = crate::approvals::sheet::app_label(&self.app);
        match (&self.origin, &self.client) {
            (Origin::SystemAgent, _) => format!("{app}'s agent asks (for the assistant)"),
            (_, Some(client)) => format!("{app}'s agent asks ({client})"),
            _ => format!("{app}'s agent asks"),
        }
    }
    /// The first question's text (or the fallback body).
    pub fn text(&self) -> &str {
        self.items.first().map(|q| q.question.as_str()).filter(|q| !q.is_empty()).unwrap_or(&self.body)
    }
    /// The first question's option labels.
    pub fn options(&self) -> Vec<String> {
        self.items.first().map(|q| q.options.iter().map(|(l, _)| l.clone()).collect()).unwrap_or_default()
    }
}

/// A subscriber: a surface that shows questions (the system chat, the
/// app-conversation overlay, later notification and glance cards). Called
/// with the model's lock held: take what you need and return; never call
/// back into this module from here.
pub trait Consumer: Send {
    fn changed(&mut self, request: &Request);
}

/// Proof that the person answered on one of the shell's own surfaces. Only
/// [`PersonAnswer::from_shell_surface`] makes one, and only the shell's
/// surfaces call it (a test scans the sources).
pub struct PersonAnswer(());

impl PersonAnswer {
    /// The person tapped an option or typed an answer on a shell surface.
    pub(crate) fn from_shell_surface() -> PersonAnswer {
        PersonAnswer(())
    }
}

/// How many answered or closed requests are kept for the surfaces.
const KEPT: usize = 64;

struct Entry {
    request: Request,
    answer: QuestionAnswer,
}

/// The model.
#[derive(Default)]
pub struct Questions {
    next: u64,
    entries: Vec<Entry>,
    consumers: Vec<Box<dyn Consumer>>,
    generation: u64,
    /// The prompt deadline in seconds (`None`: `host_tools::prompt_deadline`).
    pub deadline_s: Option<u64>,
}

fn app_of_peer(peer_app: &str) -> &str {
    crate::host_tools::app_of_peer(peer_app)
}

impl Questions {
    pub fn subscribe(&mut self, consumer: Box<dyn Consumer>) {
        self.consumers.push(consumer);
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn changed(&mut self, index: usize) {
        self.generation += 1;
        let request = self.entries[index].request.clone();
        for consumer in &mut self.consumers {
            consumer.changed(&request);
        }
    }

    /// A broker handed the host an agent's question: route it.
    pub fn requested(&mut self, peer_app: &str, account: Option<&str>, question: AgentQuestion, answer: QuestionAnswer) -> u64 {
        self.requested_at(peer_app, account, question, answer, crate::approvals::now())
    }

    /// [`Questions::requested`], asked at `now`.
    pub fn requested_at(&mut self, peer_app: &str, account: Option<&str>, question: AgentQuestion, answer: QuestionAnswer, now: u64) -> u64 {
        let app = app_of_peer(peer_app).to_string();
        let origin = Origin::from(question.turn_origin);
        let conversation = match origin {
            Origin::SystemAgent => Conversation::SystemChat,
            Origin::Person | Origin::App => Conversation::App(app.clone()),
        };
        self.next += 1;
        let id = self.next;
        let request = Request {
            id,
            app,
            peer_app: peer_app.to_string(),
            account: account.map(str::to_string),
            context_id: question.context_id,
            client: question.client,
            session_id: question.session_id,
            turn_id: question.turn_id,
            question_id: question.question_id,
            title: question.title,
            body: question.body,
            items: question.questions,
            origin,
            origin_reported: question.origin_reported,
            conversation,
            state: State::Open,
            asked: now,
        };
        self.entries.push(Entry { request, answer });
        let index = self.entries.len() - 1;
        self.changed(index);
        self.prune();
        id
    }

    /// The turn that asked ended: the question can no longer be answered.
    pub fn closed(&mut self, peer_app: &str, question_id: &str) {
        let Some(index) = self.entries.iter().position(|e| e.request.peer_app == peer_app && e.request.question_id == question_id && e.request.state == State::Open) else { return };
        self.entries[index].request.state = State::Closed;
        self.changed(index);
    }

    /// The person's answer, from a shell surface: sent to the kernel once.
    pub fn answer(&mut self, id: u64, replies: &[QuestionReply], _by: &PersonAnswer) -> Result<(), String> {
        let index = self.entries.iter().position(|e| e.request.id == id).ok_or("no such question")?;
        let entry = &self.entries[index];
        if entry.request.state != State::Open {
            return Err("this question is no longer open".into());
        }
        let count = entry.request.answer_count();
        if replies.is_empty() || replies.len() > count {
            return Err(format!("{count} answer(s) expected"));
        }
        // One answer per question: the rest are left blank.
        let mut all = replies.to_vec();
        all.resize(count, QuestionReply::default());
        if !entry.answer.respond(&all) {
            return Err("already answered".into());
        }
        let summary = replies.iter().map(|r| r.free_text.clone().unwrap_or_else(|| r.selected_labels.join(", "))).collect::<Vec<_>>().join("; ");
        self.entries[index].request.state = State::Answered(summary);
        self.changed(index);
        Ok(())
    }

    fn deadline(&self) -> std::time::Duration {
        self.deadline_s.map(std::time::Duration::from_secs).unwrap_or_else(crate::ai_host::app_peers::host_tools::prompt_deadline)
    }

    /// Once a second: a question nobody answered within the deadline is
    /// declined (free text saying so, never an option for the person) and
    /// shown expired. True when any expired.
    pub fn tick(&mut self, now: u64) -> bool {
        let deadline = self.deadline();
        let reason = crate::ai_host::app_peers::host_tools::expiry_reason(deadline);
        let due: Vec<usize> = (0..self.entries.len())
            .filter(|&i| self.entries[i].request.state == State::Open && now >= self.entries[i].request.asked.saturating_add(deadline.as_secs()))
            .collect();
        for &i in &due {
            let count = self.entries[i].request.answer_count();
            self.entries[i].answer.expire(count, &reason);
            self.entries[i].request.state = State::Expired(reason.clone());
            self.changed(i);
        }
        !due.is_empty()
    }

    /// The person pressed Stop on `app`'s conversation: its agent's open
    /// questions are declined ("stopped"), and the consumers hear it.
    pub fn stop_agent(&mut self, app: &str) -> usize {
        let open: Vec<usize> = (0..self.entries.len()).filter(|&i| self.entries[i].request.state == State::Open && self.entries[i].request.app == app).collect();
        for &i in &open {
            let count = self.entries[i].request.answer_count();
            let replies = vec![QuestionReply::text("The person stopped this turn; no answer."); count];
            self.entries[i].answer.respond(&replies);
            self.entries[i].request.state = State::Answered("(stopped)".into());
            self.changed(i);
        }
        open.len()
    }

    /// Expired questions of every app's conversation, newest last.
    pub fn expired_in_apps(&self) -> Vec<Request> {
        self.entries.iter().filter(|e| matches!(e.request.state, State::Expired(_)) && matches!(e.request.conversation, Conversation::App(_))).map(|e| e.request.clone()).collect()
    }

    /// The person saw an expired question's record: it is let go.
    pub fn dismiss(&mut self, id: u64) {
        if let Some(i) = self.entries.iter().position(|e| e.request.id == id && matches!(e.request.state, State::Expired(_))) {
            self.entries.remove(i);
            self.generation += 1;
        }
    }

    /// The open questions of one conversation, oldest first.
    pub fn open(&self, conversation: &Conversation) -> Vec<Request> {
        self.entries.iter().filter(|e| e.request.state == State::Open && &e.request.conversation == conversation).map(|e| e.request.clone()).collect()
    }

    /// The open questions of every app's conversation, oldest first.
    pub fn open_in_apps(&self) -> Vec<Request> {
        self.entries.iter().filter(|e| e.request.state == State::Open && matches!(e.request.conversation, Conversation::App(_))).map(|e| e.request.clone()).collect()
    }

    pub fn get(&self, id: u64) -> Option<&Request> {
        self.entries.iter().find(|e| e.request.id == id).map(|e| &e.request)
    }

    fn prune(&mut self) {
        let done = self.entries.iter().filter(|e| e.request.state != State::Open).count();
        if done > KEPT {
            let mut drop = done - KEPT;
            self.entries.retain(|e| {
                if drop > 0 && e.request.state != State::Open {
                    drop -= 1;
                    false
                } else {
                    true
                }
            });
        }
    }
}

static QUESTIONS: Mutex<Option<Questions>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Questions) -> R) -> R {
    let mut guard = QUESTIONS.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Questions::default))
}

fn wake() {
    makepad_widgets::makepad_platform::thread::SignalToUI::set_ui_signal();
}

/// The process's prompt deadline for questions, in seconds (`None`: the
/// default), for the two-lane scenario tests' short deadlines.
#[cfg(test)]
pub(crate) fn set_deadline_for_tests(secs: Option<u64>) {
    with(|q| q.deadline_s = secs);
}

/// Add a consumer (at startup).
pub fn subscribe(consumer: Box<dyn Consumer>) {
    with(|q| q.subscribe(consumer));
}

/// From the host (a broker's thread).
pub fn requested(peer_app: &str, account: Option<&str>, question: AgentQuestion, answer: QuestionAnswer) -> u64 {
    let id = with(|q| q.requested(peer_app, account, question, answer));
    wake();
    id
}

pub fn closed(peer_app: &str, question_id: &str) {
    with(|q| q.closed(peer_app, question_id));
    wake();
}

/// The person's answer, from a shell surface.
pub fn answer(id: u64, replies: &[QuestionReply], by: &PersonAnswer) -> Result<(), String> {
    let result = with(|q| q.answer(id, replies, by));
    wake();
    result
}

/// Once a second (with the approval router's tick). True when any expired.
pub fn tick(now: u64) -> bool {
    let any = with(|q| q.tick(now));
    if any {
        wake();
    }
    any
}

/// The Stop on `app`'s conversation: its open questions are declined.
pub fn stop_agent(app: &str) -> usize {
    let n = with(|q| q.stop_agent(app));
    wake();
    n
}

pub fn expired_in_apps() -> Vec<Request> {
    with(|q| q.expired_in_apps())
}

pub fn dismiss(id: u64) {
    with(|q| q.dismiss(id));
    wake();
}

pub fn open(conversation: &Conversation) -> Vec<Request> {
    with(|q| q.open(conversation))
}

pub fn open_in_apps() -> Vec<Request> {
    with(|q| q.open_in_apps())
}

pub fn get(id: u64) -> Option<Request> {
    with(|q| q.get(id).cloned())
}

/// Bumped on every change (for redraws).
pub fn generation() -> u64 {
    with(|q| q.generation())
}
