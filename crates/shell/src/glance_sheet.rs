//! The card window: one published glance card (glance.rs), full size and
//! centred over a dimmed desk, App Clip style. Clicking a card's toast opens
//! it here (lib.rs, by the key the notification carries); ✕, Esc or a click
//! on the dimmed desk closes it.
//!
//! The window is [`SHEET_WIDTH`] wide and as tall as its card (measured each
//! frame, so it follows the card from state to state), not clipped to the
//! glance tile's cap: at most the screen less a margin, and a taller card
//! scrolls inside. The card runs in its own isolate, under the publishing
//! app's policy, as a glance tile does (glance_card.rs).
//!
//! An L0 card is live here, as in the glance panel ([`LiveCards`]): its
//! taps and field edits run through its `L0Session` (the declared
//! transition, the §5.12 writes this host performs, a re-lowering), so a
//! Reply opens its draft and a Send changes the card. Its state lives as
//! long as the window: closing it forgets it. Its in-card chat (`sys.chat`)
//! is the host's and outlives the window (glance_chat.rs); the card is
//! lowered again when the agent's reply comes.
use crate::glance::GlanceCard;
use crate::glance_card::{GlanceTiles, LiveCards};
use crate::shell::ui::{contains, rect, DrawShellFill, HAlign, Ico, ShellDraw};
use crate::shell::{alpha, MaterialTokens, ShellTokens};
use makepad_widgets::*;

pub const SHEET_WIDTH: f64 = 380.0;
/// The window's least height, and its card's height before it is measured.
pub const SHEET_MIN_HEIGHT: f64 = 160.0;
const UNMEASURED_CARD: f64 = 320.0;
/// What the window keeps clear of the screen's edges.
const MARGIN: f64 = 16.0;
const HEADER: f64 = 44.0;
const PAD: f64 = 12.0;
const CLOSE: f64 = 28.0;
const PHONE_HEADER: f64 = 52.0;

fn phone_sheet_rect(screen: Rect, card_h: f64, safe: makepad_platform::event::SafeAreaInsets) -> Rect {
    let available = screen.size.x - safe.left - safe.right;
    let w = (available - 24.0).clamp(0.0, 420.0);
    let h = (PHONE_HEADER + card_h + PAD).max(SHEET_MIN_HEIGHT)
        .min((screen.size.y - safe.top - safe.bottom - 32.0).max(0.0));
    rect(screen.pos.x + safe.left + (available - w) * 0.5,
        screen.pos.y + screen.size.y - safe.bottom - 16.0 - h, w, h)
}

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

