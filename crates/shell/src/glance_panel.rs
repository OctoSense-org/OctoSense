//! The desktop's glance panel: the cards apps published to the glance
//! screen (glance.rs), in a column at the right edge of the desk, over the
//! windows, the way the phone shows them on its glance page.
//!
//! F9 (or `--test-action glance`) shows and hides it. Each card is live: the
//! panel hands it the pointer inside its tile and every other event
//! (typing, focus, timers, answers), so it works as it does in its app
//! (glance_card.rs). A card takes the shell's light or dark palette unless
//! it names a theme of its own.
//!
//! - **A card's own controls** (its chips, buttons and fields) work in place.
//!   A press anywhere else on it opens the card in the card window
//!   ([`ShellGlancePanelAction::OpenCard`]), as its notification does.
//! - **The hovered card** shows its actions over its top-right corner, on a
//!   backdrop: open the app that published it, and dismiss the card. They
//!   are not drawn over every card's content all the time.
//! - **The header** says how many cards there are and has Clear all and the
//!   panel's close; a click outside the column closes it too. With no card
//!   it says so ("You're all caught up").
//! - **The list** starts at the top and the wheel moves it a card at a time;
//!   a new card scrolls it no further than shows that card
//!   ([`first_to_reveal`]). "More above" heads the list and "more below"
//!   ends it.
//!
//! While it is open, toasts stack left of it. The column is [`PANEL_WIDTH`]
//! wide, as wide as a toast; each card is a tile of the column's inner width
//! at its measured height (glance_card.rs), in the glance order (priority,
//! then recency).
use crate::glance_card::{GlanceTiles, LiveCards};
use crate::shell::ui::{contains, rect, DrawShellFill, HAlign, Ico, ShellDraw};
use crate::shell::{alpha, MaterialTokens, ShellTokens};
use makepad_widgets::*;

pub const PANEL_WIDTH: f64 = 380.0;
const PAD: f64 = 16.0;
const GAP: f64 = 12.0;
const HEADER: f64 = 64.0;
/// The line "more above" or "more below" takes.
const HINT_H: f64 = 20.0;
/// How long a card that just came wears its accent mark (its toast says
/// the same; the mark shows which card it was).
const NEW_FOR_MS: u64 = 8_000;

