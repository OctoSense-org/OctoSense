//! Mounted-view regressions: real pure-controller decisions through native widgets.
use super::*;
use crate::{desktop::DesktopStyle,module_host::ModuleHost};
use crate::settings_accessibility::{Action,ActionKind,Bounds,Layout as A11yLayout,Node,Role};
use makepad_strict_json::s;
use crate::mobile_theme::Preset;

fn setup()->(Cx,ModuleHost){
    let mut cx=Cx::new(Box::new(|_,_|{}));cx.with_vm(makepad_widgets::script_mod);
    let mut host=ModuleHost::default();host.apply_style(&mut cx,&Selection::default().sheet(DesktopStyle::Android,false));
    host.create(&mut cx,1,&SETTINGS_MODULE,SETTINGS_MODULE.open_schema().empty_open().unwrap(),dvec2(400.,700.)).unwrap();
    (cx,host)
}
fn send(cx:&mut Cx,root:&WidgetRef,kind:&str,id:&str,value:Value){
    let mut view=root.borrow_mut::<SettingsView>().unwrap();view.dispatch(cx,kind,id,value);
    assert!(view.fault.is_none(),"{kind} {id}: {:?}",view.fault);
}
fn page(root:&WidgetRef)->String{root.borrow::<SettingsView>().unwrap().script_state().unwrap().get("page").unwrap().as_str().unwrap().into()}
fn state(root:&WidgetRef,key:&str)->Value{root.borrow::<SettingsView>().unwrap().script_state().unwrap().get(key).unwrap().clone()}
fn paint(cx:&mut Cx,root:&WidgetRef,size:DVec2){
    root.redraw(cx);let pass=DrawPass::new(cx);pass.set_size(cx,size);let mut list=DrawList::new(cx);let event=DrawEvent::default();let mut draw=CxDraw::new(cx,&event);
    draw.begin_pass(&pass,Some(1.));list.begin_always(&mut draw);let mut cx2d=Cx2d::new(&mut draw);cx2d.begin_root_turtle(size,Layout::flow_down());
    root.draw_walk_all(&mut cx2d,&mut Scope::empty(),Walk{width:Size::Fixed(size.x),height:Size::Fixed(size.y),..Default::default()});
    cx2d.end_pass_sized_turtle();drop(cx2d);list.end(&mut draw);draw.end_pass(&pass);
}
fn layout(cx:&mut Cx,root:&WidgetRef)->A11yLayout{root.borrow_mut::<SettingsView>().unwrap().accessibility_layout(cx,2.625)}
fn action(layout:&A11yLayout,node:&Node,kind:ActionKind)->Action{Action{request_id:1,token:layout.token.clone(),id:node.id,target:node.target(),kind,text:None}}
fn apply(cx:&mut Cx,root:&WidgetRef,action:&Action)->bool{root.borrow_mut::<SettingsView>().unwrap().accessibility_action(cx,2.625,action)}
fn contains(parent:Bounds,child:Bounds)->bool{let[x,y,w,h]=parent.0;let[a,b,c,d]=child.0;a>=x&&b>=y&&a+c<=x+w&&b+d<=y+h}

struct Impostor;
static IMPOSTOR: Impostor = Impostor;
impl AppModule for Impostor {
    fn id(&self) -> &'static str { "settings" }
    fn label(&self) -> &'static str { "Pretends to be Settings" }
    fn capabilities(&self) -> &'static [&'static str] { &["settings"] }
    fn register(&self, vm: &mut ScriptVm) { SETTINGS_MODULE.register(vm); }
    fn open_schema(&self) -> OpenSchema { OpenSchema::new(1) }
    fn create(&self, vm: &mut ScriptVm, open: ValidatedOpen, handles: InstanceHandles) -> InstanceParts {
        SETTINGS_MODULE.create(vm, open, handles)
    }
}

#[test]
fn capability_does_not_follow_claimed_module_id_metadata_or_retired_roots() {
    let (mut cx, mut host) = setup();
    host.create(&mut cx, 2, &IMPOSTOR, IMPOSTOR.open_schema().empty_open().unwrap(), dvec2(400., 700.)).unwrap();
    let authorized = host.get(1).unwrap().root.widget_uid();
    let forged = host.get(2).unwrap().root.widget_uid();
    assert_eq!(host.settings_client(authorized), Some(1));
    assert_eq!(host.settings_client(forged), None);
    let child = host.get(1).unwrap().root.widget(&mut cx, ids!(apply_theme)).widget_uid();
    assert_eq!(host.settings_client(child), None, "only the trusted root may dispatch");
    assert!(host.teardown(&mut cx, 1));
    assert_eq!(host.settings_client(authorized), None);
    host.create(&mut cx, 1, &SETTINGS_MODULE, SETTINGS_MODULE.open_schema().empty_open().unwrap(), dvec2(400., 700.)).unwrap();
    assert_ne!(host.get(1).unwrap().root.widget_uid(), authorized);
    assert_eq!(host.settings_client(authorized), None, "a queued action from a closed instance stays rejected");
}

