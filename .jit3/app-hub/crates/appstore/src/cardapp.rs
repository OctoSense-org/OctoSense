//! An installed card app as an app of its own.
//!
//! The store installs; this runs. One module, `card`, opened with the id of
//! an installed app, becomes that app's window-manager client: its own tile,
//! its own entry in recents, its own close. It re-checks the app against the
//! last verified catalog before drawing anything, so a withdrawal reaches an
//! app that was installed long ago, with no store screen involved.
//!
//! Everything about containment is the same as the store's own runner was:
//! the policy resolved from the manifest is applied to the isolate, the
//! artwork is served from a loopback origin that serves only this bundle,
//! and that origin's port is the one loopback entry the isolate may reach.
use makepad_app_module::{
    makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult},
    AppModule, ExecOutcome, InstanceHandles, InstanceParts, OpenArgKind, OpenSchema, ServiceExecutor, ValidatedOpen,
};
use makepad_widgets::*;
use octosense_app_hub::Store;
use octosense_app_policy::HostLimits;
use std::path::PathBuf;

script_mod! {
    use mod.prelude.widgets.*

    mod.widgets.CardAppView = set_type_default() do #(CardAppView::register_widget(vm)) {
        width: Fill height: Fill flow: Overlay
        show_bg: true draw_bg.color: #fff
        body := View { width: Fill height: Fill flow: Down
            notice := Label { width: Fill text: "" draw_text.color: #b00 draw_text.text_style.font_size: 12 margin: 16 }
            card := Splash { width: Fill height: Fill }
        }
        // A host service's sheet over the app (services.rs): its own isolate,
        // under no app's policy, where the person types what the app must
        // never see.
        sheet := Splash { visible: false width: Fill height: Fill }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct CardAppView {
    #[deref]
    view: View,
    #[rust]
    app_id: String,
    #[rust]
    card_surface: SplashRef,
    #[rust]
    host_sheet: SplashRef,
    #[rust]
    host_notice: LabelRef,
    #[rust]
    started: bool,
    #[rust]
    asset_server: Option<octosense_app_policy::AssetServer>,
    #[rust]
    host_dir: PathBuf,
    /// An installed app's verified copy, outside its jail, which it runs
    /// from; held for as long as it is open, then removed.
    #[rust]
    launch: Option<octosense_app_hub::PreparedLaunch>,
    #[rust]
    tool_registration: Option<crate::script_tools::Registration>,
    #[rust]
    admitted_prompts: bool,
    #[rust]
    app_suspended: bool,
}

impl CardAppView {
    fn start(&mut self, cx: &mut Cx) {
        // Capture host identities before untrusted app widgets exist.
        self.card_surface = self.view.splash(cx, ids!(body.card));
        self.host_sheet = self.view.splash(cx, ids!(sheet));
        self.host_notice = self.view.label(cx, ids!(body.notice));
        let root = crate::data_root(cx);
        // `.host` can never be an app id, so it is no app's jail.
        self.host_dir = root.join(".host");
        // A system app shipped with the build; anything else must be an
        // installed app the last verified catalog still offers.
        let (policy, bundle, statics) = match crate::system::system_app(&self.app_id) {
            Some(app) => match crate::system::prepare(&root, &app) {
                Ok((bundle, policy)) => (policy, bundle, app.assets),
                Err(e) => return self.refuse(cx, &format!("Cannot open {}: {e}", app.name)),
            },
            None => {
                let anchor = std::env::var("OCTOSENSE_HUB_ANCHOR").unwrap_or_else(|_| crate::DEFAULT_ANCHOR.to_string());
                // An install from before `.bundles` leaves the app's storage
                // before it is checked and run.
                if let Err(e) = octosense_app_hub::adopt_legacy_install(&root, &self.app_id) {
                    return self.refuse(cx, &format!("Cannot open: {e}"));
                }
                let channel = match crate::source::CatalogChannel::from_environment(&root) {
                    Ok(channel) => channel,
                    Err(e) => return self.refuse(cx, &format!("Cannot open: {e}")),
                };
                let mut store = channel.configure(Store::new(&anchor, &root, HostLimits::default())
                    .with_host_api_versions(crate::host_api::available_versions()));
                // The catalog the store last verified. Without one, nothing
                // runs: an app the device cannot show was offered is not run
                // on trust.
                let catalog = channel.read_cache(&root).unwrap_or_default();
                if let Err(e) = store.accept_catalog(&catalog) {
                    return self.refuse(cx, &format!("Cannot open {}: no verified catalog on this device ({e})", self.app_id));
                }
                // The installed release, verified against the catalog and run
                // from a copy outside the app's jail: an app cannot rewrite
                // the code it runs next time, and an update cannot change it
                // while it runs.
                match store.prepare_launch(&self.app_id) {
                    Ok(prepared) => {
                        let (policy, bundle) = (prepared.policy.clone(), prepared.bundle().to_path_buf());
                        self.launch = Some(prepared);
                        (policy, bundle, &[] as octosense_app_policy::StaticAssets)
                    }
                    Err(e) => return self.refuse(cx, &format!("Cannot open: {e}")),
                }
            }
        };
        let mut settings = policy.isolate_settings(&root);
        self.admitted_prompts = settings.host_prompts;
        settings.host_prompts &= !self.app_suspended;
        if let Err(e) = std::fs::create_dir_all(&settings.jail_root) {
            return self.refuse(cx, &format!("Cannot make the app's storage: {e}"));
        }
        let server = match octosense_app_policy::AssetServer::start_with_static(&bundle, statics) {
            Ok(server) => server,
            Err(e) => return self.refuse(cx, &format!("Cannot serve the app's artwork: {e}")),
        };
        settings.hosts.push(server.allowlist_entry());
        let origin = server.origin().to_string();
        self.asset_server = Some(server);
        let splash = self.card_surface.clone();
        let applied = octosense_app_policy::splash_adapter::apply(&splash, cx, &settings);
        log!(
            "card: {} running under {} capability(ies), {} host(s), {} bytes of storage, {} instructions, {} bytes of heap",
            self.app_id, applied.capabilities, applied.hosts, applied.storage_quota, applied.instruction_budget, applied.memory_bytes
        );
        if let Err(error) = crate::apply_device_consent(cx, &bundle, &splash) {
            return self.refuse(cx, &format!("Cannot apply app permissions: {error}"));
        }
        match crate::card_source(&bundle, &origin) {
            Ok(source) => {
                splash.set_text(cx, &source);
                match crate::script_tools::bind(cx, &self.app_id, &bundle, &splash) {
                    Ok(registration) => self.tool_registration = registration,
                    Err(error) => self.refuse(cx, &format!("App tools unavailable: {error}")),
                }
            },
            Err(e) => self.refuse(cx, &format!("The app did not open: {e}")),
        }
    }

    fn refuse(&mut self, cx: &mut Cx, reason: &str) {
        self.tool_registration = None;
        self.admitted_prompts = false;
        self.card_surface.set_host_tag(cx, None);
        self.card_surface.set_host_prompts(cx, false);
        error!("card: {reason}");
        self.host_notice.set_text(cx, reason);
        self.view.redraw(cx);
    }
}

impl Widget for CardAppView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if matches!(event, Event::Pause | Event::Background) { self.app_suspended = true; }
        else if matches!(event, Event::Resume | Event::Foreground) { self.app_suspended = false; }
        if !self.started {
            self.started = true;
            self.start(cx);
        }
        let (card, sheet) = (self.card_surface.clone(), self.host_sheet.clone());
        // Set the runner-owned baseline BEFORE scripts receive callbacks. The
        // tool pump may suppress it but must never enlarge this authority.
        let may_prompt = self.admitted_prompts && !self.app_suspended
            && !crate::script_tools::pending_for(cx, &self.app_id, &card);
        card.set_host_prompts(cx, may_prompt);
        // A sheet is modal: while it is up, the person's input is for it, and
        // the app underneath must not take a tap meant for a password field.
        let sheet_up = sheet.borrow().map(|s| s.view.visible).unwrap_or(false);
        if sheet_up && crate::services::is_sheet_input_event(event) {
            sheet.handle_event(cx, event, scope);
        } else {
            self.view.handle_event(cx, event, scope);
        }
        crate::script_tools::pump(cx, &self.app_id, &card);
        crate::services::pump(cx, &self.app_id, &self.host_dir, &card, &sheet);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

pub struct CardModule;
pub static CARD_MODULE: CardModule = CardModule;

impl AppModule for CardModule {
    fn id(&self) -> &'static str {
        "card"
    }
    fn label(&self) -> &'static str {
        "Card app"
    }
    /// The module itself asks for nothing; each app it runs is bounded by
    /// its own manifest, resolved and enforced per isolate.
    fn capabilities(&self) -> &'static [&'static str] {
        &[]
    }
    fn open_schema(&self) -> OpenSchema {
        OpenSchema::new(1).arg("app", OpenArgKind::Text, true)
    }
    fn register(&self, vm: &mut ScriptVm) {
        crate::register_policy_runtime_features();
        crate::host_api::register_runtime_feature("app_tools.dispatch", 1);
        octoscript_widgets::design::script_mod(vm);
        octoscript_widgets::kit::script_mod(vm);
        script_mod(vm);
        crate::register_card_vocabulary();
    }
    fn create(&self, vm: &mut ScriptVm, open: ValidatedOpen, _handles: InstanceHandles) -> InstanceParts {
        let value = script_eval!(vm, { use mod.widgets.* CardAppView {} });
        let root = WidgetRef::script_from_value(vm, value);
        if let Some(mut view) = root.borrow_mut::<CardAppView>() {
            view.app_id = open.text("app").unwrap_or_default().to_string();
        }
        let closing = root.clone();
        InstanceParts {
            root,
            executor: Box::new(CardExecutor),
            // The camera and any web views the app opened go when the app
            // does, not whenever its isolate is next collected.
            shutdown: Box::new(move |vm| {
                let cx = vm.cx_mut();
                if let Some(mut view) = closing.borrow_mut::<CardAppView>() { view.tool_registration = None; }
                // Use the same identities as event delivery. A guest child
                // named `sheet` must not redirect cancellation at shutdown.
                let Some((splash, sheet)) = closing.borrow::<CardAppView>()
                    .map(|view| (view.card_surface.clone(), view.host_sheet.clone())) else { return };
                let heap = splash.borrow_mut().and_then(|mut s| s.isolate_heap_key(cx));
                if let Some(heap) = heap {
                    makepad_widgets::camera_preview::release_isolate_devices(cx, heap);
                    crate::services::cancel_heap(heap);
                }
                splash.set_host_tag(cx, None);
                splash.set_host_prompts(cx, false);
                // The sheet's own requests (a sign-in form's) end with it.
                if let Some(heap) = sheet.borrow_mut().and_then(|mut s| s.isolate_heap_key(cx)) {
                    crate::services::cancel_heap(heap);
                }
            }),
        }
    }
}

