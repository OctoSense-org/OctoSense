//! Hosted close confirmation (makepad#65, `AppModule::close_requested`):
//! a module that refuses a close keeps its instance until its root emits
//! `ModuleCloseAction::Confirmed`, and a shell quit waits on every refusal.
//! The shell's side is `App::request_close`, `App::module_close_confirmed`
//! and `App::ask_before_quit` (lib.rs); these pin the decisions they act on.

use crate::module_host::{CloseGate, ModuleHost};
use makepad_ai_services::wire::{ServiceCall, ServiceManifest};
use makepad_app_module::*;
use makepad_widgets::*;

/// How a probe answers `close_requested`.
#[derive(Clone, Copy)]
enum Answer {
    /// The trait's default (no override reached).
    Default,
    Veto,
    Panic,
}

struct CloseProbe {
    id: &'static str,
    answer: Answer,
}

impl AppModule for CloseProbe {
    fn id(&self) -> &'static str {
        self.id
    }
    fn label(&self) -> &'static str {
        "Close probe"
    }
    fn capabilities(&self) -> &'static [&'static str] {
        &[]
    }
    fn open_schema(&self) -> OpenSchema {
        OpenSchema::new(1)
    }
    fn register(&self, _vm: &mut ScriptVm) {}
    fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, _handles: InstanceHandles) -> InstanceParts {
        let value = script_eval!(vm, { use mod.widgets.* View {} });
        InstanceParts {
            root: WidgetRef::script_from_value(vm, value),
            executor: Box::new(Quiet),
            shutdown: Box::new(|_| {}),
        }
    }
    fn close_requested(&self, _cx: &mut Cx, _root: &WidgetRef) -> CloseDecision {
        match self.answer {
            Answer::Default => CloseDecision::Allow,
            Answer::Veto => CloseDecision::Veto,
            Answer::Panic => panic!("probe: panic in close_requested"),
        }
    }
}

struct Quiet;
impl ServiceExecutor for Quiet {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new("close-probe", "Close probe", "test")
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(makepad_ai_services::wire::ToolResult::unavailable(&call.call_id, "probe"))
    }
}

static AGREEABLE: CloseProbe = CloseProbe { id: "agreeable", answer: Answer::Default };
static REFUSING: CloseProbe = CloseProbe { id: "refusing", answer: Answer::Veto };
static PANICKING: CloseProbe = CloseProbe { id: "panicking", answer: Answer::Panic };

fn setup() -> (Cx, ModuleHost) {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(makepad_widgets::script_mod);
    (cx, ModuleHost::default())
}

