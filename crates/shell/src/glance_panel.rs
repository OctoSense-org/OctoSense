//! The desktop's glance panel: the cards apps published to the glance
//! screen (glance.rs), in a column at the right edge of the desk, over the
//! windows, the way the phone shows them on its glance page.
//!
//! F9 (or `--test-action glance`) shows and hides it. Clicking a card opens
//! the app that published it; clicking outside the column closes it. The
//! column is [`PANEL_WIDTH`] wide; each card is a tile of the column's inner
//! width at its measured height (glance_card.rs), in the glance order
//! (priority, then recency).
use crate::glance_card::GlanceTiles;
use crate::shell::ui::{contains, rect, DrawShellFill, HAlign, ShellDraw};
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
    #[rust]
    card_rects: Vec<(Rect, String, Option<String>)>,
    /// What the last layout log said, so it is logged once per change.
    #[rust]
    logged: String,
    #[rust]
    tiles: GlanceTiles,
    #[rust]
    area: Area,
}

impl ShellGlancePanel {
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
        self.card_rects.iter().map(|(r, app, _)| (*r, app.clone())).collect()
    }
}

impl Widget for ShellGlancePanel {
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let screen = cx.turtle().rect();
        self.card_rects.clear();
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
            self.d.label_elided(cx, rect(x + PAD, top + 14.0, PANEL_WIDTH - PAD * 2.0, 24.0), true, 18.0, ink, HAlign::Left, "At a glance");
            let cards = crate::glance::shown();
            let status = if cards.is_empty() { "Nothing published yet".to_string() } else { format!("{} from your apps", cards.len()) };
            self.d.label_elided(cx, rect(x + PAD, top + 38.0, PANEL_WIDTH - PAD * 2.0, 18.0), false, 12.0, alpha(ink, 0.65), HAlign::Left, &status);
            let mut y = top + HEADER;
            let bottom = column.pos.y + column.size.y - PAD;
            for card in &cards {
                let key = card.key();
                let h = crate::glance_card::tile_height(&key);
                if y + h > bottom {
                    break;
                }
                let r = rect(x + PAD, y, PANEL_WIDTH - PAD * 2.0, h);
                self.tiles.draw(cx, &key, &card.body, r);
                self.card_rects.push((r, card.open_app.clone(), card.route.clone()));
                y += h + GAP;
            }
        }
        self.d.end_surface(cx);
        // Where the cards landed, once per change: evidence for a remote run.
        let layout: Vec<String> = self.card_rects.iter().map(|(r, app, _)| format!("{app}@{},{},{},{}", r.pos.x as i32, r.pos.y as i32, r.size.x as i32, r.size.y as i32)).collect();
        let layout = layout.join(" ");
        if self.open && layout != self.logged {
            log!("glance panel: {} card(s) {}", self.card_rects.len(), layout);
            self.logged = layout;
        }
        let live: Vec<String> = if self.open { crate::glance::shown().iter().map(|c| c.key()).collect() } else { Vec::new() };
        self.tiles.sweep(cx, &live);
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if !self.open {
            return;
        }
        if let Event::MouseDown(e) = event {
            if let Some((_, app, route)) = self.card_rects.iter().find(|(r, _, _)| contains(*r, e.abs)).cloned() {
                cx.widget_action(self.uid, ShellGlancePanelAction::Open { app, route });
            } else if !contains(self.column, e.abs) {
                self.open = false;
                self.redraw(cx);
            }
        }
    }
}
