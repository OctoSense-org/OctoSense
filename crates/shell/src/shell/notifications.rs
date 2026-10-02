//! `shell/plugins/notifications/` — the toast stack.
//!
//! `components/NotificationCard.qml`: a 380-wide card on
//! `[notifications] background` behind a 2px `[notifications] border`, 12px
//! side margins, 10px top/bottom (7 for a single-line toast), a 40×40 icon
//! slot 12px off the text column, a bold `title` (14px) summary of at most
//! two lines and a `title` body of at most three at `darker(text, 1.15)`,
//! and an 18×18 close button 3px inside the top-right corner that only
//! appears on hover.
//!
//! `Service.qml` / `NotificationLogic.js`: cards stack top-right with 8px
//! between them, newest first, cleared of the bar by `barSize + gapsOut`;
//! lifetimes are Critical → never, Low → clamp(5s..30s), Normal →
//! clamp(8s..30s), and hovering a card PAUSES its countdown.
//!
//! One deliberate addition over the QML: the card draws the countdown as a
//! bar in `[notifications] countdown` (the accent) along its bottom edge.
//! Omarchy computes that color and never draws it — the brief asks for it,
//! and it is the only way a toast says how long it has left.

use makepad_widgets::*;

use super::ui::{contains, inset, rect, DrawShellFill, Ico, ShellDraw};
use super::{darker, fade, MaterialTokens, ShellTokens};
use crate::octosense::style::{AppIconDraw, DesktopStyle};

thread_local! {
    /// A surface the toasts stay clear of while it is up ([`keep_clear_of`]).
    static KEEP_CLEAR: std::cell::Cell<Option<Rect>> = const { std::cell::Cell::new(None) };
}

/// A surface at the toasts' corner says where it is each frame it draws
/// (None while it is down), and the toasts stack left of it instead of over
/// it: the glance panel, which opens with a new card whose toast comes too.
pub fn keep_clear_of(r: Option<Rect>) {
    KEEP_CLEAR.with(|c| c.set(r));
}

/// The stack's left edge: at the screen's right, or left of a surface it
/// keeps clear of when they would overlap.
fn stack_x(screen: Rect, gaps_out: f64, clear: Option<Rect>) -> f64 {
    let x = screen.pos.x + screen.size.x - gaps_out - CARD_WIDTH;
    match clear {
        Some(r) if r.size.x > 0.0 && r.pos.x < x + CARD_WIDTH && r.pos.x + r.size.x > x => (r.pos.x - STACK_SPACING - CARD_WIDTH).max(screen.pos.x + gaps_out),
        _ => x,
    }
}

pub const CARD_WIDTH: f64 = 380.0;
const SIDE_MARGIN: f64 = 12.0;
const V_MARGIN: f64 = 10.0;
const V_MARGIN_TOAST: f64 = 7.0;
const ICON_SLOT: f64 = 40.0;
const ICON_GAP: f64 = 12.0;
const TEXT_RIGHT_MARGIN: f64 = 10.0;
const TEXT_SPACING: f64 = 2.0;
const CLOSE_SIZE: f64 = 18.0;
const CLOSE_INSET: f64 = 3.0;
const STACK_SPACING: f64 = 8.0;
const COUNTDOWN_HEIGHT: f64 = 2.0;
const SUMMARY_LINES: usize = 2;
/// The caption line's height (a glance card's app name, small).
const CAPTION_LINE: f64 = 17.0;
const BODY_LINES: usize = 3;

/// libnotify urgency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

/// `NotificationLogic.snapshotOf`.
#[derive(Clone, Debug, Default)]
pub struct Notification {
    pub id: u64,
    pub app: String,
    /// A small line above the summary: who it is from (a glance card's app
    /// name). Empty: none.
    pub caption: String,
    pub summary: String,
    pub body: String,
    pub icon: Option<Ico>,
    /// The app whose own icon the toast shows (a launcher id), drawn as the
    /// dock draws it; else `icon`.
    pub app_icon: Option<String>,
    pub urgency: Urgency,
    /// The sender's hint in seconds; 0 means "you decide".
    pub requested: f64,
}

