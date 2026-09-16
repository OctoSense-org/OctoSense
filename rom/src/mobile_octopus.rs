//! The thinking octopus: OctoSense's mascot drawn by a shader while the
//! hosted AppCard's kernel is on a turn — a round coral head that breathes,
//! eyes that blink and look about, six arms curling under it. The live island shows it in place of the AppCard glyph
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
        // One arm: a curling stroke hanging from the head's rim at `root`
        // (x in the quad's -1..1 frame), curling outward toward its rounded
        // tip. Returns the signed distance to it.
        arm: fn(q: vec2, root: float, seed: float) -> float {
            let t = clamp((q.y - 0.0) / 0.78, 0.0, 1.0)
            let side = sign(root)
            // The curl grows toward the tip; the whole arm sways slowly.
            let curl = side * (0.28 + 0.06 * sin(self.phase * 2.2 + seed)) * pow(t, 2.2)
            let sway = sin(self.phase * 3.0 + seed) * 0.05 * t
            let x = root * (1.0 + 0.1 * t) + curl + sway
            let width = mix(0.13, 0.065, t)
            let d = abs(q.x - x) - width
            let stroke = max(d, max(q.y - 0.78, 0.0 - q.y))
            let tip = length(q - vec2(root * 1.1 + side * (0.28 + 0.06 * sin(self.phase * 2.2 + seed)) + sin(self.phase * 3.0 + seed) * 0.05, 0.78)) - 0.07
            return min(stroke, tip)
        }
        pixel: fn() {
            // q: -1..1 across the quad, y down; the head is the upper half,
            // the arms curl under it.
            let q = self.pos * 2.0 - 1.0
            let breath = 1.0 + 0.03 * sin(self.phase * 3.1)
            let hq = q - vec2(0.0, -0.36)
            let dome = length(hq / vec2(0.62 * breath, 0.58 / breath)) - 1.0
            let belly = length((q - vec2(0.0, -0.12)) / vec2(0.66 * breath, 0.32)) - 1.0
            let head = min(dome, belly) * 0.5
            let a0 = self.arm(q, -0.58, 0.0)
            let a1 = self.arm(q, -0.34, 1.9)
            let a2 = self.arm(q, -0.11, 3.7)
            let a3 = self.arm(q, 0.11, 0.9)
            let a4 = self.arm(q, 0.34, 2.8)
            let a5 = self.arm(q, 0.58, 4.6)
            let arms = min(min(min(a0, a1), min(a2, a3)), min(a4, a5))
            let k = 0.16
            let h = clamp(0.5 + 0.5 * (arms - head) / k, 0.0, 1.0)
            let body = mix(arms, head, h) - k * h * (1.0 - h)
            let aa = 3.0 / max(self.rect_size.x, 1.0)
            let fill = 1.0 - smoothstep(-aa, aa, body)
            // Eyes: white with a dark pupil that looks around and a glint;
            // a blink every few seconds.
            let blink = smoothstep(0.93, 0.98, sin(self.phase * 1.4 + 1.0))
            let look = vec2(sin(self.phase * 0.9), cos(self.phase * 0.7)) * 0.035
            let er = vec2(0.17, 0.17 * (1.0 - 0.92 * blink))
            let e1 = length((q - vec2(-0.24, -0.4)) / er) - 1.0
            let e2 = length((q - vec2(0.24, -0.4)) / er) - 1.0
            let white = 1.0 - smoothstep(-aa * 5.0, aa * 5.0, min(e1, e2) * er.x)
            let pr = vec2(0.085, 0.085 * (1.0 - 0.92 * blink))
            let p1 = length((q - vec2(-0.24, -0.4) - look) / pr) - 1.0
            let p2 = length((q - vec2(0.24, -0.4) - look) / pr) - 1.0
            let pupil = 1.0 - smoothstep(-aa * 8.0, aa * 8.0, min(p1, p2) * pr.x)
            let g1 = length(q - vec2(-0.21, -0.44) - look) - 0.03
            let g2 = length(q - vec2(0.27, -0.44) - look) - 0.03
            let glint = (1.0 - smoothstep(-aa * 2.0, aa * 2.0, min(g1, g2))) * (1.0 - blink)
            // A small smile.
            let sm = abs(length(q - vec2(0.0, -0.3)) - 0.14) - 0.02
            let smile = (1.0 - smoothstep(-aa * 2.0, aa * 2.0, max(sm, -0.22 - q.y))) * (1.0 - blink * 0.5)
            let coral = vec3(0.98, 0.60, 0.56)
            let shade = mix(coral, coral * 0.82, smoothstep(-0.1, 0.6, q.y))
            let ink = vec3(0.14, 0.12, 0.2)
            let rgb = mix(shade, vec3(1.0), white)
            let rgb2 = mix(rgb, ink, max(pupil, smile))
            let rgb3 = mix(rgb2, vec3(1.0), glint)
            let a = fill * self.alpha
            return vec4(rgb3 * a, a)
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
        let w = slot.size.x.min(slot.size.y) * 1.3;
        let h = w * 1.2;
        self.draw_abs(cx, Rect { pos: dvec2(slot.pos.x + (slot.size.x - w) * 0.5, slot.pos.y + (slot.size.y - h) * 0.5), size: dvec2(w, h) });
    }
}
