//! The AI pane: a left-side column that hosts the assistant and PUSHES
//! the desk in — never over it.
//!
//! The chat is NOT the window manager's — it is an app of its own
//! (`apps/aichat`), seated here instead of in a tile, in one of two
//! bodies. On a desktop it is the aichat CHILD PROCESS, launched on first
//! use and presented by one `MpRunView` that forwards its input. Where
//! the chat is linked in as a MODULE — the web superbuild, or a desktop
//! that switched `aichat` to module hosting — the body is the chat's own
//! root, `mod.widgets.AiChatOverlay{}`, instantiated BY NAME on first open
//! exactly as a Window's F10 slot does; the WM's services reach it as
//! in-process links (`pane_links.rs`), never as frames.
//!
//! This widget owns nothing but the slot. It is the first child of the
//! desk row, LEFT of the desk, and it reserves a STRIP: its own walk width
//! (decision 17). The slide (easeOutQuint, 260 ms in, 200 ms out) animates
//! that strip from nothing to the card's width plus one outer gap, and
//! the desk — a Fill sibling in the same row — narrows by exactly that
//! every frame, so the tiles reflow beside the pane instead of vanishing
//! under it; the card itself sits right-aligned in the strip, an opaque
//! ground with a 2 px edge on its RIGHT. Open, the pane takes the pointer
//! inside its card and the keyboard; closed, the strip is zero and the
//! card sits just off the desk's left edge, still drawn and ticking (the
//! child's 8 ms tick, the module's engine), so the assistant — started
//! with the desktop, never on the first F10 — is configured, presenting
//! and answering before the pane ever slides in, and a turn in flight
//! finishes behind a hidden one.

use super::ui::{contains, inset, rect, HAlign, Ico, ShellDraw};
use super::{MaterialTokens, ShellPalette, ShellTokens};
use crate::desk::ease_out_quint;
use crate::system_chat::view::mac;
use crate::system_chat::view::Grip;
use crate::hub::ClientId;
use crate::run_view::MpRunView;
use makepad_widgets::makepad_script::script_eval;
use makepad_widgets::makepad_script::trap::NoTrap;
use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    // A Mac's spinner: twelve ticks, the brightest at `head`, the rest fading
    // behind it ([`DrawMacSpinner`]).
    set_type_default() do #(DrawMacSpinner::script_shader(vm)) {
        ..mod.draw.DrawQuad
        color: #808080
        head: 0.0
        pixel: fn() {
            let centre = self.rect_size * 0.5
            let radius = min(centre.x, centre.y)
            let p = self.pos * self.rect_size - centre
            let turn = 6.2831853
            let sector = turn / 12.0
            let a = atan2(p.x, 0.0 - p.y)
            let i = floor(a / sector + 0.5)
            let t = a - i * sector
            let r = length(p)
            let q = vec2(r * cos(t), r * sin(t))
            let tick = radius * 0.085
            let inner = radius * 0.46
            let outer = radius * 0.98 - tick
            let d = length(vec2(q.x - clamp(q.x, inner, outer), q.y)) - tick
            let cover = clamp(0.5 - d * 2.0, 0.0, 1.0)
            let age = modf(self.head - i + 24.0, 12.0)
            let a2 = cover * self.color.w * (1.0 - age / 12.0 * 0.82)
            return vec4(self.color.rgb * a2, a2)
        }
    }

    mod.widgets.ShellAiPaneBase = #(ShellAiPane::register_widget(vm))

    mod.widgets.ShellAiPane = set_type_default() do mod.widgets.ShellAiPaneBase {
        // The strip: zero while closed, the card plus a gap when open. The
        // widget rewrites this width itself as it slides.
        width: 0
        height: Fill
        draw_card +: { color: mod.wm_theme.background }
        draw_edge +: { color: mod.wm_theme.accent }
        d +: {}
        run := MpRunView{}
    }
}

/// Seconds the slide-in takes.
const SLIDE_IN: f64 = 0.26;
/// Seconds the slide-out takes.
const SLIDE_OUT: f64 = 0.20;
/// The pane's width, before the screen-fraction clamp.
const PANE_WIDTH: f64 = 440.0;
/// The most of the desk the pane may take.
const PANE_MAX_FRACTION: f64 = 0.4;
/// The edge on the pane's right.
const EDGE: f64 = 2.0;
/// The narrowest the card ever gets.
const PANE_MIN_WIDTH: f64 = 240.0;

/// The pane's geometry as plain numbers, testable without a `Cx`: the card
/// width for a row that wide, the strip the row reserves at slide position
/// `t`, and the card's rect inside a strip.
pub struct PaneStrip;

impl PaneStrip {
    /// The card's width: 440 px, at most 40 % of the row, never under 240.
    pub fn card_width(row_width: f64) -> f64 {
        PANE_WIDTH.min(row_width * PANE_MAX_FRACTION).max(PANE_MIN_WIDTH)
    }

    /// What the row reserves at slide position `t` (0 hidden, 1 shown):
    /// the card plus one outer gap, eased — the desk beside it narrows by
    /// exactly this.
    pub fn strip(t: f64, card_width: f64, gap: f64) -> f64 {
        (ease_out_quint(t) * (card_width + gap)).max(0.0)
    }

