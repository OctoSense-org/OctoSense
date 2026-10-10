use crate::module_view::MpModuleView;
use makepad_widgets::{makepad_draw::cx_draw::CxDraw, widget_async::with_isolate, *};

const PHOTOS: &str = include_str!("../../../apps/photos/bundle/main.splash");
const RESIZE_PROBE: &str = r#"
mod.resize_history = []
fn on_app_resize(width, height) {
    mod.resize_history.push([width, height])
}
View{width: Fill height: Fill}
"#;

fn photo_model(expression: &str) -> serde_json::Value {
    let source = PHOTOS.split_once("// END shared app interface\n").unwrap().1
        .split_once("\nstart_timeout(").unwrap().0;
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm {
        host: &mut host,
        bx: Box::new(ScriptVmBase::new()),
    };
    vm.bx.captured_errors = Some(Vec::new());
    let result = vm.with_instruction_limit(500_000, |vm| {
        vm.eval(ScriptMod {
            file: "photos_tile_test.splash".into(),
            code: format!("use mod.math.*\n{source}\n{expression}\n;"),
            ..Default::default()
        })
    });
    let errors = vm.take_errors();
    assert!(errors.is_empty(), "{errors:?}");
    let json = vm
        .bx
        .heap
        .string_with(result, |_, value| value.to_string())
        .unwrap();
    serde_json::from_str(&json).unwrap()
}

#[test]
fn photos_preview_has_three_images_per_card_row() {
    for rows in [1, 2, 3] {
        let images = photo_model(&format!("tile_ids({rows}).to_json()"));
        assert_eq!(
            images.as_array().unwrap().len(),
            rows * 3,
            "{rows} preview rows"
        );
    }
}

#[test]
fn photos_preview_rows_follow_available_space() {
    for (width, height, rows) in [
        (370, 60, 1),
        (370, 88, 1),
        (370, 140, 2),
        (370, 176, 2),
        (370, 210, 3),
        (370, 264, 3),
        (370, 600, 3),
        (740, 176, 1),
    ] {
        let actual = photo_model(&format!("tile_rows_for_size({width}, {height}).to_json()"));
        assert_eq!(actual, rows, "{width} × {height} card");
    }
}

#[test]
fn photos_preview_keeps_favorites_first_without_repeating_them() {
    let images = photo_model("store.favorites = [\"family\" \"forest\"]\ntile_ids(3).to_json()");
    let images = images.as_array().unwrap();
    assert_eq!(images.len(), 9);
    assert_eq!(
        &images[..2],
        &[serde_json::json!("family"), serde_json::json!("forest")]
    );
    for image in images {
        assert_eq!(
            images
                .iter()
                .filter(|candidate| *candidate == image)
                .count(),
            1
        );
    }
}

fn hosted_card() -> (Cx, WidgetRef, WidgetRef, SplashVmId) {
    hosted_card_with_handler(Box::new(|_, _| {}))
}