impl Notification {
    /// `Service.durationFor`: Critical never expires, Low is clamped to
    /// 5..30s and Normal to 8..30s.
    pub fn lifetime(&self) -> f64 {
        match self.urgency {
            Urgency::Critical => 0.0,
            Urgency::Low => {
                if self.requested > 0.0 {
                    self.requested.clamp(5.0, 30.0)
                } else {
                    5.0
                }
            }
            Urgency::Normal => {
                if self.requested > 0.0 {
                    self.requested.clamp(8.0, 30.0)
                } else {
                    8.0
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct Live {
    note: Notification,
    lifetime: f64,
    left: f64,
    hovered: bool,
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.ShellNotificationsBase = #(ShellNotifications::register_widget(vm))
    mod.widgets.ShellNotifications = set_type_default() do mod.widgets.ShellNotificationsBase {
        width: Fill
        height: Fill
        draw_bg +: {}
        d +: {}
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum ShellNotificationsAction {
    /// The card body was clicked: run the default action, then dismiss.
    Activated(u64),
    /// The ✕ or a right-click: dismiss only.
    Dismissed(u64),
    #[default]
    None,
}

#[derive(Script, ScriptHook, Widget)]
pub struct ShellNotifications {
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
    live: Vec<Live>,
    #[rust]
    next_id: u64,
    /// How far the stack is pushed down (the bar's height + gapsOut).
    #[rust]
    pub bar_clearance: f64,
    /// The desktop style app icons are drawn in, and their drawer.
    #[rust]
    pub icon_style: DesktopStyle,
    #[rust]
    app_icons: AppIconDraw,
    #[rust]
    area: Area,
    #[rust]
    screen: Rect,
    #[rust]
    card_rects: Vec<(u64, Rect, Rect)>,
    #[rust]
    next_frame: NextFrame,
    #[rust]
    last_time: f64,
    #[rust]
    pub inert: bool,
    /// What the last layout log said, so it is logged once per change.
    #[rust]
    logged: String,
}

impl ShellNotifications {
    /// The in-process notification API: the WM (or anything holding this
    /// widget) posts, and the stack owns the rest.
    pub fn post(&mut self, cx: &mut Cx, mut note: Notification) -> u64 {
        self.next_id += 1;
        note.id = self.next_id;
        let lifetime = note.lifetime();
        self.live.insert(
            0,
            Live {
                note,
                lifetime,
                left: lifetime,
                hovered: false,
            },
        );
        self.last_time = 0.0;
        self.next_frame = cx.new_next_frame();
        self.redraw(cx);
        self.next_id
    }

    /// The one-call API the WM uses for `WmRequest::Notify{title, body}`.
    pub fn notify(&mut self, cx: &mut Cx, title: &str, body: &str) -> u64 {
        self.post(
            cx,
            Notification {
                id: 0,
                app: "wm".into(),
                summary: title.to_string(),
                body: body.to_string(),
                icon: Some(Ico::Bell),
                urgency: Urgency::Normal,
                requested: 0.0,
                ..Default::default()
            },
        )
    }

    pub fn dismiss(&mut self, cx: &mut Cx, id: u64) {
        self.live.retain(|l| l.note.id != id);
        self.redraw(cx);
    }

    pub fn clear(&mut self, cx: &mut Cx) {
        self.live.clear();
        self.redraw(cx);
    }

    pub fn len(&self) -> usize {
        self.live.len()
    }

    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    /// Whether `p` is on a toast, its card or its close button: a press
    /// there is the toast's, whatever surface lies under it.
    pub fn hit(&self, p: Vec2d) -> bool {
        !self.inert && self.card_rects.iter().any(|(_, card, close)| contains(*card, p) || contains(*close, p))
    }

    /// The height a card needs for its text.
    fn card_height(&mut self, cx: &mut Cx2d, note: &Notification) -> f64 {
        let tok = self.d.tokens(self.tokens);
        let text_w = CARD_WIDTH
            - SIDE_MARGIN * 2.0
            - ICON_SLOT
            - ICON_GAP
            - TEXT_RIGHT_MARGIN
            - tok.notifications.surface.border_width * 2.0;
        let summary = self
            .d
            .wrap(cx, true, tok.font.title, &note.summary, text_w, SUMMARY_LINES)
            .len()
            .max(1);
        let body = if note.body.is_empty() {
            0
        } else {
            self.d
                .wrap(cx, false, tok.font.title, &note.body, text_w, BODY_LINES)
                .len()
        };
        let line = tok.font.title * 1.35;
        let single = summary == 1 && body == 0 && note.caption.is_empty();
        let v = if single { V_MARGIN_TOAST } else { V_MARGIN };
        let caption = if note.caption.is_empty() { 0.0 } else { CAPTION_LINE };
        let text_h = caption + summary as f64 * line
            + if body > 0 {
                TEXT_SPACING + body as f64 * line
            } else {
                0.0
            };
        (text_h.max(ICON_SLOT) + v * 2.0 + tok.notifications.surface.border_width * 2.0).ceil()
    }

    /// The material the kit paints the stack with; the next draw reads it.
    pub fn set_material(&mut self, m: MaterialTokens, palette: Option<super::ShellPalette>) {
        self.d.set_material(m);
        self.d.set_palette(palette);
    }

    /// Under glass the stack is hoisted into the kit's overlay list.
    pub fn draw_surface(&mut self, cx: &mut Cx2d, screen: Rect) {
        self.d.begin_surface(cx);
        self.draw_surface_inner(cx, screen);
        self.d.end_surface(cx);
    }

    fn draw_surface_inner(&mut self, cx: &mut Cx2d, screen: Rect) {
        self.card_rects.clear();
        self.screen = screen;
        if self.live.is_empty() {
            return;
        }
        let tok = self.d.tokens(self.tokens);
        let gaps_out = tok.spacing.gaps_out;
        let border = tok.notifications.surface.border_width;
        let mut y = screen.pos.y + self.bar_clearance.max(gaps_out);
        let x = stack_x(screen, gaps_out, KEEP_CLEAR.with(|c| c.get()));

        for entry in self.live.clone() {
            let note = entry.note.clone();
            let h = self.card_height(cx, &note);
            let card = rect(x, y, CARD_WIDTH, h);
            self.d.card(cx, card, &tok.notifications.surface);

            let inner = inset(card, border);
            let single = h <= ICON_SLOT + V_MARGIN_TOAST * 2.0 + border * 2.0;
            let v = if single { V_MARGIN_TOAST } else { V_MARGIN };
            let mut tx = inner.pos.x + SIDE_MARGIN;
            if let Some(app) = note.app_icon.as_deref() {
                // The app's own icon, as the dock shows it.
                let size = 34.0;
                let at = rect(tx + (ICON_SLOT - size) * 0.5, inner.pos.y + v + (ICON_SLOT - size) * 0.5, size, size);
                self.app_icons.draw(cx, app, self.icon_style, at, 1.0, tok.notifications.surface.text);
                tx += ICON_SLOT + ICON_GAP;
            } else if let Some(ico) = note.icon {
                self.d.icon_centered(
                    cx,
                    ico,
                    rect(tx, inner.pos.y + v, ICON_SLOT, ICON_SLOT),
                    tok.font.display_large,
                    tok.notifications.surface.text,
                );
                tx += ICON_SLOT + ICON_GAP;
            }
            let text_w = inner.pos.x + inner.size.x - SIDE_MARGIN - TEXT_RIGHT_MARGIN - tx;
            let line = tok.font.title * 1.35;
            let mut ty = inner.pos.y + v;
            if !note.caption.is_empty() {
                self.d.label_elided(
                    cx,
                    rect(tx, ty, text_w, CAPTION_LINE),
                    false,
                    tok.font.body_small,
                    darker(tok.notifications.surface.text, 1.6),
                    super::ui::HAlign::Left,
                    &note.caption,
                );
                ty += CAPTION_LINE;
            }
            let summary = self
                .d
                .wrap(cx, true, tok.font.title, &note.summary, text_w, SUMMARY_LINES);
            for l in &summary {
                self.d.label(
                    cx,
                    rect(tx, ty, text_w, line),
                    true,
                    tok.font.title,
                    tok.notifications.surface.text,
                    super::ui::HAlign::Left,
                    l,
                );
                ty += line;
            }
            if !note.body.is_empty() {
                ty += TEXT_SPACING;
                let body = self
                    .d
                    .wrap(cx, false, tok.font.title, &note.body, text_w, BODY_LINES);
                let body_color = darker(tok.notifications.surface.text, 1.15);
                for l in &body {
                    self.d.label(
                        cx,
                        rect(tx, ty, text_w, line),
                        false,
                        tok.font.title,
                        body_color,
                        super::ui::HAlign::Left,
                        l,
                    );
                    ty += line;
                }
            }

            // The ✕, visible on hover only.
            let close = rect(
                inner.pos.x + inner.size.x - CLOSE_INSET - CLOSE_SIZE,
                inner.pos.y + CLOSE_INSET,
                CLOSE_SIZE,
                CLOSE_SIZE,
            );
            if entry.hovered {
                self.d.icon_centered(
                    cx,
                    Ico::Close,
                    close,
                    CLOSE_SIZE * 0.6,
                    darker(tok.notifications.surface.text, 1.4),
                );
            }

            // The countdown, in the accent.
            if entry.lifetime > 0.0 {
                let p = (entry.left / entry.lifetime).clamp(0.0, 1.0);
                self.d.solid(
                    cx,
                    rect(
                        card.pos.x,
                        card.pos.y + card.size.y - COUNTDOWN_HEIGHT,
                        card.size.x * p,
                        COUNTDOWN_HEIGHT,
                    ),
                    fade(tok.notifications.countdown, if entry.hovered { 0.5 } else { 1.0 }),
                );
            }

            self.card_rects.push((note.id, card, close));
            y += h + STACK_SPACING;
        }
        // Where the toasts landed, once per change: evidence for a remote run.
        let layout: Vec<String> = self.card_rects.iter().map(|(id, r, _)| format!("{id}@{},{},{},{}", r.pos.x as i32, r.pos.y as i32, r.size.x as i32, r.size.y as i32)).collect();
        let layout = layout.join(" ");
        if layout != self.logged {
            log!("notifications: {} toast(s) {layout}", self.card_rects.len());
            self.logged = layout;
        }
    }
}

impl Widget for ShellNotifications {
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let screen = cx.turtle().rect();
        self.draw_surface(cx, screen);
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if self.inert {
            return;
        }
        if let Some(ne) = self.next_frame.is_event(event) {
            let dt = if self.last_time <= 0.0 {
                1.0 / 60.0
            } else {
                (ne.time - self.last_time).clamp(0.001, 0.05)
            };
            self.last_time = ne.time;
            let mut busy = false;
            for entry in self.live.iter_mut() {
                // Hovering pauses the countdown (`ticking: !card.hovered`).
                if entry.lifetime > 0.0 && !entry.hovered {
                    entry.left -= dt;
                    busy = true;
                }
            }
            let before = self.live.len();
            self.live
                .retain(|l| l.lifetime <= 0.0 || l.left > 0.0);
            if before != self.live.len() || busy {
                self.redraw(cx);
            }
            if !self.live.is_empty() {
                self.next_frame = cx.new_next_frame();
            }
        }
        match event {
            Event::MouseMove(e) => {
                let rects = self.card_rects.clone();
                let mut changed = false;
                for entry in self.live.iter_mut() {
                    let over = rects
                        .iter()
                        .find(|(id, _, _)| *id == entry.note.id)
                        .map(|(_, card, _)| contains(*card, e.abs))
                        .unwrap_or(false);
                    if over != entry.hovered {
                        entry.hovered = over;
                        changed = true;
                    }
                }
                if changed {
                    self.redraw(cx);
                }
            }
            Event::MouseDown(e) => {
                let rects = self.card_rects.clone();
                for (id, card, close) in rects {
                    if contains(close, e.abs) {
                        cx.widget_action(self.uid, ShellNotificationsAction::Dismissed(id));
                        self.dismiss(cx, id);
                        return;
                    }
                    if contains(card, e.abs) {
                        if e.button.contains(MouseButton::SECONDARY) {
                            cx.widget_action(self.uid, ShellNotificationsAction::Dismissed(id));
                        } else {
                            cx.widget_action(self.uid, ShellNotificationsAction::Activated(id));
                        }
                        self.dismiss(cx, id);
                        return;
                    }
                }
            }
            _ => {}
        }
    }
}

/// The three fixtures the gallery shows.
pub fn fixtures() -> Vec<Notification> {
    vec![
        Notification {
            id: 0,
            app: "wm".into(),
            summary: "Theme imported".into(),
            body: "tokyo-night is now the active theme, with 4 backgrounds.".into(),
            icon: Some(Ico::Moon),
            urgency: Urgency::Normal,
            requested: 0.0,
            ..Default::default()
        },
        Notification {
            id: 0,
            app: "terminal".into(),
            summary: "Build finished".into(),
            body: String::new(),
            icon: Some(Ico::Check),
            urgency: Urgency::Low,
            requested: 0.0,
            ..Default::default()
        },
        Notification {
            id: 0,
            app: "system".into(),
            summary: "Battery low".into(),
            body: "18% left. Plug in soon — this one never expires on its own.".into(),
            icon: Some(Ico::Battery),
            urgency: Urgency::Critical,
            requested: 0.0,
            ..Default::default()
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With the glance panel up at the right, the toasts stack left of it;
    /// without it (or with a surface elsewhere), at the right edge.
    #[test]
    fn toasts_stack_clear_of_the_glance_panel() {
        let screen = rect(0.0, 0.0, 1400.0, 900.0);
        let right = 1400.0 - 10.0 - CARD_WIDTH;
        assert_eq!(stack_x(screen, 10.0, None), right);
        let panel = rect(1400.0 - 10.0 - 360.0, 40.0, 360.0, 800.0);
        assert_eq!(stack_x(screen, 10.0, Some(panel)), panel.pos.x - STACK_SPACING - CARD_WIDTH);
        assert_eq!(stack_x(screen, 10.0, Some(rect(10.0, 40.0, 440.0, 500.0))), right, "the Assistant pane at the left is not in the way");
    }

    #[test]
    fn lifetimes_follow_duration_for() {
        let mut n = fixtures()[0].clone();
        n.urgency = Urgency::Critical;
        assert_eq!(n.lifetime(), 0.0);
        n.urgency = Urgency::Low;
        n.requested = 1.0;
        assert_eq!(n.lifetime(), 5.0);
        n.requested = 45.0;
        assert_eq!(n.lifetime(), 30.0);
        n.urgency = Urgency::Normal;
        n.requested = 0.0;
        assert_eq!(n.lifetime(), 8.0);
        n.requested = 2.0;
        assert_eq!(n.lifetime(), 8.0);
    }
}
