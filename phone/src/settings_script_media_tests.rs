use crate::settings_app::SettingsSnapshot;
use crate::settings_script::{Controller,Frame,Patch,bundled_source,bundled_widgets};
use crate::settings_script_bridge::{observation,basic_request};
use makepad_strict_json::{obj,s,Value};
fn new()->Controller{Controller::new(&bundled_source(),bundled_widgets()).unwrap()}
fn step(c:&mut Controller,o:&SettingsSnapshot,kind:&str,id:&str)->Frame{
 c.step(&obj(vec![("kind",s(kind)),("id",s(id)),("value",Value::Null)]),&observation(o,false).unwrap(),|f|{
  for r in &f.requests {if basic_request(r,o).is_none(){return Err(format!("Rejected test binding: {r:?}"));}}Ok(())
 }).unwrap().0
}
fn click(c:&mut Controller,o:&SettingsSnapshot,id:&str)->Frame{step(c,o,"click",id)}
fn text<'a>(f:&'a Frame,id:&str)->&'a str{f.patches.iter().rev().find_map(|p|match p{Patch::Text(k,v)if k==id=>Some(v.as_str()),_=>None}).unwrap()}
fn sounds()->SettingsSnapshot{SettingsSnapshot{android:true,sounds:crate::settings_sounds::SoundsSnapshot::decode(&crate::settings_sounds::tests::snapshot(1,43,0)),..Default::default()}}
#[test]fn script_sound_requires_captured_catalog_target_and_save_or_preview_are_explicit(){
 let mut c=new();let mut o=sounds();step(&mut c,&o,"navigate","sounds");
 assert!(click(&mut c,&o,"sound_row_1").requests.is_empty());assert!(click(&mut c,&o,"sounds_save").requests.is_empty());
 step(&mut c,&o,"press","sound_row_1");o.sounds.as_mut().unwrap().rows.swap(1,2);step(&mut c,&o,"observe","");click(&mut c,&o,"sound_row_1");assert!(click(&mut c,&o,"sounds_save").requests.is_empty());
 step(&mut c,&o,"press","sound_row_1");assert!(click(&mut c,&o,"sound_row_1").requests.is_empty());let f=click(&mut c,&o,"sounds_preview");assert_eq!(f.requests[0].get("operation").and_then(Value::as_str),Some("Preview"));
 let f=click(&mut c,&o,"sounds_save");assert_eq!(f.requests[0].get("operation").and_then(Value::as_str),Some("Save"));assert_eq!(text(&f,"sounds_current"),"Current: Silent");
}
#[test]fn script_sound_catalog_changes_do_not_rebind_draft_and_leaving_stops_preview(){
 let mut c=new();let mut o=sounds();step(&mut c,&o,"navigate","sounds");step(&mut c,&o,"press","sound_row_1");click(&mut c,&o,"sound_row_1");
 o.sounds=crate::settings_sounds::SoundsSnapshot::decode(&crate::settings_sounds::tests::snapshot(2,43,20));step(&mut c,&o,"observe","");assert!(click(&mut c,&o,"sounds_save").requests.is_empty());
 let f=step(&mut c,&o,"pause","");assert_eq!(f.requests[0].get("kind").and_then(Value::as_str),Some("sound_stop"));
 let f=step(&mut c,&o,"back","");assert_eq!(f.requests[0].get("kind").and_then(Value::as_str),Some("sound_stop"));
}
#[test]fn script_history_paging_uses_snapshot_key_and_retirement_clears_all_visible_text(){
 let mut c=new();let mut o=SettingsSnapshot{android:true,notification_history:crate::settings_notifications::HistorySnapshot::decode(&crate::settings_notifications::tests::snapshot(1,43,0)),..Default::default()};
 let f=step(&mut c,&o,"navigate","notification_history");assert_eq!(text(&f,"history_row_0_text"),"中文内容 📨");
 let f=click(&mut c,&o,"history_next");assert_eq!(f.requests[0].get("offset"),Some(&Value::Int(20)));assert_eq!(f.requests[0].get("key").and_then(Value::as_str),Some("a".repeat(64).as_str()));
 o.notification_history=None;let f=step(&mut c,&o,"observe","");assert_eq!(text(&f,"history_row_0_text"),"");assert_eq!(text(&f,"history_row_0_title"),"");assert!(click(&mut c,&o,"history_next").requests.is_empty());
}