    /// The card, right-aligned in a strip of `strip` px starting at
    /// `row.pos.x`: at rest one gap in from the row's left edge, and at
    /// strip zero fully off the row's left edge. Full desk height minus
    /// the gaps.
    pub fn card_rect(row: Rect, strip: f64, card_width: f64, gap: f64) -> Rect {
        let x = row.pos.x + strip - card_width;
        let y = row.pos.y + gap;
        let h = (row.size.y - gap * 2.0).max(100.0);
        Rect { pos: dvec2(x, y), size: dvec2(card_width, h) }
    }
}

/// A Mac's spinner: twelve ticks; `head` (0 to 11) is the brightest.
#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawMacSpinner {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    pub color: Vec4f,
    #[live]
    pub head: f32,
}

thread_local! {
    static MAC_CARD: std::cell::Cell<Option<Rect>> = const { std::cell::Cell::new(None) };
}

/// The assistant's card on a Mac, while it is up (a chat panel stands beside
/// it instead of over it).
pub fn mac_card() -> Option<Rect> {
    MAC_CARD.with(|c| c.get())
}

/// A Mac's title strip: the close light and what moves the card.
const MAC_BAR: f64 = 44.0;
/// Start near the other Mac assistant panels' size; a conversation can be
/// given more room by dragging the lower edge.
const MAC_INITIAL_HEIGHT: f64 = 400.0;
/// The app's empty state, its model-folder action and composer need this
/// much room; shorter screens still bound the frame to the available area.
const MAC_MIN_HEIGHT: f64 = 320.0;
const MAC_RESIZE_EDGE: f64 = 6.0;
/// Room above and below what stands in for the app.
const MAC_FIT_PAD: f64 = 24.0;
/// The spinner's step.
const MAC_SPIN_S: f64 = 1.0 / 12.0;

/// The card's words while the app is on its way: title and note.
const MAC_LOADING: (&str, &str) = ("Getting AI Ready\u{2026}", "This can take a few minutes the first time.");
/// The card's words when it could not open.
const MAC_FAILED: (&str, &str) = ("AI Couldn\u{2019}t Start", "Try again. If it still won\u{2019}t open, quit OctoSense and open it again.");

/// What the card shows in place of the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacBody {
    /// The app has no frame yet: however it is getting one.
    Loading,
    /// It stopped, or could not be built.
    Failed,
}

/// What a press on the card asked of the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacAsk {
    Close,
    Cancel,
    Retry,
}

/// The Mac card's own state.
#[derive(Default)]
struct MacPane {
    /// A test hook holds this state ([`crate::App::mac_ai_test_action`]).
    forced: Option<MacBody>,
    /// The child died while the card was up.
    failed: bool,
    /// Where the person moved it.
    placed: Option<Rect>,
    drag: Option<(Grip, Vec2d, Rect)>,
    pressed_bar: f64,
    close: Rect,
    /// Cancel or Close (left of the default), Try Again.
    left: Rect,
    right: Rect,
    hover: Option<Rect>,
    down: Option<Rect>,
    ask: Option<MacAsk>,
    spin: u32,
    spun: bool,
    spin_timer: Timer,
    /// What the last draw showed in place of the app.
    shown: Option<MacBody>,
    /// How tall that needs the card ([`mac_fit_height`]).
    fit: Option<f64>,
    /// Something held the keys when the card took them.
    keys_before: bool,
}

/// What the card says while the app is on its way, and when it failed: never
/// a compiler's words.
pub fn mac_body(has_frame: bool, status: &str, failed: bool) -> Option<MacBody> {
    if failed || status.starts_with("build failed") {
        Some(MacBody::Failed)
    } else if has_frame {
        None
    } else {
        Some(MacBody::Loading)
    }
}

/// What stands in for the app, top to bottom (`mac_draw_body`): the spinner
/// (22), the title 8 under it, the note, and Cancel 12 under that; or the glyph
/// (36), the title, the note's `lines` (18 each) and the buttons.
fn mac_block(body: MacBody, lines: usize) -> f64 {
    match body {
        MacBody::Loading => 110.0,
        MacBody::Failed => 108.0 + 18.0 * lines as f64,
    }
}

/// How tall the card is while something stands in for the app: its bar, then
/// what it says with room around it, as an alert is sized; never taller than
/// the card would be with the app in it.
fn mac_fit_height(body: MacBody, lines: usize) -> f64 {
    MAC_BAR + MAC_FIT_PAD * 2.0 + mac_block(body, lines) + 1.0
}

/// The initial Mac frame fits the work area, including unusually short screens.
fn mac_initial_card(work: Rect) -> Rect {
    let mut card = mac::default_column(work, &[], false);
    card.size.y = card.size.y.min(MAC_INITIAL_HEIGHT);
    card
}

fn mac_resize_edge(card: Rect) -> Rect {
    let height = MAC_RESIZE_EDGE.min(card.size.y);
    rect(card.pos.x, card.pos.y + card.size.y - height, card.size.x, height)
}

fn mac_resize_card(from: Rect, delta: Vec2d, work: Rect) -> Rect {
    let mut card = crate::system_chat::view::dragged(Grip::S, from, delta, work);
    let room = (work.pos.y + work.size.y - card.pos.y).max(0.0);
    card.size.y = card.size.y.max(MAC_MIN_HEIGHT.min(room));
    card
}

