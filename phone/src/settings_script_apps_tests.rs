use crate::settings_app::{SettingsSnapshot, SettingsRequest};
use crate::settings_apps::{AppsCatalog,AppDetails};
use crate::settings_script::{Controller,Frame,Patch,bundled_source,bundled_widgets};
use crate::settings_script_bridge::{observation,basic_request};
use makepad_strict_json::{obj,s,Value};
fn new()->Controller{Controller::new(&bundled_source(),bundled_widgets()).unwrap()}
fn step(c:&mut Controller,o:&SettingsSnapshot,kind:&str,id:&str,value:Value)->Frame{
 c.step(&obj(vec![("kind",s(kind)),("id",s(id)),("value",value)]),&observation(o,false).unwrap(),|f|{
  for r in &f.requests { if basic_request(r,o).is_none(){return Err(format!("Rejected test binding: {r:?}"));} } Ok(())
 }).unwrap().0
}
fn click(c:&mut Controller,o:&SettingsSnapshot,id:&str)->Frame{step(c,o,"click",id,Value::Null)}
fn text<'a>(f:&'a Frame,id:&str)->&'a str{f.patches.iter().rev().find_map(|p|match p{Patch::Text(k,v)if k==id=>Some(v.as_str()),_=>None}).unwrap()}
fn observe(c:&mut Controller,o:&SettingsSnapshot)->Frame{step(c,o,"observe","",Value::Null)}
fn native()->SettingsSnapshot{SettingsSnapshot{android:true,apps_catalog:AppsCatalog::decode(&crate::settings_apps::tests::catalog(0,40,"")),app_details:AppDetails::decode(&crate::settings_apps::tests::detail("com.example.app0",0,40)),..Default::default()}}
fn open(c:&mut Controller,o:&SettingsSnapshot){step(c,o,"navigate","apps",Value::Null);click(c,o,"app_row_0");}
#[test]fn script_apps_retain_filter_draft_and_detail_back_without_retargeting_held_rows(){
 let mut c=new();let mut o=native();step(&mut c,&o,"navigate","apps",Value::Null);
 step(&mut c,&o,"press","app_row_0",Value::Null);
 o.apps_catalog.as_mut().unwrap().apps.swap(0,1);observe(&mut c,&o);
 assert!(click(&mut c,&o,"app_row_0").requests.is_empty());
 assert_eq!(c.state().get("page").and_then(Value::as_str),Some("Apps"));
 o.apps_catalog.as_mut().unwrap().apps.swap(0,1);observe(&mut c,&o);
 step(&mut c,&o,"input","apps_query",s("邮件' {query}"));
 let f=click(&mut c,&o,"app_row_0");assert!(text(&f,"app_identity").contains("com.example.app0"));
 let f=step(&mut c,&o,"back","",Value::Null);
 assert_eq!(f.state.get("apps").unwrap().get("query").and_then(Value::as_str),Some("邮件' {query}"));
 assert!(!f.patches.iter().any(|p|matches!(p,Patch::Input(id,_)if id=="apps_query")));
 let f=click(&mut c,&o,"apps_search");assert_eq!(f.requests[0].get("query").and_then(Value::as_str),Some("邮件' {query}"));
 assert!(text(&f,"apps_summary").contains("unavailable or loading"));
}
#[test]fn script_apps_stale_catalog_returns_first_page_without_discarding_query(){
 let mut c=new();let mut o=native();step(&mut c,&o,"navigate","apps",Value::Null);observe(&mut c,&o);
 let f=click(&mut c,&o,"apps_next");assert_eq!(f.requests[0].get("offset"),Some(&Value::Int(20)));
 let catalog=o.apps_catalog.as_mut().unwrap();catalog.stale=true;catalog.request_id+=1;
 let f=observe(&mut c,&o);assert_eq!(f.state.get("apps").unwrap().get("offset"),Some(&Value::Int(0)));
 assert_eq!(text(&f,"apps_notice"),"Installed apps changed. Showing the first page.");
}
#[test]fn script_apps_permission_pages_show_unknowns_and_only_offer_observed_actions(){
 let mut c=new();let o=native();open(&mut c,&o);
 let f=click(&mut c,&o,"permissions_next");assert!(matches!(basic_request(&f.requests[0],&o),Some(SettingsRequest::AppDetails{permission_offset:20,..})));
 click(&mut c,&o,"permissions_first");
 let f=observe(&mut c,&o);assert!(text(&f,"app_storage").contains("Unavailable"));
 assert!(click(&mut c,&o,"app_uninstall").requests.is_empty());
 assert!(matches!(basic_request(&click(&mut c,&o,"app_android_info").requests[0],&o),Some(SettingsRequest::AppAction{..})));
}
fn policy_native(domain:&str)->SettingsSnapshot{
 let mut o=native();let raw=match domain{
 "battery"=>crate::settings_app_battery::tests::snapshot(1),"storage"=>crate::settings_app_storage::tests::snapshot(1),_=>crate::settings_app_network::tests::snapshot(1)};
 let target=raw.get("package").unwrap().as_str().unwrap();
 o.apps_catalog.as_mut().unwrap().apps[0].target=crate::settings_apps::AppTarget::decode(&s(target)).unwrap();
 o.app_details=AppDetails::decode(&crate::settings_apps::tests::detail(target,0,0));
 match domain{"battery"=>o.app_battery=crate::settings_app_battery::AppBatterySnapshot::decode(&raw),"storage"=>o.app_storage=crate::settings_app_storage::AppStorageSnapshot::decode(&raw),_=>o.app_network=crate::settings_app_network::AppNetworkSnapshot::decode(&raw)}
 o
}
#[test]fn script_battery_review_cancel_stale_identity_and_exact_native_binding(){
 let mut c=new();let mut o=policy_native("battery");open(&mut c,&o);click(&mut c,&o,"app_battery_open");
 assert!(click(&mut c,&o,"app_battery_restricted").requests.is_empty());
 click(&mut c,&o,"app_battery_cancel");assert!(click(&mut c,&o,"app_battery_confirm").requests.is_empty());
 click(&mut c,&o,"app_battery_restricted");
 o.app_battery.as_mut().unwrap().clear_actions();observe(&mut c,&o);assert!(click(&mut c,&o,"app_battery_confirm").requests.is_empty());
 o=policy_native("battery");observe(&mut c,&o);let f=click(&mut c,&o,"app_battery_confirm");assert_eq!(f.requests.len(),1);
 assert_eq!(f.requests[0].get("choice").and_then(Value::as_str),Some("Restricted"));
 assert_eq!(text(&f,"app_battery_mode"),"Observed policy: Optimized");
}
#[test]fn script_storage_requires_review_and_keeps_sizes_until_native_completion(){
 let mut c=new();let o=policy_native("storage");open(&mut c,&o);click(&mut c,&o,"app_storage_open");
 let f=click(&mut c,&o,"app_storage_clear_data");assert!(f.requests.is_empty());assert!(text(&f,"app_storage_review_copy").contains("permanently"));
 click(&mut c,&o,"app_storage_cancel");assert!(click(&mut c,&o,"app_storage_confirm").requests.is_empty());
 click(&mut c,&o,"app_storage_clear_cache");let f=click(&mut c,&o,"app_storage_confirm");assert_eq!(f.requests.len(),1);
 assert!(text(&f,"app_storage_sizes").contains("Cache: 100 bytes"));
 assert_eq!(text(&f,"app_storage_operation"),"");
}
#[test]fn script_network_rechecks_held_value_and_never_writes_unsupported_controls(){
 let mut c=new();let mut o=policy_native("network");open(&mut c,&o);click(&mut c,&o,"app_network_settings");
 assert!(click(&mut c,&o,"app_network_wifi").requests.is_empty());
 step(&mut c,&o,"press","app_network_background",Value::Null);
 o.app_network.as_mut().unwrap().controls[0].value=Some(false);observe(&mut c,&o);
 assert!(click(&mut c,&o,"app_network_background").requests.is_empty());
 let f=click(&mut c,&o,"app_network_background");assert_eq!(f.requests[0].get("enabled"),Some(&Value::Bool(true)));
 o.app_network.as_mut().unwrap().clear_actions();assert!(click(&mut c,&o,"app_network_background").requests.is_empty());
}
