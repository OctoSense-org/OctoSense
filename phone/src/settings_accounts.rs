//! Account visibility is explicit. Opaque observed keys never contain credentials.
use makepad_strict_json::Value;
use std::collections::HashSet;
pub const PAGE_SIZE:usize=20;
pub const MAX_ACCOUNTS:usize=128;
pub const MAX_PROVIDERS:usize=128;
pub const MAX_AUTHORITIES:usize=64;
macro_rules! target {
    ($name:ident)=>{
        #[derive(serde::Serialize, Clone,Debug,PartialEq,Eq,Hash)] pub struct $name(String);
        impl $name {pub fn key(&self)->&str {&self.0}
            pub(crate) fn decode(value:&Value)->Option<Self> {let key=value.as_str()?;(key.len()==64&&key.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))).then(||Self(key.into()))}}
    }
}
target!(AccountTarget);target!(ProviderTarget);target!(AuthorityTarget);
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum Visibility {Full,Limited,Unavailable}
impl Visibility {
    fn decode(value:&Value)->Option<Self> {Some(match value.as_str()? {"full"=>Self::Full,"limited"=>Self::Limited,"unavailable"=>Self::Unavailable,_=>return None})}
    pub fn description(self)->&'static str {match self {
        Self::Full=>"Accounts for the current Android user.",
        Self::Limited=>"Only accounts visible to OctoSense are listed. Other accounts may exist on this phone.",
        Self::Unavailable=>"Account information is unavailable. Unlock the device and refresh.",
    }}
}
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)]
pub enum SyncAction {Automatic(bool),SyncNow,Cancel}
impl SyncAction {
    pub fn wire(self)->&'static str {match self {Self::Automatic(_)=>"auto",Self::SyncNow=>"sync_now",Self::Cancel=>"cancel"}}
    pub fn enabled(self)->Option<bool> {if let Self::Automatic(value)=self {Some(value)}else{None}}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub enum AccountsRead {Snapshot,Details(AccountTarget)}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub enum AccountsRequest {
    Read(AccountsRead),Master(bool),Access,Add(ProviderTarget),Remove(AccountTarget),
    Sync{account:AccountTarget,authority:AuthorityTarget,action:SyncAction},
}
fn optional<T>(value:Option<&Value>,decode:impl FnOnce(&Value)->Option<T>)->Option<Option<T>> {
    match value {None|Some(Value::Null)=>Some(None),Some(value)=>decode(value).map(Some)}
}
fn text(value:&Value)->Option<String> {let text=value.as_str()?;(text.chars().count()<=512&&!text.chars().any(char::is_control)).then(||text.to_owned())}
fn count(value:&Value)->Option<usize> {usize::try_from(value.as_i64()?).ok().filter(|count|*count<=100_000)}
fn request_id(value:&Value)->Option<i64> {(value.get("schema")?.as_i64()?==1).then_some(())?;value.get("request_id")?.as_i64().filter(|id|*id>0)}
fn bounded<'a>(value:&'a Value,rows:&str,total:&str,truncated:&str,max:usize)->Option<(&'a [Value],usize,bool)> {
    let rows=value.get(rows)?.as_arr()?;let total=count(value.get(total)?)?;let truncated=value.get(truncated)?.as_bool()?;
    (rows.len()==total.min(max)&&truncated==(total>max)).then_some((rows,total,truncated))
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct AccountRow {pub target:AccountTarget,pub name:String,pub account_type:String,pub label:String}
impl AccountRow {fn decode(value:&Value)->Option<Self> {Some(Self{target:AccountTarget::decode(value.get("key")?)?,name:text(value.get("name")?)?,account_type:text(value.get("type")?)?,label:text(value.get("label")?)?})}}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct ProviderRow {pub target:ProviderTarget,pub account_type:String,pub label:String,pub can_add:bool}
impl ProviderRow {fn decode(value:&Value)->Option<Self> {Some(Self{target:ProviderTarget::decode(value.get("key")?)?,account_type:text(value.get("type")?)?,label:text(value.get("label")?)?,can_add:value.get("can_add")?.as_bool()?})}}
#[derive(serde::Serialize, Clone,Debug)]
pub struct AccountsSnapshot {
    pub request_id:i64,pub visibility:Visibility,pub master_sync:Option<bool>,pub can_set_master_sync:bool,pub can_request_access:bool,
    pub accounts:Vec<AccountRow>,pub accounts_total:usize,pub accounts_truncated:bool,
    pub providers:Vec<ProviderRow>,pub providers_total:usize,pub providers_truncated:bool,
}
impl AccountsSnapshot {
    pub fn decode(value:&Value)->Option<Self> {
        let request_id=request_id(value)?;let visibility=Visibility::decode(value.get("visibility")?)?;
        let master_sync=optional(value.get("master_sync"),Value::as_bool)?;let can_set_master_sync=value.get("can_set_master_sync")?.as_bool()?;
        let can_request_access=value.get("can_request_access")?.as_bool()?;
        let (rows,accounts_total,accounts_truncated)=bounded(value,"accounts","accounts_total","accounts_truncated",MAX_ACCOUNTS)?;
        let accounts=rows.iter().map(AccountRow::decode).collect::<Option<Vec<_>>>()?;
        let (rows,providers_total,providers_truncated)=bounded(value,"providers","providers_total","providers_truncated",MAX_PROVIDERS)?;
        let providers=rows.iter().map(ProviderRow::decode).collect::<Option<Vec<_>>>()?;
        let mut keys=HashSet::new();if accounts.iter().any(|row|!keys.insert(&row.target)) {return None;}
        let mut keys=HashSet::new();if providers.iter().any(|row|!keys.insert(&row.target)) {return None;}
        if can_set_master_sync&&master_sync.is_none()||visibility==Visibility::Unavailable&&(!accounts.is_empty()||!providers.is_empty()||can_set_master_sync||can_request_access) {return None;}
        Some(Self{request_id,visibility,master_sync,can_set_master_sync,can_request_access,accounts,accounts_total,accounts_truncated,providers,providers_total,providers_truncated})
    }
    pub fn permits(&self,request:&AccountsRequest)->bool {match request {
        AccountsRequest::Master(_)=>self.master_sync.is_some()&&self.can_set_master_sync,
        AccountsRequest::Access=>self.can_request_access,
        AccountsRequest::Add(target)=>self.providers.iter().any(|row|&row.target==target&&row.can_add),
        _=>false,
    }}
    pub fn clear_actions(&mut self) {self.can_set_master_sync=false;self.can_request_access=false;for row in &mut self.providers {row.can_add=false;}}
}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub struct AuthorityRow {
    pub target:AuthorityTarget,pub authority:String,pub label:String,pub automatic:Option<bool>,pub syncable:Option<bool>,pub active:Option<bool>,pub pending:Option<bool>,
    pub can_automatic:bool,pub can_sync:bool,pub can_cancel:bool,
}
impl AuthorityRow {
    fn decode(value:&Value)->Option<Self> {
        let automatic=optional(value.get("automatic"),Value::as_bool)?;let syncable=optional(value.get("syncable"),Value::as_bool)?;
        let active=optional(value.get("active"),Value::as_bool)?;let pending=optional(value.get("pending"),Value::as_bool)?;
        let actions=value.get("actions")?.as_arr()?;if actions.len()>3 {return None;}
        let mut keys=HashSet::new();for action in actions {let name=action.as_str()?;if !["auto","sync_now","cancel"].contains(&name)||!keys.insert(name) {return None;}}
        let can_automatic=keys.contains("auto");let can_sync=keys.contains("sync_now");let can_cancel=keys.contains("cancel");
        if can_automatic&&(automatic.is_none()||syncable!=Some(true))||can_sync&&syncable!=Some(true)||can_cancel&&active!=Some(true)&&pending!=Some(true) {return None;}
        Some(Self{target:AuthorityTarget::decode(value.get("key")?)?,authority:text(value.get("authority")?)?,label:text(value.get("label")?)?,automatic,syncable,active,pending,can_automatic,can_sync,can_cancel})
    }
    pub fn permits(&self,action:SyncAction)->bool {match action {SyncAction::Automatic(_)=>self.can_automatic,SyncAction::SyncNow=>self.can_sync,SyncAction::Cancel=>self.can_cancel}}
    fn clear_actions(&mut self) {self.can_automatic=false;self.can_sync=false;self.can_cancel=false;}
}
#[derive(serde::Serialize, Clone,Debug)]
pub struct AccountDetails {
    pub request_id:i64,pub target:AccountTarget,pub exists:bool,pub visibility:Visibility,pub name:Option<String>,pub account_type:Option<String>,pub label:Option<String>,
    pub can_remove:bool,pub authorities_available:bool,pub authorities:Vec<AuthorityRow>,pub authorities_total:usize,pub truncated:bool,
}
impl AccountDetails {
    pub fn decode(value:&Value)->Option<Self> {
        let request_id=request_id(value)?;let target=AccountTarget::decode(value.get("key")?)?;let exists=value.get("exists")?.as_bool()?;
        let visibility=Visibility::decode(value.get("visibility")?)?;let can_remove=value.get("can_remove")?.as_bool()?;let authorities_available=value.get("authorities_available")?.as_bool()?;
        let (rows,authorities_total,truncated)=bounded(value,"authorities","authorities_total","truncated",MAX_AUTHORITIES)?;
        let authorities=rows.iter().map(AuthorityRow::decode).collect::<Option<Vec<_>>>()?;let mut keys=HashSet::new();if authorities.iter().any(|row|!keys.insert(&row.target)) {return None;}
        if can_remove&&(!exists||visibility!=Visibility::Full)||!authorities_available&&!authorities.is_empty()||!exists&&(authorities_available||!authorities.is_empty()) {return None;}
        let name=optional(value.get("name"),text)?;let account_type=optional(value.get("type"),text)?;let label=optional(value.get("label"),text)?;
        if exists&&(name.is_none()||account_type.is_none()||label.is_none()) {return None;}
        Some(Self{request_id,target,exists,visibility,name,account_type,label,can_remove,authorities_available,authorities,authorities_total,truncated})
    }
    pub fn authority(&self,target:&AuthorityTarget)->Option<&AuthorityRow> {self.authorities.iter().find(|row|&row.target==target)}
    pub fn permits(&self,request:&AccountsRequest)->bool {
        if !self.exists {return false;}
        match request {AccountsRequest::Remove(target)=>&self.target==target&&self.can_remove,
            AccountsRequest::Sync{account,authority,action}=>account==&self.target&&self.authority(authority).is_some_and(|row|row.permits(*action)),_=>false}
    }
    pub fn clear_actions(&mut self) {self.can_remove=false;for row in &mut self.authorities {row.clear_actions();}}
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;use makepad_strict_json::{obj,s};
    pub fn snapshot(id:i64,count:usize)->Value {obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(id)),("visibility",s("limited")),("master_sync",Value::Bool(true)),("can_set_master_sync",Value::Bool(true)),("can_request_access",Value::Bool(true)),
        ("accounts_total",Value::Int(count as i64)),("accounts_truncated",Value::Bool(count>MAX_ACCOUNTS)),("accounts",Value::Arr((0..count.min(MAX_ACCOUNTS)).map(|index|obj(vec![("key",s(format!("{index:064x}"))),("name",s(format!("Account {index}"))),("type",s("mail")),("label",s("Mail"))])).collect())),
        ("providers_total",Value::Int(1)),("providers_truncated",Value::Bool(false)),("providers",Value::Arr(vec![obj(vec![("key",s(format!("{:064x}",500))),("type",s("mail")),("label",s("Mail")),("can_add",Value::Bool(true))])]))])}
    pub fn details(id:i64,account:usize,count:usize)->Value {obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(id)),("key",s(format!("{account:064x}"))),("exists",Value::Bool(true)),("visibility",s("full")),("name",s(format!("Account {account}"))),("type",s("mail")),("label",s("Mail")),("can_remove",Value::Bool(true)),("authorities_available",Value::Bool(true)),
        ("authorities_total",Value::Int(count as i64)),("truncated",Value::Bool(count>MAX_AUTHORITIES)),("authorities",Value::Arr((0..count.min(MAX_AUTHORITIES)).map(|index|obj(vec![("key",s(format!("{:064x}",1000+index))),("authority",s(format!("mail.sync{index}"))),("label",s(format!("Sync service {index}"))),("automatic",Value::Bool(true)),("syncable",Value::Bool(true)),("active",Value::Bool(false)),("pending",Value::Bool(true)),("actions",Value::Arr(vec![s("auto"),s("sync_now"),s("cancel")]))])).collect()))])}
    pub fn field(value:&mut Value,key:&str,replacement:Value) {let Value::Obj(fields)=value else {panic!()};fields.iter_mut().find(|(name,_)|name==key).unwrap().1=replacement;}
    #[test] fn account_visibility_and_inventory_bounds_are_not_inferred() {
        for count in [0,1,128,129] {let state=AccountsSnapshot::decode(&snapshot(1,count)).unwrap();assert_eq!(state.visibility,Visibility::Limited);}
        let mut value=snapshot(1,1);field(&mut value,"accounts_total",Value::Int(2));assert!(AccountsSnapshot::decode(&value).is_none());
        let mut value=snapshot(1,0);field(&mut value,"master_sync",Value::Null);assert!(AccountsSnapshot::decode(&value).is_none());
        field(&mut value,"can_set_master_sync",Value::Bool(false));assert!(AccountsSnapshot::decode(&value).is_some());
        assert!(AccountTarget::decode(&s("alice@example.com")).is_none());
    }
    #[test] fn account_and_authority_actions_need_the_exact_observation() {
        let mut state=AccountDetails::decode(&details(1,0,1)).unwrap();let account=state.target.clone();let authority=state.authorities[0].target.clone();
        let request=AccountsRequest::Sync{account,authority,action:SyncAction::Automatic(false)};
        assert!(state.permits(&request));state.clear_actions();assert!(!state.permits(&request));assert_eq!(state.authorities[0].automatic,Some(true));
        let mut value=details(1,0,1);field(&mut value,"visibility",s("limited"));assert!(AccountDetails::decode(&value).is_none(),"limited visibility does not authorize removal");
        let mut value=details(1,0,1);let Value::Arr(mut rows)=value.get("authorities").unwrap().clone() else {panic!()};field(&mut rows[0],"syncable",Value::Null);field(&mut value,"authorities",Value::Arr(rows));assert!(AccountDetails::decode(&value).is_none());
    }
}