#[derive(Script, ScriptHook, Widget)]
pub struct ShellAiPane {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[live]
    draw_card: DrawColor,
    #[live]
    draw_edge: DrawColor,
    /// The shell kit, for the glass card; the flat card is the two fills
    /// above.
    #[live]
    d: ShellDraw,
    /// A Mac: the card's colours and its spinner.
    #[live]
    tokens: ShellTokens,
    #[live]
    spinner: DrawMacSpinner,
    #[rust]
    mac: MacPane,
    #[rust]
    open: bool,
    /// 0 = fully hidden, 1 = fully shown.
    #[rust]
    t: f64,
    #[rust]
    anim_started: Option<(f64, f64, bool)>,
    #[rust]
    client: Option<ClientId>,
    /// The desk ROW's rect (under the bar, pane and desk together) and the
    /// outer gap, as the WM last told us; the card width and the strip
    /// derive from them.
    #[rust]
    row_rect: Rect,
    #[rust]
    gap: f64,
    /// The strip this widget currently reserves (its own walk width).
    #[rust]
    strip: f64,
    #[rust]
    next_frame: NextFrame,
    /// The keyboard was asked for while the body had no live area;
    /// claimed right after its next draw.
    #[rust]
    focus_pending: bool,
    /// The chat's module root, when the chat runs in-process: made by
    /// name on the first open, kept for the life of the desk.
    #[rust]
    overlay: Option<WidgetRef>,
}

impl ShellAiPane {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The chat runs in this process: the body is its module root.
    pub fn is_local(&self) -> bool {
        self.overlay.is_some()
    }

    /// Seat the chat's module root here, made BY NAME: `mod.widgets.
    /// AiChatOverlay{}` exists when this build links `makepad-aichat` and
    /// called its `script_mod`. False, with one log line, when it does not.
    pub fn ensure_overlay(&mut self, cx: &mut Cx) -> bool {
        if self.overlay.is_some() {
            return true;
        }
        let overlay = cx.with_vm(|vm| {
            let widgets = vm.module(id!(widgets));
            let ty = vm.bx.heap.value_path(widgets, ids!(AiChatOverlay), NoTrap);
            if ty.as_object().is_none() {
                return None;
            }
            let value = script_eval!(vm, {
                use mod.widgets.*
                AiChatOverlay {}
            });
            Some(WidgetRef::script_from_value(vm, value))
        });
        match overlay {
            Some(overlay) => {
                cx.widget_tree_insert_child(self.widget_uid(), live_id!(overlay), overlay.clone());
                self.overlay = Some(overlay);
                true
            }
            None => {
                log!("wm: this build does not link the assistant (no mod.widgets.AiChatOverlay); nothing to seat in the pane");
                false
            }
        }
    }

    /// The material the card paints with; the next draw reads it.
    pub fn set_material(&mut self, m: MaterialTokens, palette: Option<ShellPalette>) {
        self.d.set_material(m);
        self.d.set_palette(palette);
    }

    pub fn client(&self) -> Option<ClientId> {
        self.client
    }

    pub fn set_client(&mut self, client: Option<ClientId>) {
        self.client = client;
    }

    /// Where the desk row is and how far from its edges the card sits.
    pub fn set_geometry(&mut self, row_rect: Rect, gap: f64) {
        self.row_rect = row_rect;
        self.gap = gap;
        self.apply_strip();
    }

    /// The card's width for the current row.
    fn card_width(&self) -> f64 {
        PaneStrip::card_width(self.row_rect.size.x)
    }

    /// The strip the row reserves right now, and the walk that claims it.
    fn apply_strip(&mut self) {
        // A Mac's card floats: the desk keeps its width.
        self.strip = if mac::active() { 0.0 } else { PaneStrip::strip(self.t, self.card_width(), self.gap) };
        self.view.walk.width = Size::Fixed(self.strip);
    }

    /// The reserved strip in px: what the desk beside the pane has lost.
    pub fn strip(&self) -> f64 {
        self.strip
    }

    /// The card is on the move: the desk snaps its tiles to the layout
    /// every frame instead of tweening after the strip.
    pub fn is_sliding(&self) -> bool {
        self.anim_started.is_some()
    }

    /// The card as drawn right now: right-aligned in the strip.
    fn card_rect(&self) -> Rect {
        if mac::active() {
            return self.mac_card_rect();
        }
        PaneStrip::card_rect(self.row_rect, self.strip, self.card_width(), self.gap)
    }

    /// A Mac: on the right, under the menu bar and above the Dock, or where
    /// the person moved it; off the right edge while hidden.
    fn mac_card_rect(&self) -> Rect {
        let work = mac::work_area(self.row_rect, crate::desktop::shelf_bounds());
        let mut rest = match self.mac.placed {
            Some(placed) => crate::system_chat::view::placed_pane(placed, work),
            None => mac_initial_card(work),
        };
        // What stands in for the app is as tall as what it says.
        if let Some(fit) = self.mac.fit {
            rest.size.y = rest.size.y.min(fit);
        }
        let off = (self.row_rect.pos.x + self.row_rect.size.x + 40.0 - rest.pos.x).max(0.0);
        rect(rest.pos.x + off * (1.0 - ease_out_quint(self.t)), rest.pos.y, rest.size.x, rest.size.y)
    }

    /// True while the pane is on screen at all (open, or sliding out).
    fn showing(&self) -> bool {
        self.open || self.t > 0.0
    }

    pub fn contains(&self, p: DVec2) -> bool {
        self.open && (self.mac.drag.is_some() || self.mac.down.is_some() || self.card_rect().contains(p))
    }

