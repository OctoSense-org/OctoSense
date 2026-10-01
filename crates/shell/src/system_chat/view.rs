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
//! ([`ShellSystemChat::pointer`]), the keyboard (`super::key`) and text
//! input (`super::text_input`, [`ShellSystemChat::ime`]).
//!
//! - **Placing it** (a desktop, not the full-screen surface): the header
//!   (title and status) moves the pane, an edge or a corner sizes it, each
//!   with its cursor; a double-click on the header puts it back in its
//!   column. The placement holds while the shell runs and stays on screen
//!   when the window shrinks ([`placed_pane`], [`dragged`]).
//! - **Scrolling**: the wheel, and a touch drag with its fling
//!   ([`TouchScroll`]): a phone sends no scroll events for a drag. A drag
//!   that starts on a button scrolls and presses nothing.
//! - **The keyboard of a phone**: a press in the prompt takes the key focus
//!   and asks for the input method, as makepad's `TextInput` does; taking
//!   the focus again is what brings back a keyboard the person dismissed
//!   (the platform ignores a request after a dismissal until then). While
//!   the prompt holds the focus, the pane answers the input method's state
//!   query with the prompt's text ([`super::composer`]).
//!
//! The same pane is the "Ask <app>" panel (`app_panel: true`,
//! [`crate::app_chat`]): an app agent's conversation, both lanes with their
//! speakers, and its composer. The two lanes are independent: Send shows
//! whenever the PERSON's lane is idle, even while the system agent's turn
//! runs, and Stop stops the person's own turn. The system agent's running
//! turn has its own row with "Stop the system agent's task". On a desktop
//! the panel stands left of the system chat when both are open, so the two
//! lanes show side by side; on a phone it is a full-screen sheet.

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
/// The smallest the person may size the desktop pane.
pub const MIN_W: f64 = 320.0;
pub const MIN_H: f64 = 240.0;
/// The resize band along the pane's edges, and its corners' reach.
const EDGE: f64 = 6.0;
const CORNER: f64 = 14.0;
/// The header the pane is moved by: the padding, the title and the status.
const HEADER_H: f64 = PAD + 30.0 + 22.0;
/// Two presses on the header this close (seconds) put the pane back.
const DOUBLE_PRESS_S: f64 = 0.4;

/// What a press on the desktop pane's frame drags: the header moves it; an
/// edge or a corner (by compass point) sizes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grip {
    Move,
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

impl Grip {
    fn cursor(self) -> MouseCursor {
        match self {
            Grip::Move => MouseCursor::Move,
            Grip::N | Grip::S => MouseCursor::NsResize,
            Grip::E | Grip::W => MouseCursor::EwResize,
            Grip::NE | Grip::SW => MouseCursor::NeswResize,
            Grip::NW | Grip::SE => MouseCursor::NwseResize,
        }
    }
}

/// `v` within `lo..=hi`, `lo` winning when the range is empty (a screen
/// smaller than the pane's minimum).
fn between(v: f64, lo: f64, hi: f64) -> f64 {
    v.min(hi).max(lo)
}

/// The part of the surface `r` inside the window `pass` (its size; zero
/// when unknown): the surface starts under the bar yet is the window's
/// height, so its bottom would hang past the window, and with it the
/// pane's bottom edge and padding.
pub fn visible(r: Rect, pass: Vec2d) -> Rect {
    if pass.x <= 0.0 || pass.y <= 0.0 {
        return r;
    }
    rect(r.pos.x, r.pos.y, r.size.x.min((pass.x - r.pos.x).max(0.0)), r.size.y.min((pass.y - r.pos.y).max(0.0)))
}

/// A placement kept on `screen`: no smaller than the minimum (unless the
/// screen is), no larger than the screen, wholly on it.
pub fn placed_pane(r: Rect, screen: Rect) -> Rect {
    let w = between(r.size.x, MIN_W.min(screen.size.x), screen.size.x);
    let h = between(r.size.y, MIN_H.min(screen.size.y), screen.size.y);
    let x = between(r.pos.x, screen.pos.x, screen.pos.x + screen.size.x - w);
    let y = between(r.pos.y, screen.pos.y, screen.pos.y + screen.size.y - h);
    rect(x, y, w, h)
}

