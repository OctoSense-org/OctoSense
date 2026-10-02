//! The shell-drawn approval surface (ADR 0004 §8, §4): drawn by the shell,
//! over everything, in the shell's own chrome kit (`shell/ui.rs`, the
//! pattern of the menu, the notifications and the developer banner).
//!
//! - **The approval sheet**: the router's front [`Sheet`]. Each open line
//!   shows the owning app and tool, who is calling, the exact arguments
//!   (secrets redacted, hidden characters shown as `⟨U+202E⟩`) and why it
//!   needs the person, with Approve once, Deny and up to two "Always …"
//!   choices. Modal while up. The argument area shows **every** row: long
//!   rows wrap (`↳`), and past [`MAX_ARG_ROWS`] rows it scrolls (the wheel,
//!   or "More ↓"); Approve once and "Always …" stay disabled until every
//!   row has been on screen ([`ArgWindow`]).
//! - **The first-use sheet** ([`super::consent`]): what the agent may read
//!   and use and where the model runs; Allow or Don't allow. Modal.
//! - **The time-box indicator**: while a rule approves everything one app
//!   asks, a pill at the top says so, with the minutes left and Stop.
//! - **An app agent's question** ([`crate::questions`], the app's
//!   conversation): who asks, the question and its options; a tap answers
//!   it (the person's answer, [`crate::questions::PersonAnswer`]). Modal,
//!   after any approval sheet.
//! - **Stop** on an app-conversation sheet or question: denies or declines
//!   what that app's agent asks and stops the turn running on its shared
//!   conversation, whoever started it ([`super::stop_agent`]).
//! - **Expired** requests (ADR 0004 §8): an approval or question nobody
//!   answered in time ("Expired: no answer in 10 min"), withdrawn from the
//!   modal cards and kept as a small non-modal card until dismissed.
//!
//! A press on a button is the person's gesture: only here (and on the
//! Settings page) is a [`ApprovalGesture`] made.

use makepad_widgets::*;
use std::collections::HashMap;

use super::consent::AgentSummary;
use super::rules::{ApprovalGesture, Rule};
use super::sheet::{app_label, Answer, ArgWindow, Line, Sheet, WrapCache};
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
/// Rows the argument area shows at once; more scroll.
pub use super::sheet::MAX_ARG_ROWS;
const INDICATOR_H: f64 = 30.0;

