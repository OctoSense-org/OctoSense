//! Compatibility with the pinned Makepad GL backend, which stores 2D render
//! targets with top-left rows. Its older GaussStack consumer still requests a
//! Y flip on Android; sampling those targets directly keeps pixels and hits
//! aligned. Remove this override when the upstream compositor is corrected.
use makepad_widgets::{window::DrawGaussScene, *};

script_mod! {
    use mod.prelude.widgets_internal.*

    set_type_default() do #(DrawGaussScene::script_shader(vm)) {
        ..mod.draw.DrawQuad
        scene_texture: texture_2d(float)
        // Retain the uniform expected by GaussStack, but use the backend's
        // already normalized texture orientation.
        source_y_flip: uniform(0.0)
        source_offset: uniform(vec2(0.0, 0.0))
        source_scale: uniform(vec2(1.0, 1.0))
        pixel: fn() {
            let uv = self.source_offset + self.pos * self.source_scale
            return self.scene_texture.sample_as_bgra(clamp(uv, vec2(0.0, 0.0), vec2(1.0, 1.0)))
        }
    }
}
