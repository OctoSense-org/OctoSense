//! Where the kernel's state lives.
//!
//! The **core dir** is octos's data dir: `<core_dir>/profiles/_main.json` is
//! the profile the AI providers app's `llm` service writes and the kernel
//! reads. It is OctoSense's own: `<app data dir>/octos-home/.octos`, where
//! the app data dir is the platform's (a phone's files dir) or OctoSense's
//! state dir on a desktop (`~/.octosense`). On a phone the kernel is spawned
//! with that HOME (Android) or served from it (OpenHarmony,
//! `octos_cli::embedded::serve_io(home, ..)`).
//!
//! A desktop OctoSense used to share the person's own octos home
//! (`$HOME/octos-home/.octos`) with their standalone octos. It no longer
//! does, since OctoSense writes its own tool policy into its profile; on
//! first use [`migrate_shared_profile`] COPIES the provider and model
//! settings from there, never modifying or deleting the person's files.

use std::path::{Path, PathBuf};

/// Resolve the core dir, first match wins:
///
/// 1. `explicit` (the shell's [`crate::Options::core_dir`]);
/// 2. `$OCTOS_APP_CORE_DIR` (non-empty);
/// 3. `<app_data_dir>/octos-home/.octos` when the shell named its data dir
///    (a phone's files dir; OctoSense's state dir on a desktop): OctoSense's
///    own octos home;
/// 4. `octosense_llm_config::profile::default_core_dir()`:
///    `$HOME/octos-home/.octos`, for a consumer that names nothing (the
///    kernel then never writes a tool policy there, see `system_tools`).
pub fn resolve_core_dir(explicit: Option<&Path>, app_data_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = explicit.filter(|d| !d.as_os_str().is_empty()) {
        return Some(dir.to_path_buf());
    }
    if let Some(dir) = std::env::var_os("OCTOS_APP_CORE_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    if let Some(data) = app_data_dir.filter(|d| !d.as_os_str().is_empty()) {
        return Some(data.join("octos-home").join(".octos"));
    }
    octosense_llm_config::profile::default_core_dir()
}

/// The shared octos home an OctoSense core dir may inherit settings from:
/// `$HOME/octos-home/.octos`, only on a desktop whose core dir comes from
/// its app data dir (an explicit or `$OCTOS_APP_CORE_DIR` core dir is the
/// caller's choice; phones never shared it).
pub(crate) fn shared_profile_source(explicit: Option<&Path>, app_data_dir: Option<&Path>) -> Option<PathBuf> {
    let desktop = !cfg!(any(target_os = "android", target_env = "ohos", target_os = "ios"));
    let derived = explicit.is_none_or(|d| d.as_os_str().is_empty())
        && std::env::var_os("OCTOS_APP_CORE_DIR").is_none_or(|v| v.is_empty())
        && app_data_dir.is_some_and(|d| !d.as_os_str().is_empty());
    (desktop && derived).then(octosense_llm_config::profile::default_core_dir).flatten()
}

/// The person's own octos home, `$HOME/octos-home/.octos`: the one core dir
/// OctoSense never writes a tool policy into. Never `$OCTOS_APP_CORE_DIR`:
/// that names the core dir OctoSense's kernel uses (docs/ai-services.md,
/// "Run and test locally"), which is OctoSense's own by the caller's choice.
pub(crate) fn persons_octos_home() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").filter(|v| !v.is_empty())?;
    Some(PathBuf::from(home).join("octos-home").join(".octos"))
}

/// The profile keys the migration carries: the provider and model settings
/// (`llm`) and the key references beside them (`env_vars`: keychain markers
/// or keys the person saved there).
const MIGRATED_CONFIG_KEYS: &[&str] = &["llm", "env_vars"];

/// Copy the provider and model settings of `from`'s `_main` profile into a
/// new `_main` profile under `to`, once: only when `to` has no profile and
/// `from` has one. Never writes, moves or deletes anything under `from`; no
/// tool policy or other setting of the person's octos is carried over.
/// Returns whether it copied.
pub fn migrate_shared_profile(from: &Path, to: &Path) -> Result<bool, String> {
    let target = profile_path(to);
    if target.exists() || from == to {
        return Ok(false);
    }
    let Ok(bytes) = std::fs::read(profile_path(from)) else { return Ok(false) };
    let source: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("{} is not JSON: {e}", profile_path(from).display()))?;
    let mut config = serde_json::Map::new();
    for key in MIGRATED_CONFIG_KEYS {
        if let Some(value) = source["config"].get(*key) {
            config.insert((*key).to_owned(), value.clone());
        }
    }
    if config.is_empty() {
        return Ok(false);
    }
    let now = source["updated_at"].clone();
    let profile = serde_json::json!({
        "id": "_main", "name": source["name"].as_str().unwrap_or("Main"), "enabled": true,
        "created_at": source["created_at"].clone(), "updated_at": now,
        "config": config,
    });
    let body = serde_json::to_vec_pretty(&profile).map_err(|e| e.to_string())?;
    let dir = target.parent().unwrap_or(to);
    crate::network::write_private(dir, "_main.json", &body)
        .map_err(|e| format!("could not write {}: {e}", target.display()))?;
    Ok(true)
}

