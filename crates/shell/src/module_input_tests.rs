//! A press between overlapping module tiles. The desk offers pointer
//! events to its tiles topmost first (desk.rs `handle_event`), so a press
//! on the window in front reaches the tiles of the windows behind it
//! already claimed. A module tile behind lets such a press by: its app never
//! sees it. Before, the tile handed it to the app and counted on the claim,
//! which a GestureView ignores. News's story rows are GestureViews, so a
//! click on Mail's inbox footer, over News's third story, opened that story
//! (the instrument's B3).

use crate::module_host::ModuleHost;
use crate::module_view::MpModuleView;
use crate::run_view::MpRunViewAction;
use makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult};
use makepad_app_module::*;
use makepad_platform::event::{TouchPoint, TouchState, TouchUpdateEvent};
use makepad_widgets::*;
use std::cell::Cell;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.TapProbeBase = #(TapProbe::register_widget(vm))
    mod.widgets.TapProbe = set_type_default() do mod.widgets.TapProbeBase {
        width: Fill
        height: Fill
    }
}

/// An app's root that counts the presses reaching it. A tappable one takes
/// them as makepad's GestureView does: a button press through
/// `hits_with_capture_overload`, which co-captures a press another area
/// already claimed, and a touch straight from the raw stream. A claim alone
/// never keeps a press from it. An inert one takes nothing, like the empty
/// part of a window (Mail's inbox footer).
#[derive(Script, ScriptHook, Widget)]
pub struct TapProbe {
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
    tappable: bool,
    /// Presses (a button going down, a new touch) that reached the app.
    #[rust]
    seen: usize,
    /// Presses it took and saw released over it.
    #[rust]
    taps: usize,
    /// The touch it follows.
    #[rust]
    touch: Option<u64>,
}

impl Widget for TapProbe {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        match event {
            Event::MouseDown(_) => self.seen += 1,
            Event::TouchUpdate(update) => {
                self.seen += update.touches.iter().filter(|t| t.state == TouchState::Start).count();
            }
            _ => {}
        }
        if !self.tappable {
            return;
        }
        let area = self.draw_bg.area();
        if let Event::TouchUpdate(update) = event {
            let bounds = area.clipped_rect(cx);
            for t in &update.touches {
                match t.state {
                    TouchState::Start if self.touch.is_none() && bounds.contains(t.abs) => self.touch = Some(t.uid),
                    TouchState::Stop if self.touch == Some(t.uid) => {
                        self.touch = None;
                        if bounds.contains(t.abs) {
                            self.taps += 1;
                        }
                    }
                    _ => {}
                }
            }
            return;
        }
        if let Hit::FingerUp(e) = event.hits_with_capture_overload(cx, area, true) {
            if e.is_over {
                self.taps += 1;
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let rect = cx.turtle().rect();
        self.draw_bg.draw_abs(cx, rect);
        cx.end_turtle();
        DrawStep::done()
    }
}

#[derive(Clone, Copy)]
enum Root {
    /// A TapProbe that takes presses.
    Tappable,
    /// A TapProbe that takes none.
    Inert,
    /// Makepad's own GestureView.
    Gesture,
}

struct InputProbe {
    id: &'static str,
    root: Root,
}

impl AppModule for InputProbe {
    fn id(&self) -> &'static str {
        self.id
    }
    fn label(&self) -> &'static str {
        "Input probe"
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &[]
    }
    fn open_schema(&self) -> OpenSchema {
        OpenSchema::new(1)
    }
    fn register(&self, vm: &mut ScriptVm) {
        self::script_mod(vm);
    }
    fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, _handles: InstanceHandles) -> InstanceParts {
        let root = match self.root {
            // The tile draws its root filling it (`Walk::fill()`).
            Root::Gesture => {
                let value = script_eval!(vm, { use mod.widgets.* GestureView {} });
                WidgetRef::script_from_value(vm, value)
            }
            Root::Tappable | Root::Inert => {
                let value = script_eval!(vm, { use mod.widgets.* TapProbe {} });
                let root = WidgetRef::script_from_value(vm, value);
                root.borrow_mut::<TapProbe>().expect("a probe root").tappable = matches!(self.root, Root::Tappable);
                root
            }
        };
        InstanceParts { root, executor: Box::new(Idle), shutdown: Box::new(|_| {}) }
    }
}

struct Idle;
impl ServiceExecutor for Idle {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new("input-probe", "Input probe", "test")
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(ToolResult::unavailable(&call.call_id, "nothing to do"))
    }
}

