//! The provider list the service edits, without any UI: ids, key masking and
//! status, the effective route, saving into the octos profile, and the
//! provisioning payload for the phone QR.
use crate::vault::{marker_account, Vault, KEYCHAIN_MARKER};
use octosense_llm_config::{catalog, profile, qr, registry, ApiType, Provider, ProviderSet};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

/// Primary first, then the fallbacks in order.
pub fn list_of(set: &ProviderSet) -> Vec<Provider> {
    set.iter().cloned().collect()
}

/// The first entry is the primary, the rest are its fallbacks.
pub fn set_of(list: &[Provider]) -> ProviderSet {
    let mut it = list.iter().cloned();
    ProviderSet { primary: it.next(), fallbacks: it.collect() }
}

/// A provider's id: a digest of its route, so it survives reordering and
/// changes only when the route does. Two identical routes are one provider.
pub fn id_of(p: &Provider) -> String {
    let mut text = format!(
        "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
        p.family,
        p.model.as_deref().unwrap_or(""),
        p.base_url.as_deref().unwrap_or(""),
        p.api_type.map(|t| t.as_str()).unwrap_or(""),
        p.key_env
    );
    // A named catalog route is part of the address (octos: family, model,
    // route id); routes without one keep the ids they always had.
    if let Some(route) = &p.route_id {
        text.push('\u{0}');
        text.push_str(route);
    }
    // FNV-1a, 64 bits: stable across builds and platforms.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in text.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:012x}", hash & 0xffff_ffff_ffff)
}

pub fn index_of(list: &[Provider], id: &str) -> Result<usize, String> {
    list.iter().position(|p| id_of(p) == id).ok_or_else(|| "There is no such provider.".to_string())
}

