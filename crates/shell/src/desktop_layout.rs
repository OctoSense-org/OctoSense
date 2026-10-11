//! Floating presentation over the persistent tiling tree. Switching styles never
//! destroys groups, split ratios, workspace membership, or an app instance.
use crate::layout::{ClientId, LRect};

#[derive(Clone, Debug)]
pub struct DesktopWindow {
    pub client: ClientId,
    pub rect: LRect,
    pub minimized: bool,
    pub maximized: bool,
    pub snap: Option<crate::snap::Zone>,
}
#[derive(Clone, Debug, Default)]
pub struct DesktopWindows {
    pub enabled: bool,
    pub windows: Vec<DesktopWindow>,
}

/// The margins a new window keeps from the desk's edges: it opens this far
/// in from the left and the top (each later window cascading 30 points
/// further), and a window opened at an app's own size ([`preferred_size`])
/// keeps them on the right and the bottom too.
pub const MARGIN_X: f64 = 42.0;
pub const MARGIN_Y: f64 = 32.0;
/// The smallest a window gets: what a resize drag leaves it.
pub const MIN_W: f64 = 80.0;
pub const MIN_H: f64 = 60.0;

/// The part of the desk `area` a window at its app's own size may use: all
/// of it, less the `dock` points a dock that floats over the desk covers at
/// its bottom (`desktop::dock_overlay`; 0 for a dock that reserves its strip
/// or is absent).
pub fn room(area: LRect, dock: f64) -> LRect {
    LRect::new(area.x, area.y, area.w, (area.h - dock.max(0.0)).max(1.0))
}

/// The size a window whose app asks for `w` x `h` points (a script app's
/// manifest `window`) opens at on `area`, under a dock that covers `dock`
/// points of its bottom: that size, clamped to the [`room`] above the dock
/// minus [`MARGIN_X`] and [`MARGIN_Y`] on each side, so the window ends above
/// the dock, and never below [`MIN_W`] x [`MIN_H`].
pub fn preferred_size(w: f64, h: f64, area: LRect, dock: f64) -> (f64, f64) {
    let room = room(area, dock);
    (w.min(room.w - 2.0 * MARGIN_X).max(MIN_W), h.min(room.h - 2.0 * MARGIN_Y).max(MIN_H))
}

impl DesktopWindows {
    /// The client's window, made at the default size if it has none: 72% of
    /// the desk's width and 76% of its height, at most 1000 x 720 points.
    pub fn ensure(&mut self, client: ClientId, area: LRect) {
        self.ensure_sized(client, area, None, 0.0);
    }