    pub fn set_open(&mut self, cx: &mut Cx, open: bool) {
        if self.open == open {
            return;
        }
        self.open = open;
        self.anim_started = Some((cx.seconds_since_app_start(), self.t, open));
        self.next_frame = cx.new_next_frame();
        if !open {
            // A hidden pane must not keep the keyboard: typing would go on
            // reaching the chat behind nothing.
            self.focus_pending = false;
            if self.overlay.is_some() {
                cx.set_key_focus(Area::Empty);
            } else {
                self.with_run_view(cx, |cx, v| v.release_keyboard(cx));
            }
        }
        self.redraw(cx);
    }

    /// The child is gone: no client, no picture, pane shut.
    pub fn reset(&mut self, cx: &mut Cx) {
        self.client = None;
        self.with_run_view(cx, |cx, v| v.clear_run_target(cx));
        // A Mac: the card stays up and says so, with Try Again.
        if mac::active() && self.open {
            self.mac.failed = true;
            self.redraw(cx);
            return;
        }
        self.set_open(cx, false);
    }

    /// Whether something held the keys when the card took them.
    pub fn mac_keys_before(&self) -> bool {
        self.mac.keys_before
    }

    /// What a press on a Mac's card asked for.
    pub fn take_mac_ask(&mut self) -> Option<MacAsk> {
        self.mac.ask.take()
    }

    /// A new child is on its way: the failure goes.
    pub fn clear_mac_failure(&mut self, cx: &mut Cx) {
        self.mac.failed = false;
        self.redraw(cx);
    }

    /// A test hook: hold a state the app would pass through too fast to see.
    pub fn force_mac_body(&mut self, body: Option<MacBody>) {
        self.mac.forced = body;
    }

    /// The pointer on a Mac's card, before the app gets it. True when the
    /// event was the card's.
    fn mac_pointer(&mut self, cx: &mut Cx, event: &Event) -> bool {
        if let Some((grip, start, from)) = self.mac.drag {
            match event {
                Event::MouseMove(e) => {
                    let work = mac::work_area(self.row_rect, crate::desktop::shelf_bounds());
                    self.mac.placed = Some(if grip == Grip::S {
                        mac_resize_card(from, e.abs - start, work)
                    } else {
                        crate::system_chat::view::dragged(grip, from, e.abs - start, work)
                    });
                    if grip == Grip::S {
                        cx.set_cursor(MouseCursor::NsResize);
                    }
                    cx.redraw_all();
                    return true;
                }
                Event::MouseUp(_) => {
                    self.mac.drag = None;
                    return true;
                }
                _ => {}
            }
        }
        let body = self.mac_showing_body();
        let controls = [Some(self.mac.close), body.then_some(self.mac.left), body.then_some(self.mac.right)];
        let under = |p: Vec2d| controls.iter().flatten().copied().find(|r| r.size.x > 0.0 && contains(*r, p));
        match event {
            Event::MouseMove(e) => {
                let hover = under(e.abs);
                if hover != self.mac.hover {
                    self.mac.hover = hover;
                    self.redraw(cx);
                }
                if contains(mac_resize_edge(self.card_rect()), e.abs) && hover.is_none() {
                    cx.set_cursor(MouseCursor::NsResize);
                    return true;
                }
                // The app's toolbar is under the bar: its tool hears the pointer leave.
                self.mac.down.is_some()
            }
            Event::MouseDown(e) => {
                if let Some(r) = under(e.abs) {
                    self.mac.down = Some(r);
                    self.redraw(cx);
                    return true;
                }
                if contains(mac_resize_edge(self.card_rect()), e.abs) {
                    self.mac.drag = Some((Grip::S, e.abs, self.card_rect()));
                    cx.set_cursor(MouseCursor::NsResize);
                    return true;
                }
                if contains(self.mac_bar(), e.abs) {
                    if e.time - self.mac.pressed_bar < 0.4 {
                        self.mac.placed = None;
                        self.mac.pressed_bar = 0.0;
                        cx.redraw_all();
                    } else {
                        self.mac.pressed_bar = e.time;
                        self.mac.drag = Some((Grip::Move, e.abs, self.card_rect()));
                    }
                    return true;
                }
                false
            }
            Event::MouseUp(e) => {
                let Some(down) = self.mac.down.take() else { return false };
                if contains(down, e.abs) {
                    self.mac.ask = if down == self.mac.close {
                        Some(MacAsk::Close)
                    } else if down == self.mac.right {
                        Some(MacAsk::Retry)
                    } else if self.mac.right.size.x > 0.0 {
                        Some(MacAsk::Close)
                    } else {
                        Some(MacAsk::Cancel)
                    };
                }
                self.redraw(cx);
                true
            }
            _ => false,
        }
    }

    /// The toolbar's band, which moves the card. Its right end is the app's
    /// New Chat, while the app is showing.
    fn mac_bar(&self) -> Rect {
        let r = self.card_rect();
        let tool = if self.mac_showing_body() { 0.0 } else { 48.0 };
        rect(r.pos.x, r.pos.y, (r.size.x - tool).max(0.0), MAC_BAR)
    }

    /// The card shows a state of its own, not the app.
    fn mac_showing_body(&self) -> bool {
        self.mac.left.size.x > 0.0
    }

