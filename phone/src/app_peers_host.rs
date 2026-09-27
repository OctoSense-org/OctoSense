//! Apps' assistant access (Rinx ADR 0007): the shell side of
//! `octosense-app-peers`.
//!
//! When the window manager creates a native module instance, Home reads the
//! module's declared capabilities. If host policy grants any of the exact
//! `octos.*` assistant services, Home creates a scoped service for that
//! instance, backed by ONE octos peer per app and account. The peer is owned
//! by the shell's system agent (`_main:api:octosense#system`) and runs on the
//! shell's kernel (`octosense_octos_core`). The service is offered to the
//! instance for the duration of its `create` only. A module that declares or
//! is granted nothing gets no service, and no peer is ever allocated for it.
//! Tearing the instance down releases its leases and interrupts its peer's
//! running work. The kernel and other apps keep running.
//!
//! Host policy: native modules that ship with Home are granted what they
//! declare from this list. The person's AI provider choice lives in
//! Settings → Accounts → AI providers; a per-app toggle is future work.

use makepad_app_module::{AppModule, InstanceScope};

/// The native modules Home lets use the assistant, and with what.
#[cfg(any(feature = "octos-core", native_mobile))]
pub fn policy() -> &'static octosense_app_peers::hosted::HostPolicy {
    static POLICY: std::sync::OnceLock<octosense_app_peers::hosted::HostPolicy> =
        std::sync::OnceLock::new();
    POLICY.get_or_init(|| {
        let policy = octosense_app_peers::hosted::HostPolicy::default();
        // Rinx: its native mini-app host serves these to reviewed mini apps.
        policy.allow("rinx", octosense_app_peers::OCTOS_SERVICES);
        policy
    })
}

/// One instance's assistant service, held by the module host.
pub struct Assistant {
    #[cfg(any(feature = "octos-core", native_mobile))]
    broker: octosense_app_peers::broker::Broker,
}

impl Assistant {
    /// The instance is going away: release its leases and contexts.
    pub fn release(&self) {
        #[cfg(any(feature = "octos-core", native_mobile))]
        octosense_app_peers::OctosAppService::release(&self.broker);
    }
}

/// Before `module.create`: offer the instance its service, if it gets one.
pub fn offer(module: &dyn AppModule, scope: &InstanceScope) -> Option<Assistant> {
    #[cfg(any(feature = "octos-core", native_mobile))]
    {
        let broker = octosense_app_peers::hosted::launch(
            module.id(),
            module.label(),
            module.capabilities().iter().copied(),
            policy(),
        )?;
        octosense_app_peers::hosted::offer(module.id(), &scope.to_string(), &broker);
        Some(Assistant { broker })
    }
    #[cfg(not(any(feature = "octos-core", native_mobile)))]
    {
        let _ = (module, scope);
        None
    }
}

/// After `module.create`: drop an offer the module did not take.
pub fn withdraw(module: &dyn AppModule, scope: &InstanceScope) {
    #[cfg(any(feature = "octos-core", native_mobile))]
    octosense_app_peers::injection::withdraw(module.id(), &scope.to_string());
    #[cfg(not(any(feature = "octos-core", native_mobile)))]
    let _ = (module, scope);
}

#[cfg(all(test, any(feature = "octos-core", native_mobile)))]
mod tests {
    use super::*;
    use makepad_app_module::*;
    use makepad_widgets::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    static CLAIMED: AtomicBool = AtomicBool::new(false);

    /// A module that declares the assistant and records whether it got one.
    struct Probe(&'static str, &'static [&'static str]);
    impl AppModule for Probe {
        fn id(&self) -> &'static str {
            self.0
        }
        fn label(&self) -> &'static str {
            "Probe"
        }
        fn capabilities(&self) -> &'static [&'static str] {
            self.1
        }
        fn open_schema(&self) -> OpenSchema {
            OpenSchema::new(1)
        }
        fn register(&self, _vm: &mut ScriptVm) {}
        fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, handles: InstanceHandles) -> InstanceParts {
            let service = octosense_app_peers::injection::claim(self.0, &handles.scope.to_string());
            CLAIMED.store(service.is_some(), Ordering::SeqCst);
            let value = script_eval!(vm, { use mod.prelude.widgets.* View {} });
            InstanceParts {
                root: WidgetRef::script_from_value(vm, value),
                executor: Box::new(NoExecutor),
                shutdown: Box::new(|_| {}),
            }
        }
    }
    struct NoExecutor;
    impl ServiceExecutor for NoExecutor {
        fn manifest(&self) -> makepad_ai_services::wire::ServiceManifest {
            makepad_ai_services::wire::ServiceManifest::new("probe", "Probe", "test")
        }
        fn execute(&mut self, _cx: &mut Cx, call: &makepad_ai_services::wire::ServiceCall) -> ExecOutcome {
            ExecOutcome::Done(makepad_ai_services::wire::ToolResult::unavailable(&call.call_id, "test"))
        }
    }

    static AI_PROBE: Probe = Probe("assistant-probe", &["storage", "octos.session.open", "octos.turn.start"]);
    static PLAIN_PROBE: Probe = Probe("plain-probe", &["storage", "net"]);
    static UNGRANTED_PROBE: Probe = Probe("ungranted-probe", &["octos.turn.start"]);

    #[test]
    fn a_granted_module_gets_its_service_at_creation_and_others_get_none() {
        policy().allow("assistant-probe", ["octos.session.open", "octos.turn.start"]);
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut host = crate::module_host::ModuleHost::default();
        host.create(&mut cx, 1, &AI_PROBE, AI_PROBE.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
        assert!(CLAIMED.load(Ordering::SeqCst), "the granted module got a scoped service");
        assert!(host.assistant_of(1).is_some());
        host.create(&mut cx, 2, &PLAIN_PROBE, PLAIN_PROBE.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
        assert!(!CLAIMED.load(Ordering::SeqCst), "a module without assistant services gets none");
        assert!(host.assistant_of(2).is_none(), "no peer is allocated for it");
        host.create(&mut cx, 3, &UNGRANTED_PROBE, UNGRANTED_PROBE.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
        assert!(!CLAIMED.load(Ordering::SeqCst), "declaring is not being granted");
        assert!(host.assistant_of(3).is_none());
        // An offer never outlives its create: nothing is left to claim.
        assert!(octosense_app_peers::injection::claim("assistant-probe", "i1g1").is_none());
        assert!(host.teardown(&mut cx, 1));
    }
}
