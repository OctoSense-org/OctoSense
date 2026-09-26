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
fn updates()->SettingsSnapshot{SettingsSnapshot{android:true,updates:crate::settings_updates::UpdatesSnapshot::decode(&crate::settings_updates::tests::snapshot(1)),..Default::default()}}
fn network()->SettingsSnapshot{SettingsSnapshot{android:true,advanced_network:crate::settings_network::NetworkSnapshot::decode(&crate::settings_network::tests::snapshot(1)),..Default::default()}}
fn display()->SettingsSnapshot{SettingsSnapshot{android:true,display_options:crate::settings_display::DisplaySnapshot::decode(&crate::settings_display::tests::snapshot()),..Default::default()}}
#[test]fn script_updates_review_is_exact_and_new_offer_cannot_inherit_confirmation(){
 let mut c=new();let mut o=updates();step(&mut c,&o,"navigate","updates",Value::Null);
 assert!(click(&mut c,&o,"updates_rom").requests.is_empty());click(&mut c,&o,"updates_cancel_review");assert!(click(&mut c,&o,"updates_confirm").requests.is_empty());
 click(&mut c,&o,"updates_rom");let mut raw=crate::settings_updates::tests::snapshot(2);let mut offer=raw.get("offer").unwrap().clone();
 crate::settings_updates::tests::field(&mut offer,"key",s("f".repeat(64)));crate::settings_updates::tests::field(&mut raw,"offer",offer);o.updates=crate::settings_updates::UpdatesSnapshot::decode(&raw);
 observe(&mut c,&o);assert!(click(&mut c,&o,"updates_confirm").requests.is_empty());
 click(&mut c,&o,"updates_home");let f=click(&mut c,&o,"updates_confirm");assert_eq!(f.requests.len(),1);assert_eq!(f.requests[0].get("part").and_then(Value::as_str),Some("Home"));
 assert_eq!(text(&f,"updates_home_state"),"Home update: Idle");
}
#[test]fn script_updates_restart_requires_separate_current_review(){
 use crate::settings_updates::tests::field;
 let mut c=new();let mut o=updates();let mut raw=crate::settings_updates::tests::snapshot(1);
 field(&mut raw,"rom",obj(vec![("phase",s("updated_need_reboot"))]));field(&mut raw,"capabilities",Value::Arr(vec![s("reboot")]));
 let Value::Obj(fields)=&mut raw else{panic!()};fields.push(("reboot_key".into(),s("a".repeat(64))));o.updates=crate::settings_updates::UpdatesSnapshot::decode(&raw);assert!(o.updates.is_some());
 step(&mut c,&o,"navigate","updates",Value::Null);
 assert!(click(&mut c,&o,"updates_confirm").requests.is_empty());assert!(click(&mut c,&o,"updates_reboot").requests.is_empty());
 let f=click(&mut c,&o,"updates_confirm");assert_eq!(f.requests[0].get("operation").and_then(Value::as_str),Some("Reboot"));
 click(&mut c,&o,"updates_reboot");o.updates.as_mut().unwrap().clear_actions();observe(&mut c,&o);assert!(click(&mut c,&o,"updates_confirm").requests.is_empty());
}
#[test]fn script_dns_retains_edits_until_explicit_save_and_invalid_hosts_never_cross_binding(){
 let mut c=new();let o=network();step(&mut c,&o,"navigate","advanced_network",Value::Null);click(&mut c,&o,"network_dns_edit");click(&mut c,&o,"network_dns_mode_hostname");
 for host in ["https://dns.example","1.1.1.1","a..b","a-.example","[::1]","dns.example:853"]{
  step(&mut c,&o,"input","network_dns_hostname",s(host));assert!(click(&mut c,&o,"network_dns_save").requests.is_empty(),"{host}");
 }
 step(&mut c,&o,"input","network_dns_hostname",s("例子.测试"));let f=observe(&mut c,&o);assert!(!f.patches.iter().any(|p|matches!(p,Patch::Input(id,_)if id=="network_dns_hostname")));
 let f=click(&mut c,&o,"network_dns_save");assert_eq!(f.requests[0].get("hostname").and_then(Value::as_str),Some("例子.测试"));
 assert!(text(&f,"network_dns_observed").contains("Automatic"));
 click(&mut c,&o,"network_dns_edit");click(&mut c,&o,"network_dns_mode_off");click(&mut c,&o,"network_dns_cancel");assert!(click(&mut c,&o,"network_dns_save").requests.is_empty());
}
#[test]fn script_dns_and_toggle_reject_changed_observations_without_retargeting(){
 let mut c=new();let mut o=network();step(&mut c,&o,"navigate","advanced_network",Value::Null);click(&mut c,&o,"network_dns_edit");
 step(&mut c,&o,"input","network_dns_hostname",s("dns.example"));click(&mut c,&o,"network_dns_mode_hostname");step(&mut c,&o,"press","network_airplane",Value::Null);
 let mut raw=crate::settings_network::tests::snapshot(2);crate::settings_updates::tests::field(&mut raw,"key",s("f".repeat(64)));o.advanced_network=crate::settings_network::NetworkSnapshot::decode(&raw);
 let f=observe(&mut c,&o);assert!(text(&f,"network_dns_note").contains("draft is kept"));assert!(click(&mut c,&o,"network_dns_save").requests.is_empty());assert!(click(&mut c,&o,"network_airplane").requests.is_empty());
}
#[test]fn script_display_review_retains_seconds_and_invalid_time_never_applies(){
 let mut c=new();let o=display();step(&mut c,&o,"navigate","display_options",Value::Null);
 let f=click(&mut c,&o,"night_start");assert!(f.requests.is_empty());assert!(f.patches.contains(&Patch::Input("night_time_input".into(),"22:00:01".into())));
 for time in ["24:00","9:00","12:60","00:00:60"," 12:00","12:00\n"] {step(&mut c,&o,"input","night_time_input",s(time));assert!(click(&mut c,&o,"display_save").requests.is_empty(),"{time}");}
 step(&mut c,&o,"input","night_time_input",s("21:59:07"));let f=observe(&mut c,&o);assert!(!f.patches.iter().any(|p|matches!(p,Patch::Input(..))));
 let f=click(&mut c,&o,"display_save");assert_eq!(f.requests[0].get("value"),Some(&Value::Int(79147)));assert!(text(&f,"night_observed").contains("22:00:01"));
 assert!(click(&mut c,&o,"night_sunset").requests.is_empty());assert!(c.state().get("display_ui").unwrap().get("draft").unwrap().is_null());
}
#[test]fn script_density_choice_and_review_fail_closed_after_changed_key(){
 let mut c=new();let mut o=display();step(&mut c,&o,"navigate","display_options",Value::Null);click(&mut c,&o,"density_choice_0");
 let mut raw=crate::settings_display::tests::snapshot();crate::settings_updates::tests::field(&mut raw,"key",s("c".repeat(64)));o.display_options=crate::settings_display::DisplaySnapshot::decode(&raw);
 let f=observe(&mut c,&o);assert!(text(&f,"display_review_note").contains("settings changed"));assert!(click(&mut c,&o,"display_save").requests.is_empty());
 click(&mut c,&o,"display_cancel");click(&mut c,&o,"density_choice_0");assert_eq!(click(&mut c,&o,"display_save").requests.len(),1);
}
fn datetime()->SettingsSnapshot{
 let raw=makepad_strict_json::parse(format!(r#"{{"schema":1,"key":"{:064x}","civil":"2026-09-25 11:12","zone":"America/Los_Angeles","auto_time":false,"auto_zone":false,"can_auto_time":true,"can_auto_zone":true,"can_clock":true,"can_zone":true,"minimum_year":1970,"maximum_year":2099}}"#,1).as_bytes()).unwrap();
 SettingsSnapshot{android:true,device:Some(crate::settings_app::DeviceSnapshot{time_controls:crate::settings_datetime::TimeSnapshot::decode(&raw),time_zones:vec!["America/Los_Angeles".into(),"Asia/Shanghai".into(),"UTC".into()],..Default::default()}),..Default::default()}
}
#[test]fn script_civil_time_validates_calendar_and_never_falls_back_to_unscoped_setter(){
 let mut c=new();let mut o=datetime();step(&mut c,&o,"navigate","date_time",Value::Null);click(&mut c,&o,"clock_edit");
 for civil in ["2100-02-29 00:00","2026-02-29 12:00","2026-04-31 12:00","2026-09-24 24:00","2026-09-24 12:60","2026-9-24 12:00","２０２６-09-24 12:00","2026-09-24T12:00","1969-12-31 12:00"]{
  step(&mut c,&o,"input","clock_civil",s(civil));assert!(click(&mut c,&o,"clock_save").requests.is_empty(),"{civil}");
 }
 step(&mut c,&o,"input","clock_civil",s("2024-02-29 23:59"));click(&mut c,&o,"clock_second");let f=click(&mut c,&o,"clock_save");assert_eq!(f.requests[0].get("second"),Some(&Value::Bool(true)));
 o.device.as_mut().unwrap().time_controls=None;o.device.as_mut().unwrap().auto_time=Some(false);o.device.as_mut().unwrap().capabilities.insert("auto_time".into());
 assert!(click(&mut c,&o,"auto_time").requests.is_empty(),"no fallback when authoritative time context is unavailable");
}
#[test]fn script_time_zone_is_selected_then_reviewed_and_invalidated_by_policy(){
 let mut c=new();let mut o=datetime();step(&mut c,&o,"navigate","date_time",Value::Null);click(&mut c,&o,"zone_open");
 step(&mut c,&o,"input","zone_query",s("Shanghai"));let f=click(&mut c,&o,"zone_row_0");assert!(f.requests.is_empty());assert!(text(&f,"zone_review_text").contains("Shanghai"));
 click(&mut c,&o,"zone_cancel");assert!(click(&mut c,&o,"zone_apply").requests.is_empty());
 click(&mut c,&o,"zone_row_0");o.device.as_mut().unwrap().time_controls.as_mut().unwrap().auto_zone=true;observe(&mut c,&o);assert!(click(&mut c,&o,"zone_apply").requests.is_empty());
 o.device.as_mut().unwrap().time_controls.as_mut().unwrap().auto_zone=false;let f=click(&mut c,&o,"zone_apply");assert_eq!(f.requests[0].get("zone").and_then(Value::as_str),Some("Asia/Shanghai"));
}
