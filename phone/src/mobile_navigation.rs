//! App-local floating navigation. Nothing here registers a system overlay or
//! recognizes an edge gesture; only a touch starting on our controls is owned.
use makepad_widgets::*;

pub const ENABLED: bool = cfg!(any(target_os = "android", target_env = "ohos"));
pub const DIAMETER: f64 = 48.0;
const EDGE: f64 = 28.0;
const VERTICAL: f64 = 40.0;
const DRAG_SLOP: f64 = 10.0;

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
}

impl Default for FloatingNavigation {
    fn default() -> Self {
        Self { position: dvec2(1.0, 0.58), right: true, open: false, reveal: 0.0, held: None }
    }
}

pub struct Layout {
    pub bubble: Rect,
    pub panel: Rect,
    pub home: Rect,
    pub recents: Rect,
}

/// Native bars have already been excluded from `screen`. Stay farther inward
/// than the OS gesture bands, including when the software keyboard is shown.
fn travel(screen: Rect) -> Rect {
    let x = EDGE.min((screen.size.x - DIAMETER).max(0.0) * 0.5);
    let y = VERTICAL.min((screen.size.y - DIAMETER).max(0.0) * 0.5);
    Rect { pos: screen.pos + dvec2(x, y), size: dvec2(
        (screen.size.x - 2.0 * x - DIAMETER).max(0.0),
        (screen.size.y - 2.0 * y - DIAMETER).max(0.0),
    ) }
}

impl FloatingNavigation {
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
        let t = 1.0 - (-dt * 24.0).exp();
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
    fn screen() -> Rect { Rect { pos: dvec2(0.0, 0.0), size: dvec2(406.0, 777.0) } }
    fn center(r: Rect) -> Vec2d { r.pos + r.size * 0.5 }
    fn tap(nav: &mut FloatingNavigation, point: Vec2d) -> (bool, Option<NavigationHit>) {
        assert!(nav.pointer(Phase::Down, point, screen()).0);
        nav.pointer(Phase::Up, point, screen())
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
    fn mobile_floating_moves_above_native_keyboard_then_restores_its_height() {
        let mut phone = crate::mobile::PhoneState::default();
        phone.viewport = screen();
        let before = phone.navigation.layout(phone.navigation_rect()).bubble;
        phone.native_keyboard_event(&VirtualKeyboardEvent::DidShow { time: 1.0, height: 330.0 });
        let shown = phone.navigation.layout(phone.navigation_rect());
        assert!(shown.bubble.pos.y < before.pos.y);
        assert!(shown.bubble.pos.y + shown.bubble.size.y <= screen().size.y - 330.0 - VERTICAL);
        assert!(shown.panel.pos.y + shown.panel.size.y <= screen().size.y - 330.0 - VERTICAL);
        phone.native_keyboard_event(&VirtualKeyboardEvent::DidHide { time: 2.0 });
        assert_eq!(phone.navigation.layout(phone.navigation_rect()).bubble, before);
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
