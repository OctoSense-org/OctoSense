//! Bounded Wi-Fi observations. Network targets exist only after snapshot decoding.
use makepad_strict_json::Value;
use std::collections::HashSet;

pub const MAX_NETWORKS: usize = 256;
pub const PAGE_SIZE: usize = 20;

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct WifiTarget(String);
impl WifiTarget {
    pub fn key(&self) -> &str { &self.0 }
    pub(crate) fn decode(value: &Value) -> Option<Self> {
        let key = value.as_str()?;
        (key.len() == 64 && key.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))).then(|| Self(key.into()))
    }
}

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WifiAction { Connect, Configure, Forget }
impl WifiAction {
    pub fn wire(self) -> &'static str { match self { Self::Connect => "connect", Self::Configure => "configure", Self::Forget => "forget" } }
    fn decode(value: &Value) -> Option<Self> { match value.as_str()? { "connect" => Some(Self::Connect), "configure" => Some(Self::Configure), "forget" => Some(Self::Forget), _ => None } }
}

#[derive(serde::Serialize, Clone, Debug)]
pub enum WifiRequest { Snapshot, Enabled(bool), Scan, Network { target: WifiTarget, action: WifiAction }, Access }

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WifiCapability { Toggle, Scan, RequestAccess }
impl WifiCapability {
    fn decode(value: &Value) -> Option<Self> { match value.as_str()? { "toggle" => Some(Self::Toggle), "scan" => Some(Self::Scan), "request_access" => Some(Self::RequestAccess), _ => None } }
}

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WifiSecurity { Open, Owe, OpenOwe, Wep, Wpa2, Wpa3, Wpa2Wpa3, Eap, Unknown }
impl WifiSecurity {
    fn decode(value: &Value) -> Option<Self> { Some(match value.as_str()? { "open" => Self::Open, "owe" => Self::Owe, "open_owe" => Self::OpenOwe, "wep" => Self::Wep, "wpa2" => Self::Wpa2,
        "wpa3" => Self::Wpa3, "wpa2_wpa3" => Self::Wpa2Wpa3, "eap" => Self::Eap, "unknown" => Self::Unknown, _ => return None }) }
    pub fn label(self) -> &'static str { match self { Self::Open => "Open", Self::Owe => "Enhanced Open", Self::OpenOwe=>"Open / Enhanced Open", Self::Wep => "WEP", Self::Wpa2 => "WPA2",
        Self::Wpa3 => "WPA3", Self::Wpa2Wpa3 => "WPA2 / WPA3", Self::Eap => "Enterprise", Self::Unknown => "Unknown security" } }
}

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WifiScanState { Idle, Scanning, Throttled, Unavailable }
impl WifiScanState {
    fn decode(value: &Value) -> Option<Self> { Some(match value.as_str()? { "idle" => Self::Idle, "scanning" => Self::Scanning, "throttled" => Self::Throttled, "unavailable" => Self::Unavailable, _ => return None }) }
}

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InternetState { Validated, CaptivePortal, LocalOnly, Unknown }
impl InternetState {
    fn decode(value: &Value) -> Option<Self> { Some(match value.as_str()? { "validated" => Self::Validated, "captive_portal" => Self::CaptivePortal, "local_only" => Self::LocalOnly, "unknown" => Self::Unknown, _ => return None }) }
    pub fn label(self) -> &'static str { match self { Self::Validated => "Internet available", Self::CaptivePortal => "Sign-in required", Self::LocalOnly => "Local network only", Self::Unknown => "Internet status unknown" } }
}

