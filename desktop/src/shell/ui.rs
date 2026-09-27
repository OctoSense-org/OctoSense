//! `shell/Ui/*.qml` — the widget kit every omarchy surface is built from,
//! ported to splash.
//!
//! The QML kit is a set of `BorderSurface`-based controls that all paint
//! themselves out of the same four state tokens (normal / hover-cursor /
//! selected / focus) at the alphas in `Commons/Style.qml`. Here that kit is
//! `ShellDraw`: one registered splash component carrying the shaders, the
//! two type faces and the icon sheet, with one method per QML component
//! (`button`, `toggle_switch`, `panel_slider`, `text_field`, `popup_card`,
//! `panel_section_header`, `panel_separator`, `panel_hero`,
//! `panel_action_button`, `cursor_surface`, `bar_widget`…). Every surface in
//! `shell/` draws through it, so they share one look exactly like the
//! original shares `Ui/`.
//!
//! Two things the FLAT material keeps from the source:
//!  * hard square corners (`Style.cornerRadius` is 0), and
//!  * flat fills plus a 1px border — no bevels, no glows, no gradients
//!    except the hyprland border gradient a theme may name.
//!
//! A desktop style's material (`MaterialTokens`, handed to the kit by
//! `set_material`) may replace a card with Liquid Glass at the material's
//! own corner radius; see `begin_surface`.

use makepad_widgets::gauss_view::{request_window_gauss, GaussBlurSnapshot, GAUSS_VIEW_LEVELS};
use makepad_widgets::*;

