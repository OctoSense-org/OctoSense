//! The peer link's in-process leg (ADR 0004 §5, #142): a module that opens
//! Makepad's `OctosPeer` is served by the shell's peer link, the code that
//! serves a process's socket. A probe module opens its link from its own
//! code (an event), and the host attributes the link to that instance and
//! nothing else.

use crate::module_host::ModuleHost;
use makepad_ai_services::peer::{OctosPeer, PeerEvent, PendingPeerLinks};
use makepad_ai_services::wire::{ServiceCall, ServiceManifest};
use makepad_app_module::*;
use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.PeerProbeBase = #(PeerProbe::register_widget(vm))
    mod.widgets.PeerProbe = set_type_default() do mod.widgets.PeerProbeBase {
        width: Fill
        height: Fill
    }
}

/// A root that opens its peer link and asks for its conversation when told
/// to, and keeps what came back.
#[derive(Script, ScriptHook, Widget)]
pub struct PeerProbe {
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
    peer: Option<OctosPeer>,
    #[rust]
    seen: Vec<PeerEvent>,
}

impl Widget for PeerProbe {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        if let Some(peer) = &mut self.peer {
            self.seen.extend(peer.handle_event(cx, event));
        }
        if let Event::Custom(message) = event {
            match message.as_str() {
                // The app's own call, the one a process app makes too.
                "open" => self.peer = Some(OctosPeer::open(cx)),
                "open-again" => drop(OctosPeer::open(cx)),
                "session" => {
                    if let Some(peer) = &mut self.peer {
                        peer.open_session(None);
                    }
                }
                _ => {}
            }
        }
    }

    fn draw_walk(&mut self, _cx: &mut Cx2d, _scope: &mut Scope, _walk: Walk) -> DrawStep {
        DrawStep::done()
    }
}

/// A probe under an id: `peer-probe` is not in native-apps.json (granted
/// no agent); `rinx` is granted `octos.*` there (the probe never asks
/// anything, so no consent sheet or peer is involved).
struct PeerProbeModule(&'static str);

impl AppModule for PeerProbeModule {
    fn id(&self) -> &'static str {
        self.0
    }
    fn label(&self) -> &'static str {
        "Peer probe"
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
        let value = script_eval!(vm, { use mod.widgets.* PeerProbe {} });
        InstanceParts { root: WidgetRef::script_from_value(vm, value), executor: Box::new(NoTools), shutdown: Box::new(|_| {}) }
    }
}

struct NoTools;
impl ServiceExecutor for NoTools {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new("peer-probe", "Peer probe", "test")
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(makepad_ai_services::wire::ToolResult::unavailable(&call.call_id, "none"))
    }
}

static PROBE: PeerProbeModule = PeerProbeModule("peer-probe");
static GRANTED: PeerProbeModule = PeerProbeModule("rinx");

fn setup() -> (Cx, ModuleHost) {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(makepad_widgets::script_mod);
    (cx, ModuleHost::default())
}

fn create(cx: &mut Cx, host: &mut ModuleHost, client: u64) {
    create_as(cx, host, client, &PROBE);
}

fn create_as(cx: &mut Cx, host: &mut ModuleHost, client: u64, module: &'static PeerProbeModule) {
    host.create(cx, client, module, module.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
}

/// A host message to the instance, then the shell's after-event pump.
fn tell(cx: &mut Cx, host: &mut ModuleHost, client: u64, message: &str) {
    assert!(host.send_custom(cx, client, message.into()));
    host.pump_peer_links(cx);
}

fn seen(cx: &mut Cx, host: &mut ModuleHost, client: u64) -> Vec<PeerEvent> {
    host.dispatch(cx, client, "a test read", |_, root| root.borrow::<PeerProbe>().unwrap().seen.clone()).unwrap()
}

#[test]
fn should_answer_no_agent_and_hold_no_link_when_the_module_is_granted_no_agent() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 901);
    tell(&mut cx, &mut host, 901, "open");
    assert!(!host.has_peer_link(901), "refused: not a link");
    // Its request still reaches the shell's peer link, which answers it as
    // it answers a process of an app with no granted agent.
    tell(&mut cx, &mut host, 901, "session");
    tell(&mut cx, &mut host, 901, "read");
    let events = seen(&mut cx, &mut host, 901);
    match events.as_slice() {
        [PeerEvent::Reply { result: Err(error), .. }] => assert!(error.starts_with("no_agent"), "{error}"),
        other => panic!("one reply, from the shell's peer link: {other:?}"),
    }
    assert!(host.teardown(&mut cx, 901));
}

#[test]
fn should_attribute_a_link_to_the_instance_that_opened_it_and_drop_strays_and_seconds() {
    let _rinx = crate::module_host::RINX_INSTANCE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut cx, mut host) = setup();
    create_as(&mut cx, &mut host, 911, &GRANTED);
    create(&mut cx, &mut host, 912);
    // Parked while no module code ran: nobody's, dropped before the next
    // module call can be blamed for it.
    let (_peer, stray) = OctosPeer::in_process();
    cx.global::<PendingPeerLinks>().links.push(stray);
    tell(&mut cx, &mut host, 911, "hello");
    assert!(!host.has_peer_link(911));
    assert!(cx.global::<PendingPeerLinks>().links.is_empty());
    tell(&mut cx, &mut host, 911, "open");
    assert!(host.has_peer_link(911), "the instance whose code opened it");
    assert!(!host.has_peer_link(912), "and no other");
    // One link per instance.
    tell(&mut cx, &mut host, 911, "open-again");
    assert!(host.has_peer_link(911));
    assert!(host.teardown(&mut cx, 911));
    assert!(!host.has_peer_link(911));
    assert!(host.teardown(&mut cx, 912));
}

#[test]
fn should_keep_the_outer_modules_link_when_a_nested_call_runs_another_module() {
    let _rinx = crate::module_host::RINX_INSTANCE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut cx, mut host) = setup();
    create_as(&mut cx, &mut host, 921, &GRANTED);
    create(&mut cx, &mut host, 922);
    let inner = host.get(922).unwrap().vm_id;
    // The outer module opens its link, then (still inside its call) the
    // host runs the other module's code: the link stays the outer's.
    host.dispatch(&mut cx, 921, "a nested test call", |cx, root| {
        let peer = OctosPeer::open(cx);
        crate::module_host::contain(cx, inner, "a nested call", |_| ());
        root.borrow_mut::<PeerProbe>().unwrap().peer = Some(peer);
    })
    .unwrap();
    host.pump_peer_links(&mut cx);
    assert!(host.has_peer_link(921), "the outer instance's");
    assert!(!host.has_peer_link(922), "not the nested one's");
    assert!(host.teardown(&mut cx, 921) && host.teardown(&mut cx, 922));
}
