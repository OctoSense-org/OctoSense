//! OctoSense's additional desktop style, layered on the upstream widget API.
use makepad_widgets::{app_icon, desktop_style::{DesktopStyle as UpstreamStyle, StyleSheet}, *};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DesktopStyle {
    #[default]
    Omarchy,
    Macos,
    Windows,
    Windows2000,
    NextStep,
    Ios,
    Android,
    OctoSense,
}

impl DesktopStyle {
    pub const ALL: [Self; 8] = [Self::Omarchy, Self::Macos, Self::Windows, Self::Windows2000, Self::NextStep, Self::Ios, Self::Android, Self::OctoSense];

    /// OctoSense shares macOS geometry and artwork; its palette and material stay local.
    pub fn framework(self) -> UpstreamStyle {
        match self {
            Self::Omarchy => UpstreamStyle::Omarchy,
            Self::Macos | Self::OctoSense => UpstreamStyle::Macos,
            Self::Windows => UpstreamStyle::Windows,
            Self::Windows2000 => UpstreamStyle::Windows2000,
            Self::NextStep => UpstreamStyle::NextStep,
            Self::Ios => UpstreamStyle::Ios,
            Self::Android => UpstreamStyle::Android,
        }
    }
    pub fn id(self) -> &'static str {
        if self == Self::OctoSense { "octosense" } else { self.framework().id() }
    }
    pub fn label(self) -> &'static str {
        if self == Self::OctoSense { "OctoSense" } else { self.framework().label() }
    }
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.strip_suffix("-dark").unwrap_or(name);
        Self::ALL.into_iter().find(|style| style.id() == name)
    }
    pub fn supports_dark(self) -> bool { self.framework().supports_dark() }
    pub fn mobile(self) -> bool { self.framework().mobile() }
    pub fn floating(self) -> bool { self.framework().floating() }
    pub fn shelf_height(self) -> f64 { self.framework().shelf_height() }
    pub fn title_height(self) -> f64 { self.framework().title_height() }
    pub fn mac_family(self) -> bool { self.framework() == UpstreamStyle::Macos }
    pub fn next(self) -> Self { Self::ALL[(self as usize + 1) % Self::ALL.len()] }
}

pub fn load_sheet(style: DesktopStyle, dark: bool) -> StyleSheet {
    if style != DesktopStyle::OctoSense {
        return StyleSheet::load_with_appearance(style.framework(), dark);
    }
    let read = |name: &str, bundled: &str| {
        // Source checkouts reload on selection; installed/mobile builds use embedded data.
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(text) = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/themes/octosense").join(name)
        ) { return text; }
        let _ = name;
        bundled.to_string()
    };
    // Use a recognized wire family so unmodified hosted apps choose macOS icons
    // and the selected appearance. Full theme and widget overrides travel with it.
    let (theme_name, theme, widgets_name, widgets) = if dark {
        ("theme.splash", include_str!("../../resources/themes/octosense/theme.splash"),
         "widgets.splash", include_str!("../../resources/themes/octosense/widgets.splash"))
    } else {
        ("theme-light.splash", include_str!("../../resources/themes/octosense/theme-light.splash"),
         "widgets-light.splash", include_str!("../../resources/themes/octosense/widgets-light.splash"))
    };
    StyleSheet {
        name: if dark { "macos-dark" } else { "macos" }.into(),
        theme: read(theme_name, theme),
        widgets: read(widgets_name, widgets),
        icons: app_icon::load_assets(UpstreamStyle::Macos),
    }
}

#[derive(Default)]
pub struct AppIconDraw(app_icon::AppIconDraw);
impl AppIconDraw {
    pub fn draw(&mut self, cx: &mut Cx2d, name: &str, style: DesktopStyle, rect: Rect, opacity: f32, ink: Vec4f) {
        self.0.draw(cx, name, style.framework(), rect, opacity, ink);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn octosense_sheet_survives_the_unmodified_upstream_wire_protocol() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            // Reuse one VM, as hosted apps do: every palette role must reset
            // when switching appearances in either direction.
            for dark in [true, false, true] {
                let sheet = load_sheet(DesktopStyle::OctoSense, dark);
                assert_eq!(sheet.name, if dark { "macos-dark" } else { "macos" });
                assert_eq!(StyleSheet::parse(&sheet.to_json()), Some(sheet.clone()));
                assert_eq!(UpstreamStyle::parse(&sheet.name), Some(UpstreamStyle::Macos));
                assert_eq!(sheet.icons, app_icon::load_assets(UpstreamStyle::Macos));
                desktop_style::install(vm, sheet);
                vm.bx.captured_errors = Some(Vec::new());
                vm.with_reload(makepad_widgets::script_mod);
                assert!(vm.take_errors().is_empty());
                assert_eq!(desktop_style::current_style(vm), UpstreamStyle::Macos);
                let (focus, background, text) = if dark {
                    (0x5b9dffff, 0x0b1220ff, 0xd6e2ffff)
                } else {
                    (0x206bc4ff, 0xeff5f6ff, 0x203644ff)
                };
                assert_eq!(script_eval!(vm, {mod.theme.color_focus}).as_color(), Some(focus));
                assert_eq!(script_eval!(vm, {mod.theme.color_bg_app}).as_color(), Some(background));
                assert_eq!(script_eval!(vm, {mod.theme.color_text}).as_color(), Some(text));
                assert_eq!(script_eval!(vm, {mod.theme.color_terminal_bg}).as_color(), Some(background));
                assert_eq!(script_eval!(vm, {mod.theme.color_terminal_text}).as_color(), Some(text));
                assert_eq!(script_eval!(vm, {mod.theme.material.lensing_strength}).as_f64(), Some(28.0));
            }
        });
    }

    #[test]
    fn styles_keep_their_order_and_platform_behavior() {
        for (index, style) in DesktopStyle::ALL.into_iter().enumerate() {
            assert_eq!(index, style as usize);
            assert_eq!(DesktopStyle::parse(style.id()), Some(style));
            assert_eq!(style.next(), DesktopStyle::ALL[(index + 1) % 8]);
        }
        assert!(DesktopStyle::OctoSense.floating());
        assert!(DesktopStyle::OctoSense.supports_dark());
        assert_eq!(DesktopStyle::OctoSense.title_height(), DesktopStyle::Macos.title_height());
    }
}
