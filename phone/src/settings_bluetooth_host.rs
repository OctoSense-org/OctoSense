//! Bluetooth reads and actions stay bound to the foreground trusted Settings root.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_bluetooth::{BluetoothRequest,BluetoothSnapshot}};
use makepad_strict_json::Value;
use makepad_widgets::*;

type Owner=(ClientId,WidgetUid);
#[derive(Clone,Copy)]
struct Read { id:i64, owner:Owner, deadline:f64 }
#[derive(Default)]
pub(crate) struct SettingsBluetoothRuntime {
    active:Option<Owner>, read:Option<Read>, next_read:f64, fresh:bool,
    snapshot:Option<BluetoothSnapshot>, pub(crate) error:String,
}
impl SettingsBluetoothRuntime {
    pub(crate) fn snapshot(&self)->Option<BluetoothSnapshot> {
        let mut snapshot=self.snapshot.clone()?;
        if !self.fresh {snapshot.clear_actions();}
        Some(snapshot)
    }
    fn retire(&mut self) {self.read=None;self.fresh=false;self.next_read=0.;}
    fn accepts(&self,id:i64,owner:Owner)->bool {self.active==Some(owner)&&self.read.is_some_and(|read|read.id==id&&read.owner==owner)}
}
impl App {
    fn settings_bluetooth_visible(&self)->bool {
        self.module_host.settings_instance().and_then(|instance|instance.root.borrow::<SettingsView>().map(|view|view.bluetooth_visible())).unwrap_or(false)
    }
    pub(crate) fn settings_bluetooth_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool {
        if !matches!(request,SettingsRequest::Bluetooth(BluetoothRequest::Snapshot)) {return false;}
        if cfg!(target_os="android")&&self.settings_is_foreground(owner)&&self.settings_bluetooth_visible() {
            self.start_settings_bluetooth_read(cx,owner,true);
        }
        true
    }
    fn start_settings_bluetooth_read(&mut self,cx:&mut Cx,owner:Owner,manual:bool) {
        if self.module_host.settings_client(owner.1)!=Some(owner.0) {return;}
        if self.settings_runtime.bluetooth.read.is_some_and(|read|read.owner==owner) {return;}
        if self.settings_runtime.bluetooth.active!=Some(owner) {self.settings_runtime.bluetooth.retire();}
        let id=self.android_command_id(cx,"launcher","bluetooth_snapshot",vec![]);
        let bluetooth=&mut self.settings_runtime.bluetooth;
        bluetooth.active=Some(owner);bluetooth.read=Some(Read{id,owner,deadline:crate::host::now()+20.});bluetooth.next_read=crate::host::now()+5.;
        if manual {bluetooth.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_bluetooth_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64) {
        let active=visible.filter(|_|self.settings_bluetooth_visible());
        if self.settings_runtime.bluetooth.active!=active {
            let bluetooth=&mut self.settings_runtime.bluetooth;bluetooth.retire();bluetooth.active=active;bluetooth.error.clear();self.refresh_settings_app(cx);
        }
        let Some(owner)=active else {return;};
        if self.settings_runtime.bluetooth.read.is_some_and(|read|now>=read.deadline) {
            self.fail_settings_bluetooth_read(cx,"Bluetooth information did not arrive. Refresh to try again.");
        }
        if self.settings_runtime.bluetooth.read.is_none()&&now>=self.settings_runtime.bluetooth.next_read {
            self.start_settings_bluetooth_read(cx,owner,false);
        }
    }
    fn fail_settings_bluetooth_read(&mut self,cx:&mut Cx,message:&str) {
        let bluetooth=&mut self.settings_runtime.bluetooth;bluetooth.read=None;bluetooth.fresh=false;bluetooth.error=message.into();bluetooth.next_read=crate::host::now()+5.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_bluetooth_result(&mut self,cx:&mut Cx,value:&Value)->bool {
        let Some(read)=self.settings_runtime.bluetooth.read else {return false;};
        if value.get("id").and_then(Value::as_i64)!=Some(read.id) {return false;}
        if value.get("status").and_then(Value::as_i64)!=Some(0) {
            self.fail_settings_bluetooth_read(cx,"Android could not read Bluetooth information. Refresh to try again.");
        }
        true
    }
    pub(crate) fn settings_bluetooth_observe(&mut self,cx:&mut Cx,value:&Value) {
        let Some(read)=self.settings_runtime.bluetooth.read else {return;};
        if value.get("request_id").and_then(Value::as_i64)!=Some(read.id)
            || !self.settings_runtime.bluetooth.accepts(read.id,read.owner)
            || self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)
            || !self.settings_is_foreground(read.owner)||!self.settings_bluetooth_visible() {return;}
        let Some(snapshot)=BluetoothSnapshot::decode(value) else {
            self.fail_settings_bluetooth_read(cx,"Android returned invalid Bluetooth information. Refresh to try again.");return;
        };
        let bluetooth=&mut self.settings_runtime.bluetooth;bluetooth.snapshot=Some(snapshot);bluetooth.read=None;bluetooth.fresh=true;bluetooth.error.clear();bluetooth.next_read=crate::host::now()+5.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_bluetooth_permits(&self,owner:Owner,request:&BluetoothRequest)->bool {
        if !self.settings_is_foreground(owner)||!self.settings_bluetooth_visible() {return false;}
        let bluetooth=&self.settings_runtime.bluetooth;
        if bluetooth.active!=Some(owner)||!bluetooth.fresh||!bluetooth.snapshot.as_ref().is_some_and(|snapshot|snapshot.permits(request)) {return false;}
        if let Some(target)=request.target() {
            return self.module_host.settings_instance().and_then(|instance|instance.root.borrow::<SettingsView>()
                .map(|view|view.bluetooth_selected_target()==Some(target))).unwrap_or(false);
        }
        true
    }
    pub(crate) fn settings_bluetooth_resync(&mut self,cx:&mut Cx) {self.settings_runtime.bluetooth.retire();self.refresh_settings_app(cx);}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bluetooth_observations_cannot_cross_request_or_root_lifetimes() {
        let owner=(1,WidgetUid(10));let replacement=(1,WidgetUid(11));
        let mut bluetooth=SettingsBluetoothRuntime {active:Some(owner),read:Some(Read{id:4,owner,deadline:20.}),fresh:true,
            snapshot:BluetoothSnapshot::decode(&crate::settings_bluetooth::tests::snapshot(4,1)),..Default::default()};
        assert!(bluetooth.accepts(4,owner));assert!(!bluetooth.accepts(3,owner));assert!(!bluetooth.accepts(4,replacement));
        bluetooth.retire();assert!(!bluetooth.accepts(4,owner));
        let cached=bluetooth.snapshot().unwrap();assert!(cached.devices[0].actions.is_empty());assert!(cached.capabilities.is_empty());
        assert_eq!(cached.devices[0].name(),"Device 0");
    }
}
