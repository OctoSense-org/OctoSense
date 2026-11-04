//! Where the catalog and its artifacts come from.
//!
//! Two sources, one interface. A directory is a mirrored hub, which is what
//! tests and an offline device use. An HTTP origin is the ordinary case.
//! Neither is trusted: whatever comes back is verified against the anchor
//! before it is read, and a bundle is unpacked and hashed before it is
//! installed, so a hostile origin can withhold but not forge.
use octosense_app_hub::Remote;
use std::path::{Path, PathBuf};

/// Selected by the host before fetching; never inferred from remote bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogChannel {
    Legacy,
    GitHub,
}

impl CatalogChannel {
    pub fn from_environment(root: &Path) -> Result<Self, String> {
        Self::select(std::env::var("OCTOSENSE_HUB_CATALOG").ok().as_deref(), root)
    }

    fn select(configured: Option<&str>, root: &Path) -> Result<Self, String> {
        // New hosts select the authenticated GitHub channel. Legacy mirrors
        // require an explicit choice, and a persisted v2 library cannot downgrade.
        let selected = match configured {
            None | Some("") => None,
            Some("legacy") => Some(Self::Legacy),
            Some("github-v2") => Some(Self::GitHub),
            Some(_) => return Err("Unknown OCTOSENSE_HUB_CATALOG channel".into()),
        };
        selected.unwrap_or(Self::GitHub).for_library(root)
    }

    /// Validate an explicit host choice against this library's durable channel.
    /// A legacy cache alone never opts a new host out of GitHub verification.
    pub fn for_library(self, root: &Path) -> Result<Self, String> {
        if self == Self::Legacy && std::fs::symlink_metadata(root.join("catalog-v2.json")).is_ok() {
            return Err("This library already uses the GitHub catalog; downgrade refused".into());
        }
        Ok(self)
    }

    pub fn filename(self) -> &'static str {
        match self { Self::Legacy => "catalog.json", Self::GitHub => "catalog-v2.json" }
    }

    pub fn require_current(self, root: &Path) -> Result<(), String> {
        if Self::from_environment(root)? != self {
            return Err("Catalog channel changed; reopen App Hub before continuing.".into());
        }
        Ok(())
    }

    pub fn configure(self, store: octosense_app_hub::Store) -> octosense_app_hub::Store {
        match self { Self::Legacy => store, Self::GitHub => store.with_github_catalog() }
    }

    pub fn read_cache(self, root: &Path) -> Result<String, String> {
        use std::io::Read;
        let mut bytes = Vec::new();
        let limit = octosense_app_hub::github_catalog::MAX_DOCUMENT_BYTES;
        std::fs::File::open(root.join(self.filename()))
            .map_err(|e| e.to_string())?
            .take(limit as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() > limit { return Err("Catalog cache exceeds byte limit".into()); }
        String::from_utf8(bytes).map_err(|_| "Catalog cache is not UTF-8".into())
    }
}

#[derive(Clone, Debug)]
pub enum Origin {
    /// A mirrored hub: `<dir>/catalog.json` and `<dir>/artifacts/…`.
    Directory(PathBuf),
    /// `<base>/catalog.json` and `<base>/artifacts/….pack.json` over HTTP.
    Http(String),
}

