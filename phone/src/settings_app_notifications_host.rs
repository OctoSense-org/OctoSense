//! Foreground-root, selected-package and request correlation for notification controls.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_app_notifications::{AppNotificationsRead,AppNotificationsRequest,AppNotificationsSnapshot}};
use makepad_strict_json::{s,Value};
use makepad_widgets::*;
type Owner=(ClientId,WidgetUid);
#[derive(Clone)]
struct Read{id:i64,owner:Owner,key:AppNotificationsRead,deadline:f64}
#[derive(Default)]
pub(crate) struct SettingsAppNotificationsRuntime{
    active:Option<Owner>,desired:Option<AppNotificationsRead>,read:Option<Read>,next_read:f64,fresh:bool,
    snapshot:Option<AppNotificationsSnapshot>,pub(crate) error:String,
}
impl SettingsAppNotificationsRuntime {
    pub(crate) fn snapshot(&self)->Option<AppNotificationsSnapshot>{let mut s=self.snapshot.clone()?;if !self.fresh{s.clear_actions();}Some(s)}
    fn retire(&mut self){self.read=None;self.fresh=false;self.next_read=0.;}
    fn accepts(&self,id:i64,owner:Owner)->bool{self.active==Some(owner)&&self.read.as_ref().is_some_and(|r|r.id==id&&r.owner==owner)}
}
impl App {
    fn current_settings_app_notifications_read(&self)->Option<AppNotificationsRead>{self.module_host.settings_instance()?.root.borrow::<SettingsView>()?.app_notifications_read()}
    fn observed_notifications_package(&self,key:&AppNotificationsRead)->bool{self.settings_runtime.apps.details().is_some_and(|details|details.target==key.target)}
    pub(crate) fn settings_app_notifications_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool{
        let SettingsRequest::AppNotifications(AppNotificationsRequest::Snapshot(key))=request else{return false;};
        if cfg!(target_os="android")&&self.settings_is_foreground(owner)&&self.current_settings_app_notifications_read().as_ref()==Some(key)&&self.observed_notifications_package(key){self.start_settings_app_notifications_read(cx,owner,key.clone(),true);}true
    }
    fn start_settings_app_notifications_read(&mut self,cx:&mut Cx,owner:Owner,key:AppNotificationsRead,manual:bool){
        if self.module_host.settings_client(owner.1)!=Some(owner.0)||!key.valid(){return;}
        if self.settings_runtime.app_notifications.read.as_ref().is_some_and(|r|r.owner==owner&&r.key==key){return;}
        let id=self.android_command_id(cx,"launcher","app_notifications_snapshot",vec![("package",s(key.target.package())),("offset",Value::Int(key.offset as i64)),("generation",key.generation.as_ref().map(|g|s(g.wire())).unwrap_or(Value::Null))]);
        let runtime=&mut self.settings_runtime.app_notifications;
        if runtime.active!=Some(owner)||runtime.desired.as_ref()!=Some(&key){runtime.retire();}
        runtime.active=Some(owner);runtime.desired=Some(key.clone());runtime.read=Some(Read{id,owner,key,deadline:crate::host::now()+20.});runtime.next_read=crate::host::now()+5.;
        if manual{runtime.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_app_notifications_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64){
        let desired=visible.and_then(|_|self.current_settings_app_notifications_read());let active=visible.filter(|_|desired.is_some());
        let runtime=&mut self.settings_runtime.app_notifications;
        if runtime.active!=active||runtime.desired!=desired{runtime.retire();runtime.active=active;runtime.desired=desired.clone();runtime.error.clear();self.refresh_settings_app(cx);}
        let(Some(owner),Some(key))=(active,desired)else{return;};if !self.observed_notifications_package(&key){return;}
        if self.settings_runtime.app_notifications.read.as_ref().is_some_and(|r|now>=r.deadline){self.fail_settings_app_notifications_read(cx,"Notification settings did not arrive. Refresh to try again.");}
        if self.settings_runtime.app_notifications.read.is_none()&&now>=self.settings_runtime.app_notifications.next_read{self.start_settings_app_notifications_read(cx,owner,key,false);}
    }
    fn fail_settings_app_notifications_read(&mut self,cx:&mut Cx,message:&str){let r=&mut self.settings_runtime.app_notifications;r.retire();r.error=message.into();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);}
    pub(crate) fn settings_app_notifications_result(&mut self,cx:&mut Cx,v:&Value)->bool{
        let Some(read)=self.settings_runtime.app_notifications.read.as_ref()else{return false;};if v.get("id").and_then(Value::as_i64)!=Some(read.id){return false;}
        if v.get("status").and_then(Value::as_i64)!=Some(0){self.fail_settings_app_notifications_read(cx,"Android could not read notification settings. Refresh to try again.");}true
    }
    pub(crate) fn settings_app_notifications_observe(&mut self,cx:&mut Cx,v:&Value){
        let Some(read)=self.settings_runtime.app_notifications.read.clone()else{return;};
        if v.get("request_id").and_then(Value::as_i64)!=Some(read.id)||!self.settings_runtime.app_notifications.accepts(read.id,read.owner)||self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)||!self.settings_is_foreground(read.owner)||self.current_settings_app_notifications_read().as_ref()!=Some(&read.key){return;}
        let Some(snapshot)=AppNotificationsSnapshot::decode(v).filter(|s|read.key.accepts(s))else{self.fail_settings_app_notifications_read(cx,"Android returned invalid notification settings. Refresh to try again.");return;};
        let r=&mut self.settings_runtime.app_notifications;r.snapshot=Some(snapshot);r.read=None;r.fresh=true;r.error.clear();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);
        self.settings_runtime.app_notifications.desired=self.current_settings_app_notifications_read();
    }
    pub(crate) fn settings_app_notifications_permits(&self,owner:Owner,request:&AppNotificationsRequest)->bool{
        let r=&self.settings_runtime.app_notifications;
        self.settings_is_foreground(owner)&&r.active==Some(owner)&&r.fresh&&self.current_settings_app_notifications_read().as_ref()==r.desired.as_ref()&&r.snapshot.as_ref().is_some_and(|s|s.permits(request))
    }
    pub(crate) fn settings_app_notifications_resync(&mut self,cx:&mut Cx){self.settings_runtime.app_notifications.retire();self.refresh_settings_app(cx);}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn notification_reads_retire_across_owner_and_request_lifetimes(){
        let owner=(1,WidgetUid(10));let snapshot=AppNotificationsSnapshot::decode(&crate::settings_app_notifications::tests::snapshot(7,0,26)).unwrap();
        let key=AppNotificationsRead{target:snapshot.target.clone(),offset:0,generation:None};
        let mut runtime=SettingsAppNotificationsRuntime{active:Some(owner),desired:Some(key.clone()),read:Some(Read{id:7,owner,key,deadline:20.}),fresh:true,snapshot:Some(snapshot),..Default::default()};
        assert!(runtime.accepts(7,owner));assert!(!runtime.accepts(6,owner));assert!(!runtime.accepts(7,(1,WidgetUid(11))));
        runtime.retire();assert!(!runtime.accepts(7,owner));let cached=runtime.snapshot().unwrap();assert!(!cached.app.unwrap().can_set);assert!(!cached.rows[0].can_set);assert!(cached.rows[0].options.is_empty());assert_eq!(cached.rows[0].name,"Channel 00");
    }
}
