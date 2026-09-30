//! Tick credit from Makepad WM: retain one missed timer beat while a child
//! works, then send it on acknowledgement. Older children get a 50 ms retry.
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