/// The pane `from`, dragged by `grip` through `d`: moved, or one or two of
/// its edges moved, never past the minimum size or off `screen`.
pub fn dragged(grip: Grip, from: Rect, d: Vec2d, screen: Rect) -> Rect {
    if grip == Grip::Move {
        return placed_pane(rect(from.pos.x + d.x, from.pos.y + d.y, from.size.x, from.size.y), screen);
    }
    let (mut l, mut t) = (from.pos.x, from.pos.y);
    let (mut r, mut b) = (l + from.size.x, t + from.size.y);
    let (sr, sb) = (screen.pos.x + screen.size.x, screen.pos.y + screen.size.y);
    if matches!(grip, Grip::W | Grip::NW | Grip::SW) {
        l = between(l + d.x, screen.pos.x, r - MIN_W);
    }
    if matches!(grip, Grip::E | Grip::NE | Grip::SE) {
        r = between(r + d.x, l + MIN_W, sr);
    }
    if matches!(grip, Grip::N | Grip::NW | Grip::NE) {
        t = between(t + d.y, screen.pos.y, b - MIN_H);
    }
    if matches!(grip, Grip::S | Grip::SW | Grip::SE) {
        b = between(b + d.y, t + MIN_H, sb);
    }
    placed_pane(rect(l, t, r - l, b - t), screen)
}

/// The desktop pane's frame, as hits: corners, then edges, then the header,
/// so an edge wins over the header it crosses. Pushed after the controls,
/// which win over all of them.
fn grips(pane: Rect) -> Vec<(Rect, Hit)> {
    let (x, y, w, h) = (pane.pos.x, pane.pos.y, pane.size.x, pane.size.y);
    vec![
        (rect(x, y, CORNER, CORNER), Hit::Grip(Grip::NW)),
        (rect(x + w - CORNER, y, CORNER, CORNER), Hit::Grip(Grip::NE)),
        (rect(x, y + h - CORNER, CORNER, CORNER), Hit::Grip(Grip::SW)),
        (rect(x + w - CORNER, y + h - CORNER, CORNER, CORNER), Hit::Grip(Grip::SE)),
        (rect(x, y, w, EDGE), Hit::Grip(Grip::N)),
        (rect(x, y + h - EDGE, w, EDGE), Hit::Grip(Grip::S)),
        (rect(x, y, EDGE, h), Hit::Grip(Grip::W)),
        (rect(x + w - EDGE, y, EDGE, h), Hit::Grip(Grip::E)),
        (rect(x, y, w, HEADER_H.min(h)), Hit::Grip(Grip::Move)),
    ]
}

/// A drag of the pane's frame under way: by what, from where, and the pane
/// it started from.
#[derive(Clone, Copy, Debug)]
struct PaneDrag {
    grip: Grip,
    start: Vec2d,
    from: Rect,
}

#[derive(Clone, Debug, PartialEq)]
enum Hit {
    Close,
    New,
    Send,
    Stop,
    /// "Ask <app>": stop the system agent's turn in this conversation.
    StopSystemAgent,
    Option { question: String, count: usize, label: String },
    OpenProviders,
    Field,
    Pane,
    /// The desktop pane's frame ([`Grip`]).
    Grip(Grip),
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

/// The composer's button: Send, or Stop while the person's own turn runs.
/// `person_running`: the turn the person started (in "Ask <app>" the
/// person's lane only; the system agent's lane never turns Send into Stop).
/// The label, what a press does, and whether it is enabled.
fn composer_button(person_running: bool, usable: bool, draft: &str) -> (&'static str, Hit, bool) {
    if person_running {
        ("Stop", Hit::Stop, usable)
    } else {
        ("Send", Hit::Send, usable && !draft.trim().is_empty())
    }
}

/// A finger's width around a button that still counts as on it.
const FINGER: f64 = 8.0;

fn grown(r: Rect) -> Rect {
    rect(r.pos.x - FINGER, r.pos.y - FINGER, r.size.x + FINGER * 2.0, r.size.y + FINGER * 2.0)
}

/// Whether a target gets a finger's width of slop: buttons do; the pane,
/// the prompt and an answer option (an answer must be meant) do not.
fn has_slop(hit: &Hit) -> bool {
    !matches!(hit, Hit::Pane | Hit::Field | Hit::Option { .. } | Hit::Grip(_))
}

/// What a press at `p` hits: a control first (a button's finger's width
/// around it counts, for touch), else the pane itself.
fn hit_in(hits: &[(Rect, Hit)], p: Vec2d) -> Option<Hit> {
    hits.iter()
        .find(|(r, h)| *h != Hit::Pane && contains(*r, p))
        .or_else(|| hits.iter().find(|(r, h)| has_slop(h) && contains(grown(*r), p)))
        .or_else(|| hits.iter().find(|(r, _)| contains(*r, p)))
        .map(|(_, h)| h.clone())
}

/// A lift at `p` still on what was pressed (`hit` at `r`).
fn lifted_on(hit: &Hit, r: Rect, p: Vec2d) -> bool {
    contains(if has_slop(hit) { grown(r) } else { r }, p)
}

/// The label of the system agent's running row's control.
pub const STOP_SYSTEM_AGENT: &str = "Stop the system agent's task";

/// Touch scrolling: a drag past a small slop scrolls, and a lift while
/// moving flings, slowing down. Positions in pixels, times in seconds;
/// deltas in the pane's units (positive: back, toward older lines: a finger
/// moving down).
#[derive(Clone, Debug, Default)]
pub struct TouchScroll {
    /// The finger (uid), where it went down, its last y and time.
    touch: Option<(u64, Vec2d, f64, f64)>,
    dragging: bool,
    /// Pixels per second, smoothed over the drag.
    velocity: f64,
    /// The fling's speed and the time of its last step.
    fling: Option<(f64, f64)>,
}

impl TouchScroll {
    /// How far a finger moves before a press becomes a drag.
    pub const SLOP: f64 = 8.0;
    /// Slower than this, a fling stops.
    const MIN_SPEED: f64 = 30.0;
    /// The fling's speed halves about every 0.17 s.
    const DECAY: f64 = 4.0;

