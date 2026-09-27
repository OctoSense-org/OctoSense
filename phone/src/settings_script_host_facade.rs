//! Mechanical host view of script-owned subscriptions and selected context.
//! Decode once when a complete controller transition is accepted; polling hosts
//! retain their existing owner/focus/correlation/authority checks.
use std::collections::HashMap;
use makepad_strict_json::Value;
use crate::settings_app::{SettingsRequest,SettingsSnapshot};
use crate::settings_apps::AppTarget;
use crate::settings_accounts::{AccountsRead,AccountsRequest,AccountTarget,AuthorityTarget};
use crate::settings_wifi::WifiTarget;
use crate::settings_bluetooth::BluetoothTarget;
use crate::settings_roles::{RolesRead,RolesRequest};
use crate::settings_permissions::{PermissionsRead,PermissionsRequest};
use crate::settings_app_language::{LanguageRead,AppLanguageRequest};
use crate::settings_caption_language::{CaptionLanguageRead,CaptionLanguageRequest};
use crate::settings_system_language::{SystemLanguageRead,SystemLanguageRequest};
use crate::settings_keyboards::{KeyboardRead,KeyboardRequest};
use crate::settings_controls::{ControlsPage,ControlsRequest};
use crate::settings_dnd::{DndRead,DndKey,DndRequest};
use crate::settings_sounds::{SoundsRead,SoundType,SoundKey,SoundsRequest};
use crate::settings_notifications::{HistoryKey,HistoryRead};
use crate::settings_app_notifications::{AppNotificationsRead,AppNotificationsRequest,NotificationKey};
use crate::settings_app_battery::AppBatteryRequest;
use crate::settings_app_storage::AppStorageRequest;
use crate::settings_app_network::{AppNetworkRead,AppNetworkRequest};
use crate::settings_caption_custom::CaptionRequest;

fn exact(value:&Value,keys:&[&str])->Option<()>{let Value::Obj(fields)=value else{return None};(fields.len()==keys.len()&&keys.iter().all(|key|value.get(key).is_some())).then_some(())}
fn text<'a>(value:&'a Value,key:&str)->Option<&'a str>{value.get(key)?.as_str()}
fn offset(value:&Value)->Option<u32>{u32::try_from(value.get("offset")?.as_i64()?).ok()}
fn optional<T>(value:&Value,key:&str,decode:impl FnOnce(&Value)->Option<T>)->Option<Option<T>>{match value.get(key)?{Value::Null=>Some(None),value=>decode(value).map(Some)}}
fn visit(value:&Value)->Option<i64>{let text=value.as_str()?;let number=text.parse::<i64>().ok()?;(number>0&&number.to_string()==text).then_some(number)}

