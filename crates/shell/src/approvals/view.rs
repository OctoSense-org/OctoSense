//! The shell-drawn approval surface (ADR 0004 §8, §4): drawn by the shell,
//! over everything, in the shell's own chrome kit (`shell/ui.rs`, the
//! pattern of the menu, the notifications and the developer banner).
//!
//! - **The approval sheet**: the router's front [`Sheet`]. Each open line
//!   shows the owning app and tool, who is calling, the exact arguments
//!   (secrets redacted) and why it needs the person, with Approve once,
//!   Deny and up to two "Always …" choices. Modal while up.
//! - **The first-use sheet** ([`super::consent`]): what the agent may read
//!   and use and where the model runs; Allow or Don't allow. Modal.
//! - **The time-box indicator**: while a rule approves everything one app
//!   asks, a pill at the top says so, with the minutes left and Stop.
//! - **An app agent's question** ([`crate::questions`], the app's
//!   conversation): who asks, the question and its options; a tap answers
//!   it (the person's answer, [`crate::questions::PersonAnswer`]). Modal,
//!   after any approval sheet.
//!
//! A press on a button is the person's gesture: only here (and on the
//! Settings page) is a [`ApprovalGesture`] made.

use makepad_widgets::*;

use super::consent::AgentSummary;
use super::rules::{ApprovalGesture, Rule};
use super::sheet::{app_label, Answer, Line, Sheet};
use super::types::{RequestId, RuleId};
use crate::shell::ui::{contains, rect, DrawShellFill, HAlign, ShellDraw};
use crate::shell::{alpha, rgb, CtrlState, ShellTokens};

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.ShellApprovalsBase = #(ShellApprovals::register_widget(vm))
    mod.widgets.ShellApprovals = set_type_default() do mod.widgets.ShellApprovalsBase {
        width: Fill
        height: Fill
        draw_bg +: {}
        d +: {}
    }
}

const CARD_MAX_W: f64 = 600.0;
const PAD: f64 = 18.0;
const BUTTON_H: f64 = 28.0;
const ARG_LINE_H: f64 = 17.0;
const MAX_ARG_LINES: usize = 12;
const INDICATOR_H: f64 = 30.0;

/// What a press lands on.
#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Answer { sheet: u64, request: RequestId, answer: Answer },
    Consent { app: String, allow: bool },
    StopRule(RuleId),
    /// An option of an app agent's question (`None`: "Don't answer").
    QuestionOption { id: u64, label: Option<String> },
    /// The card itself (swallowed).
    Card,
}

/// A small button kit shared with the Settings page.
pub(crate) struct Buttons<'a> {
    pub d: &'a mut ShellDraw,
    pub tok: ShellTokens,
    pub hover: Option<Rect>,
}

impl Buttons<'_> {
    pub fn width(&mut self, cx: &mut Cx2d, label: &str) -> f64 {
        self.d.measure(cx, false, self.tok.font.body, label) + 26.0
    }
    /// A button; `primary` is the accent face. Returns its rect.
    pub fn draw(&mut self, cx: &mut Cx2d, x: f64, y: f64, max_w: f64, label: &str, primary: bool) -> Rect {
        let w = self.width(cx, label).min(max_w.max(40.0));
        let r = rect(x, y, w, BUTTON_H);
        let hovered = self.hover.is_some_and(|h| h == r);
        let ink = self.tok.popups.text;
        if primary {
            let accent = self.tok.notifications.countdown;
            let fill = if hovered { alpha(accent, 0.85) } else { accent };
            self.d.bordered(cx, r, fill, fill, fill, 0.0, 0.0);
            self.d.label_elided(cx, rect(r.pos.x + 10.0, r.pos.y, r.size.x - 20.0, r.size.y), true, self.tok.font.body, contrast(accent), HAlign::Center, label);
        } else {
            let state = if hovered { CtrlState::Hover } else { CtrlState::Normal };
            self.d.control(cx, r, &self.tok.controls, state);
            self.d.label_elided(cx, rect(r.pos.x + 10.0, r.pos.y, r.size.x - 20.0, r.size.y), false, self.tok.font.body, ink, HAlign::Center, label);
        }
        r
    }
}

/// Black or white, whichever reads on `c`.
pub(crate) fn contrast(c: Vec4f) -> Vec4f {
    let l = 0.299 * c.x + 0.587 * c.y + 0.114 * c.z;
    if l > 0.6 {
        rgb(0x10, 0x10, 0x10)
    } else {
        rgb(0xFF, 0xFF, 0xFF)
    }
}

