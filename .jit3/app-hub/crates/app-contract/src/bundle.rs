//! What exactly gets hashed.
//!
//! A bundle is a directory, so "the bundle bytes" needs a definition both the
//! signer and the host compute the same way, on any filesystem, in any order.
//! This is that definition: every file under the root except the manifest
//! itself, sorted by its path, each contributing its path, its length and its
//! bytes. Directory order, timestamps, permissions and the manifest's own
//! contents do not affect it.
//!
//! The manifest is excluded because it carries the digest; a symlink is
//! refused rather than followed, because what it points at is not in the
//! bundle and would not be signed.
use std::path::{Component, Path, PathBuf};

/// The file inside a bundle that carries its manifest.
pub const MANIFEST_FILE: &str = "manifest.json";

/// A bundle-relative path as a name: `/` between components, on every platform.
///
/// A bundle holds names, not host paths — `assets/icon.svg` means the same
/// thing to a manifest, to the gate and to the digest wherever they run. A
/// `Path` renders the host's own separator (`\` on Windows), so every place
/// that turns a walked path into a string, or into bytes to hash, goes
/// through here instead. `None` when a component is not a plain UTF-8 name,
/// so a caller can refuse rather than hash a lossy stand-in.
pub fn portable_path(relative: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(name) => parts.push(name.to_str()?),
            _ => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

/// Hex blake3 over the bundle's files, in the canonical order.
pub fn digest_dir(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect(root, root, &mut files)?;
    // The bytes come from the portable name: a host's separator is not part
    // of the bundle. The order stays path-component order, the order every
    // published digest was made in, and it is the same on every platform.
    // Sorting the names as text would put `kit.json` before `kit/kit.json`
    // (`.` sorts before `/`) and change such a bundle's digest everywhere.
    let mut named = Vec::with_capacity(files.len());
    for relative in files {
        let name = portable_path(&relative)
            .ok_or_else(|| format!("{}: a bundle path must be a plain UTF-8 name", relative.display()))?;
        named.push((name, relative));
    }
    named.sort_by(|a, b| a.1.cmp(&b.1));
    let mut hasher = blake3::Hasher::new();
    for (name, relative) in &named {
        let bytes = std::fs::read(root.join(relative)).map_err(|e| format!("{}: {e}", relative.display()))?;
        // Path, then length, then content: without the length a file ending
        // where the next path begins could be shuffled without changing the
        // digest.
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err(format!("{}: a bundle may not hold a symlink", path.display()));
        }
        if kind.is_dir() {
            collect(root, &path, out)?;
            continue;
        }
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?.to_path_buf();
        if relative == Path::new(MANIFEST_FILE) {
            continue;
        }
        out.push(relative);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("app-contract-bundle-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("kit")).unwrap();
        fs::write(dir.join("page.card"), b"card source").unwrap();
        fs::write(dir.join("kit/kit.json"), b"{}").unwrap();
        dir
    }

    #[test]
    fn the_manifest_itself_is_not_part_of_the_digest() {
        let dir = scratch("manifest-excluded");
        let before = digest_dir(&dir).unwrap();
        fs::write(dir.join(MANIFEST_FILE), b"{\"schema\":1}").unwrap();
        assert_eq!(before, digest_dir(&dir).unwrap());
    }

    #[test]
    fn changing_any_content_changes_the_digest() {
        let dir = scratch("content");
        let before = digest_dir(&dir).unwrap();
        fs::write(dir.join("kit/kit.json"), b"{ }").unwrap();
        assert_ne!(before, digest_dir(&dir).unwrap());
    }

    #[test]
    fn moving_content_between_files_changes_the_digest() {
        let dir = scratch("shuffle");
        fs::write(dir.join("a.txt"), b"onetwo").unwrap();
        fs::write(dir.join("b.txt"), b"").unwrap();
        let before = digest_dir(&dir).unwrap();
        fs::write(dir.join("a.txt"), b"one").unwrap();
        fs::write(dir.join("b.txt"), b"two").unwrap();
        assert_ne!(before, digest_dir(&dir).unwrap());
    }

    #[test]
    fn a_symlink_is_refused_rather_than_followed() {
        let dir = scratch("symlink");
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/hosts", dir.join("link")).unwrap();
        #[cfg(unix)]
        assert!(digest_dir(&dir).unwrap_err().contains("symlink"));
    }

    #[test]
    fn a_nested_path_is_named_with_slashes_wherever_it_is_read() {
        let nested = Path::new("kit").join("native").join("light").join("kit.json");
        assert_eq!(portable_path(&nested).as_deref(), Some("kit/native/light/kit.json"));
        assert_eq!(portable_path(&nested).unwrap().matches('\\').count(), 0);
        assert_eq!(portable_path(Path::new("page.card")).as_deref(), Some("page.card"));
    }

    #[test]
    fn the_digest_is_the_same_wherever_the_host_separates_paths() {
        let dir = scratch("portable-digest");
        // The definition, spelled out: each file's portable name, its length
        // and its bytes, in path-component order. A Windows host and a Linux
        // host must land on these bytes for the same bundle.
        let mut files = vec![
            ("kit/kit.json", fs::read(dir.join("kit").join("kit.json")).unwrap()),
            ("page.card", fs::read(dir.join("page.card")).unwrap()),
        ];
        files.sort_by(|a, b| a.0.split('/').cmp(b.0.split('/')));
        let mut hasher = blake3::Hasher::new();
        for (name, bytes) in &files {
            hasher.update(name.as_bytes());
            hasher.update(&[0]);
            hasher.update(&(bytes.len() as u64).to_le_bytes());
            hasher.update(bytes);
        }
        assert_eq!(digest_dir(&dir).unwrap(), hasher.finalize().to_hex().to_string());
    }

    #[test]
    fn a_folder_sorts_before_a_file_that_extends_its_name() {
        // `kit/kit.json` before `kit.json`, although `.` sorts before `/` as
        // text. Pinned to the digest every 1.x release so far computes for
        // this bundle, so a change of order cannot pass unseen.
        let dir = scratch("component-order");
        fs::write(dir.join("kit.json"), b"{}").unwrap();
        assert_eq!(digest_dir(&dir).unwrap(), "c84d3407bd95ee139b199b629150b7379113b964ce776a2342a9d1efca5a002b");
    }
}
