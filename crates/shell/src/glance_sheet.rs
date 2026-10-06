//! A resident card workspace above the shell, independent of Glance feed bounds.
//! Phone summaries expand to the root safe viewport; desktop cards are centred.
//! Dismissing hides the workspace without resetting the draft, L0 state, chat
//! input, or widget scroll positions. Only publication/account invalidation or
//! bounded clean-cache eviction tears down a retained workspace. Review/send
//! authority is revoked on dismissal and is never part of retained view state.
use crate::glance::GlanceCard;
use crate::mobile_gestures::SafeInsets;
use crate::glance_card::{GlanceTiles, LiveCards};
use crate::shell::ui::{contains, rect, DrawShellFill, HAlign, Ico, ShellDraw};
use crate::shell::{MaterialTokens, ShellTokens};
use makepad_widgets::*;
use octoscript_ui_l0::InstanceStore;
use crate::card_presentation::Presentation;

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

    set_type_default() do #(DrawGlanceRound::script_shader(vm)) {
        ..mod.draw.DrawQuad
        color: #fff radius: 20.0
        pixel: fn() {
            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
            sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, self.radius)
            sdf.fill(self.color)
            return sdf.result
        }
    }
    mod.widgets.ShellGlanceSheetBase = #(ShellGlanceSheet::register_widget(vm))
    mod.widgets.ShellGlanceSheet = set_type_default() do mod.widgets.ShellGlanceSheetBase {
        width: Fill
        height: Fill
        draw_bg +: {}
        round +: {}
        d +: {}
        tabs: View {
            width: Fill height: 52 flow: Right spacing: 8 padding: Inset{left: 14 right: 14 bottom: 8}
            card_tab := ButtonFlat {width: Fill height: Fill margin: 0 text: "Email"}
            chat_tab := ButtonFlat {width: Fill height: Fill margin: 0 text: "Chat"}
        }
        chat: CardChat {}
        mail: MailClip {}
        compose: View {width: Fill height: 82 flow: Down spacing: 5 padding: Inset{left: 4 right: 4 top: 6 bottom: 4}
            action := ButtonFlat {width: Fill height: 44 margin: 0 text: "Compose reply"
                draw_bg +: {color: #3668e8 color_hover: #2854c4 color_down: #2148ad border_size: 0 border_radius: 14}
                draw_text +: {color: #ffffff color_hover: #ffffff color_down: #ffffff}
            }
            hint := Label {width: Fill height: Fit flow: Right {wrap: true} max_lines: 2 text_overflow: Ellipsis draw_text.text_style: theme.font_regular{font_size: 10}}
        }
    }
}

#[derive(Script, ScriptHook)]
#[repr(C)]
struct DrawGlanceRound {
    #[deref] draw_super: DrawQuad,
    #[live] color: Vec4f,
    #[live] radius: f32,
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

/// All peer modes share one row directly above the active pane.
fn workspace_rects(sheet: Rect, has_tabs: bool, focus_layout: bool) -> (Rect, Option<Rect>) {
    if focus_layout { return (sheet, None); }
    let mut content = card_rect(sheet);
    if !has_tabs { return (content, None); }
    content.size.y = (content.size.y - TABS).max(0.0);
    let tabs = rect(sheet.pos.x, content.pos.y, sheet.size.x, TABS);
    content.pos.y += TABS;
    (content, Some(tabs))
}

/// The open card, refreshed when the admitted publication changes.
struct Open {
    key: String,
    card: GlanceCard,
    compose: crate::mail_compose::ComposeReply,
}

struct Retained {
    open: Open, live: LiveCards, tiles: GlanceTiles,
    chat: WidgetRef, mail: WidgetRef,
    chatting: bool, chat_available: bool, resume_store: Option<InstanceStore>,
    script_interacted: bool,
}
impl Retained {
    fn dirty(&self, cx: &Cx) -> bool {
        self.chat.borrow::<crate::card_chat::CardChat>().is_some_and(|c| c.has_unsent_input(cx))
            || self.live.has_local_changes() || self.script_interacted
            || self.open.card.l0.as_ref().and_then(|l| l.mail.as_ref()).is_some_and(|b| crate::mail_card::card_status(b).unsaved)
    }
}

#[derive(Script, Widget)]
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
    #[live] round: DrawGlanceRound,
    #[live]
    d: ShellDraw,
    #[live]
    tokens: ShellTokens,
    #[find] #[live] tabs: WidgetRef,
    #[find] #[live] chat: WidgetRef,
    #[find] #[live] mail: WidgetRef,
    #[find] #[live] compose: WidgetRef,
    #[rust] fullscreen: bool,
    #[rust] saved_bar_appearance: Option<SystemBarAppearance>,
    #[rust] presentation: Presentation,
    #[rust] frame: NextFrame,
    #[rust] frame_time: f64,
    #[rust] return_rect: Option<Rect>,
    #[rust] retained: Vec<Retained>,
    #[rust] visible_sheet: Rect,
    #[rust] insets: SafeInsets,
    #[rust] chatting: bool,
    #[rust] chat_available: bool,
    #[rust] resume_store: Option<InstanceStore>,
    #[rust] script_interacted: bool,
    #[rust] tabs_style: Option<(bool, Vec4f)>,
    #[rust] focus_layout: bool,
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

impl ScriptHook for ShellGlanceSheet {
    fn on_after_apply(&mut self, _vm: &mut ScriptVm, apply: &Apply, _scope: &mut Scope, _value: ScriptValue) {
        if !apply.is_eval() { self.tabs_style = None; }
    }
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
    fn reduced(cx: &Cx) -> bool { cx.accessibility_preferences().reduce_motion() || crate::shell::ui::reduce_motion() }
    pub fn present(&mut self, cx: &mut Cx, source: Option<Rect>) {
        self.return_rect = source;
        if self.fullscreen { self.presentation.open(source, Self::reduced(cx)); }
        else { self.presentation.activate(); }
        if self.presentation.moving() {
            self.frame_time = cx.seconds_since_app_start();
            self.frame = cx.new_next_frame();
        }
        self.redraw(cx);
    }
    pub fn covers_background(&self) -> bool { self.is_open() && self.presentation.covers_background() }
    pub fn is_modal(&self) -> bool { self.is_open() }
    /// A full workspace owns the complete pointer stream, including its
    /// opening/closing frames. The covered feed never scrolls underneath it.
    pub fn accepts_pointer(&mut self, _event: &Event) -> bool { self.is_open() }

    fn revoke_review(&mut self) {
        #[cfg(any(feature = "app-hub", native_mobile))]
        self.review.close();
        if let Some(open) = &self.open { crate::mail_card::cancel_for(&open.key); }
    }
    pub fn dismiss(&mut self, cx: &mut Cx) {
        if self.mail_binding().is_some() {
            if let Some(mut mail) = self.mail.borrow_mut::<crate::mail_clip::MailClip>() { let _ = mail.flush(cx); }
        }
        self.revoke_review();
        self.close_touch = None;
        cx.set_key_focus(Area::Empty); cx.hide_text_ime();
        if self.open.as_ref().is_some_and(|o| o.card.card_id == "host-review" && o.card.l0.is_none()) { self.close(cx); return; }
        self.presentation.dismiss(self.return_rect, Self::reduced(cx) || !self.fullscreen);
        if self.presentation.moving() {
            self.frame_time = cx.seconds_since_app_start();
            self.frame = cx.new_next_frame();
        }
        log!("glance workspace: suspend {}", self.open.as_ref().map(|o| o.key.as_str()).unwrap_or(""));
        self.redraw(cx);
    }
    pub fn hide_workspace(&mut self, cx: &mut Cx) {
        self.return_rect = None;
        self.dismiss(cx);
    }
    pub fn back(&mut self, cx: &mut Cx) {
        if self.keyboard_visible {
            cx.set_key_focus(Area::Empty); cx.hide_text_ime(); self.redraw(cx); return;
        }
        #[cfg(any(feature = "app-hub", native_mobile))]
        if self.review.is_active() && self.open.as_ref().is_some_and(|o| o.card.card_id != "host-review") {
            if self.open.as_ref().is_some_and(|o| crate::glance::card(&o.key).is_none()) {
                self.close(cx); return;
            }
            self.revoke_review(); self.redraw(cx); return;
        }
        self.dismiss(cx);
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
        self.open.as_ref().filter(|_| self.is_open()).map(|o| o.key.as_str())
    }
    fn mail_binding(&self) -> Option<crate::mail_card::Binding> {
        self.open.as_ref()?.card.l0.as_ref()?.mail.clone()
    }

    /// Open the published card `key`. False when it is no longer published.
    pub fn open_card(&mut self, cx: &mut Cx, key: &str) -> bool {
        if !self.live.accounts_valid() { self.close(cx); }
        if self.mail_binding().is_some() {
            if let Some(mut mail) = self.mail.borrow_mut::<crate::mail_clip::MailClip>() {
                // Unsaved conflicts stay in the draft-scoped store and in the
                // retained editor, so reopening can always resolve them.
                let _ = mail.flush(cx);
            }
        }
        cx.set_key_focus(Area::Empty); cx.hide_text_ime();
        self.close_touch = None;
        self.mail_toolbar = MailToolbar::default();
        if let Some(card) = crate::glance::card(key) {
            if self.open.as_ref().is_some_and(|o| o.key == key) {
                self.replace_publication(cx, card);
                if !self.presentation.visible() { self.presentation.activate(); }
                log!("glance workspace: resume {key}");
                self.redraw(cx);
                return true;
            }
            self.retain_current(cx);
            if let Some(index) = self.retained.iter().position(|entry| entry.open.key == key && entry.live.accounts_valid()) {
                let entry = self.retained.remove(index);
                self.open = Some(entry.open); self.live = entry.live; self.tiles = entry.tiles;
                self.chat = entry.chat; self.mail = entry.mail;
                self.chatting = entry.chatting; self.chat_available = entry.chat_available;
                self.resume_store = entry.resume_store; self.tabs_style = None;
                self.script_interacted = entry.script_interacted;
                self.replace_publication(cx, card);
                self.tabs.button(cx, ids!(card_tab)).set_text(cx, if self.mail_binding().is_some() { "Email" } else { "Card" });
                self.presentation.activate(); self.redraw(cx);
                log!("glance workspace: resume cached {key}");
                return true;
            }
        }
        self.chatting = false;
        self.chat_available = false;
        self.resume_store = None;
        self.script_interacted = false;
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
                    self.open = Some(Open { key: key.into(), compose: Default::default(), card: GlanceCard {
                        account: None,
                        app: "os.mail".into(), card_id: "host-review".into(), title: "Mail reply".into(), summary: String::new(),
                        priority: 0, published_ms: 0, expires_ms: u64::MAX, open_app: "mail".into(),
                        route: None, body: "".into(), contained: false, digests: Vec::new(), l0: None,
                    }});
                    self.presentation.activate();
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
        self.open = Some(Open { key: key.to_string(), card, compose: Default::default() });
        if let Some(binding) = self.mail_binding() {
            if let Some(mut mail) = self.mail.borrow_mut::<crate::mail_clip::MailClip>() { mail.open(cx, binding); }
        }
        self.tabs.button(cx, ids!(card_tab)).set_text(cx, if self.mail_binding().is_some() { "Email" } else { "Card" });
        self.presentation.activate();
        log!("glance sheet: opened {key}");
        self.redraw(cx);
        true
    }

    fn retain_current(&mut self, cx: &mut Cx) {
        self.revoke_review();
        let Some(open) = self.open.take() else { return; };
        if open.card.card_id == "host-review" && open.card.l0.is_none() { self.close(cx); return; }
        let (chat, mail) = cx.with_vm(|vm| {
            let chat = script_eval!(vm, {use mod.widgets.* CardChat{}});
            let mail = script_eval!(vm, {use mod.widgets.* MailClip{}});
            (WidgetRef::script_from_value(vm, chat), WidgetRef::script_from_value(vm, mail))
        });
        self.retained.push(Retained {
            open, live: std::mem::take(&mut self.live), tiles: std::mem::replace(&mut self.tiles, GlanceTiles::scrolling()),
            chat: std::mem::replace(&mut self.chat, chat), mail: std::mem::replace(&mut self.mail, mail),
            chatting: self.chatting, chat_available: self.chat_available, resume_store: self.resume_store.take(),
            script_interacted: std::mem::take(&mut self.script_interacted),
        });
        // Three inactive clean workspaces plus the active one. Human text
        // and local card changes are not evicted to meet a rendering cache limit.
        while self.retained.iter().filter(|entry| !entry.dirty(cx)).count() > 3 {
            let index = self.retained.iter().position(|entry| !entry.dirty(cx)).unwrap();
            self.retained.remove(index).tiles.sweep(cx, &[]);
        }
    }

    pub fn close(&mut self, cx: &mut Cx) {
        self.presentation.hide();
        self.visible_sheet = Rect::default();
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
        self.open.is_some() && self.presentation.visible()
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
        self.retained.retain_mut(|entry| {
            let valid = crate::glance::card(&entry.open.key).is_some() && entry.live.accounts_valid() && entry.open.card.l0.as_ref().and_then(|l| l.mail.as_ref()).is_none_or(|b| crate::mail_card::account_valid(&b.account));
            if !valid { entry.tiles.sweep(cx, &[]); }
            valid
        });
        if !self.live.accounts_valid() || self.mail_binding().is_some_and(|b| !crate::mail_card::account_valid(&b.account)) { self.close(cx); return; }
        let generation = crate::glance::generation();
        if generation == self.publication_generation { return; }
        self.publication_generation = generation;
        let Some(open) = &self.open else { return; };
        // The native composer has no Glance publication to refresh.
        if open.card.card_id == "host-review" && open.card.l0.is_none() && open.card.body.is_empty() { return; }
        match crate::glance::card(&open.key) {
            Some(card) => self.replace_publication(cx, card),
            None => {
                // Completion clears the feed immediately, but the person who
                // just sent a reply can still read the authoritative receipt.
                #[cfg(any(feature = "app-hub", native_mobile))]
                if self.review.is_active() && self.mail_binding().as_ref().is_some_and(crate::mail_card::completed) { return; }
                self.close(cx);
            }
        }
    }

    fn replace_publication(&mut self, cx: &mut Cx, card: GlanceCard) {
        let previous_binding = self.mail_binding();
        let next_binding = card.l0.as_ref().and_then(|l| l.mail.clone());
        let same_mail = self.mail_binding().is_some() && self.mail_binding() == card.l0.as_ref().and_then(|l| l.mail.clone());
        let Some(open) = self.open.as_mut() else { return; };
        if open.card == card { return; }
        let same_layout = match (&open.card.l0, &card.l0) {
            (Some(old), Some(new)) => old.source == new.source && old.mail == new.mail,
            (None, None) => open.card.body == card.body,
            _ => false,
        };
        // A background layout replacement must not destroy local edits or
        // an unfinished conversation. Keep that workspace's snapshot until
        // withdrawal/expiry; the feed still shows the newest publication.
        let dirty = self.live.has_local_changes() || self.script_interacted
            || self.chat.borrow::<crate::card_chat::CardChat>().is_some_and(|c| c.has_unsent_input(cx));
        let composed = open.compose.requested && crate::mail_compose::eligible(&open.card)
            && next_binding.is_some() && open.card.account == card.account;
        if !same_mail && !same_layout && dirty && !composed { return; }
        if same_layout && card.l0.is_none() {
            self.live.refresh_script_metadata(&Self::tile_key(&open.key), &card);
            open.card = card;
            self.redraw(cx);
            return;
        }
        let keep_conversation = same_mail || (open.card.app == card.app && self.live.accounts_valid());
        let previous = if same_layout && card.l0.is_some() {
            self.live.session_mut(&Self::tile_key(&open.key)).map(|s| s.store.clone())
        } else { None };
        open.card = card;
        // Throw away old source and queued NAV events; durable Mail state and
        // unsaved edits live in the host's draft-scoped Session store.
        self.close_touch = None;
        self.mail_toolbar = MailToolbar::default();
        self.tiles.sweep(cx, &[]);
        self.tiles = GlanceTiles::scrolling();
        self.live.clear();
        self.logged.clear();
        if !keep_conversation {
            cx.set_key_focus(Area::Empty);
            self.chatting = false;
            if let Some(mut chat) = self.chat.borrow_mut::<crate::card_chat::CardChat>() { chat.reset(cx); }
            cx.hide_text_ime();
        }
        // Keeping the conversation widget must not leave the native editor
        // bound to the previous draft (or empty after Card -> Email promotion).
        if previous_binding != next_binding {
            if let Some(binding) = next_binding {
                if let Some(mut mail) = self.mail.borrow_mut::<crate::mail_clip::MailClip>() { mail.open(cx, binding); }
            }
            if let Some(mut chat) = self.chat.borrow_mut::<crate::card_chat::CardChat>() { chat.refresh(); }
        }
        if composed { self.chatting = false; cx.set_key_focus(Area::Empty); cx.hide_text_ime(); }
        self.tabs.button(cx, ids!(card_tab)).set_text(cx, if self.mail_binding().is_some() { "Email" } else { "Card" });
        let open = self.open.as_ref().unwrap();
        let key = Self::tile_key(&open.key);
        if same_mail || open.card.l0.as_ref().is_some_and(|l| l.mail.is_some()) {
            self.live.prepare_native(&key, &open.card);
        } else { self.live.body(&key, &open.card, "glance sheet"); }
        self.chat_available = self.live.session_mut(&key).is_some_and(|s| s.has_chat());
        if let Some(store) = previous { self.live.restore_store(&key, store); }
        if !same_layout { self.resume_store = None; }
        self.redraw(cx);
    }

    fn poll_compose(&mut self, cx: &mut Cx) {
        let Some(open) = self.open.as_mut() else { return; };
        let Some(ready) = open.compose.take_ready() else { return; };
        let result = ready.and_then(|source| {
            if !open.card.account_valid() || !crate::mail_compose::eligible(&open.card) {
                return Err("The Mail account or card changed; reopen it".into());
            }
            self.live.session_mut(&Self::tile_key(&open.key))
                .ok_or_else(|| "This card's assistant is unavailable".to_string())?.compose_reply(&source)
        });
        match result {
            Ok(()) => { open.compose.requested = true; self.select_chat(cx, true); }
            Err(error) => open.compose.error = Some(error),
        }
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
                self.presentation.activate();
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
        if let Some((app, route)) = self.live.take_open_request() {
            self.hide_workspace(cx);
            cx.widget_action(self.uid, crate::glance_panel::ShellGlancePanelAction::Open { app, route });
        }
    }
}

impl Widget for ShellGlanceSheet {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let screen = cx.turtle().rect();
        self.refresh_publication(cx);
        #[cfg(any(feature = "app-hub", native_mobile))]
        if self.is_open() { self.poll_review(cx); }
        // Every frame, open or closed: the kit keeps its overlay in tree order.
        self.d.begin_surface(cx);
        let mut tok = self.d.tokens(self.tokens);
        if let Some(state) = scope.data.get_mut::<crate::WmState>().filter(|state| state.style.target.mobile()) {
            tok.notifications.surface.text = if state.style.dark {crate::shell::rgb(242, 243, 248)} else {crate::shell::rgb(26, 26, 32)};
            tok.notifications.surface.background = if state.style.dark {crate::shell::rgb(28, 30, 38)} else {crate::shell::rgb(255, 255, 255)};
            tok.notifications.surface.background_alpha = 1.0;
        }
        let ink = tok.notifications.surface.text;
        if self.is_open() && self.fullscreen {
            self.saved_bar_appearance.get_or_insert(cx.display_context.system_bar_appearance);
            cx.set_system_bar_appearance(if ink.x + ink.y + ink.z < 1.5 { SystemBarAppearance::DarkIcons } else { SystemBarAppearance::LightIcons });
        } else if let Some(previous) = self.saved_bar_appearance.take() { cx.set_system_bar_appearance(previous); }
        self.style_tabs(cx, ink);
        if let Some(mut chat) = self.chat.borrow_mut::<crate::card_chat::CardChat>() { chat.set_ink(cx, ink); }
        if let Some(mut mail) = self.mail.borrow_mut::<crate::mail_clip::MailClip>() { mail.set_ink(cx, ink); }
        if let Some(state) = scope.data.get_mut::<crate::WmState>() {
            self.insets = state.phone.insets;
            self.return_rect = self.open.as_ref().filter(|_| state.phone.screen == crate::mobile::PhoneScreen::Home)
                .and_then(|o| state.phone.pages.summary_rect(&o.key, state.phone.viewport));
        }
        if self.is_open() {
            let open = self.open.as_ref().unwrap();
            let mut background = tok.notifications.surface.background;
            background.w = 1.0;
            let card_h = crate::glance_card::measured_height(&Self::tile_key(&open.key)).unwrap_or(UNMEASURED_CARD);
            let destination = if self.fullscreen {
                let mut insets = self.insets;
                // KeyboardView already subtracts IME height from this viewport.
                if self.keyboard_visible { insets.bottom = 0.0; }
                insets.inset(screen)
            } else { sheet_rect(screen, if self.chatting || open.card.l0.as_ref().is_some_and(|l| l.mail.is_some()) {600.0} else {card_h + if self.chat_available {TABS} else {0.0}}) };
            let sheet = self.presentation.rect(destination);
            self.sheet = sheet;
            #[cfg(any(feature = "app-hub", native_mobile))]
            let reviewing = self.review.is_active();
            #[cfg(not(any(feature = "app-hub", native_mobile)))]
            let reviewing = false;
            let native_mail = open.card.l0.as_ref().is_some_and(|l| l.mail.is_some());
            let focus_layout = self.fullscreen && self.keyboard_visible && destination.size.y < 260.0
                && native_mail && !self.chatting && !reviewing
                && cx.has_key_focus(self.mail.text_input(cx, ids!(body)).area());
            if self.focus_layout != focus_layout { self.close_touch = None; }
            self.focus_layout = focus_layout;
            if self.fullscreen {
                if self.presentation.covers_background() { self.d.solid(cx, screen, background); }
                else { self.round.color = background; self.round.radius = self.presentation.radius(); self.round.draw_abs(cx, sheet); }
            } else {
                self.d.solid(cx, screen, vec4(0.0, 0.0, 0.0, 0.55));
                self.d.card(cx, sheet, &tok.notifications.surface);
            }
            self.visible_sheet = sheet.clip((screen.pos, screen.pos + screen.size));
            cx.begin_turtle(Walk::abs_rect(self.visible_sheet), Layout::default());
            let close = close_rect(sheet);
            #[cfg(any(feature = "app-hub", native_mobile))]
            let heading = if self.review.is_active() { "Mail reply" } else { &open.card.title };
            #[cfg(not(any(feature = "app-hub", native_mobile)))]
            let heading = &open.card.title;
            if !focus_layout {
                self.d.label_elided(cx, rect(sheet.pos.x + PAD + 4.0, sheet.pos.y + 4.0, sheet.size.x - PAD * 2.0 - CLOSE - 8.0, HEADER - 4.0), true, 14.0, ink, HAlign::Left, heading);
                self.d.icon_centered(cx, Ico::Close, close, 14.0, ink);
            }
            let mut card = card_rect(sheet);
            let key = Self::tile_key(&open.key);
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
                // Layout at the final size throughout the reveal. The surface
                // translates and clips; text is never squeezed/reflowed each frame.
                let body_sheet = Rect { pos: sheet.pos, size: destination.size };
                let (pane, tabs) = workspace_rects(body_sheet, self.chat_available || native_mail, focus_layout);
                card = pane;
                if let Some(tabs) = tabs { self.tabs.draw_walk_all(cx, scope, Walk::abs_rect(tabs)); }
                if !self.chatting && crate::mail_compose::eligible(&open.card) {
                    let height = 82.0_f64.min(card.size.y);
                    let dock = rect(card.pos.x, card.pos.y + card.size.y - height, card.size.x, height);
                    card.size.y = (card.size.y - height).max(0.0);
                    self.compose.button(cx, ids!(action)).set_text(cx, if open.compose.busy() { "Preparing reply…" } else { "Compose reply" });
                    self.compose.button(cx, ids!(action)).set_disabled(cx, open.compose.busy() || !self.chat_available || !open.card.account_valid());
                    self.compose.label(cx, ids!(hint)).set_text(cx, open.compose.error.as_deref().unwrap_or("Draft only · Review recipient before sending."));
                    if let Some(mut label) = self.compose.label(cx, ids!(hint)).borrow_mut() { label.draw_text.color = crate::shell::alpha(ink, 0.72); }
                    self.compose.draw_walk_all(cx, scope, Walk::abs_rect(dock));
                }
                if self.chatting {
                    if let (Some(session), Some(mut chat)) = (self.live.session_mut(&key), self.chat.borrow_mut::<crate::card_chat::CardChat>()) { chat.sync(cx, session); }
                    self.chat.draw_walk_all(cx, scope, Walk::abs_rect(rect(body_sheet.pos.x, card.pos.y, body_sheet.size.x, card.size.y)));
                } else if open.card.l0.as_ref().is_some_and(|l| l.mail.is_some()) {
                    if let Some(mut mail) = self.mail.borrow_mut::<crate::mail_clip::MailClip>() {
                        mail.set_keyboard(cx, self.keyboard_visible);
                        mail.set_focus_layout(cx, focus_layout);
                    }
                    self.mail.draw_walk_all(cx, scope, Walk::abs_rect(rect(body_sheet.pos.x, card.pos.y, body_sheet.size.x, card.size.y)));
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
            cx.end_turtle();
            let layout = format!("{} sheet@{},{},{},{} card@{},{},{},{} close@{},{}", open.key, sheet.pos.x as i32, sheet.pos.y as i32, sheet.size.x as i32, sheet.size.y as i32, card.pos.x as i32, card.pos.y as i32, card.size.x as i32, card.size.y as i32, (close.pos.x + close.size.x * 0.5) as i32, (close.pos.y + close.size.y * 0.5) as i32);
            if layout != self.logged && !self.presentation.moving() {
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
        if let Some(frame) = self.frame.is_event(event) {
            let dt = frame.time - self.frame_time;
            self.frame_time = frame.time;
            self.presentation.step(dt, Self::reduced(cx));
            if self.presentation.moving() { self.frame = cx.new_next_frame(); }
            else { log!("glance workspace: presentation {:?}", self.presentation.phase); }
            self.redraw(cx);
        }
        if !self.is_open() { return; }
        self.poll_compose(cx);
        #[cfg(any(feature = "app-hub", native_mobile))]
        self.poll_review(cx);
        if let Event::TouchUpdate(e) = event {
            use makepad_platform::event::TouchState;
            if self.close_touch.as_ref().is_some_and(|(uid, _, _)| !e.touches.iter().any(|t|
                t.uid == *uid && t.state != TouchState::Start)) {
                self.close_touch = None;
            }
            let sheet = self.sheet;
            let closes = |p| (!self.focus_layout && contains(close_rect(sheet), p)) || (!self.fullscreen && !contains(sheet, p));
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
                            if should_close { self.dismiss(cx); return; }
                        }
                    }
                }
            }
            if consumed { return; }
        }
        match event {
            Event::MouseDown(e) if (!self.focus_layout && contains(close_rect(self.sheet), e.abs)) || (!self.fullscreen && !contains(self.sheet, e.abs)) => {
                self.dismiss(cx);
                return;
            }
            Event::KeyDown(e) if e.key_code == KeyCode::Escape => {
                self.back(cx);
                return;
            }
            _ => {}
        }
        #[cfg(any(feature = "app-hub", native_mobile))]
        if self.review.is_active() {
            self.review.handle_event(event);
            if !self.review.is_active() && self.open.as_ref().is_some_and(|o|
                (o.card.card_id == "host-review" && o.card.l0.is_none() && o.card.body.is_empty())
                || crate::glance::card(&o.key).is_none()) {
                self.close(cx);
            }
            self.redraw(cx);
            return; // Host review is modal within this sheet; no L0 NAV dispatch.
        }
        if !self.focus_layout && (self.chat_available || self.mail_binding().is_some()) {
            let actions = cx.capture_actions(|cx| self.tabs.handle_event(cx, event, scope));
            if self.tabs.button(cx, ids!(card_tab)).clicked(&actions) { self.select_chat(cx, false); return; }
            if self.tabs.button(cx, ids!(chat_tab)).clicked(&actions) { self.select_chat(cx, true); return; }
        }
        if !self.chatting && self.open.as_ref().is_some_and(|o| crate::mail_compose::eligible(&o.card)) {
            let actions = cx.capture_actions(|cx| self.compose.handle_event(cx, event, scope));
            if self.compose.button(cx, ids!(action)).clicked(&actions) {
                let open = self.open.as_mut().unwrap();
                open.compose.start(&open.card);
                self.redraw(cx);
                return;
            }
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
            let show_reply = self.chat.borrow_mut::<crate::card_chat::CardChat>().is_some_and(|mut chat| chat.take_reply_request());
            if show_reply {
                if let Some(mut mail) = self.mail.borrow_mut::<crate::mail_clip::MailClip>() {
                    mail.show_details(cx, false);
                }
                self.select_chat(cx, false);
            }
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
        // Splash state lives in its resident isolate, not InstanceStore. Be
        // conservative: once interacted with, do not evict its clean cache slot.
        if self.open.as_ref().is_some_and(|o| o.card.l0.is_none())
            && matches!(event, Event::TextInput(_) | Event::MouseDown(_) | Event::TouchUpdate(_)) {
            self.script_interacted = true;
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

    fn workspace_widget(cx: &mut Cx) -> WidgetRef {
        cx.with_vm(|vm| {
            vm.bx.captured_errors = Some(Vec::new());
            makepad_widgets::script_mod(vm);
            for (name, source) in [("base", crate::theme::BUNDLED_TOKYO_NIGHT_SPLASH.to_string()), ("shell", crate::theme::shell_splash_block(crate::theme::BUNDLED_TOKYO_NIGHT_SPLASH))] {
                let mut code = source.lines().skip_while(|line| line.trim().is_empty() || line.trim().starts_with("//")).collect::<Vec<_>>().join("\n");
                code.push_str("\ntrue\n");
                vm.eval(ScriptMod {cargo_manifest_path: env!("CARGO_MANIFEST_DIR").into(), module_path: name.into(), file: "theme.splash".into(), line: 0, column: 0, code, values: vec![]});
            }
            crate::shell::ui::script_mod(vm);
            crate::card_chat::script_mod(vm); crate::mail_clip::script_mod(vm); super::script_mod(vm);
            let value = script_eval!(vm, {use mod.widgets.* ShellGlanceSheet{}});
            let widget = WidgetRef::script_from_value(vm, value);
            let errors = vm.take_errors(); assert!(errors.is_empty(), "{errors:?}");
            widget
        })
    }

    #[test]
    fn dismiss_and_cached_switch_keep_native_inputs_and_active_mode() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let widget = workspace_widget(&mut cx);
        let mut sheet = widget.borrow_mut::<ShellGlanceSheet>().unwrap();
        let source = opened(&mut cx);
        sheet.open = source.open;
        sheet.presentation.activate();
        sheet.chatting = true;
        sheet.chat_available = true;
        let input = sheet.chat.text_input(&cx, ids!(input));
        input.set_text(&mut cx, "Unsent Wednesday change");
        let mail_input = sheet.mail.text_input(&cx, ids!(body));
        mail_input.set_text(&mut cx, "Human correction kept");
        sheet.dismiss(&mut cx);
        assert!(!sheet.is_open());
        assert!(sheet.chatting);
        assert!(sheet.open.is_some());
        assert_eq!(input.text(), "Unsent Wednesday change");
        sheet.retain_current(&mut cx);
        assert_eq!(sheet.retained.len(), 1);
        let old = &sheet.retained[0];
        assert!(old.chatting && old.dirty(&cx));
        assert_eq!(old.chat.text_input(&cx, ids!(input)).text(), "Unsent Wednesday change");
        assert_eq!(old.mail.text_input(&cx, ids!(body)).text(), "Human correction kept");
        assert!(sheet.chat.text_input(&cx, ids!(input)).text().is_empty(), "a different card cannot inherit this text");
    }

    #[test]
    fn mode_row_precedes_the_active_pane_at_every_viewport_size() {
        for height in [820.0, 340.0, 820.0] {
            let (pane, tabs) = workspace_rects(rect(0.0, 0.0, 380.0, height), true, false);
            let tabs = tabs.unwrap();
            assert_eq!(tabs.pos.y, HEADER);
            assert_eq!(tabs.pos.y + tabs.size.y, pane.pos.y);
            assert_eq!(pane.pos.y + pane.size.y, height - PAD);
            assert!(tabs.size.y >= 44.0);
        }
        let viewport = rect(0.0, 24.0, 760.0, 96.0);
        let (pane, tabs) = workspace_rects(viewport, true, true);
        assert!(tabs.is_none(), "typing in a short landscape viewport temporarily hides navigation");
        assert_eq!(pane, viewport, "the keyboard must not consume the entire editor below fixed headers");
    }

    #[test]
    fn email_and_chat_share_one_row_on_narrow_phones() {
        use makepad_widgets::makepad_draw::cx_draw::CxDraw;
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let widget = workspace_widget(&mut cx);
        let tabs = widget.borrow::<ShellGlanceSheet>().unwrap().tabs.clone();
        let pass = DrawPass::new(&mut cx); let mut list = DrawList2d::new(&mut cx);
        for width in [280.0, 313.0, 372.0] {
            let size = dvec2(width, 52.0); pass.set_size(&mut cx, size);
            let event = DrawEvent::default();
            {
            let mut draw = CxDraw::new(&mut cx, &event); let mut draw = Cx2d::new(&mut draw);
            draw.begin_pass(&pass, Some(1.0)); list.begin_always(&mut draw);
            draw.begin_root_turtle(size, Layout::default());
            tabs.draw_walk_all(&mut draw, &mut Scope::empty(), Walk::fixed(width, 52.0));
            draw.end_turtle(); list.end(&mut draw); draw.end_pass(&pass);
            }
            let areas = [ids!(card_tab), ids!(chat_tab)].map(|path| tabs.button(&cx, path).area().rect(&cx));
            for r in areas { assert_eq!(r.pos.y, 0.0); assert!(r.size.y >= 44.0 && r.size.x >= 64.0); assert!(r.pos.x + r.size.x <= width); }
            assert!(areas[0].pos.x < areas[1].pos.x);
        }
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

    fn opened(cx: &mut Cx) -> ShellGlanceSheet {
        let mut sheet = cx.with_vm(ShellGlanceSheet::script_new);
        sheet.presentation.activate();
        sheet.sheet = rect(20., 60., 340., 400.);
        sheet.publication_generation = crate::glance::generation();
        sheet.open = Some(Open { key: "test/card".into(), compose: Default::default(), card: GlanceCard {
            account: None,
            app: "test".into(), card_id: "card".into(), title: "Card".into(), summary: String::new(), priority: 0,
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
    fn workspace_owns_all_pointer_streams_even_outside_its_opening_rectangle() {
        use makepad_platform::event::{TouchPoint, TouchUpdateEvent, TouchState};
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut sheet = opened(&mut cx);
        let event = |state, abs| Event::TouchUpdate(TouchUpdateEvent {time: 0.0, window_id: CxWindowPool::id_zero(), modifiers: Default::default(),
            touches: vec![TouchPoint {state, abs, time: 0.0, uid: 42, rotation_angle: 0.0, force: 0.0, radius: dvec2(1.0, 1.0), handled: Default::default(), sweep_lock: Default::default()}]});
        for state in [TouchState::Start, TouchState::Move, TouchState::Stop] {
            assert!(sheet.accepts_pointer(&event(state, dvec2(5.0, 500.0))));
        }
        sheet.dismiss(&mut cx);
        assert!(!sheet.accepts_pointer(&event(TouchState::Start, dvec2(100.0, 200.0))));
        assert!(sheet.open.is_some(), "dismissing preserves the resident task");
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
    fn compose_promotion_opens_saved_editor_and_retains_unsent_chat() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let widget = workspace_widget(&mut cx);
        let mut sheet = widget.borrow_mut::<ShellGlanceSheet>().unwrap();
        let mut original = opened(&mut cx).open.unwrap();
        original.card.app = "os.mail".into();
        original.card.account = Some("a1".into());
        original.card.card_id = format!("mail-{}", "1".repeat(40));
        original.key = original.card.key();
        original.card.l0 = Some(std::sync::Arc::new(crate::glance::L0Source {
            source: "view root Surface { TextBody(text: \"\") }".into(), data: serde_json::json!({}), mail: None,
        }));
        original.compose.requested = true;
        let mut replacement = original.card.clone();
        let binding = crate::mail_card::Binding { publisher: "os.mail".into(), account: "a1".into(),
            card_id: replacement.card_id.clone(), draft_id: "draft-promoted".into(), draft_revision: 1,
            source_message: serde_json::json!({"folder":"Inbox","message":"second-email"}), chat_thread: "reply-promoted".into() };
        std::sync::Arc::make_mut(replacement.l0.as_mut().unwrap()).mail = Some(binding);
        sheet.open = Some(original);
        sheet.chatting = true;
        sheet.chat.text_input(&cx, ids!(input)).set_text(&mut cx, "Keep my next question");
        sheet.replace_publication(&mut cx, replacement);
        assert!(!sheet.chatting);
        assert_eq!(sheet.tabs.button(&cx, ids!(card_tab)).text(), "Email");
        assert_eq!(sheet.mail.borrow::<crate::mail_clip::MailClip>().unwrap().opened_draft(), Some("draft-promoted"));
        assert_eq!(sheet.chat.text_input(&cx, ids!(input)).text(), "Keep my next question");
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

    #[test]
    fn six_workspaces_do_not_evict_an_interacted_script_or_unsent_chat() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let widget = workspace_widget(&mut cx);
        let mut sheet = widget.borrow_mut::<ShellGlanceSheet>().unwrap();
        for index in 0..6 {
            let source = opened(&mut cx);
            sheet.open = source.open;
            sheet.open.as_mut().unwrap().key = format!("test/card-{index}");
            sheet.script_interacted = index == 0;
            if index == 1 { sheet.chat.text_input(&cx, ids!(input)).set_text(&mut cx, "Keep my unfinished question"); }
            sheet.retain_current(&mut cx);
        }
        assert_eq!(sheet.retained.len(), 5, "two dirty and three clean workspaces");
        assert!(sheet.retained.iter().any(|entry| entry.open.key == "test/card-0" && entry.dirty(&cx)));
        assert!(sheet.retained.iter().any(|entry| entry.open.key == "test/card-1" && entry.dirty(&cx)));
        assert!(!sheet.retained.iter().any(|entry| entry.open.key == "test/card-2"));
    }

    #[test]
    fn a_new_background_layout_cannot_erase_local_script_work() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut sheet = opened(&mut cx);
        let original = sheet.open.as_ref().unwrap().card.clone();
        sheet.script_interacted = true;
        let mut replacement = original.clone(); replacement.body = "View{}".into();
        sheet.replace_publication(&mut cx, replacement);
        assert_eq!(sheet.open.as_ref().unwrap().card, original);
        let mut metadata = original.clone(); metadata.title = "Fresh summary".into();
        sheet.replace_publication(&mut cx, metadata.clone());
        assert_eq!(sheet.open.as_ref().unwrap().card, metadata);
        assert!(sheet.script_interacted);
    }

}
