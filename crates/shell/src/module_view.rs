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

use crate::desktop::DesktopStyle;
use crate::hub::ClientId;
use crate::module_host::{contain, is_failed};
use crate::octosense::style::AppIconDraw;
use crate::run_view::MpRunViewAction;
use crate::tile::TileHost;
use makepad_widgets::button::KeyButtons;
use makepad_widgets::gauss_view::CaptureGauss;
use makepad_widgets::*;
use std::collections::HashSet;

/// Find the focused widget in this instance's own tree. A screen rectangle
/// is not enough: overlapping windows can put two widgets at the same point.
fn focus_widget(root: &WidgetRef, area: Area) -> Option<WidgetUid> {
    if area.is_empty() {
        return None;
    }
    let mut pending = vec![root.clone()];
    let mut seen = HashSet::new();
    while let Some(widget) = pending.pop() {
        let uid = widget.widget_uid();
        if uid.0 == 0 || !seen.insert(uid.0) {
            continue;
        }
        if !widget.visible() {
            continue;
        }
        if widget.area() == area {
            return Some(uid);
        }
        widget.children(&mut |_, child| pending.push(child));
    }
    None
}

/// Resolve a remembered widget to its current area after it has redrawn.
fn focus_area(root: &WidgetRef, wanted: WidgetUid) -> Option<Area> {
    let mut pending = vec![root.clone()];
    let mut seen = HashSet::new();
    while let Some(widget) = pending.pop() {
        let uid = widget.widget_uid();
        if uid.0 == 0 || !seen.insert(uid.0) {
            continue;
        }
        if !widget.visible() {
            continue;
        }
        if uid == wanted {
            return Some(widget.area());
        }
        widget.children(&mut |_, child| pending.push(child));
    }
    None
}

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
        // The app closed after an error: its icon, a line saying so, and
        // Reopen, as a process app's face says it (resources/startup.splash).
        draw_note +: {
            text_style: theme.font_bold
            text_style.font_size: 14
        }
        draw_button +: {
            color: #x0071e3
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(0. 0. self.rect_size.x self.rect_size.y 3.)
                sdf.fill(self.color)
                return sdf.result
            }
        }
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
    /// See `TileHost::set_title_band`.
    #[rust]
    title_band: f64,
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
    /// The instance's last focused widget, not an Area that redraw may retire.
    #[rust]
    focus_bookmark: Option<(WidgetUid, bool)>,
    /// Avoid walking the same tree on every key while focus stays put.
    #[rust]
    checked_focus: Option<(Area, bool)>,
    /// The app draws a sheet whose buttons answer the keyboard: see `sheet_keys`.
    #[rust]
    sheet_up: bool,
    /// One of the app's widgets took the last hover the root was handed
    /// (`hover_claim`): it holds the hover until a move tells it otherwise.
    #[rust]
    hovered: bool,
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
    /// Which app's icon the closed face shows, in which style's frame (`set_face_icon`).
    #[rust]
    face_icon: Option<(String, DesktopStyle)>,
    #[rust]
    app_icons: AppIconDraw,
    /// Out of sight on the phone (`set_asleep`): the app gets no frames and
    /// its redraws are held, but timers, network replies and messages
    /// still reach it, so its state stays current (and audio keeps
    /// playing: a WebView plays on its own).
    #[rust]
    asleep: bool,
    /// The frames its animations waited for while asleep, replayed on wake.
    #[rust]
    owed_frame: Option<NextFrameEvent>,
    /// It asked to redraw while asleep.
    #[rust]
    redraw_owed: bool,
    /// The frame that wakes it: the owed frame and redraw land there,
    /// outside the draw that decided it is visible again.
    #[rust]
    wake_frame: Option<NextFrame>,
    /// The content, slot size and restyle generation (`module_host::style_generation`)
    /// the app last heard as `on_app_resize`.
    #[rust]
    script_viewport: Option<(WidgetUid, Vec2d, u64)>,
    /// Previous hosted bounds, used to reveal a focused editor when the IME
    /// shrinks the app. Resize the app, then scroll its own nearest container.
    #[rust]
    last_viewport_size: Option<Vec2d>,
}

impl MpModuleView {
    /// The frame that will wake it, if it is waking.
    pub fn wake_frame(&self) -> Option<NextFrame> {
        self.wake_frame
    }

