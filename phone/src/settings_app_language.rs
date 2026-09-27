//! Bounded native per-app locale hierarchy. Scripts never choose arbitrary locale tags.
use crate::settings_apps::AppTarget;
use makepad_strict_json::Value;
pub const PAGE_SIZE:u32=20;
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]pub struct LanguageKey(String);
impl LanguageKey{pub fn wire(&self)->&str{&self.0}pub(crate) fn decode(v:&Value)->Option<Self>{let s=v.as_str()?;(s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))).then(||Self(s.into()))}}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]pub struct LanguageRead{pub target:AppTarget,pub key:Option<LanguageKey>,pub parent:Option<LanguageKey>,pub query:String,pub offset:u32}
impl LanguageRead{
    pub fn root(target:AppTarget)->Self{Self{target,key:None,parent:None,query:String::new(),offset:0}}
    pub fn valid(&self)->bool{self.query.chars().count()<=128&&!self.query.chars().any(char::is_control)&&self.offset<=1020&&self.offset%PAGE_SIZE==0&&(self.key.is_some()||(self.parent.is_none()&&self.offset==0))}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]pub enum AppLanguageRequest{Snapshot(LanguageRead),Native(AppTarget),Select{target:AppTarget,key:LanguageKey,choice:LanguageKey}}
