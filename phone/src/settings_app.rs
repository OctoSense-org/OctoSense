//! Settings application: bundled Octoscript controller and layout, finite native bindings.
use crate::settings_keyboards::{KeyboardRequest,KeyboardSnapshot,KeyboardRead};
use crate::settings_system_language::{SystemLanguageRequest,SystemLanguageSnapshot,SystemLanguageRead};
use crate::settings_caption_language::{CaptionLanguageRequest,CaptionLanguageSnapshot,CaptionLanguageRead};
use crate::settings_caption_custom::{CaptionRequest,CaptionSnapshot};
use crate::settings_app_language::{AppLanguageRequest,AppLanguageSnapshot,LanguageRead};
use crate::settings_app_storage::{AppStorageRequest,AppStorageSnapshot};
use crate::settings_app_battery::{AppBatteryRequest,AppBatterySnapshot};
use crate::settings_app_network::{AppNetworkRequest,AppNetworkSnapshot,AppNetworkRead};
use crate::settings_dnd::{DndRequest,DndSnapshot,DndRead};
use crate::settings_permissions::{PermissionsRequest,PermissionsSnapshot,PermissionsRead};
use crate::settings_roles::{RolesRequest,RolesSnapshot,RolesRead};
use crate::settings_app_notifications::{AppNotificationsRequest,AppNotificationsSnapshot,AppNotificationsRead};
use crate::settings_display::{DisplayRequest,DisplaySnapshot};
use crate::settings_sounds::{SoundsRequest,SoundsSnapshot,SoundsRead};
use crate::settings_notifications::{HistoryRead,HistorySnapshot};
use crate::settings_datetime::{self,TimeRequest,TimeSnapshot};
use crate::settings_network::{NetworkRequest,NetworkSnapshot};
use crate::settings_updates::{UpdatesRequest,UpdatesSnapshot};
use crate::settings_accounts::{AccountDetails,AccountsRead,AccountsRequest,AccountsSnapshot};
use crate::mobile_theme::Selection;
use crate::settings_apps::{self,AppAction,AppDetails,AppTarget,AppsCatalog};
use crate::settings_bluetooth::{BluetoothRequest,BluetoothSnapshot,BluetoothTarget};
use crate::settings_controls::{ControlsPage,ControlsRequest,ControlsSnapshot};
use crate::settings_wifi::{WifiRequest,WifiSnapshot,WifiTarget};
use makepad_strict_json::Value;
use makepad_app_module::{
    makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult},
    AppModule, ExecOutcome, InstanceHandles, InstanceParts, OpenSchema, ServiceExecutor, ValidatedOpen,
};
use makepad_widgets::*;
use std::{collections::HashSet, sync::{atomic::AtomicU8, OnceLock}};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    Wifi, Bluetooth, Mobile, Hotspot, Vpn, Display, DisplayAccess, Sound,
    Dnd, Notifications, System, Apps, DefaultApps, Security, Accounts, Storage, Battery,
    Accessibility, Captions, Languages, Keyboards, About, All, Location, Privacy,
}
impl Destination {
    pub fn id(self) -> &'static str {
        match self {
            Self::Keyboards=>"keyboards",Self::Languages=>"languages",Self::Captions=>"captions", Self::Wifi => "wifi", Self::Bluetooth => "bluetooth", Self::Mobile => "mobile",
            Self::Hotspot => "hotspot", Self::Vpn => "vpn", Self::Display => "display",
            Self::DisplayAccess => "brightness", Self::Sound => "sound", Self::Dnd => "dnd",
            Self::Notifications => "notifications_settings", Self::System => "system", Self::Apps => "apps", Self::DefaultApps=>"default_apps",
            Self::Security => "security", Self::Accounts => "accounts", Self::Storage => "storage",
            Self::Location => "location", Self::Privacy => "privacy",
            Self::Battery => "battery", Self::Accessibility => "accessibility", Self::About => "about", Self::All => "all",
        }
    }
}

