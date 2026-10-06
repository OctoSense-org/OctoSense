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
//!   ([`ShellGlancePanelAction::OpenCard`]), as its notification does. A
//!   field claims its press; a chip (a tap target, `OctoscriptTap`) claims
//!   none and calls its card once the release's handlers have run, so a
//!   release is held for a frame: a click from that card means its control
//!   had it.
//! - **The hovered card** shows its actions over its top-right corner, on a
//!   backdrop: open the app that published it, and dismiss the card. They
//!   are not drawn over every card's content all the time.
//! - **The header** says how many cards there are and has Clear all and the
//!   panel's close; a click outside the column closes it too. With no card
//!   it says so ("You're all caught up").
//! - **The list** starts at the top and the wheel moves it a card at a time;
//!   a new card scrolls it no further than shows that card
//!   ([`first_to_reveal`]). "More above" heads the list and "more below"
//!   ends it; the card that does not fit whole peeks in above that line,
//!   cut at the list's end.
//! - **A card taller than its tile** (the cap, glance_card.rs) scrolls
//!   inside it ([`TileScroll`]), with a thumb at its edge. A card with an
//!   in-card chat (`sys.chat`) sits at its newest rows and follows them as
//!   the conversation grows, so its field and the latest exchange stay in
//!   view; any other card starts at its top. The wheel over such a card
//!   scrolls it first and moves the list only at its ends. The tile's own
//!   layout scrolls, so a press the card's controls do not claim still
//!   opens the card window.
//! - **A dismissal** (a card's close, Delete, Clear all) can be undone: the
//!   shell's toast offers Undo ([`ShellGlancePanelAction::Dismissed`]), and
//!   ⌘Z does it while the panel holds the keyboard.
//! - **The keyboard** ([`ShellGlancePanel::key`]): opened by the person, the
//!   panel holds it. The arrows move a focus ring from card to card,
//!   scrolling as they go, Return opens the card, Delete dismisses it and
//!   Esc closes the panel. Opened by a new card it does not take the
//!   keyboard from what the person was typing in; a press in it does, and
//!   a press on a card's own control hands it to that control.
//!
//! It slides in from the right as it opens ([`crate::shell::ui::slide`]),
//! and its column stops above the dock. While it is open, toasts stack left
//! of it. The column is [`PANEL_WIDTH`]
//! wide, as wide as a toast; each card is a tile of the column's inner width
//! at its measured height (glance_card.rs), in the glance order (priority,
//! then recency).
use crate::glance_card::{GlanceTiles, LiveCards};
use crate::shell::ui::{contains, rect, slide, DrawShellFill, HAlign, Ico, ShellDraw};
use crate::shell::{alpha, MaterialTokens, ShellTokens};
use makepad_widgets::*;
use std::collections::HashMap;

pub const PANEL_WIDTH: f64 = 380.0;
const PAD: f64 = 16.0;
const GAP: f64 = 12.0;
const HEADER: f64 = 64.0;
/// The line "more above" or "more below" takes.
const HINT_H: f64 = 20.0;
/// How long a card that just came wears its accent mark (its toast says
/// the same; the mark shows which card it was).
const NEW_FOR_MS: u64 = 8_000;
/// The card that does not fit under the others peeks in, cut at the list's
/// end, when at least this much of it shows (else the list ends there).
const PEEK_MIN: f64 = 72.0;
/// The column slides in from the right as it opens.
const SLIDE_IN_S: f64 = 0.24;
const SLIDE_IN_PX: f64 = 40.0;
/// The keyboard's focus ring: this far outside its card's tile, round with
/// the tile's corners (the L0 kit's 12 px).
const RING_GAP: f64 = 3.0;
const TILE_RADIUS: f64 = 12.0;

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

