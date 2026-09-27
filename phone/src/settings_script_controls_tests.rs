//! Executes the bundled Octoscript controller against actual serialized native models.
use crate::settings_app::SettingsSnapshot;
use crate::settings_controls::{ControlId, ControlValue, ControlsPage, ControlsSnapshot};
use crate::settings_script::{bundled_source, bundled_widgets, Controller, Frame, Patch};
use makepad_strict_json::{obj, s, Value};

fn controller() -> Controller { Controller::new(&bundled_source(), bundled_widgets()).unwrap() }
fn state(page: ControlsPage) -> SettingsSnapshot {
    SettingsSnapshot { android:true, controls:ControlsSnapshot::decode(&crate::settings_controls::tests::snapshot(1,page)), ..Default::default() }
}
fn event(controller:&mut Controller,state:&SettingsSnapshot,kind:&str,id:&str) -> Frame {
    controller.step(&obj(vec![("kind",s(kind)),("id",s(id)),("value",Value::Null)]),
        &crate::settings_script_bridge::observation(state,false).unwrap(), |frame| {
            for request in &frame.requests {
                if crate::settings_script_bridge::basic_request(request,state).is_none() {
                    return Err(format!("Rejected Controls binding: {request:?}"));
                }
            }
            Ok(())
        }).unwrap().0
}
fn text<'a>(frame:&'a Frame,id:&str)->&'a str {
    frame.patches.iter().rev().find_map(|p|match p{Patch::Text(target,value)if target==id=>Some(value.as_str()),_=>None}).unwrap()
}
fn enabled(frame:&Frame,id:&str)->bool {
    frame.patches.iter().rev().find_map(|p|match p{Patch::Enabled(target,value)if target==id=>Some(*value),_=>None}).unwrap()
}
fn request(frame:&Frame,field:&str,value:&str) {
    assert_eq!(frame.requests.len(),1,"{:?}",frame.requests);
    assert_eq!(frame.requests[0].get("kind").and_then(Value::as_str),Some("control"));
    assert_eq!(frame.requests[0].get("control").and_then(Value::as_str),Some(field));
    assert_eq!(frame.requests[0].get("value").and_then(Value::as_str),Some(value));
}

#[test]
fn script_controls_render_all_native_sections_and_do_not_promote_read_only_observations() {
    for page in ControlsPage::ALL.into_iter().filter(|p|*p!=ControlsPage::CaptionCustom) {
        let mut controller=controller();let mut native=state(page);
        let frame=event(&mut controller,&native,"navigate",page.wire());
        assert_eq!(text(&frame,"title"),page.title());
        for (i,field) in page.controls().iter().enumerate() {
            assert_eq!(text(&frame,&format!("control_{i}_label")),field.label());
            assert_eq!(text(&frame,&format!("control_{i}_value")),native.controls.as_ref().unwrap().control(*field).unwrap().value.unwrap().label());
        }
        native.controls.as_mut().unwrap().clear_actions();
        let frame=event(&mut controller,&native,"observe","");
        for (i,field) in page.controls().iter().enumerate() {
            assert_eq!(text(&frame,&format!("control_{i}_value")),native.controls.as_ref().unwrap().control(*field).unwrap().value.unwrap().label());
            for suffix in ["less","more"] {
                assert!(event(&mut controller,&native,"click",&format!("control_{i}_{suffix}")).requests.is_empty());
            }
        }
    }
}

#[test]
fn script_controls_bind_nearest_choice_and_explicit_choice_to_the_pressed_target() {
    let mut controller=controller();let mut native=state(ControlsPage::BatteryPolicy);
    event(&mut controller,&native,"navigate","battery_policy");
    request(&event(&mut controller,&native,"click","control_1_more"),"battery_threshold","25");
    event(&mut controller,&native,"press","control_1_more");
    native.controls.as_mut().unwrap().controls.iter_mut().find(|r|r.id==ControlId::BatteryThreshold).unwrap().value=Some(ControlValue::Percent(27));
    event(&mut controller,&native,"observe","");
    assert!(event(&mut controller,&native,"click","control_1_more").requests.is_empty());
    native=state(ControlsPage::AccessibilityVision);
    event(&mut controller,&native,"navigate","accessibility_vision");
    event(&mut controller,&native,"press","control_2_choice_0");
    native.controls.as_mut().unwrap().controls.iter_mut().find(|r|r.id==ControlId::ColorCorrectionMode).unwrap().options.reverse();
    event(&mut controller,&native,"observe","");
    assert!(event(&mut controller,&native,"click","control_2_choice_0").requests.is_empty());
    request(&event(&mut controller,&native,"click","control_2_choice_0"),"color_correction_mode","grayscale");
}

#[test]
fn script_hearing_preserves_custom_balance_and_never_optimistically_changes_readback() {
    let mut controller=controller();let native=state(ControlsPage::AccessibilityHearing);
    let frame=event(&mut controller,&native,"navigate","accessibility_hearing");
    assert_eq!(text(&frame,"control_1_value"),"23.125% right");
    request(&event(&mut controller,&native,"click","control_1_more"),"audio_balance","balance_percent:24");
    request(&event(&mut controller,&native,"click","control_1_less"),"audio_balance","balance_percent:23");
    let frame=event(&mut controller,&native,"click","control_1_center");
    request(&frame,"audio_balance","balance_percent:0");
    assert_eq!(text(&frame,"control_1_value"),"23.125% right");
    assert!(enabled(&frame,"control_3_choice_4"),"caption sizes stay available while captions are off");
    request(&event(&mut controller,&native,"click","control_3_choice_4"),"captions_font_scale","caption_scale:2.0");
}

