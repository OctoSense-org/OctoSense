//! Observed current-user Android packages. Targets are minted only by snapshot
//! decoding; scripts never provide a package string to a privileged command.
use makepad_strict_json::Value;
use std::collections::HashSet;

pub const PAGE_SIZE: u32 = 20;
pub const MAX_OFFSET: u32 = 100_000;

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct AppTarget(String);
impl AppTarget {
    pub fn package(&self) -> &str { &self.0 }
    pub(crate) fn decode(value: &Value) -> Option<Self> {
        let name = value.as_str()?;
        valid_package(name).then(||Self(name.into()))
    }
}
/// Syntax validation does not mint an observed app or authorize an operation.
pub(crate) fn valid_package(name:&str)->bool {
        if name.is_empty() || name.len() > 255 || !name.is_ascii() { return false; }
        if name != "android" && !name.contains('.') { return false; }
        for part in name.split('.') {
            let mut chars = part.chars();
            if !chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') { return false; }
            if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') { return false; }
        }
        true
}

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AppAction { Launch, Uninstall, AppInfo, Notifications, Language }
impl AppAction {
    pub fn wire(self) -> &'static str { match self {
        Self::Launch => "launch", Self::Uninstall => "uninstall", Self::AppInfo => "app_info", Self::Notifications => "notifications", Self::Language => "language",
    } }
    fn decode(value: &Value) -> Option<Self> { match value.as_str()? {
        "launch" => Some(Self::Launch), "uninstall" => Some(Self::Uninstall), "app_info" => Some(Self::AppInfo),
        "notifications" => Some(Self::Notifications), "language" => Some(Self::Language), _ => None,
    } }
}

