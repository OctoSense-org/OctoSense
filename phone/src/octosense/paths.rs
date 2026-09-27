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

/// A source tree belongs to OctoSense only when it has our provenance marker.
pub fn project_root() -> Option<PathBuf> {
    let starts = [
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf)),
        std::env::current_dir().ok(),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
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
