//! Phone shell state and geometry. Application viewports remain stable while
//! their compositor surfaces move between home, foreground and the task viewer.
use crate::{desktop::DesktopStyle, hub::ClientId, mobile_gestures::SafeInsets, mobile_tiles::HomeTiles};
use makepad_widgets::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PhoneScreen { #[default] Home, App, Recents, Drawer }

#[derive(Clone, Debug, PartialEq)]
pub enum PhoneHit {
    App(String), Card(ClientId), Home, Recents, Drawer, Back,
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
    pub screen: PhoneScreen,
}

#[derive(Clone)]
pub struct PhoneState {
    pub clock: String,
    /// The frame clock of the last stepped frame (the shade stamps its
    /// cards on it).
    pub wallpaper_time: f64,
    /// How far the wallpaper's ribbons have drifted: advances only while
    /// the shell animates or a finger is down, so an idle home page never
    /// asks for a frame just to move the wallpaper (mobile_app.rs).
    pub wallpaper_phase: f64,
    pub screen: PhoneScreen,
    pub client: Option<ClientId>,
    pub order: Vec<ClientId>,
    pub openness: f64,
    pub overview: f64,
    pub page: f64,
    pub dismiss_y: f64,
    pub gesture: Option<PhoneGesture>,
    /// Native touch owned by shell navigation; other fingers cannot replace it.
    pub touch: Option<u64>,
    pub keyboard: f64,
    pub keyboard_target: f64,
    pub keyboard_sent_height: f64,
    pub keyboard_client: Option<ClientId>,
    pub search_query: String,
    pub search_focused: bool,
    pub search_scroll: f64,
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
        Self { clock: "9:41".into(), wallpaper_time: 0.0, wallpaper_phase: 0.0, screen: PhoneScreen::Home, client: None, order: Vec::new(),
            openness: 0.0, overview: 0.0, page: 0.0, dismiss_y: 0.0, gesture: None, touch: None,
            keyboard: 0.0, keyboard_target: 0.0, keyboard_sent_height: 0.0, keyboard_client: None,
            search_query: String::new(), search_focused: false, search_scroll: 0.0,
            ime: HashMap::new(), shift: false, symbols: false,
            #[cfg(not(mobile_only))] desktop_size: None,
            #[cfg(not(mobile_only))] desktop_clients: Vec::new(),
            #[cfg(not(mobile_only))] desktop_style: DesktopStyle::Omarchy,
            viewport: Rect::default(), insets: SafeInsets::default(),
            tiles: HomeTiles::default(),
            gesture_out: None,
            exclusions: Default::default(),
            shade: Default::default(),
            pages: Default::default(),
            island: Default::default(),
            groups: Default::default() }
    }
}
impl PhoneState {
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
        self.search_focused = false;
        if self.client != Some(client) { self.keyboard_target = 0.0; }
        self.client = Some(client);
        self.order.retain(|c| *c != client);
        self.order.insert(0, client);
        self.page = 0.0;
        self.screen = PhoneScreen::App;
        self.dismiss_y = 0.0;
    }
    pub fn navigate(&mut self, screen: PhoneScreen) {
        self.search_focused = false;
        self.screen = screen;
        self.keyboard_target = 0.0;
        self.gesture = None;
        self.dismiss_y = 0.0;
    }
    pub fn step(&mut self, dt: f64) -> bool {
        let t = 1.0 - (-dt * 19.0).exp();
        let open = if matches!(self.screen, PhoneScreen::App | PhoneScreen::Recents) && self.client.is_some() { 1.0 } else { 0.0 };
        let overview = if self.screen == PhoneScreen::Recents { 1.0 } else { 0.0 };
        let mut active = false;
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
        active |= self.island.step(dt, crate::host::now(), self.gesture_out);
        active |= self.groups.step(dt);
        if self.gesture.is_none() {
            let target = self.page.round().clamp(0.0, self.order.len().saturating_sub(1) as f64);
            self.page += (target - self.page) * t;
            if (target - self.page).abs() < 0.001 { self.page = target; }
            active |= self.page != target;
        }
        active |= self.shade.step(dt, self.gesture_out, self.wallpaper_time);
        self.absorb_docked(crate::mobile_island::take_docked());
        // The island stays hidden while the sheet is (or is about to be)
        // open and comes back as it closes.
        self.island.set_shade_open(self.shade.wants_open());
        active |= self.pages.step(dt, if self.screen == PhoneScreen::Home { self.gesture_out } else { None });
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
        self.screen == PhoneScreen::Drawer && (self.search_focused || !self.search_query.is_empty())
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
    let top = if screen.size.x > screen.size.y { 24.0 } else { 42.0 };
    Rect { pos: screen.pos + dvec2(0.0, top), size: dvec2(screen.size.x, (screen.size.y - top - 24.0).max(1.0)) }
}
pub fn card_rect(screen: Rect, index: f64, page: f64) -> Rect {
    let app = app_rect(screen);
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
