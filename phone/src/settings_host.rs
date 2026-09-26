//! Host-only dispatch. Scripts never receive the Android transport itself.
use crate::{settings_app::{DeviceSnapshot, SettingsRequest, SettingsSnapshot, SettingsView}, App};
use makepad_strict_json::{s, Value};
use crate::settings_wifi::WifiRequest;
use crate::settings_controls::ControlsRequest;
use crate::settings_caption_custom::CaptionRequest;
use crate::settings_caption_language::CaptionLanguageRequest;
use crate::settings_system_language::SystemLanguageRequest;
use crate::settings_keyboards::KeyboardRequest;
use crate::settings_bluetooth::BluetoothRequest;
use crate::settings_accounts::AccountsRequest;
use crate::settings_updates::UpdatesRequest;
use crate::settings_network::NetworkRequest;
use crate::settings_app_notifications::AppNotificationsRequest;
use crate::settings_roles::RolesRequest;
use crate::settings_permissions::PermissionsRequest;
use crate::settings_dnd::DndRequest;
use crate::settings_app_network::AppNetworkRequest;
use crate::settings_app_battery::AppBatteryRequest;
use crate::settings_app_language::AppLanguageRequest;
use crate::settings_app_storage::AppStorageRequest;
use crate::settings_display::{DisplayRequest,DisplayValue};
use makepad_widgets::{widget_async::{enter_isolate, leave_isolate}, *};

#[derive(Default)]
pub struct SettingsRuntime {
    pub(crate) entries:crate::settings_entry::EntryQueue,
    pub(crate) accessibility:crate::settings_accessibility_host::SettingsAccessibilityRuntime,
    pub(crate) history:crate::settings_notifications_host::SettingsHistoryRuntime,
    pub(crate) sounds:crate::settings_sounds_host::SettingsSoundsRuntime,
    pub(crate) apps: crate::settings_apps_host::SettingsAppsRuntime,
    pub(crate) wifi: crate::settings_wifi_host::SettingsWifiRuntime,
    pub(crate) keyboards:crate::settings_keyboards_host::SettingsKeyboardsRuntime,
    pub(crate) system_language:crate::settings_system_language_host::SettingsSystemLanguageRuntime,
    pub(crate) caption_language:crate::settings_caption_language_host::SettingsCaptionLanguageRuntime,
    pub(crate) caption_custom:crate::settings_caption_custom_host::SettingsCaptionCustomRuntime,
    pub(crate) controls: crate::settings_controls_host::SettingsControlsRuntime,
    pub(crate) bluetooth: crate::settings_bluetooth_host::SettingsBluetoothRuntime,
    pub(crate) accounts: crate::settings_accounts_host::SettingsAccountsRuntime,
    pub(crate) updates: crate::settings_updates_host::SettingsUpdatesRuntime,
    pub(crate) display: crate::settings_display_host::SettingsDisplayRuntime,
    pub(crate) app_notifications:crate::settings_app_notifications_host::SettingsAppNotificationsRuntime,
    pub(crate) app_language:crate::settings_app_language_host::SettingsAppLanguageRuntime,
    pub(crate) app_storage:crate::settings_app_storage_host::SettingsAppStorageRuntime,
    pub(crate) app_battery:crate::settings_app_battery_host::SettingsAppBatteryRuntime,
    pub(crate) app_network:crate::settings_app_network_host::SettingsAppNetworkRuntime,
    pub(crate) dnd:crate::settings_dnd_host::SettingsDndRuntime,
    pub(crate) permissions:crate::settings_permissions_host::SettingsPermissionsRuntime,
    pub(crate) roles:crate::settings_roles_host::SettingsRolesRuntime,
    pub(crate) network: crate::settings_network_host::SettingsNetworkRuntime,
    pending: Option<Pending>,
    read: Option<Pending>,
    device: Option<DeviceSnapshot>,
    next_read: f64,
    last_observed: i64,
    resumed: bool,
    active: Option<(crate::hub::ClientId, WidgetUid)>,
}
#[derive(Clone, Copy)]
struct Pending { id: i64, client: crate::hub::ClientId, root: WidgetUid, deadline: f64, device: bool }

impl SettingsRuntime {
    fn activity_resumed(&mut self, observed: Option<bool>) {
        // Java can observe the first Activity resume before the native app is
        // initialized. A typed observation repairs that missed event; absence
        // never implies foreground permission to start polling.
        if let Some(resumed) = observed {
            self.resumed = resumed;
            if resumed { self.next_read = 0.0; }
        }
    }
    fn snapshot_owner(&mut self, id: i64) -> Option<Pending> {
        // A mutation emits its observation before completion. It must not
        // consume a different read or allow that older read to roll state back.
        let owner = if self.read.is_some_and(|read| read.id == id) {
            self.read.take()
        } else { self.pending.filter(|pending| pending.device && pending.id == id) };
        owner.filter(|_| id > self.last_observed)
    }
}