fn hosted_card_with_handler(handler: Box<dyn FnMut(&mut Cx, &Event)>) -> (Cx, WidgetRef, WidgetRef, SplashVmId) {
    let mut cx = Cx::new(handler);
    cx.with_vm(makepad_widgets::script_mod);
    let outer = cx.alloc_splash_vm_with_network(false);
    let root = cx.with_script_vm_id_trusted(outer, |vm| {
        let value = script_eval!(vm, {use mod.prelude.widgets.* View{card := Splash{width: Fill height: Fill}}});
        WidgetRef::script_from_value(vm, value)
    });
    let tile = cx.with_vm(|vm| {
        script_eval!(vm, {mod.wm_theme = {background: #ffffff}});
        crate::module_view::script_mod(vm);
        let value = script_eval!(vm, {use mod.widgets.* MpModuleView{}});
        WidgetRef::script_from_value(vm, value)
    });
    tile.borrow_mut::<MpModuleView>()
        .unwrap()
        .set_root(&mut cx, 1, outer, root.clone());
    set_card_body(&mut cx, &root, outer, RESIZE_PROBE);
    (cx, tile, root, outer)
}

#[test]
fn shorter_hosted_viewport_reveals_the_focused_editor_without_moving_its_header() {
    use crate::tile::TileHost;
    use std::{cell::RefCell, rc::Rc};

    let receiver = Rc::new(RefCell::new(WidgetRef::empty()));
    let events = receiver.clone();
    let (mut cx, tile, _, outer) = hosted_card_with_handler(Box::new(move |cx, event| {
        events.borrow().handle_event(cx, event, &mut Scope::empty());
    }));
    *receiver.borrow_mut() = tile.clone();
    let root = cx.with_script_vm_id_trusted(outer, |vm| {
        let value = script_eval!(vm, {
            use mod.prelude.widgets.*
            View{width: Fill height: Fill flow: Down
                header := Button{width: Fill height: 44 text: "Save"}
                scroller := ScrollYView{width: Fill height: Fill flow: Down
                    View{width: Fill height: 280}
                    notes := TextInput{width: Fill height: 100 is_multiline: true text: "Keep my draft"}
                }
            }
        });
        WidgetRef::script_from_value(vm, value)
    });
    tile.borrow_mut::<MpModuleView>().unwrap().set_root(&mut cx, 1, outer, root.clone());
    widget_tree::set_ui_root(&mut cx, &root);
    draw_card(&mut cx, &tile, 390.0, 600.0);
    assert!(tile.borrow_mut::<MpModuleView>().unwrap().focus_keyboard(&mut cx));
    let notes = root.text_input(&cx, ids!(notes));
    notes.take_key_focus(&mut cx);
    // Commit the pending focus as the platform does after input dispatch.
    cx.send_trigger(notes.area(), Trigger { id: live_id!(test_focus_settle), from: Area::Empty });
    cx.handle_triggers();
    assert!(cx.has_key_focus(notes.area()), "editor initially takes focus");
    let header = root.button(&cx, ids!(header)).area().rect(&cx);
    assert_eq!(root.view(&cx, ids!(scroller)).scroll_pos().y, 0.0);

    draw_card(&mut cx, &tile, 390.0, 280.0);
    assert!(cx.has_key_focus(notes.area()), "resizing retains editor focus");
    cx.handle_triggers();
    // The host requests the ordinary smooth focus-scroll, which advances on
    // frame events. Deliver frames before inspecting its final geometry.
    for frame in 1..=60 {
        let last = cx.new_next_frame();
        tile.handle_event(&mut cx, &Event::NextFrame(NextFrameEvent {
            frame,
            time: frame as f64 / 60.0,
            set: (0..=last.0).map(NextFrame).collect(),
        }), &mut Scope::empty());
        draw_card(&mut cx, &tile, 390.0, 280.0);
    }

    let editor = notes.area().rect(&cx);
    assert!(root.view(&cx, ids!(scroller)).scroll_pos().y > 0.0, "editor: {editor:?}");
    assert!(editor.pos.y >= header.pos.y + header.size.y, "editor overlaps fixed header: {editor:?}");
    assert!(editor.pos.y + editor.size.y <= 280.0, "editor is below keyboard: {editor:?}");
    assert_eq!(root.button(&cx, ids!(header)).area().rect(&cx), header);
    assert_eq!(notes.text(), "Keep my draft");
    assert!(cx.has_key_focus(notes.area()));
}

fn set_card_body(cx: &mut Cx, root: &WidgetRef, outer: SplashVmId, body: &str) {
    with_isolate(cx, outer, |cx| {
        root.splash(cx, ids!(card)).set_text(cx, body)
    });
}

#[test]
fn calendar_saved_event_title_wraps_above_its_details_on_a_narrow_phone() {
    let source = include_str!("../../../apps/calendar/bundle/main.splash");
    let title_type = source.lines().find(|line| line.starts_with("let UiTitle = ")).unwrap();
    let title_instance = source.lines().find(|line| line.trim_start().starts_with("event_title := ")).unwrap();
    let (mut cx, tile, root, outer) = hosted_card();
    set_card_body(&mut cx, &root, outer, &format!(
        "let ui_ink = theme.color_text\n{title_type}\nView{{width: Fill height: Fill flow: Down padding: 20 spacing: 18\n{title_instance}\nwhen := Label{{text: \"WHEN\"}}\n}}"
    ));
    widget_tree::set_ui_root(&mut cx, &root);
    let title = root.label(&cx, ids!(event_title));
    title.set_text(&mut cx, "Short");
    draw_card(&mut cx, &tile, 360.0, 600.0);
    let one_line = title.area().rect(&cx);
    title.set_text(&mut cx, "OnePlus acceptance event with a longer appointment title");
    draw_card(&mut cx, &tile, 360.0, 600.0);
    let wrapped = title.area().rect(&cx);
    let details = root.label(&cx, ids!(when)).area().rect(&cx);
    assert!(wrapped.size.y > one_line.size.y, "the saved title must grow instead of clipping: {wrapped:?}");
    assert!(wrapped.pos.x >= 20.0 && wrapped.pos.x + wrapped.size.x <= 340.0);
    assert!(details.pos.y >= wrapped.pos.y + wrapped.size.y, "event details must follow all title lines");
    assert_eq!(title.text(), "OnePlus acceptance event with a longer appointment title");
}

#[test]
fn maps_location_request_checks_the_contained_app_capability() {
    let (mut cx, _tile, root, outer) = hosted_card();
    set_card_body(&mut cx, &root, outer, "View{}");
    let source = root.splash(&cx, ids!(card)).borrow().unwrap().view.source.clone();
    let owner = cx.script_ref_vm_id(&source).unwrap();
    let heap = cx.with_script_vm_id_trusted(owner, |vm| vm.bx.heap.heap_key());
    let request = |cx: &mut Cx| cx.with_script_vm_id_trusted(owner, |vm| {
        splash::register_agent_module(vm);
        vm.bx.captured_errors = Some(Vec::new());
        let result = script_eval!(vm, {sys.request_location()});
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "{errors:?}");
        result.as_number().unwrap()
    });
    for capability in ["net", "location.get", ""] {
        splash_policy::set_policy_for_heap(heap, vec![capability.into()], vec![], None);
        assert_eq!(request(&mut cx), -1.0, "{capability} cannot start permission consent");
    }
    splash_policy::set_policy_for_heap(heap, vec!["location".into()], vec![], None);
    assert_eq!(request(&mut cx), if cfg!(target_os = "android") {1.0} else {0.0}, "unsupported platforms must not claim a request started");
    splash_policy::set_policy_for_heap(heap, vec![], vec![], None);
    assert_eq!(request(&mut cx), -1.0, "revoking the grant takes effect on the next request");
}

#[test]
fn maps_reads_are_passive_and_only_the_location_action_requests_permission() {
    let source = include_str!("../../../apps/maps/bundle/main.splash");
    let function = |name: &str| {
        let start = source.find(&format!("fn {name}(" )).unwrap();
        source[start..].split_once("\nfn ").unwrap().0
    };
    let functions = ["fix_origin", "location_note", "use_my_location"].map(function).join("\n");
    let code = format!(r#"
use mod.std.assert
mod.requests = 0
mod.result = 1
mod.fixed = false
mod.note = ""
mod.note_visible = false
mod.screen = "search"
let origin = {{lat: 37.3350 lon: -121.8850 name: "San Jose (downtown)" picked: false}}
let finding = "origin"
let sys = {{
    request_location: fn() {{mod.requests = mod.requests + 1; return mod.result}}
    gps: fn(field) {{if !mod.fixed {{return 0}}; if field == "lat" {{return 37.7}}; if field == "lon" {{return -122.4}}; return 1}}
}}
let ui = {{location_status: {{
    set_text: fn(text) {{mod.note = text}}
    set_visible: fn(value) {{mod.note_visible = value}}
}}}}
fn show(name) {{mod.screen = name}}
{functions}
fix_origin()
fix_origin()
assert(mod.requests == 0)
use_my_location()
assert(mod.requests == 1)
assert(origin.name == "San Jose (downtown)")
assert(mod.note_visible && mod.note != "")
assert(mod.screen == "route")
mod.fixed = true
fix_origin()
assert(mod.requests == 1)
assert(origin.name == "Your location" && origin.lat == 37.7 && origin.lon == -122.4)
assert(mod.note == "" && !mod.note_visible)
mod.fixed = false
mod.result = -1
use_my_location()
assert(mod.note == "This app does not have location access. Choose an origin instead.")
1
"#);
    let mut host = ScriptVmHost::new((), ());
    let mut vm = ScriptVm { host: &mut host, bx: Box::new(ScriptVmBase::new()) };
    vm.bx.captured_errors = Some(Vec::new());
    let result = vm.eval(ScriptMod { file: "maps_location_test.splash".into(), code, ..Default::default() });
    let errors = vm.take_errors();
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(result.as_number(), Some(1.0));
}

#[test]
fn permission_dialog_pause_and_resume_leave_a_new_video_ready_for_its_first_source() {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    let root = cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        let value = script_eval!(vm, {
            use mod.prelude.widgets.*
            View{preview := Video{autoplay: false show_controls: false}}
        });
        WidgetRef::script_from_value(vm, value)
    });
    let preview = root.video(&cx, ids!(preview));
    assert!(preview.is_unprepared());
    root.handle_event(&mut cx, &Event::Pause, &mut Scope::empty());
    root.handle_event(&mut cx, &Event::Resume, &mut Scope::empty());
    assert!(preview.is_unprepared(), "a system permission dialog must not invent a running camera");
    assert!(!preview.is_playing());
}

/// #406: the OS pause and resume reach a contained app's Video through its
/// tile. The resume starts again the start the pause cancelled, unless the
/// app paused the player from its own script while the OS held it.
#[test]
fn os_resume_restores_a_hosted_video_unless_its_app_paused_it_meanwhile() {
    let (mut cx, tile, root, outer) = hosted_card();
    set_card_body(&mut cx, &root, outer, r#"
mod.answers = []
fn app_pause() { mod.answers.push(ui.clip.pause_playback()) }
View{width: Fill height: Fill clip := Video{width: Fill height: 180 autoplay: false show_controls: false}}
"#);
    widget_tree::set_ui_root(&mut cx, &root);
    let clip = root.video(&cx, ids!(clip));
    // Bytes pass no network or storage policy, and this Cx has no platform
    // loop: the native prepare stays queued, so nothing decodes or plays.
    clip.set_source_in_memory(std::rc::Rc::new(vec![0; 16]));
    let os = |cx: &mut Cx, event: Event| tile.handle_event(cx, &event, &mut Scope::empty());
    clip.begin_playback(&mut cx);
    assert!(clip.is_preparing());
    os(&mut cx, Event::Pause);
    assert!(clip.is_unprepared(), "the pause cancels a start the native player would finish on its own");
    os(&mut cx, Event::Resume);
    assert!(clip.is_preparing(), "the resume starts it again");
    os(&mut cx, Event::Pause);
    let card = root.splash(&cx, ids!(card));
    assert!(with_isolate(&mut cx, outer, |cx| card.call_script_fn(cx, id!(app_pause), &[])));
    // ui.* calls wait for the widget task pump, as after a real host callback.
    makepad_widgets::makepad_platform::makepad_script_std::handle_script_tasks(&mut cx);
    os(&mut cx, Event::Resume);
    assert!(clip.is_unprepared(), "the app's pause outlasts the resume");
    let source = card.borrow().unwrap().view.source.clone();
    let owner = cx.script_ref_vm_id(&source).unwrap();
    let answers = cx.with_script_vm_id_trusted(owner, |vm| {
        let value = script_eval!(vm, {mod.answers.to_json()});
        vm.bx.heap.string_with(value, |_, value| value.to_string()).unwrap()
    });
    assert_eq!(answers, "[true]", "the script's pause is accepted, though the player was not playing");
}

fn draw_card(cx: &mut Cx, tile: &WidgetRef, width: f64, height: f64) {
    let size = dvec2(width, height);
    let pass = DrawPass::new(cx);
    pass.set_size(cx, size);
    let mut list = DrawList2d::new(cx);
    let event = DrawEvent::default();
    let mut draw = CxDraw::new(cx, &event);
    let mut draw = Cx2d::new(&mut draw);
    draw.begin_pass(&pass, Some(1.0));
    list.begin_always(&mut draw);
    draw.begin_root_turtle(size, Layout::default());
    tile.draw_walk_all(&mut draw, &mut Scope::empty(), Walk::fixed(width, height));
    draw.end_turtle();
    list.end(&mut draw);
    draw.end_pass(&pass);
}

fn resize_history(cx: &mut Cx, root: &WidgetRef) -> serde_json::Value {
    let card = root.splash(cx, ids!(card));
    let source = card.borrow().unwrap().view.source.clone();
    let owner = cx.script_ref_vm_id(&source).unwrap();
    cx.with_script_vm_id_trusted(owner, |vm| {
        vm.bx.captured_errors = Some(Vec::new());
        let value = script_eval!(vm, {mod.resize_history.to_json()});
        let errors = vm.take_errors();
        assert!(errors.is_empty(), "{errors:?}");
        let json = vm
            .bx
            .heap
            .string_with(value, |_, value| value.to_string())
            .unwrap();
        serde_json::from_str(&json).unwrap()
    })
}

#[test]
fn hosted_card_receives_size_changes_once_in_its_own_isolate() {
    let (mut cx, tile, root, _) = hosted_card();
    for height in [88.0, 88.0, 176.0, 264.0, 88.0] {
        draw_card(&mut cx, &tile, 370.0, height);
    }
    assert_eq!(
        resize_history(&mut cx, &root),
        serde_json::json!([[370, 88], [370, 176], [370, 264], [370, 88]])
    );
}

/// A restyle runs the app's script again in the same content view: its
/// layout state starts over, so it must hear its size again, though the
/// slot kept it. Quick Deck, Writer and PDF Tools all lost their layout
/// width on a light/dark switch before this.
#[test]
fn restyled_card_receives_its_size_again_in_an_unchanged_slot() {
    let (mut cx, tile, root, _) = hosted_card();
    draw_card(&mut cx, &tile, 370.0, 88.0);
    draw_card(&mut cx, &tile, 370.0, 88.0);
    crate::module_host::restyled();
    draw_card(&mut cx, &tile, 370.0, 88.0);
    draw_card(&mut cx, &tile, 370.0, 88.0);
    assert_eq!(
        resize_history(&mut cx, &root),
        serde_json::json!([[370, 88], [370, 88]])
    );
}

#[test]
fn reloaded_card_receives_its_size_even_when_the_slot_is_unchanged() {
    let (mut cx, tile, root, outer) = hosted_card();
    draw_card(&mut cx, &tile, 370.0, 88.0);
    set_card_body(&mut cx, &root, outer, &format!("{RESIZE_PROBE}\nView{{}}"));
    draw_card(&mut cx, &tile, 370.0, 88.0);
    assert_eq!(
        resize_history(&mut cx, &root),
        serde_json::json!([[370, 88]])
    );
}

#[test]
fn photos_preview_renders_resized_rows_before_the_frame_is_captured() {
    let (mut cx, tile, root, outer) = hosted_card();
    let model = PHOTOS.split_once("\nstart_timeout(").unwrap().0;
    let preview = r#"
View{width: Fill height: Fill
    tile_grid := View{width: Fill height: Fill flow: Down on_render: || {
        for row in grid_rows_of(tile_ids(tile_rows), 3) {
            View{width: Fill height: Fill}
        }
    }}
}
"#;
    set_card_body(&mut cx, &root, outer, &format!("{model}\n{preview}"));
    widget_tree::set_ui_root(&mut cx, &root);
    for (height, rows) in [(264.0, 3), (88.0, 1), (176.0, 2), (88.0, 1)] {
        draw_card(&mut cx, &tile, 370.0, height);
        let grid = root.view(&cx, ids!(tile_grid));
        let grid = grid.borrow().unwrap();
        assert_eq!(grid.children.len(), rows, "{height} point card");
        assert!(grid
            .children
            .iter()
            .all(|(_, row)| row.area().rect(&cx).size.y > 0.0));
    }
}

#[test]
fn photos_preview_title_is_the_size_of_every_card_title() {
    let (mut cx, _tile, root, outer) = hosted_card();
    set_photos_preview(&mut cx, &root, outer);
    let title = root.label(&cx, ids!(tile_title));
    assert_eq!(
        title.borrow().unwrap().draw_text.text_style.font_size,
        crate::shell::ui::px_to_pt(crate::mobile_tiles::CARD_TITLE_PX)
    );
}

fn set_photos_preview(cx: &mut Cx, root: &WidgetRef, outer: SplashVmId) {
    let model = PHOTOS.split_once("\nstart_timeout(").unwrap().0;
    let preview = PHOTOS
        .rsplit_once("\n    tile: ")
        .unwrap()
        .1
        .trim_end()
        .strip_suffix('}')
        .unwrap();
    set_card_body(
        cx,
        root,
        outer,
        &format!("{model}\nlet accent = #x007aff\npreview := {preview}"),
    );
    widget_tree::set_ui_root(cx, root);
}

#[test]
fn photos_preview_fills_the_card_with_its_title_overlaid_at_the_bottom() {
    let (mut cx, tile, root, outer) = hosted_card();
    set_photos_preview(&mut cx, &root, outer);
    let preview = root.view(&cx, ids!(preview));
    {
        let preview = preview.borrow().unwrap();
        assert_eq!(preview.layout.flow, Flow::Overlay);
        assert_eq!(preview.layout.padding.top, 0.0);
        assert_eq!(preview.layout.padding.bottom, 0.0);
        assert_eq!(preview.layout.padding.left, 0.0);
        assert_eq!(preview.layout.padding.right, 0.0);
    }
    draw_card(&mut cx, &tile, 370.0, 88.0);
    let grid = root.view(&cx, ids!(tile_grid));
    assert_eq!(grid.area().rect(&cx).size, dvec2(370.0, 88.0));
    let caption = root.view(&cx, ids!(tile_caption));
    assert_eq!(caption.borrow().unwrap().layout.align.y, 1.0);
    assert_eq!(root.label(&cx, ids!(tile_title)).text(), "Photos");
}

#[test]
fn hosted_script_colors_follow_appearance_without_losing_input() {
    let (mut cx, tile, mut root, outer) = hosted_card();
    let sheet = |dark| desktop_style::StyleSheet::load_with_appearance(desktop_style::DesktopStyle::Ios, dark);
    cx.with_script_vm_id_trusted(outer, |vm| {
        desktop_style::install(vm, sheet(false));
        vm.with_reload(makepad_widgets::script_mod);
    });
    let card = root.splash(&mut cx, ids!(card));
    card.borrow_mut().unwrap().set_stylesheet(&mut cx, sheet(false));
    set_card_body(&mut cx, &root, outer,
        "let ink = theme.color_text\nView{label := Label{text: \"Theme\" draw_text.color: ink} input := TextInput{text: \"Draft\"}}"
    );
    draw_card(&mut cx, &tile, 400.0, 700.0);
    let label = root.label(&mut cx, ids!(label));
    let light = label.borrow().unwrap().draw_text.color;
    root.text_input(&mut cx, ids!(input)).set_text(&mut cx, "Unsaved draft");
    cx.with_script_vm_id_trusted(outer, |vm| {
        desktop_style::install(vm, sheet(true));
        vm.with_reload(makepad_widgets::script_mod);
        let source = root.script_source();
        root.script_apply(vm, &Apply::ScriptReapply, &mut Scope::empty(), source.into());
    });
    draw_card(&mut cx, &tile, 400.0, 700.0);
    let dark = label.borrow().unwrap().draw_text.color;
    assert_ne!(light, dark, "restyling must reach the app's nested Splash");
    assert_eq!(root.text_input(&mut cx, ids!(input)).text(), "Unsaved draft");
}

#[cfg(feature = "app-hub")]
#[test]
fn card_runner_restyles_nested_script_without_replacing_draft() {
    use makepad_app_module::AppModule;
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(makepad_widgets::script_mod);
    let mut host = crate::module_host::ModuleHost::default();
    let sheet = |dark| desktop_style::StyleSheet::load_with_appearance(desktop_style::DesktopStyle::Ios, dark);
    host.apply_style(&mut cx, &sheet(false));
    let module = &octosense_appstore::cardapp::CARD_MODULE;
    let open = module.open_schema().validate("{\"app\":\"os.calendar\"}", &[]).unwrap();
    host.create(&mut cx, 1, module, open, dvec2(400.0, 700.0)).unwrap();
    let root = host.get(1).unwrap().root.clone();
    let owner = host.get(1).unwrap().vm_id;
    let tile = cx.with_vm(|vm| {
        script_eval!(vm, {mod.wm_theme = {background: #ffffff}});
        crate::module_view::script_mod(vm);
        let value = script_eval!(vm, {use mod.widgets.* MpModuleView{}});
        WidgetRef::script_from_value(vm, value)
    });
    tile.borrow_mut::<MpModuleView>().unwrap().set_root(&mut cx, 1, owner, root.clone());
    // Exercise the Card runner's nested-isolate restyle with only the two
    // widgets whose identity/state matter here. Loading the complete shared
    // interface made this boundary test spend its 64 ms startup budget on
    // unrelated widget prototypes under concurrent CI load. Full interface
    // colors are covered by system_app_theme_tests; keep the runtime budget.
    let probe = r#"let theme = mod.theme
        let ink = theme.color_text
        SolidView{draw_bg.color: theme.color_bg_app
            label := Label{text: "Theme" draw_text.color: ink}
            input := TextInput{text: "Draft"}}
    "#;
    set_card_body(&mut cx, &root, owner, probe);
    draw_card(&mut cx, &tile, 400.0, 700.0);
    let label = root.label(&mut cx, ids!(label));
    let light = label.borrow().expect("the contained theme probe initialized").draw_text.color;
    let input = root.text_input(&mut cx, ids!(input));
    let input_id = input.widget_uid();
    input.set_text(&mut cx, "Unsaved draft");
    host.apply_style(&mut cx, &sheet(true));
    draw_card(&mut cx, &tile, 400.0, 700.0);
    let dark = label.borrow().unwrap().draw_text.color;
    assert_ne!(light, dark, "the Card runner must restyle its hosted app, not only its outer view");
    assert_eq!(root.text_input(&mut cx, ids!(input)).widget_uid(), input_id, "restyle must retain the live editor");
    assert_eq!(root.text_input(&mut cx, ids!(input)).text(), "Unsaved draft");
    host.teardown(&mut cx, 1);
}

#[test]
#[cfg(feature = "app-hub")]
fn card_runner_restyle_keeps_dynamic_labels_wrapped_and_explicit_no_wrap() {
    use makepad_app_module::AppModule;
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(makepad_widgets::script_mod);
    let mut host = crate::module_host::ModuleHost::default();
    let sheet = |dark| desktop_style::StyleSheet::load_with_appearance(desktop_style::DesktopStyle::Android, dark);
    host.apply_style(&mut cx, &sheet(false));
    let module = &octosense_appstore::cardapp::CARD_MODULE;
    let open = module.open_schema().validate("{\"app\":\"os.calendar\"}", &[]).unwrap();
    host.create(&mut cx, 1, module, open, dvec2(390.0, 700.0)).unwrap();
    let root = host.get(1).unwrap().root.clone();
    let owner = host.get(1).unwrap().vm_id;
    let tile = cx.with_vm(|vm| {
        script_eval!(vm, {mod.wm_theme = {background: #ffffff}});
        crate::module_view::script_mod(vm);
        let value = script_eval!(vm, {use mod.widgets.* MpModuleView{}});
        WidgetRef::script_from_value(vm, value)
    });
    tile.borrow_mut::<MpModuleView>().unwrap().set_root(&mut cx, 1, owner, root.clone());
    set_card_body(&mut cx, &root, owner, r#"
fn populate(subject, message){
    ui.subject.set_text(subject)
    ui.message_body.set_text(message)
    ui.workspace.set_visible(true)
}
SolidView{width:Fill height:Fill flow:Down padding:16 spacing:10
workspace := View{width:Fill height:Fill flow:Down spacing:10 visible:false
subject := Label{width:Fill text:"" draw_text.text_style:theme.font_bold{font_size:18}}
message_view := View{width:Fill height:Fill flow:Down spacing:10
ScrollYView{width:Fill height:Fill flow:Down
message_body := Label{width:Fill text:"" draw_text.text_style.font_size:16}
}}
Button{text:"Compose reply"}
explicit_line := Label{width:Fill flow:Flow.Right{wrap:false} text:"Explicitly single line content stays single line across every retained restyle"}
input := TextInput{width:Fill text:"Draft"}
}
}
}
"#);
    widget_tree::set_ui_root(&mut cx, &root);
    draw_card(&mut cx, &tile, 390.0, 700.0);
    let subject=root.label(&cx,ids!(subject));
    let message=root.label(&cx,ids!(message_body));
    let subject_text = "A longer appointment title requiring more than one line on a narrow phone display";
    let message_text = "Hello,\n\nYour appointment is Tuesday, October 6 at 9:00 AM Pacific. Please confirm this time or suggest another appointment.\n\nCedar Clinic";
    // Match an async app callback: initially blank labels live in a hidden
    // workspace, then the app's own script populates and shows them.
    assert!(root.splash(&cx, ids!(card)).call_script_fn_with_strings(
        &mut cx, live_id!(populate), &[subject_text, message_text]
    ));
    // ui.* setters suspend script execution until the widget task pump runs,
    // as they do after an actual host callback.
    makepad_widgets::makepad_platform::makepad_script_std::handle_script_tasks(&mut cx);
    let subject_uid = subject.widget_uid();
    let message_uid = message.widget_uid();
    let input = root.text_input(&cx, ids!(input));
    let input_uid = input.widget_uid();
    input.set_text(&mut cx, "Unsaved draft");
    for restyle in [None,Some(true),Some(false)] {
        if let Some(dark)=restyle {host.apply_style(&mut cx,&sheet(dark));}
        for (width,height) in [(390.0,700.0),(390.0,320.0)] {
            draw_card(&mut cx,&tile,width,height);
            for (name,label) in [("subject",&subject),("message_body",&message)] {
                let area=label.area().rect(&cx);
                let text=label.borrow().unwrap().text_layout_rect;
                assert!(text.size.x <= area.size.x, "{restyle:?} {name}: text overflows {text:?}, frame {area:?}");
                assert!(area.size.x<=width-32.0 && area.size.x>0.0);
                assert!(area.size.y>40.0,"expected wrapped text after blank-set_text/show/restyle");
            }
            let nowrap = root.label(&cx, ids!(explicit_line));
            let nowrap = nowrap.borrow().unwrap();
            assert!(nowrap.text_layout_rect.size.x > nowrap.area().rect(&cx).size.x, "explicit no-wrap must be respected");
            assert!(nowrap.text_layout_rect.size.y < 40.0);
            assert_eq!(subject.widget_uid(), subject_uid);
            assert_eq!(message.widget_uid(), message_uid);
            assert_eq!(subject.text(), subject_text);
            assert_eq!(message.text(), message_text);
            assert_eq!(root.text_input(&cx, ids!(input)).widget_uid(), input_uid);
            assert_eq!(input.text(), "Unsaved draft");
        }
    }
    host.teardown(&mut cx,1);
}