/// The kernel's HOME for a core dir: its parent when it is the conventional
/// `<home>/.octos`, else the core dir itself (then the kernel is also told
/// `--data-dir <core_dir>`, see `launch`).
pub fn kernel_home(core_dir: &Path) -> PathBuf {
    if is_conventional(core_dir) {
        core_dir.parent().map(Path::to_path_buf).unwrap_or_else(|| core_dir.to_path_buf())
    } else {
        core_dir.to_path_buf()
    }
}

/// `<home>/.octos`: octos finds it from `HOME` alone.
pub(crate) fn is_conventional(core_dir: &Path) -> bool {
    core_dir.file_name().is_some_and(|n| n == ".octos") && core_dir.parent().is_some()
}

/// `<core_dir>/profiles/_main.json`.
pub fn profile_path(core_dir: &Path) -> PathBuf {
    octosense_llm_config::profile::profile_path(core_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test touches the process environment; the others pass `explicit`
    // or read it only through the env-free branches.
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn explicit_wins_over_everything() {
        let _g = ENV.lock().unwrap();
        let got = resolve_core_dir(Some(Path::new("/x/core")), Some(Path::new("/data")));
        assert_eq!(got, Some(PathBuf::from("/x/core")));
        assert_eq!(shared_profile_source(Some(Path::new("/x/core")), Some(Path::new("/data"))), None);
    }

    #[test]
    fn env_then_app_data_dir_then_home() {
        let _g = ENV.lock().unwrap();
        let saved_dir = std::env::var_os("OCTOS_APP_CORE_DIR");
        let saved_home = std::env::var_os("HOME");

        std::env::set_var("OCTOS_APP_CORE_DIR", "/env/core");
        assert_eq!(resolve_core_dir(None, Some(Path::new("/data"))), Some(PathBuf::from("/env/core")));
        assert_eq!(shared_profile_source(None, Some(Path::new("/data"))), None, "the caller's choice");

        std::env::set_var("OCTOS_APP_CORE_DIR", "");
        std::env::set_var("HOME", "/home/me");
        // Every platform with a data dir: OctoSense's own octos home.
        assert_eq!(
            resolve_core_dir(None, Some(Path::new("/home/me/.octosense"))),
            Some(PathBuf::from("/home/me/.octosense/octos-home/.octos"))
        );
        if !cfg!(any(target_os = "android", target_env = "ohos", target_os = "ios")) {
            assert_eq!(
                shared_profile_source(None, Some(Path::new("/home/me/.octosense"))),
                Some(PathBuf::from("/home/me/octos-home/.octos")),
                "a desktop inherits from the shared home it used to use"
            );
        }
        // `$OCTOS_APP_CORE_DIR` never makes a dir the person's own.
        std::env::set_var("OCTOS_APP_CORE_DIR", "/env/core");
        assert_eq!(persons_octos_home(), Some(PathBuf::from("/home/me/octos-home/.octos")));
        std::env::set_var("OCTOS_APP_CORE_DIR", "");
        // Nothing named: the llm service's default.
        assert_eq!(resolve_core_dir(None, None), octosense_llm_config::profile::default_core_dir());
        assert_eq!(shared_profile_source(None, None), None);

        match saved_dir {
            Some(v) => std::env::set_var("OCTOS_APP_CORE_DIR", v),
            None => std::env::remove_var("OCTOS_APP_CORE_DIR"),
        }
        match saved_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("octos-dirs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() { stack.push(path) } else { out.push((path.clone(), std::fs::read(&path).unwrap())) }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn migration_copies_provider_settings_once_and_never_touches_the_source() {
        let root = tmp("migrate");
        let shared = root.join("octos-home/.octos");
        let own = root.join("octosense/octos-home/.octos");
        std::fs::create_dir_all(shared.join("profiles")).unwrap();
        std::fs::write(profile_path(&shared), serde_json::json!({
            "id": "_main", "name": "Mine", "enabled": true,
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z",
            "config": {"llm": {"primary": {"family_id": "deepseek", "model_id": "m"}},
                       "env_vars": {"DEEPSEEK_API_KEY": "keychain:DEEPSEEK_API_KEY::_main"},
                       "tool_policy": {"allow": ["shell"]}, "channels": {"x": 1}}
        }).to_string()).unwrap();
        std::fs::write(shared.join("config.json"), "{}").unwrap();
        let before = snapshot(&shared);

        assert_eq!(migrate_shared_profile(&shared, &own), Ok(true));
        let copied: serde_json::Value = serde_json::from_slice(&std::fs::read(profile_path(&own)).unwrap()).unwrap();
        assert_eq!(copied["id"], "_main");
        assert_eq!(copied["config"]["llm"]["primary"]["family_id"], "deepseek");
        assert_eq!(copied["config"]["env_vars"]["DEEPSEEK_API_KEY"], "keychain:DEEPSEEK_API_KEY::_main");
        assert!(copied["config"].get("tool_policy").is_none(), "the person's policy is not ours");
        assert!(copied["config"].get("channels").is_none(), "only provider settings");
        assert!(!own.join("config.json").exists());

        // Once: an existing OctoSense profile is never overwritten.
        std::fs::write(profile_path(&own), r#"{"config":{"llm":{"primary":{"family_id":"zai"}}}}"#).unwrap();
        assert_eq!(migrate_shared_profile(&shared, &own), Ok(false));
        assert!(std::fs::read_to_string(profile_path(&own)).unwrap().contains("zai"));
        assert_eq!(snapshot(&shared), before, "the person's octos home is unchanged");
        let _ = std::fs::remove_dir_all(root);
    }

    /// docs/ai-services.md, "Run and test locally": a kernel whose core dir
    /// is `$OCTOS_APP_CORE_DIR` gets OctoSense's tool policy (it is not the
    /// person's own octos home), while `$HOME/octos-home/.octos` still never
    /// does.
    #[test]
    fn an_explicit_core_dir_from_the_environment_is_octosenses_own() {
        let _g = ENV.lock().unwrap();
        let saved_dir = std::env::var_os("OCTOS_APP_CORE_DIR");
        let saved_home = std::env::var_os("HOME");
        let root = tmp("env-core");
        let core = root.join("core");
        std::fs::create_dir_all(core.join("profiles")).unwrap();
        std::fs::write(profile_path(&core), r#"{"id":"_main","config":{"llm":{}}}"#).unwrap();
        std::env::set_var("HOME", root.join("home"));
        std::env::set_var("OCTOS_APP_CORE_DIR", &core);
        assert_eq!(resolve_core_dir(None, None), Some(core.clone()));
        assert_eq!(crate::system_tools::enforce(&core), crate::system_tools::Enforced::Written);
        // The person's own octos home stays refused.
        let own = root.join("home/octos-home/.octos");
        std::fs::create_dir_all(own.join("profiles")).unwrap();
        std::fs::write(profile_path(&own), r#"{"config":{"llm":{}}}"#).unwrap();
        assert!(matches!(crate::system_tools::enforce(&own), crate::system_tools::Enforced::Refused(_)));
        match saved_dir {
            Some(v) => std::env::set_var("OCTOS_APP_CORE_DIR", v),
            None => std::env::remove_var("OCTOS_APP_CORE_DIR"),
        }
        match saved_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn migration_without_a_shared_profile_does_nothing() {
        let root = tmp("migrate-none");
        assert_eq!(migrate_shared_profile(&root.join("a"), &root.join("b")), Ok(false));
        assert!(!root.join("b").exists());
    }

    #[test]
    fn kernel_home_is_the_parent_of_a_dot_octos_core_dir() {
        assert_eq!(kernel_home(Path::new("/d/octos-home/.octos")), PathBuf::from("/d/octos-home"));
        assert_eq!(kernel_home(Path::new("/srv/octos-data")), PathBuf::from("/srv/octos-data"));
        assert!(is_conventional(Path::new("/d/octos-home/.octos")));
        assert!(!is_conventional(Path::new("/srv/octos-data")));
    }

    #[test]
    fn profile_lives_under_profiles() {
        assert_eq!(profile_path(Path::new("/c")), PathBuf::from("/c/profiles/_main.json"));
    }
}
