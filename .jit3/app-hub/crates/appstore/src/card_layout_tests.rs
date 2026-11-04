//! Real native layout of a measured L0 bundle in a nonzero host viewport.
use makepad_widgets::{makepad_draw::cx_draw::CxDraw, *};
use serde_json::json;
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "card-layout-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("kit/native/light")).unwrap();
        std::fs::write(root.join("page.card"), r#"
theme light
component PositionedSurface(instance: text) { view Kit(component: "surface", instance: instance) { slot } }
component PositionedControl(instance: text) { view Kit(component: "control", instance: instance) }
view root PositionedSurface(instance: "page") {
    PositionedSurface(instance: "viewfinder") {}
    PositionedControl(instance: "shutter")
}
"#).unwrap();
        std::fs::write(root.join("kit/native/light/kit.json"), json!({
            "schema_version": 1, "theme": "light", "tokens": {}, "components": {
                "surface": {"props": {}, "slot": true, "style": {"t":"stack","variant":"surface","bg":4294967295u32}},
                "control": {"props": {}, "slot": false, "style": {"t":"button","enabled":1}}
            }
        }).to_string()).unwrap();
        std::fs::write(
            root.join("page.data.json"),
            json!({"$kit":{"theme":"light","placements":{
                "page":{"component":"surface","layout":{"x":0,"y":0,"w":406,"h":776}},
                "viewfinder":{"component":"surface","layout":{"x":10,"y":40,"w":386,"h":541}},
                "shutter":{"component":"control","layout":{"x":171,"y":660,"w":64,"h":64}}
            }}})
            .to_string(),
        )
        .unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn mount(source: &str) -> (Cx, WidgetRef) {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    crate::register_card_vocabulary();
    let root = cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        let value = script_eval!(vm, {use mod.prelude.widgets.* View{width:Fill height:Fill card := Splash{width:Fill height:Fill}}});
        WidgetRef::script_from_value(vm, value)
    });
    root.splash(&mut cx, ids!(card)).set_text(&mut cx, source);
    widget_tree::set_ui_root(&mut cx, &root);
    (cx, root)
}

fn draw(cx: &mut Cx, root: &WidgetRef, rect: Rect) {
    let size = dvec2(1200.0, 1000.0);
    let pass = DrawPass::new(cx);
    pass.set_size(cx, size);
    let mut list = DrawList2d::new(cx);
    let event = DrawEvent::default();
    let mut draw = CxDraw::new(cx, &event);
    let mut draw = Cx2d::new(&mut draw);
    draw.begin_pass(&pass, Some(1.0));
    list.begin_always(&mut draw);
    draw.begin_root_turtle(size, Layout::default());
    root.draw_walk_all(&mut draw, &mut Scope::empty(), Walk::abs_rect(rect));
    draw.end_turtle();
    list.end(&mut draw);
    draw.end_pass(&pass);
}

#[test]
fn measured_card_tracks_the_host_origin_and_scroll_reveals_its_bottom_control() {
    let fixture = Fixture::new();
    let source = crate::card_source(&fixture.0, "http://127.0.0.1:12345/").unwrap();
    let (mut cx, root) = mount(&source);
    for pos in [dvec2(0.0, 0.0), dvec2(84.0, 138.0), dvec2(250.0, 60.0)] {
        let viewport = Rect {
            pos,
            size: dvec2(300.0, 500.0),
        };
        draw(&mut cx, &root, viewport);
        let scroller = root.view(&cx, ids!(l0_canvas));
        scroller.set_scroll_pos(&mut cx, dvec2(0.0, 0.0));
        draw(&mut cx, &root, viewport);
        let panel = root.view(&cx, ids!(viewfinder)).area().rect(&cx);
        assert_eq!(panel.pos, pos + dvec2(10.0, 40.0));
        assert_eq!(panel.size, dvec2(386.0, 541.0));
        let shutter = root.button(&cx, ids!(shutter));
        assert_eq!(shutter.area().rect(&cx).pos, pos + dvec2(171.0, 660.0));
        scroller.set_scroll_pos(&mut cx, dvec2(100.0, 250.0));
        draw(&mut cx, &root, viewport);
        // Native layout is available without a GPU shader compilation. Pixel
        // clipping and wheel input are covered by the native host acceptance.
        let visible = shutter.area().rect(&cx);
        assert_eq!(visible.pos, pos + dvec2(71.0, 410.0));
        assert_eq!(visible.size, dvec2(64.0, 64.0));
        assert!(viewport.contains(visible.pos));
        assert!(viewport.contains(visible.pos + visible.size - dvec2(0.1, 0.1)));
    }
}

#[test]
fn script_apps_keep_their_own_layout_and_scrolling() {
    let fixture = Fixture::new();
    let source = "width:Fill height:Fill own := View{width:Fill height:Fill}";
    std::fs::write(fixture.0.join("main.splash"), source).unwrap();
    assert_eq!(
        crate::card_source(&fixture.0, "http://127.0.0.1:12345/").unwrap(),
        source
    );
}

#[test]
fn flow_card_fills_its_resized_pane_without_measured_placements() {
    let fixture = Fixture::new();
    std::fs::write(
        fixture.0.join("page.card"),
        "theme dark\nview root Col { Rule() Rule() }\n",
    )
    .unwrap();
    std::fs::write(fixture.0.join("page.data.json"), "{}").unwrap();
    // A minimal role kit with responsive dimensions, no native placement ledger.
    for name in [
        "_palette_dark.octoscript",
        "_derive_color.octoscript",
        "_derive.octoscript",
    ] {
        std::fs::write(fixture.0.join("kit").join(name), "").unwrap();
    }
    std::fs::write(
        fixture.0.join("kit/_kit.octoscript"),
        r#"
fn l0_col(kids) { return {t: "column", fillw: 1, fith: 1, c: kids} }
fn l0_rule() { return {t: "button", fillw: 1, h: 44, text: "Action"} }
"#,
    )
    .unwrap();
    let source = crate::card_source(&fixture.0, "http://127.0.0.1:12345/").unwrap();
    let (mut cx, root) = mount(&source);
    for width in [300.0, 640.0, 240.0] {
        let pos = dvec2(84.0, 138.0);
        draw(
            &mut cx,
            &root,
            Rect {
                pos,
                size: dvec2(width, 500.0),
            },
        );
        let first = root.button(&cx, ids!(beauty_0_0)).area().rect(&cx);
        let second = root.button(&cx, ids!(beauty_0_1)).area().rect(&cx);
        assert_eq!(first.pos.x, pos.x);
        assert_eq!(second.pos.x, pos.x);
        assert_eq!(first.size, dvec2(width, 44.0));
        assert_eq!(second.size, dvec2(width, 44.0));
        assert!(first.pos.y >= pos.y);
        assert!(second.pos.y >= first.pos.y + first.size.y);
        assert!(second.pos.y + second.size.y < pos.y + 500.0);
    }
}

#[test]
fn invalid_measured_kit_does_not_silently_use_the_role_fallback() {
    let fixture = Fixture::new();
    let path = fixture.0.join("kit/native/light/kit.json");
    let mut kit: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    kit["components"]["surface"]["style"]["t"] = json!("column");
    std::fs::write(path, kit.to_string()).unwrap();
    assert!(crate::card_source(&fixture.0, "http://127.0.0.1:12345/").is_err());
}