impl App {
    pub(crate) fn settings_entry_ready(&self,cx:&mut Cx) {
        if self.state.is_some() {cx.android_integration("settings.entry.ready",r#"{"schema":1}"#);}
    }
    pub(crate) fn settings_entry_received(&mut self,cx:&mut Cx,value:&Value) {
        let Some(entry)=crate::settings_entry::SettingsEntry::decode(value)else{return;};
        let changed=self.settings_runtime.entries.receive(value);
        // Receipt acknowledges retention/deduplication, not successful settings
        // mutation. Java retains its latest intent through native bootstrap.
        cx.android_integration("settings.entry.received",&makepad_strict_json::obj(vec![("schema",Value::Int(1)),("id",Value::Int(entry.id))]).to_json());
        if changed {self.settings_entry_tick(cx);cx.redraw_all();}
    }
    pub(crate) fn settings_entry_tick(&mut self,cx:&mut Cx) {
        if self.state.is_none()||!self.settings_runtime.resumed{return;}
        let Some(request)=self.settings_runtime.entries.pending()else{return;};
        if self.module_host.settings_instance().is_none() {
            // Use compiled catalog metadata and the trusted singleton, never an installed card's id.
            let Some(app)=crate::apps::bundled_modules_catalog().into_iter().find(|app|app.id=="settings")else{return;};
            self.launch_module_as(cx,&crate::settings_app::SETTINGS_MODULE,&app);
        }
        let Some(instance)=self.module_host.settings_instance()else{return;};
        // Never match a caller-supplied app id/title or a script impostor.
        let(client,root,vm_id)=(instance.client,instance.root.clone(),instance.vm_id);
        if !self.state.as_ref().unwrap().clients.get(&client).is_some_and(|slot|slot.closing.is_none()){return;}
        self.state_mut().phone.shade.close();self.state_mut().phone.groups.close();
        self.activate_client(cx,client);
        if request.package.is_some(){
            self.settings_runtime.apps.clear_entry_observations();
            self.settings_runtime.app_notifications=Default::default();
            self.settings_runtime.permissions=Default::default();
        }
        let isolate=enter_isolate(cx,vm_id);
        let applied=if let Some(mut view)=root.borrow_mut::<SettingsView>() {
            if let Some(package)=&request.package{view.navigate_notification_entry(cx,request.id,package);}else{view.navigate_entry(cx,request.route);}true
        }else{false};
        leave_isolate(cx,isolate);
        if applied {
            self.settings_runtime.entries.complete(request.id);
            self.settings_runtime.accessibility.invalidate();self.redraw_all(cx);
        }
    }
    fn settings_snapshot(&self) -> SettingsSnapshot {
        let Some(state) = &self.state else { return SettingsSnapshot::default(); };
        let phone = &state.phone;
        SettingsSnapshot {
            android: cfg!(target_os = "android"), connected: phone.android.connected,
            capabilities: phone.android.capabilities.as_ref().clone(), theme: phone.theme.unwrap_or_default(),
            brightness: phone.shade.brightness, automatic: phone.android.brightness_automatic,
            rotation: phone.shade.rotation_lock, dnd: phone.shade.do_not_disturb,
            network: phone.shade.network_summary.clone(),
            device: self.settings_runtime.device.clone(), font_scale: phone.android.font_scale,
            apps_catalog: self.settings_runtime.apps.catalog.clone(), app_details: self.settings_runtime.apps.details(),
            apps_error: self.settings_runtime.apps.error.clone(),
            wifi: self.settings_runtime.wifi.snapshot(), wifi_error: self.settings_runtime.wifi.error.clone(),
            keyboards:self.settings_runtime.keyboards.snapshot(),keyboards_error:self.settings_runtime.keyboards.error.clone(),
            system_language:self.settings_runtime.system_language.snapshot(),system_language_error:self.settings_runtime.system_language.error.clone(),
            caption_language:self.settings_runtime.caption_language.snapshot(),caption_language_error:self.settings_runtime.caption_language.error.clone(),
            caption_custom:self.settings_runtime.caption_custom.snapshot(),caption_custom_error:self.settings_runtime.caption_custom.error.clone(),
            controls: self.settings_runtime.controls.snapshot(), controls_error: self.settings_runtime.controls.error.clone(),
            bluetooth: self.settings_runtime.bluetooth.snapshot(), bluetooth_error: self.settings_runtime.bluetooth.error.clone(),
            accounts: self.settings_runtime.accounts.snapshot(), account_details: self.settings_runtime.accounts.details(), accounts_error: self.settings_runtime.accounts.error.clone(),
            updates: self.settings_runtime.updates.snapshot(), updates_error: self.settings_runtime.updates.error.clone(),
            app_notifications:self.settings_runtime.app_notifications.snapshot(),app_notifications_error:self.settings_runtime.app_notifications.error.clone(),
            app_language:self.settings_runtime.app_language.snapshot(),app_language_error:self.settings_runtime.app_language.error.clone(),
            app_storage:self.settings_runtime.app_storage.snapshot(),app_storage_error:self.settings_runtime.app_storage.error.clone(),
            app_battery:self.settings_runtime.app_battery.snapshot(),app_battery_error:self.settings_runtime.app_battery.error.clone(),
            app_network:self.settings_runtime.app_network.snapshot(),app_network_error:self.settings_runtime.app_network.error.clone(),
            dnd_settings:self.settings_runtime.dnd.snapshot(),dnd_error:self.settings_runtime.dnd.error.clone(),
            permissions:self.settings_runtime.permissions.snapshot(),permissions_error:self.settings_runtime.permissions.error.clone(),
            roles:self.settings_runtime.roles.snapshot(),roles_error:self.settings_runtime.roles.error.clone(),
            display_options:self.settings_runtime.display.snapshot(),display_error:self.settings_runtime.display.error.clone(),
            advanced_network: self.settings_runtime.network.snapshot(), network_error: self.settings_runtime.network.error.clone(),
            sounds:self.settings_runtime.sounds.snapshot.clone(),sounds_loading:self.settings_runtime.sounds.loading(),sounds_error:self.settings_runtime.sounds.error.clone(),
            notification_history:self.settings_runtime.history.snapshot.clone(),history_loading:self.settings_runtime.history.loading(),history_error:self.settings_runtime.history.error.clone(),
        }
    }
    pub(crate) fn refresh_settings_app(&mut self, cx: &mut Cx) {
        let state = self.settings_snapshot();
        let Some(instance) = self.module_host.settings_instance() else { return; };
        let (root, vm_id) = (instance.root.clone(), instance.vm_id);
        let entry = enter_isolate(cx, vm_id);
        if let Some(mut view) = root.borrow_mut::<SettingsView>() { view.observe(cx, state); }
        leave_isolate(cx, entry);
    }
    fn settings_outcome(&mut self, cx: &mut Cx, client: crate::hub::ClientId, pending: bool, message: &str) {
        let Some(instance) = self.module_host.get(client).filter(|i| crate::settings_app::trusted(i.module)) else { return; };
        let (root, vm_id) = (instance.root.clone(), instance.vm_id);
        let entry = enter_isolate(cx, vm_id);
        if let Some(mut view) = root.borrow_mut::<SettingsView>() { view.outcome(cx, pending, message); }
        leave_isolate(cx, entry);
    }
    pub(crate) fn settings_request(&mut self, cx: &mut Cx, uid: WidgetUid, request: SettingsRequest) {
        let Some(client) = self.module_host.settings_client(uid) else {
            log!("settings: rejected request from an unprivileged or retired root");
            return;
        };
        if !request.valid() { self.settings_outcome(cx, client, false, "Invalid setting value."); return; }
        if self.settings_apps_read_request(cx, (client, uid), &request) {return;}
        if self.settings_wifi_read_request(cx, (client, uid), &request) {return;}
        if self.settings_keyboards_read_request(cx,(client,uid),&request){return;}
        if self.settings_system_language_read_request(cx,(client,uid),&request){return;}
        if self.settings_caption_language_read_request(cx,(client,uid),&request){return;}
        if self.settings_caption_custom_read_request(cx,(client,uid),&request){return;}
        if self.settings_controls_read_request(cx, (client, uid), &request) {return;}
        if self.settings_bluetooth_read_request(cx, (client, uid), &request) {return;}
        if self.settings_accounts_read_request(cx, (client, uid), &request) {return;}
        if self.settings_updates_read_request(cx, (client, uid), &request) {return;}
        if self.settings_app_notifications_read_request(cx,(client,uid),&request){return;}
        if self.settings_roles_read_request(cx,(client,uid),&request){return;}
        if self.settings_app_language_read_request(cx,(client,uid),&request){return;}
        if self.settings_app_storage_read_request(cx,(client,uid),&request){return;}
        if self.settings_app_battery_read_request(cx,(client,uid),&request){return;}
        if self.settings_app_network_read_request(cx,(client,uid),&request){return;}
        if self.settings_permissions_read_request(cx,(client,uid),&request){return;}
        if self.settings_dnd_read_request(cx,(client,uid),&request){return;}
        if self.settings_display_read_request(cx,(client,uid),&request){return;}
        if self.settings_network_read_request(cx, (client, uid), &request) {return;}
        if self.settings_history_read_request(cx, (client, uid), &request) {return;}
        if self.settings_sounds_request(cx,(client,uid),&request){return;}
        if matches!(request, SettingsRequest::Back) {
            if self.state.as_ref().is_some_and(|state| state.style.target.mobile()) {
                self.phone_action(cx, crate::mobile::PhoneHit::Home);
            } else { self.on_wm_request(cx, client, makepad_wm_api::WmRequest::Close); }
            return;
        }
        if self.settings_runtime.pending.is_some_and(|pending| pending.root == uid) { return; }
        let state = self.settings_snapshot();
        if !state.android { self.settings_outcome(cx, client, false, "Device controls are available on Android."); return; }
        if request.capability().is_some_and(|capability| !state.permits(capability)) {
            self.settings_outcome(cx, client, false, "This control needs Android permission or a connected system service.");
            return;
        }
        if let SettingsRequest::Device(setting) = &request {
            if !state.permits_device(setting) {
                self.settings_outcome(cx, client, false, "This setting is unavailable or restricted by Android.");
                return;
            }
        }
        if matches!(request, SettingsRequest::DeviceAccess)
            && !state.device.as_ref().is_some_and(|device| device.can_request_write_settings == Some(true)) {
            self.settings_outcome(cx, client, false, "Android has not offered settings access.");
            return;
        }
        if let SettingsRequest::DateTime(request)=&request {
            if !self.settings_is_foreground((client,uid))||!self.module_host.settings_instance().and_then(|instance|instance.root.borrow::<SettingsView>().map(|view|view.date_time_visible())).unwrap_or(false)
                ||!state.device.as_ref().is_some_and(|device|device.time_controls.as_ref().is_some_and(|time|time.permits(request,&device.time_zones))) {
                self.settings_outcome(cx,client,false,"Time settings changed. Review the current values before saving.");return;
            }
        }
        let device = matches!(request, SettingsRequest::Device(_)|SettingsRequest::DateTime(_));
        if let SettingsRequest::AppAction { target, action } = &request {
            if !self.settings_apps_permits(target,*action) {
                self.settings_outcome(cx,client,false,"Refresh app details before using this action.");return;
            }
        }
        if matches!(request,SettingsRequest::AppsUsageAccess) && !self.settings_apps_usage_permitted() {
            self.settings_outcome(cx,client,false,"Storage access is unavailable for this app.");return;
        }
        if let SettingsRequest::Wifi(request)=&request {
            if !self.settings_wifi_permits((client,uid),request) {
                self.settings_outcome(cx,client,false,"Refresh Wi-Fi information. Android has not authorized this action.");return;
            }
            self.settings_wifi_resync(cx);
        }
        if let SettingsRequest::Keyboard(request)=&request{if !self.settings_keyboards_permits((client,uid),request){self.settings_outcome(cx,client,false,"Keyboard choices changed. Refresh and choose again.");return;}self.settings_keyboards_flow_started(cx);}
        if let SettingsRequest::SystemLanguages(request)=&request{if !self.settings_system_language_permits((client,uid),request){self.settings_outcome(cx,client,false,"System languages changed. Cancel the draft and refresh to review the current list.");return;}self.settings_system_language_selection_started(cx);}
        if let SettingsRequest::CaptionLanguage(request)=&request{if !self.settings_caption_language_permits((client,uid),request){self.settings_outcome(cx,client,false,"Caption languages changed. Refresh and choose again.");return;}self.settings_caption_language_selection_started(cx);}
        if let SettingsRequest::CaptionCustom(request)=&request{if !self.settings_caption_custom_permits((client,uid),request){self.settings_outcome(cx,client,false,"Caption choices changed. Refresh and choose again.");return;}self.settings_caption_custom_resync(cx);}
        if let SettingsRequest::Controls(request)=&request {
            if !self.settings_controls_permits((client,uid),request) {
                self.settings_outcome(cx,client,false,"Refresh this page. Android has not authorized this choice.");return;
            }
            self.settings_controls_resync(cx);
        }
        if let SettingsRequest::Bluetooth(request)=&request {
            if !self.settings_bluetooth_permits((client,uid),request) {
                self.settings_outcome(cx,client,false,"Refresh Bluetooth information. Android has not authorized this action.");return;
            }
            self.settings_bluetooth_resync(cx);
        }
        if let SettingsRequest::Accounts(request)=&request {
            if !self.settings_accounts_permits((client,uid),request) {
                self.settings_outcome(cx,client,false,"Refresh account information. Android has not authorized this action.");return;
            }
            self.settings_accounts_resync(cx);
        }
        if let SettingsRequest::Updates(request)=&request {
            if !self.settings_updates_permits((client,uid),request) {self.settings_outcome(cx,client,false,"Refresh system updates before using this action.");return;}
            self.settings_updates_resync(cx);
        }
        if let SettingsRequest::Dnd(request)=&request {
            if !self.settings_dnd_permits((client,uid),request){self.settings_outcome(cx,client,false,"DND settings changed. Refresh and review this choice again.");return;}
            self.settings_dnd_resync(cx);
        }
        if let SettingsRequest::AppLanguage(language)=&request {
            let permitted=match language {
                AppLanguageRequest::Native(target)=>self.settings_is_foreground((client,uid))
                    &&self.settings_apps_permits(target,crate::settings_apps::AppAction::Language)
                    &&self.module_host.settings_instance().and_then(|instance|instance.root.borrow::<SettingsView>().and_then(|view|view.app_language_read())).is_some_and(|read|read.target==*target),
                _=>self.settings_app_language_permits((client,uid),language),
            };
            if !permitted{self.settings_outcome(cx,client,false,"Language choices changed. Refresh and review this app again.");return;}
            if matches!(language,AppLanguageRequest::Select{..}){self.settings_app_language_selection_started(cx);}else{self.settings_app_language_resync(cx);}
        }
        if let SettingsRequest::AppStorage(request)=&request {
            if !self.settings_app_storage_permits((client,uid),request){self.settings_outcome(cx,client,false,"Storage changed. Refresh and review this app again.");return;}
            self.settings_app_storage_resync(cx);
        }
        if let SettingsRequest::AppBattery(request)=&request {
            if !self.settings_app_battery_permits((client,uid),request){self.settings_outcome(cx,client,false,"Battery policy changed. Refresh and review this app again.");return;}
            self.settings_app_battery_resync(cx);
        }
        if let SettingsRequest::AppNetwork(request)=&request {
            if !self.settings_app_network_permits((client,uid),request){self.settings_outcome(cx,client,false,"Network controls changed. Refresh and review this app again.");return;}
            self.settings_app_network_resync(cx);
        }
        if let SettingsRequest::Permissions(request)=&request {
            if !self.settings_permissions_permits((client,uid),request){self.settings_outcome(cx,client,false,"Permission choices changed. Refresh and choose again.");return;}
            self.settings_permissions_resync(cx);
        }
        if let SettingsRequest::Roles(request)=&request {
            if !self.settings_roles_permits((client,uid),request){self.settings_outcome(cx,client,false,"Default-app choices changed. Refresh and select again.");return;}
            self.settings_roles_resync(cx);
        }
        if let SettingsRequest::AppNotifications(request)=&request {
            if !self.settings_app_notifications_permits((client,uid),request){self.settings_outcome(cx,client,false,"Notification settings changed. Refresh and review the current values.");return;}
            self.settings_app_notifications_resync(cx);
        }
        if let SettingsRequest::Display(request)=&request {
            if !self.settings_display_permits((client,uid),request){self.settings_outcome(cx,client,false,"Refresh display settings before using this action.");return;}
            self.settings_display_resync(cx);
        }
        if let SettingsRequest::Network(request)=&request {
            if !self.settings_network_permits((client,uid),request) {self.settings_outcome(cx,client,false,"Refresh network settings before using this action.");return;}
            self.settings_network_resync(cx);
        }
        let (channel, operation, fields) = match request {
            SettingsRequest::Theme(choice) => ("launcher", "theme_apply", vec![("theme", choice.encode())]),
            SettingsRequest::Brightness { value, automatic } => ("bridge", "brightness", vec![("value", Value::F64(value)), ("automatic", Value::Bool(automatic))]),
            SettingsRequest::Rotation(enabled) => ("bridge", "rotation", vec![("enabled", Value::Bool(enabled))]),
            SettingsRequest::DoNotDisturb(enabled) => ("bridge", "dnd", vec![("enabled", Value::Bool(enabled))]),
            SettingsRequest::Open(destination) => ("launcher", "system_settings", vec![("destination", s(destination.id()))]),
            SettingsRequest::Device(setting) => ("launcher", "device_setting", vec![("setting", s(setting.key())), ("value", setting.value())]),
            SettingsRequest::DateTime(request) => {
                let mut fields=vec![("key",s(request.key.wire())),("action",s(request.action.wire())),("value",s(request.action.value()))];
                if let crate::settings_datetime::TimeAction::Clock{second,..}=request.action {fields.push(("occurrence",s(if second {"second"} else {"first"})));}
                ("launcher","date_time_set",fields)
            },
            SettingsRequest::DeviceAccess => ("launcher", "device_settings_access", vec![]),
            SettingsRequest::AppAction {target,action} => ("launcher","app_action",vec![("package",s(target.package())),("action",s(action.wire()))]),
            SettingsRequest::AppsUsageAccess => ("launcher","apps_usage_access",vec![]),
            SettingsRequest::Wifi(WifiRequest::Enabled(enabled)) => ("launcher","wifi_enabled",vec![("enabled",Value::Bool(enabled))]),
            SettingsRequest::Wifi(WifiRequest::Scan) => ("launcher","wifi_scan",vec![]),
            SettingsRequest::Wifi(WifiRequest::Network{target,action}) => ("launcher","wifi_network",vec![("key",s(target.key())),("action",s(action.wire()))]),
            SettingsRequest::Wifi(WifiRequest::Access) => ("launcher","wifi_access",vec![]),
            SettingsRequest::Keyboard(KeyboardRequest::Snapshot(_))=>unreachable!(),
            SettingsRequest::Keyboard(KeyboardRequest::Flow{key,target,operation})=>("launcher","keyboard_flow",vec![("key",s(key.wire())),("target",s(target.as_ref().map(|k|k.wire()).unwrap_or(""))),("operation",s(operation.wire()))]),
            SettingsRequest::SystemLanguages(SystemLanguageRequest::Snapshot(_))=>unreachable!(),
            SettingsRequest::SystemLanguages(SystemLanguageRequest::Apply{key,order})=>("launcher","system_languages_apply",vec![("key",s(key.wire())),("order",Value::Arr(order.iter().map(|target|s(target.wire())).collect()))]),
            SettingsRequest::CaptionLanguage(CaptionLanguageRequest::Snapshot(_))=>unreachable!(),
            SettingsRequest::CaptionLanguage(CaptionLanguageRequest::Select{key,choice})=>("launcher","caption_language_set",vec![("key",s(key.wire())),("choice",s(choice.wire()))]),
            SettingsRequest::CaptionCustom(CaptionRequest::Set{visit,field,value})=>("launcher","caption_custom_set",vec![("visit",Value::Int(visit)),("field",s(field.wire())),("value",s(value.wire()))]),
            SettingsRequest::Controls(ControlsRequest::Set{page,control,value}) => ("launcher","control_set",vec![("page",s(page.wire())),("control",s(control.wire())),("value",s(value.wire()))]),
            SettingsRequest::Bluetooth(BluetoothRequest::Enabled(enabled)) => ("launcher","bluetooth_enabled",vec![("enabled",Value::Bool(enabled))]),
            SettingsRequest::Bluetooth(BluetoothRequest::Scan(enabled)) => ("launcher","bluetooth_scan",vec![("enabled",Value::Bool(enabled))]),
            SettingsRequest::Bluetooth(BluetoothRequest::Name(name)) => ("launcher","bluetooth_name",vec![("name",s(name))]),
            SettingsRequest::Bluetooth(BluetoothRequest::Device{target,action}) => ("launcher","bluetooth_device",vec![("key",s(target.key())),("action",s(action.wire()))]),
            SettingsRequest::Bluetooth(BluetoothRequest::Sharing{target,kind,value}) => ("launcher","bluetooth_sharing",vec![("key",s(target.key())),("kind",s(kind.wire())),("value",s(value.wire()))]),
            SettingsRequest::Bluetooth(BluetoothRequest::Access) => ("launcher","bluetooth_access",vec![]),
            SettingsRequest::Accounts(AccountsRequest::Master(enabled)) => ("launcher","accounts_master_sync",vec![("enabled",Value::Bool(enabled))]),
            SettingsRequest::Accounts(AccountsRequest::Access) => ("launcher","accounts_access",vec![]),
            SettingsRequest::Accounts(AccountsRequest::Add(target)) => ("launcher","account_add",vec![("provider_key",s(target.key()))]),
            SettingsRequest::Accounts(AccountsRequest::Remove(target)) => ("launcher","account_remove",vec![("key",s(target.key()))]),
            SettingsRequest::Accounts(AccountsRequest::Sync{account,authority,action}) => {
                let mut fields=vec![("key",s(account.key())),("authority_key",s(authority.key())),("action",s(action.wire()))];
                if let Some(enabled)=action.enabled() {fields.push(("enabled",Value::Bool(enabled)));}
                ("launcher","account_sync",fields)
            },
            SettingsRequest::Updates(UpdatesRequest::Check) => ("launcher","updates_check",vec![]),
            SettingsRequest::Updates(UpdatesRequest::Install{part,offer}) => ("launcher","updates_install",vec![("part",s(part.wire())),("offer_key",s(offer.key()))]),
            SettingsRequest::Updates(UpdatesRequest::Reboot(key)) => ("launcher","updates_reboot",vec![("reboot_key",s(key.key()))]),
            SettingsRequest::Updates(UpdatesRequest::Snapshot) => unreachable!(),
            SettingsRequest::Dnd(DndRequest::Snapshot(_))=>unreachable!(),
            SettingsRequest::Dnd(DndRequest::Policy{key,field,value})=>("launcher","dnd_policy_set",vec![("key",s(key.wire())),("field",s(field.wire())),("value",s(value.wire()))]),
            SettingsRequest::Dnd(DndRequest::Save{key,target,schedule})=>("launcher","dnd_schedule_save",vec![("key",s(key.wire())),("target",target.map(|t|s(t.wire())).unwrap_or(Value::Null)),("name",s(schedule.name)),("days",Value::Arr(schedule.days.into_iter().map(|d|Value::Int(d as i64)).collect())),("start_minute",Value::Int(schedule.start as i64)),("end_minute",Value::Int(schedule.end as i64)),("exit_at_alarm",Value::Bool(schedule.exit_at_alarm)),("enabled",Value::Bool(schedule.enabled))]),
            SettingsRequest::Dnd(DndRequest::Enabled{key,target,enabled})=>("launcher","dnd_rule_enabled",vec![("key",s(key.wire())),("target",s(target.wire())),("enabled",Value::Bool(enabled))]),
            SettingsRequest::Dnd(DndRequest::Delete{key,target})=>("launcher","dnd_rule_delete",vec![("key",s(key.wire())),("target",s(target.wire()))]),
            SettingsRequest::AppLanguage(AppLanguageRequest::Snapshot(_))=>unreachable!(),
            SettingsRequest::AppLanguage(AppLanguageRequest::Select{target,key,choice})=>("launcher","app_language_set",vec![("package",s(target.package())),("key",s(key.wire())),("choice",s(choice.wire()))]),
            SettingsRequest::AppLanguage(AppLanguageRequest::Native(target))=>("launcher","app_action",vec![("package",s(target.package())),("action",s("language"))]),
            SettingsRequest::AppStorage(AppStorageRequest::Snapshot(_))=>unreachable!(),
            SettingsRequest::AppStorage(AppStorageRequest::Action{target,key,action})=>("launcher","app_storage_action",vec![("package",s(target.package())),("key",s(key.wire())),("action",s(action.wire()))]),
            SettingsRequest::AppBattery(AppBatteryRequest::Snapshot(_))=>unreachable!(),
            SettingsRequest::AppBattery(AppBatteryRequest::Set{target,key,mode})=>("launcher","app_battery_set",vec![("package",s(target.package())),("key",s(key.wire())),("mode",s(mode.wire()))]),
            SettingsRequest::AppNetwork(AppNetworkRequest::Snapshot(_))=>unreachable!(),
            SettingsRequest::AppNetwork(AppNetworkRequest::Set{package,key,field,enabled})=>("launcher","app_network_set",vec![("package",s(package.package())),("key",s(key.wire())),("field",s(field.wire())),("enabled",Value::Bool(enabled))]),
            SettingsRequest::Permissions(PermissionsRequest::Snapshot(_))=>unreachable!(),
            SettingsRequest::Permissions(PermissionsRequest::Choose{package,group,key,target})=>("launcher","permission_choice",vec![("package",s(package.package())),("group",s(group.wire())),("key",s(key.wire())),("target",s(target.wire()))]),
            SettingsRequest::Roles(RolesRequest::Snapshot(_))=>unreachable!(),
            SettingsRequest::Roles(RolesRequest::Confirm{role,key,target})=>("launcher","role_confirm",vec![("role",s(role.wire())),("key",s(key.wire())),("target",s(target.wire()))]),
            SettingsRequest::AppNotifications(AppNotificationsRequest::Snapshot(_))=>unreachable!(),
            SettingsRequest::AppNotifications(AppNotificationsRequest::Set{target,key,item,change})=>("launcher","app_notifications_set",vec![("package",s(target.package())),("key",s(key.wire())),("target",s(item.wire())),("action",s(change.action())),("value",s(change.value()))]),
            SettingsRequest::Display(DisplayRequest::Snapshot)=>unreachable!(),
            SettingsRequest::Display(DisplayRequest::Set{key,value})=>{
                if let DisplayValue::Density(choice)=value{("launcher","display_density",vec![("key",s(key.wire())),("choice",s(choice.wire()))])}
                else{("launcher","display_night",vec![("key",s(key.wire())),("setting",s(value.field().unwrap().wire())),("value",value.json())])}
            },
            SettingsRequest::Network(NetworkRequest::Snapshot) => unreachable!(),
            SettingsRequest::NotificationHistory(_) => unreachable!(),
            SettingsRequest::Sounds(_) => unreachable!(),
            SettingsRequest::Network(NetworkRequest::Airplane{key,enabled}) => ("launcher","network_airplane",vec![("key",s(key.key())),("enabled",Value::Bool(enabled))]),
            SettingsRequest::Network(NetworkRequest::DataSaver{key,enabled}) => ("launcher","network_data_saver",vec![("key",s(key.key())),("enabled",Value::Bool(enabled))]),
            SettingsRequest::Network(NetworkRequest::PrivateDns{key,mode,hostname}) => {
                let mut fields=vec![("key",s(key.key())),("mode",s(mode.wire()))];
                if let Some(hostname)=hostname {fields.push(("hostname",s(hostname)));}
                ("launcher","network_private_dns",fields)
            },
            SettingsRequest::Accounts(AccountsRequest::Read(_)) => unreachable!(),
            SettingsRequest::Bluetooth(BluetoothRequest::Snapshot) => unreachable!(),
            SettingsRequest::CaptionCustom(CaptionRequest::Snapshot{..})=>unreachable!(),
            SettingsRequest::Controls(ControlsRequest::Snapshot(_)) => unreachable!(),
            SettingsRequest::Wifi(WifiRequest::Snapshot) => unreachable!(),
            SettingsRequest::AppsCatalog {..}|SettingsRequest::AppDetails {..}|SettingsRequest::AppEntryDetails{..} => unreachable!(),
            SettingsRequest::Back => unreachable!(),
        };
        let id = self.android_command_id(cx, channel, operation, fields);
        self.settings_runtime.pending = Some(Pending { id, client, root: uid, deadline: crate::host::now() + 20.0, device });
        self.settings_outcome(cx, client, true, "Applying…");
    }
    pub(crate) fn settings_result(&mut self, cx: &mut Cx, value: &Value) {
        let id = value.get("id").and_then(Value::as_i64);
        let status = value.get("status").and_then(Value::as_i64).unwrap_or(9);
        if status == 0 { return; } // accepted is not completion
        if self.settings_runtime.read.is_some_and(|read| Some(read.id) == id) {
            let read = self.settings_runtime.read.take().unwrap();
            if read.id > self.settings_runtime.last_observed {
                self.settings_runtime.device = None;
                self.refresh_settings_app(cx);
            }
            self.settings_runtime.next_read = crate::host::now() + 5.0;
            return;
        }
        let Some(pending) = self.settings_runtime.pending else { return; };
        if id != Some(pending.id) { return; }
        self.settings_runtime.pending = None;
        if self.module_host.settings_client(pending.root) != Some(pending.client) { return; }
        let reason = value.get("reason").and_then(Value::as_str).unwrap_or("");
        if reason.starts_with("dnd_") {
            if let Some(instance)=self.module_host.settings_instance(){let(root,vm_id)=(instance.root.clone(),instance.vm_id);let isolate=enter_isolate(cx,vm_id);
                if let Some(mut view)=root.borrow_mut::<SettingsView>(){view.dnd_operation_result(cx,status==1&&reason=="dnd_applied");}leave_isolate(cx,isolate);
            }
        }
        let message = if status == 1 {
            match reason {
                "dnd_requested"=>"Android accepted the DND request. Waiting for observed state.",
                "dnd_applied"=>"DND setting saved and observed in Android.",
                "keyboard_flow_opened"=>"Android opened the keyboard control. Its current state will refresh when you return.",
                "permission_flow_opened" => "Android is applying this permission choice or asking for confirmation. The observed permission will refresh when you return.",
                "role_confirmation_opened" => "Review the choice in Android. The current default will refresh when you return.",
                "theme_applied_partial" => "OctoSense theme saved. Some Android system styling is unavailable.",
                "settings_opened" => "",
                "theme_applied" => "Theme applied.",
                "bluetooth_requested" => "Bluetooth request sent. Check the observed pairing and connection state.",
                "update_check_requested" => "",
                "update_install_requested" => "Update requested. Check the observed installation progress.",
                "update_reboot_requested" => "Restart requested.",
                "sync_applied" => "Sync setting saved.",
                "sync_requested" => "Sync request sent. Check the observed pending and active state.",
                "bluetooth_name_applied" => "Bluetooth name saved.",
                "bluetooth_sharing_applied" => "Sharing permission saved.",
                "control_requested" => "Change requested. Waiting for Android to report the new state.",
                "control_applied" => "Setting applied.",
                "time_applied" => "Date and time updated.",
                "time_requested" => "Change requested. Check the observed date and time.",
                "notifications_applied"=>"Notification setting applied and confirmed by Android.",
                "display_applied"=>"Display setting applied and confirmed by Android.",
                "display_requested"=>"Display change requested. Waiting for the observed state.",
                "languages_applied"=>"System language order applied and confirmed by Android.",
                "languages_requested"=>"System language change requested. Waiting for Android’s current order.",
                "app_language_applied"=>"Language applied.",
                "app_language_requested"=>"Language change requested. Waiting for Android readback.",
                "app_storage_requested"=>"Storage operation requested. Android reports its completion below.",
                "app_storage_flow_opened"=>"App storage manager opened. Return here to see the current storage use.",
                "app_battery_requested"=>"Battery policy requested. Waiting for Android to report the current state.",
                "network_applied" => "Network setting saved. Check the observed connection and DNS status.",
                "network_requested" => "Change requested. Waiting for the observed network state.",
                "wifi_requested" => "Wi-Fi request sent. Check the observed connection state.",
                "wifi_scan_requested" => "Scan requested. Waiting for updated results.",
                _ => "Setting applied.",
            }.to_owned()
        } else if reason=="keyboard_target_changed"{"Keyboard choices changed. Refresh and choose again.".into()} else if reason=="keyboard_restricted"{"Keyboard controls require the focused, unlocked Settings screen.".into()} else if reason == "theme_save_failed" { "Theme could not be saved. Try again.".into() }
        else if reason=="time_target_changed" {"Time settings changed. Review the current values before saving.".into()}
        else if reason=="time_invalid_local" {"This local time does not exist during the daylight-saving transition. Choose another time.".into()}
        else if reason=="time_unavailable" {"Time controls are unavailable or restricted.".into()}
        else if reason=="control_partial" {"Some linked preferences could not be confirmed. Refresh to review their current values.".into()}
        else if reason=="caption_language_changed"||reason=="caption_language_page_changed" {"Caption language choices changed. Refresh and review again.".into()}
        else if reason=="languages_target_changed" {"System languages or their available choices changed. Your draft is preserved; cancel and refresh to review it again.".into()}
        else if reason=="languages_unconfirmed" {"Android has not confirmed the system language change. The current order will refresh; this request will not be retried.".into()}
        else if reason=="languages_restricted"||reason=="languages_unavailable" {"System language changes are unavailable or restricted by Android.".into()}
        else if reason=="app_language_target_changed" {"The app or its language choices changed. Refresh and review again.".into()}
        else if reason=="app_language_unconfirmed" {"Android has not confirmed the language change. Refresh to check; it will not be retried automatically.".into()}
        else if reason=="app_language_restricted"||reason=="app_language_unavailable" {"App language changes are unavailable or restricted by Android.".into()}
        else if reason=="app_storage_target_changed" {"The app or its storage changed. Refresh and review again.".into()}
        else if reason=="app_storage_busy" {"A storage operation is already waiting for Android. Refresh to check completion.".into()}
        else if reason=="app_storage_unconfirmed" {"Android has not confirmed the storage operation. Refresh to inspect its state; it will not be retried automatically.".into()}
        else if reason=="app_storage_restricted"||reason=="app_storage_unavailable" {"Storage changes are unavailable or restricted by Android.".into()}
        else if reason=="app_battery_target_changed" {"The app or its battery policy changed. Refresh and review again.".into()}
        else if reason=="app_battery_unconfirmed"||reason=="app_battery_partial" {"Android could not confirm every part of the battery change. Refresh to inspect the current policy.".into()}
        else if reason=="app_battery_restricted"||reason=="app_battery_unavailable" {"Battery policy changes are unavailable or restricted by Android.".into()}
        else if reason=="app_network_target_changed" {"The app or its network policy changed. Refresh and review again.".into()}
        else if reason=="app_network_unconfirmed" {"Android could not confirm every part of the network change. Refresh to inspect the current state.".into()}
        else if reason=="app_network_restricted"||reason=="app_network_unavailable" {"App network changes are unavailable or restricted by Android.".into()}
        else if reason=="permission_target_changed" {"Permission choices changed. Refresh and choose again.".into()}
        else if reason=="permission_restricted" {"Permission changes are unavailable or restricted.".into()}
        else if reason=="role_target_changed" {"Default-app choices changed. Refresh and select again.".into()}
        else if reason=="role_restricted" {"Default-app changes are unavailable or restricted.".into()}
        else if reason=="notifications_target_changed" {"The app or notification settings changed. Refresh and review the current values.".into()}
        else if reason=="notifications_partial" {"Only part of the linked app and channel change completed. Refresh to check both values.".into()}
        else if reason=="notifications_unconfirmed" {"Android could not confirm the notification change. Refresh to check the current values.".into()}
        else if reason=="notifications_restricted"||reason=="notifications_unavailable" {"This notification control is unavailable or restricted.".into()}
        else if reason=="display_target_changed" {"Display settings changed. Review the current values and try again.".into()}
        else if reason=="display_location_required" {"Enable Location before choosing sunset to sunrise.".into()}
        else if reason=="display_restricted"||reason=="display_unavailable" {"This display control is unavailable or restricted.".into()}
        else if reason=="network_unavailable" {"This network setting is unavailable or restricted.".into()}
        else if reason=="network_target_changed" {"Network settings changed. Review their current values and try again.".into()}
        else if reason=="update_unavailable" {"System updates are unavailable or restricted.".into()}
        else if reason=="update_target_changed" {"The update changed. Check and review it again.".into()}
        else if reason=="sync_unavailable" {"This sync action is unavailable or restricted by Android.".into()}
        else if reason=="account_target_changed" {"That account or sync service changed. Refresh and select it again.".into()}
        else if reason=="bluetooth_unavailable" {"This Bluetooth action is unavailable or restricted by Android.".into()}
        else if reason=="bluetooth_target_changed" {"That device changed. Refresh and select it again.".into()}
        else if reason=="control_unavailable" {"This setting is unavailable. Refresh to check its current state.".into()}
        else if reason=="dnd_target_changed" {"DND settings changed. Refresh and review this choice again.".into()}
        else if reason=="dnd_restricted"||reason=="dnd_unavailable" {"This DND action is unavailable or restricted by Android.".into()}
        else if reason=="policy_restricted" {"This setting is restricted by device policy or screen lock.".into()}
        else if reason=="wifi_scan_throttled" {"Android delayed the scan. Wait before scanning again.".into()}
        else if reason=="wifi_unavailable" {"This Wi-Fi action is unavailable or restricted by Android.".into()}
        else if reason=="wifi_target_changed" {"That network changed. Refresh and select it again.".into()}
        else { crate::android_integration::result_copy(reason).1 };
        self.settings_outcome(cx, pending.client, false, &message);
        self.settings_apps_resync(cx);
        self.settings_wifi_resync(cx);
        self.settings_controls_resync(cx);
        self.settings_keyboards_resync(cx);
        self.settings_system_language_resync(cx);
        self.settings_caption_language_resync(cx);
        self.settings_caption_custom_resync(cx);
        self.settings_bluetooth_resync(cx);
        self.settings_accounts_resync(cx);
        self.settings_updates_resync(cx);
        self.settings_app_notifications_resync(cx);
        self.settings_roles_resync(cx);
        self.settings_permissions_resync(cx);
        self.settings_app_language_resync(cx);
        self.settings_app_storage_resync(cx);
        self.settings_app_battery_resync(cx);
        self.settings_app_network_resync(cx);
        self.settings_dnd_resync(cx);
        self.settings_display_resync(cx);
        self.settings_network_resync(cx);
        self.settings_history_resync(cx);
        self.settings_runtime.next_read = 0.0;
        // Show observed state, never an optimistic value. The bridge snapshot
        // callback and launcher.ui_mode also refresh this same stable view.
        // The typed Settings helper can operate without SystemBridge. Do not
        // turn an optional refresh into a failure notification for that action.
        if self.state.as_ref().is_some_and(|state| state.phone.android.connected) {
            self.android_command(cx, "bridge", "snapshot", vec![]);
        }
        self.android_command(cx, "launcher", "theme_snapshot", vec![]);
    }
    pub(crate) fn settings_device_snapshot(&mut self, cx: &mut Cx, value: &Value) {
        let Some(device) = DeviceSnapshot::decode(value) else { return; };
        let Some(owner) = self.settings_runtime.snapshot_owner(device.request_id) else { return; };
        if self.module_host.settings_client(owner.root) != Some(owner.client) { return; }
        self.settings_runtime.last_observed = device.request_id;
        self.settings_runtime.device = Some(device);
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_activity_resumed(&mut self, observed: Option<bool>) {
        self.settings_runtime.activity_resumed(observed);
        if observed==Some(true) {self.settings_runtime.accessibility.invalidate();}
    }
    pub(crate) fn settings_is_foreground(&self,owner:(crate::hub::ClientId,WidgetUid))->bool {
        self.settings_runtime.resumed && self.module_host.settings_client(owner.1)==Some(owner.0)
            && self.state.as_ref().is_some_and(|state|state.phone.foreground()==Some(owner.0))
    }
    pub(crate) fn settings_tick(&mut self, cx: &mut Cx, event: &Event) {
        if matches!(event, Event::Resume) {
            self.settings_activity_resumed(Some(true));
        } else if matches!(event, Event::Pause | Event::Background) {
            self.settings_activity_resumed(Some(false));
            self.settings_accessibility_clear(cx);
        }
        self.settings_entry_tick(cx);
        let now = crate::host::now();
        if self.settings_runtime.pending.is_some_and(|pending| now >= pending.deadline) {
            self.settings_uncertain(cx);
        }
        let visible = self.module_host.settings_instance().and_then(|instance| {
            (cfg!(target_os = "android") && self.settings_runtime.resumed
                && self.state.as_ref().is_some_and(|state| state.phone.foreground() == Some(instance.client)))
                .then_some((instance.client, instance.root.widget_uid()))
        });
        if visible != self.settings_runtime.active {
            self.settings_runtime.active = visible;
            self.settings_runtime.read = None;
            self.settings_runtime.device = None;
            self.settings_runtime.next_read = 0.0;
            self.refresh_settings_app(cx);
        }
        self.settings_apps_tick(cx,visible,now);
        self.settings_wifi_tick(cx,visible,now);
        self.settings_controls_tick(cx,visible,now);
        self.settings_keyboards_tick(cx,visible,now);
        self.settings_system_language_tick(cx,visible,now);
        self.settings_caption_language_tick(cx,visible,now);
        self.settings_caption_custom_tick(cx,visible,now);
        self.settings_bluetooth_tick(cx,visible,now);
        self.settings_accounts_tick(cx,visible,now);
        self.settings_updates_tick(cx,visible,now);
        self.settings_display_tick(cx,visible,now);
        self.settings_app_notifications_tick(cx,visible,now);
        self.settings_roles_tick(cx,visible,now);
        self.settings_permissions_tick(cx,visible,now);
        self.settings_app_language_tick(cx,visible,now);
        self.settings_app_storage_tick(cx,visible,now);
        self.settings_app_battery_tick(cx,visible,now);
        self.settings_app_network_tick(cx,visible,now);
        self.settings_dnd_tick(cx,visible,now);
        self.settings_network_tick(cx,visible,now);
        self.settings_history_tick(cx,visible,now);
        self.settings_sounds_tick(cx,visible,now);
        let Some((client, root)) = visible else { return; };
        if self.settings_runtime.read.is_some_and(|read| now >= read.deadline) {
            let read = self.settings_runtime.read.take().unwrap();
            if read.id > self.settings_runtime.last_observed {
                self.settings_runtime.device = None;
                self.refresh_settings_app(cx);
            }
            self.settings_runtime.next_read = now + 5.0;
        }
        if self.settings_runtime.read.is_none() && now >= self.settings_runtime.next_read {
            let id = self.android_command_id(cx, "launcher", "device_settings_snapshot", vec![]);
            self.settings_runtime.read = Some(Pending { id, client, root, deadline: now + 20.0, device: false });
            self.settings_runtime.next_read = now + 5.0;
        }
    }
    pub(crate) fn settings_uncertain(&mut self, cx: &mut Cx) {
        self.settings_apps_resync(cx);
        self.settings_wifi_resync(cx);
        self.settings_controls_resync(cx);
        self.settings_keyboards_resync(cx);
        self.settings_system_language_resync(cx);
        self.settings_caption_language_resync(cx);
        self.settings_caption_custom_resync(cx);
        self.settings_bluetooth_resync(cx);
        self.settings_accounts_resync(cx);
        self.settings_updates_resync(cx);
        self.settings_app_notifications_resync(cx);
        self.settings_roles_resync(cx);
        self.settings_permissions_resync(cx);
        self.settings_app_language_resync(cx);
        self.settings_app_storage_resync(cx);
        self.settings_app_battery_resync(cx);
        self.settings_app_network_resync(cx);
        self.settings_dnd_resync(cx);
        self.settings_display_resync(cx);
        self.settings_network_resync(cx);
        self.settings_history_resync(cx);
        self.settings_runtime.read = None;
        self.settings_runtime.device = None;
        self.settings_runtime.next_read = 0.0;
        self.refresh_settings_app(cx);
        let Some(pending) = self.settings_runtime.pending.take() else { return; };
        if self.module_host.settings_client(pending.root) == Some(pending.client) {
            self.settings_outcome(cx, pending.client, false, "No confirmation received. Check the current setting before trying again.");
            if self.state.as_ref().is_some_and(|state| state.phone.android.connected) {
                self.android_command(cx, "bridge", "snapshot", vec![]);
            }
            self.android_command(cx, "launcher", "theme_snapshot", vec![]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_entry_received_before_shell_start_is_retained_without_launching_or_mutating() {
        let mut cx=Cx::new(Box::new(|_,_|{}));let mut app=cx.with_vm(|vm|App::script_new(vm));
        let packet=|id,route|makepad_strict_json::obj(vec![("schema",Value::Int(1)),("id",Value::Int(id)),("route",s(route))]).to_json();
        for (id,route) in [(1,"wifi"),(3,"display"),(2,"sound")] {assert!(app.android_event(&mut cx,&Event::AndroidIntegration{channel:"settings.entry".into(),payload:packet(id,route)}));}
        assert!(app.state.is_none()&&app.module_host.is_empty());assert_eq!(app.settings_runtime.entries.pending().unwrap().route,crate::settings_entry::EntryRoute::Display);
        app.settings_entry_tick(&mut cx);assert!(app.settings_runtime.entries.pending().is_some());
        app.settings_runtime.activity_resumed(Some(true));app.settings_entry_tick(&mut cx);assert!(app.state.is_none()&&app.module_host.is_empty());assert!(app.settings_runtime.entries.pending().is_some());
    }
    #[test]
    fn successful_native_handoffs_do_not_claim_the_requested_change_was_applied(){
        use makepad_app_module::AppModule;
        use crate::{module_host::ModuleHost,settings_app::SETTINGS_MODULE,mobile_theme::Selection};
        let mut cx=Cx::new(Box::new(|_,_|{}));cx.with_vm(makepad_widgets::script_mod);
        let mut app=cx.with_vm(|vm|App::script_new(vm));app.module_host=ModuleHost::default();
        app.module_host.apply_style(&mut cx,&Selection::default().sheet(crate::desktop::DesktopStyle::Android,false));
        app.module_host.create(&mut cx,1,&SETTINGS_MODULE,SETTINGS_MODULE.open_schema().empty_open().unwrap(),dvec2(400.,700.)).unwrap();
        let root=app.module_host.get(1).unwrap().root.clone();let vm=app.module_host.get(1).unwrap().vm_id;
        for (reason,expected) in [("permission_flow_opened","Android is applying"),("role_confirmation_opened","Review the choice in Android"),("app_storage_requested","Storage operation requested."),("app_storage_flow_opened","App storage manager opened.")]{
            app.settings_runtime.pending=Some(Pending{id:5,client:1,root:root.widget_uid(),deadline:20.,device:false});
            app.settings_result(&mut cx,&makepad_strict_json::obj(vec![("id",Value::Int(5)),("status",Value::Int(1)),("reason",s(reason))]));
            let entry=enter_isolate(&mut cx,vm);
            // The desktop harness has no Android shell state; expose the same
            // platform-enabled view without changing the host's result message.
            root.borrow_mut::<SettingsView>().unwrap().observe(&mut cx,SettingsSnapshot{android:true,..Default::default()});
            let message=root.widget(&mut cx,ids!(status)).text();leave_isolate(&mut cx,entry);
            assert!(message.starts_with(expected),"{reason}: {message}");assert!(!message.contains("Setting applied"));
            if reason=="app_storage_requested"{assert!(!message.contains("Waiting"),"A transport receipt must not keep claiming to wait after the native completion arrives");}
        }
    }
    fn pending(id: i64, device: bool) -> Pending {
        Pending { id, client: 3, root: WidgetUid(7), deadline: 20.0, device }
    }
    #[test]
    fn mutation_observation_does_not_consume_or_allow_rollback_from_older_read() {
        let mut runtime = SettingsRuntime { read: Some(pending(8, false)), pending: Some(pending(9, true)), ..Default::default() };
        assert!(runtime.snapshot_owner(99).is_none());
        assert_eq!(runtime.snapshot_owner(9).unwrap().id, 9);
        assert_eq!(runtime.read.unwrap().id, 8);
        runtime.last_observed = 9;
        assert!(runtime.snapshot_owner(8).is_none());
        assert!(runtime.read.is_none());
        assert!(runtime.snapshot_owner(9).is_none());
        assert_eq!(runtime.pending.unwrap().id, 9);
    }
    #[test]
    fn unrelated_commands_and_retired_requests_cannot_supply_device_observations() {
        let mut runtime = SettingsRuntime { pending: Some(pending(12, false)), ..Default::default() };
        assert!(runtime.snapshot_owner(12).is_none());
        runtime.read = Some(pending(13, false));
        assert_eq!(runtime.snapshot_owner(13).unwrap().root, WidgetUid(7));
        assert!(runtime.snapshot_owner(13).is_none());
    }
    #[test]
    fn observed_activity_lifecycle_recovers_a_missed_initial_resume_without_guessing() {
        let mut runtime = SettingsRuntime { next_read: 50.0, ..Default::default() };
        for payload in [r#"{}"#, r#"{"activity_resumed":null}"#, r#"{"activity_resumed":"true"}"#] {
            let value = makepad_strict_json::parse(payload.as_bytes()).unwrap();
            runtime.activity_resumed(value.get("activity_resumed").and_then(Value::as_bool));
            assert!(!runtime.resumed);
            assert_eq!(runtime.next_read, 50.0);
        }
        // The cold-start native Resume event was never delivered. Java's
        // observed current state must make the first visible read eligible.
        runtime.activity_resumed(Some(true));
        assert!(runtime.resumed);
        assert_eq!(runtime.next_read, 0.0);
        runtime.read = Some(pending(14, false));
        runtime.activity_resumed(None);
        assert!(runtime.resumed);
        assert_eq!(runtime.read.unwrap().id, 14, "a mode observation must not duplicate an in-flight read");
        runtime.activity_resumed(Some(false));
        assert!(!runtime.resumed);
        runtime.activity_resumed(None);
        assert!(!runtime.resumed, "missing legacy fields must not undo a real pause");
        runtime.next_read = 75.0;
        runtime.activity_resumed(Some(true));
        assert!(runtime.resumed);
        assert_eq!(runtime.next_read, 0.0);
    }
}