/// A read key is syntax checked here even when its previous observation has
/// retired. Native services validate whether the lease is still current.
pub(crate) fn subscription(value:&Value,observed:&SettingsSnapshot)->Option<SettingsRequest>{
    if text(value,"kind")? != "read" {return None}
    if let Some(request)=crate::settings_script_consent_bridge::subscription(value){return Some(request)}
    let request=match text(value,"domain")?{
        "account_details"=>{exact(value,&["kind","domain","target"])?;SettingsRequest::Accounts(AccountsRequest::Read(AccountsRead::Details(AccountTarget::decode(value.get("target")?)?)))}
        "app_notifications"=>{
            exact(value,&["kind","domain","target","offset","generation"])?;
            SettingsRequest::AppNotifications(AppNotificationsRequest::Snapshot(AppNotificationsRead{target:AppTarget::decode(value.get("target")?)?,offset:offset(value)?,generation:optional(value,"generation",NotificationKey::decode)?}))
        }
        "sounds"=>{
            exact(value,&["kind","domain","sound_type","key","offset"])?;
            SettingsRequest::Sounds(SoundsRequest::Snapshot(SoundsRead{kind:SoundType::ALL.into_iter().find(|kind|Some(kind.wire())==text(value,"sound_type"))?,key:optional(value,"key",SoundKey::decode)?,offset:offset(value)?}))
        }
        "notification_history"=>{exact(value,&["kind","domain","key","offset"])?;SettingsRequest::NotificationHistory(HistoryRead{key:optional(value,"key",HistoryKey::decode)?,offset:offset(value)?})}
        "dnd"=>{exact(value,&["kind","domain","offset","key"])?;SettingsRequest::Dnd(DndRequest::Snapshot(DndRead{offset:offset(value)?,generation:optional(value,"key",DndKey::decode)?}))}
        "caption_custom"=>{exact(value,&["kind","domain","visit"])?;SettingsRequest::CaptionCustom(CaptionRequest::Snapshot{visit:visit(value.get("visit")?)?})}
        _=>crate::settings_script_bridge::basic_request(value,observed)?,
    };
    request.valid().then_some(request)
}
fn named(name:&str,request:&SettingsRequest)->bool{
    matches!((name,request),
        ("apps",SettingsRequest::AppsCatalog{..}|SettingsRequest::AppDetails{..}|SettingsRequest::AppEntryDetails{..})|
        ("accounts",SettingsRequest::Accounts(AccountsRequest::Read(_)))|
        ("wifi",SettingsRequest::Wifi(crate::settings_wifi::WifiRequest::Snapshot))|
        ("bluetooth",SettingsRequest::Bluetooth(crate::settings_bluetooth::BluetoothRequest::Snapshot))|
        ("controls",SettingsRequest::Controls(ControlsRequest::Snapshot(_)))|
        ("roles",SettingsRequest::Roles(RolesRequest::Snapshot(_)))|
        ("permissions",SettingsRequest::Permissions(PermissionsRequest::Snapshot(_)))|
        ("app_language",SettingsRequest::AppLanguage(AppLanguageRequest::Snapshot(_)))|
        ("caption_language",SettingsRequest::CaptionLanguage(CaptionLanguageRequest::Snapshot(_)))|
        ("system_languages",SettingsRequest::SystemLanguages(SystemLanguageRequest::Snapshot(_)))|
        ("keyboards",SettingsRequest::Keyboard(KeyboardRequest::Snapshot(_)))|
        ("app_notifications",SettingsRequest::AppNotifications(AppNotificationsRequest::Snapshot(_)))|
        ("app_battery",SettingsRequest::AppBattery(AppBatteryRequest::Snapshot(_)))|
        ("app_storage",SettingsRequest::AppStorage(AppStorageRequest::Snapshot(_)))|
        ("app_network",SettingsRequest::AppNetwork(AppNetworkRequest::Snapshot(_)))|
        ("caption_custom",SettingsRequest::CaptionCustom(CaptionRequest::Snapshot{..}))|
        ("sounds",SettingsRequest::Sounds(SoundsRequest::Snapshot(_)))|
        ("notification_history",SettingsRequest::NotificationHistory(_))|
        ("dnd",SettingsRequest::Dnd(DndRequest::Snapshot(_)))|
        ("updates",SettingsRequest::Updates(crate::settings_updates::UpdatesRequest::Snapshot))|
        ("network",SettingsRequest::Network(crate::settings_network::NetworkRequest::Snapshot))|
        ("display",SettingsRequest::Display(crate::settings_display::DisplayRequest::Snapshot)))
}