/// What the view draws, copied out of the shared state for one frame.
#[derive(Clone, Debug, Default)]
struct Frame {
    sheet: Option<Sheet>,
    more_sheets: usize,
    consent: Option<AgentSummary>,
    everything: Vec<Rule>,
    /// The oldest open question of an app's conversation.
    question: Option<crate::questions::Request>,
    now: u64,
}

fn frame() -> Frame {
    super::with(|a| {
        let now = super::now();
        Frame {
            sheet: a.router.front_sheet().cloned(),
            more_sheets: a.router.sheets().len().saturating_sub(1),
            consent: a.consent.prompt().cloned(),
            everything: a.router.rules.active_everything(now).into_iter().cloned().collect(),
            question: None,
            now,
        }
    })
    .map(|mut f| {
        f.question = crate::questions::open_in_apps().into_iter().next();
        f
    })
    .unwrap_or_default()
}

#[derive(Script, ScriptHook, Widget)]
pub struct ShellApprovals {
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
    #[rust]
    area: Area,
    #[rust]
    hits: Vec<(Rect, Hit)>,
    /// A modal card is up: every press is ours.
    #[rust]
    modal: bool,
    #[rust]
    down: Option<Hit>,
    #[rust]
    hover: Option<Rect>,
    /// What was last drawn, for the control surface and tests.
    #[rust]
    pub shown: Vec<String>,
}

impl ShellApprovals {
    fn hit_at(&self, p: Vec2d) -> Option<Hit> {
        // Buttons before the card they sit on.
        self.hits.iter().rev().find(|(r, h)| *h != Hit::Card && contains(*r, p)).or_else(|| self.hits.iter().find(|(r, _)| contains(*r, p))).map(|(_, h)| h.clone())
    }

