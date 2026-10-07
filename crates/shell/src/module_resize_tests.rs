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
    let probe = format!("{}\n{}", include_str!("../../../apps/interface.splash"),
        r#"let ink = ui_ink
        SolidView{draw_bg.color: ui_page label := UiTitle{text: "Theme" draw_text.color: ink} input := UiField{text: "Draft"}}"#);
    set_card_body(&mut cx, &root, owner, &probe);
    draw_card(&mut cx, &tile, 400.0, 700.0);
    let label = root.label(&mut cx, ids!(label));
    let light = label.borrow().unwrap().draw_text.color;
    root.text_input(&mut cx, ids!(input)).set_text(&mut cx, "Unsaved draft");
    host.apply_style(&mut cx, &sheet(true));
    draw_card(&mut cx, &tile, 400.0, 700.0);
    let dark = label.borrow().unwrap().draw_text.color;
    assert_ne!(light, dark, "the Card runner must restyle its hosted app, not only its outer view");
    assert_eq!(root.text_input(&mut cx, ids!(input)).text(), "Unsaved draft");
    host.teardown(&mut cx, 1);
}
