//! Device-native caption locales. A reviewed opaque choice is the only mutation input.
use makepad_strict_json::Value;
pub const PAGE_SIZE:u32=24;
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]pub struct CaptionLanguageKey(String);
impl CaptionLanguageKey {
    pub fn wire(&self)->&str{&self.0}
    pub(crate) fn decode(value:&Value)->Option<Self>{let value=value.as_str()?;(value.len()==64&&value.bytes().all(|c|c.is_ascii_digit()||(b'a'..=b'f').contains(&c))).then(||Self(value.into()))}
}
#[derive(serde::Serialize, Clone,Debug,Default,PartialEq,Eq)]pub struct CaptionLanguageRead{pub key:Option<CaptionLanguageKey>,pub query:String,pub offset:u32}
impl CaptionLanguageRead{pub fn valid(&self)->bool{self.query.encode_utf16().count()<=80&&!self.query.chars().any(char::is_control)&&self.offset<=1024&&self.offset%PAGE_SIZE==0&&(self.key.is_some()||self.offset==0)}}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]pub enum CaptionLanguageRequest{Snapshot(CaptionLanguageRead),Select{key:CaptionLanguageKey,choice:CaptionLanguageKey}}
impl CaptionLanguageRequest{pub fn valid(&self)->bool{match self{Self::Snapshot(read)=>read.valid(),Self::Select{..}=>true}}}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]pub enum CaptionLanguageStatus{Ready,Restricted,Unavailable,Changed,PageChanged}
impl CaptionLanguageStatus{pub fn label(self)->&'static str{match self{Self::Ready=>"",Self::Restricted=>"Unlock the device to change caption language.",Self::Unavailable=>"Caption language controls are unavailable on this installation.",Self::Changed=>"Caption language or its available choices changed. Refresh to review them.",Self::PageChanged=>"These language results changed. Refresh to return to the first page."}}}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]pub struct CaptionLanguageRow{pub choice:CaptionLanguageKey,pub label:String,pub locale:String,pub selected:bool}
#[derive(serde::Serialize, Clone,Debug)]pub struct CaptionLanguageSnapshot{
    pub request_id:i64,pub status:CaptionLanguageStatus,pub key:Option<CaptionLanguageKey>,pub current:Option<String>,pub system_default:bool,pub custom:bool,pub query:String,pub offset:u32,pub total:u32,pub rows:Vec<CaptionLanguageRow>,pub can_set:bool,
}
fn text(value:&Value,max:usize,empty:bool)->Option<String>{let value=value.as_str()?;((empty||!value.is_empty())&&value.encode_utf16().count()<=max&&!value.chars().any(char::is_control)).then(||value.into())}
impl CaptionLanguageSnapshot{
    pub fn decode(value:&Value)->Option<Self>{
        if value.get("schema")?.as_i64()?!=1||value.get("page_size")?.as_i64()?!=PAGE_SIZE as i64{return None;}
        let status=match value.get("status")?.as_str()?{"ready"=>CaptionLanguageStatus::Ready,"policy_restricted"=>CaptionLanguageStatus::Restricted,"control_unavailable"=>CaptionLanguageStatus::Unavailable,"caption_language_changed"=>CaptionLanguageStatus::Changed,"caption_language_page_changed"=>CaptionLanguageStatus::PageChanged,_=>return None};
        let key=match value.get("key")?.as_str()?{""=>None,_=>Some(CaptionLanguageKey::decode(value.get("key")?)?)};
        let count=|name:&str,max:i64|value.get(name)?.as_i64().filter(|v|*v>=0&&*v<=max).map(|v|v as u32);
        let current=text(value.get("current")?,128,true)?;let query=text(value.get("query")?,80,true)?;
        let mut rows=Vec::<CaptionLanguageRow>::new();for entry in value.get("rows")?.as_arr()?{
            let row=CaptionLanguageRow{choice:CaptionLanguageKey::decode(entry.get("choice")?)?,label:text(entry.get("label")?,256,false)?,locale:text(entry.get("locale")?,128,true)?,selected:entry.get("selected")?.as_bool()?};
            if rows.iter().any(|other|other.choice==row.choice||other.locale==row.locale)||row.selected!=(row.locale==current){return None;}rows.push(row);
        }
        let mut out=Self{request_id:value.get("request_id")?.as_i64().filter(|id|*id>0)?,status,key,current:Some(current),system_default:value.get("system_default")?.as_bool()?,custom:value.get("custom")?.as_bool()?,query,offset:count("offset",1024)?,total:count("total",1025)?,rows,can_set:status==CaptionLanguageStatus::Ready};
        if out.offset%PAGE_SIZE!=0{return None;}
        if status==CaptionLanguageStatus::Ready{
            if out.key.is_none()||out.offset>out.total||out.rows.len()!=(out.total-out.offset).min(PAGE_SIZE)as usize||out.system_default!=out.current.as_ref()?.is_empty()||(out.custom&&(out.system_default||out.rows.iter().any(|r|r.selected))){return None;}
        }else{
            if out.key.is_some()||!out.current.as_ref()?.is_empty()||out.system_default||out.custom||out.total!=0||!out.rows.is_empty(){return None;}
            out.current=None;
        }
        Some(out)
    }
    pub fn matches(&self,read:&CaptionLanguageRead)->bool{self.query==read.query&&self.offset==read.offset&&(self.status!=CaptionLanguageStatus::Ready||read.key.is_none()||self.key==read.key)}
    pub fn permits(&self,request:&CaptionLanguageRequest)->bool{let CaptionLanguageRequest::Select{key,choice}=request else{return false;};self.status==CaptionLanguageStatus::Ready&&self.can_set&&self.key.as_ref()==Some(key)&&self.rows.iter().any(|row|row.choice==*choice)}
    pub fn clear_actions(&mut self){self.can_set=false;}
}
#[cfg(test)]pub(crate) mod tests{
    use super::*;
    pub fn snapshot(id:i64)->Value{makepad_strict_json::parse(format!(r#"{{"schema":1,"page_size":24,"request_id":{id},"status":"ready","key":"{:064x}","current":"fr_FR","system_default":false,"custom":false,"query":"","offset":0,"total":3,"rows":[{{"choice":"{:064x}","label":"System default","locale":"","selected":false}},{{"choice":"{:064x}","label":"Français","locale":"fr_FR","selected":true}},{{"choice":"{:064x}","label":"简体中文","locale":"zh_CN","selected":false}}]}}"#,1,2,3,4).as_bytes()).unwrap()}
    fn field(value:&mut Value,key:&str,next:Value){let Value::Obj(fields)=value else{panic!()};fields.iter_mut().find(|(name,_)|name==key).unwrap().1=next;}
    #[test]fn exact_native_locale_and_current_page_authority(){let mut s=CaptionLanguageSnapshot::decode(&snapshot(1)).unwrap();assert_eq!(s.current.as_deref(),Some("fr_FR"));let r=CaptionLanguageRequest::Select{key:s.key.clone().unwrap(),choice:s.rows[2].choice.clone()};assert!(s.permits(&r));s.clear_actions();assert!(!s.permits(&r));assert_eq!(s.current.as_deref(),Some("fr_FR"));}
    #[test]fn malformed_or_incomplete_catalog_is_not_actionable(){let mut v=snapshot(1);field(&mut v,"total",Value::Int(4));assert!(CaptionLanguageSnapshot::decode(&v).is_none());let mut v=snapshot(1);field(&mut v,"system_default",Value::Bool(true));assert!(CaptionLanguageSnapshot::decode(&v).is_none());let mut v=snapshot(1);field(&mut v,"custom",Value::Bool(true));assert!(CaptionLanguageSnapshot::decode(&v).is_none());let mut v=snapshot(1);field(&mut v,"status",makepad_strict_json::s("control_unavailable"));assert!(CaptionLanguageSnapshot::decode(&v).is_none());}
    #[test]fn unknown_current_locale_stays_custom_and_unmodified(){let mut v=snapshot(1);field(&mut v,"current",makepad_strict_json::s("custom_NATIVE"));field(&mut v,"custom",Value::Bool(true));let mut rows=v.get("rows").unwrap().clone();let Value::Arr(entries)=&mut rows else{panic!()};field(&mut entries[1],"selected",Value::Bool(false));field(&mut v,"rows",rows);let s=CaptionLanguageSnapshot::decode(&v).unwrap();assert!(s.custom);assert_eq!(s.current.as_deref(),Some("custom_NATIVE"));assert!(!s.system_default);}
    #[test]fn queries_and_pages_are_bounded(){let mut read=CaptionLanguageRead::default();assert!(read.valid());read.offset=24;assert!(!read.valid());read.key=CaptionLanguageSnapshot::decode(&snapshot(1)).unwrap().key;assert!(read.valid());read.offset=25;assert!(!read.valid());read.offset=0;read.query="🀄".repeat(41);assert!(!read.valid());}
}
