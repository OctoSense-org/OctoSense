//! The OctoSense app store (ADR 0003 §7).
//!
//! Three screens: a searchable list of what the hub offers, a detail screen
//! that says in plain words what an app will be allowed to do BEFORE it is
//! installed, and the app itself running in its own contained isolate.
//!
//! What the store is careful about:
//!
//! - it verifies the catalog against the anchor before showing anything;
//! - it hashes a staged bundle before it reaches a jail;
//! - it shows permissions derived from the resolved policy, never from the
//!   app's own description;
//! - it refuses to open an app whose version has been withdrawn, even when
//!   that app is already installed.
use makepad_app_module::{
    makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult},
    AppModule, ExecOutcome, InstanceHandles, InstanceParts, OpenArgKind, OpenSchema, ServiceExecutor, ValidatedOpen,
};
use makepad_widgets::*;
use octosense_app_hub::{Availability, Listing, Store};
use octosense_app_policy::HostLimits;
use std::path::PathBuf;

pub mod cardapp;
pub mod card_assets;
pub mod components;
pub mod host_api;
pub mod services;
pub mod script_tools;
mod script_tools_json;
mod tool_schema;
pub mod source;
pub mod system;
pub mod ui;
#[cfg(test)]
mod card_layout_tests;

pub use makepad_widgets;

use std::sync::{Mutex, OnceLock};

/// Where installed apps live. The shell sets it from the platform's data
/// directory at startup so the store, the card host and the shell's own
/// launcher catalog all read the same place; `OCTOSENSE_APP_DATA` overrides.
static DATA_ROOT: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

pub fn set_data_root(root: PathBuf) {
    *DATA_ROOT.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(root);
}

/// The apps root: `$OCTOSENSE_APP_DATA`, else what the host set, else the
/// platform data directory, else a temp directory (tests and previews).
pub fn data_root(cx: &Cx) -> PathBuf {
    if let Some(root) = std::env::var("OCTOSENSE_APP_DATA").ok().filter(|v| !v.is_empty()) {
        return PathBuf::from(root);
    }
    if let Some(root) = DATA_ROOT.get().and_then(|m| m.lock().unwrap().clone()) {
        return root;
    }
    cx.get_data_dir()
        .map(|dir| PathBuf::from(dir).join("apps"))
        .unwrap_or_else(|| std::env::temp_dir().join("octosense-apps"))
}

/// The apps root without a `Cx`, for a host building its launcher catalog.
/// None until the host has set it (or the environment names it).
pub fn data_root_if_set() -> Option<PathBuf> {
    std::env::var("OCTOSENSE_APP_DATA")
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| DATA_ROOT.get().and_then(|m| m.lock().unwrap().clone()))
}

/// An installed card app, as a launcher needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub version: String,
}

