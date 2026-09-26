//! Data conversion and finite native bindings for the Settings script controller.
//! No navigation, draft, label, or widget-event decisions belong here.
use crate::settings_app::{Destination, DeviceSetting, SettingsRequest, SettingsSnapshot};
use makepad_strict_json::{s, Value};

const MAX_SAFE: i64 = 9_007_199_254_740_991;

/// Preserve opaque keys and full native integers; numeric measurements remain
/// numeric when exact. Correlation IDs always use decimal strings.
fn data(value: serde_json::Value, field: &str) -> Value {
    match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(value) => Value::Bool(value),
        serde_json::Value::String(value) => s(value),
        serde_json::Value::Number(value) => {
            if field == "request_id" || field == "visit" { return s(value.to_string()); }
            if let Some(n) = value.as_i64() {
                if (-MAX_SAFE..=MAX_SAFE).contains(&n) { Value::Int(n) } else { s(n.to_string()) }
            } else if let Some(n) = value.as_u64() { s(n.to_string()) }
            else { Value::F64(value.as_f64().expect("serialized finite native number")) }
        }
        serde_json::Value::Array(values) => Value::Arr(values.into_iter().map(|v| data(v, "")).collect()),
        serde_json::Value::Object(values) => Value::Obj(values.into_iter().map(|(key, value)| {
            let value = data(value, &key); (key, value)
        }).collect()),
    }
}

pub fn observation(state: &SettingsSnapshot, pending: bool) -> Result<Value, String> {
    let mut fields = Vec::new();
    macro_rules! field { ($($name:ident),+ $(,)?) => {$({
        let value = serde_json::to_value(&state.$name).map_err(|_| "Invalid native observation".to_owned())?;
        fields.push((stringify!($name).to_owned(), data(value, stringify!($name))));
    })+}; }
    field!(android, connected, capabilities, brightness, automatic, rotation, dnd,
        network, device, font_scale, apps_catalog, app_details, apps_error,
        wifi, wifi_error, keyboards, keyboards_error, system_language,
        system_language_error, caption_language, caption_language_error,
        caption_custom, caption_custom_error, controls, controls_error,
        bluetooth, bluetooth_error, accounts, account_details, accounts_error,
        updates, updates_error, app_notifications, app_notifications_error,
        app_language, app_language_error, app_storage, app_storage_error,
        app_battery, app_battery_error, app_network, app_network_error,
        dnd_settings, dnd_error, permissions, permissions_error, roles, roles_error,
        display_options, display_error, advanced_network, network_error, sounds,
        sounds_loading, sounds_error, notification_history, history_loading, history_error);
    fields.push(("theme".into(), state.theme.encode()));
    fields.push(("pending".into(), Value::Bool(pending)));
    // Allocate transport lease identities without exposing a native effect to scripts.
    static NEXT:std::sync::atomic::AtomicI64=std::sync::atomic::AtomicI64::new(0);
    let seed=NEXT.fetch_update(std::sync::atomic::Ordering::Relaxed,std::sync::atomic::Ordering::Relaxed,|n|n.checked_add(1)).ok().and_then(|n|n.checked_add(1));
    fields.push(("caption_visit_seed".into(),seed.map(|n|s(n.to_string())).unwrap_or(Value::Null)));
    Ok(Value::Obj(fields))
}

fn exact(value: &Value, keys: &[&str]) -> Option<()> {
    let Value::Obj(fields) = value else { return None; };
    (fields.len() == keys.len() && keys.iter().all(|key| value.get(key).is_some())).then_some(())
}
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> { value.get(key)?.as_str() }
fn flag(value: &Value, key: &str) -> Option<bool> { value.get(key)?.as_bool() }
fn number(value: &Value, key: &str) -> Option<f64> {
    match value.get(key)? { Value::Int(n) => Some(*n as f64), Value::F64(n) if n.is_finite() => Some(*n), _ => None }
}

fn destination(id: &str) -> Option<Destination> {
    use Destination::*;
    [Wifi, Bluetooth, Mobile, Hotspot, Vpn, Display, DisplayAccess, Sound, Dnd,
        Notifications, System, Apps, DefaultApps, Security, Accounts, Storage,
        Battery, Accessibility, Captions, Languages, Keyboards, About, All, Location, Privacy]
        .into_iter().find(|value| value.id() == id)
}