/// Where an arrow key moves the keyboard's focus among `len` cards: from
/// the card at `at` (none yet: the first shown, `first`) one down or up,
/// stopping at the ends.
pub fn step_focus(len: usize, at: Option<usize>, first: usize, down: bool) -> usize {
    let last = len.saturating_sub(1);
    match at {
        None => first.min(last),
        Some(i) if down => (i + 1).min(last),
        Some(i) => i.saturating_sub(1).min(last),
    }
}

/// Where the focus goes when the card at `i` of `keys` is dismissed: the
/// card under it takes its place, or the one above when it was the last.
pub fn focus_after_dismiss(keys: &[String], i: usize) -> Option<String> {
    keys.get(i + 1).or(i.checked_sub(1).and_then(|j| keys.get(j))).cloned()
}

/// Where a card taller than its tile sits in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TileScroll {
    /// At its newest rows, its bottom, followed as the card grows: where a
    /// card with an in-card chat starts, so its field and the latest
    /// exchange stay in view.
    Newest,
    /// This far down from its top: where any other card starts (at 0), and
    /// where the wheel leaves a chat scrolled back from its newest rows.
    Top(f64),
}

impl TileScroll {
    /// Where a card starts: one with a chat at its newest rows, any other at
    /// its top.
    pub fn start(chat: bool) -> Self {
        if chat { TileScroll::Newest } else { TileScroll::Top(0.0) }
    }

    /// How far up the card is scrolled in a tile it overflows by `overflow`.
    pub fn offset(self, overflow: f64) -> f64 {
        let overflow = overflow.max(0.0);
        match self {
            TileScroll::Newest => overflow,
            TileScroll::Top(down) => down.clamp(0.0, overflow),
        }
    }

    /// The wheel's `dy` (positive: further down the card) over a card that
    /// overflows its tile by `overflow`: it scrolls as far as it goes, and a
    /// chat back at its bottom follows its newest rows again. False when it
    /// was at that end already: the wheel is the list's.
    pub fn wheel(&mut self, dy: f64, overflow: f64, chat: bool) -> bool {
        let overflow = overflow.max(0.0);
        let at = self.offset(overflow);
        let to = (at + dy).clamp(0.0, overflow);
        if (to - at).abs() < 0.5 {
            return false;
        }
        *self = if chat && to >= overflow - 0.5 { TileScroll::Newest } else { TileScroll::Top(to) };
        true
    }
}