#[test]
fn script_view_every_public_route_mounts_with_missing_service_observations(){
    let(mut cx,host)=setup();let instance=host.get(1).unwrap();let root=instance.root.clone();let isolate=makepad_widgets::widget_async::enter_isolate(&mut cx,instance.vm_id);
    root.borrow_mut::<SettingsView>().unwrap().observe(&mut cx,SettingsSnapshot{android:true,..Default::default()});
    for route in crate::settings_entry::EntryRoute::ALL{
        root.borrow_mut::<SettingsView>().unwrap().navigate_entry(&mut cx,route);
        assert!(root.borrow::<SettingsView>().unwrap().fault.is_none(),"route {route:?}: {:?}",root.borrow::<SettingsView>().unwrap().fault);
        paint(&mut cx,&root,dvec2(400.,700.));let tree=layout(&mut cx,&root);assert!(tree.active(),"route {route:?} did not publish a pane");
        let mut previous=page(&root);
        for _ in 0..5{
            if previous=="Overview"{break;}
            send(&mut cx,&root,"back","",Value::Null);let next=page(&root);
            // A first Back may retire an editor/review in the same pane.
            if next==previous{send(&mut cx,&root,"back","",Value::Null);}
            previous=page(&root);
        }
        assert_eq!(page(&root),"Overview","route {route:?} has a broken parent chain");
    }
    makepad_widgets::widget_async::leave_isolate(&mut cx,isolate);
}

#[test]
fn script_view_live_theme_keeps_editor_cursor_and_compiled_widgets(){
    let(mut cx,mut host)=setup();let root=host.get(1).unwrap().root.clone();let vm_id=host.get(1).unwrap().vm_id;
    let isolate=makepad_widgets::widget_async::enter_isolate(&mut cx,vm_id);
    send(&mut cx,&root,"navigate","search",Value::Null);paint(&mut cx,&root,dvec2(400.,700.));
    let input=root.text_input(&mut cx,ids!(settings_query));let uid=input.widget_uid();let root_uid=root.widget_uid();
    let edits=cx.capture_actions(|cx|input.replace_range(cx,0..0,"蓝牙",makepad_widgets::text_input::UndoGroup::New).unwrap());
    root.handle_event(&mut cx,&Event::Actions(edits),&mut Scope::empty());
    input.set_cursor(&mut cx,makepad_widgets::makepad_draw::text::selection::Cursor{index:3,prefer_next_row:false},false);
    assert_eq!(state(&root,"search_query"),s("蓝牙"));
    root.borrow_mut::<SettingsView>().unwrap().observe(&mut cx,SettingsSnapshot{android:true,font_scale:1.5,..Default::default()});
    makepad_widgets::widget_async::leave_isolate(&mut cx,isolate);
    host.apply_style(&mut cx,&Selection{preset:Preset::Paper,..Default::default()}.sheet(DesktopStyle::Android,true));
    assert_eq!(root.widget_uid(),root_uid);assert_eq!(input.widget_uid(),uid);assert_eq!(input.text(),"蓝牙");assert_eq!(input.cursor().index,3);
    let isolate=makepad_widgets::widget_async::enter_isolate(&mut cx,vm_id);
    send(&mut cx,&root,"click","settings_result_0",Value::Null);assert_eq!(page(&root),"Bluetooth");
    send(&mut cx,&root,"back","",Value::Null);assert_eq!(page(&root),"Search");assert_eq!(state(&root,"search_query"),s("蓝牙"));
    makepad_widgets::widget_async::leave_isolate(&mut cx,isolate);
}

