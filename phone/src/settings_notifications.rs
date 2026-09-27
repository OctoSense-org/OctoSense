//! Bounded, read-only notification history. Values are plain text, never actions or scripts.
use makepad_strict_json::Value;
pub const PAGE_SIZE:u32=20;
pub const MAX_ROWS:u32=2000;
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct HistoryKey(String);
impl HistoryKey {
    pub fn wire(&self)->&str {&self.0}
    pub(crate) fn decode(value:&Value)->Option<Self> {let text=value.as_str()?;(text.len()==64&&text.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))).then(||Self(text.into()))}
}
#[derive(serde::Serialize, Clone,Debug,Default,PartialEq,Eq)]
pub struct HistoryRead {pub key:Option<HistoryKey>,pub offset:u32}
impl HistoryRead {
    pub fn valid(&self)->bool {self.offset<MAX_ROWS&&self.offset%PAGE_SIZE==0&&(self.offset==0||self.key.is_some())}
}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum HistoryStatus {Ready,Disabled,Restricted,Unavailable,Expired}
#[derive(serde::Serialize, Clone,Debug)]
pub struct HistoryRow {pub package:String,pub label:String,pub posted:i64,pub when:String,pub channel:String,pub title:String,pub text:String}
#[derive(serde::Serialize, Clone,Debug)]
pub struct HistorySnapshot {
    pub request_id:i64,pub status:HistoryStatus,pub enabled:Option<bool>,pub key:Option<HistoryKey>,
    pub offset:u32,pub total:u32,pub truncated:bool,pub rows:Vec<HistoryRow>,
}
fn text(value:&Value,name:&str,limit:usize)->Option<String> {
    let text=value.get(name)?.as_str()?;
    (text.chars().count()<=limit&&!text.chars().any(|ch|ch.is_control()&&ch!='\n'&&ch!='\t')).then(||text.into())
}
impl HistorySnapshot {
    pub fn decode(value:&Value)->Option<Self> {
        if value.get("schema")?.as_i64()?!=1{return None;}
        let request_id=value.get("request_id")?.as_i64().filter(|id|*id>0)?;
        let status=match value.get("status")?.as_str()? {"ready"=>HistoryStatus::Ready,"disabled"=>HistoryStatus::Disabled,"restricted"=>HistoryStatus::Restricted,"unavailable"=>HistoryStatus::Unavailable,"expired"=>HistoryStatus::Expired,_=>return None};
        let enabled=match value.get("enabled")? {Value::Null=>None,v=>Some(v.as_bool()?)};
        let key=match value.get("key")? {Value::Null=>None,v=>Some(HistoryKey::decode(v)?)};
        let offset=u32::try_from(value.get("offset")?.as_i64()?).ok()?;
        let total=u32::try_from(value.get("total")?.as_i64()?).ok()?;
        let truncated=value.get("truncated")?.as_bool()?;
        let raw=value.get("rows")?.as_arr()?;
        if offset>=MAX_ROWS||offset%PAGE_SIZE!=0||total>MAX_ROWS||raw.len()>PAGE_SIZE as usize{return None;}
        if status==HistoryStatus::Ready {
            if enabled!=Some(true)||key.is_none()||offset>total||offset==total&&offset!=0
                ||raw.len()!=((total-offset).min(PAGE_SIZE)) as usize{return None;}
        } else if key.is_some()||offset!=0||total!=0||truncated||!raw.is_empty()
            ||status==HistoryStatus::Disabled&&enabled!=Some(false)
            ||status==HistoryStatus::Restricted&&enabled.is_some()
            ||status==HistoryStatus::Expired&&enabled!=Some(true) {return None;}
        let mut rows:Vec<HistoryRow>=Vec::new();
        for raw in raw {
            let package=text(raw,"package",255)?;
            if package.is_empty()||!package.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'.'||b==b'_'){return None;}
            let posted=raw.get("posted")?.as_i64().filter(|v|*v>=0)?;
            if rows.last().is_some_and(|row|row.posted<posted){return None;}
            rows.push(HistoryRow{package,posted,label:text(raw,"label",64)?,when:text(raw,"when",96)?,channel:text(raw,"channel",64)?,title:text(raw,"title",128)?,text:text(raw,"text",512)?});
        }
        Some(Self{request_id,status,enabled,key,offset,total,truncated,rows})
    }
    pub fn page(&self,next:bool)->Option<HistoryRead> {
        if self.status!=HistoryStatus::Ready{return None;}
        let offset=if next {let offset=self.offset+PAGE_SIZE;if offset>=self.total{return None;}offset}
            else {self.offset.checked_sub(PAGE_SIZE)?};
        Some(HistoryRead{key:self.key.clone(),offset})
    }
    pub fn refresh_page(&self)->HistoryRead {HistoryRead{key:self.key.clone(),offset:self.offset}}
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;use makepad_strict_json::{obj,s};
    pub fn snapshot(id:i64,total:u32,offset:u32)->Value {
        obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(id)),("status",s("ready")),("enabled",Value::Bool(true)),("key",s("a".repeat(64))),
            ("offset",Value::Int(offset as i64)),("total",Value::Int(total as i64)),("truncated",Value::Bool(false)),("rows",Value::Arr((offset..(offset+PAGE_SIZE).min(total)).map(|index|
                obj(vec![("package",s("dev.example.mail")),("label",s("Mail")),("posted",Value::Int(2000-index as i64)),("when",s("Today")),("channel",s("Inbox")),("title",s(format!("Message {index}"))),("text",s("中文内容 📨"))])).collect()))])
    }
    fn field(v:&mut Value,name:&str,value:Value) {let Value::Obj(rows)=v else{panic!()};rows.iter_mut().find(|(key,_)|key==name).unwrap().1=value;}
    #[test] fn history_pages_are_bounded_and_snapshot_bound() {
        let state=HistorySnapshot::decode(&snapshot(1,43,0)).unwrap();assert_eq!(state.rows.len(),20);assert!(state.page(false).is_none());
        let next=state.page(true).unwrap();assert_eq!(next.offset,20);assert!(next.valid());assert_eq!(next.key,state.key);
        let last=HistorySnapshot::decode(&snapshot(2,43,40)).unwrap();assert_eq!(last.rows.len(),3);assert!(last.page(true).is_none());
        assert!(!HistoryRead{key:None,offset:20}.valid());assert!(!HistoryRead{key:state.key.clone(),offset:2000}.valid());
        let mut value=snapshot(1,43,0);field(&mut value,"total",Value::Int(2001));assert!(HistorySnapshot::decode(&value).is_none());
        let mut value=snapshot(1,43,0);field(&mut value,"offset",Value::Int(1));assert!(HistorySnapshot::decode(&value).is_none());
        let mut value=snapshot(1,43,0);field(&mut value,"status",s("disabled"));assert!(HistorySnapshot::decode(&value).is_none());
    }
    #[test] fn history_denial_cannot_carry_old_content_and_text_is_bounded() {
        for (status,enabled) in [("disabled",Value::Bool(false)),("restricted",Value::Null),("unavailable",Value::Null),("expired",Value::Bool(true))] {
            let mut value=snapshot(1,0,0);field(&mut value,"key",Value::Null);field(&mut value,"status",s(status));field(&mut value,"enabled",enabled);
            assert!(HistorySnapshot::decode(&value).is_some());field(&mut value,"rows",snapshot(2,1,0).get("rows").unwrap().clone());assert!(HistorySnapshot::decode(&value).is_none());
        }
        let mut value=snapshot(1,1,0);let Value::Arr(mut rows)=value.get("rows").unwrap().clone()else{panic!()};
        field(&mut rows[0],"text",s("中".repeat(513)));field(&mut value,"rows",Value::Arr(rows));assert!(HistorySnapshot::decode(&value).is_none());
    }
}
