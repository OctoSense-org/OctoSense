//! App-local floating navigation. Nothing here registers a system overlay or
//! recognizes an edge gesture; only a touch starting on our controls is owned.
use makepad_widgets::*;

pub const ENABLED: bool = cfg!(any(target_os = "android", target_env = "ohos"));
pub const DIAMETER: f64 = 48.0;
const EDGE: f64 = 28.0;
const VERTICAL: f64 = 40.0;
const DRAG_SLOP: f64 = 10.0;

/// Android hosted apps reserve this row outside their content. System Back
/// remains Android-owned; these controls navigate inside OctoSense.
pub const APP_DOCK_HEIGHT: f64 = 48.0;
#[derive(Clone, Copy, Debug)]
pub struct AppDock {
    pub bar: Rect,
    pub content: Rect,
    pub home: Rect,
    pub recents: Rect,
}
pub fn app_dock(screen: Rect) -> AppDock {
    let height = APP_DOCK_HEIGHT.min(screen.size.y.max(0.0));
    let bar = Rect { pos: screen.pos + dvec2(0.0, screen.size.y - height), size: dvec2(screen.size.x, height) };
    let content = Rect { pos: screen.pos, size: dvec2(screen.size.x, screen.size.y - height) };
    let width = 72.0_f64.min(screen.size.x * 0.5);
    let home = Rect { pos: bar.pos + dvec2(screen.size.x * 0.5 - width, 0.0), size: dvec2(width, height) };
    let recents = Rect { pos: bar.pos + dvec2(screen.size.x * 0.5, 0.0), size: dvec2(width, height) };
    AppDock { bar, content, home, recents }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NavigationHit { Bubble, Home, Recents, Dismiss }

#[derive(Clone, Copy, PartialEq)]
pub enum Phase { Down, Move, Up, Scroll }

#[derive(Clone)]
struct Held {
    hit: NavigationHit,
    start: Vec2d,
    origin: Vec2d,
    moved: bool,
}

#[derive(Clone)]
pub struct FloatingNavigation {
    /// Normalized travel keeps the chosen height when the viewport changes.
    position: Vec2d,
    right: bool,
    pub open: bool,
    pub reveal: f64,
    held: Option<Held>,
    /// Leave the editor's reduced viewport entirely to the app while the
    /// native IME is showing. Android's system navigation remains available.
    ime_visible: bool,
}

impl Default for FloatingNavigation {
    fn default() -> Self {
        Self { position: dvec2(1.0, 0.58), right: true, open: false, reveal: 0.0, held: None, ime_visible: false }
    }
}

pub struct Layout {
    pub bubble: Rect,
    pub panel: Rect,
    pub home: Rect,
    pub recents: Rect,
}

/// Native bars have already been excluded from `screen`. Stay farther inward
/// than the OS gesture bands. IME visibility is separate from this geometry.
fn travel(screen: Rect) -> Rect {
    let x = EDGE.min((screen.size.x - DIAMETER).max(0.0) * 0.5);
    let y = VERTICAL.min((screen.size.y - DIAMETER).max(0.0) * 0.5);
    Rect { pos: screen.pos + dvec2(x, y), size: dvec2(
        (screen.size.x - 2.0 * x - DIAMETER).max(0.0),
        (screen.size.y - 2.0 * y - DIAMETER).max(0.0),
    ) }
}

impl FloatingNavigation {
    pub fn visible(&self) -> bool { !self.ime_visible }
    pub fn set_ime_visible(&mut self, visible: bool) {
        self.ime_visible = visible;
        if visible {
            self.cancel();
            self.reveal = 0.0;
        }
    }
    pub fn tracking(&self) -> bool { self.held.is_some() }
    pub fn pressed(&self) -> Option<NavigationHit> { self.held.as_ref().map(|h| h.hit) }
    pub fn cancel(&mut self) { self.held = None; self.open = false; }

    pub fn layout(&self, screen: Rect) -> Layout {
        let range = travel(screen);
        let bubble = Rect { pos: range.pos + dvec2(range.size.x * self.position.x, range.size.y * self.position.y), size: dvec2(DIAMETER, DIAMETER) };
        let width = (screen.size.x - EDGE * 2.0 - DIAMETER - 12.0).clamp(120.0, 240.0);
        let height = 124.0_f64.min((screen.size.y - VERTICAL * 2.0).max(80.0));
        let x = if self.right { screen.pos.x + screen.size.x - EDGE - DIAMETER - width - 12.0 }
            else { screen.pos.x + EDGE + DIAMETER + 12.0 };
        let top = VERTICAL.min((screen.size.y - height).max(0.0) * 0.5);
        let y = (bubble.pos.y + DIAMETER * 0.5 - height * 0.5)
            .clamp(screen.pos.y + top, screen.pos.y + (screen.size.y - height - top).max(top));
        let panel = Rect { pos: dvec2(x, y), size: dvec2(width, height) };
        let item_width = (width - 28.0) * 0.5;
        let home = Rect { pos: panel.pos + dvec2(10.0, 34.0), size: dvec2(item_width, height - 44.0) };
        let recents = Rect { pos: home.pos + dvec2(item_width + 8.0, 0.0), size: home.size };
        Layout { bubble, panel, home, recents }
    }

    pub fn hit(&self, screen: Rect, point: Vec2d) -> Option<NavigationHit> {
        if !self.visible() { return None; }
        let layout = self.layout(screen);
        if layout.bubble.contains(point) { return Some(NavigationHit::Bubble); }
        if !self.open { return None; }
        if layout.home.contains(point) { return Some(NavigationHit::Home); }
        if layout.recents.contains(point) { return Some(NavigationHit::Recents); }
        // Consume a dismissal through Up, so its release cannot click a link
        // or begin a scroll in the hosted app underneath.
        Some(NavigationHit::Dismiss)
    }

    /// Returns (consumed, navigation action). Movement owns the whole stream,
    /// even if the finger leaves the bubble or crosses an app control.
    pub fn pointer(&mut self, phase: Phase, point: Vec2d, screen: Rect) -> (bool, Option<NavigationHit>) {
        if !self.visible() { return (false, None); }
        match phase {
            Phase::Down => {
                let Some(hit) = self.hit(screen, point) else { return (false, None); };
                self.held = Some(Held { hit, start: point, origin: self.layout(screen).bubble.pos, moved: false });
                if hit == NavigationHit::Dismiss { self.open = false; }
            }
            Phase::Move | Phase::Up => {
                let range = travel(screen);
                let Some(held) = self.held.as_mut() else { return (false, None); };
                let delta = point - held.start;
                held.moved |= delta.length() >= DRAG_SLOP;
                if held.hit == NavigationHit::Bubble && held.moved {
                    self.open = false;
                    let pos = held.origin + delta - range.pos;
                    self.position = dvec2((pos.x / range.size.x.max(1.0)).clamp(0.0, 1.0),
                                          (pos.y / range.size.y.max(1.0)).clamp(0.0, 1.0));
                }
                if phase == Phase::Up {
                    let held = self.held.take().unwrap();
                    if held.hit == NavigationHit::Bubble {
                        if held.moved { self.right = self.position.x >= 0.5; }
                        else { self.open = !self.open; }
                    } else if !held.moved && self.hit(screen, point) == Some(held.hit)
                        && matches!(held.hit, NavigationHit::Home | NavigationHit::Recents) {
                        self.open = false;
                        return (true, Some(held.hit));
                    }
                }
            }
            Phase::Scroll => return (self.open || self.tracking(), None),
        }
        (true, None)
    }

    pub fn step(&mut self, dt: f64) -> bool {
        self.step_with_motion(dt, false)
    }
    pub fn step_with_motion(&mut self, dt: f64, reduced: bool) -> bool {
        let t = if reduced {1.0} else {1.0 - (-dt * 24.0).exp()};
        let tracking = self.tracking();
        let mut active = false;
        let mut settle = |value: &mut f64, target: f64| {
            *value += (target - *value) * t;
            if (*value - target).abs() < 0.001 { *value = target; }
            active |= *value != target;
        };
        settle(&mut self.reveal, if self.open { 1.0 } else { 0.0 });
        if !tracking { settle(&mut self.position.x, if self.right { 1.0 } else { 0.0 }); }
        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_platform::event::Ease;
    fn screen() -> Rect { Rect { pos: dvec2(0.0, 0.0), size: dvec2(406.0, 777.0) } }
    fn center(r: Rect) -> Vec2d { r.pos + r.size * 0.5 }
    fn tap(nav: &mut FloatingNavigation, point: Vec2d) -> (bool, Option<NavigationHit>) {
        assert!(nav.pointer(Phase::Down, point, screen()).0);
        nav.pointer(Phase::Up, point, screen())
    }

    #[test]
    fn android_app_dock_partitions_the_native_viewport_without_covering_content() {
        for screen in [screen(), Rect { pos: dvec2(12.0, 28.0), size: dvec2(780.0, 360.0) }] {
            let dock = app_dock(screen);
            assert_eq!(dock.content.pos, screen.pos);
            assert_eq!(dock.content.size.x, screen.size.x);
            assert_eq!(dock.content.pos.y + dock.content.size.y, dock.bar.pos.y);
            assert_eq!(dock.bar.pos.y + dock.bar.size.y, screen.pos.y + screen.size.y);
            assert_eq!(dock.bar.size.y, 48.0);
            let recent = crate::mobile::card_rect_for_app(screen, dock.content, 0.0, 0.0);
            assert!((recent.size.x / recent.size.y - dock.content.size.x / dock.content.size.y).abs() < 0.0001, "Recents must preserve the captured app's aspect ratio");
            for control in [dock.home, dock.recents] {
                assert!(control.size.x >= 44.0 && control.size.y >= 44.0);
                assert!(dock.bar.contains(center(control)));
                assert!(!dock.content.contains(center(control)));
            }
            assert!(dock.home.pos.x + dock.home.size.x <= dock.recents.pos.x);
        }
    }

    #[test]
    fn app_dock_is_android_foreground_only_and_returns_space_to_the_ime() {
        use crate::mobile::{PhoneScreen, PhoneState};
        let mut phone = PhoneState::default();
        phone.viewport = screen();
        assert!(phone.app_dock_for_platform(screen(), true).is_none());
        phone.screen = PhoneScreen::App;
        let before = phone.app_dock_for_platform(screen(), true).unwrap();
        for page in [PhoneScreen::Home, PhoneScreen::Recents, PhoneScreen::App] {
            phone.screen = page;
            assert_eq!(phone.app_content_rect_for_platform(screen(), true), before.content, "app capture geometry stays stable across navigation");
        }
        assert!(phone.app_dock_for_platform(screen(), false).is_none(), "OpenHarmony keeps its existing floating navigation");
        phone.native_keyboard_event(&VirtualKeyboardEvent::WillShow {
            time: 0.0, height: 330.0, duration: 0.2, ease: Ease::OutCubic,
        });
        phone.viewport.size.y -= 330.0;
        assert!(phone.app_dock_for_platform(phone.viewport, true).is_none(), "KeyboardView's remaining viewport must have no extra reserved row");
        assert_eq!(phone.app_content_rect_for_platform(phone.viewport, true), phone.viewport);
        phone.native_keyboard_event(&VirtualKeyboardEvent::WillHide {
            time: 0.5, height: 0.0, duration: 0.2, ease: Ease::OutCubic,
        });
        assert!(phone.app_dock_for_platform(phone.viewport, true).is_none());
        phone.native_keyboard_event(&VirtualKeyboardEvent::DidHide { time: 1.0 });
        phone.viewport = screen();
        assert_eq!(phone.app_dock_for_platform(screen(), true).unwrap().content, before.content);
        phone.screen = PhoneScreen::Recents;
        assert!(phone.app_dock_for_platform(screen(), true).is_none());
    }

    #[test]
    fn hidden_app_dock_cancels_the_held_action_but_consumes_its_release() {
        use crate::mobile::{PhoneGesture, PhoneHit, PhoneScreen, PhoneState};
        let mut phone = PhoneState::default();
        phone.screen = PhoneScreen::App;
        phone.touch = Some(42);
        phone.gesture = Some(PhoneGesture {
            start: center(app_dock(screen()).home), last: center(app_dock(screen()).home),
            time: 0.0, hit: Some(PhoneHit::AppNavigation(NavigationHit::Home)),
            shell: false, glance_scroll: false, screen: PhoneScreen::App,
        });
        phone.native_keyboard_event(&VirtualKeyboardEvent::DidShow { time: 1.0, height: 330.0 });
        assert_eq!(phone.touch, Some(42));
        assert_eq!(phone.gesture.as_ref().unwrap().hit, Some(PhoneHit::AppNavigation(NavigationHit::Dismiss)));
        assert!(!phone.gesture.as_ref().unwrap().shell);
        phone.native_keyboard_event(&VirtualKeyboardEvent::DidHide { time: 2.0 });
        assert_eq!(phone.gesture.as_ref().unwrap().hit, Some(PhoneHit::AppNavigation(NavigationHit::Dismiss)), "the old Home action must not return with the controls");
        phone.cancel_navigation_input();
        assert!(phone.gesture.is_none() && phone.touch.is_none(), "focus loss may never deliver the old release");
    }

    #[test]
    fn mobile_floating_tap_opens_and_dispatches_home_then_collapses() {
        let mut nav = FloatingNavigation::default();
        let bubble = center(nav.layout(screen()).bubble);
        assert_eq!(tap(&mut nav, bubble), (true, None));
        assert!(nav.open);
        let home = center(nav.layout(screen()).home);
        assert_eq!(tap(&mut nav, home), (true, Some(NavigationHit::Home)));
        assert!(!nav.open);
        tap(&mut nav, bubble);
        let recents = center(nav.layout(screen()).recents);
        assert_eq!(tap(&mut nav, recents), (true, Some(NavigationHit::Recents)));
    }

    #[test]
    fn mobile_floating_drag_owns_stream_and_docks_without_toggling_or_navigating() {
        let mut nav = FloatingNavigation::default();
        let bubble = center(nav.layout(screen()).bubble);
        assert_eq!(nav.pointer(Phase::Down, bubble, screen()), (true, None));
        assert_eq!(nav.pointer(Phase::Move, dvec2(-90.0, 100.0), screen()), (true, None));
        assert_eq!(nav.pointer(Phase::Up, dvec2(80.0, 200.0), screen()), (true, None));
        for _ in 0..90 { nav.step(1.0 / 60.0); }
        assert!(!nav.open && !nav.tracking());
        assert_eq!(nav.layout(screen()).bubble.pos.x, EDGE);
        assert!(!nav.step(1.0 / 60.0));
    }

    #[test]
    fn mobile_floating_dismiss_never_clicks_through_and_body_is_free_when_closed() {
        let mut nav = FloatingNavigation::default();
        let point = dvec2(160.0, 200.0);
        for phase in [Phase::Down, Phase::Move, Phase::Up] {
            assert_eq!(nav.pointer(phase, point, screen()), (false, None));
        }
        let bubble = center(nav.layout(screen()).bubble);
        tap(&mut nav, bubble);
        for phase in [Phase::Down, Phase::Move, Phase::Up] {
            assert_eq!(nav.pointer(phase, point, screen()), (true, None));
        }
        assert!(!nav.open && !nav.tracking());
    }

    #[test]
    fn mobile_floating_movement_cancels_actions_and_focus_loss_cancels_drag() {
        let mut nav = FloatingNavigation::default();
        nav.open = true;
        let home = center(nav.layout(screen()).home);
        nav.pointer(Phase::Down, home, screen());
        nav.pointer(Phase::Move, home + dvec2(20.0, 0.0), screen());
        assert_eq!(nav.pointer(Phase::Up, home, screen()), (true, None));
        nav.cancel();
        assert!(!nav.tracking() && !nav.open);
    }

    #[test]
    fn mobile_floating_keyboard_hides_controls_and_restores_the_chosen_dock() {
        let mut phone = crate::mobile::PhoneState::default();
        phone.viewport = screen();
        let before = phone.navigation.layout(phone.navigation_rect()).bubble;
        tap(&mut phone.navigation, center(before));
        phone.navigation.step_with_motion(1.0, true);
        let home = center(phone.navigation.layout(screen()).home);
        phone.navigation.pointer(Phase::Down, home, screen());
        phone.touch = Some(7);
        phone.native_keyboard_event(&VirtualKeyboardEvent::WillShow {
            time: 0.5, height: 330.0, duration: 0.2, ease: Ease::OutCubic,
        });
        assert!(!phone.navigation.visible());
        assert!(!phone.navigation.open && !phone.navigation.tracking());
        assert_eq!(phone.navigation.reveal, 0.0);
        assert_eq!(phone.touch, Some(7), "the shell still consumes the old owned finger through release");
        phone.native_keyboard_event(&VirtualKeyboardEvent::DidShow { time: 1.0, height: 330.0 });
        // KeyboardView still reflows the app, but no bubble or panel owns
        // any of that smaller editing area, even where the controls were.
        phone.viewport.size.y -= 330.0;
        assert_eq!(phone.navigation_rect(), phone.viewport);
        for point in [home, center(before)] {
            assert_eq!(phone.navigation.hit(phone.navigation_rect(), point), None);
            for phase in [Phase::Down, Phase::Move, Phase::Up, Phase::Scroll] {
                assert_eq!(phone.navigation.pointer(phase, point, phone.viewport), (false, None));
            }
        }
        phone.native_keyboard_event(&VirtualKeyboardEvent::WillHide {
            time: 1.5, height: 0.0, duration: 0.2, ease: Ease::OutCubic,
        });
        assert!(!phone.navigation.visible(), "do not obscure editing during the IME exit animation");
        phone.native_keyboard_event(&VirtualKeyboardEvent::DidHide { time: 2.0 });
        phone.viewport = screen();
        assert!(phone.navigation.visible());
        assert_eq!(phone.navigation.layout(phone.navigation_rect()).bubble, before);
        assert_eq!(phone.navigation.pointer(Phase::Up, home, screen()), (false, None), "a cancelled Home press cannot fire after IME dismissal");
        assert!(!phone.navigation.open);
        assert_eq!(tap(&mut phone.navigation, center(before)), (true, None));
        assert!(phone.navigation.open, "quick actions remain available after keyboard dismissal");
    }

    #[test]
    fn mobile_floating_keyboard_visibility_does_not_depend_on_reported_height() {
        let mut phone = crate::mobile::PhoneState::default();
        phone.navigation.position = dvec2(0.0, 0.25);
        phone.navigation.right = false;
        let dock = phone.navigation.layout(screen()).bubble;
        phone.native_keyboard_event(&VirtualKeyboardEvent::WillShow {
            time: 0.0, height: 0.0, duration: 0.2, ease: Ease::OutCubic,
        });
        assert!(!phone.navigation.visible(), "an IME with unknown or floating height still owns editing");
        phone.native_keyboard_event(&VirtualKeyboardEvent::DidHide { time: 1.0 });
        assert!(phone.navigation.visible());
        assert_eq!(phone.navigation.layout(screen()).bubble, dock);
    }

    #[test]
    fn mobile_floating_focus_loss_releases_touch_cancelled_by_ime() {
        let mut phone = crate::mobile::PhoneState::default();
        let bubble = center(phone.navigation.layout(screen()).bubble);
        assert!(phone.navigation.pointer(Phase::Down, bubble, screen()).0);
        phone.touch = Some(42);
        phone.native_keyboard_event(&VirtualKeyboardEvent::DidShow { time: 1.0, height: 330.0 });
        assert_eq!(phone.touch, Some(42), "a normal release must still be consumed");
        assert!(!phone.navigation.tracking(), "IME already cancelled the held control");
        phone.cancel_navigation_input();
        assert_eq!(phone.touch, None, "focus loss may never deliver the old finger's release");
        phone.native_keyboard_event(&VirtualKeyboardEvent::DidHide { time: 2.0 });
        assert_eq!(phone.navigation.pointer(Phase::Up, bubble, screen()), (false, None));
        assert_eq!(tap(&mut phone.navigation, bubble), (true, None));
        assert!(phone.navigation.open, "new input is usable after returning to the app");
    }

    #[test]
    fn glance_navigation_stays_outside_card_content_with_and_without_ime() {
        let mut phone = crate::mobile::PhoneState::default();
        phone.viewport = screen();
        phone.pages.index = -1.0;
        for height in [777.0, 447.0, 280.0] {
            phone.viewport.size.y = height;
            let header = phone.navigation_rect();
            let layout = phone.navigation.layout(header);
            let content_top = phone.viewport.pos.y + crate::mobile_pages::GLANCE_HEADER;
            for r in [layout.bubble, layout.panel] {
                assert!(r.pos.y + r.size.y <= content_top, "navigation is confined to the reserved header");
            }
            assert_eq!(phone.navigation.hit(header, dvec2(300.0, content_top + 40.0)), None);
        }
        phone.pages.index = 0.0;
        assert_eq!(phone.navigation_rect(), phone.viewport, "ordinary home keeps its movable navigation");
    }

    #[test]
    fn mobile_floating_layout_avoids_system_edges_at_both_docks_and_after_resize() {
        for size in [dvec2(406.0, 777.0), dvec2(777.0, 320.0), dvec2(320.0, 440.0), dvec2(406.0, 280.0)] {
            let screen = Rect { pos: dvec2(8.0, 58.0), size };
            for right in [false, true] { for y in [0.0, 0.58, 1.0] {
                let nav = FloatingNavigation { position: dvec2(if right { 1.0 } else { 0.0 }, y), right, ..Default::default() };
                let layout = nav.layout(screen);
                for r in [layout.bubble, layout.panel, layout.home, layout.recents] {
                    assert!(r.pos.x >= screen.pos.x + EDGE && r.pos.y >= screen.pos.y + VERTICAL);
                    assert!(r.pos.x + r.size.x <= screen.pos.x + screen.size.x - EDGE);
                    assert!(r.pos.y + r.size.y <= screen.pos.y + screen.size.y - VERTICAL);
                    assert!(r.size.x >= 44.0 && r.size.y >= 44.0);
                }
                assert!(!layout.panel.contains(center(layout.bubble)));
            }}
        }
    }
}
