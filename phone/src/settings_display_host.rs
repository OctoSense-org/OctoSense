//! Display reads and actions stay bound to the foreground trusted Settings root.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_display::{DisplayRequest,DisplaySnapshot}};
use makepad_strict_json::Value;
use makepad_widgets::*;

type Owner=(ClientId,WidgetUid);
#[derive(Clone,Copy)]
struct Read { id:i64, owner:Owner, deadline:f64 }
#[derive(Default)]
pub(crate) struct SettingsDisplayRuntime {
    active:Option<Owner>, read:Option<Read>, next_read:f64, fresh:bool,
    snapshot:Option<DisplaySnapshot>, pub(crate) error:String,
}
impl SettingsDisplayRuntime {
    pub(crate) fn snapshot(&self)->Option<DisplaySnapshot> {
        let mut snapshot=self.snapshot.clone()?;
        if !self.fresh {snapshot.clear_actions();}
        Some(snapshot)
    }
    fn retire(&mut self) {self.read=None;self.fresh=false;self.next_read=0.;}
    fn accepts(&self,id:i64,owner:Owner)->bool {self.active==Some(owner)&&self.read.is_some_and(|read|read.id==id&&read.owner==owner)}
}
impl App {
    fn settings_display_visible(&self)->bool {
        self.module_host.settings_instance().and_then(|instance|instance.root.borrow::<SettingsView>().map(|view|view.display_visible())).unwrap_or(false)
    }
    pub(crate) fn settings_display_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool {
        if !matches!(request,SettingsRequest::Display(DisplayRequest::Snapshot)) {return false;}
        if cfg!(target_os="android")&&self.settings_is_foreground(owner)&&self.settings_display_visible() {
            self.start_settings_display_read(cx,owner,true);
        }
        true
    }
    fn start_settings_display_read(&mut self,cx:&mut Cx,owner:Owner,manual:bool) {
        if self.module_host.settings_client(owner.1)!=Some(owner.0) {return;}
        if self.settings_runtime.display.read.is_some_and(|read|read.owner==owner) {return;}
        if self.settings_runtime.display.active!=Some(owner) {self.settings_runtime.display.retire();}
        let id=self.android_command_id(cx,"launcher","display_snapshot",vec![]);
        let display=&mut self.settings_runtime.display;
        display.active=Some(owner);display.read=Some(Read{id,owner,deadline:crate::host::now()+20.});display.next_read=crate::host::now()+5.;
        if manual {display.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_display_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64) {
        let active=visible.filter(|_|self.settings_display_visible());
        if self.settings_runtime.display.active!=active {
            let display=&mut self.settings_runtime.display;display.retire();display.active=active;display.error.clear();self.refresh_settings_app(cx);
        }
        let Some(owner)=active else {return;};
        if self.settings_runtime.display.read.is_some_and(|read|now>=read.deadline) {
            self.fail_settings_display_read(cx,"Display information did not arrive. Refresh to try again.");
        }
        if self.settings_runtime.display.read.is_none()&&now>=self.settings_runtime.display.next_read {
            self.start_settings_display_read(cx,owner,false);
        }
    }
    fn fail_settings_display_read(&mut self,cx:&mut Cx,message:&str) {
        let display=&mut self.settings_runtime.display;display.read=None;display.fresh=false;display.error=message.into();display.next_read=crate::host::now()+5.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_display_result(&mut self,cx:&mut Cx,value:&Value)->bool {
        let Some(read)=self.settings_runtime.display.read else {return false;};
        if value.get("id").and_then(Value::as_i64)!=Some(read.id) {return false;}
        if value.get("status").and_then(Value::as_i64)!=Some(0) {
            self.fail_settings_display_read(cx,"Android could not read Display information. Refresh to try again.");
        }
        true
    }
    pub(crate) fn settings_display_observe(&mut self,cx:&mut Cx,value:&Value) {
        let Some(read)=self.settings_runtime.display.read else {return;};
        if value.get("request_id").and_then(Value::as_i64)!=Some(read.id)
            || !self.settings_runtime.display.accepts(read.id,read.owner)
            || self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)
            || !self.settings_is_foreground(read.owner)||!self.settings_display_visible() {return;}
        let Some(snapshot)=DisplaySnapshot::decode(value) else {
            self.fail_settings_display_read(cx,"Android returned invalid Display information. Refresh to try again.");return;
        };
        let display=&mut self.settings_runtime.display;display.snapshot=Some(snapshot);display.read=None;display.fresh=true;display.error.clear();display.next_read=crate::host::now()+5.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_display_permits(&self,owner:Owner,request:&DisplayRequest)->bool {
        if !self.settings_is_foreground(owner)||!self.settings_display_visible() {return false;}
        let display=&self.settings_runtime.display;
        if display.active!=Some(owner)||!display.fresh||!display.snapshot.as_ref().is_some_and(|snapshot|snapshot.permits(request)) {return false;}
        true
    }
    pub(crate) fn settings_display_resync(&mut self,cx:&mut Cx) {self.settings_runtime.display.retire();self.refresh_settings_app(cx);}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_observations_cannot_cross_request_or_root_lifetimes() {
        let owner=(1,WidgetUid(10));let replacement=(1,WidgetUid(11));
        let mut display=SettingsDisplayRuntime {active:Some(owner),read:Some(Read{id:4,owner,deadline:20.}),fresh:true,
            snapshot:DisplaySnapshot::decode(&crate::settings_display::tests::snapshot()),..Default::default()};
        assert!(display.accepts(4,owner));assert!(!display.accepts(3,owner));assert!(!display.accepts(4,replacement));
        display.retire();assert!(!display.accepts(4,owner));
        let cached=display.snapshot().unwrap();assert!(!cached.density.can_set);assert!(cached.night.caps.is_empty());
        assert_eq!(cached.density.current,Some(420));
    }
}
