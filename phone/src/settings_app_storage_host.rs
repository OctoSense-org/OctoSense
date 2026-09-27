//! Current trusted Settings root, visible package and one-use storage-policy observations.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_apps::AppTarget,settings_app_storage::{AppStorageRequest,AppStorageSnapshot}};
use makepad_strict_json::{s,Value};
use makepad_widgets::*;
type Owner=(ClientId,WidgetUid);
#[derive(Clone)]struct Read{id:i64,owner:Owner,target:AppTarget,deadline:f64}
#[derive(Default)]pub(crate) struct SettingsAppStorageRuntime {
    active:Option<Owner>,desired:Option<AppTarget>,read:Option<Read>,next_read:f64,fresh:bool,focused:bool,
    snapshot:Option<AppStorageSnapshot>,pub(crate) error:String,
}
impl SettingsAppStorageRuntime {
    pub(crate) fn snapshot(&self)->Option<AppStorageSnapshot>{let mut value=self.snapshot.clone()?;if !self.fresh{value.clear_actions();}Some(value)}
    fn retire(&mut self){self.read=None;self.fresh=false;self.next_read=0.;}
    fn focus(&mut self,value:Option<bool>)->bool{let Some(value)=value else{return false;};if value==self.focused{return false;}self.focused=value;self.retire();true}
}
impl App {
    pub(crate) fn settings_app_storage_focus(&mut self,cx:&mut Cx,value:Option<bool>){if self.settings_runtime.app_storage.focus(value){self.refresh_settings_app(cx);}}
    fn current_settings_app_storage_read(&self)->Option<AppTarget>{self.module_host.settings_instance()?.root.borrow::<SettingsView>()?.app_storage_read()}
    fn observed_app_storage_package(&self,target:&AppTarget)->bool{self.settings_runtime.apps.details().is_some_and(|details|details.exists&&details.target==*target)}
    pub(crate) fn settings_app_storage_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool{
        let SettingsRequest::AppStorage(AppStorageRequest::Snapshot(target))=request else{return false;};
        if cfg!(target_os="android")&&self.settings_runtime.app_storage.focused&&self.settings_is_foreground(owner)&&self.current_settings_app_storage_read().as_ref()==Some(target)&&self.observed_app_storage_package(target){self.start_settings_app_storage_read(cx,owner,target.clone(),true);}true
    }
    fn start_settings_app_storage_read(&mut self,cx:&mut Cx,owner:Owner,target:AppTarget,manual:bool){
        if self.module_host.settings_client(owner.1)!=Some(owner.0)||!self.observed_app_storage_package(&target){return;}
        if self.settings_runtime.app_storage.read.as_ref().is_some_and(|r|r.owner==owner&&r.target==target){return;}
        let id=self.android_command_id(cx,"launcher","app_storage_snapshot",vec![("package",s(target.package()))]);
        let runtime=&mut self.settings_runtime.app_storage;
        if runtime.active!=Some(owner)||runtime.desired.as_ref()!=Some(&target){runtime.retire();}
        runtime.active=Some(owner);runtime.desired=Some(target.clone());runtime.read=Some(Read{id,owner,target,deadline:crate::host::now()+20.});runtime.next_read=crate::host::now()+5.;
        if manual{runtime.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_app_storage_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64){
        let visible=visible.filter(|_|self.settings_runtime.app_storage.focused);let desired=visible.and_then(|_|self.current_settings_app_storage_read());let active=visible.filter(|_|desired.is_some());
        let r=&mut self.settings_runtime.app_storage;
        if r.active!=active||r.desired!=desired{r.retire();r.active=active;r.desired=desired.clone();r.error.clear();self.refresh_settings_app(cx);}
        let(Some(owner),Some(target))=(active,desired)else{return;};if !self.observed_app_storage_package(&target){self.settings_runtime.app_storage.retire();return;}
        if self.settings_runtime.app_storage.read.as_ref().is_some_and(|r|now>=r.deadline){self.fail_settings_app_storage_read(cx,"App storage information did not arrive. Refresh to try again.");}
        if self.settings_runtime.app_storage.read.is_none()&&now>=self.settings_runtime.app_storage.next_read{self.start_settings_app_storage_read(cx,owner,target,false);}
    }
    fn fail_settings_app_storage_read(&mut self,cx:&mut Cx,message:&str){let r=&mut self.settings_runtime.app_storage;r.retire();r.error=message.into();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);}
    pub(crate) fn settings_app_storage_result(&mut self,cx:&mut Cx,value:&Value)->bool{
        let Some(read)=self.settings_runtime.app_storage.read.as_ref()else{return false;};if value.get("id").and_then(Value::as_i64)!=Some(read.id){return false;}
        if value.get("status").and_then(Value::as_i64)!=Some(0){self.fail_settings_app_storage_read(cx,"Android could not read app storage policy. Refresh to try again.");}true
    }
    pub(crate) fn settings_app_storage_observe(&mut self,cx:&mut Cx,value:&Value){
        let Some(read)=self.settings_runtime.app_storage.read.clone()else{return;};let r=&self.settings_runtime.app_storage;
        if value.get("request_id").and_then(Value::as_i64)!=Some(read.id)||r.active!=Some(read.owner)||!r.focused||self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)||!self.settings_is_foreground(read.owner)||self.current_settings_app_storage_read().as_ref()!=Some(&read.target)||!self.observed_app_storage_package(&read.target){return;}
        let Some(snapshot)=AppStorageSnapshot::decode(value).filter(|s|s.target==read.target)else{self.fail_settings_app_storage_read(cx,"Android returned invalid app storage information.");return;};
        let r=&mut self.settings_runtime.app_storage;r.snapshot=Some(snapshot);r.read=None;r.fresh=true;r.error.clear();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_app_storage_permits(&self,owner:Owner,request:&AppStorageRequest)->bool{
        let r=&self.settings_runtime.app_storage;self.settings_is_foreground(owner)&&r.focused&&r.active==Some(owner)&&r.fresh&&r.desired.as_ref().is_some_and(|target|self.observed_app_storage_package(target))&&self.current_settings_app_storage_read().as_ref()==r.desired.as_ref()&&r.snapshot.as_ref().is_some_and(|s|s.permits(request))
    }
    pub(crate) fn settings_app_storage_resync(&mut self,cx:&mut Cx){self.settings_runtime.app_storage.retire();self.refresh_settings_app(cx);}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn storage_focus_loss_preserves_observation_but_retires_authority_and_pending_read(){let state=AppStorageSnapshot::decode(&crate::settings_app_storage::tests::snapshot(1)).unwrap();let owner=(1,WidgetUid(9));
        let mut r=SettingsAppStorageRuntime{active:Some(owner),desired:Some(state.target.clone()),read:Some(Read{id:2,owner,target:state.target.clone(),deadline:50.}),fresh:true,focused:true,snapshot:Some(state),..Default::default()};
        assert!(r.focus(Some(false)));assert!(r.read.is_none());assert!(r.snapshot().unwrap().actions.is_empty());assert!(r.focus(Some(true)));assert_eq!(r.next_read,0.);assert!(!r.fresh);}
}
