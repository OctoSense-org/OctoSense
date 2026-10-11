//! Kernel ports (ADR 0003, "An app that is an octos client"): a module that
//! opens Makepad's `OctosUiPort` gets it connected only when its entry in
//! native-apps.json names a `kernel` port; otherwise the host closes it and
//! says why. A port is the instance's whose code opened it, as a peer link
//! is (module_peer_tests.rs).

use crate::module_host::ModuleHost;
use makepad_ai_services::ui_port::{OctosUiPort, PendingUiPorts, UiPortEvent};
use makepad_ai_services::wire::{ServiceCall, ServiceManifest};
use makepad_app_module::*;
use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.PortProbeBase = #(PortProbe::register_widget(vm))
    mod.widgets.PortProbe = set_type_default() do mod.widgets.PortProbeBase {
        width: Fill
        height: Fill
    }
}

/// A root that opens its kernel port when told to.
#[derive(Script, ScriptHook, Widget)]
pub struct PortProbe {
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
    port: Option<OctosUiPort>,
}

impl Widget for PortProbe {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if let Event::Custom(message) = event {
            if message.as_str() == "open" {
                self.port = Some(OctosUiPort::open(cx));
            }
        }
    }

    fn draw_walk(&mut self, _cx: &mut Cx2d, _scope: &mut Scope, _walk: Walk) -> DrawStep {
        DrawStep::done()
    }
}

/// A probe under an id native-apps.json grants no kernel port.
struct PortProbeModule;

impl AppModule for PortProbeModule {
    fn id(&self) -> &'static str {
        "port-probe"
    }
    fn label(&self) -> &'static str {
        "Port probe"
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
        let value = script_eval!(vm, { use mod.widgets.* PortProbe {} });
        InstanceParts { root: WidgetRef::script_from_value(vm, value), executor: Box::new(NoTools), shutdown: Box::new(|_| {}) }
    }
}

struct NoTools;
impl ServiceExecutor for NoTools {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new("port-probe", "Port probe", "test")
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(makepad_ai_services::wire::ToolResult::unavailable(&call.call_id, "none"))
    }
}

static PROBE: PortProbeModule = PortProbeModule;

fn setup() -> (Cx, ModuleHost) {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(makepad_widgets::script_mod);
    (cx, ModuleHost::default())
}

/// What the probe's port has heard.
fn heard(cx: &mut Cx, host: &mut ModuleHost, client: u64) -> Option<UiPortEvent> {
    host.dispatch(cx, client, "a test read", |_, root| root.borrow_mut::<PortProbe>().unwrap().port.as_mut().and_then(|p| p.try_recv()))
        .unwrap()
}

#[test]
fn should_close_a_port_the_apps_entry_does_not_grant_and_say_why() {
    let (mut cx, mut host) = setup();
    host.create(&mut cx, 951, &PROBE, PROBE.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
    assert!(host.send_custom(&mut cx, 951, "open".into()));
    host.pump_peer_links(&mut cx);
    assert!(!host.has_kernel_port(951));
    assert_eq!(heard(&mut cx, &mut host, 951), Some(UiPortEvent::Closed { reason: "port-probe has no kernel port".into() }));
    assert!(host.teardown(&mut cx, 951));
}

#[test]
fn should_close_a_port_parked_outside_any_module() {
    let (mut cx, mut host) = setup();
    host.create(&mut cx, 952, &PROBE, PROBE.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
    let (mut stray, link) = OctosUiPort::in_process();
    cx.global::<PendingUiPorts>().ports.push(link);
    // The next module call finds it parked by nobody's code: closed, and
    // not the instance's.
    assert!(host.send_custom(&mut cx, 952, "hello".into()));
    host.pump_peer_links(&mut cx);
    assert!(cx.global::<PendingUiPorts>().ports.is_empty());
    assert!(!host.has_kernel_port(952));
    assert_eq!(stray.try_recv(), Some(UiPortEvent::Closed { reason: "opened outside any app".into() }));
    assert!(host.teardown(&mut cx, 952));
}
