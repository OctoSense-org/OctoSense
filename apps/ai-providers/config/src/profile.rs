//! The embedded kernel's octos profile, `profiles/_main.json`.
//!
//! Shape (octos-cli `profiles.rs` `UserProfile`):
//!
//! ```json
//! {"id": "_main", "name": "Main", "enabled": true,
//!  "created_at": "…", "updated_at": "…",
//!  "config": {
//!    "llm": {"primary": {"family_id": "deepseek", "model_id": "deepseek-chat",
//!                        "route": {"base_url": "…", "api_key_env": "…", "api_type": "openai"}},
//!            "fallbacks": [ … ]},
//!    "env_vars": {"DEEPSEEK_API_KEY": "sk-…"},
//!    …everything else, untouched…}}
//! ```
//!
//! `config.env_vars` values are either the raw secret or a keychain marker:
//! `"keychain:"` (bare, legacy — the keychain account is the env var name)
//! or `"keychain:<account>"` (scoped; octos uses `<ENV>::<profile id>`). octos
//! resolves markers from the OS keychain at spawn time (octos-cli
//! `auth/keychain.rs`); this crate stores what it is given verbatim and never
//! resolves a marker — see [`is_keychain_marker`].
//!
//! Writes merge: only `config.llm` and the named env vars change. The
//! envelope octos requires (`id`, `name`, `created_at`, `updated_at`) is
//! filled in when missing and `enabled` is forced on — a bare `{config}` file
//! parses as *no profile* in octos, and the AppUI bootstrap skips disabled
//! profiles, so every turn would fail with "profile '_main' is not configured".
//! The file is replaced atomically (temp file + rename) with mode 0600.

use crate::{json_error, registry, ApiType, Error, Provider, ProviderSet, Result};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Prefix of a keychain marker value in `config.env_vars`.
pub const KEYCHAIN_MARKER: &str = "keychain:";

/// Whether an env var value is a keychain marker rather than the secret.
pub fn is_keychain_marker(value: &str) -> bool {
    value.starts_with(KEYCHAIN_MARKER)
}

/// The embedded kernel's octos home: `$OCTOS_APP_CORE_DIR`, else
/// `$HOME/octos-home/.octos` (the HOME the kernel is spawned with).
pub fn default_core_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("OCTOS_APP_CORE_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").filter(|v| !v.is_empty())?;
    Some(PathBuf::from(home).join("octos-home/.octos"))
}

/// `<core_dir>/profiles/_main.json`.
pub fn profile_path(core_dir: &Path) -> PathBuf {
    core_dir.join("profiles").join("_main.json")
}

/// The LLM part of a profile. `env_vars` holds SECRETS (or keychain markers):
/// keep it out of logs and UI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loaded {
    pub set: ProviderSet,
    pub env_vars: BTreeMap<String, String>,
}

/// Read the provider set and env vars. A missing file is an empty profile.
/// Selections without a `family_id` are skipped; unknown selection fields
/// (`model_hints`, `cost_per_m`, …) are ignored here and kept on save.
pub fn load(path: &Path) -> Result<Loaded> {
    let Some(root) = read_root(path)? else {
        return Ok(Loaded::default());
    };
    let config = root.get("config");
    let llm = config.and_then(|c| c.get("llm"));
    let set = ProviderSet {
        primary: llm.and_then(|l| l.get("primary")).and_then(provider_from_selection),
        fallbacks: llm
            .and_then(|l| l.get("fallbacks"))
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(provider_from_selection).collect())
            .unwrap_or_default(),
    };
    let env_vars = config
        .and_then(|c| c.get("env_vars"))
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    Ok(Loaded { set, env_vars })
}

