use crate::settings_app::SettingsSnapshot;
use crate::settings_script::{Controller,Frame,Patch,bundled_source,bundled_widgets};
use crate::settings_script_bridge::{observation,basic_request};
use makepad_strict_json::{obj,s,Value};
fn new()->Controller{Controller::new(&bundled_source(),bundled_widgets()).unwrap()}
fn step(c:&mut Controller,o:&SettingsSnapshot,kind:&str,id:&str,value:Value)->Frame{
 c.step(&obj(vec![("kind",s(kind)),("id",s(id)),("value",value)]),&observation(o,false).unwrap(),|f|{
  crate::settings_script_host_facade::HostFacade::decode(&f.state,o)?;
  for r in &f.requests{if basic_request(r,o).is_none(){return Err(format!("Rejected {kind} {id}: {r:?}"));}}Ok(())
 }).unwrap_or_else(|error|panic!("{kind} {id}: {error}")).0
}
fn click(c:&mut Controller,o:&SettingsSnapshot,id:&str)->Frame{step(c,o,"click",id,Value::Null)}
fn page(f:&Frame)->&str{f.state.get("page").and_then(Value::as_str).unwrap()}
fn text<'a>(f:&'a Frame,id:&str)->&'a str{f.patches.iter().rev().find_map(|p|match p{Patch::Text(k,v)if k==id=>Some(v.as_str()),_=>None}).unwrap_or_else(||panic!("missing {id}"))}
fn dnd()->SettingsSnapshot{SettingsSnapshot{android:true,dnd_settings:crate::settings_dnd::DndSnapshot::decode(&crate::settings_dnd::tests::snapshot(1)),controls:crate::settings_controls::ControlsSnapshot::decode(&crate::settings_controls::tests::snapshot(1,crate::settings_controls::ControlsPage::Notifications)),..Default::default()}}
#[test]fn script_dnd_manual_and_policy_use_current_native_choices(){
 let mut c=new();let mut o=dnd();step(&mut c,&o,"navigate","dnd",Value::Null);
 assert!(click(&mut c,&o,"dnd_manual_off").requests.is_empty());let f=click(&mut c,&o,"dnd_manual_priority");assert_eq!(f.requests[0].get("value").and_then(Value::as_str),Some("priority"));
 click(&mut c,&o,"dnd_policy_open");let f=click(&mut c,&o,"dnd_policy_0_2");assert_eq!(f.requests[0].get("value").and_then(Value::as_str),Some("Starred"));
 step(&mut c,&o,"press","dnd_policy_0_1",Value::Null);o.dnd_settings.as_mut().unwrap().clear_actions();assert!(click(&mut c,&o,"dnd_policy_0_1").requests.is_empty());
}
#[test]fn script_dnd_schedule_keeps_failed_draft_and_only_matching_success_closes_editor(){
 let mut c=new();let mut o=dnd();step(&mut c,&o,"navigate","dnd",Value::Null);click(&mut c,&o,"dnd_rules_open");click(&mut c,&o,"dnd_row_0");let f=click(&mut c,&o,"dnd_edit");assert_eq!(page(&f),"DndEditor");
 step(&mut c,&o,"input","dnd_name",s("Night 中文"));let f=click(&mut c,&o,"dnd_save");assert_eq!(f.requests[0].get("schedule").unwrap().get("start"),Some(&Value::Int(1320)));
 let result=|applied|obj(vec![("domain",s("dnd")),("phase",s("completed")),("applied",Value::Bool(applied)),("pending",Value::Bool(false)),("message",s(""))]);
 let f=step(&mut c,&o,"result","",result(false));assert_eq!(page(&f),"DndEditor");assert_eq!(f.state.get("dnd_ui").unwrap().get("draft").unwrap().get("name").and_then(Value::as_str),Some("Night 中文"));
 click(&mut c,&o,"dnd_save");let mut raw=crate::settings_dnd::tests::snapshot(2);crate::settings_updates::tests::field(&mut raw,"key",s(format!("{:064x}",25)));o.dnd_settings=crate::settings_dnd::DndSnapshot::decode(&raw);
 let f=step(&mut c,&o,"result","",result(true));assert_eq!(page(&f),"DndRules");
}
#[test]fn script_dnd_delete_is_reviewed_and_cannot_follow_a_new_native_key(){
 let mut c=new();let mut o=dnd();step(&mut c,&o,"navigate","dnd",Value::Null);click(&mut c,&o,"dnd_rules_open");click(&mut c,&o,"dnd_row_0");assert!(click(&mut c,&o,"dnd_delete").requests.is_empty());
 o.dnd_settings.as_mut().unwrap().clear_actions();assert!(click(&mut c,&o,"dnd_delete_confirm").requests.is_empty());
}
#[test]fn script_accounts_selection_sync_and_provider_flows_are_bound_to_observed_targets(){
 let mut c=new();let mut o=SettingsSnapshot{android:true,accounts:crate::settings_accounts::AccountsSnapshot::decode(&crate::settings_accounts::tests::snapshot(1,25)),account_details:crate::settings_accounts::AccountDetails::decode(&crate::settings_accounts::tests::details(2,0,2)),..Default::default()};
 let f=step(&mut c,&o,"navigate","accounts",Value::Null);assert!(text(&f,"accounts_visibility").contains("Only accounts visible"));
 step(&mut c,&o,"press","account_row_0",Value::Null);o.accounts.as_mut().unwrap().accounts.swap(0,1);let f=click(&mut c,&o,"account_row_0");assert_eq!(page(&f),"Accounts");o.accounts.as_mut().unwrap().accounts.swap(0,1);
 click(&mut c,&o,"account_row_0");let f=click(&mut c,&o,"account_remove");assert_eq!(f.requests[0].get("operation").and_then(Value::as_str),Some("Remove"));
 click(&mut c,&o,"authority_row_0");let f=click(&mut c,&o,"account_sync_now");assert_eq!(f.requests[0].get("operation").and_then(Value::as_str),Some("SyncNow"));
 o.account_details.as_mut().unwrap().clear_actions();assert!(click(&mut c,&o,"account_sync_now").requests.is_empty());
 step(&mut c,&o,"navigate","accounts",Value::Null);click(&mut c,&o,"accounts_add");let f=click(&mut c,&o,"provider_row_0");assert_eq!(f.requests[0].get("operation").and_then(Value::as_str),Some("Add"));
}
fn notifications()->SettingsSnapshot{
 let mut o=SettingsSnapshot{android:true,app_notifications:crate::settings_app_notifications::AppNotificationsSnapshot::decode(&crate::settings_app_notifications::tests::snapshot(1,0,25)),apps_catalog:crate::settings_apps::AppsCatalog::decode(&crate::settings_apps::tests::catalog(0,1,"")),app_details:crate::settings_apps::AppDetails::decode(&crate::settings_apps::tests::detail("com.example.test",0,0)),..Default::default()};
 o.apps_catalog.as_mut().unwrap().apps[0].target=crate::settings_apps::AppTarget::decode(&s("com.example.test")).unwrap();o
}
#[test]fn script_notification_channel_change_needs_review_and_cannot_retarget_a_held_row(){
 let mut c=new();let mut o=notifications();step(&mut c,&o,"navigate","apps",Value::Null);click(&mut c,&o,"app_row_0");click(&mut c,&o,"app_notifications");
 step(&mut c,&o,"press","an_row_0",Value::Null);o.app_notifications.as_mut().unwrap().rows.swap(0,1);let f=click(&mut c,&o,"an_row_0");assert_eq!(page(&f),"AppNotifications");
 click(&mut c,&o,"an_row_0");assert!(click(&mut c,&o,"nc_importance_high").requests.is_empty());let f=click(&mut c,&o,"nc_save");assert_eq!(f.requests[0].get("value").and_then(Value::as_str),Some("High"));
 o.app_notifications.as_mut().unwrap().clear_actions();assert!(click(&mut c,&o,"nc_save").requests.is_empty());
}
#[test]fn script_notification_external_entry_requires_correlated_lookup_and_back_cancels_adoption(){
 let mut c=new();let o=notifications();let entry=|id:&str|obj(vec![("entry_id",s(id)),("package",s("com.example.test"))]);
 let f=step(&mut c,&o,"entry","app_notifications",entry("9223372036854775806"));assert_eq!(f.requests[0].get("domain").and_then(Value::as_str),Some("app_entry"));
 let f=step(&mut c,&o,"entry_resolved","app_notifications",entry("1"));assert!(!f.handled);assert!(f.state.get("context").unwrap().get("app_target").unwrap().is_null());
 let f=step(&mut c,&o,"entry_resolved","app_notifications",entry("9223372036854775806"));assert!(f.handled);assert_eq!(f.requests[0].get("domain").and_then(Value::as_str),Some("app_notifications"));
 step(&mut c,&o,"entry","app_notifications",entry("2"));step(&mut c,&o,"back","",Value::Null);let f=step(&mut c,&o,"entry_resolved","app_notifications",entry("2"));assert!(!f.handled);assert_eq!(page(&f),"Apps");
}
