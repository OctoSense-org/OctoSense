use crate::settings_app::{DeviceSnapshot,SettingsSnapshot};
use crate::settings_script::{bundled_source,bundled_widgets,Controller,Frame,Patch};
use makepad_strict_json::{obj,s,Value};

fn new()->Controller{Controller::new(&bundled_source(),bundled_widgets()).unwrap()}
fn step(c:&mut Controller,o:&SettingsSnapshot,kind:&str,id:&str,value:Value)->Frame{
    c.step(&obj(vec![("kind",s(kind)),("id",s(id)),("value",value)]),&crate::settings_script_bridge::observation(o,false).unwrap(),|_|Ok(())).unwrap().0
}
fn click(c:&mut Controller,o:&SettingsSnapshot,id:&str)->Frame{step(c,o,"click",id,Value::Null)}
fn page(c:&Controller)->&str{c.state().get("page").unwrap().as_str().unwrap()}
fn text<'a>(f:&'a Frame,id:&str)->&'a str{f.patches.iter().rev().find_map(|p|match p{Patch::Text(k,v)if k==id=>Some(v.as_str()),_=>None}).unwrap()}
fn enabled(f:&Frame,id:&str)->bool{f.patches.iter().rev().find_map(|p|match p{Patch::Enabled(k,v)if k==id=>Some(*v),_=>None}).unwrap()}

#[test]
fn script_search_keeps_native_rank_order_unicode_paging_and_locality(){
    let mut c=new();let o=SettingsSnapshot::default();
    click(&mut c,&o,"search_settings");
    for query in ["BRIGHTNESS","字体大小","自动同步","wi-fi","蓝牙","notification history","省电","私人DNS","airplane","节省流量","a"]{
        let frame=step(&mut c,&o,"input","settings_query",s(query));
        assert!(frame.requests.is_empty(),"local searches never contact a service");
        let rows=frame.state.get("search_rows").unwrap().as_arr().unwrap();
        let expected=crate::settings_search::search(query);
        assert_eq!(rows.len(),expected.len().min(20),"{query}");
        for (row,expected) in rows.iter().zip(expected.iter()){
            assert_eq!(row.get("title").and_then(Value::as_str),Some(expected.title));
            assert_eq!(row.get("route").and_then(Value::as_str),Some(format!("{:?}",expected.route).as_str()));
        }
        if expected.len()>20{
            let frame=click(&mut c,&o,"settings_search_next");
            assert_eq!(frame.state.get("search_offset"),Some(&Value::Int(20)));
            assert_eq!(frame.state.get("search_rows").unwrap().as_arr().unwrap()[0].get("title").and_then(Value::as_str),Some(expected[20].title));
        }
    }
    let frame=step(&mut c,&o,"input","settings_query",s("bad\nquery"));
    assert_eq!(text(&frame,"settings_search_summary"),"Use up to 128 characters without line breaks.");
    let frame=click(&mut c,&o,"settings_search_clear");
    assert!(frame.patches.contains(&Patch::Input("settings_query".into(),String::new())));
    assert!(frame.patches.contains(&Patch::Focus("settings_query".into())));
}

#[test]
fn script_search_does_not_retarget_a_held_row_and_back_preserves_the_search(){
    let mut c=new();let o=SettingsSnapshot::default();click(&mut c,&o,"search_settings");
    step(&mut c,&o,"input","settings_query",s("brightness"));
    step(&mut c,&o,"press","settings_result_0",Value::Null);
    step(&mut c,&o,"input","settings_query",s("volume"));
    assert!(click(&mut c,&o,"settings_result_0").requests.is_empty());assert_eq!(page(&c),"Search");
    click(&mut c,&o,"settings_result_0");assert_eq!(page(&c),"Sound");
    step(&mut c,&o,"back","",Value::Null);assert_eq!(page(&c),"Search");
    assert_eq!(c.state().get("search_query").and_then(Value::as_str),Some("volume"));
    step(&mut c,&o,"navigate","display",Value::Null);step(&mut c,&o,"back","",Value::Null);
    assert_eq!(page(&c),"Overview","external entry retires the prior search-return parent");
}

#[test]
fn script_theme_draft_follows_clean_observations_and_retains_dirty_edits(){
    use crate::mobile_theme::Preset;
    let mut c=new();let mut o=SettingsSnapshot::default();click(&mut c,&o,"appearance_page");
    click(&mut c,&o,"paper");o.theme.preset=Preset::Vivid;
    step(&mut c,&o,"theme","",Value::Null);
    assert_eq!(c.state().get("theme_draft").unwrap().get("preset").and_then(Value::as_str),Some("paper"));
    click(&mut c,&o,"cancel_theme");
    assert_eq!(c.state().get("theme_draft").unwrap().get("preset").and_then(Value::as_str),Some("vivid"));
    click(&mut c,&o,"minimal");
    let frame=step(&mut c,&o,"result","",obj(vec![("pending",Value::Bool(true)),("message",s("Saving…"))]));
    assert!(!enabled(&frame,"apply_theme"));assert!(click(&mut c,&o,"apply_theme").requests.is_empty());
    step(&mut c,&o,"result","",obj(vec![("pending",Value::Bool(false)),("message",s("Could not apply"))]));
    assert_eq!(click(&mut c,&o,"apply_theme").requests.len(),1,"local theme does not require Android");
    step(&mut c,&o,"back","",Value::Null);
    assert_eq!(c.state().get("theme_draft").unwrap().get("preset").and_then(Value::as_str),Some("vivid"));
}

#[test]
fn script_device_steps_use_observed_bounds_and_do_not_require_optional_bridge(){
    let mut c=new();let mut o=SettingsSnapshot{android:true,device:Some(DeviceSnapshot{
        screen_timeout_ms:Some(45000),max_screen_timeout_ms:Some(45000),font_scale:Some(1.2),
        haptic_feedback:Some(true),volume_media:Some(0.33),
        capabilities:["screen_timeout_ms".into(),"font_scale".into(),"haptic_feedback".into(),"volume_media".into()].into_iter().collect(),
        ..Default::default()}),..Default::default()};
    let frame=click(&mut c,&o,"display_page");assert!(!enabled(&frame,"timeout_more"));assert!(enabled(&frame,"timeout_less"));
    let frame=click(&mut c,&o,"timeout_less");assert_eq!(frame.requests[0].get("value"),Some(&Value::Int(30000)));
    assert!(click(&mut c,&o,"timeout_more").requests.is_empty());
    let frame=click(&mut c,&o,"font_more");assert_eq!(frame.requests[0].get("value"),Some(&Value::F64(1.3)));
    let frame=click(&mut c,&o,"haptic_feedback");assert_eq!(frame.requests[0].get("value"),Some(&Value::Bool(false)));
    step(&mut c,&o,"navigate","sound",Value::Null);
    let frame=click(&mut c,&o,"volume_more");assert_eq!(frame.requests[0].get("setting").and_then(Value::as_str),Some("volume_media"));
    assert_eq!(text(&frame,"volume_value"),"Media volume: 33%");
    step(&mut c,&o,"press","volume_more",Value::Null);
    o.device.as_mut().unwrap().volume_media=Some(0.5);
    step(&mut c,&o,"observe","",Value::Null);
    assert!(click(&mut c,&o,"volume_more").requests.is_empty());
    o.device.as_mut().unwrap().capabilities.clear();
    assert!(!enabled(&step(&mut c,&o,"observe","",Value::Null),"volume_more"));
    assert!(click(&mut c,&o,"volume_more").requests.is_empty());
}