/// A key shown in the list: its last four characters only, and only when the
/// key is long enough that four characters give nothing away.
pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() >= 12 {
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("set ••••{tail}")
    } else {
        "set ••••".to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyStatus {
    /// The key resolves; the masked form for display.
    Set(String),
    Missing,
    /// No key stored and the family does not need one.
    NotNeeded,
    /// The key is in the vault but could not be read.
    Unreadable,
}

impl KeyStatus {
    pub fn text(&self) -> String {
        match self {
            KeyStatus::Set(masked) => masked.clone(),
            KeyStatus::Missing => "missing".into(),
            KeyStatus::NotNeeded => "not needed".into(),
            KeyStatus::Unreadable => "keychain locked".into(),
        }
    }
}

/// Resolve the profile value of `key_env` the way octos does: a `keychain:`
/// marker is read from the vault, a raw value is the key itself.
pub fn resolve_key(env_vars: &BTreeMap<String, String>, key_env: &str, vault: &dyn Vault) -> Result<Option<String>, String> {
    let Some(value) = env_vars.get(key_env) else {
        return Ok(None);
    };
    if let Some(account) = marker_account(value, key_env) {
        return vault.get(account);
    }
    // A masked value (`***`) is display residue, not a key.
    if value.is_empty() || value.contains("***") {
        return Ok(None);
    }
    Ok(Some(value.clone()))
}

pub fn key_required(family: &str) -> bool {
    registry::lookup(family).map(|f| f.key_required).unwrap_or(true)
}

pub fn family_label(family: &str) -> String {
    registry::lookup(family).map(|f| f.label.to_string()).unwrap_or_else(|| family.to_string())
}

/// The effective endpoint: the route's own, else the family default.
pub fn effective_base_url(provider: &Provider) -> Option<String> {
    provider
        .base_url
        .clone()
        .filter(|u| !u.trim().is_empty())
        .or_else(|| registry::lookup(&provider.family).and_then(|f| f.default_base_url).map(str::to_string))
}

/// The protocol a route speaks when it names none: Anthropic for Anthropic
/// and the Anthropic-compatible endpoints octos defaults to, else OpenAI.
pub fn effective_api_type(provider: &Provider) -> ApiType {
    if let Some(t) = provider.api_type {
        return t;
    }
    let family = registry::lookup(&provider.family).map(|f| f.id).unwrap_or(provider.family.as_str());
    let base = effective_base_url(provider).unwrap_or_default();
    if family == "anthropic" || base.trim_end_matches('/').ends_with("/anthropic") {
        ApiType::Anthropic
    } else {
        ApiType::OpenAi
    }
}

/// The model a route uses: its own, else the family default.
pub fn effective_model(provider: &Provider) -> Option<String> {
    provider
        .model
        .clone()
        .filter(|m| !m.trim().is_empty())
        .or_else(|| registry::lookup(&provider.family).and_then(|f| f.default_model).map(str::to_string))
}

/// A model's display name: the catalog's (or one made like it).
pub fn model_label(p: &Provider) -> String {
    effective_model(p).map(|m| catalog::model_label(&m)).unwrap_or_default()
}

/// The route's display name ("Official API", "AutoDL", "Custom endpoint").
pub fn route_label(p: &Provider) -> String {
    catalog::route_label(&p.family, p.route_id.as_deref(), p.route_label.as_deref(), p.base_url.as_deref())
}

/// A route chosen on the add sheet: the family, the model, and the catalog
/// route by id (`""` or `official`: the family's own API; `custom`: the
/// typed base URL and protocol; any other: that catalog endpoint, with its
/// base URL and key env var).
pub fn provider_from_route(family: &str, model: &str, route: &str, base_url: &str, api_type: &str) -> Result<Provider, String> {
    let route = route.trim();
    match route {
        "" | catalog::OFFICIAL => provider_from(family, model, "", ""),
        "custom" => {
            if base_url.trim().is_empty() {
                return Err("Type the endpoint's base URL.".into());
            }
            provider_from(family, model, base_url, api_type)
        }
        id => {
            let mut p = provider_from(family, model, "", "")?;
            let fam = catalog::family(&p.family).ok_or("Choose a provider.")?;
            let r = fam.routes().into_iter().find(|r| r.id == id).ok_or("That route is not in the catalog.")?;
            if !r.is_official() {
                p.base_url = r.base_url.clone();
                if let Some(env) = &r.api_key_env {
                    p.key_env = env.clone();
                }
                p.route_id = Some(r.id);
                p.route_label = Some(r.label);
            }
            Ok(p)
        }
    }
}

/// The catalog route id a saved provider is on, for the edit sheet:
/// its own, `custom` for a typed endpoint, else `official`.
pub fn route_choice(p: &Provider) -> String {
    if let Some(id) = &p.route_id {
        return id.clone();
    }
    if route_label(p) == "Custom endpoint" { "custom".into() } else { catalog::OFFICIAL.into() }
}

/// A route as a person described it on the sheet, checked: a registry
/// family, an http(s) base URL, a known protocol.
pub fn provider_from(family: &str, model: &str, base_url: &str, api_type: &str) -> Result<Provider, String> {
    const MAX: usize = 512;
    for (name, value) in [("model", model), ("base URL", base_url)] {
        if value.len() > MAX || value.chars().any(char::is_control) {
            return Err(format!("The {name} is not acceptable text."));
        }
    }
    let family = registry::lookup(family).ok_or("Choose a provider.")?;
    let (model, base_url) = (model.trim(), base_url.trim());
    if !base_url.is_empty() && (!(base_url.starts_with("https://") || base_url.starts_with("http://")) || base_url.contains(' ')) {
        return Err("The base URL must start with https://.".into());
    }
    let api_type = match api_type.trim() {
        "" | "default" => None,
        other => Some(ApiType::parse(other).ok_or("Unknown protocol.")?),
    };
    let mut p = Provider::new(family.id, (!model.is_empty()).then(|| model.to_string()));
    p.base_url = (!base_url.is_empty()).then(|| base_url.to_string());
    p.api_type = api_type;
    if p.base_url.is_none() && family.default_base_url.is_none() {
        return Err(format!("{} needs a base URL.", family.label));
    }
    if effective_model(&p).is_none() {
        return Err(format!("{} needs a model.", family.label));
    }
    Ok(p)
}

/// What a save did (names only, never values).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveReport {
    /// Keys stored in the vault (env var names).
    pub vault: Vec<String>,
    /// Keys written into the profile because the vault did not keep them.
    pub in_profile: Vec<String>,
    /// Env vars dropped from the profile with the providers that used them.
    pub removed: Vec<String>,
}

/// The profile being edited, and the vault its keys live in.
#[derive(Clone)]
pub struct ProfileStore {
    pub path: PathBuf,
    pub vault: Arc<dyn Vault>,
    pub env_vars: BTreeMap<String, String>,
    pub list: Vec<Provider>,
}

