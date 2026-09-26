//! Accessibility can operate the visible trusted Settings instance, never a
//! script-selected module or a raw Android settings command.
use crate::{App,settings_accessibility::Action,settings_app::SettingsView};
use makepad_strict_json::{obj,Value};
use makepad_widgets::{widget_async::{enter_isolate,leave_isolate},*};

const INACTIVE_LAYOUT:&str=r#"{"schema":1,"active":false,"token":"","pane":"","bounds":[0,0,0,0],"scroll":null,"nodes":[]}"#;

#[derive(Default)]
pub struct SettingsAccessibilityRuntime {
    enabled:bool,
    owner:Option<(crate::hub::ClientId,WidgetUid)>,
    packet:String,
    last_publish:f64,
    last_action:i64,
    after_draw:Vec<i64>,
}
impl SettingsAccessibilityRuntime {
    pub fn invalidate(&mut self){self.packet.clear();self.last_publish=0.;}
    fn retire_results(&mut self){self.after_draw.clear();}
    fn can_queue_result(&self)->bool{self.after_draw.len()<32}
    fn published_results(&mut self,is_draw:bool)->Vec<i64>{if is_draw{std::mem::take(&mut self.after_draw)}else{Vec::new()}}
    fn accepts_sequence(&mut self,action:&Action)->bool {
        if action.request_id<=self.last_action{return false;}
        self.last_action=action.request_id;true
    }
}
impl App {
    pub(crate) fn settings_accessibility_enabled(&mut self,cx:&mut Cx,enabled:Option<bool>) {
        let Some(enabled)=enabled else{return;};
        if self.settings_runtime.accessibility.enabled!=enabled {
            self.settings_runtime.accessibility.enabled=enabled;
            self.settings_runtime.accessibility.invalidate();
            if enabled {self.redraw_all(cx);}else{self.settings_accessibility_clear(cx);}
        }
    }
    fn settings_accessibility_owner(&self)->Option<(crate::hub::ClientId,WidgetUid)> {
        if !self.settings_runtime.accessibility.enabled{return None;}
        let instance=self.module_host.settings_instance()?;let owner=(instance.client,instance.root.widget_uid());
        let phone=&self.state.as_ref()?.phone;
        (self.settings_is_foreground(owner)&&phone.openness>=0.999&&phone.overview<=0.001&&phone.shade.open<=0.001
            &&!phone.navigation.open&&!phone.search_open&&!phone.groups.window_visible()).then_some(owner)
    }
    fn settings_accessibility_retire_owner(&mut self,cx:&mut Cx) {
        self.settings_runtime.accessibility.retire_results();
        if let Some((client,uid))=self.settings_runtime.accessibility.owner.take() {
            if let Some(instance)=self.module_host.get(client).filter(|instance|instance.root.widget_uid()==uid) {
                let(root,vm_id)=(instance.root.clone(),instance.vm_id);let entry=enter_isolate(cx,vm_id);
                if let Some(mut view)=root.borrow_mut::<SettingsView>(){view.accessibility_retire();}
                leave_isolate(cx,entry);
            }
        }
    }
    pub(crate) fn settings_accessibility_clear(&mut self,cx:&mut Cx) {
        self.settings_accessibility_retire_owner(cx);
        if self.settings_runtime.accessibility.packet!=INACTIVE_LAYOUT {
            cx.android_integration("settings.a11y.layout",INACTIVE_LAYOUT);self.settings_runtime.accessibility.packet=INACTIVE_LAYOUT.into();
        }
    }
    pub(crate) fn settings_accessibility_publish(&mut self,cx:&mut Cx,event:&Event) {
        if !cfg!(target_os="android"){return;}
        let Some(owner)=self.settings_accessibility_owner()else{self.settings_accessibility_clear(cx);return;};
        if self.settings_runtime.accessibility.owner!=Some(owner) {
            self.settings_accessibility_retire_owner(cx);self.settings_runtime.accessibility.owner=Some(owner);self.settings_runtime.accessibility.invalidate();
        }
        if !matches!(event,Event::Draw(_)|Event::Timer(_)){return;}
        let is_draw=matches!(event,Event::Draw(_));
        let awaiting_draw=is_draw&&!self.settings_runtime.accessibility.after_draw.is_empty();
        let now=crate::host::now();if !awaiting_draw&&now-self.settings_runtime.accessibility.last_publish<0.25{return;}
        self.settings_runtime.accessibility.last_publish=now;
        let Some(instance)=self.module_host.get(owner.0)else{return;};
        let(root,vm_id)=(instance.root.clone(),instance.vm_id);let entry=enter_isolate(cx,vm_id);
        let layout=root.borrow_mut::<SettingsView>().map(|mut view|{let dpi=view.accessibility_dpi(cx);view.accessibility_layout(cx,dpi)});
        leave_isolate(cx,entry);
        let Some(layout)=layout else{return;};
        // A new pane needs its first real draw before geometry can be exposed.
        if !layout.active(){return;}
        let packet=layout.json();if packet!=self.settings_runtime.accessibility.packet {
            cx.android_integration("settings.a11y.layout",&packet);self.settings_runtime.accessibility.packet=packet;
        }
        // Successful actions may change visible children before a Java service
        // receives the new tree. Publish that drawn state before acknowledging
        // the action, so a Search click cannot expose the old enabled catalog.
        // Timer observations never stand in for a completed draw.
        for request_id in self.settings_runtime.accessibility.published_results(is_draw){
            cx.android_integration("settings.a11y.result",&obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(request_id)),("accepted",Value::Bool(true))]).to_json());
        }
    }
    pub(crate) fn settings_accessibility_action(&mut self,cx:&mut Cx,value:&Value) {
        let request_id=value.get("request_id").and_then(Value::as_i64).filter(|id|*id>0);
        let accepted=if let (Some(action),Some(owner))=(Action::decode(value),self.settings_accessibility_owner()) {
            if self.settings_runtime.accessibility.owner!=Some(owner)||!self.settings_runtime.accessibility.can_queue_result() {false}
            else if !self.settings_runtime.accessibility.accepts_sequence(&action){false}
            else if let Some(instance)=self.module_host.get(owner.0).filter(|instance|instance.root.widget_uid()==owner.1) {
                let(root,vm_id)=(instance.root.clone(),instance.vm_id);let entry=enter_isolate(cx,vm_id);
                let accepted=root.borrow_mut::<SettingsView>().is_some_and(|mut view|{let dpi=view.accessibility_dpi(cx);view.accessibility_action(cx,dpi,&action)});
                leave_isolate(cx,entry);accepted
            }else{false}
        }else{false};
        if accepted{
            self.settings_runtime.accessibility.after_draw.push(request_id.unwrap());
            self.settings_runtime.accessibility.invalidate();self.redraw_all(cx);
        }else if let Some(request_id)=request_id{
            cx.android_integration("settings.a11y.result",&obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(request_id)),("accepted",Value::Bool(false))]).to_json());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn successful_accessibility_actions_wait_for_draw_and_retirement_never_replays_them(){
        let mut runtime=SettingsAccessibilityRuntime::default();
        // Search can hide old children immediately, while the next catalog is
        // still loading. Neither the mutation nor a timer acknowledges old UI.
        runtime.after_draw.push(10);runtime.invalidate();
        assert!(runtime.published_results(false).is_empty());assert_eq!(runtime.after_draw,vec![10]);
        assert_eq!(runtime.published_results(true),vec![10]);assert!(runtime.published_results(true).is_empty());
        for id in 20..52{assert!(runtime.can_queue_result());runtime.after_draw.push(id);}
        assert!(!runtime.can_queue_result(),"bound pending acknowledgments before dispatching another action");
        runtime.retire_results();assert!(runtime.can_queue_result());assert!(runtime.published_results(true).is_empty(),"background/closed roots must not acknowledge retired actions");
    }

    #[test]
    fn settings_accessibility_request_ids_cannot_replay_by_changing_or_retiring_page_tokens() {
        let mut runtime=SettingsAccessibilityRuntime::default();
        let mut action=Action{request_id:10,token:"page1".into(),id:1,target:"1".into(),kind:crate::settings_accessibility::ActionKind::Click,text:None};
        assert!(runtime.accepts_sequence(&action));assert!(!runtime.accepts_sequence(&action));
        action.token="forged-or-old-page".into();action.request_id=9;assert!(!runtime.accepts_sequence(&action));
        runtime.invalidate();action.token="page2".into();action.request_id=10;assert!(!runtime.accepts_sequence(&action));
        action.request_id=11;assert!(runtime.accepts_sequence(&action));
    }
}
