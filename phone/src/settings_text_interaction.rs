//! Finite Android text and interaction policies; arbitrary observations are never setters.
use crate::settings_controls::ControlValue;
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq,Hash)]
pub enum TextControl {HighContrast,BoldText,RemoveAnimations,TouchHold,ActionTimeout,Autoclick,LargePointer}
impl TextControl {
 pub const ALL:[Self;7]=[Self::HighContrast,Self::BoldText,Self::RemoveAnimations,Self::TouchHold,Self::ActionTimeout,Self::Autoclick,Self::LargePointer];
 pub fn wire(self)->&'static str{match self{Self::HighContrast=>"high_contrast_text",Self::BoldText=>"bold_text",Self::RemoveAnimations=>"remove_animations",Self::TouchHold=>"touch_hold_delay",Self::ActionTimeout=>"action_timeout",Self::Autoclick=>"autoclick",Self::LargePointer=>"large_pointer"}}
 pub fn label(self)->&'static str{match self{Self::HighContrast=>"High contrast text",Self::BoldText=>"Bold text",Self::RemoveAnimations=>"Remove animations",Self::TouchHold=>"Touch & hold delay",Self::ActionTimeout=>"Time to take action",Self::Autoclick=>"Automatic click",Self::LargePointer=>"Large mouse pointer"}}
 pub fn options(self)->Vec<ControlValue>{use TextValue::*;match self{Self::TouchHold=>[400,1000,1500].into_iter().map(|n|ControlValue::TextInteraction(HoldPreset(n))).collect(),Self::ActionTimeout=>[0,10000,30000,60000,120000].into_iter().map(|n|ControlValue::TextInteraction(TimeoutPreset(n))).collect(),Self::Autoclick=>std::iter::once(ControlValue::Off).chain((200..=1000).step_by(100).map(|n|ControlValue::TextInteraction(AutoclickDelay(n)))).collect(),_=>vec![ControlValue::Off,ControlValue::On]}}
 pub fn sample(self)->ControlValue{use TextValue::*;match self{Self::TouchHold=>ControlValue::TextInteraction(HoldMs(400)),Self::ActionTimeout=>ControlValue::TextInteraction(TimeoutMs(0,0)),Self::Autoclick=>ControlValue::TextInteraction(Autoclick(false,600)),_=>ControlValue::Off}}
 pub fn accepts(self,value:ControlValue,mutation:bool)->bool{if mutation{return self.options().contains(&value);}match value{ControlValue::Off|ControlValue::On=>matches!(self,Self::HighContrast|Self::BoldText|Self::RemoveAnimations|Self::LargePointer),ControlValue::TextInteraction(v)=>matches!((self,v),(Self::BoldText,TextValue::FontWeight(_))|(Self::RemoveAnimations,TextValue::AnimationScales(_))|(Self::TouchHold,TextValue::HoldMs(_))|(Self::ActionTimeout,TextValue::TimeoutMs(..))|(Self::Autoclick,TextValue::Autoclick(..))),_=>false}}
}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum TextValue {FontWeight(i32),AnimationScales([u32;3]),HoldMs(u32),TimeoutMs(u32,u32),Autoclick(bool,u32),HoldPreset(u16),TimeoutPreset(u32),AutoclickDelay(u16)}
fn uint(s:&str)->Option<u32>{let n=s.parse::<u32>().ok()?;(n<=i32::MAX as u32&&s==n.to_string()).then_some(n)}
impl TextValue {
 pub fn decode(s:&str)->Option<Self>{
  if let Some(v)=s.strip_prefix("font_weight:"){let n=v.parse::<i32>().ok()?;return (v==n.to_string()).then_some(Self::FontWeight(n));}
  if let Some(v)=s.strip_prefix("animation_scales:"){let ns=v.split(',').map(|s|{if s.len()>32||s.is_empty()||!s.bytes().all(|c|c.is_ascii_digit()||b".+-eE".contains(&c)){return None;}let n=s.parse::<f32>().ok()?;(n.is_finite()&&n>=0.).then_some(n.to_bits())}).collect::<Option<Vec<_>>>()?;return ns.try_into().ok().map(Self::AnimationScales);}
  if let Some(v)=s.strip_prefix("hold_ms:"){return uint(v).map(Self::HoldMs);}
  if let Some(v)=s.strip_prefix("timeout_ms:"){return uint(v).map(|n|Self::TimeoutMs(n,n));}
  if let Some(v)=s.strip_prefix("timeout_pair_ms:"){let(a,b)=v.split_once(',')?;return Some(Self::TimeoutMs(uint(a)?,uint(b)?));}
  if let Some(v)=s.strip_prefix("autoclick_off_ms:"){return uint(v).map(|n|Self::Autoclick(false,n));}
  if let Some(v)=s.strip_prefix("autoclick_ms:"){return uint(v).map(|n|Self::Autoclick(true,n));}
  if let Some(v)=s.strip_prefix("hold:"){return match v{"short"=>Some(Self::HoldPreset(400)),"medium"=>Some(Self::HoldPreset(1000)),"long"=>Some(Self::HoldPreset(1500)),_=>None};}
  if let Some(v)=s.strip_prefix("timeout:"){return match v{"default"=>Some(Self::TimeoutPreset(0)),"seconds10"=>Some(Self::TimeoutPreset(10000)),"seconds30"=>Some(Self::TimeoutPreset(30000)),"minute1"=>Some(Self::TimeoutPreset(60000)),"minutes2"=>Some(Self::TimeoutPreset(120000)),_=>None};}
  if let Some(v)=s.strip_prefix("autoclick_delay:"){let n=uint(v)?;return ((200..=1000).contains(&n)&&n%100==0).then_some(Self::AutoclickDelay(n as u16));}None
 }
 pub fn wire(self)->String{match self{Self::FontWeight(n)=>format!("font_weight:{n}"),Self::AnimationScales(ns)=>format!("animation_scales:{:?},{:?},{:?}",f32::from_bits(ns[0]),f32::from_bits(ns[1]),f32::from_bits(ns[2])),Self::HoldMs(n)=>format!("hold_ms:{n}"),Self::TimeoutMs(a,b)=>if a==b{format!("timeout_ms:{a}")}else{format!("timeout_pair_ms:{a},{b}")},Self::Autoclick(on,n)=>format!("autoclick_{}ms:{n}",if on{""}else{"off_"}),Self::HoldPreset(n)=>format!("hold:{}",match n{400=>"short",1000=>"medium",_=>"long"}),Self::TimeoutPreset(n)=>format!("timeout:{}",match n{0=>"default",10000=>"seconds10",30000=>"seconds30",60000=>"minute1",_=>"minutes2"}),Self::AutoclickDelay(n)=>format!("autoclick_delay:{n}")}}
 pub fn label(self)->String{match self{Self::FontWeight(n)=>format!("Custom weight adjustment: {n}"),Self::AnimationScales(ns)=>format!("Custom animation scales: {}, {}, {}",f32::from_bits(ns[0]),f32::from_bits(ns[1]),f32::from_bits(ns[2])),Self::HoldMs(n)=>match n{400=>"Short (400 ms)".into(),1000=>"Medium (1 second)".into(),1500=>"Long (1.5 seconds)".into(),_=>format!("Custom: {n} ms")},Self::HoldPreset(n)=>match n{400=>"Short",1000=>"Medium",_=>"Long"}.into(),Self::TimeoutMs(a,b)if a!=b=>format!("Custom: controls {a} ms, other content {b} ms"),Self::TimeoutMs(n,_)|Self::TimeoutPreset(n)=>match n{0=>"Default".into(),10000=>"10 seconds".into(),30000=>"30 seconds".into(),60000=>"1 minute".into(),120000=>"2 minutes".into(),_=>format!("Custom: {n} ms")},Self::Autoclick(false,_)=>"Off".into(),Self::Autoclick(true,n)=>format!("On · {n} ms"),Self::AutoclickDelay(n)=>format!("{n} ms")}}
 pub fn selected_by(self,observed:Self)->bool{match(self,observed){(Self::HoldPreset(a),Self::HoldMs(b))=>a as u32==b,(Self::TimeoutPreset(n),Self::TimeoutMs(a,b))=>n==a&&a==b,(Self::AutoclickDelay(a),Self::Autoclick(true,b))=>a as u32==b,_=>self==observed}}
}

