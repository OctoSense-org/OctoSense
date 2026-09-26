//! Wi-Fi reads and actions stay bound to the foreground trusted Settings root.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_wifi::{WifiRequest,WifiSnapshot}};
use makepad_strict_json::Value;
use makepad_widgets::*;

type Owner=(ClientId,WidgetUid);
#[derive(Clone,Copy)]
struct Read { id:i64, owner:Owner, deadline:f64 }
#[derive(Default)]
pub(crate) struct SettingsWifiRuntime {
    active:Option<Owner>, read:Option<Read>, next_read:f64, fresh:bool,
    snapshot:Option<WifiSnapshot>, pub(crate) error:String,
}
impl SettingsWifiRuntime {
    pub(crate) fn snapshot(&self)->Option<WifiSnapshot> {
        let mut snapshot=self.snapshot.clone()?;
        if !self.fresh {snapshot.clear_actions();}
        Some(snapshot)
    }
    fn retire(&mut self) {self.read=None;self.fresh=false;self.next_read=0.;}
    fn accepts(&self,id:i64,owner:Owner)->bool {self.active==Some(owner)&&self.read.is_some_and(|read|read.id==id&&read.owner==owner)}
}
impl App {
    fn settings_wifi_visible(&self)->bool {
        self.module_host.settings_instance().and_then(|instance|instance.root.borrow::<SettingsView>().map(|view|view.wifi_visible())).unwrap_or(false)
    }
    pub(crate) fn settings_wifi_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool {
        if !matches!(request,SettingsRequest::Wifi(WifiRequest::Snapshot)) {return false;}
        if cfg!(target_os="android")&&self.settings_is_foreground(owner)&&self.settings_wifi_visible() {
            self.start_settings_wifi_read(cx,owner,true);
        }
        true
    }
    fn start_settings_wifi_read(&mut self,cx:&mut Cx,owner:Owner,manual:bool) {
        if self.module_host.settings_client(owner.1)!=Some(owner.0) {return;}
        let id=self.android_command_id(cx,"launcher","wifi_snapshot",vec![]);
        let wifi=&mut self.settings_runtime.wifi;
        wifi.active=Some(owner);wifi.read=Some(Read{id,owner,deadline:crate::host::now()+20.});wifi.next_read=crate::host::now()+5.;
        if manual {wifi.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_wifi_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64) {
        let active=visible.filter(|_|self.settings_wifi_visible());
        if self.settings_runtime.wifi.active!=active {
            let wifi=&mut self.settings_runtime.wifi;wifi.retire();wifi.active=active;wifi.error.clear();self.refresh_settings_app(cx);
        }
        let Some(owner)=active else {return;};
        if self.settings_runtime.wifi.read.is_some_and(|read|now>=read.deadline) {
            self.fail_settings_wifi_read(cx,"Wi-Fi information did not arrive. Refresh to try again.");
        }
        if self.settings_runtime.wifi.read.is_none()&&now>=self.settings_runtime.wifi.next_read {
            self.start_settings_wifi_read(cx,owner,false);
        }
    }
    fn fail_settings_wifi_read(&mut self,cx:&mut Cx,message:&str) {
        let wifi=&mut self.settings_runtime.wifi;wifi.read=None;wifi.fresh=false;wifi.error=message.into();wifi.next_read=crate::host::now()+5.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_wifi_result(&mut self,cx:&mut Cx,value:&Value)->bool {
        let Some(read)=self.settings_runtime.wifi.read else {return false;};
        if value.get("id").and_then(Value::as_i64)!=Some(read.id) {return false;}
        if value.get("status").and_then(Value::as_i64)!=Some(0) {
            self.fail_settings_wifi_read(cx,"Android could not read Wi-Fi information. Refresh to try again.");
        }
        true
    }
    pub(crate) fn settings_wifi_observe(&mut self,cx:&mut Cx,value:&Value) {
        let Some(read)=self.settings_runtime.wifi.read else {return;};
        if value.get("request_id").and_then(Value::as_i64)!=Some(read.id)
            || !self.settings_runtime.wifi.accepts(read.id,read.owner)
            || self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)
            || !self.settings_is_foreground(read.owner)||!self.settings_wifi_visible() {return;}
        let Some(snapshot)=WifiSnapshot::decode(value) else {
            self.fail_settings_wifi_read(cx,"Android returned invalid Wi-Fi information. Refresh to try again.");return;
        };
        let wifi=&mut self.settings_runtime.wifi;wifi.snapshot=Some(snapshot);wifi.read=None;wifi.fresh=true;wifi.error.clear();wifi.next_read=crate::host::now()+5.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_wifi_permits(&self,owner:Owner,request:&WifiRequest)->bool {
        if !self.settings_is_foreground(owner)||!self.settings_wifi_visible() {return false;}
        let wifi=&self.settings_runtime.wifi;
        if wifi.active!=Some(owner)||!wifi.fresh||!wifi.snapshot.as_ref().is_some_and(|snapshot|snapshot.permits(request)) {return false;}
        if let WifiRequest::Network{target,..}=request {
            return self.module_host.settings_instance().and_then(|instance|instance.root.borrow::<SettingsView>()
                .map(|view|view.wifi_selected_target()==Some(target))).unwrap_or(false);
        }
        true
    }
    pub(crate) fn settings_wifi_resync(&mut self,cx:&mut Cx) {self.settings_runtime.wifi.retire();self.refresh_settings_app(cx);}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wifi_observations_cannot_cross_request_or_root_lifetimes() {
        let owner=(1,WidgetUid(10));let replacement=(1,WidgetUid(11));
        let mut wifi=SettingsWifiRuntime {active:Some(owner),read:Some(Read{id:4,owner,deadline:20.}),fresh:true,
            snapshot:WifiSnapshot::decode(&crate::settings_wifi::tests::snapshot(4,1)),..Default::default()};
        assert!(wifi.accepts(4,owner));assert!(!wifi.accepts(3,owner));assert!(!wifi.accepts(4,replacement));
        wifi.retire();assert!(!wifi.accepts(4,owner));
        let cached=wifi.snapshot().unwrap();assert!(cached.networks[0].actions.is_empty());assert!(cached.capabilities.is_empty());
        assert_eq!(cached.networks[0].name(),"Network 0");
    }
}