impl std::fmt::Debug for ProfileStore {
    /// Names only: `env_vars` may hold raw keys.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProfileStore")
            .field("path", &self.path)
            .field("env_vars", &self.env_vars.keys().collect::<Vec<_>>())
            .field("list", &self.list)
            .finish()
    }
}

impl ProfileStore {
    /// Open (a missing profile is an empty one).
    pub fn open(path: PathBuf, vault: Arc<dyn Vault>) -> Result<Self, String> {
        let (env_vars, list) = if path.exists() {
            let loaded = profile::load(&path).map_err(|e| e.to_string())?;
            (loaded.env_vars, list_of(&loaded.set))
        } else {
            (BTreeMap::new(), Vec::new())
        };
        Ok(ProfileStore { path, vault, env_vars, list })
    }

    /// Reads the vault: call it off the UI thread.
    pub fn status(&self, provider: &Provider) -> KeyStatus {
        match resolve_key(&self.env_vars, &provider.key_env, self.vault.as_ref()) {
            Ok(Some(key)) => KeyStatus::Set(mask_key(&key)),
            Ok(None) if !key_required(&provider.family) => KeyStatus::NotNeeded,
            Ok(None) => KeyStatus::Missing,
            Err(_) => KeyStatus::Unreadable,
        }
    }

    pub fn key(&self, provider: &Provider) -> Option<String> {
        resolve_key(&self.env_vars, &provider.key_env, self.vault.as_ref()).ok().flatten()
    }

    /// Write `list` as the profile's provider set. `new_keys` (env var →
    /// key) go to the vault with a `keychain:` marker in the profile, or into
    /// the profile itself when the vault does not keep them. Key env vars no
    /// provider reads any more leave the profile (a vault entry is kept:
    /// other octos profiles may use it).
    pub fn save(&mut self, list: Vec<Provider>, new_keys: &BTreeMap<String, String>) -> Result<SaveReport, String> {
        let mut report = SaveReport::default();
        let mut env = BTreeMap::new();
        for (name, key) in new_keys {
            let key = key.trim();
            if key.is_empty() {
                continue;
            }
            if self.vault.put(name, key).is_ok() {
                env.insert(name.clone(), KEYCHAIN_MARKER.to_string());
                report.vault.push(name.clone());
            } else {
                env.insert(name.clone(), key.to_string());
                report.in_profile.push(name.clone());
            }
        }
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        profile::save_merge(&self.path, &set_of(&list), &env).map_err(|e| e.to_string())?;
        let in_use: BTreeSet<&str> = list.iter().map(|p| p.key_env.as_str()).collect();
        let unused: Vec<String> = self
            .list
            .iter()
            .map(|p| p.key_env.clone())
            .filter(|name| !in_use.contains(name.as_str()) && self.env_vars.contains_key(name))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if !unused.is_empty() {
            profile::remove_env(&self.path, &unused).map_err(|e| e.to_string())?;
            report.removed = unused;
        }
        let loaded = profile::load(&self.path).map_err(|e| e.to_string())?;
        self.env_vars = loaded.env_vars;
        self.list = list_of(&loaded.set);
        Ok(report)
    }

    /// The payload for the phone: the chosen providers in list order and
    /// every key they need that resolves.
    pub fn provisioning(&self, chosen: &[Provider]) -> qr::Provisioning {
        let mut secrets = BTreeMap::new();
        for p in chosen {
            if let Some(key) = self.key(p) {
                secrets.insert(p.key_env.clone(), key);
            }
        }
        qr::Provisioning { set: set_of(chosen), secrets }
    }
}

/// What importing a code into the saved list does (names only, never keys).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Merge {
    /// The list to save: the saved providers in their order, then the code's
    /// new ones as fallbacks (the code's list as is when nothing was saved).
    pub list: Vec<Provider>,
    /// Keys to store with it (env var → key).
    pub keys: BTreeMap<String, String>,
    /// Nothing was saved: the code's first provider became the primary.
    pub primary: Option<Provider>,
    /// Providers appended as fallbacks.
    pub added: Vec<Provider>,
    /// Saved providers whose key the code changed.
    pub updated: Vec<Provider>,
    /// Saved providers the code named again, unchanged.
    pub unchanged: Vec<Provider>,
}

/// `DEEPSEEK` for DeepSeek: the family's key env var without `_API_KEY`.
fn key_stem(family: &str) -> String {
    let env = registry::key_env_for(family);
    env.strip_suffix("_API_KEY").unwrap_or(&env).to_string()
}

