//! System apps' text against the host's page in every style, light and dark.
//! On a phone a hosted page shows the host's background
//! (`theme.color_bg_app`), not the app's own, so a fixed dark text colour
//! vanishes when the host is dark (apps/AGENTS.md).
use crate::octosense::style::{load_sheet, DesktopStyle};
use makepad_widgets::{desktop_style, *};

const NEWS: &str = include_str!("../../../apps/news/bundle/main.splash");

/// News's styles: its `let`s, without the boot before them or the view after.
fn news_styles() -> String {
    let after_boot = NEWS.split_once("\nlet ink = ").unwrap().1;
    format!("{}\nlet ink = {}", include_str!("../../../apps/interface.splash"), after_boot.split_once("\nHostedView{").unwrap().0)
}

/// Each colour in `expressions`, evaluated after News's styles in a VM
/// themed as a hosted app is in `style` and appearance.
fn news_colors(style: DesktopStyle, dark: bool, expressions: &[&str]) -> Vec<u32> {
    script_colors(style, dark, &news_styles(), expressions)
}

fn script_colors(style: DesktopStyle, dark: bool, styles: &str, expressions: &[&str]) -> Vec<u32> {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        desktop_style::install(vm, load_sheet(style, dark));
        vm.with_reload(makepad_widgets::script_mod);
        expressions
            .iter()
            .map(|expression| {
                vm.bx.captured_errors = Some(Vec::new());
                let value = vm.eval(ScriptMod {
                    file: "news_theme_test.splash".into(),
                    code: format!("use mod.prelude.widgets.*\n{styles}\n{expression}\n;"),
                    ..Default::default()
                });
                let errors = vm.take_errors();
                assert!(errors.is_empty(), "{expression}: {errors:?}");
                Vec4f::script_from_value(vm, value).to_u32()
            })
            .collect()
    })
}

#[test]
fn youtube_search_text_and_placeholder_read_in_normal_hover_and_focus_states() {
    let source = include_str!("../../../apps/youtube/bundle/main.splash");
    let prelude = source.split_once("// END shared app interface").unwrap().0;
    let field = source.split_once("search := ").unwrap().1;
    let mut depth = 0;
    let end = field.char_indices().find_map(|(index, ch)| {
        if ch == '{' { depth += 1; }
        if ch == '}' {
            depth -= 1;
            if depth == 0 { return Some(index + 1); }
        }
        None
    }).unwrap();
    // Use the actual shipped field and its actual shared prelude. Only
    // its network callback is replaced; the local colour overrides are
    // retained so a fixed dark background fails the light-theme checks.
    let styles = format!("{prelude}\nlet ink = ui_ink\nlet secondary = ui_muted\nfn search_for(text) {{}}\nlet Search = {}", &field[..end]);
    for style in DesktopStyle::ALL {
        for dark in [false, true] {
            if dark && !style.supports_dark() { continue; }
            let colors = script_colors(style, dark, &styles, &[
                "Search.draw_bg.color", "Search.draw_text.color",
                "Search.draw_bg.color_hover", "Search.draw_text.color_hover",
                "Search.draw_bg.color_focus", "Search.draw_text.color_focus",
                "Search.draw_bg.color_empty", "Search.draw_text.color_empty",
                "Search.draw_bg.color_hover", "Search.draw_text.color_empty_hover",
                "Search.draw_bg.color_focus", "Search.draw_text.color_empty_focus",
            ]);
            for (state, pair) in colors.chunks_exact(2).enumerate() {
                assert!(contrast(pair[0], pair[1]) >= 4.5, "{} dark={dark}, state={state}: {:08x} on {:08x}", style.id(), pair[1], pair[0]);
            }
        }
    }
}

#[test]
fn system_app_interface_keeps_text_and_controls_readable_in_every_appearance() {
    for style in DesktopStyle::ALL {
        for dark in [false, true] {
            if dark && !style.supports_dark() { continue; }
            let colors = news_colors(style, dark, &[
                "ui_page", "ui_surface", "ui_field", "ui_ink", "ui_muted", "ui_link", "ui_success", "ui_danger", "ui_primary",
            ]);
            let at = format!("{} dark={dark}", style.id());
            for surface in &colors[..3] {
                for ink in &colors[3..8] {
                    assert!(contrast(*ink, *surface) >= 4.5, "{at}: {ink:08x} on {surface:08x}: {}", contrast(*ink, *surface));
                }
            }
            assert!(contrast(0xffffffff, colors[8]) >= 4.5, "{at}: primary action");
        }
    }
}

/// WCAG contrast ratio of two opaque `0xRRGGBBAA` colours.
fn contrast(a: u32, b: u32) -> f64 {
    let luminance = |c: u32| {
        let channel = |shift: u32| {
            let v = ((c >> shift) & 0xff) as f64 / 255.0;
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(24) + 0.7152 * channel(16) + 0.0722 * channel(8)
    };
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[test]
fn news_source_tabs_read_on_the_page_in_light_and_dark() {
    for style in DesktopStyle::ALL {
        for dark in [false, true] {
            if dark && !style.supports_dark() {
                continue;
            }
            let [page, tab, active_text, active_pill] = news_colors(
                style,
                dark,
                &["theme.color_bg_app", "TabButton.draw_text.color", "TabButtonActive.draw_text.color", "TabButtonActive.draw_bg.color"],
            )[..] else {
                unreachable!()
            };
            let at = format!("{} {}", style.id(), if dark { "dark" } else { "light" });
            assert!(contrast(tab, page) >= 4.5, "{at}: a source tab's text {tab:08x} on the page {page:08x}");
            assert!(contrast(active_text, active_pill) >= 4.5, "{at}: the open tab's text {active_text:08x} on its pill {active_pill:08x}");
            assert!(contrast(active_pill, page) >= 3.0, "{at}: the open tab's pill {active_pill:08x} on the page {page:08x}");
        }
    }
}
