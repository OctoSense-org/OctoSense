//! Native runtime permission observations and finite platform-operation requests.
use crate::settings_apps::AppTarget;
use makepad_strict_json::Value;
pub const PAGE_SIZE:u32=20;
pub const MAX_GROUPS:u32=64;
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq,Hash)]
pub enum PermissionGroup{Camera,Microphone,Location,Contacts,Calendar,Phone,CallLog,Sms,NearbyDevices,ActivityRecognition,Sensors}
impl PermissionGroup{
    pub const ALL:[Self;11]=[Self::Camera,Self::Microphone,Self::Location,Self::Contacts,Self::Calendar,Self::Phone,Self::CallLog,Self::Sms,Self::NearbyDevices,Self::ActivityRecognition,Self::Sensors];
    pub fn wire(self)->&'static str{match self{Self::Camera=>"camera",Self::Microphone=>"microphone",Self::Location=>"location",Self::Contacts=>"contacts",Self::Calendar=>"calendar",Self::Phone=>"phone",Self::CallLog=>"call_log",Self::Sms=>"sms",Self::NearbyDevices=>"nearby_devices",Self::ActivityRecognition=>"activity_recognition",Self::Sensors=>"sensors"}}
    pub fn label(self)->&'static str{match self{Self::Camera=>"Camera",Self::Microphone=>"Microphone",Self::Location=>"Location",Self::Contacts=>"Contacts",Self::Calendar=>"Calendar",Self::Phone=>"Phone",Self::CallLog=>"Call logs",Self::Sms=>"SMS",Self::NearbyDevices=>"Nearby devices",Self::ActivityRecognition=>"Physical activity",Self::Sensors=>"Body sensors"}}
    fn decode(v:&Value)->Option<Self>{Self::ALL.into_iter().find(|g|Some(g.wire())==v.as_str())}
}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum PermissionChoice{Allow,AllowAlways,AllowForeground,Ask,OneTime,Deny,DenyForeground,Precise,Approximate}
impl PermissionChoice{
    pub const ALL:[Self;9]=[Self::Allow,Self::AllowAlways,Self::AllowForeground,Self::Ask,Self::OneTime,Self::Deny,Self::DenyForeground,Self::Precise,Self::Approximate];
    pub fn wire(self)->&'static str{match self{Self::Allow=>"allow",Self::AllowAlways=>"allow_always",Self::AllowForeground=>"allow_foreground",Self::Ask=>"ask",Self::OneTime=>"one_time",Self::Deny=>"deny",Self::DenyForeground=>"deny_foreground",Self::Precise=>"precise",Self::Approximate=>"approximate"}}
    fn decode(v:&Value)->Option<Self>{Self::ALL.into_iter().find(|c|Some(c.wire())==v.as_str())}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct PermissionKey(String);
impl PermissionKey{pub fn wire(&self)->&str{&self.0}pub(crate) fn decode(v:&Value)->Option<Self>{let s=v.as_str()?;(s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))).then(||Self(s.into()))}}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct PermissionsRead{pub package:AppTarget,pub group:Option<PermissionGroup>,pub offset:u32,pub generation:Option<PermissionKey>}
impl PermissionsRead{
    pub fn valid(&self)->bool{self.offset<MAX_GROUPS&&self.offset%PAGE_SIZE==0&&(self.offset==0||self.generation.is_some())&&(self.group.is_none()||self.offset==0&&self.generation.is_none())}
    pub fn accepts(&self,s:&PermissionsSnapshot)->bool{self.package==s.package&&self.group==s.group&&(s.availability!=Availability::Available||s.exists!=Some(true)||self.group.is_some()||if s.stale{self.generation.is_some()&&s.offset==0}else{s.offset==self.offset&&self.generation.as_ref().is_none_or(|g|Some(g)==s.generation.as_ref())})}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub enum PermissionsRequest{Snapshot(PermissionsRead),Choose{package:AppTarget,group:PermissionGroup,key:PermissionKey,target:PermissionKey}}
impl PermissionsRequest{pub fn valid(&self)->bool{match self{Self::Snapshot(read)=>read.valid(),Self::Choose{..}=>true}}}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum Availability{Available,Unsupported,Restricted,Unavailable}
impl Availability{
    fn decode(v:&Value)->Option<Self>{Some(match v.as_str()?{"available"=>Self::Available,"unsupported"=>Self::Unsupported,"restricted"=>Self::Restricted,"unavailable"=>Self::Unavailable,_=>return None})}
    pub fn label(self)->&'static str{match self{Self::Available=>"",Self::Unsupported=>"This permission group is not available for this app.",Self::Restricted=>"Permission choices are unavailable while locked or restricted.",Self::Unavailable=>"Permission controls are unavailable on this installation. Android app settings remain available below."}}
}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum PermissionCategory{Allowed,Ask,Denied}
impl PermissionCategory{fn decode(v:&Value)->Option<Self>{Some(match v.as_str()?{"allowed"=>Self::Allowed,"ask"=>Self::Ask,"denied"=>Self::Denied,_=>return None})}pub fn label(self)->&'static str{match self{Self::Allowed=>"Allowed",Self::Ask=>"Ask every time",Self::Denied=>"Not allowed"}}}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct PermissionRow{pub target:PermissionKey,pub group:Option<PermissionGroup>,pub label:String,pub category:PermissionCategory,pub subtitle:Option<String>}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct PermissionOption{pub choice:PermissionChoice,pub label:String,pub selected:bool,pub enabled:bool,pub target:Option<PermissionKey>}
#[derive(serde::Serialize, Clone,Debug)]
pub struct PermissionsSnapshot{
    pub request_id:i64,pub package:AppTarget,pub exists:Option<bool>,pub app_label:Option<String>,pub availability:Availability,pub group:Option<PermissionGroup>,
    pub key:Option<PermissionKey>,pub generation:Option<PermissionKey>,pub offset:u32,pub total:u32,pub stale:bool,pub truncated:bool,
    pub groups:Vec<PermissionRow>,pub choices:Vec<PermissionOption>,pub detail:Option<String>,
}
fn text(v:&Value,limit:usize)->Option<String>{let s=v.as_str()?;(!s.is_empty()&&s.chars().count()<=limit&&!s.chars().any(char::is_control)).then(||s.into())}
fn optional<T>(v:Option<&Value>,decode:impl FnOnce(&Value)->Option<T>)->Option<Option<T>>{match v{None|Some(Value::Null)=>Some(None),Some(v)=>decode(v).map(Some)}}
fn count(v:&Value)->Option<u32>{u32::try_from(v.as_i64()?).ok().filter(|n|*n<=MAX_GROUPS)}
impl PermissionsSnapshot{
    pub fn decode(v:&Value)->Option<Self>{
        if v.get("schema")?.as_i64()?!=1{return None;}
        let rows=v.get("groups")?.as_arr()?;if rows.len()>PAGE_SIZE as usize{return None;}let mut groups=Vec::new();
        for row in rows{let row=PermissionRow{target:PermissionKey::decode(row.get("target")?)?,group:optional(row.get("group"),PermissionGroup::decode)?,label:text(row.get("label")?,256)?,category:PermissionCategory::decode(row.get("category")?)?,subtitle:optional(row.get("subtitle"),|s|text(s,256))?};
            if groups.iter().any(|r:&PermissionRow|r.target==row.target||row.group.is_some()&&r.group==row.group){return None;}groups.push(row);}
        let rows=v.get("choices")?.as_arr()?;if rows.len()>PermissionChoice::ALL.len(){return None;}let mut choices=Vec::new();
        for row in rows{let row=PermissionOption{choice:PermissionChoice::decode(row.get("choice")?)?,label:text(row.get("label")?,256)?,selected:row.get("selected")?.as_bool()?,enabled:row.get("enabled")?.as_bool()?,target:optional(row.get("target"),PermissionKey::decode)?};
            if row.enabled&&(row.selected||row.choice==PermissionChoice::OneTime)||row.enabled!=row.target.is_some()||choices.iter().any(|r:&PermissionOption|r.choice==row.choice||row.target.is_some()&&r.target==row.target){return None;}choices.push(row);}
        let state=Self{request_id:v.get("request_id")?.as_i64().filter(|n|*n>0)?,package:AppTarget::decode(v.get("package")?)?,exists:optional(v.get("exists"),Value::as_bool)?,app_label:optional(v.get("app_label"),|s|text(s,256))?,availability:Availability::decode(v.get("availability")?)?,group:optional(v.get("group"),PermissionGroup::decode)?,key:optional(v.get("key"),PermissionKey::decode)?,generation:optional(v.get("generation"),PermissionKey::decode)?,offset:count(v.get("offset")?)?,total:count(v.get("total")?)?,stale:v.get("stale")?.as_bool()?,truncated:v.get("truncated")?.as_bool()?,groups,choices,detail:optional(v.get("detail"),|s|text(s,512))?};
        let no_page=state.generation.is_none()&&state.offset==0&&state.total==0&&!state.stale&&!state.truncated&&state.groups.is_empty();
        if state.availability!=Availability::Available{return (no_page&&state.choices.is_empty()&&state.key.is_none()).then_some(state);}
        match state.exists{None=>return None,Some(false)=>return (no_page&&state.choices.is_empty()&&state.key.is_none()&&state.app_label.is_none()).then_some(state),Some(true)=>{state.app_label.as_ref()?;}}
        if state.group.is_some(){return (no_page&&state.key.is_some()).then_some(state);}
        if !state.choices.is_empty()||state.key.is_some()||state.generation.is_none()||state.offset>=MAX_GROUPS||state.offset%PAGE_SIZE!=0||state.stale&&state.offset!=0{return None;}
        if state.total==0{if state.offset!=0||!state.groups.is_empty(){return None;}}else if state.offset>=state.total||state.groups.len()!=(state.total-state.offset).min(PAGE_SIZE) as usize{return None;}
        Some(state)
    }
    pub fn permits(&self,request:&PermissionsRequest)->bool{let PermissionsRequest::Choose{package,group,key,target}=request else{return true;};
        self.availability==Availability::Available&&self.exists==Some(true)&&self.package==*package&&self.group==Some(*group)&&self.key.as_ref()==Some(key)&&self.choices.iter().any(|o|o.enabled&&o.target.as_ref()==Some(target))}
    pub fn clear_actions(&mut self){for option in &mut self.choices{option.enabled=false;option.target=None;}}
}
#[cfg(test)]pub(crate) mod tests{
    use super::*;
    pub fn snapshot(id:i64,group:bool)->Value{
        let rows=if group{String::new()}else{format!(r#"{{"target":"{:064x}","group":"camera","label":"Camera","category":"denied","subtitle":null}}"#,7)};
        let choices=if group{format!(r#"{{"choice":"allow_foreground","label":"Allow only while using the app","selected":false,"enabled":true,"target":"{:064x}"}},{{"choice":"deny","label":"Don't allow","selected":true,"enabled":false,"target":null}}"#,8)}else{String::new()};
        makepad_strict_json::parse(format!(r#"{{"schema":1,"request_id":{id},"package":"fixture.permissions","exists":true,"app_label":"Fixture","availability":"available","group":{},"key":{},"generation":{},"offset":0,"total":{},"stale":false,"truncated":false,"groups":[{rows}],"choices":[{choices}],"detail":null}}"#,if group{r#""camera""#}else{"null"},if group{format!("\"{:064x}\"",1)}else{"null".into()},if group{"null".into()}else{format!("\"{:064x}\"",2)},if group{0}else{1}).as_bytes()).unwrap()
    }
    #[test]fn only_exact_observed_common_permission_choices_can_open_platform_operation(){
        let mut s=PermissionsSnapshot::decode(&snapshot(1,true)).unwrap();let request=PermissionsRequest::Choose{package:s.package.clone(),group:PermissionGroup::Camera,key:s.key.clone().unwrap(),target:s.choices[0].target.clone().unwrap()};assert!(s.permits(&request));
        let wrong=PermissionsRequest::Choose{package:s.package.clone(),group:PermissionGroup::Location,key:s.key.clone().unwrap(),target:s.choices[0].target.clone().unwrap()};assert!(!s.permits(&wrong));s.clear_actions();assert!(!s.permits(&request));assert!(s.choices[1].selected);
    }
    #[test]fn malformed_one_time_selected_and_unavailable_choices_fail_closed(){
        let field=crate::settings_updates::tests::field;
        let mut raw=snapshot(1,true);let mut rows=raw.get("choices").unwrap().as_arr().unwrap().to_vec();field(&mut rows[0],"choice",makepad_strict_json::s("one_time"));field(&mut raw,"choices",Value::Arr(rows));assert!(PermissionsSnapshot::decode(&raw).is_none());
        let mut raw=snapshot(1,true);field(&mut raw,"availability",makepad_strict_json::s("unavailable"));assert!(PermissionsSnapshot::decode(&raw).is_none());
        let mut raw=snapshot(1,false);field(&mut raw,"total",Value::Int(2));assert!(PermissionsSnapshot::decode(&raw).is_none());
        let mut raw=snapshot(1,true);field(&mut raw,"group",makepad_strict_json::s("raw_permission"));assert!(PermissionsSnapshot::decode(&raw).is_none());
    }
    #[test]fn permission_read_correlation_checks_package_group_and_generation(){
        let s=PermissionsSnapshot::decode(&snapshot(1,false)).unwrap();let read=PermissionsRead{package:s.package.clone(),group:None,offset:0,generation:None};assert!(read.valid()&&read.accepts(&s));
        let mut wrong=read.clone();wrong.group=Some(PermissionGroup::Camera);assert!(!wrong.accepts(&s));wrong.offset=20;assert!(!wrong.valid());
        let mut next=read.clone();next.offset=20;assert!(!next.valid());next.generation=s.generation.clone();assert!(next.valid()&&!next.accepts(&s));
    }
}
