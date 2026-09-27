//! Current trusted Settings root, visible package and one-use battery-policy observations.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_apps::AppTarget,settings_app_battery::{AppBatteryRequest,AppBatterySnapshot}};
use makepad_strict_json::{s,Value};
use makepad_widgets::*;
type Owner=(ClientId,WidgetUid);
#[derive(Clone)]struct Read{id:i64,owner:Owner,target:AppTarget,deadline:f64}
#[derive(Default)]pub(crate) struct SettingsAppBatteryRuntime {
    active:Option<Owner>,desired:Option<AppTarget>,read:Option<Read>,next_read:f64,fresh:bool,focused:bool,
    snapshot:Option<AppBatterySnapshot>,pub(crate) error:String,
}
impl SettingsAppBatteryRuntime {
    pub(crate) fn snapshot(&self)->Option<AppBatterySnapshot>{let mut value=self.snapshot.clone()?;if !self.fresh{value.clear_actions();}Some(value)}
    fn retire(&mut self){self.read=None;self.fresh=false;self.next_read=0.;}
    fn focus(&mut self,value:Option<bool>)->bool{let Some(value)=value else{return false;};if value==self.focused{return false;}self.focused=value;self.retire();true}
}
impl App {
    pub(crate) fn settings_app_battery_focus(&mut self,cx:&mut Cx,value:Option<bool>){if self.settings_runtime.app_battery.focus(value){self.refresh_settings_app(cx);}}
    fn current_settings_app_battery_read(&self)->Option<AppTarget>{self.module_host.settings_instance()?.root.borrow::<SettingsView>()?.app_battery_read()}
    fn observed_app_battery_package(&self,target:&AppTarget)->bool{self.settings_runtime.apps.details().is_some_and(|details|details.exists&&details.target==*target)}
    pub(crate) fn settings_app_battery_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool{
        let SettingsRequest::AppBattery(AppBatteryRequest::Snapshot(target))=request else{return false;};
        if cfg!(target_os="android")&&self.settings_runtime.app_battery.focused&&self.settings_is_foreground(owner)&&self.current_settings_app_battery_read().as_ref()==Some(target)&&self.observed_app_battery_package(target){self.start_settings_app_battery_read(cx,owner,target.clone(),true);}true
    }
    fn start_settings_app_battery_read(&mut self,cx:&mut Cx,owner:Owner,target:AppTarget,manual:bool){
        if self.module_host.settings_client(owner.1)!=Some(owner.0)||!self.observed_app_battery_package(&target){return;}
        if self.settings_runtime.app_battery.read.as_ref().is_some_and(|r|r.owner==owner&&r.target==target){return;}
        let id=self.android_command_id(cx,"launcher","app_battery_snapshot",vec![("package",s(target.package()))]);
        let runtime=&mut self.settings_runtime.app_battery;
        if runtime.active!=Some(owner)||runtime.desired.as_ref()!=Some(&target){runtime.retire();}
        runtime.active=Some(owner);runtime.desired=Some(target.clone());runtime.read=Some(Read{id,owner,target,deadline:crate::host::now()+20.});runtime.next_read=crate::host::now()+5.;
        if manual{runtime.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_app_battery_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64){
        let visible=visible.filter(|_|self.settings_runtime.app_battery.focused);let desired=visible.and_then(|_|self.current_settings_app_battery_read());let active=visible.filter(|_|desired.is_some());
        let r=&mut self.settings_runtime.app_battery;
        if r.active!=active||r.desired!=desired{r.retire();r.active=active;r.desired=desired.clone();r.error.clear();self.refresh_settings_app(cx);}
        let(Some(owner),Some(target))=(active,desired)else{return;};if !self.observed_app_battery_package(&target){self.settings_runtime.app_battery.retire();return;}
        if self.settings_runtime.app_battery.read.as_ref().is_some_and(|r|now>=r.deadline){self.fail_settings_app_battery_read(cx,"App battery information did not arrive. Refresh to try again.");}
        if self.settings_runtime.app_battery.read.is_none()&&now>=self.settings_runtime.app_battery.next_read{self.start_settings_app_battery_read(cx,owner,target,false);}
    }
    fn fail_settings_app_battery_read(&mut self,cx:&mut Cx,message:&str){let r=&mut self.settings_runtime.app_battery;r.retire();r.error=message.into();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);}
    pub(crate) fn settings_app_battery_result(&mut self,cx:&mut Cx,value:&Value)->bool{
        let Some(read)=self.settings_runtime.app_battery.read.as_ref()else{return false;};if value.get("id").and_then(Value::as_i64)!=Some(read.id){return false;}
        if value.get("status").and_then(Value::as_i64)!=Some(0){self.fail_settings_app_battery_read(cx,"Android could not read app battery policy. Refresh to try again.");}true
    }
    pub(crate) fn settings_app_battery_observe(&mut self,cx:&mut Cx,value:&Value){
        let Some(read)=self.settings_runtime.app_battery.read.clone()else{return;};let r=&self.settings_runtime.app_battery;
        if value.get("request_id").and_then(Value::as_i64)!=Some(read.id)||r.active!=Some(read.owner)||!r.focused||self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)||!self.settings_is_foreground(read.owner)||self.current_settings_app_battery_read().as_ref()!=Some(&read.target)||!self.observed_app_battery_package(&read.target){return;}
        let Some(snapshot)=AppBatterySnapshot::decode(value).filter(|s|s.target==read.target)else{self.fail_settings_app_battery_read(cx,"Android returned invalid app battery information.");return;};
        let r=&mut self.settings_runtime.app_battery;r.snapshot=Some(snapshot);r.read=None;r.fresh=true;r.error.clear();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_app_battery_permits(&self,owner:Owner,request:&AppBatteryRequest)->bool{
        let r=&self.settings_runtime.app_battery;self.settings_is_foreground(owner)&&r.focused&&r.active==Some(owner)&&r.fresh&&r.desired.as_ref().is_some_and(|target|self.observed_app_battery_package(target))&&self.current_settings_app_battery_read().as_ref()==r.desired.as_ref()&&r.snapshot.as_ref().is_some_and(|s|s.permits(request))
    }
    pub(crate) fn settings_app_battery_resync(&mut self,cx:&mut Cx){self.settings_runtime.app_battery.retire();self.refresh_settings_app(cx);}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn battery_focus_loss_preserves_observation_but_retires_authority_and_pending_read(){let state=AppBatterySnapshot::decode(&crate::settings_app_battery::tests::snapshot(1)).unwrap();let owner=(1,WidgetUid(9));
        let mut r=SettingsAppBatteryRuntime{active:Some(owner),desired:Some(state.target.clone()),read:Some(Read{id:2,owner,target:state.target.clone(),deadline:50.}),fresh:true,focused:true,snapshot:Some(state),..Default::default()};
        assert!(r.focus(Some(false)));assert!(r.read.is_none());assert!(r.snapshot().unwrap().choices.is_empty());assert!(r.focus(Some(true)));assert_eq!(r.next_read,0.);assert!(!r.fresh);}
}
