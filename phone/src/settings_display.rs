//! Display changes use finite observed choices and one reviewed field at a time.
use makepad_strict_json::{s,Value};
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct DisplayKey(String);
impl DisplayKey {
    pub fn wire(&self)->&str{&self.0}
    fn decode(v:&Value)->Option<Self>{let s=v.as_str()?;(s.len()==64&&s.bytes().all(|c|c.is_ascii_digit()||(b'a'..=b'f').contains(&c))).then(||Self(s.into()))}
}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum Availability{Available,Unsupported,Unavailable,Restricted}
impl Availability {
    fn decode(v:&Value)->Option<Self>{Some(match v.as_str()?{"available"=>Self::Available,"unsupported"=>Self::Unsupported,"unavailable"=>Self::Unavailable,"restricted"=>Self::Restricted,_=>return None})}
    pub fn label(self)->&'static str{match self{Self::Available=>"",Self::Unsupported=>"Not supported on this device.",Self::Unavailable=>"Unavailable on this installation.",Self::Restricted=>"Unavailable while locked or restricted."}}
}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum NightMode{Disabled,Custom,Sunset}
impl NightMode {
    pub const ALL:[Self;3]=[Self::Disabled,Self::Custom,Self::Sunset];
    pub fn wire(self)->&'static str{match self{Self::Disabled=>"disabled",Self::Custom=>"custom",Self::Sunset=>"sunset"}}
    pub fn label(self)->&'static str{match self{Self::Disabled=>"No schedule",Self::Custom=>"Custom times",Self::Sunset=>"Sunset to sunrise"}}
    fn decode(v:&Value)->Option<Self>{Self::ALL.into_iter().find(|m|Some(m.wire())==v.as_str())}
}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum NightField{Activated,Temperature,Mode,Start,End}
impl NightField {
    const ALL:[Self;5]=[Self::Activated,Self::Temperature,Self::Mode,Self::Start,Self::End];
    pub fn wire(self)->&'static str{match self{Self::Activated=>"activated",Self::Temperature=>"temperature",Self::Mode=>"mode",Self::Start=>"start",Self::End=>"end"}}
    fn decode(v:&Value)->Option<Self>{Self::ALL.into_iter().find(|m|Some(m.wire())==v.as_str())}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub enum DisplayValue{Density(DisplayKey),Activated(bool),Temperature(u32),Mode(NightMode),Start(u32),End(u32)}
impl DisplayValue {
    pub fn field(&self)->Option<NightField>{Some(match self{Self::Density(_)=>return None,Self::Activated(_)=>NightField::Activated,Self::Temperature(_)=>NightField::Temperature,Self::Mode(_)=>NightField::Mode,Self::Start(_)=>NightField::Start,Self::End(_)=>NightField::End})}
    pub fn json(&self)->Value{match self{Self::Density(key)=>s(key.wire()),Self::Activated(v)=>Value::Bool(*v),Self::Temperature(v)|Self::Start(v)|Self::End(v)=>Value::Int(*v as i64),Self::Mode(v)=>s(v.wire())}}
    pub fn valid(&self)->bool{match self{Self::Temperature(v)=>(1..=100000).contains(v),Self::Start(v)|Self::End(v)=>*v<86400,_=>true}}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub enum DisplayRequest{Snapshot,Set{key:DisplayKey,value:DisplayValue}}
impl DisplayRequest{pub fn valid(&self)->bool{match self{Self::Snapshot=>true,Self::Set{value,..}=>value.valid()}}}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct DensityChoice{pub key:DisplayKey,pub label:String,pub dpi:u32,pub is_default:bool}
#[derive(serde::Serialize, Clone,Debug)]
pub struct Density{pub availability:Availability,pub current:Option<u32>,pub default:Option<u32>,pub can_set:bool,pub options:Vec<DensityChoice>}
#[derive(serde::Serialize, Clone,Debug)]
pub struct Night{pub availability:Availability,pub activated:Option<bool>,pub temperature:Option<u32>,pub min:Option<u32>,pub max:Option<u32>,pub mode:Option<NightMode>,pub start:Option<u32>,pub end:Option<u32>,pub location:Option<bool>,pub caps:Vec<NightField>}
#[derive(serde::Serialize, Clone,Debug)]
pub struct DisplaySnapshot{pub request_id:i64,pub key:DisplayKey,pub density:Density,pub night:Night}
fn optional<T>(v:Option<&Value>,decode:impl FnOnce(&Value)->Option<T>)->Option<Option<T>>{match v{None|Some(Value::Null)=>Some(None),Some(v)=>decode(v).map(Some)}}
fn number(v:&Value,max:u32)->Option<u32>{u32::try_from(v.as_i64()?).ok().filter(|n|*n<=max)}
fn positive(v:&Value,max:u32)->Option<u32>{number(v,max).filter(|n|*n>0)}
impl DisplaySnapshot {
    pub fn decode(v:&Value)->Option<Self>{
        if v.get("schema")?.as_i64()?!=1{return None;}
        let d=v.get("density")?;let n=v.get("night")?;
        let mut options=Vec::new();let rows=d.get("options")?.as_arr()?;if rows.len()>8{return None;}
        for row in rows{let key=DisplayKey::decode(row.get("key")?)?;let label=row.get("label")?.as_str()?;
            if label.is_empty()||label.chars().count()>128||label.chars().any(char::is_control)||options.iter().any(|r:&DensityChoice|r.key==key){return None;}
            options.push(DensityChoice{key,label:label.into(),dpi:positive(row.get("dpi")?,10000)?,is_default:row.get("is_default")?.as_bool()?});
        }
        let density=Density{availability:Availability::decode(d.get("availability")?)?,current:optional(d.get("current_dpi"),|v|positive(v,10000))?,default:optional(d.get("default_dpi"),|v|positive(v,10000))?,can_set:d.get("can_set")?.as_bool()?,options};
        if density.can_set&&(density.availability!=Availability::Available||density.current.is_none()||density.default.is_none()||density.options.is_empty()){return None;}
        if !density.options.is_empty()&&(density.options.iter().filter(|r|r.is_default).count()!=1||density.options.iter().any(|r|r.is_default&&Some(r.dpi)!=density.default)){return None;}
        let mut caps=Vec::new();for c in n.get("can_set")?.as_arr()?{let c=NightField::decode(c)?;if caps.contains(&c){return None;}caps.push(c);}
        let night=Night{availability:Availability::decode(n.get("availability")?)?,activated:optional(n.get("activated"),Value::as_bool)?,temperature:optional(n.get("temperature_kelvin"),|v|positive(v,100000))?,min:optional(n.get("min_kelvin"),|v|positive(v,100000))?,max:optional(n.get("max_kelvin"),|v|positive(v,100000))?,mode:optional(n.get("mode"),NightMode::decode)?,start:optional(n.get("start_seconds"),|v|number(v,86399))?,end:optional(n.get("end_seconds"),|v|number(v,86399))?,location:optional(n.get("location_enabled"),Value::as_bool)?,caps};
        if !night.caps.is_empty()&&night.availability!=Availability::Available{return None;}
        if night.min.zip(night.max).is_some_and(|(a,b)|a>b){return None;}
        for cap in &night.caps{if !match cap{NightField::Activated=>night.activated.is_some(),NightField::Temperature=>night.activated==Some(true)&&night.temperature.zip(night.min.zip(night.max)).is_some_and(|(v,(min,max))|(min..=max).contains(&v)),NightField::Mode=>night.mode.is_some(),NightField::Start=>night.mode==Some(NightMode::Custom)&&night.start.is_some(),NightField::End=>night.mode==Some(NightMode::Custom)&&night.end.is_some()}{return None;}}
        Some(Self{request_id:v.get("request_id")?.as_i64().filter(|n|*n>0)?,key:DisplayKey::decode(v.get("key")?)?,density,night})
    }
    pub fn permits(&self,request:&DisplayRequest)->bool{
        let DisplayRequest::Set{key,value}=request else{return true;};if key!=&self.key||!value.valid(){return false;}
        match value{
            DisplayValue::Density(key)=>self.density.can_set&&self.density.options.iter().any(|r|&r.key==key),
            DisplayValue::Temperature(v)=>self.night.caps.contains(&NightField::Temperature)&&self.night.min.zip(self.night.max).is_some_and(|(min,max)|(min..=max).contains(v)),
            DisplayValue::Mode(NightMode::Sunset)=>self.night.caps.contains(&NightField::Mode)&&self.night.location==Some(true),
            _=>value.field().is_some_and(|f|self.night.caps.contains(&f)),
        }
    }
    pub fn clear_actions(&mut self){self.density.can_set=false;self.night.caps.clear();}
}
pub fn format_time(seconds:u32)->String{if seconds%60==0{format!("{:02}:{:02}",seconds/3600,(seconds/60)%60)}else{format!("{:02}:{:02}:{:02}",seconds/3600,(seconds/60)%60,seconds%60)}}
pub fn parse_time(text:&str)->Option<u32>{let parts:Vec<_>=text.split(':').collect();if !(2..=3).contains(&parts.len())||parts.iter().any(|p|p.len()!=2||!p.bytes().all(|b|b.is_ascii_digit())){return None;}let h=parts[0].parse::<u32>().ok()?;let m=parts[1].parse::<u32>().ok()?;let s=if parts.len()==3{parts[2].parse::<u32>().ok()?}else{0};(h<24&&m<60&&s<60).then_some(h*3600+m*60+s)}
#[cfg(test)]
pub(crate) mod tests {
 use super::*;
 pub fn snapshot()->Value{makepad_strict_json::parse(&format!(r#"{{"schema":1,"request_id":1,"key":"{0:064x}","density":{{"availability":"available","current_dpi":420,"default_dpi":420,"can_set":true,"options":[{{"key":"{1:064x}","label":"Default","dpi":420,"is_default":true}}]}},"night":{{"availability":"available","activated":true,"temperature_kelvin":2850,"min_kelvin":2000,"max_kelvin":6500,"mode":"custom","start_seconds":79201,"end_seconds":21600,"location_enabled":false,"can_set":["activated","temperature","mode","start","end"]}}}}"#,1,2).as_bytes()).unwrap()}
 #[test]fn display_write_requires_exact_choice_value_range_and_live_capability(){let mut s=DisplaySnapshot::decode(&snapshot()).unwrap();let req=DisplayRequest::Set{key:s.key.clone(),value:DisplayValue::Density(s.density.options[0].key.clone())};assert!(s.permits(&req));assert!(!s.permits(&DisplayRequest::Set{key:s.key.clone(),value:DisplayValue::Density(s.key.clone())}));assert!(!s.permits(&DisplayRequest::Set{key:s.key.clone(),value:DisplayValue::Temperature(1999)}));assert!(!s.permits(&DisplayRequest::Set{key:s.key.clone(),value:DisplayValue::Mode(NightMode::Sunset)}));s.clear_actions();assert!(!s.permits(&req));assert_eq!(s.density.current,Some(420));}
 #[test]fn night_schedule_preserves_seconds_and_rejects_invalid_clock_text(){for n in [0,1,60,79201,86399]{assert_eq!(parse_time(&format_time(n)),Some(n));}for s in ["24:00","9:00","12:60","00:00:60"," 12:00","12:00\n"]{assert_eq!(parse_time(s),None);}}
 #[test]fn unknown_observations_do_not_invent_writable_controls(){let mut v=snapshot();crate::settings_updates::tests::field(&mut v,"density",makepad_strict_json::parse(r#"{"availability":"unavailable","current_dpi":420,"can_set":false,"options":[]}"#.as_bytes()).unwrap());let s=DisplaySnapshot::decode(&v).unwrap();assert_eq!(s.density.default,None);assert!(s.density.options.is_empty());}
}
