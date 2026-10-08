//! Phone shell state and geometry. Application viewports remain stable while
//! their compositor surfaces move between home, foreground and the task viewer.
use crate::{desktop::DesktopStyle, hub::ClientId, mobile_gestures::SafeInsets, mobile_tiles::HomeTiles};
use makepad_widgets::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PhoneScreen { #[default] Home, App, Recents, Drawer }

#[derive(Clone, Debug, PartialEq)]
pub enum PhoneHit {
    App(String), TileApp(String), Card(ClientId), Home, Recents, Drawer, Search, Back,
    Floating(crate::mobile_navigation::NavigationHit),
    /// Android app navigation lives outside the app viewport.
    AppNavigation(crate::mobile_navigation::NavigationHit),
    /// The desk bar's phone strip (universal builds only): rotate the
    /// window, the style menu, Light/Dark, back to the desktop.
    #[cfg(not(mobile_only))] Rotate,
    #[cfg(not(mobile_only))] Style,
    #[cfg(not(mobile_only))] Appearance,
    #[cfg(not(mobile_only))] Desktop,
    Key(String), Shift, Symbols, HideKeyboard,
    ClearSearch, CancelSearch,
    Shade(crate::mobile_shade::ShadeHit),
    /// A page indicator dot: jump the home pager there (mobile_pages.rs).
    Page(i64),
    Island(crate::mobile_island::IslandHit),
    /// The status bar's battery icon: three quick taps switch the
    /// frame-time reporter (mobile_perf.rs).
    Perf,
    /// Tile groups (mobile_groups.rs): the tile, a member in its window,
    /// the window's scrim, a pair's "Open both", a Recents card's split
    /// button and the split divider.
    Group(String), GroupApp(String, String), GroupClose, OpenBoth(String), Split(ClientId), Divider,
    /// The app drawer's letter column: a finger on it jumps the list.
    Scrub,
    /// A published card on the glance page: open the app that published it.
    Glance(String),
    /// A host-bound Mail card: expand this exact publication, not the full app.
    ExpandGlance(String),
    /// The assistant chip on the home page: the system chat (#143).
    Assistant,
}

/// The launch effect of an Android app (`PhoneState::launch`).
#[derive(Clone, Debug, PartialEq)]
pub struct LaunchFx {
    pub app: String,
    pub origin: Rect,
    /// 0 at the tap, 1 when done (about a quarter of a second).
    pub t: f64,
}
/// An icon being dragged on the home page.
#[derive(Clone, Debug, PartialEq)]
pub struct HomeDrag {
    pub app: String,
    pub start: Vec2d,
    pub pos: Vec2d,
    /// The finger travelled: a lift without moving opens the icon's menu.
    pub moved: bool,
}
#[derive(Clone)]
pub struct PhoneGesture {
    pub start: Vec2d,
    pub last: Vec2d,
    pub time: f64,
    pub hit: Option<PhoneHit>,
    /// The gesture recognizer (mobile_gestures.rs) claimed this finger: it
    /// started in a shell band, or in the home page body.
    pub shell: bool,
    /// A vertical Glance drag owns the rest of this touch, even if it
    /// returns to its starting point before release.
    pub glance_scroll: bool,
    pub screen: PhoneScreen,
}

#[derive(Clone)]
pub struct PhoneState {
    /// The card workspace owns input, including during its transition.
    pub card_open: bool,
    /// Only the settled opaque workspace suppresses background drawing.
    pub card_covers_home: bool,
    pub theme: Option<crate::mobile_theme::Selection>,
    pub navigation: crate::mobile_navigation::FloatingNavigation,
    pub android: crate::android_integration::AndroidState,
    /// Which hidden gestures the person has found (mobile_hints.rs): the
    /// home page shows one short hint at a time until they have.
    pub hints: crate::mobile_hints::Hints,
    pub clock: String,
    /// The frame clock of the last stepped frame (the shade stamps its
    /// cards on it).
    pub wallpaper_time: f64,
    /// Procedural wallpaper phase. Navigation keeps it fixed so the rendered
    /// wallpaper can be reused; changing it invalidates the texture cache.
    pub wallpaper_phase: f64,
    pub screen: PhoneScreen,
    pub client: Option<ClientId>,
    /// The app in front was opened from another app, which Back returns to
    /// (mobile_back.rs). Any other navigation forgets it.
    pub return_to: Option<crate::mobile_back::ReturnTo>,
    pub order: Vec<ClientId>,
    pub openness: f64,
    pub overview: f64,
    pub page: f64,
    pub dismiss_y: f64,
    pub gesture: Option<PhoneGesture>,
    /// A home-page icon lifted by a long press and following the finger
    /// (mobile_app.rs `finish_home_drag` puts it down).
    pub drag: Option<HomeDrag>,
    /// An Android app just launched: its icon grows out of its place and
    /// the page dims while Android brings the app's window up.
    pub launch: Option<LaunchFx>,
    /// Frame-trace boundaries: include the final settling frame, while
    /// keeping the separate one-second status refreshes out of a gesture.
    pub animation_active: bool,
    pub draw_active: bool,
    /// Native touch owned by shell navigation; other fingers cannot replace it.
    pub touch: Option<u64>,
    pub keyboard: f64,
    pub native_keyboard: f64,
    /// The native keyboard's height when it last showed: where the next one
    /// will stop, before it reports anything.
    pub native_keyboard_seen: f64,
    /// How much the native keyboard has shortened the body right now (the
    /// window's height minus the body's).
    pub body_reflow: f64,
    /// When app search last took focus (`crate::host::now`).
    pub search_focus_at: f64,
    pub keyboard_target: f64,
    pub keyboard_sent_height: f64,
    pub keyboard_client: Option<ClientId>,
    pub search_query: String,
    /// Search is opened by Home's pull-down, independently of the app catalog.
    pub search_open: bool,
    /// Return in the search field: the app to open (mobile_app.rs takes it).
    pub search_launch: Option<String>,
    pub search_focused: bool,
    pub search_scroll: f64,
    /// The drawer keeps scrolling after a flick: points per second, decaying
    /// in `step`; the surface publishes how far the list can scroll.
    pub search_velocity: f64,
    /// The drawer pulled past its top (positive) or bottom (negative): a
    /// stretch that eases back after the lift.
    pub search_stretch: f64,
    pub search_scroll_limit: f64,
    /// The finger's recent samples on the drawer (y, time), newest last:
    /// the lift velocity is measured over them (`search_lift`).
    pub search_track: Vec<(f64, f64)>,
    /// The list was at its top when the finger landed: pulling it further
    /// down closes search (`search_pull_closes`).
    pub search_touch_at_top: bool,
    /// A finger that landed on the search field and is pulling down: where
    /// and when it landed. The field lets it go (`search_field_releases`) and
    /// the shell takes it as a pull on the list from there.
    pub search_field_pull: Option<(Vec2d, f64)>,
    pub ime: HashMap<ClientId, makepad_platform::ime::HostedImeState>,
    pub shift: bool,
    pub symbols: bool,
    /// What the desk bar's Desktop toggle restores (universal builds only).
    #[cfg(not(mobile_only))] pub desktop_size: Option<Vec2d>,
    #[cfg(not(mobile_only))] pub desktop_clients: Vec<ClientId>,
    #[cfg(not(mobile_only))] pub desktop_style: DesktopStyle,
    /// The rect the shell lays out in: the desk's rect inside `insets`.
    pub viewport: Rect,
    /// The platform's safe-area insets (the notch, the system bars): the
    /// wallpaper runs under them, everything else stays inside.
    pub insets: SafeInsets,
    /// The home page's live app tiles (mobile_tiles.rs): which client shows
    /// which tile and in which face.
    pub tiles: HomeTiles,
    /// The shell gesture recognised this frame, for every mobile surface to
    /// read (mobile_gestures.rs owns it; surfaces never touch raw fingers).
    pub gesture_out: Option<crate::mobile_gestures::ShellGesture>,
    /// Rects apps own on screen; shell gestures starting inside them are not
    /// recognised.
    pub exclusions: crate::mobile_gestures::ExclusionZones,
    /// The notification/controls shade (mobile_shade.rs).
    pub shade: crate::mobile_shade::ShadeState,
    /// The home pager: glance page, apps pages, library (mobile_pages.rs).
    pub pages: crate::mobile_pages::PagesState,
    /// The live island's activities and state (mobile_island.rs).
    pub island: crate::mobile_island::IslandState,
    /// Tile groups, the open group window and the split screen (mobile_groups.rs).
    pub groups: crate::mobile_groups::GroupsState,
}
impl Default for PhoneState {
    fn default() -> Self {
        Self { card_open: false, card_covers_home: false, clock: "9:41".into(), wallpaper_time: 0.0, wallpaper_phase: 0.0, screen: PhoneScreen::Home, client: None, return_to: None, order: Vec::new(),
            navigation: Default::default(), theme: None,
            openness: 0.0, overview: 0.0, page: 0.0, dismiss_y: 0.0, gesture: None, touch: None,
            animation_active: false, draw_active: false,
            keyboard: 0.0, native_keyboard: 0.0, native_keyboard_seen: 0.0, body_reflow: 0.0, search_focus_at: 0.0, keyboard_target: 0.0, keyboard_sent_height: 0.0, keyboard_client: None,
            search_query: String::new(), search_open: false, search_launch: None, search_focused: false, search_scroll: 0.0,
            search_velocity: 0.0, search_stretch: 0.0, search_scroll_limit: 0.0, search_track: Vec::new(), search_touch_at_top: false, search_field_pull: None,
            ime: HashMap::new(), shift: false, symbols: false,
            #[cfg(not(mobile_only))] desktop_size: None,
            #[cfg(not(mobile_only))] desktop_clients: Vec::new(),
            #[cfg(not(mobile_only))] desktop_style: DesktopStyle::Omarchy,
            viewport: Rect::default(), insets: SafeInsets::default(),
            tiles: HomeTiles::default(),
            gesture_out: None,
            hints: Default::default(),
            drag: None,
            launch: None,
            exclusions: Default::default(),
            shade: Default::default(),
            pages: Default::default(),
            island: Default::default(),
            groups: Default::default(),
            android: Default::default() }
    }
}
/// How much of the finger's travel past an end of the drawer list shows as
/// stretch, and the most it can stretch.
pub const SEARCH_STRETCH: f64 = 0.45;
pub const SEARCH_STRETCH_MAX: f64 = 72.0;
/// The stretch (points) a pull on a list already at its top reaches to close
/// search: about 80 points of finger travel. A shorter pull only stretches.
pub const SEARCH_PULL_CLOSE: f64 = 36.0;
/// How far (points) a finger on the search field moves down, mostly
/// vertically, before it is a pull and no longer a tap or a selection.
pub const SEARCH_FIELD_PULL_SLOP: f64 = 12.0;

/// Whether a finger that landed on the search field at `start` and is now at
/// `p` is pulling down a list that is at its top (`search_scroll`): the field
/// lets it go, so a pull that starts on the field closes search too. Taps,
/// sideways selection drags and pulls on a scrolled list stay with it.
pub fn search_field_releases(start: Vec2d, p: Vec2d, search_scroll: f64) -> bool {
    let d = p - start;
    search_scroll <= 0.5 && d.y >= SEARCH_FIELD_PULL_SLOP && d.y > d.x.abs() * 1.5
}
/// How far back the drawer's lift velocity looks, in seconds.
const SEARCH_VELOCITY_WINDOW: f64 = 0.1;
/// The flicked drawer's friction: its speed falls by e every 1/k seconds,
/// so it coasts v/k points (a quarter of the lift speed per second).
const SEARCH_FRICTION: f64 = 4.0;

impl PhoneState {
    /// Keep the side edges assigned to the launcher throughout a drag and
    /// its settling animation. Open workspaces, apps and the IME keep Back.
    pub fn owns_launcher_edges(&self) -> bool {
        matches!(self.screen, PhoneScreen::Home | PhoneScreen::Drawer)
            && self.openness < 0.001 && self.overview < 0.001
            && !self.card_open && !self.groups.window_visible() && !self.shade.is_open()
            && !self.navigation.open && self.navigation.visible()
            && self.keyboard < 0.001 && self.keyboard_target < 0.001 && self.native_keyboard < 0.001
    }
    /// Focus loss can interrupt the owned touch after IME suppression has
    /// already cleared Navigation's held control. No release is guaranteed
    /// after that, so do not keep waiting for its old native touch id.
    pub fn cancel_navigation_input(&mut self) {
        self.touch = None;
        self.navigation.cancel();
        if self.gesture.as_ref().is_some_and(|g| matches!(g.hit, Some(PhoneHit::AppNavigation(_)))) {
            self.gesture = None;
        }
    }
    pub fn app_dock_for_platform(&self, screen: Rect, android: bool) -> Option<crate::mobile_navigation::AppDock> {
        (android && self.screen == PhoneScreen::App && self.navigation.visible())
            .then(|| crate::mobile_navigation::app_dock(screen))
    }
    pub fn app_dock(&self, screen: Rect) -> Option<crate::mobile_navigation::AppDock> {
        self.app_dock_for_platform(screen, cfg!(target_os = "android"))
    }
    /// Shared by capture sizing, presentation and split/input geometry. The
    /// backing app keeps this size in Home/Recents too, so navigation animates
    /// its retained texture rather than resizing it as the dock disappears.
    pub fn app_content_rect_for_platform(&self, screen: Rect, android: bool) -> Rect {
        if android {
            if self.navigation.visible() { crate::mobile_navigation::app_dock(screen).content }
            else { screen }
        } else { app_rect(screen) }
    }
    pub fn app_content_rect(&self, screen: Rect) -> Rect {
        self.app_content_rect_for_platform(screen, cfg!(target_os = "android"))
    }
    pub fn card_rect(&self, screen: Rect, index: f64, page: f64) -> Rect {
        card_rect_for_app(screen, self.app_content_rect(screen), index, page)
    }
    pub fn floating_navigation_visible(&self) -> bool {
        self.navigation.visible() && !(cfg!(target_os = "android") && self.screen == PhoneScreen::App)
    }
    pub fn navigation_rect(&self) -> Rect {
        // The native KeyboardView already resizes this viewport above the
        // IME. Only the shell's simulated keyboard overlays the viewport,
        // and a search's keyboard is made room for before that resize, so
        // the bubble clears the lifted search bar (`search_keyboard_lift`).
        let lift = self.search_keyboard_lift(crate::host::now());
        let height = (self.viewport.size.y - self.keyboard - lift).max(1.0);
        // Keep the app-local navigation in Glance's reserved header. Its
        // normal mid-screen position otherwise obscures live card content.
        let height = if self.screen == PhoneScreen::Home && self.pages.on_glance() {
            height.min(crate::mobile_pages::GLANCE_HEADER)
        } else { height };
        Rect { pos: self.viewport.pos, size: dvec2(self.viewport.size.x, height) }
    }
    pub fn native_keyboard_event(&mut self, event: &VirtualKeyboardEvent) {
        match event {
            VirtualKeyboardEvent::WillShow { .. } | VirtualKeyboardEvent::DidShow { .. } => {
                self.navigation.set_ime_visible(true);
                // Consume the old owned finger through release, but never let
                // its now-hidden navigation target fire after editing starts.
                if let Some(g) = self.gesture.as_mut().filter(|g| matches!(g.hit, Some(PhoneHit::AppNavigation(_)))) {
                    g.hit = Some(PhoneHit::AppNavigation(crate::mobile_navigation::NavigationHit::Dismiss));
                    g.shell = false;
                }
            },
            VirtualKeyboardEvent::DidHide { .. } => self.navigation.set_ime_visible(false),
            // Keep the editing area clear throughout the hide animation.
            VirtualKeyboardEvent::WillHide { .. } => {}
        }
        self.native_keyboard=match event {
            VirtualKeyboardEvent::WillShow{height,..}|VirtualKeyboardEvent::DidShow{height,..}=>height.max(0.0),
            VirtualKeyboardEvent::WillHide{..}|VirtualKeyboardEvent::DidHide{..}=>0.0,
        };
        // WillShow carries the height the keyboard will reach; a rising
        // keyboard's DidShow heights only grow toward it.
        match event {
            VirtualKeyboardEvent::WillShow{height,..} if *height>0.0 => self.native_keyboard_seen=*height,
            VirtualKeyboardEvent::DidShow{height,..} if *height>self.native_keyboard_seen => self.native_keyboard_seen=*height,
            _ => {}
        }
    }
    /// How far app search's field is lifted above the body's bottom while
    /// the native keyboard is still rising: it waits where the keyboard will
    /// stop (the height it last showed at) instead of starting at the bottom
    /// and being overtaken. Zero once the body has shrunk that far, with no
    /// native keyboard (the desktop preview), and half a second after focus
    /// if none came (a hardware keyboard).
    pub fn search_keyboard_lift(&self, now: f64) -> f64 {
        if !self.search_focused || self.native_keyboard_seen <= 0.0 {
            return 0.0;
        }
        if self.native_keyboard <= 0.0 && now - self.search_focus_at > 0.5 {
            return 0.0;
        }
        (self.native_keyboard_seen - self.body_reflow).max(0.0)
    }
    /// The home page (or the app library) is fully shown and nothing is
    /// animating or being dragged: safe to reconfigure a window down to
    /// its tile face without disturbing a closing animation.
    pub fn home_settled(&self) -> bool {
        matches!(self.screen, PhoneScreen::Home | PhoneScreen::Drawer)
            && self.gesture.is_none()
            && self.openness < 0.001
            && self.overview < 0.001
    }
    /// The client the person is looking at full screen (the open app, from
    /// the first frame of its zoom-in until it is dismissed).
    pub fn foreground(&self) -> Option<ClientId> {
        if self.screen == PhoneScreen::App { self.client } else { None }
    }
    /// The home page is on screen at all (tiles need drawing and driving).
    pub fn home_visible(&self) -> bool {
        self.screen != PhoneScreen::App || self.openness < 0.999
    }
    pub fn activate(&mut self, client: ClientId) {
        self.navigation.cancel();
        self.search_open = false;
        self.search_focused = false;
        if self.client != Some(client) { self.keyboard_target = 0.0; }
        if self.return_to.is_some_and(|r| r.app != client) { self.return_to = None; }
        self.client = Some(client);
        self.order.retain(|c| *c != client);
        self.order.insert(0, client);
        self.page = 0.0;
        self.screen = PhoneScreen::App;
        self.dismiss_y = 0.0;
    }
    pub fn navigate(&mut self, screen: PhoneScreen) {
        self.navigation.cancel();
        self.search_open = false;
        self.search_focused = false;
        self.screen = screen;
        self.return_to = None;
        self.keyboard_target = 0.0;
        self.gesture = None;
        self.dismiss_y = 0.0;
    }
    pub fn open_search(&mut self) {
        self.navigate(PhoneScreen::Drawer);
        self.search_open = true;
        self.search_query.clear();
        self.search_scroll = 0.0;
        self.search_velocity = 0.0;
        self.search_stretch = 0.0;
        self.search_track.clear();
    }
    pub fn step(&mut self, dt: f64) -> bool {
        let reduced = self.android.reduce_motion;
        let t = if reduced {1.0} else {1.0 - (-dt * 19.0).exp()};
        let open = if matches!(self.screen, PhoneScreen::App | PhoneScreen::Recents) && self.client.is_some() { 1.0 } else { 0.0 };
        let overview = if self.screen == PhoneScreen::Recents { 1.0 } else { 0.0 };
        let mut active = false;
        active |= self.navigation.step_with_motion(dt, reduced);
        // A finger driving the home swipe or the back preview holds the
        // window where it is; a lifted finger lets it settle.
        let dragging = self.gesture.is_some()
            && matches!(self.gesture_out, Some(crate::mobile_gestures::ShellGesture::HomeUp { .. } | crate::mobile_gestures::ShellGesture::Back { .. }));
        for (value, target) in [(&mut self.openness, open), (&mut self.overview, overview)] {
            if !dragging {
                *value += (target - *value) * t;
                if (*value - target).abs() < 0.001 { *value = target; }
                active |= *value != target;
            }
        }
        self.keyboard += (self.keyboard_target - self.keyboard) * t;
        if (self.keyboard_target - self.keyboard).abs() < 0.25 { self.keyboard = self.keyboard_target; }
        active |= self.keyboard != self.keyboard_target;
        active |= self.island.step_with_motion(dt, crate::host::now(), self.gesture_out, reduced);
        active |= self.groups.step_with_motion(dt, reduced);
        if self.gesture.is_none() {
            let target = self.page.round().clamp(0.0, self.order.len().saturating_sub(1) as f64);
            self.page += (target - self.page) * t;
            if (target - self.page).abs() < 0.001 { self.page = target; }
            active |= self.page != target;
        }
        if let Some(launch) = self.launch.as_mut() {
            launch.t += if reduced {1.0} else {dt / 0.26};
            if launch.t >= 1.0 { self.launch = None; } else { active = true; }
        }
        // A flicked drawer coasts and slows (about a second from a fast
        // flick), stopping dead at either end of the list.
        if self.search_stretch != 0.0 && self.gesture.is_none() {
            self.search_stretch *= (-dt * 14.0).exp();
            if self.search_stretch.abs() < 0.3 { self.search_stretch = 0.0; }
            active = true;
        }
        if self.search_velocity != 0.0 {
            if self.gesture.is_none() && self.screen == PhoneScreen::Drawer {
                let before = self.search_scroll;
                self.search_scroll = (self.search_scroll - self.search_velocity * dt).clamp(0.0, self.search_scroll_limit);
                self.search_velocity *= (-dt * SEARCH_FRICTION).exp();
                if self.search_velocity.abs() < 30.0 || self.search_scroll == before { self.search_velocity = 0.0; }
                active = true;
            } else { self.search_velocity = 0.0; }
        }
        active |= self.shade.step_with_motion(dt, self.gesture_out, self.wallpaper_time, reduced);
        self.absorb_docked(crate::mobile_island::take_docked());
        // The island stays hidden while the sheet is (or is about to be)
        // open and comes back as it closes.
        self.island.set_shade_open(self.shade.wants_open());
        active |= self.pages.step_with_motion(dt, if self.screen == PhoneScreen::Home { self.gesture_out } else { None }, reduced);
        active |= self.pages.step_glance(dt, self.viewport.size.y, self.gesture.is_none() && self.screen == PhoneScreen::Home, reduced);
        if self.pages.take_library_request() { self.navigate(PhoneScreen::Drawer); }
        active
    }
    /// An activity the island dropped becomes a card in the shade, stamped
    /// on the shade's clock (the frame time, not the island's).
    pub fn absorb_docked(&mut self, notes: Vec<crate::mobile_island::DockedNote>) {
        for note in notes {
            self.shade.post(&note.app, &note.title, &note.body, self.wallpaper_time, Vec::new());
        }
    }
    pub fn accepts_app_input(&self) -> bool {
        self.screen == PhoneScreen::App && self.gesture.is_none()
            && self.overview < 0.01 && self.openness > 0.99
    }
    pub fn keyboard_height(&self) -> f64 {
        if self.viewport.size.x > self.viewport.size.y { 184.0 } else { 292.0 }
    }
    pub fn searching(&self) -> bool {
        self.screen == PhoneScreen::Drawer && self.search_open
    }
    /// A finger landed on the drawer list: a coasting list stops under it.
    pub fn search_touch(&mut self, y: f64, time: f64) {
        self.search_touch_at_top = self.search_scroll <= 0.5;
        self.search_velocity = 0.0;
        self.search_track.clear();
        self.search_track.push((y, time));
    }
    /// The finger on the drawer list moved by `dy` points (down positive) to
    /// `y` at `time`; `max` is how far the list scrolls. The list follows
    /// the finger 1:1. Past either end it stretches at `SEARCH_STRETCH` of
    /// the finger's travel (up to `SEARCH_STRETCH_MAX`), and a finger that
    /// turns back takes the stretch up before the list moves again.
    pub fn search_drag(&mut self, dy: f64, y: f64, time: f64, max: f64) {
        let max = max.max(0.0);
        // Where the finger has put the list, in scroll points; beyond
        // 0..max it is the overscroll the stretch shows.
        let pos = self.search_scroll.clamp(0.0, max) - self.search_stretch / SEARCH_STRETCH - dy;
        if pos < 0.0 {
            self.search_scroll = 0.0;
            self.search_stretch = (-pos * SEARCH_STRETCH).min(SEARCH_STRETCH_MAX);
        } else if pos > max {
            self.search_scroll = max;
            self.search_stretch = -((pos - max) * SEARCH_STRETCH).min(SEARCH_STRETCH_MAX);
        } else {
            self.search_scroll = pos;
            self.search_stretch = 0.0;
        }
        // Samples older than the velocity window are of no further use.
        self.search_track.retain(|(_, t)| time - *t <= SEARCH_VELOCITY_WINDOW);
        if self.search_track.len() >= 16 { self.search_track.remove(0); }
        self.search_track.push((y, time));
    }
    /// Lifting now closes search: the list was already at its top when the
    /// finger landed and the finger pulled it on down. Reaching the top in a
    /// pull that started further down only stretches it, so it takes another
    /// pull from the top to close, like the pull that opened search.
    pub fn search_pull_closes(&self) -> bool {
        self.searching() && self.search_touch_at_top && self.search_stretch >= SEARCH_PULL_CLOSE
    }
    /// The finger left the drawer list at `time`. A list flicked past the
    /// slop keeps going at the finger's speed over the last tenth of a
    /// second; a lift after a pause, or a tap, leaves it where it is. The
    /// stretch eases back in `step`.
    pub fn search_lift(&mut self, time: f64, moved: bool) {
        let velocity = match (self.search_track.first(), self.search_track.last()) {
            (Some(&(y0, t0)), Some(&(y1, t1))) if t1 - t0 > 0.004 && time - t1 < 0.08 => (y1 - y0) / (t1 - t0),
            _ => 0.0,
        };
        self.search_velocity = if moved && velocity.abs() > 250.0 && self.search_stretch == 0.0 { velocity } else { 0.0 };
        self.search_track.clear();
    }
}

/// What one frame of the phone scene has to set up, from the state alone
/// (desk/phone.rs follows it). The backdrop compositor — a full-screen
/// off-screen scene pass plus a full-screen copy, and a blur pyramid per
/// glass — is only worth paying for on a frame where some frosted surface
/// samples the scene; the wallpaper and the home page are only worth
/// drawing while they can be seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScenePlan {
    /// Route the scene through the backdrop compositor.
    pub compose: bool,
    /// Draw the wallpaper (else a flat fill under the system bars).
    pub wallpaper: bool,
    /// Draw the home page's chrome and tiles.
    pub home: bool,
}
impl PhoneState {
    pub fn scene_plan(&self, ios: bool) -> ScenePlan {
        let app_settled = self.screen == PhoneScreen::App && self.openness >= 0.999 && self.overview <= 0.001;
        let glass = (ios && self.openness < 0.999)
            || self.overview > 0.001
            || self.groups.window_visible()
            || self.shade.open > 0.001
            || (ios && self.keyboard > 0.5);
        // Android's app drawer is an opaque full-screen sheet and switches in
        // without a slide, so the wallpaper under it is never seen, not even
        // under the Recents blur when a home swipe starts on the drawer. The
        // wallpaper shader is the most expensive full-screen fill the phone
        // draws, and with glass up it ran once more in the backdrop scene: a
        // Recents hold over the drawer presented at 39 fps (GPU-ready 43 ms)
        // against 55 fps (13 ms) over Home. The iOS App Library is
        // translucent and keeps it.
        let drawer_covers = !ios && self.screen == PhoneScreen::Drawer;
        ScenePlan { compose: glass, wallpaper: !app_settled && !drawer_covers, home: !app_settled }
    }
}

