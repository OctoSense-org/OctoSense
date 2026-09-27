//! Observed package storage, reviewed finite actions and independent native completion.
use crate::settings_apps::AppTarget;
use makepad_strict_json::Value;
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]pub struct StorageKey(String);
impl StorageKey {pub fn wire(&self)->&str{&self.0}fn decode(v:&Value)->Option<Self>{let s=v.as_str()?;(s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))).then(||Self(s.into()))}}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]pub enum StorageAction{ClearCache,ClearData,ManageSpace}
impl StorageAction {
    pub const ALL:[Self;3]=[Self::ClearCache,Self::ClearData,Self::ManageSpace];
    pub fn wire(self)->&'static str{match self{Self::ClearCache=>"clear_cache",Self::ClearData=>"clear_data",Self::ManageSpace=>"manage_space"}}
    pub fn label(self)->&'static str{match self{Self::ClearCache=>"Clear cache…",Self::ClearData=>"Clear storage…",Self::ManageSpace=>"Manage space…"}}
    pub fn review_label(self)->&'static str{match self{Self::ClearCache=>"Confirm clear cache",Self::ClearData=>"Confirm clear storage",Self::ManageSpace=>"Open manage space"}}
    fn decode(v:&Value)->Option<Self>{Self::ALL.into_iter().find(|a|Some(a.wire())==v.as_str())}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]pub enum AppStorageRequest{Snapshot(AppTarget),Action{target:AppTarget,key:StorageKey,action:StorageAction}}
