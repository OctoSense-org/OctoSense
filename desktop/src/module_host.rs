//! The window manager as a module host (aicontrol.md §3): app instances
//! that run IN-PROCESS, one splash isolate each, instead of as child
//! processes.
//!
//! Creating one: allocate the isolate (the widget universe is installed
//! by the allocation itself), retint its stock theme from the WM palette,
//! let the module register its own families, and call `create` — all
//! inside ONE trusted entry into the isolate, so the module never holds a
//! second `&mut Cx` beside the VM. The root comes back minted in that
//! heap; the tile (`module_view.rs`) draws it; the executor answers the
//! assistant's calls through the bus's in-process leg (`ai_bus.rs`).
//!
//! Tearing one down, in order: the tile drops the root FIRST (so nothing
//! draws a widget whose heap is about to go), the instance's `shutdown`
//! runs in the isolate, the executor and the host's own root ref are
//! dropped, the isolate is freed — its script timers stop with it. What
//! the scope token does NOT yet reach — native timers, audio lanes,
//! native layers, HTTP requests the instance opened through the platform
//! — is the InstanceScope gap the next phase closes.

use crate::hub::ClientId;
use makepad_ai_services::wire::{ServiceCall, ServiceManifest};
use makepad_widgets::widget_async::{enter_isolate, leave_isolate};
use makepad_app_module::*;
use makepad_widgets::*;
use std::collections::HashMap;
use std::sync::mpsc::Receiver;

pub struct AppInstance {
    pub client: ClientId,
    pub module: &'static dyn AppModule,
    pub vm_id: SplashVmId,
    pub scope: InstanceScope,
    /// The n-th instance of this app in this session: `sheets.2`.
    pub instance_no: u64,
    pub root: WidgetRef,
    executor: Box<dyn ServiceExecutor>,
    shutdown: Option<Box<dyn FnOnce(&mut ScriptVm)>>,
    /// Results and publications the executor sent later.
    upstream: Receiver<ModuleUpstream>,
    /// The instance's requests for extra windows, and our reports of closes.
    pub windows: ModuleWindows,
    /// The instance's scoped assistant service (Rinx ADR 0007), when the
    /// module declares and is granted `octos.*` services.
    assistant: Option<octosense_ai_host::Assistant>,
}

impl AppInstance {
    pub fn manifest(&self) -> ServiceManifest {
        self.executor.manifest()
    }
}

#[derive(Default)]
pub struct ModuleHost {
    /// Whether new instances may open extra windows: the desktop shell,
    /// not the phone shell (whose apps are full-screen).
    pub extra_windows: bool,
    instances: HashMap<ClientId, AppInstance>,
    next_scope: u64,
    per_app: HashMap<String, u64>,
    style: Option<desktop_style::StyleSheet>,
}

/// The isolate removes mod.res after bootstrap. Trusted framework themes
/// still need its crate resource resolver for their bundled fonts. Expose
/// only that existing resolver during theme registration, then remove it.
fn apply_module_style(vm: &mut ScriptVm, sheet: &desktop_style::StyleSheet) {
    let mut inherited = sheet.clone();
    // A nested Splash (a Card app the `card` module runs) replays this
    // trusted theme after its ambient `mod.res` has been stripped. Bind only
    // the existing bundled-resource resolver in the theme's lexical scope, so
    // that replay can still load the style's fonts. This does not publish a
    // resource module to the card's source.
    inherited.theme = format!(
        "mod._octosense_widgets_before_style = mod.widgets\n\
         mod._octosense_prelude_before_style = mod.prelude.widgets\n\
         let crate_resource = mod.prelude.widgets.crate_resource\n{}", inherited.theme);
    // widgets_mod rebuilds these namespaces, including the prelude a Card's
    // lowered body uses. Retain host additions while letting the freshly
    // themed framework names replace their old ones.
    inherited.widgets = format!(
        "{}\n\
         mod.widgets = {{..mod._octosense_widgets_before_style, ..mod.widgets}}\n\
         mod.prelude.widgets = {{..mod._octosense_prelude_before_style, ..mod.prelude.widgets}}\n\
         mod._octosense_widgets_before_style = nil\n\
         mod._octosense_prelude_before_style = nil\n", inherited.widgets);
    desktop_style::install(vm, inherited);
    vm.with_reload(|vm| {
        script_eval!(vm, { mod.res = {crate_resource: mod.prelude.widgets.crate_resource} });
        makepad_widgets::widgets_mod(vm);
        desktop_style::apply_widgets(vm);
        script_eval!(vm, { mod.res = nil });
    });
}

