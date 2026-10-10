//! The built-in apps registry's hosting dimension (aicontrol.md §4): which
//! apps are linked in as MODULES, and which of those the person has
//! switched to module hosting.
//!
//! The launch table (`clients::registry()`: package, directory, binary,
//! launch policy — everything a PROCESS needs) stays where it is; this is
//! the overlay keyed by the same ids: the linked `AppModule`, and the
//! hosting each app gets. A linked native app is hosted on a desktop as its
//! `native-apps.json` entry says for this target (ADR 0004 §2: the Terminal
//! is a process on macOS and Windows, App Hub and Rinx are in-process),
//! unless `~/.makepad/wm/apps.splash` says otherwise (a settings file, never
//! an environment variable) or a dev run passes `--module <id>`; any other
//! linked module is a process unless switched. The uber builds ignore the
//! switch: everything is a module there.
//!
//! App Hub (feature `app-hub`, on by default; always on native mobile) adds
//! the apps its Card runner hosts: the system apps this build ships as
//! contained script bundles (`os.news`, … ADR 0004) and the apps the person
//! installed from the App Hub catalog (`hub:<manifest-id>`). Neither has a
//! process form; the linked `card` module runs every one of them.
//!
//! A product links its own modules too (`ext::linked_modules`: the
//! phone's Settings app).

use makepad_app_module::AppModule;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hosting {
    Process,
    Module,
}

pub struct AppRegistry {
    modules: Vec<&'static dyn AppModule>,
    overrides: HashMap<String, Hosting>,
}

impl Default for AppRegistry {
    fn default() -> Self {
        AppRegistry { modules: linked_modules(), overrides: HashMap::new() }
    }
}

/// A linked module by id, without a registry: what the launcher asks.
pub fn is_linked(id: &str) -> bool {
    linked_modules().iter().any(|m| m.id() == id) || is_card_app(id)
}

/// The native apps this build links (`native-apps.json`, generated into
/// `native_apps.rs` by tools/native_apps.py; ADR 0004 §1), then what the
/// product links beyond the shell (the phone's Settings). Native mobile
/// targets link App Hub, Reference and Sheets without flags; the rest come
/// with their `app-*` features. Rinx's assistant is the shell's, given at
/// creation (ai_host); it never starts a kernel of its own. The in-process
/// Terminal's PTY helper on macOS is this executable (`--exec-pty`, handled
/// in `Cx::pre_start`), so it needs no second binary shipped beside it.
fn linked_modules() -> Vec<&'static dyn AppModule> {
    let mut out: Vec<&'static dyn AppModule> = Vec::new();
    crate::native_apps::link(&mut out);
    out.extend(crate::ext::linked_modules());
    out
}

/// Manifest ids live in a separate namespace from built-ins and catalog rows
/// (catalog ids cannot contain a colon).
pub fn installed_launch_id(manifest_id: &str) -> String {
    format!("hub:{manifest_id}")
}

/// Whether `manifest_id` may be a script app's id (ADR 0004 §1, §3, §11): a
/// store or system app never takes a native app's id or tool namespace
/// (`terminal`, or `com.example.terminal`, whose tools would be
/// `terminal.*`), nor a name the shell acts under (App Hub's
/// `RESERVED_NAMES`: `system`, `toolbox`, `dev`, ...). The shell keys a
/// script app's jail and storage folders, tool declarations, executor and
/// consent by that id, so one that took a native app's would stand in for
/// it. App Hub refuses such ids at its gate, install and admission; the
/// shell checks again against its own `native-apps.json`, so a native app
/// App Hub has not heard of yet is covered too.
pub fn check_script_app_id(manifest_id: &str) -> Result<(), String> {
    let namespace = manifest_id.rsplit('.').next().unwrap_or(manifest_id);
    for name in [manifest_id, namespace] {
        if crate::native_apps::find(name).is_some() {
            return Err(format!("{manifest_id} takes the native app {name:?}'s name, which no script app may use"));
        }
    }
    #[cfg(any(feature = "app-hub", native_mobile))]
    octosense_app_policy::check_reserved_id(manifest_id)?;
    Ok(())
}

/// The manifest id the `card` module opens for a launcher row: `os.<name>`
/// for a system app, the installed app's own id for `hub:<id>`.
pub fn card_manifest_id(app: &crate::clients::AppDef) -> Option<&str> {
    if app.bin != "card" {
        return None;
    }
    app.id.strip_prefix("hub:").or_else(|| app.args.iter().find_map(|a| a.strip_prefix(SYSTEM_ARG)))
}

/// A system app's launcher row carries its manifest id as this argument.
const SYSTEM_ARG: &str = "--system=";

fn card_row(id: String, label: String, args: Vec<String>) -> crate::clients::AppDef {
    crate::clients::AppDef {
        id,
        label,
        bin: "card".into(),
        package: String::new(),
        dir: String::new(),
        manifest: None,
        args,
        policy: crate::clients::LaunchPolicy::OrFocus,
        target_dir: None,
    }
}

/// System apps (ADR 0004): first-party apps from OctoSense-System-Apps that
/// App Hub ships as contained script bundles, run by the Card runner. Each
/// keeps its short launcher id (`mail` for `os.mail`), so its icon, home
/// tile and dock place are the ones that id always had. They take
/// precedence over catalog rows of the same id (`clients::registry`); a
/// linked native module of the same id would win.
pub fn system_card_apps() -> Vec<crate::clients::AppDef> {
    #[cfg(any(feature = "app-hub", native_mobile))]
    {
        register_host_services();
        let native: Vec<&str> = linked_modules().iter().map(|m| m.id()).collect();
        return octosense_app_hub_app::system_apps()
            .into_iter()
            .filter_map(|app| {
                let short = app.id.strip_prefix("os.")?;
                (!native.contains(&short))
                    .then(|| card_row(short.into(), app.name.into(), vec![format!("{SYSTEM_ARG}{}", app.id)]))
            })
            .collect();
    }
    #[allow(unreachable_code)]
    Vec::new()
}

/// System apps a test registers for itself (the glance tile tests'
/// probes). They join App Hub's registry, the one [`system_card_apps`]
/// lists, which is process-wide and never forgets an app, so a test of the
/// build's own catalog saw them or not depending on which tests had run
/// before it in the same process. A test registers its app here, and the
/// catalog tests leave these out. Store-agent conversation fixtures use
/// isolated child processes with their own signed catalogs and app roots.
#[cfg(test)]
pub(crate) mod test_system_apps {
    use std::sync::Mutex;

    static IDS: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

    /// Register `app` for a test. It is noted first, so a catalog that
    /// lists it was read after it was noted.
    #[cfg(feature = "app-hub")]
    pub(crate) fn register(app: octosense_appstore::system::SystemApp) {
        IDS.lock().unwrap_or_else(|e| e.into_inner()).push(app.id);
        octosense_appstore::system::register_system_app(app);
    }

    /// Whether a test registered the system app `id` (an `os.` id).
    pub(crate) fn is_test_app(id: &str) -> bool {
        IDS.lock().unwrap_or_else(|e| e.into_inner()).contains(&id)
    }
}