#[test]
fn script_view_keyboard_dismissal_survives_observation_and_a11y_focus_reopens(){
    let(mut cx,host)=setup();let instance=host.get(1).unwrap();let root=instance.root.clone();let isolate=makepad_widgets::widget_async::enter_isolate(&mut cx,instance.vm_id);
    send(&mut cx,&root,"navigate","search",Value::Null);paint(&mut cx,&root,dvec2(400.,700.));
    let before=layout(&mut cx,&root);let editor=before.nodes.iter().find(|n|n.role==Role::Edit).unwrap();
    let mut edit=action(&before,editor,ActionKind::SetText);edit.text=Some("draft 中文".into());assert!(apply(&mut cx,&root,&edit));
    assert_eq!(state(&root,"search_query"),s("draft 中文"));assert!(apply(&mut cx,&root,&action(&before,editor,ActionKind::Focus)));
    let input=root.text_input(&mut cx,ids!(settings_query));cx.widget_action(root.widget_uid(),ButtonAction::Released(Default::default()));cx.handle_actions();paint(&mut cx,&root,dvec2(400.,700.));
    assert!(cx.hosted_ime_state().visible);
    root.handle_event(&mut cx,&Event::VirtualKeyboard(VirtualKeyboardEvent::DidHide{time:1.}),&mut Scope::empty());
    send(&mut cx,&root,"observe","",Value::Null);paint(&mut cx,&root,dvec2(400.,700.));
    assert!(!cx.hosted_ime_state().visible);assert!(cx.has_key_focus(input.area()));
    let current=layout(&mut cx,&root);let current_editor=current.nodes.iter().find(|n|n.role==Role::Edit).unwrap();assert_eq!(current_editor.id,editor.id);
    assert!(apply(&mut cx,&root,&action(&current,current_editor,ActionKind::Focus)));paint(&mut cx,&root,dvec2(400.,700.));assert!(cx.hosted_ime_state().visible);
    for id in ["settings_query","bt_name","network_dns_hostname"]{let config=root.text_input(&mut cx,&[LiveId::from_str(id)]).borrow().unwrap().ime_config();assert_eq!(config.soft_keyboard.autocorrect,makepad_platform::ime::AutoCorrect::Disabled);assert_eq!(config.soft_keyboard.autocapitalize,makepad_platform::ime::AutoCapitalize::None);}
    makepad_widgets::widget_async::leave_isolate(&mut cx,isolate);
}

#[test]
fn script_view_clear_search_never_blurs_during_native_touch(){
    use makepad_platform::event::{TouchPoint,TouchState,TouchUpdateEvent};
    let(mut cx,host)=setup();let instance=host.get(1).unwrap();let root=instance.root.clone();let isolate=makepad_widgets::widget_async::enter_isolate(&mut cx,instance.vm_id);
    send(&mut cx,&root,"navigate","search",Value::Null);let input=root.text_input(&mut cx,ids!(settings_query));input.set_text(&mut cx,"old");send(&mut cx,&root,"input","settings_query",s("old"));paint(&mut cx,&root,dvec2(400.,700.));
    let uid=input.widget_uid();let clear=root.widget(&mut cx,ids!(settings_search_clear));input.take_key_focus(&mut cx);cx.widget_action(clear.widget_uid(),ButtonAction::Released(Default::default()));cx.handle_actions();
    let rect=clear.area().clipped_rect(&cx);assert!(rect.size.y>0.);let center=rect.pos+rect.size*0.5;
    for cancel in [true,false]{
        for(step,touch_state)in [(0,TouchState::Start),(1,TouchState::Move),(2,TouchState::Stop)]{
            let abs=if cancel&&step>0{center+dvec2(500.,0.)}else{center};let time=1.+step as f64*0.1;
            let events=cx.capture_actions(|cx|root.handle_event(cx,&Event::TouchUpdate(TouchUpdateEvent{time,window_id:CxWindowPool::id_zero(),modifiers:Default::default(),touches:vec![TouchPoint{uid:7,state:touch_state,abs,time,rotation_angle:0.,force:1.,radius:dvec2(1.,1.),handled:Default::default(),sweep_lock:Default::default()}]}),&mut Scope::empty()));
            if cancel{assert!(!clear.as_button().clicked(&events));}root.handle_event(&mut cx,&Event::Actions(events),&mut Scope::empty());cx.widget_action(clear.widget_uid(),ButtonAction::Released(Default::default()));cx.handle_actions();
            assert!(cx.has_key_focus(input.area()),"Clear blurred during touch step {step}, canceled={cancel}");
        }
        assert_eq!(input.text(),if cancel{"old"}else{""});
    }
    let events=cx.capture_actions(|cx|root.handle_event(cx,&Event::TextInput(TextInputEvent{input:"b".into(),..Default::default()}),&mut Scope::empty()));root.handle_event(&mut cx,&Event::Actions(events),&mut Scope::empty());
    assert_eq!(input.widget_uid(),uid);assert_eq!(input.text(),"b");assert_eq!(state(&root,"search_query"),s("b"));
    makepad_widgets::widget_async::leave_isolate(&mut cx,isolate);
}

