use std::path::{Path, PathBuf};

fn resolve_home(custom: Option<&Path>, user: Option<&Path>) -> PathBuf {
    custom.map(Path::to_path_buf).unwrap_or_else(|| {
        let user = user.unwrap_or(Path::new("."));
        let current = user.join(".octosense");
        let legacy = user.join(".makeos");
        // Keep existing settings and model links usable without moving user data.
        if !current.exists() && legacy.is_dir() {
            legacy
        } else {
            current
        }
    })
}

pub fn home() -> PathBuf {
    let custom = std::env::var_os("OCTOSENSE_HOME")
        .or_else(|| std::env::var_os("MAKEOS_HOME"))
        .map(PathBuf::from);
    // Where the platform gives the app a data directory of its own (Android's
    // files directory, reported before startup), that is the home: `HOME`
    // there is not writable, and the shell's settings and every module's
    // storage would fail with a read-only file system.
    let user = makepad_widgets::makepad_platform::home::platform_data_dir().or_else(|| {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
    });
    let path = resolve_home(custom.as_deref(), user.as_deref());
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    }
}

/// The data folder a linked Rinx should use, if not its own default: under
/// an explicitly chosen OctoSense home (`OCTOSENSE_HOME`, a test or developer
/// profile), `<home>/apps/rinx/data`, so such a run never opens the person's
/// real Matrix sessions. The default home keeps Rinx's standard folder until
/// its data moves under ADR 0004 §11's layout (hagency-org/Rinx#37). An
/// explicit `RINX_DATA_DIR` always wins.
/// A folder of the host's own inside the OctoSense home (`rel`, such as
/// `logs/clients` or `sandbox`), created owner-only. The OctoSense home is
/// closed to every sandboxed app (ADR 0004 §3, G6), so what the shell keeps
/// here (a process app's log, its generated sandbox profile) is neither
/// readable nor writable by the apps, unlike the shared temp dir.
pub fn private_dir(rel: &str) -> std::io::Result<PathBuf> {
    let dir = home().join(rel);
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut at = dir.as_path();
        // Owner-only from the home down to it.
        for _ in Path::new(rel).components() {
            std::fs::set_permissions(at, std::fs::Permissions::from_mode(0o700))?;
            match at.parent() {
                Some(parent) => at = parent,
                None => break,
            }
        }
    }
    Ok(dir)
}

fn linked_rinx_data_dir(custom_home: Option<&Path>, explicit: bool) -> Option<PathBuf> {
    if explicit {
        return None;
    }
    custom_home.map(|home| home.join("apps").join("rinx").join("data"))
}

/// Point linked apps that keep their own data at the chosen OctoSense home.
/// Runs once at startup, before any module is created (Rinx reads
/// `RINX_DATA_DIR` once, on first use).
pub fn scope_linked_app_data() {
    let custom = std::env::var_os("OCTOSENSE_HOME")
        .or_else(|| std::env::var_os("MAKEOS_HOME"))
        .map(PathBuf::from)
        .map(|p| if p.is_absolute() { p } else { std::env::current_dir().unwrap_or_default().join(p) });
    let explicit = std::env::var_os("RINX_DATA_DIR").is_some() || std::env::var_os("ROBRIX_DATA_DIR").is_some();
    if let Some(dir) = linked_rinx_data_dir(custom.as_deref(), explicit) {
        if std::fs::create_dir_all(&dir).is_ok() {
            makepad_widgets::log!("rinx: data folder {} (under OCTOSENSE_HOME)", dir.display());
            std::env::set_var("RINX_DATA_DIR", &dir);
        }
    }
}

/// Whether this is a packaged (installed) build: `desktop/scripts/package.py`
/// builds with `MAKEPAD_PACKAGE_DIR` set, and Makepad then reads every
/// `crate_resource` from the package instead of the source tree.
pub const fn packaged() -> bool {
    option_env!("MAKEPAD_PACKAGE_DIR").is_some()
}

static PACKAGE_DIR: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// The running package's source directory (desktop/ or phone/), set by its
/// `octosense_main!` before the app starts (not in a packaged build). The
/// shell's own `CARGO_MANIFEST_DIR` is crates/shell, which carries no catalog.
pub fn set_package_dir(dir: &'static str) {
    if packaged() {
        return;
    }
    let _ = PACKAGE_DIR.set(dir);
}

/// The OctoSense checkout this shell runs from: its developer catalog
/// (`config/apps.json`), whose programs it builds and runs, and the Makepad
/// sources. A source tree belongs to OctoSense only when it has our
/// provenance marker.
///
/// A packaged build has none, wherever it runs: not where it was built (that
/// path belongs to the build machine), not the working directory and not its
/// executable's ancestors, so starting the installed app inside someone
/// else's checkout can never make it build and run that checkout's code.
/// `--apps <catalog>` still names a catalog explicitly.
pub fn project_root() -> Option<PathBuf> {
    project_root_for(packaged())
}

fn project_root_for(packaged: bool) -> Option<PathBuf> {
    if packaged {
        return None;
    }
    let starts = [
        PACKAGE_DIR.get().map(PathBuf::from),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf)),
        std::env::current_dir().ok(),
        // Tests and tools without a package: the desktop package beside us.
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../desktop")),
    ];
    for start in starts.into_iter().flatten() {
        for dir in start.ancestors().take(6) {
            if dir.join("upstream/makepad.json").is_file() && dir.join("Cargo.toml").is_file() {
                return Some(dir.to_path_buf());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packaged_build_has_no_checkout_even_inside_one() {
        // The tests run inside the checkout (cwd and CARGO_MANIFEST_DIR).
        assert!(project_root_for(false).is_some(), "a source build finds its checkout");
        assert_eq!(project_root_for(true), None, "a packaged build never does");
        assert!(!packaged(), "tests are a source build");
    }

    #[test]
    fn a_linked_rinx_keeps_its_data_under_a_chosen_home_only() {
        let home = Path::new("/tmp/os-home");
        assert_eq!(linked_rinx_data_dir(Some(home), false), Some(home.join("apps/rinx/data")));
        assert_eq!(linked_rinx_data_dir(None, false), None, "the default home keeps Rinx's own folder");
        assert_eq!(linked_rinx_data_dir(Some(home), true), None, "an explicit RINX_DATA_DIR wins");
    }

    #[test]
    fn octosense_state_is_separate_and_can_be_relocated() {
        assert_eq!(
            resolve_home(None, Some(Path::new("/users/person"))),
            Path::new("/users/person/.octosense")
        );
        assert_eq!(
            resolve_home(
                Some(Path::new("/tmp/isolated")),
                Some(Path::new("/users/person"))
            ),
            Path::new("/tmp/isolated")
        );
    }
    #[test]
    fn existing_state_and_weights_remain_discoverable_after_rename() {
        let root = std::env::temp_dir().join(format!(
            "octosense-home-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let legacy = root.join(".makeos");
        let current = root.join(".octosense");
        std::fs::create_dir_all(legacy.join("weights")).unwrap();
        std::fs::write(legacy.join("weights/model.gguf"), b"fixture").unwrap();
        assert_eq!(resolve_home(None, Some(&root)), legacy);
        assert_eq!(
            std::fs::read(resolve_home(None, Some(&root)).join("weights/model.gguf")).unwrap(),
            b"fixture"
        );
        assert!(!current.exists());
        std::fs::create_dir_all(&current).unwrap();
        assert_eq!(resolve_home(None, Some(&root)), current);
        assert_eq!(
            resolve_home(Some(Path::new("/explicit")), Some(&root)),
            Path::new("/explicit")
        );
    }
}