/// The services contained apps call through `host.request` (ADR 0004)
/// that are not the assistant's, registered once, before the first system
/// app can open: `mail` keeps accounts and passwords for the Mail app;
/// `glance` takes the cards apps publish to the glance screen (glance.rs).
/// `mail_demo` in MAKEPAD_APP_CONFIG serves a demo mailbox from a file vault
/// instead (no keychain, no network): `MAKEPAD_APP_CONFIG='{"mail_demo":true}'`.
/// `news` fetches News's feeds on a timer, with no model (ADR 0002), into
/// the Card runner's host directory, so it keeps fetching while News is
/// closed. Last, every system app no service answers gets the notice
/// service (glance_notice.rs), for its agent's `<namespace>.notify`.
///
/// The `llm` service (AI providers, `os.ai-providers`) is the assistant's
/// and registers with the kernel in `ai_host::start`, at startup, with the
/// `model` service (`model.complete`, ADR 0002) over the same providers.
#[cfg(any(feature = "app-hub", native_mobile))]
pub fn register_mail_services() {
    // Admission is separate from the manifest request, relay grant and
    // owner's shareable declaration. Offer only Mail's reviewed dependencies.
    octosense_appstore::system::set_agent_tool_offer("os.mail", &["calendar.events", "calendar.add_event", "calendar.notify"]);
    register_calendar_services();
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        crate::glance::register();
        let demo = std::env::var("MAKEPAD_APP_CONFIG")
            .ok()
            .and_then(|text| makepad_strict_json::parse(text.as_bytes()).ok())
            .and_then(|config| config.get("mail_demo").and_then(|v| v.as_bool()))
            .unwrap_or(false);
        if demo {
            octosense_mail_service::register_demo()
        } else {
            octosense_mail_service::register()
        }
        // `mail.notify` (Mail's agent's tool): the shell's notice card, as
        // Mail, after verifying the publishing app's admission.
        octosense_mail_service::on_notify(Some(std::sync::Arc::new(crate::glance_notice::notify)));
        octosense_mail_service::on_publish_card(Some(std::sync::Arc::new(|app: &str, args: serde_json::Value| crate::glance::publish_mail_l0_for(app, &args))));
        octosense_mail_service::drafts::on_change(Some(std::sync::Arc::new(makepad_widgets::makepad_platform::SignalToUI::set_ui_signal)));
        octosense_mail_service::drafts::on_review_requested(Some(std::sync::Arc::new(|review| {
            let key = format!("os.mail/compose-{}", review.snapshot()["draft_id"].as_str().ok_or("Missing draft identity")?);
            crate::mail_card::queue_review(key, review);
            Ok(())
        })));
        octosense_mail_service::public_review::on_review(Some(std::sync::Arc::new(
            crate::connected_review::smtp_sheet,
        )));
        crate::mail_card::publication_host_ready();
    });
}

#[cfg(any(feature = "app-hub", native_mobile))]
pub fn register_host_services() {
    register_mail_services();
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Capability declarations describe intended use. Admission still binds
        // every call to a verified app and the current host profile.
        crate::ai_host::set_model_grants(|app, host_dir| {
            crate::host_tools::script_apps::admitted_host(app, host_dir).is_ok()
        });
        crate::ai_host::contained::set_caller_admitted(|app, host_dir| {
            crate::host_tools::script_apps::admitted_host(app, host_dir).is_ok()
        });
        crate::ai_host::set_model_scope(|app, _| {
            crate::app_storage::lifecycle::contained_account(app)
                .map(|account| crate::app_storage::account_hash(&account))
        });
        crate::platform_services::register();
        crate::device_calendar::register(|app| {
            crate::app_storage::lifecycle::contained_account(app)
                .map(|account| crate::app_storage::account_hash(&account))
        });
        crate::files_service::register();
        crate::audio_service::register();
        // App Hub admits bundles before the first CardModule is opened. The
        // linked card runner already supports app tools at that point, so its
        // ABI must not depend on a card's lazy VM registration.
        octosense_appstore::host_api::register_runtime_feature("app_tools.dispatch", 1);
        octosense_appstore::host_api::register_runtime_feature("storage.binary_write", 1);
        octosense_appstore::host_api::register_runtime_feature("video.playback_controls", 1);
        octosense_appstore::host_api::register_runtime_feature("camera.capture_intent", 1);
        octosense_markdown_editor::register();
        crate::connected_review::register();
        octosense_oauth_service::host_inbox::register_with_review_hook(crate::connected_review::sheet);
        octosense_oauth_service::host_inbox::set_publication_verifier(|app, account, card_id| {
            crate::glance::card(&format!("{app}/{card_id}")).is_some_and(|card|
                card.app == app && card.card_id == card_id && card.account.as_deref() == Some(account)
                && crate::mail_background::has_publication(app,account,card_id,card.published_ms))
        });
        crate::connected_events::start();
        crate::connected_backends::register();
        octosense_oauth_service::host::register(std::sync::Arc::new(|app, provider, scopes| {
            crate::host_tools::script_apps::admitted(app)
                && provider.validate_scopes(&scopes.iter().cloned().collect::<Vec<_>>()).is_ok()
        }));
        // Provider sign-in sheets name the app from its admitted manifest and
        // show GitHub's one-time code in a host-held panel (ADR 0010).
        octosense_oauth_service::sign_in_code::register();
        octosense_oauth_service::host::set_app_names(std::sync::Arc::new(|app| {
            let (_, bundle) = crate::host_tools::script_apps::admitted_bundle(app).ok()?;
            let loaded = crate::host_tools::script_apps::from_bundle(&bundle).ok()?;
            loaded.manifest["name"].as_str().map(str::to_owned)
        }));
        octosense_oauth_service::host_api::register_with_review_hook(
            crate::connected_review::connector_sheet,
        );
        // Calendar's events and cards (its agent's `calendar.*` tools),
        // published the same way.
        register_calendar_services();
        register_news();
        // The sheet engine service (ADR 0013): gridcraft behind `sheet.*`.
        // It ships on the phone too, where the native Sheets app's agent
        // tools use it (weighed per engine: +3.4 MB stripped on Home); the
        // photo engine and the ten below are desktop only (`craft-engines`).
        #[cfg(feature = "app-hub")]
        octosense_sheets_service::register();
        // The native Sheets app's agent tools (`sheets.*`) run on that
        // service in this process; without the executor the relay would
        // route them to a bus the Sheets module does not serve.
        #[cfg(feature = "app-hub")]
        crate::host_tools::set_executor("sheets", Some(std::sync::Arc::new(crate::host_tools::engines::EngineExecutor::sheets())));
        // The photo engine service (ADR 0013): photocraft behind `photo.*`,
        // desktop only (`craft-engines`): weighed per engine, it would add
        // 26.8 MB stripped to Home.
        #[cfg(feature = "craft-engines")]
        octosense_photo_service::register();
        // Photos' own namespace service, as Mail's and News's: its agent's
        // `photos.info` on the engine, `photos.notify` on the shell's
        // notice card (so the notice service never stands in for Photos).
        // Without the photo engine (Home), the notice service answers
        // Photos' namespace (`glance_notice::serve_system_apps`):
        // `photos.notify` works, and `photos.info` says plainly that it is
        // not available on this device (`script_apps::unlinked_engine`).
        #[cfg(feature = "craft-engines")]
        {
            octosense_photo_service::register_photos();
            octosense_photo_service::on_notify(Some(std::sync::Arc::new(crate::glance_notice::notify)));
        }
        // The word engine service (ADR 0013): wordcraft behind `word.*`.
        #[cfg(feature = "craft-engines")]
        octosense_word_service::register();
        // The deck engine service (ADR 0013): deckcraft behind `deck.*`.
        #[cfg(feature = "craft-engines")]
        octosense_deck_service::register();
        // The cad engine service (ADR 0013): cadcraft behind `cad.*`.
        #[cfg(feature = "craft-engines")]
        octosense_cad_service::register();
        // The light engine service (ADR 0013): lightcraft behind `light.*`.
        #[cfg(feature = "craft-engines")]
        octosense_light_service::register();
        // The sound engine service (ADR 0013): soundcraft behind `sound.*`,
        // offline only — it never opens an audio device.
        #[cfg(feature = "craft-engines")]
        octosense_sound_service::register();
        // The design engine service (ADR 0013): designcraft behind `design.*`.
        #[cfg(feature = "craft-engines")]
        octosense_design_service::register();
        // The film engine service (ADR 0013): filmcraft behind `film.*`,
        // offline only.
        #[cfg(feature = "craft-engines")]
        octosense_film_service::register();
        // The effect engine service (ADR 0013): effectcraft behind `effect.*`.
        #[cfg(feature = "craft-engines")]
        octosense_effect_service::register();
        // The vector engine service (ADR 0013): vectorcraft behind `vector.*`.
        #[cfg(feature = "craft-engines")]
        octosense_vector_service::register();
        // The pdf engine service (ADR 0013): pdfcraft behind `pdf.*`.
        #[cfg(feature = "craft-engines")]
        octosense_pdf_service::register();
        // The ten engines' agent tools (ADR 0013): each engine's
        // `tools.json` declared under its virtual owner `os.<family>` and
        // run on the service registered above. Only the system agent is
        // granted them (`system_chat::grants::ENGINE_TOOLS`).
        #[cfg(feature = "craft-engines")]
        crate::host_tools::engines::register();
        // Every engine above works in a folder the host picks for each call
        // (ADR 0013, `host_tools::areas`): a craft engine's tool in its
        // caller's own (the system agent's workspace, an app agent's
        // account folder), an app's own engine tool in that app's agent
        // folder, an app's own request in its storage.
        #[cfg(feature = "app-hub")]
        crate::host_tools::areas::install_resolvers();
        // Apps' own WebAssembly functions (ADR 0011).
        #[cfg(wasm_functions)]
        crate::wasm_service::register();
        // After every service of the shell's own: the notice service never
        // stands in for one.
        let served = crate::glance_notice::serve_system_apps();
        makepad_widgets::log!("glance: the notice service answers {served:?} (no service of their own)");
    });
}