#[test]
fn script_view_accessibility_uses_context_labels_and_clips_large_text(){
    let(mut cx,host)=setup();let instance=host.get(1).unwrap();let root=instance.root.clone();let isolate=makepad_widgets::widget_async::enter_isolate(&mut cx,instance.vm_id);
    root.borrow_mut::<SettingsView>().unwrap().observe(&mut cx,SettingsSnapshot{android:true,font_scale:1.5,..Default::default()});send(&mut cx,&root,"navigate","sound",Value::Null);paint(&mut cx,&root,dvec2(320.,640.));
    let before=layout(&mut cx,&root);assert_eq!(before.pane,"Sound");let scroll=before.scroll.as_ref().unwrap();
    for node in &before.nodes{assert!(contains(before.bounds,node.bounds));if node.parent==Some(scroll.id){assert!(contains(scroll.bounds,node.bounds));}}
    let button=before.nodes.iter().find(|n|n.label=="Increase media volume by 10%").unwrap();assert!(!button.enabled&&button.actions.is_empty());
    assert!(!apply(&mut cx,&root,&action(&before,button,ActionKind::Click)));
    send(&mut cx,&root,"observe","",Value::Null);paint(&mut cx,&root,dvec2(320.,640.));let after=layout(&mut cx,&root);assert_eq!(after.token,before.token);assert!(after.nodes.iter().any(|n|n.id==button.id));
    makepad_widgets::widget_async::leave_isolate(&mut cx,isolate);
}

#[test]
fn script_view_recycled_app_node_retires_without_churning_search_ack_node(){
    let(mut cx,host)=setup();let instance=host.get(1).unwrap();let root=instance.root.clone();let isolate=makepad_widgets::widget_async::enter_isolate(&mut cx,instance.vm_id);
    let mut catalog=AppsCatalog::decode(&crate::settings_apps::tests::catalog(0,1,"")).unwrap();
    root.borrow_mut::<SettingsView>().unwrap().observe(&mut cx,SettingsSnapshot{android:true,apps_catalog:Some(catalog.clone()),..Default::default()});send(&mut cx,&root,"navigate","apps",Value::Null);paint(&mut cx,&root,dvec2(400.,700.));
    let row=root.widget(&mut cx,ids!(app_row_0));let y=row.area().rect(&cx).pos.y;let scroll=root.borrow::<SettingsView>().unwrap().active_scroll(&mut cx);scroll.as_view().set_scroll_pos(&mut cx,dvec2(0.,(y-400.).max(0.)));paint(&mut cx,&root,dvec2(400.,700.));
    let before=layout(&mut cx,&root);let row_before=before.nodes.iter().find(|n|n.label.contains(catalog.apps[0].target.package())).unwrap();let stale=action(&before,row_before,ActionKind::Click);
    catalog.apps[0].target=AppTarget::decode(&s("replacement.package")).unwrap();
    root.borrow_mut::<SettingsView>().unwrap().observe(&mut cx,SettingsSnapshot{android:true,apps_catalog:Some(catalog),..Default::default()});paint(&mut cx,&root,dvec2(400.,700.));
    let after=layout(&mut cx,&root);assert_eq!(before.token,after.token);assert!(after.nodes.iter().any(|n|n.label.contains("replacement.package")&&n.id!=row_before.id));assert!(!apply(&mut cx,&root,&stale));
    makepad_widgets::widget_async::leave_isolate(&mut cx,isolate);
}

