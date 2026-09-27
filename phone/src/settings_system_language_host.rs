//! Trusted Settings root and one-use native system language observations.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_system_language::{SystemLanguageRequest,SystemLanguageSnapshot,SystemLanguageRead,SystemLanguageKey,SystemLanguageKind}};
use makepad_strict_json::{s,Value};
use makepad_widgets::*;
type Owner=(ClientId,WidgetUid);
#[derive(Clone)]struct Read{id:i64,owner:Owner,target:SystemLanguageRead,deadline:f64}
#[derive(Default)]pub(crate) struct SettingsSystemLanguageRuntime {
    active:Option<Owner>,desired:Option<SystemLanguageRead>,read:Option<Read>,next_read:f64,fresh:bool,focused:bool,
    snapshot:Option<SystemLanguageSnapshot>,observed:std::collections::HashMap<SystemLanguageKey,String>,pub(crate) error:String,
}
impl SettingsSystemLanguageRuntime {
    pub(crate) fn snapshot(&self)->Option<SystemLanguageSnapshot>{let mut value=self.snapshot.clone()?;if !self.fresh{value.clear_actions();}Some(value)}
    fn retire(&mut self){self.read=None;self.fresh=false;self.next_read=0.;}
    fn focus(&mut self,value:Option<bool>)->bool{let Some(value)=value else{return false;};if value==self.focused{return false;}self.focused=value;self.retire();if !value{self.observed.clear();}true}
}
impl App {
    pub(crate) fn settings_system_language_focus(&mut self,cx:&mut Cx,value:Option<bool>){if self.settings_runtime.system_language.focus(value){self.refresh_settings_app(cx);}}
    fn current_settings_system_language_read(&self)->Option<SystemLanguageRead>{self.module_host.settings_instance()?.root.borrow::<SettingsView>()?.system_language_read()}
    pub(crate) fn settings_system_language_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool{
        let SettingsRequest::SystemLanguages(SystemLanguageRequest::Snapshot(target))=request else{return false;};
        if cfg!(target_os="android")&&self.settings_runtime.system_language.focused&&self.settings_is_foreground(owner)&&self.current_settings_system_language_read().as_ref()==Some(target){self.start_settings_system_language_read(cx,owner,target.clone(),true);}true
    }
    fn start_settings_system_language_read(&mut self,cx:&mut Cx,owner:Owner,target:SystemLanguageRead,manual:bool){
        if !target.valid()||self.module_host.settings_client(owner.1)!=Some(owner.0){return;}
        if self.settings_runtime.system_language.read.as_ref().is_some_and(|r|r.owner==owner&&r.target==target){return;}
        let id=self.android_command_id(cx,"launcher","system_languages_snapshot",vec![("key",s(target.key.as_ref().map(|k|k.wire()).unwrap_or(""))),("parent",s(target.parent.as_ref().map(|k|k.wire()).unwrap_or(""))),("query",s(&target.query)),("offset",Value::Int(target.offset as i64))]);
        let runtime=&mut self.settings_runtime.system_language;
        if runtime.active!=Some(owner)||runtime.desired.as_ref()!=Some(&target){runtime.retire();}
        runtime.active=Some(owner);runtime.desired=Some(target.clone());runtime.read=Some(Read{id,owner,target,deadline:crate::host::now()+20.});runtime.next_read=crate::host::now()+5.;
        if manual{runtime.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_system_language_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64){
        let visible=visible.filter(|_|self.settings_runtime.system_language.focused);let desired=visible.and_then(|_|self.current_settings_system_language_read());let active=visible.filter(|_|desired.is_some());
        let r=&mut self.settings_runtime.system_language;
        if r.active!=active||r.desired!=desired{r.retire();r.active=active;r.desired=desired.clone();r.error.clear();self.refresh_settings_app(cx);}
        let(Some(owner),Some(target))=(active,desired)else{return;};
        if self.settings_runtime.system_language.read.as_ref().is_some_and(|r|now>=r.deadline){self.fail_settings_system_language_read(cx,"System language information did not arrive. Refresh to try again.");}
        if self.settings_runtime.system_language.read.is_none()&&now>=self.settings_runtime.system_language.next_read{self.start_settings_system_language_read(cx,owner,target,false);}
    }
    fn fail_settings_system_language_read(&mut self,cx:&mut Cx,message:&str){let r=&mut self.settings_runtime.system_language;r.retire();r.error=message.into();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);}
    pub(crate) fn settings_system_language_result(&mut self,cx:&mut Cx,value:&Value)->bool{
        let Some(read)=self.settings_runtime.system_language.read.as_ref()else{return false;};if value.get("id").and_then(Value::as_i64)!=Some(read.id){return false;}
        if value.get("status").and_then(Value::as_i64)!=Some(0){self.fail_settings_system_language_read(cx,"Android could not read system language choices. Refresh to try again.");}true
    }
    pub(crate) fn settings_system_language_observe(&mut self,cx:&mut Cx,value:&Value){
        let Some(read)=self.settings_runtime.system_language.read.clone()else{return;};let r=&self.settings_runtime.system_language;
        if value.get("request_id").and_then(Value::as_i64)!=Some(read.id)||r.active!=Some(read.owner)||!r.focused||self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)||!self.settings_is_foreground(read.owner)||self.current_settings_system_language_read().as_ref()!=Some(&read.target){return;}
        let Some(snapshot)=SystemLanguageSnapshot::decode(value).filter(|s|s.matches(&read.target))else{self.fail_settings_system_language_read(cx,"Android returned invalid system language information.");return;};
        let r=&mut self.settings_runtime.system_language;
        if r.snapshot.as_ref().and_then(|s|s.key.as_ref())!=snapshot.key.as_ref(){r.observed.clear();}
        if let Some(current)=&snapshot.current{for row in current{r.observed.insert(row.target.clone(),row.tag.clone());}}
        for row in &snapshot.rows{if row.kind==SystemLanguageKind::Select{r.observed.insert(row.target.clone(),row.tag.clone());}}
        r.snapshot=Some(snapshot);r.read=None;r.fresh=true;r.error.clear();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);self.settings_runtime.system_language.desired=self.current_settings_system_language_read();
    }
    pub(crate) fn settings_system_language_permits(&self,owner:Owner,request:&SystemLanguageRequest)->bool{
        let SystemLanguageRequest::Apply{key,order}=request else{return false;};let r=&self.settings_runtime.system_language;
        let valid_order=request.valid()&&order.iter().all(|target|r.observed.contains_key(target))&&order.iter().enumerate().all(|(index,target)|!order[..index].iter().any(|old|r.observed.get(old)==r.observed.get(target)));
        self.settings_is_foreground(owner)&&r.focused&&r.active==Some(owner)&&r.fresh&&valid_order&&self.current_settings_system_language_read().as_ref()==r.desired.as_ref()&&r.snapshot.as_ref().is_some_and(|s|s.can_apply&&s.key.as_ref()==Some(key))
    }
    pub(crate) fn settings_system_language_resync(&mut self,cx:&mut Cx){self.settings_runtime.system_language.retire();self.refresh_settings_app(cx);}
    pub(crate) fn settings_system_language_selection_started(&mut self,cx:&mut Cx){
        self.settings_runtime.system_language.retire();self.settings_runtime.system_language.observed.clear();
        if let Some(instance)=self.module_host.settings_instance(){let(root,vm)=(instance.root.clone(),instance.vm_id);let isolate=makepad_widgets::widget_async::enter_isolate(cx,vm);if let Some(mut view)=root.borrow_mut::<SettingsView>(){view.system_language_after_selection(cx);}makepad_widgets::widget_async::leave_isolate(cx,isolate);}
        self.refresh_settings_app(cx);
    }
}
