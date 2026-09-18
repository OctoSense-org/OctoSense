//! The shade: pulled down from the top-left it is the notification list,
//! from the top-right the quick controls; once open the two halves are one
//! sideways-swipeable sheet. All of its state, layout and drawing live
//! here; the phone shell only keeps a `ShadeState` on `PhoneState`, feeds
//! it the recognised gesture each frame (`ShadeState::step`), routes taps
//! and drags through `PhoneHit::Shade` and calls `draw` last so the sheet
//! is topmost.
//!
//! Until the gesture recognizer lands, the shade opens programmatically:
//! tapping the status bar's left or right half, or `--test-action
//! shade:<notifications|controls>`.
use crate::mobile_gestures::{Dir, GestureKind, ShadeSide, ShellGesture};
use crate::{desk::WmState, desktop::{DesktopStyle, DrawDesktopChrome}, mobile::PhoneHit, octosense::style::AppIconDraw, shell::{alpha, rgb, ui::{rect, HAlign, Ico, ShellDraw}}};
use makepad_widgets::{gauss_view::{GaussBlurSnapshot, GaussRoundedView}, *};
use crate::android_integration::AndroidState;

/// One notification card. `time` is seconds since app start when it was
/// posted, so the card shows a relative age.
#[derive(Clone, Debug, PartialEq)]
pub struct ShadeNote {
    pub id: u64,
    /// The posting app's id (`wm` for the shell's own).
    pub app: String,
    /// PackageManager presentation for an Android notification, when available.
    pub app_label: String,
    pub app_icon: String,
    pub title: String,
    pub body: String,
    pub time: f64,
    /// Action buttons a left swipe reveals, by label.
    pub actions: Vec<String>,
    pub dismissible: bool,
    /// The card's horizontal swipe offset in points (right: dismissing,
    /// left: revealing the actions), animated back or away on release.
    pub offset: f64,
    /// The actions are showing (the card rests at `-actions_width`).
    pub revealed: bool,
}

/// The quick-settings toggles, in grid order. `DarkMode` is the shell's
/// appearance (what the desk bar's Light/Dark set): the app flips it
/// through its style path and mirrors the result into `ShadeState::dark`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle { Wifi, Bluetooth, Torch, RotationLock, DoNotDisturb, DarkMode }
impl Toggle {
    pub const ALL: [Toggle; 6] = [Toggle::Wifi, Toggle::Bluetooth, Toggle::Torch, Toggle::RotationLock, Toggle::DoNotDisturb, Toggle::DarkMode];
    pub fn label(self) -> &'static str {
        match self { Toggle::Wifi => "Wi-Fi", Toggle::Bluetooth => "Bluetooth", Toggle::Torch => "Torch", Toggle::RotationLock => "Rotation", Toggle::DoNotDisturb => "DND", Toggle::DarkMode => "Dark mode" }
    }
    fn index(self) -> usize { Toggle::ALL.iter().position(|t| *t == self).unwrap_or(0) }
}

