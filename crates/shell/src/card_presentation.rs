//! A resident card's presentation, independent of its app/session lifetime.
//! The feed supplies only a source rectangle. The destination is the root
//! viewport; no feed item is resized and no model work drives this clock.
use makepad_widgets::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Phase { #[default] Hidden, Opening, Active, Closing }

#[derive(Default)]
pub struct Presentation {
    pub phase: Phase,
    pub source: Option<Rect>,
    progress: f64,
    velocity: f64,
}

impl Presentation {
    pub fn visible(&self) -> bool { self.phase != Phase::Hidden }
    pub fn moving(&self) -> bool { matches!(self.phase, Phase::Opening | Phase::Closing) }
    pub fn covers_background(&self) -> bool { self.phase == Phase::Active }
    pub fn activate(&mut self) { self.phase = Phase::Active; self.progress = 1.0; self.velocity = 0.0; }
    pub fn hide(&mut self) { *self = Self::default(); }
    pub fn open(&mut self, source: Option<Rect>, reduced: bool) {
        if self.phase == Phase::Closing { self.phase = Phase::Opening; }
        else {
            self.source = source;
            self.progress = 0.0;
            self.velocity = 0.0;
            self.phase = Phase::Opening;
        }
        if reduced || self.source.is_none() { self.activate(); }
    }
    pub fn dismiss(&mut self, source: Option<Rect>, reduced: bool) {
        self.source = source;
        if reduced || source.is_none() { self.hide(); }
        else { self.phase = Phase::Closing; }
    }
    /// Analytic critically damped spring. Retargeting preserves velocity and
    /// position, including Back during opening; a delayed frame cannot explode.
    pub fn step(&mut self, dt: f64, reduced: bool) {
        if !self.moving() { return; }
        let target = if self.phase == Phase::Opening { 1.0 } else { 0.0 };
        if reduced { self.progress = target; self.velocity = 0.0; }
        else {
            let dt = dt.clamp(0.0, 0.05);
            let omega = 34.0;
            let delta = self.progress - target;
            let carry = self.velocity + omega * delta;
            let decay = (-omega * dt).exp();
            self.progress = target + (delta + carry * dt) * decay;
            self.velocity = (self.velocity - omega * carry * dt) * decay;
            if self.progress < 0.0 || self.progress > 1.0 {
                self.progress = self.progress.clamp(0.0, 1.0); self.velocity = 0.0;
            }
        }
        if (self.progress - target).abs() < 0.003 && self.velocity.abs() < 0.12 {
            if target == 1.0 { self.activate(); } else { self.hide(); }
        }
    }
    pub fn rect(&self, destination: Rect) -> Rect {
        let Some(source) = self.source.filter(|_| self.moving()) else { return destination; };
        Rect { pos: source.pos + (destination.pos - source.pos) * self.progress,
            size: source.size + (destination.size - source.size) * self.progress }
    }
    pub fn radius(&self) -> f32 { if self.moving() { (20.0 * (1.0 - self.progress)) as f32 } else { 0.0 } }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Rect { Rect { pos: dvec2(20.0, 340.0), size: dvec2(313.0, 124.0) } }
    fn full() -> Rect { Rect { pos: dvec2(0.0, 24.0), size: dvec2(353.0, 820.0) } }
    #[test]
    fn summary_reaches_root_viewport_without_a_feed_height_cap() {
        let mut p = Presentation::default(); p.open(Some(source()), false);
        assert_eq!(p.rect(full()), source());
        for _ in 0..22 { p.step(1.0 / 60.0, false); }
        assert_eq!(p.phase, Phase::Active); assert_eq!(p.rect(full()), full());
        assert!(p.covers_background());
        let keyboard = Rect { size: dvec2(353.0, 360.0), ..full() };
        assert_eq!(p.rect(keyboard), keyboard);
    }
    #[test]
    fn back_during_opening_and_reversal_preserve_position_and_velocity() {
        let mut p = Presentation::default(); p.open(Some(source()), false); p.step(0.05, false);
        let r = p.rect(full()); let v = p.velocity;
        p.dismiss(Some(source()), false);
        assert_eq!(p.rect(full()), r); assert_eq!(p.velocity, v);
        assert!(p.visible()); assert!(!p.covers_background());
        p.step(0.02, false); let r = p.rect(full()); let v = p.velocity;
        p.open(Some(source()), false); assert_eq!(p.rect(full()), r); assert_eq!(p.velocity, v);
        for _ in 0..25 { p.step(1.0 / 60.0, false); }
        p.dismiss(Some(source()), false);
        for _ in 0..25 { p.step(1.0 / 60.0, false); }
        assert!(!p.visible());
    }
    #[test]
    fn reduced_motion_and_missing_return_source_never_zoom_toward_another_card() {
        let mut p = Presentation::default(); p.open(Some(source()), true);
        assert_eq!(p.phase, Phase::Active);
        p.dismiss(None, false); assert!(!p.visible());
        p.open(None, false); assert_eq!(p.rect(full()), full());
        p.dismiss(Some(source()), true); assert!(!p.visible());
    }
}
