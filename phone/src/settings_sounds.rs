//! Finite sound types and page-bound catalog observations. Android owns every media URI.
use makepad_strict_json::{s,Value};
pub const PAGE_SIZE:u32=20;
pub const MAX_ROWS:u32=1000;
#[derive(serde::Serialize, Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum SoundType {#[default] Ringtone,Notification,Alarm}
impl SoundType {
    pub const ALL:[Self;3]=[Self::Ringtone,Self::Notification,Self::Alarm];
    pub fn wire(self)->&'static str {match self{Self::Ringtone=>"ringtone",Self::Notification=>"notification",Self::Alarm=>"alarm"}}
    pub fn title(self)->&'static str {match self{Self::Ringtone=>"Phone ringtone",Self::Notification=>"Notification sound",Self::Alarm=>"Default alarm sound"}}
    fn decode(value:&Value)->Option<Self> {Self::ALL.into_iter().find(|kind|Some(kind.wire())==value.as_str())}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct SoundKey(String);
impl SoundKey {
    pub fn wire(&self)->&str{&self.0}
    pub(crate) fn decode(value:&Value)->Option<Self> {let text=value.as_str()?;(text.len()==64&&text.bytes().all(|c|c.is_ascii_digit()||(b'a'..=b'f').contains(&c))).then(||Self(text.into()))}
}
#[derive(serde::Serialize, Clone,Debug,Default,PartialEq,Eq)]
pub struct SoundsRead {pub kind:SoundType,pub key:Option<SoundKey>,pub offset:u32}
impl SoundsRead {pub fn valid(&self)->bool{self.offset<MAX_ROWS&&self.offset%PAGE_SIZE==0&&(self.offset==0||self.key.is_some())}}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub enum SoundsRequest {Snapshot(SoundsRead),Preview{kind:SoundType,key:SoundKey,target:SoundKey},Save{kind:SoundType,key:SoundKey,target:SoundKey},Stop,Access}
impl SoundsRequest {
    pub fn command(&self)->(&'static str,Vec<(&'static str,Value)>) {
        match self {
            Self::Snapshot(read)=>("sounds_snapshot",vec![("type",s(read.kind.wire())),("key",read.key.as_ref().map(|key|s(key.wire())).unwrap_or(Value::Null)),("offset",Value::Int(read.offset as i64))]),
            Self::Preview{kind,key,target}|Self::Save{kind,key,target}=>(if matches!(self,Self::Preview{..}){"sound_preview"}else{"sound_save"},vec![("type",s(kind.wire())),("key",s(key.wire())),("target",s(target.wire()))]),
            Self::Stop=>("sound_stop",vec![]),Self::Access=>("sounds_access",vec![]),
        }
    }
}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum SoundsStatus {Ready,Restricted,Unavailable,Expired}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum CurrentSound {Sound,Silent,Unavailable}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct SoundRow {pub key:SoundKey,pub title:String,pub silent:bool,pub selected:bool}
#[derive(serde::Serialize, Clone,Debug)]
pub struct SoundsSnapshot {
    pub request_id:i64,pub kind:SoundType,pub status:SoundsStatus,pub key:Option<SoundKey>,pub current:CurrentSound,pub title:Option<String>,
    pub offset:u32,pub total:u32,pub truncated:bool,pub can_save:bool,pub can_preview:bool,pub can_request_access:bool,pub rows:Vec<SoundRow>,
}
fn display(value:&Value)->Option<String> {let value=value.as_str()?;(!value.is_empty()&&value.chars().count()<=128&&!value.chars().any(char::is_control)).then(||value.into())}
impl SoundsSnapshot {
    pub fn decode(value:&Value)->Option<Self> {
        if value.get("schema")?.as_i64()?!=1{return None;}
        let request_id=value.get("request_id")?.as_i64().filter(|id|*id>0)?;
        let kind=SoundType::decode(value.get("type")?)?;
        let status=match value.get("status")?.as_str()? {"ready"=>SoundsStatus::Ready,"restricted"=>SoundsStatus::Restricted,"unavailable"=>SoundsStatus::Unavailable,"expired"=>SoundsStatus::Expired,_=>return None};
        let key=match value.get("key")? {Value::Null=>None,key=>Some(SoundKey::decode(key)?)};
        let current=match value.get("current")?.get("state")?.as_str()? {"sound"=>CurrentSound::Sound,"silent"=>CurrentSound::Silent,"unavailable"=>CurrentSound::Unavailable,_=>return None};
        let title=match value.get("current")?.get("title")? {Value::Null=>None,title=>Some(display(title)?)};
        let offset=u32::try_from(value.get("offset")?.as_i64()?).ok()?;let total=u32::try_from(value.get("total")?.as_i64()?).ok()?;
        let truncated=value.get("truncated")?.as_bool()?;let can_save=value.get("can_save")?.as_bool()?;let can_preview=value.get("can_preview")?.as_bool()?;let can_request_access=value.get("can_request_access")?.as_bool()?;
        let raw=value.get("rows")?.as_arr()?;
        if offset>=MAX_ROWS||offset%PAGE_SIZE!=0||total>MAX_ROWS||raw.len()>PAGE_SIZE as usize{return None;}
        if status==SoundsStatus::Ready {
            if key.is_none()||current==CurrentSound::Unavailable||total==0||offset>=total||raw.len()!=((total-offset).min(PAGE_SIZE)) as usize||can_save&&can_request_access{return None;}
        } else if key.is_some()||current!=CurrentSound::Unavailable||title.is_some()||offset!=0||total!=0||truncated||can_save||can_preview||can_request_access||!raw.is_empty(){return None;}
        let mut rows:Vec<SoundRow>=Vec::new();
        for row in raw {
            let row=SoundRow{key:SoundKey::decode(row.get("key")?)?,title:display(row.get("title")?)?,silent:row.get("silent")?.as_bool()?,selected:row.get("selected")?.as_bool()?};
            if rows.iter().any(|old|old.key==row.key||old.selected&&row.selected)||row.silent&&(offset!=0||!rows.is_empty())||row.selected&&(row.silent!=(current==CurrentSound::Silent)){return None;}
            rows.push(row);
        }
        if status==SoundsStatus::Ready&&offset==0&&!rows.first()?.silent{return None;}
        Some(Self{request_id,kind,status,key,current,title,offset,total,truncated,can_save,can_preview,can_request_access,rows})
    }
    pub fn page(&self,next:bool)->Option<SoundsRead> {
        if self.status!=SoundsStatus::Ready{return None;}
        let offset=if next{let offset=self.offset+PAGE_SIZE;if offset>=self.total{return None;}offset}else{self.offset.checked_sub(PAGE_SIZE)?};
        Some(SoundsRead{kind:self.kind,key:self.key.clone(),offset})
    }
    pub fn read(&self)->SoundsRead{SoundsRead{kind:self.kind,key:self.key.clone(),offset:self.offset}}
    pub fn permits(&self,request:&SoundsRequest)->bool {
        match request {
            SoundsRequest::Stop=>true,
            SoundsRequest::Access=>self.status==SoundsStatus::Ready&&self.can_request_access,
            SoundsRequest::Preview{kind,key,target}|SoundsRequest::Save{kind,key,target}=>self.status==SoundsStatus::Ready&&self.kind==*kind&&self.key.as_ref()==Some(key)
                &&self.rows.iter().any(|row|row.key==*target)&&if matches!(request,SoundsRequest::Save{..}){self.can_save}else{self.can_preview},
            SoundsRequest::Snapshot(_)=>false,
        }
    }
    pub fn retire_actions(&mut self){self.can_save=false;self.can_preview=false;self.can_request_access=false;}
}
/// Selecting a row changes only a local draft. Replacing the catalog never rebinds that choice.
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct SoundDraft {pub kind:SoundType,pub catalog:SoundKey,pub row:SoundRow}
impl SoundDraft {
    pub fn select(state:&SoundsSnapshot,target:&SoundKey)->Option<Self>{Some(Self{kind:state.kind,catalog:state.key.clone()?,row:state.rows.iter().find(|row|row.key==*target)?.clone()})}
    pub fn request(&self,save:bool)->SoundsRequest {if save{SoundsRequest::Save{kind:self.kind,key:self.catalog.clone(),target:self.row.key.clone()}}else{SoundsRequest::Preview{kind:self.kind,key:self.catalog.clone(),target:self.row.key.clone()}}}
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;use makepad_strict_json::obj;
    pub fn snapshot(id:i64,total:u32,offset:u32)->Value {obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(id)),("type",s("ringtone")),("status",s("ready")),("key",s("a".repeat(64))),
        ("current",obj(vec![("state",s("silent")),("title",s("Silent"))])),("offset",Value::Int(offset as i64)),("total",Value::Int(total as i64)),("truncated",Value::Bool(false)),("can_save",Value::Bool(true)),("can_preview",Value::Bool(true)),("can_request_access",Value::Bool(false)),
        ("rows",Value::Arr((offset..(offset+PAGE_SIZE).min(total)).map(|i|obj(vec![("key",s(format!("{i:064x}"))),("title",s(if i==0{"Silent".into()}else{format!("Sound {i} 中")})),("silent",Value::Bool(i==0)),("selected",Value::Bool(i==0))])).collect()))])}
    fn field(v:&mut Value,name:&str,value:Value){let Value::Obj(rows)=v else{panic!()};rows.iter_mut().find(|(key,_)|key==name).unwrap().1=value;}
    #[test] fn sound_pages_are_bounded_and_targets_cannot_cross_catalog_or_type() {
        let state=SoundsSnapshot::decode(&snapshot(1,43,0)).unwrap();assert_eq!(state.rows.len(),20);assert!(!state.page(false).is_some());assert!(state.page(true).unwrap().valid());
        assert_eq!(SoundsSnapshot::decode(&snapshot(2,43,40)).unwrap().rows.len(),3);
        assert!(!SoundsRead{offset:1000,..Default::default()}.valid());assert!(!SoundsRead{offset:20,..Default::default()}.valid());
        let draft=SoundDraft::select(&state,&state.rows[1].key).unwrap();assert!(state.permits(&draft.request(true)));
        let mut other=state.clone();other.kind=SoundType::Alarm;assert!(!other.permits(&draft.request(true)));other=state.clone();other.key=Some(SoundKey("b".repeat(64)));assert!(!other.permits(&draft.request(true)));
        other=state.clone();other.retire_actions();assert!(!other.permits(&draft.request(true)));assert!(other.permits(&SoundsRequest::Stop));assert_eq!(draft.row.title,"Sound 1 中");
    }
    #[test] fn malformed_or_denied_sound_snapshots_never_authorize_cached_rows() {
        for (name,value) in [("status",s("expired")),("total",Value::Int(1001)),("offset",Value::Int(1)),("can_request_access",Value::Bool(true)),("key",s("content://media/internal/audio/media/1"))] {
            let mut raw=snapshot(1,20,0);field(&mut raw,name,value);assert!(SoundsSnapshot::decode(&raw).is_none());
        }
        let mut raw=snapshot(1,2,0);let Value::Arr(mut rows)=raw.get("rows").unwrap().clone()else{panic!()};field(&mut rows[1],"title",s("字".repeat(129)));field(&mut raw,"rows",Value::Arr(rows));assert!(SoundsSnapshot::decode(&raw).is_none());
        let state=SoundsSnapshot::decode(&snapshot(1,20,0)).unwrap();let row=state.rows[1].clone();let draft=SoundDraft::select(&state,&row.key).unwrap();let next=SoundsSnapshot::decode(&snapshot(2,40,20)).unwrap();assert!(!next.permits(&draft.request(true)));assert_eq!(draft.row,row);
    }
}