/// No raw command, intent, settings key, path or module ID is accepted.
#[derive(Clone, Debug)]
pub enum SettingsRequest {
    Keyboard(KeyboardRequest),
    SystemLanguages(SystemLanguageRequest),
    CaptionLanguage(CaptionLanguageRequest),
    AppLanguage(AppLanguageRequest),
    AppStorage(AppStorageRequest),
    AppBattery(AppBatteryRequest),
    AppNetwork(AppNetworkRequest),
    Display(DisplayRequest), Dnd(DndRequest),
    Back, Theme(Selection), Brightness { value: f64, automatic: bool },
    Rotation(bool), DoNotDisturb(bool), Open(Destination),
    AppNotifications(AppNotificationsRequest), Roles(RolesRequest), Permissions(PermissionsRequest),
    Device(DeviceSetting), DeviceAccess, DateTime(TimeRequest), NotificationHistory(HistoryRead), Sounds(SoundsRequest),
    AppsCatalog { query: String, include_system: bool, offset: u32, generation: Option<String> },
    AppDetails { target: AppTarget, permission_offset: u32 },
    AppEntryDetails { entry_id:i64, package:String },
    AppAction { target: AppTarget, action: AppAction }, AppsUsageAccess,
    CaptionCustom(CaptionRequest), Wifi(WifiRequest), Controls(ControlsRequest), Bluetooth(BluetoothRequest), Accounts(AccountsRequest), Updates(UpdatesRequest), Network(NetworkRequest),
}
impl SettingsRequest {
    pub fn valid(&self) -> bool {
        match self {
            Self::Brightness { value, .. } => value.is_finite() && (0.0..=1.0).contains(value),
            Self::Device(setting) => setting.valid(),
            Self::DateTime(request) => request.action.valid(),
            Self::NotificationHistory(request) => request.valid(),
            Self::Sounds(SoundsRequest::Snapshot(request))=>request.valid(),
            Self::Controls(request) => request.valid(),
            Self::CaptionCustom(request)=>request.valid(),
            Self::Keyboard(request)=>request.valid(),
            Self::SystemLanguages(request)=>request.valid(),
            Self::CaptionLanguage(request)=>request.valid(),
            Self::Bluetooth(request) => request.valid(),
            Self::Network(request) => request.valid(),
            Self::Display(request) => request.valid(),
            Self::AppNotifications(request)=>request.valid(),
            Self::Roles(request)=>request.valid(),
            Self::Permissions(request)=>request.valid(),
            Self::AppLanguage(request)=>request.valid(),
            Self::AppStorage(request)=>request.valid(),
            Self::AppBattery(request)=>request.valid(),
            Self::AppNetwork(request)=>request.valid(),
            Self::Dnd(request)=>request.valid(),
            Self::AppsCatalog { query, offset, generation, .. } => settings_apps::valid_query(query)
                && settings_apps::valid_offset(*offset) && (*offset == 0 || generation.is_some())
                && generation.as_ref().is_none_or(|value| settings_apps::valid_generation(value)),
            Self::AppDetails { permission_offset, .. } => settings_apps::valid_offset(*permission_offset),
            Self::AppEntryDetails {entry_id,package}=>*entry_id>0&&settings_apps::valid_package(package),
            _ => true,
        }
    }
    pub fn capability(&self) -> Option<&'static str> {
        match self { Self::Brightness { .. } => Some("brightness"), Self::Rotation(_) => Some("rotation"),
            Self::DoNotDisturb(_) => Some("dnd"), _ => None }
    }
}

pub const TIMEOUTS: [i64; 6] = [15_000, 30_000, 60_000, 120_000, 300_000, 600_000];
pub const FONT_SCALES: [f64; 5] = [0.85, 1.0, 1.15, 1.3, 1.5];

