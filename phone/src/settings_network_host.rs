//! Network reads and actions stay bound to the foreground trusted Settings root.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_network::{NetworkRequest,NetworkSnapshot}};
use makepad_strict_json::Value;
use makepad_widgets::*;

type Owner=(ClientId,WidgetUid);
#[derive(Clone,Copy)]
struct Read { id:i64, owner:Owner, deadline:f64 }
#[derive(Default)]
pub(crate) struct SettingsNetworkRuntime {
    active:Option<Owner>, read:Option<Read>, next_read:f64, fresh:bool,
    snapshot:Option<NetworkSnapshot>, pub(crate) error:String,
}
impl SettingsNetworkRuntime {
    pub(crate) fn snapshot(&self)->Option<NetworkSnapshot> {
        let mut snapshot=self.snapshot.clone()?;
        if !self.fresh {snapshot.clear_actions();}
        Some(snapshot)
    }
    fn retire(&mut self) {self.read=None;self.fresh=false;self.next_read=0.;}
    fn accepts(&self,id:i64,owner:Owner)->bool {self.active==Some(owner)&&self.read.is_some_and(|read|read.id==id&&read.owner==owner)}
}
impl App {
    fn settings_network_visible(&self)->bool {
        self.module_host.settings_instance().and_then(|instance|instance.root.borrow::<SettingsView>().map(|view|view.network_visible())).unwrap_or(false)
    }
    pub(crate) fn settings_network_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool {
        if !matches!(request,SettingsRequest::Network(NetworkRequest::Snapshot)) {return false;}
        if cfg!(target_os="android")&&self.settings_is_foreground(owner)&&self.settings_network_visible() {
            self.start_settings_network_read(cx,owner,true);
        }
        true
    }
    fn start_settings_network_read(&mut self,cx:&mut Cx,owner:Owner,manual:bool) {
        if self.module_host.settings_client(owner.1)!=Some(owner.0) {return;}
        if self.settings_runtime.network.read.is_some_and(|read|read.owner==owner) {return;}
        if self.settings_runtime.network.active!=Some(owner) {self.settings_runtime.network.retire();}
        let id=self.android_command_id(cx,"launcher","network_snapshot",vec![]);
        let network=&mut self.settings_runtime.network;
        network.active=Some(owner);network.read=Some(Read{id,owner,deadline:crate::host::now()+20.});network.next_read=crate::host::now()+5.;
        if manual {network.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_network_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64) {
        let active=visible.filter(|_|self.settings_network_visible());
        if self.settings_runtime.network.active!=active {
            let network=&mut self.settings_runtime.network;network.retire();network.active=active;network.error.clear();self.refresh_settings_app(cx);
        }
        let Some(owner)=active else {return;};
        if self.settings_runtime.network.read.is_some_and(|read|now>=read.deadline) {
            self.fail_settings_network_read(cx,"Network information did not arrive. Refresh to try again.");
        }
        if self.settings_runtime.network.read.is_none()&&now>=self.settings_runtime.network.next_read {
            self.start_settings_network_read(cx,owner,false);
        }
    }
    fn fail_settings_network_read(&mut self,cx:&mut Cx,message:&str) {
        let network=&mut self.settings_runtime.network;network.read=None;network.fresh=false;network.error=message.into();network.next_read=crate::host::now()+5.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_network_result(&mut self,cx:&mut Cx,value:&Value)->bool {
        let Some(read)=self.settings_runtime.network.read else {return false;};
        if value.get("id").and_then(Value::as_i64)!=Some(read.id) {return false;}
        if value.get("status").and_then(Value::as_i64)!=Some(0) {
            self.fail_settings_network_read(cx,"Android could not read Network information. Refresh to try again.");
        }
        true
    }
    pub(crate) fn settings_network_observe(&mut self,cx:&mut Cx,value:&Value) {
        let Some(read)=self.settings_runtime.network.read else {return;};
        if value.get("request_id").and_then(Value::as_i64)!=Some(read.id)
            || !self.settings_runtime.network.accepts(read.id,read.owner)
            || self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)
            || !self.settings_is_foreground(read.owner)||!self.settings_network_visible() {return;}
        let Some(snapshot)=NetworkSnapshot::decode(value) else {
            self.fail_settings_network_read(cx,"Android returned invalid Network information. Refresh to try again.");return;
        };
        let network=&mut self.settings_runtime.network;network.snapshot=Some(snapshot);network.read=None;network.fresh=true;network.error.clear();network.next_read=crate::host::now()+5.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_network_permits(&self,owner:Owner,request:&NetworkRequest)->bool {
        if !self.settings_is_foreground(owner)||!self.settings_network_visible() {return false;}
        let network=&self.settings_runtime.network;
        if network.active!=Some(owner)||!network.fresh||!network.snapshot.as_ref().is_some_and(|snapshot|snapshot.permits(request)) {return false;}
        true
    }
    pub(crate) fn settings_network_resync(&mut self,cx:&mut Cx) {self.settings_runtime.network.retire();self.refresh_settings_app(cx);}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_observations_cannot_cross_request_or_root_lifetimes() {
        let owner=(1,WidgetUid(10));let replacement=(1,WidgetUid(11));
        let mut network=SettingsNetworkRuntime {active:Some(owner),read:Some(Read{id:4,owner,deadline:20.}),fresh:true,
            snapshot:NetworkSnapshot::decode(&crate::settings_network::tests::snapshot(4)),..Default::default()};
        assert!(network.accepts(4,owner));assert!(!network.accepts(3,owner));assert!(!network.accepts(4,replacement));
        network.retire();assert!(!network.accepts(4,owner));
        let cached=network.snapshot().unwrap();assert!(!cached.airplane.can_set);assert!(!cached.data_saver.can_set);assert!(!cached.dns.can_set);
        assert_eq!(cached.airplane.enabled,Some(false));
    }
}