#[derive(Clone,Default)]
pub(crate) struct HostFacade{
    reads:HashMap<String,SettingsRequest>,wifi:Option<WifiTarget>,bluetooth:Option<BluetoothTarget>,
    account:Option<AccountTarget>,authority:Option<AuthorityTarget>,account_scopes:Vec<String>,
    app:Option<AppTarget>,caption:Option<i64>,date_time:bool,
}
impl HostFacade{
    pub(crate) fn decode(state:&Value,observed:&SettingsSnapshot)->Result<Self,String>{
        let fail=||"Invalid script-owned host subscriptions".to_owned();
        let Value::Obj(reads)=state.get("subscriptions").ok_or_else(fail)? else{return Err(fail())};
        if reads.len()>24{return Err(fail())}
        let mut out=Self::default();
        for(name,value)in reads{
            let request=subscription(value,observed).filter(|r|named(name,r)).ok_or_else(fail)?;
            if out.reads.insert(name.clone(),request).is_some(){return Err(fail())}
        }
        let context=state.get("host_context").ok_or_else(fail)?;
        exact(context,&["wifi_target","bluetooth_target","account_target","authority_target","account_scopes","app_target","caption_visit","date_time"]).ok_or_else(fail)?;
        out.wifi=optional(context,"wifi_target",WifiTarget::decode).ok_or_else(fail)?;
        out.bluetooth=optional(context,"bluetooth_target",BluetoothTarget::decode).ok_or_else(fail)?;
        out.account=optional(context,"account_target",AccountTarget::decode).ok_or_else(fail)?;
        out.authority=optional(context,"authority_target",AuthorityTarget::decode).ok_or_else(fail)?;
        out.app=optional(context,"app_target",AppTarget::decode).ok_or_else(fail)?;
        out.caption=optional(context,"caption_visit",visit).ok_or_else(fail)?;
        out.date_time=context.get("date_time").and_then(Value::as_bool).ok_or_else(fail)?;
        for scope in context.get("account_scopes").and_then(Value::as_arr).ok_or_else(fail)?{
            let scope=scope.as_str().filter(|scope|["master","access","add","remove","sync"].contains(scope)).ok_or_else(fail)?;
            if out.account_scopes.iter().any(|s|s==scope){return Err(fail())}out.account_scopes.push(scope.into());
        }
        if out.wifi.is_some()&&!out.wifi_visible()||out.bluetooth.is_some()&&!out.bluetooth_visible(){return Err(fail())}
        if let Some(account)=&out.account{if out.accounts_read()!=Some(AccountsRead::Details(account.clone())){return Err(fail())}}
        if out.authority.is_some()&&out.account.is_none(){return Err(fail())}
        if !out.account_scopes.is_empty()&&!out.active("accounts"){return Err(fail())}
        for scope in &out.account_scopes{
            if ["remove","sync"].contains(&scope.as_str())&&out.account.is_none(){return Err(fail())}
            if scope=="sync"&&out.authority.is_none(){return Err(fail())}
        }
        if let Some(target)=&out.app{
            if !matches!(out.apps_read(),Some(SettingsRequest::AppDetails{target:read,..})if &read==target){return Err(fail())}
        }
        let app_reads=[
            out.permissions_read().map(|read|read.package),out.app_language_read().map(|read|read.target),
            out.app_notifications_read().map(|read|read.target),out.app_battery_read(),out.app_storage_read(),
            out.app_network_read().map(|read|read.package),
        ];
        if app_reads.iter().flatten().any(|target|out.app.as_ref()!=Some(target)){return Err(fail())}
        if out.caption!=out.reads.get("caption_custom").and_then(|r|if let SettingsRequest::CaptionCustom(CaptionRequest::Snapshot{visit})=r{Some(*visit)}else{None}){return Err(fail())}
        Ok(out)
    }
    pub(crate) fn active(&self,domain:&str)->bool{self.reads.contains_key(domain)}
    pub(crate) fn apps_read(&self)->Option<SettingsRequest>{self.reads.get("apps").cloned()}
    pub(crate) fn wifi_visible(&self)->bool{self.active("wifi")}
    pub(crate) fn bluetooth_visible(&self)->bool{self.active("bluetooth")}
    pub(crate) fn display_visible(&self)->bool{self.active("display")}
    pub(crate) fn network_visible(&self)->bool{self.active("network")}
    pub(crate) fn updates_visible(&self)->bool{self.active("updates")}
    pub(crate) fn date_time_visible(&self)->bool{self.date_time}
    pub(crate) fn notification_history_visible(&self)->bool{self.active("notification_history")}
    pub(crate) fn wifi_selected_target(&self)->Option<&WifiTarget>{self.wifi.as_ref()}
    pub(crate) fn bluetooth_selected_target(&self)->Option<&BluetoothTarget>{self.bluetooth.as_ref()}
    pub(crate) fn app_target(&self)->Option<&AppTarget>{self.app.as_ref()}
    pub(crate) fn caption_visit(&self)->Option<i64>{self.caption}
    pub(crate) fn accounts_read(&self)->Option<AccountsRead>{match self.reads.get("accounts")?{SettingsRequest::Accounts(AccountsRequest::Read(read))=>Some(read.clone()),_=>None}}
    pub(crate) fn accounts_context_permits(&self,request:&AccountsRequest)->bool{
        let scope=match request{
            AccountsRequest::Read(read)=>return self.accounts_read().as_ref()==Some(read),
            AccountsRequest::Master(_)=>"master",AccountsRequest::Access=>"access",AccountsRequest::Add(_)=>"add",
            AccountsRequest::Remove(account)=>{if self.account.as_ref()!=Some(account){return false}"remove"},
            AccountsRequest::Sync{account,authority,..}=>{if self.account.as_ref()!=Some(account)||self.authority.as_ref()!=Some(authority){return false}"sync"},
        };
        self.account_scopes.iter().any(|allowed|allowed==scope)
    }
    pub(crate) fn controls_page(&self)->Option<ControlsPage>{match self.reads.get("controls")?{SettingsRequest::Controls(ControlsRequest::Snapshot(page))=>Some(*page),_=>None}}
    pub(crate) fn roles_read(&self)->Option<RolesRead>{match self.reads.get("roles")?{SettingsRequest::Roles(RolesRequest::Snapshot(read))=>Some(read.clone()),_=>None}}
    pub(crate) fn permissions_read(&self)->Option<PermissionsRead>{match self.reads.get("permissions")?{SettingsRequest::Permissions(PermissionsRequest::Snapshot(read))=>Some(read.clone()),_=>None}}
    pub(crate) fn app_language_read(&self)->Option<LanguageRead>{match self.reads.get("app_language")?{SettingsRequest::AppLanguage(AppLanguageRequest::Snapshot(read))=>Some(read.clone()),_=>None}}
    pub(crate) fn caption_language_read(&self)->Option<CaptionLanguageRead>{match self.reads.get("caption_language")?{SettingsRequest::CaptionLanguage(CaptionLanguageRequest::Snapshot(read))=>Some(read.clone()),_=>None}}
    pub(crate) fn system_language_read(&self)->Option<SystemLanguageRead>{match self.reads.get("system_languages")?{SettingsRequest::SystemLanguages(SystemLanguageRequest::Snapshot(read))=>Some(read.clone()),_=>None}}
    pub(crate) fn keyboards_read(&self)->Option<KeyboardRead>{match self.reads.get("keyboards")?{SettingsRequest::Keyboard(KeyboardRequest::Snapshot(read))=>Some(read.clone()),_=>None}}
    pub(crate) fn app_notifications_read(&self)->Option<AppNotificationsRead>{match self.reads.get("app_notifications")?{SettingsRequest::AppNotifications(AppNotificationsRequest::Snapshot(read))=>Some(read.clone()),_=>None}}
    pub(crate) fn app_battery_read(&self)->Option<AppTarget>{match self.reads.get("app_battery")?{SettingsRequest::AppBattery(AppBatteryRequest::Snapshot(target))=>Some(target.clone()),_=>None}}
    pub(crate) fn app_storage_read(&self)->Option<AppTarget>{match self.reads.get("app_storage")?{SettingsRequest::AppStorage(AppStorageRequest::Snapshot(target))=>Some(target.clone()),_=>None}}
    pub(crate) fn app_network_read(&self)->Option<AppNetworkRead>{match self.reads.get("app_network")?{SettingsRequest::AppNetwork(AppNetworkRequest::Snapshot(read))=>Some(read.clone()),_=>None}}
    pub(crate) fn dnd_read(&self)->Option<DndRead>{match self.reads.get("dnd")?{SettingsRequest::Dnd(DndRequest::Snapshot(read))=>Some(read.clone()),_=>None}}
    pub(crate) fn sounds_read(&self)->Option<SoundsRead>{match self.reads.get("sounds")?{SettingsRequest::Sounds(SoundsRequest::Snapshot(read))=>Some(read.clone()),_=>None}}
    pub(crate) fn history_read(&self)->Option<HistoryRead>{match self.reads.get("notification_history")?{SettingsRequest::NotificationHistory(read)=>Some(read.clone()),_=>None}}
}