/// Registers only local HTTP endpoints for the explicitly marked Android backend lab.
/// No account, credential, callback, transport or vault is injected. Production builds
/// cannot compile this startup action, and ordinary provider registrations stay HTTPS.
#[cfg(all(feature = "dev-mode", feature = "acceptance-fixtures", target_os = "android"))]
pub(crate) fn register_android_backend_fixture() -> Result<(), String> {
    use octosense_oauth_service::backend::{BackendClient, BackendRegistration};
    let apps = octosense_app_hub_app::data_root_if_set().ok_or("Missing isolated app root")?;
    let host = apps.join(".host");
    octosense_oauth_service::acceptance_fixtures::validate_root(&host)?;
    let path = host.join("fixtures/backend-registrations.json");
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| "Missing backend fixture registration")?;
    if !metadata.is_file() || metadata.len() > 65_536 {
        return Err("Invalid backend fixture registration".into());
    }
    let registrations: Vec<BackendRegistration> = serde_json::from_slice(
        &std::fs::read(path).map_err(|_| "Cannot read backend fixture registration")?
    ).map_err(|_| "Invalid backend fixture registration")?;
    let expected = ["org.octosense.samples.backend", "org.octosense.samples.backendother"];
    if registrations.len() != expected.len() || !expected.iter().all(|id|
        registrations.iter().filter(|registration| registration.app_id == *id).count() == 1
    ) {
        return Err("Backend fixture requires exactly its two isolated sample apps".into());
    }
    let clients: Vec<_> = registrations.into_iter()
        .map(BackendClient::new_loopback_fixture).collect::<Result<_, _>>()?;
    for client in clients {
        octosense_oauth_service::host::register_backend_fixture(&host, client)?;
    }
    Ok(())
}

/// Calendar is a granted dependency of Mail even in a headless background job.
/// Register its service without creating a Calendar peer or opening an app.
#[cfg(any(feature = "app-hub", native_mobile))]
fn register_calendar_services() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        octosense_calendar_service::register();
        octosense_calendar_service::on_publish_card(Some(std::sync::Arc::new(|app: &str, args: serde_json::Value| crate::glance::publish_for(app, &args))));
        octosense_calendar_service::on_withdraw_card(Some(std::sync::Arc::new(|app, id| {
            crate::glance::withdraw_for(app, id)
        })));
    });
}

/// The `news` service, in the directory the Card runner hands every host
/// service (`<apps root>/.host`), so the timer starts now rather than at
/// News's first request. Without an apps root yet it attaches at that
/// request instead.
#[cfg(any(feature = "app-hub", native_mobile))]
fn register_news() {
    let mut options = octosense_news_service::Options::default()
        .on_fetch(|report| {
            // M3 routes this to News's peer, to wake its agent; logged for now.
            let failed = report.sources.iter().filter(|s| s.status == "error").count();
            makepad_widgets::log!(
                "news: fetched {} new, {} kept, {} sources ({failed} failed)",
                report.new,
                report.total,
                report.sources.len()
            );
        })
        // `news.notify` (News's agent's tool): the shell's notice card, as News.
        .on_notify(crate::glance_notice::notify);
    match octosense_app_hub_app::data_root_if_set() {
        Some(root) => {
            let host_dir = root.join(".host");
            makepad_widgets::log!("news: service registered, host dir {}", host_dir.display());
            options = options.host_dir(host_dir);
        }
        None => makepad_widgets::log!("news: service registered; starts at the first request"),
    }
    octosense_news_service::register_with(options);
}

/// One app that declares an agent, for Settings and consent (ADR 0004 §4).
#[derive(Clone, Debug, PartialEq)]
pub struct AgentApp {
    /// The consent key: a native app's id (`rinx`), a script app's manifest
    /// id (`os.mail`, `org.example.timer`).
    pub id: String,
    pub name: String,
    /// The `octos.*` services it declares.
    pub octos: Vec<String>,
    /// Its manifest, as the first-use sheet reads it.
    pub manifest: serde_json::Value,
    /// A native app (`native-apps.json`); else a script app (App Hub).
    pub native: bool,
}

/// Every app that declares an agent (ADR 0004 §4): native apps whose
/// `native-apps.json` entry grants `octos.*`, and script apps (system and
/// installed) whose manifest declares `octos.*` or an `agent` block, or
/// whose admitted bundle ships `tools.json` ([`script_agent_app`]), whether
/// or not they have asked yet.
pub fn agent_apps() -> Vec<AgentApp> {
    let mut out: Vec<AgentApp> = crate::native_apps::APPS
        .iter()
        .filter(|a| !a.octos.is_empty())
        .map(|a| AgentApp {
            id: a.id.to_string(),
            name: crate::approvals::sheet::app_label(a.id),
            octos: a.octos.iter().map(|s| s.to_string()).collect(),
            manifest: serde_json::json!({ "agent": { "octos": a.octos } }),
            native: true,
        })
        .collect();
    out.extend(script_agent_apps());
    out
}

/// The `octos.*` services script app `app_id`'s manifest declares (`None`:
/// no opted-in agent here). The list is disclosure metadata; an admitted,
/// consented agent exposes the four supported assistant methods independently.
pub fn declared_octos(app_id: &str) -> Option<std::collections::BTreeSet<String>> {
    script_agent_apps().into_iter().find(|a| a.id == app_id).map(|a| a.octos.into_iter().collect())
}

fn octos_of(capabilities: &serde_json::Value) -> Vec<String> {
    capabilities.as_array().into_iter().flatten().filter_map(|c| c.as_str()).filter(|c| crate::ai_host::app_peers::OCTOS_SERVICES.contains(c)).map(str::to_string).collect()
}

