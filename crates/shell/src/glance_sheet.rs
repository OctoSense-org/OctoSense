//! The card window: one published glance card (glance.rs), full size and
//! centred over a dimmed desk, App Clip style. Clicking a card's toast opens
//! it here (lib.rs, by the key the notification carries); ✕, Esc or a click
//! on the dimmed desk closes it.
//!
//! The card is [`SHEET_WIDTH`]×[`SHEET_HEIGHT`] (smaller on a small screen),
//! not clipped to the glance tile's 260 pt. It runs in its own isolate, under
//! the publishing app's policy, as a glance tile does (glance_card.rs).
//!
//! An L0 card is live here: its taps and field edits run through an
//! [`L0Session`] (the declared transition, the §5.12 writes this host
//! performs, a re-lowering), so a Reply opens its draft and a Send changes
//! the card. Its state lives as long as the window: closing it forgets it.
use crate::glance_card::{GlanceTiles, L0Session};
use crate::shell::ui::{contains, rect, DrawShellFill, HAlign, Ico, ShellDraw};
use crate::shell::{alpha, MaterialTokens, ShellTokens};
use makepad_widgets::*;
use std::sync::Arc;

pub const SHEET_WIDTH: f64 = 380.0;
pub const SHEET_HEIGHT: f64 = 520.0;
const HEADER: f64 = 44.0;
const PAD: f64 = 12.0;
const CLOSE: f64 = 28.0;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.ShellGlanceSheetBase = #(ShellGlanceSheet::register_widget(vm))
    mod.widgets.ShellGlanceSheet = set_type_default() do mod.widgets.ShellGlanceSheetBase {
        width: Fill
        height: Fill
        draw_bg +: {}
        d +: {}
    }
}

/// Where the window sits on a screen: centred, at most the App Clip size.
pub fn sheet_rect(screen: Rect) -> Rect {
    let w = SHEET_WIDTH.min(screen.size.x - 32.0).max(200.0);
    let h = SHEET_HEIGHT.min(screen.size.y - 32.0).max(200.0);
    rect(screen.pos.x + (screen.size.x - w) * 0.5, screen.pos.y + (screen.size.y - h) * 0.5, w, h)
}

/// The ✕ in a window at `sheet`.
pub fn close_rect(sheet: Rect) -> Rect {
    rect(sheet.pos.x + sheet.size.x - PAD - CLOSE + 4.0, sheet.pos.y + (HEADER - CLOSE) * 0.5 + 2.0, CLOSE, CLOSE)
}

/// The card's own area in a window at `sheet`.
pub fn card_rect(sheet: Rect) -> Rect {
    rect(sheet.pos.x + PAD, sheet.pos.y + HEADER, sheet.size.x - PAD * 2.0, sheet.size.y - HEADER - PAD)
}

/// The open card.
struct Open {
    key: String,
    app: String,
    title: String,
    contained: bool,
    body: Arc<str>,
    /// An L0 card's live state; `None` for a script card (which keeps its own).
    session: Option<L0Session>,
}

#[derive(Script, ScriptHook, Widget)]
pub struct ShellGlanceSheet {
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
    open: Option<Open>,
    #[rust]
    sheet: Rect,
    #[rust]
    tiles: GlanceTiles,
    /// What the last layout log said, so it is logged once per change.
    #[rust]
    logged: String,
    #[rust]
    area: Area,
}

impl ShellGlanceSheet {
    /// The key (`app/card_id`) of the open card.
    pub fn open_key(&self) -> Option<&str> {
        self.open.as_ref().map(|o| o.key.as_str())
    }

    /// Open the published card `key`. False when it is no longer published.
    pub fn open_card(&mut self, cx: &mut Cx, key: &str) -> bool {
        let Some(card) = crate::glance::card(key) else {
            return false;
        };
        let session = card.l0.as_deref().map(L0Session::new);
        let body: Arc<str> = match session.as_ref().map(L0Session::body) {
            Some(Ok(body)) => body.into(),
            Some(Err(e)) => {
                log!("glance sheet: {key} lowers as published only: {e}");
                card.body.clone()
            }
            None => card.body.clone(),
        };
        // A fresh isolate for each opening: the card starts as published.
        self.tiles.sweep(cx, &[]);
        self.open = Some(Open { key: key.to_string(), app: card.app.clone(), title: card.title.clone(), contained: card.contained, body, session });
        log!("glance sheet: opened {key}");
        self.redraw(cx);
        true
    }