struct CardExecutor;

impl ServiceExecutor for CardExecutor {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new("card", "Card app", "An installed card app, running in its own contained isolate.")
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(ToolResult::unavailable(&call.call_id, "A card app offers no tools to the assistant yet"))
    }
    fn cancel(&mut self, _cx: &mut Cx, _call_id: &str) {}
    fn subscribe(&mut self, _cx: &mut Cx, _sub_id: &str, _topic: &str, _filter: Option<&str>) {}
    fn unsubscribe(&mut self, _cx: &mut Cx, _sub_id: &str) {}
}

#[allow(dead_code)]
fn _unused(_: PathBuf) {}

#[cfg(test)]
mod modal_tests {
    use super::*;
    use std::cell::RefCell;
    use makepad_widgets::makepad_script::ScriptFnRef;
    thread_local! {static SEEN: RefCell<Vec<String>> = const {RefCell::new(Vec::new())};}
    script_mod! {
        use mod.prelude.widgets.*
        mod.widgets.ModalInputProbe = set_type_default() do #(ModalInputProbe::register_widget(vm)) {}
        mod.prelude.widgets.ModalInputProbe = mod.widgets.ModalInputProbe
        mod.widgets.PromptCallbackProbe = set_type_default() do #(PromptCallbackProbe::register_widget(vm)) {}
        mod.prelude.widgets.PromptCallbackProbe = mod.widgets.PromptCallbackProbe
    }
    #[test]
    fn real_start_binds_admitted_storage_identity_before_guest_source() {
        const CHILD: &str = "APP_HUB_TEST_RUNNER_IDENTITY";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "cardapp::modal_tests::real_start_binds_admitted_storage_identity_before_guest_source", "--nocapture"])
                .env(CHILD, "1").output().unwrap();
            assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            return;
        }
        const APP: &str = "os.identity-fixture";
        let directory = std::env::temp_dir().join(format!("appstore-real-identity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let bundle = directory.join("source");
        let data = directory.join("apps");
        std::fs::create_dir_all(&bundle).unwrap();
        std::env::set_var("OCTOSENSE_APP_DATA", &data);
        crate::set_data_root(data.clone());
        std::fs::write(bundle.join("main.splash"), r#"
            fs.write("identity.txt", "owned")
            host.request("identity_probe.ready", {text: fs.read("identity.txt")}, fn(r){})
            Label {text: "Identity fixture"}
        "#).unwrap();
        let digest = octosense_app_policy::digest_dir(&bundle).unwrap();
        std::fs::write(bundle.join("manifest.json"), serde_json::json!({
            "schema":1,"id":APP,"name":"Identity fixture","version":"1.0.0",
            "capabilities":[],"integrity":{"bundle_blake3":digest}
        }).to_string()).unwrap();
        let pack = serde_json::to_string(&octosense_app_hub::pack::pack_dir(&bundle).unwrap()).unwrap();
        crate::system::register_system_app(crate::system::SystemApp {
            id: APP, name: "Identity fixture", pack: Box::leak(pack.into_boxed_str()), assets: &[],
        });
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let root = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            CARD_MODULE.register(vm);
            let value = script_eval!(vm, {use mod.widgets.* CardAppView {}});
            WidgetRef::script_from_value(vm, value)
        });
        let card = {
            let mut runner = root.borrow_mut::<CardAppView>().unwrap();
            runner.app_id = APP.into();
            // This is the production start path: no manual set_host_tag shim.
            runner.start(&mut cx);
            runner.card_surface.clone()
        };
        let heap = card.borrow_mut().unwrap().isolate_heap_key(&mut cx).unwrap();
        let requests = splash_host::take_splash_host_requests_for(&[heap]);
        assert_eq!(requests.len(), 1, "boot-time host request must reach the host");
        assert_eq!(requests[0].app_tag, APP);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&requests[0].args_json).unwrap(), serde_json::json!({"text":"owned"}));
        assert_eq!(std::fs::read_to_string(data.join(APP).join("identity.txt")).unwrap(), "owned");
        #[cfg(feature = "text-input-state-query")]
        {
            let storage = splash_storage::storage_for_heap(heap, APP).expect("the admitted app owns its live jail");
            assert_eq!(storage.read_bytes("identity.txt").unwrap(), b"owned");
            assert!(splash_storage::storage_for_heap(heap, "os.other-identity").is_none());
            // Identity permits lookup, never device access without consent.
            assert!(splash_policy::service_allowed(heap, "camera.capture").is_err());
            assert!(splash_policy::service_allowed(heap, "microphone.record_start").is_err());
            assert!(splash_policy::service_allowed(heap, "location.get").is_err());
        }
        // A failed replacement must not leave the old app's native identity.
        std::fs::write(bundle.join("manifest.json"), "invalid manifest").unwrap();
        assert!(crate::apply_device_consent(&mut cx, &bundle, &card).is_err());
        #[cfg(feature = "text-input-state-query")]
        assert!(splash_storage::storage_for_heap(heap, APP).is_none());
        card.set_text(&mut cx, r#"host.request("identity_probe.after_error", {}, fn(r){}) Label{}"#);
        let heap = card.borrow_mut().unwrap().isolate_heap_key(&mut cx).unwrap();
        let requests = splash_host::take_splash_host_requests_for(&[heap]);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].app_tag, "");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[derive(Script, ScriptHook, Widget)]
    struct ModalInputProbe {
        #[deref] view: View,
        #[live] label: String,
    }
    impl Widget for ModalInputProbe {
        fn handle_event(&mut self, _: &mut Cx, event: &Event, _: &mut Scope) {
            if matches!(event, Event::TextInput(_) | Event::KeyDown(_)) {
                SEEN.with(|seen| seen.borrow_mut().push(self.label.clone()));
            }
        }
        fn draw_walk(&mut self, _: &mut Cx2d, _: &mut Scope, _: Walk) -> DrawStep {DrawStep::done()}
    }
    #[test]
    fn host_sheet_exclusively_receives_text_and_keys_above_installed_app() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        widget_async::register_splash_isolate_mod(|vm| {script_mod(vm);});
        let root = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            super::script_mod(vm);
            let value = script_eval!(vm, {use mod.widgets.* CardAppView {}});
            WidgetRef::script_from_value(vm, value)
        });
        {
            let mut runner = root.borrow_mut::<CardAppView>().unwrap();
            runner.started = true;
            runner.app_id = "org.example.modal-input".into();
            runner.card_surface = runner.view.splash(&mut cx, ids!(body.card));
            runner.host_sheet = runner.view.splash(&mut cx, ids!(sheet));
            runner.card_surface.set_text(&mut cx, "sheet := View{}\nModalInputProbe {label: \"app\"}");
            runner.host_sheet.set_text(&mut cx, "ModalInputProbe {label: \"host\"}");
            runner.host_sheet.borrow_mut().unwrap().view.visible = true;
        }
        SEEN.with(|seen| seen.borrow_mut().clear());
        root.handle_event(&mut cx, &Event::TextInput(TextInputEvent {input: "private sheet text".into(), ..Default::default()}), &mut Scope::empty());
        root.handle_event(&mut cx, &Event::KeyDown(KeyEvent {key_code: KeyCode::KeyA, ..Default::default()}), &mut Scope::empty());
        assert_eq!(SEEN.with(|seen| seen.borrow().clone()), ["host", "host"], "underlying app must not receive host-sheet text or keys");
    }

    #[derive(Script, ScriptHook, Widget)]
    struct PromptCallbackProbe {
        #[deref] view: View,
        #[live] on_probe: ScriptFnRef,
    }
    impl Widget for PromptCallbackProbe {
        fn handle_event(&mut self, cx: &mut Cx, event: &Event, _: &mut Scope) {
            if matches!(event, Event::KeyDown(_)) {
                let vm_id = cx.script_ref_vm_id(&self.view.source).unwrap();
                cx.with_script_vm_id(vm_id, |vm| { vm.call(self.on_probe.clone().into(), &[]); });
            }
        }
        fn draw_walk(&mut self, _: &mut Cx2d, _: &mut Scope, _: Walk) -> DrawStep { DrawStep::done() }
    }

    #[test]
    fn runner_applies_admitted_foreground_and_tool_policy_before_guest_callbacks() {
        use crate::services::{self, HostService, Replier, ServiceHost};
        use std::sync::{Arc,Mutex};
        struct PromptProbe(Arc<Mutex<Vec<bool>>>);
        impl HostService for PromptProbe {
            fn family(&self) -> &'static str { "runner_prompt_test" }
            fn call(&mut self, call: services::ServiceCall, reply: Replier, _: &mut dyn ServiceHost) {
                self.0.lock().unwrap().push(call.may_prompt);
                reply.send(Ok(serde_json::Value::Null));
            }
        }
        let seen = Arc::new(Mutex::new(Vec::new()));
        services::register_host_service(Box::new(PromptProbe(seen.clone())));
        let mut cx = Cx::new(Box::new(|_, _| {}));
        widget_async::register_splash_isolate_mod(|vm| { script_mod(vm); });
        let root = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            super::script_mod(vm);
            let value = script_eval!(vm, {use mod.widgets.* CardAppView {}});
            WidgetRef::script_from_value(vm, value)
        });
        let app = "org.example.runner";
        let body = r#"fn probe() { host.request("runner_prompt_test.permission", {}, fn(r){}) }
            fn app_tool(name, call_id) {}
            probe_widget := PromptCallbackProbe {on_probe: fn(){probe()}}"#;
        let card = {
            let mut runner = root.borrow_mut::<CardAppView>().unwrap();
            runner.started = true;
            runner.app_id = app.into();
            runner.admitted_prompts = true;
            runner.card_surface = runner.view.splash(&mut cx, ids!(body.card));
            runner.host_sheet = runner.view.splash(&mut cx, ids!(sheet));
            runner.card_surface.set_host_tag(&mut cx,Some(app.into()));
            runner.card_surface.set_host_caps(&mut cx,vec!["runner_prompt_test".into()]);
            runner.card_surface.set_policy(&mut cx,Some(vec![]),Some(100_000));
            runner.card_surface.set_text(&mut cx,body);
            runner.card_surface.clone()
        };
        let key = Event::KeyDown(KeyEvent {key_code:KeyCode::KeyA,..Default::default()});
        for event in [&Event::Background,&key,&Event::Resume,&key] {
            root.handle_event(&mut cx,event,&mut Scope::empty());
        }
        root.borrow_mut::<CardAppView>().unwrap().admitted_prompts = false;
        root.handle_event(&mut cx,&key,&mut Scope::empty());
        root.borrow_mut::<CardAppView>().unwrap().admitted_prompts = true;
        let dir = std::env::temp_dir().join(format!("runner-prompt-tools-{}",std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.splash"),body).unwrap();
        std::fs::write(dir.join("tools.json"),serde_json::json!({"schema":1,"tools":[{
            "name":"runner.wait","description":"Wait","implemented_by":"app","risk":"read",
            "input_schema":{"type":"object"},"output_schema":{"type":"object"}
        }]}).to_string()).unwrap();
        let mut manifest = serde_json::json!({"schema":1,"id":app,"name":"Fixture","version":"0.1.0","requires":["script-tools-v1"],"integrity":{"bundle_blake3":""}});
        std::fs::write(dir.join("manifest.json"),manifest.to_string()).unwrap();
        manifest["integrity"]["bundle_blake3"] = serde_json::json!(octosense_app_policy::contract::digest_dir(&dir).unwrap());
        std::fs::write(dir.join("manifest.json"),manifest.to_string()).unwrap();
        let registration = crate::script_tools::bind(&mut cx,app,&dir,&card).unwrap();
        let token = crate::script_tools::submit(app,"runner.wait",serde_json::json!({}),"device","system",std::time::Duration::from_secs(10),Box::new(|_|{})).unwrap();
        // This guest callback runs inside view.handle_event BEFORE tool pump.
        root.handle_event(&mut cx,&key,&mut Scope::empty());
        crate::script_tools::cancel(&token);
        drop(registration);
        assert_eq!(*seen.lock().unwrap(),[false,true,false,false]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn shutdown_cancels_the_real_sheet_even_when_a_guest_reuses_its_id() {
        use crate::services::{self, HostService, Replier, ServiceHost};
        use makepad_app_module::{InstanceScope, ModuleWindows, ReplySink, Viewport};
        use std::sync::{Arc, Mutex};
        struct Hold(Arc<Mutex<Vec<Replier>>>);
        impl HostService for Hold {
            fn family(&self) -> &'static str { "card_shutdown_probe" }
            fn call(&mut self, _: services::ServiceCall, reply: Replier, _: &mut dyn ServiceHost) {
                self.0.lock().unwrap().push(reply);
            }
        }
        let held = Arc::new(Mutex::new(Vec::new()));
        services::register_host_service(Box::new(Hold(held.clone())));
        let mut cx = Cx::new(Box::new(|_, _| {}));
        let mut parts = cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            CARD_MODULE.register(vm);
            let handles = InstanceHandles {
                scope: InstanceScope::new(932, 1),
                storage: vm.cx_mut().storage("card-shutdown-test"),
                viewport: Viewport::default(),
                replies: ReplySink::pair().0,
                windows: ModuleWindows::default(),
            };
            let open = CARD_MODULE.open_schema().validate(r#"{"app":"org.example.shutdown"}"#, &[]).unwrap();
            CARD_MODULE.create(vm, open, handles)
        });
        let (card, sheet) = {
            let mut runner = parts.root.borrow_mut::<CardAppView>().unwrap();
            runner.started = true;
            runner.card_surface = runner.view.splash(&mut cx, ids!(body.card));
            runner.host_sheet = runner.view.splash(&mut cx, ids!(sheet));
            (runner.card_surface.clone(), runner.host_sheet.clone())
        };
        card.set_text(&mut cx, "sheet := View{}\nhost.request(\"card_shutdown_probe.hold\", {}, fn(r){})");
        sheet.set_text(&mut cx, "host.request(\"card_shutdown_probe.hold\", {}, fn(r){})\nLabel{text: \"Host sheet\"}");
        sheet.borrow_mut().unwrap().view.visible = true;
        assert_ne!(parts.root.widget(&mut cx, ids!(sheet)).widget_uid(), sheet.widget_uid());
        let heaps = [
            card.borrow_mut().unwrap().isolate_heap_key(&mut cx).unwrap(),
            sheet.borrow_mut().unwrap().isolate_heap_key(&mut cx).unwrap(),
        ];
        services::pump(&mut cx, "org.example.shutdown", &std::env::temp_dir(), &card, &sheet);
        assert_eq!(held.lock().unwrap().len(), 2);
        cx.with_vm(|vm| (parts.shutdown)(vm));
        for reply in held.lock().unwrap().drain(..) { reply.send(Ok(serde_json::Value::Null)); }
        assert!(services::take_replies_for(&heaps).is_empty(), "neither the closed app nor its real sheet may receive a late answer");
    }
}
