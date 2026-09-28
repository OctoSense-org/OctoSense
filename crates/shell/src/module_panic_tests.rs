//! Panic containment at the module boundary (ADR 0004 plan step 9;
//! module_host.rs "PANIC CONTAINMENT"): a probe module panics in create,
//! in an event, in its draw, in a tool call, in its shutdown and in its
//! drop — twice where it can — and the host, the draw context and a
//! second, well-behaved instance all go on.

use crate::module_host::{self, ModuleHost, STOPPED_REASON};
use crate::module_view::MpModuleView;
use makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolOutcome};
use makepad_app_module::*;
use makepad_widgets::*;
use std::cell::Cell;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.PanicProbeBase = #(PanicProbe::register_widget(vm))
    mod.widgets.PanicProbe = set_type_default() do mod.widgets.PanicProbeBase {
        width: Fill
        height: Fill
    }
}

/// Where a probe panics.
#[derive(Clone, Copy, Default, Debug)]
struct Faults {
    create: bool,
    event: bool,
    draw: bool,
    execute: bool,
    shutdown: bool,
    drop: bool,
}

thread_local! {
    /// How many times a probe's drop or shutdown panicked on this thread.
    static SECOND_PANICS: Cell<usize> = const { Cell::new(0) };
}

#[derive(Script, ScriptHook, Widget)]
pub struct PanicProbe {
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
    faults: Faults,
    /// `Event::Custom`s this root saw.
    #[rust]
    customs: usize,
    #[rust]
    draws: usize,
}

impl Drop for PanicProbe {
    fn drop(&mut self) {
        if self.faults.drop && !std::thread::panicking() {
            SECOND_PANICS.with(|n| n.set(n.get() + 1));
            panic!("probe: panic in drop");
        }
    }
}

impl Widget for PanicProbe {
    fn handle_event(&mut self, _cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if let Event::Custom(message) = event {
            self.customs += 1;
            if self.faults.event && message == "panic" {
                panic!("probe: panic in an event");
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        if self.faults.draw {
            // Leave the stacks unbalanced, as a real mid-draw panic does.
            cx.begin_turtle(Walk::fill(), Layout::flow_down());
            cx.begin_turtle(Walk::fill(), Layout::flow_right());
            panic!("probe: panic in draw");
        }
        let rect = cx.turtle().rect();
        self.draw_bg.draw_abs(cx, rect);
        self.draws += 1;
        cx.end_turtle();
        DrawStep::done()
    }
}

struct ProbeModule {
    id: &'static str,
    faults: Faults,
}

impl AppModule for ProbeModule {
    fn id(&self) -> &'static str {
        self.id
    }
    fn label(&self) -> &'static str {
        "Probe"
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
        if self.faults.create {
            panic!("probe: panic in create");
        }
        let value = script_eval!(vm, { use mod.widgets.* PanicProbe {} });
        let root = WidgetRef::script_from_value(vm, value);
        root.borrow_mut::<PanicProbe>().expect("a probe root").faults = self.faults;
        let faults = self.faults;
        InstanceParts {
            root,
            executor: Box::new(ProbeExecutor(faults)),
            shutdown: Box::new(move |_| {
                if faults.shutdown {
                    SECOND_PANICS.with(|n| n.set(n.get() + 1));
                    panic!("probe: panic in shutdown");
                }
            }),
        }
    }
}

struct ProbeExecutor(Faults);
impl ServiceExecutor for ProbeExecutor {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new("probe", "Probe", "test")
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        if self.0.execute {
            panic!("probe: panic in a tool call");
        }
        ExecOutcome::Done(makepad_ai_services::wire::ToolResult::unavailable(&call.call_id, "probe ok"))
    }
}