#[cfg(any(feature = "app-hub", native_mobile))]
fn script_agent_apps() -> Vec<AgentApp> {
    // Read once per data root and App Hub generation (an install or update
    // bumps it): the contained service asks on every call.
    type Cache = Option<((std::path::PathBuf, u64), Vec<AgentApp>)>;
    static CACHE: std::sync::Mutex<Cache> = std::sync::Mutex::new(None);
    let Some(root) = octosense_app_hub_app::data_root_if_set() else { return Vec::new() };
    let key = (root.clone(), octosense_app_hub_app::icons::generation());
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((k, apps)) = cache.as_ref() {
        if *k == key {
            return apps.clone();
        }
    }
    let apps = read_script_agent_apps(&root);
    *cache = Some((key, apps.clone()));
    apps
}

#[cfg(any(feature = "app-hub", native_mobile))]
fn read_script_agent_apps(root: &Path) -> Vec<AgentApp> {
    let root = root.to_path_buf();
    let mut out = Vec::new();
    for app in octosense_app_hub_app::system_apps() {
        let Ok((dir, _)) = octosense_appstore::system::prepare(&root, &app) else { continue };
        if let Some(a) = script_agent_app(&dir.join("manifest.json"), app.id, app.name) {
            out.push(a);
        }
    }
    for app in octosense_app_hub_app::installed_apps(&root) {
        if let Some(a) = script_agent_app(&installed_bundle(&root, &app.id).join("manifest.json"), &app.id, &app.name) {
            out.push(a);
        }
    }
    out
}

#[cfg(not(any(feature = "app-hub", native_mobile)))]
fn script_agent_apps() -> Vec<AgentApp> {
    Vec::new()
}

/// One script app's agent, from its admitted manifest and bundle (the
/// manifest's directory): it has one when the manifest declares `octos.*`
/// or an `agent` block, or the bundle ships its own tools (`tools.json`,
/// loaded as App Hub admits it: `AgentBundle::load`, digest and every gate
/// rule checked). News ships tools and declares no `octos.*`: it has an
/// agent all the same.
pub fn script_agent_app(manifest_path: &Path, id: &str, name: &str) -> Option<AgentApp> {
    let text = std::fs::read_to_string(manifest_path).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&text).ok()?;
    if manifest["id"].as_str() != Some(id) { return None; }
    let octos = octos_of(&manifest["capabilities"]);
    let agent_block = manifest.get("agent").is_some_and(serde_json::Value::is_object);
    let declares = !octos.is_empty() || agent_block || bundle_ships_agent(manifest_path.parent()?, &text);
    declares.then(|| AgentApp { id: id.to_string(), name: name.to_string(), octos, manifest, native: false })
}

/// Whether the bundle beside a manifest ships an agent App Hub admits.
#[cfg(any(feature = "app-hub", native_mobile))]
fn bundle_ships_agent(bundle: &Path, manifest: &str) -> bool {
    let Ok(parsed) = octosense_app_contract::AppManifest::parse(manifest) else { return false };
    matches!(octosense_app_policy::AgentBundle::load(bundle, &parsed), Ok(Some(_)))
}

/// Without App Hub nothing admits a bundle: its `tools.json` is enough.
#[cfg(not(any(feature = "app-hub", native_mobile)))]
fn bundle_ships_agent(bundle: &Path, _manifest: &str) -> bool {
    bundle.join("tools.json").is_file()
}

/// Card apps App Hub installed: each is an app of its own in the launcher,
/// hosted by the linked `card` module under its `hub:<manifest-id>` identity.
/// Listed once per data root and App Hub generation: an install or update
/// bumps the generation (`App::installed_app_changed`), so it shows at once
/// without the install directory being read on every frame.
pub fn installed_card_apps() -> Vec<crate::clients::AppDef> {
    #[cfg(any(feature = "app-hub", native_mobile))]
    if let Some(root) = octosense_app_hub_app::data_root_if_set() {
        let key = (root.clone(), octosense_app_hub_app::icons::generation());
        return cached_installed_apps(key, || {
            octosense_app_hub_app::installed_apps(&root)
                .into_iter()
                .map(|app| card_row(installed_launch_id(&app.id), app.name, Vec::new()))
                .collect()
        });
    }
    Vec::new()
}

#[cfg(any(feature = "app-hub", native_mobile))]
thread_local! {
    static INSTALLED: std::cell::RefCell<Option<((std::path::PathBuf, u64), Vec<crate::clients::AppDef>)>> = const { std::cell::RefCell::new(None) };
}

#[cfg(any(feature = "app-hub", native_mobile))]
fn cached_installed_apps(key: (std::path::PathBuf, u64), load: impl FnOnce() -> Vec<crate::clients::AppDef>) -> Vec<crate::clients::AppDef> {
    INSTALLED.with(|slot| {
        let mut slot = slot.borrow_mut();
        match slot.as_ref() {
            Some((cached, apps)) if *cached == key => apps.clone(),
            _ => {
                let apps = load();
                *slot = Some((key, apps.clone()));
                apps
            }
        }
    })
}

/// Every launcher row the Card runner opens: system apps, then installed ones.
pub fn card_apps() -> Vec<crate::clients::AppDef> {
    let mut apps = system_card_apps();
    apps.extend(installed_card_apps());
    apps
}

/// Whether the `card` module hosts `id`. Installed ids carry their prefix,
/// so only they read the installed library.
fn is_card_app(id: &str) -> bool {
    if id.starts_with("hub:") {
        installed_card_apps().iter().any(|app| app.id == id)
    } else {
        system_card_apps().iter().any(|app| app.id == id)
    }
}

/// Launch-or-focus: a Card app focuses only an instance of that same app,
/// never a built-in (or another installed app) whose name it shares.
pub fn matches_running_app(app: &crate::clients::AppDef, running_id: &str, title: &str) -> bool {
    if app.bin == "card" || running_id.starts_with("hub:") {
        running_id == app.id
    } else {
        crate::clients::word_match(running_id, &app.id) || crate::clients::word_match(title, &app.id)
    }
}

/// What a launch of `app` opens `module` with. The Card runner opens the app
/// the launcher row names — a system app by its `os.*` manifest id, an
/// installed one by its own. Any other module opens with what
/// MAKEPAD_APP_CONFIG gives it (`{"module_open": {"<id>": {...}}}`, or
/// `mail_endpoint` for a linked `mail` module), else empty.
pub fn module_open(module: &'static dyn AppModule, app: &crate::clients::AppDef) -> Result<makepad_app_module::ValidatedOpen, String> {
    let schema = module.open_schema();
    if module.id() == "card" {
        let manifest_id = card_manifest_id(app).ok_or_else(|| format!("{} names no app for the card runner", app.id))?;
        return schema.validate(&format!("{{\"app\":{}}}", makepad_strict_json::Value::Str(manifest_id.into()).to_json()), &[]);
    }
    let config = std::env::var("MAKEPAD_APP_CONFIG").ok().and_then(|text| makepad_strict_json::parse(text.as_bytes()).ok());
    if let Some(json) = config.as_ref().and_then(|c| c.get("module_open")).and_then(|v| v.get(module.id())).map(|v| v.to_json()) {
        return schema.validate(&json, &[]);
    }
    if module.id() == "mail" {
        if let Some(endpoint) = config.as_ref().and_then(|c| c.get("mail_endpoint")).and_then(|v| v.as_str()) {
            return schema.validate(&format!("{{\"endpoint\":{}}}", makepad_strict_json::Value::Str(endpoint.into()).to_json()), &[]);
        }
    }
    schema.empty_open()
}

/// An installed host has no checkout catalog. Its linked modules, the system
/// apps and the installed apps carry all the information needed to populate
/// the launcher without filesystem paths.
pub fn bundled_catalog() -> Vec<crate::clients::AppDef> {
    let mut catalog = bundled_modules_catalog();
    catalog.extend(card_apps());
    catalog.retain(|app| catalog_visible(&app.id));
    catalog
}