#[test]
fn script_view_caption_replacement_cannot_retarget_a_held_button(){
    use crate::settings_caption_custom::{CaptionRequest,CaptionSnapshot};
    fn button_event(cx:&mut Cx,root:&WidgetRef,button:&WidgetRef,event:ButtonAction)->ActionsBuf{
        let actions=cx.capture_actions(|cx|cx.widget_action(button.widget_uid(),event));
        cx.capture_actions(|cx|root.handle_event(cx,&Event::Actions(actions),&mut Scope::empty()))
    }
    let(mut cx,host)=setup();let instance=host.get(1).unwrap();let root=instance.root.clone();let isolate=makepad_widgets::widget_async::enter_isolate(&mut cx,instance.vm_id);
    let mut observed=SettingsSnapshot{android:true,..Default::default()};
    root.borrow_mut::<SettingsView>().unwrap().observe(&mut cx,observed.clone());send(&mut cx,&root,"navigate","caption_custom",Value::Null);
    let old_visit=state(&root,"caption_ui").get("visit").unwrap().as_str().unwrap().parse::<i64>().unwrap();
    observed.caption_custom=CaptionSnapshot::decode(&crate::settings_caption_custom::tests::snapshot(1,old_visit));
    root.borrow_mut::<SettingsView>().unwrap().observe(&mut cx,observed.clone());send(&mut cx,&root,"click","control_1_more",Value::Null);paint(&mut cx,&root,dvec2(400.,900.));
    let button=root.widget(&mut cx,ids!(caption_choice_3));
    button_event(&mut cx,&root,&button,ButtonAction::Pressed(Default::default()));
    assert!(root.borrow::<SettingsView>().unwrap().button_presses["caption_choice_3"].is_some());
    root.borrow_mut::<SettingsView>().unwrap().renew_caption_visit(&mut cx);
    let visit=state(&root,"caption_ui").get("visit").unwrap().as_str().unwrap().parse::<i64>().unwrap();assert_ne!(old_visit,visit);
    observed.caption_custom=CaptionSnapshot::decode(&crate::settings_caption_custom::tests::snapshot(2,visit));
    root.borrow_mut::<SettingsView>().unwrap().observe(&mut cx,observed.clone());
    // The replacement lease is already usable and the script's held target was
    // cleared by renewal. The old physical release must still issue no request.
    assert!(!root.borrow::<SettingsView>().unwrap().disabled.contains("caption_choice_3"));
    let stale=button_event(&mut cx,&root,&button,ButtonAction::Clicked(Default::default()));assert!(stale.find_widget_action(root.widget_uid()).is_none());
    // A transient revoked snapshot cannot resurrect a press if the same semantic
    // target becomes available again before release.
    button_event(&mut cx,&root,&button,ButtonAction::Pressed(Default::default()));
    let mut retired=observed.clone();retired.caption_custom.as_mut().unwrap().retire();root.borrow_mut::<SettingsView>().unwrap().observe(&mut cx,retired);
    root.borrow_mut::<SettingsView>().unwrap().observe(&mut cx,observed);
    let stale=button_event(&mut cx,&root,&button,ButtonAction::Clicked(Default::default()));assert!(stale.find_widget_action(root.widget_uid()).is_none());
    // Fresh accessibility actions use a new Pressed/Clicked pair and remain
    // actionable against the replacement visit.
    paint(&mut cx,&root,dvec2(400.,900.));let tree=layout(&mut cx,&root);let label=format!("Text color: {}",button.text());let node=tree.nodes.iter().find(|n|n.label==label).unwrap();
    let actions=cx.capture_actions(|cx|assert!(apply(cx,&root,&action(&tree,node,ActionKind::Click))));
    assert!(actions.find_widget_action(root.widget_uid()).is_some_and(|a|matches!(a.action.downcast_ref::<SettingsRequest>(),Some(SettingsRequest::CaptionCustom(CaptionRequest::Set{visit:v,..}))if *v==visit)));
    makepad_widgets::widget_async::leave_isolate(&mut cx,isolate);
}

#[test]
fn script_view_search_return_restores_real_scroll_position(){
    let(mut cx,host)=setup();let instance=host.get(1).unwrap();let root=instance.root.clone();let isolate=makepad_widgets::widget_async::enter_isolate(&mut cx,instance.vm_id);
    send(&mut cx,&root,"navigate","search",Value::Null);send(&mut cx,&root,"input","settings_query",s("a"));paint(&mut cx,&root,dvec2(400.,600.));
    let scroll=root.borrow::<SettingsView>().unwrap().active_scroll(&mut cx);scroll.as_view().set_scroll_pos(&mut cx,dvec2(0.,180.));paint(&mut cx,&root,dvec2(400.,600.));let position=scroll.as_view().scroll_pos();assert!(position.y>100.);
    send(&mut cx,&root,"click","settings_result_3",Value::Null);assert_ne!(page(&root),"Search");send(&mut cx,&root,"back","",Value::Null);paint(&mut cx,&root,dvec2(400.,600.));
    assert_eq!(page(&root),"Search");assert_eq!(scroll.as_view().scroll_pos(),position);assert_eq!(state(&root,"search_query"),s("a"));
    makepad_widgets::widget_async::leave_isolate(&mut cx,isolate);
}