    pub fn start(&mut self, uid: u64, at: Vec2d, time: f64) {
        self.touch = Some((uid, at, at.y, time));
        self.dragging = false;
        self.velocity = 0.0;
        self.fling = None;
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// The finger moved: the scroll delta, once it is a drag.
    pub fn moved(&mut self, uid: u64, at: Vec2d, time: f64) -> Option<f64> {
        let (id, start, last_y, last_t) = self.touch?;
        if id != uid {
            return None;
        }
        if !self.dragging && (at - start).length() < Self::SLOP {
            return None;
        }
        let first = !self.dragging;
        self.dragging = true;
        // The first step of a drag starts where the slop ended, so the
        // content does not jump by the slop.
        let dy = if first { 0.0 } else { at.y - last_y };
        let dt = time - last_t;
        if dt > 0.0 && !first {
            self.velocity = self.velocity * 0.3 + (dy / dt) * 0.7;
        }
        self.touch = Some((id, start, at.y, time));
        Some(dy)
    }

    /// The finger lifted: true when it was a drag (so no press). A drag
    /// still moving starts a fling.
    pub fn stop(&mut self, uid: u64, time: f64) -> bool {
        let Some((id, _, _, last_t)) = self.touch else { return false };
        if id != uid {
            return false;
        }
        self.touch = None;
        let dragged = std::mem::take(&mut self.dragging);
        // A finger that rested before lifting does not fling.
        if dragged && time - last_t < 0.1 && self.velocity.abs() > Self::MIN_SPEED {
            self.fling = Some((self.velocity, time));
        }
        dragged
    }

    pub fn is_flinging(&self) -> bool {
        self.fling.is_some()
    }

    pub fn cancel_fling(&mut self) {
        self.fling = None;
    }

    /// One frame of a fling at `time`: the delta to scroll by.
    pub fn fling_step(&mut self, time: f64) -> Option<f64> {
        let (speed, last) = self.fling?;
        // A frame's time from another clock, or a long pause: one frame.
        let dt = if time > last && time - last <= 0.05 { time - last } else { 1.0 / 60.0 };
        let next = speed * (-Self::DECAY * dt).exp();
        if next.abs() < Self::MIN_SPEED {
            self.fling = None;
        } else {
            self.fling = Some((next, time));
        }
        Some(speed * dt)
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
    #[rust]
    touch: TouchScroll,
    #[rust]
    fling_frame: NextFrame,
    /// The prompt's rect (the input method's anchor).
    #[rust]
    field: Rect,
    /// The prompt's text the input method was last told.
    #[rust]
    ime_text: Option<String>,
    /// The prompt could be typed into at the last frame.
    #[rust]
    usable: bool,
    /// The rect of what the press went down on (a tap acts on it if the
    /// finger lifts within it, give or take a finger's width).
    #[rust]
    down_rect: Option<Rect>,
    /// What the last frame showed, one string per line (for tests and the
    /// hidden-window runs' logs).
    #[rust]
    pub shown: Vec<String>,
    /// Where the person put the desktop pane (None: its column).
    #[rust]
    placed: Option<Rect>,
    /// The surface the pane was last drawn on (a drag stays on it).
    #[rust]
    screen: Rect,
    /// The pane's frame being dragged.
    #[rust]
    drag: Option<PaneDrag>,
    /// The grip under the pointer (its cursor is worn).
    #[rust]
    grip_hover: Option<Grip>,
    /// When the header was last pressed (a second press soon after puts
    /// the pane back).
    #[rust]
    header_pressed: f64,
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
    fn draft_state(self) -> makepad_widgets::makepad_platform::event::FullTextState {
        match self {
            Source::System => super::draft_state(),
            Source::App => crate::app_chat::draft_state(),
        }
    }
    fn text_input(self, event: &makepad_widgets::makepad_platform::event::TextInputEvent) -> bool {
        match self {
            Source::System => super::text_input(event),
            Source::App => crate::app_chat::text_input(event),
        }
    }
    /// The person's own turn runs (Stop instead of Send).
    fn person_running(self, model: &ChatModel) -> bool {
        match self {
            Source::System => model.phase().running_turn().is_some(),
            Source::App => crate::app_chat::person_running(),
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
                format!("Talk to {name}'s agent. The system agent can ask it things too: both show here, each with who spoke, and each side sees the other's recent turns. You can send while the system agent works; Stop stops your own request.")
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
        hit_in(&self.hits, p)
    }

    /// The frame's grip at `p`, if a control is not there first.
    fn grip_at(&self, p: Vec2d) -> Option<Grip> {
        match self.hit_at(p) {
            Some(Hit::Grip(g)) => Some(g),
            _ => None,
        }
    }

    /// The shell's pointer hook: the pane's own rect is its own while open.
    pub fn pointer(&mut self, cx: &mut Cx, event: &Event) -> Outcome {
        let source = self.source();
        if !source.is_open() || self.pane.size.x <= 0.0 {
            self.drag = None;
            return Outcome::Ignored;
        }
        // A drag of the frame has the pointer until the button comes up,
        // wherever it goes.
        if let Some(drag) = self.drag {
            match event {
                Event::MouseMove(e) => {
                    self.placed = Some(dragged(drag.grip, drag.from, e.abs - drag.start, self.screen));
                    cx.set_cursor(drag.grip.cursor());
                    self.redraw(cx);
                    return Outcome::Taken;
                }
                Event::MouseUp(_) => {
                    self.drag = None;
                    return Outcome::Taken;
                }
                Event::MouseDown(_) => return Outcome::Taken,
                _ => {}
            }
        }
        match event {
            Event::Scroll(e) if contains(self.pane, e.abs) => {
                self.touch.cancel_fling();
                source.scroll_by(e.scroll.y, self.max_scroll);
                self.redraw(cx);
                return Outcome::Taken;
            }
            Event::MouseMove(e) => {
                let hover = self.hits.iter().find(|(r, h)| *h != Hit::Pane && !matches!(h, Hit::Grip(_)) && contains(*r, e.abs)).map(|(r, _)| *r);
                if hover != self.hover {
                    self.hover = hover;
                    self.redraw(cx);
                }
                let grip = self.grip_at(e.abs);
                if let Some(g) = grip {
                    cx.set_cursor(g.cursor());
                } else if self.grip_hover.is_some() {
                    cx.set_cursor(MouseCursor::Default);
                }
                self.grip_hover = grip;
                return if contains(self.pane, e.abs) { Outcome::Taken } else { Outcome::Ignored };
            }
            Event::MouseDown(e) => {
                if let Some(grip) = self.grip_at(e.abs) {
                    if grip == Grip::Move && e.time - self.header_pressed < DOUBLE_PRESS_S {
                        // A double-click on the header: back in its column.
                        self.placed = None;
                        self.header_pressed = 0.0;
                        self.redraw(cx);
                        return Outcome::Taken;
                    }
                    if grip == Grip::Move {
                        self.header_pressed = e.time;
                    }
                    self.down = None;
                    self.drag = Some(PaneDrag { grip, start: e.abs, from: self.pane });
                    cx.set_cursor(grip.cursor());
                    return Outcome::Taken;
                }
                return self.press(cx, source, e.abs);
            }
            Event::MouseUp(e) => return self.release(cx, source, e.abs),
            Event::TouchUpdate(e) => {
                use makepad_widgets::makepad_platform::event::TouchState;
                let mut outcome = Outcome::Ignored;
                for t in &e.touches {
                    let this = match t.state {
                        TouchState::Start => {
                            let taken = self.press(cx, source, t.abs);
                            if taken != Outcome::Ignored {
                                self.touch.start(t.uid, t.abs, t.time);
                            }
                            taken
                        }
                        TouchState::Move => match self.touch.moved(t.uid, t.abs, t.time) {
                            Some(dy) => {
                                // A drag presses nothing.
                                self.down = None;
                                source.scroll_by(dy, self.max_scroll);
                                self.redraw(cx);
                                Outcome::Taken
                            }
                            None if contains(self.pane, t.abs) => Outcome::Taken,
                            None => Outcome::Ignored,
                        },
                        TouchState::Stop => {
                            if self.touch.stop(t.uid, t.time) {
                                self.down = None;
                                if self.touch.is_flinging() {
                                    self.fling_frame = cx.new_next_frame();
                                }
                                Outcome::Taken
                            } else {
                                // A tap: what the finger went down on, even
                                // if the layout moved under it before the
                                // lift (the keyboard's suggestion bar, a
                                // keyboard rising or settling).
                                self.tap(cx, source, t.abs)
                            }
                        }
                        _ => Outcome::Ignored,
                    };
                    if outcome == Outcome::Ignored {
                        outcome = this;
                    }
                }
                return outcome;
            }
            _ => {}
        }
        Outcome::Ignored
    }

    /// A press (mouse down, a finger's start).
    fn press(&mut self, cx: &mut Cx, source: Source, p: Vec2d) -> Outcome {
        if !contains(self.pane, p) {
            return Outcome::Ignored;
        }
        self.touch.cancel_fling();
        self.down = self.hit_at(p);
        self.down_rect = self.down.as_ref().and_then(|d| self.hits.iter().find(|(_, h)| h == d).map(|(r, _)| *r));
        // A press in a pane gives it the keyboard.
        crate::app_chat::focus(source == Source::App);
        if self.down == Some(Hit::Field) {
            self.take_keyboard(cx);
        }
        Outcome::Taken
    }

    /// A finger lifted without dragging: what it went down on acts, if
    /// the lift is still on it (give or take a finger's width; an answer
    /// option must be hit exactly). Sliding off cancels.
    fn tap(&mut self, cx: &mut Cx, source: Source, p: Vec2d) -> Outcome {
        let (Some(hit), Some(r)) = (self.down.take(), self.down_rect.take()) else {
            return if contains(self.pane, p) { Outcome::Taken } else { Outcome::Ignored };
        };
        if !lifted_on(&hit, r, p) {
            return if contains(self.pane, p) { Outcome::Taken } else { Outcome::Ignored };
        }
        let outcome = act(source, hit);
        self.redraw(cx);
        outcome
    }

    /// A release (mouse up).
    fn release(&mut self, cx: &mut Cx, source: Source, p: Vec2d) -> Outcome {
        let hit = self.hit_at(p);
        let pressed = self.down.take();
        if hit.is_none() || hit != pressed {
            return if contains(self.pane, p) { Outcome::Taken } else { Outcome::Ignored };
        }
        let outcome = act(source, hit.unwrap());
        self.redraw(cx);
        outcome
    }

    /// The prompt takes the key focus and the input method, as makepad's
    /// `TextInput` does on a press: the new focus also clears a dismissal,
    /// so a keyboard the person put away comes back on this tap.
    fn take_keyboard(&mut self, cx: &mut Cx) {
        if self.area.is_empty() {
            return;
        }
        cx.set_key_focus(self.area);
        self.ime_text = None;
        self.show_ime(cx);
    }

    fn show_ime(&mut self, cx: &mut Cx) {
        use makepad_widgets::makepad_platform::ime::{ReturnKeyType, SoftKeyboardConfig, TextInputConfig};
        let config = TextInputConfig {
            soft_keyboard: SoftKeyboardConfig { return_key_type: ReturnKeyType::Send, ..SoftKeyboardConfig::default() },
            submit_on_enter: true,
            // Android drops a single-line field's Enter key (hardware, or a
            // keyboard that sends the key instead of its action): as
            // multi-line, Enter commits a line break, which the prompt
            // turns into Send (`Composer::take_submit`).
            is_multiline: true,
            ..TextInputConfig::default()
        };
        cx.show_text_ime_with_config(self.area, self.field, config);
    }

    /// This is the "Ask <app>" panel.
    pub fn is_app_panel(&self) -> bool {
        self.app_panel
    }

    /// The prompt holds the key focus.
    pub fn has_keyboard(&self, cx: &Cx) -> bool {
        !self.area.is_empty() && self.source().is_open() && cx.has_key_focus(self.area)
    }

    /// Text input and the input method's state query, while the prompt
    /// holds the key focus. True when the event was the pane's.
    pub fn ime(&mut self, cx: &mut Cx, event: &Event) -> bool {
        if !self.has_keyboard(cx) {
            return false;
        }
        match event {
            Event::TextInputStateQuery(response) => {
                *response.borrow_mut() = Some((self.uid.0, self.source().draft_state()));
                true
            }
            Event::TextInput(t) => {
                let taken = self.source().text_input(t);
                // What the input method sent is what it holds already.
                self.ime_text = Some(self.source().draft());
                self.redraw(cx);
                taken
            }
            _ => false,
        }
    }

    /// Give up the key focus (the pane closed).
    pub fn release_keyboard(&mut self, cx: &mut Cx) {
        if !self.area.is_empty() && cx.has_key_focus(self.area) {
            cx.set_key_focus(Area::Empty);
            cx.hide_text_ime();
        }
        self.ime_text = None;
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
        self.screen = screen;
        let pane = if full {
            screen
        } else if let Some(placed) = self.placed {
            // Where the person put it, kept on screen.
            let kept = placed_pane(placed, screen);
            self.placed = Some(kept);
            kept
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
        let usable = source.usable(&model);
        self.usable = usable;
        let (label, hit, enabled) = composer_button(source.person_running(&model), usable, &draft);
        {
            let mut b = Buttons { d: &mut self.d, tok, hover };
            let bw = b.width(cx, label);
            let button = b.draw(cx, x + cw - bw, field_y + (FIELD_H - 28.0) * 0.5, bw, label, enabled);
            hits.push((button, hit));
            shown.push(format!("button: {label}"));
            let field = rect(x, field_y, cw - bw - 8.0, FIELD_H);
            self.field = field;
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

        // "Ask <app>": the system agent's running turn, on its own row with
        // its own Stop (never in the Send button's place).
        if source == Source::App && crate::app_chat::system_agent_running() {
            let mut b = Buttons { d: &mut self.d, tok, hover };
            let row_y = list_bottom - 30.0;
            let w = b.width(cx, STOP_SYSTEM_AGENT).min(cw);
            let r = b.draw(cx, x + cw - w, row_y, w, STOP_SYSTEM_AGENT, false);
            hits.push((r, Hit::StopSystemAgent));
            let note = "The system agent is working here\u{2026}";
            b.d.label_elided(cx, rect(x, row_y + 5.0, (cw - w - 8.0).max(0.0), 18.0), false, tok.font.body_small, dim, HAlign::Left, note);
            shown.push(format!("{note} [{STOP_SYSTEM_AGENT}]"));
            list_bottom = row_y - 8.0;
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
        // The frame last: the controls win over it.
        if !full {
            self.hits.extend(grips(pane));
        }
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
            Hit::StopSystemAgent => crate::app_chat::stop_system_agent(),
            Hit::Option { question, count, label } => crate::app_chat::answer_option(&question, count, &label),
            Hit::New | Hit::OpenProviders | Hit::Field | Hit::Pane | Hit::Grip(_) => {}
        }
        return Outcome::Taken;
    }
    match hit {
        Hit::Close => super::close(),
        Hit::New => super::new_conversation(),
        Hit::Send => super::send_draft(),
        Hit::Stop => super::interrupt(),
        Hit::StopSystemAgent => {}
        Hit::Option { question, count, label } => super::answer_option(&question, count, &label),
        Hit::OpenProviders => return Outcome::OpenProviders,
        Hit::Field | Hit::Pane | Hit::Grip(_) => {}
    }
    Outcome::Taken
}

impl Widget for ShellSystemChat {
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let screen = visible(cx.turtle().rect(), cx.current_pass_size());
        self.d.begin_surface(cx);
        self.draw_pane(cx, screen);
        self.d.end_surface(cx);
        cx.end_turtle_with_area(&mut self.area);
        // While the prompt holds the key focus, keep the input method up
        // (the platform dedups the request, and ignores it after the
        // person dismissed the keyboard until the next press) and tell it
        // about text it did not type (Backspace, Send's clearing).
        if self.has_keyboard(cx) {
            if self.usable {
                self.show_ime(cx);
                let state = self.source().draft_state();
                if self.ime_text.as_deref() != Some(state.text.as_str()) {
                    self.ime_text = Some(state.text.clone());
                    cx.sync_ime_state(state.text, state.selection, state.composition);
                }
            }
        } else if !self.source().is_open() {
            // Closed with the keyboard: it goes with the pane.
            self.release_keyboard(cx);
        }
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        // A fling goes on frame by frame.
        if let Some(ne) = self.fling_frame.is_event(event) {
            if let Some(dy) = self.touch.fling_step(ne.time) {
                self.source().scroll_by(dy, self.max_scroll);
                self.redraw(cx);
                if self.touch.is_flinging() {
                    self.fling_frame = cx.new_next_frame();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "Ask <app>" while the system agent's turn runs: the person's lane is
    /// idle, so the button is Send (enabled once there is text), never Stop.
    #[test]
    fn send_shows_whenever_the_persons_lane_is_idle() {
        assert_eq!(composer_button(false, true, ""), ("Send", Hit::Send, false));
        assert_eq!(composer_button(false, true, "why?"), ("Send", Hit::Send, true));
        assert_eq!(composer_button(false, false, "why?"), ("Send", Hit::Send, false), "not before the conversation is ready");
        assert_eq!(composer_button(true, true, "why?"), ("Stop", Hit::Stop, true), "the person's own turn");
    }

    /// The device: Send tapped while the soft keyboard was up missed. A
    /// finger slightly off a button still hits it; the prompt and answer
    /// options get no slop; a lift off the target cancels.
    #[test]
    fn a_tap_near_a_button_hits_it_and_sliding_off_cancels() {
        let pane = rect(0.0, 0.0, 400.0, 800.0);
        let field = rect(16.0, 700.0, 300.0, 36.0);
        let send = rect(324.0, 704.0, 60.0, 28.0);
        let option = Hit::Option { question: "q".into(), count: 1, label: "Yes".into() };
        let yes = rect(16.0, 650.0, 60.0, 28.0);
        let hits = vec![(pane, Hit::Pane), (send, Hit::Send), (field, Hit::Field), (yes, option.clone())];
        assert_eq!(hit_in(&hits, dvec2(350.0, 715.0)), Some(Hit::Send));
        assert_eq!(hit_in(&hits, dvec2(350.0, 738.0)), Some(Hit::Send), "a finger's width below it");
        assert_eq!(hit_in(&hits, dvec2(310.0, 715.0)), Some(Hit::Field), "the prompt itself first");
        assert_eq!(hit_in(&hits, dvec2(200.0, 300.0)), Some(Hit::Pane));
        assert_eq!(hit_in(&hits, dvec2(40.0, 664.0)), Some(option.clone()));
        assert_eq!(hit_in(&hits, dvec2(40.0, 682.0)), Some(Hit::Pane), "no slop for an answer");
        // The lift.
        assert!(lifted_on(&Hit::Send, send, dvec2(350.0, 739.0)), "within a finger's width");
        assert!(!lifted_on(&Hit::Send, send, dvec2(350.0, 790.0)), "slid off: cancelled");
        assert!(lifted_on(&option, yes, dvec2(40.0, 664.0)));
        assert!(!lifted_on(&option, yes, dvec2(40.0, 682.0)), "an answer is exact");
    }

    /// The desktop pane's frame: the header's buttons win over the header
    /// they sit in, an edge wins over the header it crosses, and the inside
    /// is still the pane's.
    #[test]
    fn the_frame_moves_and_sizes_the_pane_but_never_takes_a_button() {
        let pane = rect(100.0, 50.0, 440.0, 700.0);
        let close = rect(470.0, 66.0, 54.0, 28.0);
        let mut hits = vec![(pane, Hit::Pane), (close, Hit::Close)];
        hits.extend(grips(pane));
        assert_eq!(hit_in(&hits, dvec2(490.0, 80.0)), Some(Hit::Close), "a button in the header is a button");
        assert_eq!(hit_in(&hits, dvec2(250.0, 80.0)), Some(Hit::Grip(Grip::Move)), "the header moves it");
        assert_eq!(hit_in(&hits, dvec2(250.0, 52.0)), Some(Hit::Grip(Grip::N)), "its top edge sizes it");
        assert_eq!(hit_in(&hits, dvec2(102.0, 400.0)), Some(Hit::Grip(Grip::W)));
        assert_eq!(hit_in(&hits, dvec2(537.0, 747.0)), Some(Hit::Grip(Grip::SE)));
        assert_eq!(hit_in(&hits, dvec2(300.0, 400.0)), Some(Hit::Pane), "the transcript stays the pane's");
    }

    /// Moving keeps the pane on screen; sizing moves only the dragged edges,
    /// never below the minimum; a placement shrinks with the window.
    #[test]
    fn a_dragged_pane_stays_on_screen_and_above_its_minimum() {
        let screen = rect(0.0, 30.0, 1400.0, 860.0);
        let pane = rect(944.0, 40.0, 440.0, 840.0);
        assert_eq!(dragged(Grip::Move, pane, dvec2(-300.0, 0.0), screen), rect(644.0, 40.0, 440.0, 840.0));
        assert_eq!(dragged(Grip::Move, pane, dvec2(500.0, -100.0), screen), rect(960.0, 30.0, 440.0, 840.0), "never off screen");
        assert_eq!(dragged(Grip::W, pane, dvec2(-200.0, 0.0), screen), rect(744.0, 40.0, 640.0, 840.0), "the left edge only");
        assert_eq!(dragged(Grip::W, pane, dvec2(400.0, 0.0), screen), rect(1064.0, 40.0, MIN_W, 840.0), "no narrower than the minimum");
        assert_eq!(dragged(Grip::SE, pane, dvec2(-100.0, -500.0), screen), rect(944.0, 40.0, 340.0, 340.0));
        assert_eq!(dragged(Grip::N, pane, dvec2(0.0, 700.0), screen), rect(944.0, 640.0, 440.0, MIN_H));
        let small = rect(0.0, 0.0, 800.0, 500.0);
        assert_eq!(placed_pane(rect(600.0, 300.0, 440.0, 840.0), small), rect(360.0, 0.0, 440.0, 500.0), "a window that shrank keeps it whole");
        let tiny = rect(0.0, 0.0, 200.0, 100.0);
        assert_eq!(placed_pane(pane, tiny), tiny, "a screen smaller than the minimum: the whole screen");
        // A surface under a 33 px bar, as tall as the window: cut to it.
        assert_eq!(visible(rect(0.0, 33.0, 1400.0, 894.0), dvec2(1400.0, 894.0)), rect(0.0, 33.0, 1400.0, 861.0));
        assert_eq!(visible(rect(0.0, 33.0, 1400.0, 894.0), dvec2(0.0, 0.0)), rect(0.0, 33.0, 1400.0, 894.0), "no window size known");
    }

    #[test]
    fn a_touch_drag_scrolls_and_presses_nothing() {
        let mut t = TouchScroll::default();
        t.start(1, dvec2(100.0, 400.0), 0.0);
        assert_eq!(t.moved(1, dvec2(101.0, 404.0), 0.01), None, "within the slop: still a press");
        assert_eq!(t.moved(2, dvec2(100.0, 500.0), 0.02), None, "another finger");
        assert_eq!(t.moved(1, dvec2(100.0, 420.0), 0.02), Some(0.0), "the drag starts where the slop ended");
        assert_eq!(t.moved(1, dvec2(100.0, 450.0), 0.03), Some(30.0), "a finger moving down scrolls back");
        assert_eq!(t.moved(1, dvec2(100.0, 440.0), 0.04), Some(-10.0));
        assert!(t.is_dragging());
        assert!(t.stop(1, 0.2), "a drag's lift presses nothing");
        assert!(!t.is_flinging(), "a finger that rested does not fling");
        // A tap is a press.
        t.start(1, dvec2(100.0, 400.0), 1.0);
        assert!(!t.stop(1, 1.05));
    }

    #[test]
    fn a_quick_lift_flings_and_slows_down() {
        let mut t = TouchScroll::default();
        t.start(7, dvec2(0.0, 400.0), 0.0);
        let mut y = 400.0;
        let mut time = 0.0;
        for _ in 0..8 {
            y -= 20.0;
            time += 0.016;
            t.moved(7, dvec2(0.0, y), time);
        }
        assert!(t.stop(7, time + 0.01));
        assert!(t.is_flinging());
        let first = t.fling_step(time + 0.026).unwrap();
        assert!(first < 0.0, "on toward the newest lines: {first}");
        let mut last = first;
        let mut frames = 0;
        while let Some(dy) = t.fling_step(time + 0.026 + frames as f64 * 0.016) {
            assert!(dy.abs() <= last.abs() + 1e-9, "slows down");
            last = dy;
            frames += 1;
            assert!(frames < 400, "a fling ends");
        }
        assert!(!t.is_flinging());
        // A press stops a fling.
        t.start(8, dvec2(0.0, 0.0), 10.0);
        assert!(!t.is_flinging());
    }

    /// A frame clock that is not the touch clock still ends the fling.
    #[test]
    fn a_fling_ends_whatever_the_frame_clock() {
        let mut t = TouchScroll::default();
        t.start(1, dvec2(0.0, 0.0), 100.0);
        t.moved(1, dvec2(0.0, 20.0), 100.01);
        t.moved(1, dvec2(0.0, 60.0), 100.02);
        assert!(t.stop(1, 100.03));
        let mut frames = 0;
        while t.fling_step(0.0).is_some() {
            frames += 1;
            assert!(frames < 1000);
        }
    }
}
