//! Data-only phone themes. The Android picker reads the same bundled catalog.
//! Appearance changes never select another desktop/navigation family.
use crate::{desktop::DesktopStyle, shell};
use makepad_strict_json::Value;
use makepad_widgets::{desktop_style::StyleSheet, Vec4f};
use std::{collections::BTreeMap, sync::OnceLock};

pub const CATALOG: &str = include_str!("../resources/themes/mobile-presets.json");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Preset { #[default] OctoSense, Minimal, Paper, Vivid }
impl Preset {
    pub const ALL: [Self; 4] = [Self::OctoSense, Self::Minimal, Self::Paper, Self::Vivid];
    pub fn id(self) -> &'static str {
        match self { Self::OctoSense => "octosense", Self::Minimal => "minimal", Self::Paper => "paper", Self::Vivid => "vivid" }
    }
    fn parse(id: &str) -> Option<Self> { Self::ALL.into_iter().find(|p| p.id() == id) }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance { #[default] System, Light, Dark }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wallpaper { #[default] Gradient, Solid }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub preset: Preset,
    pub appearance: Appearance,
    pub wallpaper: Wallpaper,
}
impl Selection {
    pub fn encode(self) -> Value {
        use makepad_strict_json::{obj, s};
        obj(vec![("version", Value::Int(1)), ("preset", s(self.preset.id())),
            ("appearance", s(match self.appearance { Appearance::System => "system", Appearance::Light => "light", Appearance::Dark => "dark" })),
            ("wallpaper", s(match self.wallpaper { Wallpaper::Gradient => "gradient", Wallpaper::Solid => "solid" }))])
    }
    /// Reject unsupported/corrupt records rather than partially changing a theme.
    pub fn decode(value: &Value) -> Option<Self> {
        if value.get("version")?.as_i64()? != 1 { return None; }
        Some(Self {
            preset: Preset::parse(value.get("preset")?.as_str()?)?,
            appearance: match value.get("appearance")?.as_str()? {
                "system" => Appearance::System, "light" => Appearance::Light,
                "dark" => Appearance::Dark, _ => return None,
            },
            wallpaper: match value.get("wallpaper")?.as_str()? {
                "gradient" => Wallpaper::Gradient, "solid" => Wallpaper::Solid, _ => return None,
            },
        })
    }
    pub fn dark(self, system: bool) -> bool {
        match self.appearance { Appearance::System => system, Appearance::Light => false, Appearance::Dark => true }
    }
    pub fn palette(self, dark: bool) -> Palette {
        let mut palette = palettes()[self.preset as usize][dark as usize];
        if self.wallpaper == Wallpaper::Solid {
            palette.wallpaper_top = palette.background;
            palette.wallpaper_bottom = palette.background;
        }
        palette
    }
    pub fn sheet(self, style: DesktopStyle, dark: bool) -> StyleSheet {
        let mut sheet = crate::octosense::style::load_sheet(style, dark);
        let p = self.palette(dark);
        let mut colors = BTreeMap::new();
        for (names, color) in [
            ("bg_app fg_app", p.background), ("bg_container", p.surface),
            ("bg_highlight bg_highlight_inline inset inset_1 inset_2 outset outset_1 outset_2", p.surface_variant),
            ("focus ctrl_selected ctrl_active outset_active", p.accent),
            ("text_on_accent", p.on_accent), ("cursor text_cursor", p.accent),
            ("bevel_inset_1 bevel_inset_2 bevel_outset_1 bevel_outset_2", p.border),
            ("terminal_bg", p.background), ("terminal_text", p.text),
        ] { for name in names.split_whitespace() { colors.insert(format!("color_{name}"), color); } }
        for name in ["text", "label", "label_inner", "label_outer", "icon"] {
            for state in ["", "_hover", "_focus", "_active", "_down", "_disabled"] {
                colors.insert(format!("color_{name}{state}"), if state == "_disabled" { p.muted } else { p.text });
            }
        }
        for name in ["inset", "inset_1", "inset_2", "outset", "outset_1", "outset_2", "bevel_inset_1", "bevel_inset_2", "bevel_outset_1", "bevel_outset_2"] {
            for state in ["hover", "focus", "active", "down", "disabled", "empty", "drag"] {
                let color = if name.starts_with("bevel") { if state == "focus" {p.accent} else {p.border} } else {p.surface_variant};
                colors.insert(format!("color_{name}_{state}"), color);
            }
        }
        // Remove old literal assignments as the shell's role scanner reads the
        // first assignment. Preserve the framework's fonts, widgets and behavior.
        sheet.theme = sheet.theme.lines().filter(|line| {
            let key = line.split_once('=').map(|(key, _)| key.trim());
            !key.and_then(|key| key.strip_prefix("mod.theme.")).is_some_and(|key|
                colors.contains_key(key) || key == "corner_radius" || key == "container_corner_radius")
        }).collect::<Vec<_>>().join("\n");
        sheet.theme.push('\n');
        for (key, color) in colors { sheet.theme.push_str(&format!("mod.theme.{key} = {}\n", hex(color))); }
        sheet.theme.push_str(&format!("mod.theme.corner_radius = {:.1}\nmod.theme.container_corner_radius = {:.1}\ntrue\n", p.radius / 2.0, p.radius));
        sheet
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: Vec4f, pub surface: Vec4f, pub surface_variant: Vec4f,
    pub text: Vec4f, pub muted: Vec4f, pub accent: Vec4f, pub on_accent: Vec4f,
    pub border: Vec4f, pub wallpaper_top: Vec4f, pub wallpaper_bottom: Vec4f,
    pub radius: f64,
}
impl Palette {
    pub fn shell(self) -> shell::ShellPalette {
        shell::ShellPalette { background: self.surface, text: self.text, accent: self.accent,
            on_accent: self.on_accent, border: self.border, error: shell::rgb(220, 60, 70) }
    }
}
fn hex(c: Vec4f) -> String {
    format!("#{:02x}{:02x}{:02x}", (c.x * 255.0).round() as u8, (c.y * 255.0).round() as u8, (c.z * 255.0).round() as u8)
}
fn palettes() -> &'static Vec<[Palette; 2]> {
    static PALETTES: OnceLock<Vec<[Palette; 2]>> = OnceLock::new();
    PALETTES.get_or_init(|| {
        let catalog = makepad_strict_json::parse(CATALOG.as_bytes()).expect("bundled theme catalog");
        let presets = catalog.get("presets").and_then(Value::as_arr).expect("theme presets");
        Preset::ALL.into_iter().map(|id| {
            let spec = presets.iter().find(|v| v.get("id").and_then(Value::as_str) == Some(id.id())).expect("bundled preset");
            let radius = spec.get("radius").and_then(Value::as_i64).expect("theme radius") as f64;
            ["light", "dark"].map(|mode| {
                let v = spec.get(mode).expect("theme appearance");
                let color = |key: &str| {
                    let text = v.get(key).and_then(Value::as_str).expect("theme color");
                    assert_eq!(text.len(), 6);
                    let c = u32::from_str_radix(text, 16).expect("RGB theme color");
                    shell::rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
                };
                Palette { background: color("background"), surface: color("surface"), surface_variant: color("surface_variant"),
                    text: color("text"), muted: color("muted"), accent: color("accent"), on_accent: color("on_accent"),
                    border: color("border"), wallpaper_top: color("wallpaper_top"), wallpaper_bottom: color("wallpaper_bottom"), radius }
            })
        }).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_choices_do_not_partially_apply() {
        let valid = r#"{"version":1,"preset":"paper","appearance":"system","wallpaper":"solid"}"#;
        let decode = |s: &str| Selection::decode(&makepad_strict_json::parse(s.as_bytes()).unwrap());
        let choice = decode(valid).unwrap();
        assert_eq!(choice.preset, Preset::Paper);
        assert!(!choice.dark(false)); assert!(choice.dark(true));
        for bad in [valid.replace("paper", "missing"), valid.replace("system", "auto"), valid.replace(":1", ":2"), valid.replace("solid", "video")] {
            assert!(decode(&bad).is_none());
        }
    }
    fn contrast(a: Vec4f, b: Vec4f) -> f32 {
        let luminance = |c: Vec4f| {
            let linear = |v: f32| if v <= 0.04045 {v / 12.92} else {((v + 0.055) / 1.055).powf(2.4)};
            linear(c.x) * 0.2126 + linear(c.y) * 0.7152 + linear(c.z) * 0.0722
        };
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
    #[test]
    fn every_bundled_palette_has_readable_text_and_selection() {
        for preset in Preset::ALL { for dark in [false, true] {
            let p = Selection { preset, ..Default::default() }.palette(dark);
            for (ink, ground) in [(p.text, p.background), (p.text, p.surface), (p.muted, p.surface), (p.on_accent, p.accent), (p.accent, p.surface)] {
                assert!(contrast(ink, ground) >= 4.5, "{} dark={dark}: {}", preset.id(), contrast(ink, ground));
            }
        } }
    }
    #[test]
    fn themes_preserve_phone_family_and_replace_scanned_roles() {
        for preset in Preset::ALL { for dark in [false, true] {
            let choice = Selection { preset, ..Default::default() };
            let original = crate::octosense::style::load_sheet(DesktopStyle::Android, dark);
            let themed = choice.sheet(DesktopStyle::Android, dark);
            assert_eq!(original.name, themed.name);
            assert_eq!(original.widgets, themed.widgets);
            assert_eq!(crate::theme::scan_style_roles(&themed.theme).text, choice.palette(dark).text);
            assert_eq!(crate::theme::scan_style_roles(&themed.theme).focus, choice.palette(dark).accent);
        } }
    }
}
