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
    let source = PHOTOS.split_once("\nstart_timeout(").unwrap().0;
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
    let mut cx = Cx::new(Box::new(|_, _| {}));
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

fn set_card_body(cx: &mut Cx, root: &WidgetRef, outer: SplashVmId, body: &str) {
    with_isolate(cx, outer, |cx| {
        root.splash(cx, ids!(card)).set_text(cx, body)
    });
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
