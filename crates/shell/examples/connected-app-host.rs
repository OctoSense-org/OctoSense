//! Isolated development host with real connected services and ordinary bundle
//! admission. No agent kernel, catalog installation or synthetic approval.
//! --bundle=<bundle> --app-data=<isolated directory> --remote
use makepad_widgets::*;
use std::{path::PathBuf, sync::Arc};
app_main!(App, font_set: International);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root { main_window := Window {
            window.inner_size: vec2(430, 850)
            body +: {flow: Overlay
                app := Splash {width: Fill height: Fill}
                sheet := Splash {width: Fill height: Fill visible: false}
            }
        }}
    }
}
#[derive(Script, ScriptHook)]
struct App {
    #[live] ui: WidgetRef,
    #[rust] app: String,
    #[rust] root: PathBuf,
    #[rust] assets: Option<octosense_app_policy::AssetServer>,
}
impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        let arg = |name: &str| std::env::args().find_map(|a| a.strip_prefix(name).map(str::to_owned));
        let bundle = PathBuf::from(arg("--bundle=").expect("Pass --bundle=<bundle>"));
        let root = PathBuf::from(arg("--app-data=").expect("Pass --app-data=<isolated directory>"));
        let manifest = std::fs::read_to_string(bundle.join("manifest.json")).expect("Read manifest");
        let digest = octosense_app_policy::digest_dir(&bundle).expect("Digest bundle");
        let policy = octosense_app_policy::admit_and_resolve_dir(&manifest, &digest,
            &octosense_app_policy::HostLimits::default().with_require_signature(false),
            &octosense_app_policy::RefuseAllSignatures).expect("Admit unchanged ordinary bundle");
        self.app = policy.app_id.clone();
        self.root = root.join(".host");
        let scopes = policy.capabilities.clone();
        let caller = self.app.clone();
        octosense_oauth_service::host::register(Arc::new(move |app, provider, requested| {
            use octosense_oauth_service::Provider;
            app == caller && scopes.contains("auth") && requested.iter().all(|scope| match provider {
                Provider::Github => scope == "read:user" || scopes.contains("github"),
                Provider::Google => match scope.as_str() {
                    "openid" | "email" | "profile" => true,
                    s if s.starts_with("https://www.googleapis.com/auth/calendar.") => scopes.contains("gcalendar"),
                    s if s.starts_with("https://www.googleapis.com/auth/gmail.") => scopes.contains("gmail"),
                    _ => false,
                },
            })
        }));
        octosense_oauth_service::host_api::register();
        octosense_oauth_service::host_inbox::register_with_review_hook(octosense_shell::connected_review::sheet);
        let mut settings = policy.isolate_settings(&root);
        std::fs::create_dir_all(&settings.jail_root).expect("Create app jail");
        let assets = octosense_app_policy::AssetServer::start_with_static(&bundle, &[]).expect("Serve bundle artwork");
        settings.hosts.push(assets.allowlist_entry());
        let source = octosense_app_policy::script_source(&bundle, assets.origin()).expect("Script bundle").expect("Read source");
        self.assets = Some(assets);
        let app = self.ui.splash(cx, ids!(app));
        octosense_app_policy::splash_adapter::apply(&app, cx, &settings);
        app.set_text(cx, &source);
    }
}
impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::theme_mod(vm);
        script_eval!(vm, {mod.theme = mod.themes.light});
        makepad_widgets::widgets_mod(vm);
        widget_async::set_splash_theme(widget_async::SplashTheme::Light);
        octosense_markdown_editor::register();
        octosense_shell::connected_review::register();
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        let app = self.ui.splash(cx, ids!(app));
        let sheet = self.ui.splash(cx, ids!(sheet));
        // Match CardAppView's modal routing: a host sheet owns input while
        // visible; the underlying app still receives non-input lifecycle work.
        let sheet_up = sheet.borrow().map(|s| s.view.visible).unwrap_or(false);
        if sheet_up && event.requires_visibility() {
            sheet.handle_event(cx, event, &mut Scope::empty());
        } else {
            self.ui.handle_event(cx, event, &mut Scope::empty());
        }
        octosense_appstore::services::pump(cx, &self.app, &self.root, &app, &sheet);
    }
}
