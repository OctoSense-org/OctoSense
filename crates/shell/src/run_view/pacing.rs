//! Tick credit from Makepad WM: retain one missed timer beat while a child
//! works, then send it on acknowledgement. Older children get a 50 ms retry.
/// A child's clock. It draws and runs its timers on the beats, so they stay at full
/// speed while anything happens. One with nothing new to show needs far fewer, and each
/// beat wakes every widget in the shell. Under a text cursor's blink, so that a terminal
/// at rest counts as quiet.
pub(super) const TICK_FULL: f64 = 0.008;
pub(super) const TICK_QUIET: f64 = 0.05;
const QUIET_AFTER: f64 = 0.25;

#[derive(Default)]
pub(super) struct TickRate {
    last_activity: f64,
    quiet: bool,
}

impl TickRate {
    /// Something happened. True when the clock must go back to full speed.
    pub fn stir(&mut self, now: f64) -> bool {
        self.last_activity = now;
        std::mem::take(&mut self.quiet)
    }

    /// On a beat. True when the clock may slow down now.
    pub fn settle(&mut self, now: f64) -> bool {
        if self.quiet || now - self.last_activity < QUIET_AFTER {
            return false;
        }
        self.quiet = true;
        true
    }
}

#[derive(Default)]
pub(super) struct TickPacer {
    outstanding: Option<f64>,
    deferred: bool,
}

impl TickPacer {
    pub fn beat(&mut self, now: f64) -> bool {
        self.deferred = true;
        if self.outstanding.is_none_or(|sent| now - sent >= 0.050) {
            self.send(now);
            true
        } else {
            false
        }
    }

    pub fn complete(&mut self, now: f64) -> bool {
        if self.outstanding.take().is_some() && self.deferred {
            self.send(now);
            true
        } else {
            false
        }
    }

    fn send(&mut self, now: f64) {
        self.outstanding = Some(now);
        self.deferred = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quiet_child_slows_once_and_anything_at_all_brings_it_back() {
        let mut rate = TickRate::default();
        assert!(!rate.stir(10.0), "already at full speed");
        assert!(!rate.settle(10.2), "not quiet long enough yet");
        assert!(rate.settle(10.25));
        assert!(!rate.settle(10.3), "it slows once");
        assert!(rate.stir(10.4), "back to full speed");
        assert!(!rate.stir(10.5), "and only once");
        assert!(!rate.settle(10.7));
        assert!(rate.settle(10.75));
    }

    #[test]
    fn slow_child_keeps_one_credit_and_resumes_without_an_extra_timer_delay() {
        let mut pace = TickPacer::default();
        assert!(pace.beat(0.0));
        for ms in 1..25 {
            assert!(!pace.beat(ms as f64 / 1000.0));
        }
        assert!(pace.complete(0.025));
        assert!(!pace.complete(0.026));
        assert!(!pace.complete(0.027));
        assert!(pace.beat(0.032));
    }

    #[test]
    fn legacy_child_gets_a_bounded_retry() {
        let mut pace = TickPacer::default();
        assert!(pace.beat(0.0));
        assert!(!pace.beat(0.049));
        assert!(pace.beat(0.051));
        assert!(!pace.beat(0.080));
        assert!(pace.beat(0.102));
    }

    #[test]
    fn fast_child_waits_for_the_next_beat_and_new_target_has_no_old_credit() {
        let mut pace = TickPacer::default();
        assert!(pace.beat(0.0));
        assert!(!pace.complete(0.001));
        assert!(pace.beat(0.008));
        assert!(!pace.beat(0.016));
        pace = TickPacer::default();
        assert!(!pace.complete(0.017));
        assert!(pace.beat(0.018));
    }
}