/// News: rows that take any press that reaches them.
static NEWS: InputProbe = InputProbe { id: "news-probe", root: Root::Tappable };
/// Mail: nothing under the click.
static MAIL: InputProbe = InputProbe { id: "mail-probe", root: Root::Inert };
/// News with makepad's GestureView as its row, as its bundle has it.
static GESTURE_NEWS: InputProbe = InputProbe { id: "gesture-probe", root: Root::Gesture };

fn setup() -> (Cx, ModuleHost) {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(makepad_widgets::script_mod);
    (cx, ModuleHost::default())
}

fn create(cx: &mut Cx, host: &mut ModuleHost, client: u64, module: &'static dyn AppModule) {
    host.create(cx, client, module, module.open_schema().empty_open().unwrap(), dvec2(400.0, 500.0)).unwrap();
}

fn tile(cx: &mut Cx, host: &ModuleHost, client: u64) -> WidgetRef {
    let tile = cx.with_vm(|vm| {
        script_eval!(vm, { mod.wm_theme = { background: #1a1b26 } });
        crate::module_view::script_mod(vm);
        let value = script_eval!(vm, { use mod.widgets.* MpModuleView {} });
        WidgetRef::script_from_value(vm, value)
    });
    let instance = host.get(client).unwrap();
    let (vm_id, root) = (instance.vm_id, instance.root.clone());
    tile.borrow_mut::<MpModuleView>().unwrap().set_root(cx, client, vm_id, root);
    tile
}

/// A window's rect: 400 by 500 at (x, y).
fn at(x: f64, y: f64) -> Rect {
    Rect { pos: dvec2(x, y), size: dvec2(400.0, 500.0) }
}

/// The areas a tile hit-tests against live in the frame's draw list: kept
/// while the test offers events.
struct Frame {
    _pass: DrawPass,
    _list: DrawList2d,
}

/// One frame with the tiles drawn back to front, each at its own rect, as
/// the desk draws cascaded windows. The root turtle ends as a window's does
/// (`end_pass_sized_turtle`), which applies the clips every hit test reads.
fn draw_stack(cx: &mut Cx, tiles: &[(&WidgetRef, Rect)]) -> Frame {
    let pass = DrawPass::new(cx);
    pass.set_size(cx, dvec2(900.0, 800.0));
    let mut list = DrawList2d::new(cx);
    let event = DrawEvent::default();
    let mut draw = CxDraw::new(cx, &event);
    let mut cx = Cx2d::new(&mut draw);
    cx.begin_pass(&pass, Some(1.0));
    list.begin_always(&mut cx);
    cx.begin_root_turtle(dvec2(900.0, 800.0), Layout::flow_overlay());
    for (tile, rect) in tiles {
        tile.draw_walk_all(&mut cx, &mut Scope::empty(), Walk::abs_rect(*rect));
    }
    cx.end_pass_sized_turtle();
    list.end(&mut cx);
    cx.end_pass(&pass);
    Frame { _pass: pass, _list: list }
}

fn mouse_down(abs: Vec2d) -> Event {
    Event::MouseDown(MouseDownEvent {
        abs,
        button: MouseButton::PRIMARY,
        window_id: WindowId(0, 0),
        modifiers: Default::default(),
        handled: Cell::new(Area::Empty),
        time: 1.0,
    })
}

fn mouse_up(abs: Vec2d) -> Event {
    Event::MouseUp(MouseUpEvent { abs, button: MouseButton::PRIMARY, window_id: WindowId(0, 0), modifiers: Default::default(), time: 1.1 })
}

fn touch(abs: Vec2d, state: TouchState) -> Event {
    Event::TouchUpdate(TouchUpdateEvent {
        time: 1.0,
        window_id: WindowId(0, 0),
        modifiers: Default::default(),
        touches: vec![TouchPoint { state, abs, time: 1.0, uid: 1, rotation_angle: 0.0, force: 0.0, radius: dvec2(1.0, 1.0), handled: Default::default(), sweep_lock: Default::default() }],
    })
}

/// What a tile asked the window manager for.
#[derive(Debug, PartialEq)]
enum Asked {
    Raise(u64),
    Restart(u64),
}

/// Offer each event to the tiles front to back, as the desk does, and
/// collect what the tiles asked for.
fn offer(cx: &mut Cx, front_to_back: &[&WidgetRef], events: &[Event]) -> Vec<Asked> {
    let actions = cx.capture_actions(|cx| {
        for event in events {
            for tile in front_to_back {
                tile.handle_event(cx, event, &mut Scope::empty());
            }
        }
    });
    actions
        .iter()
        .filter_map(|action| action.as_widget_action())
        .filter_map(|wa| match wa.cast::<MpRunViewAction>() {
            MpRunViewAction::Clicked { client } => Some(Asked::Raise(client)),
            MpRunViewAction::Restart { client } => Some(Asked::Restart(client)),
            _ => None,
        })
        .collect()
}

fn click(cx: &mut Cx, front_to_back: &[&WidgetRef], abs: Vec2d) -> Vec<Asked> {
    offer(cx, front_to_back, &[mouse_down(abs), mouse_up(abs)])
}

fn tap(cx: &mut Cx, front_to_back: &[&WidgetRef], abs: Vec2d) -> Vec<Asked> {
    offer(cx, front_to_back, &[touch(abs, TouchState::Start), touch(abs, TouchState::Stop)])
}

/// (presses seen, taps) of a probe app.
fn counts(cx: &mut Cx, host: &mut ModuleHost, client: u64) -> (usize, usize) {
    host.dispatch(cx, client, "a test read", |_, root| {
        let probe = root.borrow::<TapProbe>().unwrap();
        (probe.seen, probe.taps)
    })
    .unwrap()
}

/// News is opened, then Mail, which cascades over it. A press inside both
/// lands on Mail where Mail has nothing to take it, over a News row.
const NEWS_AT: (f64, f64) = (20.0, 40.0);
const MAIL_AT: (f64, f64) = (120.0, 140.0);
/// Inside Mail, over News.
const OVER_BOTH: (f64, f64) = (300.0, 450.0);
/// Inside News where Mail does not cover it.
const NEWS_ONLY: (f64, f64) = (60.0, 450.0);

fn news_and_mail(news: &'static dyn AppModule) -> (Cx, ModuleHost, WidgetRef, WidgetRef, Frame) {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, news);
    create(&mut cx, &mut host, 2, &MAIL);
    let news = tile(&mut cx, &host, 1);
    let mail = tile(&mut cx, &host, 2);
    let frame = draw_stack(&mut cx, &[(&news, at(NEWS_AT.0, NEWS_AT.1)), (&mail, at(MAIL_AT.0, MAIL_AT.1))]);
    assert!(at(MAIL_AT.0, MAIL_AT.1).contains(dvec2(OVER_BOTH.0, OVER_BOTH.1)) && at(NEWS_AT.0, NEWS_AT.1).contains(dvec2(OVER_BOTH.0, OVER_BOTH.1)));
    (cx, host, news, mail, frame)
}

fn close(mut cx: Cx, mut host: ModuleHost, tiles: Vec<WidgetRef>, frame: Frame, clients: &[u64]) {
    drop(tiles);
    drop(frame);
    for client in clients {
        assert!(host.teardown(&mut cx, *client));
    }
}

#[test]
fn a_click_on_the_window_in_front_never_reaches_the_app_behind() {
    let (mut cx, mut host, news, mail, frame) = news_and_mail(&NEWS);
    let over_both = dvec2(OVER_BOTH.0, OVER_BOTH.1);
    assert_eq!(click(&mut cx, &[&mail, &news], over_both), [Asked::Raise(2)], "Mail alone is raised");
    assert_eq!(counts(&mut cx, &mut host, 2), (1, 0), "Mail's app saw the press");
    assert_eq!(counts(&mut cx, &mut host, 1), (0, 0), "News's app never saw it, so no story opened");
    // Where News is in front of nothing, the same row takes the click.
    assert_eq!(click(&mut cx, &[&mail, &news], dvec2(NEWS_ONLY.0, NEWS_ONLY.1)), [Asked::Raise(1)]);
    assert_eq!(counts(&mut cx, &mut host, 1), (1, 1), "a click where News is in front opens its story");
    assert_eq!(counts(&mut cx, &mut host, 2), (1, 0));
    close(cx, host, vec![news, mail], frame, &[1, 2]);
}

/// A finger: the rows follow new touches from the raw stream, as GestureView
/// does, so only the tile can keep one from them.
#[test]
fn a_touch_on_the_window_in_front_never_reaches_the_app_behind() {
    let (mut cx, mut host, news, mail, frame) = news_and_mail(&NEWS);
    assert_eq!(tap(&mut cx, &[&mail, &news], dvec2(OVER_BOTH.0, OVER_BOTH.1)), [Asked::Raise(2)]);
    assert_eq!(counts(&mut cx, &mut host, 2), (1, 0));
    assert_eq!(counts(&mut cx, &mut host, 1), (0, 0), "News's app never saw the touch");
    assert_eq!(tap(&mut cx, &[&mail, &news], dvec2(NEWS_ONLY.0, NEWS_ONLY.1)), [Asked::Raise(1)]);
    assert_eq!(counts(&mut cx, &mut host, 1), (1, 1));
    close(cx, host, vec![news, mail], frame, &[1, 2]);
}

/// With makepad's own GestureView as the row: its tap fires on the release
/// of a press it captured, and it captured the one Mail's tile had claimed.
#[test]
fn a_gesture_view_behind_the_window_in_front_never_captures_its_press() {
    let (mut cx, mut host, news, mail, frame) = news_and_mail(&GESTURE_NEWS);
    let row = host.dispatch(&mut cx, 1, "a test read", |_, root| root.area()).unwrap();
    let over_both = dvec2(OVER_BOTH.0, OVER_BOTH.1);
    assert_eq!(offer(&mut cx, &[&mail, &news], &[mouse_down(over_both)]), [Asked::Raise(2)]);
    assert!(!cx.fingers.is_area_captured(row), "News's GestureView took a press made on Mail");
    offer(&mut cx, &[&mail, &news], &[mouse_up(over_both)]);
    let news_only = dvec2(NEWS_ONLY.0, NEWS_ONLY.1);
    assert_eq!(offer(&mut cx, &[&mail, &news], &[mouse_down(news_only)]), [Asked::Raise(1)]);
    assert!(cx.fingers.is_area_captured(row), "where News is in front its row takes the press");
    offer(&mut cx, &[&mail, &news], &[mouse_up(news_only)]);
    close(cx, host, vec![news, mail], frame, &[1, 2]);
}

/// The closed face of an app that stopped after an error takes presses the
/// same way. A click on a window over its Restart does not restart it, and a
/// click on the face raises no window behind it.
#[test]
fn a_stopped_tile_takes_only_a_press_nothing_in_front_took() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, &NEWS);
    create(&mut cx, &mut host, 2, &MAIL);
    create(&mut cx, &mut host, 3, &NEWS);
    let (stopped, over, under) = (tile(&mut cx, &host, 1), tile(&mut cx, &host, 2), tile(&mut cx, &host, 3));
    stopped.borrow_mut::<MpModuleView>().unwrap().show_failed(&mut cx, "News");
    // The stopped app at (100, 100); Mail over its left part and its Restart;
    // another app under it, reaching out right of Mail.
    let (under_at, stopped_at, mail_at) = (at(150.0, 0.0), at(100.0, 100.0), at(60.0, 120.0));
    let frame = draw_stack(&mut cx, &[(&under, under_at), (&stopped, stopped_at), (&over, mail_at)]);
    let restart = stopped.borrow::<MpModuleView>().unwrap().restart_button().expect("the closed face has its Restart");
    assert!(restart.is_inside_of(mail_at) && under_at.contains(restart.center()), "Mail covers the Restart: {restart:?}");
    let front_to_back = [&over, &stopped, &under];
    assert_eq!(click(&mut cx, &front_to_back, restart.center()), [Asked::Raise(2)], "a click on Mail is Mail's");
    assert_eq!(counts(&mut cx, &mut host, 3), (0, 0));
    // On the face, clear of Mail: the stopped tile is raised, and the app
    // under it never sees the press.
    let face = dvec2(480.0, 300.0);
    assert!(stopped_at.contains(face) && !mail_at.contains(face) && under_at.contains(face));
    assert_eq!(click(&mut cx, &front_to_back, face), [Asked::Raise(1)]);
    assert_eq!(counts(&mut cx, &mut host, 3), (0, 0));
    // Mail moved off it: its Restart restarts it.
    drop(frame);
    let mail_at = at(500.0, 300.0);
    let frame = draw_stack(&mut cx, &[(&under, under_at), (&stopped, stopped_at), (&over, mail_at)]);
    let restart = stopped.borrow::<MpModuleView>().unwrap().restart_button().unwrap();
    assert!(!mail_at.contains(restart.center()));
    assert_eq!(click(&mut cx, &front_to_back, restart.center()), [Asked::Restart(1)]);
    assert_eq!(counts(&mut cx, &mut host, 3), (0, 0));
    close(cx, host, vec![stopped, over, under], frame, &[1, 2, 3]);
}
