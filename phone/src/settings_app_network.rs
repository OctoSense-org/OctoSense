//! Per-app network controls are observations of the package's Android UID policy.
use crate::settings_apps::AppTarget;
use makepad_strict_json::Value;

#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum NetworkField { Background, Unrestricted, Network, Wifi, Mobile, Vpn }
impl NetworkField {
    pub const ALL:[Self;6]=[Self::Background,Self::Unrestricted,Self::Network,Self::Wifi,Self::Mobile,Self::Vpn];
    pub fn wire(self)->&'static str {match self{Self::Background=>"background",Self::Unrestricted=>"unrestricted",Self::Network=>"network",Self::Wifi=>"wifi",Self::Mobile=>"mobile",Self::Vpn=>"vpn"}}
    pub fn label(self)->&'static str {match self{Self::Background=>"Background data",Self::Unrestricted=>"Unrestricted data with Data Saver",Self::Network=>"Network access",Self::Wifi=>"Wi-Fi access",Self::Mobile=>"Mobile data access",Self::Vpn=>"VPN access"}}
    fn decode(v:&Value)->Option<Self>{Self::ALL.into_iter().find(|f|Some(f.wire())==v.as_str())}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct AppNetworkKey(String);
impl AppNetworkKey {
    pub fn wire(&self)->&str{&self.0}
    fn decode(v:&Value)->Option<Self>{let s=v.as_str()?;crate::settings_apps::valid_generation(s).then(||Self(s.into()))}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct AppNetworkRead {pub package:AppTarget}
impl AppNetworkRead {
    pub fn valid(&self)->bool{true}
    pub fn accepts(&self,s:&AppNetworkSnapshot)->bool{self.package==s.package}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub enum AppNetworkRequest {
    Snapshot(AppNetworkRead),
    Set{package:AppTarget,key:AppNetworkKey,field:NetworkField,enabled:bool},
}
impl AppNetworkRequest {pub fn valid(&self)->bool{true}}
#[derive(serde::Serialize, Clone,Debug)]
pub struct AppNetworkControl {pub field:NetworkField,pub value:Option<bool>,pub can_set:bool}
#[derive(serde::Serialize, Clone,Debug)]
pub struct AppNetworkSnapshot {
    pub request_id:i64,pub package:AppTarget,pub availability:String,pub key:Option<AppNetworkKey>,
    pub label:String,pub packages:Vec<AppTarget>,pub data_saver:Option<bool>,pub controls:Vec<AppNetworkControl>,
}
fn boolean(v:&Value)->Option<Option<bool>>{match v{Value::Null=>Some(None),_=>Some(Some(v.as_bool()?))}}
impl AppNetworkSnapshot {
    pub fn decode(v:&Value)->Option<Self>{
        if v.get("schema")?.as_i64()?!=1{return None;}
        let availability=v.get("availability")?.as_str()?;
        if !["available","unavailable","restricted","missing"].contains(&availability){return None;}
        let available=availability=="available";
        let key=match v.get("key")?{Value::Null=>None,v=>Some(AppNetworkKey::decode(v)?)};
        if available!=key.is_some(){return None;}
        let label=v.get("label")?.as_str()?.to_owned();if label.chars().count()>1024{return None;}
        let raw=v.get("packages")?.as_arr()?;if raw.len()>50||available==raw.is_empty(){return None;}
        let mut packages=Vec::new();for p in raw{let p=AppTarget::decode(p)?;if packages.contains(&p){return None;}packages.push(p);}
        let package=AppTarget::decode(v.get("package")?)?;if available&&!packages.contains(&package){return None;}
        let data_saver=boolean(v.get("data_saver")?)?;if available!=data_saver.is_some(){return None;}
        let raw=v.get("controls")?.as_arr()?;if raw.len()!=6{return None;}
        let mut controls=Vec::new();for (i,r) in raw.iter().enumerate(){
            let field=NetworkField::decode(r.get("field")?)?;if field!=NetworkField::ALL[i]{return None;}
            let value=boolean(r.get("value")?)?;let can_set=r.get("can_set")?.as_bool()?;
            if (!available&&(value.is_some()||can_set))||(can_set&&value.is_none()){return None;}
            controls.push(AppNetworkControl{field,value,can_set});
        }
        Some(Self{request_id:v.get("request_id")?.as_i64().filter(|n|*n>0)?,package,availability:availability.into(),key,label,packages,data_saver,controls})
    }
    pub fn permits(&self,r:&AppNetworkRequest)->bool{match r{
        AppNetworkRequest::Snapshot(read)=>read.accepts(self),
        AppNetworkRequest::Set{package,key,field,enabled}=>self.availability=="available"&&package==&self.package&&self.key.as_ref()==Some(key)
            &&self.controls.iter().any(|r|r.field==*field&&r.can_set&&r.value==Some(!enabled)),
    }}
    pub fn clear_actions(&mut self){for row in &mut self.controls{row.can_set=false;}}
}

#[cfg(test)]pub(crate) mod tests{
    use super::*;
    use makepad_strict_json::{obj,s};
    pub fn snapshot(id:i64)->Value{obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(id)),("package",s("fixture.network")),("availability",s("available")),("key",s("a".repeat(64))),("label",s("Network fixture")),("packages",Value::Arr(vec![s("fixture.network")])),("data_saver",Value::Bool(false)),("controls",Value::Arr(NetworkField::ALL.into_iter().enumerate().map(|(i,f)|obj(vec![("field",s(f.wire())),("value",if i<2{Value::Bool(i==0)}else{Value::Null}),("can_set",Value::Bool(i<2))])).collect()))])}
    #[test]fn selected_unknown_changed_and_retired_choices_never_authorize_writes(){
        let mut s=AppNetworkSnapshot::decode(&snapshot(1)).unwrap();
        let request=AppNetworkRequest::Set{package:s.package.clone(),key:s.key.clone().unwrap(),field:NetworkField::Background,enabled:false};assert!(s.permits(&request));
        let mut same=request.clone();if let AppNetworkRequest::Set{enabled,..}=&mut same{*enabled=true;}assert!(!s.permits(&same));
        let mut unsupported=request.clone();if let AppNetworkRequest::Set{field,..}=&mut unsupported{*field=NetworkField::Wifi;}assert!(!s.permits(&unsupported));
        s.key=Some(AppNetworkKey("b".repeat(64)));assert!(!s.permits(&request));s.key=Some(AppNetworkKey("a".repeat(64)));s.clear_actions();assert!(!s.permits(&request));assert_eq!(s.controls[0].value,Some(true));
    }
    #[test]fn partial_or_wrong_uid_observations_are_rejected(){
        for (key,value) in [("packages",Value::Arr(vec![s("different.package")])),("controls",Value::Arr(vec![])),("data_saver",Value::Null),("availability",s("unavailable")),("key",Value::Null)]{
            let mut v=snapshot(1);crate::settings_updates::tests::field(&mut v,key,value);assert!(AppNetworkSnapshot::decode(&v).is_none(),"{key}");
        }
    }
}