use super::{
    alpha, BarTokens, ControlTokens, CtrlState, FontTokens, MaterialTokens, MenuTokens,
    NotificationTokens, ShellTokens, SpacingTokens, SurfaceTokens,
};

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    // A flat fill. Premultiplied so a translucent card composites over the
    // wallpaper the way the QML `Rectangle{color: Util.alpha(...)}` does.
    set_type_default() do #(DrawShellFill::script_shader(vm)) {
        ..mod.draw.DrawQuad
        color: #ffffff
        pixel: fn() {
            return vec4(self.color.rgb * self.color.w, self.color.w)
        }
    }

    // `BorderSurface`: a fill plus a hard square ring measured straight off
    // the quad edges. The ring takes two stops and an angle so a theme's
    // hyprland `active-border` gradient (what `[popups] border` resolves to)
    // draws as a gradient, exactly like `BorderOverlay`'s shape path.
    set_type_default() do #(DrawShellChrome::script_shader(vm)) {
        ..mod.draw.DrawQuad
        color: #00000000
        border_color: #ffffff
        border_color_end: #ffffff
        border_angle: 0.0
        border_width: 1.0
        pixel: fn() {
            let p = self.pos * self.rect_size
            let d = min(min(p.x, p.y), min(self.rect_size.x - p.x, self.rect_size.y - p.y))
            let cov = clamp((self.border_width - d) * 3.0 + 0.5, 0.0, 1.0)
            let rad = self.border_angle * 0.017453292
            let dir = vec2(cos(rad), sin(rad))
            let half = self.rect_size * 0.5
            let extent = max(abs(dir.x) * half.x + abs(dir.y) * half.y, 0.001)
            let t = clamp(0.5 + dot(p - half, dir) / (2.0 * extent), 0.0, 1.0)
            let bc = mix(self.border_color, self.border_color_end, t)
            let c = mix(self.color, bc, cov * self.border_color.w)
            return vec4(c.rgb * c.w, c.w)
        }
    }

    // Liquid Glass: `AppleGlassRoundedView`'s material (widgets/src/
    // gauss_view.rs) on an immediate-mode quad. The pyramid textures and
    // the material-wide uniforms are bound by `ShellDraw::bind_snapshot`; the
    // per-surface values (tint, border, radius, shadow) are the Rust
    // struct's instance fields, set per draw. No press ripple: nothing
    // here animates, and nothing reads draw_pass.time.
    set_type_default() do #(DrawShellGlass::script_shader(vm)) {
        ..mod.draw.DrawQuad
        tint_color: #f8fbff0f
        border_color: #ffffff8c
        border_width: 1.0
        corner_radius: 6.0
        shadow_color: #00000070
        shadow_radius: 0.0
        shadow_offset_y: 0.0
        fallback_color: #334156
        specular_strength: 0.22
        noise_strength: 0.004
        opacity: 1.0

        scene_texture: texture_2d(float)
        mip0_texture: texture_2d(float)
        mip1_texture: texture_2d(float)
        mip2_texture: texture_2d(float)
        mip3_texture: texture_2d(float)
        mip4_texture: texture_2d(float)
        mip5_texture: texture_2d(float)
        has_gauss: uniform(0.0)
        source_size: uniform(vec2(1.0, 1.0))
        source_y_flip: uniform(0.0)
        blur_level: uniform(5.2)
        lensing_effect: uniform(0.94)
        lensing_strength: uniform(28.0)
        lensing_width: uniform(20.0)
        diffraction_strength: uniform(4.4)

        rect_size2: varying(vec2(0.0))
        rect_size3: varying(vec2(0.0))
        rect_pos2: varying(vec2(0.0))
        rect_shift: varying(vec2(0.0))
        sdf_rect_pos: varying(vec2(0.0))
        sdf_rect_size: varying(vec2(0.0))

        vertex: fn() {
            let shadow_offset = vec2(0.0, self.shadow_offset_y)
            let min_offset = min(shadow_offset, vec2(0.0, 0.0))
            self.rect_size2 = self.rect_size + 2.0 * vec2(self.shadow_radius)
            self.rect_size3 = self.rect_size2 + abs(shadow_offset)
            self.rect_pos2 = self.rect_pos - vec2(self.shadow_radius) + min_offset
            self.sdf_rect_size = self.rect_size2 - vec2(self.shadow_radius * 2.0 + self.border_width * 2.0)
            self.sdf_rect_pos = -min_offset + vec2(self.border_width + self.shadow_radius)
            self.rect_shift = -min_offset
            return self.clip_and_transform_vertex(self.rect_pos2, self.rect_size3)
        }

        bicubic_h: fn(uv: vec2, size: vec2) -> vec4 {
            let tc = uv * size - 0.5
            let f = fract(tc)
            let tc0 = floor(tc)
            let f2 = f * f
            let f3 = f2 * f
            let omf = 1.0 - f
            let w1 = (f3 * 3.0 - f2 * 6.0 + 4.0) / 6.0
            let g0 = omf * omf * omf / 6.0 + w1
            let h0 = clamp((tc0 - 0.5 + w1 / g0) / size, vec2(0.0, 0.0), vec2(1.0, 1.0))
            let h1 = clamp((tc0 + 1.5 + (f3 / 6.0) / (1.0 - g0)) / size, vec2(0.0, 0.0), vec2(1.0, 1.0))
            return vec4(h0.x, h0.y, h1.x, h1.y)
        }

        bicubic_g0: fn(uv: vec2, size: vec2) -> vec2 {
            let f = fract(uv * size - 0.5)
            let f2 = f * f
            let omf = 1.0 - f
            return omf * omf * omf / 6.0 + (f2 * f * 3.0 - f2 * 6.0 + 4.0) / 6.0
        }

        sample_level: fn(level: float, uv: vec2) -> vec4 {
            let source_uv = vec2(uv.x, mix(uv.y, 1.0 - uv.y, self.source_y_flip))
            let safe_uv = clamp(source_uv, vec2(0.0, 0.0), vec2(1.0, 1.0))
            if level < 0.5 {
                return self.scene_texture.sample_as_bgra(safe_uv)
            }
            if level < 1.5 {
                let size = max(self.mip0_texture.size(), vec2(1.0, 1.0))
                let h = self.bicubic_h(safe_uv, size)
                let g0 = self.bicubic_g0(safe_uv, size)
                let g1 = 1.0 - g0
                return self.mip0_texture.sample_as_bgra(vec2(h.x, h.y)) * (g0.x * g0.y)
                    + self.mip0_texture.sample_as_bgra(vec2(h.z, h.y)) * (g1.x * g0.y)
                    + self.mip0_texture.sample_as_bgra(vec2(h.x, h.w)) * (g0.x * g1.y)
                    + self.mip0_texture.sample_as_bgra(vec2(h.z, h.w)) * (g1.x * g1.y)
            }
            if level < 2.5 {
                let size = max(self.mip1_texture.size(), vec2(1.0, 1.0))
                let h = self.bicubic_h(safe_uv, size)
                let g0 = self.bicubic_g0(safe_uv, size)
                let g1 = 1.0 - g0
                return self.mip1_texture.sample_as_bgra(vec2(h.x, h.y)) * (g0.x * g0.y)
                    + self.mip1_texture.sample_as_bgra(vec2(h.z, h.y)) * (g1.x * g0.y)
                    + self.mip1_texture.sample_as_bgra(vec2(h.x, h.w)) * (g0.x * g1.y)
                    + self.mip1_texture.sample_as_bgra(vec2(h.z, h.w)) * (g1.x * g1.y)
            }
            if level < 3.5 {
                let size = max(self.mip2_texture.size(), vec2(1.0, 1.0))
                let h = self.bicubic_h(safe_uv, size)
                let g0 = self.bicubic_g0(safe_uv, size)
                let g1 = 1.0 - g0
                return self.mip2_texture.sample_as_bgra(vec2(h.x, h.y)) * (g0.x * g0.y)
                    + self.mip2_texture.sample_as_bgra(vec2(h.z, h.y)) * (g1.x * g0.y)
                    + self.mip2_texture.sample_as_bgra(vec2(h.x, h.w)) * (g0.x * g1.y)
                    + self.mip2_texture.sample_as_bgra(vec2(h.z, h.w)) * (g1.x * g1.y)
            }
            if level < 4.5 {
                let size = max(self.mip3_texture.size(), vec2(1.0, 1.0))
                let h = self.bicubic_h(safe_uv, size)
                let g0 = self.bicubic_g0(safe_uv, size)
                let g1 = 1.0 - g0
                return self.mip3_texture.sample_as_bgra(vec2(h.x, h.y)) * (g0.x * g0.y)
                    + self.mip3_texture.sample_as_bgra(vec2(h.z, h.y)) * (g1.x * g0.y)
                    + self.mip3_texture.sample_as_bgra(vec2(h.x, h.w)) * (g0.x * g1.y)
                    + self.mip3_texture.sample_as_bgra(vec2(h.z, h.w)) * (g1.x * g1.y)
            }
            if level < 5.5 {
                let size = max(self.mip4_texture.size(), vec2(1.0, 1.0))
                let h = self.bicubic_h(safe_uv, size)
                let g0 = self.bicubic_g0(safe_uv, size)
                let g1 = 1.0 - g0
                return self.mip4_texture.sample_as_bgra(vec2(h.x, h.y)) * (g0.x * g0.y)
                    + self.mip4_texture.sample_as_bgra(vec2(h.z, h.y)) * (g1.x * g0.y)
                    + self.mip4_texture.sample_as_bgra(vec2(h.x, h.w)) * (g0.x * g1.y)
                    + self.mip4_texture.sample_as_bgra(vec2(h.z, h.w)) * (g1.x * g1.y)
            }
            let size = max(self.mip5_texture.size(), vec2(1.0, 1.0))
            let h = self.bicubic_h(safe_uv, size)
            let g0 = self.bicubic_g0(safe_uv, size)
            let g1 = 1.0 - g0
            return self.mip5_texture.sample_as_bgra(vec2(h.x, h.y)) * (g0.x * g0.y)
                + self.mip5_texture.sample_as_bgra(vec2(h.z, h.y)) * (g1.x * g0.y)
                + self.mip5_texture.sample_as_bgra(vec2(h.x, h.w)) * (g0.x * g1.y)
                + self.mip5_texture.sample_as_bgra(vec2(h.z, h.w)) * (g1.x * g1.y)
        }

        sample_blur: fn(level: float, uv: vec2) -> vec4 {
            let safe_level = clamp(level, 0.0, 6.0)
            if safe_level >= 5.999 {
                return self.sample_level(6.0, uv)
            }
            let base_level = floor(safe_level)
            let t = safe_level - base_level
            let l1 = base_level
            let l2 = min(base_level + 1.0, 6.0)
            let blend = t * t * (3.0 - 2.0 * t)
            let c1 = self.sample_level(l1, uv)
            let c2 = self.sample_level(l2, uv)
            return c1.mix(c2, blend)
        }

        sample_gauss: fn(uv: vec2) -> vec4 {
            return self.sample_blur(self.blur_level, uv)
        }

        rounded_edge_normal: fn(shape: float) -> vec2 {
            let gradient = vec2(dFdx(shape), dFdy(shape))
            if length(gradient) > 0.00001 {
                return normalize(gradient)
            }
            return vec2(0.0, 1.0)
        }

        eff_lensing_width: fn() -> float {
            let cap = max(min(self.sdf_rect_size.x, self.sdf_rect_size.y) * 0.35, 1.0)
            return min(max(self.lensing_width, 1.0), cap)
        }

        eff_lensing_scale: fn() -> float {
            return self.eff_lensing_width() / max(self.lensing_width, 1.0)
        }

        rounded_edge_lens: fn(shape: float) -> float {
            let edge = clamp(1.0 - abs(shape) / self.eff_lensing_width(), 0.0, 1.0)
            return pow(edge, 1.45) * clamp(self.lensing_effect, 0.0, 1.0)
        }

        pixel: fn() {
            let sdf = Sdf2d.viewport(self.pos * self.rect_size3)
            sdf.box(
                self.sdf_rect_pos.x
                self.sdf_rect_pos.y
                self.sdf_rect_size.x
                self.sdf_rect_size.y
                max(1.0, self.corner_radius)
            )
            if self.shadow_radius > 0.0 && sdf.shape > -1.0 {
                let m = self.shadow_radius
                let o = vec2(0.0, self.shadow_offset_y) + self.rect_shift
                let v = GaussShadow.rounded_box_shadow(
                    vec2(m) + o
                    self.rect_size2 + o
                    self.pos * (self.rect_size3 + vec2(m))
                    self.shadow_radius * 0.5
                    self.corner_radius * 2.0
                )
                sdf.clear(vec4(self.shadow_color.rgb, self.shadow_color.w * v))
            }

            let screen_pos = self.rect_pos2 + self.pos * self.rect_size3
            let src = max(self.source_size, vec2(1.0, 1.0))
            let uv = screen_pos / src
            let lens = self.rounded_edge_lens(sdf.shape)
            let normal = self.rounded_edge_normal(sdf.shape)
            let base_offset = normal * (lens * self.lensing_strength * self.eff_lensing_scale()) / src
            let color_offset = normal * (lens * self.diffraction_strength) / src
            let uv_g = clamp(uv + base_offset, vec2(0.0, 0.0), vec2(1.0, 1.0))
            let uv_r = clamp(uv_g + color_offset, vec2(0.0, 0.0), vec2(1.0, 1.0))
            let uv_b = clamp(uv_g - color_offset, vec2(0.0, 0.0), vec2(1.0, 1.0))
            let sample_r = self.sample_gauss(uv_r)
            let sample_g = self.sample_gauss(uv_g)
            let sample_b = self.sample_gauss(uv_b)
            let refracted = vec4(sample_r.r, sample_g.g, sample_b.b, 1.0)
            let fallback = vec4(self.fallback_color.rgb, 1.0)
            let base = fallback.mix(refracted, self.has_gauss)

            let material = base.rgb.mix(self.tint_color.rgb, self.tint_color.w)
            let edge_uv = abs(self.pos * 2.0 - 1.0)
            let edge_gradient = clamp((edge_uv.x + edge_uv.y) * 0.5, 0.0, 1.0)
            let sparkle = lens * self.diffraction_strength * 0.004
            let highlight = self.specular_strength * (0.45 * edge_gradient + 0.55 * lens + 0.30 * (1.0 - self.pos.y))
            // Static de-banding grain, hashed from screen position only.
            let noise = (Math.random_2d(screen_pos) - 0.5) * self.noise_strength
            sdf.fill_keep(vec4(material + highlight + sparkle + noise, 1.0))
            if self.border_width > 0.0 {
                sdf.stroke(self.border_color, self.border_width)
            }
            // Premultiplied, so one multiply fades shadow, fill and ring
            // together (a window's popin, a style tween's crossfade).
            return sdf.result * self.opacity
        }
    }

    // ------------------------------------------------------------------
    // The token object. THE contract: everything below reads
    // `mod.wm_theme.shell`, and nothing at runtime reads an omarchy file.
    // Registering it as a type default means every `#[live] tokens:
    // ShellTokens` field in every surface is themed without repeating this.
    //
    // Each section type is registered first: a `#[live]` field of an
    // unregistered struct type has no default object, and merging into it
    // fails with "field ... not found in type-check and has no default".
    // ------------------------------------------------------------------
    set_type_default() do #(BarTokens::script_component(vm)) {}
    set_type_default() do #(SurfaceTokens::script_component(vm)) {}
    set_type_default() do #(NotificationTokens::script_component(vm)) {}
    set_type_default() do #(MenuTokens::script_component(vm)) {}
    set_type_default() do #(ControlTokens::script_component(vm)) {}
    set_type_default() do #(SpacingTokens::script_component(vm)) {}
    set_type_default() do #(FontTokens::script_component(vm)) {}
    set_type_default() do #(ShellIcons::script_component(vm)) {}

    set_type_default() do #(ShellTokens::script_component(vm)) {
        corner_radius: mod.wm_theme.shell.corner_radius
        bar +: {
            background: mod.wm_theme.shell.bar.background
            background_alpha: mod.wm_theme.shell.bar.background_alpha
            text: mod.wm_theme.shell.bar.text
            active: mod.wm_theme.shell.bar.active
            size_horizontal: mod.wm_theme.shell.bar.size_horizontal
            size_vertical: mod.wm_theme.shell.bar.size_vertical
            icon_slot: mod.wm_theme.shell.bar.icon_slot
            icon_canvas: mod.wm_theme.shell.bar.icon_canvas
            icon_font: mod.wm_theme.shell.bar.icon_font
            status_slot: mod.wm_theme.shell.bar.status_slot
        }
        popups +: {
            background: mod.wm_theme.shell.popups.background
            background_alpha: mod.wm_theme.shell.popups.background_alpha
            text: mod.wm_theme.shell.popups.text
            border: mod.wm_theme.shell.popups.border
            border_end: mod.wm_theme.shell.popups.border_end
            border_angle: mod.wm_theme.shell.popups.border_angle
            border_alpha: mod.wm_theme.shell.popups.border_alpha
            border_width: mod.wm_theme.shell.popups.border_width
        }
        tooltip +: {
            background: mod.wm_theme.shell.tooltip.background
            background_alpha: mod.wm_theme.shell.tooltip.background_alpha
            text: mod.wm_theme.shell.tooltip.text
            border: mod.wm_theme.shell.tooltip.border
            border_end: mod.wm_theme.shell.tooltip.border_end
            border_angle: mod.wm_theme.shell.tooltip.border_angle
            border_alpha: mod.wm_theme.shell.tooltip.border_alpha
            border_width: mod.wm_theme.shell.tooltip.border_width
        }
        notifications +: {
            countdown: mod.wm_theme.shell.notifications.countdown
            surface +: {
                background: mod.wm_theme.shell.notifications.background
                background_alpha: mod.wm_theme.shell.notifications.background_alpha
                text: mod.wm_theme.shell.notifications.text
                border: mod.wm_theme.shell.notifications.border
                border_end: mod.wm_theme.shell.notifications.border_end
                border_angle: mod.wm_theme.shell.notifications.border_angle
                border_alpha: mod.wm_theme.shell.notifications.border_alpha
                border_width: mod.wm_theme.shell.notifications.border_width
            }
        }
        menu +: {
            scrim: mod.wm_theme.shell.menu.scrim
            scrim_alpha: mod.wm_theme.shell.menu.scrim_alpha
            selected_background: mod.wm_theme.shell.menu.selected_background
            selected_background_alpha: mod.wm_theme.shell.menu.selected_background_alpha
            selected_text: mod.wm_theme.shell.menu.selected_text
            selected_border: mod.wm_theme.shell.menu.selected_border
            selected_border_alpha: mod.wm_theme.shell.menu.selected_border_alpha
            surface +: {
                background: mod.wm_theme.shell.menu.background
                background_alpha: mod.wm_theme.shell.menu.background_alpha
                text: mod.wm_theme.shell.menu.text
                border: mod.wm_theme.shell.menu.border
                border_end: mod.wm_theme.shell.menu.border_end
                border_angle: mod.wm_theme.shell.menu.border_angle
                border_alpha: mod.wm_theme.shell.menu.border_alpha
                border_width: mod.wm_theme.shell.menu.border_width
            }
        }
        launcher +: {
            scrim: mod.wm_theme.shell.launcher.scrim
            scrim_alpha: mod.wm_theme.shell.launcher.scrim_alpha
            selected_background: mod.wm_theme.shell.launcher.selected_background
            selected_background_alpha: mod.wm_theme.shell.launcher.selected_background_alpha
            selected_text: mod.wm_theme.shell.launcher.selected_text
            selected_border: mod.wm_theme.shell.launcher.selected_border
            selected_border_alpha: mod.wm_theme.shell.launcher.selected_border_alpha
            surface +: {
                background: mod.wm_theme.shell.launcher.background
                background_alpha: mod.wm_theme.shell.launcher.background_alpha
                text: mod.wm_theme.shell.launcher.text
                border: mod.wm_theme.shell.launcher.border
                border_end: mod.wm_theme.shell.launcher.border_end
                border_angle: mod.wm_theme.shell.launcher.border_angle
                border_alpha: mod.wm_theme.shell.launcher.border_alpha
                border_width: mod.wm_theme.shell.launcher.border_width
            }
        }
        controls +: {
            normal_color: mod.wm_theme.shell.controls.normal_color
            normal_fill_alpha: mod.wm_theme.shell.controls.normal_fill_alpha
            normal_border: mod.wm_theme.shell.controls.normal_border
            normal_border_width: mod.wm_theme.shell.controls.normal_border_width
            normal_border_alpha: mod.wm_theme.shell.controls.normal_border_alpha
            hover_color: mod.wm_theme.shell.controls.hover_color
            hover_fill_alpha: mod.wm_theme.shell.controls.hover_fill_alpha
            hover_border: mod.wm_theme.shell.controls.hover_border
            hover_border_width: mod.wm_theme.shell.controls.hover_border_width
            hover_border_alpha: mod.wm_theme.shell.controls.hover_border_alpha
            focus_color: mod.wm_theme.shell.controls.focus_color
            focus_fill_alpha: mod.wm_theme.shell.controls.focus_fill_alpha
            focus_border: mod.wm_theme.shell.controls.focus_border
            focus_border_width: mod.wm_theme.shell.controls.focus_border_width
            focus_border_alpha: mod.wm_theme.shell.controls.focus_border_alpha
            selected_color: mod.wm_theme.shell.controls.selected_color
            selected_fill_alpha: mod.wm_theme.shell.controls.selected_fill_alpha
            selected_border: mod.wm_theme.shell.controls.selected_border
            selected_border_width: mod.wm_theme.shell.controls.selected_border_width
            selected_border_alpha: mod.wm_theme.shell.controls.selected_border_alpha
            pressed_fill_alpha: mod.wm_theme.shell.controls.pressed_fill_alpha
            selection_fill_alpha: mod.wm_theme.shell.controls.selection_fill_alpha
        }
        spacing +: {
            xxs: mod.wm_theme.shell.spacing.xxs
            xs: mod.wm_theme.shell.spacing.xs
            sm: mod.wm_theme.shell.spacing.sm
            md: mod.wm_theme.shell.spacing.md
            lg: mod.wm_theme.shell.spacing.lg
            xl: mod.wm_theme.shell.spacing.xl
            xxl: mod.wm_theme.shell.spacing.xxl
            xxxl: mod.wm_theme.shell.spacing.xxxl
            huge: mod.wm_theme.shell.spacing.huge
            control_gap: mod.wm_theme.shell.spacing.control_gap
            control_padding_x: mod.wm_theme.shell.spacing.control_padding_x
            control_padding_y: mod.wm_theme.shell.spacing.control_padding_y
            input_padding_y: mod.wm_theme.shell.spacing.input_padding_y
            control_height: mod.wm_theme.shell.spacing.control_height
            popup_row_height: mod.wm_theme.shell.spacing.popup_row_height
            dropdown_width: mod.wm_theme.shell.spacing.dropdown_width
            searchable_dropdown_width: mod.wm_theme.shell.spacing.searchable_dropdown_width
            number_field_width: mod.wm_theme.shell.spacing.number_field_width
            searchable_popup_min_height: mod.wm_theme.shell.spacing.searchable_popup_min_height
            row_gap: mod.wm_theme.shell.spacing.row_gap
            row_padding_x: mod.wm_theme.shell.spacing.row_padding_x
            label_gap: mod.wm_theme.shell.spacing.label_gap
            panel_gap: mod.wm_theme.shell.spacing.panel_gap
            panel_padding: mod.wm_theme.shell.spacing.panel_padding
            popup_padding: mod.wm_theme.shell.spacing.popup_padding
            gaps_out: mod.wm_theme.shell.spacing.gaps_out
        }
        font +: {
            base_size: mod.wm_theme.shell.font.base_size
            caption: mod.wm_theme.shell.font.caption
            body_small: mod.wm_theme.shell.font.body_small
            body: mod.wm_theme.shell.font.body
            subtitle: mod.wm_theme.shell.font.subtitle
            title: mod.wm_theme.shell.font.title
            heading: mod.wm_theme.shell.font.heading
            display: mod.wm_theme.shell.font.display
            display_large: mod.wm_theme.shell.font.display_large
            icon_small: mod.wm_theme.shell.font.icon_small
            icon: mod.wm_theme.shell.font.icon
            icon_large: mod.wm_theme.shell.font.icon_large
        }
    }

    // ------------------------------------------------------------------
    // The kit. `Style.font.family` is "monospace", which on an omarchy box
    // is JetBrains Mono — the variable cut is the one makepad ships, so
    // `bold` is the same face at weight 700 (never a different family).
    // ------------------------------------------------------------------
    set_type_default() do #(ShellDraw::script_component(vm)) {
        fill +: {}
        chrome +: {}
        glass +: {}
        text +: {
            text_style: TextStyle{
                font_family: FontFamily{
                    latin := FontMember{
                        res: crate_resource("makepad_widgets:resources/jetbrains_mono_variable.ttf")
                        asc: 0.0 desc: 0.0 weight: 400.0
                    }
                    emoji := FontMember{
                        res: crate_resource("makepad_widgets:resources/NotoColorEmoji.ttf")
                        asc: 0.0 desc: 0.0
                    }
                }
                font_size: 9.0
                line_spacing: 1.2
            }
            color: #ffffff
        }
        text_bold +: {
            text_style: TextStyle{
                font_family: FontFamily{
                    latin := FontMember{
                        res: crate_resource("makepad_widgets:resources/jetbrains_mono_variable.ttf")
                        asc: 0.0 desc: 0.0 weight: 700.0
                    }
                    emoji := FontMember{
                        res: crate_resource("makepad_widgets:resources/NotoColorEmoji.ttf")
                        asc: 0.0 desc: 0.0
                    }
                }
                font_size: 9.0
                line_spacing: 1.2
            }
            color: #ffffff
        }
        icons +: {
            menu +: {svg: crate_resource("self:resources/icons/menu.svg")}
            dot +: {svg: crate_resource("self:resources/icons/dot.svg")}
            volume_0 +: {svg: crate_resource("self:resources/icons/volume-0.svg")}
            volume_1 +: {svg: crate_resource("self:resources/icons/volume-1.svg")}
            volume_2 +: {svg: crate_resource("self:resources/icons/volume-2.svg")}
            volume_3 +: {svg: crate_resource("self:resources/icons/volume-3.svg")}
            mic +: {svg: crate_resource("self:resources/icons/mic.svg")}
            mic_off +: {svg: crate_resource("self:resources/icons/mic-off.svg")}
            brightness +: {svg: crate_resource("self:resources/icons/brightness.svg")}
            bluetooth +: {svg: crate_resource("self:resources/icons/bluetooth.svg")}
            bluetooth_off +: {svg: crate_resource("self:resources/icons/bluetooth-off.svg")}
            wifi +: {svg: crate_resource("self:resources/icons/wifi.svg")}
            wifi_off +: {svg: crate_resource("self:resources/icons/wifi-off.svg")}
            monitor +: {svg: crate_resource("self:resources/icons/monitor.svg")}
            battery +: {svg: crate_resource("self:resources/icons/battery.svg")}
            power +: {svg: crate_resource("self:resources/icons/power.svg")}
            calendar +: {svg: crate_resource("self:resources/icons/calendar.svg")}
            keyboard +: {svg: crate_resource("self:resources/icons/keyboard.svg")}
            refresh +: {svg: crate_resource("self:resources/icons/refresh.svg")}
            bell +: {svg: crate_resource("self:resources/icons/bell.svg")}
            bell_off +: {svg: crate_resource("self:resources/icons/bell-off.svg")}
            moon +: {svg: crate_resource("self:resources/icons/moon.svg")}
            record +: {svg: crate_resource("self:resources/icons/record.svg")}
            chevron_left +: {svg: crate_resource("self:resources/icons/chevron-left.svg")}
            chevron_right +: {svg: crate_resource("self:resources/icons/chevron-right.svg")}
            chevron_down +: {svg: crate_resource("self:resources/icons/chevron-down.svg")}
            check +: {svg: crate_resource("self:resources/icons/check.svg")}
            close +: {svg: crate_resource("self:resources/icons/close.svg")}
            search +: {svg: crate_resource("self:resources/icons/search.svg")}
            cpu +: {svg: crate_resource("self:resources/icons/cpu.svg")}
            globe +: {svg: crate_resource("self:resources/icons/globe.svg")}
            play +: {svg: crate_resource("self:resources/icons/play.svg")}
            shirt +: {svg: crate_resource("self:resources/icons/shirt.svg")}
            pulse +: {svg: crate_resource("self:resources/icons/pulse.svg")}
            photo +: {svg: crate_resource("self:resources/icons/photo.svg")}
            window_min +: {svg: crate_resource("self:resources/icons/window-min.svg")}
            window_max +: {svg: crate_resource("self:resources/icons/window-max.svg")}
            window_restore +: {svg: crate_resource("self:resources/icons/window-restore.svg")}
            speaker +: {svg: crate_resource("self:resources/icons/speaker.svg")}
            headphone +: {svg: crate_resource("self:resources/icons/headphone.svg")}
            lock +: {svg: crate_resource("self:resources/icons/lock.svg")}
        }
    }
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawShellFill {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    pub color: Vec4f,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawShellChrome {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    pub color: Vec4f,
    #[live]
    pub border_color: Vec4f,
    #[live]
    pub border_color_end: Vec4f,
    #[live(0.0)]
    pub border_angle: f32,
    #[live(1.0)]
    pub border_width: f32,
}

/// The glass card. Instance fields are per draw (`ShellDraw::glass_rect`
/// fills them from the material); the pyramid textures and material-wide
/// uniforms are bound once per surface (`ShellDraw::bind_snapshot`). Colours
/// carry their alpha in `w`.
#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawShellGlass {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    pub tint_color: Vec4f,
    #[live]
    pub border_color: Vec4f,
    #[live(1.0)]
    pub border_width: f32,
    /// Sdf2d half-radius.
    #[live(6.0)]
    pub corner_radius: f32,
    #[live]
    pub shadow_color: Vec4f,
    #[live(0.0)]
    pub shadow_radius: f32,
    #[live(0.0)]
    pub shadow_offset_y: f32,
    #[live]
    pub fallback_color: Vec4f,
    #[live(0.22)]
    pub specular_strength: f32,
    #[live(0.004)]
    pub noise_strength: f32,
    /// Whole-quad fade, 1 = as the material says.
    #[live(1.0)]
    pub opacity: f32,
}

