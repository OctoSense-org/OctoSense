//! Script transitions validated by the same finite request bindings as the host.
use crate::settings_app::{SettingsSnapshot,SettingsRequest};
use crate::settings_apps::{AppsCatalog,AppDetails,AppTarget};
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
fn observe(c:&mut Controller,o:&SettingsSnapshot)->Frame{step(c,o,"observe","",Value::Null)}
fn text<'a>(f:&'a Frame,id:&str)->&'a str{f.patches.iter().rev().find_map(|p|match p{Patch::Text(k,v)if k==id=>Some(v.as_str()),_=>None}).unwrap_or_else(||panic!("missing text {id}"))}
fn enabled(f:&Frame,id:&str)->bool{f.patches.iter().rev().find_map(|p|match p{Patch::Enabled(k,v)if k==id=>Some(*v),_=>None}).unwrap_or_else(||panic!("missing enabled {id}"))}
fn native()->SettingsSnapshot{SettingsSnapshot{android:true,..Default::default()}}
fn with_app(o:&mut SettingsSnapshot,package:&str){
 o.apps_catalog=AppsCatalog::decode(&crate::settings_apps::tests::catalog(0,1,""));
 o.apps_catalog.as_mut().unwrap().apps[0].target=AppTarget::decode(&s(package)).unwrap();
 o.app_details=AppDetails::decode(&crate::settings_apps::tests::detail(package,0,0));
}
fn open_app(c:&mut Controller,o:&SettingsSnapshot){step(c,o,"navigate","apps",Value::Null);click(c,o,"app_row_0");}
fn submitted(c:&mut Controller,o:&SettingsSnapshot,domain:&str)->Frame{step(c,o,"result","",obj(vec![("domain",s(domain)),("phase",s("submitted")),("pending",Value::Bool(false)),("message",s("Requested"))]))}

#[test]
fn script_consent_unavailable_subscriptions_remain_finite_and_retire_on_navigation(){
 use crate::settings_script_host_facade::HostFacade;
 let mut c=new();let mut o=native();
 for(route,domain)in [("default_apps","roles"),("caption_language","caption_language"),("system_languages","system_languages"),("keyboards","keyboards")]{
  step(&mut c,&o,"navigate",route,Value::Null);
  let f=observe(&mut c,&o);let facade=HostFacade::decode(&f.state,&o).unwrap();assert!(facade.active(domain),"{domain} must keep its bounded read while unavailable");
  step(&mut c,&o,"navigate","overview",Value::Null);assert!(!HostFacade::decode(c.state(),&o).unwrap().active(domain));
 }
 with_app(&mut o,"fixture.permissions");
 for(button,domain)in [("app_permission_settings","permissions"),("app_language_open","app_language")]{
  open_app(&mut c,&o);click(&mut c,&o,button);o.app_details=None;
  let f=observe(&mut c,&o);let facade=HostFacade::decode(&f.state,&o).unwrap();assert!(facade.active(domain));assert!(facade.apps_read().is_some());
  assert!(f.requests.is_empty(),"poll metadata is not an effect or automatic mutation retry");
  step(&mut c,&o,"navigate","overview",Value::Null);let facade=HostFacade::decode(c.state(),&o).unwrap();assert!(!facade.active(domain));assert!(facade.app_target().is_none());
  with_app(&mut o,"fixture.permissions");
 }
}

