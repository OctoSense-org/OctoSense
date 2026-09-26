//! History is visible only to the foreground trusted Settings instance.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_notifications::{HistoryRead,HistorySnapshot,HistoryStatus}};
use makepad_strict_json::{s,Value};
use makepad_widgets::*;
type Owner=(ClientId,WidgetUid);
#[derive(Clone)]
struct Read {id:i64,owner:Owner,request:HistoryRead,deadline:f64}
#[derive(Default)]
pub(crate) struct SettingsHistoryRuntime {
    active:Option<Owner>,read:Option<Read>,next_read:f64,
    pub(crate) snapshot:Option<HistorySnapshot>,pub(crate) error:String,
}
impl SettingsHistoryRuntime {
    pub(crate) fn loading(&self)->bool {self.read.is_some()}
    fn retire(&mut self) {self.read=None;self.snapshot=None;self.error.clear();self.next_read=0.;}
}
impl App {
    fn settings_history_visible(&self,owner:Owner)->bool {
        self.settings_is_foreground(owner)&&self.module_host.settings_instance().is_some_and(|instance|
            instance.root.borrow::<SettingsView>().is_some_and(|view|view.notification_history_visible()))
    }
    pub(crate) fn settings_history_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool {
        let SettingsRequest::NotificationHistory(request)=request else{return false;};
        if cfg!(target_os="android")&&request.valid()&&self.settings_history_visible(owner) {
            if request.key.is_some()&&!self.settings_runtime.history.snapshot.as_ref().is_some_and(|state|state.key==request.key){return true;}
            self.start_history_read(cx,owner,request.clone());
        }
        true
    }
    fn start_history_read(&mut self,cx:&mut Cx,owner:Owner,request:HistoryRead) {
        if self.settings_runtime.history.read.is_some(){return;}
        let id=self.android_command_id(cx,"launcher","notification_history",vec![("offset",Value::Int(request.offset as i64)),("key",request.key.as_ref().map(|key|s(key.wire())).unwrap_or(Value::Null))]);
        let history=&mut self.settings_runtime.history;
        history.active=Some(owner);history.error.clear();
        if request.key.is_none(){history.snapshot=None;}
        history.read=Some(Read{id,owner,request,deadline:crate::host::now()+20.});history.next_read=crate::host::now()+10.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_history_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64) {
        let active=visible.filter(|owner|self.settings_history_visible(*owner));
        if self.settings_runtime.history.active!=active {
            self.settings_runtime.history.retire();self.settings_runtime.history.active=active;self.refresh_settings_app(cx);
        }
        let Some(owner)=active else{return;};
        if self.settings_runtime.history.read.as_ref().is_some_and(|read|now>=read.deadline) {self.fail_history_read(cx,"Notification history did not arrive. Refresh to try again.");}
        let history=&self.settings_runtime.history;
        if history.read.is_none()&&now>=history.next_read {
            if history.snapshot.as_ref().is_some_and(|state|state.status==HistoryStatus::Expired){return;}
            self.start_history_read(cx,owner,history.snapshot.as_ref().map(HistorySnapshot::refresh_page).unwrap_or_default());
        }
    }
    fn fail_history_read(&mut self,cx:&mut Cx,message:&str) {
        let history=&mut self.settings_runtime.history;history.read=None;history.snapshot=None;history.error=message.into();history.next_read=crate::host::now()+10.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_history_observe(&mut self,cx:&mut Cx,value:&Value) {
        let Some(read)=self.settings_runtime.history.read.clone()else{return;};
        if value.get("request_id").and_then(Value::as_i64)!=Some(read.id)||self.settings_runtime.history.active!=Some(read.owner)||!self.settings_history_visible(read.owner){return;}
        let Some(state)=HistorySnapshot::decode(value).filter(|state|state.status!=HistoryStatus::Ready
            ||state.offset==read.request.offset&&(read.request.key.is_none()||state.key==read.request.key))else{
            self.fail_history_read(cx,"Android returned invalid history information. Refresh to try again.");return;
        };
        let history=&mut self.settings_runtime.history;history.read=None;history.snapshot=Some(state);history.error.clear();history.next_read=crate::host::now()+10.;self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_history_result(&mut self,cx:&mut Cx,value:&Value)->bool {
        let Some(read)=self.settings_runtime.history.read.as_ref()else{return false;};
        if value.get("id").and_then(Value::as_i64)!=Some(read.id){return false;}
        if value.get("status").and_then(Value::as_i64)!=Some(0){self.fail_history_read(cx,"Android could not read notification history. Refresh to try again.");}
        true
    }
    pub(crate) fn settings_history_resync(&mut self,cx:&mut Cx) {self.settings_runtime.history.retire();self.refresh_settings_app(cx);}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn history_retirement_drops_content_and_pending_reads() {
        let owner=(1,WidgetUid(9));let mut runtime=SettingsHistoryRuntime{active:Some(owner),read:Some(Read{id:3,owner,request:HistoryRead::default(),deadline:20.}),
            snapshot:HistorySnapshot::decode(&crate::settings_notifications::tests::snapshot(2,20,0)),..Default::default()};
        assert!(runtime.loading());runtime.retire();assert!(!runtime.loading());assert!(runtime.snapshot.is_none());
    }
}
