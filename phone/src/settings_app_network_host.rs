//! Foreground-root, selected-package and request correlation for app network controls.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_app_network::{AppNetworkRead,AppNetworkRequest,AppNetworkSnapshot}};
use makepad_strict_json::{s,Value};
use makepad_widgets::*;
type Owner=(ClientId,WidgetUid);
#[derive(Clone)]
struct Read{id:i64,owner:Owner,key:AppNetworkRead,deadline:f64}
#[derive(Default)]
pub(crate) struct SettingsAppNetworkRuntime{
    active:Option<Owner>,desired:Option<AppNetworkRead>,read:Option<Read>,next_read:f64,fresh:bool,focused:bool,
    snapshot:Option<AppNetworkSnapshot>,pub(crate) error:String,
}
impl SettingsAppNetworkRuntime {
    pub(crate) fn snapshot(&self)->Option<AppNetworkSnapshot>{let mut s=self.snapshot.clone()?;if !self.fresh{s.clear_actions();}Some(s)}
    fn focus(&mut self,observed:Option<bool>)->bool{
        let Some(focused)=observed else{return false;};
        if self.focused==focused{return false;}
        self.focused=focused;self.retire();true
    }
    fn retire(&mut self){self.read=None;self.fresh=false;self.next_read=0.;}
    fn accepts(&self,id:i64,owner:Owner)->bool{self.active==Some(owner)&&self.read.as_ref().is_some_and(|r|r.id==id&&r.owner==owner)}
}
impl App {
    pub(crate) fn settings_app_network_focus(&mut self,cx:&mut Cx,observed:Option<bool>){
        if self.settings_runtime.app_network.focus(observed){self.refresh_settings_app(cx);}
    }
    fn current_settings_app_network_read(&self)->Option<AppNetworkRead>{self.module_host.settings_instance()?.root.borrow::<SettingsView>()?.app_network_read()}
    fn observed_app_network_package(&self,key:&AppNetworkRead)->bool{self.settings_runtime.apps.details().is_some_and(|details|details.exists&&details.target==key.package)}
    pub(crate) fn settings_app_network_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool{
        let SettingsRequest::AppNetwork(AppNetworkRequest::Snapshot(key))=request else{return false;};
        if cfg!(target_os="android")&&self.settings_runtime.app_network.focused&&self.settings_is_foreground(owner)&&self.current_settings_app_network_read().as_ref()==Some(key)&&self.observed_app_network_package(key){self.start_settings_app_network_read(cx,owner,key.clone(),true);}true
    }
    fn start_settings_app_network_read(&mut self,cx:&mut Cx,owner:Owner,key:AppNetworkRead,manual:bool){
        if self.module_host.settings_client(owner.1)!=Some(owner.0)||!key.valid(){return;}
        if self.settings_runtime.app_network.read.as_ref().is_some_and(|r|r.owner==owner&&r.key==key){return;}
        let id=self.android_command_id(cx,"launcher","app_network_snapshot",vec![("package",s(key.package.package()))]);
        let runtime=&mut self.settings_runtime.app_network;
        if runtime.active!=Some(owner)||runtime.desired.as_ref()!=Some(&key){runtime.retire();}
        runtime.active=Some(owner);runtime.desired=Some(key.clone());runtime.read=Some(Read{id,owner,key,deadline:crate::host::now()+20.});runtime.next_read=crate::host::now()+5.;
        if manual{runtime.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_app_network_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64){
        let visible=visible.filter(|_|self.settings_runtime.app_network.focused);
        let desired=visible.and_then(|_|self.current_settings_app_network_read());let active=visible.filter(|_|desired.is_some());
        let runtime=&mut self.settings_runtime.app_network;
        if runtime.active!=active||runtime.desired!=desired{runtime.retire();runtime.active=active;runtime.desired=desired.clone();runtime.error.clear();self.refresh_settings_app(cx);}
        let(Some(owner),Some(key))=(active,desired)else{return;};if !self.observed_app_network_package(&key){return;}
        if self.settings_runtime.app_network.read.as_ref().is_some_and(|r|now>=r.deadline){self.fail_settings_app_network_read(cx,"Network settings did not arrive. Refresh to try again.");}
        if self.settings_runtime.app_network.read.is_none()&&now>=self.settings_runtime.app_network.next_read{self.start_settings_app_network_read(cx,owner,key,false);}
    }
    fn fail_settings_app_network_read(&mut self,cx:&mut Cx,message:&str){let r=&mut self.settings_runtime.app_network;r.retire();r.error=message.into();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);}
    pub(crate) fn settings_app_network_result(&mut self,cx:&mut Cx,v:&Value)->bool{
        let Some(read)=self.settings_runtime.app_network.read.as_ref()else{return false;};if v.get("id").and_then(Value::as_i64)!=Some(read.id){return false;}
        if v.get("status").and_then(Value::as_i64)!=Some(0){self.fail_settings_app_network_read(cx,"Android could not read app network settings. Refresh to try again.");}true
    }
    pub(crate) fn settings_app_network_observe(&mut self,cx:&mut Cx,v:&Value){
        let Some(read)=self.settings_runtime.app_network.read.clone()else{return;};
        if v.get("request_id").and_then(Value::as_i64)!=Some(read.id)||!self.settings_runtime.app_network.accepts(read.id,read.owner)||self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)||!self.settings_is_foreground(read.owner)||self.current_settings_app_network_read().as_ref()!=Some(&read.key){return;}
        let Some(snapshot)=AppNetworkSnapshot::decode(v).filter(|s|read.key.accepts(s))else{self.fail_settings_app_network_read(cx,"Android returned invalid app network settings. Refresh to try again.");return;};
        let r=&mut self.settings_runtime.app_network;r.snapshot=Some(snapshot);r.read=None;r.fresh=true;r.error.clear();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);
        self.settings_runtime.app_network.desired=self.current_settings_app_network_read();
    }
    pub(crate) fn settings_app_network_permits(&self,owner:Owner,request:&AppNetworkRequest)->bool{
        let r=&self.settings_runtime.app_network;
        self.settings_is_foreground(owner)&&r.focused&&r.active==Some(owner)&&r.fresh&&self.current_settings_app_network_read().as_ref().is_some_and(|key|self.observed_app_network_package(key))&&self.current_settings_app_network_read().as_ref()==r.desired.as_ref()&&r.snapshot.as_ref().is_some_and(|s|s.permits(request))
    }
    pub(crate) fn settings_app_network_resync(&mut self,cx:&mut Cx){self.settings_runtime.app_network.retire();self.refresh_settings_app(cx);}
}
