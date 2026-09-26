//! Bounded Bluetooth observations and finite device actions. No pairing secrets.
use makepad_strict_json::Value;
use std::collections::HashSet;

pub const MAX_DEVICES: usize = 256;
pub const PAGE_SIZE: usize = 20;
pub fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 248 && !name.chars().any(char::is_control)
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct BluetoothTarget(String);
impl BluetoothTarget {
    pub fn key(&self) -> &str { &self.0 }
    pub(crate) fn decode(value: &Value) -> Option<Self> {
        let key=value.as_str()?;
        (key.len()==64 && key.bytes().all(|c|c.is_ascii_digit()||(b'a'..=b'f').contains(&c))).then(||Self(key.into()))
    }
}

macro_rules! vocabulary {
    ($name:ident {$($variant:ident => ($wire:literal,$label:literal)),+ $(,)?}) => {
        #[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name { $($variant),+ }
        impl $name {
            fn decode(value:&Value)->Option<Self> {Some(match value.as_str()? {$($wire=>Self::$variant,)+ _=>return None})}
            pub fn label(self)->&'static str {match self {$(Self::$variant=>$label,)+}}
        }
    }
}
vocabulary!(AdapterState {
    On => ("on","On"), Off => ("off","Off"), TurningOn => ("turning_on","Turning on…"), TurningOff => ("turning_off","Turning off…"),
    Unsupported => ("unsupported","Not supported"), Unavailable => ("unavailable","Unavailable"),
});
vocabulary!(DeviceKind {
    Audio => ("audio","Audio"), Computer => ("computer","Computer"), Phone => ("phone","Phone"), Input => ("input","Input device"),
    Wearable => ("wearable","Wearable"), Imaging => ("imaging","Imaging"), Other => ("other","Other"), Unknown => ("unknown","Unknown device type"),
});
vocabulary!(Transport { Classic => ("classic","Classic"), Le => ("le","Low Energy"), Dual => ("dual","Classic / Low Energy"), Unknown => ("unknown","Unknown transport") });
vocabulary!(BondState { None => ("none","Not paired"), Bonding => ("bonding","Pairing…"), Bonded => ("bonded","Paired"), Unknown => ("unknown","Pairing state unavailable") });
vocabulary!(ConnectionState {
    Connected => ("connected","Connected"), Connecting => ("connecting","Connecting…"), Disconnecting => ("disconnecting","Disconnecting…"), Disconnected => ("disconnected","Disconnected"),
});

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BluetoothCapability { Toggle,Scan,StopScan,Rename,RequestAccess }
impl BluetoothCapability {
    fn decode(value:&Value)->Option<Self> {Some(match value.as_str()? {"toggle"=>Self::Toggle,"scan"=>Self::Scan,"stop_scan"=>Self::StopScan,"rename"=>Self::Rename,"request_access"=>Self::RequestAccess,_=>return None})}
}
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BluetoothAction { Pair,CancelPair,Connect,Disconnect,Forget }
impl BluetoothAction {
    pub fn wire(self)->&'static str {match self {Self::Pair=>"pair",Self::CancelPair=>"cancel_pair",Self::Connect=>"connect",Self::Disconnect=>"disconnect",Self::Forget=>"forget"}}
    fn decode(value:&Value)->Option<Self> {Some(match value.as_str()? {"pair"=>Self::Pair,"cancel_pair"=>Self::CancelPair,"connect"=>Self::Connect,"disconnect"=>Self::Disconnect,"forget"=>Self::Forget,_=>return None})}
    fn accepts(self,bond:BondState)->bool {match self {Self::Pair=>bond==BondState::None,Self::CancelPair=>bond==BondState::Bonding,_=>bond==BondState::Bonded}}
}
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharingKind { Phonebook,Messages }
impl SharingKind {pub fn wire(self)->&'static str {match self {Self::Phonebook=>"phonebook",Self::Messages=>"messages"}}}
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharingValue { Ask,Allow,Deny }
impl SharingValue {
    pub const ALL:[Self;3]=[Self::Ask,Self::Allow,Self::Deny];
    pub fn wire(self)->&'static str {match self {Self::Ask=>"ask",Self::Allow=>"allow",Self::Deny=>"deny"}}
    pub fn label(self)->&'static str {match self {Self::Ask=>"Ask",Self::Allow=>"Allow",Self::Deny=>"Deny"}}
    fn decode(value:&Value)->Option<Self> {Self::ALL.into_iter().find(|choice|Some(choice.wire())==value.as_str())}
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub enum BluetoothRequest {
    Snapshot, Enabled(bool), Scan(bool), Name(String), Access,
    Device {target:BluetoothTarget,action:BluetoothAction},
    Sharing {target:BluetoothTarget,kind:SharingKind,value:SharingValue},
}
impl BluetoothRequest {
    pub fn valid(&self)->bool {match self {Self::Name(name)=>valid_name(name),_=>true}}
    pub fn target(&self)->Option<&BluetoothTarget> {match self {Self::Device{target,..}|Self::Sharing{target,..}=>Some(target),_=>None}}
}

fn optional<T>(value:Option<&Value>,decode:impl FnOnce(&Value)->Option<T>)->Option<Option<T>> {
    match value {None|Some(Value::Null)=>Some(None),Some(value)=>decode(value).map(Some)}
}
fn text(value:&Value)->Option<String> {let text=value.as_str()?;(text.chars().count()<=256&&!text.chars().any(char::is_control)).then(||text.to_owned())}
#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub struct SharingObservation {pub value:Option<SharingValue>,pub options:Vec<SharingValue>}
impl SharingObservation {
    fn decode(value:&Value)->Option<Self> {
        let observed=optional(value.get("value"),SharingValue::decode)?;
        let options=value.get("options")?.as_arr()?;
        if options.len()>3 {return None;}
        let options=options.iter().map(SharingValue::decode).collect::<Option<Vec<_>>>()?;
        if (!options.is_empty()&&observed.is_none())||options.iter().enumerate().any(|(index,value)|options[..index].contains(value)) {return None;}
        Some(Self{value:observed,options})
    }
}
#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub struct BluetoothAdapter {pub state:AdapterState,pub name:Option<String>,pub discovering:Option<bool>}
#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub struct BluetoothDevice {
    pub target:BluetoothTarget,pub name:Option<String>,pub address:Option<String>,pub kind:DeviceKind,pub transport:Transport,
    pub bond:BondState,pub connection:Option<ConnectionState>,pub nearby:bool,pub last_seen_age_ms:Option<i64>,
    pub actions:Vec<BluetoothAction>,pub phonebook:SharingObservation,pub messages:SharingObservation,
}
impl BluetoothDevice {
    pub fn name(&self)->&str {self.name.as_deref().filter(|name|!name.is_empty()).or(self.address.as_deref()).unwrap_or("Unnamed Bluetooth device")}
    pub fn sharing(&self,kind:SharingKind)->&SharingObservation {match kind {SharingKind::Phonebook=>&self.phonebook,SharingKind::Messages=>&self.messages}}
    fn decode(value:&Value)->Option<Self> {
        let bond=BondState::decode(value.get("bond")?)?;
        let actions=value.get("actions")?.as_arr()?;
        if actions.len()>5 {return None;}
        let actions=actions.iter().map(BluetoothAction::decode).collect::<Option<Vec<_>>>()?;
        if actions.iter().enumerate().any(|(index,action)|!action.accepts(bond)||actions[..index].contains(action)) {return None;}
        let sharing=value.get("sharing")?;
        let phonebook=SharingObservation::decode(sharing.get("phonebook")?)?;
        let messages=SharingObservation::decode(sharing.get("messages")?)?;
        if bond!=BondState::Bonded&&(!phonebook.options.is_empty()||!messages.options.is_empty()) {return None;}
        Some(Self{target:BluetoothTarget::decode(value.get("key")?)?,name:optional(value.get("name"),text)?,address:optional(value.get("address"),text)?,
            kind:DeviceKind::decode(value.get("kind")?)?,transport:Transport::decode(value.get("transport")?)?,bond,
            connection:optional(value.get("connection"),ConnectionState::decode)?,nearby:value.get("nearby")?.as_bool()?,
            last_seen_age_ms:optional(value.get("last_seen_age_ms"),|value|value.as_i64().filter(|age|*age>=0))?,actions,phonebook,messages})
    }
}
#[derive(serde::Serialize, Clone, Debug)]
pub struct BluetoothSnapshot {
    pub request_id:i64,pub adapter:BluetoothAdapter,pub capabilities:HashSet<BluetoothCapability>,
    pub paired_available:bool,pub scan_available:bool,pub devices:Vec<BluetoothDevice>,pub devices_total:usize,pub truncated:bool,
}
impl BluetoothSnapshot {
    pub fn decode(value:&Value)->Option<Self> {
        if value.get("schema")?.as_i64()?!=1 {return None;}
        let request_id=value.get("request_id")?.as_i64().filter(|id|*id>0)?;
        let adapter=value.get("adapter")?;
        let adapter=BluetoothAdapter{state:AdapterState::decode(adapter.get("state")?)?,name:optional(adapter.get("name"),text)?,discovering:optional(adapter.get("discovering"),Value::as_bool)?};
        let caps=value.get("capabilities")?.as_arr()?;
        if caps.len()>5 {return None;}
        let capabilities=caps.iter().map(BluetoothCapability::decode).collect::<Option<HashSet<_>>>()?;
        if capabilities.len()!=caps.len() {return None;}
        let devices_total=usize::try_from(value.get("devices_total")?.as_i64()?).ok().filter(|total|*total<=100_000)?;
        let truncated=value.get("truncated")?.as_bool()?;
        let rows=value.get("devices")?.as_arr()?;
        if rows.len()!=devices_total.min(MAX_DEVICES)||truncated!=(devices_total>MAX_DEVICES) {return None;}
        let devices=rows.iter().map(BluetoothDevice::decode).collect::<Option<Vec<_>>>()?;
        let mut targets=HashSet::new();if devices.iter().any(|row|!targets.insert(&row.target)) {return None;}
        Some(Self{request_id,adapter,capabilities,paired_available:value.get("paired_available")?.as_bool()?,scan_available:value.get("scan_available")?.as_bool()?,devices,devices_total,truncated})
    }
    pub fn device(&self,target:&BluetoothTarget)->Option<&BluetoothDevice> {self.devices.iter().find(|device|&device.target==target)}
    pub fn permits(&self,request:&BluetoothRequest)->bool {
        if !request.valid() {return false;}
        match request {
            BluetoothRequest::Snapshot=>true,
            BluetoothRequest::Enabled(_)=>matches!(self.adapter.state,AdapterState::On|AdapterState::Off)&&self.capabilities.contains(&BluetoothCapability::Toggle),
            BluetoothRequest::Scan(true)=>self.adapter.state==AdapterState::On&&self.capabilities.contains(&BluetoothCapability::Scan),
            BluetoothRequest::Scan(false)=>self.adapter.discovering==Some(true)&&self.capabilities.contains(&BluetoothCapability::StopScan),
            BluetoothRequest::Name(_)=>self.adapter.name.is_some()&&self.capabilities.contains(&BluetoothCapability::Rename),
            BluetoothRequest::Access=>self.capabilities.contains(&BluetoothCapability::RequestAccess),
            BluetoothRequest::Device{target,action}=>self.device(target).is_some_and(|device|device.actions.contains(action)),
            BluetoothRequest::Sharing{target,kind,value}=>self.device(target).is_some_and(|device|device.sharing(*kind).options.contains(value)),
        }
    }
    pub fn clear_actions(&mut self) {
        self.capabilities.clear();for device in &mut self.devices {device.actions.clear();device.phonebook.options.clear();device.messages.options.clear();}
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;use makepad_strict_json::{obj,s};
    pub fn snapshot(id:i64,count:usize)->Value {
        let sharing=||obj(vec![("value",s("ask")),("options",Value::Arr(vec![s("ask"),s("allow"),s("deny")]))]);
        obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(id)),("adapter",obj(vec![("state",s("on")),("name",s("OctoSense phone")),("discovering",Value::Bool(false))])),
            ("capabilities",Value::Arr(vec![s("toggle"),s("scan"),s("rename")])),("paired_available",Value::Bool(true)),("scan_available",Value::Bool(true)),
            ("devices_total",Value::Int(count as i64)),("truncated",Value::Bool(count>MAX_DEVICES)),("devices",Value::Arr((0..count.min(MAX_DEVICES)).map(|index|obj(vec![
                ("key",s(format!("{index:064x}"))),("name",s(format!("Device {index}"))),("address",Value::Null),("kind",s("audio")),("transport",s("dual")),("bond",s("bonded")),
                ("connection",Value::Null),("nearby",Value::Bool(true)),("last_seen_age_ms",Value::Null),("actions",Value::Arr(vec![s("connect"),s("disconnect"),s("forget")])),
                ("sharing",obj(vec![("phonebook",sharing()),("messages",sharing())]))])).collect()))])
    }
    fn field(value:&mut Value,key:&str,replacement:Value) {let Value::Obj(fields)=value else{panic!()};fields.iter_mut().find(|(name,_)|name==key).unwrap().1=replacement;}
    #[test]
    fn bluetooth_inventory_targets_and_action_states_are_bounded() {
        for count in [0,1,256,257] {assert!(BluetoothSnapshot::decode(&snapshot(1,count)).is_some());}
        let mut value=snapshot(1,2);let Value::Arr(rows)=value.get("devices").unwrap().clone() else{panic!()};
        field(&mut value,"devices",Value::Arr(vec![rows[0].clone(),rows[0].clone()]));assert!(BluetoothSnapshot::decode(&value).is_none());
        let mut value=snapshot(1,1);let Value::Arr(mut rows)=value.get("devices").unwrap().clone() else{panic!()};
        field(&mut rows[0],"bond",s("unknown"));field(&mut value,"devices",Value::Arr(rows));assert!(BluetoothSnapshot::decode(&value).is_none(),"unknown bond cannot authorize forget or sharing");
        assert!(BluetoothTarget::decode(&s("AA:BB:CC:DD:EE:FF")).is_none(),"display address is not an action target");
    }
    #[test]
    fn adapter_names_are_utf8_bounded_and_discovery_stop_requires_ownership() {
        assert!(valid_name(&"猫".repeat(82)));assert!(!valid_name(&"猫".repeat(83)));
        for name in ["","  ","phone\nname","phone\0name"] {assert!(!valid_name(name));}
        let mut state=BluetoothSnapshot::decode(&snapshot(1,1)).unwrap();state.adapter.discovering=Some(true);
        assert!(!state.permits(&BluetoothRequest::Scan(false)),"another app's discovery cannot be stopped");
        state.capabilities.insert(BluetoothCapability::StopScan);assert!(state.permits(&BluetoothRequest::Scan(false)));
    }
    #[test]
    fn missing_connection_and_retired_permissions_never_imply_success() {
        let mut state=BluetoothSnapshot::decode(&snapshot(1,1)).unwrap();assert!(state.devices[0].connection.is_none());
        let target=state.devices[0].target.clone();let request=BluetoothRequest::Sharing{target:target.clone(),kind:SharingKind::Phonebook,value:SharingValue::Allow};
        assert!(state.permits(&request));state.clear_actions();assert!(!state.permits(&request));
        assert!(!state.permits(&BluetoothRequest::Device{target,action:BluetoothAction::Forget}));
        assert_eq!(state.devices[0].name(),"Device 0");assert_eq!(state.devices[0].phonebook.value,Some(SharingValue::Ask));
    }
}
