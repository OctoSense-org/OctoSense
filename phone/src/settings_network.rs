//! Network policy observations and finite actions bound to the exact read state.
use makepad_strict_json::Value;

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub struct NetworkKey(String);
impl NetworkKey {
    pub fn key(&self) -> &str { &self.0 }
    fn decode(value: &Value) -> Option<Self> {
        let key = value.as_str()?;
        (key.len() == 64 && key.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))).then(|| Self(key.into()))
    }
}
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DnsMode { Off, Automatic, Hostname }
impl DnsMode {
    pub const ALL: [Self; 3] = [Self::Off, Self::Automatic, Self::Hostname];
    pub fn wire(self) -> &'static str { match self { Self::Off => "off", Self::Automatic => "automatic", Self::Hostname => "hostname" } }
    pub fn label(self) -> &'static str { match self { Self::Off => "Off", Self::Automatic => "Automatic", Self::Hostname => "Provider hostname" } }
    fn decode(value: &Value) -> Option<Self> { Self::ALL.into_iter().find(|mode| Some(mode.wire()) == value.as_str()) }
}
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DnsValidation { Validated, Unvalidated, Offline, Unknown }
impl DnsValidation {
    fn decode(value: &Value) -> Option<Self> { Some(match value.as_str()? { "validated" => Self::Validated, "unvalidated" => Self::Unvalidated, "offline" => Self::Offline, "unknown" => Self::Unknown, _ => return None }) }
    pub fn label(self) -> &'static str { match self { Self::Validated => "Validated", Self::Unvalidated => "Not validated", Self::Offline => "Offline", Self::Unknown => "Unavailable" } }
}
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InternetState { Validated, CaptivePortal, LocalOnly, Unknown }
impl InternetState {
    fn decode(value: &Value) -> Option<Self> { Some(match value.as_str()? { "validated" => Self::Validated, "captive_portal" => Self::CaptivePortal, "local_only" => Self::LocalOnly, "unknown" => Self::Unknown, _ => return None }) }
    pub fn label(self) -> &'static str { match self { Self::Validated => "Internet validated", Self::CaptivePortal => "Network sign-in required", Self::LocalOnly => "Internet not validated", Self::Unknown => "Internet state unavailable" } }
}
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport { Wifi, Mobile, Ethernet, Vpn, Other }
impl Transport {
    fn decode(value: &Value) -> Option<Self> { Some(match value.as_str()? { "wifi" => Self::Wifi, "mobile" => Self::Mobile, "ethernet" => Self::Ethernet, "vpn" => Self::Vpn, "other" => Self::Other, _ => return None }) }
    pub fn label(self) -> &'static str { match self { Self::Wifi => "Wi-Fi", Self::Mobile => "Mobile", Self::Ethernet => "Ethernet", Self::Vpn => "VPN", Self::Other => "Other" } }
}
fn optional<T>(value: Option<&Value>, decode: impl FnOnce(&Value) -> Option<T>) -> Option<Option<T>> {
    match value { None | Some(Value::Null) => Some(None), Some(value) => decode(value).map(Some) }
}

