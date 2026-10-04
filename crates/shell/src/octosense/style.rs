//! OctoSense's additional desktop style, layered on the upstream widget API.
use makepad_widgets::{app_icon, desktop_style::{DesktopStyle as UpstreamStyle, StyleSheet}, *};
use super::icon_frame;
#[cfg(any(feature = "app-hub", native_mobile))]
use super::icon_frame::{DrawIconImage, SvgIconTexture};

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
        let mut sheet = StyleSheet::load_with_appearance(style.framework(), dark);
        sheet.icons = icon_assets(style.framework());
        return sheet;
    }
    let read = |name: &str, bundled: &str| {
        // Source checkouts reload on selection; packaged and mobile builds
        // use the embedded data (never a path on the build machine).
        #[cfg(not(target_arch = "wasm32"))]
        if !super::paths::packaged() {
            if let Ok(text) = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/themes/octosense").join(name)
            ) { return text; }
        }
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
        icons: icon_assets(UpstreamStyle::Macos),
    }
}

/// The framework's artwork for a style with this shell's own laid over it:
/// News as redrawn here, and OctosMap, which the framework has no art for.
pub fn icon_assets(style: UpstreamStyle) -> Vec<app_icon::IconAsset> {
    fn wear(assets: &mut Vec<app_icon::IconAsset>, name: &str, svg: String) {
        match assets.iter_mut().find(|asset| asset.name == name) {
            Some(asset) => asset.svg = svg,
            None => assets.push(app_icon::IconAsset { name: name.into(), svg }),
        }
    }
    let mut assets = app_icon::load_assets(style);
    match own_artwork(style) {
        Some([news, maps]) => {
            wear(&mut assets, "news", news.into());
            wear(&mut assets, "maps", maps.into());
        }
        // Windows 2000's sixteen-pixel art stays the framework's: its News,
        // and for OctosMap the route app's, which OctosMap is built on.
        None => {
            let route = assets.iter().find(|asset| asset.name == "route").map(|asset| asset.svg.clone());
            if let Some(route) = route {
                wear(&mut assets, "maps", route);
            }
        }
    }
    wear(&mut assets, "apphub", icon_frame::styled_svg(apphub_svg(), style));
    // The system chat's dock entry and home chip (#143): its own art, so it
    // never reads as the AI pane's app.
    wear(&mut assets, "assistant", icon_frame::styled_svg(include_str!("../../resources/icons/apps/assistant.svg"), style));
    assets.sort_by(|a, b| a.name.cmp(&b.name));
    assets
}

/// App Hub's store icon: App Hub's own when it is linked, else the same icon
/// kept beside the other app art.
fn apphub_svg() -> &'static str {
    #[cfg(any(feature = "app-hub", native_mobile))]
    return octosense_app_hub_app::APP_ICON_SVG;
    #[cfg(not(any(feature = "app-hub", native_mobile)))]
    return include_str!("../../resources/icons/apps/apphub.svg");
}

/// Whether the icon catalog still wears this shell's art for `style`: its
/// App Hub icon, which the framework's own catalog does not have.
fn wears_shell_art(cx: &mut Cx, style: UpstreamStyle) -> bool {
    app_icon::source(cx, style, "apphub").contains(icon_frame::SHELL_ART_MARKER)
}

/// News and OctosMap as `tools/build_app_icons.py` draws them, in that order.
fn own_artwork(style: UpstreamStyle) -> Option<[&'static str; 2]> {
    macro_rules! pair {
        ($style:literal) => {
            [
                include_str!(concat!("../../resources/icons/apps/", $style, "/news.svg")),
                include_str!(concat!("../../resources/icons/apps/", $style, "/maps.svg")),
            ]
        };
    }
    Some(match style {
        UpstreamStyle::Omarchy => pair!("omarchy"),
        UpstreamStyle::Macos => pair!("macos"),
        UpstreamStyle::Windows => pair!("windows"),
        UpstreamStyle::NextStep => pair!("nextstep"),
        UpstreamStyle::Ios => pair!("ios"),
        UpstreamStyle::Android => pair!("android"),
        UpstreamStyle::Windows2000 => return None,
    })
}