/// Whether `env` is one of `family`'s key slots: its own key env var, or a
/// numbered one an import made (`DEEPSEEK_2_API_KEY`).
fn is_family_slot(env: &str, family: &str) -> bool {
    if env == registry::key_env_for(family) {
        return true;
    }
    let stem = key_stem(family);
    env.strip_prefix(stem.as_str())
        .and_then(|rest| rest.strip_prefix('_'))
        .and_then(|rest| rest.strip_suffix("_API_KEY"))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The same route: family, model, endpoint and protocol, whatever slot its
/// key is in (as long as it is one of the family's).
fn same_route(a: &Provider, b: &Provider) -> bool {
    (a.family.as_str(), a.model.as_deref(), a.base_url.as_deref(), a.api_type, a.route_id.as_deref())
        == (b.family.as_str(), b.model.as_deref(), b.base_url.as_deref(), b.api_type, b.route_id.as_deref())
        && (a.key_env == b.key_env || (is_family_slot(&a.key_env, &a.family) && is_family_slot(&b.key_env, &b.family)))
}

/// Merge an opened code into `store`'s saved list. Nothing saved: the code's
/// list as is. Otherwise nothing saved moves or goes: a provider the code
/// names again keeps its place and takes the code's key; every other one is
/// appended as a fallback. A new provider whose key env var a saved provider
/// already reads, with a different key in the code (or a saved key that
/// cannot be read), gets a fresh slot `<FAMILY>_<n>_API_KEY`, so an import
/// never overwrites a saved provider's key; the same key shares the slot.
pub fn merge_import(store: &ProfileStore, code: &qr::Provisioning) -> Result<Merge, String> {
    let incoming = list_of(&code.set);
    if incoming.is_empty() {
        return Err("The code names no provider.".into());
    }
    if store.list.is_empty() {
        let mut it = incoming.clone().into_iter();
        return Ok(Merge {
            list: incoming,
            keys: code.secrets.clone(),
            primary: it.next(),
            added: it.collect(),
            ..Merge::default()
        });
    }
    let mut m = Merge { list: store.list.clone(), ..Merge::default() };
    let current = |m: &Merge, env: &str| current_key(store, m, env);
    let find = |list: &[Provider], p: &Provider| {
        list.iter().position(|s| id_of(s) == id_of(p)).or_else(|| list.iter().position(|s| same_route(s, p)))
    };
    // The saved providers the code names again.
    let named: BTreeSet<usize> = incoming.iter().filter_map(|p| find(&store.list, p)).collect();
    // Where each of the code's key env vars went, for its later providers.
    let mut slots: BTreeMap<String, String> = BTreeMap::new();
    for p in incoming {
        let secret = code.secrets.get(&p.key_env).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        if let Some(at) = find(&m.list, &p) {
            let saved = m.list[at].clone();
            let changes = secret.as_ref().filter(|key| current(&m, &saved.key_env).ok().flatten().as_deref() != Some(key.as_str()));
            match changes {
                Some(key) => {
                    // A slot another saved provider reads (one the code does
                    // not name) is not overwritten: this one moves to its own.
                    let shared = m.list.iter().enumerate().any(|(i, s)| i != at && !named.contains(&i) && s.key_env == saved.key_env);
                    let slot = if shared { fresh_slot(store, &m, &saved.family)? } else { saved.key_env.clone() };
                    m.keys.insert(slot.clone(), key.clone());
                    m.list[at].key_env = slot;
                    if at < store.list.len() && !m.updated.contains(&saved) {
                        m.updated.push(saved);
                    }
                }
                None => {
                    if at < store.list.len() && !m.unchanged.contains(&saved) && !m.updated.contains(&saved) {
                        m.unchanged.push(saved);
                    }
                }
            }
            continue;
        }
        let slot = match (slots.get(&p.key_env), &secret) {
            (Some(slot), _) => slot.clone(),
            // No key in the code: nothing to write, the slot is shared.
            (None, None) => p.key_env.clone(),
            (None, Some(key)) => {
                let slot = slot_for(store, &m, &p, key)?;
                if current(&m, &slot).ok().flatten().as_deref() != Some(key.as_str()) {
                    m.keys.insert(slot.clone(), key.clone());
                }
                slot
            }
        };
        slots.insert(p.key_env.clone(), slot.clone());
        let mut added = p;
        added.key_env = slot;
        m.list.push(added.clone());
        m.added.push(added);
    }
    Ok(m)
}

/// What a slot holds now: a key this import writes, else the saved one.
fn current_key(store: &ProfileStore, m: &Merge, env: &str) -> Result<Option<String>, String> {
    match m.keys.get(env) {
        Some(k) => Ok(Some(k.clone())),
        None => resolve_key(&store.env_vars, env, store.vault.as_ref()),
    }
}

/// The slot a new provider's `key` goes in: its own env var when no provider
/// reads it or it already holds `key`, else a slot of its family that holds
/// `key`, else the first free `<FAMILY>_<n>_API_KEY`.
fn slot_for(store: &ProfileStore, m: &Merge, p: &Provider, key: &str) -> Result<String, String> {
    let in_use = |env: &str| m.list.iter().any(|s| s.key_env == env) || m.keys.contains_key(env);
    let holds_key = |env: &str| current_key(store, m, env).ok().flatten().as_deref() == Some(key);
    if !in_use(&p.key_env) || holds_key(&p.key_env) {
        return Ok(p.key_env.clone());
    }
    if let Some(s) = m.list.iter().find(|s| is_family_slot(&s.key_env, &p.family) && holds_key(&s.key_env)) {
        return Ok(s.key_env.clone());
    }
    fresh_slot(store, m, &p.family)
}

/// The first free `<FAMILY>_<n>_API_KEY` (n = 2, 3, …): no provider reads
/// it, this import writes nothing to it, the profile has no value for it and
/// the vault keeps nothing under it (another octos profile may use it).
fn fresh_slot(store: &ProfileStore, m: &Merge, family: &str) -> Result<String, String> {
    let stem = key_stem(family);
    for n in 2..100 {
        let name = format!("{stem}_{n}_API_KEY");
        let taken = m.list.iter().any(|s| s.key_env == name) || m.keys.contains_key(&name) || store.env_vars.contains_key(&name);
        if !taken && matches!(store.vault.get(&name), Ok(None)) {
            return Ok(name);
        }
    }
    Err(format!("{} has no free key slot.", family_label(family)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{InProfile, MemoryVault};

    fn p(family: &str) -> Provider {
        Provider::new(family, None)
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("llm-model-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn masking_shows_only_the_last_four() {
        assert_eq!(mask_key("sk-test-abcdefgh1234"), "set ••••1234");
        assert_eq!(mask_key("short-key"), "set ••••");
    }

    #[test]
    fn ids_follow_the_route_not_the_position() {
        let a = p("openai");
        let mut b = p("openai");
        assert_eq!(id_of(&a), id_of(&b));
        b.model = Some("gpt-test".into());
        assert_ne!(id_of(&a), id_of(&b));
        assert_eq!(index_of(&[b.clone(), a.clone()], &id_of(&a)).unwrap(), 1);
    }

    #[test]
    fn a_sheet_route_is_checked() {
        assert!(provider_from("nope", "", "", "").is_err());
        assert!(provider_from("openai", "", "ftp://x", "").is_err());
        assert!(provider_from("vllm", "m", "", "").unwrap_err().contains("base URL"));
        assert!(provider_from("vllm", "", "http://gpu:8000/v1", "").unwrap_err().contains("model"));
        let ok = provider_from("DeepSeek", " deepseek-chat ", "", "openai").unwrap();
        assert_eq!(ok.family, "deepseek");
        assert_eq!(ok.model.as_deref(), Some("deepseek-chat"));
        assert_eq!(ok.api_type, Some(ApiType::OpenAi));
        assert_eq!(ok.key_env, "DEEPSEEK_API_KEY");
    }

    #[test]
    fn api_type_defaults() {
        assert_eq!(effective_api_type(&p("anthropic")), ApiType::Anthropic);
        assert_eq!(effective_api_type(&p("zai-coding")), ApiType::Anthropic);
        assert_eq!(effective_api_type(&p("openai")), ApiType::OpenAi);
    }

    #[test]
    fn keys_go_to_the_vault_or_the_profile_and_leave_with_their_provider() {
        let dir = temp_dir("save");
        let path = profile::profile_path(&dir);
        let mut store = ProfileStore::open(path.clone(), Arc::new(MemoryVault::default())).unwrap();
        let list = vec![p("openai"), p("anthropic"), p("ollama")];
        let keys: BTreeMap<String, String> = [
            ("OPENAI_API_KEY".to_string(), "sk-test-openai-aaaa1111".to_string()),
            ("ANTHROPIC_API_KEY".to_string(), "sk-test-anthropic-bbbb2222".to_string()),
        ]
        .into();
        let report = store.save(list.clone(), &keys).unwrap();
        assert_eq!(report.vault, ["ANTHROPIC_API_KEY", "OPENAI_API_KEY"]);
        assert!(!std::fs::read_to_string(&path).unwrap().contains("sk-test-"), "markers, not keys");
        assert_eq!(store.status(&list[1]), KeyStatus::Set("set ••••2222".into()));
        assert_eq!(store.status(&list[2]), KeyStatus::NotNeeded);
        let prov = store.provisioning(&[list[0].clone(), list[2].clone()]);
        assert_eq!(prov.secrets.len(), 1);
        assert_eq!(prov.secrets["OPENAI_API_KEY"], "sk-test-openai-aaaa1111");

        let report = store.save(vec![list[0].clone(), list[2].clone()], &BTreeMap::new()).unwrap();
        assert_eq!(report.removed, ["ANTHROPIC_API_KEY"]);
        assert!(!store.env_vars.contains_key("ANTHROPIC_API_KEY"));

        // No vault: the key is in the owner-only profile.
        let mut plain = ProfileStore::open(path.clone(), Arc::new(InProfile)).unwrap();
        let keys: BTreeMap<String, String> = [("DEEPSEEK_API_KEY".to_string(), "sk-test-ds-cccc3333".to_string())].into();
        let report = plain.save(vec![p("deepseek")], &keys).unwrap();
        assert_eq!(report.in_profile, ["DEEPSEEK_API_KEY"]);
        assert_eq!(plain.status(&p("deepseek")), KeyStatus::Set("set ••••3333".into()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o077, 0);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    fn with_key(family: &str, model: &str, key_env: &str) -> Provider {
        let mut p = Provider::new(family, Some(model.into()));
        p.key_env = key_env.into();
        p
    }

    #[test]
    fn an_import_never_overwrites_a_key_another_saved_provider_reads() {
        let dir = temp_dir("merge");
        let vault = Arc::new(MemoryVault::default());
        let mut store = ProfileStore::open(profile::profile_path(&dir), vault.clone()).unwrap();
        // Two saved DeepSeek routes on one key; DEEPSEEK_2 is taken in the
        // vault by something else (another octos profile).
        let saved = vec![with_key("deepseek", "a", "DEEPSEEK_API_KEY"), with_key("deepseek", "b", "DEEPSEEK_API_KEY")];
        store.save(saved.clone(), &[("DEEPSEEK_API_KEY".to_string(), "sk-test-saved-11112222".to_string())].into()).unwrap();
        vault.put("DEEPSEEK_2_API_KEY", "sk-test-elsewhere").unwrap();

        // The code names `b` again with another key: `b` keeps its place but
        // moves to a slot of its own, since `a` reads the saved key.
        let code = qr::Provisioning {
            set: set_of(&[with_key("deepseek", "b", "DEEPSEEK_API_KEY"), with_key("deepseek", "c", "DEEPSEEK_API_KEY")]),
            secrets: [("DEEPSEEK_API_KEY".to_string(), "sk-test-code-33334444".to_string())].into(),
        };
        let m = merge_import(&store, &code).unwrap();
        assert_eq!(
            m.list,
            [
                with_key("deepseek", "a", "DEEPSEEK_API_KEY"),
                with_key("deepseek", "b", "DEEPSEEK_3_API_KEY"),
                with_key("deepseek", "c", "DEEPSEEK_3_API_KEY"),
            ],
            "the new one shares the slot that now holds the same key"
        );
        assert_eq!(m.keys, [("DEEPSEEK_3_API_KEY".to_string(), "sk-test-code-33334444".to_string())].into());
        assert_eq!((m.updated.len(), m.added.len()), (1, 1));

        // Slots are recognised as the family's: the same route in any of
        // them is the same provider.
        assert!(is_family_slot("DEEPSEEK_12_API_KEY", "deepseek"));
        assert!(!is_family_slot("DEEPSEEK_X_API_KEY", "deepseek"));
        assert!(!is_family_slot("ZAI_2_API_KEY", "deepseek"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
