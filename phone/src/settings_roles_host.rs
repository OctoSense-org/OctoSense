//! Foreground-root, selected-package and request correlation for default roles.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_roles::{RolesRead,RolesRequest,RolesSnapshot}};
use makepad_strict_json::{s,Value};
use makepad_widgets::*;
type Owner=(ClientId,WidgetUid);
#[derive(Clone)]
struct Read{id:i64,owner:Owner,key:RolesRead,deadline:f64}
#[derive(Default)]
pub(crate) struct SettingsRolesRuntime{
    active:Option<Owner>,desired:Option<RolesRead>,read:Option<Read>,next_read:f64,fresh:bool,
    snapshot:Option<RolesSnapshot>,pub(crate) error:String,
}
impl SettingsRolesRuntime {
    pub(crate) fn snapshot(&self)->Option<RolesSnapshot>{let mut s=self.snapshot.clone()?;if !self.fresh{s.clear_actions();}Some(s)}
    fn retire(&mut self){self.read=None;self.fresh=false;self.next_read=0.;}
    fn accepts(&self,id:i64,owner:Owner)->bool{self.active==Some(owner)&&self.read.as_ref().is_some_and(|r|r.id==id&&r.owner==owner)}
}
impl App {
    fn current_settings_roles_read(&self)->Option<RolesRead>{self.module_host.settings_instance()?.root.borrow::<SettingsView>()?.roles_read()}
    pub(crate) fn settings_roles_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool{
        let SettingsRequest::Roles(RolesRequest::Snapshot(key))=request else{return false;};
        if cfg!(target_os="android")&&self.settings_is_foreground(owner)&&self.current_settings_roles_read().as_ref()==Some(key){self.start_settings_roles_read(cx,owner,key.clone(),true);}true
    }
    fn start_settings_roles_read(&mut self,cx:&mut Cx,owner:Owner,key:RolesRead,manual:bool){
        if self.module_host.settings_client(owner.1)!=Some(owner.0)||!key.valid(){return;}
        if self.settings_runtime.roles.read.as_ref().is_some_and(|r|r.owner==owner&&r.key==key){return;}
        let id=self.android_command_id(cx,"launcher","roles_snapshot",vec![("role",key.role.map(|r|s(r.wire())).unwrap_or(Value::Null)),("offset",Value::Int(key.offset as i64)),("generation",key.generation.as_ref().map(|g|s(g.wire())).unwrap_or(Value::Null))]);
        let runtime=&mut self.settings_runtime.roles;
        if runtime.active!=Some(owner)||runtime.desired.as_ref()!=Some(&key){runtime.retire();}
        runtime.active=Some(owner);runtime.desired=Some(key.clone());runtime.read=Some(Read{id,owner,key,deadline:crate::host::now()+20.});runtime.next_read=crate::host::now()+5.;
        if manual{runtime.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_roles_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64){
        let desired=visible.and_then(|_|self.current_settings_roles_read());let active=visible.filter(|_|desired.is_some());
        let runtime=&mut self.settings_runtime.roles;
        if runtime.active!=active||runtime.desired!=desired{runtime.retire();runtime.active=active;runtime.desired=desired.clone();runtime.error.clear();self.refresh_settings_app(cx);}
        let(Some(owner),Some(key))=(active,desired)else{return;};
        if self.settings_runtime.roles.read.as_ref().is_some_and(|r|now>=r.deadline){self.fail_settings_roles_read(cx,"Default-app choices did not arrive. Refresh to try again.");}
        if self.settings_runtime.roles.read.is_none()&&now>=self.settings_runtime.roles.next_read{self.start_settings_roles_read(cx,owner,key,false);}
    }
    fn fail_settings_roles_read(&mut self,cx:&mut Cx,message:&str){let r=&mut self.settings_runtime.roles;r.retire();r.error=message.into();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);}
    pub(crate) fn settings_roles_result(&mut self,cx:&mut Cx,v:&Value)->bool{
        let Some(read)=self.settings_runtime.roles.read.as_ref()else{return false;};if v.get("id").and_then(Value::as_i64)!=Some(read.id){return false;}
        if v.get("status").and_then(Value::as_i64)!=Some(0){self.fail_settings_roles_read(cx,"Android could not read default-app choices. Refresh to try again.");}true
    }
    pub(crate) fn settings_roles_observe(&mut self,cx:&mut Cx,v:&Value){
        let Some(read)=self.settings_runtime.roles.read.clone()else{return;};
        if v.get("request_id").and_then(Value::as_i64)!=Some(read.id)||!self.settings_runtime.roles.accepts(read.id,read.owner)||self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)||!self.settings_is_foreground(read.owner)||self.current_settings_roles_read().as_ref()!=Some(&read.key){return;}
        let Some(snapshot)=RolesSnapshot::decode(v).filter(|s|read.key.accepts(s))else{self.fail_settings_roles_read(cx,"Android returned invalid default-app choices. Refresh to try again.");return;};
        let r=&mut self.settings_runtime.roles;r.snapshot=Some(snapshot);r.read=None;r.fresh=true;r.error.clear();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);
        self.settings_runtime.roles.desired=self.current_settings_roles_read();
    }
    pub(crate) fn settings_roles_permits(&self,owner:Owner,request:&RolesRequest)->bool{
        let r=&self.settings_runtime.roles;
        self.settings_is_foreground(owner)&&r.active==Some(owner)&&r.fresh&&self.current_settings_roles_read().as_ref()==r.desired.as_ref()&&r.snapshot.as_ref().is_some_and(|s|s.permits(request))
    }
    pub(crate) fn settings_roles_resync(&mut self,cx:&mut Cx){self.settings_runtime.roles.retire();self.refresh_settings_app(cx);}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn role_reads_retire_across_owner_and_request_lifetimes(){
        let owner=(1,WidgetUid(10));let snapshot=RolesSnapshot::decode(&crate::settings_roles::tests::snapshot(7,0,25)).unwrap();
        let key=RolesRead{role:snapshot.role,offset:0,generation:None};
        let mut runtime=SettingsRolesRuntime{active:Some(owner),desired:Some(key.clone()),read:Some(Read{id:7,owner,key,deadline:20.}),fresh:true,snapshot:Some(snapshot),..Default::default()};
        assert!(runtime.accepts(7,owner));assert!(!runtime.accepts(6,owner));assert!(!runtime.accepts(7,(1,WidgetUid(11))));
        runtime.retire();assert!(!runtime.accepts(7,owner));let cached=runtime.snapshot().unwrap();assert!(!cached.candidates[1].can_select);assert!(cached.none_target.is_none());assert_eq!(cached.candidates[1].label,"Browser 01");
    }
}
