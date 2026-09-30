//! The system chat pane, drawn by the shell like its other surfaces
//! (approvals/view.rs, glance_panel.rs): a column at the right of the
//! desktop, the whole screen on a phone (or any narrow window).
//!
//! It shows the conversation ([`super::model`]): the person's messages,
//! the assistant's streamed text, each tool call with its status, the
//! approvals the shell's sheet holds (a note only: the sheet answers them),
//! notices, and the open question with its options. Below it the prompt,
//! Send (Stop while a turn runs); above it New conversation and Close.
//!
//! The shell gives it pointer events first while it is open
//! ([`ShellSystemChat::pointer`]) and the keyboard (`super::key`).
//!
//! The same pane is the "Ask <app>" panel (`app_panel: true`,
//! [`crate::app_chat`]): an app agent's conversation, both lanes with their
//! speakers, its composer and Stop. On a desktop it stands left of the
//! system chat when both are open, so the two lanes show side by side; on a
//! phone it is a full-screen sheet.

use makepad_widgets::*;

use super::model::{ApprovalState, ChatModel, Item, Phase, Role, ToolStatus};
use crate::approvals::view::Buttons;
use crate::shell::ui::{contains, rect, DrawShellFill, HAlign, ShellDraw};
use crate::shell::{alpha, ShellTokens};

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.ShellSystemChatBase = #(ShellSystemChat::register_widget(vm))
    mod.widgets.ShellSystemChat = set_type_default() do mod.widgets.ShellSystemChatBase {
        width: Fill
        height: Fill
        draw_bg +: {}
        d +: {}
    }
}

/// The desktop column's width.
pub const PANE_W: f64 = 440.0;
/// Narrower than this, the pane takes the whole screen (the phone surface).
pub const FULL_SCREEN_BELOW: f64 = 720.0;
const PAD: f64 = 16.0;
const FIELD_H: f64 = 36.0;

#[derive(Clone, Debug, PartialEq)]
enum Hit {
    Close,
    New,
    Send,
    Stop,
    Option { question: String, count: usize, label: String },
    OpenProviders,
    Field,
    Pane,
}

/// What a pointer event did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Not the pane's.
    Ignored,
    Taken,
    /// "Open AI providers": the shell launches it.
    OpenProviders,
}

/// The line of text the header shows for a phase.
pub fn phase_text(phase: &Phase) -> String {
    match phase {
        Phase::Idle | Phase::Connecting => "Connecting to the assistant\u{2026}".into(),
        Phase::NoKernel(why) => format!("The assistant isn't available on this device: {why}."),
        Phase::NoProvider => "No model provider is set up yet. Add one in AI providers.".into(),
        Phase::Ready => "The system agent \u{00b7} ready".into(),
        Phase::Running { .. } => "Working\u{2026}".into(),
        Phase::Reconnecting(why) => format!("{why}; reconnecting\u{2026}"),
    }
}

/// One drawn line of the transcript.
#[derive(Clone, Debug, PartialEq)]
struct Line {
    text: String,
    bold: bool,
    small: bool,
    dim: bool,
    accent: bool,
    gap_before: f64,
}

#[derive(Script, ScriptHook, Widget)]
pub struct ShellSystemChat {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[redraw]
    #[live]
    draw_bg: DrawShellFill,
    #[live]
    d: ShellDraw,
    #[live]
    tokens: ShellTokens,
    /// The "Ask <app>" panel ([`crate::app_chat`]) instead of the system chat.
    #[live]
    app_panel: bool,
    #[rust]
    area: Area,
    #[rust]
    hits: Vec<(Rect, Hit)>,
    #[rust]
    pane: Rect,
    #[rust]
    down: Option<Hit>,
    #[rust]
    hover: Option<Rect>,
    /// How far the transcript can scroll back.
    #[rust]
    max_scroll: f64,
    /// What the last frame showed, one string per line (for tests and the
    /// hidden-window runs' logs).
    #[rust]
    pub shown: Vec<String>,
}

/// The conversation a pane shows: the system chat's or an app's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    System,
    App,
}