static CALM: ProbeModule = ProbeModule { id: "calm-probe", faults: Faults { create: false, event: false, draw: false, execute: false, shutdown: false, drop: false } };
static CREATE_BOMB: ProbeModule = ProbeModule { id: "create-bomb", faults: Faults { create: true, event: false, draw: false, execute: false, shutdown: false, drop: false } };
/// Panics in an event, and again in its shutdown and its drop.
static EVENT_BOMB: ProbeModule = ProbeModule { id: "event-bomb", faults: Faults { create: false, event: true, draw: false, execute: false, shutdown: true, drop: true } };
static DRAW_BOMB: ProbeModule = ProbeModule { id: "draw-bomb", faults: Faults { create: false, event: false, draw: true, execute: false, shutdown: true, drop: true } };
static TOOL_BOMB: ProbeModule = ProbeModule { id: "tool-bomb", faults: Faults { create: false, event: false, draw: false, execute: true, shutdown: false, drop: false } };
/// Healthy until it is closed: then its shutdown and its drop panic.
static CLOSE_BOMB: ProbeModule = ProbeModule { id: "close-bomb", faults: Faults { create: false, event: false, draw: false, execute: false, shutdown: true, drop: true } };

fn setup() -> (Cx, ModuleHost) {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(makepad_widgets::script_mod);
    (cx, ModuleHost::default())
}

fn create(cx: &mut Cx, host: &mut ModuleHost, client: u64, module: &'static dyn AppModule) -> Result<(), String> {
    host.create(cx, client, module, module.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0))
}

fn customs(cx: &mut Cx, host: &mut ModuleHost, client: u64) -> usize {
    host.dispatch(cx, client, "a test read", |_, root| root.borrow::<PanicProbe>().unwrap().customs).unwrap()
}

/// The script VM still answers: nothing was left installed or taken.
fn vm_alive(cx: &mut Cx) {
    let value = cx.with_vm(|vm| vm.eval(script! { 6 * 7 }));
    assert_eq!(value.as_f64(), Some(42.0));
}

fn call(id: &str) -> ServiceCall {
    ServiceCall { call_id: id.into(), tool: "ping".into(), args: "{}".into() }
}

#[test]
fn a_module_that_panics_in_create_never_becomes_an_instance() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, &CALM).unwrap();
    let error = create(&mut cx, &mut host, 2, &CREATE_BOMB).unwrap_err();
    assert!(error.contains("panicked while starting") && error.contains("panic in create"), "{error}");
    assert!(!host.is_module(2) && host.len() == 1, "no instance, no entry");
    assert!(host.take_faults(&mut cx).is_empty(), "create's fault is the launch's error, not a tile's");
    vm_alive(&mut cx);
    // The other instance, and new ones, carry on.
    assert!(host.send_custom(&mut cx, 1, "hello".into()));
    assert_eq!(customs(&mut cx, &mut host, 1), 1);
    create(&mut cx, &mut host, 3, &CALM).unwrap();
    assert!(host.teardown(&mut cx, 1) && host.teardown(&mut cx, 3));
}

#[test]
fn a_panic_in_an_event_stops_only_that_instance_and_its_second_panics_are_contained() {
    SECOND_PANICS.with(|n| n.set(0));
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, &EVENT_BOMB).unwrap();
    create(&mut cx, &mut host, 2, &CALM).unwrap();
    assert!(host.send_custom(&mut cx, 1, "panic".into()), "the panic stays inside the host call");
    vm_alive(&mut cx);
    // Closed to every later call before the shell has even looked.
    let ran = Cell::new(false);
    assert!(host.dispatch(&mut cx, 1, "after", |_, _| ran.set(true)).is_none());
    assert!(!ran.get(), "nothing reaches a failed instance");
    // The other instance still gets its events.
    assert!(host.send_custom(&mut cx, 2, "hello".into()));
    assert_eq!(customs(&mut cx, &mut host, 2), 1);
    // The shell hears of it once, with the module's name.
    assert_eq!(host.take_faults(&mut cx), vec![(1, "Probe".to_string())]);
    assert!(host.take_faults(&mut cx).is_empty());
    assert!(host.is_failed(1) && !host.is_failed(2));
    assert!(host.get(1).unwrap().failure().unwrap().contains("panic in an event"));
    // Release: its shutdown panics again and its root's drop panics again.
    host.release_failed(&mut cx, 1);
    assert_eq!(SECOND_PANICS.with(Cell::get), 2, "the shutdown and the drop both panicked, contained");
    vm_alive(&mut cx);
    // Its tools answer, unavailable.
    match host.execute(&mut cx, 1, &call("c1")) {
        Some(ExecOutcome::Done(result)) => {
            assert_eq!(result.outcome, ToolOutcome::Unavailable);
            assert!(result.text.contains(STOPPED_REASON));
        }
        _ => panic!("a failed instance answers its calls"),
    }
    assert_eq!(host.get(1).unwrap().manifest().id, "probe", "the manifest outlives the executor");
    assert!(host.send_custom(&mut cx, 2, "again".into()));
    assert_eq!(customs(&mut cx, &mut host, 2), 2);
    assert!(host.teardown(&mut cx, 1), "closing a failed instance");
    assert!(host.teardown(&mut cx, 2));
    assert!(host.is_empty());
}

