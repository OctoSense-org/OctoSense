//! Finite caption choices are distinct from arbitrary native observations.
use makepad_strict_json::Value;
use crate::settings_controls::{Availability,ControlId,ControlValue,ControlsPage,ControlsSnapshot};

#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq,Hash)]
pub enum CaptionField {Typeface,Foreground,ForegroundOpacity,EdgeType,EdgeColor,Background,BackgroundOpacity,Window,WindowOpacity}
impl CaptionField {
    pub const ALL:[Self;9]=[Self::Typeface,Self::Foreground,Self::ForegroundOpacity,Self::EdgeType,Self::EdgeColor,Self::Background,Self::BackgroundOpacity,Self::Window,Self::WindowOpacity];
    pub fn wire(self)->&'static str{match self{Self::Typeface=>"caption_typeface",Self::Foreground=>"caption_foreground_color",Self::ForegroundOpacity=>"caption_foreground_opacity",Self::EdgeType=>"caption_edge_type",Self::EdgeColor=>"caption_edge_color",Self::Background=>"caption_background_color",Self::BackgroundOpacity=>"caption_background_opacity",Self::Window=>"caption_window_color",Self::WindowOpacity=>"caption_window_opacity"}}
    pub fn label(self)->&'static str{match self{Self::Typeface=>"Typeface",Self::Foreground=>"Text color",Self::ForegroundOpacity=>"Text opacity",Self::EdgeType=>"Edge type",Self::EdgeColor=>"Edge color",Self::Background=>"Background color",Self::BackgroundOpacity=>"Background opacity",Self::Window=>"Window color",Self::WindowOpacity=>"Window opacity"}}
    pub fn opacity(self)->bool{matches!(self,Self::ForegroundOpacity|Self::BackgroundOpacity|Self::WindowOpacity)}
    pub fn colors(self)->bool{matches!(self,Self::Foreground|Self::EdgeColor|Self::Background|Self::Window)}
    pub fn choices(self)->Vec<CaptionValue>{
        if self.opacity(){return [64,128,192,255].into_iter().map(CaptionValue::Alpha).collect();}
        if self==Self::Typeface{return (0..9).map(CaptionValue::Typeface).collect();}
        if self==Self::EdgeType{return (0..6).map(CaptionValue::Edge).collect();}
        let mut choices=Vec::new();if matches!(self,Self::Background|Self::Window){choices.push(CaptionValue::SwatchNone);}
        choices.push(CaptionValue::SwatchDefault);let named=[63,0,48,60,12,15,3,51];
        choices.extend(named.into_iter().map(CaptionValue::Swatch));choices.extend((0..64).filter(|n|!named.contains(n)).map(CaptionValue::Swatch));choices
    }
    pub fn accepts(self,value:CaptionValue,write:bool)->bool{if write{return self.choices().contains(&value);}
        match value{CaptionValue::Packed(_)=>self.colors(),CaptionValue::Opacity(_)=>self.opacity(),CaptionValue::EdgeObserved(_)=>self==Self::EdgeType,CaptionValue::Typeface(n)=>self==Self::Typeface&&n<9,CaptionValue::CustomTypeface=>self==Self::Typeface,_=>false}}
    pub fn sample(self)->CaptionValue{if self.colors(){CaptionValue::Packed(0xffffffff)}else if self.opacity(){CaptionValue::Opacity(255)}else if self==Self::EdgeType{CaptionValue::EdgeObserved(1)}else{CaptionValue::Typeface(0)}}
}
const FACES:[&str;9]=["default","sans","condensed","mono_sans","serif","mono_serif","casual","cursive","small_caps"];
const FACE_LABELS:[&str;9]=["Default","Sans-serif","Sans-serif condensed","Sans-serif monospace","Serif","Serif monospace","Casual","Cursive","Small capitals"];
const EDGES:[&str;6]=["default","none","outline","shadow","raised","depressed"];
const EDGE_LABELS:[&str;6]=["Default","None","Outline","Drop shadow","Raised","Depressed"];
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq,Hash)]
pub enum CaptionValue {Packed(u32),Opacity(u8),EdgeObserved(i32),CustomTypeface,Typeface(u8),SwatchDefault,SwatchNone,Swatch(u8),Alpha(u8),Edge(u8)}
impl CaptionValue {
    pub fn decode(text:&str)->Option<Self>{
        if let Some(raw)=text.strip_prefix("caption_color_argb:"){return(raw.len()==8&&raw.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))).then(||u32::from_str_radix(raw,16).ok()).flatten().map(Self::Packed);}
        if let Some(raw)=text.strip_prefix("caption_opacity:"){let n=raw.parse::<u8>().ok()?;return(raw==n.to_string()).then_some(Self::Opacity(n));}
        if let Some(raw)=text.strip_prefix("caption_edge_value:"){let n=raw.parse::<i32>().ok()?;return(raw==n.to_string()).then_some(Self::EdgeObserved(n));}
        if text=="caption_typeface_custom"{return Some(Self::CustomTypeface);}
        if let Some(raw)=text.strip_prefix("caption_typeface:"){return FACES.iter().position(|s|*s==raw).map(|n|Self::Typeface(n as u8));}
        if let Some(raw)=text.strip_prefix("caption_edge:"){return EDGES.iter().position(|s|*s==raw).map(|n|Self::Edge(n as u8));}
        if let Some(raw)=text.strip_prefix("caption_alpha:"){let n=raw.parse::<u8>().ok()?;return(raw==n.to_string()&&[64,128,192,255].contains(&n)).then_some(Self::Alpha(n));}
        if let Some(raw)=text.strip_prefix("caption_swatch:"){if raw=="default"{return Some(Self::SwatchDefault);}if raw=="none"{return Some(Self::SwatchNone);}let n=raw.parse::<u8>().ok()?;return(n<64&&raw==n.to_string()).then_some(Self::Swatch(n));}None
    }
    pub fn wire(self)->String{match self{Self::Packed(v)=>format!("caption_color_argb:{v:08x}"),Self::Opacity(v)=>format!("caption_opacity:{v}"),Self::EdgeObserved(v)=>format!("caption_edge_value:{v}"),Self::CustomTypeface=>"caption_typeface_custom".into(),Self::Typeface(v)=>format!("caption_typeface:{}",FACES.get(v as usize).unwrap_or(&"invalid")),Self::SwatchDefault=>"caption_swatch:default".into(),Self::SwatchNone=>"caption_swatch:none".into(),Self::Swatch(v)=>format!("caption_swatch:{v}"),Self::Alpha(v)=>format!("caption_alpha:{v}"),Self::Edge(v)=>format!("caption_edge:{}",EDGES.get(v as usize).unwrap_or(&"invalid"))}}
    pub fn rgb(index:u8)->u32{0xff000000|((index as u32/16)*85<<16)|(((index as u32/4)%4)*85<<8)|(index as u32%4)*85}
    pub fn rgba(self)->Option<[f32;4]>{let n=match self{Self::Swatch(v)if v<64=>Self::rgb(v),Self::Packed(v)if v>>24!=0=>v,_=>return None};Some([((n>>16)&255)as f32/255.,((n>>8)&255)as f32/255.,(n&255)as f32/255.,(n>>24)as f32/255.])}
    pub fn selected_by(self,observed:Self)->bool{match(self,observed){(Self::SwatchDefault,Self::Packed(v))=>v>>24==0&&(v&0xffff00)!=0,(Self::SwatchNone,Self::Packed(v))=>v>>24==0&&(v&0xffff00)==0,(Self::Swatch(n),Self::Packed(v))=>v>>24!=0&&(v&0xffffff)==(Self::rgb(n)&0xffffff),(Self::Alpha(n),Self::Opacity(v))=>n==v,(Self::Edge(n),Self::EdgeObserved(v))=>n as i32-1==v,_=>self==observed}}
    pub fn label(self)->String{match self{
        Self::Packed(v)if v>>24==0=>if(v&0xffff00)==0{"None"}else{"Default (set by app)"}.into(),Self::Packed(v)=>color_label(v),Self::Swatch(v)=>color_label(Self::rgb(v)),Self::SwatchDefault=>"Default (set by app)".into(),Self::SwatchNone=>"None".into(),
        Self::Opacity(v)|Self::Alpha(v)=>match v{64=>"25%".into(),128=>"50%".into(),192=>"75%".into(),255=>"100%".into(),_=>format!("{:.1}%",v as f64*100./255.)},Self::EdgeObserved(v)=>v.checked_add(1).and_then(|n|usize::try_from(n).ok()).and_then(|n|EDGE_LABELS.get(n)).map(|s|(*s).to_string()).unwrap_or_else(||format!("Custom edge type ({v})")),Self::Edge(v)=>EDGE_LABELS.get(v as usize).unwrap_or(&"Unknown").to_string(),Self::Typeface(v)=>FACE_LABELS.get(v as usize).unwrap_or(&"Unknown").to_string(),Self::CustomTypeface=>"Custom typeface (kept)".into()}}
}
fn color_label(n:u32)->String{match n&0xffffff{0xffffff=>"White".into(),0=>"Black".into(),0xff0000=>"Red".into(),0xffff00=>"Yellow".into(),0x00ff00=>"Green".into(),0x00ffff=>"Cyan".into(),0x0000ff=>"Blue".into(),0xff00ff=>"Magenta".into(),_=>format!("RGB {}, {}, {}",(n>>16)&255,(n>>8)&255,n&255)}}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]pub enum CaptionRequest {Snapshot{visit:i64},Set{visit:i64,field:CaptionField,value:CaptionValue}}
impl CaptionRequest {pub fn visit(&self)->i64{match self{Self::Snapshot{visit}|Self::Set{visit,..}=>*visit}}pub fn valid(&self)->bool{self.visit()>0&&match self{Self::Snapshot{..}=>true,Self::Set{field,value,..}=>field.accepts(*value,true)}}}
#[derive(serde::Serialize, Clone,Debug)]pub struct CaptionSnapshot {pub visit:i64,pub custom:Option<bool>,pub active:bool,pub controls:ControlsSnapshot}
impl CaptionSnapshot {
    pub fn decode(value:&Value)->Option<Self>{let controls=ControlsSnapshot::decode(value)?;if controls.page!=ControlsPage::CaptionCustom{return None;}
        let visit=value.get("visit")?.as_i64().filter(|n|*n>0)?;let active=value.get("scope_active")?.as_bool()?;let custom=match value.get("custom_selected")?{Value::Null=>None,v=>Some(v.as_bool()?)};
        if(!active||custom!=Some(true))&&controls.controls.iter().any(|r|!r.options.is_empty()){return None;}Some(Self{visit,custom,active,controls})}
    pub fn permits(&self,request:&CaptionRequest)->bool{if !request.valid()||self.visit!=request.visit(){return false;}match request{CaptionRequest::Snapshot{..}=>true,CaptionRequest::Set{field,value,..}=>self.active&&self.custom==Some(true)&&self.controls.control(ControlId::CaptionCustom(*field)).is_some_and(|row|row.availability==Availability::Available&&row.value.is_some()&&row.options.contains(&ControlValue::CaptionDetail(*value)))}}
    pub fn retire(&mut self){self.active=false;self.controls.clear_actions();}
}

