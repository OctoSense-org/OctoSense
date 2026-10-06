//! OctoSense Home, the phone package: the shell (crates/shell,
//! `octosense_shell`) plus the built-in Settings app (settings_*.rs) and its
//! Android channels (android_settings.rs).
//!
//! The Settings code is `impl App` blocks over this package's `App`, which
//! wraps the shell's (`#[deref] shell`): shell fields and methods read
//! through. The shell's modules are re-exported at this crate's root, so
//! `crate::module_host`, `crate::mobile_back`, … name the shell's.

#![allow(dead_code)]

// The shell's names win over makepad's where both have one (`DragState`).
#[allow(ambiguous_glob_reexports)]
pub use octosense_shell::*;
use octosense_shell::App as ShellApp;

mod android_settings;
mod android_mail;
mod settings_app;
mod settings_script;
mod settings_script_bridge;
mod settings_script_host_facade;
#[cfg(test)] mod settings_script_caption_tests;
#[cfg(test)] mod settings_script_domain_tests;
mod settings_script_consent_bridge;
#[cfg(test)] mod settings_script_controls_tests;
#[cfg(test)] mod settings_script_core_tests;
#[cfg(test)] mod settings_script_apps_tests;
#[cfg(test)] mod settings_script_editors_tests;
#[cfg(test)] mod settings_script_wireless_tests;
#[cfg(test)] mod settings_script_media_tests;
#[cfg(test)] mod settings_script_consent_tests;
mod settings_entry;
mod settings_accessibility;
mod settings_accessibility_host;
mod settings_apps;
mod settings_apps_host;
mod settings_wifi;
mod settings_wifi_host;
mod settings_hearing;
mod settings_caption_custom;
mod settings_caption_custom_host;
mod settings_keyboards;
mod settings_keyboards_host;
mod settings_system_language;
mod settings_system_language_host;
mod settings_caption_language;
mod settings_caption_language_host;
mod settings_controls;
mod settings_text_interaction;
mod settings_controls_host;
mod settings_bluetooth;
mod settings_bluetooth_host;
mod settings_accounts;
mod settings_accounts_host;
// Retained only as a migration oracle for script search ranking.
#[cfg(test)] mod settings_search;
mod settings_updates;
mod settings_updates_host;
mod settings_network;
mod settings_app_notifications;
mod settings_app_notifications_host;
mod settings_app_language;
mod settings_app_language_host;
mod settings_app_storage;
mod settings_app_storage_host;
mod settings_app_battery;
mod settings_app_battery_host;
mod settings_app_network;
mod settings_app_network_host;
mod settings_dnd;
mod settings_dnd_host;
mod settings_permissions;
mod settings_permissions_host;
mod settings_roles;
mod settings_roles_host;
mod settings_display;
mod settings_display_host;
mod settings_datetime;
mod settings_notifications;
mod settings_notifications_host;
mod settings_sounds;
mod settings_sounds_host;
mod settings_network_host;
mod settings_host;

use makepad_widgets::*;
use makepad_app_module::AppModule;
use settings_host::SettingsRuntime;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    startup() do #(App::script_component(vm)){
        ui: mod.widgets.OctoSenseRoot{}
    }
}

/// The shell's `App` and the Settings app's state.
#[derive(Script, ScriptHook)]
pub struct App {
    #[deref]
    shell: ShellApp,
    #[rust]
    settings_runtime: SettingsRuntime,
}

/// Settings is linked in every phone build.
fn linked_modules() -> Vec<&'static dyn AppModule> {
    vec![&crate::settings_app::SETTINGS_MODULE]
}

/// Privilege derives from the compiled singleton, never a script-supplied
/// module id or a self-declared capability string.
fn trusted_module(module: &dyn AppModule) -> bool {
    crate::settings_app::trusted(module)
}

/// Tells the shell what only this package knows (before the shell starts;
/// tests that host Settings call it too).
pub(crate) fn install_ext() {
    octosense_shell::ext::install(octosense_shell::ext::Ext { linked_modules, trusted_module });
}

thread_local! {
    /// `App::modules_launched` when Settings last observed it.
    static SEEN_LAUNCHES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        install_ext();
        ShellApp::shell_script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        // Before the shell: the shell queues the Android packets it does not
        // name for this product (android_settings.rs), from startup on;
        // Settings' timers and activity state; a Settings entry intent
        // (which may arrive before the shell is up) is Settings' alone; and
        // a Settings entry in flight is cancelled by Android's Home intent
        // (the shell then goes home).
        if matches!(event, Event::Startup) {
            crate::android_settings::install();
        }
        self.settings_tick(cx, event);
        if self.settings_android_early(cx, event) {
            return;
        }
        if matches!(event, Event::HomeIntent) {
            self.settings_runtime.entries.cancel();
        }
        self.shell.shell_handle_event(cx, event);
        crate::android_mail::event(self, cx, event);
        // After the shell: the Android packets it queued for Settings.
        self.settings_android_packets(cx);
        // After the shell: Settings' requests to the host, attributed by the
        // posting root; a module launched this event (Settings re-reads what
        // it shows); a Settings entry opened from outside (Android intent);
        // the accessibility tree Android reads.
        if let Event::Actions(actions) = event {
            for action in actions {
                let Some(wa) = action.as_widget_action() else { continue };
                if let Some(request) = wa.action.downcast_ref::<crate::settings_app::SettingsRequest>() {
                    self.settings_request(cx, wa.widget_uid, request.clone());
                }
            }
        }
        if SEEN_LAUNCHES.with(|seen| seen.replace(self.modules_launched)) != self.modules_launched {
            self.refresh_settings_app(cx);
        }
        if matches!(event, Event::Startup | Event::Resume) {
            self.settings_entry_ready(cx);
        }
        self.settings_entry_tick(cx);
        self.settings_accessibility_publish(cx, event);
    }
}

octosense_shell::octosense_main!();