fn variant<T: serde::Serialize>(values: impl IntoIterator<Item=T>, name: &str) -> Option<T> {
    values.into_iter().find(|v| serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_owned)).as_deref() == Some(name))
}

fn policy_request(value: &Value, observed: &SettingsSnapshot) -> Option<SettingsRequest> {
    use crate::settings_app_battery::{AppBatteryRequest, BatteryMode};
    use crate::settings_app_storage::{AppStorageRequest, StorageAction};
    use crate::settings_app_network::{AppNetworkRequest, NetworkField};
    use crate::settings_apps::{AppTarget, AppAction};
    let target = AppTarget::decode(value.get("target")?)?;
    Some(match text(value, "kind")? {
        "app_action" => {
            exact(value, &["kind","target","action"])?;
            let action = variant([AppAction::Launch,AppAction::Uninstall,AppAction::AppInfo,AppAction::Notifications,AppAction::Language],text(value,"action")?)?;
            let details = observed.app_details.as_ref()?;
            if details.target != target || !details.exists || !details.actions.contains(&action) { return None; }
            SettingsRequest::AppAction { target, action }
        }
        "app_battery" => {
            exact(value, &["kind","target","key","choice"])?;
            let state = observed.app_battery.as_ref()?;
            let key = state.key.as_ref().filter(|k| k.wire() == text(value,"key").unwrap_or(""))?.clone();
            let request = AppBatteryRequest::Set {target,key,mode:variant(BatteryMode::ALL,text(value,"choice")?)?};
            if !state.permits(&request) { return None; }
            SettingsRequest::AppBattery(request)
        }
        "app_storage" => {
            exact(value, &["kind","target","key","choice"])?;
            let state = observed.app_storage.as_ref()?;
            let key = state.key.as_ref().filter(|k| k.wire() == text(value,"key").unwrap_or(""))?.clone();
            let request = AppStorageRequest::Action {target,key,action:variant(StorageAction::ALL,text(value,"choice")?)?};
            if !state.permits(&request) { return None; }
            SettingsRequest::AppStorage(request)
        }
        "app_network" => {
            exact(value, &["kind","target","key","choice","enabled"])?;
            let state = observed.app_network.as_ref()?;
            let key = state.key.as_ref().filter(|k| k.wire() == text(value,"key").unwrap_or(""))?.clone();
            let request = AppNetworkRequest::Set {package:target,key,field:variant(NetworkField::ALL,text(value,"choice")?)?,enabled:flag(value,"enabled")?};
            if !state.permits(&request) { return None; }
            SettingsRequest::AppNetwork(request)
        }
        _ => return None,
    })
}

