//! Updates reads and actions stay bound to the foreground trusted Settings root.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_updates::{UpdatesRequest,UpdatesSnapshot}};
use makepad_strict_json::Value;
use makepad_widgets::*;

type Owner=(ClientId,WidgetUid);
#[derive(Clone,Copy)]
struct Read { id:i64, owner:Owner, deadline:f64 }
#[derive(Default)]
pub(crate) struct SettingsUpdatesRuntime {
    active:Option<Owner>, read:Option<Read>, next_read:f64, fresh:bool,
    snapshot:Option<UpdatesSnapshot>, pub(crate) error:String,
}
impl SettingsUpdatesRuntime {
    pub(crate) fn snapshot(&self)->Option<UpdatesSnapshot> {
        let mut snapshot=self.snapshot.clone()?;
        if !self.fresh {snapshot.clear_actions();}
        Some(snapshot)
    }
    fn retire(&mut self) {self.read=None;self.fresh=false;self.next_read=0.;}
    fn accepts(&self,id:i64,owner:Owner)->bool {self.active==Some(owner)&&self.read.is_some_and(|read|read.id==id&&read.owner==owner)}
}
impl App {
    fn settings_updates_visible(&self)->bool {
        self.module_host.settings_instance().and_then(|instance|instance.root.borrow::<SettingsView>().map(|view|view.updates_visible())).unwrap_or(false)
    }
    pub(crate) fn settings_updates_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool {
        if !matches!(request,SettingsRequest::Updates(UpdatesRequest::Snapshot)) {return false;}
        if cfg!(target_os="android")&&self.settings_is_foreground(owner)&&self.settings_updates_visible() {
            self.start_settings_updates_read(cx,owner,true);
        }
        true
    }
    fn start_settings_updates_read(&mut self,cx:&mut Cx,owner:Owner,manual:bool) {
        if self.module_host.settings_client(owner.1)!=Some(owner.0) {return;}
        if self.settings_runtime.updates.read.is_some_and(|read|read.owner==owner) {return;}
        if self.settings_runtime.updates.active!=Some(owner) {self.settings_runtime.updates.retire();}
        let id=self.android_command_id(cx,"launcher","updates_snapshot",vec![]);
        let updates=&mut self.settings_runtime.updates;
        updates.active=Some(owner);updates.read=Some(Read{id,owner,deadline:crate::host::now()+20.});updates.next_read=crate::host::now()+5.;
        if manual {updates.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_updates_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64) {
        let active=visible.filter(|_|self.settings_updates_visible());
        if self.settings_runtime.updates.active!=active {
            let updates=&mut self.settings_runtime.updates;updates.retire();updates.active=active;updates.error.clear();self.refresh_settings_app(cx);
        }
        let Some(owner)=active else {return;};
        if self.settings_runtime.updates.read.is_some_and(|read|now>=read.deadline) {
            self.fail_settings_updates_read(cx,"Updates information did not arrive. Refresh to try again.");
        }
        if self.settings_runtime.updates.read.is_none()&&now>=self.settings_runtime.updates.next_read {
            self.start_settings_updates_read(cx,owner,false);
        }
    }
    fn fail_settings_updates_read(&mut self,cx:&mut Cx,message:&str) {
        let updates=&mut self.settings_runtime.updates;updates.read=None;updates.fresh=false;updates.error=message.into();updates.next_read=crate::host::now()+5.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_updates_result(&mut self,cx:&mut Cx,value:&Value)->bool {
        let Some(read)=self.settings_runtime.updates.read else {return false;};
        if value.get("id").and_then(Value::as_i64)!=Some(read.id) {return false;}
        if value.get("status").and_then(Value::as_i64)!=Some(0) {
            self.fail_settings_updates_read(cx,"Android could not read Updates information. Refresh to try again.");
        }
        true
    }
    pub(crate) fn settings_updates_observe(&mut self,cx:&mut Cx,value:&Value) {
        let Some(read)=self.settings_runtime.updates.read else {return;};
        if value.get("request_id").and_then(Value::as_i64)!=Some(read.id)
            || !self.settings_runtime.updates.accepts(read.id,read.owner)
            || self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)
            || !self.settings_is_foreground(read.owner)||!self.settings_updates_visible() {return;}
        let Some(snapshot)=UpdatesSnapshot::decode(value) else {
            self.fail_settings_updates_read(cx,"Android returned invalid Updates information. Refresh to try again.");return;
        };
        let updates=&mut self.settings_runtime.updates;updates.snapshot=Some(snapshot);updates.read=None;updates.fresh=true;updates.error.clear();updates.next_read=crate::host::now()+5.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_updates_permits(&self,owner:Owner,request:&UpdatesRequest)->bool {
        if !self.settings_is_foreground(owner)||!self.settings_updates_visible() {return false;}
        let updates=&self.settings_runtime.updates;
        if updates.active!=Some(owner)||!updates.fresh||!updates.snapshot.as_ref().is_some_and(|snapshot|snapshot.permits(request)) {return false;}
        true
    }
    pub(crate) fn settings_updates_resync(&mut self,cx:&mut Cx) {self.settings_runtime.updates.retire();self.refresh_settings_app(cx);}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn updates_observations_cannot_cross_request_or_root_lifetimes() {
        let owner=(1,WidgetUid(10));let replacement=(1,WidgetUid(11));
        let mut updates=SettingsUpdatesRuntime {active:Some(owner),read:Some(Read{id:4,owner,deadline:20.}),fresh:true,
            snapshot:UpdatesSnapshot::decode(&crate::settings_updates::tests::snapshot(4)),..Default::default()};
        assert!(updates.accepts(4,owner));assert!(!updates.accepts(3,owner));assert!(!updates.accepts(4,replacement));
        updates.retire();assert!(!updates.accepts(4,owner));
        let cached=updates.snapshot().unwrap();assert!(cached.capabilities.is_empty());
        assert_eq!(cached.rom_version.as_deref(),Some("build1"));
    }
}
