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
//!
//! The tile is also the fault line (ADR 0004 plan step 9). The root's
//! events and draw run under `module_host::contain`: a panic there is
//! caught HERE, not by the platform. The isolate and the script VM come
//! back on their own on the way up (`with_isolate`); a draw cuts the draw
//! context's stacks — turtles, clips, draw lists, passes, the overlay
//! scope, gauss captures — back to where the tile's own draw stood
//! (`Cx2d::unwind_mark` / `unwind_to`) and restores the modal bounds, so
//! the desk's frame still ends. The tile lets go of the root and the
//! keyboard at once; the shell then shows the app closed with a Restart
//! (`show_failed`) and frees the instance in the host's order.

use crate::hub::ClientId;
use crate::module_host::{contain, is_failed};
use crate::run_view::MpRunViewAction;
use crate::tile::TileHost;
use makepad_widgets::gauss_view::CaptureGauss;
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
        // The app closed after an error: a line saying so, and Restart.
        draw_note +: {
            text_style: theme.font_regular
            text_style.font_size: 13
        }
        draw_button +: { color: #0a84ff }
        draw_button_text +: {
            color: #fff
            text_style: theme.font_regular
            text_style.font_size: 13
        }
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
    #[live]
    draw_note: DrawText,
    #[live]
    draw_button: DrawColor,
    #[live]
    draw_button_text: DrawText,
    /// The instance panicked: nothing reaches its root again. `Some` with
    /// the line the tile shows once the shell has named the app.
    #[rust]
    stopped: Option<String>,
    /// Where the Restart button was drawn.
    #[rust]
    restart_rect: Option<Rect>,
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
        self.stopped = None;
        self.restart_rect = None;
        self.draw_bg.redraw(cx);
    }

    /// The instance's module panicked (here or in a host call): let go of
    /// the root and of the keyboard — nothing draws or dispatches to it
    /// again — and show the app closed. The shell names it (`label`) when
    /// it drains the fault; until then the tile says so plainly.
    pub fn show_failed(&mut self, cx: &mut Cx, label: &str) {
        self.stop(cx);
        self.stopped = Some(format!("{label} stopped after an error"));
    }

    /// Whether this tile shows a failed instance.
    pub fn failed(&self) -> bool {
        self.stopped.is_some()
    }

    fn stop(&mut self, cx: &mut Cx) {
        if self.root.take().is_some() || self.focused {
            // A field inside the root may hold the keyboard (and the IME).
            if self.focused {
                cx.set_key_focus(Area::Empty);
            }
        }
        self.focused = false;
        if self.stopped.is_none() {
            self.stopped = Some("The app stopped after an error".to_string());
        }
        self.draw_bg.redraw(cx);
    }

    /// The closed face: a line and a Restart button, centred.
    fn draw_stopped(&mut self, cx: &mut Cx2d, rect: Rect) {
        let Some(line) = self.stopped.clone() else { return };
        let ground = self.draw_bg.color;
        let luminance = 0.299 * ground.x + 0.587 * ground.y + 0.114 * ground.z;
        self.draw_note.color = if luminance > 0.5 { vec4(0.11, 0.11, 0.12, 1.0) } else { vec4(0.94, 0.94, 0.96, 1.0) };
        cx.begin_turtle(Walk::abs_rect(rect), Layout { align: Align { x: 0.5, y: 0.5 }, spacing: 14.0, ..Layout::flow_down() });
        self.draw_note.draw_walk(cx, Walk::fit(), Align::default(), &line);
        let button = cx.walk_turtle(Walk::fixed(120.0, 36.0));
        self.draw_button.draw_abs(cx, button);
        cx.begin_turtle(Walk::abs_rect(button), Layout { align: Align { x: 0.5, y: 0.5 }, ..Layout::default() });
        self.draw_button_text.draw_walk(cx, Walk::fit(), Align::default(), "Restart");
        cx.end_turtle();
        self.restart_rect = Some(button);
        cx.end_turtle();
    }

    /// A press on the closed face: Restart, or just focus the tile.
    fn handle_stopped_event(&mut self, cx: &mut Cx, event: &Event) {
        let abs = match event {
            Event::MouseDown(e) => Some(e.abs),
            Event::TouchUpdate(update) => update.touches.iter()
                .find(|point| point.state == makepad_platform::event::TouchState::Start)
                .map(|point| point.abs),
            _ => None,
        };
        let (Some(abs), Some(client)) = (abs, self.client) else { return };
        if self.restart_rect.is_some_and(|r| r.contains(abs)) {
            cx.widget_action(self.uid, MpRunViewAction::Restart { client });
        } else if self.area.is_valid(cx) && self.area.rect(cx).contains(abs) {
            cx.widget_action(self.uid, MpRunViewAction::Clicked { client });
        }
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

/// The claim cell of a press or new touch no tile in front has taken yet
/// (a wheel step has none: it raises nothing).
fn unclaimed_press(event: &Event) -> Option<&std::cell::Cell<Area>> {
    let handled = match event {
        Event::MouseDown(e) => &e.handled,
        Event::TouchUpdate(update) => &update.touches.iter()
            .find(|point| point.state == makepad_platform::event::TouchState::Start)?
            .handled,
        _ => return None,
    };
    handled.get().is_empty().then_some(handled)
}

impl Widget for MpModuleView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if self.stopped.is_some() {
            self.handle_stopped_event(cx, event);
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        // Failed through another path (a host call) since the last event.
        if is_failed(cx, self.vm_id) {
            self.stop(cx);
            return;
        }
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
        let start = pointer_start(event, rect);
        if start == PointerStart::Outside {
            return;
        }
        // A press inside, with the cell that records who claimed it. Tiles
        // see input topmost first (desk.rs), so a claimed press belongs to a
        // tile in front: overlapping windows of one app (its extra windows)
        // must not raise the one behind.
        let press = if start == PointerStart::Inside { unclaimed_press(event) } else { None };
        if press.is_some() {
            if let Some(client) = self.client {
                // The WM moves focus here (and back to us through
                // `focus_keyboard`), exactly as for a process tile.
                cx.widget_action(self.uid, MpRunViewAction::Clicked { client });
            }
        }
        if contain(cx, self.vm_id, "an event", |cx| root.handle_event(cx, event, scope)).is_none() {
            self.stop(cx);
            return;
        }
        // A press inside this tile is this tile's, even where none of the
        // app's widgets took it, so no window behind reacts to it.
        if let Some(handled) = press {
            if handled.get().is_empty() {
                handled.set(self.area);
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let rect = cx.turtle().rect();
        if self.root.is_some() && is_failed(cx, self.vm_id) {
            self.stop(cx);
        }
        if self.root.is_some() {
            // Read the instance's current theme: the host's module-view
            // template was registered before mobile/light styles were applied.
            // Transparent app roots must not get dark text on an old dark ground.
            let vm_id = self.vm_id;
            let ground = contain(cx, vm_id, "its theme", |cx| {
                cx.with_script_vm_id_trusted(vm_id, |vm| script_eval!(vm, {mod.theme.color_bg_app})).as_color()
            });
            match ground {
                Some(Some(color)) => self.draw_bg.color = Vec4f::from_u32(color),
                Some(None) => {}
                None => self.stop(cx),
            }
        }
        self.draw_bg.draw_abs(cx, rect);
        if let Some(root) = self.root.clone() {
            // The instance's modals dim and centre within this tile: app
            // overlays (Rinx's mini-app and editor modals) stay in this
            // viewport, including its origin when the phone shell draws the
            // module into a shifted capture.
            let outer = std::mem::replace(&mut cx.global::<ModalBounds>().0, Some(rect));
            // What a panicking draw leaves open is cut back to here.
            //
            // WHAT A MID-DRAW PANIC LEAVES BEHIND. A Makepad draw only
            // RECORDS: no GPU call is made during `Event::Draw` — the
            // platform walks the recorded passes and lists after the event
            // returns, binding textures and render targets itself. So a
            // guest's panic cannot leave a texture bound or a render pass
            // open on the GPU. What it can leave is CPU-side state the next
            // GPU submission reads:
            //
            // Restored here: the draw context's stacks — turtles, finished
            // rows and walks, clips, the alignment list, the draw-call
            // parent chain, the pass stack, the draw-list stack, the
            // overlay scope, the nesting depth (`unwind_to`); the gauss
            // capture scope (`unwind_scope_to`); `ModalBounds`; and the
            // isolate and script VM (`with_isolate`). The desk's own
            // `end_*` calls then pair and its frame ends.
            //
            // NOT restored, and possibly inconsistent for the next frame:
            // - instances the guest already appended to the tile's (the
            //   desk's) draw list this frame stay in it for THIS frame, and
            //   a draw list or pass the guest began is left half-recorded —
            //   the platform may render that partial list once; they go when
            //   the root is dropped at release and the tile redraws;
            // - render-target textures a guest pass created or resized, and
            //   draw lists it owns, stay allocated until that root drops;
            // - process-wide caches mutated mid-operation: the font atlas
            //   (a glyph slot reserved but only partly rasterized; its dirty
            //   rect is still uploaded next frame), the shaper and layout
            //   caches, shader/geometry pools. Makepad's std Mutexes there
            //   tolerate poisoning, but a half-written atlas region can show
            //   as a garbled glyph until the atlas resets;
            // - any GPU call a module makes itself (FFI, a native layer)
            //   is outside all of this.
            let mark = cx.unwind_mark();
            let captures = CaptureGauss::scope_depth(cx);
            let drawn = contain(cx, self.vm_id, "its draw", |cx| root.draw_walk_all(cx, scope, Walk::fill()));
            if drawn.is_none() {
                cx.unwind_to(mark);
                CaptureGauss::unwind_scope_to(cx, captures);
            }
            cx.global::<ModalBounds>().0 = outer;
            if drawn.is_none() {
                self.stop(cx);
            }
            self.drawn = true;
        }
        if self.stopped.is_some() {
            self.draw_stopped(cx, rect);
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

    /// A new touch is a press this tile may claim; the rest of its stroke
    /// and a frame are not. (A press a window in front already claimed has
    /// a non-empty cell and raises nothing behind it.)
    #[test]
    fn only_a_new_touch_or_button_is_a_press_to_claim() {
        assert!(unclaimed_press(&touch(dvec2(200.0, 400.0), TouchState::Start)).is_some());
        assert!(unclaimed_press(&touch(dvec2(200.0, 400.0), TouchState::Move)).is_none(), "only a start is a press");
        assert!(unclaimed_press(&Event::Startup).is_none());
    }
}