/// Whether an id may be a launcher row: the `card` host is internal (the
/// apps it runs are the rows), and the retired empty `appstore` stays out.
pub fn catalog_visible(id: &str) -> bool {
    !matches!(id, "card" | "appstore")
}

/// Registry rows a shell surface launches but no list shows as an app:
/// `aichat` is the assistant pane's own process (F10), which the pane
/// starts (`clients::find_app("aichat")`); the dock and the launcher do not.
pub const PANE_ONLY: &[&str] = &["aichat"];

/// Whether a registry row is listed to the person and the system agent:
/// the launcher, the dock, the phone's home, the `os` app list.
pub fn listed(id: &str) -> bool {
    catalog_visible(id) && !PANE_ONLY.contains(&id)
}

/// The linked modules as launcher rows.
pub fn bundled_modules_catalog() -> Vec<crate::clients::AppDef> {
    linked_modules()
        .iter()
        .filter(|module| catalog_visible(module.id()))
        .map(|module| crate::clients::AppDef {
            id: module.id().into(),
            label: module.label().into(),
            bin: module.id().into(),
            package: String::new(),
            dir: String::new(),
            manifest: None,
            args: Vec::new(),
            policy: if module.id() == "reference" {
                crate::clients::LaunchPolicy::AlwaysNew
            } else {
                crate::clients::LaunchPolicy::OrFocus
            },
            target_dir: None,
        })
        .collect()
}

/// The launcher checks the selected host, not merely whether a module is linked.
pub fn is_launchable(app: &crate::clients::AppDef) -> bool {
    let args: Vec<String> = std::env::args().collect();
    let registry = AppRegistry::load(&crate::theme::makepad_home().join("wm/apps.splash"), &args);
    match registry.hosting(&app.id) {
        Hosting::Module => registry.module(&app.id).is_some(),
        Hosting::Process => crate::host::processes_available() && app.is_available(),
    }
}

/// A linked native app's hosting when the person has not switched it: what
/// its `native-apps.json` entry declares for this target. A declared process
/// runs in-process instead where it has no process form to start
/// (`process_form`: no checkout to `cargo run` it from and no sibling binary;
/// release packages do not ship process apps' binaries yet, OctoSense #94),
/// and `process-if-vulkan` only outside a Vulkan build in a Wayland session.
pub fn manifest_default(declared: crate::native_apps::Hosting, process_form: impl FnOnce() -> bool, vulkan_wayland: bool) -> Hosting {
    use crate::native_apps::Hosting as Declared;
    let wants_process = match declared {
        // `None` is a process-only app's where it cannot run; nothing links
        // such an app, so it never reaches here as a module.
        Declared::Module | Declared::None => false,
        Declared::Process => true,
        Declared::ProcessIfVulkan => vulkan_wayland,
    };
    if wants_process && process_form() { Hosting::Process } else { Hosting::Module }
}

/// Whether `id` can start as a process here: its launcher row resolves to a
/// checkout `cargo run` builds it from, or to a binary beside this one.
pub fn process_form(id: &str) -> bool {
    crate::clients::find_app(id).is_some_and(|app| app.is_available())
}

/// Whether the Terminal runs as its own process on this device (ADR 0004
/// §10, §12, G12): a device with processes, the Terminal's hosting resolved
/// to a process (its manifest's `process`, or `process-if-vulkan` on a
/// Vulkan build in a Wayland session, unless the person switched it to a
/// module) and a process form to start. Only then does `terminal.run`
/// exist: the in-process Terminal offers its read tools only.
pub fn terminal_runs_as_process() -> bool {
    let args: Vec<String> = std::env::args().collect();
    let registry = AppRegistry::load(&crate::theme::makepad_home().join("wm/apps.splash"), &args);
    runs_as_process(registry.hosting(TERMINAL), crate::host::processes_available(), process_form(TERMINAL))
}

/// The Terminal's app id.
pub const TERMINAL: &str = "terminal";

/// [`terminal_runs_as_process`] for a given hosting, device and process form.
pub fn runs_as_process(hosting: Hosting, processes: bool, process_form: bool) -> bool {
    hosting == Hosting::Process && processes && process_form
}

/// A Makepad Vulkan build (`MAKEPAD=vulkan`, crates/shell/build.rs) running
/// in a Wayland session: where Linux shares a child's frames zero-copy
/// (DMA-BUF). Vulkan windowing panics on X11.
pub fn vulkan_wayland() -> bool {
    cfg!(all(target_os = "linux", makepad_vulkan)) && std::env::var_os("WAYLAND_DISPLAY").is_some()
}

impl AppRegistry {
    /// The registry with the person's overrides: the settings file first,
    /// then the command line's `--module <id>` flags on top.
    pub fn load(settings: &Path, args: &[String]) -> Self {
        let mut registry = Self::default();
        if let Ok(text) = std::fs::read_to_string(settings) {
            for (id, hosting) in Self::parse_overrides(&text) {
                registry.overrides.insert(id, hosting);
            }
        }
        let mut i = 0;
        while i < args.len() {
            if args[i] == "--module" {
                if let Some(id) = args.get(i + 1) {
                    registry.overrides.insert(id.to_lowercase(), Hosting::Module);
                }
                i += 2;
            } else {
                i += 1;
            }
        }
        registry
    }

    /// The linked module for an app, if this build has one. A system or
    /// installed app has none of its own: the `card` module hosts it.
    pub fn module(&self, id: &str) -> Option<&'static dyn AppModule> {
        if let Some(module) = self.modules.iter().copied().find(|m| m.id() == id) {
            return Some(module);
        }
        if is_card_app(id) {
            return self.modules.iter().copied().find(|m| m.id() == "card");
        }
        None
    }

    /// How a launch of `id` is hosted. On a desktop: a linked native app as
    /// the person switched it, else as `native-apps.json` says for this
    /// target ([`manifest_default`]); any other linked module is a Module
    /// only when the person (or the dev flag) asked for it. In a build
    /// without processes (mobile/web): every linked module is a module, and
    /// everything else is simply not there.
    pub fn hosting(&self, id: &str) -> Hosting {
        if !crate::host::processes_available() {
            return if self.module(id).is_some() { Hosting::Module } else { Hosting::Process };
        }
        // Neither the store nor Settings has a process form.
        if matches!(id, "apphub" | "settings") && self.module(id).is_some() {
            return Hosting::Module;
        }
        if self.modules.iter().any(|m| m.id() == id) && !self.overrides.contains_key(id) {
            if let Some(app) = crate::native_apps::find(id) {
                return manifest_default(app.on_this_target(), || process_form(id), vulkan_wayland());
            }
        }
        // A system or installed app has no process form anywhere: the `card`
        // module hosts it on every platform, no switch needed.
        if self.modules.iter().any(|m| m.id() == "card") && !self.modules.iter().any(|m| m.id() == id) && is_card_app(id) {
            return Hosting::Module;
        }
        match self.overrides.get(id) {
            Some(Hosting::Module) if self.module(id).is_some() => Hosting::Module,
            _ => Hosting::Process,
        }
    }

    /// Whether the assistant is the aichat MODULE seated in the pane
    /// in-process (feature `app-aichat`): always where there are no
    /// processes; on a desktop only when `aichat` is switched to module
    /// hosting, the child process being the default.
    pub fn pane_in_process(&self) -> bool {
        if !cfg!(feature = "app-aichat") {
            return false;
        }
        !crate::host::processes_available() || self.overrides.get("aichat") == Some(&Hosting::Module)
    }

    pub fn linked_ids(&self) -> Vec<&'static str> {
        self.modules.iter().map(|m| m.id()).collect()
    }

    /// `~/.makepad/wm/apps.splash`: one `id: Module` or `id: Process` per
    /// line, optionally inside `{ }`, commas and `//` comments allowed —
    /// the same shape as the theme files, small enough to read without
    /// the VM.
    pub fn parse_overrides(text: &str) -> Vec<(String, Hosting)> {
        let mut out = Vec::new();
        for raw in text.lines() {
            let line = raw.split("//").next().unwrap_or("").trim().trim_matches(|c| c == '{' || c == '}' || c == ',').trim();
            if line.is_empty() {
                continue;
            }
            let Some((id, hosting)) = line.split_once(':') else { continue };
            let hosting = match hosting.trim().trim_matches(',').trim().to_lowercase().as_str() {
                "module" => Hosting::Module,
                "process" => Hosting::Process,
                _ => continue,
            };
            out.push((id.trim().to_lowercase(), hosting));
        }
        out
    }
}