/// The first card to show so the card at `n` is on screen, scrolling as
/// little as that takes (from the top when it fits there): `heights` are
/// the cards' tile heights and `room` the list's height. A hint line is
/// kept for "more above" when the list does not start at the top, and one
/// for "more below".
pub fn first_to_reveal(heights: &[f64], n: usize, room: f64) -> usize {
    let n = n.min(heights.len().saturating_sub(1));
    for first in 0..=n {
        let above = if first > 0 { HINT_H } else { 0.0 };
        let used: f64 = heights[first..=n].iter().sum::<f64>() + GAP * (n - first) as f64 + above + HINT_H;
        if used <= room {
            return first;
        }
    }
    n
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.ShellGlancePanelBase = #(ShellGlancePanel::register_widget(vm))
    mod.widgets.ShellGlancePanel = set_type_default() do mod.widgets.ShellGlancePanelBase {
        width: Fill
        height: Fill
        draw_bg +: {}
        d +: {}
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum ShellGlancePanelAction {
    /// A card's open action: open this launcher id (and route, when given).
    Open { app: String, route: Option<String> },
    /// A press on a card, outside its own controls: open the card (its
    /// key) in the card window.
    OpenCard { key: String },
    #[default]
    None,
}

#[derive(Script, ScriptHook, Widget)]
pub struct ShellGlancePanel {
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
    pub open: bool,
    /// How far the column is pushed down (the bar's height).
    #[rust]
    pub bar_clearance: f64,
    #[rust]
    column: Rect,
    /// Each card drawn: its tile, the app it opens (and route), its key.
    #[rust]
    card_rects: Vec<(Rect, String, Option<String>, String)>,
    /// The header's close button and Clear all.
    #[rust]
    close: Rect,
    #[rust]
    clear_all: Rect,
    /// The card under the pointer (its actions show), and the card a press
    /// went down on outside its own controls (a release on it opens it).
    #[rust]
    hover: Option<String>,
    #[rust]
    card_press: Option<String>,
    /// A new card came: the next draw scrolls as little as shows it.
    #[rust]
    reveal_newest: bool,
    /// When the newest mark goes (a redraw drops it).
    #[rust]
    new_timer: Timer,
    /// What the last layout log said, so it is logged once per change.
    #[rust]
    logged: String,
    #[rust]
    tiles: GlanceTiles,
    /// The L0 cards' live state, by card key, as the card window keeps its
    /// card's (glance_card.rs `LiveCards`): their chips, buttons and field
    /// edits run here as there, their in-card chat (`sys.chat`) with the
    /// publishing app's own agent included. A card's state lasts while the
    /// panel shows it; closing the panel forgets it.
    #[rust]
    live: LiveCards,
    /// The panel's own area: what `redraw` repaints (`draw_bg` draws
    /// nothing).
    #[redraw]
    #[rust]
    area: Area,
    /// The first card shown (the wheel moves it a card at a time), the
    /// wheel's travel not yet turned into a step, and how many cards did
    /// not fit below the last one drawn.
    #[rust]
    first: usize,
    #[rust]
    wheel: f64,
    #[rust]
    below: usize,
}

/// How far the wheel travels for one card.
const WHEEL_STEP: f64 = 60.0;

impl ShellGlancePanel {
    /// Whether a pointer event at `p` is the panel's: anywhere in the open
    /// column (its cards' or its own), or a press anywhere (outside the
    /// column, it closes the panel).
    pub fn owns_pointer(&self, p: DVec2, press: bool) -> bool {
        self.open && (press || contains(self.column, p))
    }
    /// Bring the newest card on screen (a new card opened the panel),
    /// scrolling as little as that takes ([`first_to_reveal`]).
    pub fn show_newest(&mut self) {
        self.reveal_newest = true;
        self.wheel = 0.0;
    }
    /// The card at `p`, by key.
    fn card_at(&self, p: DVec2) -> Option<String> {
        self.card_rects.iter().find(|(r, ..)| contains(*r, p)).map(|(.., key)| key.clone())
    }
    pub fn toggle(&mut self, cx: &mut Cx) {
        self.open = !self.open;
        self.redraw(cx);
    }
    pub fn set_material(&mut self, m: MaterialTokens, palette: Option<crate::shell::ShellPalette>) {
        self.d.set_material(m);
        self.d.set_palette(palette);
    }
    /// Where each card is, for a test driver.
    pub fn card_rects(&self) -> Vec<(Rect, String)> {
        self.card_rects.iter().map(|(r, app, _, _)| (*r, app.clone())).collect()
    }
}

impl Widget for ShellGlancePanel {
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let screen = cx.turtle().rect();
        self.card_rects.clear();
        self.close = Rect::default();
        // Every frame, open or closed: the kit keeps its overlay in tree order.
        self.d.begin_surface(cx);
        self.clear_all = Rect::default();
        if self.open {
            let tok = self.d.tokens(self.tokens);
            let top = screen.pos.y + self.bar_clearance + tok.spacing.gaps_out;
            let x = screen.pos.x + screen.size.x - tok.spacing.gaps_out - PANEL_WIDTH;
            let column = rect(x, top, PANEL_WIDTH, (screen.pos.y + screen.size.y - tok.spacing.gaps_out - top).max(HEADER));
            self.column = column;
            self.d.card(cx, column, &tok.notifications.surface);
            let ink = tok.notifications.surface.text;
            let dim = alpha(ink, 0.65);
            let accent = tok.notifications.countdown;
            let cards = crate::glance::shown();
            // The header: the title and how many; Clear all and close.
            self.close = rect(x + PANEL_WIDTH - PAD - 28.0, top + 12.0, 28.0, 28.0);
            self.d.icon_centered(cx, Ico::Close, self.close, 14.0, alpha(ink, 0.8));
            let mut title_w = PANEL_WIDTH - PAD * 2.0 - 32.0;
            if !cards.is_empty() {
                let w = self.d.measure(cx, false, 13.0 * self.d.text_scale(), "Clear all") + 16.0;
                self.clear_all = rect(self.close.pos.x - 6.0 - w, top + 14.0, w, 24.0);
                self.d.label(cx, self.clear_all, false, 13.0, accent, HAlign::Center, "Clear all");
                title_w = self.clear_all.pos.x - x - PAD - 8.0;
            }
            self.d.label_elided(cx, rect(x + PAD, top + 14.0, title_w, 24.0), true, 18.0, ink, HAlign::Left, "At a glance");
            let count = match cards.len() {
                0 => "Nothing new".to_string(),
                1 => "1 card from your apps".to_string(),
                n => format!("{n} cards from your apps"),
            };
            self.d.label_elided(cx, rect(x + PAD, top + 38.0, PANEL_WIDTH - PAD * 2.0, 18.0), false, 12.0, dim, HAlign::Left, &count);
            let list_top = top + HEADER;
            let bottom = column.pos.y + column.size.y - PAD;
            if cards.is_empty() {
                // Nothing to show: say so, in the middle of the column.
                let mid = list_top + (bottom - list_top) * 0.38;
                self.d.icon_centered(cx, Ico::Check, rect(x, mid - 44.0, PANEL_WIDTH, 32.0), 26.0, dim);
                self.d.label(cx, rect(x + PAD, mid, PANEL_WIDTH - PAD * 2.0, 22.0), true, 15.0, ink, HAlign::Center, "You\u{2019}re all caught up");
                self.d.label(cx, rect(x + PAD, mid + 24.0, PANEL_WIDTH - PAD * 2.0, 18.0), false, 12.0, dim, HAlign::Center, "Cards from your apps appear here.");
            } else {
                let heights: Vec<f64> = cards.iter().map(|c| crate::glance_card::tile_height(&c.key())).collect();
                if std::mem::take(&mut self.reveal_newest) {
                    if let Some(n) = cards.iter().enumerate().max_by_key(|(_, c)| c.published_ms).map(|(i, _)| i) {
                        self.first = first_to_reveal(&heights, n, bottom - list_top);
                    }
                }
                self.first = self.first.min(cards.len() - 1);
                let mut y = list_top;
                if self.first > 0 {
                    let more = format!("\u{2191} {} more above", self.first);
                    self.d.label(cx, rect(x + PAD, y - 2.0, PANEL_WIDTH - PAD * 2.0, HINT_H - 4.0), false, 12.0, dim, HAlign::Center, &more);
                    y += HINT_H;
                }
                let limit = bottom - HINT_H;
                let now = crate::glance::now_ms();
                let mut mark_left: Option<u64> = None;
                let mut drawn = 0;
                for (i, card) in cards.iter().enumerate().skip(self.first) {
                    let key = card.key();
                    let h = heights[i];
                    // The first card shown always draws (clipped by its cap).
                    if drawn > 0 && y + h > limit {
                        break;
                    }
                    drawn += 1;
                    let r = rect(x + PAD, y, PANEL_WIDTH - PAD * 2.0, h);
                    let body = self.live.body(&key, card, "glance panel");
                    self.tiles.draw(cx, &key, &card.app, card.contained, &body, r);
                    // A card that just came: an accent mark in the gutter
                    // beside it for a few seconds, the one its toast is about.
                    let age = now.saturating_sub(card.published_ms);
                    if age < NEW_FOR_MS {
                        self.d.solid(cx, rect(r.pos.x - 9.0, r.pos.y + 14.0, 3.0, (r.size.y - 28.0).max(8.0)), accent);
                        let left = NEW_FOR_MS - age;
                        mark_left = Some(mark_left.map_or(left, |m| m.min(left)));
                    }
                    // The hovered card's actions, on a backdrop so they read
                    // over its content: dismiss, and open its app.
                    if self.hover.as_deref() == Some(key.as_str()) {
                        let open = crate::glance_card::open_button(r);
                        let dismiss = crate::glance_card::dismiss_button(r);
                        let backdrop = rect(dismiss.pos.x - 4.0, dismiss.pos.y - 4.0, open.pos.x + open.size.x - dismiss.pos.x + 8.0, open.size.y + 8.0);
                        self.d.card(cx, backdrop, &tok.notifications.surface);
                        self.d.icon_centered(cx, Ico::Close, dismiss, 12.0, ink);
                        self.d.icon_centered(cx, Ico::ChevronRight, open, 14.0, ink);
                    }
                    self.card_rects.push((r, card.open_app.clone(), card.route.clone(), key));
                    y += h + GAP;
                }
                if let Some(left) = mark_left {
                    if self.new_timer.is_empty() {
                        self.new_timer = cx.start_timeout(left as f64 / 1000.0 + 0.05);
                    }
                }
                // The cards the column does not show: the wheel brings them.
                self.below = cards.len().saturating_sub(self.first + drawn);
                if self.below > 0 {
                    let more = format!("{} more below \u{2193}", self.below);
                    self.d.label(cx, rect(x + PAD, bottom - HINT_H + 2.0, PANEL_WIDTH - PAD * 2.0, HINT_H - 4.0), false, 12.0, dim, HAlign::Center, &more);
                }
            }
        }
        self.d.end_surface(cx);
        // The toasts stack clear of the open column (notifications.rs).
        crate::shell::notifications::keep_clear_of(self.open.then_some(self.column));
        // Where the cards landed, once per change: evidence for a remote run.
        let layout: Vec<String> = self.card_rects.iter().map(|(r, app, _, _)| format!("{app}@{},{},{},{}", r.pos.x as i32, r.pos.y as i32, r.size.x as i32, r.size.y as i32)).collect();
        let layout = layout.join(" ");
        if self.open && layout != self.logged {
            log!("glance panel: {} card(s) {}", self.card_rects.len(), layout);
            self.logged = layout;
        }
        let live: Vec<String> = if self.open { crate::glance::shown().iter().map(|c| c.key()).collect() } else { Vec::new() };
        self.tiles.sweep(cx, &live);
        self.live.retain(&live);
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        // A newest mark's time is up.
        if self.new_timer.is_event(event).is_some() {
            self.new_timer = Timer::empty();
            self.redraw(cx);
        }
        // The wheel over the column: a card at a time.
        if let Event::Scroll(e) = event {
            if self.open && contains(self.column, e.abs) {
                self.wheel += e.scroll.y;
                let mut moved = false;
                while self.wheel >= WHEEL_STEP {
                    self.wheel -= WHEEL_STEP;
                    if self.below > 0 {
                        self.first += 1;
                        self.below -= 1;
                        moved = true;
                    }
                }
                while self.wheel <= -WHEEL_STEP {
                    self.wheel += WHEEL_STEP;
                    if self.first > 0 {
                        self.first -= 1;
                        moved = true;
                    }
                }
                if moved {
                    self.redraw(cx);
                }
                return;
            }
        }
        // The card under the pointer shows its actions.
        if let Event::MouseMove(e) = event {
            if self.open {
                let hover = self.card_at(e.abs);
                if hover != self.hover {
                    self.hover = hover;
                    self.redraw(cx);
                }
            }
        }
        if let Event::MouseDown(e) = event {
            if !self.open {
                return;
            }
            if contains(self.close, e.abs) {
                self.open = false;
                log!("wm: glance panel closed (its close button)");
                self.redraw(cx);
                return;
            }
            if contains(self.clear_all, e.abs) {
                let keys: Vec<String> = crate::glance::shown().iter().map(|c| c.key()).collect();
                for key in &keys {
                    crate::glance::dismiss(key);
                }
                log!("wm: glance panel cleared {} card(s)", keys.len());
                self.first = 0;
                self.redraw(cx);
                return;
            }
            // The hovered card's actions.
            let hovered = self.card_rects.iter().find(|(.., key)| self.hover.as_deref() == Some(key.as_str())).cloned();
            if let Some((r, app, route, key)) = hovered {
                if contains(crate::glance_card::open_button(r), e.abs) {
                    cx.widget_action(self.uid, ShellGlancePanelAction::Open { app, route });
                    return;
                }
                if contains(crate::glance_card::dismiss_button(r), e.abs) {
                    if crate::glance::dismiss(&key) {
                        log!("wm: glance card {key} dismissed");
                    }
                    self.hover = None;
                    self.redraw(cx);
                    return;
                }
            }
            if !contains(self.column, e.abs) {
                self.open = false;
                log!("wm: glance panel closed (a press outside it)");
                self.redraw(cx);
                return;
            }
        }
        // The cards' own input and answers (glance_card.rs). A closed panel
        // has swept its tiles; pointer events reach them only while open.
        if self.open || !event.requires_visibility() {
            // A press its card's own controls do not claim (the claim shows
            // across this dispatch) opens the card on its release.
            let claim_before = event.pointer_claimed_area();
            self.tiles.handle_event(cx, event);
            match event {
                Event::MouseDown(e) => {
                    let claimed = event.pointer_claimed_area() != claim_before;
                    self.card_press = if claimed { None } else { self.card_at(e.abs) };
                }
                Event::MouseUp(e) => {
                    if let Some(key) = self.card_press.take() {
                        if self.card_at(e.abs).as_deref() == Some(key.as_str()) {
                            log!("wm: glance card {key} opens in the card window");
                            cx.widget_action(self.uid, ShellGlancePanelAction::OpenCard { key });
                        }
                    }
                }
                _ => {}
            }
            // Then the L0 taps those tiles queued, as the card window runs
            // its card's, and the cards whose chat moved (a reply came).
            if self.live.dispatch(cx, &self.tiles, "glance panel") {
                self.redraw(cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new card is brought on screen scrolling as little as that takes:
    /// from the top when everything up to it fits, else as far as needed.
    #[test]
    fn a_new_card_scrolls_the_list_no_further_than_shows_it() {
        let heights = [300.0, 200.0, 250.0];
        assert_eq!(first_to_reveal(&heights, 1, 800.0), 0, "the first two fit: the list stays at the top");
        // All three take 750 + two gaps (24) + the "more below" line (20).
        assert_eq!(first_to_reveal(&heights, 2, 794.0), 0, "all three fit exactly");
        assert_eq!(first_to_reveal(&heights, 2, 780.0), 1, "all three do not: one card scrolls away");
        assert_eq!(first_to_reveal(&heights, 2, 200.0), 2, "the card alone does not fit: it heads the list");
        assert_eq!(first_to_reveal(&heights, 9, 2000.0), 0, "an index past the end is the last card");
    }
}
