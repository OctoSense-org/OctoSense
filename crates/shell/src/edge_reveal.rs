//! A strip that hides past a screen edge and comes back when the pointer
//! rests on that edge, as macOS's Dock, menu bar and title bars do beside a
//! full-screen app: the dock at the bottom (`desktop::DesktopShelf`), the
//! top bar and a full-screen window's title bar at the top (`App`).
//!
//! The owner decides from the pointer whether it wants the strip out
//! ([`EdgeReveal::want`]) and feeds every event to
//! [`EdgeReveal::handle_event`], which moves a 0..1 value (0 past the edge,
//! 1 out) that the drawing reads.
use makepad_widgets::*;

/// The pointer this close to an edge calls a hidden strip out.
pub const EDGE: f64 = 2.0;
/// How long the pointer rests on the edge before the strip comes out, so a
/// pass along it does not bring it out.
pub const SHOW_DELAY: f64 = 0.15;
/// How long after the pointer leaves the strip it goes back.
pub const HIDE_DELAY: f64 = 0.45;
/// How long the strip takes to slide out or back.
pub const SLIDE: f64 = 0.22;

#[derive(Default)]
pub struct EdgeReveal {
    /// Where the strip is going: out, or back past the edge.
    pub out: bool,
    /// Where the timer sends it when it fires; None when nothing waits.
    pending: Option<bool>,
    timer: Timer,
    frame: NextFrame,
    time: f64,
}

impl EdgeReveal {
    /// The pointer wants the strip `out` or not: start the delay toward
    /// that ([`SHOW_DELAY`], [`HIDE_DELAY`]), or cancel a change it no
    /// longer wants.
    pub fn want(&mut self, cx: &mut Cx, out: bool) {
        if out == self.out {
            if self.pending.take().is_some() {
                cx.stop_timer(self.timer);
            }
        } else if self.pending != Some(out) {
            cx.stop_timer(self.timer);
            self.pending = Some(out);
            self.timer = cx.start_timeout(if out { SHOW_DELAY } else { HIDE_DELAY });
        }
    }

    /// Slide out or back now, without the delay: a change of mode.
    pub fn set(&mut self, cx: &mut Cx, out: bool) {
        cx.stop_timer(self.timer);
        self.pending = None;
        self.slide(cx, out);
    }

    fn slide(&mut self, cx: &mut Cx, out: bool) {
        self.out = out;
        self.time = 0.0;
        self.frame = cx.new_next_frame();
    }

    /// Fire the delay and step the slide: `value` moves toward 1 (out) or 0
    /// at [`SLIDE`]. True when `value` changed, so the owner redraws.
    pub fn handle_event(&mut self, cx: &mut Cx, event: &Event, value: &mut f64) -> bool {
        if self.timer.is_event(event).is_some() {
            if let Some(out) = self.pending.take() {
                self.slide(cx, out);
            }
        }
        let Some(ne) = self.frame.is_event(event) else {
            return false;
        };
        let dt = if self.time == 0.0 { 1.0 / 60.0 } else { (ne.time - self.time).min(0.05) };
        self.time = ne.time;
        let target = if self.out { 1.0 } else { 0.0 };
        let step = dt / SLIDE;
        let before = *value;
        *value = if target > *value { (*value + step).min(target) } else { (*value - step).max(target) };
        if *value != target {
            self.frame = cx.new_next_frame();
        }
        *value != before
    }
}

/// `t` (0..1) eased in and out, as a strip moves along its slide.
pub fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slide_is_eased_and_clamped() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert_eq!(ease(0.5), 0.5);
        assert!(ease(0.1) < 0.1 && ease(0.9) > 0.9);
        assert_eq!(ease(-1.0), 0.0);
        assert_eq!(ease(2.0), 1.0);
    }
}
