pub use makepad_widgets::desktop_style::DesktopStyle;
use makepad_widgets::*;

/// Everything one desktop style says about the shell's geometry and family
/// behaviour, in one row. `StyleTween::mix` blends rows by the tween weights
/// exactly as the literal arrays it replaces did. Numbers are logical px.
/// Anything that blends through a tween goes in a spec field; a discrete
/// family branch uses `DesktopStyle::mac_family()`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StyleSpec {
    pub style: DesktopStyle,
    /// Tiling desk: Omarchy's ring and gaps; no floating chrome.
    pub tiling: bool,
    /// What the desk reserves at the bottom of the work area for the shelf;
    /// a floating shelf (the macOS dock) reserves nothing.
    pub reserved_height: f64,
    pub title_height: f64,
    /// Child inset from the tile rect: Omarchy's ring, the retro bevel frame,
    /// or MakeOS's glass ring — 2 × the material's 1 px border, since the
    /// stroke is centred one border-width in and a 1 px inset would show only
    /// half of it. A `glass_chrome` row's inset counts only under a glass
    /// material (`StyleTween::frame_inset`).
    pub frame_inset: f64,
    /// Window corner rounding through the captured window surface.
    pub rounding: f64,
    pub chrome_radius: f64,
    pub frame_width: f64,
    pub caption_width: f64,
    /// Shelf (dock/taskbar) corner radius.
    pub shelf_radius: f64,
    /// Bottom resize bar height (NeXTSTEP).
    pub resize_bar: f64,
    /// Window shadow opacity when focused (unfocused is ×0.16/0.28, see
    /// `StyleTween::window_shadow_opacity`); 0 = no shadow. The row owns the
    /// window geometry, so the desk's shadow reads this, not the sheet.
    pub shadow: f64,
    pub glass_shelf: bool,
    /// The desk runs the backdrop compositor for this style.
    pub composes: bool,
    pub caption_mac: bool,
    pub bevel_classic: bool,
    pub bevel_next: bool,
    /// Ground gradient (top, bottom), light appearance.
    pub ground: ((u8, u8, u8), (u8, u8, u8)),
    /// Ground gradient in the dark appearance; styles without a dark ground
    /// repeat `ground`. `supports_dark()` decides which is read.
    pub ground_dark: ((u8, u8, u8), (u8, u8, u8)),
    /// Popup menu offset from the screen bottom (macOS 98, Windows 66, W2K 34);
    /// 0 = the style's own placement.
    pub menu_bottom_offset: f64,
    /// Minimize warps the window into its shelf icon (the dock genie) instead
    /// of fading it out; restore plays it back.
    pub dock_warp: bool,
    /// Chrome that is dark by identity, whatever the appearance flag says:
    /// MakeOS's sheet has no light look. Read through `dark_chrome`.
    pub dark_chrome: bool,
    /// This style's chrome is the Liquid Glass material: the window frame,
    /// the shelf pill and the kit's surfaces paint from the sheet's material
    /// block. A second glass style needs only this flag.
    pub glass_chrome: bool,
}

/// One row per `DesktopStyle`, at the discriminant the tween weights index.
/// A static rather than a const so `StyleTween::spec` can hand out a row.
pub static SPECS: [StyleSpec; 8] = [
    StyleSpec {
        style: DesktopStyle::Omarchy,
        tiling: true,
        reserved_height: 0.0, title_height: 0.0,
        frame_inset: crate::desk::BORDER_SIZE, rounding: 0.0, chrome_radius: 0.0,
        frame_width: 2.0, caption_width: 30.0, shelf_radius: 0.0, resize_bar: 0.0, shadow: 0.0,
        glass_shelf: false, composes: false, caption_mac: false, bevel_classic: false, bevel_next: false,
        ground: ((16, 19, 21), (24, 30, 34)),
        ground_dark: ((16, 19, 21), (24, 30, 34)),
        menu_bottom_offset: 0.0,
        dock_warp: false,
        dark_chrome: false,
        glass_chrome: false,
    },
    StyleSpec {
        style: DesktopStyle::Macos,
        tiling: false,
        reserved_height: 0.0, title_height: 32.0,
        frame_inset: 0.0, rounding: 14.0, chrome_radius: 10.0,
        frame_width: 2.0, caption_width: 30.0, shelf_radius: 18.0, resize_bar: 0.0, shadow: 0.28,
        glass_shelf: true, composes: true, caption_mac: true, bevel_classic: false, bevel_next: false,
        ground: ((39, 43, 87), (171, 109, 131)),
        ground_dark: ((12, 15, 36), (65, 36, 69)),
        menu_bottom_offset: 98.0,
        dock_warp: true,
        dark_chrome: false,
        glass_chrome: false,
    },
    StyleSpec {
        style: DesktopStyle::Windows,
        tiling: false,
        reserved_height: 54.0, title_height: 34.0,
        frame_inset: 0.0, rounding: 8.0, chrome_radius: 8.0,
        frame_width: 2.0, caption_width: 46.0, shelf_radius: 0.0, resize_bar: 0.0, shadow: 0.28,
        glass_shelf: false, composes: false, caption_mac: false, bevel_classic: false, bevel_next: false,
        ground: ((10, 45, 108), (24, 137, 210)),
        ground_dark: ((10, 19, 34), (21, 49, 72)),
        menu_bottom_offset: 66.0,
        dock_warp: false,
        dark_chrome: false,
        glass_chrome: false,
    },
    StyleSpec {
        style: DesktopStyle::Windows2000,
        tiling: false,
        reserved_height: 34.0, title_height: 20.0,
        frame_inset: 3.0, rounding: 0.0, chrome_radius: 0.0,
        frame_width: 3.0, caption_width: 16.0, shelf_radius: 0.0, resize_bar: 0.0, shadow: 0.0,
        glass_shelf: false, composes: false, caption_mac: false, bevel_classic: true, bevel_next: false,
        ground: ((0, 128, 128), (0, 128, 128)),
        ground_dark: ((0, 128, 128), (0, 128, 128)),
        menu_bottom_offset: 34.0,
        dock_warp: false,
        dark_chrome: false,
        glass_chrome: false,
    },
    StyleSpec {
        style: DesktopStyle::NextStep,
        tiling: false,
        reserved_height: 0.0, title_height: 22.0,
        frame_inset: 1.0, rounding: 0.0, chrome_radius: 0.0,
        frame_width: 1.0, caption_width: 14.0, shelf_radius: 0.0, resize_bar: 8.0, shadow: 0.0,
        glass_shelf: false, composes: false, caption_mac: false, bevel_classic: false, bevel_next: true,
        ground: ((85, 85, 85), (85, 85, 85)),
        ground_dark: ((85, 85, 85), (85, 85, 85)),
        menu_bottom_offset: 0.0,
        dock_warp: false,
        dark_chrome: false,
        glass_chrome: false,
    },
    // Phone modes draw no desktop chrome; every geometry number is 0.
    StyleSpec {
        style: DesktopStyle::Ios,
        tiling: false,
        reserved_height: 0.0, title_height: 0.0,
        frame_inset: 0.0, rounding: 0.0, chrome_radius: 0.0,
        frame_width: 0.0, caption_width: 0.0, shelf_radius: 0.0, resize_bar: 0.0, shadow: 0.0,
        glass_shelf: false, composes: false, caption_mac: false, bevel_classic: false, bevel_next: false,
        ground: ((38, 78, 137), (159, 207, 227)),
        ground_dark: ((38, 78, 137), (159, 207, 227)),
        menu_bottom_offset: 0.0,
        dock_warp: false,
        dark_chrome: false,
        glass_chrome: false,
    },
    StyleSpec {
        style: DesktopStyle::Android,
        tiling: false,
        reserved_height: 0.0, title_height: 0.0,
        frame_inset: 0.0, rounding: 0.0, chrome_radius: 0.0,
        frame_width: 0.0, caption_width: 0.0, shelf_radius: 0.0, resize_bar: 0.0, shadow: 0.0,
        glass_shelf: false, composes: false, caption_mac: false, bevel_classic: false, bevel_next: false,
        ground: ((50, 46, 73), (158, 156, 204)),
        ground_dark: ((50, 46, 73), (158, 156, 204)),
        menu_bottom_offset: 0.0,
        dock_warp: false,
        dark_chrome: false,
        glass_chrome: false,
    },
    // MakeOS floats like macOS: its dock overlays the desk rather than
    // reserving a strip, and the title bar and menu share macOS's placement.
    // Dark only, so both grounds are the same night gradient.
    // The row owns the window geometry (rounding, frame_inset, shadow); the
    // sheet's material block owns the kit's surfaces (cards, the shelf pill,
    // the ring's look). The two sets of numbers are kept in agreement by
    // hand: rounding 12 is the material's corner_radius, shadow 0.44 its
    // shadow_alpha, frame_inset 2 is twice its border_width. The pane
    // derives its child inset from material.border_width where a window
    // uses the row's frame_inset; both round the child to "outer radius
    // minus inset, halved".
    StyleSpec {
        style: DesktopStyle::MakeOs,
        tiling: false,
        reserved_height: 0.0, title_height: 32.0,
        frame_inset: 2.0, rounding: 12.0, chrome_radius: 10.0,
        frame_width: 1.0, caption_width: 30.0, shelf_radius: 24.0, resize_bar: 0.0, shadow: 0.44,
        glass_shelf: true, composes: true, caption_mac: true, bevel_classic: false, bevel_next: false,
        ground: ((11, 18, 32), (5, 7, 14)),
        ground_dark: ((11, 18, 32), (5, 7, 14)),
        menu_bottom_offset: 98.0,
        dock_warp: true,
        dark_chrome: true,
        glass_chrome: true,
    },
];