    /// The phone puts an app it does not show to sleep, and wakes it when
    /// it shows (or starts to show) again.
    pub fn set_asleep(&mut self, cx: &mut Cx, asleep: bool) {
        if self.asleep == asleep {
            return;
        }
        self.asleep = asleep;
        if !asleep && (self.owed_frame.is_some() || self.redraw_owed) {
            self.wake_frame = Some(cx.new_next_frame());
        }
    }

    /// Seat an instance's root here. The WM's view of the client is set
    /// with it so a press can name the tile to focus.
    pub fn set_root(&mut self, cx: &mut Cx, client: ClientId, vm_id: SplashVmId, root: WidgetRef) {
        cx.widget_tree_insert_child(self.uid, live_id!(root), root.clone());
        self.root = Some(root);
        self.focus_bookmark = None;
        self.checked_focus = None;
        self.client = Some(client);
        self.vm_id = vm_id;
        self.script_viewport = None;
        self.last_viewport_size = None;
        self.drawn = false;
        self.stopped = None;
        self.draw_bg.redraw(cx);
    }

    /// The instance's module panicked (here or in a host call): let go of
    /// the root and of the keyboard — nothing draws or dispatches to it
    /// again — and show the app closed. The shell names it (`label`) when
    /// it drains the fault; until then the tile says so plainly.
    pub fn show_failed(&mut self, cx: &mut Cx, label: &str) {
        self.stop(cx);
        self.stopped = Some(format!("{label} quit unexpectedly"));
    }

    /// Whether this tile shows a failed instance.
    pub fn failed(&self) -> bool {
        self.stopped.is_some()
    }

    /// The desk says, as it draws the tile, whose icon a closed face wears.
    pub fn set_face_icon(&mut self, app: &str, style: DesktopStyle) {
        if self.face_icon.as_ref().is_none_or(|(have, was)| have != app || *was != style) {
            self.face_icon = Some((app.to_string(), style));
        }
    }

    fn stop(&mut self, cx: &mut Cx) {
        if self.root.take().is_some() || self.focused {
            // A field inside the root may hold the keyboard (and the IME).
            if self.focused {
                cx.set_key_focus(Area::Empty);
            }
        }
        self.focused = false;
        self.focus_bookmark = None;
        self.checked_focus = None;
        self.sheet_keys(cx, false);
        self.hovered = false;
        if self.stopped.is_none() {
            self.stopped = Some("The app quit unexpectedly".to_string());
        }
        self.draw_bg.redraw(cx);
    }

    /// The closed face: the app's icon, a line and a Reopen button, centred
    /// as one block. A press is tested against the drawn button (`on_restart`).
    fn draw_stopped(&mut self, cx: &mut Cx2d, rect: Rect) {
        let Some(line) = self.stopped.clone() else { return };
        let ground = self.draw_bg.color;
        let luminance = 0.299 * ground.x + 0.587 * ground.y + 0.114 * ground.z;
        let ink = if luminance > 0.5 { vec4(0.11, 0.11, 0.12, 1.0) } else { vec4(0.94, 0.94, 0.96, 1.0) };
        self.draw_note.color = ink;
        // A phone's finger, or a desktop's pointer and thirteen points.
        let (size, button) = if cfg!(mobile_only) { (13.0, (120.0, 36.0)) } else { (9.75, (96.0, 28.0)) };
        self.draw_button_text.text_style.font_size = size;
        let room = (rect.size.x - 32.0).max(1.0) as f32;
        let words = self.draw_note.layout(cx, 0.0, 0.0, Some(room), true, Align { x: 0.5, y: 0.0 }, &line);
        let tall = words.size_in_lpxs.height as f64;
        let below = tall + 14.0 + button.1;
        // The icon gives way in a window too short for all of it.
        let icon = self.face_icon.clone().filter(|_| rect.size.y >= below + 74.0 + 16.0);
        let middle = rect.pos.x + rect.size.x * 0.5;
        let mut top = rect.pos.y + (rect.size.y - below - if icon.is_some() { 74.0 } else { 0.0 }) * 0.5;
        if let Some((app, style)) = icon {
            let at = Rect { pos: dvec2(middle - 32.0, top), size: dvec2(64.0, 64.0) };
            self.app_icons.draw(cx, &app, style, at, 1.0, ink);
            top += 74.0;
        }
        // The lines are centred in the room they were laid out in, so that's the box
        // they need: one only as wide as the words clips them away.
        let at = Rect { pos: dvec2(middle - room as f64 * 0.5, top), size: dvec2(room as f64, tall) };
        cx.begin_turtle(Walk::abs_rect(at), Layout::default());
        self.draw_note.draw_walk_laidout(cx, Walk::fit(), &words);
        cx.end_turtle();
        let button = Rect { pos: dvec2(middle - button.0 * 0.5, top + tall + 14.0), size: dvec2(button.0, button.1) };
        self.draw_button.draw_abs(cx, button);
        cx.begin_turtle(Walk::abs_rect(button), Layout { align: Align { x: 0.5, y: 0.5 }, ..Layout::default() });
        self.draw_button_text.draw_walk(cx, Walk::fit(), Align::default(), "Reopen");
        cx.end_turtle();
    }

