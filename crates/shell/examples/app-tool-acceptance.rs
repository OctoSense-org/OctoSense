//! Hidden native runner for an explicitly supplied, local app-tool fixture.
//! Uses ephemeral signed admission and the real dispatcher, without a model.
//! The test caller enters after the production consent/peer relay: no consent claim.
use makepad_app_module::AppModule;
use makepad_widgets::*;
use octosense_app_hub::{check_bundle, sign_manifest, HubKey, PublisherKeys};
use octosense_app_policy::{AppManifest, HostLimits};
use octosense_appstore::{host_api, script_tools, services};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
app_main!(App, font_set: International);
script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root { main_window := Window {
            window.inner_size: vec2(412, 740)
            body +: {flow: Overlay
                app := Splash {width: Fill height: Fill}
                sheet := Splash {width: Fill height: Fill visible: false}
            }
        }}
    }
}
fn arg(name: &str) -> String {
    std::env::args()
        .find_map(|a| a.strip_prefix(name).map(str::to_owned))
        .unwrap_or_else(|| panic!("Missing {name}"))
}
fn copy_bundle(from: &Path, to: &Path) {
    assert!(
        std::fs::symlink_metadata(from).unwrap().is_dir(),
        "Bundle must be a directory"
    );
    std::fs::create_dir(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            copy_bundle(&entry.path(), &to.join(entry.file_name()));
        } else if kind.is_file() {
            std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        } else {
            panic!("Fixture refuses symlinks and special files");
        }
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
    bundle_digest: String,
    #[rust]
    receipt: PathBuf,
    #[rust]
    trigger: PathBuf,
    #[rust]
    tool: String,
    #[rust]
    args: Value,
    #[rust]
    invalid: Value,
    #[rust]
    expected: Value,
    #[rust]
    registration: Option<script_tools::Registration>,
    #[rust]
    assets: Option<octosense_app_policy::AssetServer>,
    #[rust]
    card: SplashRef,
    #[rust]
    sheet: SplashRef,
    #[rust]
    replies: Arc<Mutex<Vec<Result<Value, String>>>>,
    #[rust]
    refusals: Value,
    #[rust]
    triggered: bool,
    #[rust]
    finished: bool,
    #[rust]
    timer: Option<Timer>,
}
impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        let source = PathBuf::from(arg("--bundle="));
        self.root = PathBuf::from(arg("--app-data="));
        self.receipt = PathBuf::from(arg("--receipt="));
        self.trigger = PathBuf::from(arg("--trigger-file="));
        assert!(
            !self.receipt.exists() && !self.trigger.exists(),
            "Receipt and trigger must be new"
        );
        let preview = std::env::args().any(|a| a == "--preview");
        if !preview {
            self.tool = arg("--tool=");
            self.args = serde_json::from_str(&arg("--args=")).unwrap();
            self.invalid = serde_json::from_str(&arg("--invalid-args=")).unwrap();
            self.expected = serde_json::from_str(&arg("--expected=")).unwrap();
        }
        let original = std::fs::read_to_string(source.join("manifest.json")).unwrap();
        let mut manifest = AppManifest::parse(&original).unwrap();
        self.app = manifest.id.clone();
        self.bundle_digest = octosense_app_policy::digest_dir(&source).unwrap();
        assert_eq!(
            manifest.integrity.bundle_blake3, self.bundle_digest,
            "Stamp source first"
        );
        assert!(!self.app.starts_with("os."), "Only ordinary app fixtures");
        let marker = self.root.join(".app-tool-acceptance.json");
        if marker.exists() {
            assert!(!std::fs::symlink_metadata(&self.root)
                .unwrap()
                .file_type()
                .is_symlink());
            let saved: Value = serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
            assert_eq!(
                saved,
                json!({"app":self.app,"bundle_digest":self.bundle_digest})
            );
        } else {
            if self.root.exists() {
                assert!(
                    std::fs::read_dir(&self.root).unwrap().next().is_none(),
                    "Use a new isolated profile"
                );
            } else {
                std::fs::create_dir_all(&self.root).unwrap();
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o700))
                    .unwrap();
            }
            std::fs::write(
                &marker,
                serde_json::to_vec(&json!({"app":self.app,"bundle_digest":self.bundle_digest}))
                    .unwrap(),
            )
            .unwrap();
        }
        let bundle = self
            .root
            .join(format!("signed-bundle-{}", std::process::id()));
        copy_bundle(&source, &bundle);
        let publisher = HubKey::generate();
        let keys = PublisherKeys::new().with("app-tool-acceptance", &publisher.public_hex());
        sign_manifest(&publisher, &mut manifest, "app-tool-acceptance").unwrap();
        std::fs::write(
            bundle.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let policy = if preview {
            // Bootstrap real listing pixels; preview is never signed gate acceptance.
            octosense_app_policy::admit_and_resolve_dir(
                &serde_json::to_string(&manifest).unwrap(),
                &self.bundle_digest,
                &HostLimits::default(),
                &keys,
            )
            .unwrap()
        } else {
            let report = check_bundle(&bundle, &HostLimits::default(), &keys, None).unwrap();
            assert!(report.passed(), "{}", report.render());
            report.policy.expect("Signed admitted policy")
        };
        host_api::check_manifest(&manifest).unwrap();
        octosense_appstore::set_data_root(self.root.join("apps"));
        let mut settings = policy.isolate_settings(&self.root.join("apps"));
        std::fs::create_dir_all(&settings.jail_root).unwrap();
        let assets = octosense_app_policy::AssetServer::start_with_static(&bundle, &[]).unwrap();
        settings.hosts.push(assets.allowlist_entry());
        self.card = self.ui.splash(cx, ids!(app));
        self.sheet = self.ui.splash(cx, ids!(sheet));
        octosense_app_policy::splash_adapter::apply(&self.card, cx, &settings);
        let source_text = octosense_app_policy::script_source(&bundle, assets.origin())
            .unwrap()
            .unwrap();
        self.card.set_text(cx, &source_text);
        self.assets = Some(assets);
        self.registration = script_tools::bind(cx, &self.app, &bundle, &self.card).unwrap();
        assert!(
            preview || self.registration.is_some(),
            "Bundle must declare app tools"
        );
        self.triggered = preview; // Preview renders only; it never dispatches a tool.
        self.timer = Some(cx.start_interval(0.05));
        println!("APP_TOOL_ACCEPTANCE_READY {}", self.app);
    }
}
impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::theme_mod(vm);
        script_eval!(vm, {mod.theme = mod.themes.light});
        makepad_widgets::widgets_mod(vm);
        widget_async::set_splash_theme(widget_async::SplashTheme::Light);
        octosense_appstore::cardapp::CARD_MODULE.register(vm);
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        let may_prompt = !script_tools::pending_for(cx, &self.app, &self.card);
        self.card.set_host_prompts(cx, may_prompt);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        if !self.triggered && self.trigger.is_file() {
            self.triggered = true;
            let submit = |tool, args, account| {
                script_tools::submit(
                    &self.app,
                    tool,
                    args,
                    account,
                    "native-acceptance",
                    Duration::from_secs(10),
                    Box::new(|_| {}),
                )
            };
            let wrong_account = submit(&self.tool, self.args.clone(), "other-account").unwrap_err();
            let undeclared = submit("fixture.not_declared", json!({}), "device").unwrap_err();
            let invalid = submit(&self.tool, self.invalid.clone(), "device").unwrap_err();
            self.refusals = json!({"wrong_account":wrong_account,"undeclared_tool":undeclared,"invalid_arguments":invalid});
            let replies = self.replies.clone();
            script_tools::submit(
                &self.app,
                &self.tool,
                self.args.clone(),
                "device",
                "native-acceptance",
                Duration::from_secs(10),
                Box::new(move |r| replies.lock().unwrap().push(r)),
            )
            .unwrap();
        }
        script_tools::pump(cx, &self.app, &self.card);
        services::pump(
            cx,
            &self.app,
            &self.root.join("host"),
            &self.card,
            &self.sheet,
        );
        let reply = self.replies.lock().unwrap().pop();
        if let Some(reply) = reply {
            assert!(!self.finished);
            self.finished = true;
            self.registration.take();
            let closed = script_tools::submit(
                &self.app,
                &self.tool,
                self.args.clone(),
                "device",
                "native-acceptance",
                Duration::from_secs(1),
                Box::new(|_| {}),
            )
            .unwrap_err();
            let passed = reply.as_ref().is_ok_and(|actual| actual == &self.expected)
                && self.refusals["wrong_account"]
                    .as_str()
                    .unwrap()
                    .contains("account_scope")
                && self.refusals["undeclared_tool"]
                    .as_str()
                    .unwrap()
                    .contains("tool_not_declared")
                && self.refusals["invalid_arguments"]
                    .as_str()
                    .unwrap()
                    .contains("invalid_arguments")
                && closed.contains("app_not_running");
            let result = json!({"schema":1,"result":if passed {"pass"} else {"failed"},"app":self.app,
                "bundle_digest":self.bundle_digest,"signed_admission":true,"tool":self.tool,"args":self.args,
                "expected":self.expected,"actual":reply,"refusals":self.refusals,"closed_app":closed,
                "not_verified":["model reasoning","peer relay and human agent consent","public catalog installation","device/platform approval"]});
            std::fs::write(&self.receipt, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
            println!("APP_TOOL_ACCEPTANCE_RESULT {result}");
        }
    }
}