/// The chrome appearance a style draws: dark by identity (its row's
/// `dark_chrome`), else the appearance flag when the style has a dark look
/// at all — `supports_dark()` is false for MakeOS, so the flag alone would
/// read it as light.
pub fn dark_chrome(style: DesktopStyle, dark: bool) -> bool {
    SPECS[style as usize].dark_chrome || (style.supports_dark() && dark)
}

#[derive(Clone, Debug)]
pub struct StyleTween {
    pub target: DesktopStyle,
    pub dark: bool,
    pub weights: [f64; 8],
    from: [f64; 8],
    elapsed: f64,
}
impl Default for StyleTween {
    fn default() -> Self {
        Self {
            target: DesktopStyle::Omarchy,
            dark: false,
            weights: [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            from: [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            elapsed: 1.0,
        }
    }
}
impl StyleTween {
    pub fn select(&mut self, style: DesktopStyle) {
        self.from = self.weights;
        self.target = style;
        self.elapsed = 0.0;
    }
    pub fn step(&mut self, dt: f64) -> bool {
        self.elapsed = (self.elapsed + dt / 0.65).min(1.0);
        let t = self.elapsed * self.elapsed * (3.0 - 2.0 * self.elapsed);
        for i in 0..self.weights.len() {
            self.weights[i] = self.from[i]
                + ((if i == self.target as usize { 1.0 } else { 0.0 }) - self.from[i]) * t;
        }
        self.elapsed < 1.0
    }
    pub fn active(&self) -> bool {
        self.elapsed < 1.0
    }
    /// Blend a spec field by the current weights (the literal arrays' sum).
    pub fn mix(&self, f: impl Fn(&StyleSpec) -> f64) -> f64 {
        self.weights.iter().zip(SPECS.iter()).map(|(w, s)| w * f(s)).sum()
    }
    /// The weight of the styles for which `f` holds (a family predicate as a
    /// 0..1 mix), exact by construction: the terms are ×1.0 and +0.0.
    pub fn share(&self, f: impl Fn(&StyleSpec) -> bool) -> f64 {
        self.mix(|s| if f(s) { 1.0 } else { 0.0 })
    }
    /// The row the tween is heading to.
    pub fn spec(&self) -> &'static StyleSpec {
        &SPECS[self.target as usize]
    }
    pub fn reserved_height(&self) -> f64 {
        self.mix(|s| s.reserved_height)
    }
    pub fn title_height(&self) -> f64 {
        self.mix(|s| s.title_height)
    }
    /// The window shadow's opacity: the table's focused value, scaled to the
    /// 0.16 an unfocused window always had against the 0.28 of the styles that
    /// cast one, so those styles read the same numbers as before and MakeOS
    /// gets its own darker 0.44.
    pub fn window_shadow_opacity(&self, focus: f64, fade: f64) -> f32 {
        (self.mix(|s| s.shadow) * fade * if focus > 0.5 { 1.0 } else { 0.16 / 0.28 }) as f32
    }
    /// The glass chrome's share of the floating chrome: how far the glass
    /// frame has taken over from the title fill, the sheet's roles from the
    /// ink — 1 settled, `glass_chrome / (1 - tiling)` on the way in from a
    /// tiled desk so the crossfade follows the chrome's own opacity, and 0
    /// when there is no glass material to paint the frame with.
    pub fn glass_share(&self, is_glass: bool) -> f64 {
        let floating = 1.0 - self.share(|s| s.tiling);
        if !is_glass || floating <= 0.001 {
            0.0
        } else {
            self.share(|s| s.glass_chrome) / floating
        }
    }
    /// The child inset from the tile rect. A glass-chrome row's inset is the
    /// room for its ring, so it counts only while there is a glass material
    /// to draw one; without it the body sits flush, as it does under macOS.
    pub fn frame_inset(&self, is_glass: bool) -> f64 {
        self.mix(|s| if s.glass_chrome && !is_glass { 0.0 } else { s.frame_inset })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_switch_starts_from_the_visible_mix_and_lands_exactly() {
        let mut t = StyleTween::default();
        t.select(DesktopStyle::Macos);
        t.step(0.2);
        let old = t.weights;
        t.select(DesktopStyle::Windows2000);
        t.step(0.0);
        assert_eq!(t.weights, old);
        t.step(1.0);
        assert_eq!(t.weights, [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
    }
    #[test]
    fn specs_reproduce_the_literal_arrays_for_every_style() {
        // The arrays these replace, verbatim from the pre-refactor code, padded
        // with the zeros the phone rows must hold for a tween into them to land,
        // then the MakeOS row appended after the table existed.
        let reserved = [0.0, 0.0, 54.0, 34.0, 0.0, 0.0, 0.0, 0.0];
        let title = [0.0, 32.0, 34.0, 20.0, 22.0, 0.0, 0.0, 32.0];
        let inset = [2.0, 0.0, 0.0, 3.0, 1.0, 0.0, 0.0, 2.0]; // BORDER_SIZE = 2.0
        let rounding = [0.0, 14.0, 8.0, 0.0, 0.0, 0.0, 0.0, 12.0];
        let chrome_radius = [0.0, 10.0, 8.0, 0.0, 0.0, 0.0, 0.0, 10.0];
        let frame_width = [2.0, 2.0, 2.0, 3.0, 1.0, 0.0, 0.0, 1.0];
        let caption_width = [30.0, 30.0, 46.0, 16.0, 14.0, 0.0, 0.0, 30.0];
        // MakeOS's 24 is macOS's frosted pill as seen: that shader takes its
        // DSL corner_radius of 12 as the SDF radius raw, the kit and the
        // chrome halve a visual one.
        let shelf_radius = [0.0, 18.0, 0.0, 0.0, 0.0, 0.0, 0.0, 24.0];
        for (i, style) in DesktopStyle::ALL.iter().enumerate() {
            let s = &SPECS[i];
            assert_eq!(s.style, *style);
            assert_eq!(s.reserved_height, reserved[i]);
            assert_eq!(s.title_height, title[i]);
            assert_eq!(s.frame_inset, inset[i]);
            assert_eq!(s.rounding, rounding[i]);
            assert_eq!(s.chrome_radius, chrome_radius[i]);
            assert_eq!(s.frame_width, frame_width[i]);
            assert_eq!(s.caption_width, caption_width[i]);
            assert_eq!(s.shelf_radius, shelf_radius[i]);
            assert_eq!(s.title_height, style.title_height(), "wm spec mirrors widgets");
        }
        // The table index is the enum discriminant, which is what the weights index.
        assert_eq!(SPECS.len(), DesktopStyle::ALL.len());
        assert!(SPECS.iter().enumerate().all(|(i, s)| s.style as usize == i));
        // Family predicates the weights[i] sites encode.
        assert!(SPECS[0].tiling && !SPECS[1].tiling);
        assert!(SPECS[1].glass_shelf && SPECS[1].composes && SPECS[1].shadow > 0.0 && SPECS[1].caption_mac);
        assert!(SPECS[2].shadow > 0.0 && !SPECS[2].glass_shelf);
        assert!(SPECS[3].bevel_classic && SPECS[4].bevel_next && SPECS[4].resize_bar == 8.0);
        assert!(SPECS[7].glass_shelf && SPECS[7].composes && SPECS[7].shadow > 0.0 && SPECS[7].caption_mac && !SPECS[7].tiling);
        // The rest of the row, from the match arms it centralises.
        assert_eq!(SPECS.map(|s| s.menu_bottom_offset), [0.0, 98.0, 66.0, 34.0, 0.0, 0.0, 0.0, 98.0]);
        assert_eq!(SPECS.map(|s| s.dock_warp), [false, true, false, false, false, false, false, true]);
        assert_eq!(SPECS.map(|s| s.dark_chrome), [false, false, false, false, false, false, false, true]);
        assert_eq!(SPECS.map(|s| s.glass_chrome), [false, false, false, false, false, false, false, true]);
        // The shadow reads the table: bit-neutral at the f32 the uniform takes
        // for the styles that had 0.28 / 0.16, MakeOS's own 0.44 above them.
        for (style, focused, unfocused) in [
            (DesktopStyle::Macos, 0.28f32, 0.16f32),
            (DesktopStyle::Windows, 0.28, 0.16),
            (DesktopStyle::MakeOs, 0.44, (0.44 * (0.16 / 0.28)) as f32),
        ] {
            let mut t = StyleTween::default();
            t.select(style);
            t.step(1.0);
            assert_eq!(t.window_shadow_opacity(1.0, 1.0), focused, "{style:?}");
            assert_eq!(t.window_shadow_opacity(0.0, 1.0), unfocused, "{style:?}");
            assert_eq!(t.window_shadow_opacity(1.0, 0.5), focused * 0.5, "{style:?} fades");
        }
        assert!((SPECS[7].shadow * (0.16 / 0.28) - 0.2514).abs() < 1e-3);
        assert_eq!(SPECS.map(|s| s.ground), [
            ((16, 19, 21), (24, 30, 34)),
            ((39, 43, 87), (171, 109, 131)),
            ((10, 45, 108), (24, 137, 210)),
            ((0, 128, 128), (0, 128, 128)),
            ((85, 85, 85), (85, 85, 85)),
            ((38, 78, 137), (159, 207, 227)),
            ((50, 46, 73), (158, 156, 204)),
            ((11, 18, 32), (5, 7, 14)),
        ]);
        assert_eq!(SPECS.map(|s| s.ground_dark), [
            ((16, 19, 21), (24, 30, 34)),
            ((12, 15, 36), (65, 36, 69)),
            ((10, 19, 34), (21, 49, 72)),
            ((0, 128, 128), (0, 128, 128)),
            ((85, 85, 85), (85, 85, 85)),
            ((38, 78, 137), (159, 207, 227)),
            ((50, 46, 73), (158, 156, 204)),
            ((11, 18, 32), (5, 7, 14)),
        ]);
        // Blending is the same sum the arrays gave, mid-tween into a desktop
        // style and into a phone one; the two heights are bit-exact.
        for target in [DesktopStyle::Windows, DesktopStyle::Ios] {
            let mut t = StyleTween::default();
            t.select(target);
            t.step(0.3);
            let w = t.weights;
            let expect: f64 = w.iter().zip(reserved).map(|(w, v)| w * v).sum();
            assert_eq!(t.reserved_height(), expect);
            let expect: f64 = w.iter().zip(title).map(|(w, v)| w * v).sum();
            assert_eq!(t.title_height(), expect);
            let expect: f64 = w.iter().zip(rounding).map(|(w, v)| w * v).sum();
            assert!((t.mix(|s| s.rounding) - expect).abs() < 1e-12);
            assert_eq!(t.share(|s| s.tiling), w[0]);
        }
    }
    #[test]
    fn the_glass_share_is_makeos_within_the_floating_chrome() {
        let mut t = StyleTween::default();
        t.select(DesktopStyle::MakeOs);
        t.step(1.0);
        assert_eq!(t.glass_share(true), 1.0, "settled MakeOS");
        assert_eq!(t.glass_share(false), 0.0, "no glass material, no glass frame");
        t.select(DesktopStyle::Macos);
        t.step(1.0);
        assert_eq!(t.glass_share(true), 0.0, "settled macOS");
        // Into MakeOS from the tiled desk: the share follows the chrome's own
        // opacity, which is what fades in — never more than 1.
        let mut t = StyleTween::default();
        t.select(DesktopStyle::MakeOs);
        t.step(0.3);
        let makeos = t.weights[DesktopStyle::MakeOs as usize];
        let tiling = t.weights[DesktopStyle::Omarchy as usize];
        assert!(makeos > 0.0 && makeos < 1.0);
        assert!((t.glass_share(true) - makeos / (1.0 - tiling)).abs() < 1e-12);
        // From macOS, the floating share is already 1: the glass share is the
        // MakeOS weight itself.
        let mut t = StyleTween::default();
        t.select(DesktopStyle::Macos);
        t.step(1.0);
        t.select(DesktopStyle::MakeOs);
        t.step(0.3);
        assert!((t.glass_share(true) - t.weights[DesktopStyle::MakeOs as usize]).abs() < 1e-12);
    }
    #[test]
    fn a_glass_chrome_inset_needs_a_glass_material() {
        // MakeOS's 2 px is the room for its ring: gone without a glass
        // material. The other rows' insets do not read the material at all.
        for (style, glass, flat) in [
            (DesktopStyle::MakeOs, 2.0, 0.0),
            (DesktopStyle::Macos, 0.0, 0.0),
            (DesktopStyle::NextStep, 1.0, 1.0),
        ] {
            let mut t = StyleTween::default();
            t.select(style);
            t.step(1.0);
            assert_eq!(t.frame_inset(true), glass, "{style:?} under glass");
            assert_eq!(t.frame_inset(false), flat, "{style:?} flat");
        }
    }
    #[test]
    fn shelf_geometry_reads_the_table() {
        // The shelf rect the five-wide literal arrays placed, verbatim, so the
        // table keeps every shelf where it was: settled on each desktop style,
        // and part way into one.
        let screen = rect(0.0, 0.0, 1440.0, 900.0);
        let n = 6;
        let old = |t: &StyleTween| {
            let dock_width = (((n + 1) as f64) * 62.0 + 20.0).min((screen.size.x - 24.0).max(1.0));
            let next_height = ((n + 1) as f64 * 56.0).min((screen.size.y - 48.0).max(1.0));
            let value = |v: [f64; 5]| -> f64 { t.weights.iter().zip(v).map(|(w, v)| w * v).sum() };
            rect(
                screen.pos.x + value([8.0, (screen.size.x - dock_width) * 0.5, 0.0, 0.0, screen.size.x - 64.0]),
                screen.pos.y + value([0.0, screen.size.y - 88.0, screen.size.y - 54.0, screen.size.y - 34.0, 40.0]),
                value([32.0, dock_width, screen.size.x, screen.size.x, 56.0]),
                value([0.0, 78.0, 54.0, 34.0, next_height]),
            )
        };
        let mut t = StyleTween::default();
        for style in [
            DesktopStyle::Omarchy,
            DesktopStyle::Macos,
            DesktopStyle::Windows,
            DesktopStyle::Windows2000,
            DesktopStyle::NextStep,
        ] {
            t.select(style);
            t.step(1.0);
            assert_eq!(shelf_geometry(screen, &t, n), old(&t), "{style:?}");
        }
        t.select(DesktopStyle::Macos);
        t.step(0.3);
        assert_eq!(shelf_geometry(screen, &t, n), old(&t), "mid-tween");
        // Into a phone row the old five-wide zip stopped short of the weight
        // that is growing; the eight-term mix multiplies it by the row's zeros.
        t.select(DesktopStyle::Ios);
        t.step(0.3);
        assert_eq!(shelf_geometry(screen, &t, n), old(&t), "into a phone row");
        // MakeOS's dock is macOS's dock, settled: same rect from the same arms.
        t.select(DesktopStyle::Macos);
        t.step(1.0);
        let mac = shelf_geometry(screen, &t, n);
        t.select(DesktopStyle::MakeOs);
        t.step(1.0);
        assert_eq!(shelf_geometry(screen, &t, n), mac, "settled MakeOS");
    }
    #[test]
    fn makeos_chrome_is_dark_by_identity_and_the_others_follow_the_flag() {
        assert!(dark_chrome(DesktopStyle::MakeOs, false));
        assert!(dark_chrome(DesktopStyle::MakeOs, true));
        for style in [DesktopStyle::Macos, DesktopStyle::Windows, DesktopStyle::Ios, DesktopStyle::Android] {
            assert!(dark_chrome(style, true), "{style:?}");
            assert!(!dark_chrome(style, false), "{style:?}");
        }
        // No dark look at all: the flag is ignored, as it always was.
        for style in [DesktopStyle::Omarchy, DesktopStyle::Windows2000, DesktopStyle::NextStep] {
            assert!(!dark_chrome(style, true), "{style:?}");
        }
    }
    #[test]
    fn the_shelf_glass_is_split_between_the_frosted_and_the_liquid_pill() {
        let mut t = StyleTween::default();
        assert_eq!(shelf_glass_split(&t, false), (0.0, 0.0), "Omarchy");
        assert_eq!(shelf_glass_split(&t, true), (0.0, 0.0), "Omarchy, glass material");
        t.select(DesktopStyle::Macos);
        t.step(1.0);
        assert_eq!(shelf_glass_split(&t, false), (1.0, 0.0), "settled macOS");
        t.select(DesktopStyle::MakeOs);
        t.step(0.3);
        let (frosted, makeos) = shelf_glass_split(&t, true);
        assert!(frosted > 0.0 && makeos > 0.0, "mid-tween both pills are up");
        assert!((frosted + makeos - t.share(|s| s.glass_shelf)).abs() < 1e-12, "one pill's worth of glass");
        t.step(1.0);
        assert_eq!(shelf_glass_split(&t, true), (0.0, 1.0), "settled MakeOS, glass");
        assert_eq!(shelf_glass_split(&t, false), (1.0, 0.0), "settled MakeOS, flat: the frosted pill stands in");
    }
    #[test]
    fn the_dock_backdrop_is_as_deep_as_the_pill_that_samples_it() {
        let flat = MaterialTokens::default();
        let glass = MaterialTokens { glass: 1.0, blur_level: 5.2, ..flat };
        let shallow = MaterialTokens { glass: 1.0, blur_level: 3.0, ..flat };
        let mut t = StyleTween::default();
        t.select(DesktopStyle::MakeOs);
        t.step(1.0);
        assert_eq!(dock_backdrop_level(&t, &glass), 5.2);
        assert_eq!(dock_backdrop_level(&t, &shallow), FROSTED_SHELF_BLUR_LEVEL, "never shallower than the frosted pill");
        assert_eq!(dock_backdrop_level(&t, &flat), FROSTED_SHELF_BLUR_LEVEL, "flat material: the frosted pill");
        t.select(DesktopStyle::Macos);
        t.step(1.0);
        assert_eq!(dock_backdrop_level(&t, &glass), FROSTED_SHELF_BLUR_LEVEL);
        assert_eq!(dock_backdrop_level(&t, &flat), FROSTED_SHELF_BLUR_LEVEL);
    }
}

use crate::desk::WmState;
use crate::hub::ClientId;
use crate::shell::{
    alpha, rgb, MaterialTokens,
    ui::{rect, HAlign, Ico, ShellDraw},
};
use makepad_widgets::app_icon::AppIconDraw;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Script, ScriptHook)]
#[repr(u32)]
pub enum MacCaption {
    #[pick]
    None = 0,
    Close = 1,
    Minimize = 2,
    Maximize = 3,
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*
    let MacCaption = set_type_default() do #(MacCaption::script_api(vm))
    set_type_default() do #(DrawDesktopChrome::script_shader(vm)) {
        ..mod.draw.DrawQuad
        color: #eeeeee
        radius: 6.0
        bevel: 0.0
        pressed: 0.0
        selected: 0.0
        top_only: 0.0
        title_gradient: 0.0
        title_gradient_end: #a6caf0
        frame_width: 0.0
        caption: MacCaption.None
        caption_ink: #0000
        pixel: fn() {
            let p = self.pos * self.rect_size
            let sdf = Sdf2d.viewport(p)
            // The content quad has hard, pixel-aligned edges. Extend the
            // title's SDF by half a device pixel so its straight edges and
            // bottom junction cover the same pixels without a dark AA seam.
            let bleed = if self.top_only > 0.5 {0.5 / self.draw_pass.dpi_factor} else {0.0}
            sdf.box_y(-bleed, -bleed, self.rect_size.x+2.0*bleed, self.rect_size.y+2.0*bleed, self.radius*0.5, self.radius*0.5*(1.0-self.top_only))
            if self.frame_width > 0.0 {
                sdf.rect(self.frame_width, self.frame_width, self.rect_size.x - 2.0*self.frame_width, self.rect_size.y - 2.0*self.frame_width)
                sdf.subtract()
            }
            let pixel=floor(p*self.draw_pass.dpi_factor)
            let checker=modf(pixel.x+pixel.y,2.0)
            let fill=mix(self.color.rgb,vec3(1.0),self.selected*checker)
            let base=mix(fill,self.title_gradient_end.rgb,self.pos.x*self.title_gradient)
            let tl=min(p.x,p.y)
            let br=min(self.rect_size.x-p.x,self.rect_size.y-p.y)
            let outer=if tl<br {mix(vec3(0.831,0.816,0.784),vec3(0.502),self.pressed)}else{mix(vec3(0.251),vec3(1.0),self.pressed)}
            let inner=if tl<br {mix(vec3(1.0),vec3(0.251),self.pressed)}else{mix(vec3(0.502),vec3(0.831,0.816,0.784),self.pressed)}
            let distance=min(tl,br)
            let edge=if distance<1.0 {outer}else{inner}
            let color=mix(base,edge,(1.0-step(2.0,distance))*self.bevel)
            // Window titles are rectangular content inside the complete
            // window's compositor mask. A second SDF coverage ramp here
            // exposes the wallpaper along the join with the app surface.
            if self.top_only > 0.5 && self.radius == 0.0 {
                return vec4(color*self.color.w,self.color.w)
            }
            sdf.fill(vec4(color,self.color.w))
            let c = self.rect_size * 0.5
            match self.caption {
                MacCaption.Close => {
                    sdf.move_to(c.x-2.8,c.y-2.8)
                    sdf.line_to(c.x+2.8,c.y+2.8)
                    sdf.move_to(c.x-2.8,c.y+2.8)
                    sdf.line_to(c.x+2.8,c.y-2.8)
                    sdf.stroke(self.caption_ink,1.35)
                }
                MacCaption.Minimize => {
                    sdf.move_to(c.x-3.25,c.y)
                    sdf.line_to(c.x+3.25,c.y)
                    sdf.stroke(self.caption_ink,1.5)
                }
                MacCaption.Maximize => {
                    sdf.move_to(c.x-3.1,c.y-0.9)
                    sdf.line_to(c.x+0.9,c.y+3.1)
                    sdf.line_to(c.x-3.1,c.y+3.1)
                    sdf.close_path()
                    sdf.fill(self.caption_ink)
                    sdf.move_to(c.x+3.1,c.y+0.9)
                    sdf.line_to(c.x-0.9,c.y-3.1)
                    sdf.line_to(c.x+3.1,c.y-3.1)
                    sdf.close_path()
                    sdf.fill(self.caption_ink)
                }
                _ => {}
            }
            return sdf.result
        }
    }
    mod.widgets.DesktopShelfBase = #(DesktopShelf::register_widget(vm))
    mod.widgets.DesktopShelf = set_type_default() do mod.widgets.DesktopShelfBase {
        width: Fill height: Fill
        d +: {text.text_style: theme.font_regular text_bold.text_style: theme.font_bold}
        chrome +: {}
        glass: GlassPanel {
            width: Fill height: Fill
            draw_bg +: {
                blur_level: #(FROSTED_SHELF_BLUR_LEVEL)
                corner_radius: 12.0
                lensing_strength: 0.0
                diffraction_strength: 0.0
                tint_color: #dddded
                tint_alpha: 0.22
                surface_alpha: 0.88
                specular_strength: 0.025
                border_alpha: 0.20
                border_width: 0.65
                shadow_radius: 10.0
                shadow_color: #0005
            }
        }
    }
}
#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawDesktopChrome {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    pub color: Vec4f,
    #[live]
    pub radius: f32,
    #[live]
    pub bevel: f32,
    #[live]
    pub pressed: f32,
    #[live]
    pub selected: f32,
    #[live]
    pub top_only: f32,
    #[live]
    pub title_gradient: f32,
    #[live]
    pub title_gradient_end: Vec4f,
    #[live]
    pub frame_width: f32,
    #[live]
    pub caption: MacCaption,
    #[live]
    pub caption_ink: Vec4f,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ShelfHit {
    Launcher,
    App(String),
    Window(ClientId),
    ShowDesktop,
}
#[derive(Script, ScriptHook, Widget)]
pub struct DesktopShelf {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[redraw]
    #[live]
    chrome: DrawDesktopChrome,
    #[rust]
    app_icons: AppIconDraw,
    #[rust]
    window_apps: HashMap<ClientId, String>,
    #[rust]
    active_window: Option<ClientId>,
    #[live]
    d: ShellDraw,
    #[live]
    glass: WidgetRef,
    #[redraw]
    #[rust]
    area: Area,
    #[rust]
    hits: Vec<(Rect, ShelfHit)>,
    #[rust]
    bounds: Rect,
    #[rust]
    overlay: Option<DrawList2d>,
    #[rust]
    hover: Option<ShelfHit>,
    /// The appearance flag as the tween carries it; `button()` resolves it
    /// per style through `dark_chrome`.
    #[rust]
    dark: bool,
    /// The appearance the frosted pill was last tinted for.
    #[rust]
    tint_dark: bool,
    #[rust]
    hover_mix: Vec<(ShelfHit, f64)>,
    #[rust]
    hover_frame: NextFrame,
    #[rust]
    hover_time: f64,
}
impl DesktopShelf {
    pub fn hit(&self, p: Vec2d) -> Option<ShelfHit> {
        self.hits
            .iter()
            .find(|(r, _)| r.contains(p))
            .map(|(_, h)| h.clone())
    }
    pub fn contains(&self, p: Vec2d) -> bool {
        self.bounds.contains(p) && self.bounds.size.y > 0.0
    }
    pub fn hover_at(&mut self, cx: &mut Cx, p: Vec2d) {
        let h = self.hit(p);
        if h != self.hover {
            self.hover = h;
            self.hover_time = 0.0;
            self.hover_frame = cx.new_next_frame();
            self.redraw(cx);
        }
    }
    fn button(
        &mut self,
        cx: &mut Cx2d,
        r: Rect,
        hit: ShelfHit,
        _icon: Ico,
        label: &str,
        running: bool,
        style: DesktopStyle,
        opacity: f32,
    ) {
        let hover = self.hover.as_ref() == Some(&hit);
        let amount = self
            .hover_mix
            .iter()
            .find(|(h, _)| h == &hit)
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let mac = style.mac_family();
        let classic = style == DesktopStyle::Windows2000;
        let selected = classic && matches!(&hit, ShelfHit::Window(c) if Some(*c) == self.active_window);
        let inset = if selected { 1.0 } else { 0.0 };
        self.chrome.pressed = if selected { 1.0 } else { 0.0 };
        self.chrome.selected = self.chrome.pressed;
        let ink = alpha(
            if dark_chrome(style, self.dark) {
                rgb(240, 240, 245)
            } else {
                rgb(28, 30, 36)
            },
            opacity,
        );
        if !mac && style != DesktopStyle::NextStep && (classic || hover) {
            self.chrome.radius = if classic { 0.0 } else { 5.0 };
            self.chrome.bevel = if classic { 1.0 } else { 0.0 };
            self.chrome.color = alpha(
                if classic {
                    rgb(212, 208, 200)
                } else {
                    rgb(255, 255, 255)
                },
                opacity * if classic { 1.0 } else { 0.4 },
            );
            self.chrome.draw_abs(cx, r);
        }
        let icon_rect = if classic {
            rect(r.pos.x + 4.0 + inset, r.pos.y + inset, 22.0, r.size.y)
        } else {
            rect(
                r.pos.x,
                r.pos.y - if mac { 7.0 * amount } else { 0.0 },
                r.size.x,
                if mac { 50.0 } else { r.size.y },
            )
        };
        let app_id = match &hit {
            ShelfHit::App(app) => app.as_str(),
            ShelfHit::Window(client) => self
                .window_apps
                .get(client)
                .map(String::as_str)
                .unwrap_or("app"),
            ShelfHit::Launcher => "applications",
            _ => "app",
        };
        let size = if style == DesktopStyle::NextStep { r.size.x.min(r.size.y) } else if mac {
            54.0 + 8.0 * amount
        } else if classic {
            16.0
        } else {
            26.0
        };
        let icon_box = if mac {
            mac_icon_box(r, amount)
        } else {
            rect(
                icon_rect.pos.x + (icon_rect.size.x - size) * 0.5,
                icon_rect.pos.y + (icon_rect.size.y - size) * 0.5,
                size,
                size,
            )
        };
        self.app_icons
            .draw(cx, app_id, style, icon_box, opacity, ink);
        if classic {
            self.d.label_elided(
                cx,
                rect(
                    r.pos.x + 28.0 + inset,
                    r.pos.y + inset,
                    (r.size.x - 32.0).max(1.0),
                    r.size.y,
                ),
                selected,
                12.0,
                ink,
                HAlign::Left,
                label,
            );
        }
        if running && !classic && style != DesktopStyle::NextStep {
            self.chrome.radius = 2.0;
            self.chrome.bevel = 0.0;
            self.chrome.color = if mac {
                alpha(rgb(220, 220, 230), opacity * 0.82)
            } else {
                ink
            };
            self.chrome.draw_abs(
                cx,
                rect(
                    r.pos.x + r.size.x * 0.5 - 1.8,
                    if mac { mac_icon_box(r, 0.0).pos.y + 59.0 } else { r.pos.y + r.size.y - 4.0 },
                    3.6,
                    3.6,
                ),
            );
        }
        if hover && !classic && style != DesktopStyle::NextStep {
            let width = (label.len() as f64 * 7.0 + 22.0).max(60.0);
            let tip = rect(
                r.pos.x + (r.size.x - width) * 0.5,
                r.pos.y - 35.0,
                width,
                25.0,
            );
            self.chrome.color = alpha(
                if dark_chrome(style, self.dark) {
                    rgb(48, 48, 52)
                } else {
                    rgb(240, 240, 244)
                },
                opacity,
            );
            self.chrome.radius = 6.0;
            self.chrome.bevel = 0.0;
            self.chrome.draw_abs(cx, tip);
            self.d
                .label(cx, tip, false, 12.0, ink, HAlign::Center, label);
        }
        self.hits.push((r, hit));
    }
}
pub fn app_icon(id: &str) -> Ico {
    match id {
        "browser" => Ico::Globe,
        "photos" | "image" => Ico::Photo,
        "terminal" => Ico::Keyboard,
        "files" => Ico::Menu,
        "sheets" => Ico::Calendar,
        "task" => Ico::Pulse,
        "video" => Ico::Play,
        "pdf" => Ico::Check,
        "mixer" => Ico::Speaker,
        "vj" => Ico::Headphone,
        "score" => Ico::Bell,
        "route" => Ico::Globe,
        "fabric" => Ico::Shirt,
        "fab" => Ico::Refresh,
        "studio" => Ico::Moon,
        _ => Ico::Monitor,
    }
}
fn shelf_geometry(screen: Rect, style: &StyleTween, app_count: usize) -> Rect {
    let dock_width = (((app_count + 1) as f64) * 62.0 + 20.0).min((screen.size.x - 24.0).max(1.0));
    let next_height = ((app_count + 1) as f64 * 56.0).min((screen.size.y - 48.0).max(1.0));
    // Each shelf's placement depends on the screen, so it is a match per
    // style rather than a table number; the phone rows have no shelf.
    rect(
        screen.pos.x + style.mix(|s| match s.style {
            DesktopStyle::Omarchy => 8.0,
            DesktopStyle::Macos | DesktopStyle::MakeOs => (screen.size.x - dock_width) * 0.5,
            DesktopStyle::Windows | DesktopStyle::Windows2000 => 0.0,
            DesktopStyle::NextStep => screen.size.x - 64.0,
            DesktopStyle::Ios | DesktopStyle::Android => 0.0,
        }),
        screen.pos.y + style.mix(|s| match s.style {
            DesktopStyle::Omarchy => 0.0,
            DesktopStyle::Macos | DesktopStyle::MakeOs => screen.size.y - 88.0,
            DesktopStyle::Windows => screen.size.y - 54.0,
            DesktopStyle::Windows2000 => screen.size.y - 34.0,
            DesktopStyle::NextStep => 40.0,
            DesktopStyle::Ios | DesktopStyle::Android => 0.0,
        }),
        style.mix(|s| match s.style {
            DesktopStyle::Omarchy => 32.0,
            DesktopStyle::Macos | DesktopStyle::MakeOs => dock_width,
            DesktopStyle::Windows | DesktopStyle::Windows2000 => screen.size.x,
            DesktopStyle::NextStep => 56.0,
            DesktopStyle::Ios | DesktopStyle::Android => 0.0,
        }),
        style.mix(|s| match s.style {
            DesktopStyle::Omarchy => 0.0,
            DesktopStyle::Macos | DesktopStyle::MakeOs => 78.0,
            DesktopStyle::Windows => 54.0,
            DesktopStyle::Windows2000 => 34.0,
            DesktopStyle::NextStep => next_height,
            DesktopStyle::Ios | DesktopStyle::Android => 0.0,
        }),
    )
}

/// Center the icon and its running indicator as one group inside each dock cell.
fn mac_icon_box(cell: Rect, hover: f64) -> Rect {
    let size = 54.0 + 8.0 * hover;
    let top = cell.pos.y + (cell.size.y - 63.0) * 0.5;
    rect(cell.pos.x + (cell.size.x-size)*0.5, top - 8.0*hover, size, size)
}

/// The compositor samples exactly the shelf it will paint this frame, including
/// an interrupted style tween. Glass outside the dock cannot add a blur stack.
fn dock_app_ids(state: &WmState) -> Vec<String> {
    let mut apps: Vec<_> = crate::shell::launcher::apps()
        .into_iter().filter(|app| !app.disabled).map(|app| app.id).collect();
    for client in state.layout.clients_on(state.layout.active) {
        if let Some(client) = state.clients.get(&client) {
            let id=format!("apps.{}",client.app);
            if !apps.contains(&id) {apps.push(id);}
        }
    }
    apps
}
pub fn dock_bounds(state: &WmState, size: Vec2d) -> Rect {
    shelf_geometry(rect(0.0,0.0,size.x,size.y), &state.style, dock_app_ids(state).len())
}
pub fn dock_icon_bounds(state: &WmState, size: Vec2d, app: &str) -> Rect {
    let apps=dock_app_ids(state);
    let dock=shelf_geometry(rect(0.0,0.0,size.x,size.y), &state.style, apps.len());
    let slot=apps.iter().position(|id| id==&format!("apps.{app}")).map(|i|i+1).unwrap_or(0);
    let cell=(dock.size.x-20.0)/(apps.len()+1) as f64;
    mac_icon_box(rect(dock.pos.x+10.0+slot as f64*cell,dock.pos.y+6.0,cell,dock.size.y-12.0), 0.0)
}

/// How the shelf's glass is shared between its two pills, `(frosted,
/// makeos)`: macOS's frosted `GaussRoundedView` and MakeOS's Liquid Glass
/// from the kit, which paints only under a glass material — a MakeOS sheet
/// without one keeps the frosted pill. `glass_shelf` is the weight of every
/// glass-shelf style, macOS's plus MakeOS's, so `frosted = glass_shelf -
/// makeos` is macOS's own weight — or the whole glass share when the
/// material is flat and the frosted pill stands in for MakeOS too. Through
/// a macOS<->MakeOS tween the two sum to one pill's worth of glass.
pub fn shelf_glass_split(t: &StyleTween, glass_material: bool) -> (f64, f64) {
    let glass_shelf = t.share(|s| s.glass_shelf);
    let makeos = if glass_material { t.share(|s| s.glass_chrome) } else { 0.0 };
    (glass_shelf - makeos, makeos)
}

/// Where the frosted pill samples the pyramid; the shelf's GlassPanel
/// `blur_level` in the DSL reads it too.
pub const FROSTED_SHELF_BLUR_LEVEL: f64 = 4.5;

/// The pyramid depth the desk renders for the dock. The compositor renders
/// floor(level)+1 mips; the frosted pill samples at the constant and the
/// Liquid Glass pill at the material's own level, so while that one is up
/// the request is the deeper of the two, or the kit would read mips that
/// were never rendered.
pub fn dock_backdrop_level(t: &StyleTween, m: &MaterialTokens) -> f64 {
    let (_, makeos) = shelf_glass_split(t, m.is_glass());
    if makeos > 0.001 { m.blur_level.max(FROSTED_SHELF_BLUR_LEVEL) } else { FROSTED_SHELF_BLUR_LEVEL }
}

impl Widget for DesktopShelf {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let screen = cx.turtle().rect();
        self.hits.clear();
        self.bounds = Rect::default();
        if let Some(state) = scope.data.get_mut::<WmState>() {
            self.active_window = state.layout.focused_client()
                .filter(|c| !state.layout.desktop.minimized(*c));
            self.window_apps = state
                .clients
                .iter()
                .map(|(c, s)| (*c, s.app.clone()))
                .collect();
            let t = &state.style;
            let style = t.target;
            self.dark = t.dark;
            // The target's chrome appearance: the flat pill's fill and the
            // frosted pill's tint. Only macOS's GaussRoundedView takes the
            // tint; MakeOS's Liquid Glass reads its own from the material.
            let chrome_dark = dark_chrome(style, t.dark);
            if self.tint_dark != chrome_dark {
                self.tint_dark = chrome_dark;
                let tint = if chrome_dark {
                    rgb(24, 24, 28)
                } else {
                    rgb(221, 221, 237)
                };
                let tint_alpha = if chrome_dark { 0.32 } else { 0.20 };
                script_apply_eval!(cx,self.glass,{draw_bg +: {tint_color: #(tint) tint_alpha: #(tint_alpha)}});
            }
            // The pill paints from the sheet the state carries, like every kit.
            self.d.set_material(state.material);
            let opacity = (1.0 - t.share(|s| s.tiling)) as f32;
            if opacity > 0.001 && !style.mobile() {
                let mut apps: Vec<_> = crate::shell::launcher::apps()
                    .into_iter()
                    .filter(|a| !a.disabled)
                    .collect();
                let clients: Vec<_> = state
                    .layout
                    .clients_on(state.layout.active)
                    .into_iter()
                    .filter_map(|c| {
                        state
                            .clients
                            .get(&c)
                            .map(|s| (c, s.app.clone(), s.display_title().to_string()))
                    })
                    .collect();
                for (_, app, title) in &clients {
                    if !apps.iter().any(|item| item.id == format!("apps.{app}")) {
                        apps.push(crate::shell::menu::MenuItem {
                            id: format!("apps.{app}"),
                            label: title.clone(),
                            icon: Some(app_icon(app)),
                            kind: crate::shell::menu::MenuKind::App,
                            checked: false,
                            disabled: false,
                            description: String::new(),
                            aliases: vec![app.clone()],
                        });
                    }
                }
                let n = (apps.len() + 1).max(1) as f64;
                let r = shelf_geometry(screen, t, apps.len());
                let (x, y, w, h) = (r.pos.x, r.pos.y, r.size.x, r.size.y);
                self.bounds = r;
                let glass_shelf = t.share(|s| s.glass_shelf);
                // One pill's worth of glass between the two pills.
                let (frosted, makeos) = shelf_glass_split(t, self.d.material().is_glass());
                // Window-backed Gaussian blur, sampled from the live desktop.
                if frosted > 0.01 {
                    if let Some(mut glass) = self.glass.borrow_mut::<gauss_view::GaussRoundedView>()
                    {
                        glass.draw_surface_with_backdrop(
                            cx,
                            r,
                            state.dock_backdrop.clone(),
                            frosted as f32,
                        );
                    }
                }
                // The same snapshot, rendered as deep as the material samples
                // (`dock_backdrop_level`). The radius is the table's visual
                // one: the kit and the chrome halve it for Sdf2d, while the
                // frosted GaussRoundedView takes its DSL corner_radius as
                // the SDF radius raw — MakeOS's row carries twice that, so
                // the pills a tween overlays share their corners. The quad
                // lands in this list, under the foreground overlay below.
                if makeos > 0.001 {
                    self.d.bind_snapshot(cx, state.dock_backdrop.clone());
                    self.d.glass_pill(cx, r, t.mix(|s| s.shelf_radius), makeos as f32);
                }
                // The pills draw in the scene list; the icons go to a
                // separate overlay list so they composite above the glass.
                // Opaque shelves stay in the scene's recording so closing a
                // child cannot detach them.
                let glass_foreground = glass_shelf > 0.001;
                if glass_foreground {
                    if self.overlay.is_none() {
                        self.overlay = Some(DrawList2d::new(cx));
                    }
                    self.overlay.as_mut().unwrap().begin_always(cx);
                }
                self.chrome.new_draw_call(cx);
                self.chrome.pressed = 0.0;
                self.chrome.selected = 0.0;
                self.chrome.color = alpha(
                    if style == DesktopStyle::Windows2000 {
                        rgb(212, 208, 200)
                    } else if chrome_dark {
                        rgb(32, 32, 32)
                    } else {
                        rgb(234, 238, 245)
                    },
                    opacity * (1.0 - glass_shelf) as f32,
                );
                self.chrome.radius = t.mix(|s| s.shelf_radius) as f32;
                self.chrome.bevel = t.share(|s| s.bevel_classic) as f32;
                self.chrome.draw_abs(cx, r);
                for style in [
                    DesktopStyle::Macos,
                    DesktopStyle::Windows,
                    DesktopStyle::Windows2000,
                    DesktopStyle::NextStep,
                    DesktopStyle::MakeOs,
                ] {
                    let opacity = t.weights[style as usize] as f32;
                    if opacity < 0.001 {
                        continue;
                    }
                    let hit_start = self.hits.len();
                    if style == DesktopStyle::NextStep {
                        let cell = h / n;
                        self.button(cx, rect(x,y,w,cell), ShelfHit::Launcher, Ico::Menu, "Workspace", false, style, opacity);
                        for (i, app) in apps.iter().enumerate() {
                            let id = app.id.trim_start_matches("apps.");
                            self.button(cx, rect(x,y+(i+1) as f64*cell,w,cell), ShelfHit::App(id.into()), app_icon(id), &app.label, clients.iter().any(|(_,a,_)| a==id), style, opacity);
                        }
                    } else if style == DesktopStyle::Windows2000 {
                        self.button(
                            cx,
                            rect(x + 3.0, y + 3.0, 76.0, (h - 6.0).max(1.0)),
                            ShelfHit::Launcher,
                            Ico::Menu,
                            "Start",
                            false,
                            style,
                            opacity,
                        );
                        let available = (w - 126.0).max(1.0);
                        let cell = (available / clients.len().max(1) as f64).min(180.0);
                        for (i, (c, app, title)) in clients.iter().enumerate() {
                            self.button(
                                cx,
                                rect(
                                    x + 86.0 + i as f64 * cell,
                                    y + 3.0,
                                    cell - 3.0,
                                    (h - 6.0).max(1.0),
                                ),
                                ShelfHit::Window(*c),
                                app_icon(app),
                                title,
                                !state.layout.desktop.minimized(*c),
                                style,
                                opacity,
                            );
                        }
                    } else {
                        let cell = if style.mac_family() {
                            (w - 20.0) / n
                        } else {
                            48.0_f64.min((w - 36.0) / n)
                        };
                        let start = if style.mac_family() {
                            x + 10.0
                        } else {
                            x + (w - cell * n) * 0.5
                        };
                        self.button(
                            cx,
                            rect(start, y + 6.0, cell, (h - 12.0).max(1.0)),
                            ShelfHit::Launcher,
                            Ico::Menu,
                            if style.mac_family() {
                                "Applications"
                            } else {
                                "Start"
                            },
                            false,
                            style,
                            opacity,
                        );
                        for (i, app) in apps.iter().enumerate() {
                            let id = app.id.trim_start_matches("apps.");
                            self.button(
                                cx,
                                rect(
                                    start + (i + 1) as f64 * cell,
                                    y + 6.0,
                                    cell,
                                    (h - 12.0).max(1.0),
                                ),
                                ShelfHit::App(id.into()),
                                app_icon(id),
                                &app.label,
                                clients.iter().any(|(_, a, _)| a == id),
                                style,
                                opacity,
                            );
                        }
                    }
                    if !style.mac_family() && style != DesktopStyle::NextStep {
                        let button = rect(x + w - 22.0, y + 3.0, 19.0, (h - 6.0).max(1.0));
                        self.hits.push((button, ShelfHit::ShowDesktop));
                        self.d.solid(
                            cx,
                            rect(button.pos.x, button.pos.y, 1.0, button.size.y),
                            alpha(rgb(128, 128, 128), opacity),
                        );
                    }
                    if style != t.target {
                        self.hits.truncate(hit_start);
                    }
                }
                if glass_foreground { self.overlay.as_mut().unwrap().end(cx); }
            }
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if let Some(ne) = self.hover_frame.is_event(event) {
            if let Some(hit) = &self.hover {
                if !self.hover_mix.iter().any(|(h, _)| h == hit) {
                    self.hover_mix.push((hit.clone(), 0.0));
                }
            }
            let dt = if self.hover_time == 0.0 {
                1.0 / 60.0
            } else {
                (ne.time - self.hover_time).min(0.05)
            };
            self.hover_time = ne.time;
            let mut active = false;
            for (hit, value) in &mut self.hover_mix {
                let target = if Some(&*hit) == self.hover.as_ref() {
                    1.0
                } else {
                    0.0
                };
                *value += (target - *value) * (1.0 - (-dt * 20.0).exp());
                active |= (target - *value).abs() > 0.001;
            }
            self.hover_mix
                .retain(|(hit, value)| *value > 0.001 || Some(hit) == self.hover.as_ref());
            if active {
                self.hover_frame = cx.new_next_frame();
            }
            self.redraw(cx);
        }
    }
}