    /// The press at `abs` is on the Restart button, where it is drawn: the
    /// face hit-tests it by its own area, as any widget is hit-tested, so it
    /// follows the face's centring and whatever clips the window.
    fn on_restart(&self, cx: &Cx, abs: Vec2d) -> bool {
        let button = self.draw_button.area();
        button.is_valid(cx) && button.clipped_rect(cx).contains(abs)
    }

    /// A press on the closed face: Restart, or just focus the tile. The face
    /// takes presses as the live tile does: only one inside it that nothing
    /// in front claimed, and it claims what it takes, so a click on a window
    /// over it never restarts the app and a click on it raises no window
    /// behind it. It claims a hover inside it the same way, so nothing behind
    /// it lights up under the face.
    fn handle_stopped_event(&mut self, cx: &mut Cx, event: &Event) {
        let rect = self.area.is_valid(cx).then(|| self.area.rect(cx));
        if let Some((claim, abs)) = hover_claim(cx, event) {
            if claim.get().is_empty() && rect.is_some_and(|r| r.contains(abs)) {
                claim.set(self.area);
            }
            return;
        }
        if pointer_start(event, rect) != PointerStart::Inside {
            return;
        }
        let abs = match event {
            Event::MouseDown(e) => Some(e.abs),
            Event::TouchUpdate(update) => update.touches.iter()
                .find(|point| point.state == makepad_platform::event::TouchState::Start)
                .map(|point| point.abs),
            _ => None,
        };
        let (Some(abs), Some(claim), Some(client)) = (abs, press_claim(event), self.client) else { return };
        if !claim.get().is_empty() {
            return;
        }
        claim.set(self.area);
        if self.on_restart(cx, abs) {
            cx.widget_action(self.uid, MpRunViewAction::Restart { client });
        } else {
            cx.widget_action(self.uid, MpRunViewAction::Clicked { client });
        }
    }

    /// While a sheet whose buttons answer the keyboard shows, it owns the keyboard, as a
    /// Mac's does: nothing under it hears a key. What had the keyboard has it again when
    /// the sheet leaves, unless someone asked for it since. Only this window's: one app's
    /// sheet never takes the keyboard from the window a person is typing in.
    fn sheet_keys(&mut self, cx: &mut Cx, up: bool) {
        if up == self.sheet_up {
            return;
        }
        self.sheet_up = up;
        if up {
            if self.focused {
                cx.sheet_takes_keyboard(self.uid.0);
            }
        } else if let Some((under, pressed)) = cx.sheet_left_keyboard(self.uid.0) {
            let here = cx.key_focus();
            if self.focused && cx.next_key_focus() == here && !here.is_valid(cx) && under.is_valid(cx) {
                cx.restore_key_focus(under, pressed);
            }
        }
    }

    /// Drop the root — called by the host right before the instance's
    /// isolate is freed. The tile keeps drawing its ground through the
    /// close animation, nothing else.
    pub fn clear_root(&mut self, cx: &mut Cx) {
        self.root = None;
        self.focused = false;
        self.focus_bookmark = None;
        self.checked_focus = None;
        self.sheet_keys(cx, false);
        self.hovered = false;
        self.draw_bg.redraw(cx);
    }

    pub fn root(&self) -> Option<WidgetRef> {
        self.root.clone()
    }

    /// Where the closed face drew its Restart (module_input_tests.rs).
    #[cfg(test)]
    pub(crate) fn restart_button(&self, cx: &Cx) -> Option<Rect> {
        let button = self.draw_button.area();
        button.is_valid(cx).then(|| button.rect(cx))
    }
}