impl AppLanguageRequest{pub fn valid(&self)->bool{match self{Self::Snapshot(r)=>r.valid(),Self::Select{..}|Self::Native(_)=>true}}}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]pub enum LanguageAvailability{Available,Restricted,Unsupported,Unavailable,Missing,Stale}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]pub enum LanguageReason{None,Locked,AppMissing,NotSupported,ServiceUnavailable,CatalogExpired,TargetChanged,CatalogLimit,InvalidConfig}
impl LanguageReason{pub fn label(self)->&'static str{match self{Self::None=>"",Self::Locked=>"Unlock the device to change app language.",Self::AppMissing=>"This app is no longer installed.",Self::NotSupported=>"Android does not offer app language choices for this app.",Self::ServiceUnavailable=>"App language controls are unavailable on this installation. Android language settings remain available below.",Self::CatalogExpired=>"These language choices expired. Refresh to review the current choices.",Self::TargetChanged=>"This app or its language configuration changed. Refresh to review the current choices.",Self::CatalogLimit=>"Android’s language catalogue exceeds the supported size. Open Android language settings to continue.",Self::InvalidConfig=>"This app’s language configuration could not be read. Open Android language settings to continue."}}}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]pub enum LanguageLevel{Language,Region,Numbering}
impl LanguageLevel{fn decode(v:&Value)->Option<Self>{Some(match v.as_str()?{"language"=>Self::Language,"region"=>Self::Region,"numbering"=>Self::Numbering,_=>return None})}pub fn label(self)->&'static str{match self{Self::Language=>"Languages",Self::Region=>"Regions",Self::Numbering=>"Numbering systems"}}}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]pub enum LanguageKind{Open,Select}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]pub struct CurrentLanguage{pub tag:String,pub label:String}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]pub struct LanguageRow{pub key:LanguageKey,pub label:String,pub secondary:Option<String>,pub kind:LanguageKind,pub selected:bool,pub system_default:bool,pub suggested:bool}
#[derive(serde::Serialize, Clone,Debug)]pub struct AppLanguageSnapshot{
    pub request_id:i64,pub target:AppTarget,pub availability:LanguageAvailability,pub reason:LanguageReason,pub key:Option<LanguageKey>,pub parent:Option<LanguageKey>,pub parent_label:Option<String>,pub label:Option<String>,pub current:Option<Vec<CurrentLanguage>>,pub system_default:Option<bool>,pub can_set:bool,pub level:LanguageLevel,pub query:String,pub offset:u32,pub total:u32,pub rows:Vec<LanguageRow>,
}
fn text(v:&Value,max:usize)->Option<String>{let s=v.as_str()?;(!s.trim().is_empty()&&s.chars().count()<=max&&!s.chars().any(char::is_control)).then(||s.into())}
fn optional<T>(v:Option<&Value>,f:impl FnOnce(&Value)->Option<T>)->Option<Option<T>>{match v{None|Some(Value::Null)=>Some(None),Some(v)=>f(v).map(Some)}}
impl AppLanguageSnapshot{
    pub fn decode(v:&Value)->Option<Self>{
        if v.get("schema")?.as_i64()?!=1||v.get("page_size")?.as_i64()?!=20{return None;}
        let availability=match v.get("availability")?.as_str()?{"available"=>LanguageAvailability::Available,"restricted"=>LanguageAvailability::Restricted,"unsupported"=>LanguageAvailability::Unsupported,"unavailable"=>LanguageAvailability::Unavailable,"missing"=>LanguageAvailability::Missing,"stale"=>LanguageAvailability::Stale,_=>return None};
        let reason=match v.get("reason")?.as_str()?{"none"=>LanguageReason::None,"locked"=>LanguageReason::Locked,"app_missing"=>LanguageReason::AppMissing,"not_supported"=>LanguageReason::NotSupported,"service_unavailable"=>LanguageReason::ServiceUnavailable,"catalog_expired"=>LanguageReason::CatalogExpired,"target_changed"=>LanguageReason::TargetChanged,"catalog_limit"=>LanguageReason::CatalogLimit,"invalid_config"=>LanguageReason::InvalidConfig,_=>return None};
        let n=|name:&str,max:i64|v.get(name)?.as_i64().filter(|n|*n>=0&&*n<=max).map(|n|n as u32);
        let mut rows=Vec::<LanguageRow>::new();for raw in v.get("rows")?.as_arr()?{let row=LanguageRow{key:LanguageKey::decode(raw.get("key")?)?,label:text(raw.get("label")?,256)?,secondary:optional(raw.get("secondary"),|v|text(v,256))?,kind:match raw.get("kind")?.as_str()?{"open"=>LanguageKind::Open,"select"=>LanguageKind::Select,_=>return None},selected:raw.get("selected")?.as_bool()?,system_default:raw.get("system_default")?.as_bool()?,suggested:raw.get("suggested")?.as_bool()?};if rows.iter().any(|r|r.key==row.key)||(row.kind==LanguageKind::Open&&(row.selected||row.system_default)){return None;}rows.push(row);}
        let current=optional(v.get("current"),|v|{let raw=v.as_arr()?;if raw.len()>128{return None;}raw.iter().map(|v|Some(CurrentLanguage{tag:text(v.get("tag")?,256)?,label:text(v.get("label")?,256)?})).collect::<Option<Vec<_>>>()})?;
        let out=Self{request_id:v.get("request_id")?.as_i64().filter(|n|*n>0)?,target:AppTarget::decode(v.get("package")?)?,availability,reason,key:optional(v.get("key"),LanguageKey::decode)?,parent:optional(v.get("parent"),LanguageKey::decode)?,parent_label:optional(v.get("parent_label"),|v|text(v,256))?,label:optional(v.get("label"),|v|text(v,256))?,current,system_default:optional(v.get("system_default"),Value::as_bool)?,can_set:v.get("can_set")?.as_bool()?,level:LanguageLevel::decode(v.get("level")?)?,query:v.get("query")?.as_str()?.into(),offset:n("offset",1020)?,total:n("total",1024)?,rows};
        if out.query.chars().count()>128||out.query.chars().any(char::is_control)||out.offset%20!=0{return None;}
        if availability==LanguageAvailability::Available{
            if out.key.is_none()||out.label.is_none()||out.reason!=LanguageReason::None||out.system_default!=Some(out.current.as_ref()?.is_empty())||out.parent.is_some()!=out.parent_label.is_some()||out.parent.is_none()!=(out.level==LanguageLevel::Language){return None;}
            if out.offset>=out.total&&(out.offset!=0||out.total!=0){return None;}if out.rows.len()!=(out.total-out.offset).min(20)as usize{return None;}
        }else if out.key.is_some()||out.parent.is_some()||out.parent_label.is_some()||out.label.is_some()||out.current.is_some()||out.system_default.is_some()||out.can_set||out.offset!=0||out.total!=0||!out.rows.is_empty(){return None;}
        Some(out)
    }
    pub fn matches(&self,r:&LanguageRead)->bool{self.target==r.target&&self.query==r.query&&(self.availability!=LanguageAvailability::Available||(self.parent==r.parent&&(r.key.is_none()||self.key==r.key)&&(self.offset==r.offset||(self.offset==0&&r.offset>=self.total))))}
    pub fn permits(&self,r:&AppLanguageRequest)->bool{let AppLanguageRequest::Select{target,key,choice}=r else{return false;};self.availability==LanguageAvailability::Available&&self.can_set&&self.target==*target&&self.key.as_ref()==Some(key)&&self.rows.iter().any(|row|row.key==*choice&&row.kind==LanguageKind::Select)}
    pub fn clear_actions(&mut self){self.can_set=false;}
}
#[cfg(test)]pub(crate) mod tests{
    use super::*;
    pub fn snapshot(id:i64)->Value{makepad_strict_json::parse(format!(r#"{{"schema":1,"request_id":{id},"package":"fixture.language","availability":"available","reason":"none","key":"{:064x}","parent":null,"label":"Fixture","current":[{{"tag":"fr-CA","label":"French (Canada)"}},{{"tag":"zh-Hant-TW","label":"Traditional Chinese"}}],"system_default":false,"can_set":true,"level":"language","query":"","offset":0,"total":2,"page_size":20,"rows":[{{"key":"{:064x}","label":"System default","kind":"select","selected":false,"system_default":true,"suggested":false}},{{"key":"{:064x}","label":"French","kind":"open","selected":false,"system_default":false,"suggested":false}}]}}"#,1,2,3).as_bytes()).unwrap()}
    #[test]fn native_language_choices_only_and_complete_custom_list(){let mut s=AppLanguageSnapshot::decode(&snapshot(1)).unwrap();assert_eq!(s.current.as_ref().unwrap().len(),2);let request=AppLanguageRequest::Select{target:s.target.clone(),key:s.key.clone().unwrap(),choice:s.rows[0].key.clone()};assert!(s.permits(&request));let mut folder=request.clone();if let AppLanguageRequest::Select{choice,..}=&mut folder{*choice=s.rows[1].key.clone();}assert!(!s.permits(&folder));s.clear_actions();assert!(!s.permits(&request));assert_eq!(s.current.unwrap()[1].tag,"zh-Hant-TW");}
    #[test]fn native_language_page_integrity_and_no_fabricated_default(){use crate::settings_updates::tests::field;let mut v=snapshot(1);field(&mut v,"total",Value::Int(3));assert!(AppLanguageSnapshot::decode(&v).is_none());let mut v=snapshot(1);field(&mut v,"system_default",Value::Bool(true));assert!(AppLanguageSnapshot::decode(&v).is_none());let mut v=snapshot(1);field(&mut v,"availability",makepad_strict_json::s("unsupported"));assert!(AppLanguageSnapshot::decode(&v).is_none());}
}
