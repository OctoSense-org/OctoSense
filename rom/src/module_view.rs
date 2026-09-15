//! A tile hosting an in-process module instance (aicontrol.md §3).
//!
//! Where `MpRunView` presents a child process's swapchain, this widget
//! draws an instance's ROOT — a widget minted inside the instance's own
//! splash isolate by `module_host.rs` — as a subtree of the desk, at the
//! rect the layout gives the tile. Pointer events reach the root as they
//! reach any widget (coordinates stay absolute: `Area` hit-testing is
//! `Cx`-absolute); keys reach it only while the window manager's focus is
//! on this tile, which is the gate a process tile gets from its swapchain
//! forwarding and a module tile has to make explicit. A press inside the
//! tile tells the WM to focus it, the way a press on a process tile does.
//!
//! The root is set by the host after `create` and cleared BEFORE the
//! instance's isolate is freed: the tile outlives the instance by its
//! close animation, and a widget whose heap is gone must not be drawn.

use crate::hub::ClientId;
use crate::run_view::MpRunViewAction;
use crate::tile::TileHost;
use makepad_widgets::widget_async::{enter_isolate, leave_isolate};
use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.MpModuleViewBase = #(MpModuleView::register_widget(vm))

    mod.widgets.MpModuleView = set_type_default() do mod.widgets.MpModuleViewBase {
        width: Fill
        height: Fill
        // The instance's ground: the theme's window background, so a root
        // that paints only its own chrome still sits on the desk's colour.
        draw_bg +: { color: mod.wm_theme.background }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct MpModuleView {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[redraw]
    #[live]
    draw_bg: DrawColor,
    #[rust]
    root: Option<WidgetRef>,
    #[rust]
    client: Option<ClientId>,
    /// The isolate the root was minted in: installed on `Cx` around every
    /// draw and every event the root sees, so its lazily made children,
    /// first-draw shader compiles and callbacks resolve in their own heap.
    #[rust]
    vm_id: SplashVmId,
    #[rust]
    area: Area,
    /// FOCUS RULE (see `MpRunView`): a preview never takes the keyboard.
    #[rust(true)]
    takes_key_focus: bool,
    /// The WM's focus is on this tile: keys reach the root.
    #[rust]
    focused: bool,
    /// The root has drawn at least once.
    #[rust]
    drawn: bool,
    /// The popin fade the desk drives; a module tile has no frozen frame to
    /// fade, so the ground follows it and the root draws solid.
    #[rust(1.0f32)]
    fade: f32,
}

impl MpModuleView {
    /// Seat an instance's root here. The WM's view of the client is set
    /// with it so a press can name the tile to focus.
    pub fn set_root(&mut self, cx: &mut Cx, client: ClientId, vm_id: SplashVmId, root: WidgetRef) {
        cx.widget_tree_insert_child(self.uid, live_id!(root), root.clone());
        self.root = Some(root);
        self.client = Some(client);
        self.vm_id = vm_id;
        self.drawn = false;
        self.draw_bg.redraw(cx);
    }

    /// Drop the root — called by the host right before the instance's
    /// isolate is freed. The tile keeps drawing its ground through the
    /// close animation, nothing else.
    pub fn clear_root(&mut self, cx: &mut Cx) {
        self.root = None;
        self.focused = false;
        self.draw_bg.redraw(cx);
    }

    pub fn root(&self) -> Option<WidgetRef> {
        self.root.clone()
    }
}

impl TileHost for MpModuleView {
    fn client(&self) -> Option<ClientId> {
        self.client
    }

    fn set_status_line(&mut self, _cx: &mut Cx, _line: &str) {
        // A module has no build, no exec scan, no stdout: nothing to show.
    }

    /// The WM's focus lands here: keys may reach the root from now on.
    /// The widget INSIDE that holds the keyboard is the root's own affair
    /// — a cell the person clicked, a text field — so this never moves
    /// the key focus itself (a process tile must, to forward keys; a
    /// module's widgets are in this very tree and claim it themselves).
    fn focus_keyboard(&mut self, cx: &mut Cx) -> bool {
        if !self.takes_key_focus {
            return true;
        }
        if !self.area.is_valid(cx) {
            return false;
        }
        self.focused = true;
        true
    }

    fn release_keyboard(&mut self, cx: &mut Cx) {
        self.focused = false;
        // Whatever inside held the keyboard must let go too, or a field in
        // a tile behind the pane would keep eating keys.
        cx.set_key_focus(Area::Empty);
    }

    fn set_takes_key_focus(&mut self, on: bool) {
        self.takes_key_focus = on;
    }

    fn set_remote_cursor(&mut self, _cx: &mut Cx, _cursor: MouseCursor) {}

    fn has_frame(&self) -> bool {
        self.drawn
    }

    fn arrival_fade(&self) -> f32 {
        1.0
    }

    fn set_target_size(&mut self, _size: Option<Vec2d>) {}

    fn set_close_crop(&mut self, _crop: Option<(Vec2d, Vec2d)>) {}

    fn set_fade(&mut self, fade: f32) {
        self.fade = fade;
    }
}

/// Where a pointer event starts relative to the tile's rect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PointerStart {
    /// A press, a new touch or a wheel step inside the tile.
    Inside,
    /// One that begins outside it: not this tile's event.
    Outside,
    /// Not a pointer start (a move, a release, a key, a frame...).
    None,
}