#[test]
fn script_view_app_details_back_preserves_list_filter_scroll_and_read_target(){
    let(mut cx,host)=setup();let instance=host.get(1).unwrap();let root=instance.root.clone();let isolate=makepad_widgets::widget_async::enter_isolate(&mut cx,instance.vm_id);
    root.borrow_mut::<SettingsView>().unwrap().observe(&mut cx,SettingsSnapshot{android:true,..Default::default()});send(&mut cx,&root,"navigate","apps",Value::Null);
    send(&mut cx,&root,"input","apps_query",s("mail"));send(&mut cx,&root,"click","apps_search",Value::Null);
    let catalog=AppsCatalog::decode(&crate::settings_apps::tests::catalog(0,20,"mail")).unwrap();
    root.borrow_mut::<SettingsView>().unwrap().observe(&mut cx,SettingsSnapshot{android:true,apps_catalog:Some(catalog),..Default::default()});paint(&mut cx,&root,dvec2(400.,600.));
    let scroll=root.borrow::<SettingsView>().unwrap().active_scroll(&mut cx);scroll.as_view().set_scroll_pos(&mut cx,dvec2(0.,180.));paint(&mut cx,&root,dvec2(400.,600.));let position=scroll.as_view().scroll_pos();assert!(position.y>100.);
    send(&mut cx,&root,"click","app_row_3",Value::Null);assert_eq!(page(&root),"AppDetails");
    let requests=cx.capture_actions(|cx|send(cx,&root,"back","",Value::Null));paint(&mut cx,&root,dvec2(400.,600.));assert_eq!(page(&root),"Apps");assert_eq!(scroll.as_view().scroll_pos(),position);
    assert_eq!(state(&root,"apps").get("filter").and_then(Value::as_str),Some("mail"));
    assert!(requests.find_widget_action(root.widget_uid()).is_some_and(|a|matches!(a.action.downcast_ref::<SettingsRequest>(),Some(SettingsRequest::AppsCatalog{query,..})if query=="mail")));
    makepad_widgets::widget_async::leave_isolate(&mut cx,isolate);
}

#[test]
fn script_view_button_swipe_hands_off_without_click_or_native_request(){
    use makepad_platform::event::{TouchPoint,TouchState,TouchUpdateEvent};
    for swipe in [true,false]{
        let(mut cx,host)=setup();let instance=host.get(1).unwrap();let root=instance.root.clone();let isolate=makepad_widgets::widget_async::enter_isolate(&mut cx,instance.vm_id);
        paint(&mut cx,&root,dvec2(400.,600.));let button=root.widget(&mut cx,ids!(sound_page));let rect=button.area().clipped_rect(&cx);assert!(rect.size.y>0.);let start=rect.pos+rect.size*0.5;
        let scroll=root.borrow::<SettingsView>().unwrap().active_scroll(&mut cx);let before=scroll.as_view().scroll_pos();
        for(step,touch_state)in [(0,TouchState::Start),(1,TouchState::Move),(2,TouchState::Move),(3,TouchState::Stop)]{
            let time=1.+step as f64*0.1;let abs=start-dvec2(0.,step.min(2)as f64*if swipe{50.}else{1.});
            let actions=cx.capture_actions(|cx|root.handle_event(cx,&Event::TouchUpdate(TouchUpdateEvent{time,window_id:CxWindowPool::id_zero(),modifiers:Default::default(),touches:vec![TouchPoint{uid:7,state:touch_state,abs,time,rotation_angle:0.,force:1.,radius:dvec2(1.,1.),handled:Default::default(),sweep_lock:Default::default()}]}),&mut Scope::empty()));
            if swipe{assert!(!button.as_button().clicked(&actions));}
            let routed=cx.capture_actions(|cx|root.handle_event(cx,&Event::Actions(actions),&mut Scope::empty()));
            if swipe{assert!(routed.find_widget_action(root.widget_uid()).is_none());}
            paint(&mut cx,&root,dvec2(400.,600.));
        }
        if swipe{assert_eq!(page(&root),"Overview");assert!(scroll.as_view().scroll_pos().y>before.y+80.);assert!(root.borrow::<SettingsView>().unwrap().button_scroll_press.is_none());}
        else{assert_eq!(page(&root),"Sound");}
        makepad_widgets::widget_async::leave_isolate(&mut cx,isolate);
    }
}
