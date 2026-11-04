//! The bytes an installed app launches from.
//!
//! An installed bundle sits inside its app's storage jail, so the app can
//! write to it. A launch therefore never runs it in place: it copies the
//! bundle, within the gate's limits, to `<app data root>/.running/<nonce>`
//! (`.running` can never be an app id, so it is no app's jail), and the copy
//! is what the store verifies against the catalog and what the app runs.
//! An update replacing the install cannot change a running app, and the copy
//! is removed when the launch is dropped.
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub(crate) const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_FILES: usize = 2048;
const MAX_DEPTH: usize = 32;

pub(crate) struct LaunchSnapshot {
    root: PathBuf,
}

impl LaunchSnapshot {
    pub(crate) fn copy(source: &Path, app_data_root: &Path) -> Result<Self, String> {
        let parent = app_data_root.join(".running");
        fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        if fs::symlink_metadata(&parent).map_err(|e| e.to_string())?.file_type().is_symlink() {
            return Err("the launch directory may not be a symlink".into());
        }
        let mut nonce = [0u8; 16];
        rand_core::TryRngCore::try_fill_bytes(&mut rand_core::OsRng, &mut nonce).map_err(|e| e.to_string())?;
        let root = parent.join(hex::encode(nonce));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        // Owned from here, so a failed copy cleans up after itself.
        let snapshot = Self { root };
        let mut budget = Budget::new();
        copy_bounded(source, &snapshot.root, 0, &mut budget)?;
        Ok(snapshot)
    }

    pub(crate) fn bundle(&self) -> &Path {
        &self.root
    }
}

impl Drop for LaunchSnapshot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Budget {
    remaining: u64,
    files: usize,
}

impl Budget {
    fn new() -> Self {
        Budget { remaining: crate::gate::MAX_BUNDLE_BYTES + MAX_MANIFEST_BYTES, files: 0 }
    }

    /// Account for one file of `len` bytes; `max` is what it may be.
    fn take(&mut self, len: u64, max: u64) -> Result<(), String> {
        self.files += 1;
        if self.files > MAX_FILES {
            return Err("the installed bundle exceeds the file count limit".into());
        }
        if len > max.min(self.remaining) {
            return Err("the installed bundle exceeds the launch size limit".into());
        }
        self.remaining -= len;
        Ok(())
    }
}

fn file_max(depth: usize, name: &std::ffi::OsStr, budget: &Budget) -> u64 {
    if depth == 0 && name == octosense_app_policy::MANIFEST_FILE {
        budget.remaining.min(MAX_MANIFEST_BYTES)
    } else {
        budget.remaining
    }
}

fn copy_bounded(source: &Path, target: &Path, depth: usize, budget: &mut Budget) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("the installed bundle exceeds the directory depth limit".into());
    }
    fs::create_dir_all(target).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let destination = target.join(entry.file_name());
        if kind.is_dir() {
            copy_bounded(&entry.path(), &destination, depth + 1, budget)?;
        } else if kind.is_file() {
            let max = file_max(depth, &entry.file_name(), budget);
            // Read at most one byte past the limit: the length on disk is
            // only a claim, and the file may grow while it is read.
            let mut bytes = Vec::new();
            fs::File::open(entry.path())
                .map_err(|e| e.to_string())?
                .take(max + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            budget.take(bytes.len() as u64, max)?;
            fs::write(destination, bytes).map_err(|e| e.to_string())?;
        } else {
            return Err("the installed bundle may hold only regular files and directories".into());
        }
    }
    Ok(())
}

/// Refuse an installed bundle beyond the launch limits before hashing it.
pub(crate) fn check_bounds(bundle: &Path) -> Result<(), String> {
    fn walk(dir: &Path, depth: usize, budget: &mut Budget) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("the installed bundle exceeds the directory depth limit".into());
        }
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                walk(&entry.path(), depth + 1, budget)?;
            } else if kind.is_file() {
                let max = file_max(depth, &entry.file_name(), budget);
                budget.take(entry.metadata().map_err(|e| e.to_string())?.len(), max)?;
            } else {
                return Err("the installed bundle may hold only regular files and directories".into());
            }
        }
        Ok(())
    }
    walk(bundle, 0, &mut Budget::new())
}

/// The installed manifest's text, within its size limit.
pub(crate) fn read_manifest(bundle: &Path) -> Result<String, String> {
    let mut text = String::new();
    fs::File::open(bundle.join(octosense_app_policy::MANIFEST_FILE))
        .map_err(|e| format!("cannot read the installed manifest: {e}"))?
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("the installed manifest exceeds the size limit".into());
    }
    Ok(text)
}
