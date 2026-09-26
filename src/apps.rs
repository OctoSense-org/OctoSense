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
    linked_modules().iter().any(|m| m.id() == id) || is_system_app(id)
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
    #[cfg(any(feature = "app-photos", target_os = "android", target_os = "ios"))]
    out.push(&makepad_photos::PHOTOS_MODULE);
    #[cfg(any(feature = "app-appcard", target_os = "android", target_os = "ios"))]
    out.push(&octosense_appcard::APPCARD_MODULE);
    #[cfg(feature = "app-rinx")]
    out.push(&rinx::module::RINX_MODULE);
    // The Card runner: not an app a person opens, but the host every system
    // app (and, later, every installed card app) runs in.
    #[cfg(feature = "system-apps")]
    out.push(&octosense_app_hub_app::CARD_MODULE);
    out
}

/// A system app's launcher row carries its manifest id as this argument.
const SYSTEM_ARG: &str = "--system=";

/// The manifest id a `card` launcher row opens (`os.news`).
pub fn card_manifest_id(app: &crate::clients::AppDef) -> Option<&str> {
    if app.bin != "card" {
        return None;
    }
    app.args.iter().find_map(|a| a.strip_prefix(SYSTEM_ARG))
}

/// System apps (OctoSense ROM ADR 0004): first-party apps shipped as
/// contained script bundles (`system-apps.json`), each run by the Card
/// runner in its own isolate under its manifest's policy. Each keeps its
/// short launcher id (`news` for `os.news`), so its icon, dock place and
/// window matching are the ones a native app of that name would have. On
/// the desktop a system app takes its id over a catalog row of the same
/// name (Makepad's example Mail and Photos), since it has no process form.
pub fn system_card_apps() -> &'static [crate::clients::AppDef] {
    static APPS: std::sync::OnceLock<Vec<crate::clients::AppDef>> = std::sync::OnceLock::new();
    APPS.get_or_init(|| {
        #[cfg(feature = "system-apps")]
        {
            register_host_services();
            return octosense_app_hub_app::system_apps()
                .into_iter()
                .filter_map(|app| {
                    let short = app.id.strip_prefix("os.")?;
                    Some(crate::clients::AppDef {
                        id: short.into(),
                        label: app.name.into(),
                        bin: "card".into(),
                        package: String::new(),
                        dir: String::new(),
                        manifest: None,
                        args: vec![format!("{SYSTEM_ARG}{}", app.id)],
                        policy: crate::clients::LaunchPolicy::OrFocus,
                        target_dir: None,
                    })
                })
                .collect();
        }
        #[allow(unreachable_code)]
        Vec::new()
    })
}

/// Whether `id` (a short launcher id) is a system app this build ships.
pub fn is_system_app(id: &str) -> bool {
    system_card_apps().iter().any(|app| app.id == id)
}

/// The services contained apps call through `host.request` (ADR 0004),
/// registered once, before the first system app can open. A new service is
/// one line here plus its app in `system-apps.json`.
///
/// - `mail`: Mail's accounts, sign-in sheet and passwords (the macOS
///   keychain; `OCTOSENSE_MAIL_VAULT=file` keeps them in an owner-only file
///   under the host directory instead). `mail_demo` in MAKEPAD_APP_CONFIG
///   serves a demo mailbox, as in OctoSense ROM Home.
#[cfg(feature = "system-apps")]
fn register_host_services() {
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

/// An installed host has no checkout catalog. Its linked modules carry all
/// the information needed to populate the launcher without filesystem paths.
pub fn bundled_catalog() -> Vec<crate::clients::AppDef> {
    // The `card` host module is not an app a person opens; the apps it runs are.
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

    /// The linked module for an app, if this build has one. A system app's
    /// module is the Card runner that hosts it.
    pub fn module(&self, id: &str) -> Option<&'static dyn AppModule> {
        let id = if is_system_app(id) { "card" } else { id };
        self.modules.iter().copied().find(|m| m.id() == id)
    }

    /// How a launch of `id` is hosted. On a desktop: Module only when a
    /// module is linked AND the person (or the dev flag) asked for it. In a
    /// build without processes (mobile/web): every linked module is a module,
    /// and everything else is simply not there.
    pub fn hosting(&self, id: &str) -> Hosting {
        // A system app is a script bundle: it has no process form anywhere,
        // so the Card runner hosts it in-process with no switch needed.
        if is_system_app(id) && self.module(id).is_some() {
            return Hosting::Module;
        }
        if !crate::host::processes_available() {
            return if self.module(id).is_some() { Hosting::Module } else { Hosting::Process };
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
                   ["reference", "sheets", "photos", "appcard"]);
        assert!(catalog.iter().all(|app| app.manifest.is_none()));
        assert_eq!(catalog[0].policy, crate::clients::LaunchPolicy::AlwaysNew);
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut host = crate::module_host::ModuleHost::default();
        host.apply_style(&mut cx, &desktop_style::StyleSheet::load(desktop_style::DesktopStyle::Android));
        for (index, app) in catalog.iter().enumerate() {
            // The linked module itself: a system app of the same id (Photos)
            // would route the launcher to the Card runner instead.
            let module = linked_modules()
                .into_iter()
                .find(|m| m.id() == app.id)
                .unwrap();
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

    /// The system apps system-apps.json selects are launcher rows under
    /// their short ids, hosted by the Card runner with their `os.` id and no
    /// process form (OctoSense ROM ADR 0004).
    #[cfg(feature = "system-apps")]
    #[test]
    fn system_apps_are_card_rows_under_short_ids() {
        let system = system_card_apps();
        assert!(system.iter().any(|app| app.id == "news"), "{system:?}");
        let registry = AppRegistry::default();
        for app in system {
            let manifest_id = card_manifest_id(app).unwrap();
            assert_eq!(manifest_id.strip_prefix("os."), Some(app.id.as_str()));
            assert!(is_linked(&app.id));
            assert_eq!(registry.hosting(&app.id), Hosting::Module);
            assert_eq!(registry.module(&app.id).map(|m| m.id()), Some("card"));
        }
        // The runner itself is no app a person opens.
        assert!(bundled_catalog().iter().all(|app| app.id != "card"));
        assert!(crate::clients::registry()
            .iter()
            .any(|app| app.id == "news" && app.bin == "card"));
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