/// Every app installed under `root`, read from each bundle's own manifest.
pub fn installed_apps(root: &std::path::Path) -> Vec<InstalledApp> {
    let mut out = Vec::new();
    // Installs from before `.bundles` move out of their apps' storage here,
    // where every shell first looks for them.
    if let Err(e) = octosense_app_hub::adopt_legacy_installs(root) {
        error!("store: {e}");
    }
    let Ok(entries) = std::fs::read_dir(root.join(octosense_app_hub::INSTALLS_DIR)) else { return out };
    for entry in entries.flatten() {
        let manifest = entry.path().join("bundle").join(octosense_app_policy::MANIFEST_FILE);
        let Ok(json) = std::fs::read_to_string(&manifest) else { continue };
        if let Ok(m) = octosense_app_policy::AppManifest::parse(&json) {
            out.push(InstalledApp { id: m.id, name: m.name, version: m.version });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// What the store asks its host to do. A host that links the store handles
/// these; the store never launches an app itself, because an app is a
/// client of the window manager, not a screen inside the store.
#[derive(Clone, Debug, Default)]
pub enum AppStoreAction {
    /// Open this installed app as an app of its own.
    Launch(String),
    /// An app was installed or removed: the launcher's catalog changed.
    CatalogChanged,
    #[default]
    None,
}
pub use octoscript_widgets;

/// The hub anchor this build trusts (ADR 0003 §4). Shipped in the binary;
/// the hub's working key is certified by it, so rotating that key needs no
/// release. `OCTOSENSE_HUB_ANCHOR` overrides it for development only. The
/// `hub` tool checks catalog history against the same constant.
pub use octosense_app_hub::DEFAULT_ANCHOR;

/// The hub this build reads by default: the OctoSense organisation's hub,
/// served from its repository. `OCTOSENSE_HUB` overrides it (a mirror
/// directory or another base URL).
pub const DEFAULT_HUB: &str = "https://raw.githubusercontent.com/OctoSense-org/OctoSense-App-Hub/main/";

script_mod! {
    use mod.prelude.widgets.*

    mod.widgets.AppStoreView = set_type_default() do #(AppStoreView::register_widget(vm)) {
        width: Fill height: Fill flow: Down
        show_bg: true draw_bg.color: #fff
        scroll_bars: mod.widgets.ScrollBars { show_scroll_x: false, show_scroll_y: true }
        header := View {
            width: Fill height: Fit flow: Down padding: Inset{left: 16., right: 16., top: 14., bottom: 6.} spacing: 10
            Label { text: "Apps" draw_text.color: #000 draw_text.text_style.font_size: 30 }
            search := TextInput {
                width: Fill height: Fit
                empty_text: "Games, Apps, Publishers"
                draw_bg.color: #eeeef2 draw_bg.radius: 10 draw_bg.border_width: 0
                draw_text.color: #000
            }
            origin_label := Label { text: "" draw_text.color: #8e8e93 draw_text.text_style.font_size: 10 }
        }
        // One container per screen state: the store swaps what is IN it
        // rather than toggling visibility, so there is never a hidden screen
        // drawing behind a visible one.
        screen := Splash { width: Fill height: Fit }
        // The running app. Empty body means no app, and it takes no space.
        card := Splash { width: Fill height: Fill }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Screen {
    List,
    Detail(usize),
    Running(String),
}

#[derive(Script, ScriptHook, Widget)]
pub struct AppStoreView {
    #[deref]
    view: View,
    #[rust]
    started: bool,
    #[rust]
    query: String,
    #[rust]
    listings: Vec<Listing>,
    #[rust(Screen::List)]
    screen: Screen,
    #[rust]
    store: Option<Store>,
    #[rust]
    channel: Option<source::CatalogChannel>,
    #[rust]
    origin: Option<source::Origin>,
    #[rust]
    app_data_root: PathBuf,
    #[rust]
    status: String,
    /// Serves the running app its own artwork, and dies with it.
    #[rust]
    asset_server: Option<octosense_app_policy::AssetServer>,
    #[rust]
    layout_logged: bool,
}

impl AppStoreView {
    /// Load the catalog and verify it. Everything the store shows comes from
    /// a catalog that passed this; a failure leaves the list empty and says
    /// why, rather than showing unverified apps.
    fn start(&mut self, cx: &mut Cx) {
        // Installed apps live under the platform's data directory (the
        // app's files dir on Android), one jail per app; the environment
        // overrides it for development.
        self.app_data_root = data_root(cx);
        let anchor = std::env::var("OCTOSENSE_HUB_ANCHOR").unwrap_or_else(|_| DEFAULT_ANCHOR.to_string());
        let channel = match source::CatalogChannel::from_environment(&self.app_data_root) {
            Ok(channel) => channel,
            Err(error) => {
                self.status = error;
                self.refresh(cx);
                return;
            }
        };
        let mut store = channel.configure(Store::new(&anchor, &self.app_data_root, HostLimits::default()).with_host_api_versions(crate::host_api::available_versions()));
        self.channel = Some(channel);

        // A hub override on disk (`<data dir>/hub.txt`, a path or a base URL)
        // wins over the built-in hub: how a device with no route to the
        // public hub reads a mirror, and how a test phone reads one over USB.
        let override_path = cx.get_data_dir().map(|dir| PathBuf::from(dir).join("hub.txt"));
        let override_value = override_path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());
        self.origin = match override_value {
            Some(value) => Some(source::Origin::parse(&value)),
            None => source::Origin::from_env(),
        };
        match &self.origin {
            None => self.status = "No hub configured. Set OCTOSENSE_HUB to a hub mirror.".into(),
            Some(origin) => match origin.catalog_for(channel).and_then(|json| {
                std::fs::create_dir_all(&self.app_data_root).map_err(|e| e.to_string())?;
                store.accept_catalog_and_cache(&json, &self.app_data_root.join(channel.filename()))
            }) {
                Ok(()) => {
                    // Persist exactly the verified bytes, including v2 proof,
                    // for launches later. Never fetch a second unverified copy.
                    let count = store.catalog().map(|c| c.entries.len()).unwrap_or(0);
                    let today = octosense_app_hub::today();
                    self.status = match store.installs_allowed(&today) {
                        Ok(()) => format!(
                            "{count} app(s) from {} · catalog {} day(s) old",
                            origin.describe(),
                            store.catalog_age_days(&today).unwrap_or(0)
                        ),
                        Err(reason) => format!("{count} app(s) · {reason}"),
                    };
                }
                Err(e) => {
                    // The status line truncates on a phone; the log holds
                    // the whole reason.
                    error!("appstore: catalog from {} refused: {e}", origin.describe());
                    self.status = format!("This catalog was refused: {e}");
                }
            },
        }
        self.store = Some(store);
        self.refresh(cx);
    }

    /// Re-read the listings for the current query and rebuild the screen.
    fn refresh(&mut self, cx: &mut Cx) {
        self.listings = self.store.as_ref().map(|s| s.search(&self.query)).unwrap_or_default();
        self.view.label(cx, ids!(origin_label)).set_text(cx, &self.status);
        let screen = self.screen.clone();
        let asset_base = self.asset_base();
        let body = match &screen {
            Screen::List => ui::list_source(&self.listings, &self.query, asset_base.as_deref()),
            Screen::Detail(index) => match self.listings.get(*index) {
                Some(listing) => ui::detail_source(listing, &self.status, asset_base.as_deref()),
                None => {
                    self.screen = Screen::List;
                    return self.refresh(cx);
                }
            },
            Screen::Running(app_id) => ui::running_source(app_id, &self.status),
        };
        self.mount(cx, ids!(screen), &body);
        self.layout_logged = false;
        if !matches!(screen, Screen::Running(_)) {
            // No app running: the card container holds nothing and its
            // isolate has nothing to draw.
            self.view.splash(cx, ids!(card)).set_text(cx, "");
        }
        self.view.redraw(cx);
    }

    /// Where listing images come from: the hub over HTTP. A mirror directory
    /// has no origin the widgets can load from, so its listings show no
    /// pictures.
    fn asset_base(&self) -> Option<String> {
        match &self.origin {
            Some(source::Origin::Http(base)) => Some(format!("{}/", base.trim_end_matches('/'))),
            _ => None,
        }
    }

    /// Evaluate generated source into `target`. Trusted-tier UI only: see the
    /// note at the top of `ui.rs`.
    fn mount(&mut self, cx: &mut Cx, target: &[LiveId], body: &str) {
        let code = format!("use mod.prelude.widgets.*\nreturn View{{{body}}}");
        let script_mod = ScriptMod {
            cargo_manifest_path: env!("CARGO_MANIFEST_DIR").into(),
            module_path: module_path!().into(),
            file: file!().into(),
            line: 1,
            column: 0,
            code,
            values: Vec::new(),
        };
        let built = cx.with_vm(|vm| vm.eval_checked(script_mod, 2_000_000).map(|value| View::script_from_value(vm, value)));
        let Some(built) = built else {
            error!("appstore: the store's own screen did not evaluate");
            return;
        };
        let host = self.view.widget(cx, target);
        let Some(mut host_splash) = host.borrow_mut::<makepad_widgets::splash::Splash>() else {
            error!("appstore: {target:?} is not a screen container");
            return;
        };
        host_splash.view = built;
        let uid = host_splash.widget_uid();
        let mut children = Vec::new();
        host_splash.children(&mut |id, child| children.push((id, child)));
        drop(host_splash);
        for (id, child) in children {
            cx.widget_tree_insert_child_deep(uid, id, child);
        }
        cx.widget_tree_mark_dirty(uid);
    }

    /// Install the app at `index`, then show it as installed. Every check the
    /// client makes is reported rather than swallowed: a refusal is the most
    /// useful thing the store can say.
    fn install(&mut self, cx: &mut Cx, index: usize) {
        if let Err(error) = self.require_current_catalog_channel() {
            self.status = error;
            return self.refresh(cx);
        }
        let Some(listing) = self.listings.get(index).cloned() else { return };
        let (Some(store), Some(origin)) = (self.store.as_ref(), self.origin.as_ref()) else { return };
        let Some(entry) = store.entry(&listing.app_id) else { return };
        let artifact = entry.artifact.clone();
        let manifest = entry.manifest.clone();
        let staging = self.app_data_root.join(&listing.app_id);
        if let Err(e) = std::fs::create_dir_all(&staging) {
            self.status = format!("Cannot prepare {}: {e}", listing.app_id);
            return self.refresh(cx);
        }
        self.status = match origin
            .stage(&artifact, &staging)
            .and_then(|staged| {
                self.require_current_catalog_channel()?;
                // The publisher's key comes from the signed catalog, so the
                // store checks their signature itself rather than trusting
                // that the hub did.
                let keys = store.publisher_keys();
                // Its shared components first (App Hub ADR 0003), from the
                // same verified catalog: the app never lands without them.
                components::install(store, origin, &manifest)?;
                store
                    .install_staged(&listing.app_id, &staged, &keys, &octosense_app_hub::today())
                    .map(|policy| (staged, policy))
            })
        {
            Ok((staged, policy)) => {
                let _ = std::fs::remove_dir_all(&staged);
                // An update may leave a component no installed app pins.
                let _ = store.collect_components();
                format!("Installed {} {} — {} capability(ies)", listing.name, listing.version, policy.capabilities.len())
            }
            Err(e) => format!("Not installed: {e}"),
        };
        self.refresh(cx);
    }

    /// Open an installed app: check it may still run, apply its policy to the
    /// isolate, then hand the card's source to that isolate.
    fn open(&mut self, cx: &mut Cx, app_id: &str) {
        if let Err(error) = self.require_current_catalog_channel() {
            self.status = error;
            return self.refresh(cx);
        }
        let Some(store) = self.store.as_ref() else { return };
        let policy = match store.may_run(app_id) {
            Ok(policy) => policy,
            Err(e) => {
                self.status = format!("Cannot open: {e}");
                return self.refresh(cx);
            }
        };
        let mut settings = policy.isolate_settings(&self.app_data_root);
        if let Err(e) = std::fs::create_dir_all(&settings.jail_root) {
            self.status = format!("Cannot make the app's jail: {e}");
            return self.refresh(cx);
        }
        let bundle = store.install_dir(app_id);
        // One asset origin per running app, serving only that app's bundle.
        // It is the ONE loopback entry the isolate may reach: exactly this
        // port, so a sibling app's origin or a stray dev server is refused.
        let origin = match octosense_app_policy::AssetServer::start(&bundle) {
            Ok(server) => {
                let origin = server.origin().to_string();
                settings.hosts.push(server.allowlist_entry());
                self.asset_server = Some(server);
                origin
            }
            Err(e) => {
                self.status = format!("Cannot serve the app's artwork: {e}");
                return self.refresh(cx);
            }
        };
        let splash = self.view.splash(cx, ids!(card));
        let applied = octosense_app_policy::splash_adapter::apply(&splash, cx, &settings);
        if let Err(error) = apply_device_consent(cx, &bundle, &splash) {
            self.status = format!("Cannot apply app permissions: {error}");
            return self.refresh(cx);
        }
        match card_source(&bundle, &origin) {
            Ok(code) => {
                splash.set_text(cx, &code);
                self.screen = Screen::Running(app_id.to_string());
                self.status = format!(
                    "{} running · storage {} bytes · {} instructions · {} MB heap · {} host(s)",
                    app_id,
                    applied.storage_quota,
                    applied.instruction_budget,
                    applied.memory_bytes / (1024 * 1024),
                    applied.hosts
                );
            }
            Err(e) => self.status = format!("The app did not open: {e}"),
        }
        self.refresh(cx);
    }

    fn require_current_catalog_channel(&self) -> Result<(), String> {
        self.channel.ok_or("No verified catalog channel is available")?
            .require_current(&self.app_data_root)
    }
}

/// The cards this process runs draw with the kit, so every isolate gets
/// that vocabulary. Process-wide by the runtime's design (see ADR 0002's
/// open question on per-app vocabulary); registered once, whichever host
/// links the store or the card module.
///
/// Once per THREAD, not per process: makepad keeps the isolate-mod list in a
/// thread-local (`register_splash_isolate_mod`), so a process-wide `Once`
/// left every thread but the first registrant without `DesignSurface`,
/// `KitButton` and `sys` (a host UI has one thread; parallel tests do not).
pub(crate) fn register_card_vocabulary() {
    thread_local! {
        static REGISTERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if REGISTERED.with(|done| done.replace(true)) {
        return;
    }
    fn design(vm: &mut ScriptVm) {
        octoscript_widgets::design::script_mod(vm);
    }
    fn kit(vm: &mut ScriptVm) {
        octoscript_widgets::kit::script_mod(vm);
    }
    makepad_widgets::widget_async::register_splash_isolate_mod(crate::script_tools::script_mod);
    makepad_widgets::widget_async::register_splash_isolate_mod(design);
    makepad_widgets::widget_async::register_splash_isolate_mod(kit);
    // `sys`: places, routes, weather and the other live-data helpers a
    // script app reads, every fetch held to the app's host list, the
    // device's location to its `location` grant. It also carries
    // `agent.notify`, which an app under a policy may call only with the
    // `agent` grant.
    makepad_widgets::widget_async::register_splash_isolate_mod(makepad_widgets::splash::register_agent_module);
}

/// Inventory for the patched runtime's policy ABI, independent of which
/// device services a shell registers. Plain Makepad must not advertise it.
pub fn register_policy_runtime_features() {
    #[cfg(feature = "text-input-state-query")]
    crate::host_api::register_runtime_feature("app_policy.device_consent", 1);
}

/// Bind an already admitted bundle's identity and device-consent boundary
/// before untrusted source runs. Callers must supply the verified launch copy
/// (or a shipped system bundle), never a path or identity from app arguments.
/// Plain Makepad hosts cannot advertise OctoSense's permission boundary.
/// Failure clears any previous heap identity instead of retaining another app.
pub fn apply_device_consent(cx: &mut Cx, bundle: &std::path::Path, splash: &SplashRef) -> Result<(), String> {
    splash.set_host_tag(cx, None);
    let text = std::fs::read_to_string(bundle.join("manifest.json")).map_err(|error| error.to_string())?;
    let manifest = octosense_app_policy::AppManifest::parse(&text)?;
    #[cfg(not(feature = "text-input-state-query"))]
    let required = manifest.requires.iter().any(|feature| feature == "host-api-v1");
    #[cfg(feature = "text-input-state-query")]
    {
        // Apply to every admitted app, including legacy manifests: otherwise
        // an omitted host-api-v1 could inherit the shell's OS permission.
        splash.set_device_consent(cx, true);
        let explicit_capture_intent = crate::host_api::available_versions().get("camera.capture_intent") == Some(&1);
        splash.set_host_caps(cx, octosense_app_policy::containers::public_runtime_capabilities(
            &manifest.capabilities, true, explicit_capture_intent,
        ));
    }
    #[cfg(not(feature = "text-input-state-query"))]
    {
        if required { return Err("This standalone build does not implement the host-api-v1 device permission broker".into()); }
    }
    splash.set_host_tag(cx, Some(manifest.id));
    Ok(())
}

/// Lower a bundle to isolate source. Nothing outside the bundle is read. A
/// script app's program runs as it is; a card is lowered to widgets.
pub(crate) fn card_source(bundle: &std::path::Path, asset_origin: &str) -> Result<String, String> {
    if let Some(script) = octosense_app_policy::script_source(bundle, asset_origin) {
        return script;
    }
    let card = std::fs::read_to_string(bundle.join("page.card")).map_err(|e| format!("page.card: {e}"))?;
    let data_text = std::fs::read_to_string(bundle.join("page.data.json")).unwrap_or_else(|_| "{}".into());
    let mut data: serde_json::Value = serde_json::from_str(&data_text).map_err(|e| format!("page.data.json: {e}"))?;
    octosense_app_policy::rewrite_assets(&mut data, asset_origin);
    let mut prepared = card_assets::prepare(&card, &data, bundle, asset_origin)?;
    // The card occupies a host-owned pane, which need not start at (0, 0).
    // Measured frames are card-local; absolute window coordinates misplace
    // descendants when the same bundle opens inside the desktop shell.
    let ui = match octoscript_makepad::design::to_makepad_ui_in_slot(&prepared.tree) {
        Ok(ui) => ui,
        Err(error) if prepared.native_components => return Err(error),
        Err(_) => {
            // Role-based L0 (Col, Surface, ...) has no measured frames. Use
            // the same flow lowering as card-host; malformed measured kits
            // above must still fail rather than change rendering semantics.
            octoscript_makepad::l0::inspectable(&mut prepared.tree);
            octoscript_makepad::to_makepad_l0_ui(&prepared.tree)
        }
    };
    // Preserve the measured canvas instead of clipping its controls when the
    // host pane is smaller. Native scrolling supplies both axes and keeps
    // clipping/hit testing inside the app's own viewport. Script apps above
    // continue to own their responsive layout and scroll containers.
    Ok(format!("width:Fill height:Fill flow:Overlay\n\
        l0_canvas := ScrollXYView {{ width:Fill height:Fill flow:Overlay\n{ui}\n}}"))
}

impl Widget for AppStoreView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if !self.started {
            self.started = true;
            self.start(cx);
        }
        self.view.handle_event(cx, event, scope);
        let Event::Actions(actions) = event else { return };

        if let Some(query) = self.view.text_input(cx, ids!(search)).changed(actions) {
            self.query = query;
            if matches!(self.screen, Screen::List) {
                self.refresh(cx);
            }
        }
        match self.screen.clone() {
            Screen::List => {
                for index in 0..self.listings.len() {
                    if self.view.button(cx, &[LiveId::from_str(&format!("open_{index}"))]).clicked(actions) {
                        self.screen = Screen::Detail(index);
                        self.status.clear();
                        self.refresh(cx);
                        break;
                    }
                }
            }
            Screen::Detail(index) => {
                if self.view.button(cx, ids!(back_button)).clicked(actions) {
                    self.screen = Screen::List;
                    self.status.clear();
                    self.refresh(cx);
                } else if self.view.button(cx, ids!(action_button)).clicked(actions) {
                    match self.listings.get(index).map(|l| (l.app_id.clone(), l.availability.clone())) {
                        Some((app_id, Availability::Installed { .. })) => {
                            // The host opens it as an app of its own. With no
                            // host listening (the standalone store), fall back
                            // to running it in place.
                            if std::env::var("OCTOSENSE_STORE_INLINE").is_ok() {
                                self.open(cx, &app_id);
                            } else {
                                cx.widget_action(self.view.widget_uid(), AppStoreAction::Launch(app_id));
                            }
                        }
                        Some((_, Availability::Installable)) => {
                            self.install(cx, index);
                            cx.widget_action(self.view.widget_uid(), AppStoreAction::CatalogChanged);
                        }
                        _ => {}
                    }
                } else if self.view.button(cx, ids!(remove_button)).clicked(actions) {
                    if let (Some(store), Some(listing)) = (self.store.as_ref(), self.listings.get(index)) {
                        self.status = match store.remove(&listing.app_id) {
                            Ok(()) => format!("Removed {} and its data", listing.name),
                            Err(e) => format!("Not removed: {e}"),
                        };
                        self.refresh(cx);
                        cx.widget_action(self.view.widget_uid(), AppStoreAction::CatalogChanged);
                    }
                }
            }
            Screen::Running(_) => {
                if self.view.button(cx, ids!(close_button)).clicked(actions) {
                    // The app's asset origin goes when the app does.
                    self.asset_server = None;
                    self.screen = Screen::List;
                    self.refresh(cx);
                }
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let step = self.view.draw_walk(cx, scope, walk);
        // Layout evidence for a host with no instrument (a phone): where the
        // detail screen's controls landed, once per screen change.
        if matches!(self.screen, Screen::Detail(_)) && !self.layout_logged {
            let names = ["back_button", "action_button", "remove_button", "status_label"];
            let mut report = Vec::new();
            for name in names {
                let widget = self.view.widget(cx, &[LiveId::from_str(name)]);
                let rect = if widget.is_empty() { None } else { Some(widget.area().rect(cx)) };
                report.push(format!("{name}={:?}", rect.map(|r| (r.pos.x as i32, r.pos.y as i32, r.size.x as i32, r.size.y as i32))));
            }
            let screen = self.view.widget(cx, ids!(screen)).area().rect(cx);
            log!("appstore layout: screen container {:?}; {}", (screen.pos.y as i32, screen.size.y as i32), report.join(" "));
            self.layout_logged = true;
        }
        step
    }
}

pub struct AppStoreModule;
pub static APPSTORE_MODULE: AppStoreModule = AppStoreModule;

impl AppModule for AppStoreModule {
    fn id(&self) -> &'static str {
        "appstore"
    }
    fn label(&self) -> &'static str {
        "Apps"
    }
    /// Storage for what it installs; nothing else. The store holds the anchor
    /// and writes jails because it is trusted-tier code, not because a
    /// capability grants it.
    fn capabilities(&self) -> &'static [&'static str] {
        &["storage"]
    }
    fn open_schema(&self) -> OpenSchema {
        OpenSchema::new(1).arg("app", OpenArgKind::Text, false)
    }
    fn register(&self, vm: &mut ScriptVm) {
        register_policy_runtime_features();
        octoscript_widgets::design::script_mod(vm);
        octoscript_widgets::kit::script_mod(vm);
        script_mod(vm);
        register_card_vocabulary();
    }
    fn create(&self, vm: &mut ScriptVm, open: ValidatedOpen, handles: InstanceHandles) -> InstanceParts {
        let value = script_eval!(vm, { use mod.widgets.* AppStoreView {} });
        let root = WidgetRef::script_from_value(vm, value);
        if let Some(mut store) = root.borrow_mut::<AppStoreView>() {
            // Opening straight onto an app, so a launcher entry can point at
            // one; the store still verifies before it runs anything.
            if let Some(app) = open.text("app") {
                store.screen = Screen::Running(app.to_owned());
            }
            let _ = handles.viewport;
        }
        InstanceParts {
            root,
            executor: Box::new(AppStoreExecutor),
            shutdown: Box::new(|_vm| {}),
        }
    }
}

struct AppStoreExecutor;

impl ServiceExecutor for AppStoreExecutor {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new("appstore", "Apps", "Browse, install and open apps published to the OctoSense app hub.")
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        // Installing is a decision a person makes in front of the screen,
        // not a tool an agent calls.
        ExecOutcome::Done(ToolResult::unavailable(&call.call_id, "Use the Apps interface"))
    }
    fn cancel(&mut self, _cx: &mut Cx, _call_id: &str) {}
    fn subscribe(&mut self, _cx: &mut Cx, _sub_id: &str, _topic: &str, _filter: Option<&str>) {}
    fn unsubscribe(&mut self, _cx: &mut Cx, _sub_id: &str) {}
}

#[cfg(test)]
mod card_vocabulary_tests {
    use super::*;

    /// A card isolate allocated on this thread names the kit after the Card
    /// runner registers, whether or not another thread registered first.
    fn card_isolate_names_the_kit() -> Vec<String> {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        cx.with_vm(|vm| cardapp::CARD_MODULE.register(vm));
        let card = cx.alloc_splash_vm_with_network(false);
        let errors = cx.with_script_vm_id_trusted(card, |vm| {
            vm.bx.captured_errors = Some(Vec::new());
            let value = script_eval!(vm, {use mod.prelude.widgets.* DesignSurface{title := Label{text: "Trail Notes"}}});
            let root = WidgetRef::script_from_value(vm, value);
            let mut errors: Vec<String> = vm.take_errors().into_iter().map(|e| format!("{e:?}")).collect();
            if root.label(vm.cx_mut(), ids!(title)).text() != "Trail Notes" {
                errors.push("DesignSurface did not build its title".into());
            }
            errors
        });
        cx.free_splash_vm(card);
        errors
    }

    #[test]
    fn policy_abi_is_advertised_only_by_the_patched_runner() {
        register_policy_runtime_features();
        let versions = crate::host_api::available_versions();
        #[cfg(feature = "text-input-state-query")]
        assert_eq!(versions.get("app_policy.device_consent"),Some(&1));
        #[cfg(not(feature = "text-input-state-query"))]
        assert!(!versions.contains_key("app_policy.device_consent"));
    }

    #[test]
    fn every_thread_that_registers_the_card_runner_gets_the_kit() {
        // makepad's isolate-mod list is per thread: registering on one thread
        // must not stop a second thread from registering its own.
        let first = std::thread::spawn(card_isolate_names_the_kit).join().unwrap();
        assert_eq!(first, Vec::<String>::new());
        let second = std::thread::spawn(card_isolate_names_the_kit).join().unwrap();
        assert_eq!(second, Vec::<String>::new());
    }
}

#[cfg(test)]
mod installed_apps_tests {
    use super::*;

    fn manifest(dir: &std::path::Path, id: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join(octosense_app_policy::MANIFEST_FILE),
            serde_json::json!({"schema": 1, "id": id, "version": "1.0.0", "name": id, "integrity": {"bundle_blake3": ""}}).to_string(),
        )
        .unwrap();
    }

    /// The library lists installs under `.bundles`, and moves one made
    /// before that layout out of its app's storage first.
    #[test]
    fn installed_apps_lists_installs_outside_the_apps_storage() {
        let root = std::env::temp_dir().join(format!("appstore-installed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        manifest(&octosense_app_hub::installed_bundle_dir(&root, "dev.example.new"), "dev.example.new");
        manifest(&root.join("dev.example.old").join("bundle"), "dev.example.old");
        let ids: Vec<String> = installed_apps(&root).into_iter().map(|a| a.id).collect();
        assert_eq!(ids, ["dev.example.new", "dev.example.old"]);
        assert!(!root.join("dev.example.old/bundle").exists(), "the old install left its app's storage");
        assert!(octosense_app_hub::installed_bundle_dir(&root, "dev.example.old").join(octosense_app_policy::MANIFEST_FILE).is_file());
        let _ = std::fs::remove_dir_all(&root);
    }
}
