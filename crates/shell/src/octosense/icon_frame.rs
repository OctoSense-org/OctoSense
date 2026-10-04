//! Outer app-icon geometry belongs to the selected shell style, never an app.
use makepad_widgets::{desktop_style::DesktopStyle, *};

/// Geometry on the framework icon catalog's 64-point canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct IconFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
}

impl IconFrame {
    pub fn for_style(style: DesktopStyle) -> Option<Self> {
        let (x, y, width, height, radius) = match style {
            DesktopStyle::Android => (1., 1., 62., 62., 31.),
            DesktopStyle::Macos => (3., 3., 58., 57., 13.),
            DesktopStyle::Ios => (1., 1., 62., 62., 15.),
            DesktopStyle::Windows => (5., 5., 54., 54., 12.),
            DesktopStyle::NextStep => (0., 0., 64., 64., 0.),
            DesktopStyle::Omarchy | DesktopStyle::Windows2000 => return None,
        };
        Some(Self { x, y, width, height, radius })
    }

    pub fn fit(self, canvas: Rect) -> (Rect, f32) {
        let scale = canvas.size.x.min(canvas.size.y) / 64.;
        let origin = canvas.pos + (canvas.size - dvec2(64., 64.) * scale) * 0.5;
        (Rect {
            pos: origin + dvec2(self.x, self.y) * scale,
            size: dvec2(self.width, self.height) * scale,
        }, (self.radius * scale) as f32)
    }

    fn svg(self) -> String {
        if self.radius * 2. == self.width && self.width == self.height {
            format!(r#"<circle cx="32" cy="32" r="{}" fill="url(#tile)"/>"#, self.radius)
        } else {
            format!(r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="url(#tile)"/>"#,
                self.x, self.y, self.width, self.height, self.radius)
        }
    }
}

// These shell-owned SVGs share an explicit tile element. Keep foreground
// identity and gradients intact; don't try to rewrite arbitrary store SVGs.
const SOURCE_TILE: &str = r#"<rect x="3" y="3" width="58" height="58" rx="15" fill="url(#tile)"/>"#;
pub(super) const SHELL_ART_MARKER: &str = "<!-- OctoSense platform icon frame -->";

pub(super) fn styled_svg(source: &str, style: DesktopStyle) -> String {
    debug_assert!(source.contains(SOURCE_TILE), "shell icon needs the shared tile element");
    let tile = IconFrame::for_style(style).map_or_else(String::new, IconFrame::svg);
    source.replacen(SOURCE_TILE, &format!("{SHELL_ART_MARKER}{tile}"), 1)
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.shader.*

    mod.draw.AppIconMask = {
        coverage: fn(p: vec2, size: vec2, radius: float) -> float {
            let h = size * 0.5
            let r = min(radius, min(h.x, h.y))
            let q = abs(p - h) - h + vec2(r)
            let d = min(max(q.x, q.y), 0.0) + length(max(q, vec2(0.0))) - r
            let aa = max(length(vec2(dFdx(d), dFdy(d))), 0.001)
            return clamp(0.5 - d / aa, 0.0, 1.0)
        }
    }

    set_type_default() do #(DrawIconImage::script_shader(vm)) {
        ..mod.draw.DrawQuad
        image: texture_2d(float)
        source_size: vec2(1.0, 1.0)
        radius: 0.0
        opacity: 1.0
        backing: #fffdf7
        premultiplied: 0.0
        mask_coverage: mod.draw.AppIconMask.coverage
        pixel: fn() {
            let coverage = self.mask_coverage(self.pos * self.rect_size, self.rect_size, self.radius)
            if coverage <= 0.0 { discard() }
            var color = vec4(0.0)
            // Contain non-square legacy artwork instead of distorting it.
            let fit = min(self.rect_size.x / self.source_size.x, self.rect_size.y / self.source_size.y)
            let uv = (self.pos - vec2(0.5)) * self.rect_size / (self.source_size * fit) + vec2(0.5)
            if uv.x >= 0.0 && uv.y >= 0.0 && uv.x <= 1.0 && uv.y <= 1.0 {
                if self.premultiplied > 0.5 {
                    color = self.image.sample(uv)
                } else {
                    color = Pal.premul(self.image.sample_as_bgra(uv))
                }
            }
            let composed = color + Pal.premul(self.backing) * (1.0 - color.a)
            return composed * (coverage * self.opacity)
        }
    }

}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawIconImage {
    #[deref] pub draw_super: DrawQuad,
    #[live] pub source_size: Vec2f,
    #[live] pub radius: f32,
    #[live] pub opacity: f32,
    #[live] pub backing: Vec4f,
    #[live] pub premultiplied: f32,
}

/// Flatten SVG layers at full opacity once, then let the PNG shader mask
/// and fade the complete icon. Reused at smaller sizes and across styles.
/// This also avoids extending DrawSvg's packed instance data across its
/// trailing Rust alignment padding.
pub(super) struct SvgIconTexture {
    pass: DrawPass,
    list: DrawList2d,
    texture: Texture,
    draw: DrawSvg,
    pixels: usize,
}

