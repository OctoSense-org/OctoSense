//! The thinking octopus: OctoSense's mascot drawn by a shader while the
//! hosted AppCard's kernel is on a turn — its head breathing, its eight
//! arms waving. The live island shows it in place of the AppCard glyph
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
        // One arm: a wavy stroke hanging from the head's rim at `root`
        // (x in the quad's -1..1 frame), thinning to its tip. Returns the
        // signed distance to it in quad units.
        arm: fn(q: vec2, root: float, seed: float) -> float {
            let t = clamp((q.y - 0.05) / 0.9, 0.0, 1.0)
            // The wave grows along the arm and drifts with the phase; each
            // arm runs at its own offset so they never move in step.
            let sway = sin(q.y * 4.5 - self.phase * 4.2 + seed) * 0.16 * t
                + sin(q.y * 9.0 + self.phase * 2.7 + seed * 1.7) * 0.05 * t
            let x = root * (1.0 + 0.25 * t) + sway
            let width = mix(0.085, 0.02, t)
            let d = abs(q.x - x) - width
            // Below the tip and above the root the arm is not there.
            return max(d, max(q.y - 0.97, 0.05 - q.y))
        }
        pixel: fn() {
            // q: -1..1 across the quad, y down; the head sits in the upper
            // third, the arms hang under it.
            let q = self.pos * 2.0 - 1.0
            let breath = 1.0 + 0.04 * sin(self.phase * 3.1)
            let head_c = vec2(0.0, -0.42)
            let head_r = vec2(0.52 * breath, 0.44 / breath)
            let hq = (q - head_c) / head_r
            let head = (length(hq) - 1.0) * min(head_r.x, head_r.y)
            let a0 = self.arm(q, -0.62, 0.0)
            let a1 = self.arm(q, -0.44, 1.3)
            let a2 = self.arm(q, -0.26, 2.6)
            let a3 = self.arm(q, -0.09, 3.9)
            let a4 = self.arm(q, 0.09, 5.2)
            let a5 = self.arm(q, 0.26, 0.7)
            let a6 = self.arm(q, 0.44, 2.0)
            let a7 = self.arm(q, 0.62, 3.3)
            let arms = min(min(min(a0, a1), min(a2, a3)), min(min(a4, a5), min(a6, a7)))
            // Smooth union of head and arms: the arms grow out of the rim.
            let k = 0.12
            let h = clamp(0.5 + 0.5 * (arms - head) / k, 0.0, 1.0)
            let body = mix(arms, head, h) - k * h * (1.0 - h)
            let aa = 1.5 / max(self.rect_size.x, 1.0) * 2.0
            let fill = 1.0 - smoothstep(-aa, aa, body)
            // Eyes: two dark dots that blink every few seconds.
            let blink = smoothstep(0.92, 0.97, sin(self.phase * 1.3 + 1.0))
            let eye_r = vec2(0.075, 0.075 * (1.0 - 0.9 * blink))
            let e1 = length((q - vec2(-0.18, -0.4)) / eye_r) - 1.0
            let e2 = length((q - vec2(0.18, -0.4)) / eye_r) - 1.0
            let eyes = 1.0 - smoothstep(-aa * 4.0, aa * 4.0, min(e1, e2) * eye_r.x)
            let rgb = self.color.rgb * (1.0 - 0.85 * eyes)
            let a = fill * self.alpha
            return vec4(rgb * a, a)
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
        let w = slot.size.x.min(slot.size.y);
        let h = w * 1.25;
        self.draw_abs(cx, Rect { pos: dvec2(slot.pos.x + (slot.size.x - w) * 0.5, slot.pos.y + (slot.size.y - h) * 0.5), size: dvec2(w, h) });
    }
}