impl AppStorageRequest{pub fn valid(&self)->bool{true}}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]pub enum StorageAvailability{Available,Restricted,Unavailable,Missing}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]pub enum StorageReason{None,DevicePolicy,ProtectedApp,SharedUid,ServiceUnavailable,StatsUnavailable,AppMissing,Locked,OperationPending}
impl StorageReason{pub fn label(self)->&'static str{match self{Self::None=>"",Self::DevicePolicy=>"Your administrator restricts storage changes for this app.",Self::ProtectedApp=>"Android protects storage for this essential app.",Self::SharedUid=>"This app shares an Android identity. Clearing storage here could affect other apps, so these controls are unavailable.",Self::ServiceUnavailable=>"Storage controls are unavailable on this installation. Android app settings remain available below.",Self::StatsUnavailable=>"Android has not provided current storage sizes. Clearing is unavailable until storage can be read.",Self::AppMissing=>"This app is no longer installed.",Self::Locked=>"Unlock the device to review this app’s storage.",Self::OperationPending=>"Another storage operation is still waiting for Android to confirm completion."}}}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]pub enum StorageOperationState{Pending,Succeeded,Failed,Uncertain}
#[derive(serde::Serialize, Clone,Debug)]pub struct StorageOperation{pub id:StorageKey,pub action:StorageAction,pub state:StorageOperationState}
impl StorageOperation{pub fn text(&self)->&'static str{match(self.action,self.state){(StorageAction::ClearCache,StorageOperationState::Pending)=>"Android is clearing cache…",(_,StorageOperationState::Pending)=>"Android is clearing app storage…",(StorageAction::ClearCache,StorageOperationState::Succeeded)=>"Android completed the cache clear. The app may create new cache while it runs.",(_,StorageOperationState::Succeeded)=>"Android completed the storage clear. Sizes below are the latest observation.",(_,StorageOperationState::Failed)=>"Android could not complete the storage operation. Check the current state before choosing again.",(_,StorageOperationState::Uncertain)=>"Completion has not been confirmed. Refresh to check Android’s response; the operation is not retried automatically."}}}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]pub struct StorageSizes{pub app_bytes:i64,pub data_bytes:i64,pub cache_bytes:i64}
#[derive(serde::Serialize, Clone,Debug)]pub struct AppStorageSnapshot{
    pub request_id:i64,pub target:AppTarget,pub availability:StorageAvailability,pub reason:StorageReason,pub key:Option<StorageKey>,pub label:Option<String>,pub shared_uid:bool,pub stats:Option<StorageSizes>,pub actions:Vec<StorageAction>,pub operation:Option<StorageOperation>,
}
fn optional<T>(v:Option<&Value>,f:impl FnOnce(&Value)->Option<T>)->Option<Option<T>>{match v{None|Some(Value::Null)=>Some(None),Some(v)=>f(v).map(Some)}}
impl AppStorageSnapshot{
    pub fn decode(v:&Value)->Option<Self>{
        if v.get("schema")?.as_i64()?!=1{return None;}
        let availability=match v.get("availability")?.as_str()?{"available"=>StorageAvailability::Available,"restricted"=>StorageAvailability::Restricted,"unavailable"=>StorageAvailability::Unavailable,"missing"=>StorageAvailability::Missing,_=>return None};
        let reason=match v.get("reason")?.as_str()?{"none"=>StorageReason::None,"device_policy"=>StorageReason::DevicePolicy,"protected_app"=>StorageReason::ProtectedApp,"shared_uid"=>StorageReason::SharedUid,"service_unavailable"=>StorageReason::ServiceUnavailable,"stats_unavailable"=>StorageReason::StatsUnavailable,"app_missing"=>StorageReason::AppMissing,"locked"=>StorageReason::Locked,"operation_pending"=>StorageReason::OperationPending,_=>return None};
        let mut actions=Vec::new();for raw in v.get("actions")?.as_arr()?{let a=StorageAction::decode(raw)?;if actions.contains(&a){return None;}actions.push(a);}
        let stats=optional(v.get("stats"),|s|{let n=|key|s.get(key)?.as_i64().filter(|n|*n>=0);let out=StorageSizes{app_bytes:n("app_bytes")?,data_bytes:n("data_bytes")?,cache_bytes:n("cache_bytes")?};(out.data_bytes>=out.cache_bytes).then_some(out)})?;
        let operation=optional(v.get("operation"),|o|{let action=StorageAction::decode(o.get("action")?)?;if action==StorageAction::ManageSpace{return None;}Some(StorageOperation{id:StorageKey::decode(o.get("id")?)?,action,state:match o.get("state")?.as_str()?{"pending"=>StorageOperationState::Pending,"succeeded"=>StorageOperationState::Succeeded,"failed"=>StorageOperationState::Failed,"uncertain"=>StorageOperationState::Uncertain,_=>return None}})})?;
        let out=Self{request_id:v.get("request_id")?.as_i64().filter(|n|*n>0)?,target:AppTarget::decode(v.get("package")?)?,availability,reason,key:optional(v.get("key"),StorageKey::decode)?,label:optional(v.get("label"),|v|{let s=v.as_str()?;(!s.trim().is_empty()&&s.chars().count()<=256&&!s.chars().any(char::is_control)).then(||s.into())})?,shared_uid:v.get("shared_uid")?.as_bool()?,stats,actions,operation};
        if availability!=StorageAvailability::Available{if out.key.is_some()||out.label.is_some()||out.stats.is_some()||out.shared_uid||!out.actions.is_empty(){return None;}}
        else if out.key.is_none(){return None;}
        if !out.actions.is_empty()&&(out.shared_uid||out.reason!=StorageReason::None||out.stats.is_none()||out.operation.as_ref().is_some_and(|o|o.state==StorageOperationState::Pending)){return None;}
        if out.actions.contains(&StorageAction::ClearData)&&out.actions.contains(&StorageAction::ManageSpace){return None;}
        if out.actions.contains(&StorageAction::ClearCache)&&out.stats?.cache_bytes==0{return None;}
        if (out.actions.contains(&StorageAction::ClearData)||out.actions.contains(&StorageAction::ManageSpace))&&out.stats?.data_bytes<=out.stats?.cache_bytes{return None;}
        Some(out)
    }
    pub fn permits(&self,request:&AppStorageRequest)->bool{let AppStorageRequest::Action{target,key,action}=request else{return false;};self.availability==StorageAvailability::Available&&self.target==*target&&self.key.as_ref()==Some(key)&&self.actions.contains(action)}
    pub fn clear_actions(&mut self){self.actions.clear();}
}
#[cfg(test)]pub(crate) mod tests{
    use super::*;
    pub fn snapshot(id:i64)->Value{makepad_strict_json::parse(format!(r#"{{"schema":1,"request_id":{id},"package":"fixture.storage","availability":"available","reason":"none","key":"{:064x}","label":"Fixture","shared_uid":false,"stats":{{"app_bytes":100,"data_bytes":300,"cache_bytes":100}},"actions":["clear_cache","clear_data"],"operation":null}}"#,1).as_bytes()).unwrap()}
    #[test]fn storage_requires_current_observation_and_native_completion_is_separate(){let mut s=AppStorageSnapshot::decode(&snapshot(1)).unwrap();let r=AppStorageRequest::Action{target:s.target.clone(),key:s.key.clone().unwrap(),action:StorageAction::ClearData};assert!(s.permits(&r));assert!(s.operation.is_none());s.clear_actions();assert!(!s.permits(&r));assert_eq!(s.stats.unwrap().data_bytes,300);}
    #[test]fn storage_rejects_inconsistent_sizes_and_destructive_capabilities(){use crate::settings_updates::tests::field;let mut raw=snapshot(1);field(&mut raw,"shared_uid",Value::Bool(true));assert!(AppStorageSnapshot::decode(&raw).is_none());let mut raw=snapshot(1);field(&mut raw,"stats",Value::Null);assert!(AppStorageSnapshot::decode(&raw).is_none());let mut raw=snapshot(1);field(&mut raw,"operation",makepad_strict_json::parse(format!(r#"{{"id":"{:064x}","action":"clear_data","state":"pending"}}"#,9).as_bytes()).unwrap());assert!(AppStorageSnapshot::decode(&raw).is_none());field(&mut raw,"actions",Value::Arr(vec![]));assert_eq!(AppStorageSnapshot::decode(&raw).unwrap().operation.unwrap().state,StorageOperationState::Pending);}
}
