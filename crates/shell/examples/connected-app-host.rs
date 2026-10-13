//! Isolated development host with real connected services and ordinary bundle
//! admission or signed Store installation. No agent kernel or approval bypass.
//! --bundle=<bundle> or --installed-app=<id> or --install-bundle=<bundle>
//! --app-data=<isolated directory> --remote
//! A separate acceptance-fixtures build may add --provider-fixture=github|github-sign-in|calendar.
use makepad_widgets::*;
use std::{path::PathBuf, sync::Arc};
mod connected_support;
app_main!(App, font_set: International);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root { main_window := Window {
            window.inner_size: #(if std::env::args().any(|arg| arg == "--wide") {vec2(1200.0, 820.0)} else {vec2(430.0, 850.0)})
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
    assets: Option<octosense_app_policy::AssetServer>,
    #[rust]
    launch: Option<octosense_app_hub::PreparedLaunch>,
    #[rust]
    card: SplashRef,
    #[rust]
    sheet: SplashRef,
    #[rust]
    browser_capture: Option<PathBuf>,
    #[rust]
    last_browser_url: String,
    #[rust]
    webview_control: Option<PathBuf>,
    #[rust]
    last_webview_command: u64,
    #[rust]
    webview_timer: Option<Timer>,
}
impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        let arg =
            |name: &str| std::env::args().find_map(|a| a.strip_prefix(name).map(str::to_owned));
        let root = PathBuf::from(arg("--app-data=").expect("Pass --app-data=<isolated directory>"));
        let install = arg("--install-bundle=");
        let installed = arg("--installed-app=");
        let direct = arg("--bundle=");
        assert_eq!(
            [install.is_some(), installed.is_some(), direct.is_some()]
                .into_iter()
                .filter(|v| *v)
                .count(),
            1,
            "Pass exactly one of --bundle, --install-bundle or --installed-app"
        );
        let installed = install
            .map(|path| {
                let receipt = connected_support::install(&[PathBuf::from(path)], &root)
                    .expect("Install signed fixture");
                receipt["apps"][0]["id"].as_str().unwrap().to_string()
            })
            .or(installed);
        let (bundle, policy) = if let Some(id) = installed {
            // connected_support only serves isolated legacy test catalogs.
            std::env::set_var("OCTOSENSE_HUB_CATALOG", "legacy");
            let launch = connected_support::open(&root, &id).expect("Verify installed launch");
            let result = (launch.bundle().to_path_buf(), launch.policy.clone());
            println!(
                "CONNECTED_INSTALLED {}",
                serde_json::json!({"app":id,"bundle_digest":launch.manifest.integrity.bundle_blake3,
                "signed":true,"prepared_launch_verified":true})
            );
            self.launch = Some(launch);
            result
        } else {
            let bundle = PathBuf::from(direct.unwrap());
            let manifest =
                std::fs::read_to_string(bundle.join("manifest.json")).expect("Read manifest");
            let digest = octosense_app_policy::digest_dir(&bundle).expect("Digest bundle");
            let policy = octosense_app_policy::admit_and_resolve_dir(
                &manifest,
                &digest,
                &octosense_app_policy::HostLimits::default().with_require_signature(false),
                &octosense_app_policy::RefuseAllSignatures,
            )
            .expect("Admit unchanged ordinary bundle");
            (bundle, policy)
        };
        self.app = policy.app_id.clone();
        self.root = root.join(".host");
        if let Some(fixture) = arg("--provider-fixture=") {
            assert!(
                self.launch.is_some(),
                "Provider fixtures require signed installed mode"
            );
            #[cfg(feature = "acceptance-fixtures")]
            {
                std::fs::create_dir_all(&self.root).expect("Create isolated host root");
                let result = match fixture.as_str() {
                    "github" => {
                        octosense_oauth_service::acceptance_github::install(&self.root, &self.app)
                    }
                    "github-sign-in" => octosense_oauth_service::acceptance_github::install_sign_in(
                        &self.root, &self.app,
                    ),
                    "calendar" => {
                        octosense_oauth_service::acceptance_calendar::install(&self.root, &self.app)
                    }
                    _ => Err("Unknown compiled provider fixture".into()),
                }
                .expect("Register isolated synthetic provider");
                println!("CONNECTED_PROVIDER_FIXTURE {}", result);
            }
            #[cfg(not(feature = "acceptance-fixtures"))]
            panic!("Provider fixture {fixture} is unavailable in this build");
        }
        if let Some(registration) = arg("--backend-fixture=") {
            assert!(
                self.launch.is_some(),
                "Backend fixtures require signed installed mode"
            );
            #[cfg(feature = "acceptance-fixtures")]
            {
                std::fs::create_dir_all(&self.root).expect("Create isolated host root");
                let registration: octosense_oauth_service::backend::BackendRegistration =
                    serde_json::from_slice(
                        &std::fs::read(registration).expect("Read fixture registration"),
                    )
                    .expect("Parse fixture registration");
                assert_eq!(
                    registration.app_id, self.app,
                    "Fixture belongs to installed app"
                );
                let client = octosense_oauth_service::backend::BackendClient::new_loopback_fixture(
                    registration,
                )
                .expect("Validate local backend fixture");
                octosense_oauth_service::host::register_backend_fixture(&self.root, client)
                    .expect("Register real HTTP backend fixture");
            }
            #[cfg(not(feature = "acceptance-fixtures"))]
            panic!("Backend fixture {registration} is unavailable in this build");
        }
        if let Some(path) = arg("--capture-browser-url=") {
            #[cfg(feature = "acceptance-fixtures")]
            {
                // Example-only diagnostic handoff. The driver launches this exact
                // host URL in its own browser profile instead of opening the user's
                // default browser. No provider response or callback is synthesized.
                let path = PathBuf::from(path);
                assert!(
                    path.is_absolute(),
                    "Browser capture must use a private absolute path"
                );
                assert!(
                    !path.exists(),
                    "Browser capture must not replace an existing file"
                );
                self.browser_capture = Some(path);
            }
            #[cfg(not(feature = "acceptance-fixtures"))]
            panic!("Browser capture {path} is unavailable in this build");
        }
        if let Some(path) = arg("--webview-control=") {
            #[cfg(all(feature = "acceptance-fixtures", target_os = "macos"))]
            {
                assert!(
                    arg("--backend-fixture=").is_some(),
                    "WebView inspection requires the fictional backend fixture"
                );
                let path = PathBuf::from(path);
                assert!(
                    path.is_absolute() && !path.exists(),
                    "Pass a new private command file"
                );
                self.webview_control = Some(path);
                self.webview_timer = Some(cx.start_interval(0.1));
            }
            #[cfg(not(all(feature = "acceptance-fixtures", target_os = "macos")))]
            panic!("WebView fixture control {path} is unavailable in this build");
        }
        let scopes = policy.capabilities.clone();
        let caller = self.app.clone();
        octosense_oauth_service::host::register(Arc::new(move |app, provider, requested| {
            use octosense_oauth_service::Provider;
            app == caller
                && scopes.contains("auth")
                && requested.iter().all(|scope| match provider {
                    Provider::Backend => requested.len() == 1 && scope == "app.session",
                    Provider::Github => scope == "read:user" || scopes.contains("github"),
                    Provider::Google => match scope.as_str() {
                        "openid" | "email" | "profile" => true,
                        s if s.starts_with("https://www.googleapis.com/auth/calendar.") => {
                            scopes.contains("gcalendar")
                        }
                        s if s.starts_with("https://www.googleapis.com/auth/gmail.") => {
                            scopes.contains("gmail")
                        }
                        _ => false,
                    },
                })
        }));
        // The sheet names the app as the shell does: from its admitted manifest.
        let name = std::fs::read(bundle.join("manifest.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|manifest| manifest["name"].as_str().map(str::to_owned));
        let named = self.app.clone();
        octosense_oauth_service::host::set_app_names(Arc::new(move |app| {
            (app == named).then(|| name.clone()).flatten()
        }));
        octosense_oauth_service::host_api::register_with_review_hook(
            octosense_shell::connected_review::connector_sheet,
        );
        octosense_oauth_service::host_inbox::register_with_review_hook(
            octosense_shell::connected_review::sheet,
        );
        let mut settings = policy.isolate_settings(&root);
        std::fs::create_dir_all(&settings.jail_root).expect("Create app jail");
        let assets = octosense_app_policy::AssetServer::start_with_static(&bundle, &[])
            .expect("Serve bundle artwork");
        settings.hosts.push(assets.allowlist_entry());
        let source = octosense_app_policy::script_source(&bundle, assets.origin())
            .expect("Script bundle")
            .expect("Read source");
        self.assets = Some(assets);
        // Resolve host children before any untrusted app source can add IDs.
        self.card = self.ui.splash(cx, ids!(app));
        self.sheet = self.ui.splash(cx, ids!(sheet));
        let app = self.card.clone();
        octosense_app_policy::splash_adapter::apply(&app, cx, &settings);
        app.set_host_tag(cx, Some(self.app.clone()));
        octosense_appstore::apply_device_consent(cx, &bundle, &app)
            .expect("Apply the admitted app's device consent boundary");
        app.set_text(cx, &source);
    }
}
impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::theme_mod(vm);
        script_eval!(vm, {mod.theme = mod.themes.light});
        makepad_widgets::widgets_mod(vm);
        octosense_shell::charts::register(vm);
        widget_async::set_splash_theme(widget_async::SplashTheme::Light);
        octosense_markdown_editor::register();
        octosense_oauth_service::sign_in_code::register();
        octosense_shell::connected_review::register();
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        let app = self.card.clone();
        let sheet = self.sheet.clone();
        // Match CardAppView's modal routing: a host sheet owns input while
        // visible; the underlying app still receives non-input lifecycle work.
        let sheet_up = sheet.borrow().map(|s| s.view.visible).unwrap_or(false);
        if sheet_up && octosense_appstore::services::is_sheet_input_event(event) {
            sheet.handle_event(cx, event, &mut Scope::empty());
        } else {
            self.ui.handle_event(cx, event, &mut Scope::empty());
        }
        octosense_appstore::services::pump(cx, &self.app, &self.root, &app, &sheet);
        #[cfg(all(feature = "acceptance-fixtures", target_os = "macos"))]
        if let Some(path) = &self.webview_control {
            // Only this explicit fictional fixture may inspect native auth DOM.
            // No command path exists in production shells or app script APIs.
            let mut browser = None;
            sheet.children(&mut |_, child| auth_fixture_reader(&child, &mut browser));
            if let Some(browser) = browser {
                use std::io::Read;
                let command = std::fs::File::open(path).ok().and_then(|file| {
                    let mut bytes = Vec::new();
                    file.take(65_537).read_to_end(&mut bytes).ok()?;
                    (bytes.len() <= 65_536)
                        .then(|| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                        .flatten()
                });
                if let Some(command) = command {
                    if let Some(id) = command["id"]
                        .as_u64()
                        .filter(|id| *id > self.last_webview_command)
                    {
                        if let Some(result) = command["result_path"]
                            .as_str()
                            .filter(|p| PathBuf::from(p).is_absolute())
                        {
                            self.last_webview_command = id;
                            match command["op"].as_str() {
                                Some("eval") => {
                                    if let Some(script) =
                                        command["script"].as_str().filter(|s| s.len() <= 32_768)
                                    {
                                        cx.system_browser(browser)
                                            .evaluate_auth(script.to_owned(), result.to_owned());
                                    }
                                }
                                Some("inspect") => {
                                    let image = command["snapshot_path"]
                                        .as_str()
                                        .filter(|p| PathBuf::from(p).is_absolute())
                                        .map(str::to_owned);
                                    cx.system_browser(browser).inspect(
                                        result.to_owned(),
                                        image,
                                        None,
                                    );
                                }
                                _ => (),
                            }
                        }
                    }
                }
            }
        }
        #[cfg(feature = "acceptance-fixtures")]
        if let Some(path) = &self.browser_capture {
            if sheet.borrow().is_some_and(|s| s.view.visible) {
                let mut urls = Vec::new();
                sheet.children(&mut |_, child| collect_browser_urls(&child, &mut urls));
                if let Some(url) = urls
                    .into_iter()
                    .find(|url| !url.is_empty() && url != &self.last_browser_url)
                {
                    use std::io::Write;
                    let mut options = std::fs::OpenOptions::new();
                    options.write(true).create_new(true);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::OpenOptionsExt;
                        options.mode(0o600);
                    }
                    // Each launch captures only one attempt. Refuse existing or
                    // symlink destinations; callers restart with a new private path.
                    let mut file = options.open(path).expect("Create private browser handoff");
                    file.write_all(url.as_bytes())
                        .expect("Write private browser handoff");
                    self.last_browser_url = url;
                    self.browser_capture = None;
                }
            }
        }
    }
}

#[cfg(feature = "acceptance-fixtures")]
fn collect_browser_urls(widget: &WidgetRef, urls: &mut Vec<String>) {
    if let Some(link) = widget.borrow::<LinkLabel>() {
        if !link.url.is_empty() {
            urls.push(link.url.clone());
        }
    }
    widget.children(&mut |_, child| collect_browser_urls(&child, urls));
}

#[cfg(all(feature = "acceptance-fixtures", target_os = "macos"))]
fn auth_fixture_reader(widget: &WidgetRef, browser: &mut Option<SystemBrowserId>) {
    if let Some(mut reader) = widget.borrow_mut::<makepad_widgets::web_reader::WebReader>() {
        reader.enable_auth_inspection();
        if let Some(id) = reader.auth_browser_id() {
            *browser = Some(id);
        }
    }
    widget.children(&mut |_, child| auth_fixture_reader(&child, browser));
}
