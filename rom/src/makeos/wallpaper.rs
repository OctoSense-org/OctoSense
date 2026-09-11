//! Cover the desktop with the bundled vector scene without patching Image.
use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    mod.widgets.MakeosWallpaper = set_type_default() do #(MakeosWallpaper::register_widget(vm)) {
        width: Fill height: Fill
        visible: false
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct MakeosWallpaper {
    #[uid] uid: WidgetUid,
    #[source] source: ScriptObjectRef,
    #[walk] walk: Walk,
    #[visible] #[live] visible: bool,
    #[redraw] #[live] draw_svg: DrawSvg,
}

fn cover_rect(rect: Rect, aspect: f64) -> Rect {
    if !(rect.size.x > 0.0 && rect.size.y > 0.0 && aspect > 0.0) {
        return rect;
    }
    let size = if rect.size.x / rect.size.y > aspect {
        dvec2(rect.size.x, rect.size.x / aspect)
    } else {
        dvec2(rect.size.y * aspect, rect.size.y)
    };
    Rect { pos: rect.pos + (rect.size - size) * 0.5, size }
}

impl Widget for MakeosWallpaper {
    fn handle_event(&mut self, _cx: &mut Cx, _event: &Event, _scope: &mut Scope) {}

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        if !self.visible { return DrawStep::done(); }
        if self.draw_svg.svg_doc.is_none() {
            self.draw_svg.load_from_str(crate::theme::BUNDLED_MAKEOS_WALLPAPER);
        }
        let walk = cx.resolve_walk(walk, ResolveAt::BeforeBegin);
        let rect = cx.walk_turtle(walk);
        let size = self.draw_svg.content_size;
        let aspect = if size.x > 0.0 && size.y > 0.0 { size.x / size.y } else { 0.0 };
        self.draw_svg.render_to_rect(cx, &cover_rect(rect, aspect), 0.0);
        DrawStep::done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_covers_and_centers_for_wide_and_tall_windows() {
        for size in [dvec2(1920.0, 1080.0), dvec2(1000.0, 1000.0)] {
            let viewport = Rect { pos: dvec2(10.0, 20.0), size };
            let scene = cover_rect(viewport, 1.6);
            assert!(scene.size.x >= size.x && scene.size.y >= size.y);
            assert!((scene.size.x / scene.size.y - 1.6).abs() < 1e-10);
            assert_eq!(scene.pos + scene.size * 0.5, viewport.pos + size * 0.5);
        }
    }
}
