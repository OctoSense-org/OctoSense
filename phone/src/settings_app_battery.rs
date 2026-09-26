//! Per-app battery policy, bound to a current observed Android package incarnation.
use crate::settings_apps::AppTarget;
use makepad_strict_json::Value;

#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct BatteryKey(String);
impl BatteryKey {
    pub fn wire(&self)->&str{&self.0}
    fn decode(value:&Value)->Option<Self>{let value=value.as_str()?;(value.len()==64&&value.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))).then(||Self(value.into()))}
}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum BatteryMode {Restricted,Optimized,Unrestricted}
impl BatteryMode {
    pub const ALL:[Self;3]=[Self::Restricted,Self::Optimized,Self::Unrestricted];
    pub fn wire(self)->&'static str{match self{Self::Restricted=>"restricted",Self::Optimized=>"optimized",Self::Unrestricted=>"unrestricted"}}
    pub fn label(self)->&'static str{match self{Self::Restricted=>"Restricted",Self::Optimized=>"Optimized",Self::Unrestricted=>"Unrestricted"}}
    fn decode(value:&Value)->Option<Self>{Self::ALL.into_iter().find(|mode|Some(mode.wire())==value.as_str())}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub enum AppBatteryRequest {Snapshot(AppTarget),Set{target:AppTarget,key:BatteryKey,mode:BatteryMode}}
impl AppBatteryRequest {pub fn valid(&self)->bool{true}}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum BatteryAvailability {Available,Restricted,Unavailable,Missing}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum BatteryReason {None,DevicePolicy,ProtectedApp,SharedUid,ServiceUnavailable,AppMissing,Locked}
impl BatteryReason {
    pub fn label(self)->&'static str{match self{
        Self::None=>"",Self::DevicePolicy=>"Your administrator restricts changes to this app.",
        Self::ProtectedApp=>"Android manages the battery policy for this protected or essential app.",
        Self::SharedUid=>"This app shares an Android identity. Battery policy changes are unavailable here because they can affect other apps.",
        Self::ServiceUnavailable=>"App battery controls are unavailable on this installation. Android app settings remain available below.",
        Self::AppMissing=>"This app is no longer installed.",Self::Locked=>"Unlock the device to review this app’s battery policy.",
    }}
}
#[derive(serde::Serialize, Clone,Debug)]
pub struct AppBatterySnapshot {
    pub request_id:i64,pub target:AppTarget,pub availability:BatteryAvailability,pub reason:BatteryReason,
    pub key:Option<BatteryKey>,pub label:Option<String>,pub mode:Option<BatteryMode>,pub custom:bool,pub shared_uid:bool,pub choices:Vec<BatteryMode>,
}
fn optional<T>(value:Option<&Value>,decode:impl FnOnce(&Value)->Option<T>)->Option<Option<T>>{match value{None|Some(Value::Null)=>Some(None),Some(value)=>decode(value).map(Some)}}
impl AppBatterySnapshot {
    pub fn decode(value:&Value)->Option<Self>{
        if value.get("schema")?.as_i64()?!=1{return None;}
        let availability=match value.get("availability")?.as_str()?{"available"=>BatteryAvailability::Available,"restricted"=>BatteryAvailability::Restricted,"unavailable"=>BatteryAvailability::Unavailable,"missing"=>BatteryAvailability::Missing,_=>return None};
        let reason=match value.get("reason")?.as_str()?{"none"=>BatteryReason::None,"device_policy"=>BatteryReason::DevicePolicy,"protected_app"=>BatteryReason::ProtectedApp,"shared_uid"=>BatteryReason::SharedUid,"service_unavailable"=>BatteryReason::ServiceUnavailable,"app_missing"=>BatteryReason::AppMissing,"locked"=>BatteryReason::Locked,_=>return None};
        let raw_mode=optional(value.get("mode"),|value|value.as_str().map(str::to_owned))?;
        let custom=raw_mode.as_deref()==Some("custom");let mode=if custom{None}else{optional(value.get("mode"),BatteryMode::decode)?};
        let mut choices=Vec::new();for item in value.get("choices")?.as_arr()?{let item=BatteryMode::decode(item)?;if choices.contains(&item){return None;}choices.push(item);}
        let state=Self{request_id:value.get("request_id")?.as_i64().filter(|n|*n>0)?,target:AppTarget::decode(value.get("package")?)?,availability,reason,
            key:optional(value.get("key"),BatteryKey::decode)?,label:optional(value.get("label"),|value|{let value=value.as_str()?;(!value.trim().is_empty()&&value.chars().count()<=256&&!value.chars().any(char::is_control)).then(||value.to_owned())})?,mode,custom,shared_uid:value.get("shared_uid")?.as_bool()?,choices};
        let can_set=value.get("can_set")?.as_bool()?;
        if can_set!=(state.choices.len()==3)||!can_set&&!state.choices.is_empty(){return None;}
        if state.availability!=BatteryAvailability::Available {
            if state.key.is_some()||state.label.is_some()||state.mode.is_some()||state.custom||can_set||state.shared_uid{return None;}
        }else if state.key.is_none()||state.mode.is_none()&&!state.custom||can_set&&(state.shared_uid||state.reason!=BatteryReason::None){return None;}
        Some(state)
    }
    pub fn permits(&self,request:&AppBatteryRequest)->bool{
        let AppBatteryRequest::Set{target,key,mode}=request else{return false;};
        self.availability==BatteryAvailability::Available&&target==&self.target&&Some(key)==self.key.as_ref()&&self.choices.contains(mode)&&self.mode!=Some(*mode)
    }
    pub fn clear_actions(&mut self){self.choices.clear();}
    pub fn mode_label(&self)->&'static str{self.mode.map(BatteryMode::label).unwrap_or(if self.custom{"Custom Android state"}else{"Unavailable"})}
}
#[cfg(test)]pub(crate) mod tests {
    use super::*;
    pub fn snapshot(id:i64)->Value{makepad_strict_json::parse(format!(r#"{{"schema":1,"request_id":{id},"package":"com.example.test","availability":"available","reason":"none","key":"{:064x}","label":"Example","mode":"optimized","shared_uid":false,"can_set":true,"choices":["restricted","optimized","unrestricted"]}}"#,1).as_bytes()).unwrap()}
    #[test]fn battery_policy_requires_current_package_key_and_observed_choices(){let mut s=AppBatterySnapshot::decode(&snapshot(1)).unwrap();let r=AppBatteryRequest::Set{target:s.target.clone(),key:s.key.clone().unwrap(),mode:BatteryMode::Restricted};assert!(s.permits(&r));
        s.key=Some(BatteryKey("f".repeat(64)));assert!(!s.permits(&r));s.clear_actions();assert!(s.choices.is_empty());assert_eq!(s.mode,Some(BatteryMode::Optimized));}
    #[test]fn battery_custom_and_unavailable_never_invent_a_selected_mode(){let field=crate::settings_updates::tests::field;let mut raw=snapshot(1);field(&mut raw,"mode",makepad_strict_json::s("custom"));let s=AppBatterySnapshot::decode(&raw).unwrap();assert!(s.custom&&s.mode.is_none());
        field(&mut raw,"availability",makepad_strict_json::s("unavailable"));assert!(AppBatterySnapshot::decode(&raw).is_none());let mut raw=snapshot(1);field(&mut raw,"shared_uid",Value::Bool(true));assert!(AppBatterySnapshot::decode(&raw).is_none());
        let mut raw=snapshot(1);field(&mut raw,"mode",makepad_strict_json::s("raw_appop"));assert!(AppBatterySnapshot::decode(&raw).is_none());}
}
