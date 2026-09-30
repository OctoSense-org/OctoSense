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

struct PeerProbeModule;

impl AppModule for PeerProbeModule {
    fn id(&self) -> &'static str {
        // Not in native-apps.json: granted no agent.
        "peer-probe"
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

static PROBE: PeerProbeModule = PeerProbeModule;

fn setup() -> (Cx, ModuleHost) {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    cx.with_vm(makepad_widgets::script_mod);
    (cx, ModuleHost::default())
}

fn create(cx: &mut Cx, host: &mut ModuleHost, client: u64) {
    host.create(cx, client, &PROBE, PROBE.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
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
fn should_serve_a_modules_peer_link_like_a_process_socket_when_the_module_opens_one() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 901);
    create(&mut cx, &mut host, 902);
    tell(&mut cx, &mut host, 901, "open");
    assert!(host.has_peer_link(901), "the link is the instance's that opened it");
    assert!(!host.has_peer_link(902), "and no other instance's");
    // Its request reaches the shell's peer link, which answers it as it
    // answers a process of an app with no granted agent.
    tell(&mut cx, &mut host, 901, "session");
    tell(&mut cx, &mut host, 901, "read");
    let events = seen(&mut cx, &mut host, 901);
    match events.as_slice() {
        [PeerEvent::Reply { result: Err(error), .. }] => assert!(error.starts_with("no_agent"), "{error}"),
        other => panic!("one reply, from the shell's peer link: {other:?}"),
    }
    assert!(host.teardown(&mut cx, 901) && host.teardown(&mut cx, 902));
}

#[test]
fn should_drop_a_link_when_no_instance_or_a_second_one_opened_it() {
    let (mut cx, mut host) = setup();
    create(&mut cx, &mut host, 911);
    // Parked while no module code ran: nobody's, dropped before the next
    // module call can be blamed for it.
    let (_peer, stray) = OctosPeer::in_process();
    cx.global::<PendingPeerLinks>().links.push(stray);
    tell(&mut cx, &mut host, 911, "hello");
    assert!(!host.has_peer_link(911));
    assert!(cx.global::<PendingPeerLinks>().links.is_empty());
    // One link per instance.
    tell(&mut cx, &mut host, 911, "open");
    tell(&mut cx, &mut host, 911, "open-again");
    assert!(host.has_peer_link(911));
    assert!(host.teardown(&mut cx, 911));
    assert!(!host.has_peer_link(911));
}