impl Source {
    fn is_open(self) -> bool {
        match self {
            Source::System => super::is_open(),
            Source::App => crate::app_chat::is_open(),
        }
    }
    fn snapshot(self) -> ChatModel {
        match self {
            Source::System => super::snapshot(),
            Source::App => crate::app_chat::snapshot(),
        }
    }
    fn draft(self) -> String {
        match self {
            Source::System => super::draft(),
            Source::App => crate::app_chat::draft(),
        }
    }
    fn scroll(self) -> f64 {
        match self {
            Source::System => super::scroll(),
            Source::App => crate::app_chat::scroll(),
        }
    }
    fn scroll_by(self, dy: f64, max: f64) {
        match self {
            Source::System => super::scroll_by(dy, max),
            Source::App => crate::app_chat::scroll_by(dy, max),
        }
    }
    fn title(self) -> String {
        match self {
            Source::System => "Assistant".into(),
            Source::App => format!("Ask {}", crate::app_chat::app().map(|a| a.name).unwrap_or_default()),
        }
    }
    fn status(self, model: &ChatModel) -> String {
        match self {
            Source::System => phase_text(model.phase()),
            Source::App => crate::app_chat::status_text(),
        }
    }
    fn placeholder(self, question: bool) -> String {
        if question {
            return "Answer the question\u{2026}".into();
        }
        match self {
            Source::System => "Ask the system agent\u{2026}".into(),
            Source::App => format!("Ask {}\u{2026}", crate::app_chat::app().map(|a| a.name).unwrap_or_default()),
        }
    }
    fn hint(self) -> String {
        match self {
            Source::System => "Ask the system agent anything: it can read and write its own workspace, search the web and brief your apps' agents. It asks you before anything outward.".into(),
            Source::App => {
                let name = crate::app_chat::app().map(|a| a.name).unwrap_or_default();
                format!("Talk to {name}'s agent. The system agent can ask it things too: both show here, each with who spoke, and each side sees the other's recent turns. Stop stops both.")
            }
        }
    }
    fn assistant_label(self) -> &'static str {
        "Assistant"
    }
    /// The composer is usable.
    fn usable(self, model: &ChatModel) -> bool {
        match self {
            Source::System => matches!(model.phase(), Phase::Ready | Phase::Running { .. }),
            Source::App => crate::app_chat::status() == crate::app_chat::Status::Ready,
        }
    }
}

impl ShellSystemChat {
    fn source(&self) -> Source {
        if self.app_panel {
            Source::App
        } else {
            Source::System
        }
    }

    fn hit_at(&self, p: Vec2d) -> Option<Hit> {
        self.hits.iter().find(|(r, h)| *h != Hit::Pane && contains(*r, p)).or_else(|| self.hits.iter().find(|(r, _)| contains(*r, p))).map(|(_, h)| h.clone())
    }

    /// The shell's pointer hook: the pane's own rect is its own while open.
    pub fn pointer(&mut self, cx: &mut Cx, event: &Event) -> Outcome {
        let source = self.source();
        if !source.is_open() || self.pane.size.x <= 0.0 {
            return Outcome::Ignored;
        }
        match event {
            Event::Scroll(e) if contains(self.pane, e.abs) => {
                source.scroll_by(e.scroll.y, self.max_scroll);
                self.redraw(cx);
                return Outcome::Taken;
            }
            Event::MouseMove(e) => {
                let hover = self.hits.iter().find(|(r, h)| *h != Hit::Pane && contains(*r, e.abs)).map(|(r, _)| *r);
                if hover != self.hover {
                    self.hover = hover;
                    self.redraw(cx);
                }
                return if contains(self.pane, e.abs) { Outcome::Taken } else { Outcome::Ignored };
            }
            _ => {}
        }
        let (down, up) = match event {
            Event::MouseDown(e) => (Some(e.abs), None),
            Event::MouseUp(e) => (None, Some(e.abs)),
            Event::TouchUpdate(e) => (
                e.touches.iter().find(|t| t.state == makepad_widgets::makepad_platform::event::TouchState::Start).map(|t| t.abs),
                e.touches.iter().find(|t| t.state == makepad_widgets::makepad_platform::event::TouchState::Stop).map(|t| t.abs),
            ),
            _ => (None, None),
        };
        if let Some(p) = down {
            if !contains(self.pane, p) {
                return Outcome::Ignored;
            }
            self.down = self.hit_at(p);
            // A press in a pane gives it the keyboard.
            crate::app_chat::focus(source == Source::App);
            if self.down == Some(Hit::Field) {
                cx.show_text_ime(self.area, dvec2(self.pane.pos.x + PAD, self.pane.pos.y + self.pane.size.y - PAD - FIELD_H));
            }
            return Outcome::Taken;
        }
        if let Some(p) = up {
            let hit = self.hit_at(p);
            let pressed = self.down.take();
            if hit.is_none() || hit != pressed {
                return if contains(self.pane, p) { Outcome::Taken } else { Outcome::Ignored };
            }
            let outcome = act(source, hit.unwrap());
            self.redraw(cx);
            return outcome;
        }
        Outcome::Ignored
    }

