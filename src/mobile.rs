//! Phone shell state and geometry. Application viewports remain stable while
//! their compositor surfaces move between home, foreground and the task viewer.
use crate::{desktop::DesktopStyle, hub::ClientId, mobile_tiles::HomeTiles};
use makepad_widgets::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PhoneScreen { #[default] Home, App, Recents, Drawer }

#[derive(Clone, Debug, PartialEq)]
pub enum PhoneHit {
    App(String), Card(ClientId), Close(ClientId), Home, Recents, Drawer, Back,
    Rotate, Style, Appearance, Desktop, Key(String), Shift, Symbols, HideKeyboard,
    ClearSearch, CancelSearch,
}

#[derive(Clone)]
pub struct PhoneGesture {
    pub start: Vec2d,
    pub last: Vec2d,
    pub time: f64,
    pub hit: Option<PhoneHit>,
    pub bottom: bool,
    pub edge: bool,
    pub screen: PhoneScreen,
}

#[derive(Clone)]
pub struct PhoneState {
    pub clock: String,
    pub wallpaper_time: f64,
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
    pub desktop_size: Option<Vec2d>,
    pub desktop_clients: Vec<ClientId>,
    pub desktop_style: DesktopStyle,
    pub viewport: Rect,
    /// The home page's live app tiles (mobile_tiles.rs): which client shows
    /// which tile and in which face.
    pub tiles: HomeTiles,
}
impl Default for PhoneState {
    fn default() -> Self {
        Self { clock: "9:41".into(), wallpaper_time: 0.0, screen: PhoneScreen::Home, client: None, order: Vec::new(),
            openness: 0.0, overview: 0.0, page: 0.0, dismiss_y: 0.0, gesture: None, touch: None,
            keyboard: 0.0, keyboard_target: 0.0, keyboard_sent_height: 0.0, keyboard_client: None,
            search_query: String::new(), search_focused: false, search_scroll: 0.0,
            ime: HashMap::new(), shift: false, symbols: false,
            desktop_size: None, desktop_clients: Vec::new(), desktop_style: DesktopStyle::Omarchy, viewport: Rect::default(),
            tiles: HomeTiles::default() }
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
        let dragging = self.gesture.as_ref().is_some_and(|g| g.bottom);
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
        if self.gesture.is_none() {
            let target = self.page.round().clamp(0.0, self.order.len().saturating_sub(1) as f64);
            self.page += (target - self.page) * t;
            if (target - self.page).abs() < 0.001 { self.page = target; }
            active |= self.page != target;
        }
        active
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

pub fn phone_size(style: DesktopStyle) -> Vec2d {
    if style == DesktopStyle::Ios { dvec2(402.0, 874.0) } else { dvec2(412.0, 892.0) }
}
/// A real phone (iOS, Android, OpenHarmony) rather than the desktop's phone
/// emulation: the platform owns the window size, the keyboard, rotation and
/// the appearance, so the emulation's controls (the style/appearance/rotate
/// toolbar, the drawn keyboard, window resizing) stay out of the way.
pub const fn real_phone() -> bool {
    cfg!(any(target_os = "ios", target_os = "android", target_env = "ohos"))
}
/// The platform draws its status bar OUTSIDE our surface: OpenHarmony lays
/// the XComponent out under the system bar, so the emulated status bar is
/// not drawn and the app area starts at the top of the surface. iOS and
/// Android overlay theirs on the surface instead and keep the emulated bar
/// as a placeholder until safe-area insets are wired through.
pub const fn status_bar_outside() -> bool {
    cfg!(target_env = "ohos")
}
/// A device whose own system owns the bottom-edge gesture: OpenHarmony's home
/// swipe backgrounds the whole app before the shell sees it, so the shell
/// cannot use that swipe to leave an app. Where this holds the phone shows a
/// tappable Back / Home / Recents bar instead of the thin home indicator.
pub const fn nav_buttons() -> bool {
    cfg!(target_env = "ohos")
}
/// A strip kept clear at the very bottom, above the platform's own home bar,
/// so the shell's nav buttons do not sit under it.
pub fn bottom_inset() -> f64 {
    if nav_buttons() { 18.0 } else { 0.0 }
}
/// The bottom navigation strip's height: a real button bar where the swipe is
/// unavailable, the thin home indicator otherwise.
pub fn nav_height() -> f64 {
    if nav_buttons() { 52.0 } else { 24.0 }
}
/// The emulated status bar's height on this screen, 0 where the platform
/// keeps its own outside the surface.
pub fn status_height(screen: Rect) -> f64 {
    if status_bar_outside() { 0.0 } else if screen.size.x > screen.size.y { 24.0 } else { 42.0 }
}
pub fn app_rect(screen: Rect) -> Rect {
    let top = status_height(screen);
    let bottom = nav_height() + bottom_inset();
    Rect { pos: screen.pos + dvec2(0.0, top), size: dvec2(screen.size.x, (screen.size.y - top - bottom).max(1.0)) }
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