#[test]
fn a_panicking_tool_call_answers_outcome_unknown_and_fails_the_instance() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, &TOOL_BOMB).unwrap();
    create(&mut cx, &mut host, 2, &CALM).unwrap();
    // `ping` is not in the probe's manifest: a tool of unknown risk counts
    // as one that may have acted.
    match host.execute(&mut cx, 1, &call("c1")) {
        Some(ExecOutcome::Done(result)) => {
            assert_eq!(result.outcome, ToolOutcome::TimedOut);
            assert_eq!(result.data, module_host::OUTCOME_UNKNOWN_DATA);
        }
        _ => panic!("the call is answered"),
    }
    assert_eq!(host.take_faults(&mut cx), vec![(1, "Probe".to_string())]);
    match host.execute(&mut cx, 2, &call("c2")) {
        Some(ExecOutcome::Done(result)) => assert_eq!(result.text, "probe ok"),
        _ => panic!("the healthy instance answers"),
    }
    host.release_failed(&mut cx, 1);
    assert!(host.teardown(&mut cx, 1) && host.teardown(&mut cx, 2));
}

#[test]
fn a_module_that_panics_while_being_closed_still_closes() {
    SECOND_PANICS.with(|n| n.set(0));
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, &CLOSE_BOMB).unwrap();
    create(&mut cx, &mut host, 2, &CALM).unwrap();
    assert!(host.teardown(&mut cx, 1));
    assert_eq!(SECOND_PANICS.with(Cell::get), 2, "shutdown and drop panicked, both contained");
    assert!(!host.is_module(1));
    vm_alive(&mut cx);
    assert!(host.send_custom(&mut cx, 2, "hello".into()));
    assert_eq!(customs(&mut cx, &mut host, 2), 1);
    assert!(host.teardown(&mut cx, 2));
}

/// A payload whose own drop panics: dropping it in the recovery path is
/// the textbook second panic.
struct NastyPayload;
impl Drop for NastyPayload {
    fn drop(&mut self) {
        panic!("the payload panics while being dropped");
    }
}

#[test]
fn a_payload_that_panics_in_drop_is_forgotten_not_rethrown() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, &CALM).unwrap();
    let vm_id = host.get(1).unwrap().vm_id;
    let out: Option<()> = module_host::contain(&mut cx, vm_id, "a test", |_| std::panic::panic_any(NastyPayload));
    assert!(out.is_none());
    assert!(module_host::is_failed(&mut cx, vm_id));
    let faults = host.take_faults(&mut cx);
    assert_eq!(faults, vec![(1, "Probe".to_string())]);
    assert_eq!(host.get(1).unwrap().failure(), Some("a panic with no message"));
    host.release_failed(&mut cx, 1);
    assert!(host.teardown(&mut cx, 1));
    vm_alive(&mut cx);
}

// ---- the tile: events and draw ----