#[cfg(test)]mod tests{
 use super::*;use crate::settings_controls::{ControlsSnapshot,ControlsPage,ControlsRequest,ControlId};
 #[test]fn text_interaction_finite_choices_and_raw_observations_do_not_cross(){
  let page=ControlsPage::AccessibilityTextInteraction;let mut state=ControlsSnapshot::decode(&crate::settings_controls::tests::snapshot(1,page)).unwrap();
  for field in TextControl::ALL{let id=ControlId::TextInteraction(field);for value in field.options(){assert!(state.permits(&ControlsRequest::Set{page,control:id,value}));assert!(!ControlsRequest::Set{page:ControlsPage::Privacy,control:id,value}.valid());}assert_eq!(state.control(id).unwrap().value,Some(field.sample()));}
  assert_eq!(TextControl::Autoclick.options().len(),10);assert!(!TextControl::Autoclick.accepts(ControlValue::On,true));
  for (field,raw) in [(TextControl::BoldText,"font_weight:125"),(TextControl::RemoveAnimations,"animation_scales:0.5,1.0,0.25"),(TextControl::TouchHold,"hold_ms:735"),(TextControl::ActionTimeout,"timeout_pair_ms:30000,12500"),(TextControl::Autoclick,"autoclick_ms:625")]{let value=ControlValue::TextInteraction(TextValue::decode(raw).unwrap());assert!(field.accepts(value,false));assert!(!field.accepts(value,true));}
  state.clear_actions();assert_eq!(state.control(ControlId::TextInteraction(TextControl::TouchHold)).unwrap().value,Some(TextControl::TouchHold.sample()));assert!(state.controls.iter().all(|r|r.options.is_empty()));
 }
 #[test]fn text_interaction_parser_rejects_unknown_and_noncanonical_writes(){
  for raw in ["hold:unknown","hold_ms:-1","hold_ms:0400","timeout:minutes3","timeout_ms:2147483648","autoclick_delay:0200","autoclick_delay:2000","autoclick_delay:201","animation_scales:NaN,1,1","animation_scales:0,1,-1","animation_scales:1,1","font_weight:+300"]{assert!(TextValue::decode(raw).is_none(),"{raw}");}
  for field in TextControl::ALL{for value in field.options(){if let ControlValue::TextInteraction(value)=value{assert_eq!(TextValue::decode(&value.wire()),Some(value));}}}
  assert!(TextValue::HoldPreset(1000).selected_by(TextValue::HoldMs(1000)));assert!(!TextValue::TimeoutPreset(10000).selected_by(TextValue::TimeoutMs(10000,30000)));assert!(ControlValue::Off.selected_by(ControlValue::TextInteraction(TextValue::Autoclick(false,600))));
 }
}
