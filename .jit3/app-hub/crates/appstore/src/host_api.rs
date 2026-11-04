//! Descriptions of implemented host methods. No credentials or account data.
use crate::services::{AgentAccess, HostApiMethod, Replier, ServiceCall};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

static METHODS: OnceLock<Mutex<BTreeMap<String, Vec<HostApiMethod>>>> = OnceLock::new();
static FEATURES: OnceLock<Mutex<BTreeMap<String, u32>>> = OnceLock::new();

pub(crate) fn register_methods(family: &str, methods: Vec<HostApiMethod>) {
    let mut accepted = BTreeMap::new();
    for method in methods {
        if method.validate().is_err() || method.name.split('.').next() != Some(family)
            || method.name.split('.').any(|s| s == "sheet") {
            makepad_widgets::error!("invalid public host API descriptor for {family}");
            continue;
        }
        if accepted.insert(method.name.clone(), method).is_some() {
            makepad_widgets::error!("duplicate host API descriptor for {family}");
            // Ambiguous descriptions must never make an app installable.
            accepted.clear();
            break;
        }
    }
    METHODS.get_or_init(Default::default).lock().unwrap()
        .insert(family.to_owned(), accepted.into_values().collect());
}

/// A runtime integration may advertise its own non-request ABI, such as the
/// full card runner's app_tools.dispatch. Register only after that ABI exists.
pub fn register_runtime_feature(name: &str, version: u32) {
    assert!(octosense_app_policy::contract::host_api::valid_method(name) && version > 0);
    FEATURES.get_or_init(Default::default).lock().unwrap().insert(name.to_owned(), version);
}

pub fn platform() -> &'static str {
    if cfg!(target_os = "android") { "android" }
    else if cfg!(target_os = "macos") { "macos" }
    else if cfg!(target_os = "windows") { "windows" }
    else if cfg!(target_os = "ios") { "ios" }
    else if cfg!(target_env = "ohos") { "openharmony" }
    else if cfg!(target_arch = "wasm32") { "web" }
    else if cfg!(target_os = "linux") { "linux" }
    else { "unknown" }
}

fn discovery_methods() -> Vec<HostApiMethod> {
    [
        ("runtime.list", "List implemented host API descriptions; availability does not grant access.", json!({"type":"object","properties":{},"additionalProperties":false})),
        ("runtime.describe", "Describe one host API or report that this host does not implement it.", json!({"type":"object","properties":{"method":{"type":"string"}},"required":["method"],"additionalProperties":false})),
    ].into_iter().map(|(name, summary, input)| HostApiMethod::new(name, 1, "runtime", summary, input, json!({"type":"object"}))
        .with_platforms(&["android", "macos", "windows", "linux", "ios", "openharmony", "web"])
        .with_agent_access(AgentAccess::Allowed)).collect()
}

pub fn methods() -> Vec<HostApiMethod> {
    let mut methods: Vec<_> = METHODS.get_or_init(Default::default).lock().unwrap().values().flatten().cloned().collect();
    methods.extend(discovery_methods());
    methods.sort_by(|a, b| a.name.cmp(&b.name));
    methods
}

pub fn available_versions() -> BTreeMap<String, u32> {
    let mut versions: BTreeMap<_, _> = methods().into_iter().filter(|m| m.supports(platform())).map(|m| (m.name, m.version)).collect();
    versions.extend(FEATURES.get_or_init(Default::default).lock().unwrap().clone());
    versions
}

pub fn check_manifest(manifest: &octosense_app_policy::AppManifest) -> Result<(), String> {
    manifest.check_host_apis(&available_versions())
}

fn runtime_features() -> BTreeMap<String, u32> {
    FEATURES.get_or_init(Default::default).lock().unwrap().clone()
}

fn describe_name(name: &str) -> Value {
    if let Some(method) = methods().into_iter().find(|m| m.name == name) {
        return describe(method);
    }
    if let Some(version) = runtime_features().get(name) {
        return json!({"method":name,"version":version,"kind":"runtime-abi","implemented":true,
            "supported":true,"callable_via_host_request":false,"authorization":"checked-on-use"});
    }
    json!({"method":name,"implemented":false,"supported":false,"configured":null,"authorization":"unavailable"})
}

fn describe(method: HostApiMethod) -> Value {
    json!({"implemented":true,"supported":method.supports(platform()),"configured":null,
        "authorization":"checked-on-call", "descriptor":method})
}

pub(crate) fn dispatch(call: ServiceCall, reply: Replier) {
    let Some(args) = call.args.as_object() else { reply.send(Err("runtime arguments must be an object".into())); return };
    let result = match call.method() {
        "list" if args.is_empty() => Ok(json!({"schema":1,"platform":platform(),"runtime_features":runtime_features(),"methods":methods().into_iter().map(describe).collect::<Vec<_>>(),
            "notice":"API presence does not imply a configured account, app consent or OS permission. Use the service's status method; authorization is rechecked on every operation."})),
        "describe" if args.len() == 1 && args.get("method").is_some_and(Value::is_string) => {
            let name = args["method"].as_str().unwrap();
            Ok(describe_name(name))
        }
        _ => Err("runtime supports list {} and describe {method}; unknown arguments are refused".into()),
    };
    reply.send(result);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_abis_are_discoverable_without_claiming_request_handlers() {
        register_runtime_feature("feature_probe.dispatch", 1);
        let described = describe_name("feature_probe.dispatch");
        assert_eq!(described["kind"], "runtime-abi");
        assert_eq!(described["callable_via_host_request"], false);
        assert_eq!(available_versions().get("feature_probe.dispatch"), Some(&1));
        assert_eq!(describe_name("missing_probe.read")["implemented"], false);
    }

    #[test]
    fn only_implemented_supported_public_methods_are_advertised() {
        let method = HostApiMethod::new("metadata_probe.read",1,"storage","Read",json!({}),json!({})).with_platforms(&[platform()]);
        let private = HostApiMethod::new("metadata_probe.sheet.save",1,"storage","Private",json!({}),json!({})).with_platforms(&[platform()]);
        register_methods("metadata_probe", vec![method,private]);
        assert_eq!(available_versions().get("metadata_probe.read"),Some(&1));
        assert!(!available_versions().contains_key("metadata_probe.sheet.save"));
        register_methods("metadata_probe",Vec::new());
        assert!(!available_versions().contains_key("metadata_probe.read"));
    }
}
