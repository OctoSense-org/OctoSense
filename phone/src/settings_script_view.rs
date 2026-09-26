// Mounted Makepad adapter. Application decisions are supplied by the bundled controller.
use crate::settings_script::{Controller,Patch};
use crate::settings_script_host_facade::HostFacade;
use std::collections::HashMap;
#[path="settings_script_accessibility.rs"] mod script_accessibility;
use script_accessibility::{AccessibilityUi,Metadata};

struct ButtonScrollPress {
    page:String,
    start:makepad_platform::event::TouchUpdateEvent,
    button:WidgetRef,
    scroll:WidgetRef,
}
#[derive(Clone,PartialEq,Eq)]
struct ButtonTarget {
    page:String,
    widget:WidgetUid,
    semantic:String,
}
#[derive(Script,Widget)]
pub struct SettingsView {
    #[deref] view:View,
    #[rust] controller:Option<Controller>,
    #[rust] script_facade:HostFacade,
    #[rust] state:SettingsSnapshot,
    #[rust] widgets:HashMap<String,WidgetRef>,
    #[rust] buttons:Vec<(String,ButtonRef)>,
    #[rust] inputs:Vec<(String,TextInputRef)>,
    #[rust] semantic:HashMap<String,String>,
    #[rust] labels:HashMap<String,String>,
    #[rust] disabled:HashSet<String>,
    #[rust] accessibility:AccessibilityUi,
    #[rust] applied_font_scale:f64,
    #[rust] button_scroll_press:Option<ButtonScrollPress>,
    // None is an invalidated press, retained until release so a later snapshot
    // cannot turn its Clicked event into a fresh action on a replacement row.
    #[rust] button_presses:HashMap<String,Option<ButtonTarget>>,
    #[rust] focus_touch:Option<(u64,String,String)>,
    #[rust] focus_mouse:Option<(String,String)>,
    #[rust] fault:Option<String>,
}
impl ScriptHook for SettingsView {
    fn on_after_apply(&mut self,vm:&mut ScriptVm,_apply:&Apply,_scope:&mut Scope,_value:ScriptValue){
        self.applied_font_scale=0.;
        vm.with_cx_mut(|cx|{
            self.widgets.clear();self.buttons.clear();self.inputs.clear();
            let tree=octoscript_makepad::design::prepare(include_str!("../resources/settings/settings.splash")).expect("bundled Settings layout");
            let mut nodes=vec![&tree];
            while let Some(node)=nodes.pop(){
                if let Some(id)=&node.attrs.id{
                    let mut widget=self.view.widget(cx,&[LiveId::from_str(id)]);
                    if widget.borrow::<Button>().is_some(){self.buttons.push((id.clone(),widget.as_button()));}
                    if widget.borrow::<TextInput>().is_some(){
                        script_apply_eval!(cx,widget,{autocorrect: mod.ime.AutoCorrect.Disabled autocapitalize: mod.ime.AutoCapitalize.None});
                        self.inputs.push((id.clone(),widget.as_text_input()));
                    }
                    self.widgets.insert(id.clone(),widget);
                }
                nodes.extend(node.children.iter().rev());
            }
            self.dispatch(cx,"theme","",Value::Null);
        });
    }
}
impl SettingsView {
    fn script_state(&self)->Option<&Value>{self.controller.as_ref().map(Controller::state)}
    fn script_text(&self,key:&str)->&str{self.script_state().and_then(|s|s.get(key)).and_then(Value::as_str).unwrap_or("")}
    fn page_token(&self)->String{self.script_state().and_then(|s|s.get("accessibility")).and_then(|s|s.get("key")).and_then(Value::as_str).unwrap_or("").into()}
    fn widget(&self,cx:&mut Cx,id:&str)->WidgetRef{self.widgets.get(id).cloned().unwrap_or_else(||self.view.widget(cx,&[LiveId::from_str(id)]))}
    fn active_scroll(&self,cx:&mut Cx)->WidgetRef{self.widget(cx,self.script_text("active_scroll"))}
    fn text(&self,cx:&mut Cx,id:&str,value:&str){self.widget(cx,id).set_text(cx,value);}
    fn pending(&self)->bool{self.script_state().and_then(|s|s.get("pending")).and_then(Value::as_bool).unwrap_or(false)}
    fn message(&self)->String{self.script_text("message").into()}
    fn fail(&mut self,cx:&mut Cx,message:String){self.fault=Some(message.clone());self.text(cx,"status",&message);self.view.redraw(cx);}
    fn dispatch(&mut self,cx:&mut Cx,kind:&str,id:&str,value:Value)->bool{
        use makepad_strict_json::{obj,s};
        if matches!(kind,"pause"|"entry"|"navigate"|"back"|"cancel_gesture") {
            for target in self.button_presses.values_mut(){*target=None;}
        }
        if self.controller.is_none(){match Controller::new(&crate::settings_script::bundled_source(),crate::settings_script::bundled_widgets()){
            Ok(controller)=>self.controller=Some(controller),Err(error)=>{self.fail(cx,error);return false}
        }}
        let mut observed=match crate::settings_script_bridge::observation(&self.state,self.pending()){Ok(value)=>value,Err(error)=>{self.fail(cx,error);return false}};
        if let Value::Obj(fields)=&mut observed{
            let position=self.active_scroll(cx).as_view().scroll_pos();
            fields.push(("scroll_y".into(),Value::F64(position.y.max(0.))));
        }
        let event=obj(vec![("kind",s(kind)),("id",s(id)),("value",value)]);
        let widgets=&self.widgets;let native=&self.state;
        let result=self.controller.as_mut().unwrap().step(&event,&observed,|frame|{
            let facade=HostFacade::decode(&frame.state,native)?;
            let _=Metadata::decode(&frame.state)?;
            let links=frame.state.get("focus_links").and_then(Value::as_arr).filter(|links|links.len()<=16).ok_or("Invalid script focus links")?;
            for link in links {
                let Value::Obj(fields)=link else{return Err("Invalid script focus link".into())};
                if fields.len()!=2 || !link.get("button").and_then(Value::as_str).and_then(|id|widgets.get(id)).is_some_and(|w|w.borrow::<Button>().is_some())
                    || !link.get("input").and_then(Value::as_str).and_then(|id|widgets.get(id)).is_some_and(|w|w.borrow::<TextInput>().is_some()) {
                    return Err("Invalid script focus link target".into());
                }
            }
            for patch in &frame.patches{
                let valid=match patch{
                    Patch::Input(id,_)|Patch::Focus(id)=>widgets.get(id).is_some_and(|w|w.borrow::<TextInput>().is_some()),
                    Patch::Scroll(id,..)=>widgets.get(id).is_some_and(|w|w.borrow::<View>().is_some()),
                    Patch::Text(id,_)=>widgets.get(id).is_some_and(|w|w.borrow::<TextInput>().is_none()),
                    _=>true,
                };
                if !valid{return Err("Settings controller returned an incompatible widget patch".into())}
            }
            let requests=frame.requests.iter().map(|r|{
                if r.get("kind").and_then(Value::as_str)==Some("read"){crate::settings_script_host_facade::subscription(r,native)}
                else{crate::settings_script_bridge::basic_request(r,native)}
                .filter(SettingsRequest::valid).ok_or_else(||"Settings controller requested an unavailable native operation".to_owned())
            }).collect::<Result<Vec<_>,_>>()?;
            Ok((facade,requests))
        });
        let (frame,(facade,requests))=match result{Ok(result)=>result,Err(error)=>{self.fail(cx,error);return false}};
        self.fault=None;self.script_facade=facade;
        // Each frame declares the current semantic context; retired rows cannot
        // inherit an old target when their mounted widget is reused.
        self.semantic.clear();self.labels.clear();
        for patch in frame.patches{
            match patch{
                Patch::Text(id,text)=>{let widget=self.widget(cx,&id);if widget.text()!=text{widget.set_text(cx,&text);}},
                Patch::Input(id,text)=>{let widget=self.widget(cx,&id);if widget.text()!=text{widget.set_text(cx,&text);}},
                Patch::Enabled(id,enabled)=>{if enabled{self.disabled.remove(&id);}else{self.disabled.insert(id.clone());}self.widget(cx,&id).set_disabled(cx,!enabled);},
                Patch::Visible(id,visible)=>self.widget(cx,&id).set_visible(cx,visible),
                Patch::Scroll(id,x,y)=>self.widget(cx,&id).as_view().set_scroll_pos(cx,dvec2(x,y)),
                Patch::Focus(id)=>self.widget(cx,&id).as_text_input().set_key_focus(cx),
                Patch::Blur=>{cx.set_key_focus(Area::Empty);cx.hide_text_ime();},
                Patch::Semantic(id,value)=>{self.semantic.insert(id,value);},
                Patch::Label(id,value)=>{self.labels.insert(id,value);},
                Patch::Color(id,foreground,[r,g,b,a])=>{let mut widget=self.widget(cx,&id);let color=vec4(r,g,b,a);if foreground{script_apply_eval!(cx,widget,{draw_text +: {color:#(color)}});}else{script_apply_eval!(cx,widget,{draw_bg +: {color:#(color)}});}},
            }
        }
        self.retire_changed_button_presses(cx);
        self.apply_font_scale(cx);self.view.redraw(cx);
        for request in requests{cx.widget_action(self.widget_uid(),request);}
        frame.handled
    }
    pub fn observe(&mut self,cx:&mut Cx,state:SettingsSnapshot){self.state=state;self.dispatch(cx,"observe","",Value::Null);}
    pub fn outcome(&mut self,cx:&mut Cx,pending:bool,message:&str){use makepad_strict_json::{obj,s};self.dispatch(cx,"result","",obj(vec![("pending",Value::Bool(pending)),("message",s(message))]));}
    fn domain_result(&mut self,cx:&mut Cx,domain:&str,phase:&str,applied:bool){use makepad_strict_json::{obj,s};self.dispatch(cx,"result","",obj(vec![("domain",s(domain)),("phase",s(phase)),("applied",Value::Bool(applied)),("pending",Value::Bool(self.pending())),("message",s(self.message()))]));}
    pub(crate) fn app_language_after_selection(&mut self,cx:&mut Cx){self.domain_result(cx,"app_language","submitted",false);}
    pub(crate) fn caption_language_after_selection(&mut self,cx:&mut Cx){self.domain_result(cx,"caption_language","submitted",false);}
    pub(crate) fn system_language_after_selection(&mut self,cx:&mut Cx){self.domain_result(cx,"system_languages","submitted",false);}
    pub(crate) fn keyboards_after_flow(&mut self,cx:&mut Cx){self.domain_result(cx,"keyboards","submitted",false);}
    pub(crate) fn dnd_operation_result(&mut self,cx:&mut Cx,applied:bool){self.domain_result(cx,"dnd","completed",applied);}
    pub(crate) fn renew_caption_visit(&mut self,cx:&mut Cx){self.dispatch(cx,"caption_renew","",Value::Null);}
    pub(crate) fn navigate_entry(&mut self,cx:&mut Cx,route:crate::settings_entry::EntryRoute){self.accessibility_retire();self.dispatch(cx,"entry",route.wire(),Value::Null);}
    pub(crate) fn navigate_notification_entry(&mut self,cx:&mut Cx,id:i64,package:&str){use makepad_strict_json::{obj,s};self.accessibility_retire();self.dispatch(cx,"entry","app_notifications",obj(vec![("entry_id",s(id.to_string())),("package",s(package))]));}
    pub(crate) fn resolve_notification_entry(&mut self,cx:&mut Cx,id:i64,details:&AppDetails)->bool{
        use makepad_strict_json::{obj,s};
        // The owner/lookup correlation was checked by the native host. Expose
        // the result as data; only the script may adopt it as selected context.
        self.state.app_details=Some(details.clone());
        self.dispatch(cx,"entry_resolved","app_notifications",obj(vec![("entry_id",s(id.to_string())),("package",s(details.target.package()))]))
    }
    fn begin_button_scroll(&mut self, cx: &mut Cx, event: &Event) {
        use makepad_platform::event::TouchState;
        let Event::TouchUpdate(update) = event else { return; };
        if update.touches.len() != 1 { self.button_scroll_press = None; return; }
        let touch = &update.touches[0];
        if touch.state != TouchState::Start { return; }
        self.button_scroll_press = None;
        let Some(captured) = cx.fingers.touch_capture_area(touch.uid) else { return; };
        let scroll = self.active_scroll(cx);
        if !scroll.area().clipped_rect(cx).contains(touch.abs) { return; }
        fn find_button(widget: &WidgetRef, area: Area) -> Option<WidgetRef> {
            if widget.area() == area && widget.borrow::<Button>().is_some() { return Some(widget.clone()); }
            let mut found = None;
            widget.children(&mut |_, child| { if found.is_none() { found = find_button(&child, area); } });
            found
        }
        if let Some(button) = find_button(&scroll, captured) {
            self.button_scroll_press = Some(ButtonScrollPress { page: self.page_token(), start: update.clone(), button, scroll });
        }
    }
    fn handoff_button_scroll(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        use makepad_platform::event::TouchState;
        if matches!(event, Event::Pause | Event::BackPressed { .. }) { self.button_scroll_press = None; }
        let Event::TouchUpdate(update) = event else { return; };
        let Some(press) = self.button_scroll_press.take() else { return; };
        if update.touches.len() != 1 || self.page_token() != press.page { return; }
        let touch = &update.touches[0];
        if touch.uid != press.start.touches[0].uid || matches!(touch.state, TouchState::Start | TouchState::Stop) { return; }
        let delta = touch.abs - press.start.touches[0].abs;
        if delta.x.abs().max(delta.y.abs()) <= 8.0 {
            self.button_scroll_press = Some(press);
            return;
        }
        // Only a deliberate vertical gesture takes over. Text selection and
        // Back are never candidates, and taps keep the native Button path.
        if delta.y.abs() <= delta.x.abs() { return; }
        let from = press.button.area();
        let to = press.scroll.area();
        if !press.button.visible() || !to.is_valid(cx)
            || cx.fingers.touch_capture_area(touch.uid) != Some(from)
            || !cx.switch_finger_capture(from, to, Area::Empty) { return; }
        // The scrollbar needs the original down sample for its native drag and
        // fling. The transferred capture + handled area prevent another child
        // from treating this initialization as a new press.
        let start = press.start;
        start.touches[0].handled.set(to);
        press.scroll.handle_event(cx, &Event::TouchUpdate(start), scope);
        press.button.handle_event(cx, &Event::ClearHover, scope);
        cx.widget_action(press.button.widget_uid(), ButtonAction::Released(update.modifiers));
        self.dispatch(cx,"cancel_gesture","",Value::Null);
    }
    fn apply_font_scale(&mut self, cx: &mut Cx) {
        let scale = self.state.device.as_ref().and_then(|d| d.font_scale)
            .unwrap_or(self.state.font_scale);
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        if (self.applied_font_scale - scale).abs() < 0.0001 { return; }
        self.applied_font_scale = scale;
        fn apply(cx: &mut Cx, mut widget: WidgetRef, scale: f64) {
            if let Some(mut label) = widget.borrow_mut::<Label>() {
                label.draw_text.font_scale = scale as f32;
            }
            let is_text_control = widget.borrow::<Button>().is_some() || widget.borrow::<TextInput>().is_some();
            if is_text_control {
                // Apply only the text scale, preserving the button, its state,
                // its semantic theme and its ID. Fit height allows wrapped text
                // at large system font sizes without shrinking touch targets.
                script_apply_eval!(cx, widget, {draw_text.font_scale: #(scale)});
            }
            let mut children = Vec::new();
            widget.children(&mut |_, child| children.push(child));
            for child in children { apply(cx, child, scale); }
        }
        let mut children = Vec::new();
        self.view.children(&mut |_, child| children.push(child));
        for child in children { apply(cx, child, scale); }
    }
    fn focus_links(&self)->Vec<(String,String)>{
        self.script_state().and_then(|s|s.get("focus_links")).and_then(Value::as_arr).map(|links|links.iter().filter_map(|v|Some((v.get("button")?.as_str()?.into(),v.get("input")?.as_str()?.into()))).collect()).unwrap_or_default()
    }
    fn button_target(&self,cx:&mut Cx,id:&str)->Option<ButtonTarget>{
        if self.fault.is_some(){return None}
        let target=self.widgets.get(id)?.widget_uid();
        let semantics:HashMap<LiveId,&str>=self.semantic.iter().map(|(id,value)|(LiveId::from_str(id),value.as_str())).collect();
        let disabled:HashSet<LiveId>=self.disabled.iter().map(|id|LiveId::from_str(id)).collect();
        fn find(cx:&mut Cx,widget:&WidgetRef,id:LiveId,target:WidgetUid,context:&str,semantics:&HashMap<LiveId,&str>,disabled:&HashSet<LiveId>)->Option<String>{
            if !widget.visible() || disabled.contains(&id) || widget.disabled(cx){return None}
            let context=semantics.get(&id).copied().unwrap_or(context);
            if widget.widget_uid()==target{return Some(context.into())}
            let mut found=None;
            widget.children(&mut |id,child|{if found.is_none(){found=find(cx,&child,id,target,context,semantics,disabled)}});
            found
        }
        let mut context=None;
        self.view.children(&mut |id,child|{if context.is_none(){context=find(cx,&child,id,target,"",&semantics,&disabled)}});
        Some(ButtonTarget{page:self.page_token(),widget:target,semantic:context?})
    }
    fn retire_changed_button_presses(&mut self,cx:&mut Cx){
        let ids:Vec<_>=self.button_presses.keys().cloned().collect();
        for id in ids {
            let current=self.button_target(cx,&id);
            if self.button_presses.get(&id)!=Some(&current){self.button_presses.insert(id,None);}
        }
    }
    fn focus_link_event(&mut self,cx:&mut Cx,event:&Event,scope:&mut Scope)->bool{
        use makepad_platform::event::TouchState;
        let links=self.focus_links();
        let selected=links.iter().find(|(button,input)|{
            let input=self.widget(cx,input);let button=self.widget(cx,button);if !cx.has_key_focus(input.area())||!button.visible(){return false}
            let rect=button.area().clipped_rect(cx);
            match event{Event::TouchUpdate(u)=>u.touches.iter().any(|t|t.state==TouchState::Start&&rect.contains(t.abs)),Event::MouseDown(e)=>rect.contains(e.abs),_=>false}
        }).cloned();
        let route=match event{
            Event::TouchUpdate(u)=>{
                if let Some((button,input))=selected{let rect=self.widget(cx,&button).area().clipped_rect(cx);if let Some(t)=u.touches.iter().find(|t|t.state==TouchState::Start&&rect.contains(t.abs)){self.focus_touch=Some((t.uid,button,input));}}
                let route=self.focus_touch.as_ref().filter(|(uid,b,i)|links.contains(&(b.clone(),i.clone()))&&u.touches.iter().any(|t|t.uid==*uid)).map(|(_,b,i)|(b.clone(),i.clone()));
                if self.focus_touch.as_ref().is_some_and(|(uid,_,_)|u.touches.iter().any(|t|t.uid==*uid&&t.state==TouchState::Stop)){self.focus_touch=None;}route
            },
            Event::MouseDown(_)=>{self.focus_mouse=selected;self.focus_mouse.clone()},
            Event::MouseMove(_)=>self.focus_mouse.clone(),Event::MouseUp(_)=>self.focus_mouse.take(),_=>None,
        };
        let Some((button,input))=route else{return false};
        if !links.contains(&(button.clone(),input.clone())){return false}
        self.widget(cx,&button).handle_event(cx,event,scope);self.widget(cx,&input).as_text_input().set_key_focus(cx);true
    }
    fn handle_settings_event(&mut self,cx:&mut Cx,event:&Event,scope:&mut Scope){
        if matches!(event,Event::VirtualKeyboard(VirtualKeyboardEvent::DidHide{..}))&&self.inputs.iter().any(|(_,input)|cx.has_key_focus(input.area())){cx.text_ime_was_dismissed();}
        if matches!(event,Event::Pause){self.focus_touch=None;self.focus_mouse=None;self.dispatch(cx,"pause","",Value::Null);}
        if self.focus_link_event(cx,event,scope){return}
        self.handoff_button_scroll(cx,event,scope);
        if let Event::BackPressed{handled}=event{if self.dispatch(cx,"back","",Value::Null){handled.set(true)}return}
        self.view.handle_event(cx,event,scope);self.begin_button_scroll(cx,event);
        let Event::Actions(actions)=event else{return};
        // Canonical TextInput editing owns composition/caret. Deliver the final
        // edit before Return when Android batches both in a single action event.
        for(id,input)in self.inputs.clone(){
            if input.changed(actions).is_some(){self.dispatch(cx,"input",&id,makepad_strict_json::s(input.text()));}
            if input.returned(actions).is_some(){self.dispatch(cx,"return",&id,makepad_strict_json::s(input.text()));}
        }
        for(id,button)in self.buttons.clone(){
            if button.pressed(actions){
                let target=self.button_target(cx,&id);
                self.button_presses.insert(id.clone(),target.clone());
                if target.is_some(){self.dispatch(cx,"press",&id,Value::Null);}
            }
            if button.clicked(actions){
                let current=self.button_target(cx,&id);
                let accepted=match self.button_presses.remove(&id){Some(held)=>held.is_some()&&held==current,None=>current.is_some()};
                if accepted{self.dispatch(cx,"click",&id,Value::Null);}else{self.dispatch(cx,"release",&id,Value::Null);}
            }
            else if button.released(actions){self.button_presses.remove(&id);self.dispatch(cx,"release",&id,Value::Null);}
        }
    }
    fn accessibility_metadata(&self)->Option<Metadata>{Metadata::decode(self.script_state()?).ok()}
    pub(crate) fn accessibility_dpi(&self,cx:&mut Cx)->f64{cx.get_dpi_factor_of(&self.view.area())}
    pub(crate) fn accessibility_retire(&mut self){self.accessibility.retire();}
    pub(crate) fn accessibility_layout(&mut self,cx:&mut Cx,dpi:f64)->crate::settings_accessibility::Layout{
        let Some(metadata)=self.accessibility_metadata()else{return Default::default()};let root=self.widget_uid();
        self.accessibility.layout(cx,&self.view,root,&metadata,&self.semantic,&self.labels,&self.disabled,dpi)
    }
    pub(crate) fn accessibility_action(&mut self,cx:&mut Cx,dpi:f64,action:&crate::settings_accessibility::Action)->bool{
        let Some(metadata)=self.accessibility_metadata()else{return false};let root=self.widget_uid();
        let Some(prepared)=self.accessibility.prepare_action(cx,&self.view,root,&metadata,&self.semantic,&self.labels,&self.disabled,dpi,action)else{return false};
        let handled=prepared.dispatch(cx,|cx,event|self.handle_event(cx,&event,&mut Scope::empty()));self.view.redraw(cx);handled
    }
    pub(crate) fn apps_read(&self)->Option<SettingsRequest>{self.script_facade.apps_read()}
    pub(crate) fn wifi_visible(&self)->bool{self.script_facade.wifi_visible()}
    pub(crate) fn bluetooth_visible(&self)->bool{self.script_facade.bluetooth_visible()}
    pub(crate) fn display_visible(&self)->bool{self.script_facade.display_visible()}
    pub(crate) fn network_visible(&self)->bool{self.script_facade.network_visible()}
    pub(crate) fn updates_visible(&self)->bool{self.script_facade.updates_visible()}
    pub(crate) fn date_time_visible(&self)->bool{self.script_facade.date_time_visible()}
    pub(crate) fn notification_history_visible(&self)->bool{self.script_facade.notification_history_visible()}
    pub(crate) fn wifi_selected_target(&self)->Option<&WifiTarget>{self.script_facade.wifi_selected_target()}
    pub(crate) fn bluetooth_selected_target(&self)->Option<&BluetoothTarget>{self.script_facade.bluetooth_selected_target()}
    pub(crate) fn caption_visit(&self)->Option<i64>{self.script_facade.caption_visit()}
    pub(crate) fn accounts_read(&self)->Option<AccountsRead>{self.script_facade.accounts_read()}
    pub(crate) fn controls_page(&self)->Option<ControlsPage>{self.script_facade.controls_page()}
    pub(crate) fn roles_read(&self)->Option<RolesRead>{self.script_facade.roles_read()}
    pub(crate) fn permissions_read(&self)->Option<PermissionsRead>{self.script_facade.permissions_read()}
    pub(crate) fn app_language_read(&self)->Option<LanguageRead>{self.script_facade.app_language_read()}
    pub(crate) fn caption_language_read(&self)->Option<CaptionLanguageRead>{self.script_facade.caption_language_read()}
    pub(crate) fn system_language_read(&self)->Option<SystemLanguageRead>{self.script_facade.system_language_read()}
    pub(crate) fn keyboards_read(&self)->Option<KeyboardRead>{self.script_facade.keyboards_read()}
    pub(crate) fn app_notifications_read(&self)->Option<AppNotificationsRead>{self.script_facade.app_notifications_read()}
    pub(crate) fn app_battery_read(&self)->Option<AppTarget>{self.script_facade.app_battery_read()}
    pub(crate) fn app_storage_read(&self)->Option<AppTarget>{self.script_facade.app_storage_read()}
    pub(crate) fn app_network_read(&self)->Option<AppNetworkRead>{self.script_facade.app_network_read()}
    pub(crate) fn dnd_read(&self)->Option<DndRead>{self.script_facade.dnd_read()}
    pub(crate) fn sounds_read(&self)->Option<SoundsRead>{self.script_facade.sounds_read()}
    pub(crate) fn history_read(&self)->Option<HistoryRead>{self.script_facade.history_read()}
    pub fn accounts_context_permits(&self,request:&AccountsRequest)->bool{self.script_facade.accounts_context_permits(request)}
}
impl Widget for SettingsView {
    fn draw_walk(&mut self,cx:&mut Cx2d,scope:&mut Scope,walk:Walk)->DrawStep{self.view.draw_walk(cx,scope,walk)}
    fn handle_event(&mut self,cx:&mut Cx,event:&Event,scope:&mut Scope){self.handle_settings_event(cx,event,scope);if self.accessibility.tracking(){if let Some(metadata)=self.accessibility_metadata(){let root=self.widget_uid();self.accessibility.sync_page(root,&metadata);}}}
}