impl TileHost for MpModuleView {
    fn set_title_band(&mut self, height: f64) {
        self.title_band = height;
    }

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
        let returning = !self.focused;
        self.focused = true;
        // A widget clicked in this event takes precedence over the bookmark.
        // A sheet above the app's document keeps its keyboard until it leaves.
        if returning && !self.sheet_up && cx.next_key_focus().is_empty() {
            if let (Some(root), Some((uid, pressed))) = (self.root.as_ref(), self.focus_bookmark) {
                let area = contain(cx, self.vm_id, "restoring keyboard focus", |_cx| focus_area(root, uid)).flatten();
                if let Some(area) = area.filter(|area| area.is_valid(cx)) {
                    cx.restore_key_focus(area, pressed);
                }
            }
        }
        true
    }

    fn sheet_owns_keyboard(&self) -> bool {
        self.sheet_up
    }

    fn remember_keyboard(&mut self, cx: &mut Cx) {
        if !self.focused {
            return;
        }
        let current = cx.key_focus();
        let pressed = cx.key_focus_pressed();
        if current.is_empty() || self.checked_focus == Some((current, pressed)) {
            return;
        }
        self.checked_focus = Some((current, pressed));
        if let Some(uid) = self.root.as_ref().and_then(|root| {
            contain(cx, self.vm_id, "remembering keyboard focus", |_cx| focus_widget(root, current)).flatten()
        }) {
            self.focus_bookmark = Some((uid, pressed));
        }
    }

    fn release_keyboard(&mut self, cx: &mut Cx) {
        let current = cx.key_focus();
        self.remember_keyboard(cx);
        let owns_current = self.root.as_ref().is_some_and(|root| {
            contain(cx, self.vm_id, "checking keyboard focus", |_cx| focus_widget(root, current)).flatten().is_some()
        });
        self.focused = false;
        // Do not erase a different widget's focus request from this same
        // press; otherwise clear the old window's keyboard on release.
        if owns_current && cx.next_key_focus() == current {
            cx.set_key_focus(Area::Empty);
        }
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

/// The cell that records who claimed a press or a new touch (a wheel step
/// has none: it raises nothing). Tiles see input topmost first (desk.rs),
/// after the shell's own surfaces over the desk, so a press that reaches a
/// tile already claimed was taken by something in front of it.
fn press_claim(event: &Event) -> Option<&std::cell::Cell<Area>> {
    match event {
        Event::MouseDown(e) => Some(&e.handled),
        Event::TouchUpdate(update) => update.touches.iter()
            .find(|point| point.state == makepad_platform::event::TouchState::Start)
            .map(|point| &point.handled),
        _ => None,
    }
}

/// A hover: a mouse move with no button held, with the cell that records
/// who took it and where it is. Like a press, a hover that reaches a tile
/// already claimed was taken in front of it. A move with a button held is
/// no hover: it belongs to whatever captured the press, and passes wherever
/// it goes.
fn hover_claim<'a>(cx: &Cx, event: &'a Event) -> Option<(&'a std::cell::Cell<Area>, Vec2d)> {
    match event {
        Event::MouseMove(e) if cx.fingers.first_mouse_button.is_none() => Some((&e.handled, e.abs)),
        _ => None,
    }
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
        // Asleep: frames wait (its animations stop asking for more); the
        // newest is kept, with every waiting animation in it.
        if self.asleep {
            if let Event::NextFrame(frame) = event {
                match self.owed_frame.as_mut() {
                    Some(owed) => {
                        owed.set.extend(frame.set.iter().copied());
                        owed.frame = frame.frame;
                        owed.time = frame.time;
                    }
                    None => self.owed_frame = Some(frame.clone()),
                }
                return;
            }
        }
        // Woken: the frames it missed arrive with this one, and its held
        // redraw happens.
        let woken;
        let event = match (event, self.wake_frame) {
            (Event::NextFrame(frame), Some(wake)) if frame.set.contains(&wake) => {
                self.wake_frame = None;
                if std::mem::take(&mut self.redraw_owed) {
                    root.redraw(cx);
                }
                let mut merged = self.owed_frame.take().unwrap_or_else(|| frame.clone());
                merged.set.extend(frame.set.iter().copied());
                merged.frame = frame.frame;
                merged.time = frame.time;
                woken = Event::NextFrame(merged);
                &woken
            }
            _ => event,
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
        // A press inside, with the cell that records who claimed it. One that
        // is claimed already was taken in front of this tile (`press_claim`):
        // by a window over it, this app's own extra windows included, or by
        // a shell surface. Like a press outside, it is not this instance's:
        // it raises nothing, and the root never sees it. The claim alone
        // would not keep it from the app's widgets. Makepad lets a widget
        // co-capture a press another area has claimed
        // (`hits_with_capture_overload`: GestureView's taps, a list's drag, a
        // View's `on_item_tap`), and GestureView follows new touches from
        // the raw stream, so a click on Mail's inbox footer opened the News
        // story under it.
        let press = if start == PointerStart::Inside { press_claim(event) } else { None };
        if press.is_some_and(|claim| !claim.get().is_empty()) {
            return;
        }
        // A hover inside, with its claim (`hover_claim`). One a window in
        // front took is not this instance's either: the root never sees it,
        // so no widget under that window lights up or sets its cursor. The
        // one exception: while a widget of this app still holds the hover
        // from the last move (`hovered`), the root gets this move too, so
        // that widget sees its hover end. Claimed, the move lets no other
        // hover start.
        let hover = hover_claim(cx, event).map(|(claim, abs)| (claim, rect.is_some_and(|r| r.contains(abs))));
        if hover.is_some_and(|(claim, inside)| inside && !claim.get().is_empty() && !self.hovered) {
            return;
        }
        let hover_unclaimed = hover.is_some_and(|(claim, _)| claim.get().is_empty());
        if press.is_some() {
            if let Some(client) = self.client {
                // The WM moves focus here (and back to us through
                // `focus_keyboard`), exactly as for a process tile.
                cx.widget_action(self.uid, MpRunViewAction::Clicked { client });
                // A press on the app's own content: for a few seconds the app
                // may take its window full screen (app_fullscreen.rs). One on
                // a host service's sheet over the app is the host's. Asked
                // before the app sees the press, which may close the sheet.
                match contain(cx, self.vm_id, "its sheet", |cx| crate::app_fullscreen::host_sheet_up(cx, &root)) {
                    Some(false) => cx.widget_action(self.uid, MpRunViewAction::Pressed { client }),
                    Some(true) => {}
                    None => {
                        self.stop(cx);
                        return;
                    }
                }
            }
        }
        // A redraw an asleep app asks for (a timer, a network reply) is held
        // until it wakes: it would redraw, and re-record, the phone's home
        // scene that covers it.
        let held = self.asleep.then(|| {
            let pending = &cx.new_draw_event;
            (pending.draw_lists.len(), pending.draw_lists_and_children.len(), pending.redraw_all)
        });
        // A sheet's buttons answer the keyboard: Return or Escape is a press on one, as a
        // click on it would be, unless a host service's sheet over them has the keyboard.
        if let (true, Event::KeyDown(key)) = (self.sheet_up, event) {
            let Some(covered) = contain(cx, self.vm_id, "its sheet", |cx| crate::app_fullscreen::host_sheet_up(cx, &root)) else {
                self.stop(cx);
                return;
            };
            let buttons = cx.global::<KeyButtons>();
            buttons.covered = covered;
            // What an app that panicked on its way through a hidden view left counted.
            buttons.hidden = 0;
            let answers = !key.is_repeat && matches!(key.key_code, KeyCode::ReturnKey | KeyCode::NumpadEnter | KeyCode::Escape);
            if let (false, true, Some(client)) = (covered, answers, self.client) {
                cx.widget_action(self.uid, MpRunViewAction::Pressed { client });
            }
        }
        if contain(cx, self.vm_id, "an event", |cx| root.handle_event(cx, event, scope)).is_none() {
            self.stop(cx);
            return;
        }
        if let Some((lists, with_children, all)) = held {
            let pending = &mut cx.new_draw_event;
            if pending.draw_lists.len() > lists
                || pending.draw_lists_and_children.len() > with_children
                || pending.redraw_all != all
            {
                pending.draw_lists.truncate(lists);
                pending.draw_lists_and_children.truncate(with_children);
                pending.redraw_all = all;
                self.redraw_owed = true;
            }
        }
        // A press inside this tile is this tile's, even where none of the
        // app's widgets took it. Claimed, it reaches no window behind: a
        // module tile lets it by (above), a process tile's `event.hits`
        // misses it.
        if let Some(handled) = press {
            if handled.get().is_empty() {
                // In the band of a Mac window's unified toolbar, it moves the
                // window: none of the app's controls wanted it.
                if let (Event::MouseDown(e), Some(client), Some(rect)) = (event, self.client, rect) {
                    if e.button.contains(MouseButton::PRIMARY) && e.abs.y - rect.pos.y < self.title_band {
                        cx.widget_action(self.uid, MpRunViewAction::TitlePress { client, abs: e.abs, time: e.time });
                    }
                }
                handled.set(self.area);
            }
        }
        // A hover inside this tile is the tile's in the same way. Before, no
        // tile claimed a move, so a hover over the part of Mail that takes
        // none reached News behind it unclaimed: its widgets lit up and set
        // their cursors under Mail. A process tile's `event.hits` claims
        // hovers too.
        if let Some((claim, inside)) = hover {
            self.hovered = hover_unclaimed && !claim.get().is_empty();
            if inside && claim.get().is_empty() {
                claim.set(self.area);
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
            let viewport = &mut self.script_viewport;
            let resized = contain(cx, self.vm_id, "its resize", |cx| {
                let card = root.splash(cx, ids!(card));
                let content = card
                    .borrow()
                    .filter(|splash| !splash.view.source.is_zero())
                    .map(|splash| splash.view.widget_uid());
                if let Some(content) = content {
                    let current = (content, rect.size, crate::module_host::style_generation());
                    if *viewport != Some(current) {
                        *viewport = Some(current);
                        return card.call_script_fn(
                            cx,
                            id!(on_app_resize),
                            &[rect.size.x.into(), rect.size.y.into()],
                        );
                    }
                }
                false
            });
            if resized == Some(true) {
                cx.with_vm_and_async(|_| {});
            }
            // One app's fields are one group: Tab stays in its window.
            let nav_outer = cx.set_nav_group(self.uid.0);
            let buttons_outer = std::mem::take(&mut cx.global::<KeyButtons>().drawn);
            let drawn = resized.and_then(|_| {
                contain(cx, self.vm_id, "its draw", |cx| {
                    // A host service's sheet over the app is over its web views too.
                    if let Some(mut sheet) = root.splash(cx, ids!(sheet)).borrow_mut() {
                        sheet.view.over_web = true;
                    }
                    root.draw_walk_all(cx, scope, Walk::fill())
                })
            });
            cx.set_nav_group(nav_outer);
            let sheet_up = std::mem::replace(&mut cx.global::<KeyButtons>().drawn, buttons_outer);
            if drawn.is_some() {
                self.sheet_keys(cx, sheet_up);
            }
            if drawn.is_none() {
                cx.unwind_to(mark);
                CaptureGauss::unwind_scope_to(cx, captures);
            }
            cx.global::<ModalBounds>().0 = outer;
            if drawn.is_none() {
                self.stop(cx);
            }
            if drawn.is_some() && self.focused && self.takes_key_focus
                && self.last_viewport_size.is_some_and(|size| rect.size.y < size.y - 0.5)
            {
                if let Some(list) = cx.get_current_draw_list_id() {
                    if let Some((area, stack)) = CxDraw::iterate_nav_stops(cx, list, |cx, stop| {
                        cx.has_key_focus(stop.area).then_some(stop.area)
                    }) {
                        let focused = area.rect(cx);
                        if focused.pos.y + focused.size.y > rect.pos.y + rect.size.y - 5.0 {
                            NavControl::send_trigger_to_scroll_stack(cx, stack);
                        }
                    }
                }
            }
            self.last_viewport_size = Some(rect.size);
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

    /// A new touch is a press with a claim; the rest of its stroke and a
    /// frame are not. (A press a window in front already claimed has a
    /// non-empty cell: the tile behind lets it by, module_input_tests.rs.)
    #[test]
    fn only_a_new_touch_or_button_is_a_press_to_claim() {
        assert!(press_claim(&touch(dvec2(200.0, 400.0), TouchState::Start)).is_some());
        assert!(press_claim(&touch(dvec2(200.0, 400.0), TouchState::Move)).is_none(), "only a start is a press");
        assert!(press_claim(&Event::Startup).is_none());
    }

    /// A mouse move with no button held is a hover with a claim. With a
    /// button held it is a drag, which passes to whatever captured the press;
    /// a finger's move is no hover either.
    #[test]
    fn only_a_mouse_move_with_no_button_held_is_a_hover_to_claim() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let at = dvec2(200.0, 400.0);
        let moved = Event::MouseMove(MouseMoveEvent {
            abs: at,
            lock_delta: Vec2d::default(),
            window_id: WindowId(0, 0),
            modifiers: Default::default(),
            time: 0.0,
            handled: Default::default(),
        });
        assert!(hover_claim(&cx, &moved).is_some_and(|(_, abs)| abs == at));
        assert!(hover_claim(&cx, &touch(at, TouchState::Move)).is_none());
        assert!(hover_claim(&cx, &Event::Startup).is_none());
        cx.fingers.first_mouse_button = Some((MouseButton::PRIMARY, WindowId(0, 0)));
        assert!(hover_claim(&cx, &moved).is_none(), "a drag is no hover");
    }
}
