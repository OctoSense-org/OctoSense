//! Native acceptance host for the Host API Lab fixture. Uses signed Store
//! admission, the real script-tool dispatcher and native OS permission status.
//! It does not start a model, confer agent consent or approve device access.
use makepad_app_module::AppModule;
use makepad_widgets::*;
use octosense_appstore::{host_api, script_tools, services};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
mod connected_support;
app_main!(App, font_set: International);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root { main_window := Window {
            window.inner_size: vec2(640, 650)
            body +: {flow: Overlay
                app := Splash {width: Fill height: Fill}
                sheet := Splash {width: Fill height: Fill visible: false}
            }
        }}
    }
}

#[derive(Script, ScriptHook)]
struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    app: String,
    #[rust]
    root: PathBuf,
    #[rust]
    receipt: Option<PathBuf>,
    #[rust]
    launch: Option<octosense_app_hub::PreparedLaunch>,
    #[rust]
    assets: Option<octosense_app_policy::AssetServer>,
    #[rust]
    registration: Option<script_tools::Registration>,
    #[rust]
    card: SplashRef,
    #[rust]
    sheet: SplashRef,
    #[rust]
    timer: Option<Timer>,
    #[rust]
    replies: Arc<Mutex<Vec<Result<Value, String>>>>,
    #[rust]
    preliminary: Value,
    #[rust]
    admitted_prompts: bool,
    #[rust]
    suspended: bool,
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        let arg =
            |name: &str| std::env::args().find_map(|a| a.strip_prefix(name).map(str::to_owned));
        let root =
            PathBuf::from(arg("--app-data=").expect("Pass an isolated --app-data directory"));
        let bundle = PathBuf::from(arg("--bundle=").expect("Pass --bundle=<Host API Lab bundle>"));
        let preview = std::env::args().any(|a| a == "--preview");
        octosense_appstore::set_data_root(root.clone());
        let policy = if preview {
            let manifest = std::fs::read_to_string(bundle.join("manifest.json")).unwrap();
            let digest = octosense_app_policy::digest_dir(&bundle).unwrap();
            octosense_app_policy::admit_and_resolve_dir(
                &manifest,
                &digest,
                &octosense_app_policy::HostLimits::default().with_require_signature(false),
                &octosense_app_policy::RefuseAllSignatures,
            )
            .expect("Preview admission")
        } else {
            let installed =
                connected_support::install(&[bundle.clone()], &root).expect("Signed installation");
            std::env::set_var(
                "OCTOSENSE_HUB_ANCHOR",
                installed["anchor"].as_str().unwrap(),
            );
            let launch = connected_support::open(&root, "org.octosense.samples.apilab").unwrap();
            let policy = launch.policy.clone();
            self.launch = Some(launch);
            policy
        };
        assert_eq!(policy.app_id, "org.octosense.samples.apilab");
        self.app = policy.app_id.clone();
        self.root = root.join(".host");
        let bundle = self
            .launch
            .as_ref()
            .map(|l| l.bundle().to_path_buf())
            .unwrap_or(bundle);
        let mut settings = policy.isolate_settings(&root);
        self.admitted_prompts = settings.host_prompts;
        std::fs::create_dir_all(&settings.jail_root).unwrap();
        let assets = octosense_app_policy::AssetServer::start_with_static(&bundle, &[]).unwrap();
        settings.hosts.push(assets.allowlist_entry());
        let source = octosense_app_policy::script_source(&bundle, assets.origin())
            .unwrap()
            .unwrap();
        self.card = self.ui.splash(cx, ids!(app));
        self.sheet = self.ui.splash(cx, ids!(sheet));
        octosense_app_policy::splash_adapter::apply(&self.card, cx, &settings);
        octosense_appstore::apply_device_consent(cx, &bundle, &self.card).unwrap();
        self.card.set_text(cx, &source);
        self.assets = Some(assets);
        self.registration = script_tools::bind(cx, &self.app, &bundle, &self.card).unwrap();
        self.timer = Some(cx.start_interval(0.05));
        if preview {
            return;
        }
        self.receipt = Some(PathBuf::from(
            arg("--receipt=").expect("Pass a new --receipt file"),
        ));
        assert!(
            !self.receipt.as_ref().unwrap().exists(),
            "Receipt must be a new file"
        );
        let submit = |name, args, account| {
            script_tools::submit(
                &self.app,
                name,
                args,
                account,
                "native-acceptance",
                Duration::from_secs(15),
                Box::new(|_| {}),
            )
        };
        let wrong_account = submit("apilab.inspect", json!({}), "other-account").unwrap_err();
        let undeclared = submit("apilab.not_declared", json!({}), "device").unwrap_err();
        let invalid_args =
            submit("apilab.inspect", json!({"injected":true}), "device").unwrap_err();
        self.preliminary = json!({"wrong_account":wrong_account,"undeclared_tool":undeclared,"invalid_arguments":invalid_args});
        let out = self.replies.clone();
        script_tools::submit(
            &self.app,
            "apilab.inspect",
            json!({}),
            "device",
            "native-acceptance",
            Duration::from_secs(15),
            Box::new(move |r| out.lock().unwrap().push(r)),
        )
        .unwrap();
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::theme_mod(vm);
        script_eval!(vm, {mod.theme = mod.themes.light});
        makepad_widgets::widgets_mod(vm);
        widget_async::set_splash_theme(widget_async::SplashTheme::Light);
        octosense_appstore::cardapp::CARD_MODULE.register(vm);
        octosense_shell::platform_services::register();
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        if matches!(event, Event::Pause | Event::Background) {
            self.suspended = true;
        }
        if matches!(event, Event::Resume | Event::Foreground) {
            self.suspended = false;
        }
        // Match the production CardAppView baseline; the test must not hide
        // provenance bugs by leaving this false forever after its first tool.
        let may_prompt = self.admitted_prompts
            && !self.suspended
            && !script_tools::pending_for(cx, &self.app, &self.card);
        self.card.set_host_prompts(cx, may_prompt);
        let sheet_up = self.sheet.borrow().is_some_and(|s| s.view.visible);
        if sheet_up && services::is_sheet_input_event(event) {
            self.sheet.handle_event(cx, event, &mut Scope::empty());
        } else {
            self.ui.handle_event(cx, event, &mut Scope::empty());
        }
        script_tools::pump(cx, &self.app, &self.card);
        services::pump(cx, &self.app, &self.root, &self.card, &self.sheet);
        octosense_shell::platform_services::handle_event(cx, event);
        let reply = self.replies.lock().unwrap().pop();
        if let Some(reply) = reply {
            let path = self.receipt.take().expect("One acceptance reply");
            self.registration = None;
            let closed = script_tools::submit(
                &self.app,
                "apilab.inspect",
                json!({}),
                "device",
                "native-acceptance",
                Duration::from_secs(1),
                Box::new(|_| {}),
            )
            .unwrap_err();
            let result = json!({"schema":1,"platform":host_api::platform(),
                "proof":"signed app -> app_tool -> host.request -> native OS permission status",
                "signed_install":true,"bundle_digest":self.launch.as_ref().unwrap().manifest.integrity.bundle_blake3,
                "tool_result":reply,"refusals":self.preliminary,"closed_app":closed,
                "host_sheet_visible":self.sheet.borrow().is_some_and(|s| s.view.visible),
                "not_verified":["model or peer relay consent", "physical OS permission approval", "camera capture", "Android runtime"]});
            use std::io::Write;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap()
                .write_all(&serde_json::to_vec_pretty(&result).unwrap())
                .unwrap();
            println!("HOST_API_LAB_COMPLETE");
        }
    }
}