    pub fn close(&mut self, cx: &mut Cx) {
        if let Some(open) = self.open.take() {
            log!("glance sheet: closed {}", open.key);
        }
        self.tiles.sweep(cx, &[]);
        self.logged.clear();
        self.redraw(cx);
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn set_material(&mut self, m: MaterialTokens, palette: Option<crate::shell::ShellPalette>) {
        self.d.set_material(m);
        self.d.set_palette(palette);
    }

    fn tile_key(key: &str) -> String {
        // Its own key: the panel's tile height for this card is not ours.
        format!("sheet:{key}")
    }

    /// Run the open card's queued taps through its L0 session.
    fn dispatch_taps(&mut self, cx: &mut Cx) {
        let Some(open) = self.open.as_mut() else { return };
        let tile = Self::tile_key(&open.key);
        let Some(heap) = self.tiles.heap_key(cx, &tile) else { return };
        let taps = crate::glance_card::take_taps(heap);
        let Some(session) = open.session.as_mut() else { return };
        let mut relower = false;
        for tap in taps {
            match session.tap(&tap.target, tap.typed.as_deref()) {
                Ok(outcome) => {
                    log!("glance sheet: {} tap {} (applied {}, relower {})", open.key, outcome.event, outcome.applied, outcome.relower);
                    relower |= outcome.relower;
                }
                Err(e) => log!("glance sheet: {} tap refused: {e}", open.key),
            }
        }
        if relower {
            match session.body() {
                Ok(body) => open.body = body.into(),
                Err(e) => log!("glance sheet: {} does not lower: {e}", open.key),
            }
            self.redraw(cx);
        }
    }
}

impl Widget for ShellGlanceSheet {
    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let screen = cx.turtle().rect();
        // Every frame, open or closed: the kit keeps its overlay in tree order.
        self.d.begin_surface(cx);
        if let Some(open) = &self.open {
            let tok = self.d.tokens(self.tokens);
            let ink = tok.notifications.surface.text;
            self.d.solid(cx, screen, vec4(0.0, 0.0, 0.0, 0.55));
            let sheet = sheet_rect(screen);
            self.sheet = sheet;
            self.d.card(cx, sheet, &tok.notifications.surface);
            let close = close_rect(sheet);
            self.d.label_elided(cx, rect(sheet.pos.x + PAD + 4.0, sheet.pos.y + 4.0, sheet.size.x - PAD * 2.0 - CLOSE - 8.0, HEADER - 4.0), false, 12.0, alpha(ink, 0.7), HAlign::Left, &open.title);
            self.d.icon_centered(cx, Ico::Close, close, 14.0, ink);
            let card = card_rect(sheet);
            let (key, app, contained, body) = (Self::tile_key(&open.key), open.app.clone(), open.contained, open.body.clone());
            self.tiles.draw(cx, &key, &app, contained, &body, card);
            let layout = format!("{} sheet@{},{},{},{} card@{},{},{},{} close@{},{}", open.key, sheet.pos.x as i32, sheet.pos.y as i32, sheet.size.x as i32, sheet.size.y as i32, card.pos.x as i32, card.pos.y as i32, card.size.x as i32, card.size.y as i32, (close.pos.x + close.size.x * 0.5) as i32, (close.pos.y + close.size.y * 0.5) as i32);
            if layout != self.logged {
                log!("glance sheet: {layout}");
                self.logged = layout;
            }
        }
        self.d.end_surface(cx);
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if self.open.is_none() {
            return;
        }
        match event {
            Event::MouseDown(e) if contains(close_rect(self.sheet), e.abs) || !contains(self.sheet, e.abs) => {
                self.close(cx);
                return;
            }
            Event::KeyDown(e) if e.key_code == KeyCode::Escape => {
                self.close(cx);
                return;
            }
            _ => {}
        }
        self.tiles.handle_event(cx, event);
        self.dispatch_taps(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_centred_at_the_app_clip_size() {
        let screen = rect(0.0, 0.0, 1280.0, 800.0);
        let sheet = sheet_rect(screen);
        assert_eq!((sheet.pos.x, sheet.pos.y, sheet.size.x, sheet.size.y), (450.0, 140.0, SHEET_WIDTH, SHEET_HEIGHT));
        let card = card_rect(sheet);
        assert!(card.size.y > crate::glance_card::TILE_MAX_HEIGHT, "not clipped at the tile cap");
        assert!(contains(sheet, close_rect(sheet).pos));
        // A small screen keeps it on screen.
        let small = sheet_rect(rect(0.0, 0.0, 360.0, 480.0));
        assert!(small.size.x <= 328.0 && small.size.y <= 448.0 && small.pos.x >= 16.0);
    }
}