fn read_request(value: &Value, observed: &SettingsSnapshot) -> Option<SettingsRequest> {
    use crate::settings_apps::AppTarget;
    Some(match text(value,"domain")? {
        "caption_custom"=>{
            exact(value,&["kind","domain","visit"])?;
            let raw=text(value,"visit")?; let visit=raw.parse::<i64>().ok().filter(|n|*n>0&&n.to_string()==raw)?;
            SettingsRequest::CaptionCustom(crate::settings_caption_custom::CaptionRequest::Snapshot{visit})
        }
        "dnd"=>{
            exact(value,&["kind","domain","offset","key"])?;
            let generation=match value.get("key")?{Value::Null=>None,v=>Some(observed.dnd_settings.as_ref().filter(|n|Some(n.generation.wire())==v.as_str())?.generation.clone())};
            SettingsRequest::Dnd(crate::settings_dnd::DndRequest::Snapshot(crate::settings_dnd::DndRead{offset:u32::try_from(value.get("offset")?.as_i64()?).ok()?,generation}))
        }
        "accounts"=>{exact(value,&["kind","domain"])?;SettingsRequest::Accounts(crate::settings_accounts::AccountsRequest::Read(crate::settings_accounts::AccountsRead::Snapshot))}
        "account_details"=>{
            exact(value,&["kind","domain","target"])?;
            let target=text(value,"target")?;
            let account=observed.accounts.as_ref().and_then(|s|s.accounts.iter().find(|a|a.target.key()==target).map(|a|a.target.clone())).or_else(||observed.account_details.as_ref().filter(|d|d.target.key()==target).map(|d|d.target.clone()))?;
            SettingsRequest::Accounts(crate::settings_accounts::AccountsRequest::Read(crate::settings_accounts::AccountsRead::Details(account)))
        }
        "app_entry"=>{
            exact(value,&["kind","domain","entry_id","package"])?;
            SettingsRequest::AppEntryDetails{entry_id:text(value,"entry_id")?.parse().ok()?,package:text(value,"package")?.into()}
        }
        "app_notifications"=>{
            exact(value,&["kind","domain","target","offset","generation"])?;
            let target=AppTarget::decode(value.get("target")?)?;
            let generation=match value.get("generation")?{Value::Null=>None,v=>Some(observed.app_notifications.as_ref().filter(|s|s.target==target)?.generation.as_ref().filter(|g|Some(g.wire())==v.as_str())?.clone())};
            SettingsRequest::AppNotifications(crate::settings_app_notifications::AppNotificationsRequest::Snapshot(crate::settings_app_notifications::AppNotificationsRead{target,offset:u32::try_from(value.get("offset")?.as_i64()?).ok()?,generation}))
        }
        "notification_history"=>{
            exact(value,&["kind","domain","key","offset"])?;
            let key=match value.get("key")?{Value::Null=>None,v=>Some(observed.notification_history.as_ref()?.key.as_ref().filter(|k|Some(k.wire())==v.as_str())?.clone())};
            SettingsRequest::NotificationHistory(crate::settings_notifications::HistoryRead{key,offset:u32::try_from(value.get("offset")?.as_i64()?).ok()?})
        }
        "sounds"=>{
            use crate::settings_sounds::{SoundsRead,SoundsRequest,SoundType};
            exact(value,&["kind","domain","sound_type","key","offset"])?;
            let kind=SoundType::ALL.into_iter().find(|k|k.wire()==text(value,"sound_type").unwrap_or(""))?;
            let key=match value.get("key")?{Value::Null=>None,v=>Some(observed.sounds.as_ref().filter(|s|s.kind==kind)?.key.as_ref().filter(|k|Some(k.wire())==v.as_str())?.clone())};
            SettingsRequest::Sounds(SoundsRequest::Snapshot(SoundsRead{kind,key,offset:u32::try_from(value.get("offset")?.as_i64()?).ok()?}))
        }
        "bluetooth"=>{exact(value,&["kind","domain"])?;SettingsRequest::Bluetooth(crate::settings_bluetooth::BluetoothRequest::Snapshot)}
        "wifi" => {exact(value,&["kind","domain"])?;SettingsRequest::Wifi(crate::settings_wifi::WifiRequest::Snapshot)}
        "display" => {exact(value,&["kind","domain"])?;SettingsRequest::Display(crate::settings_display::DisplayRequest::Snapshot)}
        "network" => {exact(value,&["kind","domain"])?;SettingsRequest::Network(crate::settings_network::NetworkRequest::Snapshot)}
        "updates" => { exact(value,&["kind","domain"])?; SettingsRequest::Updates(crate::settings_updates::UpdatesRequest::Snapshot) }
        "apps" => {
            exact(value,&["kind","domain","query","include_system","offset","generation"])?;
            let generation = match value.get("generation")? { Value::Null=>None, v=>Some(v.as_str()?.to_owned()) };
            SettingsRequest::AppsCatalog {query:text(value,"query")?.into(),include_system:flag(value,"include_system")?,offset:u32::try_from(value.get("offset")?.as_i64()?).ok()?,generation}
        }
        "app_details" => {
            exact(value,&["kind","domain","target","offset"])?;
            SettingsRequest::AppDetails {target:AppTarget::decode(value.get("target")?)?,permission_offset:u32::try_from(value.get("offset")?.as_i64()?).ok()?}
        }
        "controls" => {
            exact(value,&["kind","domain","page"])?;
            SettingsRequest::Controls(crate::settings_controls::ControlsRequest::Snapshot(crate::settings_controls::ControlsPage::decode(value.get("page")?)?))
        }
        "app_battery" | "app_storage" | "app_network" => {
            exact(value,&["kind","domain","target"])?;
            let target = AppTarget::decode(value.get("target")?)?;
            match text(value,"domain")? {
                "app_battery" => SettingsRequest::AppBattery(crate::settings_app_battery::AppBatteryRequest::Snapshot(target)),
                "app_storage" => SettingsRequest::AppStorage(crate::settings_app_storage::AppStorageRequest::Snapshot(target)),
                _ => SettingsRequest::AppNetwork(crate::settings_app_network::AppNetworkRequest::Snapshot(crate::settings_app_network::AppNetworkRead {package:target})),
            }
        }
        _ => return None,
    })
}

