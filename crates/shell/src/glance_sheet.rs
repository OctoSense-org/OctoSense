//! The card window: one published glance card (glance.rs), full size and
//! full-screen on a phone or centred over a dimmed desktop. A card's toast opens
//! it here (lib.rs, by the key the notification carries); ✕, Esc or a click
//! on the dimmed desk closes it.
//!
//! On desktop the window is [`SHEET_WIDTH`] wide and as tall as its card (measured each
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
//! lowered again when the agent's reply comes. A single declared conversation
//! can use card_chat.rs's native virtualized transcript and fixed composer.
use crate::glance::GlanceCard;
use crate::mobile_gestures::SafeInsets;
use crate::glance_card::{GlanceTiles, LiveCards};
use crate::shell::ui::{contains, rect, DrawShellFill, HAlign, Ico, ShellDraw};
use crate::shell::{alpha, MaterialTokens, ShellTokens};
use makepad_widgets::*;
use octoscript_ui_l0::InstanceStore;

#[cfg(any(feature = "app-hub", native_mobile))]
use crate::mail_review::{MailReview, MailToolbar};
#[cfg(not(any(feature = "app-hub", native_mobile)))]
#[derive(Default)]
struct MailReview;
#[cfg(not(any(feature = "app-hub", native_mobile)))]
#[derive(Default)]
struct MailToolbar;

pub const SHEET_WIDTH: f64 = 380.0;
/// The window's least height, and its card's height before it is measured.
pub const SHEET_MIN_HEIGHT: f64 = 160.0;
const UNMEASURED_CARD: f64 = 320.0;
/// What the window keeps clear of the screen's edges.
const MARGIN: f64 = 16.0;
const HEADER: f64 = 44.0;
const PAD: f64 = 12.0;
const CLOSE: f64 = 44.0;
const TABS: f64 = 52.0;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.ShellGlanceSheetBase = #(ShellGlanceSheet::register_widget(vm))
    mod.widgets.ShellGlanceSheet = set_type_default() do mod.widgets.ShellGlanceSheetBase {
        width: Fill
        height: Fill
        draw_bg +: {}
        d +: {}
        tabs: View {
            width: Fill height: 52 flow: Right spacing: 8 padding: Inset{left: 14 right: 14 bottom: 8}
            chat_tab := ButtonFlat {width: Fill height: Fill margin: 0 text: "Chat"}
            card_tab := ButtonFlat {width: Fill height: Fill margin: 0 text: "Card"}
        }
        chat: CardChat {}
        mail: MailClip {}
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

/// The open card, refreshed when the admitted publication changes.
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
    #[find] #[live] tabs: WidgetRef,
    #[find] #[live] chat: WidgetRef,
    #[find] #[live] mail: WidgetRef,
    #[rust] fullscreen: bool,
    #[rust] insets: SafeInsets,
    #[rust] chatting: bool,
    #[rust] chat_available: bool,
    #[rust] resume_store: Option<InstanceStore>,
    #[rust] tabs_style: Option<(bool, Vec4f)>,
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
    // Retain a close gesture through release so it cannot activate content
    // beneath the sheet; modal takeover/Android pointer reuse cancels it.
    #[rust]
    close_touch: Option<(u64, Vec2d, bool)>,
    #[rust]
    review: MailReview,
    #[rust]
    review_generation: u64,
    #[rust]
    publication_generation: u64,
    #[rust]
    mail_toolbar: MailToolbar,
    #[rust]
    keyboard_visible: bool,
}

