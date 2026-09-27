//! Where the kernel's state lives.
//!
//! The **core dir** is octos's data dir: `<core_dir>/profiles/_main.json` is
//! the profile the AI providers app's `llm` service writes and the kernel
//! reads. On a phone it is `<home>/.octos` under the kernel's own HOME,
//! `<app data dir>/octos-home` (the app-private dir AppCard has always used);
//! the kernel is spawned with that HOME (Android) or served from it
//! (OpenHarmony, `octos_cli::embedded::serve_io(home, ..)`).

use std::path::{Path, PathBuf};

/// Resolve the core dir, first match wins:
///
/// 1. `explicit` (the shell's [`crate::Options::core_dir`]);
/// 2. `$OCTOS_APP_CORE_DIR` (non-empty);
/// 3. on Android and OpenHarmony, `<app_data_dir>/octos-home/.octos` when the
///    shell named its data dir (it registers services before the hosted app
///    has pointed `$HOME` there);
/// 4. `octosense_llm_config::profile::default_core_dir()`:
///    `$HOME/octos-home/.octos`.
pub fn resolve_core_dir(explicit: Option<&Path>, app_data_dir: Option<&Path>) -> Option<PathBuf> {
    resolve_core_dir_for(explicit, app_data_dir, cfg!(any(target_os = "android", target_env = "ohos")))
}

pub(crate) fn resolve_core_dir_for(explicit: Option<&Path>, app_data_dir: Option<&Path>, phone: bool) -> Option<PathBuf> {
    if let Some(dir) = explicit.filter(|d| !d.as_os_str().is_empty()) {
        return Some(dir.to_path_buf());
    }
    if let Some(dir) = std::env::var_os("OCTOS_APP_CORE_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    if phone {
        if let Some(data) = app_data_dir.filter(|d| !d.as_os_str().is_empty()) {
            return Some(data.join("octos-home").join(".octos"));
        }
    }
    octosense_llm_config::profile::default_core_dir()
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
        let got = resolve_core_dir_for(Some(Path::new("/x/core")), Some(Path::new("/data")), true);
        assert_eq!(got, Some(PathBuf::from("/x/core")));
    }

    #[test]
    fn env_then_phone_data_dir_then_home() {
        let _g = ENV.lock().unwrap();
        let saved_dir = std::env::var_os("OCTOS_APP_CORE_DIR");
        let saved_home = std::env::var_os("HOME");

        std::env::set_var("OCTOS_APP_CORE_DIR", "/env/core");
        assert_eq!(resolve_core_dir_for(None, Some(Path::new("/data")), true), Some(PathBuf::from("/env/core")));

        std::env::set_var("OCTOS_APP_CORE_DIR", "");
        std::env::set_var("HOME", "/home/me");
        // A phone with the shell's data dir: the app-private octos home.
        assert_eq!(
            resolve_core_dir_for(None, Some(Path::new("/data/user/0/app/files")), true),
            Some(PathBuf::from("/data/user/0/app/files/octos-home/.octos"))
        );
        // A desktop ignores the data dir: $HOME/octos-home/.octos.
        assert_eq!(
            resolve_core_dir_for(None, Some(Path::new("/data")), false),
            Some(PathBuf::from("/home/me/octos-home/.octos"))
        );
        // Same as the llm service's default when nothing is named.
        assert_eq!(resolve_core_dir_for(None, None, true), octosense_llm_config::profile::default_core_dir());

        match saved_dir {
            Some(v) => std::env::set_var("OCTOS_APP_CORE_DIR", v),
            None => std::env::remove_var("OCTOS_APP_CORE_DIR"),
        }
        match saved_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
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
