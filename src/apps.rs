//! The built-in apps registry's hosting dimension (aicontrol.md §4): which
//! apps are linked in as MODULES, and which of those the person has
//! switched to module hosting.
//!
//! The launch table (`clients::registry()`: package, directory, binary,
//! launch policy — everything a PROCESS needs) stays where it is; this is
//! the overlay keyed by the same ids: the linked `AppModule`, and the
//! hosting each app gets. Desktop default is Process (decision 5): a
//! linked module is still launched as a process unless
//! `~/.makepad/wm/apps.splash` says otherwise (a settings file, never an
//! environment variable) or a dev run passes `--module <id>`. The uber
//! builds ignore the switch: everything is a module there.
//!
//! App Hub (feature `app-hub`, on by default) adds the apps its Card runner
//! hosts, the same way OctoSense ROM's Home does: the system apps this build
//! ships as contained script bundles (`os.news`, … ADR 0004) and the apps the
//! person installed from the App Hub catalog (`hub:<manifest-id>`). Neither
//! has a process form; the linked `card` module runs every one of them.

use makepad_app_module::AppModule;
use std::collections::HashMap;
use std::path::Path;

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

/// Mobile includes its bundled modules automatically; desktop opts in with
/// `app-*` features and continues to use process hosting by default.
fn linked_modules() -> Vec<&'static dyn AppModule> {
    #[allow(unused_mut)]
    let mut out: Vec<&'static dyn AppModule> = Vec::new();
    #[cfg(any(feature = "app-reference", target_os = "android", target_os = "ios"))]
    out.push(&octosense_reference::REFERENCE_MODULE);
    #[cfg(any(feature = "app-sheets", target_os = "android", target_os = "ios"))]
    out.push(&makepad_sheets::SHEETS_MODULE);
    // Makepad's native Photos, for comparison only: by default Photos is the
    // system app (`os.photos`), and a linked module of the same id wins.
    #[cfg(feature = "app-photos")]
    out.push(&makepad_photos::PHOTOS_MODULE);
    #[cfg(any(feature = "app-appcard", target_os = "android", target_os = "ios"))]
    out.push(&octosense_appcard::APPCARD_MODULE);
    #[cfg(feature = "app-rinx")]
    out.push(&rinx::module::RINX_MODULE);
    // The trust anchor stays native: the store, and the runner every system
    // and installed app is hosted by.
    #[cfg(feature = "app-hub")]
    {
        out.push(&octosense_app_hub_app::APP_HUB_MODULE);
        out.push(&octosense_app_hub_app::CARD_MODULE);
    }
    out
}

/// Manifest ids live in a separate namespace from built-ins and catalog rows
/// (catalog ids cannot contain a colon).
pub fn installed_launch_id(manifest_id: &str) -> String {
    format!("hub:{manifest_id}")
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
/// keeps its short launcher id (`mail` for `os.mail`), so its icon and dock
/// place are the ones that id always had. They take precedence over catalog
/// rows of the same id (`clients::registry`); a linked native module of the
/// same id wins, for comparison builds (`app-photos`).
pub fn system_card_apps() -> Vec<crate::clients::AppDef> {
    #[cfg(feature = "app-hub")]
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

/// The services contained apps call through `host.request` (ADR 0004): `mail`
/// keeps accounts and passwords for the Mail app. `mail_demo` in
/// MAKEPAD_APP_CONFIG serves a demo mailbox from a file vault instead (no
/// keychain, no network): `MAKEPAD_APP_CONFIG='{"mail_demo":true}'`.
#[cfg(feature = "app-hub")]
pub fn register_host_services() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
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
    });
}