impl ShellGlanceSheet {
    fn style_tabs(&mut self, cx: &mut Cx, ink: Vec4f) {
        if self.tabs_style == Some((self.chatting, ink)) { return; }
        self.tabs_style = Some((self.chatting, ink));
        let light = ink.x + ink.y + ink.z < 1.5;
        for (path, active) in [(ids!(card_tab), !self.chatting), (ids!(chat_tab), self.chatting)] {
            let face = match (light, active) {
                (true, true) => crate::shell::rgb(229, 237, 255), (true, false) => crate::shell::rgb(243, 245, 249),
                (false, true) => crate::shell::rgb(42, 58, 87), (false, false) => crate::shell::rgb(35, 39, 48),
            };
            let text = if active && light { crate::shell::rgb(38, 80, 170) } else { ink };
            let mut button = self.tabs.widget(cx, path);
            script_apply_eval!(cx, button, {
                draw_bg +: {color: #(face) color_hover: #(face) color_down: #(face) color_focus: #(face) border_size: 0 border_radius: 10}
                draw_text +: {color: #(text) color_hover: #(text) color_down: #(text) color_focus: #(text)}
            });
        }
    }
    pub fn set_presentation(&mut self, fullscreen: bool, insets: SafeInsets) {
        self.fullscreen = fullscreen;
        self.insets = insets;
    }

    fn select_chat(&mut self, cx: &mut Cx, chatting: bool) {
        if chatting && self.mail_binding().is_some() {
            if let Some(mut mail) = self.mail.borrow_mut::<crate::mail_clip::MailClip>() {
                if mail.flush(cx).is_err() { return; }
            }
        }
        if !chatting {
            if let (Some(open), Some(store)) = (&self.open, self.resume_store.take()) {
                self.live.restore_store(&Self::tile_key(&open.key), store);
            }
        }
        self.chatting = chatting && self.chat_available;
        self.mail_toolbar = MailToolbar::default();
        cx.set_key_focus(Area::Empty);
        cx.hide_text_ime();
        self.redraw(cx);
    }
    /// The key (`app/card_id`) of the open card.
    pub fn open_key(&self) -> Option<&str> {
        self.open.as_ref().map(|o| o.key.as_str())
    }
    fn mail_binding(&self) -> Option<crate::mail_card::Binding> {
        self.open.as_ref()?.card.l0.as_ref()?.mail.clone()
    }

    /// Open the published card `key`. False when it is no longer published.
    pub fn open_card(&mut self, cx: &mut Cx, key: &str) -> bool {
        self.chatting = false;
        self.chat_available = false;
        self.resume_store = None;
        if let Some(mut chat) = self.chat.borrow_mut::<crate::card_chat::CardChat>() { chat.reset(cx); }
        cx.set_key_focus(Area::Empty);
        cx.hide_text_ime();
        self.publication_generation = crate::glance::generation();
        let card = match crate::glance::card(key) {
            Some(card) => card,
            None => {
                #[cfg(any(feature = "app-hub", native_mobile))]
                if let Some(review) = crate::mail_card::take_review(key) {
                    // Superseding a view revokes its token only: both reviews
                    // may refer to the same deduplicated send operation.
                    self.review.replace();
                    self.close(cx);
                    // A legacy Mail composer supplies an opaque host review,
                    // not generated card source or authority-bearing JSON.
                    self.open = Some(Open { key: key.into(), card: GlanceCard {
                        app: "os.mail".into(), card_id: "host-review".into(), title: "Mail reply".into(),
                        priority: 0, published_ms: 0, expires_ms: u64::MAX, open_app: "mail".into(),
                        route: None, body: "".into(), contained: false, digests: Vec::new(), l0: None,
                    }});
                    self.review.open(review);
                    self.review_generation = crate::mail_card::review_generation(key);
                    cx.set_key_focus(Area::Empty);
                    cx.hide_text_ime();
                    self.redraw(cx);
                    return true;
                }
                self.close(cx);
                return false;
            }
        };
        #[cfg(any(feature = "app-hub", native_mobile))]
        {
            if let Some(old) = self.open.as_ref().filter(|old| old.key != key) { crate::mail_card::cancel_for(&old.key); }
            self.review.replace();
        }
        // A gesture/error from another card must not target this binding.
        self.close_touch = None;
        self.mail_toolbar = MailToolbar::default();
        // Mail opens native panes; its admitted L0 publication still identifies
        // the conversation, but rebuilding its generated layout here is wasted.
        self.tiles.sweep(cx, &[]);
        self.tiles = GlanceTiles::scrolling();
        self.live.clear();
        if card.l0.as_ref().is_some_and(|l| l.mail.is_some()) {
            self.live.prepare_native(&Self::tile_key(key), &card);
        } else { self.live.body(&Self::tile_key(key), &card, "glance sheet"); }
        self.chat_available = self.live.session_mut(&Self::tile_key(key)).is_some_and(|s| s.has_chat());
        self.open = Some(Open { key: key.to_string(), card });
        if let Some(binding) = self.mail_binding() {
            if let Some(mut mail) = self.mail.borrow_mut::<crate::mail_clip::MailClip>() { mail.open(cx, binding); }
        }
        self.tabs.button(cx, ids!(card_tab)).set_text(cx, if self.mail_binding().is_some() { "Reply" } else { "Card" });
        log!("glance sheet: opened {key}");
        self.redraw(cx);
        true
    }

    pub fn close(&mut self, cx: &mut Cx) {
        if self.mail_binding().is_some() {
            if let Some(mut mail) = self.mail.borrow_mut::<crate::mail_clip::MailClip>() { let _ = mail.flush(cx); }
        }
        cx.set_key_focus(Area::Empty);
        cx.hide_text_ime();
        self.chatting = false;
        self.chat_available = false;
        self.resume_store = None;
        self.close_touch = None;
        self.mail_toolbar = MailToolbar::default();
        #[cfg(any(feature = "app-hub", native_mobile))]
        self.review.close();
        if let Some(open) = self.open.take() {
            crate::mail_card::cancel_for(&open.key);
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

    fn refresh_publication(&mut self, cx: &mut Cx) {
        let generation = crate::glance::generation();
        if generation == self.publication_generation { return; }
        self.publication_generation = generation;
        let Some(open) = &self.open else { return; };
        // The native composer has no Glance publication to refresh.
        if open.card.card_id == "host-review" && open.card.l0.is_none() && open.card.body.is_empty() { return; }
        match crate::glance::card(&open.key) {
            Some(card) => self.replace_publication(cx, card),
            None => self.close(cx),
        }
    }

    fn replace_publication(&mut self, cx: &mut Cx, card: GlanceCard) {
        let same_mail = self.mail_binding().is_some() && self.mail_binding() == card.l0.as_ref().and_then(|l| l.mail.clone());
        let Some(open) = self.open.as_mut() else { return; };
        if open.card == card { return; }
        open.card = card;
        // Throw away old source and queued NAV events; durable Mail state and
        // unsaved edits live in the host's draft-scoped Session store.
        self.close_touch = None;
        self.mail_toolbar = MailToolbar::default();
        self.tiles.sweep(cx, &[]);
        self.tiles = GlanceTiles::scrolling();
        self.live.clear();
        self.logged.clear();
        if !same_mail {
            cx.set_key_focus(Area::Empty);
            self.chatting = false;
            if let Some(mut chat) = self.chat.borrow_mut::<crate::card_chat::CardChat>() { chat.reset(cx); }
            cx.hide_text_ime();
            if let Some(binding) = self.mail_binding() {
                if let Some(mut mail) = self.mail.borrow_mut::<crate::mail_clip::MailClip>() { mail.open(cx, binding); }
            }
        }
        let open = self.open.as_ref().unwrap();
        let key = Self::tile_key(&open.key);
        if same_mail || open.card.l0.as_ref().is_some_and(|l| l.mail.is_some()) {
            self.live.prepare_native(&key, &open.card);
        } else { self.live.body(&key, &open.card, "glance sheet"); }
        self.chat_available = self.live.session_mut(&key).is_some_and(|s| s.has_chat());
        self.resume_store = None;
        self.redraw(cx);
    }

    #[cfg(any(feature = "app-hub", native_mobile))]
    fn poll_review(&mut self, cx: &mut Cx) {
        if let Some(open) = &self.open {
            let generation = crate::mail_card::review_generation(&open.key);
            if self.review.is_active() && generation != self.review_generation {
                self.review.invalidate("This card or its review changed. Request a fresh review before sending.");
                self.review_generation = generation;
            }
            if let Some(review) = crate::mail_card::take_review(&open.key) {
                self.review.open(review);
                self.review_generation = generation;
                cx.set_key_focus(Area::Empty);
                cx.hide_text_ime();
            }
        }
        self.review.poll();
    }

    /// Run the open card's queued taps through its L0 session, and lower it
    /// again when the agent's reply came (glance_card.rs `LiveCards`).
    fn dispatch_taps(&mut self, cx: &mut Cx) {
        let Some(open) = &self.open else { return; };
        let key = Self::tile_key(&open.key);
        let previous = self.live.session_mut(&key).map(|s| s.store.clone());
        if self.live.dispatch(cx, &self.tiles, "glance sheet") {
            if self.live.session_mut(&key).is_some_and(|s| s.shows_chat()) {
                self.resume_store = previous;
                self.select_chat(cx, true);
            }
            self.redraw(cx);
        }
    }
}

impl Widget for ShellGlanceSheet {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let screen = cx.turtle().rect();
        self.refresh_publication(cx);
        #[cfg(any(feature = "app-hub", native_mobile))]
        self.poll_review(cx);
        // Every frame, open or closed: the kit keeps its overlay in tree order.
        self.d.begin_surface(cx);
        let ink = self.d.tokens(self.tokens).notifications.surface.text;
        self.style_tabs(cx, ink);
        if let Some(mut chat) = self.chat.borrow_mut::<crate::card_chat::CardChat>() { chat.set_ink(cx, ink); }
        if let Some(mut mail) = self.mail.borrow_mut::<crate::mail_clip::MailClip>() { mail.set_ink(cx, ink); }
        if let Some(open) = &self.open {
            let tok = self.d.tokens(self.tokens);
            let ink = tok.notifications.surface.text;
            let mut background = tok.notifications.surface.background;
            background.w = 1.0;
            self.d.solid(cx, screen, if self.fullscreen { background } else { vec4(0.0, 0.0, 0.0, 0.55) });
            let card_h = crate::glance_card::measured_height(&Self::tile_key(&open.key)).unwrap_or(UNMEASURED_CARD);
            #[cfg(any(feature = "app-hub", native_mobile))]
            let card_h = if self.review.is_active() { 560.0 } else if !self.keyboard_visible && open.card.l0.as_ref().is_some_and(|l| l.mail.is_some()) { card_h + MailToolbar::HEIGHT } else { card_h };
            let sheet = if self.fullscreen {
                let mut insets = self.insets;
                // KeyboardView has already shortened the available viewport.
                if self.keyboard_visible { insets.bottom = 0.0; }
                insets.inset(screen)
            } else { sheet_rect(screen, if self.chatting || open.card.l0.as_ref().is_some_and(|l| l.mail.is_some()) {600.0} else {card_h + if self.chat_available {TABS} else {0.0}}) };
            self.sheet = sheet;
            if !self.fullscreen { self.d.card(cx, sheet, &tok.notifications.surface); }
            let close = close_rect(sheet);
            #[cfg(any(feature = "app-hub", native_mobile))]
            let heading = if self.review.is_active() { "Mail reply" } else { &open.card.title };
            #[cfg(not(any(feature = "app-hub", native_mobile)))]
            let heading = &open.card.title;
            self.d.label_elided(cx, rect(sheet.pos.x + PAD + 4.0, sheet.pos.y + 4.0, sheet.size.x - PAD * 2.0 - CLOSE - 8.0, HEADER - 4.0), false, 12.0, alpha(ink, 0.7), HAlign::Left, heading);
            self.d.icon_centered(cx, Ico::Close, close, 14.0, ink);
            let mut card = card_rect(sheet);
            let key = Self::tile_key(&open.key);
            #[cfg(any(feature = "app-hub", native_mobile))]
            let reviewing = self.review.is_active();
            #[cfg(not(any(feature = "app-hub", native_mobile)))]
            let reviewing = false;
            if reviewing {
                #[cfg(any(feature = "app-hub", native_mobile))]
                self.review.draw(cx, &mut self.d, card, &tok);
            } else {
                // Only the active pane draws and receives input. The generated
                // draft and the native chat share the exact publication/session.
                if !self.chatting && open.card.l0.as_ref().is_none_or(|l| l.mail.is_none()) {
                    let new_session = self.live.session_mut(&key).is_none();
                    self.live.body(&key, &open.card, "glance sheet");
                    if new_session { self.chat_available = self.live.session_mut(&key).is_some_and(|s| s.has_chat()); }
                }
                if self.chat_available {
                    self.tabs.draw_walk_all(cx, scope, Walk::abs_rect(rect(sheet.pos.x, card.pos.y, sheet.size.x, TABS)));
                    card.pos.y += TABS;
                    card.size.y = (card.size.y - TABS).max(0.0);
                }
                if self.chatting {
                    if let (Some(session), Some(mut chat)) = (self.live.session_mut(&key), self.chat.borrow_mut::<crate::card_chat::CardChat>()) { chat.sync(cx, session); }
                    self.chat.draw_walk_all(cx, scope, Walk::abs_rect(rect(sheet.pos.x, card.pos.y, sheet.size.x, card.size.y)));
                } else if open.card.l0.as_ref().is_some_and(|l| l.mail.is_some()) {
                    self.mail.draw_walk_all(cx, scope, Walk::abs_rect(rect(sheet.pos.x, card.pos.y, sheet.size.x, card.size.y)));
                } else {
                #[cfg(any(feature = "app-hub", native_mobile))]
                let card = if let Some(binding) = open.card.l0.as_ref().and_then(|l| l.mail.as_ref()).filter(|_| !self.keyboard_visible) {
                    let toolbar = rect(card.pos.x, card.pos.y + (card.size.y - MailToolbar::HEIGHT).max(0.), card.size.x, MailToolbar::HEIGHT);
                    self.mail_toolbar.draw(cx, &mut self.d, toolbar, &tok, binding);
                    rect(card.pos.x, card.pos.y, card.size.x, (card.size.y - MailToolbar::HEIGHT).max(20.))
                } else { card };
                let body = self.live.body(&key, &open.card, "glance sheet");
                self.tiles.draw(cx, &key, &open.card.app, open.card.contained, &body, card);
                }
            }
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

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if let Event::VirtualKeyboard(keyboard) = event {
            let visible = match keyboard {
                VirtualKeyboardEvent::WillShow { height, .. } | VirtualKeyboardEvent::DidShow { height, .. } => *height > 0.0,
                VirtualKeyboardEvent::WillHide { .. } | VirtualKeyboardEvent::DidHide { .. } => false,
            };
            if visible != self.keyboard_visible { self.mail_toolbar = MailToolbar::default(); }
            self.keyboard_visible = visible;
            self.redraw(cx);
        }
        self.refresh_publication(cx);
        if matches!(event, Event::Pause | Event::Background) { self.close_touch = None; }
        if self.open.is_none() {
            return;
        }
        #[cfg(any(feature = "app-hub", native_mobile))]
        self.poll_review(cx);
        if let Event::TouchUpdate(e) = event {
            use makepad_platform::event::TouchState;
            if self.close_touch.as_ref().is_some_and(|(uid, _, _)| !e.touches.iter().any(|t|
                t.uid == *uid && t.state != TouchState::Start)) {
                self.close_touch = None;
            }
            let sheet = self.sheet;
            let closes = |p| contains(close_rect(sheet), p) || !contains(sheet, p);
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
        #[cfg(any(feature = "app-hub", native_mobile))]
        if self.review.is_active() {
            self.review.handle_event(event);
            if !self.review.is_active() && self.open.as_ref().is_some_and(|o| o.card.card_id == "host-review" && o.card.l0.is_none() && o.card.body.is_empty()) {
                self.close(cx);
            }
            self.redraw(cx);
            return; // Host review is modal within this sheet; no L0 NAV dispatch.
        }
        if self.chat_available {
            let actions = cx.capture_actions(|cx| self.tabs.handle_event(cx, event, scope));
            if self.tabs.button(cx, ids!(card_tab)).clicked(&actions) { self.select_chat(cx, false); return; }
            if self.tabs.button(cx, ids!(chat_tab)).clicked(&actions) { self.select_chat(cx, true); return; }
        }
        if self.chatting {
            let key = Self::tile_key(&self.open.as_ref().unwrap().key);
            if let (Some(session), Some(mut chat)) = (self.live.session_mut(&key), self.chat.borrow_mut::<crate::card_chat::CardChat>()) { chat.sync(cx, session); }
            self.chat.handle_event(cx, event, scope);
            if let Some(mut chat) = self.chat.borrow_mut::<crate::card_chat::CardChat>() {
                if let Some(text) = chat.take_submit() {
                    let result = self.live.session_mut(&key).ok_or_else(|| "This conversation is unavailable".to_string()).and_then(|session| session.chat_submit(&text));
                    chat.submitted(cx, result);
                }
            }
            let open_reply = self.chat.borrow_mut::<crate::card_chat::CardChat>().is_some_and(|mut chat| chat.take_open_reply());
            if open_reply { self.select_chat(cx, false); }
            return;
        }
        if self.mail_binding().is_some() {
            self.mail.handle_event(cx, event, scope);
            #[cfg(any(feature = "app-hub", native_mobile))]
            {
                if let Some(mut mail) = self.mail.borrow_mut::<crate::mail_clip::MailClip>() {
                    if let Some(binding) = mail.take_review() {
                        self.review_generation = crate::mail_card::review_generation(&binding.key());
                        if let Err(error) = self.review.open_receipt_or_review(&binding) { mail.review_error(cx, &error); }
                        if self.review.is_active() { cx.hide_text_ime(); cx.set_key_focus(Area::Empty); }
                    }
                }
                self.poll_review(cx);
            }
            return;
        }
        #[cfg(any(feature = "app-hub", native_mobile))]
        if let Some(binding) = self.open.as_ref().and_then(|o| o.card.l0.as_ref()).and_then(|l| l.mail.clone()).filter(|_| !self.keyboard_visible) {
            if self.mail_toolbar.handle_event(event, &binding, &mut self.review) {
                if self.review.is_active() {
                    self.review_generation = crate::mail_card::review_generation(&binding.key());
                    cx.set_key_focus(Area::Empty);
                    cx.hide_text_ime();
                }
                self.poll_review(cx);
                self.redraw(cx);
                return;
            }
        }
        self.tiles.handle_event(cx, event);
        self.dispatch_taps(cx);
        #[cfg(any(feature = "app-hub", native_mobile))]
        { self.poll_review(cx); if self.review.is_active() { self.redraw(cx); } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn opened(cx: &mut Cx) -> ShellGlanceSheet {
        let mut sheet = cx.with_vm(ShellGlanceSheet::script_new);
        sheet.sheet = rect(20., 60., 340., 400.);
        sheet.publication_generation = crate::glance::generation();
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
    fn republishing_refreshes_visible_source_and_cancels_old_touch_capture() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut sheet = opened(&mut cx);
        sheet.close_touch = Some((9, dvec2(330., 82.), true));
        let mut replacement = sheet.open.as_ref().unwrap().card.clone();
        replacement.title = "Updated reply".into();
        replacement.body = "new source".into();
        sheet.replace_publication(&mut cx, replacement.clone());
        assert_eq!(sheet.open.as_ref().unwrap().card, replacement);
        assert_eq!(sheet.open_key(), Some("test/card"));
        assert!(sheet.close_touch.is_none());
    }

    #[test]
    fn withdrawn_card_key_closes_the_previous_sheet_instead_of_showing_wrong_content() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut sheet = opened(&mut cx);
        assert!(!sheet.open_card(&mut cx, "missing-publisher/withdrawn-card"));
        assert!(!sheet.is_open());
        assert!(sheet.open_key().is_none());
    }

}