/// Replace `config.llm` with `set` (an empty set removes it), upsert `env`
/// into `config.env_vars`, keep everything else, heal the envelope and write
/// atomically. A selection that keeps its family and model keeps its extra
/// octos fields (`model_hints`, `context_window`, …).
pub fn save_merge(path: &Path, set: &ProviderSet, env: &BTreeMap<String, String>) -> Result<()> {
    edit(path, |cfg| {
        let old = cfg.get("llm").cloned().unwrap_or(Value::Null);
        if set.is_empty() {
            cfg.remove("llm");
        } else {
            let old_fallbacks = old.get("fallbacks").and_then(Value::as_array);
            let mut llm = Map::new();
            if let Some(p) = &set.primary {
                llm.insert("primary".into(), selection_json(p, old.get("primary")));
            }
            let fallbacks = set
                .fallbacks
                .iter()
                .enumerate()
                .map(|(i, p)| selection_json(p, old_fallbacks.and_then(|a| a.get(i))))
                .collect();
            llm.insert("fallbacks".into(), Value::Array(fallbacks));
            cfg.insert("llm".into(), Value::Object(llm));
        }
        upsert_env(cfg, env)
    })
}

/// Replace only `config.llm.primary`, keeping the fallbacks verbatim — what a
/// legacy single-provider QR means. Env vars and envelope as [`save_merge`].
pub fn save_merge_primary(path: &Path, primary: &Provider, env: &BTreeMap<String, String>) -> Result<()> {
    edit(path, |cfg| {
        let llm = cfg.entry("llm").or_insert_with(|| Value::Object(Map::new()));
        if !llm.is_object() {
            *llm = Value::Object(Map::new());
        }
        let llm = llm.as_object_mut().expect("object ensured above");
        let sel = selection_json(primary, llm.get("primary"));
        llm.insert("primary".into(), sel);
        llm.entry("fallbacks").or_insert_with(|| Value::Array(Vec::new()));
        upsert_env(cfg, env)
    })
}

/// Delete env vars (e.g. the key of a provider being removed). A missing
/// file or name is not an error.
pub fn remove_env(path: &Path, names: &[String]) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    edit(path, |cfg| {
        if let Some(env) = cfg.get_mut("env_vars").and_then(Value::as_object_mut) {
            for n in names {
                env.remove(n);
            }
        }
        Ok(())
    })
}

// ---------------------------------------------------------------------------

fn read_root(path: &Path) -> Result<Option<Value>> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let root: Value = serde_json::from_slice(&bytes)
        .map_err(|e| Error::Invalid(format!("parse {}: {}", path.display(), json_error(&e))))?;
    if !root.is_object() {
        return Err(Error::Invalid(format!("{} is not a JSON object", path.display())));
    }
    Ok(Some(root))
}

/// Load (or start) the profile, heal the envelope, let `f` edit `config`,
/// write it back atomically.
fn edit(path: &Path, f: impl FnOnce(&mut Map<String, Value>) -> Result<()>) -> Result<()> {
    let mut root = read_root(path)?.unwrap_or_else(|| Value::Object(Map::new()));
    let obj = root.as_object_mut().expect("read_root returns objects");
    heal_envelope(obj);
    if !obj.get("config").is_some_and(Value::is_object) {
        obj.insert("config".into(), Value::Object(Map::new()));
    }
    f(obj.get_mut("config").and_then(Value::as_object_mut).expect("config ensured above"))?;
    let body = serde_json::to_vec_pretty(&root).map_err(|e| Error::Invalid(json_error(&e)))?;
    write_atomic(path, &body)
}

fn heal_envelope(obj: &mut Map<String, Value>) {
    if !obj.get("id").is_some_and(Value::is_string) {
        obj.insert("id".into(), Value::from("_main"));
    }
    if !obj.get("name").is_some_and(Value::is_string) {
        obj.insert("name".into(), Value::from("Main"));
    }
    obj.insert("enabled".into(), Value::Bool(true));
    let now = chrono::Utc::now().to_rfc3339();
    if !obj.get("created_at").is_some_and(Value::is_string) {
        obj.insert("created_at".into(), Value::from(now.clone()));
    }
    obj.insert("updated_at".into(), Value::from(now));
}

