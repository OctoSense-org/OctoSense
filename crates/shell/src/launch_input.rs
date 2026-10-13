//! Launch input: settings that arrive with a launch.
//!
//! **Where it comes from.** On a desktop these variables are the local
//! user's own environment, the same trust as a command-line flag. On a
//! phone they are not: Home's main activity is exported (the app drawer
//! entry, the Home app, a share target and a deep-link target), so any
//! installed app can start it with extras, and Makepad's Android glue copies
//! the extras `makepad.<NAME>` into the environment as `MAKEPAD_<NAME>`
//! (`APP_CONFIG`, `PROVISION_CONFIG`, `SEED_*` and others). OpenHarmony passes
//! the Want's `makepad.*` parameters on the same way.
//!
//! **The rule.** On Android and OpenHarmony (`cfg(native_mobile)`) OctoSense
//! reads the app config only in a development build (`cfg(dev_mode)`: a debug
//! build, or `--features dev-mode`; never a store build); a release build
//! there ignores it and says so once in the log, without the value. A desktop
//! reads it in every build, because its environment is the local user's.
//! `--test-action` flags on a desktop command line are arguments only the
//! local user can pass, and work in every build (`App::run_test_actions` in
//! `lib.rs`).
//!
//! A phone build that needs these hooks is a development build under a
//! separate package name (`rom/scripts/build-home.py --dev-mode`).

use crate::dev_mode::BuildKind;
use makepad_strict_json::Value;

/// The app config: one JSON object,
/// `{"test_actions": ["<name>", …], "mail_demo": true,
/// "module_open": {"<module id>": {…}}, "mail_endpoint": "…"}`.
pub const APP_CONFIG: &str = "MAKEPAD_APP_CONFIG";

/// Whether this build reads the app config. A desktop always does; Android
/// and OpenHarmony, where another app can set it, only in a development
/// build.
pub fn honoured(build: BuildKind) -> bool {
    honoured_for(cfg!(native_mobile), build)
}

/// [`honoured`] split out from the target for testing: `native_mobile` is
/// Android and OpenHarmony (and iOS, which has no such channel but is a
/// phone all the same).
fn honoured_for(native_mobile: bool, build: BuildKind) -> bool {
    !native_mobile || build == BuildKind::Development
}

/// The app config, parsed, when this build reads launch input. A phone
/// release build answers `None` whatever the variable holds.
pub fn app_config() -> Option<Value> {
    let build = BuildKind::current();
    if !honoured(build) {
        if std::env::var_os(APP_CONFIG).is_some() {
            note_ignored();
        }
        return None;
    }
    std::env::var(APP_CONFIG).ok().and_then(|text| makepad_strict_json::parse(text.as_bytes()).ok())
}

/// [`app_config`] for a given target and build, over the given text.
fn app_config_for(native_mobile: bool, build: BuildKind, text: &str) -> Option<Value> {
    if !honoured_for(native_mobile, build) {
        return None;
    }
    makepad_strict_json::parse(text.as_bytes()).ok()
}

/// The app config's `test_actions`, in order, as `--test-action` names; none
/// in a phone release build.
pub fn test_actions() -> Vec<String> {
    app_config().map(|config| test_actions_in(&config)).unwrap_or_default()
}

/// [`test_actions`] for a given target and build, over the given text.
fn test_actions_for(native_mobile: bool, build: BuildKind, text: &str) -> Vec<String> {
    app_config_for(native_mobile, build, text).map(|config| test_actions_in(&config)).unwrap_or_default()
}

fn test_actions_in(config: &Value) -> Vec<String> {
    config
        .get("test_actions")
        .and_then(|actions| actions.as_arr())
        .map(|actions| actions.iter().filter_map(|action| action.as_str()).map(str::to_owned).collect())
        .unwrap_or_default()
}

/// The `true`/`false` value `key` has in the app config, when this build
/// reads it.
pub fn flag(key: &str) -> bool {
    app_config().and_then(|config| config.get(key).and_then(|value| value.as_bool())).unwrap_or(false)
}

fn note_ignored() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        makepad_widgets::log!(
            "launch input: {APP_CONFIG} ignored; a release build on a phone takes no test actions or demo settings from a launch"
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"{"test_actions":["capture:/sdcard/Download/frame.png","launch-mail","system-chat-send:hello"],"mail_demo":true}"#;
    const ACTIONS: [&str; 3] = ["capture:/sdcard/Download/frame.png", "launch-mail", "system-chat-send:hello"];

    #[test]
    fn a_phone_release_build_ignores_test_actions_from_the_launch() {
        for build in [BuildKind::Release, BuildKind::Store] {
            assert!(!honoured_for(true, build), "phone {build:?}");
            assert!(test_actions_for(true, build, CONFIG).is_empty(), "phone {build:?}");
            assert!(app_config_for(true, build, CONFIG).is_none(), "phone {build:?}");
        }
    }

    #[test]
    fn a_desktop_build_reads_them_in_every_build() {
        for build in [BuildKind::Release, BuildKind::Store, BuildKind::Development] {
            assert!(honoured_for(false, build), "desktop {build:?}");
            assert_eq!(test_actions_for(false, build, CONFIG), ACTIONS, "desktop {build:?}");
        }
    }

    #[test]
    fn a_development_build_keeps_them_on_a_phone_too() {
        assert!(honoured_for(true, BuildKind::Development));
        assert_eq!(test_actions_for(true, BuildKind::Development, CONFIG), ACTIONS);
        let config = app_config_for(true, BuildKind::Development, CONFIG).expect("parsed");
        assert_eq!(config.get("mail_demo").and_then(|v| v.as_bool()), Some(true));
    }

    #[test]
    fn malformed_or_missing_actions_give_none() {
        for text in ["", "not json", r#"{"test_actions":"launch-mail"}"#, r#"{"mail_demo":true}"#] {
            assert!(test_actions_for(false, BuildKind::Release, text).is_empty(), "{text:?}");
        }
        // Entries that are not strings are skipped, the rest kept.
        assert_eq!(test_actions_for(false, BuildKind::Release, r#"{"test_actions":[1,"launch-news",null]}"#), ["launch-news"]);
    }

    /// The shell has one reader of the app config: this module. Anything
    /// else reading the variable directly would bypass the build check.
    #[test]
    fn no_other_shell_source_reads_the_app_config_directly() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("launch_input.rs") {
                    let text = std::fs::read_to_string(&path).expect("read source");
                    if text.contains("env::var(\"MAKEPAD_APP_CONFIG\"") || text.contains("env::var_os(\"MAKEPAD_APP_CONFIG\"") {
                        offenders.push(path.strip_prefix(&root).unwrap().display().to_string());
                    }
                }
            }
        }
        assert!(offenders.is_empty(), "read MAKEPAD_APP_CONFIG through launch_input: {offenders:?}");
    }
}
