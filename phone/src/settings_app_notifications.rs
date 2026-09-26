//! Observed per-app notification targets. Raw Android channel IDs never enter scripts.
use crate::settings_apps::AppTarget;
use makepad_strict_json::Value;
pub const PAGE_SIZE:u32=20;
pub const MAX_ROWS:u32=6000;
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct NotificationKey(String);
impl NotificationKey {
    pub fn wire(&self)->&str{&self.0}
    pub(crate) fn decode(v:&Value)->Option<Self>{let s=v.as_str()?;(s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))).then(||Self(s.into()))}
}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum Importance{None,Min,Low,Default,High,Unspecified}
impl Importance {
    pub const CHOICES:[Self;4]=[Self::Min,Self::Low,Self::Default,Self::High];
    pub fn wire(self)->&'static str{match self{Self::None=>"none",Self::Min=>"min",Self::Low=>"low",Self::Default=>"default",Self::High=>"high",Self::Unspecified=>"unspecified"}}
    pub fn label(self)->&'static str{match self{Self::None=>"Off",Self::Min=>"Silent, minimized",Self::Low=>"Silent",Self::Default=>"Alerting",Self::High=>"Alerting, allow pop-up",Self::Unspecified=>"App default"}}
    fn decode(v:&Value)->Option<Self>{[Self::None,Self::Min,Self::Low,Self::Default,Self::High,Self::Unspecified].into_iter().find(|i|Some(i.wire())==v.as_str())}
}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum NotificationChange{App(bool),Group(bool),Channel(bool),Importance(Importance)}
impl NotificationChange {
    pub fn action(self)->&'static str{match self{Self::App(_)=>"app_enabled",Self::Group(_)=>"group_enabled",Self::Channel(_)=>"channel_enabled",Self::Importance(_)=>"channel_importance"}}
    pub fn value(self)->&'static str{match self{Self::App(v)|Self::Group(v)|Self::Channel(v)=>if v{"on"}else{"off"},Self::Importance(v)=>v.wire()}}
    pub fn valid(self)->bool{!matches!(self,Self::Importance(Importance::None|Importance::Unspecified))}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct AppNotificationsRead{pub target:AppTarget,pub offset:u32,pub generation:Option<NotificationKey>}
impl AppNotificationsRead {
    pub fn valid(&self)->bool{self.offset<MAX_ROWS&&self.offset%PAGE_SIZE==0&&(self.offset==0||self.generation.is_some())}
    pub fn accepts(&self,s:&AppNotificationsSnapshot)->bool{self.target==s.target&&(s.availability!=Availability::Available||if s.stale{self.generation.is_some()&&s.offset==0}else{s.offset==self.offset&&self.generation.as_ref().is_none_or(|g|Some(g)==s.generation.as_ref())})}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub enum AppNotificationsRequest{
    Snapshot(AppNotificationsRead),
    Set{target:AppTarget,key:NotificationKey,item:NotificationKey,change:NotificationChange},
}
impl AppNotificationsRequest{pub fn valid(&self)->bool{match self{Self::Snapshot(r)=>r.valid(),Self::Set{change,..}=>change.valid()}}}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum Availability{Available,Unavailable,Restricted}
impl Availability {
    fn decode(v:&Value)->Option<Self>{Some(match v.as_str()?{"available"=>Self::Available,"unavailable"=>Self::Unavailable,"restricted"=>Self::Restricted,_=>return None})}
    pub fn label(self)->&'static str{match self{Self::Available=>"",Self::Unavailable=>"App notification controls are unavailable on this installation. Android notification settings remain available below.",Self::Restricted=>"Notification settings are unavailable while locked or restricted."}}
}
#[derive(serde::Serialize, Clone,Debug)]
pub struct NotificationApp{pub key:NotificationKey,pub label:String,pub enabled:bool,pub permission_requested:bool,pub permission_fixed:bool,pub importance_locked:bool,pub suspended:bool,pub can_set:bool}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum RowKind{Channel,Group}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct NotificationRow{
    pub key:NotificationKey,pub kind:RowKind,pub name:String,pub group_name:Option<String>,
    pub enabled:bool,pub effective_enabled:bool,pub can_set:bool,pub importance:Option<Importance>,
    pub options:Vec<Importance>,pub sound_changes:Vec<Importance>,pub sound:Option<String>,
    pub user_locked_importance:bool,pub group_blocked:bool,pub linked_app:bool,
}
#[derive(serde::Serialize, Clone,Debug)]
pub struct AppNotificationsSnapshot {
    pub request_id:i64,pub target:AppTarget,pub exists:Option<bool>,pub availability:Availability,
    pub key:Option<NotificationKey>,pub app:Option<NotificationApp>,pub generation:Option<NotificationKey>,
    pub offset:u32,pub total:u32,pub truncated:bool,pub stale:bool,pub rows:Vec<NotificationRow>,
}
fn text(v:&Value)->Option<String>{let s=v.as_str()?;(!s.is_empty()&&s.chars().count()<=256&&!s.chars().any(char::is_control)).then(||s.into())}
fn optional<T>(v:Option<&Value>,decode:impl FnOnce(&Value)->Option<T>)->Option<Option<T>>{match v{None|Some(Value::Null)=>Some(None),Some(v)=>decode(v).map(Some)}}
fn count(v:&Value)->Option<u32>{u32::try_from(v.as_i64()?).ok().filter(|v|*v<=MAX_ROWS)}
impl AppNotificationsSnapshot {
    pub fn decode(v:&Value)->Option<Self>{
        if v.get("schema")?.as_i64()?!=1{return None;}
        let app=optional(v.get("app"),|a|Some(NotificationApp{key:NotificationKey::decode(a.get("key")?)?,label:text(a.get("label")?)?,enabled:a.get("enabled")?.as_bool()?,permission_requested:a.get("permission_requested")?.as_bool()?,permission_fixed:a.get("permission_fixed")?.as_bool()?,importance_locked:a.get("importance_locked")?.as_bool()?,suspended:a.get("suspended")?.as_bool()?,can_set:a.get("can_set")?.as_bool()?}))?;
        let mut rows=Vec::new();let items=v.get("rows")?.as_arr()?;if items.len()>PAGE_SIZE as usize{return None;}
        for r in items {
            let kind=match r.get("kind")?.as_str()?{"channel"=>RowKind::Channel,"group"=>RowKind::Group,_=>return None};
            let choices=|key:&str|->Option<Vec<Importance>>{let mut out=Vec::new();for c in r.get(key)?.as_arr()?{let c=Importance::decode(c)?;if !Importance::CHOICES.contains(&c)||out.contains(&c){return None;}out.push(c);}Some(out)};
            let row=NotificationRow{key:NotificationKey::decode(r.get("key")?)?,kind,name:text(r.get("name")?)?,group_name:optional(r.get("group_name"),|v|if v.as_str()==Some(""){Some(String::new())}else{text(v)})?.filter(|s|!s.is_empty()),enabled:r.get("enabled")?.as_bool()?,effective_enabled:r.get("effective_enabled")?.as_bool()?,can_set:r.get("can_set")?.as_bool()?,importance:optional(r.get("importance"),Importance::decode)?,options:choices("importance_options")?,sound_changes:choices("sound_change_options")?,sound:optional(r.get("sound"),text)?,user_locked_importance:r.get("user_locked_importance")?.as_bool()?,group_blocked:r.get("group_blocked")?.as_bool()?,linked_app:r.get("linked_app")?.as_bool()?};
            if rows.iter().any(|old:&NotificationRow|old.key==row.key)||row.sound_changes.iter().any(|c|!row.options.contains(c)||!matches!(c,Importance::Default|Importance::High)){return None;}
            if !row.options.is_empty()&&(!row.can_set||!row.enabled||row.group_blocked||row.linked_app){return None;}
            if kind==RowKind::Group&&(row.importance.is_some()||!row.options.is_empty()||row.sound.is_some()||row.linked_app||row.user_locked_importance){return None;}
            if row.importance.is_some_and(|i|row.enabled==(i==Importance::None)){return None;}
            rows.push(row);
        }
        let state=Self{request_id:v.get("request_id")?.as_i64().filter(|n|*n>0)?,target:AppTarget::decode(v.get("package")?)?,exists:optional(v.get("exists"),Value::as_bool)?,availability:Availability::decode(v.get("availability")?)?,key:optional(v.get("key"),NotificationKey::decode)?,app,generation:optional(v.get("generation"),NotificationKey::decode)?,offset:count(v.get("offset")?)?,total:count(v.get("total")?)?,truncated:v.get("truncated")?.as_bool()?,stale:v.get("stale")?.as_bool()?,rows};
        if state.availability!=Availability::Available {
            return (state.exists.is_none()&&state.key.is_none()&&state.app.is_none()&&state.generation.is_none()&&state.offset==0&&state.total==0&&state.rows.is_empty()&&!state.stale&&!state.truncated).then_some(state);
        }
        if state.exists.is_none()||state.key.is_none()||state.generation.is_none()||state.app.is_some()!=(state.exists==Some(true))||state.offset%PAGE_SIZE!=0||state.offset>=MAX_ROWS||state.stale&&state.offset!=0{return None;}
        if state.total==0 {if state.offset!=0||!state.rows.is_empty(){return None;}}else if state.offset>=state.total||state.rows.len()!=(state.total-state.offset).min(PAGE_SIZE) as usize{return None;}
        if state.exists==Some(false)&&(state.total!=0||state.truncated){return None;}
        if let Some(app)=&state.app {
            if app.can_set&&(!app.permission_requested||app.permission_fixed||app.importance_locked||app.suspended){return None;}
            for row in &state.rows{if row.effective_enabled!=(app.enabled&&row.enabled&&!row.group_blocked)||!row.options.is_empty()&&!app.enabled||row.key==app.key{return None;}}
        }
        Some(state)
    }
    pub fn permits(&self,request:&AppNotificationsRequest)->bool{
        let AppNotificationsRequest::Set{target,key,item,change}=request else{return true;};
        if !change.valid()||target!=&self.target||Some(key)!=self.key.as_ref()||self.availability!=Availability::Available{return false;}
        if let NotificationChange::App(_)=change{return self.app.as_ref().is_some_and(|a|a.can_set&&&a.key==item);}
        self.rows.iter().any(|row|&row.key==item&&row.can_set&&match change{NotificationChange::Group(_)=>row.kind==RowKind::Group,NotificationChange::Channel(_)=>row.kind==RowKind::Channel,NotificationChange::Importance(value)=>row.kind==RowKind::Channel&&row.options.contains(value),_=>false})
    }
    pub fn clear_actions(&mut self){if let Some(app)=&mut self.app{app.can_set=false;}for row in &mut self.rows{row.can_set=false;row.options.clear();row.sound_changes.clear();}}
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn snapshot(id:i64,offset:u32,total:u32)->Value{
        let rows=(offset..(offset+20).min(total)).map(|i|format!(r#"{{"key":"{:064x}","kind":"channel","name":"Channel {i:02}","group_name":"","enabled":true,"effective_enabled":true,"can_set":true,"importance":"low","importance_options":["min","low","default","high"],"sound_change_options":["default","high"],"sound":"Silent","user_locked_importance":false,"group_blocked":false,"linked_app":false}}"#,100+i)).collect::<Vec<_>>().join(",");
        makepad_strict_json::parse(format!(r#"{{"schema":1,"request_id":{id},"package":"com.example.test","exists":true,"availability":"available","key":"{1:064x}","app":{{"key":"{2:064x}","label":"Example","enabled":true,"permission_requested":true,"permission_fixed":false,"importance_locked":false,"suspended":false,"can_set":true}},"generation":"{3:064x}","offset":{offset},"total":{total},"truncated":false,"stale":false,"rows":[{0}]}}"#,rows,1,2,3).as_bytes()).unwrap()
    }
    #[test]
    fn notification_page_targets_and_protected_choices_are_not_interchangeable(){
        let mut state=AppNotificationsSnapshot::decode(&snapshot(1,0,26)).unwrap();assert_eq!(state.rows[0].group_name,None);
        let request=AppNotificationsRequest::Set{target:state.target.clone(),key:state.key.clone().unwrap(),item:state.rows[0].key.clone(),change:NotificationChange::Importance(Importance::High)};
        assert!(state.permits(&request));let mut forged=request.clone();if let AppNotificationsRequest::Set{change,..}=&mut forged{*change=NotificationChange::App(false);}assert!(!state.permits(&forged));
        let page_two=AppNotificationsSnapshot::decode(&snapshot(2,20,26)).unwrap();assert!(!page_two.permits(&request));
        state.clear_actions();assert!(!state.permits(&request));assert_eq!(state.rows[0].importance,Some(Importance::Low));assert_eq!(state.rows[0].sound,Some("Silent".into()));
    }
    #[test]
    fn notification_decode_rejects_incomplete_pages_and_unknown_actions(){
        let field=crate::settings_updates::tests::field;
        let mut raw=snapshot(1,0,26);field(&mut raw,"total",Value::Int(1));assert!(AppNotificationsSnapshot::decode(&raw).is_none());
        let mut raw=snapshot(1,0,1);let Value::Obj(fields)=&mut raw else{panic!()};let Value::Arr(rows)=&mut fields.iter_mut().find(|(k,_)|k=="rows").unwrap().1 else{panic!()};
        field(&mut rows[0],"importance_options",Value::Arr(vec![makepad_strict_json::s("bypass_dnd")]));assert!(AppNotificationsSnapshot::decode(&raw).is_none());
        let mut raw=snapshot(1,0,1);field(&mut raw,"availability",makepad_strict_json::s("unavailable"));assert!(AppNotificationsSnapshot::decode(&raw).is_none(),"unavailable observations cannot carry actionable rows");
    }
    #[test]
    fn notification_read_requires_matching_package_page_and_stale_reset(){
        let mut state=AppNotificationsSnapshot::decode(&snapshot(1,20,26)).unwrap();let read=AppNotificationsRead{target:state.target.clone(),offset:20,generation:state.generation.clone()};assert!(read.valid()&&read.accepts(&state));
        state.offset=0;assert!(!read.accepts(&state));state.stale=true;assert!(read.accepts(&state));
        assert!(!AppNotificationsRead{generation:None,..read}.valid());
    }
}