/// An installed store app's bundle under App Hub's apps root. App Hub keeps
/// it outside the app's storage, in `<root>/.bundles/<id>/bundle`
/// (`octosense_app_hub::installed_bundle_dir`); a device whose App Hub has
/// not moved it yet still has it in `<root>/<id>/bundle`. The new place
/// wins when both exist.
pub(crate) fn installed_bundle(root: &Path, app_id: &str) -> PathBuf {
    let current = root.join(".bundles").join(app_id).join("bundle");
    if current.is_dir() {
        current
    } else {
        root.join(app_id).join("bundle")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn script_agent_discovery_requires_matching_identity_and_genuine_opt_in() {
        let root = std::env::temp_dir().join(format!("shell-agent-discovery-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("manifest.json");
        let app = "org.example.notes";
        for manifest in [
            serde_json::json!({"id":app,"capabilities":[],"agent":{}}),
            serde_json::json!({"id":app,"capabilities":["octos.session.open"]}),
        ] {
            std::fs::write(&path, manifest.to_string()).unwrap();
            assert!(script_agent_app(&path, app, "Notes").is_some());
            assert!(script_agent_app(&path, "org.example.other", "Other").is_none());
        }
        for manifest in [
            serde_json::json!({"id":app,"capabilities":[],"agent":null}),
            serde_json::json!({"id":app,"capabilities":["octos.unrestricted"]}),
        ] {
            std::fs::write(&path, manifest.to_string()).unwrap();
            assert!(script_agent_app(&path, app, "Notes").is_none());
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The assistant pane's own process is a registry row (F10 starts it
    /// from there) but no list shows it as an app; the apps people pick
    /// are listed.
    #[test]
    fn the_panes_own_process_is_never_listed_as_an_app() {
        assert!(!listed("aichat"));
        assert!(!listed("card") && !listed("appstore"));
        assert!(listed("notes") && listed("terminal") && listed("os.mail"));
        assert!(crate::shell::launcher::apps().iter().all(|a| a.id != "apps.aichat" && a.id != "aichat"));
    }

    /// App Hub keeps an installed bundle in `<root>/.bundles/<id>/bundle`
    /// (outside the app's storage); a device App Hub has not updated yet
    /// still has it in `<root>/<id>/bundle`. Either is found, the new one
    /// first.
    #[test]
    fn an_installed_bundle_is_found_in_either_layout() {
        let root = std::env::temp_dir().join(format!("shell-installed-bundle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let legacy = root.join("dev.example.app/bundle");
        std::fs::create_dir_all(&legacy).unwrap();
        assert_eq!(installed_bundle(&root, "dev.example.app"), legacy);
        let current = root.join(".bundles/dev.example.app/bundle");
        std::fs::create_dir_all(&current).unwrap();
        assert_eq!(installed_bundle(&root, "dev.example.app"), current, "the new layout wins");
        assert_eq!(installed_bundle(&root, "dev.example.none"), root.join("dev.example.none/bundle"));
        let _ = std::fs::remove_dir_all(&root);
    }
    use super::*;

    /// ADR 0004 §3: no script app takes a native app's id or namespace.
    /// App Hub's `RESERVED_NAMES` must name every native app this shell
    /// ships, so its gate refuses them before a device sees one.
    #[test]
    fn a_script_app_may_not_take_a_native_apps_id_or_namespace() {
        for app in crate::native_apps::APPS {
            assert!(check_script_app_id(app.id).is_err(), "{}", app.id);
            assert!(check_script_app_id(&format!("com.example.{}", app.id)).is_err(), "{}", app.id);
            #[cfg(any(feature = "app-hub", native_mobile))]
            assert!(octosense_app_policy::RESERVED_NAMES.contains(&app.id), "App Hub's RESERVED_NAMES lacks the native app {}", app.id);
        }
        for ok in ["os.news", "org.example.timer", "dev.example.news", "org.example.terminal-notes"] {
            assert_eq!(check_script_app_id(ok), Ok(()), "{ok}");
        }
        #[cfg(any(feature = "app-hub", native_mobile))]
        for host in ["system", "toolbox", "com.example.dev"] {
            assert!(check_script_app_id(host).is_err(), "{host}");
        }
    }

    /// The system apps this build's system-apps.json selects, in its order:
    /// the desktop and the phone pack different sets (no Camera on the
    /// desktop), chosen by `OCTOSENSE_SYSTEM_APPS` in `.cargo/config.toml`.
    #[cfg(any(feature = "app-hub", native_mobile))]
    fn system_app_ids() -> Vec<&'static str> {
        let text = include_str!(env!("OCTOSENSE_SYSTEM_APPS"));
        let json: serde_json::Value = serde_json::from_str(text).expect("system-apps.json parses");
        json["apps"].as_array().expect("system-apps.json has apps").iter()
            .map(|id| &*Box::leak(id.as_str().expect("app ids are strings").to_owned().into_boxed_str()))
            .collect()
    }

    /// The build's own rows among `rows`: without the system apps other
    /// tests registered, or the store apps they installed, for themselves
    /// ([`super::test_system_apps`]).
    /// `rows` must be read before this looks, as an argument is.
    fn build_rows(rows: Vec<crate::clients::AppDef>) -> Vec<crate::clients::AppDef> {
        rows.into_iter().filter(|row| !card_manifest_id(row).is_some_and(super::test_system_apps::is_test_app)).collect()
    }

    #[cfg(feature = "mobile-apps")]
    #[test]
    fn bundled_apps_open_without_catalog_files_or_child_processes() {
        use makepad_widgets::*;
        if crate::module_host::run_with_isolated_module_data("apps::tests::bundled_apps_open_without_catalog_files_or_child_processes") { return; }
        let _one_rinx = crate::module_host::RINX_INSTANCE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let catalog = build_rows(bundled_catalog());
        // The linked modules in link order (AppCard is opt-in, `app-appcard`,
        // not part of `mobile-apps`; `settings` is the phone product's), then
        // the system apps no native module of the same id replaces.
        let linked = linked_modules();
        let native: Vec<&str> = linked.iter().map(|m| m.id()).filter(|id| catalog_visible(id)).collect();
        let mut expected: Vec<&str> = native.clone();
        expected.extend(system_app_ids().iter().copied().filter(|id| !native.contains(id)));
        assert_eq!(catalog.iter().map(|app| app.id.as_str()).collect::<Vec<_>>(), expected);
        assert_eq!(native.contains(&"appcard"), cfg!(feature = "app-appcard"));
        assert_eq!(native.contains(&"rinx"), cfg!(feature = "app-rinx"));
        assert!(catalog.iter().all(|app| app.manifest.is_none()));
        // The system apps without a native module: the Card runner hosts
        // them, launched by their manifest id (ADR 0004).
        let registry = AppRegistry::default();
        for id in system_app_ids().iter().filter(|id| !native.contains(id)) {
            let app = catalog.iter().find(|app| app.id == *id).unwrap();
            assert_eq!(card_manifest_id(app), Some(format!("os.{id}").as_str()));
            assert_eq!(registry.module(id).map(|m| m.id()), Some("card"));
            assert_eq!(registry.hosting(id), Hosting::Module, "{id} has no process form");
        }
        let catalog: Vec<_> = catalog.into_iter().filter(|app| app.bin != "card").collect();
        assert_eq!(catalog.iter().find(|app| app.id == "reference").unwrap().policy, crate::clients::LaunchPolicy::AlwaysNew);
        if let Some(rinx) = catalog.iter().find(|app| app.id == "rinx") {
            assert_eq!(rinx.policy, crate::clients::LaunchPolicy::OrFocus);
        }
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut host = crate::module_host::ModuleHost::default();
        host.apply_style(&mut cx, &desktop_style::StyleSheet::load(desktop_style::DesktopStyle::Android));
        for (index, app) in catalog.iter().enumerate() {
            let module = registry.module(&app.id).unwrap();
            let client = index as u64 + 1;
            host.create(&mut cx, client, module, module.open_schema().empty_open().unwrap(), dvec2(400.0, 700.0)).unwrap();
            let instance = host.get(client).unwrap();
            assert!(!instance.root.is_empty(), "{} must provide a real view", app.id);
            cx.with_script_vm_id_trusted(instance.vm_id, |vm| {
                assert!(script_eval!(vm, {mod.theme.font_regular.font_family.latin.res}).as_handle().is_some(),
                        "{} must have the Android font resource", app.id);
                assert!(script_eval!(vm, {mod.res}).is_nil(), "resource loading stays restricted after registration");
                assert!(vm.take_errors().is_empty(), "{} must initialize without script errors", app.id);
            });
            assert!(host.teardown(&mut cx, client));
        }
    }

    #[test]
    fn bundled_apps_receive_same_base_theme_without_recreation() {
        use crate::mobile_theme::{Preset, Selection};
        use makepad_widgets::*;
        if crate::module_host::run_with_isolated_module_data("apps::tests::bundled_apps_receive_same_base_theme_without_recreation") { return; }
        let _one_rinx = crate::module_host::RINX_INSTANCE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let registry = AppRegistry::default();
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut host = crate::module_host::ModuleHost::default();
        for (index, app) in build_rows(bundled_catalog()).iter().enumerate() {
            let module = registry.module(&app.id).unwrap();
            let client = index as u64 + 1;
            host.create(&mut cx, client, module, module_open(module, app).unwrap(), dvec2(400.0, 700.0)).unwrap();
            let uid = host.get(client).unwrap().root.widget_uid();
            for (preset, dark) in [(Preset::Paper, true), (Preset::Vivid, false)] {
                let choice = Selection { preset, ..Default::default() };
                host.apply_style(&mut cx, &choice.sheet(crate::desktop::DesktopStyle::Android, dark));
                let instance = host.get(client).unwrap();
                assert_eq!(instance.root.widget_uid(), uid, "{} must retain its instance", app.id);
                cx.with_script_vm_id_trusted(instance.vm_id, |vm| {
                    let theme = vm.module(id!(theme));
                    let p = choice.palette(dark);
                    for (role, color) in [("color_bg_app", p.background), ("color_text", p.text), ("color_focus", p.accent)] {
                        let rgba = (((color.x * 255.0).round() as u32) << 24) | (((color.y * 255.0).round() as u32) << 16) | (((color.z * 255.0).round() as u32) << 8) | 255;
                        assert_eq!(vm.bx.heap.value(theme, LiveId::from_str(role).into(), NoTrap).as_color(), Some(rgba), "{} {role}", app.id);
                    }
                    assert!(vm.take_errors().is_empty(), "{} must accept a shared theme", app.id);
                });
            }
            assert!(host.teardown(&mut cx, client));
        }
    }

    #[test]
    fn installed_card_identity_never_focuses_a_builtin_with_the_same_name() {
        let mut app = card_row("news".into(), "News".into(), Vec::new());
        app.bin = "news".into();
        assert!(matches_running_app(&app, "news", "News"));
        assert!(!matches_running_app(&app, "hub:news", "News"));
        app.id = installed_launch_id("news");
        app.bin = "card".into();
        assert_eq!(card_manifest_id(&app), Some("news"));
        assert!(matches_running_app(&app, "hub:news", "News"));
        assert!(!matches_running_app(&app, "news", "News"));
        assert!(!matches_running_app(&app, "hub:news-other", "News"));
        // A system app is opened by the manifest id its row carries.
        let system = card_row("mail".into(), "Mail".into(), vec![format!("{SYSTEM_ARG}os.mail")]);
        assert_eq!(card_manifest_id(&system), Some("os.mail"));
        assert!(!matches_running_app(&system, "mailer", "Mail"), "a Card app focuses only itself");
    }

    /// The system apps this build packs (system-apps.json): each a Card app
    /// under its short id unless a native module of that id is linked, and
    /// each bundle registered with the runner.
    #[cfg(any(feature = "app-hub", native_mobile))]
    #[test]
    fn the_system_apps_ship_as_card_apps() {
        let registry = AppRegistry::default();
        let native = registry.linked_ids();
        let ids: Vec<String> = build_rows(system_card_apps()).into_iter().map(|app| app.id).collect();
        let expected: Vec<&str> = system_app_ids().iter().copied().filter(|id| !native.contains(id)).collect();
        assert_eq!(ids, expected);
        for id in &ids {
            assert_eq!(registry.hosting(id), Hosting::Module);
            assert!(is_linked(id));
        }
        assert_eq!(registry.hosting("apphub"), Hosting::Module, "the store has no process form");
        if system_app_ids().contains(&"camera") {
            assert!(octosense_app_hub_app::system_icon("camera").is_some(), "Camera ships its own icon");
        }
    }

    /// native-apps.json is the one declaration: every native module this
    /// build links is an entry under its own id (App Hub's runner `card`
    /// rides on `apphub`), linked by the entry's feature, and the host
    /// ships exactly the `octos.*` grants the entries record.
    #[test]
    fn the_linked_native_modules_are_the_manifests() {
        let mut linked = Vec::new();
        crate::native_apps::link(&mut linked);
        for module in &linked {
            let id = if module.id() == "card" { "apphub" } else { module.id() };
            let app = crate::native_apps::find(id).unwrap_or_else(|| panic!("{id} is not in native-apps.json"));
            assert!(app.feature.starts_with("app-"), "{id} is linked by an app-* feature");
        }
        let shipped = octosense_ai_host::Policy::shipped();
        for app in crate::native_apps::APPS {
            let granted: Vec<&str> = shipped
                .grants()
                .find(|(module, _)| *module == app.id)
                .map(|(_, services)| services.iter().map(String::as_str).collect())
                .unwrap_or_default();
            assert_eq!(granted, app.octos, "{}'s agent grants", app.id);
        }
    }

    /// G12: `terminal.run` has a target only where the Terminal runs as a
    /// process on this device.
    #[test]
    fn the_terminal_runs_as_a_process_only_with_processes_a_process_hosting_and_a_process_form() {
        assert!(runs_as_process(Hosting::Process, true, true));
        assert!(!runs_as_process(Hosting::Module, true, true), "in-process (Linux without Vulkan, switched to a module)");
        assert!(!runs_as_process(Hosting::Process, false, true), "no processes (phones, wasm)");
        assert!(!runs_as_process(Hosting::Process, true, false), "no binary to start");
        // process-if-vulkan resolves to a module without Vulkan and Wayland.
        use crate::native_apps::Hosting as Declared;
        assert!(!runs_as_process(manifest_default(Declared::ProcessIfVulkan, || true, false), true, true));
        assert!(runs_as_process(manifest_default(Declared::ProcessIfVulkan, || true, true), true, true));
    }

    /// A declared process runs in-process where it cannot start one;
    /// `process-if-vulkan` is a process only on Vulkan with Wayland.
    #[test]
    fn manifest_hosting_falls_back_in_process() {
        use crate::native_apps::Hosting as Declared;
        assert_eq!(manifest_default(Declared::Module, || panic!("a module never asks"), true), Hosting::Module);
        assert_eq!(manifest_default(Declared::Process, || true, false), Hosting::Process);
        assert_eq!(manifest_default(Declared::Process, || false, true), Hosting::Module, "no binary: in-process");
        assert_eq!(manifest_default(Declared::ProcessIfVulkan, || true, false), Hosting::Module, "OpenGL or X11");
        assert_eq!(manifest_default(Declared::ProcessIfVulkan, || true, true), Hosting::Process);
        // Every app but the Terminal stays in-process everywhere (ADR 0004 §2).
        for id in ["apphub", "rinx", "sheets", "reference", "appcard"] {
            let app = crate::native_apps::find(id).unwrap();
            assert_eq!(manifest_default(app.on_this_target(), || true, true), Hosting::Module, "{id}");
        }
    }

    #[test]
    fn overrides_parse_the_settings_shape_and_ignore_noise() {
        let text = "// which apps run in-process\n{\n  sheets: Module,\n  Terminal: process\n  files: Sideways\n  nonsense\n}\n";
        assert_eq!(
            AppRegistry::parse_overrides(text),
            vec![("sheets".to_string(), Hosting::Module), ("terminal".to_string(), Hosting::Process)]
        );
    }

    #[test]
    fn hosting_is_process_unless_a_linked_module_is_switched_on() {
        let registry = AppRegistry::load(Path::new("/nonexistent/apps.splash"), &["--module".to_string(), "sheets".to_string(), "--module".to_string(), "files".to_string()]);
        // files has no linked module: the flag cannot make it one.
        assert_eq!(registry.hosting("files"), Hosting::Process);
        #[cfg(not(feature = "app-terminal"))]
        assert_eq!(registry.hosting("terminal"), Hosting::Process, "no linked terminal: a process");
        #[cfg(feature = "app-sheets")]
        {
            assert_eq!(registry.hosting("sheets"), Hosting::Module);
            assert!(registry.linked_ids().contains(&"sheets"));
            let plain = AppRegistry::default();
            assert_eq!(plain.hosting("sheets"), Hosting::Module, "Terminal is the only process app for now (ADR 0004 §2)");
        }
    }

    /// Terminal is a system app: linked, it is hosted as native-apps.json
    /// says (its own process on macOS and Windows where it can start one,
    /// in-process on phones), and a person who switched it keeps that.
    #[cfg(feature = "app-terminal")]
    #[test]
    fn the_linked_terminal_is_hosted_as_the_manifest_says() {
        use crate::native_apps::Hosting as Declared;
        let plain = AppRegistry::default();
        assert!(plain.linked_ids().contains(&"terminal"));
        let app = crate::native_apps::find("terminal").expect("Terminal is in native-apps.json");
        assert_eq!((app.macos, app.windows, app.linux), (Declared::Process, Declared::Process, Declared::ProcessIfVulkan));
        let expected = if !crate::host::processes_available() {
            Hosting::Module
        } else {
            manifest_default(app.on_this_target(), || process_form("terminal"), vulkan_wayland())
        };
        assert_eq!(plain.hosting("terminal"), expected);
        if cfg!(target_os = "macos") && process_form("terminal") {
            assert_eq!(plain.hosting("terminal"), Hosting::Process, "a process on macOS");
        }
        for switch in [Hosting::Module, Hosting::Process] {
            let mut switched = AppRegistry::default();
            switched.overrides.insert("terminal".into(), switch);
            if crate::host::processes_available() {
                assert_eq!(switched.hosting("terminal"), switch, "the person's switch wins");
            }
        }
        let term: &'static dyn AppModule = &makepad_terminal::TERMINAL_MODULE;
        assert!(module_open(term, &card_row("terminal".into(), "Terminal".into(), Vec::new())).is_ok());
    }

    #[test]
    fn rinx_is_module_hosted_on_the_desktop_by_default() {
        let plain = AppRegistry::default();
        if plain.module("rinx").is_some() {
            assert_eq!(plain.hosting("rinx"), Hosting::Module, "Rinx ships only as a linked module");
        }
    }

    #[test]
    fn module_open_names_the_card_app_and_opens_others_empty() {
        #[cfg(any(feature = "app-hub", native_mobile))]
        {
            let card: &'static dyn AppModule = &octosense_app_hub_app::CARD_MODULE;
            let system = card_row("mail".into(), "Mail".into(), vec![format!("{SYSTEM_ARG}os.mail")]);
            assert!(module_open(card, &system).is_ok());
            let nameless = card_row("mail".into(), "Mail".into(), Vec::new());
            assert!(module_open(card, &nameless).is_err(), "a card row names its app");
        }
        #[cfg(any(feature = "app-sheets", native_mobile))]
        {
            let sheets: &'static dyn AppModule = &makepad_sheets::SHEETS_MODULE;
            let row = card_row("sheets".into(), "Sheets".into(), Vec::new());
            assert!(module_open(sheets, &row).is_ok());
        }
    }

    #[cfg(any(feature = "app-hub", native_mobile))]
    #[test]
    fn installed_apps_are_read_once_per_data_root_and_hub_generation() {
        let reads = std::cell::Cell::new(0);
        let timer = card_row(installed_launch_id("org.example.timer"), "Timer".into(), Vec::new());
        let load = || {
            reads.set(reads.get() + 1);
            vec![timer.clone()]
        };
        let root = std::path::PathBuf::from("hub-root-a");
        assert_eq!(cached_installed_apps((root.clone(), 7), &load)[0].id, "hub:org.example.timer");
        assert_eq!(cached_installed_apps((root.clone(), 7), &load)[0].id, "hub:org.example.timer");
        assert_eq!(reads.get(), 1, "the same data root and generation reuse the list");
        cached_installed_apps((root, 8), &load);
        assert_eq!(reads.get(), 2, "an install or update bumps the generation and is read at once");
        cached_installed_apps(("hub-root-b".into(), 8), &load);
        assert_eq!(reads.get(), 3, "another data root is read");
    }
}

#[cfg(all(test, feature = "app-appcard"))]
mod appcard_isolate_tests {
    /// The app's cards are Splash widgets, each in an ISOLATE that is minted
    /// without the framework's `sys`/`agent` engine; the AppCard module must
    /// therefore install it as an isolate mod when it registers. Without it a
    /// card body fails with "variable sys not found in scope" and the tile
    /// draws nothing — silently, since the live Splash keeps its previous view.
    #[test]
    fn appcard_isolates_carry_the_sys_engine_after_register() {
        use makepad_widgets::*;
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        cx.with_vm(|vm| makepad_app_module::AppModule::register(&octosense_appcard::APPCARD_MODULE, vm));
        let mini = "let x = sys.geocodenum(\"Cupertino\", \"lat\")\nView{ Label{ text: \"lat=\" + x } }";
        assert_eq!(makepad_widgets::splash::validate_splash_body(&mut cx, mini, true), Vec::<String>::new());
    }
}
