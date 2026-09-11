//! Compatibility with Makepad 74b63be8's retained GPU working-set scan.
//! Its pass iterator includes retired pool slots, which can still name freed
//! draw lists after a style reload. Detach those passes before GPU submission.
use makepad_widgets::*;

pub fn clear_retired_roots(cx: &mut Cx) {
    for pass in cx.passes.id_iter() {
        if cx.passes[pass].main_draw_list_id.is_some_and(|id| cx.draw_lists.is_id_freed(id)) {
            let pass = &mut cx.passes[pass];
            pass.main_draw_list_id = None;
            pass.parent = CxDrawPassParent::None;
            pass.live_with_parent = false;
            pass.paint_dirty = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_roots_are_cleared_without_detaching_live_passes() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let retired_pass = DrawPass::new(&mut cx);
        let retired_list = DrawList::new(&mut cx);
        let retired_id = retired_list.draw_list_id();
        cx.passes[retired_pass.draw_pass_id()].main_draw_list_id = Some(retired_id);
        drop(retired_list);
        let live_pass = DrawPass::new(&mut cx);
        let live_list = DrawList::new(&mut cx);
        let live_id = live_list.draw_list_id();
        cx.passes[live_pass.draw_pass_id()].main_draw_list_id = Some(live_id);
        assert!(cx.draw_lists.is_id_freed(retired_id));
        assert!(!cx.draw_lists.is_id_freed(live_id));
        cx.passes[retired_pass.draw_pass_id()].parent = CxDrawPassParent::DrawPass(live_pass.draw_pass_id());
        cx.passes[retired_pass.draw_pass_id()].live_with_parent = true;
        cx.passes[retired_pass.draw_pass_id()].paint_dirty = true;
        clear_retired_roots(&mut cx);
        assert_eq!(cx.passes[retired_pass.draw_pass_id()].main_draw_list_id, None);
        assert_eq!(cx.passes[live_pass.draw_pass_id()].main_draw_list_id, Some(live_id));
        let retired = &cx.passes[retired_pass.draw_pass_id()];
        assert!(matches!(retired.parent, CxDrawPassParent::None));
        assert!(!retired.live_with_parent);
        assert!(!retired.paint_dirty);
    }
}
