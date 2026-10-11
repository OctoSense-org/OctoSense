//! System apps' text against the host's page in every style, light and dark.
//! On a phone a hosted page shows the host's background
//! (`theme.color_bg_app`), not the app's own, so a fixed dark text colour
//! vanishes when the host is dark (apps/AGENTS.md).
use crate::octosense::style::{load_sheet, DesktopStyle};
use makepad_widgets::{desktop_style, *};

const NEWS: &str = include_str!("../../../apps/news/bundle/main.splash");

#[test]
fn camera_recording_failure_settles_controls_without_stopping_a_healthy_recording() {
    let source = include_str!("../../../apps/camera/bundle/main.splash");
    let logic = source.split_once("\nstart_timeout(").expect("Camera boot boundary").0;
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        vm.bx.captured_errors = Some(Vec::new());
        // Execute the shipped app's callbacks in the native VM. Camera,
        // timers and rendering endpoints are test doubles: this verifies app
        // state and endpoint calls, not pixels or successful hardware capture.
        let value = vm.eval(ScriptMod {
            file: "camera_recording_failure_test.splash".into(),
            code: format!(r#"
use mod.std.assert
let fs = {{}}
let native_recording = false
let native_error = ""
let timer_visible = false
let retry_visible = false
let toast = ""
let timers = []
let stopped = []
let stop_requests = 0
let microphone_allowed = true
let host = {{request: fn(service, args, reply){{
    assert(service == "microphone.permission.request")
    reply({{is_ok: true data: {{app_consent: microphone_allowed os_permission: "granted"}}}})
}}}}
fn time_now() {{ 123 }}
fn start_interval(seconds, callback) {{ timers.push(callback); timers.len() }}
fn stop_timer(timer) {{ stopped.push(timer) }}
let widget = {{render: fn(){{}} set_text: fn(text){{}}}}
let ui = {{
    cam: {{
        record_start: fn(options){{ assert(options.audio && !options.library); native_recording = true; true }}
        record_stop: fn(){{ stop_requests += 1; native_recording = false }}
        is_running: fn(){{ true }}
        is_recording: fn(){{ native_recording }}
        error: fn(){{ native_error }}
        set_aspect: fn(aspect){{}}
    }}
    rec: widget
    rec_box: {{set_visible: fn(value){{ timer_visible = value }}}}
    shutter_face: widget modes: widget
    toast: {{set_text: fn(value){{ toast = value }}}}
    retry_camera: {{set_visible: fn(value){{ retry_visible = value }}}}
}}
{logic}
set_mode("video")
start_recording()
assert(recording && timer_visible && rec_timer == 1)
// Android queues a request, then reports that recording is unsupported.
native_recording = false
native_error = "video failed: recording is not implemented on Android yet"
failed()
assert(!recording && !timer_visible && rec_timer == nil)
assert(stopped.len() == 1 && stopped[0] == 1)
assert(stop_requests == 0 && retry_visible)
assert(toast == native_error)
set_mode("photo")
assert(mode == "photo")
failed()
assert(stopped.len() == 1)
// An unrelated error while the native recorder remains active must not
// discard the UI state or permit a mode change during that recording.
set_mode("video")
start_recording()
native_error = "photo failed: synthetic still failure"
failed()
assert(recording && timer_visible && rec_timer == 2)
assert(stopped.len() == 1 && stop_requests == 0)
set_mode("photo")
assert(mode == "video")
stop_recording()
assert(!recording && !timer_visible && rec_timer == nil)
assert(stopped.len() == 2 && stopped[1] == 2 && stop_requests == 1)
// Rejected app consent cannot start another native recording or UI timer.
microphone_allowed = false
start_recording()
assert(!recording && !native_recording && !timer_visible && rec_timer == nil)
assert(timers.len() == 2 && retry_visible && !permission_pending)
true
;"#),
            ..Default::default()
        });
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "Camera app errors: {errors:?}");
        assert!(!value.is_err(), "Camera callbacks returned {value:?}");
    });
}

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
fn youtube_committed_query_restores_field_without_overwriting_unsent_typing() {
    let source = include_str!("../../../apps/youtube/bundle/main.splash");
    let logic = source.split_once("// END shared app interface\n").unwrap().1
        .split_once("\nstart_timeout(").unwrap().0;
    for (stored, expected) in [("\"NASA\"", "NASA"), ("nil", "lofi hip hop radio")] {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            vm.bx.captured_errors = Some(Vec::new());
            // Run the shipped callbacks with local storage/network/UI spies.
            // No provider request or actual persisted user file is involved.
            let value = vm.eval(ScriptMod {
                file: "youtube_query_restore_test.splash".into(),
                code: format!(r#"
use mod.std.assert
let stored_query = {stored}
let writes = []
let field_text = ""
let field_sets = 0
let status_text = ""
let video_count = "—"
let fs = {{
    exists: fn(path){{ path == "accounts/device/query.json" && stored_query != nil }}
    read: fn(path){{ assert(path == "accounts/device/query.json"); {{q: stored_query}}.to_json() }}
    write: fn(path, data){{ writes.push({{path: path data: data}}) }}
    remove: fn(path){{ assert(false) }}
}}
let sys = {{video: fn(query, index, field){{ if field == "count" {{ return video_count }}; "" }}}}
let widget = {{render: fn(){{}} set_visible: fn(value){{}}}}
let ui = {{
    search: {{set_text: fn(value){{ field_text = value; field_sets += 1 }}}}
    status: {{set_text: fn(value){{ status_text = value }}}}
    main: widget player_pane: widget tabs: widget list: widget
}}
{logic}
boot()
assert(q == "{expected}" && field_text == q)
assert(status_text == "Searching YouTube for “{expected}”…")
assert(writes.len() == 0 && field_sets == 1)
// A committed chip/search replaces the field and persists its trimmed query.
search_for("  NASA science  ")
assert(q == "NASA science" && field_text == q)
assert(writes.len() == 1 && writes[0].path == "accounts/device/query.json")
assert(writes[0].data.parse_json().q == q && field_sets == 2)
// Result polling and tab changes must not discard a draft still being typed.
field_text = "Unsubmitted next query"
video_count = "0"
read_hits()
sync_status()
show("history")
show("search")
assert(loaded_q == q && field_text == "Unsubmitted next query")
assert(field_sets == 2 && writes.len() == 1)
search_for("   ")
assert(q == "NASA science" && field_text == "Unsubmitted next query")
assert(field_sets == 2 && writes.len() == 1)
true
;"#),
                ..Default::default()
            });
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "YouTube stored={stored}: {errors:?}");
            assert!(!value.is_err(), "YouTube callbacks returned {value:?}");
        });
    }
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