    /// [`ensure`](Self::ensure) for a client whose app asks to open at
    /// `preferred` (width, height) points: [`preferred_size`] instead of the
    /// default size, placed as every window is, in the [`room`] above a dock
    /// that covers `dock` points of the desk's bottom, so a cascaded window
    /// ends above the dock too. `None` is the default size on the whole desk,
    /// whatever the dock.
    pub fn ensure_sized(&mut self, client: ClientId, area: LRect, preferred: Option<(f64, f64)>, dock: f64) {
        if self.get(client).is_some() {
            return;
        }
        let offset = (self.windows.len() % 7) as f64 * 30.0;
        let (bounds, (w, h)) = match preferred {
            Some((w, h)) => (room(area, dock), preferred_size(w, h, area, dock)),
            None => (area, ((area.w * 0.72).min(1000.0), (area.h * 0.76).min(720.0))),
        };
        let mut rect = bounds.centered(w, h);
        rect.x = (bounds.x + MARGIN_X + offset).min(bounds.x + bounds.w - rect.w);
        rect.y = (bounds.y + MARGIN_Y + offset).min(bounds.y + bounds.h - rect.h);
        self.windows.push(DesktopWindow {
            client,
            rect,
            minimized: false,
            maximized: false,
            snap: None,
        });
    }
    pub fn get(&self, client: ClientId) -> Option<&DesktopWindow> {
        self.windows.iter().find(|w| w.client == client)
    }
    pub fn get_mut(&mut self, client: ClientId) -> Option<&mut DesktopWindow> {
        self.windows.iter_mut().find(|w| w.client == client)
    }
    pub fn minimized(&self, client: ClientId) -> bool {
        self.enabled && self.get(client).is_some_and(|w| w.minimized)
    }
    pub fn raise(&mut self, client: ClientId) {
        if let Some(i) = self.windows.iter().position(|w| w.client == client) {
            let w = self.windows.remove(i);
            self.windows.push(w);
        }
    }
    pub fn rects(&self, clients: &[ClientId], area: LRect) -> Vec<(ClientId, LRect)> {
        self.windows
            .iter()
            .filter(|w| clients.contains(&w.client) && !w.minimized)
            .map(|w| {
                let r = if w.maximized { area } else if let Some(zone)=w.snap {zone.bounds(area)} else { fit(w.rect, area) };
                (w.client, r)
            })
            .collect()
    }
}
/// Keep every title bar reachable after an output/pane resize.
pub fn fit(mut r: LRect, area: LRect) -> LRect {
    r.w = r.w.min(area.w).max(1.0);
    r.h = r.h.min(area.h).max(1.0);
    // Allow the body offscreen/behind a dock, keeping a title-bar grab reachable.
    let reachable_w = r.w.min(80.0);
    let reachable_h = r.h.min(28.0);
    r.x =
        r.x.clamp(area.x - r.w + reachable_w, area.x + area.w - reachable_w);
    r.y = r.y.clamp(area.y, area.y + area.h - reachable_h);
    r
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maximize_and_minimize_keep_restore_rect_and_stack() {
        let area = LRect::new(0.0, 30.0, 1200.0, 800.0);
        let mut d = DesktopWindows::default();
        d.enabled = true;
        d.ensure(1, area);
        d.ensure(2, area);
        let original = d.get(1).unwrap().rect;
        d.get_mut(1).unwrap().maximized = true;
        assert_eq!(d.rects(&[1, 2], area)[0].1, area);
        d.get_mut(1).unwrap().minimized = true;
        assert_eq!(d.rects(&[1, 2], area).len(), 1);
        d.get_mut(1).unwrap().minimized = false;
        d.get_mut(1).unwrap().maximized = false;
        d.raise(1);
        assert_eq!(d.rects(&[1, 2], area).last(), Some(&(1, original)));
    }
    #[test]
    fn shrinking_the_workarea_keeps_windows_reachable() {
        assert_eq!(
            fit(
                LRect::new(900.0, 700.0, 800.0, 500.0),
                LRect::new(10.0, 40.0, 300.0, 200.0)
            ),
            LRect::new(230.0, 212.0, 300.0, 200.0)
        );
    }
    #[test]
    fn without_a_hint_a_window_opens_at_the_default_size() {
        // 72% x 76% of the desk, at most 1000 x 720, 42 and 32 points in.
        let area = LRect::new(0.0, 30.0, 1200.0, 800.0);
        let mut d = DesktopWindows::default();
        d.ensure(1, area);
        d.ensure_sized(2, area, None, 0.0);
        assert_eq!(d.get(1).unwrap().rect, LRect::new(42.0, 62.0, 864.0, 608.0));
        // The second cascades 30 points, exactly as before.
        assert_eq!(d.get(2).unwrap().rect, LRect::new(72.0, 92.0, 864.0, 608.0));
        let big = LRect::new(0.0, 30.0, 2560.0, 1400.0);
        d.ensure(3, big);
        assert_eq!(d.get(3).unwrap().rect, LRect::new(102.0, 122.0, 1000.0, 720.0));
    }

    #[test]
    fn a_hint_smaller_than_the_desk_opens_at_its_size() {
        // PDF Tools asks for 1536 x 1024: larger than the default's cap.
        let area = LRect::new(0.0, 30.0, 2560.0, 1400.0);
        let mut d = DesktopWindows::default();
        d.ensure_sized(1, area, Some((1536.0, 1024.0)), 0.0);
        assert_eq!(d.get(1).unwrap().rect, LRect::new(42.0, 62.0, 1536.0, 1024.0));
        // Placed as every window is: a later one cascades.
        d.ensure_sized(2, area, Some((1536.0, 1024.0)), 0.0);
        assert_eq!(d.get(2).unwrap().rect, LRect::new(72.0, 92.0, 1536.0, 1024.0));
    }

    #[test]
    fn a_hint_larger_than_the_desk_is_clamped_to_it_minus_the_margins() {
        // The hidden shell's desk on a 1400 x 900 screen.
        let area = LRect::new(12.0, 44.0, 1375.0, 793.0);
        let mut d = DesktopWindows::default();
        d.ensure_sized(1, area, Some((1536.0, 1024.0)), 0.0);
        let r = d.get(1).unwrap().rect;
        assert_eq!(r, LRect::new(54.0, 76.0, 1291.0, 729.0));
        // The margins hold on the right and the bottom too.
        assert_eq!((area.x + area.w) - (r.x + r.w), MARGIN_X);
        assert_eq!((area.y + area.h) - (r.y + r.h), MARGIN_Y);
        // Cascaded, it moves as every window does and still ends inside the desk.
        d.ensure_sized(2, area, Some((1536.0, 1024.0)), 0.0);
        let r2 = d.get(2).unwrap().rect;
        assert_eq!(r2, LRect::new(84.0, 106.0, 1291.0, 729.0));
        assert!(r2.x + r2.w <= area.x + area.w && r2.y + r2.h <= area.y + area.h);
        // Only the side that is too large is clamped.
        d.ensure_sized(3, area, Some((1536.0, 500.0)), 0.0);
        assert_eq!((d.get(3).unwrap().rect.w, d.get(3).unwrap().rect.h), (1291.0, 500.0));
    }

    #[test]
    fn a_hint_ends_above_a_dock_that_floats_over_the_desk() {
        // The hidden shell's desk: OctoSense's dock covers its bottom 78
        // points, from y = 811 down.
        let area = LRect::new(10.0, 42.0, 1380.0, 847.0);
        let dock_top = area.y + area.h - 78.0;
        let mut d = DesktopWindows::default();
        d.ensure_sized(1, area, Some((1536.0, 1024.0)), 78.0);
        // The margins hold above the dock as they do above the desk's edge.
        let r = d.get(1).unwrap().rect;
        assert_eq!(r, LRect::new(52.0, 74.0, 1296.0, 705.0));
        assert_eq!(dock_top - (r.y + r.h), MARGIN_Y);
        // Cascaded windows move up rather than down behind the dock.
        for client in 2..=7 {
            d.ensure_sized(client, area, Some((1536.0, 1024.0)), 78.0);
            let r = d.get(client).unwrap().rect;
            assert!(r.y + r.h <= dock_top, "window {client} ends at {} below the dock's top {dock_top}", r.y + r.h);
        }
        assert_eq!(d.get(3).unwrap().rect, LRect::new(94.0, 106.0, 1296.0, 705.0));
        // A hint the room above the dock holds is not shrunk.
        d.ensure_sized(8, area, Some((900.0, 600.0)), 78.0);
        assert_eq!((d.get(8).unwrap().rect.w, d.get(8).unwrap().rect.h), (900.0, 600.0));
        // A dock that reserves its strip (the work area already stops above
        // it) or no dock at all: 0, the size as before.
        assert_eq!(preferred_size(1536.0, 1024.0, area, 0.0), (1296.0, 783.0));
        // A default-size window keeps the whole desk, whatever the dock.
        let mut plain = DesktopWindows::default();
        plain.ensure(1, area);
        let mut docked = DesktopWindows::default();
        docked.ensure_sized(1, area, None, 78.0);
        assert_eq!(docked.get(1).unwrap().rect, plain.get(1).unwrap().rect);
    }

    #[test]
    fn a_hint_never_opens_a_window_below_the_minimum() {
        let area = LRect::new(0.0, 0.0, 150.0, 100.0);
        let mut d = DesktopWindows::default();
        d.ensure_sized(1, area, Some((1536.0, 1024.0)), 0.0);
        assert_eq!(d.get(1).unwrap().rect, LRect::new(42.0, 32.0, MIN_W, MIN_H));
        assert_eq!(preferred_size(320.0, 240.0, LRect::new(0.0, 0.0, 10.0, 10.0), 0.0), (MIN_W, MIN_H));
    }

    #[test]
    fn snapping_keeps_restore_size_and_tracks_a_resized_workarea() {
        let area=LRect::new(10.,42.,1380.,794.);
        let mut d=DesktopWindows::default();
        d.ensure(1,area);
        let restore=d.get(1).unwrap().rect;
        d.get_mut(1).unwrap().snap=Some(crate::snap::Zone::BottomRight);
        assert_eq!(d.rects(&[1],area)[0].1,LRect::new(700.,439.,690.,397.));
        let small=LRect::new(10.,42.,800.,600.);
        assert_eq!(d.rects(&[1],small)[0].1,LRect::new(410.,342.,400.,300.));
        assert_eq!(d.get(1).unwrap().rect,restore);
        d.get_mut(1).unwrap().snap=None;
        assert_eq!(d.rects(&[1],area)[0].1,restore);
    }
}
