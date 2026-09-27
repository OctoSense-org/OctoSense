use crate::settings_app::SettingsSnapshot;
use crate::settings_script::{Controller,Frame,Patch,bundled_source,bundled_widgets};
use crate::settings_script_bridge::{observation,basic_request};
use crate::settings_caption_custom::{CaptionSnapshot,CaptionField,CaptionValue};
use makepad_strict_json::{obj,s,Value};
fn new()->Controller{Controller::new(&bundled_source(),bundled_widgets()).unwrap()}
fn step(c:&mut Controller,o:&SettingsSnapshot,kind:&str,id:&str)->Frame{
 c.step(&obj(vec![("kind",s(kind)),("id",s(id)),("value",Value::Null)]),&observation(o,false).unwrap(),|f|{
  crate::settings_script_host_facade::HostFacade::decode(&f.state,o)?;
  for r in &f.requests {if basic_request(r,o).is_none(){return Err(format!("Rejected test binding: {r:?}"));}}Ok(())
 }).unwrap().0
}
fn click(c:&mut Controller,o:&SettingsSnapshot,id:&str)->Frame{step(c,o,"click",id)}
fn text<'a>(f:&'a Frame,id:&str)->&'a str{f.patches.iter().rev().find_map(|p|match p{Patch::Text(k,v)if k==id=>Some(v.as_str()),_=>None}).unwrap()}
fn open()->(Controller,SettingsSnapshot){let mut c=new();let mut o=SettingsSnapshot{android:true,..Default::default()};let f=step(&mut c,&o,"navigate","caption_custom");let visit=f.state.get("caption_ui").unwrap().get("visit").unwrap().as_str().unwrap().parse().unwrap();o.caption_custom=CaptionSnapshot::decode(&crate::settings_caption_custom::tests::snapshot(1,visit));step(&mut c,&o,"observe","");(c,o)}
#[test]fn script_caption_palette_pages_render_all_finite_native_choices_and_selected_labels(){
 let(mut c,o)=open();
 for (index,field) in CaptionField::ALL.into_iter().enumerate(){
  let mut f=click(&mut c,&o,&format!("control_{index}_more"));assert_eq!(f.state.get("page").and_then(Value::as_str),Some("CaptionChoice"));
  for(i,value)in field.choices().into_iter().enumerate(){if i>0&&i%20==0{f=click(&mut c,&o,"caption_choice_next");}
   let expected=format!("{}{}",value.label(),if value.selected_by(field.sample()){" · Selected"}else{""});assert_eq!(text(&f,&format!("caption_choice_{}",i%20)),expected);
  }
  step(&mut c,&o,"back","");
 }
}
#[test]fn script_caption_lease_change_rejects_held_choice_and_readback_stays_native(){
 let(mut c,mut o)=open();click(&mut c,&o,"control_1_more");step(&mut c,&o,"press","caption_choice_3");
 let f=click(&mut c,&o,"caption_choice_3");assert_eq!(f.requests[0].get("value").and_then(Value::as_str),Some("caption_swatch:48"));assert_eq!(text(&f,"caption_choice_observed"),"Text color: White");
 step(&mut c,&o,"press","caption_choice_3");let f=step(&mut c,&o,"caption_renew","");let visit=f.requests[0].get("visit").and_then(Value::as_str).unwrap().parse().unwrap();assert_ne!(o.caption_custom.as_ref().unwrap().visit,visit);
 assert!(click(&mut c,&o,"caption_choice_3").requests.is_empty());
 o.caption_custom=CaptionSnapshot::decode(&crate::settings_caption_custom::tests::snapshot(2,visit));let f=step(&mut c,&o,"observe","");assert_eq!(text(&f,"caption_choice_observed"),"Text color: White");o.caption_custom.as_mut().unwrap().retire();assert!(click(&mut c,&o,"caption_choice_3").requests.is_empty());
}
#[test]fn script_caption_inherited_colors_and_transparent_background_keep_distinct_previews(){
 let(mut c,mut o)=open();let n=o.caption_custom.as_mut().unwrap();
 for row in &mut n.controls.controls {if row.id==crate::settings_controls::ControlId::CaptionCustom(CaptionField::Foreground){row.value=Some(crate::settings_controls::ControlValue::CaptionDetail(CaptionValue::Packed(0x00ffff80)));}if row.id==crate::settings_controls::ControlId::CaptionCustom(CaptionField::Background){row.value=Some(crate::settings_controls::ControlValue::CaptionDetail(CaptionValue::Packed(0x00000080)));}}
 let f=step(&mut c,&o,"observe","");assert_eq!(text(&f,"control_1_value"),"Default (set by app)");assert_eq!(text(&f,"control_5_value"),"None");
 assert!(f.patches.contains(&Patch::Color("caption_preview_text".into(),true,[1.,1.,1.,1.])));assert!(f.patches.contains(&Patch::Color("caption_preview_background".into(),false,[0.,0.,0.,0.])));
}