/// Bounded candidate syntax. Android performs authoritative IDNA normalization
/// and validation; a valid candidate is not a claim that DNS can connect.
pub fn valid_hostname_input(value: &str) -> bool {
    if value.is_empty() || value.encode_utf16().count() > 253 { return false; }
    let normalized: String = value.chars().map(|c| if matches!(c, '\u{3002}' | '\u{ff0e}' | '\u{ff61}') { '.' } else { c }).collect();
    let normalized = normalized.strip_suffix('.').unwrap_or(&normalized);
    let labels: Vec<_> = normalized.split('.').collect();
    labels.iter().all(|label| !label.is_empty() && (!label.is_ascii() || label.len() <= 63)
        && !label.starts_with('-') && !label.ends_with('-')
        && label.chars().all(|c| c.is_alphanumeric() || c == '-'))
        && labels.last().and_then(|label| label.chars().next()).is_some_and(|c| !c.is_ascii_digit())
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub enum NetworkRequest {
    Snapshot,
    Airplane { key: NetworkKey, enabled: bool },
    DataSaver { key: NetworkKey, enabled: bool },
    PrivateDns { key: NetworkKey, mode: DnsMode, hostname: Option<String> },
}
impl NetworkRequest {
    pub fn valid(&self) -> bool { match self {
        Self::PrivateDns { mode: DnsMode::Hostname, hostname, .. } => hostname.as_ref().is_some_and(|host| valid_hostname_input(host)),
        Self::PrivateDns { hostname, .. } => hostname.is_none(),
        _ => true,
    } }
}
#[derive(serde::Serialize, Clone, Debug)]
pub struct NetworkToggle { pub enabled: Option<bool>, pub can_set: bool }
impl NetworkToggle {
    fn decode(value: &Value) -> Option<Self> {
        let enabled = optional(value.get("enabled"), Value::as_bool)?;
        let can_set = value.get("can_set")?.as_bool()?;
        if can_set && enabled.is_none() { return None; }
        Some(Self { enabled, can_set })
    }
}
#[derive(serde::Serialize, Clone, Debug)]
pub struct PrivateDns {
    pub mode: Option<DnsMode>, pub hostname: Option<String>, pub active: Option<bool>,
    pub validation: DnsValidation, pub can_set: bool,
}
#[derive(serde::Serialize, Clone, Debug)]
pub struct NetworkSnapshot {
    pub request_id: i64, pub key: NetworkKey, pub airplane: NetworkToggle, pub data_saver: NetworkToggle,
    pub dns: PrivateDns, pub connected: Option<bool>, pub transport: Option<Transport>, pub internet: InternetState,
}
impl NetworkSnapshot {
    pub fn decode(value: &Value) -> Option<Self> {
        if value.get("schema")?.as_i64()? != 1 { return None; }
        let dns = value.get("private_dns")?;
        let mode = optional(dns.get("mode"), DnsMode::decode)?;
        let hostname = optional(dns.get("hostname"), |value| {
            let value = value.as_str()?;
            (value.is_ascii() && valid_hostname_input(value)).then(|| value.to_owned())
        })?;
        if mode == Some(DnsMode::Hostname) && hostname.is_none() { return None; }
        let connection = value.get("connection")?;
        Some(Self {
            request_id: value.get("request_id")?.as_i64().filter(|id| *id > 0)?, key: NetworkKey::decode(value.get("key")?)?,
            airplane: NetworkToggle::decode(value.get("airplane")?)?, data_saver: NetworkToggle::decode(value.get("data_saver")?)?,
            dns: PrivateDns { mode, hostname, active: optional(dns.get("active"), Value::as_bool)?,
                validation: DnsValidation::decode(dns.get("validation")?)?, can_set: dns.get("can_set")?.as_bool()? },
            connected: optional(connection.get("connected"), Value::as_bool)?,
            transport: optional(connection.get("transport"), Transport::decode)?, internet: InternetState::decode(connection.get("internet")?)?,
        })
    }
    pub fn permits(&self, request: &NetworkRequest) -> bool {
        if !request.valid() { return false; }
        match request {
            NetworkRequest::Snapshot => true,
            NetworkRequest::Airplane { key, .. } => key == &self.key && self.airplane.can_set && self.airplane.enabled.is_some(),
            NetworkRequest::DataSaver { key, .. } => key == &self.key && self.data_saver.can_set && self.data_saver.enabled.is_some(),
            NetworkRequest::PrivateDns { key, .. } => key == &self.key && self.dns.can_set,
        }
    }
    pub fn clear_actions(&mut self) { self.airplane.can_set = false; self.data_saver.can_set = false; self.dns.can_set = false; }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use makepad_strict_json::{obj, s};
    pub fn snapshot(id: i64) -> Value {
        obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(id)),("key",s(format!("{:064x}",1))),
            ("airplane",obj(vec![("enabled",Value::Bool(false)),("can_set",Value::Bool(true))])),
            ("data_saver",obj(vec![("enabled",Value::Bool(false)),("can_set",Value::Bool(true))])),
            ("private_dns",obj(vec![("mode",s("automatic")),("hostname",Value::Null),("active",Value::Null),("validation",s("unknown")),("can_set",Value::Bool(true))])),
            ("connection",obj(vec![("connected",Value::Bool(true)),("transport",s("wifi")),("internet",s("local_only"))]))])
    }
    #[test]
    fn network_actions_require_exact_observed_keys_and_authority() {
        let mut state=NetworkSnapshot::decode(&snapshot(1)).unwrap();
        let request=NetworkRequest::Airplane{key:state.key.clone(),enabled:true};assert!(state.permits(&request));
        let mut changed=snapshot(2);crate::settings_updates::tests::field(&mut changed,"key",s(format!("{:064x}",2)));
        assert!(!NetworkSnapshot::decode(&changed).unwrap().permits(&request));
        state.clear_actions();assert!(!state.permits(&request));assert_eq!(state.airplane.enabled,Some(false));
        assert!(state.dns.active.is_none());assert_eq!(state.internet,InternetState::LocalOnly);
    }
    #[test]
    fn unknown_dns_mode_can_be_repaired_without_inventing_an_observation() {
        let mut value=snapshot(1);let mut dns=value.get("private_dns").unwrap().clone();
        crate::settings_updates::tests::field(&mut dns,"mode",Value::Null);crate::settings_updates::tests::field(&mut value,"private_dns",dns);
        let state=NetworkSnapshot::decode(&value).unwrap();assert!(state.dns.mode.is_none());
        assert!(state.permits(&NetworkRequest::PrivateDns{key:state.key.clone(),mode:DnsMode::Automatic,hostname:None}));
        crate::settings_updates::tests::field(&mut value,"airplane",obj(vec![("enabled",Value::Null),("can_set",Value::Bool(true))]));
        assert!(NetworkSnapshot::decode(&value).is_none());
    }
    #[test]
    fn private_dns_candidates_reject_urls_ips_and_mismatched_mode_payloads() {
        for hostname in ["dns.example", "DNS.EXAMPLE.", "例子.测试", "xn--fsqu00a.xn--0zwm56d"] {assert!(valid_hostname_input(hostname),"{hostname}");}
        for hostname in ["", "https://dns.example", "dns.example/path", "1.1.1.1", "[::1]", "dns.example:853", "a..b", "-a.example", "a-.example", "a.example\n"] {assert!(!valid_hostname_input(hostname),"{hostname}");}
        let state=NetworkSnapshot::decode(&snapshot(1)).unwrap();
        assert!(!state.permits(&NetworkRequest::PrivateDns{key:state.key.clone(),mode:DnsMode::Hostname,hostname:None}));
        assert!(!state.permits(&NetworkRequest::PrivateDns{key:state.key.clone(),mode:DnsMode::Off,hostname:Some("dns.example".into())}));
    }
}