#[test]
fn script_autoclick_picker_exposes_all_ten_native_options_and_rejects_reordered_press() {
    let mut controller=controller();let mut native=state(ControlsPage::AccessibilityTextInteraction);
    event(&mut controller,&native,"navigate","accessibility_text_interaction");
    let frame=event(&mut controller,&native,"click","control_5_more");
    assert_eq!(frame.state.get("page").and_then(Value::as_str),Some("ControlChoices"));
    assert_eq!(text(&frame,"title"),"Automatic click");
    assert_eq!(text(&frame,"control_picker_0"),"Off · Selected");
    assert_eq!(text(&frame,"control_picker_9"),"1000 ms");
    assert!(!enabled(&frame,"control_picker_next"));
    assert!(!enabled(&frame,"control_picker_10"));
    request(&event(&mut controller,&native,"click","control_picker_9"),"autoclick","autoclick_delay:1000");
    event(&mut controller,&native,"press","control_picker_1");
    native.controls.as_mut().unwrap().controls[5].options.swap(1,2);
    event(&mut controller,&native,"observe","");
    assert!(event(&mut controller,&native,"click","control_picker_1").requests.is_empty());
    let frame=event(&mut controller,&native,"back","");
    assert_eq!(frame.state.get("page").and_then(Value::as_str),Some("Controls"));
    assert_eq!(text(&frame,"control_5_value"),"Off");
}

#[test]
fn script_notification_history_disable_requires_current_review_and_current_authority() {
    let mut controller=controller();let mut native=state(ControlsPage::Notifications);
    native.controls.as_mut().unwrap().controls.iter_mut().find(|r|r.id==ControlId::NotificationHistory).unwrap().value=Some(ControlValue::On);
    event(&mut controller,&native,"navigate","notifications");
    assert!(event(&mut controller,&native,"click","history_disable_confirm").requests.is_empty());
    let review=event(&mut controller,&native,"click","control_2_less");
    assert!(review.requests.is_empty());assert!(enabled(&review,"history_disable_confirm"));
    native.controls.as_mut().unwrap().clear_actions();
    let stale=event(&mut controller,&native,"observe","");assert!(!enabled(&stale,"history_disable_confirm"));
    assert!(event(&mut controller,&native,"click","history_disable_confirm").requests.is_empty());
    event(&mut controller,&native,"click","history_disable_cancel");
    native=state(ControlsPage::Notifications);native.controls.as_mut().unwrap().controls[2].value=Some(ControlValue::On);
    event(&mut controller,&native,"observe","");
    assert!(event(&mut controller,&native,"click","history_disable_confirm").requests.is_empty());
    event(&mut controller,&native,"click","control_2_less");
    request(&event(&mut controller,&native,"click","history_disable_confirm"),"notification_history","off");
}

#[test]
fn script_custom_text_observations_are_labels_and_not_arbitrary_setters() {
    use crate::settings_text_interaction::{TextControl,TextValue};
    let mut controller=controller();let mut native=state(ControlsPage::AccessibilityTextInteraction);
    for (field,value) in [(TextControl::BoldText,TextValue::FontWeight(125)),(TextControl::RemoveAnimations,TextValue::AnimationScales([0.5f32.to_bits(),1.0f32.to_bits(),0.25f32.to_bits()])),(TextControl::TouchHold,TextValue::HoldMs(735)),(TextControl::ActionTimeout,TextValue::TimeoutMs(30000,12500))] {
        native.controls.as_mut().unwrap().controls.iter_mut().find(|r|r.id==ControlId::TextInteraction(field)).unwrap().value=Some(ControlValue::TextInteraction(value));
    }
    let frame=event(&mut controller,&native,"navigate","accessibility_text_interaction");
    assert_eq!(text(&frame,"control_1_value"),"Custom weight adjustment: 125");
    assert_eq!(text(&frame,"control_2_value"),"Custom animation scales: 0.5, 1, 0.25");
    assert_eq!(text(&frame,"control_3_value"),"Custom: 735 ms");
    assert_eq!(text(&frame,"control_4_value"),"Custom: controls 30000 ms, other content 12500 ms");
    request(&event(&mut controller,&native,"click","control_2_more"),"remove_animations","on");
    request(&event(&mut controller,&native,"click","control_3_choice_0"),"touch_hold_delay","hold:short");
    let frame=event(&mut controller,&native,"click","control_4_choice_1");
    request(&frame,"action_timeout","timeout:seconds10");
    assert_eq!(text(&frame,"control_4_value"),"Custom: controls 30000 ms, other content 12500 ms");
}

#[test]
fn script_loop_ending_conditional_branches_have_explicit_values_across_calls() {
    let source=r#"
        fn settings_new(){return {count:0}}
        fn settings_step(s,e,o){
            let i=0
            while i<2 {
                if i==0 { let j=0; while j<2 {s.count=s.count+1;j=j+1}; nil }
                else {for j in [1,2] {s.count=s.count+1}; nil}
                i=i+1
            }
            return {state:s,patch:[],requests:[],handled:true}
        }
    "#;
    let mut c=Controller::new(source,Default::default()).unwrap();
    for expected in [4,8,12] {
        c.step(&Value::Null,&Value::Null,|_|Ok(())).unwrap();
        assert_eq!(c.state().get("count"),Some(&Value::Int(expected)));
    }
}
