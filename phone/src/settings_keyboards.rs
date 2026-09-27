//! Observed Android keyboard inventory and finite platform-owned consent flows.
use makepad_strict_json::Value;
pub const PAGE_SIZE:u32=20;
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]pub struct KeyboardKey(String);
impl KeyboardKey{pub fn wire(&self)->&str{&self.0}pub(crate) fn decode(v:&Value)->Option<Self>{let s=v.as_str()?;(s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))).then(||Self(s.into()))}}
#[derive(serde::Serialize, Clone,Debug,Default,PartialEq,Eq)]pub struct KeyboardRead{pub query:String,pub offset:u32}
impl KeyboardRead{pub fn valid(&self)->bool{self.query.chars().count()<=80&&!self.query.chars().any(char::is_control)&&self.offset<128&&self.offset%PAGE_SIZE==0}}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]pub enum KeyboardOperation{Enable,Disable,Settings,Subtypes,ChooseDefault}
impl KeyboardOperation{pub fn wire(self)->&'static str{match self{Self::Enable=>"enable",Self::Disable=>"disable",Self::Settings=>"provider_settings",Self::Subtypes=>"subtypes",Self::ChooseDefault=>"choose_default"}}}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]pub enum KeyboardRequest{Snapshot(KeyboardRead),Flow{key:KeyboardKey,target:Option<KeyboardKey>,operation:KeyboardOperation}}
impl KeyboardRequest{pub fn valid(&self)->bool{match self{Self::Snapshot(r)=>r.valid(),Self::Flow{target,operation,..}=>(*operation==KeyboardOperation::ChooseDefault)==target.is_none()}}}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]pub enum KeyboardAvailability{Available,Restricted,Unavailable}
impl KeyboardAvailability{pub fn label(self)->&'static str{match self{Self::Available=>"Android manages keyboard warnings and current-keyboard selection.",Self::Restricted=>"Keyboard controls are unavailable while locked or restricted.",Self::Unavailable=>"Keyboard controls are unavailable on this installation."}}}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]pub struct KeyboardRow{pub target:KeyboardKey,pub id:String,pub package:String,pub label:String,pub summary:String,pub restriction:String,pub enabled:bool,pub selected:bool,pub system:bool,pub direct_boot:bool,pub can_enable:bool,pub can_disable:bool,pub can_settings:bool,pub can_subtypes:bool}
impl KeyboardRow{pub fn permits(&self,op:KeyboardOperation)->bool{match op{KeyboardOperation::Enable=>self.can_enable&&!self.enabled,KeyboardOperation::Disable=>self.can_disable&&self.enabled,KeyboardOperation::Settings=>self.can_settings,KeyboardOperation::Subtypes=>self.can_subtypes,KeyboardOperation::ChooseDefault=>false}}}
#[derive(serde::Serialize, Clone,Debug)]pub struct KeyboardSnapshot{pub request_id:i64,pub availability:KeyboardAvailability,pub key:Option<KeyboardKey>,pub default_id:Option<String>,pub can_choose_default:bool,pub query:String,pub offset:u32,pub total:u32,pub rows:Vec<KeyboardRow>,pub fresh:bool}
fn text(value:&Value,max:usize,empty:bool)->Option<String>{let s=value.as_str()?;((empty||!s.trim().is_empty())&&s.chars().count()<=max&&!s.chars().any(char::is_control)).then(||s.into())}
impl KeyboardSnapshot{
    pub fn decode(value:&Value)->Option<Self>{
        if value.get("schema")?.as_i64()?!=1||value.get("page_size")?.as_i64()?!=PAGE_SIZE as i64{return None;}
        let availability=match value.get("availability")?.as_str()?{"available"=>KeyboardAvailability::Available,"restricted"=>KeyboardAvailability::Restricted,"unavailable"=>KeyboardAvailability::Unavailable,_=>return None};
        let optional_key=|name|match value.get(name)?{Value::Null=>Some(None),v=>Some(Some(KeyboardKey::decode(v)?))};
        let key=optional_key("key")?;let default_id=match value.get("default_id")?{Value::Null=>None,v=>Some(text(v,512,true)?)};
        let count=|name|value.get(name)?.as_i64().filter(|n|*n>=0&&*n<=128).map(|n|n as u32);
        let mut rows=Vec::<KeyboardRow>::new();for item in value.get("rows")?.as_arr()?{
            let row=KeyboardRow{target:KeyboardKey::decode(item.get("target")?)?,id:text(item.get("id")?,512,false)?,package:text(item.get("package")?,256,false)?,label:text(item.get("label")?,256,false)?,summary:text(item.get("summary")?,2048,true)?,restriction:text(item.get("restriction")?,256,true)?,enabled:item.get("enabled")?.as_bool()?,selected:item.get("selected")?.as_bool()?,system:item.get("system")?.as_bool()?,direct_boot:item.get("direct_boot")?.as_bool()?,can_enable:item.get("can_enable")?.as_bool()?,can_disable:item.get("can_disable")?.as_bool()?,can_settings:item.get("can_settings")?.as_bool()?,can_subtypes:item.get("can_subtypes")?.as_bool()?};
            if rows.iter().any(|r|r.id==row.id||r.target==row.target)||row.can_enable&&row.enabled||row.can_disable&&!row.enabled||row.selected&&(!row.enabled||default_id.as_ref()!=Some(&row.id)){return None;}rows.push(row);
        }
        let out=Self{request_id:value.get("request_id")?.as_i64().filter(|n|*n>0)?,availability,key,default_id,can_choose_default:value.get("can_choose_default")?.as_bool()?,query:text(value.get("query")?,80,true)?,offset:count("offset")?,total:count("total")?,rows,fresh:true};
        if out.offset%PAGE_SIZE!=0||out.offset>=128{return None;}
        if availability==KeyboardAvailability::Available{if out.key.is_none()||out.offset>out.total||out.rows.len()!=(out.total-out.offset).min(PAGE_SIZE)as usize{return None;}}
        else if out.key.is_some()||out.default_id.is_some()||out.can_choose_default||out.total!=0||out.offset!=0||!out.rows.is_empty(){return None;}
        Some(out)
    }
    pub fn matches(&self,read:&KeyboardRead)->bool{self.query==read.query&&(self.offset==read.offset||self.offset==0&&read.offset>=self.total)}
    pub fn permits(&self,request:&KeyboardRequest)->bool{let KeyboardRequest::Flow{key,target,operation}=request else{return false;};self.fresh&&self.availability==KeyboardAvailability::Available&&self.key.as_ref()==Some(key)&&if *operation==KeyboardOperation::ChooseDefault{target.is_none()&&self.can_choose_default}else{target.as_ref().is_some_and(|target|self.rows.iter().any(|r|r.target==*target&&r.permits(*operation)))}}
    pub fn clear_actions(&mut self){self.fresh=false;self.can_choose_default=false;for row in &mut self.rows{row.can_enable=false;row.can_disable=false;row.can_settings=false;row.can_subtypes=false;}}
}

#[cfg(test)]pub(crate) mod tests{
    use super::*;
    pub fn snapshot(id:i64)->Value{makepad_strict_json::parse(format!(r#"{{"schema":1,"page_size":20,"request_id":{id},"availability":"available","reason":"none","key":"{:064x}","default_id":"native/.Ime","can_choose_default":true,"query":"","offset":0,"total":2,"rows":[{{"target":"{:064x}","id":"native/.Ime","package":"native","label":"Native keyboard","summary":"English","restriction":"Required keyboard","enabled":true,"selected":true,"system":true,"direct_boot":true,"can_enable":false,"can_disable":false,"can_settings":true,"can_subtypes":true}},{{"target":"{:064x}","id":"fixture/.Ime","package":"fixture","label":"Fixture keyboard","summary":"中文","restriction":"","enabled":false,"selected":false,"system":false,"direct_boot":false,"can_enable":true,"can_disable":false,"can_settings":true,"can_subtypes":false}}]}}"#,1,2,3).as_bytes()).unwrap()}
    #[test]fn observed_native_choices_are_finite_and_retire_without_fabricating_readback(){let mut s=KeyboardSnapshot::decode(&snapshot(1)).unwrap();let r=KeyboardRequest::Flow{key:s.key.clone().unwrap(),target:Some(s.rows[1].target.clone()),operation:KeyboardOperation::Enable};assert!(s.permits(&r));s.clear_actions();assert!(!s.permits(&r));assert!(!s.rows[1].enabled);assert_eq!(s.default_id.as_deref(),Some("native/.Ime"));}
    #[test]fn default_picker_never_accepts_caller_selected_target(){let s=KeyboardSnapshot::decode(&snapshot(1)).unwrap();let r=KeyboardRequest::Flow{key:s.key.clone().unwrap(),target:Some(s.rows[0].target.clone()),operation:KeyboardOperation::ChooseDefault};assert!(!r.valid());assert!(!s.permits(&r));let r=KeyboardRequest::Flow{key:s.key.clone().unwrap(),target:None,operation:KeyboardOperation::ChooseDefault};assert!(r.valid());assert!(s.permits(&r));}
    #[test]fn required_provider_and_wrong_operation_cannot_be_mutated(){let s=KeyboardSnapshot::decode(&snapshot(1)).unwrap();let r=KeyboardRequest::Flow{key:s.key.clone().unwrap(),target:Some(s.rows[0].target.clone()),operation:KeyboardOperation::Disable};assert!(!s.permits(&r));let r=KeyboardRequest::Flow{key:s.key.clone().unwrap(),target:Some(s.rows[1].target.clone()),operation:KeyboardOperation::Disable};assert!(!s.permits(&r));}
    #[test]fn keyboard_filters_and_offsets_match_native_bounds(){assert!(KeyboardRead{query:"😀".repeat(80),offset:120}.valid());assert!(!KeyboardRead{query:"😀".repeat(81),offset:0}.valid());assert!(!KeyboardRead{offset:128,..Default::default()}.valid());assert!(!KeyboardRead{offset:1,..Default::default()}.valid());}
}