fn tile(cx: &mut Cx) -> WidgetRef {
    cx.with_vm(|vm| {
        script_eval!(vm, { mod.wm_theme = { background: #1a1b26 } });
        crate::module_view::script_mod(vm);
        let value = script_eval!(vm, { use mod.widgets.* MpModuleView {} });
        WidgetRef::script_from_value(vm, value)
    })
}

fn seat(cx: &mut Cx, host: &ModuleHost, tile: &WidgetRef, client: u64) {
    let instance = host.get(client).unwrap();
    let (vm_id, root) = (instance.vm_id, instance.root.clone());
    tile.borrow_mut::<MpModuleView>().unwrap().set_root(cx, client, vm_id, root);
}

/// One frame with `tiles` side by side; asserts the tile draws leave the
/// draw context's stacks exactly where they found them.
fn draw_frame(cx: &mut Cx, tiles: &[&WidgetRef]) {
    let pass = DrawPass::new(cx);
    pass.set_size(cx, dvec2(800.0, 600.0));
    let mut list = DrawList2d::new(cx);
    let event = DrawEvent::default();
    let mut draw = CxDraw::new(cx, &event);
    let mut cx = Cx2d::new(&mut draw);
    cx.begin_pass(&pass, Some(1.0));
    list.begin_always(&mut cx);
    cx.begin_root_turtle(dvec2(800.0, 600.0), Layout::flow_right());
    for tile in tiles {
        let before = cx.unwind_mark();
        tile.draw_walk_all(&mut cx, &mut Scope::empty(), Walk::fixed(400.0, 600.0));
        assert!(cx.unwind_mark().is_balanced_with(&before), "a tile's draw pairs its stacks");
    }
    cx.end_turtle();
    list.end(&mut cx);
    cx.end_pass(&pass);
}

fn probe_draws(cx: &mut Cx, host: &mut ModuleHost, client: u64) -> usize {
    host.dispatch(cx, client, "a test read", |_, root| root.borrow::<PanicProbe>().unwrap().draws).unwrap()
}

#[test]
fn a_root_that_panics_in_draw_is_cut_back_and_its_tile_shows_it_closed() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, &DRAW_BOMB).unwrap();
    create(&mut cx, &mut host, 2, &CALM).unwrap();
    let (bomb, calm) = (tile(&mut cx), tile(&mut cx));
    seat(&mut cx, &host, &bomb, 1);
    seat(&mut cx, &host, &calm, 2);
    // The bomb draws first: the calm tile after it still draws, in the
    // same frame, and the frame ends.
    draw_frame(&mut cx, &[&bomb, &calm]);
    vm_alive(&mut cx);
    assert!(bomb.borrow::<MpModuleView>().unwrap().failed(), "the tile shows the app closed");
    assert!(bomb.borrow::<MpModuleView>().unwrap().root().is_none(), "and has let go of the root");
    assert_eq!(probe_draws(&mut cx, &mut host, 2), 1);
    let faults = host.take_faults(&mut cx);
    assert_eq!(faults, vec![(1, "Probe".to_string())]);
    bomb.borrow_mut::<MpModuleView>().unwrap().show_failed(&mut cx, &faults[0].1);
    host.release_failed(&mut cx, 1);
    // Later frames: the closed face (with its Restart) and the healthy app.
    draw_frame(&mut cx, &[&bomb, &calm]);
    draw_frame(&mut cx, &[&bomb, &calm]);
    assert_eq!(probe_draws(&mut cx, &mut host, 2), 3);
    drop(calm);
    assert!(host.teardown(&mut cx, 1) && host.teardown(&mut cx, 2));
}

#[test]
fn a_root_that_panics_in_an_event_stops_only_its_own_tile() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, &EVENT_BOMB).unwrap();
    create(&mut cx, &mut host, 2, &CALM).unwrap();
    let (bomb, calm) = (tile(&mut cx), tile(&mut cx));
    seat(&mut cx, &host, &bomb, 1);
    seat(&mut cx, &host, &calm, 2);
    for tile in [&bomb, &calm] {
        tile.handle_event(&mut cx, &Event::Custom("panic".into()), &mut Scope::empty());
    }
    assert!(bomb.borrow::<MpModuleView>().unwrap().failed());
    assert!(!calm.borrow::<MpModuleView>().unwrap().failed());
    assert_eq!(customs(&mut cx, &mut host, 2), 1, "the other tile still got the event");
    // A second event goes nowhere near the failed root, and the other tile
    // keeps receiving.
    for tile in [&bomb, &calm] {
        tile.handle_event(&mut cx, &Event::Custom("panic".into()), &mut Scope::empty());
    }
    assert_eq!(customs(&mut cx, &mut host, 2), 2);
    assert_eq!(host.take_faults(&mut cx), vec![(1, "Probe".to_string())]);
    host.release_failed(&mut cx, 1);
    drop(calm);
    assert!(host.teardown(&mut cx, 1) && host.teardown(&mut cx, 2));
}

