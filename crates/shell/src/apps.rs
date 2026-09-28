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

/// The services contained apps call through `host.request` (ADR 0004)
/// that are not the assistant's, registered once, before the first system
/// app can open: `mail` keeps accounts and passwords for the Mail app;
/// `glance` takes the cards apps publish to the glance screen (glance.rs).
/// `mail_demo` in MAKEPAD_APP_CONFIG serves a demo mailbox from a file vault
/// instead (no keychain, no network): `MAKEPAD_APP_CONFIG='{"mail_demo":true}'`.
/// `news` fetches News's feeds on a timer, with no model (ADR 0002), into
/// the Card runner's host directory, so it keeps fetching while News is
/// closed.
///
/// The `llm` service (AI providers, `os.ai-providers`) is the assistant's
/// and registers with the kernel in `ai_host::start`, at startup, with the
/// `model` service (`model.complete`, ADR 0002) over the same providers.
#[cfg(any(feature = "app-hub", native_mobile))]
fn register_host_services() {
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
        register_news();
    });
}

/// The `news` service, in the directory the Card runner hands every host
/// service (`<apps root>/.host`), so the timer starts now rather than at
/// News's first request. Without an apps root yet it attaches at that
/// request instead.
#[cfg(any(feature = "app-hub", native_mobile))]
fn register_news() {
    let mut options = octosense_news_service::Options::default().on_fetch(|report| {
        // M3 routes this to News's peer, to wake its agent; logged for now.
        let failed = report.sources.iter().filter(|s| s.status == "error").count();
        makepad_widgets::log!(
            "news: fetched {} new, {} kept, {} sources ({failed} failed)",
            report.new,
            report.total,
            report.sources.len()
        );
    });
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
        Declared::Module => false,
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[cfg(feature = "mobile-apps")]
    #[test]
    fn bundled_apps_open_without_catalog_files_or_child_processes() {
        use makepad_widgets::*;
        let _one_rinx = crate::module_host::RINX_INSTANCE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let catalog = bundled_catalog();
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
        let _one_rinx = crate::module_host::RINX_INSTANCE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let registry = AppRegistry::default();
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(makepad_widgets::script_mod);
        let mut host = crate::module_host::ModuleHost::default();
        for (index, app) in bundled_catalog().iter().enumerate() {
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
        let ids: Vec<String> = system_card_apps().into_iter().map(|app| app.id).collect();
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
        // App Hub and Rinx stay in-process everywhere (ADR 0004 §2).
        for id in ["apphub", "rinx"] {
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
            let expected = if process_form("sheets") { Hosting::Process } else { Hosting::Module };
            assert_eq!(plain.hosting("sheets"), expected, "desktop default is a process where it can start");
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