/// Classify `event` against the tile's rect (`None` while the tile has not
/// drawn yet: nothing is inside a rect that does not exist). Moves and
/// releases pass through: a finger the root captured inside keeps reaching
/// it wherever it goes, as for any widget.
fn pointer_start(event: &Event, rect: Option<Rect>) -> PointerStart {
    let abs = match event {
        Event::MouseDown(e) => Some(e.abs),
        Event::Scroll(e) => Some(e.abs),
        Event::TouchUpdate(update) => match update.touches.iter()
            .find(|point| point.state == makepad_platform::event::TouchState::Start) {
            Some(point) => Some(point.abs),
            None => return PointerStart::None,
        },
        _ => None,
    };
    match (abs, rect) {
        (None, _) => PointerStart::None,
        (Some(abs), Some(rect)) if rect.contains(abs) => PointerStart::Inside,
        (Some(_), _) => PointerStart::Outside,
    }
}

impl Widget for MpModuleView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let Some(root) = self.root.clone() else {
            return;
        };
        // Keys only while the WM focus is here: a text field inside a tile
        // in the background must not eat what the person types elsewhere.
        if matches!(event, Event::KeyDown(_) | Event::KeyUp(_) | Event::TextInput(_)) && !self.focused {
            return;
        }
        // The root is a whole app: its widgets hit-test by their own areas,
        // which cover exactly the tile, but a press, a touch or a wheel
        // that begins OUTSIDE the tile is not this instance's to see — it
        // is the shell's (a band, the shade, another tile). A process tile
        // gets the same gate from `event.hits(cx, self.area)`.
        let rect = self.area.is_valid(cx).then(|| self.area.rect(cx));
        match pointer_start(event, rect) {
            PointerStart::Outside => return,
            PointerStart::Inside => {
                if let Some(client) = self.client {
                    // The WM moves focus here (and back to us through
                    // `focus_keyboard`), exactly as for a process tile.
                    cx.widget_action(self.uid, MpRunViewAction::Clicked { client });
                }
            }
            PointerStart::None => {}
        }
        let entry = enter_isolate(cx, self.vm_id);
        root.handle_event(cx, event, scope);
        leave_isolate(cx, entry);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let rect = cx.turtle().rect();
        if self.root.is_some() {
            // Read the instance's current theme: the host's module-view
            // template was registered before mobile/light styles were applied.
            // Transparent app roots must not get dark text on an old dark ground.
            if let Some(color) = cx.with_script_vm_id_trusted(self.vm_id,
                |vm| script_eval!(vm, {mod.theme.color_bg_app})).as_color() {
                self.draw_bg.color = Vec4f::from_u32(color);
            }
        }
        self.draw_bg.draw_abs(cx, rect);
        if let Some(root) = self.root.clone() {
            let entry = enter_isolate(cx, self.vm_id);
            root.draw_walk_all(cx, scope, Walk::fill());
            leave_isolate(cx, entry);
            self.drawn = true;
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_platform::event::{TouchPoint, TouchState, TouchUpdateEvent};

    fn tile() -> Option<Rect> {
        Some(Rect { pos: dvec2(0.0, 42.0), size: dvec2(412.0, 826.0) })
    }
    fn touch(abs: Vec2d, state: TouchState) -> Event {
        Event::TouchUpdate(TouchUpdateEvent {
            time: 0.0,
            window_id: WindowId(0, 0),
            modifiers: Default::default(),
            touches: vec![TouchPoint { state, abs, time: 0.0, uid: 1, rotation_angle: 0.0, force: 0.0, radius: dvec2(1.0, 1.0), handled: Default::default(), sweep_lock: Default::default() }],
        })
    }

    /// A finger in the shell's status band or navigation band never starts
    /// inside the tile; one on the app does; the rest of its stroke passes.
    #[test]
    fn only_pointer_starts_inside_the_tile_reach_the_root() {
        assert_eq!(pointer_start(&touch(dvec2(200.0, 20.0), TouchState::Start), tile()), PointerStart::Outside, "status band");
        assert_eq!(pointer_start(&touch(dvec2(200.0, 880.0), TouchState::Start), tile()), PointerStart::Outside, "navigation band");
        assert_eq!(pointer_start(&touch(dvec2(200.0, 400.0), TouchState::Start), tile()), PointerStart::Inside);
        assert_eq!(pointer_start(&touch(dvec2(200.0, 20.0), TouchState::Move), tile()), PointerStart::None, "a move passes wherever it is");
        assert_eq!(pointer_start(&touch(dvec2(200.0, 20.0), TouchState::Stop), tile()), PointerStart::None);
        // Before the first draw there is no rect: nothing is inside it.
        assert_eq!(pointer_start(&touch(dvec2(200.0, 400.0), TouchState::Start), None), PointerStart::Outside);
        assert_eq!(pointer_start(&Event::Startup, tile()), PointerStart::None);
    }
}
