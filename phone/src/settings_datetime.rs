//! Manual civil-time drafts and the platform's finite time capabilities.
use makepad_strict_json::Value;
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct TimeKey(String);
impl TimeKey {
    pub fn wire(&self)->&str {&self.0}
    fn decode(value:&Value)->Option<Self> {let text=value.as_str()?;(text.len()==64&&text.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))).then(||Self(text.into()))}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub enum TimeAction {AutoTime(bool),AutoZone(bool),Clock{civil:String,second:bool},Zone(String)}
impl TimeAction {
    pub fn wire(&self)->&'static str {match self {Self::AutoTime(_)=>"auto_time",Self::AutoZone(_)=>"auto_zone",Self::Clock{..}=>"clock",Self::Zone(_)=>"zone"}}
    pub fn value(&self)->String {match self {Self::AutoTime(v)|Self::AutoZone(v)=>if *v {"on"}else{"off"}.into(),Self::Clock{civil,..}=>civil.clone(),Self::Zone(zone)=>zone.clone()}}
    pub fn valid(&self)->bool {match self {Self::Clock{civil,..}=>valid_civil(civil,1970,9999),Self::Zone(zone)=>valid_zone(zone),_=>true}}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct TimeRequest {pub key:TimeKey,pub action:TimeAction}
#[derive(serde::Serialize, Clone,Debug)]
pub struct TimeSnapshot {
    pub key:TimeKey,pub civil:String,pub zone:String,pub auto_time:bool,pub auto_zone:bool,
    pub can_auto_time:bool,pub can_auto_zone:bool,pub can_clock:bool,pub can_zone:bool,
    pub minimum_year:i64,pub maximum_year:i64,
}
impl TimeSnapshot {
    pub fn decode(value:&Value)->Option<Self> {
        if value.get("schema")?.as_i64()?!=1{return None;}
        let minimum_year=value.get("minimum_year")?.as_i64()?;let maximum_year=value.get("maximum_year")?.as_i64()?;
        if !(1970..=9999).contains(&minimum_year)||!(minimum_year..=9999).contains(&maximum_year){return None;}
        let civil=value.get("civil")?.as_str()?.to_owned();let zone=value.get("zone")?.as_str()?.to_owned();
        // The observed clock can lie outside the range Android permits editing.
        if !valid_civil(&civil,1,9999)||!valid_zone(&zone){return None;}
        Some(Self{key:TimeKey::decode(value.get("key")?)?,civil,zone,minimum_year,maximum_year,
            auto_time:value.get("auto_time")?.as_bool()?,auto_zone:value.get("auto_zone")?.as_bool()?,
            can_auto_time:value.get("can_auto_time")?.as_bool()?,can_auto_zone:value.get("can_auto_zone")?.as_bool()?,
            can_clock:value.get("can_clock")?.as_bool()?,can_zone:value.get("can_zone")?.as_bool()?})
    }
    pub fn permits(&self,request:&TimeRequest,zones:&[String])->bool {
        self.key==request.key&&request.action.valid()&&match &request.action {
            TimeAction::AutoTime(_)=>self.can_auto_time,TimeAction::AutoZone(_)=>self.can_auto_zone,
            TimeAction::Clock{civil,..}=>self.can_clock&&!self.auto_time&&valid_civil(civil,self.minimum_year,self.maximum_year),
            TimeAction::Zone(zone)=>self.can_zone&&!self.auto_zone&&zones.contains(zone),
        }
    }
}
pub fn valid_zone(value:&str)->bool {!value.is_empty()&&value.len()<=100&&value.bytes().all(|b|b.is_ascii_alphanumeric()||b"/_+-".contains(&b))}
pub fn zones(value:Option<&Value>)->Vec<String> {
    let Some(rows)=value.and_then(Value::as_arr).filter(|rows|rows.len()<=1024) else{return vec![];};
    let Some(zones):Option<Vec<String>>=rows.iter().map(|v|v.as_str().filter(|s|valid_zone(s)).map(str::to_owned)).collect()else{return vec![];};
    if zones.windows(2).any(|pair|pair[0]>=pair[1]) {vec![]} else {zones}
}
pub fn valid_civil(value:&str,minimum_year:i64,maximum_year:i64)->bool {
    let bytes=value.as_bytes();if bytes.len()!=16{return false;}
    if bytes[4]!=b'-'||bytes[7]!=b'-'||bytes[10]!=b' '||bytes[13]!=b':'||bytes.iter().enumerate().any(|(i,c)|![4,7,10,13].contains(&i)&&!c.is_ascii_digit()){return false;}
    let number=|a:usize,b:usize|value[a..b].parse::<i64>().unwrap_or(-1);
    let (year,month,day,hour,minute)=(number(0,4),number(5,7),number(8,10),number(11,13),number(14,16));
    let leap=year%4==0&&(year%100!=0||year%400==0);
    let days=match month {1|3|5|7|8|10|12=>31,4|6|9|11=>30,2=>if leap {29}else{28},_=>0};
    (minimum_year..=maximum_year).contains(&year)&&(1..=days).contains(&day)&&(0..=23).contains(&hour)&&(0..=59).contains(&minute)
}
#[derive(serde::Serialize, Clone,Debug)]
pub struct ClockDraft {pub key:TimeKey,pub civil:String,pub second:bool}
impl ClockDraft {
    pub fn new(state:&TimeSnapshot)->Self {Self{key:state.key.clone(),civil:state.civil.clone(),second:false}}
    pub fn request(&self)->TimeRequest {TimeRequest{key:self.key.clone(),action:TimeAction::Clock{civil:self.civil.clone(),second:self.second}}}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn calendar_and_time_are_strict_and_bounded() {
        for civil in ["2024-02-29 23:59","2000-02-29 00:00"] {assert!(valid_civil(civil,1970,2099));}
        for civil in ["2100-02-29 00:00","2026-02-29 12:00","2026-04-31 12:00","2026-09-24 24:00","2026-09-24 12:60","2026-9-24 12:00","２０２６-09-24 12:00","2026-09-24T12:00","1969-12-31 12:00"] {assert!(!valid_civil(civil,1970,2099),"{civil}");}
    }
    #[test] fn malformed_zone_catalog_never_authorizes_arbitrary_values() {
        for source in [r#"["America/Los_Angeles","America/Los_Angeles"]"#,r#"["UTC","Europe/London"]"#,r#"["UTC",null]"#,r#"["UTC\n"]"#] {assert!(zones(Some(&makepad_strict_json::parse(source.as_bytes()).unwrap())).is_empty());}
    }
}