/// A tall card's place in its tile, as the panel keeps it while the card is
/// drawn tall: the publish it is for (a newer one starts over), whether the
/// card has a chat, and where it sits.
#[derive(Clone, Copy, Debug)]
struct TallCard {
    published_ms: u64,
    chat: bool,
    at: TileScroll,
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
    /// The person dismissed cards (a card's close, Delete, Clear all): the
    /// shell offers to undo it.
    Dismissed { count: usize },
    /// ⌘Z while the panel holds the keyboard: the last dismissed come back.
    Undo,
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
    /// That press released on its card: it opens the card window on the next
    /// frame unless the card clicked by then (one of its chips had it).
    #[rust]
    card_release: Option<String>,
    #[rust]
    release_frame: NextFrame,
    /// A new card came: the next draw scrolls as little as shows it.
    #[rust]
    reveal_newest: bool,
    /// When the newest mark goes (a redraw drops it).
    #[rust]
    new_timer: Timer,
    /// The panel holds the keyboard ([`Self::key`]).
    #[rust]
    pub keyboard: bool,
    /// The card the keyboard is on (by key; its ring shows), and the card
    /// the next draw scrolls to (the keyboard moved onto it).
    #[rust]
    focus: Option<String>,
    #[rust]
    reveal: Option<String>,
    /// Opening for a new card ([`Self::show_newest`]): it does not take the
    /// keyboard.
    #[rust]
    quiet: bool,
    /// The slide in: due on the next draw, when it started, its frames.
    #[rust]
    opening: bool,
    #[rust]
    opened_at: f64,
    #[rust]
    slide_frame: NextFrame,
    #[rust]
    sliding: bool,
    /// What the last layout log said, so it is logged once per change.
    #[rust]
    logged: String,
    /// The cards taller than their tiles: where each sits (by key), the ones
    /// this frame drew with how far each overflows, and what the last log of
    /// their places said.
    #[rust]
    tall: HashMap<String, TallCard>,
    #[rust]
    tall_drawn: Vec<(String, f64)>,
    #[rust]
    tall_logged: String,
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
    /// scrolling as little as that takes ([`first_to_reveal`]). Opening so,
    /// the panel leaves the keyboard where it was.
    pub fn show_newest(&mut self) {
        self.reveal_newest = true;
        self.wheel = 0.0;
        self.quiet = true;
    }
    /// Open or close it. Opened by the person it holds the keyboard; opened
    /// for a new card ([`Self::show_newest`]) it does not.
    pub fn set_open(&mut self, open: bool) {
        if open && !self.open {
            self.opening = true;
        }
        self.open = open;
        self.keyboard = open && !std::mem::take(&mut self.quiet);
        self.focus = None;
    }
    fn close(&mut self, cx: &mut Cx, why: &str) {
        self.set_open(false);
        log!("wm: glance panel closed ({why})");
        self.redraw(cx);
    }
    /// The cards an undo brought back: the keyboard's focus goes to the
    /// first, when the panel holds the keyboard.
    pub fn focus_restored(&mut self, keys: &[String]) {
        if self.open && self.keyboard {
            if let Some(key) = keys.first() {
                self.focus = Some(key.clone());
                self.reveal = self.focus.clone();
            }
        }
    }
    /// Put the keyboard's focus on the first card shown (F9 opened the
    /// panel: the keyboard is already in use).
    pub fn focus_first(&mut self) {
        self.focus = crate::glance::listed().get(self.first).map(|c| c.key());
    }
    /// A key while the panel holds the keyboard: what it asks of the shell
    /// (`Some(None)` when it was the panel's alone), or `None` when it is not
    /// the panel's, for the shell to route on. The arrows move the focus,
    /// Return opens the focused card, Delete (or Backspace) dismisses it, ⌘Z
    /// brings the last dismissed back, Esc closes the panel.
    pub fn key(&mut self, cx: &mut Cx, e: &KeyEvent) -> Option<ShellGlancePanelAction> {
        if !self.open || !self.keyboard {
            return None;
        }
        let m = &e.modifiers;
        let plain = !m.logo && !m.control && !m.alt;
        let keys: Vec<String> = crate::glance::listed().iter().map(|c| c.key()).collect();
        let at = self.focus.as_ref().and_then(|f| keys.iter().position(|k| k == f));
        let act = match e.key_code {
            KeyCode::Escape if plain => {
                self.close(cx, "Esc");
                ShellGlancePanelAction::None
            }
            KeyCode::ArrowDown | KeyCode::ArrowUp if plain && !keys.is_empty() => {
                let n = step_focus(keys.len(), at, self.first, e.key_code == KeyCode::ArrowDown);
                self.focus = Some(keys[n].clone());
                self.reveal = self.focus.clone();
                ShellGlancePanelAction::None
            }
            KeyCode::ReturnKey | KeyCode::NumpadEnter if plain && at.is_some() => {
                let key = keys[at.unwrap()].clone();
                log!("wm: glance card {key} opens in the card window (Return)");
                ShellGlancePanelAction::OpenCard { key }
            }
            KeyCode::Delete | KeyCode::Backspace if plain && at.is_some() => {
                let i = at.unwrap();
                let count = crate::glance::dismiss_all(&[keys[i].clone()]);
                log!("wm: glance card {} dismissed (Delete)", keys[i]);
                self.focus = focus_after_dismiss(&keys, i);
                self.reveal = self.focus.clone();
                ShellGlancePanelAction::Dismissed { count }
            }
            KeyCode::KeyZ if (m.logo || m.control) && !m.shift && !m.alt => ShellGlancePanelAction::Undo,
            _ => return None,
        };
        self.redraw(cx);
        Some(act)
    }
    /// The card at `p`, by key.
    fn card_at(&self, p: DVec2) -> Option<String> {
        self.card_rects.iter().find(|(r, ..)| contains(*r, p)).map(|(.., key)| key.clone())
    }
    /// How far up the tall card `card` is scrolled in its tile, which it
    /// overflows by `overflow`: where it sits, kept from frame to frame for
    /// its publish (first seen: a card with a chat at its newest rows, any
    /// other at its top), and noted as drawn for the wheel.
    fn tall_scroll(&mut self, key: &str, card: &crate::glance::GlanceCard, overflow: f64) -> f64 {
        let fresh = || {
            let chat = card.l0.as_ref().is_some_and(|l0| crate::glance_chat::reads_chat(&l0.source));
            TallCard { published_ms: card.published_ms, chat, at: TileScroll::start(chat) }
        };
        let tall = self.tall.entry(key.to_string()).or_insert_with(fresh);
        if tall.published_ms != card.published_ms {
            *tall = fresh();
        }
        let at = tall.at.offset(overflow);
        self.tall_drawn.push((key.to_string(), overflow));
        at
    }
    /// The wheel's `dy` at `p`: a tall card there scrolls first. False when
    /// none is there, or it is at its end that way: the wheel is the list's.
    fn scroll_tall(&mut self, p: DVec2, dy: f64) -> bool {
        let Some(key) = self.card_at(p) else { return false };
        let Some(overflow) = self.tall_drawn.iter().find(|(k, _)| *k == key).map(|(_, o)| *o) else { return false };
        let Some(tall) = self.tall.get_mut(&key) else { return false };
        let chat = tall.chat;
        tall.at.wheel(dy, overflow, chat)
    }
    pub fn toggle(&mut self, cx: &mut Cx) {
        self.set_open(!self.open);
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
        self.tall_drawn.clear();
        self.close = Rect::default();
        // Every frame, open or closed: the kit keeps its overlay in tree order.
        self.d.begin_surface(cx);
        self.clear_all = Rect::default();
        if self.open {
            let tok = self.d.tokens(self.tokens);
            // Sliding in, from the right.
            let now_s = cx.seconds_since_app_start();
            if std::mem::take(&mut self.opening) {
                self.opened_at = now_s;
                self.slide_frame = cx.new_next_frame();
            }
            let top = screen.pos.y + self.bar_clearance + tok.spacing.gaps_out;
            let dx = slide((now_s - self.opened_at) / SLIDE_IN_S, SLIDE_IN_PX);
            self.sliding = dx > 0.0;
            let x = screen.pos.x + screen.size.x - tok.spacing.gaps_out - PANEL_WIDTH + dx;
            // The column ends above the dock when the dock is under it.
            let mut end = screen.pos.y + screen.size.y - tok.spacing.gaps_out;
            let shelf = crate::desktop::shelf_bounds();
            if shelf.size.y > 0.0 && shelf.pos.y > top && shelf.pos.x < x + PANEL_WIDTH && shelf.pos.x + shelf.size.x > x {
                end = end.min(shelf.pos.y - tok.spacing.gaps_out);
            }
            let column = rect(x, top, PANEL_WIDTH, (end - top).max(HEADER));
            self.column = column;
            self.d.card(cx, column, &tok.notifications.surface);
            let ink = tok.notifications.surface.text;
            // Secondary text, faded toward the column: 4.5:1 or more on a
            // flat one, light or dark.
            let dim = alpha(ink, 0.72);
            let accent = tok.notifications.countdown;
            let cards = crate::glance::listed();
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
                // The keyboard moved onto a card: scroll as little as shows it.
                if let Some(n) = self.reveal.take().and_then(|key| cards.iter().position(|c| c.key() == key)) {
                    self.first = if n < self.first { n } else { self.first.max(first_to_reveal(&heights, n, bottom - list_top)) };
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
                    let mut h = heights[i];
                    // The first card shown always draws (clipped by its cap).
                    // The one that does not fit under the others peeks in,
                    // cut at the list's end (its tile clips), so the list
                    // reads as going on; it still counts as below.
                    let cut = drawn > 0 && y + h > limit;
                    if cut {
                        if limit - y < PEEK_MIN {
                            break;
                        }
                        h = limit - y;
                    } else {
                        drawn += 1;
                    }
                    let r = rect(x + PAD, y, PANEL_WIDTH - PAD * 2.0, h);
                    let body = self.live.body(&key, card, "glance panel");
                    // A card taller than its tile scrolls inside it (one that
                    // peeks in shows its top), with a thumb at its edge, under
                    // the card's actions.
                    let overflow = if cut { 0.0 } else { crate::glance_card::overflow(&key) };
                    let scroll = if overflow > 0.0 { self.tall_scroll(&key, card, overflow) } else { 0.0 };
                    self.tiles.draw_scrolled(cx, &key, &card.app, card.contained, &body, r, scroll);
                    if overflow > 0.0 {
                        let track = rect(r.pos.x + r.size.x - 6.0, r.pos.y + 40.0, 3.0, (r.size.y - 50.0).max(12.0));
                        let thumb = (track.size.y * r.size.y / (r.size.y + overflow)).clamp(12.0, track.size.y);
                        self.d.solid(cx, rect(track.pos.x, track.pos.y + (track.size.y - thumb) * scroll / overflow, track.size.x, thumb), alpha(ink, 0.35));
                    }
                    // A card that just came: an accent mark in the gutter
                    // beside it for a few seconds, the one its toast is about.
                    let age = now.saturating_sub(card.published_ms);
                    if age < NEW_FOR_MS {
                        self.d.solid(cx, rect(r.pos.x - 9.0, r.pos.y + 14.0, 3.0, (r.size.y - 28.0).max(8.0)), accent);
                        let left = NEW_FOR_MS - age;
                        mark_left = Some(mark_left.map_or(left, |m| m.min(left)));
                    }
                    // The keyboard's card: a ring round it, and its actions.
                    let focused = self.keyboard && self.focus.as_deref() == Some(key.as_str());
                    if focused {
                        let ring = rect(r.pos.x - RING_GAP, r.pos.y - RING_GAP, r.size.x + RING_GAP * 2.0, r.size.y + RING_GAP * 2.0);
                        self.d.focus_ring(cx, ring, TILE_RADIUS + RING_GAP, 2.0, accent);
                    }
                    // The hovered card's actions, on a backdrop so they read
                    // over its content: dismiss, and open its app.
                    if focused || self.hover.as_deref() == Some(key.as_str()) {
                        let open = crate::glance_card::open_button(r);
                        let dismiss = crate::glance_card::dismiss_button(r);
                        let backdrop = rect(dismiss.pos.x - 4.0, dismiss.pos.y - 4.0, open.pos.x + open.size.x - dismiss.pos.x + 8.0, open.size.y + 8.0);
                        self.d.card(cx, backdrop, &tok.notifications.surface);
                        self.d.icon_centered(cx, Ico::Close, dismiss, 12.0, ink);
                        self.d.icon_centered(cx, Ico::ChevronRight, open, 14.0, ink);
                    }
                    self.card_rects.push((r, card.open_app.clone(), card.route.clone(), key));
                    y += h + GAP;
                    if cut {
                        break;
                    }
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
        // Where the cards landed, once per change and once the slide is
        // done: evidence for a remote run.
        let layout: Vec<String> = self.card_rects.iter().map(|(r, app, _, _)| format!("{app}@{},{},{},{}", r.pos.x as i32, r.pos.y as i32, r.size.x as i32, r.size.y as i32)).collect();
        let layout = layout.join(" ");
        if self.open && !self.sliding && layout != self.logged {
            log!("glance panel: {} card(s) {}", self.card_rects.len(), layout);
            self.logged = layout;
        }
        // Where the tall cards sit, once per change: evidence for a remote run.
        let places: Vec<String> = self
            .tall_drawn
            .iter()
            .filter_map(|(key, overflow)| {
                let tall = self.tall.get(key)?;
                let newest = if tall.at == TileScroll::Newest { " newest" } else { "" };
                Some(format!("{key} {:.0}/{:.0}{newest}", tall.at.offset(*overflow), overflow))
            })
            .collect();
        let places = places.join(", ");
        if self.open && !self.sliding && places != self.tall_logged {
            if !places.is_empty() {
                log!("glance panel: scrolled {places}");
            }
            self.tall_logged = places;
        }
        let live: Vec<String> = if self.open { crate::glance::listed().iter().map(|c| c.key()).collect() } else { Vec::new() };
        self.tiles.sweep(cx, &live);
        self.live.retain(&live);
        // A card that is not tall now (it fits, peeks in or is scrolled out
        // of the list) starts over when it is again.
        let tall_drawn = &self.tall_drawn;
        self.tall.retain(|key, _| tall_drawn.iter().any(|(k, _)| k == key));
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        // The slide in's frames.
        if self.slide_frame.is_event(event).is_some() {
            if cx.seconds_since_app_start() - self.opened_at < SLIDE_IN_S {
                self.slide_frame = cx.new_next_frame();
            }
            self.redraw(cx);
        }
        // A newest mark's time is up.
        if self.new_timer.is_event(event).is_some() {
            self.new_timer = Timer::empty();
            self.redraw(cx);
        }
        // The wheel over the column: a card at a time, after a tall card
        // under it has scrolled to its end that way.
        if let Event::Scroll(e) = event {
            if self.open && contains(self.column, e.abs) {
                if self.scroll_tall(e.abs, e.scroll.y) {
                    self.wheel = 0.0;
                    self.redraw(cx);
                    return;
                }
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
                self.close(cx, "its close button");
                return;
            }
            if contains(self.clear_all, e.abs) {
                let keys: Vec<String> = crate::glance::listed().iter().map(|c| c.key()).collect();
                let count = crate::glance::dismiss_all(&keys);
                log!("wm: glance panel cleared {count} card(s)");
                self.first = 0;
                self.focus = None;
                cx.widget_action(self.uid, ShellGlancePanelAction::Dismissed { count });
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
                    let count = crate::glance::dismiss_all(&[key.clone()]);
                    if count > 0 {
                        log!("wm: glance card {key} dismissed");
                        cx.widget_action(self.uid, ShellGlancePanelAction::Dismissed { count });
                    }
                    self.hover = None;
                    self.redraw(cx);
                    return;
                }
            }
            if !contains(self.column, e.abs) {
                self.close(cx, "a press outside it");
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
                    // A press in the column takes the keyboard, unless a
                    // card's own control claimed it (its field types then).
                    // The pointer hides the focus ring until a key brings it.
                    if self.open && contains(self.column, e.abs) {
                        self.keyboard = !claimed;
                        self.focus = None;
                    }
                }
                Event::MouseUp(e) => {
                    if let Some(key) = self.card_press.take() {
                        if self.card_at(e.abs).as_deref() == Some(key.as_str()) {
                            // A chip claims no press and clicks once this
                            // release's handlers have run: wait a frame.
                            self.card_release = Some(key);
                            self.release_frame = cx.new_next_frame();
                        }
                    }
                }
                _ => {}
            }
            // The released card clicked: the press was one of its chips',
            // not the card's. Seen before its taps run, on any event after
            // the release's.
            if let Some(key) = self.card_release.as_deref() {
                if self.tiles.heap_key(cx, key).is_some_and(crate::glance_card::has_clicks) {
                    self.card_release = None;
                }
            }
            if self.release_frame.is_event(event).is_some() {
                if let Some(key) = self.card_release.take() {
                    log!("wm: glance card {key} opens in the card window");
                    cx.widget_action(self.uid, ShellGlancePanelAction::OpenCard { key });
                }
            }
            // Then the L0 taps those tiles queued, as the card window runs
            // its card's, and the cards whose chat moved (a reply came).
            if self.live.dispatch(cx, &self.tiles, "glance panel") {
                self.redraw(cx);
            }
            if let Some((app, route)) = self.live.take_open_request() {
                cx.widget_action(self.uid, ShellGlancePanelAction::Open { app, route });
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

    /// The arrows move the focus a card at a time and stop at the ends; the
    /// first press lands on the first card shown.
    #[test]
    fn the_arrows_step_through_the_cards_and_stop_at_the_ends() {
        assert_eq!(step_focus(5, None, 2, true), 2, "first press: the first card shown");
        assert_eq!(step_focus(5, None, 2, false), 2);
        assert_eq!(step_focus(5, Some(2), 0, true), 3);
        assert_eq!(step_focus(5, Some(2), 0, false), 1);
        assert_eq!(step_focus(5, Some(4), 0, true), 4, "the last card stays");
        assert_eq!(step_focus(5, Some(0), 0, false), 0, "the first card stays");
        assert_eq!(step_focus(3, None, 7, true), 2, "a stale first is clamped");
    }

    /// A chat card past its tile's cap (after the first exchange) sits at
    /// its newest rows, so its field and the latest exchange show, and stays
    /// there as the conversation grows; any other card shows its top.
    #[test]
    fn a_chat_card_taller_than_its_tile_shows_its_newest_rows() {
        let chat = TileScroll::start(true);
        assert_eq!(chat.offset(80.0), 80.0, "scrolled up by all it overflows: its bottom shows");
        assert_eq!(chat.offset(140.0), 140.0, "a reply came: still at the newest rows");
        assert_eq!(chat.offset(0.0), 0.0, "it fits: nothing to scroll");
        let other = TileScroll::start(false);
        assert_eq!((other.offset(80.0), other.offset(140.0)), (0.0, 0.0), "its top, as before");
    }

    /// The wheel over a tall card scrolls it as far as it goes and is the
    /// list's at the card's ends; a chat scrolled back to its bottom follows
    /// its newest rows again, and one scrolled up stays where it was read.
    #[test]
    fn the_wheel_scrolls_a_tall_card_and_leaves_the_list_its_ends() {
        let mut at = TileScroll::start(true);
        assert!(!at.wheel(40.0, 80.0, true), "at the newest rows already: the list's");
        assert!(at.wheel(-30.0, 80.0, true));
        assert_eq!(at, TileScroll::Top(50.0));
        assert_eq!(at.offset(120.0), 50.0, "a reply while it was scrolled back: the rows read stay put");
        assert!(at.wheel(-100.0, 80.0, true));
        assert_eq!(at, TileScroll::Top(0.0));
        assert!(!at.wheel(-10.0, 80.0, true), "at its top: the list's");
        assert!(at.wheel(500.0, 80.0, true));
        assert_eq!(at, TileScroll::Newest, "back at the bottom: it follows the newest rows");
        let mut other = TileScroll::start(false);
        assert!(!other.wheel(-10.0, 80.0, false));
        assert!(other.wheel(30.0, 80.0, false));
        assert!(other.wheel(500.0, 80.0, false));
        assert_eq!(other, TileScroll::Top(80.0), "a card with no chat does not follow its bottom");
    }

    /// A dismissed card's focus passes to the card under it, or above it
    /// when it was the last, or to none when it was the only one.
    #[test]
    fn a_dismissed_cards_focus_passes_to_its_neighbour() {
        let keys: Vec<String> = ["a", "b", "c"].iter().map(|k| k.to_string()).collect();
        assert_eq!(focus_after_dismiss(&keys, 0).as_deref(), Some("b"));
        assert_eq!(focus_after_dismiss(&keys, 1).as_deref(), Some("c"));
        assert_eq!(focus_after_dismiss(&keys, 2).as_deref(), Some("b"), "the last: the one above");
        assert_eq!(focus_after_dismiss(&keys[..1], 0), None, "the only card: no focus");
    }
}