fn create(cx: &mut Cx, host: &mut ModuleHost, client: u64, module: &'static dyn AppModule) {
    host.create(cx, client, module, module.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
}

fn uid(host: &ModuleHost, client: u64) -> WidgetUid {
    host.get(client).unwrap().root.widget_uid()
}

// ---- the gate's decisions, without a module ----

#[test]
fn an_allowed_close_tears_down_and_leaves_nothing_pending() {
    let mut gate = CloseGate::default();
    assert!(gate.answered(1, CloseDecision::Allow, WidgetUid(10)));
    assert!(!gate.is_pending(1));
    assert_eq!(gate.confirmed(WidgetUid(10)), None, "nothing was waiting for a yes");
}

#[test]
fn a_vetoed_close_keeps_the_instance_and_waits_for_its_root() {
    let mut gate = CloseGate::default();
    assert!(!gate.answered(1, CloseDecision::Veto, WidgetUid(10)), "a veto never tears down");
    assert!(gate.is_pending(1));
    // Asked again (the person clicked close twice) and it now allows: the
    // mark goes and the instance closes.
    assert!(gate.answered(1, CloseDecision::Allow, WidgetUid(10)));
    assert!(!gate.is_pending(1));
}

#[test]
fn only_the_pending_root_confirms_its_close() {
    let mut gate = CloseGate::default();
    gate.answered(1, CloseDecision::Veto, WidgetUid(10));
    gate.answered(2, CloseDecision::Veto, WidgetUid(20));
    assert_eq!(gate.confirmed(WidgetUid(99)), None, "another widget's yes is ignored");
    assert!(gate.is_pending(1) && gate.is_pending(2));
    assert_eq!(gate.confirmed(WidgetUid(20)), Some(2));
    assert!(!gate.is_pending(2) && gate.is_pending(1));
    assert_eq!(gate.confirmed(WidgetUid(20)), None, "a second yes closes nothing more");
    gate.forget(1);
    assert_eq!(gate.confirmed(WidgetUid(10)), None, "a torn-down instance's root confirms nothing");
}

#[test]
fn a_quit_nobody_refuses_goes_ahead_at_once() {
    let mut gate = CloseGate::default();
    assert!(gate.quit_asked([]));
    assert!(!gate.quit_waiting());
    assert!(!gate.take_quit_ready(), "it quit already; nothing is waiting");
}

#[test]
fn a_quit_waits_for_every_refusing_instance() {
    let mut gate = CloseGate::default();
    assert!(!gate.quit_asked([(1, WidgetUid(10)), (2, WidgetUid(20))]));
    assert!(gate.quit_waiting());
    assert_eq!(gate.confirmed(WidgetUid(10)), Some(1));
    assert!(!gate.take_quit_ready(), "one still asks");
    assert_eq!(gate.confirmed(WidgetUid(20)), Some(2));
    assert!(gate.take_quit_ready(), "the last yes lets the quit go");
    assert!(!gate.take_quit_ready(), "once");
}

#[test]
fn a_failed_instance_does_not_hold_a_quit_forever() {
    let mut gate = CloseGate::default();
    assert!(!gate.quit_asked([(1, WidgetUid(10))]));
    gate.forget(1);
    assert!(gate.take_quit_ready());
}

#[test]
fn closing_one_app_abandons_a_waiting_quit() {
    // The person declined the quit's question in the terminal, then later
    // closes that terminal and says yes: only the terminal closes.
    let mut gate = CloseGate::default();
    assert!(!gate.quit_asked([(1, WidgetUid(10))]));
    gate.close_asked();
    assert!(!gate.answered(1, CloseDecision::Veto, WidgetUid(10)));
    assert_eq!(gate.confirmed(WidgetUid(10)), Some(1));
    assert!(!gate.take_quit_ready(), "the shell stays up");
}

// ---- the host, with real instances ----

#[test]
fn a_module_that_allows_is_closed_and_one_that_refuses_is_kept() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, &AGREEABLE);
    create(&mut cx, &mut host, 2, &REFUSING);

    assert_eq!(host.ask_close(&mut cx, 1), CloseDecision::Allow);
    assert!(!host.close_pending(1));
    assert!(host.teardown(&mut cx, 1));

    assert_eq!(host.ask_close(&mut cx, 2), CloseDecision::Veto);
    assert!(host.close_pending(2), "a refusal is remembered");
    assert!(host.is_module(2), "and the instance stays");

    assert!(host.teardown(&mut cx, 2));
    assert!(!host.close_pending(2), "a teardown clears the mark");
}

#[test]
fn a_confirmation_closes_only_the_instance_whose_root_sent_it() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, &REFUSING);
    create(&mut cx, &mut host, 2, &REFUSING);
    let (first, second) = (uid(&host, 1), uid(&host, 2));
    assert_ne!(first, second);

    assert_eq!(host.ask_close(&mut cx, 1), CloseDecision::Veto);
    // Instance 2 was never asked: its yes means nothing.
    assert_eq!(host.close_confirmed(second), None);
    assert_eq!(host.close_confirmed(first), Some(1));
    assert!(!host.close_pending(1));
    assert!(host.teardown(&mut cx, 1) && host.teardown(&mut cx, 2));
}

#[test]
fn a_module_that_panics_while_answering_is_let_go() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, &PANICKING);
    assert_eq!(host.ask_close(&mut cx, 1), CloseDecision::Allow);
    assert!(!host.close_pending(1));
    assert_eq!(host.take_faults(&mut cx).len(), 1, "the panic is contained and reported");
    assert!(host.teardown(&mut cx, 1));
}

#[test]
fn a_shell_quit_asks_every_instance_and_waits_for_the_refusals() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 1, &AGREEABLE);
    create(&mut cx, &mut host, 2, &REFUSING);
    create(&mut cx, &mut host, 3, &REFUSING);

    assert_eq!(host.ask_quit(&mut cx), vec![2, 3]);
    assert!(!host.take_quit_ready());
    assert_eq!(host.close_confirmed(uid(&host, 3)), Some(3));
    assert!(host.teardown(&mut cx, 3));
    assert!(!host.take_quit_ready(), "instance 2 still asks");
    assert_eq!(host.close_confirmed(uid(&host, 2)), Some(2));
    assert!(host.teardown(&mut cx, 2));
    assert!(host.take_quit_ready());

    // Only instances that allow: the quit goes at once.
    assert!(host.ask_quit(&mut cx).is_empty());
    assert!(!host.take_quit_ready());
    assert!(host.teardown(&mut cx, 1));
}
