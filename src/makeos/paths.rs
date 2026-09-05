use std::path::{Path, PathBuf};

fn resolve_home(custom: Option<&Path>, user: Option<&Path>) -> PathBuf {
    custom
        .map(Path::to_path_buf)
        .unwrap_or_else(|| user.unwrap_or(Path::new(".")).join(".makeos"))
}

pub fn home() -> PathBuf {
    let custom = std::env::var_os("MAKEOS_HOME").map(PathBuf::from);
    let user = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    let path = resolve_home(custom.as_deref(), user.as_deref());
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    }
}

/// A source tree belongs to MakeOS only when it has our provenance marker.
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
    fn makeos_state_is_separate_and_can_be_relocated() {
        assert_eq!(
            resolve_home(None, Some(Path::new("/users/person"))),
            Path::new("/users/person/.makeos")
        );
        assert_eq!(
            resolve_home(
                Some(Path::new("/tmp/isolated")),
                Some(Path::new("/users/person"))
            ),
            Path::new("/tmp/isolated")
        );
    }
}