#[test]fn script_roles_native_none_paging_stale_press_and_back_use_current_observed_keys(){
 use crate::settings_roles::{RolesSnapshot,RolesRequest};
 let mut c=new();let mut o=native();o.roles=RolesSnapshot::decode(&crate::settings_roles::tests::snapshot(1,0,25));o.roles.as_mut().unwrap().role=None;
 step(&mut c,&o,"navigate","default_apps",Value::Null);click(&mut c,&o,"role_overview_0");
 o.roles=RolesSnapshot::decode(&crate::settings_roles::tests::snapshot(2,0,25));observe(&mut c,&o);
 assert!(!enabled(&observe(&mut c,&o),"role_row_0"));
 step(&mut c,&o,"press","role_row_1",Value::Null);o.roles.as_mut().unwrap().candidates.swap(1,2);observe(&mut c,&o);
 assert!(click(&mut c,&o,"role_row_1").requests.is_empty());
 let f=click(&mut c,&o,"role_none");assert!(matches!(basic_request(&f.requests[0],&o),Some(SettingsRequest::Roles(RolesRequest::Confirm{..}))));
 let f=click(&mut c,&o,"role_next");assert_eq!(f.requests[0].get("offset"),Some(&Value::Int(20)));
 o.roles=RolesSnapshot::decode(&crate::settings_roles::tests::snapshot(3,0,25));o.roles.as_mut().unwrap().stale=true;
 let f=observe(&mut c,&o);assert_eq!(text(&f,"role_notice"),"Available apps changed. Showing the first page.");
 let f=step(&mut c,&o,"back","",Value::Null);assert_eq!(f.state.get("page").and_then(Value::as_str),Some("DefaultApps"));
 assert_eq!(f.requests[0].get("role"),Some(&Value::Null));
}
#[test]fn script_permission_choices_preserve_selected_observation_and_reject_retired_capabilities(){
 use crate::settings_permissions::{PermissionsSnapshot,PermissionsRequest};
 let mut c=new();let mut o=native();with_app(&mut o,"fixture.permissions");o.permissions=PermissionsSnapshot::decode(&crate::settings_permissions::tests::snapshot(1,false));
 open_app(&mut c,&o);click(&mut c,&o,"app_permission_settings");click(&mut c,&o,"permission_group_0");
 o.permissions=PermissionsSnapshot::decode(&crate::settings_permissions::tests::snapshot(2,true));let f=observe(&mut c,&o);
 assert_eq!(text(&f,"title"),"Camera permissions");assert_eq!(text(&f,"permission_choice_1"),"Don't allow · Selected");
 let f=click(&mut c,&o,"permission_choice_0");assert!(matches!(basic_request(&f.requests[0],&o),Some(SettingsRequest::Permissions(PermissionsRequest::Choose{..}))));
 assert_eq!(text(&f,"permission_choice_1"),"Don't allow · Selected");
 step(&mut c,&o,"press","permission_choice_0",Value::Null);o.permissions.as_mut().unwrap().clear_actions();let f=observe(&mut c,&o);assert!(!enabled(&f,"permission_choice_0"));
 assert!(click(&mut c,&o,"permission_choice_0").requests.is_empty());
 let f=step(&mut c,&o,"back","",Value::Null);assert_eq!(f.requests[0].get("group"),Some(&Value::Null));
}
#[test]fn script_caption_language_waits_for_new_native_readback_and_preserves_custom_filter(){
 use crate::settings_caption_language::CaptionLanguageSnapshot;
 let mut c=new();let mut o=native();o.caption_language=CaptionLanguageSnapshot::decode(&crate::settings_caption_language::tests::snapshot(1));
 let f=step(&mut c,&o,"navigate","caption_language",Value::Null);assert!(!enabled(&f,"caption_language_row_0"));
 o.caption_language.as_mut().unwrap().request_id=2;let f=observe(&mut c,&o);assert!(text(&f,"caption_language_current").contains("fr_FR"));
 step(&mut c,&o,"input","caption_language_query",s("unsubmitted 中文"));
 let f=click(&mut c,&o,"caption_language_row_2");assert_eq!(f.requests[0].get("kind").and_then(Value::as_str),Some("caption_language"));
 let f=submitted(&mut c,&o,"caption_language");assert!(!enabled(&f,"caption_language_row_2"));
 let f=observe(&mut c,&o);assert!(!enabled(&f,"caption_language_row_2"));
 o.caption_language.as_mut().unwrap().request_id=3;let f=observe(&mut c,&o);assert!(enabled(&f,"caption_language_row_2"));
 assert_eq!(f.state.get("caption_language_ui").unwrap().get("draft").and_then(Value::as_str),Some("unsubmitted 中文"));
 step(&mut c,&o,"input","caption_language_query",s("😀".repeat(41)));assert!(click(&mut c,&o,"caption_language_search").requests.is_empty());
 let f=step(&mut c,&o,"back","",Value::Null);assert_eq!(f.state.get("controls_section").and_then(Value::as_str),Some("AccessibilityHearing"));
}
#[test]fn script_app_language_hierarchy_preserves_unsent_filter_and_full_custom_current_list(){
 use crate::settings_app_language::{AppLanguageSnapshot,LanguageLevel};
 let mut c=new();let mut o=native();with_app(&mut o,"fixture.language");o.app_language=AppLanguageSnapshot::decode(&crate::settings_app_language::tests::snapshot(1));
 open_app(&mut c,&o);click(&mut c,&o,"app_language_open");o.app_language.as_mut().unwrap().request_id=2;let f=observe(&mut c,&o);
 assert!(text(&f,"app_language_current").contains("zh-Hant-TW"));
 step(&mut c,&o,"input","app_language_query",s("unsubmitted Français"));
 let f=click(&mut c,&o,"app_language_row_1");let parent=f.requests[0].get("parent").unwrap().clone();
 let original=o.app_language.as_ref().unwrap().clone();let row=o.app_language.as_mut().unwrap();row.parent=Some(crate::settings_app_language::LanguageKey::decode(&parent).unwrap());row.parent_label=Some("French".into());row.level=LanguageLevel::Region;row.request_id=3;
 observe(&mut c,&o);let f=step(&mut c,&o,"back","",Value::Null);assert_eq!(f.requests[0].get("parent"),Some(&Value::Null));
 assert!(f.patches.iter().any(|p|matches!(p,Patch::Input(id,value)if id=="app_language_query"&&value=="unsubmitted Français")));
 o.app_language=Some(original);o.app_language.as_mut().unwrap().request_id=4;observe(&mut c,&o);
 let f=click(&mut c,&o,"app_language_row_0");assert_eq!(f.requests[0].get("kind").and_then(Value::as_str),Some("app_language"));
 assert!(text(&f,"app_language_current").contains("fr-CA"));
}
#[test]fn script_system_language_review_stale_draft_confirmation_and_unicode_tags_are_preserved(){
 use crate::settings_system_language::SystemLanguageSnapshot;
 let mut c=new();let mut o=native();o.system_language=SystemLanguageSnapshot::decode(&crate::settings_system_language::tests::snapshot(1));
 step(&mut c,&o,"navigate","system_languages",Value::Null);o.system_language.as_mut().unwrap().request_id=2;observe(&mut c,&o);
 assert!(click(&mut c,&o,"system_language_up_1").requests.is_empty());
 let f=click(&mut c,&o,"system_language_review");assert!(text(&f,"system_language_review_text").contains("zz-Latn-ZZ-u-nu-latn"));assert!(enabled(&f,"system_language_apply"));
 o.system_language.as_mut().unwrap().can_apply=false;observe(&mut c,&o);assert!(click(&mut c,&o,"system_language_apply").requests.is_empty());
 o.system_language.as_mut().unwrap().can_apply=true;observe(&mut c,&o);let f=click(&mut c,&o,"system_language_apply");assert_eq!(f.requests[0].get("kind").and_then(Value::as_str),Some("system_languages"));
 let f=submitted(&mut c,&o,"system_languages");assert!(!enabled(&f,"system_language_apply"));assert!(text(&f,"system_language_notice").contains("requested"));
 o.system_language.as_mut().unwrap().current.as_mut().unwrap().reverse();o.system_language.as_mut().unwrap().request_id=3;
 let f=observe(&mut c,&o);assert!(text(&f,"system_language_notice").contains("match your reviewed order"));assert_eq!(f.state.get("system_language_ui").unwrap().get("draft"),Some(&Value::Null));
}
#[test]fn script_system_language_branch_back_cancel_and_last_language_are_local_decisions(){
 use crate::settings_system_language::SystemLanguageSnapshot;
 let mut c=new();let mut o=native();o.system_language=SystemLanguageSnapshot::decode(&crate::settings_system_language::tests::snapshot(1));
 step(&mut c,&o,"navigate","system_languages",Value::Null);o.system_language.as_mut().unwrap().request_id=2;observe(&mut c,&o);
 click(&mut c,&o,"system_language_remove_1");assert!(!enabled(&observe(&mut c,&o),"system_language_remove_0"));
 click(&mut c,&o,"system_language_add");o.system_language.as_mut().unwrap().request_id=3;observe(&mut c,&o);
 step(&mut c,&o,"input","system_language_query",s("unsubmitted العربية"));click(&mut c,&o,"system_language_row_0");
 let f=step(&mut c,&o,"back","",Value::Null);assert!(f.patches.iter().any(|p|matches!(p,Patch::Input(id,value)if id=="system_language_query"&&value=="unsubmitted العربية")));
 let f=click(&mut c,&o,"system_language_cancel");assert_eq!(f.state.get("system_language_ui").unwrap().get("draft"),Some(&Value::Null));assert_eq!(f.requests[0].get("key"),Some(&Value::Null));
}
#[test]fn script_keyboard_flows_bind_provider_incarnation_and_default_picker_has_no_target(){
 use crate::settings_keyboards::KeyboardSnapshot;
 let mut c=new();let mut o=native();o.keyboards=KeyboardSnapshot::decode(&crate::settings_keyboards::tests::snapshot(1));
 step(&mut c,&o,"navigate","keyboards",Value::Null);o.keyboards.as_mut().unwrap().request_id=2;observe(&mut c,&o);
 let f=click(&mut c,&o,"keyboard_choose_default");assert_eq!(f.requests[0].get("target"),Some(&Value::Null));
 click(&mut c,&o,"keyboard_row_1");step(&mut c,&o,"press","keyboard_enable",Value::Null);
 o.keyboards.as_mut().unwrap().rows[1].target=crate::settings_keyboards::KeyboardKey::decode(&s("f".repeat(64))).unwrap();o.keyboards.as_mut().unwrap().request_id=3;
 observe(&mut c,&o);assert!(click(&mut c,&o,"keyboard_enable").requests.is_empty());
 click(&mut c,&o,"keyboard_detail_refresh");o.keyboards.as_mut().unwrap().request_id=4;observe(&mut c,&o);
 let f=click(&mut c,&o,"keyboard_enable");assert_eq!(f.requests[0].get("operation").and_then(Value::as_str),Some("enable"));assert!(text(&f,"keyboard_state").contains("Enabled: Off"));
 let f=submitted(&mut c,&o,"keyboards");assert!(!enabled(&f,"keyboard_enable"));
 let row=&mut o.keyboards.as_mut().unwrap().rows[1];row.enabled=true;row.can_enable=false;row.can_disable=true;o.keyboards.as_mut().unwrap().request_id=5;
 let f=observe(&mut c,&o);assert!(text(&f,"keyboard_state").contains("Enabled: On"));assert!(enabled(&f,"keyboard_disable"));
 step(&mut c,&o,"back","",Value::Null);assert_eq!(c.state().get("page").and_then(Value::as_str),Some("Keyboards"));
}
#[test]fn script_consent_bindings_reject_extra_fields_and_stale_native_authority(){
 use crate::settings_caption_language::CaptionLanguageSnapshot;
 let mut o=native();o.caption_language=CaptionLanguageSnapshot::decode(&crate::settings_caption_language::tests::snapshot(1));
 let state=o.caption_language.as_ref().unwrap();let request=obj(vec![("kind",s("caption_language")),("key",s(state.key.as_ref().unwrap().wire())),("choice",s(state.rows[0].choice.wire()))]);
 assert!(basic_request(&request,&o).is_some());let mut extra=request.clone();crate::settings_apps::tests::replace(&mut extra,"value",s("arbitrary"));assert!(basic_request(&extra,&o).is_none());
 o.caption_language.as_mut().unwrap().clear_actions();assert!(basic_request(&request,&o).is_none());
 let read=obj(vec![("kind",s("read")),("domain",s("caption_language")),("key",Value::Null),("query",s("😀".repeat(41))),("offset",Value::Int(0))]);assert!(basic_request(&read,&o).is_none());
}

