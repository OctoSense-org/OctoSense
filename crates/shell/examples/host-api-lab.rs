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
        #[cfg(not(target_os = "android"))]
        let arg =
            |name: &str| std::env::args().find_map(|a| a.strip_prefix(name).map(str::to_owned));
        #[cfg(not(target_os = "android"))]
        let root =
            PathBuf::from(arg("--app-data=").expect("Pass an isolated --app-data directory"));
        #[cfg(not(target_os = "android"))]
        let bundle = PathBuf::from(arg("--bundle=").expect("Pass --bundle=<Host API Lab bundle>"));
        #[cfg(target_os = "android")]
        let data = PathBuf::from(cx.get_data_dir().expect("Android private data directory"))
            .join("host-api-lab");
        #[cfg(target_os = "android")]
        let (root, bundle) = (data.join("apps"), data.join("bundle"));
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
            // This example creates an ephemeral legacy catalog, not the public Hub.
            std::env::set_var("OCTOSENSE_HUB_CATALOG", "legacy");
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
        #[cfg(not(target_os = "android"))]
        let receipt = PathBuf::from(arg("--receipt=").expect("Pass a new --receipt file"));
        #[cfg(target_os = "android")]
        let receipt = data.join("receipt.json");
        self.receipt = Some(receipt);
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
        octosense_shell::files_service::register();
        octosense_shell::audio_service::register();
        // A synthetic transport plus fresh profiles make accidental delivery
        // impossible. Calendar acceptance checks status/refusal only.
        octosense_mail_service::register_demo();
        octosense_shell::device_calendar::register(|app| {
            (app == "org.octosense.samples.apilab").then(|| "device".to_owned())
        });
        host_api::register_runtime_feature("storage.binary_write", 1);
        host_api::register_runtime_feature("video.playback_controls", 1);
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
        octosense_shell::files_service::handle_event(cx, event);
        octosense_shell::audio_service::handle_event(cx, event);
        octosense_shell::device_calendar::handle_event(cx, event);
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
            let mut checks = os_batch_checks(&reply);
            checks
                .as_object_mut()
                .unwrap()
                .extend(public_service_checks(&reply));
            let result = json!({"schema":1,"platform":host_api::platform(),"checks":checks,
                "proof":"signed app -> app_tool -> host.request -> native OS permission status",
                "signed_install":true,"bundle_digest":self.launch.as_ref().unwrap().manifest.integrity.bundle_blake3,
                "tool_result":reply,"refusals":self.preliminary,"closed_app":closed,
                "host_sheet_visible":self.sheet.borrow().is_some_and(|s| s.view.visible),
                "not_verified":["model or peer relay consent", "physical OS permission approval", "camera capture", "interactive file selection/export", "live location sampling", "Calendar event reads/writes", "SMTP delivery", "native photo/share choosers", "live audio recording/playback", "Video playback (separate fixture)"]});
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

/// Evaluate values returned by the signed app's live VM, never substitute a
/// native filesystem write for the contained fs.write_bytes/read_bytes calls.
fn os_batch_checks(reply: &Result<Value, String>) -> Value {
    let batch = reply
        .as_ref()
        .ok()
        .and_then(|value| value.get("os_batch"))
        .cloned()
        .unwrap_or(Value::Null);
    let described = |key: &str, name: &str| {
        batch[key]["implemented"] == true
            && batch[key]["supported"] == true
            && batch[key]["descriptor"]["name"] == name
    };
    let denied = |allowed: &str, error: &str| {
        batch[allowed] == false
            && batch[error]
                .as_str()
                .is_some_and(|error| error.contains("background"))
    };
    json!({
        "files_status_discovery": described("files_status_discovery", "files.status"),
        "files_import_discovery": described("files_import_discovery", "files.import"),
        "files_export_discovery": described("files_export_discovery", "files.export"),
        "storage_binary_write_discovery": batch["storage_discovery"]["implemented"] == true
            && batch["storage_discovery"]["supported"] == true
            && batch["storage_discovery"]["method"] == "storage.binary_write"
            && batch["storage_discovery"]["version"] == 1
            && batch["storage_discovery"]["kind"] == "runtime-abi"
            && batch["storage_discovery"]["callable_via_host_request"] == false,
        "contained_binary_roundtrip": batch["binary"]["length"] == 4
            && batch["binary"]["values"] == json!([0,127,128,255]),
        "files_status_truthful": batch["files_status"]["import_supported"] == true
            && batch["files_status"]["export_supported"] == true
            && batch["files_status"]["storage_granted"] == true
            && batch["files_status"]["foreground_required"] == true
            && batch["files_status"]["max_file_bytes"] == makepad_widgets::splash_storage::MAX_FILE_BYTES,
        "location_sample_discovery": described("location_discovery", "location.sample")
            && batch["location_discovery"]["descriptor"]["agent_access"] == "foreground-only",
        "background_import_refused": denied("background_import_allowed", "background_import_error"),
        "background_export_refused": denied("background_export_allowed", "background_export_error"),
        // Refused before any OS call. Since makepad#118 (OctoSense #450) the
        // runtime no longer refuses on the app's grants: App Hub's dispatcher
        // refuses a foreground-only method from a background surface, and the
        // device service checks the person's consent behind it.
        "location_without_consent_refused": batch["background_location_allowed"] == false
            && batch["background_location_error"] == "location.sample is unavailable to agents/background surfaces",
    })
}