fn upsert_env(cfg: &mut Map<String, Value>, env: &BTreeMap<String, String>) -> Result<()> {
    if env.is_empty() {
        return Ok(());
    }
    for name in env.keys() {
        if !crate::is_env_name(name) {
            return Err(Error::Invalid(format!("`{name}` is not a legal env var name")));
        }
    }
    let slot = cfg.entry("env_vars").or_insert_with(|| Value::Object(Map::new()));
    if !slot.is_object() {
        *slot = Value::Object(Map::new());
    }
    let map = slot.as_object_mut().expect("object ensured above");
    for (k, v) in env {
        map.insert(k.clone(), Value::from(v.clone()));
    }
    Ok(())
}

fn same_family(a: &str, b: &str) -> bool {
    let canon = |s: &str| registry::lookup(s).map(|f| f.id.to_string()).unwrap_or_else(|| s.to_ascii_lowercase());
    canon(a) == canon(b)
}

/// The octos `LlmModelSelectionConfig` JSON for `p`, starting from `old`
/// when it is the same family and model so its extra fields survive.
fn selection_json(p: &Provider, old: Option<&Value>) -> Value {
    let reuse = old.and_then(Value::as_object).filter(|o| {
        o.get("family_id").and_then(Value::as_str).is_some_and(|f| same_family(f, &p.family))
            && o.get("model_id").and_then(Value::as_str) == p.model.as_deref()
    });
    let mut sel = reuse.cloned().unwrap_or_default();
    sel.insert("family_id".into(), Value::from(p.family.clone()));
    match &p.model {
        Some(m) => sel.insert("model_id".into(), Value::from(m.clone())),
        None => sel.remove("model_id"),
    };

    let mut route = sel.get("route").and_then(Value::as_object).cloned().unwrap_or_default();
    if route.get("base_url").and_then(Value::as_str) != p.base_url.as_deref() {
        // A route id/label names an endpoint; it no longer describes this one.
        route.remove("route_id");
        route.remove("label");
    }
    // A catalog route other than the official one is named (octos
    // `LlmRouteConfig.route_id` / `label`); the official one is left as it
    // was (octos reads a missing id as "official").
    if let Some(id) = &p.route_id {
        route.insert("route_id".into(), Value::from(id.clone()));
        match &p.route_label {
            Some(l) => route.insert("label".into(), Value::from(l.clone())),
            None => route.remove("label"),
        };
    }
    match &p.base_url {
        Some(u) => route.insert("base_url".into(), Value::from(u.trim())),
        None => route.remove("base_url"),
    };
    match p.api_type {
        Some(t) => route.insert("api_type".into(), Value::from(t.as_str())),
        None => route.remove("api_type"),
    };
    // Only a non-default key var is spelled out; octos derives the default.
    if p.key_env != registry::key_env_for(&p.family) {
        route.insert("api_key_env".into(), Value::from(p.key_env.clone()));
    } else {
        route.remove("api_key_env");
    }
    if route.is_empty() {
        sel.remove("route");
    } else {
        sel.insert("route".into(), Value::Object(route));
    }
    Value::Object(sel)
}

