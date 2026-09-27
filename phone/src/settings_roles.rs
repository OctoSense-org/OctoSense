//! Default apps are observed native roles. Selection opens platform consent, never a raw grant.
use crate::settings_apps::AppTarget;
use makepad_strict_json::Value;
pub const PAGE_SIZE:u32=20;
pub const MAX_ROWS:u32=2000;
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq,Hash)]
pub enum RoleId{Browser,Home,Assistant,Dialer,Sms,CallScreening,CallRedirection,Wallet}
impl RoleId{
    pub const ALL:[Self;8]=[Self::Browser,Self::Home,Self::Assistant,Self::Dialer,Self::Sms,Self::CallScreening,Self::CallRedirection,Self::Wallet];
    pub fn wire(self)->&'static str{match self{Self::Browser=>"browser",Self::Home=>"home",Self::Assistant=>"assistant",Self::Dialer=>"dialer",Self::Sms=>"sms",Self::CallScreening=>"call_screening",Self::CallRedirection=>"call_redirection",Self::Wallet=>"wallet"}}
    pub fn label(self)->&'static str{match self{Self::Browser=>"Browser",Self::Home=>"Home app",Self::Assistant=>"Digital assistant",Self::Dialer=>"Phone app",Self::Sms=>"SMS app",Self::CallScreening=>"Call screening",Self::CallRedirection=>"Call redirection",Self::Wallet=>"Wallet"}}
    fn decode(v:&Value)->Option<Self>{Self::ALL.into_iter().find(|r|Some(r.wire())==v.as_str())}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct RoleKey(String);
impl RoleKey{pub fn wire(&self)->&str{&self.0}pub(crate) fn decode(v:&Value)->Option<Self>{let s=v.as_str()?;(s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))).then(||Self(s.into()))}}
#[derive(serde::Serialize, Clone,Debug,Default,PartialEq,Eq)]
pub struct RolesRead{pub role:Option<RoleId>,pub offset:u32,pub generation:Option<RoleKey>}
impl RolesRead{
    pub fn valid(&self)->bool{self.offset<MAX_ROWS&&self.offset%PAGE_SIZE==0&&(self.offset==0||self.generation.is_some())&&(self.role.is_some()||self.offset==0&&self.generation.is_none())}
    pub fn accepts(&self,s:&RolesSnapshot)->bool{self.role==s.role&&(s.availability!=Availability::Available||self.role.is_none()||if s.stale{self.generation.is_some()&&s.offset==0}else{s.offset==self.offset&&self.generation.as_ref().is_none_or(|g|Some(g)==s.generation.as_ref())})}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub enum RolesRequest{Snapshot(RolesRead),Confirm{role:RoleId,key:RoleKey,target:RoleKey}}
impl RolesRequest{pub fn valid(&self)->bool{match self{Self::Snapshot(read)=>read.valid(),Self::Confirm{..}=>true}}}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum Availability{Available,Unsupported,Restricted,Unavailable}
impl Availability{
    fn decode(v:&Value)->Option<Self>{Some(match v.as_str()?{"available"=>Self::Available,"unsupported"=>Self::Unsupported,"restricted"=>Self::Restricted,"unavailable"=>Self::Unavailable,_=>return None})}
    pub fn label(self)->&'static str{match self{Self::Available=>"",Self::Unsupported=>"This default-app role is not supported on this device.",Self::Restricted=>"Default-app choices are unavailable while locked or restricted.",Self::Unavailable=>"Default-app choices are unavailable on this installation. Android default-app settings remain available below."}}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct Holder{pub package:AppTarget,pub label:String}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct RoleOverview{pub role:RoleId,pub label:String,pub available:bool,pub restricted:bool,pub holder:Option<Holder>}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct RoleCandidate{pub target:RoleKey,pub package:AppTarget,pub label:String,pub selected:bool,pub can_select:bool}
#[derive(serde::Serialize, Clone,Debug)]
pub struct RolesSnapshot{
    pub request_id:i64,pub availability:Availability,pub roles:Vec<RoleOverview>,pub role:Option<RoleId>,
    pub key:Option<RoleKey>,pub generation:Option<RoleKey>,pub offset:u32,pub total:u32,pub stale:bool,pub truncated:bool,
    pub candidates:Vec<RoleCandidate>,pub none_target:Option<RoleKey>,
}
fn text(v:&Value)->Option<String>{let s=v.as_str()?;(!s.is_empty()&&s.chars().count()<=256&&!s.chars().any(char::is_control)).then(||s.into())}
fn optional<T>(v:Option<&Value>,decode:impl FnOnce(&Value)->Option<T>)->Option<Option<T>>{match v{None|Some(Value::Null)=>Some(None),Some(v)=>decode(v).map(Some)}}
fn count(v:&Value)->Option<u32>{u32::try_from(v.as_i64()?).ok().filter(|n|*n<=MAX_ROWS)}
impl RolesSnapshot{
    pub fn decode(v:&Value)->Option<Self>{
        if v.get("schema")?.as_i64()?!=1{return None;}
        let mut roles=Vec::new();let entries=v.get("roles")?.as_arr()?;if entries.len()>RoleId::ALL.len(){return None;}
        for entry in entries{let row=RoleOverview{role:RoleId::decode(entry.get("role")?)?,label:text(entry.get("label")?)?,available:entry.get("available")?.as_bool()?,restricted:entry.get("restricted")?.as_bool()?,holder:optional(entry.get("holder"),|h|Some(Holder{package:AppTarget::decode(h.get("package")?)?,label:text(h.get("label")?)?}))?};
            if roles.iter().any(|r:&RoleOverview|r.role==row.role)||!row.available&&(row.holder.is_some()||row.restricted){return None;}roles.push(row);}
        let mut candidates=Vec::new();let entries=v.get("candidates")?.as_arr()?;if entries.len()>PAGE_SIZE as usize{return None;}
        for entry in entries{let row=RoleCandidate{target:RoleKey::decode(entry.get("target")?)?,package:AppTarget::decode(entry.get("package")?)?,label:text(entry.get("label")?)?,selected:entry.get("selected")?.as_bool()?,can_select:entry.get("can_select")?.as_bool()?};
            if row.selected&&row.can_select||candidates.iter().any(|r:&RoleCandidate|r.target==row.target||r.package==row.package){return None;}candidates.push(row);}
        let state=Self{request_id:v.get("request_id")?.as_i64().filter(|n|*n>0)?,availability:Availability::decode(v.get("availability")?)?,roles,role:optional(v.get("role"),RoleId::decode)?,key:optional(v.get("key"),RoleKey::decode)?,generation:optional(v.get("generation"),RoleKey::decode)?,offset:count(v.get("offset")?)?,total:count(v.get("total")?)?,stale:v.get("stale")?.as_bool()?,truncated:v.get("truncated")?.as_bool()?,candidates,none_target:optional(v.get("none_target"),RoleKey::decode)?};
        let no_choices=state.key.is_none()&&state.generation.is_none()&&state.offset==0&&state.total==0&&!state.stale&&!state.truncated&&state.candidates.is_empty()&&state.none_target.is_none();
        if matches!(state.availability,Availability::Restricted|Availability::Unavailable){return (no_choices&&state.roles.is_empty()).then_some(state);}
        if state.roles.len()!=RoleId::ALL.len(){return None;}
        if state.availability==Availability::Unsupported{return (no_choices&&state.role.is_some_and(|r|state.roles.iter().any(|o|o.role==r&&!o.available))).then_some(state);}
        let Some(role)=state.role else{return no_choices.then_some(state);};
        let overview=state.roles.iter().find(|r|r.role==role)?;
        if !overview.available||state.key.is_none()||state.generation.is_none()||state.offset>=MAX_ROWS||state.offset%PAGE_SIZE!=0||state.stale&&state.offset!=0{return None;}
        if state.total==0{if state.offset!=0||!state.candidates.is_empty(){return None;}}else if state.offset>=state.total||state.candidates.len()!=(state.total-state.offset).min(PAGE_SIZE) as usize{return None;}
        if overview.restricted&&(state.none_target.is_some()||state.candidates.iter().any(|c|c.can_select))||state.none_target.is_some()&&overview.holder.is_none(){return None;}
        for candidate in &state.candidates{if candidate.selected!=overview.holder.as_ref().is_some_and(|h|h.package==candidate.package)||Some(&candidate.target)==state.none_target.as_ref(){return None;}}
        Some(state)
    }
    pub fn permits(&self,request:&RolesRequest)->bool{let RolesRequest::Confirm{role,key,target}=request else{return true;};
        self.availability==Availability::Available&&self.role==Some(*role)&&self.key.as_ref()==Some(key)
            &&(self.none_target.as_ref()==Some(target)||self.candidates.iter().any(|c|&c.target==target&&c.can_select))}
    pub fn clear_actions(&mut self){self.none_target=None;for candidate in &mut self.candidates{candidate.can_select=false;}}
}
#[cfg(test)]pub(crate) mod tests{
    use super::*;
    pub fn snapshot(id:i64,offset:u32,total:u32)->Value{
        let roles=RoleId::ALL.into_iter().map(|r|format!(r#"{{"role":"{}","label":"{}","available":true,"restricted":false,"holder":{{"package":"fixture.browser00","label":"Browser 00"}}}}"#,r.wire(),r.label())).collect::<Vec<_>>().join(",");
        let rows=(offset..(offset+20).min(total)).map(|i|format!(r#"{{"target":"{:064x}","package":"fixture.browser{i:02}","label":"Browser {i:02}","selected":{},"can_select":{}}}"#,i+100,i==0,i!=0)).collect::<Vec<_>>().join(",");
        makepad_strict_json::parse(format!(r#"{{"schema":1,"request_id":{id},"availability":"available","roles":[{roles}],"role":"browser","key":"{0:064x}","generation":"{1:064x}","offset":{offset},"total":{total},"stale":false,"truncated":false,"candidates":[{rows}],"none_target":"{2:064x}"}}"#,1,2,3).as_bytes()).unwrap()
    }
    #[test]fn native_role_choices_are_finite_observed_and_page_bound(){
        let mut state=RolesSnapshot::decode(&snapshot(7,0,25)).unwrap();let request=RolesRequest::Confirm{role:RoleId::Browser,key:state.key.clone().unwrap(),target:state.candidates[1].target.clone()};assert!(state.permits(&request));
        let wrong=RolesRequest::Confirm{role:RoleId::Home,key:state.key.clone().unwrap(),target:state.candidates[1].target.clone()};assert!(!state.permits(&wrong));
        assert!(!RolesSnapshot::decode(&snapshot(8,20,25)).unwrap().permits(&request));state.clear_actions();assert!(!state.permits(&request));assert_eq!(state.roles[0].holder.as_ref().unwrap().label,"Browser 00");
    }
    #[test]fn malformed_role_observations_cannot_grant_authority(){
        let field=crate::settings_updates::tests::field;
        let mut raw=snapshot(1,0,25);field(&mut raw,"total",Value::Int(1));assert!(RolesSnapshot::decode(&raw).is_none());
        let mut raw=snapshot(1,0,1);field(&mut raw,"role",makepad_strict_json::s("system_gallery"));assert!(RolesSnapshot::decode(&raw).is_none());
        let mut raw=snapshot(1,0,1);field(&mut raw,"availability",makepad_strict_json::s("unavailable"));assert!(RolesSnapshot::decode(&raw).is_none());
        assert!(!RolesRead{role:Some(RoleId::Browser),offset:20,generation:None}.valid());assert!(!RolesRead{role:None,offset:20,generation:Some(RoleKey("1".repeat(64)))}.valid());
    }
    #[test]fn role_paging_accepts_only_matching_generation_or_new_stale_reset(){let mut state=RolesSnapshot::decode(&snapshot(1,20,25)).unwrap();let read=RolesRead{role:Some(RoleId::Browser),offset:20,generation:state.generation.clone()};assert!(read.accepts(&state));state.offset=0;assert!(!read.accepts(&state));state.stale=true;assert!(read.accepts(&state));}
}