/// Public methods must be discoverable without granting access to a calendar
/// or a mailbox. Values below originate in the signed app's real VM callbacks.
fn public_service_checks(reply: &Result<Value, String>) -> serde_json::Map<String, Value> {
    let p = reply
        .as_ref()
        .ok()
        .and_then(|v| v.get("public_services"))
        .cloned()
        .unwrap_or(Value::Null);
    let described = |key: &str, method: &str, access: &str| {
        p[key]["ok"] == true
            && p[key]["data"]["implemented"] == true
            && p[key]["data"]["supported"] == true
            && p[key]["data"]["descriptor"]["name"] == method
            && p[key]["data"]["descriptor"]["agent_access"] == access
    };
    let refused = |key: &str, contains: &str| {
        p[key]["ok"] == false
            && p[key]["error"]
                .as_str()
                .is_some_and(|e| e.contains(contains))
    };
    json!({
        "public_calendar_read_discovery": described("calendar_discovery", "device_calendar.events.list", "allowed"),
        "public_calendar_write_discovery": described("calendar_write_discovery", "device_calendar.events.create", "foreground-only"),
        "public_mail_compose_discovery": described("mail_discovery", "mail.compose", "allowed"),
        "public_mail_send_discovery": described("mail_send_discovery", "mail.send", "foreground-only"),
        "calendar_status_without_data_access": p["calendar_status"]["ok"] == true && p["calendar_status"]["data"]["supported"] == true && p["calendar_status"]["data"]["app_consent"] == false,
        "calendar_choices_without_consent_refused": refused("calendar_choices", "authorization_required"),
        "background_calendar_permission_refused": refused("calendar_permission", "background"),
        "background_calendar_selection_refused": refused("calendar_select", "background"),
        "background_calendar_write_refused": refused("calendar_create", "background"),
        "mail_compose_without_account_refused": refused("mail_compose", "account"),
        "background_mail_send_refused": refused("mail_send", "background"),
        "photo_picker_discovery": described("photo_discovery", "files.pick_photo", "foreground-only"),
        "text_share_discovery_truthful": if cfg!(target_os = "android") {
            described("share_discovery", "files.share", "foreground-only")
        } else {
            p["share_discovery"]["ok"] == true && p["share_discovery"]["data"]["implemented"] == false
        },
        "background_photo_picker_refused": refused("photo_pick", "background"),
        "background_text_share_refused": refused("text_share", if cfg!(target_os = "android") {"background"} else {"foreground_required"}),
        "audio_playback_discovery": described("audio_discovery", "audio.play", "foreground-only"),
        "video_controls_discovery": p["video_discovery"]["ok"] == true
            && p["video_discovery"]["data"]["implemented"] == true
            && p["video_discovery"]["data"]["supported"] == true
            && p["video_discovery"]["data"]["kind"] == "runtime-abi"
            && p["video_discovery"]["data"]["method"] == "video.playback_controls"
            && p["video_discovery"]["data"]["callable_via_host_request"] == false
            && p["video_discovery"]["data"]["version"] == 1,
        "microphone_recording_discovery": described("record_discovery", "microphone.record_start", "foreground-only"),
        "background_audio_playback_refused": refused("audio_play", "background"),
        // A readable permission status never authorizes recording. The tool's
        // callback retains background provenance even without a declaration.
        "background_microphone_recording_refused": p["record_start"]["ok"] == false
            && p["record_start"]["error"].as_str()
                == Some("microphone.record_start is unavailable to agents/background surfaces")
    }).as_object().unwrap().clone()
}