/// Our own SVGs on `DrawVector` — omarchy draws Nerd-Font glyphs, we draw
/// vectors (never an SDF icon shader).
#[derive(Script, ScriptHook)]
pub struct ShellIcons {
    #[live]
    pub menu: DrawSvg,
    #[live]
    pub dot: DrawSvg,
    #[live]
    pub volume_0: DrawSvg,
    #[live]
    pub volume_1: DrawSvg,
    #[live]
    pub volume_2: DrawSvg,
    #[live]
    pub volume_3: DrawSvg,
    #[live]
    pub mic: DrawSvg,
    #[live]
    pub mic_off: DrawSvg,
    #[live]
    pub brightness: DrawSvg,
    #[live]
    pub bluetooth: DrawSvg,
    #[live]
    pub bluetooth_off: DrawSvg,
    #[live]
    pub wifi: DrawSvg,
    #[live]
    pub wifi_off: DrawSvg,
    #[live]
    pub monitor: DrawSvg,
    #[live]
    pub battery: DrawSvg,
    #[live]
    pub power: DrawSvg,
    #[live]
    pub calendar: DrawSvg,
    #[live]
    pub keyboard: DrawSvg,
    #[live]
    pub refresh: DrawSvg,
    #[live]
    pub bell: DrawSvg,
    #[live]
    pub bell_off: DrawSvg,
    #[live]
    pub moon: DrawSvg,
    #[live]
    pub record: DrawSvg,
    #[live]
    pub chevron_left: DrawSvg,
    #[live]
    pub chevron_right: DrawSvg,
    #[live]
    pub chevron_down: DrawSvg,
    #[live]
    pub check: DrawSvg,
    #[live]
    pub close: DrawSvg,
    #[live]
    pub search: DrawSvg,
    #[live]
    pub cpu: DrawSvg,
    #[live]
    pub globe: DrawSvg,
    #[live]
    pub play: DrawSvg,
    #[live]
    pub shirt: DrawSvg,
    #[live]
    pub pulse: DrawSvg,
    #[live]
    pub photo: DrawSvg,
    #[live]
    pub window_min: DrawSvg,
    #[live]
    pub window_max: DrawSvg,
    #[live]
    pub window_restore: DrawSvg,
    #[live]
    pub speaker: DrawSvg,
    #[live]
    pub headphone: DrawSvg,
    #[live]
    pub lock: DrawSvg,
}