/// Apps App Hub installed: each is an app of its own in the launcher, hosted
/// by the linked `card` module under its `hub:<manifest-id>` identity. Read
/// fresh each time, so an install shows up without a restart.
pub fn installed_card_apps() -> Vec<crate::clients::AppDef> {
    #[cfg(feature = "app-hub")]
    if let Some(root) = octosense_app_hub_app::data_root_if_set() {
        return octosense_app_hub_app::installed_apps(&root)
            .into_iter()
            .map(|app| card_row(installed_launch_id(&app.id), app.name, Vec::new()))
            .collect();
    }
    Vec::new()
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
/// installed one by its own; every other module opens empty.
pub fn module_open(module: &'static dyn AppModule, app: &crate::clients::AppDef) -> Result<makepad_app_module::ValidatedOpen, String> {
    let schema = module.open_schema();
    if module.id() != "card" {
        return schema.empty_open();
    }
    let manifest_id = card_manifest_id(app).ok_or_else(|| format!("{} names no app for the card runner", app.id))?;
    schema.validate(&format!("{{\"app\":{}}}", makepad_strict_json::Value::Str(manifest_id.into()).to_json()), &[])
}

/// An installed host has no checkout catalog. Its linked modules, the system
/// apps and the installed apps carry all the information needed to populate
/// the launcher without filesystem paths.
pub fn bundled_catalog() -> Vec<crate::clients::AppDef> {
    let mut catalog = bundled_modules_catalog();
    catalog.extend(card_apps());
    catalog
}

/// The linked modules as launcher rows. The `card` host module is not an app
/// a person opens; the apps it runs are.
pub fn bundled_modules_catalog() -> Vec<crate::clients::AppDef> {
    linked_modules().iter().filter(|module| module.id() != "card").map(|module| crate::clients::AppDef {
        id: module.id().into(),
        label: module.label().into(),
        bin: module.id().into(),
        package: String::new(),
        dir: String::new(),
        manifest: None,
        args: Vec::new(),
        policy: if module.id() == "reference" { crate::clients::LaunchPolicy::AlwaysNew }
            else { crate::clients::LaunchPolicy::OrFocus },
        target_dir: None,
    }).collect()
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

    /// How a launch of `id` is hosted. On a desktop: Module only when a
    /// module is linked AND the person (or the dev flag) asked for it. In a
    /// build without processes (mobile/web): every linked module is a module,
    /// and everything else is simply not there.
    pub fn hosting(&self, id: &str) -> Hosting {
        if !crate::host::processes_available() {
            return if self.module(id).is_some() { Hosting::Module } else { Hosting::Process };
        }
        // The store has no process form, and neither has a system or
        // installed app: the `card` module hosts them on every platform.
        if id == "apphub" && self.module(id).is_some() {
            return Hosting::Module;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "mobile-apps")]
    #[test]
    fn bundled_apps_open_without_catalog_files_or_child_processes() {
        use makepad_widgets::*;
        let catalog = bundled_catalog();
        assert_eq!(catalog.iter().map(|app| app.id.as_str()).collect::<Vec<_>>(),
                   ["reference", "sheets", "appcard", "apphub", "news", "photos", "maps", "camera", "mail"]);
        assert!(catalog.iter().all(|app| app.manifest.is_none()));
        // The system apps have no native module: the Card runner hosts them,
        // launched by their manifest id (ADR 0004).
        let registry = AppRegistry::default();
        for id in ["news", "photos", "maps", "camera", "mail"] {
            let app = catalog.iter().find(|app| app.id == id).unwrap();
            assert_eq!(card_manifest_id(app), Some(format!("os.{id}").as_str()));
            assert_eq!(registry.module(id).map(|m| m.id()), Some("card"));
            assert_eq!(registry.hosting(id), Hosting::Module, "{id} has no process form");
        }
        let catalog: Vec<_> = catalog.into_iter().filter(|app| app.bin != "card").collect();
        assert_eq!(catalog[0].policy, crate::clients::LaunchPolicy::AlwaysNew);
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
    /// under its short id, and each bundle registered with the runner.
    #[cfg(feature = "app-hub")]
    #[test]
    fn the_system_apps_ship_as_card_apps() {
        let ids: Vec<String> = system_card_apps().into_iter().map(|app| app.id).collect();
        assert_eq!(ids, ["news", "photos", "maps", "camera", "mail"]);
        let registry = AppRegistry::default();
        for id in &ids {
            assert_eq!(registry.hosting(id), Hosting::Module);
            assert!(is_linked(id));
        }
        assert_eq!(registry.hosting("apphub"), Hosting::Module, "the store has no process form");
        assert!(octosense_app_hub_app::system_icon("camera").is_some(), "Camera ships its own icon");
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
        assert_eq!(registry.hosting("terminal"), Hosting::Process);
        #[cfg(feature = "app-sheets")]
        {
            assert_eq!(registry.hosting("sheets"), Hosting::Module);
            assert!(registry.linked_ids().contains(&"sheets"));
            let plain = AppRegistry::default();
            assert_eq!(plain.hosting("sheets"), Hosting::Process, "desktop default is a process");
        }
    }
}

#[cfg(all(test, feature = "mobile-apps"))]
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