#[test]fn script_consent_bridge_rechecks_every_domain_instead_of_trusting_controller_choices(){
 use crate::settings_roles::RolesSnapshot;
 use crate::settings_permissions::PermissionsSnapshot;
 use crate::settings_app_language::AppLanguageSnapshot;
 use crate::settings_keyboards::KeyboardSnapshot;
 use crate::settings_system_language::SystemLanguageSnapshot;
 let replace=crate::settings_apps::tests::replace;
 let mut o=native();o.roles=RolesSnapshot::decode(&crate::settings_roles::tests::snapshot(1,0,2));
 let role=o.roles.as_ref().unwrap();let mut request=obj(vec![("kind",s("roles")),("role",s("browser")),("key",s(role.key.as_ref().unwrap().wire())),("target",s(role.candidates[1].target.wire()))]);
 assert!(basic_request(&request,&o).is_some());replace(&mut request,"role",s("home"));assert!(basic_request(&request,&o).is_none());
 replace(&mut request,"role",s("browser"));replace(&mut request,"target",s(o.roles.as_ref().unwrap().candidates[0].target.wire()));assert!(basic_request(&request,&o).is_none());
 with_app(&mut o,"fixture.permissions");o.permissions=PermissionsSnapshot::decode(&crate::settings_permissions::tests::snapshot(1,true));
 let p=o.permissions.as_ref().unwrap();let mut request=obj(vec![("kind",s("permission")),("package",s("fixture.permissions")),("group",s("camera")),("key",s(p.key.as_ref().unwrap().wire())),("target",s(p.choices[0].target.as_ref().unwrap().wire()))]);
 assert!(basic_request(&request,&o).is_some());replace(&mut request,"group",s("microphone"));assert!(basic_request(&request,&o).is_none());
 replace(&mut request,"group",s("camera"));o.app_details.as_mut().unwrap().exists=false;assert!(basic_request(&request,&o).is_none());
 with_app(&mut o,"fixture.language");o.app_language=AppLanguageSnapshot::decode(&crate::settings_app_language::tests::snapshot(1));
 let p=o.app_language.as_ref().unwrap();let mut request=obj(vec![("kind",s("app_language")),("target",s("fixture.language")),("key",s(p.key.as_ref().unwrap().wire())),("choice",s(p.rows[0].key.wire()))]);
 assert!(basic_request(&request,&o).is_some());replace(&mut request,"choice",s(o.app_language.as_ref().unwrap().rows[1].key.wire()));assert!(basic_request(&request,&o).is_none());
 o.keyboards=KeyboardSnapshot::decode(&crate::settings_keyboards::tests::snapshot(1));let p=o.keyboards.as_ref().unwrap();
 let mut request=obj(vec![("kind",s("keyboards")),("key",s(p.key.as_ref().unwrap().wire())),("target",Value::Null),("operation",s("choose_default"))]);
 assert!(basic_request(&request,&o).is_some());replace(&mut request,"target",s(o.keyboards.as_ref().unwrap().rows[0].target.wire()));assert!(basic_request(&request,&o).is_none());
 replace(&mut request,"operation",s("disable"));assert!(basic_request(&request,&o).is_none());
 replace(&mut request,"operation",s("provider_settings"));assert!(basic_request(&request,&o).is_some());
 replace(&mut request,"key",s("f".repeat(64)));assert!(basic_request(&request,&o).is_none());
 o.system_language=SystemLanguageSnapshot::decode(&crate::settings_system_language::tests::snapshot(1));let p=o.system_language.as_ref().unwrap();let target=s(p.current.as_ref().unwrap()[0].target.wire());
 let mut request=obj(vec![("kind",s("system_languages")),("key",s(p.key.as_ref().unwrap().wire())),("order",Value::Arr(vec![target.clone()]))]);
 assert!(basic_request(&request,&o).is_some());replace(&mut request,"order",Value::Arr(vec![target.clone(),target.clone()]));assert!(basic_request(&request,&o).is_none());
 replace(&mut request,"order",Value::Arr(vec![]));assert!(basic_request(&request,&o).is_none());
 replace(&mut request,"order",Value::Arr(vec![target]));o.system_language.as_mut().unwrap().clear_actions();assert!(basic_request(&request,&o).is_none());
}
