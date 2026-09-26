use crate::settings_app::SettingsSnapshot;
use crate::settings_script::{Controller,Frame,Patch,bundled_source,bundled_widgets};
use crate::settings_script_bridge::{observation,basic_request};
use makepad_strict_json::{obj,s,Value};
fn new()->Controller{Controller::new(&bundled_source(),bundled_widgets()).unwrap()}
fn step(c:&mut Controller,o:&SettingsSnapshot,kind:&str,id:&str,value:Value)->Frame{
 c.step(&obj(vec![("kind",s(kind)),("id",s(id)),("value",value)]),&observation(o,false).unwrap(),|f|{
  for r in &f.requests {if basic_request(r,o).is_none(){return Err(format!("Rejected test binding: {r:?}"));}}Ok(())
 }).unwrap().0
}
fn click(c:&mut Controller,o:&SettingsSnapshot,id:&str)->Frame{step(c,o,"click",id,Value::Null)}
fn observe(c:&mut Controller,o:&SettingsSnapshot)->Frame{step(c,o,"observe","",Value::Null)}
fn text<'a>(f:&'a Frame,id:&str)->&'a str{f.patches.iter().rev().find_map(|p|match p{Patch::Text(k,v)if k==id=>Some(v.as_str()),_=>None}).unwrap()}
fn wifi()->SettingsSnapshot{SettingsSnapshot{android:true,wifi:crate::settings_wifi::WifiSnapshot::decode(&crate::settings_wifi::tests::snapshot(1,40)),..Default::default()}}
fn bluetooth()->SettingsSnapshot{SettingsSnapshot{android:true,bluetooth:crate::settings_bluetooth::BluetoothSnapshot::decode(&crate::settings_bluetooth::tests::snapshot(1,40)),..Default::default()}}
#[test]fn script_wifi_row_identity_and_review_survive_only_current_authority(){
 let mut c=new();let mut o=wifi();step(&mut c,&o,"navigate","wifi",Value::Null);step(&mut c,&o,"press","wifi_row_0",Value::Null);
 o.wifi.as_mut().unwrap().networks.swap(0,1);observe(&mut c,&o);assert!(click(&mut c,&o,"wifi_row_0").requests.is_empty());assert_eq!(c.state().get("page").and_then(Value::as_str),Some("Wifi"));
 click(&mut c,&o,"wifi_row_0");assert!(click(&mut c,&o,"wifi_forget_confirm").requests.is_empty());click(&mut c,&o,"wifi_forget");click(&mut c,&o,"wifi_forget_cancel");assert!(click(&mut c,&o,"wifi_forget_confirm").requests.is_empty());
 click(&mut c,&o,"wifi_forget");let f=click(&mut c,&o,"wifi_forget_confirm");assert_eq!(f.requests.len(),1);assert_eq!(f.requests[0].get("target").and_then(Value::as_str),Some(o.wifi.as_ref().unwrap().networks[0].target.key()));
 click(&mut c,&o,"wifi_forget");o.wifi.as_mut().unwrap().clear_actions();observe(&mut c,&o);assert!(click(&mut c,&o,"wifi_forget_confirm").requests.is_empty());
}
#[test]fn script_wifi_inventory_paging_resets_when_observed_list_shrinks(){
 let mut c=new();let mut o=wifi();step(&mut c,&o,"navigate","wifi",Value::Null);click(&mut c,&o,"wifi_saved_tab");click(&mut c,&o,"wifi_next");
 o.wifi=crate::settings_wifi::WifiSnapshot::decode(&crate::settings_wifi::tests::snapshot(2,1));let f=observe(&mut c,&o);
 assert_eq!(f.state.get("wifi_ui").unwrap().get("offset"),Some(&Value::Int(0)));assert!(text(&f,"wifi_list_summary").contains("1–1"));
}
#[test]fn script_bluetooth_name_draft_is_preserved_through_observation_and_detail_back(){
 let mut c=new();let mut o=bluetooth();step(&mut c,&o,"navigate","bluetooth",Value::Null);step(&mut c,&o,"input","bt_name",s("中文 phone"));
 o.bluetooth.as_mut().unwrap().adapter.name=Some("Other observed name".into());let f=observe(&mut c,&o);assert_eq!(text(&f,"bt_name_note"),"Unsaved name");assert!(!f.patches.iter().any(|p|matches!(p,Patch::Input(id,_)if id=="bt_name")));
 click(&mut c,&o,"bt_row_0");step(&mut c,&o,"back","",Value::Null);let f=click(&mut c,&o,"bt_name_save");assert_eq!(f.requests[0].get("name").and_then(Value::as_str),Some("中文 phone"));
 assert_eq!(text(&f,"bt_name_observed"),"Current name: Other observed name");
 click(&mut c,&o,"bt_name_cancel");assert!(click(&mut c,&o,"bt_name_save").requests.is_empty());
 for name in ["", "line\nbreak"]{step(&mut c,&o,"input","bt_name",s(name));assert!(click(&mut c,&o,"bt_name_save").requests.is_empty());}
}
#[test]fn script_bluetooth_sharing_and_forget_use_selected_device_and_revoked_options(){
 let mut c=new();let mut o=bluetooth();step(&mut c,&o,"navigate","bluetooth",Value::Null);click(&mut c,&o,"bt_row_0");
 assert!(click(&mut c,&o,"bt_phonebook_ask").requests.is_empty());let f=click(&mut c,&o,"bt_phonebook_allow");assert_eq!(f.requests[0].get("sharing").and_then(Value::as_str),Some("Phonebook"));assert_eq!(text(&f,"bt_phonebook_value"),"Contact sharing: Ask");
 assert!(click(&mut c,&o,"bt_forget_confirm").requests.is_empty());click(&mut c,&o,"bt_forget");assert_eq!(click(&mut c,&o,"bt_forget_confirm").requests.len(),1);
 click(&mut c,&o,"bt_forget");o.bluetooth.as_mut().unwrap().clear_actions();observe(&mut c,&o);assert!(click(&mut c,&o,"bt_forget_confirm").requests.is_empty());assert!(click(&mut c,&o,"bt_messages_allow").requests.is_empty());
}