    /// The shell's pointer hook (`approvals::pointer`). True when taken.
    pub fn pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let (down, up, moved) = match event {
            Event::MouseDown(e) => (Some(e.abs), None, None),
            Event::MouseUp(e) => (None, Some(e.abs), None),
            Event::MouseMove(e) => (None, None, Some(e.abs)),
            Event::TouchUpdate(e) => (
                e.touches.iter().find(|t| t.state == makepad_widgets::makepad_platform::event::TouchState::Start).map(|t| t.abs),
                e.touches.iter().find(|t| t.state == makepad_widgets::makepad_platform::event::TouchState::Stop).map(|t| t.abs),
                None,
            ),
            _ => (None, None, None),
        };
        if let Some(p) = moved {
            let hover = self.hits.iter().find(|(r, h)| *h != Hit::Card && contains(*r, p)).map(|(r, _)| *r);
            if hover != self.hover {
                self.hover = hover;
                self.redraw(cx);
            }
            return self.modal;
        }
        if let Some(p) = down {
            self.down = self.hit_at(p);
            return self.modal || self.down.is_some();
        }
        if let Some(p) = up {
            let hit = self.hit_at(p);
            let pressed = self.down.take();
            if let (Some(h), Some(d)) = (&hit, &pressed) {
                if h == d {
                    act(h.clone());
                    self.redraw(cx);
                }
            }
            return self.modal || hit.is_some();
        }
        self.modal
    }

    fn draw_all(&mut self, cx: &mut Cx2d, screen: Rect) {
        self.hits.clear();
        self.shown.clear();
        self.modal = false;
        let f = frame();
        let tok = self.d.tokens(self.tokens);
        self.draw_indicator(cx, screen, &f, tok);
        if let Some(summary) = &f.consent {
            self.modal = true;
            self.draw_consent(cx, screen, summary, tok);
        } else if let Some(sheet) = &f.sheet {
            self.modal = true;
            self.draw_sheet(cx, screen, sheet, f.more_sheets, tok);
        } else if let Some(question) = &f.question {
            self.modal = true;
            self.draw_question(cx, screen, question, tok);
        }
    }

    /// An app agent's question, in the app's conversation: who asks, the
    /// question, one button per option and "Don't answer".
    fn draw_question(&mut self, cx: &mut Cx2d, screen: Rect, q: &crate::questions::Request, tok: ShellTokens) {
        self.scrim(cx, screen);
        let options = q.options();
        let card = Self::card_rect(screen, PAD * 2.0 + 26.0 + 20.0 + 12.0 + 40.0 + 12.0 + (options.len() as f64 + 1.0) * (BUTTON_H + 8.0));
        self.d.card(cx, card, &tok.popups);
        self.hits.push((card, Hit::Card));
        let ink = tok.popups.text;
        let dim = alpha(ink, 0.65);
        let x = card.pos.x + PAD;
        let w = card.size.x - PAD * 2.0;
        let mut y = card.pos.y + PAD;
        let title = if q.title.is_empty() { q.asked_by() } else { q.title.clone() };
        self.d.label_elided(cx, rect(x, y, w, 24.0), true, tok.font.heading, ink, HAlign::Left, &title);
        y += 26.0;
        let sub = format!("{} \u{00b7} in {}'s conversation", q.asked_by(), app_label(&q.app));
        self.d.label_elided(cx, rect(x, y, w, 18.0), false, tok.font.body_small, dim, HAlign::Left, &sub);
        y += 20.0 + 12.0;
        let text = q.text().to_string();
        self.d.label_elided(cx, rect(x, y, w, 36.0), false, tok.font.body, ink, HAlign::Left, &text);
        y += 40.0 + 12.0;
        let mut buttons = Buttons { d: &mut self.d, tok, hover: self.hover };
        let mut hits = Vec::new();
        for (i, label) in options.iter().enumerate() {
            let r = buttons.draw(cx, x, y, w, label, i == 0);
            hits.push((r, Hit::QuestionOption { id: q.id, label: Some(label.clone()) }));
            y += BUTTON_H + 8.0;
        }
        let r = buttons.draw(cx, x, y, w, "Don't answer", false);
        hits.push((r, Hit::QuestionOption { id: q.id, label: None }));
        self.hits.extend(hits);
        self.shown.push(title);
        self.shown.push(sub);
        self.shown.push(text);
        self.shown.extend(options);
    }

    fn draw_indicator(&mut self, cx: &mut Cx2d, screen: Rect, f: &Frame, tok: ShellTokens) {
        let mut y = screen.pos.y + 36.0;
        for rule in &f.everything {
            let left = rule.minutes_left(f.now).unwrap_or(0);
            let text = format!("Approving everything {} asks \u{00b7} {left} min left", app_label(&rule.app));
            let px = tok.font.body;
            let w = (self.d.measure(cx, true, px, &text) + 110.0).min(screen.size.x - 24.0);
            let pill = rect(screen.pos.x + (screen.size.x - w) * 0.5, y, w, INDICATOR_H);
            let ground = rgb(0xE0, 0x8A, 0x00);
            let ink = rgb(0x1A, 0x12, 0x00);
            self.d.bordered(cx, pill, ground, ground, ground, 0.0, 0.0);
            let stop = rect(pill.pos.x + pill.size.x - 64.0, pill.pos.y + 4.0, 58.0, INDICATOR_H - 8.0);
            self.d.solid(cx, stop, ink);
            self.d.label(cx, stop, true, px, ground, HAlign::Center, "Stop");
            self.d.label_elided(cx, rect(pill.pos.x + 12.0, pill.pos.y, stop.pos.x - pill.pos.x - 20.0, INDICATOR_H), true, px, ink, HAlign::Left, &text);
            self.hits.push((stop, Hit::StopRule(rule.id.clone())));
            self.hits.push((pill, Hit::Card));
            self.shown.push(text);
            y += INDICATOR_H + 6.0;
        }
    }

    fn card_rect(screen: Rect, h: f64) -> Rect {
        let w = CARD_MAX_W.min(screen.size.x - 32.0).max(200.0);
        let h = h.min(screen.size.y - 48.0);
        rect(screen.pos.x + (screen.size.x - w) * 0.5, screen.pos.y + ((screen.size.y - h) * 0.5).max(24.0), w, h)
    }

    fn scrim(&mut self, cx: &mut Cx2d, screen: Rect) {
        self.d.solid(cx, screen, Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 0.45 });
    }

    fn line_height(line: &Line) -> f64 {
        let args = line.args.len().min(MAX_ARG_LINES) as f64 * ARG_LINE_H + if line.args.len() > MAX_ARG_LINES { ARG_LINE_H } else { 0.0 };
        let note = if line.surfaced.note().is_empty() { 0.0 } else { 18.0 };
        let always = if line.always.is_empty() { 0.0 } else { BUTTON_H + 8.0 };
        22.0 + 18.0 + 8.0 + args + 12.0 + note + BUTTON_H + 8.0 + always + 14.0
    }

    fn draw_sheet(&mut self, cx: &mut Cx2d, screen: Rect, sheet: &Sheet, more: usize, tok: ShellTokens) {
        self.scrim(cx, screen);
        let lines: Vec<&Line> = sheet.open_lines().collect();
        let header = 26.0 + 20.0 + 14.0;
        let body: f64 = lines.iter().map(|l| Self::line_height(l)).sum();
        let footer = if more > 0 { 20.0 } else { 0.0 };
        let card = Self::card_rect(screen, PAD * 2.0 + header + body + footer);
        self.d.card(cx, card, &tok.popups);
        self.hits.push((card, Hit::Card));
        let ink = tok.popups.text;
        let dim = alpha(ink, 0.65);
        let x = card.pos.x + PAD;
        let w = card.size.x - PAD * 2.0;
        let bottom = card.pos.y + card.size.y - PAD;
        let mut y = card.pos.y + PAD;
        let title = sheet.title();
        self.d.label_elided(cx, rect(x, y, w, 24.0), true, tok.font.heading, ink, HAlign::Left, &title);
        y += 26.0;
        let sub = sheet.subtitle();
        self.d.label_elided(cx, rect(x, y, w, 18.0), false, tok.font.body_small, dim, HAlign::Left, &sub);
        y += 20.0 + 14.0;
        self.shown.push(title);
        self.shown.push(sub);
        let mut buttons = Buttons { d: &mut self.d, tok, hover: self.hover };
        let mut hits = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let h = Self::line_height(line);
            if y + h > bottom + 1.0 {
                let rest = lines.len() - i;
                buttons.d.label_elided(cx, rect(x, y, w, 18.0), false, tok.font.body_small, dim, HAlign::Left, &format!("{rest} more below: answer these first"));
                break;
            }
            if i > 0 {
                buttons.d.solid(cx, rect(x, y - 8.0, w, 1.0), alpha(ink, 0.12));
            }
            let heading = line.heading();
            buttons.d.label_elided(cx, rect(x, y, w, 20.0), true, tok.font.subtitle, ink, HAlign::Left, &heading);
            y += 22.0;
            let asked = format!("Asked by {}", line.caller);
            buttons.d.label_elided(cx, rect(x, y, w, 16.0), false, tok.font.body_small, dim, HAlign::Left, &asked);
            y += 18.0 + 8.0;
            let shown_args = line.args.len().min(MAX_ARG_LINES);
            let extra = if line.args.len() > MAX_ARG_LINES { 1 } else { 0 };
            let box_h = (shown_args + extra) as f64 * ARG_LINE_H + 8.0;
            buttons.d.solid(cx, rect(x, y - 4.0, w, box_h), alpha(ink, 0.06));
            for a in line.args.iter().take(MAX_ARG_LINES) {
                buttons.d.label_elided(cx, rect(x + 8.0, y, w - 16.0, ARG_LINE_H), false, tok.font.body_small, ink, HAlign::Left, a);
                y += ARG_LINE_H;
            }
            if extra == 1 {
                buttons.d.label_elided(cx, rect(x + 8.0, y, w - 16.0, ARG_LINE_H), false, tok.font.body_small, dim, HAlign::Left, &format!("\u{2026} {} more lines", line.args.len() - MAX_ARG_LINES));
                y += ARG_LINE_H;
            }
            y += 12.0;
            let note = line.surfaced.note();
            if !note.is_empty() {
                buttons.d.label_elided(cx, rect(x, y, w, 16.0), false, tok.font.body_small, rgb(0xE0, 0x8A, 0x00), HAlign::Left, note);
                y += 18.0;
            }
            let mut bx = x;
            let once = buttons.draw(cx, bx, y, w, "Approve once", true);
            hits.push((once, Hit::Answer { sheet: sheet.id, request: line.request.clone(), answer: Answer::Once }));
            bx += once.size.x + 8.0;
            let deny = buttons.draw(cx, bx, y, x + w - bx, "Deny", false);
            hits.push((deny, Hit::Answer { sheet: sheet.id, request: line.request.clone(), answer: Answer::Deny }));
            y += BUTTON_H + 8.0;
            if !line.always.is_empty() {
                let mut bx = x;
                for (k, choice) in line.always.iter().enumerate().take(2) {
                    let room = x + w - bx;
                    if room < 80.0 {
                        break;
                    }
                    let r = buttons.draw(cx, bx, y, if k == 0 { room * 0.6 } else { room }, &choice.label, false);
                    hits.push((r, Hit::Answer { sheet: sheet.id, request: line.request.clone(), answer: Answer::Always(k) }));
                    bx += r.size.x + 8.0;
                }
                y += BUTTON_H + 8.0;
            }
            y += 14.0;
            self.shown.push(heading);
            self.shown.push(asked);
            self.shown.extend(line.args.iter().cloned());
        }
        if more > 0 {
            buttons.d.label_elided(cx, rect(x, bottom - 18.0, w, 18.0), false, tok.font.body_small, dim, HAlign::Left, &format!("{more} more sheet{} after this one", if more == 1 { "" } else { "s" }));
        }
        self.hits.extend(hits);
    }

    fn draw_consent(&mut self, cx: &mut Cx2d, screen: Rect, s: &AgentSummary, tok: ShellTokens) {
        self.scrim(cx, screen);
        let rows = 2 + s.reads.len() + 1 + s.uses.len() + 1 + 1;
        let card = Self::card_rect(screen, PAD * 2.0 + 26.0 + 20.0 + 12.0 + rows as f64 * 20.0 + 12.0 + BUTTON_H);
        self.d.card(cx, card, &tok.popups);
        self.hits.push((card, Hit::Card));
        let ink = tok.popups.text;
        let dim = alpha(ink, 0.65);
        let x = card.pos.x + PAD;
        let w = card.size.x - PAD * 2.0;
        let mut y = card.pos.y + PAD;
        let title = format!("Let {}'s agent start?", s.name);
        self.d.label_elided(cx, rect(x, y, w, 24.0), true, tok.font.heading, ink, HAlign::Left, &title);
        y += 26.0;
        self.d.label_elided(cx, rect(x, y, w, 18.0), false, tok.font.body_small, dim, HAlign::Left, "The first time an app asks for its agent, you decide. You can change it in Settings.");
        y += 20.0 + 12.0;
        let row = |d: &mut ShellDraw, cx: &mut Cx2d, y: &mut f64, bold: bool, text: &str| {
            d.label_elided(cx, rect(x, *y, w, 18.0), bold, tok.font.body, if bold { ink } else { alpha(ink, 0.85) }, HAlign::Left, text);
            *y += 20.0;
        };
        row(&mut self.d, cx, &mut y, true, "It may read");
        for r in &s.reads {
            row(&mut self.d, cx, &mut y, false, &format!("\u{2022} {r}"));
        }
        row(&mut self.d, cx, &mut y, true, "It may use");
        for u in &s.uses {
            row(&mut self.d, cx, &mut y, false, &format!("\u{2022} {u}"));
        }
        row(&mut self.d, cx, &mut y, true, "Where the model runs");
        row(&mut self.d, cx, &mut y, false, &s.model);
        y += 12.0;
        let mut buttons = Buttons { d: &mut self.d, tok, hover: self.hover };
        let allow = buttons.draw(cx, x, y, w, "Allow", true);
        let deny = buttons.draw(cx, x + allow.size.x + 8.0, y, w, "Don't allow", false);
        self.hits.push((allow, Hit::Consent { app: s.app.clone(), allow: true }));
        self.hits.push((deny, Hit::Consent { app: s.app.clone(), allow: false }));
        self.shown.push(title);
        self.shown.extend(s.reads.iter().cloned());
        self.shown.extend(s.uses.iter().cloned());
        self.shown.push(s.model.clone());
    }
}