#[cfg(test)]pub(crate) mod tests{
    use super::*;use makepad_strict_json::{s,Value};
    pub fn snapshot(id:i64,visit:i64)->Value{let mut v=crate::settings_controls::tests::snapshot(id,ControlsPage::CaptionCustom);if let Value::Obj(fields)=&mut v{fields.extend([("visit".into(),Value::Int(visit)),("scope_active".into(),Value::Bool(true)),("custom_selected".into(),Value::Bool(true))]);}v}
    #[test]fn caption_palette_is_complete_and_writes_cannot_target_raw_colors(){
        for field in CaptionField::ALL{let choices=field.choices();assert_eq!(choices.len(),match field{CaptionField::Typeface=>9,CaptionField::EdgeType=>6,f if f.opacity()=>4,CaptionField::Background|CaptionField::Window=>66,_=>65});for (i,v) in choices.iter().enumerate(){assert_eq!(CaptionValue::decode(&v.wire()),Some(*v));assert!(field.accepts(*v,true));assert!(!choices[..i].contains(v));}assert!(field.accepts(field.sample(),false));}
        for value in ["caption_swatch:64","caption_swatch:01","caption_alpha:127","caption_color_argb:fffffffff","caption_color_argb:FF000000","caption_typeface:arbitrary"]{assert!(CaptionValue::decode(value).is_none());}
        assert!(!CaptionField::Foreground.accepts(CaptionValue::Packed(0xffff0000),true));assert!(!CaptionField::Foreground.accepts(CaptionValue::SwatchNone,true));
        assert_eq!(CaptionValue::EdgeObserved(i32::MAX).label(),"Custom edge type (2147483647)");
    }
    #[test]fn caption_inherited_none_alpha_and_native_rgb_choices_stay_distinct(){assert!(CaptionValue::SwatchDefault.selected_by(CaptionValue::Packed(0x00ffff80)));assert!(CaptionValue::SwatchNone.selected_by(CaptionValue::Packed(0x00000080)));assert!(!CaptionValue::SwatchDefault.selected_by(CaptionValue::Packed(0x00000080)));for i in 0..64{assert!(CaptionValue::Swatch(i).selected_by(CaptionValue::Packed(0x80000000|(CaptionValue::rgb(i)&0xffffff))));}assert!(CaptionValue::Alpha(128).selected_by(CaptionValue::Opacity(128)));assert!(!CaptionValue::Alpha(128).selected_by(CaptionValue::Opacity(127)));}
    #[test]fn caption_scope_and_native_options_are_required_separately_from_unscoped_controls(){let mut state=CaptionSnapshot::decode(&snapshot(3,7)).unwrap();let request=CaptionRequest::Set{visit:7,field:CaptionField::Foreground,value:CaptionValue::Swatch(48)};assert!(state.permits(&request));assert!(!state.permits(&CaptionRequest::Set{visit:8,field:CaptionField::Foreground,value:CaptionValue::Swatch(48)}));assert!(!ControlsRequestForTest::unscoped());state.retire();assert!(!state.permits(&request));let mut v=snapshot(4,8);if let Value::Obj(fields)=&mut v{fields.iter_mut().find(|(k,_)|k=="scope_active").unwrap().1=Value::Bool(false);}assert!(CaptionSnapshot::decode(&v).is_none());let _=s("bounded");}
    struct ControlsRequestForTest;impl ControlsRequestForTest{fn unscoped()->bool{crate::settings_controls::ControlsRequest::Set{page:ControlsPage::CaptionCustom,control:ControlId::CaptionCustom(CaptionField::Foreground),value:ControlValue::CaptionDetail(CaptionValue::Swatch(48))}.valid()}}
}