    fn transcript(&mut self, cx: &mut Cx2d, model: &ChatModel, width: f64, tok: &ShellTokens) -> Vec<Line> {
        let body = tok.font.body;
        let small = tok.font.body_small;
        let mut lines = Vec::new();
        let running = model.phase().running_turn().is_some();
        let last = model.items.len().saturating_sub(1);
        let push_wrapped = |d: &mut ShellDraw, cx: &mut Cx2d, lines: &mut Vec<Line>, text: &str, small_text: bool, dim: bool, accent: bool, gap: f64| {
            let px = if small_text { small } else { body };
            let mut first = true;
            for para in text.split('\n') {
                let wrapped = if para.trim().is_empty() { vec![String::new()] } else { d.wrap(cx, false, px, para, width, 400) };
                for l in wrapped {
                    lines.push(Line { text: l, bold: false, small: small_text, dim, accent, gap_before: if first { gap } else { 0.0 } });
                    first = false;
                }
            }
        };
        for (i, item) in model.items.iter().enumerate() {
            match item {
                Item::Message { role, text, speaker, .. } => {
                    let who = match (speaker, role) {
                        (Some(name), _) => name.as_str(),
                        (None, Role::User) => "You",
                        (None, Role::Assistant) => self.source().assistant_label(),
                    };
                    lines.push(Line { text: who.into(), bold: true, small: true, dim: *role == Role::User, accent: false, gap_before: 12.0 });
                    let mut text = text.clone();
                    if running && i == last && *role == Role::Assistant {
                        text.push('\u{258d}');
                    }
                    push_wrapped(&mut self.d, cx, &mut lines, &text, false, false, false, 2.0);
                }
                Item::Tool { name, status, detail, .. } => {
                    let state = match status {
                        ToolStatus::Running => "running\u{2026}",
                        ToolStatus::Done => "done",
                        ToolStatus::Failed => "failed",
                    };
                    let mut text = format!("\u{2699} {name} \u{00b7} {state}");
                    if !detail.is_empty() {
                        text.push_str(&format!(" \u{00b7} {detail}"));
                    }
                    push_wrapped(&mut self.d, cx, &mut lines, &text, true, true, false, 6.0);
                }
                Item::Approval { tool, title, state, .. } => {
                    let state = match state {
                        ApprovalState::Waiting => "waiting for you on the approval sheet",
                        ApprovalState::Approved => "approved",
                        ApprovalState::Denied => "denied",
                        ApprovalState::Cancelled => "withdrawn",
                        ApprovalState::External => "asked by an outside client; that client answers it",
                    };
                    let what = if title.is_empty() { tool.clone() } else { format!("{tool}: {title}") };
                    push_wrapped(&mut self.d, cx, &mut lines, &format!("\u{2691} Approval \u{00b7} {what} \u{00b7} {state}"), true, false, true, 6.0);
                }
                Item::Question { title, body: q, answered, .. } => {
                    let head = if title.is_empty() { "Question".to_string() } else { title.clone() };
                    push_wrapped(&mut self.d, cx, &mut lines, &format!("? {head}: {q}"), false, false, true, 10.0);
                    if let Some(a) = answered {
                        push_wrapped(&mut self.d, cx, &mut lines, &format!("You answered: {a}"), true, true, false, 2.0);
                    }
                }
                Item::Notice(text) => push_wrapped(&mut self.d, cx, &mut lines, text, true, true, false, 8.0),
            }
        }
        lines
    }

