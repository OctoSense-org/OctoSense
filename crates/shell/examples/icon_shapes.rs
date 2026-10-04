//! Visual regression sheet using the production shell renderer.
//! Run from phone/ with mobile-apps so Camera is in the bundled catalog.
pub use makepad_widgets;
use makepad_widgets::*;
use octosense_shell::octosense::style::{AppIconDraw, DesktopStyle};

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    mod.widgets.IconSheet = set_type_default() do #(IconSheet::register_widget(vm)) {
        width: Fill height: Fill
        background +: {
            pixel: fn() {
                let band = fract((self.pos.y * self.rect_size.y - 32.0) / 120.0)
                if self.pos.y * self.rect_size.y >= 32.0 && band > 0.6 && band < 0.85 { return #edf1f8 }
                return #243044
            }
        }
        text +: {color: #fff text_style: theme.font_regular {font_size: 11}}
    }
    startup() do #(App::script_component(vm)) {
        ui: Root { main_window := Window {
            window.inner_size: vec2(1040, 920)
            body +: { sheet := mod.widgets.IconSheet {} }
        } }
    }
}

#[derive(Script, ScriptHook)]
pub struct App { #[live] ui: WidgetRef }
impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        // Initializes the very same shaders and resource catalogs as Home.
        octosense_shell::App::shell_script_mod(vm);
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if matches!(event, Event::Startup) {
            let root = std::env::var_os("OCTOSENSE_HOME").expect("set an isolated OCTOSENSE_HOME for this preview");
            let root = std::path::PathBuf::from(root).join("icon-fixtures");
            for (id, icon) in [("shape-svg", "icon.svg"), ("shape-png", "icon.png")] {
                let bundle = root.join(id).join("bundle");
                std::fs::create_dir_all(&bundle).unwrap();
                std::fs::write(bundle.join("listing.json"), serde_json::json!({
                    "schema":1, "description":"Icon shape regression fixture", "category":"utilities",
                    "platforms":["android"], "age_rating":"all", "icon":icon,
                    "publisher":{"name":"Fixture", "support":"https://example.com",
                    "privacy_policy_url":"https://example.com/privacy"}
                }).to_string()).unwrap();
                if id == "shape-svg" {
                    std::fs::write(bundle.join(icon), r##"<svg viewBox="0 0 64 64"><rect width="64" height="64" fill="#e45757"/><circle cx="32" cy="32" r="12" fill="#fff"/></svg>"##).unwrap();
                } else {
                    use makepad_widgets::makepad_zune_png::{makepad_zune_core::{bit_depth::BitDepth,
                        colorspace::ColorSpace, options::EncoderOptions}, PngEncoder};
                    let mut pixels = vec![0u8; 64 * 64 * 4];
                    for y in 16..48 { for x in 16..48 {
                        pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4].copy_from_slice(&[40, 140, 220, 255]);
                    } }
                    let mut png = Vec::new();
                    PngEncoder::new(&pixels, EncoderOptions::new(64, 64, ColorSpace::RGBA, BitDepth::Eight))
                        .encode(&mut png).unwrap();
                    std::fs::write(bundle.join(icon), png).unwrap();
                }
            }
            octosense_app_hub_app::set_data_root(root);
        }
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct IconSheet {
    #[uid] uid: WidgetUid,
    #[walk] walk: Walk,
    #[redraw] #[live] background: DrawQuad,
    #[live] text: DrawText,
    #[rust] icons: AppIconDraw,
}
impl Widget for IconSheet {
    fn handle_event(&mut self, _: &mut Cx, _: &Event, _: &mut Scope) {}
    fn draw_walk(&mut self, cx: &mut Cx2d, _: &mut Scope, walk: Walk) -> DrawStep {
        let canvas = cx.walk_turtle(walk);
        self.background.draw_abs(cx, canvas);
        let names = ["photos", "apphub", "assistant", "camera", "ai-providers", "youtube", "hub:shape-svg", "hub:shape-png"];
        for (row, style) in [DesktopStyle::Android, DesktopStyle::Macos, DesktopStyle::Ios,
            DesktopStyle::Windows, DesktopStyle::NextStep, DesktopStyle::Omarchy, DesktopStyle::Windows2000].into_iter().enumerate() {
            let y = canvas.pos.y + 32. + row as f64 * 120.;
            self.text.draw_abs(cx, dvec2(canvas.pos.x + 10., y + 25.), style.label());
            for (col, name) in names.into_iter().enumerate() {
                let x = canvas.pos.x + 135. + col as f64 * 110.;
                if row == 0 { self.text.draw_abs(cx, dvec2(x - 8., canvas.pos.y + 8.), name.strip_prefix("hub:").unwrap_or(name)); }
                for (size, dx, dy) in [(64., 0., 0.), (24., 0., 74.), (24., 34., 74.)] {
                    self.icons.draw(cx, name, style, Rect { pos: dvec2(x + dx, y + dy), size: dvec2(size, size) },
                        if dx > 0. {0.5} else {1.}, vec4(0.9, 0.94, 1., 1.));
                }
            }
        }
        DrawStep::done()
    }
}