    /// The card on a Mac: a rounded panel in the shell's material, the app
    /// or what stands in for it.
    fn draw_mac(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, Layout { clip_x: false, clip_y: false, ..Layout::default() });
        // What stands in for the app, if anything: the card fits it.
        let (failed, forced) = (self.mac.failed, self.mac.forced);
        let state = if self.overlay.is_some() {
            None
        } else {
            self.with_run_view(cx, |_, v| forced.or_else(|| mac_body(v.has_frame(), v.status_line(), failed))).unwrap_or(None)
        };
        self.d.begin_surface(cx);
        let lines = match state {
            Some(MacBody::Failed) => {
                let w = (self.card_rect().size.x - 58.0).max(1.0);
                self.d.wrap_lines(cx, false, 13.0, MAC_FAILED.1, w, false).len()
            }
            _ => 0,
        };
        self.mac.fit = state.map(|body| mac_fit_height(body, lines));
        let r = self.card_rect();
        MAC_CARD.with(|c| c.set(self.open.then_some(r)));
        mac::publish(2, self.showing().then_some(r));
        let m = self.d.material();
        let tok = self.d.tokens(self.tokens);
        let inks = mac::Inks::of(&tok);
        self.d.card(cx, r, &tok.popups);
        let inner = inset(r, m.border_width.max(1.0));
        let round = ((m.corner_radius - 1.0) * 0.5).max(0.0);
        if state.is_some() {
            // What stands in for the app looks as the app does, and as the
            // shell's own Ask panels: the title on the bar's grey over a rule,
            // the content's ground under it.
            self.d.rounded(cx, inner, round, inks.content);
            self.d.rounded(cx, rect(inner.pos.x, inner.pos.y, inner.size.x, (r.pos.y + MAC_BAR - inner.pos.y).max(0.0)), round, inks.bar);
            self.d.rounded(cx, rect(inner.pos.x, r.pos.y + MAC_BAR * 0.5, inner.size.x, MAC_BAR * 0.5), 0.0, inks.bar);
        } else {
            self.d.rounded(cx, inner, round, inks.even);
        }
        self.mac.spun = false;

        let half = ((m.corner_radius - m.border_width).max(0.0) * 0.5) as f32;
        if self.overlay.is_none() {
            self.with_run_view(cx, |_, v| {
                v.set_corner_radius(half);
                v.set_own_placeholder(true);
            });
        }
        // The app's own toolbar is the card's, with the lights over it, so the
        // card and the app read as one window. What stands in for the app has
        // a title of its own.
        if state.is_some() {
            self.d.label_elided(cx, rect(r.pos.x + 84.0, r.pos.y, (r.size.x - 168.0).max(1.0), MAC_BAR), true, 13.0, inks.ink, HAlign::Center, "Ask AI");
            self.d.solid(cx, rect(r.pos.x, r.pos.y + MAC_BAR - 1.0, r.size.x, 1.0), inks.hair);
        }

        let top = if state.is_some() { MAC_BAR } else { 1.0 };
        let body = rect(r.pos.x + 1.0, r.pos.y + top, (r.size.x - 2.0).max(1.0), (r.size.y - top - 1.0).max(1.0));
        match self.overlay.clone() {
            Some(overlay) => overlay.draw_walk_all(cx, scope, Walk::abs_rect(body)),
            None => while self.view.draw_walk(cx, scope, Walk::abs_rect(body)).is_step() {},
        }
        self.mac.left = Rect::default();
        self.mac.right = Rect::default();
        self.mac.shown = state;
        if let Some(state) = state {
            self.mac_draw_body(cx, body, state, &inks);
        }