/// These variants are the entire additional settings vocabulary. In particular,
/// callers cannot supply a SettingsProvider table, key, or arbitrary JSON value.
#[derive(Clone, Debug)]
pub enum DeviceSetting {
    ScreenTimeout(i64), FontScale(f64), TouchSounds(bool), HapticFeedback(bool),
    HourFormat(bool), AutoTime(bool), AutoTimeZone(bool),
    MediaVolume(f64), AlarmVolume(f64), RingVolume(f64), NotificationVolume(f64),
}
impl DeviceSetting {
    pub const KEYS: [&'static str; 11] = ["screen_timeout_ms", "font_scale", "touch_sounds", "haptic_feedback",
        "hour_format", "auto_time", "auto_time_zone", "volume_media", "volume_alarm", "volume_ring", "volume_notification"];
    pub fn key(&self) -> &'static str { match self {
        Self::ScreenTimeout(_) => "screen_timeout_ms", Self::FontScale(_) => "font_scale",
        Self::TouchSounds(_) => "touch_sounds", Self::HapticFeedback(_) => "haptic_feedback",
        Self::HourFormat(_) => "hour_format", Self::AutoTime(_) => "auto_time", Self::AutoTimeZone(_) => "auto_time_zone",
        Self::MediaVolume(_) => "volume_media", Self::AlarmVolume(_) => "volume_alarm", Self::RingVolume(_) => "volume_ring", Self::NotificationVolume(_) => "volume_notification",
    } }
    pub fn value(&self) -> Value { match self {
        Self::ScreenTimeout(value) => Value::Int(*value),
        Self::FontScale(value) | Self::MediaVolume(value) | Self::AlarmVolume(value) | Self::RingVolume(value) | Self::NotificationVolume(value) => Value::F64(*value),
        Self::TouchSounds(value) | Self::HapticFeedback(value) | Self::HourFormat(value) | Self::AutoTime(value) | Self::AutoTimeZone(value) => Value::Bool(*value),
    } }
    pub fn valid(&self) -> bool { match self {
        Self::ScreenTimeout(value) => TIMEOUTS.contains(value),
        Self::FontScale(value) => FONT_SCALES.contains(value),
        Self::MediaVolume(value) | Self::AlarmVolume(value) | Self::RingVolume(value) | Self::NotificationVolume(value) => value.is_finite() && (0.0..=1.0).contains(value),
        _ => true,
    } }
}

