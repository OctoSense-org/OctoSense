//! Reviewed update offers and finite actions. Download URLs never enter the script contract.
use makepad_strict_json::Value;
use std::collections::HashSet;
macro_rules! key {
    ($name:ident)=>{
        #[derive(serde::Serialize, Clone,Debug,PartialEq,Eq,Hash)] pub struct $name(String);
        impl $name {pub fn key(&self)->&str {&self.0} fn decode(value:&Value)->Option<Self> {let key=value.as_str()?;(key.len()==64&&key.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))).then(||Self(key.into()))}}
    }
}
key!(UpdateOfferKey);key!(UpdateRebootKey);
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)] pub enum UpdatePart {Rom,Home}
impl UpdatePart {pub fn wire(self)->&'static str {match self {Self::Rom=>"rom",Self::Home=>"home"}}pub fn label(self)->&'static str {match self {Self::Rom=>"System",Self::Home=>"OctoSense Home"}}}
#[derive(serde::Serialize, Clone,Debug,PartialEq,Eq)]
pub enum UpdatesRequest {Snapshot,Check,Install{part:UpdatePart,offer:UpdateOfferKey},Reboot(UpdateRebootKey)}
macro_rules! vocabulary {
    ($name:ident {$($variant:ident=>($wire:literal,$label:literal)),+ $(,)?})=>{
        #[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq)] pub enum $name {$($variant),+}
        impl $name {fn decode(value:&Value)->Option<Self> {Some(match value.as_str()? {$($wire=>Self::$variant,)+_=>return None})}pub fn label(self)->&'static str {match self {$(Self::$variant=>$label,)+}}}
    }
}
vocabulary!(UpdateAvailability {Available=>("available","System update service available"),Unavailable=>("unavailable","System update service is unavailable on this installation."),Restricted=>("restricted","System updates are restricted or the device is locked.")});
vocabulary!(UpdateCheckState {Never=>("never","Not checked yet"),Checking=>("checking","Checking for updates…"),Checked=>("checked","Check completed"),Failed=>("failed","Update check failed")});
vocabulary!(RomUpdatePhase {
    Idle=>("idle","Idle"),Checking=>("checking","Checking"),Available=>("update_available","Update available"),Starting=>("starting","Starting"),Downloading=>("downloading","Downloading"),
    Verifying=>("verifying","Verifying"),Finalizing=>("finalizing","Finalizing"),Reboot=>("updated_need_reboot","Restart required"),ReportingError=>("reporting_error_event","Reporting update error"),
    Rollback=>("attempting_rollback","Attempting rollback"),Disabled=>("disabled","Disabled"),Permission=>("need_permission_to_update","Permission required"),Cleanup=>("cleanup_previous_update","Cleaning up previous update"),
    Cancelled=>("cancelled","Cancelled"),Failed=>("failed","Failed"),Unavailable=>("unavailable","Unavailable"),Unknown=>("unknown","Unknown"),
});
vocabulary!(HomeUpdatePhase {Idle=>("idle","Idle"),Downloading=>("downloading","Downloading"),Verifying=>("verifying","Verifying"),Installing=>("installing","Installing"),Installed=>("installed","Installed"),Failed=>("failed","Failed"),Unavailable=>("unavailable","Unavailable"),Unknown=>("unknown","Unknown")});
#[derive(serde::Serialize, Clone,Copy,Debug,PartialEq,Eq,Hash)]pub enum UpdateCapability {Check,InstallRom,InstallHome,Reboot}
impl UpdateCapability {fn decode(value:&Value)->Option<Self> {Some(match value.as_str()? {"check"=>Self::Check,"install_rom"=>Self::InstallRom,"install_home"=>Self::InstallHome,"reboot"=>Self::Reboot,_=>return None})}}
fn optional<T>(value:Option<&Value>,decode:impl FnOnce(&Value)->Option<T>)->Option<Option<T>> {match value {None|Some(Value::Null)=>Some(None),Some(value)=>decode(value).map(Some)}}
fn text(value:&Value)->Option<String> {let text=value.as_str()?;(text.chars().count()<=512&&!text.chars().any(char::is_control)).then(||text.to_owned())}
fn integer(value:&Value)->Option<i64> {value.as_i64().filter(|value|*value>=0)}
fn progress(value:&Value)->Option<f64> {let value=match value {Value::Int(value)=>*value as f64,Value::F64(value)=>*value,_=>return None};(value.is_finite()&&(0.0..=1.0).contains(&value)).then_some(value)}
#[derive(serde::Serialize, Clone,Debug)]
pub struct UpdateOffer {pub key:UpdateOfferKey,pub release:String,pub rom_version:Option<String>,pub home_version:Option<i64>,pub rom_newer:bool,pub home_newer:bool}
impl UpdateOffer {fn decode(value:&Value)->Option<Self> {Some(Self{key:UpdateOfferKey::decode(value.get("key")?)?,release:text(value.get("release")?)?,rom_version:optional(value.get("rom_version"),text)?,home_version:optional(value.get("home_version"),integer)?,rom_newer:value.get("rom_newer")?.as_bool()?,home_newer:value.get("home_newer")?.as_bool()?})}}
#[derive(serde::Serialize, Clone,Debug)]
pub struct UpdatesSnapshot {
    pub request_id:i64,pub availability:UpdateAvailability,pub capabilities:HashSet<UpdateCapability>,
    pub rom_version:Option<String>,pub home_version:Option<i64>,pub device:Option<String>,pub model:Option<String>,pub slot:Option<String>,
    pub check_state:UpdateCheckState,pub checked_at_ms:Option<i64>,pub checked_at_text:Option<String>,pub check_error:Option<String>,
    pub offer:Option<UpdateOffer>,pub rom_phase:RomUpdatePhase,pub rom_progress:Option<f64>,pub rom_error:Option<String>,pub home_phase:HomeUpdatePhase,pub home_error:Option<String>,pub reboot_key:Option<UpdateRebootKey>,
}
impl UpdatesSnapshot {
    pub fn decode(value:&Value)->Option<Self> {
        if value.get("schema")?.as_i64()?!=1 {return None;}let request_id=value.get("request_id")?.as_i64().filter(|id|*id>0)?;
        let availability=UpdateAvailability::decode(value.get("availability")?)?;let current=value.get("current")?;let check=value.get("check")?;let rom=value.get("rom")?;let home=value.get("home")?;
        let caps=value.get("capabilities")?.as_arr()?;if caps.len()>4 {return None;}let capabilities=caps.iter().map(UpdateCapability::decode).collect::<Option<HashSet<_>>>()?;
        if caps.len()!=capabilities.len()||availability!=UpdateAvailability::Available&&!caps.is_empty() {return None;}
        let offer=optional(value.get("offer"),UpdateOffer::decode)?;let reboot_key=optional(value.get("reboot_key"),UpdateRebootKey::decode)?;let rom_phase=RomUpdatePhase::decode(rom.get("phase")?)?;
        if capabilities.contains(&UpdateCapability::InstallRom)&&!offer.as_ref().is_some_and(|offer|offer.rom_newer&&offer.rom_version.is_some())
            ||capabilities.contains(&UpdateCapability::InstallHome)&&!offer.as_ref().is_some_and(|offer|offer.home_newer&&offer.home_version.is_some())
            ||capabilities.contains(&UpdateCapability::Reboot)&&(rom_phase!=RomUpdatePhase::Reboot||reboot_key.is_none()) {return None;}
        Some(Self{request_id,availability,capabilities,rom_version:optional(current.get("rom_version"),text)?,home_version:optional(current.get("home_version"),integer)?,device:optional(current.get("device"),text)?,model:optional(current.get("model"),text)?,slot:optional(current.get("slot"),text)?,
            check_state:UpdateCheckState::decode(check.get("state")?)?,checked_at_ms:optional(check.get("checked_at_ms"),integer)?,checked_at_text:optional(check.get("checked_at_text"),text)?,check_error:optional(check.get("error"),text)?,offer,
            rom_phase,rom_progress:optional(rom.get("progress"),progress)?,rom_error:optional(rom.get("error"),text)?,home_phase:HomeUpdatePhase::decode(home.get("phase")?)?,home_error:optional(home.get("error"),text)?,reboot_key})
    }
    pub fn install_request(&self,part:UpdatePart)->Option<UpdatesRequest> {Some(UpdatesRequest::Install{part,offer:self.offer.as_ref()?.key.clone()})}
    pub fn permits(&self,request:&UpdatesRequest)->bool {match request {
        UpdatesRequest::Snapshot=>true,UpdatesRequest::Check=>self.capabilities.contains(&UpdateCapability::Check),
        UpdatesRequest::Install{part,offer}=>self.offer.as_ref().is_some_and(|current|&current.key==offer)&&self.capabilities.contains(&match part {UpdatePart::Rom=>UpdateCapability::InstallRom,UpdatePart::Home=>UpdateCapability::InstallHome}),
        UpdatesRequest::Reboot(key)=>self.reboot_key.as_ref()==Some(key)&&self.rom_phase==RomUpdatePhase::Reboot&&self.capabilities.contains(&UpdateCapability::Reboot),
    }}
    pub fn clear_actions(&mut self) {self.capabilities.clear();}
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;use makepad_strict_json::{obj,s};
    pub fn snapshot(id:i64)->Value {obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(id)),("availability",s("available")),("current",obj(vec![("rom_version",s("build1")),("home_version",Value::Int(100)),("model",s("Test phone")),("device",s("test")),("slot",s("_a"))])),
        ("check",obj(vec![("state",s("checked"))])),("offer",obj(vec![("key",s(format!("{:064x}",1))),("release",s("Release 2")),("rom_version",s("build2")),("home_version",Value::Int(200)),("rom_newer",Value::Bool(true)),("home_newer",Value::Bool(true))])),
        ("rom",obj(vec![("phase",s("idle"))])),("home",obj(vec![("phase",s("idle"))])),("capabilities",Value::Arr(vec![s("check"),s("install_rom"),s("install_home")]))])}
    pub fn field(value:&mut Value,key:&str,replacement:Value) {let Value::Obj(fields)=value else {panic!()};fields.iter_mut().find(|(name,_)|name==key).unwrap().1=replacement;}
    #[test]fn updates_only_authorize_the_exact_reviewed_offer() {
        let mut state=UpdatesSnapshot::decode(&snapshot(1)).unwrap();let request=state.install_request(UpdatePart::Rom).unwrap();assert!(state.permits(&request));
        let mut value=snapshot(2);let mut offer=value.get("offer").unwrap().clone();field(&mut offer,"key",s(format!("{:064x}",2)));field(&mut value,"offer",offer);assert!(!UpdatesSnapshot::decode(&value).unwrap().permits(&request));
        state.clear_actions();assert!(!state.permits(&request));assert_eq!(state.offer.unwrap().rom_version.as_deref(),Some("build2"));
    }
    #[test]fn unknown_update_data_never_authorizes_install_or_reboot() {
        let mut value=snapshot(1);field(&mut value,"offer",Value::Null);assert!(UpdatesSnapshot::decode(&value).is_none());
        field(&mut value,"capabilities",Value::Arr(vec![]));let state=UpdatesSnapshot::decode(&value).unwrap();assert!(state.rom_progress.is_none());assert!(state.checked_at_ms.is_none());
        field(&mut value,"capabilities",Value::Arr(vec![s("reboot")]));assert!(UpdatesSnapshot::decode(&value).is_none());
        let mut value=snapshot(1);field(&mut value,"availability",s("unavailable"));assert!(UpdatesSnapshot::decode(&value).is_none());
        let mut value=snapshot(1);field(&mut value,"rom",obj(vec![("phase",s("downloading")),("progress",Value::F64(1.01))]));assert!(UpdatesSnapshot::decode(&value).is_none());
    }
}