/// Lenient read of an octos selection: known fields only, unknown ignored.
pub(crate) fn provider_from_selection(v: &Value) -> Option<Provider> {
    let family = v.get("family_id")?.as_str()?.trim().to_string();
    if family.is_empty() {
        return None;
    }
    let route = v.get("route");
    let rs = |k: &str| {
        route
            .and_then(|r| r.get(k))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    // The official route (by name, or by the family's own id, as the TUI
    // spells it) is no route: octos reads a missing id as "official".
    let route_id = rs("route_id").filter(|id| id != crate::catalog::OFFICIAL && !same_family(id, &family));
    Some(Provider {
        model: v.get("model_id").and_then(Value::as_str).map(str::to_string),
        base_url: rs("base_url"),
        api_type: rs("api_type").as_deref().and_then(ApiType::parse),
        key_env: rs("api_key_env").unwrap_or_else(|| registry::key_env_for(&family)),
        route_label: route_id.as_ref().and_then(|_| rs("label")),
        route_id,
        family,
    })
}

/// The octos selection JSON for `p` with no prior state (QR payloads).
pub(crate) fn selection_value(p: &Provider) -> Value {
    selection_json(p, None)
}

fn write_atomic(path: &Path, body: &[u8]) -> Result<()> {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    std::fs::create_dir_all(&dir)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("profile.json");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".{name}.{}.{nanos}.tmp", std::process::id()));
    let result = (|| {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(body)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(result?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d = std::env::temp_dir().join(format!("llm-config-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn read(path: &Path) -> Value {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn missing_file_loads_empty() {
        let d = tmpdir("missing");
        let l = load(&d.join("nope.json")).unwrap();
        assert_eq!(l, Loaded::default());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn merge_preserves_unrelated_keys_and_heals_envelope() {
        let d = tmpdir("merge");
        let path = profile_path(&d);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            br#"{"custom_top":1,"config":{"memory":{"max_inject_tokens":60000},"custom":"keep-me",
                "env_vars":{"OTHER":"keychain:OTHER::_main"},
                "llm":{"primary":{"family_id":"deepseek","model_id":"deepseek-chat","context_window":65536,
                        "route":{"route_id":"official","label":"Official"}},
                       "fallbacks":[{"family_id":"openai"}]}}}"#,
        )
        .unwrap();

        let mut fb = Provider::new("zai", Some("glm-4.6".into()));
        fb.api_type = Some(ApiType::Anthropic);
        let set = ProviderSet { primary: Some(Provider::new("deepseek", Some("deepseek-chat".into()))), fallbacks: vec![fb] };
        let env = BTreeMap::from([("ZAI_API_KEY".to_string(), "zai-test".to_string())]);
        save_merge(&path, &set, &env).unwrap();

        let v = read(&path);
        assert_eq!(v["id"], "_main");
        assert_eq!(v["name"], "Main");
        assert_eq!(v["enabled"], true);
        assert!(v["created_at"].is_string() && v["updated_at"].is_string());
        assert_eq!(v["custom_top"], 1);
        assert_eq!(v["config"]["custom"], "keep-me");
        assert_eq!(v["config"]["memory"]["max_inject_tokens"], 60000);
        assert_eq!(v["config"]["env_vars"]["OTHER"], "keychain:OTHER::_main");
        assert_eq!(v["config"]["env_vars"]["ZAI_API_KEY"], "zai-test");
        let primary = &v["config"]["llm"]["primary"];
        // Same family+model: the operator's extras survive, the route is unchanged.
        assert_eq!(primary["context_window"], 65536);
        assert_eq!(primary["route"]["route_id"], "official");
        let fbs = v["config"]["llm"]["fallbacks"].as_array().unwrap();
        assert_eq!(fbs.len(), 1);
        assert_eq!(fbs[0]["family_id"], "zai");
        assert_eq!(fbs[0]["route"]["api_type"], "anthropic");
        assert!(fbs[0]["route"].get("api_key_env").is_none());

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.set, set);
        assert_eq!(loaded.env_vars["ZAI_API_KEY"], "zai-test");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        // No temp files left behind.
        assert_eq!(std::fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn switching_provider_drops_the_old_route_and_custom_key_env_round_trips() {
        let d = tmpdir("switch");
        let path = d.join("_main.json");
        let mut p = Provider::new("openai", Some("qwen3.8-27b".into()));
        p.base_url = Some("http://127.0.0.1:30880/v1".into());
        p.api_type = Some(ApiType::OpenAi);
        p.key_env = "LOCAL_LLM_KEY".into();
        save_merge(&path, &ProviderSet { primary: Some(p.clone()), fallbacks: vec![] }, &BTreeMap::new()).unwrap();
        assert_eq!(read(&path)["config"]["llm"]["primary"]["route"]["api_key_env"], "LOCAL_LLM_KEY");
        assert_eq!(load(&path).unwrap().set.primary.unwrap(), p);

        save_merge_primary(&path, &Provider::new("zai", Some("glm-5.3-flash".into())), &BTreeMap::new()).unwrap();
        let v = read(&path);
        assert!(v["config"]["llm"]["primary"].get("route").is_none());
        assert_eq!(v["config"]["llm"]["fallbacks"], serde_json::json!([]));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn a_catalog_route_is_saved_as_octos_names_it() {
        let d = tmpdir("route");
        let path = d.join("_main.json");
        let mut autodl = Provider::new("deepseek", Some("deepseek-v4-flash".into()));
        autodl.route_id = Some("autodl".into());
        autodl.route_label = Some("AutoDL".into());
        autodl.base_url = Some("https://www.autodl.art/api/v1".into());
        autodl.key_env = "AUTODL_API_KEY".into();
        let set = ProviderSet { primary: Some(Provider::new("deepseek", Some("deepseek-v4-flash".into()))), fallbacks: vec![autodl.clone()] };
        save_merge(&path, &set, &BTreeMap::new()).unwrap();
        let v = read(&path);
        assert!(v["config"]["llm"]["primary"].get("route").is_none(), "the official route is no route");
        assert_eq!(
            v["config"]["llm"]["fallbacks"][0]["route"],
            serde_json::json!({"route_id": "autodl", "label": "AutoDL", "base_url": "https://www.autodl.art/api/v1", "api_key_env": "AUTODL_API_KEY"})
        );
        assert_eq!(load(&path).unwrap().set, set);

        // The TUI's spelling of the official route reads as no route.
        std::fs::write(&path, br#"{"config":{"llm":{"primary":{"family_id":"zai","model_id":"glm-5.2","route":{"route_id":"zai","label":"Z.ai"}}}}}"#).unwrap();
        let p = load(&path).unwrap().set.primary.unwrap();
        assert_eq!((p.route_id, p.route_label), (None, None));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn primary_only_merge_keeps_fallbacks_verbatim() {
        let d = tmpdir("primary");
        let path = d.join("_main.json");
        std::fs::write(&path, br#"{"config":{"llm":{"fallbacks":[{"family_id":"openai","cost_per_m":2.5}]}}}"#).unwrap();
        save_merge_primary(&path, &Provider::new("deepseek", None), &BTreeMap::new()).unwrap();
        let v = read(&path);
        assert_eq!(v["config"]["llm"]["fallbacks"][0]["cost_per_m"], 2.5);
        assert_eq!(v["config"]["llm"]["primary"]["family_id"], "deepseek");
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn remove_env_and_empty_set() {
        let d = tmpdir("remove");
        let path = d.join("_main.json");
        let env = BTreeMap::from([("A_API_KEY".to_string(), "a".to_string()), ("B_API_KEY".to_string(), "b".to_string())]);
        save_merge(&path, &ProviderSet { primary: Some(Provider::new("deepseek", None)), fallbacks: vec![] }, &env).unwrap();
        remove_env(&path, &["A_API_KEY".to_string(), "NOT_THERE".to_string()]).unwrap();
        save_merge(&path, &ProviderSet::default(), &BTreeMap::new()).unwrap();
        let v = read(&path);
        assert!(v["config"]["env_vars"].get("A_API_KEY").is_none());
        assert_eq!(v["config"]["env_vars"]["B_API_KEY"], "b");
        assert!(v["config"].get("llm").is_none());
        remove_env(&d.join("absent.json"), &["X".to_string()]).unwrap();
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn corrupt_profile_errors_without_echoing_contents() {
        let d = tmpdir("corrupt");
        let path = d.join("_main.json");
        std::fs::write(&path, br#"{"config":{"env_vars":{"K":"sk-secret-value"}"#).unwrap();
        let err = save_merge(&path, &ProviderSet::default(), &BTreeMap::new()).unwrap_err();
        assert!(!err.to_string().contains("sk-secret"), "{err}");
        std::fs::write(&path, br#"[1]"#).unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn keychain_markers_are_recognised() {
        assert!(is_keychain_marker("keychain:"));
        assert!(is_keychain_marker("keychain:VERTEX_SA_JSON::_main"));
        assert!(!is_keychain_marker("sk-abc"));
    }
}