// ---- in-flight tool calls when the module panics ----

/// A module with a read, an act and a destructive tool. `move` and `wipe`
/// answer `Pending` (they finish later through the reply sink, as a tool
/// behind the app's own confirm sheet does); `crash_move` panics while it
/// acts; `crash_look` panics while it reads; `quick_move` answers later
/// but has already sent its answer when the app panics.
struct FlightModule;
struct FlightExecutor(ReplySink);

impl AppModule for FlightModule {
    fn id(&self) -> &'static str {
        "flight-probe"
    }
    fn label(&self) -> &'static str {
        "Flight"
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
    fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, handles: InstanceHandles) -> InstanceParts {
        let value = script_eval!(vm, { use mod.widgets.* PanicProbe {} });
        let root = WidgetRef::script_from_value(vm, value);
        root.borrow_mut::<PanicProbe>().unwrap().faults = Faults { event: true, ..Default::default() };
        InstanceParts { root, executor: Box::new(FlightExecutor(handles.replies)), shutdown: Box::new(|_| {}) }
    }
}

impl ServiceExecutor for FlightExecutor {
    fn manifest(&self) -> ServiceManifest {
        use makepad_ai_services::wire::{Risk, ToolDef};
        let mut manifest = ServiceManifest::new("flight", "Flight", "test");
        for (name, risk) in [("look", Risk::Read), ("crash_look", Risk::Read), ("move", Risk::Act),
                             ("crash_move", Risk::Act), ("quick_move", Risk::Act), ("wipe", Risk::Destructive)] {
            manifest.tools.push(ToolDef::new(name, "test", r#"{"type":"object"}"#, risk));
        }
        manifest
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        use makepad_ai_services::wire::ToolResult;
        match call.tool.as_str() {
            "look" => ExecOutcome::Done(ToolResult::ok(&call.call_id, "seen", "seen")),
            "crash_look" => panic!("probe: panic while reading"),
            "crash_move" => panic!("probe: panic halfway through a move"),
            "quick_move" => {
                self.0.reply(ToolResult::ok(&call.call_id, "moved", "moved"));
                ExecOutcome::Pending
            }
            _ => ExecOutcome::Pending,
        }
    }
}

static FLIGHT: FlightModule = FlightModule;

fn flight_call(id: &str, tool: &str) -> ServiceCall {
    ServiceCall { call_id: id.into(), tool: tool.into(), args: "{}".into() }
}

fn is_outcome_unknown(result: &makepad_ai_services::wire::ToolResult) -> bool {
    !result.outcome.is_ok()
        && result.outcome == ToolOutcome::TimedOut
        && result.data == module_host::OUTCOME_UNKNOWN_DATA
        && result.text.starts_with("Outcome unknown")
        && result.disposition == makepad_ai_services::wire::Disposition::EndTurn
}

fn done(outcome: Option<ExecOutcome>) -> makepad_ai_services::wire::ToolResult {
    match outcome {
        Some(ExecOutcome::Done(result)) => result,
        _ => panic!("expected an answer now"),
    }
}

#[test]
fn a_call_its_executor_panics_in_answers_outcome_unknown_when_it_may_have_acted() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, &FLIGHT).unwrap();
    create(&mut cx, &mut host, 2, &FLIGHT).unwrap();
    let acted = done(host.execute(&mut cx, 1, &flight_call("a1", "crash_move")));
    assert!(is_outcome_unknown(&acted), "a half-run act is neither success nor retryable: {acted:?}");
    assert_eq!(acted.call_id, "a1");
    let read = done(host.execute(&mut cx, 2, &flight_call("r1", "crash_look")));
    assert_eq!(read.outcome, ToolOutcome::Unavailable, "a read that crashed changed nothing");
    // After the failure nothing runs: those calls are plainly unavailable.
    let after = done(host.execute(&mut cx, 1, &flight_call("a2", "wipe")));
    assert_eq!(after.outcome, ToolOutcome::Unavailable);
    assert_eq!(host.take_faults(&mut cx).len(), 2);
    for client in [1, 2] {
        host.release_failed(&mut cx, client);
        assert!(host.teardown(&mut cx, client));
    }
}