/// Which glyph a module wants. Named for what it MEANS, so the bar can
/// pick a battery level or a volume step without knowing about files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ico {
    Menu,
    Dot,
    Volume0,
    Volume1,
    Volume2,
    Volume3,
    Mic,
    MicOff,
    Brightness,
    Bluetooth,
    BluetoothOff,
    Wifi,
    WifiOff,
    Monitor,
    Battery,
    Power,
    Calendar,
    Keyboard,
    Refresh,
    Bell,
    BellOff,
    Moon,
    Record,
    ChevronLeft,
    ChevronRight,
    ChevronDown,
    Check,
    Close,
    Search,
    Cpu,
    Speaker,
    Headphone,
    Lock,
    Globe,
    Play,
    Shirt,
    Pulse,
    Photo,
    WindowMin,
    WindowMax,
    WindowRestore,
}

impl ShellIcons {
    fn get(&mut self, ico: Ico) -> &mut DrawSvg {
        match ico {
            Ico::Menu => &mut self.menu,
            Ico::Dot => &mut self.dot,
            Ico::Volume0 => &mut self.volume_0,
            Ico::Volume1 => &mut self.volume_1,
            Ico::Volume2 => &mut self.volume_2,
            Ico::Volume3 => &mut self.volume_3,
            Ico::Mic => &mut self.mic,
            Ico::MicOff => &mut self.mic_off,
            Ico::Brightness => &mut self.brightness,
            Ico::Bluetooth => &mut self.bluetooth,
            Ico::BluetoothOff => &mut self.bluetooth_off,
            Ico::Wifi => &mut self.wifi,
            Ico::WifiOff => &mut self.wifi_off,
            Ico::Monitor => &mut self.monitor,
            Ico::Battery => &mut self.battery,
            Ico::Power => &mut self.power,
            Ico::Calendar => &mut self.calendar,
            Ico::Keyboard => &mut self.keyboard,
            Ico::Refresh => &mut self.refresh,
            Ico::Bell => &mut self.bell,
            Ico::BellOff => &mut self.bell_off,
            Ico::Moon => &mut self.moon,
            Ico::Record => &mut self.record,
            Ico::ChevronLeft => &mut self.chevron_left,
            Ico::ChevronRight => &mut self.chevron_right,
            Ico::ChevronDown => &mut self.chevron_down,
            Ico::Check => &mut self.check,
            Ico::Close => &mut self.close,
            Ico::Search => &mut self.search,
            Ico::Cpu => &mut self.cpu,
            Ico::Globe => &mut self.globe,
            Ico::Play => &mut self.play,
            Ico::Shirt => &mut self.shirt,
            Ico::Pulse => &mut self.pulse,
            Ico::Photo => &mut self.photo,
            Ico::WindowMin => &mut self.window_min,
            Ico::WindowMax => &mut self.window_max,
            Ico::WindowRestore => &mut self.window_restore,
            Ico::Speaker => &mut self.speaker,
            Ico::Headphone => &mut self.headphone,
            Ico::Lock => &mut self.lock,
        }
    }
}

