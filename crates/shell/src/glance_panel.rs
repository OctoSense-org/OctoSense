//! The desktop's glance panel: the cards apps published to the glance
//! screen (glance.rs), in a column at the right edge of the desk, over the
//! windows, the way the phone shows them on its glance page.
//!
//! F9 (or `--test-action glance`) shows and hides it. Each card is live: the
//! panel hands it the pointer inside its tile and every other event
//! (typing, focus, timers, answers), so it works as it does in its app
//! (glance_card.rs). The open button at a card's top-right corner opens the
//! app that published it, the close button left of it dismisses the card;
//! the header's close button, or a click outside the column, closes the
//! panel. While it is open, toasts stack left of it. The
//! column is [`PANEL_WIDTH`] wide; each card is a tile of the column's inner
//! width at its measured height (glance_card.rs), in the glance order
//! (priority, then recency).
use crate::glance_card::{GlanceTiles, LiveCards};
use crate::shell::ui::{contains, rect, DrawShellFill, HAlign, Ico, ShellDraw};
use crate::shell::{alpha, MaterialTokens, ShellTokens};
use makepad_widgets::*;

pub const PANEL_WIDTH: f64 = 360.0;
const PAD: f64 = 16.0;
const GAP: f64 = 12.0;
const HEADER: f64 = 64.0;

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
    /// A card was clicked: open this launcher id (and route, when given).
    Open { app: String, route: Option<String> },
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
    /// The header's close button.
    #[rust]
    close: Rect,
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
    /// Show the newest card first (a new card opened the panel).
    pub fn show_newest(&mut self) {
        let cards = crate::glance::shown();
        self.first = cards.iter().enumerate().max_by_key(|(_, c)| c.published_ms).map_or(0, |(i, _)| i);
        self.wheel = 0.0;
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
        if self.open {
            let tok = self.d.tokens(self.tokens);
            let top = screen.pos.y + self.bar_clearance + tok.spacing.gaps_out;
            let x = screen.pos.x + screen.size.x - tok.spacing.gaps_out - PANEL_WIDTH;
            let column = rect(x, top, PANEL_WIDTH, (screen.pos.y + screen.size.y - tok.spacing.gaps_out - top).max(HEADER));
            self.column = column;
            self.d.card(cx, column, &tok.notifications.surface);
            let ink = tok.notifications.surface.text;
            self.d.label_elided(cx, rect(x + PAD, top + 14.0, PANEL_WIDTH - PAD * 2.0 - 32.0, 24.0), true, 18.0, ink, HAlign::Left, "At a glance");
            self.close = rect(x + PANEL_WIDTH - PAD - 28.0, top + 12.0, 28.0, 28.0);
            self.d.icon_centered(cx, Ico::Close, self.close, 14.0, alpha(ink, 0.8));
            let cards = crate::glance::shown();
            self.first = self.first.min(cards.len().saturating_sub(1));
            let status = if cards.is_empty() { "Nothing published yet".to_string() } else { format!("{} from your apps", cards.len()) };
            self.d.label_elided(cx, rect(x + PAD, top + 38.0, PANEL_WIDTH - PAD * 2.0, 18.0), false, 12.0, alpha(ink, 0.65), HAlign::Left, &status);
            let mut y = top + HEADER;
            let bottom = column.pos.y + column.size.y - PAD - 20.0;
            let mut drawn = 0;
            for card in cards.iter().skip(self.first) {
                let key = card.key();
                let h = crate::glance_card::tile_height(&key);
                // The first card shown always draws (clipped by its cap).
                if drawn > 0 && y + h > bottom {
                    break;
                }
                drawn += 1;
                let r = rect(x + PAD, y, PANEL_WIDTH - PAD * 2.0, h);
                let body = self.live.body(&key, card, "glance panel");
                self.tiles.draw(cx, &key, &card.app, card.contained, &body, r);
                let open = crate::glance_card::open_button(r);
                self.d.card(cx, open, &tok.notifications.surface);
                self.d.icon_centered(cx, Ico::ChevronRight, open, 14.0, ink);
                let dismiss = crate::glance_card::dismiss_button(r);
                self.d.card(cx, dismiss, &tok.notifications.surface);
                self.d.icon_centered(cx, Ico::Close, dismiss, 12.0, ink);
                self.card_rects.push((r, card.open_app.clone(), card.route.clone(), key));
                y += h + GAP;
            }
            // The cards the column does not show: the wheel brings them.
            self.below = cards.len().saturating_sub(self.first + drawn);
            let more = match (self.first, self.below) {
                (0, 0) => String::new(),
                (0, n) => format!("{n} more below \u{2193}"),
                (n, 0) => format!("\u{2191} {n} more above"),
                (a, b) => format!("\u{2191} {a} above \u{00b7} {b} below \u{2193}"),
            };
            if !more.is_empty() {
                self.d.label_elided(cx, rect(x + PAD, column.pos.y + column.size.y - PAD - 16.0, PANEL_WIDTH - PAD * 2.0, 16.0), false, 12.0, alpha(ink, 0.65), HAlign::Center, &more);
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
            if let Some((_, app, route, _)) = self.card_rects.iter().find(|(r, ..)| contains(crate::glance_card::open_button(*r), e.abs)).cloned() {
                cx.widget_action(self.uid, ShellGlancePanelAction::Open { app, route });
                return;
            } else if let Some((.., key)) = self.card_rects.iter().find(|(r, ..)| contains(crate::glance_card::dismiss_button(*r), e.abs)).cloned() {
                if crate::glance::dismiss(&key) {
                    log!("wm: glance card {key} dismissed");
                }
                self.redraw(cx);
                return;
            } else if !contains(self.column, e.abs) {
                self.open = false;
                log!("wm: glance panel closed (a press outside it)");
                self.redraw(cx);
                return;
            }
        }
        // The cards' own input and answers (glance_card.rs). A closed panel
        // has swept its tiles; pointer events reach them only while open.
        if self.open || !event.requires_visibility() {
            self.tiles.handle_event(cx, event);
            // Then the L0 taps those tiles queued, as the card window runs
            // its card's, and the cards whose chat moved (a reply came).
            if self.live.dispatch(cx, &self.tiles, "glance panel") {
                self.redraw(cx);
            }
        }
    }
}