/// Decode the core finite binding vocabulary. This is an additional check;
/// the existing host and Android services still recheck authority at execution.
pub fn basic_request(value: &Value, observed: &SettingsSnapshot) -> Option<SettingsRequest> {
    if let Some(request)=crate::settings_script_consent_bridge::request(value,observed) {return request.valid().then_some(request);}
    let request = match text(value, "kind")? {
        "caption_custom"=>{
            use crate::settings_caption_custom::{CaptionField,CaptionValue,CaptionRequest};
            if !observed.android {return None}
            exact(value,&["kind","visit","field","value"])?;
            let raw=text(value,"visit")?;let visit=raw.parse::<i64>().ok().filter(|n|*n>0&&n.to_string()==raw)?;
            let field=CaptionField::ALL.into_iter().find(|f|Some(f.wire())==text(value,"field"))?;
            let request=CaptionRequest::Set{visit,field,value:CaptionValue::decode(text(value,"value")?)?};
            if !observed.caption_custom.as_ref()?.permits(&request){return None}
            SettingsRequest::CaptionCustom(request)
        }
        "dnd"=>{
            use crate::settings_dnd::{DndRequest,PolicyField,Schedule};
            if !observed.android {return None;}
            let state=observed.dnd_settings.as_ref()?;
            if text(value,"key")?!=state.key.wire(){return None;}
            let key=state.key.clone();
            let target=||state.rules.iter().find(|r|r.target.wire()==text(value,"target").unwrap_or("")).map(|r|r.target.clone());
            let request=match text(value,"operation")? {
                "Policy"=>{
                    exact(value,&["kind","operation","key","field","value"])?;
                    let field=variant(PolicyField::ALL,text(value,"field")?)?;
                    DndRequest::Policy{key,field,value:variant(field.choices().iter().copied(),text(value,"value")?)?}
                }
                "Enabled"=>{exact(value,&["kind","operation","key","target","enabled"])?;DndRequest::Enabled{key,target:target()?,enabled:flag(value,"enabled")?}}
                "Delete"=>{exact(value,&["kind","operation","key","target"])?;DndRequest::Delete{key,target:target()?}}
                "Save"=>{
                    exact(value,&["kind","operation","key","target","schedule"])?;
                    let target=if value.get("target")?.is_null(){None}else{Some(target()?)};
                    let schedule=value.get("schedule")?;exact(schedule,&["name","days","start","end","exit_at_alarm","enabled"])?;
                    let days=schedule.get("days")?.as_arr()?.iter().map(|v|u8::try_from(v.as_i64()?).ok()).collect::<Option<Vec<_>>>()?;
                    DndRequest::Save{key,target,schedule:Schedule{name:text(schedule,"name")?.into(),days,start:u16::try_from(schedule.get("start")?.as_i64()?).ok()?,end:u16::try_from(schedule.get("end")?.as_i64()?).ok()?,exit_at_alarm:flag(schedule,"exit_at_alarm")?,enabled:flag(schedule,"enabled")?}}
                }
                _=>return None,
            };
            if !state.permits(&request){return None;}
            SettingsRequest::Dnd(request)
        }
        "accounts"=>{
            use crate::settings_accounts::{AccountsRequest,SyncAction};
            if !observed.android {return None;}
            let request=match text(value,"operation")? {
                "Master"=>{exact(value,&["kind","operation","enabled"])?;AccountsRequest::Master(flag(value,"enabled")?)}
                "Access"=>{exact(value,&["kind","operation"])?;AccountsRequest::Access}
                "Add"=>{exact(value,&["kind","operation","target"])?;AccountsRequest::Add(observed.accounts.as_ref()?.providers.iter().find(|p|p.target.key()==text(value,"target").unwrap_or(""))?.target.clone())}
                "Remove"=>{exact(value,&["kind","operation","target"])?;AccountsRequest::Remove(observed.account_details.as_ref().filter(|d|d.target.key()==text(value,"target").unwrap_or(""))?.target.clone())}
                "Automatic"|"SyncNow"|"Cancel"=>{
                    let operation=text(value,"operation")?;
                    if operation=="Automatic" {exact(value,&["kind","operation","account","authority","enabled"])?;}else{exact(value,&["kind","operation","account","authority"])?;}
                    let d=observed.account_details.as_ref().filter(|d|d.target.key()==text(value,"account").unwrap_or(""))?;
                    let authority=d.authorities.iter().find(|a|a.target.key()==text(value,"authority").unwrap_or(""))?.target.clone();
                    AccountsRequest::Sync{account:d.target.clone(),authority,action:match operation{"Automatic"=>SyncAction::Automatic(flag(value,"enabled")?),"SyncNow"=>SyncAction::SyncNow,_=>SyncAction::Cancel}}
                }
                _=>return None,
            };
            if !observed.accounts.as_ref().is_some_and(|s|s.permits(&request))&&!observed.account_details.as_ref().is_some_and(|s|s.permits(&request)){return None;}
            SettingsRequest::Accounts(request)
        }
        "app_notifications"=>{
            use crate::settings_app_notifications::{AppNotificationsRequest,NotificationChange,Importance};
            exact(value,&["kind","target","key","item","change","value"])?;
            if !observed.android {return None;}
            let state=observed.app_notifications.as_ref()?;
            if text(value,"target")?!=state.target.package(){return None;}
            let key=state.key.as_ref().filter(|k|k.wire()==text(value,"key").unwrap_or(""))?.clone();
            let item=state.app.as_ref().filter(|a|a.key.wire()==text(value,"item").unwrap_or("")).map(|a|a.key.clone()).or_else(||state.rows.iter().find(|r|r.key.wire()==text(value,"item").unwrap_or("")).map(|r|r.key.clone()))?;
            let change=match text(value,"change")?{
                "App"=>NotificationChange::App(flag(value,"value")?),"Group"=>NotificationChange::Group(flag(value,"value")?),"Channel"=>NotificationChange::Channel(flag(value,"value")?),
                "Importance"=>NotificationChange::Importance(variant(Importance::CHOICES,text(value,"value")?)?),_=>return None,
            };
            let request=AppNotificationsRequest::Set{target:state.target.clone(),key,item,change};
            if !state.permits(&request){return None;}
            SettingsRequest::AppNotifications(request)
        }
        "sounds"=>{
            use crate::settings_sounds::{SoundsRequest,SoundType};
            if !observed.android {return None;}
            let state=observed.sounds.as_ref()?;
            let request=match text(value,"operation")? {
                "Access"=>{exact(value,&["kind","operation"])?;SoundsRequest::Access}
                "Preview"|"Save"=>{
                    exact(value,&["kind","operation","sound_type","key","target"])?;
                    let kind=SoundType::ALL.into_iter().find(|k|k.wire()==text(value,"sound_type").unwrap_or(""))?;
                    let key=state.key.as_ref().filter(|k|k.wire()==text(value,"key").unwrap_or(""))?.clone();
                    let target=state.rows.iter().find(|r|r.key.wire()==text(value,"target").unwrap_or(""))?.key.clone();
                    if text(value,"operation")?=="Save"{SoundsRequest::Save{kind,key,target}}else{SoundsRequest::Preview{kind,key,target}}
                }
                _=>return None,
            };
            if !state.permits(&request){return None;}
            SettingsRequest::Sounds(request)
        }
        "datetime"=>{
            use crate::settings_datetime::{TimeRequest,TimeAction};
            if !observed.android {return None;}
            let device=observed.device.as_ref()?;let state=device.time_controls.as_ref()?;
            if text(value,"key")?!=state.key.wire(){return None;}
            let action=match text(value,"operation")? {
                "AutoTime"|"AutoZone"=>{exact(value,&["kind","operation","key","enabled"])?;let enabled=flag(value,"enabled")?;if text(value,"operation")?=="AutoTime"{TimeAction::AutoTime(enabled)}else{TimeAction::AutoZone(enabled)}}
                "Clock"=>{exact(value,&["kind","operation","key","civil","second"])?;TimeAction::Clock{civil:text(value,"civil")?.into(),second:flag(value,"second")?}}
                "Zone"=>{exact(value,&["kind","operation","key","zone"])?;TimeAction::Zone(text(value,"zone")?.into())}
                _=>return None,
            };
            let request=TimeRequest{key:state.key.clone(),action};
            if !state.permits(&request,&device.time_zones){return None;}
            SettingsRequest::DateTime(request)
        }
        "bluetooth"=>{
            use crate::settings_bluetooth::{BluetoothRequest,BluetoothAction,SharingKind,SharingValue};
            if !observed.android {return None;}
            let state=observed.bluetooth.as_ref()?;
            let request=match text(value,"operation")? {
                "Enabled"|"Scan"=>{exact(value,&["kind","operation","enabled"])?;let enabled=flag(value,"enabled")?;if text(value,"operation")?=="Enabled"{BluetoothRequest::Enabled(enabled)}else{BluetoothRequest::Scan(enabled)}}
                "Access"=>{exact(value,&["kind","operation"])?;BluetoothRequest::Access}
                "Name"=>{exact(value,&["kind","operation","name"])?;BluetoothRequest::Name(text(value,"name")?.into())}
                "Device"=>{
                    exact(value,&["kind","operation","target","action"])?;
                    BluetoothRequest::Device{target:state.devices.iter().find(|r|r.target.key()==text(value,"target").unwrap_or(""))?.target.clone(),action:variant([BluetoothAction::Pair,BluetoothAction::CancelPair,BluetoothAction::Connect,BluetoothAction::Disconnect,BluetoothAction::Forget],text(value,"action")?)?}
                }
                "Sharing"=>{
                    exact(value,&["kind","operation","target","sharing","value"])?;
                    BluetoothRequest::Sharing{target:state.devices.iter().find(|r|r.target.key()==text(value,"target").unwrap_or(""))?.target.clone(),kind:variant([SharingKind::Phonebook,SharingKind::Messages],text(value,"sharing")?)?,value:variant(SharingValue::ALL,text(value,"value")?)?}
                }
                _=>return None,
            };
            if !state.permits(&request){return None;}
            SettingsRequest::Bluetooth(request)
        }
        "wifi" => {
            use crate::settings_wifi::{WifiRequest,WifiAction};
            if !observed.android {return None;}
            let state=observed.wifi.as_ref()?;
            let request=match text(value,"operation")? {
                "Enabled"=>{exact(value,&["kind","operation","enabled"])?;WifiRequest::Enabled(flag(value,"enabled")?)}
                "Scan"=>{exact(value,&["kind","operation"])?;WifiRequest::Scan}
                "Access"=>{exact(value,&["kind","operation"])?;WifiRequest::Access}
                "Network"=>{
                    exact(value,&["kind","operation","target","action"])?;
                    WifiRequest::Network{target:state.networks.iter().find(|r|r.target.key()==text(value,"target").unwrap_or(""))?.target.clone(),action:variant([WifiAction::Connect,WifiAction::Configure,WifiAction::Forget],text(value,"action")?)?}
                }
                _=>return None,
            };
            if !state.permits(&request) {return None;}
            SettingsRequest::Wifi(request)
        }
        "display" => {
            use crate::settings_display::{DisplayRequest,DisplayValue,NightMode};
            exact(value,&["kind","key","field","value"])?;
            if !observed.android {return None;}
            let state=observed.display_options.as_ref()?;
            if text(value,"key")?!=state.key.wire() {return None;}
            let integer=||u32::try_from(value.get("value")?.as_i64()?).ok();
            let choice=match text(value,"field")? {
                "Density"=>DisplayValue::Density(state.density.options.iter().find(|c|c.key.wire()==text(value,"value").unwrap_or(""))?.key.clone()),
                "Activated"=>DisplayValue::Activated(flag(value,"value")?),
                "Temperature"=>DisplayValue::Temperature(integer()?),
                "Mode"=>DisplayValue::Mode(variant(NightMode::ALL,text(value,"value")?)?),
                "Start"=>DisplayValue::Start(integer()?),"End"=>DisplayValue::End(integer()?),
                _=>return None,
            };
            let request=DisplayRequest::Set {key:state.key.clone(),value:choice};
            if !state.permits(&request) {return None;}
            SettingsRequest::Display(request)
        }
        "network" => {
            use crate::settings_network::{NetworkRequest,DnsMode};
            if !observed.android {return None;}
            let state=observed.advanced_network.as_ref()?;
            if text(value,"key")?!=state.key.key() {return None;}
            let key=state.key.clone();
            let request=match text(value,"operation")? {
                "Airplane"|"DataSaver"=>{
                    exact(value,&["kind","operation","key","enabled"])?;
                    let enabled=flag(value,"enabled")?;
                    if text(value,"operation")?=="Airplane" {NetworkRequest::Airplane{key,enabled}} else {NetworkRequest::DataSaver{key,enabled}}
                }
                "PrivateDns"=>{
                    exact(value,&["kind","operation","key","mode","hostname"])?;
                    let hostname=match value.get("hostname")? {Value::Null=>None,v=>Some(v.as_str()?.into())};
                    NetworkRequest::PrivateDns{key,mode:variant(DnsMode::ALL,text(value,"mode")?)?,hostname}
                }
                _=>return None,
            };
            if !state.permits(&request) {return None;}
            SettingsRequest::Network(request)
        }
        "updates" => {
            use crate::settings_updates::{UpdatesRequest,UpdatePart};
            if !observed.android { return None; }
            let state=observed.updates.as_ref()?;
            let request=match text(value,"operation")? {
                "Check"=>{exact(value,&["kind","operation"])?;UpdatesRequest::Check}
                "Reboot"=>{exact(value,&["kind","operation","key"])?;UpdatesRequest::Reboot(state.reboot_key.as_ref().filter(|k| k.key()==text(value,"key").unwrap_or(""))?.clone())}
                "Install"=>{
                    exact(value,&["kind","operation","part","key"])?;
                    UpdatesRequest::Install{part:variant([UpdatePart::Rom,UpdatePart::Home],text(value,"part")?)?,offer:state.offer.as_ref().filter(|o|o.key.key()==text(value,"key").unwrap_or(""))?.key.clone()}
                }
                _=>return None,
            };
            if !state.permits(&request) {return None;}
            SettingsRequest::Updates(request)
        }
        "apps_usage_access" => {
            exact(value,&["kind","target"])?;
            let state = observed.app_details.as_ref()?;
            if !observed.android || !state.exists || !state.storage.can_request_usage_access || state.target.package()!=text(value,"target")? { return None; }
            SettingsRequest::AppsUsageAccess
        }
        "read" => { if !observed.android { return None; } read_request(value,observed)? }
        "app_action" | "app_battery" | "app_storage" | "app_network" => {
            if !observed.android { return None; }
            policy_request(value,observed)?
        }
        "control" => {
            exact(value,&["kind","page","control","value"])?;
            if !observed.android { return None; }
            let state = observed.controls.as_ref()?;
            let page = crate::settings_controls::ControlsPage::decode(value.get("page")?)?;
            let control = page.controls().iter().find(|c| c.wire()==text(value,"control").unwrap_or(""))?;
            let choice = state.control(*control)?.options.iter().find(|c| c.wire()==text(value,"value").unwrap_or(""))?;
            let request = crate::settings_controls::ControlsRequest::Set {page,control:*control,value:*choice};
            if !state.permits(&request) { return None; }
            SettingsRequest::Controls(request)
        }
        "back" => { exact(value, &["kind"])?; SettingsRequest::Back }
        "theme" => {
            exact(value, &["kind", "theme"])?;
            let theme = value.get("theme")?;
            let mut pairs = match theme { Value::Obj(pairs) => pairs.clone(), _ => return None };
            if theme.get("version").is_none() { pairs.push(("version".into(), Value::Int(1))); }
            let theme = Value::Obj(pairs);
            exact(&theme, &["version", "preset", "appearance", "wallpaper"])?;
            SettingsRequest::Theme(crate::mobile_theme::Selection::decode(&theme)?)
        }
        "brightness" => {
            exact(value, &["kind", "value", "automatic"])?;
            if !observed.permits("brightness") { return None; }
            SettingsRequest::Brightness { value: number(value, "value")?, automatic: flag(value, "automatic")? }
        }
        "rotation" => {
            exact(value, &["kind", "value"])?;
            if !observed.permits("rotation") { return None; }
            SettingsRequest::Rotation(flag(value, "value")?)
        }
        "open" => {
            exact(value, &["kind", "destination"])?;
            if !observed.android { return None; }
            SettingsRequest::Open(destination(text(value, "destination")?)?)
        }
        "device_access" => {
            exact(value, &["kind"])?;
            if !observed.android || !observed.device.as_ref()?.can_request_write_settings? { return None; }
            SettingsRequest::DeviceAccess
        }
        "device" => {
            exact(value, &["kind", "setting", "value"])?;
            let setting = match text(value, "setting")? {
                "screen_timeout_ms" => DeviceSetting::ScreenTimeout(value.get("value")?.as_i64()?),
                "font_scale" => DeviceSetting::FontScale(number(value, "value")?),
                "touch_sounds" => DeviceSetting::TouchSounds(flag(value, "value")?),
                "haptic_feedback" => DeviceSetting::HapticFeedback(flag(value, "value")?),
                "hour_format" => DeviceSetting::HourFormat(flag(value, "value")?),
                "auto_time" => DeviceSetting::AutoTime(flag(value, "value")?),
                "auto_time_zone" => DeviceSetting::AutoTimeZone(flag(value, "value")?),
                "volume_media" => DeviceSetting::MediaVolume(number(value, "value")?),
                "volume_alarm" => DeviceSetting::AlarmVolume(number(value, "value")?),
                "volume_ring" => DeviceSetting::RingVolume(number(value, "value")?),
                "volume_notification" => DeviceSetting::NotificationVolume(number(value, "value")?),
                _ => return None,
            };
            if !observed.permits_device(&setting) { return None; }
            SettingsRequest::Device(setting)
        }
        "sound_stop" => { exact(value, &["kind"])?; SettingsRequest::Sounds(crate::settings_sounds::SoundsRequest::Stop) }
        _ => return None,
    };
    request.valid().then_some(request)
}