/// Horizontal placement of a label inside its box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

/// The kit — see the module note. One of these per surface widget.
#[derive(Script, ScriptHook)]
pub struct ShellDraw {
    #[live]
    pub fill: DrawShellFill,
    #[live]
    pub chrome: DrawShellChrome,
    #[live]
    pub glass: DrawShellGlass,
    #[live]
    pub text: DrawText,
    #[live]
    pub text_bold: DrawText,
    #[live]
    pub icons: ShellIcons,
    /// The material every surface drawn through this kit paints with —
    /// `set_material`; the flat one until `App::apply_material_to_chrome`
    /// (main.rs) hands over another.
    #[rust]
    material: MaterialTokens,
    #[rust]
    palette: Option<super::ShellPalette>,
    /// The overlay draw list a glass surface is hoisted into — created on
    /// the first glass draw, reused on every one after.
    #[rust]
    overlay: Option<DrawList2d>,
    /// Between `begin_surface` and `end_surface`: the surface was hoisted
    /// into `overlay`.
    #[rust]
    hoisted: bool,
    /// The frame `overlay` was last begun in — a surface hoists once per
    /// frame (see `begin_surface`).
    #[rust]
    hoist_redraw_id: u64,
    /// The double-hoist warning fires once per kit.
    #[rust]
    hoist_warned: bool,
    /// A refused second hoist (see `begin_surface`) has this one surface
    /// drawing flat. A flag rather than a write to `material`: the material
    /// stays what `set_material` said, and the flag heals itself at the
    /// next `begin_surface` even when the caller skipped `end_surface`.
    #[rust]
    force_flat: bool,
}

/// Makepad sizes text in POINTS; the QML scale is in pixels.
pub fn px_to_pt(px: f64) -> f32 {
    (px * 0.75) as f32
}

pub fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect {
        pos: dvec2(x, y),
        size: dvec2(w.max(0.0), h.max(0.0)),
    }
}

/// Shrink a rect on every side (QML `anchors.margins`).
pub fn inset(r: Rect, by: f64) -> Rect {
    rect(
        r.pos.x + by,
        r.pos.y + by,
        (r.size.x - by * 2.0).max(0.0),
        (r.size.y - by * 2.0).max(0.0),
    )
}

pub fn contains(r: Rect, p: Vec2d) -> bool {
    p.x >= r.pos.x && p.x < r.pos.x + r.size.x && p.y >= r.pos.y && p.y < r.pos.y + r.size.y
}

/// Cut `h` off the top of `r`, returning (the strip, the rest).
pub fn cut_top(r: Rect, h: f64) -> (Rect, Rect) {
    let h = h.min(r.size.y);
    (
        rect(r.pos.x, r.pos.y, r.size.x, h),
        rect(r.pos.x, r.pos.y + h, r.size.x, r.size.y - h),
    )
}

impl ShellDraw {
    // ------------------------------------------------------------- text

    fn face(&mut self, bold: bool) -> &mut DrawText {
        if bold {
            &mut self.text_bold
        } else {
            &mut self.text
        }
    }

    /// Width of one line at a px size — QML's `Text.implicitWidth`.
    pub fn measure(&mut self, cx: &mut Cx2d, bold: bool, px: f64, s: &str) -> f64 {
        if s.is_empty() {
            return 0.0;
        }
        let face = self.face(bold);
        face.text_style.font_size = px_to_pt(px);
        face.prepare_single_line_run(cx, s)
            .map(|r| r.width_in_lpxs as f64)
            .unwrap_or(0.0)
    }

    /// `elide: Text.ElideRight`.
    pub fn elide(&mut self, cx: &mut Cx2d, bold: bool, px: f64, s: &str, max_w: f64) -> String {
        if max_w <= 0.0 {
            return String::new();
        }
        if self.measure(cx, bold, px, s) <= max_w {
            return s.to_string();
        }
        let chars: Vec<char> = s.chars().collect();
        let mut lo = 0usize;
        let mut hi = chars.len();
        while lo < hi {
            let mid = (lo + hi + 1) / 2;
            let candidate: String = chars[..mid].iter().collect::<String>() + "\u{2026}";
            if self.measure(cx, bold, px, &candidate) <= max_w {
                lo = mid;
            } else {
                hi = mid - 1;
            }
            if lo == hi {
                break;
            }
        }
        chars[..lo].iter().collect::<String>() + "\u{2026}"
    }