impl ModuleHost {
    /// Build one instance of `module` for the client id the WM gave it.
    /// `viewport` is the tile size the layout will give it.
    pub fn create(
        &mut self,
        cx: &mut Cx,
        client: ClientId,
        module: &'static dyn AppModule,
        open: ValidatedOpen,
        viewport: DVec2,
    ) -> Result<(), String> {
        if self.instances.contains_key(&client) {
            return Err(format!("client {client} already hosts an instance"));
        }
        self.next_scope += 1;
        let scope = InstanceScope::new(client, self.next_scope);
        let instance_no = {
            let n = self.per_app.entry(module.id().to_string()).or_insert(0);
            *n += 1;
            *n
        };
        // The storage jail: a namespace of the Cx storage API, one per
        // instance (§3b's mount and the web's IndexedDB sit under it).
        let storage = cx.storage(&format!("{}.{}", module.id(), instance_no));
        let (replies, upstream) = ReplySink::pair();
        let windows = ModuleWindows::new(self.extra_windows);
        let handles = InstanceHandles { scope, storage, viewport: Viewport { size: viewport }, replies, windows: windows.clone() };
        let vm_id = cx.alloc_splash_vm_with_network(false);
        // The assistant is offered to THIS instance for the duration of its
        // create only; the module takes it there or never gets it.
        let offer = octosense_ai_host::offer(module, &scope);
        let parts = cx.with_script_vm_id_trusted(vm_id, |vm| {
            // The isolate came up with the stock theme; the WM's palette
            // retints it exactly as it retints a child process's.
            if let Some(sheet)=&self.style {
                apply_module_style(vm, sheet);
            }
            makepad_wm_theme::apply(vm);
            module.register(vm);
            module.create(vm, open, handles)
        });
        let assistant = offer.finish();
        log!(
            "wm: module instance {}.{} for client {} in isolate {:?} (scope {})",
            module.id(),
            instance_no,
            client,
            vm_id,
            scope
        );
        self.instances.insert(
            client,
            AppInstance {
                client,
                module,
                vm_id,
                scope,
                instance_no,
                root: parts.root,
                executor: parts.executor,
                shutdown: Some(parts.shutdown),
                upstream,
                windows,
                assistant,
            },
        );
        Ok(())
    }

    pub fn apply_style(&mut self,cx:&mut Cx,sheet:&desktop_style::StyleSheet) {
        self.style=Some(sheet.clone());
        for instance in self.instances.values_mut() {
            cx.with_script_vm_id_trusted(instance.vm_id,|vm| {
                apply_module_style(vm, sheet);
                vm.with_reload(|vm| {
                    makepad_wm_theme::apply(vm);
                    instance.module.register(vm);
                });
                let source=instance.root.widget_type_id().and_then(|ty|vm.bx.heap.type_default_for_id(ty)).unwrap_or_else(||instance.root.script_source());
                instance.root.script_apply(vm,&Apply::ScriptReapply,&mut Scope::empty(),source.into());
            });
            instance.root.redraw(cx);
        }
    }

    /// The assistant service the shell gave this instance, if any.
    pub fn assistant_of(&self, client: ClientId) -> Option<&octosense_ai_host::Assistant> {
        self.instances.get(&client)?.assistant.as_ref()
    }

    pub fn is_module(&self, client: ClientId) -> bool {
        self.instances.contains_key(&client)
    }

    pub fn get(&self, client: ClientId) -> Option<&AppInstance> {
        self.instances.get(&client)
    }