impl SvgIconTexture {
    pub fn new(cx: &mut Cx, source: &str) -> Option<Self> {
        let mut draw = cx.with_vm(|vm| DrawSvg::script_new_with_default(vm));
        draw.load_from_str(source);
        let (width, height) = draw.svg_doc.as_ref()?.logical_size();
        if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. { return None; }
        draw.content_bounds = (0., 0., width, height);
        let pass = DrawPass::new_with_name(cx, "app_icon_svg");
        let texture = Texture::new_with_format(cx, TextureFormat::RenderBGRAu8 {
            size: TextureSize::Auto, initial: true,
        });
        pass.set_color_texture(cx, &texture, DrawPassClearColor::ClearWith(vec4(0., 0., 0., 0.)));
        Some(Self { pass, list: DrawList2d::new(cx), texture, draw, pixels: 0 })
    }

    pub fn texture(&mut self, cx: &mut Cx2d, size: DVec2) -> &Texture {
        let pixels = raster_size(size.x.min(size.y) * cx.current_dpi_factor());
        cx.make_child_pass(&self.pass);
        if pixels > self.pixels {
            self.pixels = pixels;
            let size = dvec2(pixels as f64, pixels as f64);
            self.pass.set_size(cx, size);
            cx.begin_pass(&self.pass, Some(1.));
            self.list.begin_always(cx);
            cx.begin_root_turtle(size, Layout::flow_overlay());
            self.draw.draw_abs(cx, Rect { pos: dvec2(0., 0.), size });
            cx.end_pass_sized_turtle();
            self.list.end(cx);
            cx.end_pass(&self.pass);
        }
        &self.texture
    }
}

fn raster_size(device_size: f64) -> usize {
    // Bounded cache: avoid per-frame work during a drag or style crossfade,
    // while supporting launcher badges through large HiDPI dock icons.
    (device_size.ceil().clamp(64., 512.) as usize).next_power_of_two()
}

/// Used by both bundle formats on every draw, so cached artwork follows
/// style changes without reloading or rerendering cached SVG artwork.
pub(super) fn placement(style: DesktopStyle, canvas: Rect) -> (Rect, f32, Vec4f) {
    match IconFrame::for_style(style) {
        Some(frame) => {
            let (rect, radius) = frame.fit(canvas);
            (rect, radius, vec4(1., 253. / 255., 247. / 255., 1.))
        }
        None => (canvas, 0., vec4(0., 0., 0., 0.)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_match_the_platform_catalog_and_keep_freeform_themes() {
        let canvas = Rect { pos: dvec2(10., 20.), size: dvec2(64., 64.) };
        let (circle, radius, backing) = placement(DesktopStyle::Android, canvas);
        assert_eq!(circle.pos, dvec2(11., 21.));
        assert_eq!(circle.size, dvec2(62., 62.));
        assert_eq!(radius, 31.);
        assert_eq!(backing.w, 1.);
        for (style, size, radius) in [
            (DesktopStyle::Macos, dvec2(58., 57.), 13.),
            (DesktopStyle::Ios, dvec2(62., 62.), 15.),
            (DesktopStyle::Windows, dvec2(54., 54.), 12.),
            (DesktopStyle::NextStep, dvec2(64., 64.), 0.),
        ] {
            let (rect, actual_radius, backing) = placement(style, canvas);
            assert_eq!(rect.size, size);
            assert_eq!(actual_radius, radius);
            assert_eq!(backing.w, 1.);
        }
        for style in [DesktopStyle::Omarchy, DesktopStyle::Windows2000] {
            assert_eq!(placement(style, canvas), (canvas, 0., vec4(0., 0., 0., 0.)));
        }
    }

    #[test]
    fn non_square_slots_center_the_frame_without_stretching() {
        let (rect, radius, _) = placement(DesktopStyle::Android,
            Rect { pos: dvec2(0., 0.), size: dvec2(128., 64.) });
        assert_eq!(rect.pos, dvec2(33., 1.));
        assert_eq!(rect.size, dvec2(62., 62.));
        assert_eq!(radius, 31.);
    }

    #[test]
    fn svg_raster_buckets_are_bounded_for_badges_and_hidpi_icons() {
        for (size, expected) in [(16., 64), (60., 64), (120., 128), (180., 256), (360., 512), (2048., 512)] {
            assert_eq!(raster_size(size), expected);
        }
    }

    #[test]
    fn icon_shaders_register_with_the_real_widget_vm() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut image = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            vm.bx.captured_errors = Some(Vec::new());
            script_mod(vm);
            let image = DrawIconImage::script_new_with_default(vm);
            assert_eq!(image.opacity, 1.);
            assert!(image.draw_vars.draw_shader_id.is_some(), "PNG mask shader must compile");
            assert!(vm.take_errors().is_empty());
            image
        });
        // Read the actual packed shader inputs, not just the Rust fields:
        // extending DrawSvg used to upload its tail padding as the radius.
        // Both bundle formats now go through this one instance layout.
        for (radius, opacity, premultiplied) in [(31., 1., 0.), (13., 0.5, 1.), (15., 0., 1.), (0., 1., 0.)] {
            image.radius = radius;
            image.opacity = opacity;
            image.premultiplied = premultiplied;
            for (id, expected) in [(id!(radius), radius), (id!(opacity), opacity), (id!(premultiplied), premultiplied)] {
                let mut uploaded = [f32::NAN];
                image.draw_vars.get_instance(&mut cx, id, &mut uploaded);
                assert_eq!(uploaded, [expected], "{id:?} shader input must follow a style/fade change");
            }
        }
    }
}