/// Where the window sits on a screen for a card `card_h` tall: centred, as
/// tall as the card (the header above, a margin below), within the screen.
pub fn sheet_rect(screen: Rect, card_h: f64) -> Rect {
    let w = SHEET_WIDTH.min(screen.size.x - MARGIN * 2.0).max(200.0);
    let h = (HEADER + card_h + PAD).max(SHEET_MIN_HEIGHT).min(screen.size.y - MARGIN * 2.0).max(120.0);
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

/// The open card, as it was published when the window opened.
struct Open {
    key: String,
    card: GlanceCard,
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
    /// An L0 card's live state (a script card keeps its own).
    #[rust]
    live: LiveCards,
    /// What the last layout log said, so it is logged once per change.
    #[rust]
    logged: String,
    /// The window's own area: what `redraw` repaints (`draw_bg` draws
    /// nothing), so an agent's reply that comes later shows at once.
    #[redraw]
    #[rust]
    area: Area,
    // Close on the matching release, retaining the whole gesture so its
    // release cannot activate a card or control below the modal surface.
    #[rust]
    close_touch: Option<(u64, Vec2d, bool)>,
    #[rust]
    pub mobile: bool,
}

impl ShellGlanceSheet {
    fn close_target(&self) -> Rect {
        if self.mobile {
            rect(self.sheet.pos.x + self.sheet.size.x - 52.0, self.sheet.pos.y + 4.0, 44.0, 44.0)
        } else { close_rect(self.sheet) }
    }
    /// The key (`app/card_id`) of the open card.
    pub fn open_key(&self) -> Option<&str> {
        self.open.as_ref().map(|o| o.key.as_str())
    }

    /// Open the published card `key`. False when it is no longer published.
    pub fn open_card(&mut self, cx: &mut Cx, key: &str) -> bool {
        let Some(card) = crate::glance::card(key) else {
            return false;
        };
        // A fresh isolate and session for each opening: the card starts as
        // published (lowered now, so one that does not lower says so once).
        self.tiles.sweep(cx, &[]);
        self.tiles = GlanceTiles::scrolling();
        self.live.clear();
        self.live.body(&Self::tile_key(key), &card, "glance sheet");
        self.open = Some(Open { key: key.to_string(), card });
        log!("glance sheet: opened {key}");
        self.redraw(cx);
        true
    }

    pub fn close(&mut self, cx: &mut Cx) {
        self.close_touch = None;
        if let Some(open) = self.open.take() {
            log!("glance sheet: closed {}", open.key);
        }
        self.tiles.sweep(cx, &[]);
        self.live.clear();
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

    /// Run the open card's queued taps through its L0 session, and lower it
    /// again when the agent's reply came (glance_card.rs `LiveCards`).
    fn dispatch_taps(&mut self, cx: &mut Cx) {
        if self.open.is_some() && self.live.dispatch(cx, &self.tiles, "glance sheet") {
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
            let (fill, ink) = if self.mobile {
                crate::shell::ui::phone_card_colors(crate::glance_card::dark())
            } else { (tok.notifications.surface.bg(), tok.notifications.surface.text) };
            self.d.solid(cx, screen, vec4(0.0, 0.0, 0.0, if self.mobile { 0.32 } else { 0.55 }));
            let card_h = crate::glance_card::measured_height(&Self::tile_key(&open.key)).unwrap_or(UNMEASURED_CARD);
            let safe = cx.display_context.safe_area_insets;
            let sheet = if self.mobile { phone_sheet_rect(screen, card_h, safe) }
                else { sheet_rect(screen, card_h) };
            self.sheet = sheet;
            let close = if self.mobile {
                rect(sheet.pos.x + sheet.size.x - 52.0, sheet.pos.y + 4.0, 44.0, 44.0)
            } else { close_rect(sheet) };
            let card = if self.mobile {
                self.d.phone_card(cx, sheet, fill, 1.0);
                self.d.label(cx, rect(sheet.pos.x + 20.0, sheet.pos.y + 8.0, sheet.size.x - 80.0, 36.0),
                    true, 12.0, alpha(ink, 0.65), HAlign::Left, "Notification");
                self.d.rounded(cx, rect(close.pos.x + 7.0, close.pos.y + 7.0, 30.0, 30.0), 15.0, alpha(ink, 0.06));
                rect(sheet.pos.x + PAD, sheet.pos.y + PHONE_HEADER, sheet.size.x - PAD * 2.0,
                    sheet.size.y - PHONE_HEADER - PAD)
            } else {
                self.d.card(cx, sheet, &tok.notifications.surface);
                self.d.label_elided(cx, rect(sheet.pos.x + PAD + 4.0, sheet.pos.y + 4.0, sheet.size.x - PAD * 2.0 - CLOSE - 8.0, HEADER - 4.0), false, 12.0, alpha(ink, 0.7), HAlign::Left, &open.card.title);
                card_rect(sheet)
            };
            self.d.icon_centered(cx, Ico::Close, close, if self.mobile { 12.0 } else { 14.0 }, ink);
            let key = Self::tile_key(&open.key);
            let body = self.live.body(&key, &open.card, "glance sheet");
            self.tiles.draw(cx, &key, &open.card.app, open.card.contained, &body, card);
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
        if matches!(event, Event::Pause | Event::Background) { self.close_touch = None; }
        if self.open.is_none() {
            return;
        }
        if let Event::TouchUpdate(e) = event {
            use makepad_platform::event::TouchState;
            if self.close_touch.as_ref().is_some_and(|(uid, _, _)| !e.touches.iter().any(|t|
                t.uid == *uid && t.state != TouchState::Start)) {
                self.close_touch = None;
            }
            let sheet = self.sheet;
            let close = self.close_target();
            let closes = |p| contains(close, p) || !contains(sheet, p);
            let mut consumed = self.close_touch.is_some();
            for t in &e.touches {
                if self.close_touch.is_none() && t.state == TouchState::Start && closes(t.abs) {
                    self.close_touch = Some((t.uid, t.abs, true));
                    consumed = true;
                }
                if let Some((uid, start, armed)) = self.close_touch.as_mut() {
                    if *uid == t.uid {
                        if (t.abs - *start).length() > 12.0 { *armed = false; }
                        if t.state == TouchState::Stop {
                            let should_close = *armed && closes(t.abs);
                            self.close_touch = None;
                            if should_close { self.close(cx); return; }
                        }
                    }
                }
            }
            if consumed { return; }
        }
        match event {
            Event::MouseDown(e) if contains(self.close_target(), e.abs) || !contains(self.sheet, e.abs) => {
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
    fn phone_sheet_and_close_target_stay_inside_system_insets() {
        use makepad_platform::event::SafeAreaInsets;
        let mut cx = Cx::new(Box::new(|_, _| {}));
        for (w, h) in [(320.0, 640.0), (384.0, 810.0), (810.0, 384.0)] {
            let safe = SafeAreaInsets { top: 32.0, right: 24.0, bottom: 24.0, left: 8.0 };
            let mut sheet = opened(&mut cx);
            sheet.mobile = true;
            sheet.sheet = phone_sheet_rect(rect(0.0, 0.0, w, h), 900.0, safe);
            let r = sheet.sheet;
            assert!(r.pos.x >= safe.left + 12.0 && r.pos.y >= safe.top + 16.0);
            assert!(r.pos.x + r.size.x <= w - safe.right - 12.0);
            assert!(r.pos.y + r.size.y <= h - safe.bottom - 16.0);
            let close = sheet.close_target();
            assert_eq!(close.size, dvec2(44.0, 44.0));
            let at = close.pos + close.size * 0.5;
            touch(&mut sheet, &mut cx, makepad_platform::event::TouchState::Start, at);
            touch(&mut sheet, &mut cx, makepad_platform::event::TouchState::Stop, at);
            assert!(!sheet.is_open());
        }
    }

    fn opened(cx: &mut Cx) -> ShellGlanceSheet {
        let mut sheet = cx.with_vm(ShellGlanceSheet::script_new);
        sheet.sheet = rect(20., 60., 340., 400.);
        sheet.open = Some(Open { key: "test/card".into(), card: GlanceCard {
            app: "test".into(), card_id: "card".into(), title: "Card".into(), priority: 0,
            published_ms: 0, expires_ms: u64::MAX, open_app: "test".into(), route: None,
            body: "".into(), contained: false, digests: Vec::new(), l0: None,
        }});
        sheet
    }
    fn touch(sheet: &mut ShellGlanceSheet, cx: &mut Cx, state: makepad_platform::event::TouchState, abs: Vec2d) {
        use makepad_platform::event::{TouchPoint, TouchUpdateEvent};
        let event = Event::TouchUpdate(TouchUpdateEvent {
            time: 0., window_id: CxWindowPool::id_zero(), modifiers: Default::default(),
            touches: vec![TouchPoint { state, abs, time: 0., uid: 9, rotation_angle: 0., force: 0.,
                radius: dvec2(1., 1.), handled: Default::default(), sweep_lock: Default::default() }],
        });
        sheet.handle_event(cx, &event, &mut Scope::empty());
    }
    #[test]
    fn phone_card_close_and_backdrop_taps_close_on_release_only() {
        use makepad_platform::event::TouchState::*;
        let mut cx = Cx::new(Box::new(|_, _| {}));
        for at in [dvec2(330., 82.), dvec2(5., 500.)] {
            let mut sheet = opened(&mut cx);
            touch(&mut sheet, &mut cx, Start, at);
            assert!(sheet.is_open(), "the modal keeps ownership through release");
            touch(&mut sheet, &mut cx, Stop, at);
            assert!(!sheet.is_open());
            assert!(sheet.close_touch.is_none());
        }
    }
    #[test]
    fn dragging_from_the_phone_card_close_target_does_not_close_it() {
        use makepad_platform::event::TouchState::*;
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut sheet = opened(&mut cx);
        let at = dvec2(330., 82.);
        touch(&mut sheet, &mut cx, Start, at);
        touch(&mut sheet, &mut cx, Move, at + dvec2(0., 60.));
        touch(&mut sheet, &mut cx, Stop, at);
        assert!(sheet.is_open());
        assert!(sheet.close_touch.is_none());
    }

    #[test]
    fn a_modal_consuming_card_close_release_does_not_poison_the_next_gesture() {
        use makepad_platform::event::TouchState::*;
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut sheet = opened(&mut cx);
        touch(&mut sheet, &mut cx, Start, dvec2(330., 82.));
        // Stop is consumed above this sheet. A new Start reusing Android's
        // pointer ID inside the card must cancel that old close gesture.
        touch(&mut sheet, &mut cx, Start, dvec2(100., 200.));
        assert!(sheet.close_touch.is_none());
        touch(&mut sheet, &mut cx, Stop, dvec2(100., 200.));
        assert!(sheet.is_open());
        touch(&mut sheet, &mut cx, Start, dvec2(330., 82.));
        touch(&mut sheet, &mut cx, Stop, dvec2(330., 82.));
        assert!(!sheet.is_open());
    }

    #[test]
    fn the_window_is_centred_and_sized_to_its_card() {
        let screen = rect(0.0, 0.0, 1280.0, 800.0);
        let sheet = sheet_rect(screen, 300.0);
        assert_eq!((sheet.pos.x, sheet.size.x, sheet.size.y), (450.0, SHEET_WIDTH, HEADER + 300.0 + PAD));
        assert_eq!(sheet.pos.y, (800.0 - sheet.size.y) * 0.5);
        let card = card_rect(sheet);
        assert_eq!(card.size.y, 300.0, "the card fills the window: no empty area below it");
        assert!(contains(sheet, close_rect(sheet).pos));
        // Taller than the tile cap, it is not clipped there.
        let tall = crate::glance_card::TILE_MAX_HEIGHT + 100.0;
        assert_eq!(card_rect(sheet_rect(screen, tall)).size.y, tall);
        // Taller than the screen, the window stops at the margin (the card
        // scrolls inside); a tiny card keeps the least height.
        assert_eq!(sheet_rect(screen, 5000.0).size.y, 800.0 - MARGIN * 2.0);
        assert_eq!(sheet_rect(screen, 10.0).size.y, SHEET_MIN_HEIGHT);
        let small = sheet_rect(rect(0.0, 0.0, 360.0, 480.0), 600.0);
        assert!(small.size.x <= 328.0 && small.size.y <= 448.0 && small.pos.x >= 16.0);
    }
}