#[derive(Default)]
pub struct AppIconDraw {
    draw: app_icon::AppIconDraw,
    #[cfg(any(feature = "app-hub", native_mobile))]
    library: InstalledIcons,
    /// The styles whose catalog this drawer has seen to, by discriminant.
    installed: [bool; UpstreamStyle::ALL.len()],
}
impl AppIconDraw {
    pub fn draw(&mut self, cx: &mut Cx2d, name: &str, style: DesktopStyle, rect: Rect, opacity: f32, ink: Vec4f) {
        #[cfg(any(feature = "app-hub", native_mobile))]
        if self.library.draw(cx, name, style.framework(), rect, opacity) { return; }
        let style = style.framework();
        // A style can be drawn before its sheet is applied (a crossfade's
        // target, the first frame); the framework would then fall back to
        // its own artwork, which has no OctosMap. And a fresh isolate (a
        // glance card's tile, a script app) that finds no sheet in its heap
        // installs the framework's own catalog over this shell's: makepad's
        // `desktop_style::current` falls back to `MAKEPAD_WIDGET_STYLE`,
        // which this shell sets in its own environment for its children. So
        // the shell's art goes back whenever its App Hub icon has gone.
        if !std::mem::replace(&mut self.installed[style as usize], true) || !wears_shell_art(cx, style) {
            app_icon::install(cx, style, &icon_assets(style));
        }
        self.draw.draw(cx, name, style, rect, opacity, ink);
    }
}

#[cfg(any(feature = "app-hub", native_mobile))]
#[derive(Default)]
struct InstalledIcons {
    root: Option<std::path::PathBuf>,
    generation: u64,
    entries: std::collections::HashMap<String, Option<InstalledIcon>>,
}
#[cfg(any(feature = "app-hub", native_mobile))]
enum InstalledIcon { Svg(DrawIconImage, SvgIconTexture), Png(DrawIconImage, Texture) }

#[cfg(any(feature = "app-hub", native_mobile))]
impl InstalledIcons {
    fn draw(&mut self, cx: &mut Cx2d, name: &str, style: UpstreamStyle, rect: Rect, opacity: f32) -> bool {
        use octosense_app_hub_app::icons::{self, IconData};
        // A system app's own art, when its bundle ships one (ADR 0004).
        let system = name.strip_prefix("hub:").is_none() && crate::apps::system_card_apps().iter().any(|a| a.id == name);
        let Some(id) = name.strip_prefix("hub:").or(system.then_some(name)) else { return false; };
        let Some(root) = octosense_app_hub_app::data_root_if_set() else { return false; };
        let generation = icons::generation();
        if self.root.as_ref() != Some(&root) || self.generation != generation {
            self.entries.clear();
            self.root = Some(root.clone());
            self.generation = generation;
        }
        if self.entries.len() >= 256 && !self.entries.contains_key(name) { self.entries.clear(); }
        let icon = self.entries.entry(name.into()).or_insert_with(|| {
            let data = if system { octosense_app_hub_app::system_icon(id)? } else { icons::read_installed_icon(&root, id)? };
            match data {
                IconData::Svg(source) => {
                    let texture = SvgIconTexture::new(cx, &source)?;
                    let mut draw = cx.with_vm(|vm| DrawIconImage::script_new_with_default(vm));
                    draw.premultiplied = 1.;
                    Some(InstalledIcon::Svg(draw, texture))
                }
                IconData::Png(data) => {
                    let buffer = image_cache::ImageBuffer::from_png(&data).ok()?;
                    let source_size = vec2(buffer.width as f32, buffer.height as f32);
                    let texture = buffer.into_new_texture(cx);
                    let mut draw = cx.with_vm(|vm| DrawIconImage::script_new_with_default(vm));
                    draw.source_size = source_size;
                    Some(InstalledIcon::Png(draw, texture))
                }
            }
        });
        let (rect, radius, backing) = icon_frame::placement(style, rect);
        match icon {
            Some(InstalledIcon::Svg(draw, texture)) => {
                draw.draw_vars.set_texture(0, texture.texture(cx, rect.size));
                draw.radius = radius;
                draw.backing = backing;
                draw.opacity = opacity;
                draw.draw_abs(cx, rect);
            }
            Some(InstalledIcon::Png(draw, texture)) => {
                draw.draw_vars.set_texture(0, texture);
                draw.radius = radius;
                draw.backing = backing;
                draw.opacity = opacity;
                draw.draw_abs(cx, rect);
            }
            None => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svg_of<'a>(assets: &'a [app_icon::IconAsset], name: &str) -> &'a str {
        &assets.iter().find(|asset| asset.name == name).unwrap_or_else(|| panic!("no {name} icon")).svg
    }