/// What a press lands on.
#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Answer { sheet: u64, request: RequestId, answer: Answer },
    Consent { app: String, allow: bool },
    StopRule(RuleId),
    /// An option of an app agent's question (`None`: "Don't answer").
    QuestionOption { id: u64, label: Option<String> },
    /// Stop the app agent's running turn (the app's conversation).
    StopAgent { app: String },
    /// Scroll a line's argument area by `rows` (down when positive).
    ScrollArgs { request: RequestId, rows: isize },
    /// Dismiss an expired approval's record, or an expired question's.
    DismissExpired(RequestId),
    DismissQuestion(u64),
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
    /// A button that cannot be pressed yet (no hit is registered for it).
    pub fn draw_disabled(&mut self, cx: &mut Cx2d, x: f64, y: f64, max_w: f64, label: &str) -> Rect {
        let w = self.width(cx, label).min(max_w.max(40.0));
        let r = rect(x, y, w, BUTTON_H);
        let ink = alpha(self.tok.popups.text, 0.35);
        self.d.bordered(cx, r, alpha(self.tok.popups.text, 0.06), alpha(self.tok.popups.text, 0.18), alpha(self.tok.popups.text, 0.06), 0.0, 0.0);
        self.d.label_elided(cx, rect(r.pos.x + 10.0, r.pos.y, r.size.x - 20.0, r.size.y), false, self.tok.font.body, ink, HAlign::Center, label);
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
    /// What expired unanswered: approvals, then questions.
    expired: Vec<super::router::Expired>,
    expired_questions: Vec<crate::questions::Request>,
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
            expired: a.router.expired().to_vec(),
            expired_questions: Vec::new(),
            now,
        }
    })
    .map(|mut f| {
        f.question = crate::questions::open_in_apps().into_iter().next();
        f.expired_questions = crate::questions::expired_in_apps();
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
    /// Each open line's argument area: where it is scrolled, how far down
    /// the person has seen.
    #[rust]
    windows: HashMap<RequestId, ArgWindow>,
    /// The argument areas drawn last frame: (area, line, rows, rows shown).
    #[rust]
    arg_boxes: Vec<(Rect, RequestId, usize, usize)>,
    /// Each open line's wrapped rows, and the width they were wrapped to.
    #[rust]
    wrapped: WrapCache,
}

impl ShellApprovals {
    /// The shell's theme (light or dark) and material, as its other
    /// surfaces take them (desktop_app.rs `apply_material_to_chrome`): a
    /// sheet is lifted with them, so it stays over the chat panes.
    pub fn set_material(&mut self, m: crate::shell::MaterialTokens, palette: Option<crate::shell::ShellPalette>) {
        self.d.set_material(m);
        self.d.set_palette(palette);
    }

    fn hit_at(&self, p: Vec2d) -> Option<Hit> {
        // Buttons before the card they sit on.
        self.hits.iter().rev().find(|(r, h)| *h != Hit::Card && contains(*r, p)).or_else(|| self.hits.iter().find(|(r, _)| contains(*r, p))).map(|(_, h)| h.clone())
    }

    /// Scroll `request`'s argument area.
    fn scroll_args(&mut self, request: &RequestId, rows: isize) {
        let Some((_, _, total, shown)) = self.arg_boxes.iter().find(|(_, r, _, _)| r == request).cloned() else { return };
        self.windows.entry(request.clone()).or_default().scroll(rows, total, shown);
    }

    /// The shell's pointer hook (`approvals::pointer`). True when taken.
    pub fn pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        if let Event::Scroll(e) = event {
            let over = self.arg_boxes.iter().find(|(r, ..)| contains(*r, e.abs)).map(|(_, id, ..)| id.clone());
            if let Some(id) = over {
                let rows = (e.scroll.y / ARG_LINE_H).round() as isize;
                let rows = if rows == 0 { e.scroll.y.signum() as isize } else { rows };
                self.scroll_args(&id, rows);
                self.redraw(cx);
                return true;
            }
            return self.modal;
        }
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
                    match h.clone() {
                        Hit::ScrollArgs { request, rows } => self.scroll_args(&request, rows),
                        h => act(h),
                    }
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
        self.draw_expired(cx, screen, &f, tok);
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
        let card = Self::card_rect(screen, PAD * 2.0 + 26.0 + 20.0 + 12.0 + 40.0 + 12.0 + (options.len() as f64 + 2.0) * (BUTTON_H + 8.0));
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
        y += BUTTON_H + 8.0;
        let stop = stop_label(&q.app);
        let r = buttons.draw(cx, x, y, w, &stop, false);
        hits.push((r, Hit::StopAgent { app: q.app.clone() }));
        self.hits.extend(hits);
        self.shown.push(stop);
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

    /// Expired requests, non-modal, at the bottom: "Expired: no answer in
    /// 10 min" with what it was, and Dismiss.
    fn draw_expired(&mut self, cx: &mut Cx2d, screen: Rect, f: &Frame, tok: ShellTokens) {
        let mut rows: Vec<(String, Hit)> = f
            .expired
            .iter()
            .map(|e| (format!("{} \u{00b7} {} ({})", e.status(), e.heading, e.caller), Hit::DismissExpired(e.id.clone())))
            .collect();
        rows.extend(f.expired_questions.iter().filter_map(|q| {
            let crate::questions::State::Expired(reason) = &q.state else { return None };
            Some((format!("Expired: {reason} \u{00b7} {}", q.asked_by()), Hit::DismissQuestion(q.id)))
        }));
        let px = tok.font.body_small;
        let mut y = screen.pos.y + screen.size.y - 16.0;
        for (text, hit) in rows.into_iter().rev().take(3) {
            let w = (self.d.measure(cx, false, px, &text) + 110.0).min(screen.size.x - 24.0);
            y -= INDICATOR_H + 6.0;
            let pill = rect(screen.pos.x + (screen.size.x - w) * 0.5, y, w, INDICATOR_H);
            let ground = alpha(tok.popups.text, 0.85);
            let ink = contrast(ground);
            self.d.bordered(cx, pill, ground, ground, ground, 0.0, 0.0);
            let dismiss = rect(pill.pos.x + pill.size.x - 78.0, pill.pos.y + 4.0, 72.0, INDICATOR_H - 8.0);
            self.d.solid(cx, dismiss, alpha(ink, 0.15));
            self.d.label(cx, dismiss, false, px, ink, HAlign::Center, "Dismiss");
            self.d.label_elided(cx, rect(pill.pos.x + 12.0, pill.pos.y, dismiss.pos.x - pill.pos.x - 20.0, INDICATOR_H), false, px, ink, HAlign::Left, &text);
            self.hits.push((dismiss, hit));
            self.hits.push((pill, Hit::Card));
            self.shown.push(text);
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

    /// A line's height with `rows` argument rows on screen (and the scroll
    /// row when `scrolls`).
    fn line_height(line: &Line, rows: usize, scrolls: bool) -> f64 {
        let args = rows as f64 * ARG_LINE_H + if scrolls { BUTTON_H + 4.0 } else { 0.0 };
        let note = if line.surfaced.note().is_empty() { 0.0 } else { 18.0 };
        let always = if line.always.is_empty() { 0.0 } else { BUTTON_H + 8.0 };
        22.0 + 18.0 + 8.0 + args + 8.0 + 12.0 + note + BUTTON_H + 8.0 + always + 14.0
    }

    fn draw_sheet(&mut self, cx: &mut Cx2d, screen: Rect, sheet: &Sheet, more: usize, tok: ShellTokens) {
        self.scrim(cx, screen);
        let lines: Vec<&Line> = sheet.open_lines().collect();
        // Forget the windows of lines no longer open.
        self.windows.retain(|id, _| lines.iter().any(|l| l.request == *id));
        self.arg_boxes.clear();
        let card_w = Self::card_rect(screen, 100.0).size.x;
        let row_w = card_w - PAD * 2.0 - 16.0;
        // Every argument row, wrapped to the card: nothing is elided.
        let px = tok.font.body_small;
        let scaled = px * self.d.text_scale();
        self.wrapped.retain(|id| lines.iter().any(|l| l.request == *id));
        let mut wrapped: Vec<Vec<String>> = Vec::with_capacity(lines.len());
        for l in &lines {
            let d = &mut self.d;
            let (rows, rewrapped) = self.wrapped.rows(&l.request, row_w, scaled, &l.args, |t| d.measure(cx, false, scaled, t));
            if rewrapped {
                // New rows (a resize, a text-scale change): what was seen
                // before counts no more (review of #218).
                self.windows.remove(&l.request);
            }
            wrapped.push(rows);
        }
        let shown_rows = |n: usize| n.min(MAX_ARG_ROWS);
        let header = 26.0 + 20.0 + 14.0;
        let body: f64 = lines.iter().zip(&wrapped).map(|(l, w)| Self::line_height(l, shown_rows(w.len()), w.len() > MAX_ARG_ROWS)).sum();
        let stop = sheet.stop_target().map(str::to_string);
        let footer = if more > 0 { 20.0 } else { 0.0 } + if stop.is_some() { BUTTON_H + 8.0 } else { 0.0 };
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
        let mut hits = Vec::new();
        let mut boxes = Vec::new();
        for (i, (line, rows)) in lines.iter().zip(&wrapped).enumerate() {
            let total = rows.len();
            let visible = shown_rows(total);
            let scrolls = total > visible;
            let h = Self::line_height(line, visible, scrolls);
            if y + h > bottom + 1.0 {
                let rest = lines.len() - i;
                self.d.label_elided(cx, rect(x, y, w, 18.0), false, tok.font.body_small, dim, HAlign::Left, &format!("{rest} more below: answer these first"));
                break;
            }
            if i > 0 {
                self.d.solid(cx, rect(x, y - 8.0, w, 1.0), alpha(ink, 0.12));
            }
            let heading = line.heading();
            self.d.label_elided(cx, rect(x, y, w, 20.0), true, tok.font.subtitle, ink, HAlign::Left, &heading);
            y += 22.0;
            let asked = format!("Asked by {}", line.caller);
            self.d.label_elided(cx, rect(x, y, w, 16.0), false, tok.font.body_small, dim, HAlign::Left, &asked);
            y += 18.0 + 8.0;
            let window = self.windows.entry(line.request.clone()).or_default();
            let range = window.show(total, visible);
            let (at_end, at_top, seen_all) = (window.at_end(total, visible), window.top == 0, window.seen_all(total));
            if seen_all && !line.seen {
                // The router waits for this before it takes an approval.
                let (sheet_id, request) = (sheet.id, line.request.clone());
                super::with(|a| a.router.mark_seen(sheet_id, &request));
            }
            let box_h = visible as f64 * ARG_LINE_H + 8.0;
            let area = rect(x, y - 4.0, w, box_h);
            self.d.solid(cx, area, alpha(ink, 0.06));
            boxes.push((area, line.request.clone(), total, visible));
            for row in &rows[range.clone()] {
                // Wrapped already: fits, never elided.
                self.d.label(cx, rect(x + 8.0, y, w - 16.0, ARG_LINE_H), false, px, ink, HAlign::Left, row);
                y += ARG_LINE_H;
            }
            y += 8.0;
            let mut buttons = Buttons { d: &mut self.d, tok, hover: self.hover };
            if scrolls {
                let status = format!("Rows {}\u{2013}{} of {total}", range.start + 1, range.end);
                if !at_end {
                    let r = buttons.draw(cx, x, y, w, &format!("More \u{2193} ({} rows left)", total - range.end), false);
                    hits.push((r, Hit::ScrollArgs { request: line.request.clone(), rows: visible as isize }));
                    buttons.d.label_elided(cx, rect(r.pos.x + r.size.x + 10.0, y, (x + w - r.pos.x - r.size.x - 10.0).max(0.0), BUTTON_H), false, px, dim, HAlign::Left, &status);
                } else if !at_top {
                    let r = buttons.draw(cx, x, y, w, "Back to the top \u{2191}", false);
                    hits.push((r, Hit::ScrollArgs { request: line.request.clone(), rows: -(total as isize) }));
                    buttons.d.label_elided(cx, rect(r.pos.x + r.size.x + 10.0, y, (x + w - r.pos.x - r.size.x - 10.0).max(0.0), BUTTON_H), false, px, dim, HAlign::Left, &status);
                }
                y += BUTTON_H + 4.0;
            }
            y += 12.0;
            let note = if seen_all { line.surfaced.note() } else { "Read every argument (scroll to the end) to approve." };
            if !note.is_empty() || !line.surfaced.note().is_empty() {
                buttons.d.label_elided(cx, rect(x, y, w, 16.0), false, px, rgb(0xE0, 0x8A, 0x00), HAlign::Left, note);
                y += 18.0;
            }
            let mut bx = x;
            let once = if seen_all {
                let r = buttons.draw(cx, bx, y, w, "Approve once", true);
                hits.push((r, Hit::Answer { sheet: sheet.id, request: line.request.clone(), answer: Answer::Once }));
                r
            } else {
                buttons.draw_disabled(cx, bx, y, w, "Approve once")
            };
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
                    let max = if k == 0 { room * 0.6 } else { room };
                    let r = if seen_all {
                        let r = buttons.draw(cx, bx, y, max, &choice.label, false);
                        hits.push((r, Hit::Answer { sheet: sheet.id, request: line.request.clone(), answer: Answer::Always(k) }));
                        r
                    } else {
                        buttons.draw_disabled(cx, bx, y, max, &choice.label)
                    };
                    bx += r.size.x + 8.0;
                }
                y += BUTTON_H + 8.0;
            }
            y += 14.0;
            self.shown.push(heading);
            self.shown.push(asked);
            self.shown.extend(line.args.iter().cloned());
            self.shown.push(if seen_all { "Approve once".to_string() } else { format!("Approve once (disabled: rows {}\u{2013}{} of {total} seen)", 1, window_seen(&self.windows, &line.request, total)) });
        }
        let mut buttons = Buttons { d: &mut self.d, tok, hover: self.hover };
        if let Some(app) = &stop {
            let label = stop_label(app);
            let top = if more > 0 { bottom - 20.0 - BUTTON_H - 4.0 } else { bottom - BUTTON_H };
            let r = buttons.draw(cx, x, top, w, &label, false);
            hits.push((r, Hit::StopAgent { app: app.clone() }));
            self.shown.push(label);
        }
        if more > 0 {
            buttons.d.label_elided(cx, rect(x, bottom - 18.0, w, 18.0), false, tok.font.body_small, dim, HAlign::Left, &format!("{more} more sheet{} after this one", if more == 1 { "" } else { "s" }));
        }
        self.hits.extend(hits);
        self.arg_boxes = boxes;
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

/// How many of a line's `total` rows the person has seen.
fn window_seen(windows: &HashMap<RequestId, ArgWindow>, id: &RequestId, total: usize) -> usize {
    windows.get(id).map_or(0, |w| w.seen_to.min(total))
}

/// "Stop Rinx's agent".
fn stop_label(app: &str) -> String {
    format!("Stop {}'s agent", app_label(app))
}

/// A press on one of the surface's buttons, exactly as
/// [`ShellApprovals::pointer`] hands it over: the two-lane scenario tests
/// (`host_tools/scenario_tests.rs`) press the drawn sheet's and question
/// card's buttons without a window.
#[cfg(test)]
pub(crate) fn press(hit: Hit) {
    act(hit)
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
        Hit::StopAgent { app } => {
            let stopped = super::stop_agent(&app);
            log!("approvals: the person stopped {app}'s agent ({} turn(s))", stopped.len());
        }
        Hit::DismissExpired(id) => super::dismiss_expired(&id),
        Hit::DismissQuestion(id) => crate::questions::dismiss(id),
        // The view scrolls its own argument areas (`pointer`).
        Hit::ScrollArgs { .. } | Hit::Card => {}
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
