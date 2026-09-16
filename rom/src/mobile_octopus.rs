//! The thinking octopus: the OctoSense mark (the web logo's line-art
//! octopus — an open-arc head, two curls, eight wavy legs) drawn by a
//! shader while the hosted AppCard's kernel is on a turn, its legs' wave
//! travelling and its head breathing. The live island shows it in place of the AppCard glyph
//! (mobile_island.rs) while a turn is in flight, and the island keeps the
//! frame loop running for it. Nothing here reads the pass clock: the phase
//! comes from the frame that draws it, so an idle screen stays idle.
use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*
    set_type_default() do #(DrawOctopus::script_shader(vm)) {
        ..mod.draw.DrawQuad
        // Seconds; wraps every 8 s so the float stays precise.
        phase: 0.0
        color: #ffffff
        alpha: 1.0
        // Signed distance to the OctoSense mark (cc-ppt/octopus-icon/
        // octopus.svg, 1024 viewBox): an open arc for the head, a curl at
        // each end of it, and eight wavy bumps between the curls for the
        // legs — line art, painted in `color` like the web logo's
        // currentColor. `u` is in the SVG's units; `wave` travels along the
        // legs, `bob` breathes the head.
        mark: fn(u: vec2, wave: float, bob: float) -> float {
            // Head: circle r 230 about (512, 460), open at the bottom between
            // its two ends at y 628.
            let hc = vec2(512.0, 460.0 + bob)
            let hd = abs(length(u - hc) - 230.0) - 12.0
            let below = step(hc.y + 150.0, u.y) * step(abs(u.x - 512.0), (u.y - hc.y) * 0.93)
            let head = mix(hd, 1000.0, below)
            // Curls: small rings hanging off the arc's ends.
            let c1 = abs(length(u - vec2(322.0, 646.0 + bob * 0.5)) - 22.0) - 12.0
            let c2 = abs(length(u - vec2(702.0, 646.0 + bob * 0.5)) - 22.0) - 12.0
            // Legs: eight bumps from x 300 to 700, 50 wide, 20 high.
            let lx = clamp(u.x, 300.0, 700.0)
            let ly = 670.0 + bob * 0.3 - 10.0 * cos((u.x - 300.0) * 0.12566 + wave)
            let legs = length(vec2(u.x - lx, u.y - ly)) - 10.0
            return min(min(head, min(c1, c2)), legs)
        }
        pixel: fn() {
            let q = self.pos * 2.0 - 1.0
            // The mark's box (x 282..742, y 230..700) fills the quad.
            let u = vec2(512.0, 470.0) + q * 262.0
            let wave = self.phase * 3.5
            let bob = 5.0 * sin(self.phase * 2.4)
            let d = self.mark(u, wave, bob)
            let aa = 262.0 * 2.0 / max(self.rect_size.x, 1.0)
            let fill = 1.0 - smoothstep(-aa, aa, d)
            let a = fill * self.alpha
            return vec4(self.color.rgb * a, a)
        }
    }
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawOctopus {
    #[deref] draw_super: DrawQuad,
    #[live] phase: f32,
    #[live] color: Vec4,
    #[live] alpha: f32,
}

impl DrawOctopus {
    /// Draw the octopus in `slot` at animation time `now` (seconds), in
    /// `color` at `alpha`. The slot is stretched a little taller than wide
    /// so the arms have room under the head.
    pub fn draw(&mut self, cx: &mut Cx2d, slot: Rect, now: f64, color: Vec4, alpha: f32) {
        self.phase = (now % 8.0) as f32;
        self.color = color;
        self.alpha = alpha;
        let w = slot.size.x.min(slot.size.y) * 1.25;
        let h = w;
        self.draw_abs(cx, Rect { pos: dvec2(slot.pos.x + (slot.size.x - w) * 0.5, slot.pos.y + (slot.size.y - h) * 0.5), size: dvec2(w, h) });
    }
}