    /// The lowest client id hosting an instance of module `id`, if any.
    pub fn client_of_module(&self, id: &str) -> Option<ClientId> {
        self.instances
            .values()
            .filter(|i| i.module.id() == id)
            .map(|i| i.client)
            .min()
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// One of the assistant's calls, to the instance's executor — inside
    /// the instance's isolate, as the tile dispatches events: an executor
    /// reaches into its app's widgets (AppCard's `ask` is the composer).
    pub fn execute(&mut self, cx: &mut Cx, client: ClientId, call: &ServiceCall) -> Option<ExecOutcome> {
        let instance = self.instances.get_mut(&client)?;
        let entry = enter_isolate(cx, instance.vm_id);
        let outcome = instance.executor.execute(cx, call);
        leave_isolate(cx, entry);
        Some(outcome)
    }

    pub fn cancel(&mut self, cx: &mut Cx, client: ClientId, call_id: &str) {
        if let Some(instance) = self.instances.get_mut(&client) {
            instance.executor.cancel(cx, call_id);
        }
    }

    pub fn subscribe(
        &mut self,
        cx: &mut Cx,
        client: ClientId,
        sub_id: &str,
        topic: &str,
        filter: Option<&str>,
    ) {
        if let Some(instance) = self.instances.get_mut(&client) {
            instance.executor.subscribe(cx, sub_id, topic, filter);
        }
    }

    pub fn unsubscribe(&mut self, cx: &mut Cx, client: ClientId, sub_id: &str) {
        if let Some(instance) = self.instances.get_mut(&client) {
            instance.executor.unsubscribe(cx, sub_id);
        }
    }

    pub fn chat_open(&mut self, cx: &mut Cx, open: bool) {
        for instance in self.instances.values_mut() {
            instance.executor.chat_open(cx, open);
        }
    }

    /// Every result or publication an executor sent later, with its client.
    /// Instances' pending window requests: (owner, its isolate, its app id, request).
    pub fn take_window_requests(&mut self) -> Vec<(ClientId, SplashVmId, &'static str, WindowRequest)> {
        let mut out = Vec::new();
        for (client, instance) in &self.instances {
            for request in instance.windows.take_requests() {
                out.push((*client, instance.vm_id, instance.module.id(), request));
            }
        }
        out
    }

    /// The host starts or stops showing extra windows (desktop vs phone shell).
    pub fn set_extra_windows(&mut self, on: bool) {
        self.extra_windows = on;
        for instance in self.instances.values() {
            instance.windows.set_supported(on);
        }
    }

    /// The person closed `owner`'s window `key`.
    pub fn notify_window_closed(&self, owner: ClientId, key: LiveId) {
        if let Some(instance) = self.instances.get(&owner) {
            instance.windows.notify_closed(key);
        }
    }

    pub fn drain_upstream(&mut self) -> Vec<(ClientId, ModuleUpstream)> {
        let mut out = Vec::new();
        for (client, instance) in &self.instances {
            while let Ok(message) = instance.upstream.try_recv() {
                out.push((*client, message));
            }
        }
        out
    }

    /// End the instance: its shutdown runs in its isolate, then the isolate
    /// is freed. The caller has already cleared the tile's root.
    pub fn teardown(&mut self, cx: &mut Cx, client: ClientId) -> bool {
        let Some(mut instance) = self.instances.remove(&client) else {
            return false;
        };
        if let Some(shutdown) = instance.shutdown.take() {
            cx.with_script_vm_id_trusted(instance.vm_id, |vm| shutdown(vm));
        }
        // Release the app's assistant leases; the shared kernel stays.
        if let Some(assistant) = instance.assistant.take() {
            assistant.release();
        }
        let vm_id = instance.vm_id;
        let label = format!("{}.{}", instance.module.id(), instance.instance_no);
        // The last refs into the isolate's heap go before the heap does.
        drop(instance);
        cx.free_splash_vm(vm_id);
        log!("wm: module instance {label} torn down; isolate {vm_id:?} freed");
        true
    }
}

#[cfg(all(test, feature="app-sheets"))]
mod style_tests {
    use super::*;
    #[test]
    fn module_restyle_updates_custom_roles_and_keeps_instance() {
        let mut cx=Cx::new(Box::new(|_,_|{}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut host=ModuleHost::default();
        let module=&makepad_sheets::module::SHEETS_MODULE;
        let open=module.open_schema().validate("{}", &[]).unwrap();
        host.create(&mut cx,1,module,open,dvec2(900.0,700.0)).unwrap();
        let uid=host.get(1).unwrap().root.widget_uid();
        host.apply_style(&mut cx,&desktop_style::StyleSheet::load(desktop_style::DesktopStyle::Macos));
        let instance=host.get(1).unwrap();
        assert_eq!(instance.root.widget_uid(),uid);
        cx.with_script_vm_id_trusted(instance.vm_id,|vm| {
            let palette=makepad_wm_theme::current_for_vm(vm).unwrap();
            assert_eq!(palette.get("background"),Some("#ececec"));
            let sheets=vm.module(id!(sheets));
            assert_eq!(vm.bx.heap.value(sheets,id!(bg).into(),NoTrap).as_color(),Some(0xecececff));
            assert!(vm.take_errors().is_empty());
        });
        host.teardown(&mut cx,1);
    }
}

// The shell side of apps' assistant access (Rinx ADR 0007) is
// octosense-ai-host (`offer` above); these check it through a real create.
/// Rinx runs one instance per process: tests that create it take turns.
#[cfg(test)]
pub(crate) static RINX_INSTANCE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(all(test, any(feature = "octos-core", target_os = "android", target_os = "ios")))]
mod assistant_tests {
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
            let service = octosense_ai_host::app_peers::injection::claim(self.0, &handles.scope.to_string());
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
        octosense_ai_host::grant("assistant-probe", ["octos.session.open", "octos.turn.start"]);
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
        assert!(octosense_ai_host::app_peers::injection::claim("assistant-probe", "i1g1").is_none());
        assert!(host.teardown(&mut cx, 1));
    }

    /// The real Rinx module: hosted from creation, with the shell's service,
    /// and no kernel started by creating it (ADR 0007 criterion 7).
    #[cfg(feature = "app-rinx")]
    #[test]
    fn rinx_is_hosted_with_the_shells_service_and_starts_no_kernel() {
        let _one_rinx = super::RINX_INSTANCE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut host = crate::module_host::ModuleHost::default();
        let module = &rinx::module::RINX_MODULE;
        assert!(module.capabilities().contains(&"octos.turn.start"), "Rinx declares its assistant needs");
        host.create(&mut cx, 7, module, module.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
        assert!(host.assistant_of(7).is_some(), "the shell gave Rinx a scoped service");
        assert!(rinx::octos_service::is_hosted(), "hosted mode comes from module creation");
        let service = rinx::octos_service::service().expect("Rinx took the injected service");
        assert_eq!(service.deployment(), octosense_ai_host::app_peers::Deployment::Hosted);
        assert_eq!(service.settings_entry(), octosense_ai_host::app_peers::SettingsEntry::Host);
        assert!(!octosense_ai_host::kernel_running(), "creating Rinx starts no kernel");
        assert!(host.teardown(&mut cx, 7));
    }
}