#[test]
fn in_flight_act_and_destructive_calls_answer_outcome_unknown_when_their_app_panics() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, &FLIGHT).unwrap();
    create(&mut cx, &mut host, 2, &FLIGHT).unwrap();
    assert!(matches!(host.execute(&mut cx, 1, &flight_call("m1", "move")), Some(ExecOutcome::Pending)));
    assert!(matches!(host.execute(&mut cx, 1, &flight_call("w1", "wipe")), Some(ExecOutcome::Pending)));
    assert!(matches!(host.execute(&mut cx, 1, &flight_call("q1", "quick_move")), Some(ExecOutcome::Pending)));
    assert!(matches!(host.execute(&mut cx, 1, &flight_call("c1", "move")), Some(ExecOutcome::Pending)));
    host.cancel(&mut cx, 1, "c1");
    assert!(matches!(host.execute(&mut cx, 2, &flight_call("m2", "move")), Some(ExecOutcome::Pending)));
    assert_eq!(done(host.execute(&mut cx, 1, &flight_call("l1", "look"))).outcome, ToolOutcome::Ok);
    // The app panics in an event while three of its calls are in flight.
    host.send_custom(&mut cx, 1, "panic".into());
    assert_eq!(host.take_faults(&mut cx), vec![(1, "Flight".to_string())]);
    host.release_failed(&mut cx, 1);
    let mut answers: Vec<_> = host.drain_upstream().into_iter().map(|(client, up)| match up {
        ModuleUpstream::Result(result) => (client, result),
        ModuleUpstream::Message { .. } => panic!("no publications here"),
    }).collect();
    answers.sort_by(|a, b| a.1.call_id.cmp(&b.1.call_id));
    let ids: Vec<(u64, &str)> = answers.iter().map(|(c, r)| (*c, r.call_id.as_str())).collect();
    assert_eq!(ids, [(1, "m1"), (1, "q1"), (1, "w1")], "each in-flight call answered once; the cancelled one not at all");
    let by_id = |id: &str| &answers.iter().find(|(_, r)| r.call_id == id).unwrap().1;
    assert!(is_outcome_unknown(by_id("m1")), "an in-flight act: {:?}", by_id("m1"));
    assert!(is_outcome_unknown(by_id("w1")), "an in-flight destructive call: {:?}", by_id("w1"));
    assert_eq!(by_id("q1").outcome, ToolOutcome::Ok, "an answer sent before the panic is the real one");
    assert!(host.drain_upstream().is_empty(), "nothing is answered twice");
    // The other instance's call is untouched: still in flight, still its own.
    assert!(!host.is_failed(2));
    for client in [1, 2] {
        assert!(host.teardown(&mut cx, client));
    }
}