    fn draw_pane(&mut self, cx: &mut Cx2d, screen: Rect) {
        self.hits.clear();
        self.shown.clear();
        let source = self.source();
        if !source.is_open() {
            self.pane = Rect::default();
            return;
        }
        let tok = self.d.tokens(self.tokens);
        let full = screen.size.x < FULL_SCREEN_BELOW;
        let pane = if full {
            screen
        } else {
            let gap = tok.spacing.gaps_out;
            // "Ask <app>" stands left of the system chat when both are open.
            let beside = if source == Source::App && super::is_open() { PANE_W + gap } else { 0.0 };
            rect(screen.pos.x + screen.size.x - gap - PANE_W - beside, screen.pos.y + gap, PANE_W, (screen.size.y - gap * 2.0).max(240.0))
        };
        self.pane = pane;
        self.d.card(cx, pane, &tok.popups);
        self.hits.push((pane, Hit::Pane));
        let ink = tok.popups.text;
        let dim = alpha(ink, 0.62);
        let accent = tok.notifications.countdown;
        let x = pane.pos.x + PAD;
        let cw = pane.size.x - PAD * 2.0;
        let mut hits = Vec::new();
        let mut shown = Vec::new();
        let model = source.snapshot();
        let draft = source.draft();
        let hover = self.hover;

        // Header.
        let mut y = pane.pos.y + PAD;
        {
            let mut b = Buttons { d: &mut self.d, tok, hover };
            let close_w = b.width(cx, "Close");
            let close = b.draw(cx, x + cw - close_w, y, close_w, "Close", false);
            hits.push((close, Hit::Close));
            let mut left = close.pos.x;
            if source == Source::System {
                let new_w = b.width(cx, "New conversation");
                let new = b.draw(cx, close.pos.x - 8.0 - new_w, y, new_w, "New conversation", false);
                hits.push((new, Hit::New));
                left = new.pos.x;
            }
            b.d.label_elided(cx, rect(x, y, left - x - 8.0, 24.0), true, tok.font.heading, ink, HAlign::Left, &source.title());
        }
        shown.push(source.title());
        y += 30.0;
        let status = source.status(&model);
        self.d.label_elided(cx, rect(x, y, cw, 16.0), false, tok.font.body_small, dim, HAlign::Left, &status);
        shown.push(status);
        y += 22.0;
        self.d.separator(cx, rect(x, y, cw, 1.0), ink, 0.12);
        let top = y + 6.0;

        // Composer at the bottom.
        let bottom = pane.pos.y + pane.size.y - PAD;
        let field_y = bottom - FIELD_H;
        let running = model.phase().running_turn().is_some();
        let (label, hit) = if running { ("Stop", Hit::Stop) } else { ("Send", Hit::Send) };
        let usable = source.usable(&model);
        {
            let mut b = Buttons { d: &mut self.d, tok, hover };
            let bw = b.width(cx, label);
            let button = b.draw(cx, x + cw - bw, field_y + (FIELD_H - 28.0) * 0.5, bw, label, usable && (running || !draft.trim().is_empty()));
            hits.push((button, hit));
            let field = rect(x, field_y, cw - bw - 8.0, FIELD_H);
            let placeholder = source.placeholder(model.open_question().is_some());
            b.d.text_field(cx, field, &tok, &draft, &placeholder, true, hover == Some(field), ink);
            hits.push((field, Hit::Field));
        }
        shown.push(format!("prompt: {draft}"));
        let mut list_bottom = field_y - 10.0;

        // The open question's options, over the composer.
        if let Some(Item::Question { id, options, count, answered: None, .. }) = model.items.iter().rev().find(|i| matches!(i, Item::Question { answered: None, .. })) {
            if !options.is_empty() {
                let mut b = Buttons { d: &mut self.d, tok, hover };
                let row_y = list_bottom - 30.0;
                let mut ox = x;
                for label in options {
                    let w = b.width(cx, label).min(cw);
                    if ox + w > x + cw {
                        break;
                    }
                    let r = b.draw(cx, ox, row_y, w, label, false);
                    hits.push((r, Hit::Option { question: id.clone(), count: *count, label: label.clone() }));
                    shown.push(format!("option: {label}"));
                    ox += w + 8.0;
                }
                list_bottom = row_y - 8.0;
            }
        }

        // No provider: say so, with the way to fix it.
        if source == Source::System && model.phase() == &Phase::NoProvider {
            let mut b = Buttons { d: &mut self.d, tok, hover };
            let w = b.width(cx, "Open AI providers");
            let r = b.draw(cx, x, top + 12.0, w, "Open AI providers", true);
            hits.push((r, Hit::OpenProviders));
            shown.push("Open AI providers".into());
        }

        // The transcript, newest at the bottom, scrolled back by `scroll`.
        let lines = self.transcript(cx, &model, cw, &tok);
        let heights: Vec<f64> = lines.iter().map(|l| l.gap_before + if l.small { tok.font.body_small * 1.45 } else { tok.font.body * 1.45 }).collect();
        let total: f64 = heights.iter().sum();
        let room = (list_bottom - top).max(0.0);
        self.max_scroll = (total - room).max(0.0);
        let scroll = source.scroll().min(self.max_scroll);
        let mut ly = list_bottom - total + scroll;
        for (line, h) in lines.iter().zip(&heights) {
            let line_top = ly + line.gap_before;
            ly += h;
            if line_top < top || ly > list_bottom + 0.5 {
                continue;
            }
            let px = if line.small { tok.font.body_small } else { tok.font.body };
            let color = if line.accent { accent } else if line.dim { dim } else { ink };
            self.d.label_elided(cx, rect(x, line_top, cw, h - line.gap_before), line.bold, px, color, HAlign::Left, &line.text);
        }
        if model.items.is_empty() && usable {
            let hint = source.hint();
            let wrapped = self.d.wrap(cx, false, tok.font.body_small, &hint, cw, 6);
            let mut hy = top + 12.0;
            for l in wrapped {
                self.d.label_elided(cx, rect(x, hy, cw, 18.0), false, tok.font.body_small, dim, HAlign::Left, &l);
                hy += 18.0;
            }
        }
        shown.extend(lines.iter().map(|l| l.text.clone()));
        self.hits.extend(hits);
        self.shown = shown;
    }
}

/// What a press does.
fn act(source: Source, hit: Hit) -> Outcome {
    if source == Source::App {
        match hit {
            Hit::Close => crate::app_chat::close(),
            Hit::Send => crate::app_chat::send_draft(),
            Hit::Stop => crate::app_chat::stop(),
            Hit::Option { question, count, label } => crate::app_chat::answer_option(&question, count, &label),
            Hit::New | Hit::OpenProviders | Hit::Field | Hit::Pane => {}
        }
        return Outcome::Taken;
    }
    match hit {
        Hit::Close => super::close(),
        Hit::New => super::new_conversation(),
        Hit::Send => super::send_draft(),
        Hit::Stop => super::interrupt(),
        Hit::Option { question, count, label } => super::answer_option(&question, count, &label),
        Hit::OpenProviders => return Outcome::OpenProviders,
        Hit::Field | Hit::Pane => {}
    }
    Outcome::Taken
}

impl Widget for ShellSystemChat {
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let screen = cx.turtle().rect();
        self.d.begin_surface(cx);
        self.draw_pane(cx, screen);
        self.d.end_surface(cx);
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, _cx: &mut Cx, _event: &Event, _scope: &mut Scope) {}
}