        // A Mac panel's lights, over whatever is in the toolbar's place.
        let slot = rect(r.pos.x + 10.0, r.pos.y + (MAC_BAR - 24.0) * 0.5, 24.0, 24.0);
        self.mac.close = slot;
        let (over, down) = (self.mac.hover == Some(slot), self.mac.down == Some(slot));
        self.d.mac_panel_lights(cx, slot.pos.x + 6.0, slot.pos.y + 6.0, super::ui::PanelLights { dark: inks.dark, front: true, over, down });
        self.d.end_surface(cx);
        cx.end_turtle();
        // A closed pane still draws, off the row's edge, and nobody is watching it spin.
        if self.mac.spun && self.showing() && self.mac.spin_timer.is_empty() && !super::ui::reduce_motion() {
            self.mac.spin_timer = cx.start_timeout(MAC_SPIN_S);
        }
        if self.focus_pending && self.open {
            self.focus_keyboard(cx);
        }
        DrawStep::done()
    }

    /// Loading: a spinner, what is happening in plain words, and Cancel.
    /// Failed: why in one sentence, Close and Try Again. Either sits in the
    /// middle of what the card gives it ([`mac_block`]).
    fn mac_draw_body(&mut self, cx: &mut Cx2d, body: Rect, state: MacBody, inks: &mac::Inks) {
        let bw = 76.0;
        let (w, x) = ((body.size.x - 56.0).max(1.0), body.pos.x + 28.0);
        let lines = if state == MacBody::Failed { self.d.wrap_lines(cx, false, 13.0, MAC_FAILED.1, w, false) } else { Vec::new() };
        let top = (body.pos.y + ((body.size.y - mac_block(state, lines.len())) * 0.5).max(0.0)).round();
        match state {
            MacBody::Loading => {
                self.mac.spun = true;
                let s = rect((body.pos.x + (body.size.x - 22.0) * 0.5).round(), top, 22.0, 22.0);
                self.spinner.color = inks.dim;
                self.spinner.head = (self.mac.spin % 12) as f32;
                self.spinner.draw_abs(cx, s);
                self.d.label_elided(cx, rect(x, top + 30.0, w, 20.0), true, 13.0, inks.ink, HAlign::Center, MAC_LOADING.0);
                self.d.label_elided(cx, rect(x, top + 52.0, w, 18.0), false, 13.0, inks.dim, HAlign::Center, MAC_LOADING.1);
                let cancel = rect((body.pos.x + (body.size.x - bw) * 0.5).round(), top + 82.0, bw, 28.0);
                let pressed = self.mac.down == Some(cancel);
                mac::push_button(&mut self.d, cx, cancel, "Cancel", mac::Btn::Push, true, pressed, inks);
                self.mac.left = cancel;
            }
            MacBody::Failed => {
                let g = rect((body.pos.x + (body.size.x - 36.0) * 0.5).round(), top, 36.0, 36.0);
                self.d.icon(cx, Ico::Sparkle, g, inks.glyph);
                self.d.label_elided(cx, rect(x, top + 44.0, w, 20.0), true, 15.0, inks.ink, HAlign::Center, MAC_FAILED.0);
                let mut y = top + 68.0;
                for line in lines {
                    self.d.label_elided(cx, rect(x, y, w, 18.0), false, 13.0, inks.dim, HAlign::Center, &line);
                    y += 18.0;
                }
                let gap = 8.0;
                let lx = (body.pos.x + (body.size.x - bw * 2.0 - gap) * 0.5).round();
                let close = rect(lx, y + 12.0, bw, 28.0);
                let retry = rect(lx + bw + gap, y + 12.0, bw, 28.0);
                mac::push_button(&mut self.d, cx, close, "Close", mac::Btn::Push, true, self.mac.down == Some(close), inks);
                mac::push_button(&mut self.d, cx, retry, "Try Again", mac::Btn::Default, true, self.mac.down == Some(retry), inks);
                self.mac.left = close;
                self.mac.right = retry;
            }
        }
    }

    /// Advance the slide; true while it still moves.
    fn animate(&mut self, now: f64) -> bool {
        let Some((start, from, opening)) = self.anim_started else { return false };
        let dur = if opening { SLIDE_IN } else { SLIDE_OUT };
        let to = if opening { 1.0 } else { 0.0 };
        let p = ((now - start) / dur).clamp(0.0, 1.0);
        self.t = from + (to - from) * p;
        if p >= 1.0 {
            self.t = to;
            self.anim_started = None;
            self.apply_strip();
            return false;
        }
        self.apply_strip();
        true
    }

    pub fn with_run_view<R>(&mut self, cx: &mut Cx, f: impl FnOnce(&mut Cx, &mut MpRunView) -> R) -> Option<R> {
        let run = self.view.widget(cx, ids!(run));
        let mut view = run.borrow_mut::<MpRunView>()?;
        Some(f(cx, &mut view))
    }

    /// Claim the keyboard for the body. It is not drawn at all while the
    /// pane is hidden, so at open time its area is stale (or, the first
    /// time, empty): the claim is then kept pending and made right after
    /// the pane's next draw, when the area is live again.
    pub fn focus_keyboard(&mut self, cx: &mut Cx) {
        if mac::active() && !self.with_run_view(cx, |cx, v| v.holds_keyboard(cx)).unwrap_or(false) {
            self.mac.keys_before = !cx.key_focus().is_empty();
        }
        let ok = match &self.overlay {
            Some(overlay) => {
                let input = overlay.text_input(cx, ids!(panel.input));
                let live = input.area().is_valid(cx);
                if live {
                    input.set_key_focus(cx);
                }
                live
            }
            None => self.with_run_view(cx, |cx, v| v.focus_keyboard(cx)).unwrap_or(false),
        };
        self.focus_pending = !ok;
        if !ok {
            self.redraw(cx);
        }
    }
}

