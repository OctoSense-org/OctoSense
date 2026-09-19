//! The bottom sheet's detents: it rests at peek, half or full height, a
//! drag moves it with the finger between the lowest and the highest, and a
//! release snaps it to a detent — the nearest, or for a flick the next one
//! the way the finger was going. Heights only; the view draws and eases.

/// The handle, the title, a line under it and the primary button.
const PEEK_HEIGHT: f64 = 132.0;
/// Of the viewport.
const HALF_FRACTION: f64 = 0.45;
/// What full height leaves of the viewport above it: the search bar stays.
const FULL_TOP_GAP: f64 = 96.0;
/// Pixels a second: faster than this at release is a flick.
const FLICK_VELOCITY: f64 = 600.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Detent {
    #[default]
    Peek,
    Half,
    Full,
}

#[derive(Clone, Debug, Default)]
pub struct Sheet {
    detent: Detent,
    /// While a finger is on it: the height the drag started from, and the
    /// height now.
    drag: Option<(f64, f64)>,
}

impl Sheet {
    /// The three resting heights in a viewport this tall, rising.
    pub fn heights(viewport_h: f64) -> [f64; 3] {
        // In a short viewport the three close up rather than cross.
        let half = viewport_h * HALF_FRACTION;
        [
            PEEK_HEIGHT.min(half),
            half,
            (viewport_h - FULL_TOP_GAP).max(half),
        ]
    }

    pub fn detent(&self) -> Detent {
        self.detent
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The height to show now: under the finger while dragging, else the
    /// detent's.
    pub fn height(&self, viewport_h: f64) -> f64 {
        match self.drag {
            Some((_, now)) => now,
            None => Self::heights(viewport_h)[self.detent as usize],
        }
    }

    pub fn set(&mut self, detent: Detent) {
        self.detent = detent;
        self.drag = None;
    }

    pub fn drag_start(&mut self, viewport_h: f64) {
        let from = self.height(viewport_h);
        self.drag = Some((from, from));
    }

    /// The finger is `dy` pixels below where the drag started (negative:
    /// above).
    pub fn drag_by(&mut self, dy: f64, viewport_h: f64) {
        let [lowest, _, highest] = Self::heights(viewport_h);
        if let Some((from, now)) = self.drag.as_mut() {
            // The sheet grows as the finger goes up the screen.
            *now = (*from - dy).clamp(lowest, highest);
        }
    }

    /// The finger lifted, moving at `velocity` pixels a second downwards
    /// (negative: upwards). Returns where the sheet comes to rest.
    pub fn release(&mut self, velocity: f64, viewport_h: f64) -> Detent {
        const DETENTS: [Detent; 3] = [Detent::Peek, Detent::Half, Detent::Full];
        let Some((_, at)) = self.drag.take() else {
            return self.detent;
        };
        let heights = Self::heights(viewport_h);
        let index = if velocity <= -FLICK_VELOCITY {
            // Upwards: the first detent above the finger, else the top.
            heights.iter().position(|h| *h > at + 0.5).unwrap_or(2)
        } else if velocity >= FLICK_VELOCITY {
            heights.iter().rposition(|h| *h < at - 0.5).unwrap_or(0)
        } else {
            let distance = |i: &usize| (heights[*i] - at).abs();
            (0..3)
                .min_by(|a, b| distance(a).total_cmp(&distance(b)))
                .unwrap_or(0)
        };
        self.detent = DETENTS[index];
        self.detent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: f64 = 800.0;

    #[test]
    fn three_heights_rising_in_any_viewport() {
        assert_eq!(Sheet::heights(VIEWPORT), [132.0, 360.0, 704.0]);
        // A wide home tile or a split view: still three heights in order,
        // none taller than the room.
        for viewport in [90.0, 200.0, 320.0] {
            let [peek, half, full] = Sheet::heights(viewport);
            assert!(
                peek <= half && half <= full && full <= viewport && peek > 0.0,
                "{viewport}"
            );
        }
    }

    #[test]
    fn at_rest_it_is_as_tall_as_its_detent() {
        let mut sheet = Sheet::default();
        assert_eq!(sheet.detent(), Detent::Peek);
        assert_eq!(sheet.height(VIEWPORT), 132.0);
        sheet.set(Detent::Half);
        assert_eq!(sheet.height(VIEWPORT), 360.0);
        sheet.set(Detent::Full);
        assert_eq!(sheet.height(VIEWPORT), 704.0);
    }

    #[test]
    fn a_drag_follows_the_finger_between_the_lowest_and_the_highest() {
        let mut sheet = Sheet::default();
        sheet.drag_start(VIEWPORT);
        assert!(sheet.is_dragging());
        sheet.drag_by(-100.0, VIEWPORT);
        assert_eq!(sheet.height(VIEWPORT), 232.0);
        sheet.drag_by(-5_000.0, VIEWPORT);
        assert_eq!(sheet.height(VIEWPORT), 704.0);
        sheet.drag_by(5_000.0, VIEWPORT);
        assert_eq!(sheet.height(VIEWPORT), 132.0);
        // A move with no drag begun is nothing.
        let mut idle = Sheet::default();
        idle.drag_by(-100.0, VIEWPORT);
        assert_eq!(idle.height(VIEWPORT), 132.0);
    }

    #[test]
    fn a_slow_release_snaps_to_the_nearest() {
        let mut sheet = Sheet::default();
        sheet.drag_start(VIEWPORT);
        sheet.drag_by(-60.0, VIEWPORT);
        assert_eq!(sheet.release(0.0, VIEWPORT), Detent::Peek);
        assert!(!sheet.is_dragging());
        sheet.drag_start(VIEWPORT);
        sheet.drag_by(-180.0, VIEWPORT);
        assert_eq!(sheet.release(-50.0, VIEWPORT), Detent::Half);
        assert_eq!(sheet.height(VIEWPORT), 360.0);
        sheet.drag_start(VIEWPORT);
        sheet.drag_by(-300.0, VIEWPORT);
        assert_eq!(sheet.release(50.0, VIEWPORT), Detent::Full);
    }

    #[test]
    fn a_flick_goes_to_the_next_detent_its_way_and_never_past_it() {
        // Barely moved, but fast upwards: half, not back to peek.
        let mut sheet = Sheet::default();
        sheet.drag_start(VIEWPORT);
        sheet.drag_by(-20.0, VIEWPORT);
        assert_eq!(sheet.release(-2_000.0, VIEWPORT), Detent::Half);
        // However fast, one detent at a time from where the finger is.
        sheet.drag_start(VIEWPORT);
        sheet.drag_by(-10.0, VIEWPORT);
        assert_eq!(sheet.release(-9_000.0, VIEWPORT), Detent::Full);
        // Downwards from just under full: half.
        sheet.drag_start(VIEWPORT);
        sheet.drag_by(30.0, VIEWPORT);
        assert_eq!(sheet.release(2_000.0, VIEWPORT), Detent::Half);
        // Downwards from peek there is nowhere lower.
        sheet.set(Detent::Peek);
        sheet.drag_start(VIEWPORT);
        assert_eq!(sheet.release(2_000.0, VIEWPORT), Detent::Peek);
    }

    #[test]
    fn a_release_with_no_drag_keeps_the_detent() {
        let mut sheet = Sheet::default();
        sheet.set(Detent::Half);
        assert_eq!(sheet.release(-5_000.0, VIEWPORT), Detent::Half);
    }
}
