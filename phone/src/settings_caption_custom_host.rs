//! A native caption appearance session belongs to one foreground Settings visit.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_caption_custom::{CaptionRequest,CaptionSnapshot}};
use makepad_strict_json::Value;
use makepad_widgets::*;
type Owner=(ClientId,WidgetUid);
#[derive(Clone,Copy)]struct Read{id:i64,owner:Owner,visit:i64,deadline:f64}
#[derive(Default)]pub(crate) struct SettingsCaptionCustomRuntime{active:Option<(Owner,i64)>,read:Option<Read>,next_read:f64,fresh:bool,focused:bool,snapshot:Option<CaptionSnapshot>,pub(crate) error:String}
impl SettingsCaptionCustomRuntime{
    pub(crate) fn snapshot(&self)->Option<CaptionSnapshot>{let mut s=self.snapshot.clone()?;if !self.fresh{s.retire();}Some(s)}
    fn retire_read(&mut self){self.read=None;self.fresh=false;self.next_read=0.;}
}
impl App{
    fn current_caption_visit(&self)->Option<i64>{self.module_host.settings_instance()?.root.borrow::<SettingsView>()?.caption_visit()}
    fn close_caption_scope(&mut self,cx:&mut Cx){if let Some((_,visit))=self.settings_runtime.caption_custom.active.take(){self.android_command_id(cx,"launcher","caption_custom_close",vec![("visit",Value::Int(visit))]);}self.settings_runtime.caption_custom.retire_read();}
    pub(crate) fn settings_caption_custom_focus(&mut self,cx:&mut Cx,focused:Option<bool>){let Some(focused)=focused else{return;};if self.settings_runtime.caption_custom.focused==focused{return;}
        self.settings_runtime.caption_custom.focused=focused;self.close_caption_scope(cx);
        if let Some(instance)=self.module_host.settings_instance(){let(root,vm)=(instance.root.clone(),instance.vm_id);let isolate=makepad_widgets::widget_async::enter_isolate(cx,vm);if let Some(mut view)=root.borrow_mut::<SettingsView>(){if view.caption_visit().is_some(){view.renew_caption_visit(cx);}}makepad_widgets::widget_async::leave_isolate(cx,isolate);}
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_caption_custom_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool{
        let SettingsRequest::CaptionCustom(CaptionRequest::Snapshot{visit})=request else{return false;};
        if cfg!(target_os="android")&&self.settings_runtime.caption_custom.focused&&self.settings_is_foreground(owner)&&self.current_caption_visit()==Some(*visit){self.start_caption_read(cx,owner,*visit,true);}true
    }
    fn start_caption_read(&mut self,cx:&mut Cx,owner:Owner,visit:i64,manual:bool){
        if visit<=0||self.module_host.settings_client(owner.1)!=Some(owner.0){return;}
        if self.settings_runtime.caption_custom.active!=Some((owner,visit)){self.close_caption_scope(cx);self.settings_runtime.caption_custom.active=Some((owner,visit));}
        if self.settings_runtime.caption_custom.read.is_some(){return;}
        let id=self.android_command_id(cx,"launcher","caption_custom_snapshot",vec![("visit",Value::Int(visit))]);let r=&mut self.settings_runtime.caption_custom;
        r.read=Some(Read{id,owner,visit,deadline:crate::host::now()+20.});r.next_read=crate::host::now()+5.;if manual{r.error.clear();}self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_caption_custom_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64){
        let desired=visible.filter(|_|self.settings_runtime.caption_custom.focused).and_then(|owner|self.current_caption_visit().map(|visit|(owner,visit)));
        if self.settings_runtime.caption_custom.active!=desired{
            let lost_visibility=self.settings_runtime.caption_custom.active.is_some()&&desired.is_none();self.close_caption_scope(cx);
            if lost_visibility{if let Some(instance)=self.module_host.settings_instance(){let(root,vm)=(instance.root.clone(),instance.vm_id);let isolate=makepad_widgets::widget_async::enter_isolate(cx,vm);if let Some(mut view)=root.borrow_mut::<SettingsView>(){if view.caption_visit().is_some(){view.renew_caption_visit(cx);}}makepad_widgets::widget_async::leave_isolate(cx,isolate);}}
            self.settings_runtime.caption_custom.active=desired;self.settings_runtime.caption_custom.error.clear();self.refresh_settings_app(cx);
        }
        let Some((owner,visit))=desired else{return;};
        if self.settings_runtime.caption_custom.read.is_some_and(|r|now>=r.deadline){self.fail_caption_read(cx,"Caption appearance did not arrive. Refresh to try again.");}
        if self.settings_runtime.caption_custom.read.is_none()&&now>=self.settings_runtime.caption_custom.next_read{self.start_caption_read(cx,owner,visit,false);}
    }
    fn fail_caption_read(&mut self,cx:&mut Cx,message:&str){let r=&mut self.settings_runtime.caption_custom;r.retire_read();r.error=message.into();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);}
    pub(crate) fn settings_caption_custom_result(&mut self,cx:&mut Cx,value:&Value)->bool{let Some(read)=self.settings_runtime.caption_custom.read else{return false;};if value.get("id").and_then(Value::as_i64)!=Some(read.id){return false;}if value.get("status").and_then(Value::as_i64)!=Some(0){self.fail_caption_read(cx,"Android could not read caption appearance.");}true}
    pub(crate) fn settings_caption_custom_observe(&mut self,cx:&mut Cx,value:&Value){let Some(read)=self.settings_runtime.caption_custom.read else{return;};let r=&self.settings_runtime.caption_custom;
        if value.get("request_id").and_then(Value::as_i64)!=Some(read.id)||!r.focused||r.active!=Some((read.owner,read.visit))||!self.settings_is_foreground(read.owner)||self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)||self.current_caption_visit()!=Some(read.visit){return;}
        let Some(snapshot)=CaptionSnapshot::decode(value).filter(|s|s.visit==read.visit)else{self.fail_caption_read(cx,"Android returned invalid caption appearance information.");return;};
        let r=&mut self.settings_runtime.caption_custom;r.read=None;r.fresh=true;r.snapshot=Some(snapshot);r.error.clear();r.next_read=crate::host::now()+5.;self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_caption_custom_permits(&self,owner:Owner,request:&CaptionRequest)->bool{let r=&self.settings_runtime.caption_custom;r.focused&&r.fresh&&r.active==Some((owner,request.visit()))&&self.current_caption_visit()==Some(request.visit())&&self.settings_is_foreground(owner)&&r.snapshot.as_ref().is_some_and(|s|s.permits(request))}
    pub(crate) fn settings_caption_custom_resync(&mut self,cx:&mut Cx){self.settings_runtime.caption_custom.retire_read();self.refresh_settings_app(cx);}
}
#[cfg(test)]mod tests{use super::*;
    #[test]fn caption_retirement_keeps_observed_values_but_removes_scope_authority(){let snapshot=CaptionSnapshot::decode(&crate::settings_caption_custom::tests::snapshot(1,7)).unwrap();let mut state=SettingsCaptionCustomRuntime{focused:true,fresh:true,active:Some(((1,WidgetUid(2)),7)),snapshot:Some(snapshot),read:Some(Read{id:3,owner:(1,WidgetUid(2)),visit:7,deadline:20.}),..Default::default()};state.retire_read();assert!(state.read.is_none());let s=state.snapshot().unwrap();assert!(!s.active);assert!(s.controls.controls.iter().all(|r|r.options.is_empty()&&r.value.is_some()));assert_eq!(s.visit,7);}
}