pub fn valid_query(query: &str) -> bool { query.chars().count() <= 128 && !query.chars().any(char::is_control) }
pub fn valid_offset(offset: u32) -> bool { offset <= MAX_OFFSET && offset % PAGE_SIZE == 0 }
pub fn valid_generation(generation: &str) -> bool { generation.len() == 64 && generation.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) }
fn text(value: &Value, key: &str) -> Option<String> { value.get(key).and_then(Value::as_str).filter(|s| !s.is_empty() && s.chars().count() <= 1024).map(str::to_owned) }
fn positive(value: &Value, key: &str) -> Option<i64> { value.get(key).and_then(Value::as_i64).filter(|v| *v >= 0) }
fn integer(value: &Value, key: &str) -> Option<u32> { value.get(key).and_then(Value::as_u64).and_then(|v| u32::try_from(v).ok()) }
fn request_id(value: &Value) -> Option<i64> {
    if value.get("schema").and_then(Value::as_i64) != Some(1) { return None; }
    value.get("request_id").and_then(Value::as_i64).filter(|v| *v > 0)
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub struct AndroidApp {
    pub target: AppTarget, pub label: String, pub system: bool, pub enabled: bool, pub launchable: bool,
}
#[derive(serde::Serialize, Clone, Debug)]
pub struct AppsCatalog {
    pub request_id: i64, pub query: String, pub include_system: bool, pub offset: u32,
    pub page_size: u32, pub total: u32, pub generation: String, pub stale: bool, pub apps: Vec<AndroidApp>,
}
impl AppsCatalog {
    pub fn decode(value: &Value) -> Option<Self> {
        let request_id = request_id(value)?;
        let query = value.get("query")?.as_str()?.to_owned();
        let include_system = value.get("include_system")?.as_bool()?;
        let offset = integer(value, "offset")?;
        let page_size = integer(value, "page_size")?;
        let total = integer(value, "total")?;
        let generation = text(value, "generation")?;
        let stale = value.get("stale")?.as_bool()?;
        let rows = value.get("apps")?.as_arr()?;
        if !valid_query(&query) || !valid_offset(offset) || page_size != PAGE_SIZE || total > MAX_OFFSET
            || !valid_generation(&generation) || rows.len() > PAGE_SIZE as usize || (stale && offset != 0) { return None; }
        let mut seen = HashSet::new();
        let mut apps = Vec::new();
        for row in rows {
            let target = AppTarget::decode(row.get("package")?)?;
            if !seen.insert(target.clone()) { return None; }
            apps.push(AndroidApp { target, label: text(row, "label")?, system: row.get("system")?.as_bool()?,
                enabled: row.get("enabled")?.as_bool()?, launchable: row.get("launchable")?.as_bool()? });
        }
        if (total == 0 && offset != 0) || (total > 0 && offset >= total)
            || apps.len() as u32 != PAGE_SIZE.min(total.saturating_sub(offset)) { return None; }
        Some(Self { request_id, query, include_system, offset, page_size, total, generation, stale, apps })
    }
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct AppPermission { pub name: String, pub label: Option<String>, pub granted: Option<bool>, pub runtime: Option<bool>, pub protection: Option<String> }
#[derive(serde::Serialize, Clone, Debug, Default)]
pub struct AppStorage {
    pub app_bytes: Option<i64>, pub data_bytes: Option<i64>, pub cache_bytes: Option<i64>,
    pub available: bool, pub can_request_usage_access: bool, pub shared_uid: bool,
}
#[derive(serde::Serialize, Clone, Debug)]
pub struct AppDetails {
    pub request_id: i64, pub target: AppTarget, pub exists: bool, pub label: Option<String>,
    pub version_name: Option<String>, pub version_code: Option<i64>, pub target_sdk: Option<i64>,
    pub enabled: Option<bool>, pub system: Option<bool>, pub suspended: Option<bool>,
    pub first_install_ms: Option<i64>, pub last_update_ms: Option<i64>,
    pub first_install_text: Option<String>, pub last_update_text: Option<String>,
    pub actions: Vec<AppAction>, pub permissions: Vec<AppPermission>, pub permissions_total: u32,
    pub permissions_offset: u32, pub storage: AppStorage,
}
impl AppDetails {
    pub fn decode(value: &Value) -> Option<Self> {
        let request_id = request_id(value)?;
        let target = AppTarget::decode(value.get("package")?)?;
        let exists = value.get("exists")?.as_bool()?;
        let actions = if exists { value.get("actions")?.as_arr()? } else { value.get("actions").and_then(Value::as_arr).unwrap_or_default() };
        let permissions = if exists { value.get("permissions")?.as_arr()? } else { value.get("permissions").and_then(Value::as_arr).unwrap_or_default() };
        let permissions_total = if exists { integer(value, "permissions_total")? } else { integer(value, "permissions_total").unwrap_or(0) };
        let permissions_offset = if exists { integer(value, "permissions_offset")? } else { integer(value, "permissions_offset").unwrap_or(0) };
        if actions.len() > 4 || permissions.len() > PAGE_SIZE as usize || permissions_total > MAX_OFFSET || !valid_offset(permissions_offset)
            || (!exists && (!actions.is_empty() || !permissions.is_empty())) { return None; }
        let actions = actions.iter().map(AppAction::decode).collect::<Option<Vec<_>>>()?;
        let mut seen = HashSet::new();
        let permissions = permissions.iter().map(|permission| {
            let name = text(permission, "name")?;
            if !seen.insert(name.clone()) { return None; }
            Some(AppPermission { name, label: text(permission, "label"), granted: permission.get("granted").and_then(Value::as_bool),
                runtime: permission.get("runtime").and_then(Value::as_bool), protection: text(permission, "protection") })
        }).collect::<Option<Vec<_>>>()?;
        if exists && ((permissions_total == 0 && permissions_offset != 0) || (permissions_total > 0 && permissions_offset >= permissions_total)
            || permissions.len() as u32 != PAGE_SIZE.min(permissions_total.saturating_sub(permissions_offset))) { return None; }
        let empty = Value::Null;
        let storage = value.get("storage").unwrap_or(&empty);
        let available = storage.get("available").and_then(Value::as_bool).unwrap_or(false);
        Some(Self {
            request_id, target, exists, label: text(value, "label"), version_name: text(value, "version_name"),
            version_code: positive(value, "version_code"), target_sdk: positive(value, "target_sdk"),
            enabled: value.get("enabled").and_then(Value::as_bool), system: value.get("system").and_then(Value::as_bool), suspended: value.get("suspended").and_then(Value::as_bool),
            first_install_ms: positive(value, "first_install_ms"), last_update_ms: positive(value, "last_update_ms"),
            first_install_text: text(value, "first_install_text"), last_update_text: text(value, "last_update_text"),
            actions, permissions, permissions_total, permissions_offset,
            storage: AppStorage {
                app_bytes: available.then(|| positive(storage, "app_bytes")).flatten(), data_bytes: available.then(|| positive(storage, "data_bytes")).flatten(),
                cache_bytes: available.then(|| positive(storage, "cache_bytes")).flatten(), available,
                can_request_usage_access: storage.get("can_request_usage_access").and_then(Value::as_bool).unwrap_or(false),
                shared_uid: storage.get("shared_uid").and_then(Value::as_bool).unwrap_or(false),
            },
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use makepad_strict_json::{obj, s};
    pub fn catalog(offset: u32, total: u32, query: &str) -> Value {
        obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(9)),("query",s(query)),("include_system",Value::Bool(false)),
            ("offset",Value::Int(offset as i64)),("page_size",Value::Int(20)),("total",Value::Int(total as i64)),("generation",s("a".repeat(64))),
            ("stale",Value::Bool(false)),("apps",Value::Arr((offset..(offset+PAGE_SIZE).min(total)).map(|n| obj(vec![
                ("package",s(format!("com.example.app{n}"))),("label",s(format!("Example app {n}"))),
                ("system",Value::Bool(false)),("enabled",Value::Bool(true)),("launchable",Value::Bool(true)),
            ])).collect()))])
    }
    pub fn detail(package: &str, permission_offset: u32, total: u32) -> Value {
        obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(10)),("package",s(package)),("exists",Value::Bool(true)),("label",s("Example app")),
            ("enabled",Value::Bool(true)),("system",Value::Bool(false)),("suspended",Value::Bool(false)),("version_name",s("2.3")),("version_code",Value::Int(23)),
            ("target_sdk",Value::Int(35)),("first_install_text",s("Sep 24, 2026")),("last_update_text",s("Sep 25, 2026")),
            ("actions",Value::Arr(vec![s("launch"),s("app_info"),s("notifications")])),
            ("permissions_total",Value::Int(total as i64)),("permissions_offset",Value::Int(permission_offset as i64)),
            ("permissions",Value::Arr((permission_offset..(permission_offset+PAGE_SIZE).min(total)).map(|n| obj(vec![
                ("name",s(format!("android.permission.TEST_{n}"))),("granted",Value::Bool(n%2==0)),("runtime",Value::Bool(true)),("protection",s("dangerous")),
            ])).collect())),("storage",obj(vec![("available",Value::Bool(false)),("can_request_usage_access",Value::Bool(true)),("shared_uid",Value::Bool(true)),
                ("app_bytes",Value::Int(0)),("data_bytes",Value::Int(0)),("cache_bytes",Value::Int(0))]))])
    }
    pub fn replace(value: &mut Value, key: &str, new_value: Value) {
        let Value::Obj(fields) = value else {panic!("object required")};
        if let Some((_, value)) = fields.iter_mut().find(|(name,_)| name == key) { *value = new_value; }
        else { fields.push((key.into(),new_value)); }
    }
    #[test]
    fn catalog_rejects_partial_pages_invalid_targets_and_missing_identity() {
        assert!(AppsCatalog::decode(&catalog(20,39,"邮件")).is_some());
        assert!(AppsCatalog::decode(&catalog(0,0,"")).is_some());
        for (key,value) in [("page_size",Value::Int(40)),("offset",Value::Int(1)),("total",Value::Int(19)),
            ("generation",s("not-a-generation")),("apps",Value::Arr(vec![]))] {
            let mut data = catalog(0,20,"");replace(&mut data,key,value);
            assert!(AppsCatalog::decode(&data).is_none(), "accepted malformed {key}");
        }
        for name in ["../com.example", "com..example", "com.example$Other", "com.示例", "com.example/app", "standalone", "com.1invalid"] {
            let mut data = catalog(0,1,"");
            let Value::Obj(fields) = &mut data else {unreachable!()};
            let Value::Arr(rows) = &mut fields.iter_mut().find(|(k,_)| k=="apps").unwrap().1 else {unreachable!()};
            replace(&mut rows[0],"package",s(name));
            assert!(AppsCatalog::decode(&data).is_none(), "accepted target {name}");
        }
        assert!(AppTarget::decode(&s("android")).is_some());
        assert!(valid_query(&"邮".repeat(128))); assert!(!valid_query(&"邮".repeat(129)));
        assert!(!valid_query("mail\nsettings"));
    }
    #[test]
    fn details_keep_unavailable_storage_and_permission_state_distinct_from_zero_or_denied() {
        let mut data = detail("com.example.app0",0,1);
        let Value::Obj(fields) = &mut data else {unreachable!()};
        let Value::Arr(rows) = &mut fields.iter_mut().find(|(k,_)| k=="permissions").unwrap().1 else {unreachable!()};
        replace(&mut rows[0],"granted",Value::Null);replace(&mut rows[0],"runtime",Value::Null);
        let details = AppDetails::decode(&data).unwrap();
        assert!(!details.storage.available);assert!(details.storage.app_bytes.is_none());
        assert!(details.storage.data_bytes.is_none());assert!(details.storage.can_request_usage_access);
        assert!(details.permissions[0].granted.is_none());assert!(details.permissions[0].runtime.is_none());
        assert_eq!(details.target.package(),"com.example.app0");
        assert!(!details.actions.contains(&AppAction::Uninstall));
        replace(&mut data,"permissions",Value::Arr(vec![]));
        assert!(AppDetails::decode(&data).is_none(),"an incomplete permission page cannot claim completeness");
    }
    #[test]
    fn removed_apps_have_no_actions_and_unknown_actions_fail_closed() {
        let mut removed = obj(vec![("schema",Value::Int(1)),("request_id",Value::Int(11)),("package",s("com.example.gone")),("exists",Value::Bool(false))]);
        assert!(!AppDetails::decode(&removed).unwrap().exists);
        replace(&mut removed,"actions",Value::Arr(vec![s("uninstall")]));
        assert!(AppDetails::decode(&removed).is_none());
        let mut data = detail("com.example.app0",0,0);
        replace(&mut data,"actions",Value::Arr(vec![s("grant_permission")]));
        assert!(AppDetails::decode(&data).is_none());
    }
}