    #[test]
    fn added_app_icons_follow_the_selected_platform_shape() {
        for name in ["apphub", "assistant"] {
            let android = icon_assets(UpstreamStyle::Android);
            assert!(svg_of(&android, name).contains("<circle cx=\"32\" cy=\"32\" r=\"31\""),
                "{name} must share Android's circular silhouette");
            let macos = icon_assets(UpstreamStyle::Macos);
            assert!(svg_of(&macos, name).contains("width=\"58\" height=\"57\" rx=\"13\""),
                "{name} must share macOS's rounded tile");
            let ios = icon_assets(UpstreamStyle::Ios);
            assert!(svg_of(&ios, name).contains("width=\"62\" height=\"62\" rx=\"15\""),
                "{name} must share iOS's rounded tile");
        }
    }

    #[test]
    fn news_and_octosmap_wear_this_shells_artwork_in_every_style() {
        for style in UpstreamStyle::ALL {
            let framework = app_icon::load_assets(style);
            let assets = icon_assets(style);
            // Nothing the framework draws is lost, and the list stays sorted
            // by name as the framework's is. Added: OctosMap, App Hub and the
            // system chat's Assistant (#143).
            assert_eq!(assets.len(), framework.len() + 3, "{}", style.id());
            assert!(svg_of(&assets, "assistant").contains("OctoSense #143"), "{}", style.id());
            assert!(assets.windows(2).all(|pair| pair[0].name < pair[1].name), "{}", style.id());
            for asset in &framework {
                if asset.name != "news" {
                    assert_eq!(svg_of(&assets, &asset.name), asset.svg, "{} {}", style.id(), asset.name);
                }
            }
            if style == UpstreamStyle::Windows2000 {
                // Sixteen-pixel art: the framework's News, and its route for OctosMap.
                assert_eq!(svg_of(&assets, "news"), svg_of(&framework, "news"));
                assert_eq!(svg_of(&assets, "maps"), svg_of(&framework, "route"));
            } else {
                assert_ne!(svg_of(&assets, "news"), svg_of(&framework, "news"), "{}", style.id());
                assert_ne!(svg_of(&assets, "maps"), svg_of(&framework, "route"), "{}", style.id());
            }
        }
    }

    #[test]
    fn this_shells_artwork_is_what_the_renderer_can_draw() {
        for style in UpstreamStyle::ALL {
            let assets = icon_assets(style);
            for name in ["news", "maps", "apphub"] {
                let svg = svg_of(&assets, name);
                assert!(svg.starts_with("<svg "), "{} {name}", style.id());
                // No clip paths, masks, filters or text: the renderer has none.
                for unsupported in ["<clipPath", "<mask", "<filter", "<text", "<image", "<use"] {
                    assert!(!svg.contains(unsupported), "{} {name}: {unsupported}", style.id());
                }
            }
        }
    }

    #[test]
    fn every_sheet_carries_the_artwork() {
        for style in DesktopStyle::ALL {
            let sheet = load_sheet(style, false);
            assert_eq!(sheet.icons, icon_assets(style.framework()), "{}", style.id());
        }
    }

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
                assert_eq!(sheet.icons, icon_assets(UpstreamStyle::Macos));
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
