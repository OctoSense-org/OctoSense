//! What the host keeps to resume an app's peer (ADR 0004 §11): its host
//! token (UPCR-2026-034, the kernel's credential for the peer it created)
//! and the workspace it was created with (octos resumes a peer only under
//! that one), in ONE record per memory namespace, written at once:
//!
//! ```text
//! <state dir>/                      mode 0700
//!     <namespace, '/' → '_'>.peer   mode 0600: {"token", "cwd", "namespace", "name"}
//! ```
//!
//! Brokers before this kept `<ns>.token` and `<ns>.cwd` side by side (the
//! second written separately, and sometimes not at all); [`load`] still
//! reads them, and the next [`save`] replaces them with the record.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PeerRecord {
    pub token: String,
    /// The workspace the peer was created with; `None`: not known (a peer
    /// from before the record whose `.cwd` was never written).
    pub cwd: Option<String>,
    /// The kernel memory namespace the peer was made under, when it is not
    /// the one the record is kept under (a peer made before accounts were
    /// normalized); `None` in records from before this field: the key.
    pub namespace: Option<String>,
    /// The name a resume finds the peer by; `None` in records from before
    /// this field: the app label and 32 bits of the namespace's tag.
    pub name: Option<String>,
    /// Read from the legacy `.token`/`.cwd` files: save it to migrate.
    pub legacy: bool,
}

fn stem(namespace: &str) -> String {
    namespace.replace('/', "_")
}

fn record_path(dir: &Path, namespace: &str) -> PathBuf {
    dir.join(format!("{}.peer", stem(namespace)))
}

fn legacy_paths(dir: &Path, namespace: &str) -> (PathBuf, PathBuf) {
    let stem = stem(namespace);
    (dir.join(format!("{stem}.token")), dir.join(format!("{stem}.cwd")))
}

fn non_empty(text: Option<String>) -> Option<String> {
    text.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

/// The record of `namespace` under `dir`: the record file, else the legacy
/// files.
pub fn load(dir: &Path, namespace: &str) -> Option<PeerRecord> {
    if let Ok(text) = std::fs::read_to_string(record_path(dir, namespace)) {
        let value: Value = serde_json::from_str(&text).ok()?;
        let token = non_empty(value["token"].as_str().map(str::to_owned))?;
        let text = |field: &str| non_empty(value[field].as_str().map(str::to_owned));
        return Some(PeerRecord { token, cwd: text("cwd"), namespace: text("namespace"), name: text("name"), legacy: false });
    }
    let (token, cwd) = legacy_paths(dir, namespace);
    let token = non_empty(std::fs::read_to_string(token).ok())?;
    let cwd = non_empty(std::fs::read_to_string(cwd).ok());
    Some(PeerRecord { token, cwd, namespace: None, name: None, legacy: true })
}

/// Write the record atomically (a temporary file, owner-only, renamed over
/// the old one) in an owner-only directory, then drop the legacy files.
pub fn save(dir: &Path, namespace: &str, record: &PeerRecord) -> Result<(), String> {
    private_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = record_path(dir, namespace);
    let tmp = path.with_extension("peer.tmp");
    let body = json!({ "token": record.token, "cwd": record.cwd, "namespace": record.namespace, "name": record.name }).to_string();
    let write = || -> std::io::Result<()> {
        let _ = std::fs::remove_file(&tmp);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&tmp)?;
        std::io::Write::write_all(&mut file, body.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, &path)
    };
    write().map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })?;
    let (token, cwd) = legacy_paths(dir, namespace);
    let _ = std::fs::remove_file(token);
    let _ = std::fs::remove_file(cwd);
    Ok(())
}

/// `dir` (and its parents) created, and `dir` itself owner-only.
pub fn private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Forget `namespace`'s record and legacy files (moved under another key).
pub fn remove(dir: &Path, namespace: &str) {
    let (token, cwd) = legacy_paths(dir, namespace);
    for path in [record_path(dir, namespace), token, cwd] {
        let _ = std::fs::remove_file(path);
    }
}

/// The namespaces with a record (or legacy token) under `dir` whose
/// namespace starts with `prefix` (for example `app/<app id>/acct-`, every
/// account of one app).
pub fn namespaces_under(dir: &Path, prefix: &str) -> Vec<String> {
    let stem_prefix = stem(prefix);
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let rest = name.strip_prefix(&stem_prefix)?;
            let rest = rest.strip_suffix(".peer").or_else(|| rest.strip_suffix(".token"))?;
            (!rest.is_empty() && !rest.contains('/')).then(|| format!("{prefix}{rest}"))
        })
        .collect();
    found.sort();
    found.dedup();
    found
}