pub fn phone_size(style: DesktopStyle) -> Vec2d {
    if style == DesktopStyle::Ios { dvec2(402.0, 874.0) } else { dvec2(412.0, 892.0) }
}
pub fn app_rect(screen: Rect) -> Rect {
    // The Android/OpenHarmony viewport already excludes native status/navigation
    // bars. Floating navigation does not reserve any content space.
    if crate::mobile_navigation::ENABLED { return screen; }
    let top = if screen.size.x > screen.size.y { 24.0 } else { 42.0 };
    Rect { pos: screen.pos + dvec2(0.0, top), size: dvec2(screen.size.x, (screen.size.y - top - 24.0).max(1.0)) }
}
pub fn card_rect(screen: Rect, index: f64, page: f64) -> Rect {
    card_rect_for_app(screen, app_rect(screen), index, page)
}
pub fn card_rect_for_app(screen: Rect, app: Rect, index: f64, page: f64) -> Rect {
    let scale = if screen.size.x > screen.size.y { 0.74 } else { 0.76 };
    let size = app.size * scale;
    Rect { pos: app.pos + (app.size - size) * 0.5 + dvec2((index-page)*(size.x+22.0), -4.0), size }
}
pub fn mix_rect(a: Rect, b: Rect, t: f64) -> Rect {
    Rect { pos: a.pos + (b.pos-a.pos)*t, size: a.size + (b.size-a.size)*t }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_edges_stay_owned_during_paging_but_release_for_back_targets() {
        let mut phone=PhoneState::default();
        phone.pages.sync(&["a".into(),"b".into(),"c".into()],1,1);
        for page in [-1,0,1,2] {
            phone.pages.jump(page);
            assert!(phone.owns_launcher_edges(),"page {page}");
        }
        phone.pages.step(0.016,Some(crate::mobile_gestures::ShellGesture::PageSwipe {
            dir:crate::mobile_gestures::Dir::Right,progress:0.25,
        }));
        assert!(phone.owns_launcher_edges(),"a moving page must retain edge ownership");
        phone.screen=PhoneScreen::Drawer;
        assert!(phone.owns_launcher_edges());
        for screen in [PhoneScreen::App,PhoneScreen::Recents] {
            phone.screen=screen;
            assert!(!phone.owns_launcher_edges());
        }
        phone.screen=PhoneScreen::Home;
        phone.card_open=true;assert!(!phone.owns_launcher_edges());phone.card_open=false;
        phone.keyboard_target=300.0;assert!(!phone.owns_launcher_edges());phone.keyboard_target=0.0;
        phone.native_keyboard=300.0;assert!(!phone.owns_launcher_edges());phone.native_keyboard=0.0;
        phone.navigation.open=true;assert!(!phone.owns_launcher_edges());phone.navigation.open=false;
        phone.overview=0.5;assert!(!phone.owns_launcher_edges());phone.overview=0.0;
        phone.openness=0.5;assert!(!phone.owns_launcher_edges());phone.openness=0.0;
        phone.shade.open=0.5;assert!(!phone.owns_launcher_edges());phone.shade.open=0.0;
        assert!(phone.owns_launcher_edges());
    }

    #[test]
    fn reduced_motion_settles_transitions_but_preserves_touch_and_scroll_physics() {
        use crate::mobile_gestures::{Dir, GestureKind, ShellGesture};
        let mut phone = PhoneState::default();
        phone.android.reduce_motion = true;
        phone.activate(10);
        phone.step(1.0 / 60.0);
        assert!(phone.accepts_app_input());
        phone.navigate(PhoneScreen::Recents);
        phone.step(1.0 / 60.0);
        assert_eq!(phone.overview, 1.0);
        phone.navigate(PhoneScreen::Home);
        phone.navigation.open = true;
        phone.step(1.0 / 60.0);
        assert_eq!(phone.openness, 0.0);assert_eq!(phone.overview, 0.0);assert_eq!(phone.navigation.reveal, 1.0);
        phone.pages.sync(&["reference".into(), "news".into(), "mail".into()], 1, 1);
        phone.gesture_out = Some(ShellGesture::PageSwipe {dir:Dir::Left,progress:0.4});
        phone.step(1.0/60.0);
        assert!((phone.pages.position()-0.4).abs()<0.001, "page follows the finger before release");
        phone.gesture_out = Some(ShellGesture::Commit(GestureKind::Page(Dir::Left)));
        phone.step(1.0/60.0);
        assert_eq!(phone.pages.position(),1.0);
        phone.gesture_out = None;
        phone.navigate(PhoneScreen::Drawer);
        phone.search_scroll=500.0;phone.search_scroll_limit=2000.0;phone.search_velocity=-600.0;
        phone.step(1.0/60.0);
        assert_eq!(phone.search_scroll,510.0,"reduced motion cannot advance the physics clock");
        assert!(phone.search_velocity < -500.0);
    }

    /// A finger on the drawer the way phone_pointer_at (mobile_app.rs)
    /// drives it: the recognizer first; while it owns nothing the list
    /// scrolls; a committed Back from the library goes Home (commit_gesture).
    fn drawer_finger(phone: &mut PhoneState, points: &[(f64, f64)], secs: f64) {
        use crate::mobile_gestures::*;
        let ctx = GestureContext { screen: phone.viewport, insets: SafeInsets::default(), phone: phone.screen, body: true, glance: None, system_edges: true, shade: false };
        let zones = ExclusionZones::default();
        let mut rec = GestureRecognizer::default();
        let (start, t0) = (dvec2(points[0].0, points[0].1), 10.0);
        rec.feed(FingerPhase::Down, start, t0, &ctx, &zones);
        phone.search_touch(start.y, t0);
        let mut shell = rec.active();
        let mut last = start;
        let n = points.len() - 1;
        for (i, &(x, y)) in points.iter().enumerate().skip(1) {
            let (p, time) = (dvec2(x, y), t0 + secs * i as f64 / n as f64);
            let out = if shell { rec.feed(FingerPhase::Move, p, time, &ctx, &zones) } else { None };
            shell = rec.active();
            if out.is_none() { phone.search_drag(p.y - last.y, p.y, time, phone.search_scroll_limit); }
            last = p;
            phone.step(1.0 / 120.0);
        }
        let out = if shell { rec.feed(FingerPhase::Up, last, t0 + secs, &ctx, &zones) } else { None };
        // As mobile_app.rs: a pull on a list at its top closes search (Cancel).
        let close = !shell && phone.search_pull_closes();
        phone.search_lift(t0 + secs, (last - start).length() >= 12.0);
        if close || out == Some(ShellGesture::Commit(GestureKind::Back)) { phone.navigate(PhoneScreen::Home); }
    }
    fn searching_phone() -> PhoneState {
        let mut phone = PhoneState::default();
        phone.viewport = Rect { pos: dvec2(0.0, 0.0), size: dvec2(412.0, 892.0) };
        phone.open_search();
        phone.search_query = "a".into();
        phone.search_scroll_limit = 2000.0;
        phone
    }

    #[test]
    fn a_pull_down_on_search_already_at_the_top_closes_it() {
        // Straight down, and down along a right thumb's arc (it starts a
        // little sideways, and must not be taken for the swipe back): 300
        // points over six moves with the list at 0.
        let straight = [(200.0, 400.0), (200.0, 450.0), (200.0, 500.0), (200.0, 550.0), (200.0, 600.0), (200.0, 650.0), (200.0, 700.0)];
        let arc = [(200.0, 400.0), (214.0, 410.0), (240.0, 460.0), (270.0, 520.0), (295.0, 580.0), (315.0, 640.0), (330.0, 700.0)];
        for (what, path, secs) in [("straight", &straight, 0.3), ("arc", &arc, 0.3), ("fast arc", &arc, 0.1)] {
            let mut phone = searching_phone();
            drawer_finger(&mut phone, path, secs);
            assert!(!phone.searching(), "{what}: a pull at the top closes search");
            assert_eq!(phone.screen, PhoneScreen::Home, "{what}");
        }
    }

    #[test]
    fn the_search_field_lets_a_pull_down_go_to_the_list_and_keeps_taps_and_selection() {
        let start = dvec2(200.0, 300.0);
        // A pull down from the field, at the list's top: the list takes it.
        assert!(search_field_releases(start, dvec2(203.0, 314.0), 0.0));
        // A tap's jitter, a sideways selection drag and an upward drag stay
        // with the field.
        assert!(!search_field_releases(start, dvec2(201.0, 306.0), 0.0));
        assert!(!search_field_releases(start, dvec2(260.0, 320.0), 0.0));
        assert!(!search_field_releases(start, dvec2(200.0, 260.0), 0.0));
        // A scrolled list is not pulled from the field.
        assert!(!search_field_releases(start, dvec2(200.0, 340.0), 120.0));
    }
    #[test]
    fn a_short_pull_at_the_top_stretches_and_springs_back_without_closing_search() {
        let short = [(200.0, 400.0), (200.0, 420.0), (200.0, 440.0), (200.0, 460.0)];
        let mut phone = searching_phone();
        drawer_finger(&mut phone, &short, 0.3);
        assert!(phone.searching(), "60 points only stretches");
        assert_eq!(phone.search_query, "a");
        assert_eq!(phone.search_scroll, 0.0);
        assert!(phone.search_stretch > 0.0, "the list stretches under the finger");
        assert_eq!(phone.search_velocity, 0.0, "a stretched list does not coast");
        for _ in 0..60 { phone.step(1.0 / 60.0); }
        assert_eq!(phone.search_stretch, 0.0, "the stretch springs back");
        assert!(phone.searching());
    }

    #[test]
    fn a_pull_that_brings_the_list_back_to_its_top_keeps_search_open() {
        let path = [(200.0, 300.0), (200.0, 400.0), (200.0, 500.0), (200.0, 600.0), (200.0, 700.0)];
        let mut phone = searching_phone();
        phone.search_scroll = 250.0;
        drawer_finger(&mut phone, &path, 0.4);
        assert_eq!(phone.search_scroll, 0.0, "the list reached its top");
        assert!(phone.search_stretch > 0.0, "and stretched past it");
        assert!(phone.searching(), "it takes another pull from the top to close");
    }

    #[test]
    fn the_search_list_follows_the_finger_one_to_one_and_a_flick_coasts_and_slows() {
        let mut phone = searching_phone();
        phone.search_scroll = 500.0;
        phone.search_touch(600.0, 1.0);
        // 150 points up over six moves: the list scrolls exactly 150.
        for i in 1..=6 {
            phone.search_drag(-25.0, 600.0 - 25.0 * i as f64, 1.0 + 0.1 * i as f64, 2000.0);
            assert_eq!(phone.search_scroll, 500.0 + 25.0 * i as f64);
        }
        // Lifted after a pause: it stays put.
        phone.search_lift(1.8, true);
        assert_eq!(phone.search_velocity, 0.0);
        // A flick: 200 points in a tenth of a second, lifted at once.
        phone.search_touch(600.0, 2.0);
        for i in 1..=5 { phone.search_drag(-40.0, 600.0 - 40.0 * i as f64, 2.0 + 0.02 * i as f64, 2000.0); }
        assert_eq!(phone.search_scroll, 850.0);
        phone.search_lift(2.1, true);
        assert!((phone.search_velocity + 2000.0).abs() < 1.0, "the finger's speed: {}", phone.search_velocity);
        let mut before = phone.search_scroll;
        let mut speed = f64::MAX;
        for _ in 0..30 {
            phone.step(1.0 / 60.0);
            let moved = phone.search_scroll - before;
            assert!(moved > 0.0 && moved < speed, "it keeps going, slower each frame");
            speed = moved;
            before = phone.search_scroll;
        }
        for _ in 0..240 { phone.step(1.0 / 60.0); }
        assert_eq!(phone.search_velocity, 0.0, "and comes to rest");
        let coasted = phone.search_scroll - 850.0;
        assert!((coasted - 2000.0 / 4.0).abs() < 20.0, "about v/friction: {coasted}");
    }

    #[test]
    fn a_finger_turning_back_takes_up_the_stretch_before_the_list_moves() {
        let mut phone = searching_phone();
        phone.search_touch(400.0, 1.0);
        phone.search_drag(100.0, 500.0, 1.1, 2000.0);
        assert_eq!((phone.search_scroll, phone.search_stretch), (0.0, 45.0));
        phone.search_drag(-60.0, 440.0, 1.2, 2000.0);
        assert_eq!(phone.search_scroll, 0.0);
        assert!((phone.search_stretch - 18.0).abs() < 1e-9);
        phone.search_drag(-60.0, 380.0, 1.3, 2000.0);
        assert!((phone.search_scroll - 20.0).abs() < 1e-9 && phone.search_stretch == 0.0);
        // At the bottom it stretches the other way.
        phone.search_drag(-100.0, 280.0, 1.4, 50.0);
        assert_eq!(phone.search_scroll, 50.0);
        assert!((phone.search_stretch + 70.0 * SEARCH_STRETCH).abs() < 1e-9);
    }

    #[test]
    fn search_is_separate_from_the_catalog_even_after_a_previous_query() {
        let mut phone = PhoneState::default();
        phone.navigate(PhoneScreen::Drawer);
        assert!(!phone.searching());
        phone.open_search();
        assert!(phone.searching(), "a pull-down opens search before the editor gets focus");
        phone.search_query = "News".into();
        phone.search_focused = false;
        assert!(phone.searching(), "hiding the keyboard must not turn search into the catalog");
        phone.navigate(PhoneScreen::Home);
        phone.pages.sync(&["reference".into(), "news".into()], 1, 1);
        phone.pages.jump(phone.pages.library_index());
        phone.step(1.0 / 60.0);
        assert_eq!(phone.screen, PhoneScreen::Drawer);
        assert!(!phone.searching(), "paging into the catalog must not reopen the previous search");
    }

    #[test]
    fn a_docked_activity_becomes_a_shade_card_and_the_island_hides_under_the_open_shade() {
        use crate::mobile_gestures::{GestureKind, ShadeSide, ShellGesture};
        let mut phone = PhoneState::default();
        phone.viewport = Rect { pos: dvec2(0.0, 0.0), size: dvec2(412.0, 892.0) };
        phone.wallpaper_time = 42.0;
        phone.absorb_docked(vec![crate::mobile_island::DockedNote { id: "x".into(), app: "AppCard".into(), title: "Fetching forecast".into(), body: "done".into(), time: 7.0 }]);
        let note = &phone.shade.notifications[0];
        assert_eq!((note.app.as_str(), note.title.as_str(), note.time), ("AppCard", "Fetching forecast", 42.0), "stamped on the shade's clock");
        // The shade commits open: the island learns it on the same step.
        phone.gesture_out = Some(ShellGesture::Commit(GestureKind::Shade(ShadeSide::Notifications)));
        phone.step(1.0 / 60.0);
        assert!(phone.shade.wants_open() && phone.island.shade_open);
        phone.gesture_out = None;
        phone.shade.close();
        phone.step(1.0 / 60.0);
        assert!(!phone.island.shade_open, "the island returns as the sheet closes");
    }
    #[test]
    fn the_scene_composes_only_for_glass_and_skips_what_an_open_app_covers() {
        let mut phone = PhoneState::default();
        // An idle Android home page: nothing frosted, straight to the window.
        assert_eq!(phone.scene_plan(false), ScenePlan { compose: false, wallpaper: true, home: true });
        // iOS's dock is glass: the home page composes.
        assert_eq!(phone.scene_plan(true), ScenePlan { compose: true, wallpaper: true, home: true });
        // The shade over the home page samples the scene.
        phone.shade.open = 0.5;
        assert!(phone.scene_plan(false).compose);
        phone.shade.open = 0.0;
        // An open, settled app covers the wallpaper and the home page.
        phone.activate(3);
        for _ in 0..80 { phone.step(1.0 / 60.0); }
        assert_eq!(phone.scene_plan(false), ScenePlan { compose: false, wallpaper: false, home: false });
        // …until it starts to leave (a home swipe, Recents).
        phone.overview = 0.3;
        assert_eq!(phone.scene_plan(false), ScenePlan { compose: true, wallpaper: true, home: true });
        phone.overview = 0.0;
        phone.openness = 0.9;
        assert_eq!(phone.scene_plan(false), ScenePlan { compose: false, wallpaper: true, home: true });
    }
    #[test]
    fn both_orientations_reserve_system_bars_and_keep_selected_card_inside() {
        for size in [phone_size(DesktopStyle::Ios), phone_size(DesktopStyle::Android)] {
            for size in [size, dvec2(size.y, size.x)] {
                let screen = Rect { pos: dvec2(0.0, 32.0), size: size-dvec2(0.0,32.0) };
                let app = app_rect(screen);
                let card = card_rect(screen, 2.0, 2.0);
                assert!(app.size.x > 0.0 && app.size.y > 200.0);
                assert!(app.pos.y > screen.pos.y);
                assert!(card.pos.x >= app.pos.x && card.pos.y >= app.pos.y);
                assert!(card.pos.x+card.size.x <= app.pos.x+app.size.x);
                assert!(card.pos.y+card.size.y <= app.pos.y+app.size.y);
            }
        }
    }
    #[test]
    fn home_and_task_switcher_preserve_instances_and_settle() {
        let mut phone = PhoneState::default();
        phone.activate(10); phone.activate(11); phone.activate(10);
        assert_eq!(phone.order, [10,11]);
        for screen in [PhoneScreen::Recents, PhoneScreen::Home, PhoneScreen::App] {
            phone.navigate(screen);
            for _ in 0..80 { phone.step(1.0/60.0); }
            assert_eq!(phone.client, Some(10));
            assert_eq!(phone.order.len(), 2);
            assert!(!phone.step(1.0/60.0));
        }
        assert!(phone.accepts_app_input());
    }
    #[test]
    fn the_android_drawer_hides_the_wallpaper_even_under_glass() {
        let mut phone = PhoneState::default();
        phone.navigate(PhoneScreen::Drawer);
        assert_eq!(phone.scene_plan(false), ScenePlan { compose: false, wallpaper: false, home: true }, "the opaque drawer covers the wallpaper");
        phone.overview = 0.4;
        assert_eq!(phone.scene_plan(false), ScenePlan { compose: true, wallpaper: false, home: true }, "a home swipe from the drawer blurs the drawer, not the wallpaper");
        assert!(phone.scene_plan(true).wallpaper, "the iOS App Library is translucent: its wallpaper stays");
        phone.overview = 0.0;
        phone.navigate(PhoneScreen::Home);
        assert!(phone.scene_plan(false).wallpaper, "back on Home the wallpaper draws");
    }
    #[test]
    fn compact_faces_wait_for_the_home_page_to_settle() {
        let mut phone = PhoneState::default();
        assert!(phone.home_settled() && phone.foreground().is_none());
        phone.activate(4);
        for _ in 0..80 { phone.step(1.0/60.0); }
        assert_eq!(phone.foreground(), Some(4));
        assert!(!phone.home_settled() && !phone.home_visible());
        phone.navigate(PhoneScreen::Home);
        assert_eq!(phone.foreground(), None, "dismissed: no longer the app the person looks at");
        assert!(!phone.home_settled(), "the window is still animating into its icon");
        assert!(phone.home_visible());
        for _ in 0..80 { phone.step(1.0/60.0); }
        assert!(phone.home_settled());
        phone.navigate(PhoneScreen::Recents);
        for _ in 0..80 { phone.step(1.0/60.0); }
        assert!(!phone.home_settled(), "Recents keeps every card in its full face");
    }
}