impl Origin {
    /// `OCTOSENSE_HUB` when set (a path, or a URL when it starts with http),
    /// else the built-in hub.
    pub fn from_env() -> Option<Origin> {
        let value = std::env::var("OCTOSENSE_HUB")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| crate::DEFAULT_HUB.to_string());
        Some(Origin::parse(&value))
    }

    /// A URL when it starts with http, else a mirror directory.
    pub fn parse(value: &str) -> Origin {
        if value.starts_with("http") { Origin::Http(value.to_string()) } else { Origin::Directory(PathBuf::from(value)) }
    }

    pub fn describe(&self) -> String {
        match self {
            Origin::Directory(path) => path.display().to_string(),
            Origin::Http(base) => base.clone(),
        }
    }

    /// Legacy catalog accessor retained for compatibility. Host runners use
    /// `catalog_for` with their selected channel; verification stays in the caller.
    pub fn catalog(&self) -> Result<String, String> {
        self.catalog_for(CatalogChannel::Legacy)
    }

    pub fn catalog_for(&self, channel: CatalogChannel) -> Result<String, String> {
        match self {
            Origin::Directory(dir) => channel.read_cache(dir).map_err(|e| format!("catalog: {e}")),
            Origin::Http(base) => match channel {
                CatalogChannel::Legacy => Remote::new(base).catalog(),
                CatalogChannel::GitHub => Remote::new(base).github_catalog(),
            },
        }
    }

    /// A shared component's file, `artifacts/<id>-<version>.wasm` (App Hub
    /// ADR 0003), unverified: the store checks its size and digest against
    /// the verified catalog before it keeps it.
    pub fn component(&self, artifact: &str) -> Result<Vec<u8>, String> {
        let max = octosense_app_hub::components::MAX_COMPONENT_BYTES;
        match self {
            Origin::Directory(dir) => octosense_app_hub::admission::read_bounded(&dir.join(artifact), max)
                .map_err(|e| format!("{artifact} is not in this hub mirror: {e}")),
            Origin::Http(base) => Remote::new(base).component(artifact, max),
        }
    }

    /// Stage an artifact for install: put the bundle somewhere the store can
    /// hash it before it goes anywhere near an app jail.
    pub fn stage(&self, artifact: &str, into: &Path) -> Result<PathBuf, String> {
        let staged = into.join("staged");
        if staged.exists() {
            std::fs::remove_dir_all(&staged).map_err(|e| e.to_string())?;
        }
        match self {
            Origin::Directory(dir) => {
                let from = dir.join(artifact);
                if !from.is_dir() {
                    return Err(format!("{} is not in this hub mirror", from.display()));
                }
                copy_tree(&from, &staged)?;
            }
            Origin::Http(base) => {
                let pack = Remote::new(base).pack(artifact)?;
                octosense_app_hub::unpack(&pack, &staged)?;
            }
        }
        Ok(staged)
    }
}

#[cfg(test)]
mod channel_tests {
    use super::*;

    #[test]
    fn fresh_and_legacy_cached_libraries_default_to_github() {
        let root = std::env::temp_dir().join(format!("hub-channel-default-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(CatalogChannel::select(None, &root).unwrap(), CatalogChannel::GitHub);
        assert_eq!(CatalogChannel::select(Some(""), &root).unwrap(), CatalogChannel::GitHub);
        std::fs::write(root.join("catalog.json"), "legacy cache is not a channel preference").unwrap();
        assert_eq!(CatalogChannel::select(None, &root).unwrap(), CatalogChannel::GitHub);
        assert_eq!(CatalogChannel::select(Some("legacy"), &root).unwrap(), CatalogChannel::Legacy);
        // Selecting v2 does not synthesize a proof or convert an offline legacy cache.
        assert!(CatalogChannel::GitHub.read_cache(&root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_channel_and_persisted_v2_prevent_downgrade() {
        let root = std::env::temp_dir().join(format!("hub-channel-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(CatalogChannel::select(Some("github-v2"), &root).unwrap(), CatalogChannel::GitHub);
        assert!(CatalogChannel::select(Some("typo"), &root).is_err());
        std::fs::write(root.join("catalog-v2.json"), "even a corrupted v2 cache must not cause fallback").unwrap();
        assert_eq!(CatalogChannel::select(None, &root).unwrap(), CatalogChannel::GitHub);
        assert!(CatalogChannel::Legacy.require_current(&root).is_err());
        assert!(CatalogChannel::select(Some("legacy"), &root).is_err());
        assert!(CatalogChannel::Legacy.for_library(&root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_v2_mirror_does_not_fetch_available_legacy() {
        let root = std::env::temp_dir().join(format!("hub-missing-v2-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("catalog.json"), "legacy fixture").unwrap();
        let origin = Origin::Directory(root.clone());
        assert!(origin.catalog_for(CatalogChannel::GitHub).is_err());
        assert_eq!(origin.catalog().unwrap(), "legacy fixture");
        std::fs::remove_dir_all(root).unwrap();
    }
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("a bundle may not hold a symlink".into());
        }
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