/// What a press does. Each answer here is the person's.
fn act(hit: Hit) {
    let now = super::now();
    match hit {
        Hit::Answer { sheet, request, answer } => {
            let r = super::with(|a| a.router.answer(sheet, &request, answer, &ApprovalGesture::sheet_tap(), now));
            if let Some(Err(e)) = r {
                log!("approvals: {e}");
            }
        }
        Hit::Consent { app, allow } => {
            super::with(|a| a.consent.set(&ApprovalGesture::sheet_tap(), &app, allow, now));
        }
        Hit::StopRule(id) => {
            super::with(|a| a.router.delete_rule(&id));
        }
        Hit::QuestionOption { id, label } => {
            use crate::ai_host::app_peers::host_tools::QuestionReply;
            let reply = match label {
                Some(label) => QuestionReply::option(label),
                None => QuestionReply::text("The person chose not to answer."),
            };
            if let Err(e) = crate::questions::answer(id, &[reply], &crate::questions::PersonAnswer::from_shell_surface()) {
                log!("questions: {e}");
            }
        }
        Hit::Card => {}
    }
}

impl Widget for ShellApprovals {
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let screen = cx.turtle().rect();
        self.d.begin_surface(cx);
        self.draw_all(cx, screen);
        self.d.end_surface(cx);
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    // The shell routes presses (`approvals::pointer`); nothing here.
    fn handle_event(&mut self, _cx: &mut Cx, _event: &Event, _scope: &mut Scope) {}
}