impl Widget for ShellAiPane {
    /// What a Mac's card says and where its controls are, for a test driver's /snap.
    fn text(&self) -> String {
        if !mac::active() {
            return String::new();
        }
        if !self.open {
            return "state: closed".into();
        }
        let mut lines = Vec::new();
        let hit = |name: &str, r: Rect, lines: &mut Vec<String>| {
            lines.push(format!("hit {name} | {:.0} {:.0} {:.0} {:.0}", r.pos.x, r.pos.y, r.size.x, r.size.y));
        };
        match self.mac.shown {
            None => lines.push("state: app".to_string()),
            Some(MacBody::Loading) => {
                lines.push("state: loading".to_string());
                lines.push(format!("title: {}", MAC_LOADING.0));
                lines.push(format!("note: {}", MAC_LOADING.1));
                lines.push("button: Cancel".to_string());
                hit("Cancel", self.mac.left, &mut lines);
            }
            Some(MacBody::Failed) => {
                lines.push("state: failed".to_string());
                lines.push(format!("title: {}", MAC_FAILED.0));
                lines.push(format!("note: {}", MAC_FAILED.1));
                lines.push("button: Close".to_string());
                lines.push("button: Try Again".to_string());
                hit("Close", self.mac.left, &mut lines);
                hit("Try Again", self.mac.right, &mut lines);
            }
        }
        hit("Close Light", self.mac.close, &mut lines);
        hit("Resize", mac_resize_edge(self.card_rect()), &mut lines);
        lines.join("\n")
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if self.mac.spin_timer.is_event(event).is_some() {
            self.mac.spin_timer = Timer::empty();
            if self.mac.spun {
                self.mac.spin = self.mac.spin.wrapping_add(1);
                self.redraw(cx);
            }
        }
        if mac::active() && self.showing() && self.mac_pointer(cx, event) {
            return;
        }
        if self.next_frame.is_event(event).is_some() {
            let now = cx.seconds_since_app_start();
            if self.animate(now) {
                self.next_frame = cx.new_next_frame();
            }
            // The strip changed: the desk beside us must lay out again,
            // not just this widget.
            cx.redraw_all();
        }
        // The body is ticked whether or not the pane shows (the child's
        // 8 ms tick timer is what makes it run at all; the module's engine
        // pumps on every event); pointer and keys reach it only while the
        // pane is on screen, so a hidden pane's stale area can never
        // swallow a click meant for a tile.
        if let Some(overlay) = self.overlay.clone() {
            let pointer = matches!(event, Event::MouseMove(_) | Event::MouseDown(_) | Event::MouseUp(_) | Event::Scroll(_));
            let keys = matches!(event, Event::KeyDown(_) | Event::KeyUp(_) | Event::TextInput(_));
            if (!pointer && !keys) || self.showing() {
                overlay.handle_event(cx, event, scope);
            }
            return;
        }
        if self.showing() || matches!(event, Event::Timer(_)) {
            self.view.handle_event(cx, event, scope);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if mac::active() {
            return self.draw_mac(cx, scope, walk);
        }
        // The strip is this widget's walk: `walk` arrives with its width,
        // the desk row lays the desk out after it. The card is drawn
        // right-aligned in the strip — off the row's left edge while
        // hidden, where a child process still keeps a real rect (so it is
        // configured, gets a swapchain and presents its frames) and a
        // module root keeps a live picture: the first slide-in shows the
        // assistant already running, not a "starting…" wash.
        cx.begin_turtle(walk, Layout::default());
        let r = self.card_rect();
        // Under glass the card is one glass quad, hoisted into the kit's
        // overlay list — and the body rides in that same list, drawn over
        // it: an overlay composites above the pass body, so a body left
        // in the pass would sit UNDER the glass. The accent edge belongs
        // to the flat look; glass keeps the material's own ring, with the
        // body inside it. (Both kit calls are no-ops under flat.)
        self.d.begin_surface(cx);
        let m = self.d.material();
        // The body's corner clip: inside the ring, concentric with the card
        // (an Sdf2d half-radius, like `draw_window_surface` halves the
        // window's). Square under flat, whose card is square.
        let child_half = if m.is_glass() { ((m.corner_radius - m.border_width).max(0.0) * 0.5) as f32 } else { 0.0 };
        self.with_run_view(cx, |_, v| {
            v.set_corner_radius(child_half);
            v.set_own_placeholder(false);
        });
        let inner = if m.is_glass() {
            self.d.glass_rect(cx, r, m.corner_radius, true);
            inset(r, m.border_width)
        } else {
            self.draw_card.draw_abs(cx, r);
            self.draw_edge.draw_abs(
                cx,
                Rect { pos: dvec2(r.pos.x + r.size.x - EDGE, r.pos.y), size: dvec2(EDGE, r.size.y) },
            );
            Rect { pos: r.pos, size: dvec2(r.size.x - EDGE, r.size.y) }
        };
        match self.overlay.clone() {
            Some(overlay) => overlay.draw_walk_all(cx, scope, Walk::abs_rect(inner)),
            None => {
                while self.view.draw_walk(cx, scope, Walk::abs_rect(inner)).is_step() {}
            }
        }
        self.d.end_surface(cx);
        cx.end_turtle();
        if self.focus_pending && self.open {
            // The body just drew: its area is live, the claim can land.
            self.focus_keyboard(cx);
        }
        DrawStep::done()
    }
}

impl crate::App {
    /// What a press on a Mac's card asked for.
    pub(crate) fn mac_ai_requests(&mut self, cx: &mut Cx) {
        let ask = self.with_ai_pane(cx, |_, p| p.take_mac_ask()).flatten();
        match ask {
            Some(MacAsk::Close) => self.close_ai_pane(cx),
            // Stop what is on its way as well: the next open starts it again.
            Some(MacAsk::Cancel) => {
                let client = self.ai_bus.pane_client;
                self.close_ai_pane(cx);
                if let Some(client) = client {
                    self.close_client(cx, client, false);
                }
            }
            Some(MacAsk::Retry) => {
                self.with_ai_pane(cx, |cx, p| p.clear_mac_failure(cx));
                if self.ai_bus.pane_client.is_none() {
                    self.launch_ai_pane(cx);
                }
                self.focus_pane(cx);
                self.redraw_all(cx);
            }
            None => {}
        }
    }

    /// Where the keys go when a Mac's card closes: `Some(true)` back to
    /// what had them before the card, `Some(false)` they stay where they
    /// are, `None` the window in front takes them.
    pub(crate) fn mac_ai_keys(&mut self, cx: &mut Cx) -> Option<bool> {
        if !mac::active() {
            return None;
        }
        let (local, before) = self.with_ai_pane(cx, |_, p| (p.is_local(), p.mac_keys_before()))?;
        if local || !before {
            return None;
        }
        let held = self.with_pane_run_view(cx, |cx, v| v.holds_keyboard(cx)).unwrap_or(false);
        (held || !cx.key_focus().is_empty()).then_some(held)
    }