    /// `wrapMode: WordWrap` with `elide: ElideRight` on the last line and a
    /// `maximumLineCount` cap — the notification card's summary (2) and
    /// body (3).
    pub fn wrap(
        &mut self,
        cx: &mut Cx2d,
        bold: bool,
        px: f64,
        text: &str,
        max_w: f64,
        max_lines: usize,
    ) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        for para in text.split('\n') {
            let mut line = String::new();
            for word in para.split_whitespace() {
                let candidate = if line.is_empty() {
                    word.to_string()
                } else {
                    format!("{} {}", line, word)
                };
                if self.measure(cx, bold, px, &candidate) <= max_w || line.is_empty() {
                    line = candidate;
                } else {
                    lines.push(std::mem::take(&mut line));
                    line = word.to_string();
                    if lines.len() == max_lines {
                        break;
                    }
                }
            }
            if !line.is_empty() && lines.len() < max_lines {
                lines.push(line);
            }
            if lines.len() >= max_lines {
                break;
            }
        }
        // Anything that did not fit is elided onto the last line.
        let overflowed = {
            let joined = lines.join(" ");
            joined.split_whitespace().count() < text.split_whitespace().count()
        };
        if overflowed {
            if let Some(last) = lines.last_mut() {
                let s = format!("{}\u{2026}", last);
                *last = s;
            }
        }
        if lines.is_empty() && !text.is_empty() {
            lines.push(self.elide(cx, bold, px, text, max_w));
        }
        lines
    }

    /// One line, placed at an absolute top-left.
    pub fn text_at(
        &mut self,
        cx: &mut Cx2d,
        pos: Vec2d,
        bold: bool,
        px: f64,
        color: Vec4f,
        s: &str,
    ) {
        if s.is_empty() {
            return;
        }
        let face = self.face(bold);
        face.text_style.font_size = px_to_pt(px);
        face.color = color;
        face.draw_abs(cx, pos, s);
    }

    /// One line inside a box: horizontally per `align`, vertically centered
    /// on the ink — what every QML label in the kit does.
    pub fn label(
        &mut self,
        cx: &mut Cx2d,
        r: Rect,
        bold: bool,
        px: f64,
        color: Vec4f,
        align: HAlign,
        s: &str,
    ) {
        if s.is_empty() {
            return;
        }
        let face = self.face(bold);
        face.text_style.font_size = px_to_pt(px);
        let run = face.layout(cx, 0.0, 0.0, None, false, Align::default(), s);
        let w = run.size_in_lpxs.width as f64;
        let x = match align {
            HAlign::Left => r.pos.x,
            HAlign::Center => r.pos.x + (r.size.x - w) * 0.5,
            HAlign::Right => r.pos.x + r.size.x - w,
        };
        let y = r.pos.y + (r.size.y - run.size_in_lpxs.height as f64) * 0.5 + run.ink_center_offset_in_lpxs() as f64;
        let dpi = cx.current_dpi_factor();
        self.text_at(cx, dvec2((x*dpi).round()/dpi, (y*dpi).round()/dpi), bold, px, color, s);
    }

    /// As `label`, elided to the box first.
    pub fn label_elided(
        &mut self,
        cx: &mut Cx2d,
        r: Rect,
        bold: bool,
        px: f64,
        color: Vec4f,
        align: HAlign,
        s: &str,
    ) {
        let s = self.elide(cx, bold, px, s, r.size.x);
        self.label(cx, r, bold, px, color, align, &s);
    }

    // ---------------------------------------------------------- material

    /// Hand the kit the material its surfaces paint with. Nothing redraws
    /// here; the next `begin_surface` and `card` read it.
    pub fn set_material(&mut self, m: MaterialTokens) {
        self.material = m;
    }

    pub fn set_palette(&mut self, palette: Option<super::ShellPalette>) {
        self.palette = palette;
    }

    pub fn tokens(&self, base: ShellTokens) -> ShellTokens {
        base.with_palette(self.palette)
    }

    /// What `begin_surface` and `card` paint with right now — the flat
    /// material until `set_material` says otherwise, and reported flat for
    /// the rest of a surface whose hoist was refused, so a caller branching
    /// on `is_glass()` agrees with what the kit will draw.
    pub fn material(&self) -> MaterialTokens {
        if self.force_flat {
            MaterialTokens {
                glass: 0.0,
                ..self.material
            }
        } else {
            self.material
        }
    }

    /// Whether the glass paths draw right now: a glass material, and no
    /// refused hoist in progress.
    fn paints_glass(&self) -> bool {
        self.material.is_glass() && !self.force_flat
    }

    // ---------------------------------------------------------- surfaces

    /// Start drawing one surface. Under glass this hoists everything drawn
    /// until `end_surface` into the kit's own overlay draw list — the
    /// pyramid snapshot is only handed out while an overlay is drawing,
    /// and overlays stay out of the capture, so glass never refracts
    /// itself — and binds the snapshot to the glass shader. Under flat it
    /// returns at once. Overlay lists composite in DRAW order — each
    /// `begin_overlay_reuse` stamps this frame's order — so the call must
    /// happen every frame, open or closed, to keep the surface in tree
    /// order. A surface may be hoisted once per frame: a second hoist
    /// would clear the same list under entries the ancestor still aligns.
    pub fn begin_surface(&mut self, cx: &mut Cx2d) {
        // Both reset here, not only in `end_surface`, so a surface that
        // returned early last frame does not stay hoisted or flat for good.
        self.hoisted = false;
        self.force_flat = false;
        if !self.material.is_glass() {
            return;
        }
        if !cx.is_drawing_overlay() {
            let redraw_id = cx.redraw_id();
            if self.hoist_redraw_id == redraw_id {
                if !self.hoist_warned {
                    log!("ShellDraw: surface hoisted twice in one frame; the second hoist is skipped and that draw is flat");
                    self.hoist_warned = true;
                }
                // The un-hoisted body draws flat, by flag: the material is
                // the kit's own state and only `set_material` writes it.
                self.force_flat = true;
                return;
            }
            self.hoist_redraw_id = redraw_id;
            if self.overlay.is_none() {
                self.overlay = Some(DrawList2d::new(cx));
            }
            self.overlay.as_mut().unwrap().begin_overlay_reuse(cx);
            self.hoisted = true;
        }
        let snapshot = request_window_gauss(cx);
        self.bind_snapshot(cx, snapshot);
    }

    /// Close what `begin_surface` opened: ends the kit's overlay list when
    /// this draw was hoisted and lifts a refused hoist's flat flag; a no-op
    /// under flat.
    pub fn end_surface(&mut self, cx: &mut Cx2d) {
        if self.hoisted {
            if let Some(list) = self.overlay.as_mut() {
                list.end(cx);
            }
            self.hoisted = false;
        }
        self.force_flat = false;
    }

    /// The pyramid textures and the material-wide uniforms onto the glass
    /// shader (mirrors `GaussRoundedView::bind_snapshot`). `begin_surface`
    /// binds the window's own capture; a compositor with a snapshot of its
    /// own hands it in here instead.
    pub fn bind_snapshot(&mut self, cx: &mut Cx2d, snapshot: Option<GaussBlurSnapshot>) {
        let m = self.material;
        let draw = &mut self.glass.draw_vars;
        match snapshot {
            Some(s) => {
                draw.set_texture(0, &s.scene_texture);
                for slot in 1..=GAUSS_VIEW_LEVELS {
                    match s.mip_textures.get(slot - 1) {
                        Some(t) => draw.set_texture(slot, t),
                        None => draw.empty_texture(slot),
                    }
                }
                draw.set_uniform(
                    cx,
                    live_id!(source_size),
                    &[s.source_size.x as f32, s.source_size.y as f32],
                );
                draw.set_uniform(cx, live_id!(source_y_flip), &[s.source_y_flip]);
                draw.set_uniform(cx, live_id!(has_gauss), &[1.0]);
            }
            None => {
                for slot in 0..=GAUSS_VIEW_LEVELS {
                    draw.empty_texture(slot);
                }
                draw.set_uniform(cx, live_id!(source_size), &[1.0, 1.0]);
                draw.set_uniform(cx, live_id!(source_y_flip), &[0.0]);
                draw.set_uniform(cx, live_id!(has_gauss), &[0.0]);
            }
        }
        draw.set_uniform(cx, live_id!(blur_level), &[m.blur_level as f32]);
        draw.set_uniform(cx, live_id!(lensing_effect), &[m.lensing_effect as f32]);
        draw.set_uniform(cx, live_id!(lensing_strength), &[m.lensing_strength as f32]);
        draw.set_uniform(cx, live_id!(lensing_width), &[m.lensing_width as f32]);
        draw.set_uniform(
            cx,
            live_id!(diffraction_strength),
            &[m.diffraction_strength as f32],
        );
    }

    /// One glass quad: the material at `radius` (visual px), with or
    /// without its drop shadow. Draws nothing unless the kit paints glass
    /// right now — flat callers paint their own fill, and after a refused
    /// hoist `begin_surface` returned before `bind_snapshot`, so a quad
    /// here would sample stale or empty textures in the body pass.
    pub fn glass_rect(&mut self, cx: &mut Cx2d, r: Rect, radius: f64, shadow: bool) {
        self.glass_quad(cx, r, radius, shadow, self.material.border_alpha, 1.0);
    }

    /// A window's glass frame: the material without its shadow (the desk
    /// casts the window's), the ring at `border_alpha` — the desk dims it on
    /// an unfocused window — and the whole quad at `opacity`, the chrome's
    /// popin and style-tween fade. Same guard as `glass_rect`.
    pub fn glass_frame(&mut self, cx: &mut Cx2d, r: Rect, radius: f64, border_alpha: f32, opacity: f32) {
        self.glass_quad(cx, r, radius, false, border_alpha, opacity);
    }

    /// A free-floating glass surface — the shelf's pill: `glass_rect` with
    /// the material's shadow, the whole quad at `opacity` so a style tween
    /// fades shadow, fill and ring as one. Same guard as `glass_rect`.
    pub fn glass_pill(&mut self, cx: &mut Cx2d, r: Rect, radius: f64, opacity: f32) {
        self.glass_quad(cx, r, radius, true, self.material.border_alpha, opacity);
    }

    fn glass_quad(&mut self, cx: &mut Cx2d, r: Rect, radius: f64, shadow: bool, border_alpha: f32, opacity: f32) {
        if !self.paints_glass() || r.size.x <= 0.0 || r.size.y <= 0.0 || opacity <= 0.0 {
            return;
        }
        let m = self.material;
        let g = &mut self.glass;
        g.tint_color = alpha(m.tint_color, m.tint_alpha);
        g.border_color = alpha(m.border_color, border_alpha);
        g.border_width = m.border_width as f32;
        g.corner_radius = (radius * 0.5) as f32;
        g.shadow_color = alpha(m.shadow_color, if shadow { m.shadow_alpha } else { 0.0 });
        g.shadow_radius = if shadow { m.shadow_radius as f32 } else { 0.0 };
        g.shadow_offset_y = if shadow { m.shadow_offset_y as f32 } else { 0.0 };
        g.fallback_color = m.fallback_color;
        g.specular_strength = m.specular_strength;
        g.noise_strength = m.noise_strength;
        g.opacity = opacity.min(1.0);
        g.draw_abs(cx, r);
    }

    /// The bar's strip under glass: the material, near-square (the shader
    /// floors the half-radius at 1px), no shadow. Nothing under flat — the
    /// flat bar paints its own fill (`glass_rect` holds the guard).
    pub fn glass_strip(&mut self, cx: &mut Cx2d, r: Rect) {
        self.glass_rect(cx, r, 0.0, false);
    }

    /// A flat fill.
    pub fn solid(&mut self, cx: &mut Cx2d, r: Rect, color: Vec4f) {
        if color.w <= 0.0 || r.size.x <= 0.0 || r.size.y <= 0.0 {
            return;
        }
        self.fill.color = color;
        self.fill.draw_abs(cx, r);
    }

    /// `BorderSurface`: fill + ring. `border_end`/`angle` let a theme's
    /// hyprland gradient through.
    pub fn bordered(
        &mut self,
        cx: &mut Cx2d,
        r: Rect,
        fill: Vec4f,
        border: Vec4f,
        border_end: Vec4f,
        angle: f32,
        width: f64,
    ) {
        if r.size.x <= 0.0 || r.size.y <= 0.0 {
            return;
        }
        self.chrome.color = fill;
        self.chrome.border_color = if width > 0.0 {
            border
        } else {
            alpha(border, 0.0)
        };
        self.chrome.border_color_end = if width > 0.0 {
            border_end
        } else {
            alpha(border_end, 0.0)
        };
        self.chrome.border_angle = angle;
        self.chrome.border_width = width as f32;
        self.chrome.draw_abs(cx, r);
    }

    /// A themed card: `[popups]` / `[menu]` / `[notifications]` chrome.
    /// Flat: the token fill and ring. Glass: the material, refracting what
    /// lies beneath — the token's own colours are the flat look's, and the
    /// radius is the material's (the shader takes it per draw, so the card
    /// hands it over rather than the token).
    pub fn card(&mut self, cx: &mut Cx2d, r: Rect, s: &SurfaceTokens) {
        if self.paints_glass() {
            self.glass_rect(cx, r, self.material.corner_radius, true);
            return;
        }
        self.bordered(
            cx,
            r,
            s.bg(),
            s.border_start(),
            s.border_stop(),
            s.border_angle,
            s.border_width,
        );
    }

    /// A control face in one of the shared states (`Style.controlFill` +
    /// `Border.controlSpec`).
    pub fn control(&mut self, cx: &mut Cx2d, r: Rect, c: &ControlTokens, state: CtrlState) {
        let border = c.border(state);
        self.bordered(
            cx,
            r,
            c.fill(state),
            border,
            border,
            0.0,
            c.border_width(state),
        );
    }

    /// `CursorSurface`: nothing at rest, the hover fill under the cursor,
    /// the selected fill for the current row.
    pub fn cursor_surface(
        &mut self,
        cx: &mut Cx2d,
        r: Rect,
        c: &ControlTokens,
        has_cursor: bool,
        current: bool,
    ) {
        let fill = c.cursor_fill(has_cursor, current);
        if fill.w > 0.0 {
            self.solid(cx, r, fill);
        }
    }

    /// `PanelSeparator` — a 1px rule at `foreground` × strength (0.12).
    pub fn separator(&mut self, cx: &mut Cx2d, r: Rect, foreground: Vec4f, strength: f32) {
        self.solid(
            cx,
            rect(r.pos.x, r.pos.y, r.size.x, 1.0),
            alpha(foreground, strength),
        );
    }

    // ------------------------------------------------------------- icons

    /// One SVG, fitted into `r` and tinted.
    pub fn icon(&mut self, cx: &mut Cx2d, ico: Ico, r: Rect, color: Vec4f) {
        if r.size.x <= 0.0 || r.size.y <= 0.0 || color.w <= 0.0 {
            return;
        }
        let svg = self.icons.get(ico);
        svg.color = color;
        svg.draw_abs(cx, r);
    }

    /// An icon centered in a slot at `size` px square (the bar's
    /// `iconCanvas` inside its `iconSlot`).
    pub fn icon_centered(&mut self, cx: &mut Cx2d, ico: Ico, slot: Rect, size: f64, color: Vec4f) {
        let r = rect(
            (slot.pos.x + (slot.size.x - size) * 0.5).floor(),
            (slot.pos.y + (slot.size.y - size) * 0.5).floor(),
            size,
            size,
        );
        self.icon(cx, ico, r, color);
    }

    // --------------------------------------------------------- controls

    /// `Ui/Button.qml`: `[icon] [label]` centered, `controlPaddingX/Y`
    /// padding, `controlGap` between them. Returns the rect it drew into.
    #[allow(clippy::too_many_arguments)]
    pub fn button(
        &mut self,
        cx: &mut Cx2d,
        r: Rect,
        tok: &ShellTokens,
        state: CtrlState,
        icon: Option<Ico>,
        text: &str,
        px: f64,
        foreground: Vec4f,
        bordered: bool,
    ) {
        let c = &tok.controls;
        if bordered || !matches!(state, CtrlState::Normal) {
            self.control(cx, r, c, state);
        } else if state == CtrlState::Selected {
            self.solid(cx, r, c.fill(CtrlState::Selected));
        }
        let fg = if state == CtrlState::Disabled {
            super::darker(foreground, 2.0)
        } else {
            foreground
        };
        let gap = tok.spacing.control_gap;
        let icon_w = if icon.is_some() { tok.font.icon } else { 0.0 };
        let text_w = self.measure(cx, state == CtrlState::Selected, px, text);
        let total = icon_w + if icon.is_some() && !text.is_empty() { gap } else { 0.0 } + text_w;
        let mut x = r.pos.x + (r.size.x - total) * 0.5;
        if let Some(ico) = icon {
            self.icon_centered(
                cx,
                ico,
                rect(x, r.pos.y, icon_w, r.size.y),
                icon_w,
                fg,
            );
            x += icon_w + if text.is_empty() { 0.0 } else { gap };
        }
        if !text.is_empty() {
            self.label(
                cx,
                rect(x, r.pos.y, text_w, r.size.y),
                state == CtrlState::Selected,
                px,
                fg,
                HAlign::Left,
                text,
            );
        }
    }

    /// The implicit width of a `Button` — label + `controlPaddingX * 2`.
    pub fn button_width(
        &mut self,
        cx: &mut Cx2d,
        tok: &ShellTokens,
        icon: bool,
        text: &str,
        px: f64,
    ) -> f64 {
        let text_w = self.measure(cx, false, px, text);
        let icon_w = if icon { tok.font.icon } else { 0.0 };
        let gap = if icon && !text.is_empty() {
            tok.spacing.control_gap
        } else {
            0.0
        };
        text_w + icon_w + gap + tok.spacing.control_padding_x * 2.0
    }

    /// `Ui/ToggleSwitch.qml`: a track `max(22, round(controlHeight*0.55))`
    /// high, `1.9x` as wide, with a `0.72x` knob inset by the remainder.
    pub fn toggle_switch(
        &mut self,
        cx: &mut Cx2d,
        at: Vec2d,
        tok: &ShellTokens,
        checked: bool,
        foreground: Vec4f,
    ) -> Rect {
        let c = &tok.controls;
        let track_h = (tok.spacing.control_height * 0.55).round().max(22.0);
        let track_w = (track_h * 1.9).round();
        let knob = (track_h * 0.72).round().max(6.0);
        let inset_px = ((track_h - knob) / 2.0).round().max(1.0);
        let track = rect(at.x, at.y, track_w, track_h);
        let state = if checked {
            CtrlState::Selected
        } else {
            CtrlState::Normal
        };
        self.control(cx, track, c, state);
        let knob_x = if checked {
            track.pos.x + track.size.x - knob - inset_px
        } else {
            track.pos.x + inset_px
        };
        let knob_color = if checked {
            c.selected_color
        } else {
            super::darker(foreground, 1.25)
        };
        self.solid(
            cx,
            rect(knob_x, track.pos.y + inset_px, knob, knob),
            knob_color,
        );
        track
    }

    /// `Ui/PanelSlider.qml`: a `max(4, round(controlHeight*0.11))` track
    /// with a `max(14, round(controlHeight*0.38))` knob ringed in the panel
    /// background — the one place the kit uses a flat ring instead of a
    /// state token.
    #[allow(clippy::too_many_arguments)]
    pub fn panel_slider(
        &mut self,
        cx: &mut Cx2d,
        r: Rect,
        tok: &ShellTokens,
        progress: f64,
        foreground: Vec4f,
        background: Vec4f,
        hot: bool,
    ) {
        let c = &tok.controls;
        let track_h = (tok.spacing.control_height * 0.11).round().max(4.0);
        let knob = (tok.spacing.control_height * 0.38).round().max(14.0);
        let track = rect(
            r.pos.x,
            r.pos.y + (r.size.y - track_h) * 0.5,
            r.size.x,
            track_h,
        );
        self.solid(cx, track, alpha(foreground, c.selected_fill_alpha));
        let p = progress.clamp(0.0, 1.0);
        self.solid(
            cx,
            rect(track.pos.x, track.pos.y, track.size.x * p, track.size.y),
            foreground,
        );
        let scale = if hot { 1.15 } else { 1.0 };
        let ks = (knob * scale).round();
        let kx = (track.pos.x + track.size.x * p - ks * 0.5)
            .clamp(track.pos.x, track.pos.x + track.size.x - ks);
        let ky = r.pos.y + (r.size.y - ks) * 0.5;
        self.bordered(
            cx,
            rect(kx, ky, ks, ks),
            foreground,
            background,
            background,
            0.0,
            2.0,
        );
    }

    /// `Ui/TextField.qml`: the control face plus the text (or the
    /// placeholder at `darker(fg, 1.6)`) inset by `controlPaddingX` /
    /// `inputPaddingY`, with a 1px caret when focused.
    #[allow(clippy::too_many_arguments)]
    pub fn text_field(
        &mut self,
        cx: &mut Cx2d,
        r: Rect,
        tok: &ShellTokens,
        text: &str,
        placeholder: &str,
        focused: bool,
        hot: bool,
        foreground: Vec4f,
    ) {
        let state = if focused {
            CtrlState::Focus
        } else if hot {
            CtrlState::Hover
        } else {
            CtrlState::Normal
        };
        self.control(cx, r, &tok.controls, state);
        let inner = rect(
            r.pos.x + tok.spacing.control_padding_x,
            r.pos.y,
            (r.size.x - tok.spacing.control_padding_x * 2.0).max(0.0),
            r.size.y,
        );
        let px = tok.font.body;
        if text.is_empty() {
            self.label_elided(
                cx,
                inner,
                false,
                px,
                super::darker(foreground, 1.6),
                HAlign::Left,
                placeholder,
            );
        } else {
            self.label_elided(cx, inner, false, px, foreground, HAlign::Left, text);
            if focused {
                let w = self.measure(cx, false, px, text).min(inner.size.x);
                self.solid(
                    cx,
                    rect(
                        inner.pos.x + w + 1.0,
                        inner.pos.y + (inner.size.y - px * 1.1) * 0.5,
                        1.0,
                        px * 1.1,
                    ),
                    foreground,
                );
            }
        }
    }

    /// `Ui/PanelSectionHeader.qml`: bold caption in `darker(fg, 1.4)`.
    pub fn section_header(&mut self, cx: &mut Cx2d, r: Rect, tok: &ShellTokens, fg: Vec4f, s: &str) {
        self.label(
            cx,
            r,
            true,
            tok.font.caption,
            super::darker(fg, 1.4),
            HAlign::Left,
            s,
        );
    }

    /// `Ui/PanelHero.qml`: the icon, a bold `title` line and an uppercase
    /// `caption` meta line at `darker(fg, 1.4)`, with an optional trailing
    /// control kept clear on the right.
    #[allow(clippy::too_many_arguments)]
    pub fn panel_hero(
        &mut self,
        cx: &mut Cx2d,
        r: Rect,
        tok: &ShellTokens,
        fg: Vec4f,
        icon: Ico,
        title: &str,
        meta: &str,
        trailing_inset: f64,
    ) {
        let icon_size = tok.font.display;
        self.icon_centered(
            cx,
            icon,
            rect(r.pos.x, r.pos.y, icon_size, r.size.y),
            icon_size,
            fg,
        );
        let x = r.pos.x + icon_size + 14.0;
        let w = (r.size.x - (x - r.pos.x) - trailing_inset).max(0.0);
        let dim = super::darker(fg, 1.4);
        let title_h = tok.font.title * 1.4;
        let meta_h = tok.font.caption * 1.4;
        let total = title_h + 2.0 + meta_h;
        let top = r.pos.y + (r.size.y - total) * 0.5;
        self.label_elided(
            cx,
            rect(x, top, w, title_h),
            true,
            tok.font.title,
            fg,
            HAlign::Left,
            title,
        );
        self.label_elided(
            cx,
            rect(x, top + title_h + 2.0, w, meta_h),
            true,
            tok.font.caption,
            dim,
            HAlign::Left,
            &meta.to_uppercase(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_are_three_quarters_of_pixels() {
        // The QML scale is px; makepad's font_size is pt.
        assert_eq!(px_to_pt(12.0), 9.0);
        assert_eq!(px_to_pt(28.0), 21.0);
    }

    #[test]
    fn rect_helpers_never_invert() {
        let r = rect(10.0, 10.0, 20.0, 20.0);
        assert_eq!(inset(r, 30.0).size.x, 0.0);
        let (top, rest) = cut_top(r, 5.0);
        assert_eq!(top.size.y, 5.0);
        assert_eq!(rest.pos.y, 15.0);
        assert_eq!(rest.size.y, 15.0);
        assert!(contains(r, dvec2(10.0, 10.0)));
        assert!(!contains(r, dvec2(30.0, 10.0)));
    }

    /// Evaluate one theme block the way `App::script_mod` does: leading
    /// comment lines shift the parser's spans, and the final statement is
    /// the result expression, so the body starts at the first statement
    /// and ends in a benign `true`.
    fn eval_theme(vm: &mut ScriptVm, name: &str, code: &str) {
        let mut body = code
            .lines()
            .skip_while(|l| {
                let t = l.trim();
                t.is_empty() || t.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n");
        body.push_str("\ntrue\n");
        vm.eval(ScriptMod {
            cargo_manifest_path: env!("CARGO_MANIFEST_DIR").to_string(),
            module_path: name.to_string(),
            file: "theme.splash".to_string(),
            line: 0,
            column: 0,
            code: body,
            values: vec![],
        });
    }

    /// A syntax-and-mapping guard: the kit's DSL evaluates without error,
    /// its literals land on the glass struct, and a kit nobody handed a
    /// material reports flat until `set_material`. Not a shader guard —
    /// an error in a shader body only surfaces in a draw.
    #[test]
    fn the_kit_defaults_to_the_flat_material() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let (mut d, errors) = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            // The kit's token defaults read `mod.wm_theme.shell.*`, so the
            // bundled theme goes in first, as at startup. Script errors are
            // captured: a kit whose defaults failed to evaluate is not the
            // kit this test is about.
            vm.bx.captured_errors = Some(Vec::new());
            let theme = crate::theme::BUNDLED_TOKYO_NIGHT_SPLASH;
            eval_theme(vm, "wm_theme", theme);
            eval_theme(
                vm,
                "wm_theme_shell",
                &crate::theme::shell_splash_block(theme),
            );
            script_mod(vm);
            let d = ShellDraw::script_new_with_default(vm);
            (d, vm.take_errors())
        });
        assert!(
            errors.is_empty(),
            "kit defaults did not evaluate: {errors:?}"
        );
        // Two literals from the DSL type default, on the struct.
        assert_eq!(d.glass.corner_radius, 6.0);
        assert_eq!(d.glass.border_width, 1.0);
        // The flat omarchy look every existing style gets.
        assert!(!d.material().is_glass());
        d.set_material(MaterialTokens {
            glass: 1.0,
            ..Default::default()
        });
        // No draw here, so no hoist was ever refused: the material is
        // reported as set, not forced flat.
        assert!(d.material().is_glass());
    }
}
