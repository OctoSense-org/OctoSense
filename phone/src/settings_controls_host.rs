//! Settings controls reads and actions stay bound to the foreground trusted Settings root.
use crate::{App,hub::ClientId,settings_app::{SettingsRequest,SettingsView},settings_controls::{ControlsPage,ControlsRequest,ControlsSnapshot}};
use makepad_strict_json::Value;
use makepad_widgets::*;

type Owner=(ClientId,WidgetUid);
type Active=(Owner,ControlsPage);
#[derive(Clone,Copy)]
struct Read { id:i64, owner:Owner, page:ControlsPage, deadline:f64 }
#[derive(Default)]
pub(crate) struct SettingsControlsRuntime {
    active:Option<Active>, read:Option<Read>, next_read:f64, fresh:bool,
    snapshot:Option<ControlsSnapshot>, pub(crate) error:String,
}
impl SettingsControlsRuntime {
    pub(crate) fn snapshot(&self)->Option<ControlsSnapshot> {
        let mut snapshot=self.snapshot.clone()?;
        if !self.fresh {snapshot.clear_actions();}
        Some(snapshot)
    }
    fn retire(&mut self) {self.read=None;self.fresh=false;self.next_read=0.;}
    fn accepts(&self,id:i64,owner:Owner,page:ControlsPage)->bool {self.active==Some((owner,page))&&self.read.is_some_and(|read|read.id==id&&read.owner==owner&&read.page==page)}
}
impl App {
    fn settings_controls_page(&self)->Option<ControlsPage> {
        self.module_host.settings_instance().and_then(|instance|instance.root.borrow::<SettingsView>().and_then(|view|view.controls_page())).filter(|page|*page!=ControlsPage::CaptionCustom)
    }
    pub(crate) fn settings_controls_read_request(&mut self,cx:&mut Cx,owner:Owner,request:&SettingsRequest)->bool {
        let SettingsRequest::Controls(ControlsRequest::Snapshot(page))=request else {return false;};
        if cfg!(target_os="android")&&self.settings_is_foreground(owner)&&self.settings_controls_page()==Some(*page) {
            self.start_settings_controls_read(cx,owner,*page,true);
        }
        true
    }
    fn start_settings_controls_read(&mut self,cx:&mut Cx,owner:Owner,page:ControlsPage,manual:bool) {
        if self.module_host.settings_client(owner.1)!=Some(owner.0) {return;}
        if self.settings_runtime.controls.read.is_some_and(|read|read.owner==owner&&read.page==page) {return;}
        if self.settings_runtime.controls.active!=Some((owner,page)) {self.settings_runtime.controls.retire();}
        let id=self.android_command_id(cx,"launcher","controls_snapshot",vec![("page",makepad_strict_json::s(page.wire()))]);
        let controls=&mut self.settings_runtime.controls;
        controls.active=Some((owner,page));controls.read=Some(Read{id,owner,page,deadline:crate::host::now()+20.});controls.next_read=crate::host::now()+5.;
        if manual {controls.error.clear();self.refresh_settings_app(cx);}
    }
    pub(crate) fn settings_controls_tick(&mut self,cx:&mut Cx,visible:Option<Owner>,now:f64) {
        let active=visible.and_then(|owner|self.settings_controls_page().map(|page|(owner,page)));
        if self.settings_runtime.controls.active!=active {
            let controls=&mut self.settings_runtime.controls;controls.retire();controls.active=active;controls.error.clear();self.refresh_settings_app(cx);
        }
        let Some((owner,page))=active else {return;};
        if self.settings_runtime.controls.read.is_some_and(|read|now>=read.deadline) {
            self.fail_settings_controls_read(cx,"Settings information did not arrive. Refresh to try again.");
        }
        if self.settings_runtime.controls.read.is_none()&&now>=self.settings_runtime.controls.next_read {
            self.start_settings_controls_read(cx,owner,page,false);
        }
    }
    fn fail_settings_controls_read(&mut self,cx:&mut Cx,message:&str) {
        let controls=&mut self.settings_runtime.controls;controls.read=None;controls.fresh=false;controls.error=message.into();controls.next_read=crate::host::now()+5.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_controls_result(&mut self,cx:&mut Cx,value:&Value)->bool {
        let Some(read)=self.settings_runtime.controls.read else {return false;};
        if value.get("id").and_then(Value::as_i64)!=Some(read.id) {return false;}
        if value.get("status").and_then(Value::as_i64)!=Some(0) {
            self.fail_settings_controls_read(cx,"Android could not read this settings page. Refresh to try again.");
        }
        true
    }
    pub(crate) fn settings_controls_observe(&mut self,cx:&mut Cx,value:&Value) {
        let Some(read)=self.settings_runtime.controls.read else {return;};
        if value.get("request_id").and_then(Value::as_i64)!=Some(read.id)
            || !self.settings_runtime.controls.accepts(read.id,read.owner,read.page)
            || self.module_host.settings_client(read.owner.1)!=Some(read.owner.0)
            || !self.settings_is_foreground(read.owner)||self.settings_controls_page()!=Some(read.page) {return;}
        let Some(snapshot)=ControlsSnapshot::decode(value).filter(|state|state.page==read.page) else {
            self.fail_settings_controls_read(cx,"Android returned invalid settings information. Refresh to try again.");return;
        };
        let controls=&mut self.settings_runtime.controls;controls.snapshot=Some(snapshot);controls.read=None;controls.fresh=true;controls.error.clear();controls.next_read=crate::host::now()+5.;
        self.refresh_settings_app(cx);
    }
    pub(crate) fn settings_controls_permits(&self,owner:Owner,request:&ControlsRequest)->bool {
        if !self.settings_is_foreground(owner)||self.settings_controls_page()!=Some(request.page()) {return false;}
        let controls=&self.settings_runtime.controls;
        controls.active==Some((owner,request.page()))&&controls.fresh&&controls.snapshot.as_ref().is_some_and(|snapshot|snapshot.permits(request))
    }
    pub(crate) fn settings_controls_resync(&mut self,cx:&mut Cx) {self.settings_runtime.controls.retire();self.refresh_settings_app(cx);}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controls_observations_cannot_cross_pages_requests_or_root_lifetimes() {
        let owner=(1,WidgetUid(10));let replacement=(1,WidgetUid(11));let page=ControlsPage::Privacy;
        let mut state=SettingsControlsRuntime {active:Some((owner,page)),read:Some(Read{id:4,owner,page,deadline:20.}),fresh:true,
            snapshot:ControlsSnapshot::decode(&crate::settings_controls::tests::snapshot(4,page)),..Default::default()};
        assert!(state.accepts(4,owner,page));assert!(!state.accepts(3,owner,page));
        assert!(!state.accepts(4,replacement,page));assert!(!state.accepts(4,owner,ControlsPage::BatteryPolicy));
        state.retire();assert!(!state.accepts(4,owner,page));
        let cached=state.snapshot().unwrap();assert!(cached.controls.iter().all(|row|row.options.is_empty()));
        assert_eq!(cached.page,page);assert!(cached.controls[0].value.is_some());
    }
}
