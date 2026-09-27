//! Trusted Settings root and one-use native keyboard observations.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_keyboards::{KeyboardRequest,KeyboardSnapshot,KeyboardRead}};
use makepad_strict_json::{s,Value};
use makepad_widgets::*;
type Owner=(ClientId,WidgetUid);
#[derive(Clone)]struct Read{id:i64,owner:Owner,target:KeyboardRead,deadline:f64}
#[derive(Default)]pub(crate) struct SettingsKeyboardsRuntime {
    active:Option<Owner>,desired:Option<KeyboardRead>,read:Option<Read>,next_read:f64,fresh:bool,focused:bool,
    snapshot:Option<KeyboardSnapshot>,pub(crate) error:String,
}
impl SettingsKeyboardsRuntime {
    pub(crate) fn snapshot(&self)->Option<KeyboardSnapshot>{let mut value=self.snapshot.clone()?;if !self.fresh{value.clear_actions();}Some(value)}
    fn retire(&mut self){self.read=None;self.fresh=false;self.next_read=0.;}
    fn focus(&mut self,value:Option<bool>)->bool{let Some(value)=value else{return false;};if value==self.focused{return false;}self.focused=value;self.retire();true}
}
impl App {
    pub(crate) fn settings_keyboards_focus(&mut self,cx:&mut Cx,value:Option<bool>){if self.settings_runtime.keyboards.focus(value){self.refresh_settings_app(cx);}}
    fn current_settings_keyboards_read(&self)->Option<KeyboardRead>{self.module_host.settings_instance()?.root.borrow::<SettingsView>()?.keyboards_read()}
    pub(crate) fn settings_keyboards_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool{
        let SettingsRequest::Keyboard(KeyboardRequest::Snapshot(target))=request else{return false;};
        if cfg!(target_os="android")&&self.settings_runtime.keyboards.focused&&self.settings_is_foreground(owner)&&self.current_settings_keyboards_read().as_ref()==Some(target){self.start_settings_keyboards_read(cx,owner,target.clone(),true);}true
    }
    fn start_settings_keyboards_read(&mut self,cx:&mut Cx,owner:Owner,target:KeyboardRead,manual:bool){
        if !target.valid()||self.module_host.settings_client(owner.1)!=Some(owner.0){return;}
        if self.settings_runtime.keyboards.read.as_ref().is_some_and(|r|r.owner==owner&&r.target==target){return;}
        let id=self.android_command_id(cx,"launcher","keyboards_snapshot",vec![("query",s(&target.query)),("offset",Value::Int(target.offset as i64))]);
        let runtime=&mut self.settings_runtime.keyboards;
        if runtime.active!=Some(owner)||runtime.desired.as_ref()!=Some(&target){runtime.retire();}
        runtime.active=Some(owner);runtime.desired=Some(target.clone());runtime.read=Some(Read{id,owner,target,deadline:crate::host::now()+20.});runtime.next_read=crate::host::now()+5.;
        if manual{runtime.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_keyboards_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64){
        let visible=visible.filter(|_|self.settings_runtime.keyboards.focused);let desired=visible.and_then(|_|self.current_settings_keyboards_read());let active=visible.filter(|_|desired.is_some());
        let r=&mut self.settings_runtime.keyboards;
        if r.active!=active||r.desired!=desired{r.retire();r.active=active;r.desired=desired.clone();r.error.clear();self.refresh_settings_app(cx);}
        let(Some(owner),Some(target))=(active,desired)else{return;};
        if self.settings_runtime.keyboards.read.as_ref().is_some_and(|r|now>=r.deadline){self.fail_settings_keyboards_read(cx,"Keyboard information did not arrive. Refresh to try again.");}
        if self.settings_runtime.keyboards.read.is_none()&&now>=self.settings_runtime.keyboards.next_read{self.start_settings_keyboards_read(cx,owner,target,false);}
    }
    fn fail_settings_keyboards_read(&mut self,cx:&mut Cx,message:&str){let r=&mut self.settings_runtime.keyboards;r.retire();r.error=message.into();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);}
    pub(crate) fn settings_keyboards_result(&mut self,cx:&mut Cx,value:&Value)->bool{
        let Some(read)=self.settings_runtime.keyboards.read.as_ref()else{return false;};if value.get("id").and_then(Value::as_i64)!=Some(read.id){return false;}
        if value.get("status").and_then(Value::as_i64)!=Some(0){self.fail_settings_keyboards_read(cx,"Android could not read keyboard choices. Refresh to try again.");}true
    }
    pub(crate) fn settings_keyboards_observe(&mut self,cx:&mut Cx,value:&Value){
        let Some(read)=self.settings_runtime.keyboards.read.clone()else{return;};let r=&self.settings_runtime.keyboards;
        if value.get("request_id").and_then(Value::as_i64)!=Some(read.id)||r.active!=Some(read.owner)||!r.focused||self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)||!self.settings_is_foreground(read.owner)||self.current_settings_keyboards_read().as_ref()!=Some(&read.target){return;}
        let Some(snapshot)=KeyboardSnapshot::decode(value).filter(|s|s.matches(&read.target))else{self.fail_settings_keyboards_read(cx,"Android returned invalid keyboard information.");return;};
        let r=&mut self.settings_runtime.keyboards;r.snapshot=Some(snapshot);r.read=None;r.fresh=true;r.error.clear();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);self.settings_runtime.keyboards.desired=self.current_settings_keyboards_read();
    }
    pub(crate) fn settings_keyboards_permits(&self,owner:Owner,request:&KeyboardRequest)->bool{
        let r=&self.settings_runtime.keyboards;self.settings_is_foreground(owner)&&r.focused&&r.active==Some(owner)&&r.fresh&&self.current_settings_keyboards_read().as_ref()==r.desired.as_ref()&&r.snapshot.as_ref().is_some_and(|s|s.permits(request))
    }
    pub(crate) fn settings_keyboards_resync(&mut self,cx:&mut Cx){self.settings_runtime.keyboards.retire();self.refresh_settings_app(cx);}
    pub(crate) fn settings_keyboards_flow_started(&mut self,cx:&mut Cx){
        self.settings_runtime.keyboards.retire();
        if let Some(instance)=self.module_host.settings_instance(){let(root,vm)=(instance.root.clone(),instance.vm_id);let isolate=makepad_widgets::widget_async::enter_isolate(cx,vm);if let Some(mut view)=root.borrow_mut::<SettingsView>(){view.keyboards_after_flow(cx);}makepad_widgets::widget_async::leave_isolate(cx,isolate);}
        self.refresh_settings_app(cx);
    }
}