#[derive(serde::Serialize, Clone, Debug, Default)]
pub struct DeviceSnapshot {
    pub request_id: i64,
    pub capabilities: HashSet<String>,
    pub can_request_write_settings: Option<bool>,
    pub screen_timeout_ms: Option<i64>, pub max_screen_timeout_ms: Option<i64>, pub font_scale: Option<f64>,
    pub touch_sounds: Option<bool>, pub haptic_feedback: Option<bool>, pub hour_format: Option<bool>,
    pub auto_time: Option<bool>, pub auto_time_zone: Option<bool>,
    pub volume_media: Option<f64>, pub volume_alarm: Option<f64>, pub volume_ring: Option<f64>, pub volume_notification: Option<f64>,
    pub manufacturer: Option<String>, pub model: Option<String>, pub android_version: Option<String>, pub api_level: Option<i64>,
    pub security_patch: Option<String>, pub build: Option<String>, pub kernel: Option<String>,
    pub memory_total: Option<i64>, pub memory_available: Option<i64>,
    pub storage_total: Option<i64>, pub storage_available: Option<i64>,
    pub battery_level: Option<f64>, pub battery_status: Option<String>, pub battery_plugged: Option<String>, pub battery_temperature: Option<f64>,
    pub local_time: Option<String>, pub time_zone: Option<String>, pub locale: Option<String>,
    pub time_controls:Option<TimeSnapshot>,pub time_zones:Vec<String>,
}
fn number(value: &Value) -> Option<f64> { match value { Value::Int(v) => Some(*v as f64), Value::F64(v) if v.is_finite() => Some(*v), _ => None } }
impl DeviceSnapshot {
    pub fn decode(value: &Value) -> Option<Self> {
        if value.get("schema").and_then(Value::as_i64) != Some(1) { return None; }
        let request_id = value.get("request_id")?.as_i64().filter(|id| *id > 0)?;
        let values = value.get("values").filter(|v| matches!(v, Value::Obj(_)))?;
        let caps = value.get("capabilities")?.as_arr()?;
        if caps.iter().any(|cap| cap.as_str().is_none()) { return None; }
        let part = |section: &str, key: &str| value.get(section).and_then(|v| v.get(key));
        let text = |section: &str, key: &str| part(section, key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned);
        let bytes = |section: &str, key: &str| part(section, key).and_then(Value::as_i64).filter(|v| *v >= 0);
        let volume = |key: &str| values.get(key).and_then(number).filter(|v| (0.0..=1.0).contains(v));
        Some(Self {
            request_id,
            capabilities: caps.iter().filter_map(Value::as_str).filter(|key| DeviceSetting::KEYS.contains(key)).map(str::to_owned).collect(),
            can_request_write_settings: value.get("can_request_write_settings").and_then(Value::as_bool),
            screen_timeout_ms: values.get("screen_timeout_ms").and_then(Value::as_i64).filter(|v| *v > 0),
            max_screen_timeout_ms: value.get("max_screen_timeout_ms").and_then(Value::as_i64).filter(|v| *v >= 0),
            font_scale: values.get("font_scale").and_then(number).filter(|v| *v > 0.0),
            touch_sounds: values.get("touch_sounds").and_then(Value::as_bool), haptic_feedback: values.get("haptic_feedback").and_then(Value::as_bool),
            hour_format: values.get("hour_format").and_then(Value::as_bool), auto_time: values.get("auto_time").and_then(Value::as_bool), auto_time_zone: values.get("auto_time_zone").and_then(Value::as_bool),
            volume_media: volume("volume_media"), volume_alarm: volume("volume_alarm"), volume_ring: volume("volume_ring"), volume_notification: volume("volume_notification"),
            manufacturer: text("about", "manufacturer"), model: text("about", "model"), android_version: text("about", "android_version"),
            api_level: part("about", "api_level").and_then(Value::as_i64).filter(|v| *v > 0),
            security_patch: text("about", "security_patch"), build: text("about", "build"), kernel: text("about", "kernel"),
            memory_total: bytes("about", "memory_total"), memory_available: bytes("about", "memory_available"),
            storage_total: bytes("storage", "total_bytes"), storage_available: bytes("storage", "available_bytes"),
            battery_level: part("battery", "level").and_then(number).filter(|v| (0.0..=100.0).contains(v)),
            battery_status: text("battery", "status").filter(|v| ["charging", "discharging", "full", "not_charging", "unknown"].contains(&v.as_str())),
            battery_plugged: text("battery", "plugged").filter(|v| ["ac", "usb", "wireless", "dock", "battery", "unknown"].contains(&v.as_str())),
            battery_temperature: part("battery", "temperature_celsius").and_then(number),
            time_controls:value.get("time_controls").and_then(TimeSnapshot::decode),time_zones:settings_datetime::zones(value.get("time_zones")),
            local_time: text("date_time", "local_time"), time_zone: text("date_time", "time_zone"), locale: text("date_time", "locale"),
        })
    }
    pub fn permits(&self, setting: &DeviceSetting) -> bool {
        setting.valid() && self.capabilities.contains(setting.key()) && match setting {
            DeviceSetting::ScreenTimeout(value) => self.screen_timeout_ms.is_some() && self.max_screen_timeout_ms.is_some_and(|max| max == 0 || *value <= max),
            DeviceSetting::FontScale(_) => self.font_scale.is_some(), DeviceSetting::TouchSounds(_) => self.touch_sounds.is_some(),
            DeviceSetting::HapticFeedback(_) => self.haptic_feedback.is_some(), DeviceSetting::HourFormat(_) => self.hour_format.is_some(),
            DeviceSetting::AutoTime(_) => self.auto_time.is_some(), DeviceSetting::AutoTimeZone(_) => self.auto_time_zone.is_some(),
            DeviceSetting::MediaVolume(_) => self.volume_media.is_some(), DeviceSetting::AlarmVolume(_) => self.volume_alarm.is_some(), DeviceSetting::RingVolume(_) => self.volume_ring.is_some(), DeviceSetting::NotificationVolume(_) => self.volume_notification.is_some(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SettingsSnapshot {
    pub android: bool,
    pub connected: bool,
    pub capabilities: HashSet<String>,
    pub theme: Selection,
    pub brightness: f64,
    pub automatic: bool,
    pub rotation: bool,
    pub dnd: bool,
    pub network: String,
    pub device: Option<DeviceSnapshot>,
    pub font_scale: f64,
    pub apps_catalog: Option<AppsCatalog>,
    pub app_details: Option<AppDetails>,
    pub apps_error: String,
    pub wifi: Option<WifiSnapshot>, pub wifi_error: String,
    pub keyboards:Option<KeyboardSnapshot>,pub keyboards_error:String,
    pub system_language:Option<SystemLanguageSnapshot>,pub system_language_error:String,
    pub caption_language:Option<CaptionLanguageSnapshot>,pub caption_language_error:String,
    pub caption_custom:Option<CaptionSnapshot>,pub caption_custom_error:String,
    pub controls: Option<ControlsSnapshot>, pub controls_error: String,
    pub bluetooth: Option<BluetoothSnapshot>, pub bluetooth_error: String,
    pub accounts: Option<AccountsSnapshot>, pub account_details: Option<AccountDetails>, pub accounts_error:String,
    pub updates:Option<UpdatesSnapshot>,pub updates_error:String,
    pub app_notifications:Option<AppNotificationsSnapshot>,pub app_notifications_error:String,
    pub app_language:Option<AppLanguageSnapshot>,pub app_language_error:String,
    pub app_storage:Option<AppStorageSnapshot>,pub app_storage_error:String,
    pub app_battery:Option<AppBatterySnapshot>,pub app_battery_error:String,
    pub app_network:Option<AppNetworkSnapshot>,pub app_network_error:String,
    pub dnd_settings:Option<DndSnapshot>,pub dnd_error:String,
    pub permissions:Option<PermissionsSnapshot>,pub permissions_error:String,
    pub roles:Option<RolesSnapshot>,pub roles_error:String,
    pub display_options:Option<DisplaySnapshot>,pub display_error:String,
    pub advanced_network:Option<NetworkSnapshot>,pub network_error:String,
    pub sounds:Option<SoundsSnapshot>,pub sounds_loading:bool,pub sounds_error:String,
    pub notification_history:Option<HistorySnapshot>,pub history_loading:bool,pub history_error:String,
}
impl SettingsSnapshot {
    pub fn permits(&self, operation: &str) -> bool {
        self.android && self.connected && self.capabilities.contains(operation)
    }
    pub fn permits_device(&self, setting: &DeviceSetting) -> bool { self.android && self.device.as_ref().is_some_and(|state| state.permits(setting)) }
}

script_mod! {
    use mod.prelude.widgets.*
    mod.widgets.SettingsViewBase = #(SettingsView::register_widget(vm))
}

/// The real Octoscript renderer is used once per bundled source, rather than
/// evaluating / replacing a form whenever a device observation arrives.
fn presentation() -> &'static str {
    static UI: OnceLock<String> = OnceLock::new();
    UI.get_or_init(|| {
        let tree = octoscript_makepad::design::prepare(include_str!("../resources/settings/settings.splash"))
            .expect("bundled Settings Octoscript must evaluate");
        // Only primitive layout, text and buttons: all colors and fonts are
        // inherited from mod.theme, with no process-global Material palette.
        octoscript_makepad::to_makepad_l0_ui(&tree)
    })
}

include!("settings_script_view.rs");

// A non-zero-size singleton gives the host an identity independent of claimed
// ID strings and metadata. Native code is trusted; this is a script boundary.
pub struct SettingsModule { _identity: AtomicU8 }
pub static SETTINGS_MODULE: SettingsModule = SettingsModule { _identity: AtomicU8::new(0) };
pub fn trusted(module: &dyn AppModule) -> bool {
    std::ptr::addr_eq(module as *const dyn AppModule, &SETTINGS_MODULE as *const SettingsModule)
}
impl AppModule for SettingsModule {
    fn id(&self) -> &'static str { "settings" }
    fn label(&self) -> &'static str { "OctoSense Settings" }
    fn register(&self, vm: &mut ScriptVm) {
        script_mod(vm);
        let source = ScriptMod {
            cargo_manifest_path: env!("CARGO_MANIFEST_DIR").into(), module_path: module_path!().into(),
            file: "resources/settings/settings-widgets.splash".into(), line: 0, column: 0, values: Vec::new(),
            code: include_str!("../resources/settings/settings-widgets.splash").replace("__SETTINGS_BODY__",presentation()),
        };
        vm.eval_checked(source, 2_000_000).expect("bundled Settings widget source must evaluate");
    }
    fn open_schema(&self) -> OpenSchema { OpenSchema::new(1) }
    fn capabilities(&self) -> &'static [&'static str] { &["settings"] }
    fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, _handles: InstanceHandles) -> InstanceParts {
        let value = script_eval!(vm, { mod.widgets.SettingsView {} });
        InstanceParts { root: WidgetRef::script_from_value(vm, value), executor: Box::new(SettingsExecutor), shutdown: Box::new(|_| {}) }
    }
}
struct SettingsExecutor;
impl ServiceExecutor for SettingsExecutor {
    fn manifest(&self) -> ServiceManifest { ServiceManifest::new("settings", "Settings", "Phone settings controlled by the user.") }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(ToolResult::unavailable(&call.call_id, "Settings has no agent mutation tools"))
    }
}

#[cfg(test)] #[path="settings_script_view_tests.rs"] mod script_view_tests;
#[cfg(test)] #[path="settings_native_contract_tests.rs"] mod native_contract_tests;
