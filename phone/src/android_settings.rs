//! The Home app's Android channels: what the Settings app reads from and
//! is told by the Android side. Phone packaging only (Settings is part of
//! the Home product, not the shell). The shell's `android_integration`
//! names none of these channels; it queues every packet it parsed for the
//! product (`take_product_packets`) once `install()` has run, and the Home
//! app's event loop calls, in this order:
//!
//! 1. `settings_android_early(cx, event)` before the shell's
//!    `android_event` (the entry intent may arrive before the shell is up);
//! 2. `settings_android_packets(cx)` right after it.
use crate::{android_integration, App};
use makepad_strict_json::Value;
use makepad_widgets::*;

/// Tell the shell the Home product is here (idempotent): it queues its
/// Android packets for us and leaves failed-command notices to us.
pub fn install() {
    android_integration::set_product(true);
}

impl App {
    /// Android may deliver the Activity entry before the shell is ready.
    /// Preserve only the finite latest navigation request, never raw
    /// extras. True: the event was the entry intent (consumed).
    pub(crate) fn settings_android_early(&mut self, cx: &mut Cx, event: &Event) -> bool {
        let Event::AndroidIntegration { channel, payload } = event else { return false };
        if channel != "settings.entry" {
            return false;
        }
        if let Ok(value) = makepad_strict_json::parse_depth(payload.as_bytes(), 4) {
            self.settings_entry_received(cx, &value);
        }
        true
    }
    /// The packets the shell queued for the product in this event.
    pub(crate) fn settings_android_packets(&mut self, cx: &mut Cx) {
        for (channel, value) in android_integration::take_product_packets() {
            match channel.as_str() {
                "bridge.result" | "launcher.result" => {
                    if !result(self, cx, &value) {
                        self.android_result_notice(cx, &value);
                    }
                }
                "integration.resync" => self.settings_uncertain(cx),
                "launcher.ui_mode" => ui_mode(self, cx, &value),
                other => { observe(self, cx, other, &value); }
            }
            if matches!(channel.as_str(), "bridge.connection" | "bridge.snapshot" | "launcher.ui_mode") {
                self.refresh_settings_app(cx);
            }
        }
    }
}

fn observe(app: &mut App, cx: &mut Cx, channel: &str, value: &Value) -> bool {
    match channel {
        "launcher.device_settings" => app.settings_device_snapshot(cx, value),
        "launcher.apps_catalog" | "launcher.app_details" => app.settings_apps_observe(cx, channel, value),
        "launcher.updates_state" => app.settings_updates_observe(cx, value),
        "launcher.app_notifications_state" => app.settings_app_notifications_observe(cx, value),
        "launcher.app_language" => app.settings_app_language_observe(cx, value),
        "launcher.app_storage_state" => app.settings_app_storage_observe(cx, value),
        "launcher.app_battery_state" => app.settings_app_battery_observe(cx, value),
        "launcher.app_network_state" => app.settings_app_network_observe(cx, value),
        "launcher.dnd_state" => app.settings_dnd_observe(cx, value),
        "launcher.permissions_state" => app.settings_permissions_observe(cx, value),
        "launcher.roles_state" => app.settings_roles_observe(cx, value),
        "launcher.display_state" => app.settings_display_observe(cx, value),
        "launcher.network_state" => app.settings_network_observe(cx, value),
        "launcher.accounts_state" => app.settings_accounts_observe(cx, value, false),
        "launcher.account_details" => app.settings_accounts_observe(cx, value, true),
        "launcher.bluetooth_state" => app.settings_bluetooth_observe(cx, value),
        "launcher.keyboards_state" => app.settings_keyboards_observe(cx, value),
        "launcher.system_languages_state" => app.settings_system_language_observe(cx, value),
        "launcher.caption_language" => app.settings_caption_language_observe(cx, value),
        "launcher.caption_custom_state" => app.settings_caption_custom_observe(cx, value),
        "launcher.controls_state" => app.settings_controls_observe(cx, value),
        "launcher.notification_history" => app.settings_history_observe(cx, value),
        "launcher.sounds_state" => app.settings_sounds_observe(cx, value),
        "launcher.wifi_state" => app.settings_wifi_observe(cx, value),
        "settings.a11y.action" => app.settings_accessibility_action(cx, value),
        "settings.a11y.enabled" => app.settings_accessibility_enabled(cx, value.get("enabled").and_then(Value::as_bool)),
        _ => return false,
    }
    true
}

/// Read failures stay on their Settings page; background retries must not
/// create a notification every five seconds. False: not one of Settings'
/// own commands (the shell's notice applies).
fn result(app: &mut App, cx: &mut Cx, value: &Value) -> bool {
    if app.settings_apps_result(cx, value) { return true; }
    if app.settings_wifi_result(cx, value) { return true; }
    if app.settings_keyboards_result(cx, value) || app.settings_system_language_result(cx, value) || app.settings_caption_language_result(cx, value) || app.settings_caption_custom_result(cx, value) || app.settings_controls_result(cx, value) { return true; }
    if app.settings_bluetooth_result(cx, value) { return true; }
    if app.settings_accounts_result(cx, value) { return true; }
    if app.settings_updates_result(cx, value) { return true; }
    if app.settings_app_notifications_result(cx, value) { return true; }
    if app.settings_roles_result(cx, value) { return true; }
    if app.settings_permissions_result(cx, value) { return true; }
    if app.settings_app_language_result(cx, value) { return true; }
    if app.settings_app_storage_result(cx, value) { return true; }
    if app.settings_app_battery_result(cx, value) { return true; }
    if app.settings_app_network_result(cx, value) { return true; }
    if app.settings_dnd_result(cx, value) { return true; }
    if app.settings_display_result(cx, value) { return true; }
    if app.settings_network_result(cx, value) { return true; }
    if app.settings_history_result(cx, value) { return true; }
    if app.settings_sounds_result(cx, value) { return true; }
    app.settings_result(cx, value);
    false
}

fn ui_mode(app: &mut App, cx: &mut Cx, value: &Value) {
    let focused = value.get("activity_focused").and_then(Value::as_bool);
    app.settings_activity_resumed(value.get("activity_resumed").and_then(Value::as_bool));
    app.settings_permissions_focus(cx, focused);
    app.settings_app_language_focus(cx, focused);
    app.settings_keyboards_focus(cx, focused);
    app.settings_system_language_focus(cx, focused);
    app.settings_caption_language_focus(cx, focused);
    app.settings_caption_custom_focus(cx, focused);
    app.settings_app_storage_focus(cx, focused);
    app.settings_app_battery_focus(cx, focused);
    app.settings_app_network_focus(cx, focused);
    app.settings_dnd_focus(cx, focused);
    if value.get("activity_resumed").and_then(Value::as_bool) == Some(true) {
        app.settings_entry_ready(cx);
    }
    app.settings_accessibility_enabled(cx, value.get("accessibility_enabled").and_then(Value::as_bool));
}