fn optional<T>(value: Option<&Value>, decode: impl FnOnce(&Value) -> Option<T>) -> Option<Option<T>> {
    match value { None | Some(Value::Null) => Some(None), Some(value) => decode(value).map(Some) }
}
fn text(value: &Value) -> Option<String> {
    let text = value.as_str()?;
    (text.chars().count() <= 256 && !text.chars().any(char::is_control)).then(|| text.to_owned())
}
fn nonnegative(value: &Value) -> Option<i64> { value.as_i64().filter(|value| *value >= 0) }

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub struct WifiConnection {
    pub ssid: Option<String>, pub bssid: Option<String>, pub ip_address: Option<String>,
    pub rssi_dbm: Option<i64>, pub link_speed_mbps: Option<i64>, pub frequency_mhz: Option<i64>,
    pub target: Option<WifiTarget>, pub internet: InternetState,
}
impl WifiConnection {
    fn decode(value: &Value) -> Option<Self> {
        Some(Self { ssid: optional(value.get("ssid"),text)?, bssid: optional(value.get("bssid"),text)?, ip_address: optional(value.get("ip_address"),text)?,
            rssi_dbm: optional(value.get("rssi_dbm"),|v| v.as_i64().filter(|v| (-127..=0).contains(v)))?,
            link_speed_mbps: optional(value.get("link_speed_mbps"),nonnegative)?, frequency_mhz: optional(value.get("frequency_mhz"),nonnegative)?,
            target: optional(value.get("network_key"),WifiTarget::decode)?, internet: InternetState::decode(value.get("internet")?)? })
    }
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub struct WifiNetwork {
    pub target: WifiTarget, pub ssid: Option<String>, pub security: WifiSecurity, pub signal_level: Option<i64>,
    pub available: bool, pub saved: bool, pub connected: bool, pub actions: Vec<WifiAction>,
}
impl WifiNetwork {
    fn decode(value: &Value) -> Option<Self> {
        let actions = value.get("actions")?.as_arr()?;
        if actions.len() > 3 { return None; }
        let actions = actions.iter().map(WifiAction::decode).collect::<Option<Vec<_>>>()?;
        if actions.iter().enumerate().any(|(i,action)| actions[..i].contains(action)) { return None; }
        let row = Self { target: WifiTarget::decode(value.get("key")?)?, ssid: optional(value.get("ssid"),text)?,
            security: WifiSecurity::decode(value.get("security")?)?, signal_level: optional(value.get("signal_level"),|v| v.as_i64().filter(|v| (0..=4).contains(v)))?,
            available: value.get("available")?.as_bool()?, saved: value.get("saved")?.as_bool()?, connected: value.get("connected")?.as_bool()?, actions };
        if (row.actions.contains(&WifiAction::Forget) && !row.saved)
            || (row.actions.contains(&WifiAction::Connect) && !row.saved) { return None; }
        Some(row)
    }
    pub fn name(&self) -> &str { self.ssid.as_deref().filter(|name| !name.is_empty()).unwrap_or("Hidden or unavailable network name") }
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct WifiSnapshot {
    pub request_id: i64, pub enabled: Option<bool>, pub connection: Option<WifiConnection>, pub capabilities: HashSet<WifiCapability>,
    pub scan_state: WifiScanState, pub scan_age_ms: Option<i64>, pub saved_available: bool, pub scan_available: bool,
    pub networks: Vec<WifiNetwork>, pub networks_total: usize, pub truncated: bool,
}
impl WifiSnapshot {
    pub fn decode(value: &Value) -> Option<Self> {
        if value.get("schema")?.as_i64()? != 1 { return None; }
        let request_id = value.get("request_id")?.as_i64().filter(|id| *id > 0)?;
        let total = usize::try_from(value.get("networks_total")?.as_i64()?).ok().filter(|total| *total <= 100_000)?;
        let truncated = value.get("truncated")?.as_bool()?;
        let rows = value.get("networks")?.as_arr()?;
        if rows.len() != total.min(MAX_NETWORKS) || truncated != (total > MAX_NETWORKS) { return None; }
        let networks = rows.iter().map(WifiNetwork::decode).collect::<Option<Vec<_>>>()?;
        let mut keys = HashSet::new();
        if networks.iter().any(|network| !keys.insert(&network.target)) { return None; }
        let caps = value.get("capabilities")?.as_arr()?;
        if caps.len() > 3 { return None; }
        let capabilities = caps.iter().map(WifiCapability::decode).collect::<Option<HashSet<_>>>()?;
        Some(Self { request_id, enabled: optional(value.get("enabled"),Value::as_bool)?,
            connection: optional(value.get("connection"),WifiConnection::decode)?, capabilities,
            scan_state: WifiScanState::decode(value.get("scan_state")?)?, scan_age_ms: optional(value.get("scan_age_ms"),nonnegative)?,
            saved_available: value.get("saved_available")?.as_bool()?, scan_available: value.get("scan_available")?.as_bool()?,
            networks, networks_total: total, truncated })
    }
    pub fn network(&self, target: &WifiTarget) -> Option<&WifiNetwork> { self.networks.iter().find(|network| &network.target == target) }
    pub fn permits(&self, request: &WifiRequest) -> bool { match request {
        WifiRequest::Snapshot => true,
        WifiRequest::Enabled(_) => self.enabled.is_some() && self.capabilities.contains(&WifiCapability::Toggle),
        WifiRequest::Scan => self.capabilities.contains(&WifiCapability::Scan),
        WifiRequest::Access => self.capabilities.contains(&WifiCapability::RequestAccess),
        WifiRequest::Network {target,action} => self.network(target).is_some_and(|network| network.actions.contains(action)),
    } }
    pub fn clear_actions(&mut self) { self.capabilities.clear(); for network in &mut self.networks { network.actions.clear(); } }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use makepad_strict_json::{obj,s};
    pub fn snapshot(id:i64, count:usize) -> Value {
        obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(id)),("enabled",Value::Bool(true)),("connection",Value::Null),
            ("capabilities",Value::Arr(vec![s("toggle"),s("scan")])),("scan_state",s("idle")),("scan_age_ms",Value::Null),
            ("saved_available",Value::Bool(true)),("scan_available",Value::Bool(true)),("networks_total",Value::Int(count as i64)),("truncated",Value::Bool(count>MAX_NETWORKS)),
            ("networks",Value::Arr((0..count.min(MAX_NETWORKS)).map(|index|obj(vec![("key",s(format!("{index:064x}"))),("ssid",s(format!("Network {index}"))),
                ("security",s("wpa2")),("signal_level",Value::Null),("available",Value::Bool(true)),("saved",Value::Bool(true)),("connected",Value::Bool(false)),
                ("actions",Value::Arr(vec![s("connect"),s("configure"),s("forget")]))])).collect()))])
    }
    fn replace(value:&mut Value,key:&str,replacement:Value) { let Value::Obj(fields)=value else {panic!()}; fields.iter_mut().find(|(name,_)|name==key).unwrap().1=replacement; }
    #[test]
    fn wifi_targets_and_inventory_must_be_observed_and_bounded() {
        for count in [0,1,256,257] {assert!(WifiSnapshot::decode(&snapshot(1,count)).is_some());}
        let mut value=snapshot(1,1);replace(&mut value,"networks_total",Value::Int(2));assert!(WifiSnapshot::decode(&value).is_none());
        let mut value=snapshot(1,2);if let Value::Arr(rows)=value.get("networks").unwrap().clone() {replace(&mut value,"networks",Value::Arr(vec![rows[0].clone(),rows[0].clone()]));}
        assert!(WifiSnapshot::decode(&value).is_none());
        assert!(WifiTarget::decode(&s("ssid; shell command")).is_none());
        let mut value=snapshot(0,1);assert!(WifiSnapshot::decode(&value).is_none());replace(&mut value,"request_id",Value::Int(1));
        replace(&mut value,"capabilities",Value::Arr(vec![s("arbitrary_setting")]));assert!(WifiSnapshot::decode(&value).is_none());
    }
    #[test]
    fn unavailable_signal_and_connection_do_not_become_plausible_defaults() {
        assert_eq!(WifiSecurity::decode(&s("open_owe")).unwrap().label(),"Open / Enhanced Open");
        let observed=WifiSnapshot::decode(&snapshot(1,1)).unwrap();
        assert!(observed.connection.is_none());assert!(observed.scan_age_ms.is_none());assert!(observed.networks[0].signal_level.is_none());
        let mut value=snapshot(1,1);replace(&mut value,"enabled",s("true"));assert!(WifiSnapshot::decode(&value).is_none());
        let mut value=snapshot(1,1);replace(&mut value,"scan_age_ms",Value::Int(-1));assert!(WifiSnapshot::decode(&value).is_none());
    }
    #[test]
    fn retired_observations_keep_labels_but_revoke_network_actions() {
        let mut observed=WifiSnapshot::decode(&snapshot(1,1)).unwrap();let target=observed.networks[0].target.clone();
        let request=WifiRequest::Network {target,action:WifiAction::Forget};assert!(observed.permits(&request));
        observed.clear_actions();assert!(!observed.permits(&request));assert!(!observed.permits(&WifiRequest::Enabled(false)));
        assert_eq!(observed.networks[0].name(),"Network 0");
    }
}