/// Everything on the shade a finger can land on; `PhoneHit::Shade` carries
/// one. Taps go through `ShadeState::tap`, drags through `drag`/`release`.
#[derive(Clone, Debug, PartialEq)]
pub enum ShadeHit {
    /// The status bar's left or right half while the shade is closed.
    Open(ShadeSide),
    /// The sheet itself: sideways switches the page, upward closes.
    Sheet,
    /// The dimmed rest of the screen: a tap closes.
    Backdrop,
    /// A notification card: right dismisses, left reveals its actions.
    Note(u64),
    /// A revealed action button (`index == actions.len()` is "Clear").
    Action(u64, usize),
    ClearAll,
    Brightness,
    Volume,
    Toggle(Toggle),
    SystemAccess,
    Settings(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DragAxis { Undecided, Horizontal, Vertical }

/// The shade's whole state; animated by `step` every phone frame.
#[derive(Clone, Debug)]
pub struct ShadeState {
    /// 0 closed .. 1 open; follows the pull directly, animates otherwise.
    pub open: f64,
    open_target: f64,
    pub side: ShadeSide,
    /// 0 = notifications, 1 = controls, animated for the sideways swipe.
    pub page: f64,
    page_target: f64,
    pub notifications: Vec<ShadeNote>,
    next_id: u64,
    pub brightness: f64,
    pub volume: f64,
    pub wifi: bool,
    pub bluetooth: bool,
    pub torch: bool,
    pub rotation_lock: bool,
    pub do_not_disturb: bool,
    /// The shell's appearance, mirrored from `WmState::style.dark` by the
    /// app whenever it changes (the tile reads it; tapping goes to the app).
    pub dark: bool,
    pub control_enabled: [bool; 6],
    pub slider_enabled: [bool; 2],
    pub network_summary: String,
    pub notification_access: bool,
    pub bridge_connected: bool,
    pub battery_saver: bool,
    /// Per-toggle 0..1 shape animation (pill → rounded rectangle).
    toggle_anim: [f64; 6],
    /// Battery percent and charging, from the status sampler the bar uses.
    pub battery: Option<(u32, bool)>,
    /// Seconds since app start at the last frame, for the cards' ages.
    pub now: f64,
    /// A `ShadePull` drove `open` this frame: hold the animation.
    pulling: bool,
    /// A finger on the sheet: the axis it settled on, and page/open at the
    /// drag's start so the offsets are absolute.
    drag: Option<(DragAxis, f64, f64)>,
    /// A finger on a card: it holds its offset until release.
    dragging_note: Option<u64>,
    seeded: bool,
}
impl Default for ShadeState {
    fn default() -> Self {
        Self {
            open: 0.0, open_target: 0.0, side: ShadeSide::Notifications, page: 0.0, page_target: 0.0,
            notifications: Vec::new(), next_id: 0,
            brightness: 0.62, volume: 0.45, wifi: true, bluetooth: false, torch: false, rotation_lock: false, do_not_disturb: false, dark: false,
            toggle_anim: [1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            control_enabled: if cfg!(target_os = "android") {[false,false,false,false,false,true]} else {[true;6]},
            slider_enabled: [!cfg!(target_os = "android");2],
            network_summary: "Connecting to Android…".into(), notification_access: false, bridge_connected: false, battery_saver: false,
            battery: None, now: 0.0, pulling: false, drag: None, dragging_note: None, seeded: false,
        }
    }
}

/// Width of the revealed action strip behind a card.
const ACTION_W: f64 = 84.0;
/// Bottom band the sheet leaves to the navigation pill.
const NAV_H: f64 = 24.0;
const CARD_RADIUS: f32 = 22.0;

impl ShadeState {
    pub fn is_open(&self) -> bool { self.open > 0.001 || self.open_target > 0.5 }
    /// The sheet is open or opening (not merely being pulled): what the
    /// island reads to hide, so it still docks with a pull in progress.
    pub fn wants_open(&self) -> bool { self.open_target > 0.5 }
    pub fn toggled(&self, t: Toggle) -> bool {
        match t { Toggle::Wifi => self.wifi, Toggle::Bluetooth => self.bluetooth, Toggle::Torch => self.torch, Toggle::RotationLock => self.rotation_lock, Toggle::DoNotDisturb => self.do_not_disturb, Toggle::DarkMode => self.dark }
    }
    fn flip(&mut self, t: Toggle) {
        match t { Toggle::Wifi => self.wifi = !self.wifi, Toggle::Bluetooth => self.bluetooth = !self.bluetooth, Toggle::Torch => self.torch = !self.torch, Toggle::RotationLock => self.rotation_lock = !self.rotation_lock, Toggle::DoNotDisturb => self.do_not_disturb = !self.do_not_disturb, Toggle::DarkMode => self.dark = !self.dark }
    }

    /// Open on `side`, animated; the page follows the side.
    pub fn open_on(&mut self, side: ShadeSide) {
        self.side = side;
        self.page_target = if side == ShadeSide::Controls { 1.0 } else { 0.0 };
        if self.open < 0.001 { self.page = self.page_target; }
        self.open_target = 1.0;
        self.pulling = false;
        if self.notifications.is_empty() && !self.seeded { self.seed_fixtures(); }
    }
    pub fn close(&mut self) { self.open_target = 0.0; self.pulling = false; self.drag = None; }

    /// Post a notification; the shell's stack and the WM protocol both
    /// mirror into here. Returns the card id.
    pub fn post(&mut self, app: &str, title: &str, body: &str, now: f64, actions: Vec<String>) -> u64 {
        self.next_id += 1;
        self.notifications.insert(0, ShadeNote { id: self.next_id, app: app.into(), app_label: String::new(), app_icon: String::new(), title: title.into(), body: body.into(), time: now, actions, dismissible: true, offset: 0.0, revealed: false });
        self.next_id
    }
    /// Demo content until Android's real notifications are wired: shown
    /// the first time the shade opens onto an empty list.
    pub fn seed_fixtures(&mut self) {
        self.seeded = true;
        if cfg!(target_os = "android") { return; }
        let now = self.now;
        self.post("photos", "Memories", "A new memory from this day last year is ready to watch.", now - 240.0, vec!["Watch".into()]);
        self.post("terminal", "Build finished", "octosense — release build completed in 4m 12s", now - 900.0, vec!["Open".into(), "Rerun".into()]);
        self.post("wm", "Welcome to OctoSense", "Pull down from the top-left for notifications and from the top-right for controls.", now - 3600.0, vec![]);
    }
    pub fn dismiss(&mut self, id: u64) { self.notifications.retain(|n| n.id != id); }
    pub fn clear_all(&mut self) { self.notifications.clear(); }

    /// The sheet's rect for `screen` at the current openness: it grows from
    /// the top edge and leaves the navigation band.
    pub fn sheet_rect(&self, screen: Rect) -> Rect {
        let h = (screen.size.y - NAV_H).max(1.0);
        rect(screen.pos.x, screen.pos.y, screen.size.x, h * self.open.clamp(0.0, 1.0))
    }
    /// Where the sheet's content sits: attached to the sheet's bottom edge,
    /// so it slides down with the pull.
    fn content_rect(&self, screen: Rect) -> Rect {
        let h = (screen.size.y - NAV_H).max(1.0);
        rect(screen.pos.x, screen.pos.y - (1.0 - self.open.clamp(0.0, 1.0)) * h, screen.size.x, h)
    }
    fn slider_track(content: Rect, which: ShadeHit) -> Rect {
        let y = content.pos.y + if which == ShadeHit::Brightness { 128.0 } else { 186.0 };
        rect(content.pos.x + 20.0, y, content.size.x - 40.0, 46.0)
    }
    fn toggle_cell(content: Rect, index: usize) -> Rect {
        let gap = 10.0;
        let w = (content.size.x - 40.0 - gap * 2.0) / 3.0;
        rect(content.pos.x + 20.0 + (index % 3) as f64 * (w + gap), content.pos.y + 262.0 + (index / 3) as f64 * 74.0, w, 64.0)
    }

    /// A tap on a shade hit (the shell's tap detection already happened).
    /// A hit the sheet drags itself (the recognizer lets the finger go):
    /// everything but the closed status bar, whose pull is the
    /// recognizer's `ShadePull`.
    pub fn drags(hit: &ShadeHit) -> bool { !matches!(hit, ShadeHit::Open(_)) }
    pub fn tap(&mut self, hit: ShadeHit) {
        match hit {
            ShadeHit::Open(side) => self.open_on(side),
            ShadeHit::Backdrop => self.close(),
            ShadeHit::Sheet => {}
            ShadeHit::SystemAccess | ShadeHit::Settings(_) => {}
            ShadeHit::Note(id) => {
                if let Some(n) = self.notifications.iter_mut().find(|n| n.id == id) { n.revealed = false; }
            }
            ShadeHit::Action(id, _) => self.dismiss(id),
            ShadeHit::ClearAll => self.clear_all(),
            ShadeHit::Brightness | ShadeHit::Volume => {}
            ShadeHit::Toggle(t) => self.flip(t),
        }
    }

    /// A finger moved with `hit` under its start: `p` is where it is now,
    /// `delta` its travel from the start, `screen` the phone's viewport.
    pub fn drag(&mut self, hit: &ShadeHit, p: Vec2d, delta: Vec2d, screen: Rect) {
        match hit {
            ShadeHit::Sheet | ShadeHit::Backdrop | ShadeHit::ClearAll => {
                let (axis, page0, open0) = self.drag.unwrap_or((DragAxis::Undecided, self.page, self.open));
                let axis = match axis {
                    DragAxis::Undecided if delta.length() < 8.0 => DragAxis::Undecided,
                    DragAxis::Undecided => if delta.x.abs() > delta.y.abs() { DragAxis::Horizontal } else { DragAxis::Vertical },
                    a => a,
                };
                self.drag = Some((axis, page0, open0));
                match axis {
                    DragAxis::Horizontal => self.page = (page0 - delta.x / screen.size.x.max(1.0)).clamp(-0.2, 1.2),
                    DragAxis::Vertical => {
                        let h = (screen.size.y - NAV_H).max(1.0);
                        self.open = (open0 + delta.y / h).clamp(0.0, 1.0);
                        self.pulling = true;
                    }
                    DragAxis::Undecided => {}
                }
            }
            ShadeHit::Note(id) => {
                // Sideways moves the card; a clearly vertical drag over a
                // card is the sheet's, as on a phone: it pulls the shade shut.
                let (axis, page0, open0) = self.drag.unwrap_or((DragAxis::Undecided, self.page, self.open));
                let axis = match axis {
                    DragAxis::Undecided if delta.length() < 8.0 => DragAxis::Undecided,
                    DragAxis::Undecided => if delta.y.abs() > delta.x.abs() * 1.5 { DragAxis::Vertical } else { DragAxis::Horizontal },
                    a => a,
                };
                self.drag = Some((axis, page0, open0));
                if axis == DragAxis::Vertical {
                    let h = (screen.size.y - NAV_H).max(1.0);
                    self.open = (open0 + delta.y / h).clamp(0.0, 1.0);
                    self.pulling = true;
                    return;
                }
                self.dragging_note = Some(*id);
                if let Some(n) = self.notifications.iter_mut().find(|n| n.id == *id) {
                    let base = if n.revealed { -(n.actions.len() as f64 + if n.dismissible {1.0} else {0.0}) * ACTION_W } else { 0.0 };
                    let min = -(n.actions.len() as f64 + if n.dismissible {1.0} else {0.0}) * ACTION_W - 20.0;
                    n.offset = (base + delta.x).clamp(min, screen.size.x);
                }
            }
            ShadeHit::Brightness | ShadeHit::Volume => {
                let track = Self::slider_track(self.content_rect(screen), hit.clone());
                let v = ((p.x - track.pos.x) / track.size.x.max(1.0)).clamp(0.0, 1.0);
                if *hit == ShadeHit::Brightness { self.brightness = v; } else { self.volume = v; }
            }
            _ => {}
        }
    }

    /// The finger lifted after a drag (not a tap) on `hit`; `delta` is the
    /// travel and `dt` the seconds it took, for flicks.
    pub fn release(&mut self, hit: &ShadeHit, delta: Vec2d, dt: f64) {
        let fast = dt < 0.3;
        match hit {
            ShadeHit::Sheet | ShadeHit::Backdrop | ShadeHit::ClearAll => {
                let (axis, page0, _) = self.drag.take().unwrap_or((DragAxis::Undecided, self.page, self.open));
                self.pulling = false;
                match axis {
                    DragAxis::Horizontal => {
                        let flick = if fast && delta.x < -40.0 { 1.0 } else if fast && delta.x > 40.0 { -1.0 } else { 0.0 };
                        self.page_target = if flick != 0.0 { (page0 + flick).clamp(0.0, 1.0) } else { self.page.round().clamp(0.0, 1.0) };
                        self.side = if self.page_target > 0.5 { ShadeSide::Controls } else { ShadeSide::Notifications };
                    }
                    DragAxis::Vertical => {
                        if delta.y < -60.0 || (fast && delta.y < -25.0) || self.open < 0.5 { self.open_target = 0.0; } else { self.open_target = 1.0; }
                    }
                    DragAxis::Undecided => {}
                }
            }
            ShadeHit::Note(id) => {
                self.dragging_note = None;
                if let Some((DragAxis::Vertical, _, _)) = self.drag.take() {
                    self.pulling = false;
                    if delta.y < -60.0 || (fast && delta.y < -25.0) || self.open < 0.5 { self.open_target = 0.0; } else { self.open_target = 1.0; }
                    return;
                }
                let Some(n) = self.notifications.iter_mut().find(|n| n.id == *id) else { return };
                let reveal_w = (n.actions.len() as f64 + if n.dismissible {1.0} else {0.0}) * ACTION_W;
                if n.dismissible && (n.offset > 96.0 || (fast && delta.x > 40.0)) {
                    let id = *id;
                    self.dismiss(id);
                } else if n.offset < -reveal_w * 0.4 {
                    n.revealed = true;
                } else {
                    n.revealed = false;
                }
            }
            _ => {}
        }
    }

    /// The rect the sheet owns while it shows: every shell edge under it is
    /// the shade's. The desk adds it to the frame's exclusion zones (which
    /// it clears once per frame), so the shade never removes anything.
    pub fn exclusion(&self, screen: Rect) -> Option<Rect> {
        if self.open <= 0.01 { return None; }
        Some(if self.open > 0.5 { screen } else { self.sheet_rect(screen) })
    }
    /// Per frame: apply this frame's shell gesture and animate. Returns
    /// true while anything is still moving.
    pub fn step(&mut self, dt: f64, gesture: Option<ShellGesture>, now: f64) -> bool {
        if now > 0.0 { self.now = now; }
        self.apply_gesture(gesture);
        let t = 1.0 - (-dt * 16.0).exp();
        let mut active = false;
        if !self.pulling {
            self.open += (self.open_target - self.open) * t;
            if (self.open - self.open_target).abs() < 0.002 { self.open = self.open_target; }
            active |= self.open != self.open_target;
        }
        if self.drag.map_or(true, |(a, _, _)| a != DragAxis::Horizontal) {
            self.page += (self.page_target - self.page) * t;
            if (self.page - self.page_target).abs() < 0.002 { self.page = self.page_target; }
            active |= self.page != self.page_target;
        }
        for n in &mut self.notifications {
            if self.dragging_note == Some(n.id) { continue; }
            let target = if n.revealed { -(n.actions.len() as f64 + if n.dismissible {1.0} else {0.0}) * ACTION_W } else { 0.0 };
            n.offset += (target - n.offset) * t;
            if (n.offset - target).abs() < 0.3 { n.offset = target; }
            active |= n.offset != target;
        }
        for (i, tg) in Toggle::ALL.iter().enumerate() {
            let target = if self.toggled(*tg) { 1.0 } else { 0.0 };
            let v = &mut self.toggle_anim[i];
            *v += (target - *v) * t;
            if (*v - target).abs() < 0.005 { *v = target; }
            active |= *v != target;
        }
        self.pulling = false;
        active
    }

    fn apply_gesture(&mut self, gesture: Option<ShellGesture>) {
        match gesture {
            Some(ShellGesture::ShadePull { side, progress }) => {
                if self.open_target < 0.5 || !self.is_open() {
                    self.side = side;
                    self.page_target = if side == ShadeSide::Controls { 1.0 } else { 0.0 };
                    self.page = self.page_target;
                    if self.notifications.is_empty() && !self.seeded { self.seed_fixtures(); }
                }
                self.open = progress.clamp(0.0, 1.0);
                self.pulling = true;
            }
            Some(ShellGesture::Commit(GestureKind::Shade(side))) => self.open_on(side),
            Some(ShellGesture::Cancel(GestureKind::Shade(_))) => self.close(),
            Some(ShellGesture::PageSwipe { dir, progress }) if self.is_open() => {
                let from = self.page_target;
                let to = match dir { Dir::Left => (from + 1.0).min(1.0), Dir::Right => (from - 1.0).max(0.0) };
                self.page = from + (to - from) * progress.clamp(0.0, 1.0);
                self.drag = Some((DragAxis::Horizontal, from, self.open));
            }
            Some(ShellGesture::Commit(GestureKind::Page(dir))) if self.is_open() => {
                self.page_target = match dir { Dir::Left => (self.page_target + 1.0).min(1.0), Dir::Right => (self.page_target - 1.0).max(0.0) };
                self.side = if self.page_target > 0.5 { ShadeSide::Controls } else { ShadeSide::Notifications };
                self.drag = None;
            }
            Some(ShellGesture::Cancel(GestureKind::Page(_))) if self.is_open() => { self.drag = None; }
            _ => {}
        }
    }
}

/// "now", "4m", "2h", "1d".
fn age(now: f64, then: f64) -> String {
    let s = (now - then).max(0.0);
    if s < 60.0 { "now".into() } else if s < 3600.0 { format!("{}m", (s / 60.0) as u64) } else if s < 86400.0 { format!("{}h", (s / 3600.0) as u64) } else { format!("{}d", (s / 86400.0) as u64) }
}

fn app_label(id: &str) -> String {
    if id == "wm" { return "OctoSense".into(); }
    crate::clients::find_app(id).map(|a| a.label).unwrap_or_else(|| {
        let mut c = id.chars();
        match c.next() { Some(f) => f.to_uppercase().collect::<String>() + c.as_str(), None => String::new() }
    })
}

fn rounded(chrome: &mut DrawDesktopChrome, cx: &mut Cx2d, r: Rect, radius: f32, color: Vec4f) {
    chrome.radius = radius;
    chrome.bevel = 0.0;
    chrome.color = color;
    chrome.draw_abs(cx, r);
}

/// The closed shade's tap targets: the status bar's two halves open the
/// side under the finger. Registered right after the status bar is drawn,
/// under the island's own targets (hits are last-wins), so the pill and
/// the clock keep their taps.
pub fn status_bar_hits(hits: &mut Vec<(Rect, PhoneHit)>, state: &WmState, screen: Rect) {
    if state.phone.shade.open >= 0.001 { return; }
    let status_h = if screen.size.x > screen.size.y { 24.0 } else { 42.0 };
    let half = screen.size.x * 0.5;
    hits.push((rect(screen.pos.x, screen.pos.y, half, status_h), PhoneHit::Shade(ShadeHit::Open(ShadeSide::Notifications))));
    hits.push((rect(screen.pos.x + half, screen.pos.y, half, status_h), PhoneHit::Shade(ShadeHit::Open(ShadeSide::Controls))));
}

/// Draw the shade over everything else. `hits` receives the sheet's
/// tappable regions (the closed shade's are `status_bar_hits`).
#[allow(clippy::too_many_arguments)]
/// The sheet's content — status line, notification cards, controls —
/// recorded once per state into a frame the size of the settled sheet, and
/// shown as one quad while the sheet moves. Nothing inside the sheet changes
/// during a pull, yet every moving frame used to lay out and encode all of
/// it again (the shell's 4–5 ms of CPU per frame on the phone) and
/// rasterize it. A settled sheet draws live, so sliders and taps respond.
#[derive(Default)]
pub struct ShadeContentCache {
    frame: Option<crate::dock_warp::WindowFrame>,
    key: Option<u64>,
}

/// Everything the recorded content depends on. Card ages tick by the
/// minute, so the minute is part of it.
fn content_key(shade: &ShadeState, android: &AndroidState, clock: &str, dark: bool, ios: bool, size: Vec2d, dpi: f64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for note in &shade.notifications {
        (note.id, &note.app, &note.app_label, &note.app_icon, &note.title, &note.body, &note.actions).hash(&mut h);
        note.time.to_bits().hash(&mut h);
        note.offset.to_bits().hash(&mut h);
        note.revealed.hash(&mut h);
        note.dismissible.hash(&mut h);
        android.icons.contains_key(&note.app_icon).hash(&mut h);
    }
    shade.dragging_note.hash(&mut h);
    for t in Toggle::ALL { shade.toggled(t).hash(&mut h); }
    shade.brightness.to_bits().hash(&mut h);
    shade.volume.to_bits().hash(&mut h);
    shade.control_enabled.hash(&mut h);
    shade.slider_enabled.hash(&mut h);
    (&shade.network_summary, shade.notification_access, shade.bridge_connected, shade.battery_saver).hash(&mut h);
    shade.page.to_bits().hash(&mut h);
    shade.battery.hash(&mut h);
    for n in &shade.notifications { ((shade.now - n.time) / 60.0).floor().to_bits().hash(&mut h); }
    clock.hash(&mut h);
    (dark, ios).hash(&mut h);
    size.x.to_bits().hash(&mut h);
    size.y.to_bits().hash(&mut h);
    dpi.to_bits().hash(&mut h);
    h.finish()
}

#[allow(clippy::too_many_arguments)]
pub fn draw(cx: &mut Cx2d, d: &mut ShellDraw, chrome: &mut DrawDesktopChrome, icons: &mut AppIconDraw, native_icon: &mut DrawImage, glass: &mut GaussRoundedView, hits: &mut Vec<(Rect, PhoneHit)>, state: &WmState, screen: Rect, backdrop: Option<GaussBlurSnapshot>, cache: &mut ShadeContentCache, present: &mut dyn FnMut(&mut Cx2d, &Texture, Rect)) {
    let shade = &state.phone.shade;
    let style = state.style.target;
    let dark = state.style.dark;
    let ios = style == DesktopStyle::Ios;
    let content = shade.content_rect(screen);
    let covered = backdrop.is_some();
    let ink = if dark { rgb(245, 245, 250) } else { rgb(26, 26, 34) };
    let accent = if ios { rgb(0, 122, 255) } else { rgb(103, 80, 164) };
    let card = if dark { alpha(rgb(44, 46, 62), 0.86) } else { alpha(rgb(255, 255, 255), 0.88) };
    // The content: live on a settled sheet (its sliders and cards respond),
    // one recorded quad while the sheet moves over a kept scene. Recorded at
    // the settled content rect, presented at the current one. With the shade
    // closed, an idle home frame records it too, its hits discarded, so the
    // first pull finds it; a moving frame whose key does not fit draws live
    // rather than paying a record inside the gesture.
    let settled = rect(screen.pos.x, screen.pos.y, content.size.x, content.size.y);
    let key = content_key(shade, &state.phone.android, &state.phone.clock, dark, ios, content.size, cx.current_dpi_factor());
    if shade.open < 0.001 {
        if covered && cache.key != Some(key) {
            let mut scratch = Vec::new();
            let frame = cache.frame.get_or_insert_with(|| crate::dock_warp::WindowFrame::new_with_name(cx, "wm_phone_shade_content"));
            frame.begin(cx, settled);
            draw_content(cx, d, chrome, icons, native_icon, &mut scratch, state, screen, settled, ink, accent, card);
            frame.end(cx);
            cache.key = Some(key);
        }
        return;
    }
    let open = shade.open.clamp(0.0, 1.0) as f32;
    let sheet = shade.sheet_rect(screen);
    // The dimmed, blurred backdrop, then the sheet's own frosted surface.
    // The dim, only where the sheet does not cover: with a backdrop the
    // sheet's glass fills at alpha 1 from a snapshot taken before this
    // overlay, so dim drawn under it never reached the screen.
    let dim = if covered { rect(screen.pos.x, sheet.pos.y + sheet.size.y, screen.size.x, (screen.pos.y + screen.size.y - sheet.pos.y - sheet.size.y).max(0.0)) } else { screen };
    if dim.size.y > 0.0 { rounded(chrome, cx, dim, 0.0, alpha(rgb(0, 0, 0), 0.28 * open)); }
    hits.push((screen, PhoneHit::Shade(ShadeHit::Backdrop)));
    // The sheet: one glass whose tint is the overview tint (#101329 at 0.20)
    // with the sheet colour composited over it, folded into a single mix:
    // mix(mix(T, g, 0.2), c, a) = mix(T, g', 1 - 0.8 (1 - a)). The rim and
    // inner shadow scale by (1 - a), as the tint layer used to cover them.
    let (tint, tint_alpha, edge) = if dark { (vec4(12.56 / 255.0, 12.98 / 255.0, 22.95 / 255.0, 1.0), 0.64, 0.45) } else { (vec4(211.98 / 255.0, 214.09 / 255.0, 223.61 / 255.0, 1.0), 0.696, 0.38) };
    glass.set_blurriness(cx, 3.0);
    let bg = &mut glass.draw_bg.draw_vars;
    bg.set_dyn_instance(cx, live_id!(tint_color), &[tint.x, tint.y, tint.z, tint.w]);
    bg.set_uniform(cx, live_id!(tint_alpha), &[tint_alpha]);
    bg.set_dyn_instance(cx, live_id!(rim_alpha), &[0.55 * edge]);
    bg.set_dyn_instance(cx, live_id!(inner_shadow_alpha), &[0.10 * edge]);
    glass.draw_surface_with_backdrop(cx, sheet, backdrop, 1.0);
    hits.push((sheet, PhoneHit::Shade(ShadeHit::Sheet)));
    let moving = shade.open < 0.999;
    if moving && covered && cache.key == Some(key) && cache.frame.is_some() {
        let frame = cache.frame.as_ref().unwrap();
        frame.attach(cx);
        present(cx, frame.texture(), content);
    } else if !moving && covered {
        let frame = cache.frame.get_or_insert_with(|| crate::dock_warp::WindowFrame::new_with_name(cx, "wm_phone_shade_content"));
        frame.begin(cx, settled);
        draw_content(cx, d, chrome, icons, native_icon, hits, state, screen, settled, ink, accent, card);
        frame.end(cx);
        cache.key = Some(key);
        present(cx, frame.texture(), content);
    } else {
        draw_content(cx, d, chrome, icons, native_icon, hits, state, screen, content, ink, accent, card);
    }
    // Page dots and the drag handle along the sheet's bottom edge.
    let by = sheet.pos.y + sheet.size.y;
    for i in 0..2 {
        let on = 1.0 - (shade.page - i as f64).abs().clamp(0.0, 1.0);
        rounded(chrome, cx, rect(screen.pos.x + screen.size.x * 0.5 - 11.0 + i as f64 * 14.0, by - 36.0, 8.0, 8.0), 4.0, alpha(ink, 0.3 + 0.6 * on as f32));
    }
    rounded(chrome, cx, rect(screen.pos.x + screen.size.x * 0.5 - 24.0, by - 16.0, 48.0, 5.0), 2.5, alpha(ink, 0.35));
}

/// The status line, then the notifications page and the controls page,
/// side by side across `shade.page`, in `content`.
#[allow(clippy::too_many_arguments)]
fn draw_content(cx: &mut Cx2d, d: &mut ShellDraw, chrome: &mut DrawDesktopChrome, icons: &mut AppIconDraw, native_icon: &mut DrawImage, hits: &mut Vec<(Rect, PhoneHit)>, state: &WmState, screen: Rect, content: Rect, ink: Vec4f, accent: Vec4f, card: Vec4f) {
    let shade = &state.phone.shade;
    let style = state.style.target;
    let dark = state.style.dark;
    let w = content.size.x;
    let pages = [content.pos.x - shade.page * w, content.pos.x + (1.0 - shade.page) * w];
    // The status line on both pages: clock left, battery right.
    for (page, x) in pages.iter().enumerate() {
        if *x + w < screen.pos.x || *x > screen.pos.x + screen.size.x { continue; }
        let top = rect(*x + 24.0, content.pos.y + 14.0, w - 48.0, 30.0);
        d.label(cx, top, true, 22.0, ink, HAlign::Left, &state.phone.clock);
        let (percent, charging) = shade.battery.unwrap_or((100, false));
        let text = if charging { format!("{percent}% charging") } else { format!("{percent}%") };
        let tw = d.measure(cx, false, 13.0, &text);
        d.label(cx, rect(top.pos.x + top.size.x - tw, top.pos.y, tw, 30.0), false, 13.0, alpha(ink, 0.85), HAlign::Right, &text);
        let bx = top.pos.x + top.size.x - tw - 32.0;
        let by = top.pos.y + 9.0;
        rounded(chrome, cx, rect(bx, by, 24.0, 12.0), 3.0, alpha(ink, 0.45));
        rounded(chrome, cx, rect(bx + 24.5, by + 3.5, 2.0, 5.0), 1.0, alpha(ink, 0.45));
        rounded(chrome, cx, rect(bx + 2.0, by + 2.0, 20.0 * (percent.min(100) as f64 / 100.0).max(0.05), 8.0), 1.5, if percent <= 20 && !charging { rgb(228, 66, 52) } else { ink });
        if page == 0 { draw_notifications(cx, d, chrome, icons, native_icon, hits, shade, &state.phone.android, style, dark, ink, accent, card, rect(*x, content.pos.y, w, content.size.y)); }
        else { draw_controls(cx, d, chrome, hits, shade, dark, ink, accent, card, rect(*x, content.pos.y, w, content.size.y)); }
    }
}

/// Rasterize the shade's glyphs and tessellate its notification icons once,
/// before the first pull: both pages drawn from a seeded copy of the shade,
/// off-screen and fully transparent, on an idle frame. The first pull used to
/// pay it inside its first frames (glyph atlas packing under `draw_overlay`,
/// ~30 ms of a 55 ms frame on the OnePlus 6T). The live shade is untouched:
/// its fixtures still seed on the first real open.
pub fn prewarm(cx: &mut Cx2d, d: &mut ShellDraw, chrome: &mut DrawDesktopChrome, icons: &mut AppIconDraw, native_icon: &mut DrawImage, state: &WmState, screen: Rect) {
    let mut shade = state.phone.shade.clone();
    if shade.notifications.is_empty() { shade.seed_fixtures(); }
    let style = state.style.target;
    let ios = style == DesktopStyle::Ios;
    let clear = vec4(0.0, 0.0, 0.0, 0.0);
    let accent = if ios { rgb(0, 122, 255) } else { rgb(103, 80, 164) };
    let mut hits = Vec::new();
    // Past the right edge of the window: recorded, never rasterized on screen.
    let x = screen.pos.x + screen.size.x * 3.0;
    let page = rect(x, screen.pos.y, screen.size.x, screen.size.y);
    d.label(cx, rect(x + 24.0, page.pos.y + 14.0, 200.0, 30.0), true, 22.0, clear, HAlign::Left, &state.phone.clock);
    d.label(cx, rect(x + 24.0, page.pos.y + 14.0, 200.0, 30.0), false, 13.0, clear, HAlign::Right, "0123456789% charging");
    draw_notifications(cx, d, chrome, icons, native_icon, &mut hits, &shade, &state.phone.android, style, state.style.dark, clear, accent, clear, page);
    draw_controls(cx, d, chrome, &mut hits, &shade, state.style.dark, clear, accent, clear, rect(x + screen.size.x, screen.pos.y, screen.size.x, screen.size.y));
}

#[allow(clippy::too_many_arguments)]
fn draw_notifications(cx: &mut Cx2d, d: &mut ShellDraw, chrome: &mut DrawDesktopChrome, icons: &mut AppIconDraw, native_icon: &mut DrawImage, hits: &mut Vec<(Rect, PhoneHit)>, shade: &ShadeState, android: &AndroidState, style: DesktopStyle, dark: bool, ink: Vec4f, accent: Vec4f, card: Vec4f, page: Rect) {
    let x = page.pos.x;
    let w = page.size.x;
    let head = rect(x + 24.0, page.pos.y + 58.0, w - 48.0, 30.0);
    d.label(cx, head, true, 17.0, ink, HAlign::Left, "Notifications");
    if shade.notifications.iter().any(|note| note.dismissible) {
        let pill = rect(head.pos.x + head.size.x - 84.0, head.pos.y, 84.0, 30.0);
        rounded(chrome, cx, pill, 15.0, alpha(ink, 0.10));
        d.label(cx, pill, false, 12.5, ink, HAlign::Center, "Clear all");
        hits.push((pill, PhoneHit::Shade(ShadeHit::ClearAll)));
    }
    let mut y = head.pos.y + 44.0;
    let bottom = page.pos.y + page.size.y - 48.0;
    if cfg!(target_os = "android") {
        let setup = rect(x + 16.0, y, w - 32.0, if shade.notification_access {44.0} else {80.0});
        rounded(chrome,cx,setup,16.0,alpha(accent,0.13));
        let title=if shade.notification_access {"Notification settings  ›"} else if shade.bridge_connected {"Enable notifications  ›"} else {"Connect notification access  ›"};
        d.label(cx,rect(setup.pos.x+16.0,y+7.0,setup.size.x-32.0,30.0),true,14.0,accent,HAlign::Left,title);
        if !shade.notification_access {
            d.label(cx,rect(setup.pos.x+16.0,y+38.0,setup.size.x-32.0,28.0),false,12.0,ink,HAlign::Left,"Read, reply and dismiss from Home");
        }
        hits.push((setup,PhoneHit::Shade(ShadeHit::Settings("notifications"))));
        y += setup.size.y + 12.0;
    }
    if shade.notifications.is_empty() && (!cfg!(target_os = "android") || shade.notification_access) {
        d.label(cx, rect(x, page.pos.y + page.size.y * 0.4, w, 30.0), false, 15.0, alpha(ink, 0.5), HAlign::Center, "No notifications");
    }
    for n in &shade.notifications {
        if y > bottom { break; }
        let slot = rect(x + 16.0, y, w - 32.0, 0.0);
        let text_w = slot.size.x - 78.0;
        let lines = d.wrap(cx, false, 13.0, &n.body, text_w, 2);
        let h = 62.0 + lines.len() as f64 * 18.0 + if lines.is_empty() { 0.0 } else { 4.0 };
        let slot = rect(slot.pos.x, y, slot.size.x, h);
        // The action strip behind the card, revealed by a left swipe.
        if n.offset < -8.0 {
            let mut labels: Vec<&str> = n.actions.iter().map(|s| s.as_str()).collect();
            if n.dismissible { labels.push("Clear"); }
            let strip_w = (labels.len() as f64) * ACTION_W;
            let visible = (-n.offset).min(strip_w);
            for (i, label) in labels.iter().enumerate() {
                let bx = slot.pos.x + slot.size.x - strip_w + i as f64 * ACTION_W;
                let r = rect(bx + 6.0, slot.pos.y + 8.0, ACTION_W - 12.0, h - 16.0);
                if bx + ACTION_W <= slot.pos.x + slot.size.x - visible + 1.0 { continue; }
                let last = n.dismissible && i + 1 == labels.len();
                rounded(chrome, cx, r, 16.0, if last { alpha(rgb(228, 66, 52), 0.9) } else { accent });
                d.label_elided(cx, r, true, 12.5, rgb(255, 255, 255), HAlign::Center, label);
                hits.push((r, PhoneHit::Shade(ShadeHit::Action(n.id, i))));
            }
        }
        let fade = (1.0 - (n.offset.max(0.0) / (slot.size.x * 0.8))).clamp(0.0, 1.0) as f32;
        let r = rect(slot.pos.x + n.offset, slot.pos.y, slot.size.x, h);
        rounded(chrome, cx, r, CARD_RADIUS, alpha(card, fade));
        let ink_f = alpha(ink, fade);
        let icon = rect(r.pos.x + 16.0, r.pos.y + 16.0, 40.0, 40.0);
        if let Some(texture) = android.icons.get(&n.app_icon) {
            native_icon.draw_vars.set_texture(0, texture);
            native_icon.opacity = fade;
            native_icon.draw_abs(cx, icon);
        } else if n.app == "wm" || crate::clients::find_app(&n.app).is_none() {
            rounded(chrome, cx, icon, 12.0, alpha(accent, 0.9 * fade));
            d.icon_centered(cx, Ico::Bell, icon, 20.0, alpha(rgb(255, 255, 255), fade));
        } else {
            icons.draw(cx, &n.app, style, icon, fade, ink_f);
        }
        let tx = r.pos.x + 68.0;
        let fallback;
        let label=if n.app_label.is_empty() {fallback=app_label(&n.app);fallback.as_str()} else {n.app_label.as_str()};
        d.label_elided(cx, rect(tx, r.pos.y + 14.0, text_w - 40.0, 16.0), false, 11.0, alpha(ink, 0.6 * fade), HAlign::Left, label);
        let when = age(shade.now, n.time);
        d.label(cx, rect(tx + text_w - 40.0, r.pos.y + 14.0, 40.0, 16.0), false, 11.0, alpha(ink, 0.5 * fade), HAlign::Right, &when);
        d.label_elided(cx, rect(tx, r.pos.y + 32.0, text_w, 22.0), true, 14.5, ink_f, HAlign::Left, &n.title);
        for (i, line) in lines.iter().enumerate() {
            d.label(cx, rect(tx, r.pos.y + 56.0 + i as f64 * 18.0, text_w, 18.0), false, 13.0, alpha(ink, 0.8 * fade), HAlign::Left, line);
        }
        let _ = dark;
        // The card's own hit sits over the strip only where the card is.
        hits.push((r, PhoneHit::Shade(ShadeHit::Note(n.id))));
        y += h + 10.0;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_controls(cx: &mut Cx2d, d: &mut ShellDraw, chrome: &mut DrawDesktopChrome, hits: &mut Vec<(Rect, PhoneHit)>, shade: &ShadeState, dark: bool, ink: Vec4f, accent: Vec4f, card: Vec4f, page: Rect) {
    let x = page.pos.x;
    let w = page.size.x;
    d.label(cx, rect(x + 24.0, page.pos.y + 58.0, w - 48.0, 30.0), true, 17.0, ink, HAlign::Left, "Controls");
    if cfg!(target_os = "android") {
        let access=rect(x+w-144.0,page.pos.y+58.0,120.0,30.0);
        d.label(cx,access,false,12.0,accent,HAlign::Right,"System setup");
        hits.push((access,PhoneHit::Shade(ShadeHit::SystemAccess)));
    }
    if cfg!(target_os = "android") {
        let network=rect(x+24.0,page.pos.y+94.0,w-48.0,28.0);
        d.label_elided(cx,network,false,12.0,accent,HAlign::Left,&shade.network_summary);
        hits.push((network,PhoneHit::Shade(ShadeHit::Settings("internet"))));
    }
    for (hit, value, ico) in [(ShadeHit::Brightness, shade.brightness, Ico::Brightness), (ShadeHit::Volume, shade.volume, if shade.volume < 0.01 { Ico::Volume0 } else if shade.volume < 0.5 { Ico::Volume1 } else { Ico::Volume3 })] {
        let enabled=shade.slider_enabled[if hit==ShadeHit::Brightness {0} else {1}];
        let track = ShadeState::slider_track(page, hit.clone());
        rounded(chrome, cx, track, 23.0, card);
        let fill = rect(track.pos.x, track.pos.y, (track.size.x * value).max(46.0), track.size.y);
        rounded(chrome, cx, fill, 23.0, alpha(accent, if enabled {0.92} else {0.2}));
        d.icon_centered(cx, ico, rect(track.pos.x + 4.0, track.pos.y, 40.0, track.size.y), 20.0, rgb(255, 255, 255));
        d.label(cx, rect(track.pos.x, track.pos.y, track.size.x - 18.0, track.size.y), false, 12.5, alpha(ink, 0.7), HAlign::Right, &format!("{}%", (value * 100.0).round() as u32));
        let target=if enabled {hit.clone()} else {ShadeHit::Settings(if hit==ShadeHit::Brightness {"brightness"} else {"sound"})};
        hits.push((rect(track.pos.x - 8.0, track.pos.y - 6.0, track.size.x + 16.0, track.size.y + 12.0), PhoneHit::Shade(target)));
        if !enabled {
            d.label(cx,rect(track.pos.x+48.0,track.pos.y,track.size.x-120.0,track.size.y),false,12.0,ink,HAlign::Left,"Tap to set up");
        }
    }
    // Android 16 style toggles: a pill at rest, a rounded rectangle on.
    for (i, t) in Toggle::ALL.iter().enumerate() {
        let cell = ShadeState::toggle_cell(page, i);
        let on = shade.toggle_anim[t.index()] as f32;
        let radius = (cell.size.y * 0.5) as f32 * (1.0 - on) + 16.0 * on;
        let face = if on > 0.5 { alpha(accent, 0.5 + on * 0.5) } else if dark { alpha(rgb(255, 255, 255), 0.10 + on * 0.5) } else { alpha(card, 1.0) };
        rounded(chrome, cx, cell, radius, face);
        let fg = alpha(if on > 0.5 { rgb(255, 255, 255) } else { ink },if shade.control_enabled[i] {1.0} else {0.75});
        let ico = match t {
            Toggle::Wifi => if shade.wifi { Ico::Wifi } else { Ico::WifiOff },
            Toggle::Bluetooth => if shade.bluetooth { Ico::Bluetooth } else { Ico::BluetoothOff },
            Toggle::Torch => Ico::Power,
            Toggle::RotationLock => Ico::Lock,
            Toggle::DoNotDisturb => Ico::BellOff,
            Toggle::DarkMode => Ico::Moon,
        };
        d.icon_centered(cx, ico, rect(cell.pos.x + 10.0, cell.pos.y, 30.0, cell.size.y), 18.0, fg);
        let text = rect(cell.pos.x + 42.0, cell.pos.y, cell.size.x - 50.0, cell.size.y);
        let (tw, size) = if cell.size.x < 110.0 { (text.size.x, 11.0) } else { (text.size.x, 12.5) };
        d.label_elided(cx, rect(text.pos.x, text.pos.y, tw, if cfg!(target_os="android") {38.0} else {text.size.y}), false, size, fg, HAlign::Left, t.label());
        if cfg!(target_os="android") {
            let hint=if shade.control_enabled[i] {if shade.toggled(*t) {"On"} else {"Off"}} else if matches!(t,Toggle::Wifi|Toggle::Bluetooth|Toggle::DoNotDisturb) {"Settings"} else {"Set up"};
            d.label_elided(cx,rect(text.pos.x,text.pos.y+34.0,tw,20.0),false,10.0,fg,HAlign::Left,hint);
        }
        hits.push((cell, PhoneHit::Shade(ShadeHit::Toggle(*t))));
    }
    if cfg!(target_os="android") {
        for (i,(label,destination)) in [
            ("Wi-Fi & mobile data", "internet"), ("Pair Bluetooth", "bluetooth"),
            ("Hotspot", "hotspot"), ("VPN", "vpn"),
            (if shade.battery_saver {"Battery saver · On"} else {"Battery saver"}, "battery"), ("Display & sleep", "display"),
            ("Sound & vibration", "sound"), ("Accessibility", "accessibility"),
        ].iter().enumerate() {
            let cell=rect(x+20.0+(i%2) as f64*(w-30.0)/2.0,page.pos.y+426.0+(i/2) as f64*52.0,(w-50.0)/2.0,44.0);
            if cell.pos.y+cell.size.y > page.pos.y+page.size.y-48.0 {continue;}
            rounded(chrome,cx,cell,14.0,card);
            d.label_elided(cx,rect(cell.pos.x+12.0,cell.pos.y,cell.size.x-24.0,cell.size.y),false,12.0,ink,HAlign::Left,label);
            hits.push((cell,PhoneHit::Shade(ShadeHit::Settings(destination))));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobile_gestures::ExclusionZones;
    fn screen() -> Rect { rect(0.0, 0.0, 412.0, 892.0) }
    fn settle(s: &mut ShadeState) { for _ in 0..120 { s.step(1.0 / 60.0, None, 100.0); } }
    /// The frame's exclusion zones as the desk rebuilds them: cleared, then the shade's.
    fn zones(s: &ShadeState) -> ExclusionZones { let mut ex = ExclusionZones::default(); if let Some(z) = s.exclusion(screen()) { ex.add(z, [true; 4]); } ex }

    #[test]
    fn connection_access_and_dismissibility_invalidate_cached_shade() {
        let mut shade=ShadeState::default();
        let android=AndroidState::default();
        let key=|shade:&ShadeState|content_key(shade,&android,"9:41",false,false,screen().size,1.0);
        let mut before=key(&shade);
        shade.network_summary="Wi-Fi · Connected".into(); assert_ne!(before,key(&shade)); before=key(&shade);
        shade.notification_access=true; assert_ne!(before,key(&shade)); before=key(&shade);
        shade.bridge_connected=true; assert_ne!(before,key(&shade));
        let id=shade.post("test","Ongoing","",0.0,vec![]);
        before=key(&shade); shade.notifications[0].dismissible=false; assert_ne!(before,key(&shade));
        shade.drag(&ShadeHit::Note(id),dvec2(200.0,200.0),dvec2(160.0,0.0),screen());
        shade.release(&ShadeHit::Note(id),dvec2(160.0,0.0),0.2);
        assert_eq!(shade.notifications.len(),1,"ongoing notification dismissed locally");
    }

    #[test]
    fn native_notification_presentation_invalidates_recorded_shade_content() {
        let mut shade=ShadeState::default();
        let mut android=AndroidState::default();
        shade.post("example.package","Title","Body",1.0,vec![]);
        let key=|shade:&ShadeState,android:&AndroidState|content_key(shade,android,"9:41",false,false,screen().size,1.0);
        let initial=key(&shade,&android);
        shade.notifications[0].app_label="Actual app name".into();
        let label=key(&shade,&android);assert_ne!(initial,label);
        shade.notifications[0].app_icon="/owned/icon.png".into();
        let icon=key(&shade,&android);assert_ne!(label,icon);
        android.catalog_revision+=1;
        assert_eq!(icon,key(&shade,&android),"unrelated catalog updates do not invalidate recorded notification content");
    }

    #[test]
    fn pull_progress_drives_open_and_commit_finishes_it() {
        let mut s = ShadeState::default();
        s.step(1.0 / 60.0, Some(ShellGesture::ShadePull { side: ShadeSide::Controls, progress: 0.4 }), 1.0);
        assert!((s.open - 0.4).abs() < 1e-9 && s.side == ShadeSide::Controls);
        assert_eq!(s.page, 1.0, "the controls half is the page the pull opens onto");
        assert!(!zones(&s).zones.is_empty(), "a showing sheet owns the edges under it");
        s.step(1.0 / 60.0, Some(ShellGesture::Commit(GestureKind::Shade(ShadeSide::Controls))), 1.0);
        settle(&mut s);
        assert_eq!(s.open, 1.0);
        let ex = zones(&s);
        assert_eq!(ex.zones.len(), 1, "one zone per frame: the desk clears, the shade adds");
        assert_eq!(ex.zones[0].rect, screen());
    }

    #[test]
    fn an_upward_drag_over_a_card_pulls_the_sheet_shut() {
        let mut s = ShadeState::default();
        s.open_on(ShadeSide::Notifications);
        settle(&mut s);
        let id = s.post("Photos", "Memories", "ready", 1.0, Vec::new());
        let hit = ShadeHit::Note(id);
        for k in 1..=6 { s.drag(&hit, dvec2(200.0, 600.0 - 40.0 * k as f64), dvec2(2.0, -40.0 * k as f64), screen()); }
        assert!(s.open < 1.0, "the sheet followed the finger up: {}", s.open);
        assert_eq!(s.notifications[0].offset, 0.0, "the card did not slide");
        s.release(&hit, dvec2(2.0, -240.0), 0.2);
        settle(&mut s);
        assert_eq!(s.open, 0.0);
        // A sideways drag on the card is still the card's.
        s.open_on(ShadeSide::Notifications);
        settle(&mut s);
        s.drag(&hit, dvec2(260.0, 400.0), dvec2(60.0, 3.0), screen());
        assert!(s.notifications[0].offset > 0.0 && s.open == 1.0);
        s.release(&hit, dvec2(60.0, 3.0), 0.5);
        assert!(s.drag.is_none());
    }

    #[test]
    fn cancel_animates_back_closed_and_drops_the_exclusion() {
        let mut s = ShadeState::default();
        s.step(1.0 / 60.0, Some(ShellGesture::ShadePull { side: ShadeSide::Notifications, progress: 0.3 }), 1.0);
        s.step(1.0 / 60.0, Some(ShellGesture::Cancel(GestureKind::Shade(ShadeSide::Notifications))), 1.0);
        settle(&mut s);
        assert_eq!(s.open, 0.0);
        assert!(zones(&s).zones.is_empty());
        assert!(!s.step(1.0 / 60.0, None, 1.0), "settled: nothing left to animate");
    }

    #[test]
    fn sideways_drag_and_page_gesture_switch_halves() {
        let mut s = ShadeState::default();
        s.open_on(ShadeSide::Notifications);
        settle(&mut s);
        assert_eq!(s.page, 0.0);
        s.drag(&ShadeHit::Sheet, dvec2(100.0, 300.0), dvec2(-260.0, 4.0), screen());
        assert!(s.page > 0.5 && s.page < 1.0);
        s.release(&ShadeHit::Sheet, dvec2(-260.0, 4.0), 0.6);
        settle(&mut s);
        assert_eq!((s.page, s.side), (1.0, ShadeSide::Controls));
        // A short fast flick right goes back even before halfway.
        s.drag(&ShadeHit::Sheet, dvec2(200.0, 300.0), dvec2(60.0, 0.0), screen());
        s.release(&ShadeHit::Sheet, dvec2(60.0, 0.0), 0.1);
        settle(&mut s);
        assert_eq!((s.page, s.side), (0.0, ShadeSide::Notifications));
        // The recognizer's page swipe does the same once it exists.
        s.step(1.0 / 60.0, Some(ShellGesture::PageSwipe { dir: Dir::Left, progress: 0.5 }), 1.0);
        assert!((s.page - 0.5).abs() < 1e-9);
        s.step(1.0 / 60.0, Some(ShellGesture::Commit(GestureKind::Page(Dir::Left))), 1.0);
        settle(&mut s);
        assert_eq!(s.page, 1.0);
    }

    #[test]
    fn swipe_up_on_the_sheet_and_backdrop_tap_close() {
        let mut s = ShadeState::default();
        s.open_on(ShadeSide::Controls);
        settle(&mut s);
        s.drag(&ShadeHit::Sheet, dvec2(200.0, 400.0), dvec2(2.0, -120.0), screen());
        assert!(s.open < 1.0, "the sheet follows the finger up");
        s.release(&ShadeHit::Sheet, dvec2(2.0, -120.0), 0.5);
        settle(&mut s);
        assert_eq!(s.open, 0.0);
        s.open_on(ShadeSide::Controls);
        settle(&mut s);
        s.tap(ShadeHit::Backdrop);
        settle(&mut s);
        assert_eq!(s.open, 0.0);
    }

    #[test]
    fn dismiss_reveal_actions_and_clear_all() {
        let mut s = ShadeState::default();
        let a = s.post("wm", "A", "first", 1.0, vec![]);
        let b = s.post("messages", "B", "second", 2.0, vec!["Reply".into()]);
        assert_eq!(s.notifications.len(), 2);
        s.drag(&ShadeHit::Note(a), dvec2(0.0, 0.0), dvec2(150.0, 0.0), screen());
        s.release(&ShadeHit::Note(a), dvec2(150.0, 0.0), 0.5);
        assert!(s.notifications.iter().all(|n| n.id != a), "a right swipe dismisses");
        s.drag(&ShadeHit::Note(b), dvec2(0.0, 0.0), dvec2(-120.0, 0.0), screen());
        s.release(&ShadeHit::Note(b), dvec2(-120.0, 0.0), 0.5);
        settle(&mut s);
        let note = s.notifications.iter().find(|n| n.id == b).unwrap();
        assert!(note.revealed && (note.offset + 2.0 * ACTION_W).abs() < 1e-6, "a left swipe rests on the action strip");
        s.tap(ShadeHit::Action(b, 0));
        assert!(s.notifications.is_empty());
        s.post("wm", "C", "", 3.0, vec![]);
        s.post("wm", "D", "", 3.0, vec![]);
        s.tap(ShadeHit::ClearAll);
        assert!(s.notifications.is_empty());
    }

    #[test]
    fn sliders_follow_the_finger_and_toggles_flip() {
        let mut s = ShadeState::default();
        s.open_on(ShadeSide::Controls);
        s.open = 1.0;
        let track = ShadeState::slider_track(s.content_rect(screen()), ShadeHit::Brightness);
        s.drag(&ShadeHit::Brightness, dvec2(track.pos.x + track.size.x * 0.25, track.pos.y), dvec2(0.0, 0.0), screen());
        assert!((s.brightness - 0.25).abs() < 1e-9);
        assert!(!s.torch);
        s.tap(ShadeHit::Toggle(Toggle::Torch));
        assert!(s.torch);
        assert_eq!(age(100.0, 100.0), "now");
        assert_eq!(age(400.0, 100.0), "5m");
        assert_eq!(age(8000.0, 100.0), "2h");
    }
}
