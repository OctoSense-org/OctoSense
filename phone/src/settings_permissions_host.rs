//! Foreground-root, selected-package and request correlation for permission controls.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_permissions::{PermissionsRead,PermissionsRequest,PermissionsSnapshot}};
use makepad_strict_json::{s,Value};
use makepad_widgets::*;
type Owner=(ClientId,WidgetUid);
#[derive(Clone)]
struct Read{id:i64,owner:Owner,key:PermissionsRead,deadline:f64}
#[derive(Default)]
pub(crate) struct SettingsPermissionsRuntime{
    active:Option<Owner>,desired:Option<PermissionsRead>,read:Option<Read>,next_read:f64,fresh:bool,focused:bool,
    snapshot:Option<PermissionsSnapshot>,pub(crate) error:String,
}
impl SettingsPermissionsRuntime {
    pub(crate) fn snapshot(&self)->Option<PermissionsSnapshot>{let mut s=self.snapshot.clone()?;if !self.fresh{s.clear_actions();}Some(s)}
    fn focus(&mut self,observed:Option<bool>)->bool{
        let Some(focused)=observed else{return false;};
        if self.focused==focused{return false;}
        self.focused=focused;self.retire();true
    }
    fn retire(&mut self){self.read=None;self.fresh=false;self.next_read=0.;}
    fn accepts(&self,id:i64,owner:Owner)->bool{self.active==Some(owner)&&self.read.as_ref().is_some_and(|r|r.id==id&&r.owner==owner)}
}
impl App {
    pub(crate) fn settings_permissions_focus(&mut self,cx:&mut Cx,observed:Option<bool>){
        if self.settings_runtime.permissions.focus(observed){self.refresh_settings_app(cx);}
    }
    fn current_settings_permissions_read(&self)->Option<PermissionsRead>{self.module_host.settings_instance()?.root.borrow::<SettingsView>()?.permissions_read()}
    fn observed_permissions_package(&self,key:&PermissionsRead)->bool{self.settings_runtime.apps.details().is_some_and(|details|details.target==key.package)}
    pub(crate) fn settings_permissions_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool{
        let SettingsRequest::Permissions(PermissionsRequest::Snapshot(key))=request else{return false;};
        if cfg!(target_os="android")&&self.settings_runtime.permissions.focused&&self.settings_is_foreground(owner)&&self.current_settings_permissions_read().as_ref()==Some(key)&&self.observed_permissions_package(key){self.start_settings_permissions_read(cx,owner,key.clone(),true);}true
    }
    fn start_settings_permissions_read(&mut self,cx:&mut Cx,owner:Owner,key:PermissionsRead,manual:bool){
        if self.module_host.settings_client(owner.1)!=Some(owner.0)||!key.valid(){return;}
        if self.settings_runtime.permissions.read.as_ref().is_some_and(|r|r.owner==owner&&r.key==key){return;}
        let id=self.android_command_id(cx,"launcher","permissions_snapshot",vec![("package",s(key.package.package())),("group",key.group.map(|g|s(g.wire())).unwrap_or(Value::Null)),("offset",Value::Int(key.offset as i64)),("generation",key.generation.as_ref().map(|g|s(g.wire())).unwrap_or(Value::Null))]);
        let runtime=&mut self.settings_runtime.permissions;
        if runtime.active!=Some(owner)||runtime.desired.as_ref()!=Some(&key){runtime.retire();}
        runtime.active=Some(owner);runtime.desired=Some(key.clone());runtime.read=Some(Read{id,owner,key,deadline:crate::host::now()+20.});runtime.next_read=crate::host::now()+5.;
        if manual{runtime.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_permissions_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64){
        let visible=visible.filter(|_|self.settings_runtime.permissions.focused);
        let desired=visible.and_then(|_|self.current_settings_permissions_read());let active=visible.filter(|_|desired.is_some());
        let runtime=&mut self.settings_runtime.permissions;
        if runtime.active!=active||runtime.desired!=desired{runtime.retire();runtime.active=active;runtime.desired=desired.clone();runtime.error.clear();self.refresh_settings_app(cx);}
        let(Some(owner),Some(key))=(active,desired)else{return;};if !self.observed_permissions_package(&key){return;}
        if self.settings_runtime.permissions.read.as_ref().is_some_and(|r|now>=r.deadline){self.fail_settings_permissions_read(cx,"Permission settings did not arrive. Refresh to try again.");}
        if self.settings_runtime.permissions.read.is_none()&&now>=self.settings_runtime.permissions.next_read{self.start_settings_permissions_read(cx,owner,key,false);}
    }
    fn fail_settings_permissions_read(&mut self,cx:&mut Cx,message:&str){let r=&mut self.settings_runtime.permissions;r.retire();r.error=message.into();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);}
    pub(crate) fn settings_permissions_result(&mut self,cx:&mut Cx,v:&Value)->bool{
        let Some(read)=self.settings_runtime.permissions.read.as_ref()else{return false;};if v.get("id").and_then(Value::as_i64)!=Some(read.id){return false;}
        if v.get("status").and_then(Value::as_i64)!=Some(0){self.fail_settings_permissions_read(cx,"Android could not read permission settings. Refresh to try again.");}true
    }
    pub(crate) fn settings_permissions_observe(&mut self,cx:&mut Cx,v:&Value){
        let Some(read)=self.settings_runtime.permissions.read.clone()else{return;};
        if v.get("request_id").and_then(Value::as_i64)!=Some(read.id)||!self.settings_runtime.permissions.accepts(read.id,read.owner)||self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)||!self.settings_is_foreground(read.owner)||self.current_settings_permissions_read().as_ref()!=Some(&read.key){return;}
        let Some(snapshot)=PermissionsSnapshot::decode(v).filter(|s|read.key.accepts(s))else{self.fail_settings_permissions_read(cx,"Android returned invalid permission settings. Refresh to try again.");return;};
        let r=&mut self.settings_runtime.permissions;r.snapshot=Some(snapshot);r.read=None;r.fresh=true;r.error.clear();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);
        self.settings_runtime.permissions.desired=self.current_settings_permissions_read();
    }
    pub(crate) fn settings_permissions_permits(&self,owner:Owner,request:&PermissionsRequest)->bool{
        let r=&self.settings_runtime.permissions;
        self.settings_is_foreground(owner)&&r.focused&&r.active==Some(owner)&&r.fresh&&self.current_settings_permissions_read().as_ref()==r.desired.as_ref()&&r.snapshot.as_ref().is_some_and(|s|s.permits(request))
    }
    pub(crate) fn settings_permissions_resync(&mut self,cx:&mut Cx){self.settings_runtime.permissions.retire();self.refresh_settings_app(cx);}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn permission_return_waits_for_actual_focus_and_refreshes_without_poll_delay(){
        let owner=(1,WidgetUid(10));let snapshot=PermissionsSnapshot::decode(&crate::settings_permissions::tests::snapshot(7,true)).unwrap();
        let key=PermissionsRead{package:snapshot.package.clone(),group:snapshot.group,offset:0,generation:None};
        let mut runtime=SettingsPermissionsRuntime{active:Some(owner),desired:Some(key.clone()),snapshot:Some(snapshot),next_read:50.,..Default::default()};
        assert!(!runtime.focus(None));assert!(!runtime.focus(Some(false)));assert!(!runtime.focused);
        assert!(runtime.focus(Some(true)));assert_eq!(runtime.next_read,0.);assert!(!runtime.fresh);
        runtime.fresh=true;runtime.read=Some(Read{id:8,owner,key,deadline:25.});runtime.next_read=55.;
        assert!(!runtime.focus(Some(true)));assert_eq!(runtime.read.as_ref().unwrap().id,8);
        assert!(runtime.focus(Some(false)));assert!(runtime.read.is_none());assert!(!runtime.fresh);
        assert!(runtime.snapshot().unwrap().choices.iter().all(|c|!c.enabled));
        assert!(runtime.focus(Some(true)));assert_eq!(runtime.next_read,0.);assert!(!runtime.fresh,"focus gain never reauthorizes a cached one-use choice");
    }
    #[test]fn permission_reads_retire_across_owner_and_request_lifetimes(){
        let owner=(1,WidgetUid(10));let snapshot=PermissionsSnapshot::decode(&crate::settings_permissions::tests::snapshot(7,true)).unwrap();
        let key=PermissionsRead{package:snapshot.package.clone(),group:snapshot.group,offset:0,generation:None};
        let mut runtime=SettingsPermissionsRuntime{active:Some(owner),desired:Some(key.clone()),read:Some(Read{id:7,owner,key,deadline:20.}),fresh:true,snapshot:Some(snapshot),..Default::default()};
        assert!(runtime.accepts(7,owner));assert!(!runtime.accepts(6,owner));assert!(!runtime.accepts(7,(1,WidgetUid(11))));
        runtime.retire();assert!(!runtime.accepts(7,owner));let cached=runtime.snapshot().unwrap();assert!(!cached.choices[0].enabled);assert!(cached.choices[0].target.is_none());assert!(cached.choices[1].selected);
    }
}
