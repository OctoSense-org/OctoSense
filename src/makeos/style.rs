//! MakeOS's additional desktop style, layered on the upstream widget API.
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
    MakeOs,
}

impl DesktopStyle {
    pub const ALL: [Self; 8] = [Self::Omarchy, Self::Macos, Self::Windows, Self::Windows2000, Self::NextStep, Self::Ios, Self::Android, Self::MakeOs];

    /// MakeOS shares macOS geometry and artwork; its palette and material stay local.
    pub fn framework(self) -> UpstreamStyle {
        match self {
            Self::Omarchy => UpstreamStyle::Omarchy,
            Self::Macos | Self::MakeOs => UpstreamStyle::Macos,
            Self::Windows => UpstreamStyle::Windows,
            Self::Windows2000 => UpstreamStyle::Windows2000,
            Self::NextStep => UpstreamStyle::NextStep,
            Self::Ios => UpstreamStyle::Ios,
            Self::Android => UpstreamStyle::Android,
        }
    }
    pub fn id(self) -> &'static str {
        if self == Self::MakeOs { "makeos" } else { self.framework().id() }
    }
    pub fn label(self) -> &'static str {
        if self == Self::MakeOs { "MakeOS" } else { self.framework().label() }
    }
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.strip_suffix("-dark").unwrap_or(name);
        Self::ALL.into_iter().find(|style| style.id() == name)
    }
    pub fn supports_dark(self) -> bool { self != Self::MakeOs && self.framework().supports_dark() }
    pub fn mobile(self) -> bool { self.framework().mobile() }
    pub fn floating(self) -> bool { self.framework().floating() }
    pub fn shelf_height(self) -> f64 { self.framework().shelf_height() }
    pub fn title_height(self) -> f64 { self.framework().title_height() }
    pub fn mac_family(self) -> bool { self.framework() == UpstreamStyle::Macos }
    pub fn next(self) -> Self { Self::ALL[(self as usize + 1) % Self::ALL.len()] }
}

pub fn load_sheet(style: DesktopStyle, dark: bool) -> StyleSheet {
    if style != DesktopStyle::MakeOs {
        return StyleSheet::load_with_appearance(style.framework(), dark);
    }
    let read = |name: &str, bundled: &str| {
        // Source checkouts reload on selection; installed/mobile builds use embedded data.
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(text) = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/themes/makeos").join(name)
        ) { return text; }
        let _ = name;
        bundled.to_string()
    };
    // Use a recognized wire family so unmodified hosted apps choose macOS icons
    // and dark appearance. The full theme and widget overrides travel with it.
    StyleSheet {
        name: "macos-dark".into(),
        theme: read("theme.splash", include_str!("../../resources/themes/makeos/theme.splash")),
        widgets: read("widgets.splash", include_str!("../../resources/themes/makeos/widgets.splash")),
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
    fn makeos_sheet_survives_the_unmodified_upstream_wire_protocol() {
        let sheet = load_sheet(DesktopStyle::MakeOs, false);
        assert_eq!(StyleSheet::parse(&sheet.to_json()), Some(sheet.clone()));
        assert_eq!(UpstreamStyle::parse(&sheet.name), Some(UpstreamStyle::Macos));
        assert_eq!(sheet.icons, app_icon::load_assets(UpstreamStyle::Macos));
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            desktop_style::install(vm, sheet);
            vm.bx.captured_errors = Some(Vec::new());
            vm.with_reload(makepad_widgets::script_mod);
            assert!(vm.take_errors().is_empty());
            assert_eq!(desktop_style::current_style(vm), UpstreamStyle::Macos);
            assert_eq!(script_eval!(vm, {mod.theme.color_focus}).as_color(), Some(0x5b9dffff));
            assert_eq!(script_eval!(vm, {mod.theme.material.lensing_strength}).as_f64(), Some(28.0));
        });
    }

    #[test]
    fn styles_keep_their_order_and_platform_behavior() {
        for (index, style) in DesktopStyle::ALL.into_iter().enumerate() {
            assert_eq!(index, style as usize);
            assert_eq!(DesktopStyle::parse(style.id()), Some(style));
            assert_eq!(style.next(), DesktopStyle::ALL[(index + 1) % 8]);
        }
        assert!(DesktopStyle::MakeOs.floating());
        assert!(!DesktopStyle::MakeOs.supports_dark());
        assert_eq!(DesktopStyle::MakeOs.title_height(), DesktopStyle::Macos.title_height());
    }
}