    /// Escape closes a Mac's card, as it does the chat panels.
    pub(crate) fn mac_ai_escape(&mut self, cx: &mut Cx) -> bool {
        if !mac::active() || !self.ai_pane_is_open(cx) {
            return false;
        }
        let held = self.with_pane_run_view(cx, |cx, v| v.holds_keyboard(cx)).unwrap_or(false);
        // A field being typed in keeps its Escape; a list reading arrow keys has no use for it.
        if !held && !cx.key_focus().is_empty() && !cx.key_focus_kept() {
            return false;
        }
        self.close_ai_pane(cx);
        true
    }

    /// `--test-action ai-loading`, `ai-failed`: the card held in a state the
    /// app passes through too fast to look at (an installed or built app
    /// starts in a blink). Nothing is launched.
    pub(crate) fn mac_ai_test_action(&mut self, cx: &mut Cx, name: &str) -> bool {
        let body = match name {
            "ai-loading" => MacBody::Loading,
            "ai-failed" => MacBody::Failed,
            _ => return false,
        };
        self.sync_ai_pane_geometry(cx);
        self.with_ai_pane(cx, |cx, p| {
            p.force_mac_body(Some(body));
            p.set_open(cx, true);
        });
        self.redraw_all(cx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_card_starts_compact_and_fits_short_work_areas() {
        let work = rect(8.0, 40.0, 1384.0, 764.0);
        let card = mac_initial_card(work);
        assert_eq!(card, rect(1012.0, 40.0, 380.0, 400.0));
        assert_eq!(mac_resize_edge(card), rect(1012.0, 434.0, 380.0, 6.0));
        let short = rect(8.0, 40.0, 300.0, 180.0);
        assert_eq!(mac_initial_card(short), short);
    }

    /// Getting ready and a failure are as tall as what they say, as an alert
    /// is, not the conversation's 400 points with blank above and below.
    #[test]
    fn a_macs_card_fits_what_stands_in_for_the_app() {
        assert_eq!(mac_fit_height(MacBody::Loading, 0), 44.0 + 48.0 + 110.0 + 1.0);
        assert_eq!(mac_fit_height(MacBody::Failed, 2), 44.0 + 48.0 + 144.0 + 1.0);
        assert!(mac_fit_height(MacBody::Failed, 3) < MAC_INITIAL_HEIGHT);
    }

    #[test]
    fn mac_card_resize_keeps_its_minimum_without_overrunning_short_screens() {
        let work = rect(8.0, 40.0, 1384.0, 764.0);
        let initial = mac_initial_card(work);
        assert_eq!(mac_resize_card(initial, dvec2(0.0, -900.0), work), rect(1012.0, 40.0, 380.0, 320.0));
        assert_eq!(mac_resize_card(initial, dvec2(0.0, 900.0), work), rect(1012.0, 40.0, 380.0, 764.0));
        let short = rect(8.0, 40.0, 300.0, 180.0);
        assert_eq!(mac_resize_card(short, dvec2(0.0, -900.0), short), short);
    }

    /// What a Mac's card shows while its app is on the way: never the
    /// compiler's words, always the app once it has a frame.
    #[test]
    fn a_macs_card_says_what_is_happening() {
        assert_eq!(mac_body(false, "compiling makepad-widgets", false), Some(MacBody::Loading));
        assert_eq!(mac_body(false, "waiting for another build", false), Some(MacBody::Loading));
        assert_eq!(mac_body(false, "build failed: 3 errors", false), Some(MacBody::Failed));
        assert_eq!(mac_body(false, "", true), Some(MacBody::Failed), "the child died");
        assert_eq!(mac_body(true, "", false), None, "the app itself");
    }

    #[test]
    fn the_card_width_follows_the_row_within_its_bounds() {
        assert_eq!(PaneStrip::card_width(1400.0), 440.0);
        assert_eq!(PaneStrip::card_width(800.0), 320.0, "40 % of a narrow row");
        assert_eq!(PaneStrip::card_width(300.0), 240.0, "never under the minimum");
    }

    #[test]
    fn the_strip_grows_from_nothing_to_the_card_plus_a_gap() {
        assert_eq!(PaneStrip::strip(0.0, 440.0, 10.0), 0.0);
        assert_eq!(PaneStrip::strip(1.0, 440.0, 10.0), 450.0);
        let mid = PaneStrip::strip(0.5, 440.0, 10.0);
        assert!(mid > 0.0 && mid < 450.0, "{mid}");
    }

    #[test]
    fn the_card_rests_one_gap_in_and_hides_off_the_left_edge() {
        let row = Rect { pos: dvec2(0.0, 26.0), size: dvec2(1400.0, 874.0) };
        let w = PaneStrip::card_width(row.size.x);
        let rest = PaneStrip::card_rect(row, PaneStrip::strip(1.0, w, 10.0), w, 10.0);
        assert_eq!(rest.pos, dvec2(10.0, 36.0));
        assert_eq!(rest.size, dvec2(440.0, 854.0));
        // The desk starts where the strip ends: a tile one gap past the
        // card's right edge, never under it.
        let strip = PaneStrip::strip(1.0, w, 10.0);
        assert_eq!(row.pos.x + strip + 10.0, rest.pos.x + rest.size.x + 10.0);
        let hidden = PaneStrip::card_rect(row, 0.0, w, 10.0);
        assert_eq!(hidden.pos.x + hidden.size.x, row.pos.x, "fully off the left edge");
    }
}