#[cfg(test)] mod tests {
    use super::*;
    use makepad_strict_json::obj;
    #[test] fn native_ids_and_float_observations_keep_their_exact_representation() {
        let mut state = SettingsSnapshot::default();
        state.device = Some(crate::settings_app::DeviceSnapshot { request_id: i64::MAX, storage_total: Some(i64::MAX), ..Default::default() });
        let wire = observation(&state, false).unwrap();
        let device = wire.get("device").unwrap();
        assert_eq!(device.get("request_id").and_then(Value::as_str), Some("9223372036854775807"));
        assert_eq!(device.get("storage_total").and_then(Value::as_str), Some("9223372036854775807"));
        let raw = crate::settings_hearing::NativeFloat::decode("0.33333334", true).unwrap();
        assert_eq!(data(serde_json::to_value(raw).unwrap(), ""), s("0.33333334"));
    }
    #[test] fn core_binding_rechecks_capability_bounds_and_refuses_generic_android_access() {
        let request = obj(vec![("kind", s("brightness")), ("value", Value::F64(0.7)), ("automatic", Value::Bool(false))]);
        let mut state = SettingsSnapshot::default();
        assert!(basic_request(&request, &state).is_none());
        state.android = true; state.connected = true; state.capabilities.insert("brightness".into());
        assert!(matches!(basic_request(&request, &state), Some(SettingsRequest::Brightness { value, automatic: false }) if value == 0.7));
        for request in [
            obj(vec![("kind", s("brightness")), ("value", Value::F64(1.1)), ("automatic", Value::Bool(false))]),
            obj(vec![("kind", s("open")), ("destination", s("android.settings.WIFI_SETTINGS"))]),
            obj(vec![("kind", s("device")), ("setting", s("enabled_input_methods")), ("value", s("provider/.IME"))]),
            obj(vec![("kind", s("back")), ("shell", s("ignored command"))]),
        ] { assert!(basic_request(&request, &state).is_none()); }
    }
}
