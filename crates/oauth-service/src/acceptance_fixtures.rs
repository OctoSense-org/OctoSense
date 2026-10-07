//! Compile-only dependency seam for native acceptance executables.
//!
//! Production builds have neither this module nor its lookup. No environment
//! variable, JSON request, app script or model can enable it. A native fixture
//! explicitly supplies dependencies for exactly one existing canonical host
//! root; every other root still uses the platform vault and HTTPS. Callers must
//! label synthetic provider evidence and never treat it as OAuth/live delivery.
use crate::{transport::Transport, CredentialStore};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone)]
pub struct Backend {
    pub vault: Arc<dyn CredentialStore>,
    pub transport: Arc<dyn Transport>,
}

fn registry() -> &'static Mutex<HashMap<PathBuf, Backend>> {
    static REGISTRY: OnceLock<Mutex<HashMap<PathBuf, Backend>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Native setup only, before starting the real host. Registration cannot
/// replace a backend already used by a profile. The root itself must exist.
pub fn install(root: &Path, backend: Backend) -> Result<(), String> {
    let root = validate_root(root)?;
    let mut registry = registry().lock().unwrap_or_else(|e| e.into_inner());
    if registry.contains_key(&root) {
        return Err("Acceptance fixture root is already registered".into());
    }
    registry.insert(root, backend);
    Ok(())
}

/// Call before constructing a fixture store as well as at final registration.
/// A fixture build cannot accidentally attach to an ordinary host profile.
pub fn validate_root(root: &Path) -> Result<PathBuf, String> {
    if !root.is_absolute() || !root.is_dir() {
        return Err("Acceptance fixture needs an existing absolute host root".into());
    }
    if root.file_name().is_none_or(|name| name != ".host") {
        return Err("Acceptance fixture requires the marked profile's .host directory".into());
    }
    let parent = root.parent().ok_or("Missing fixture profile")?;
    for directory in [root, parent] {
        let kind = std::fs::symlink_metadata(directory)
            .map_err(|_| "Cannot inspect fixture profile")?
            .file_type();
        if !kind.is_dir() || kind.is_symlink() {
            return Err("Fixture profile cannot be a symlink".into());
        }
    }
    let read = |path: &Path| -> Result<serde_json::Value, String> {
        let meta =
            std::fs::symlink_metadata(path).map_err(|_| "Missing acceptance profile marker")?;
        if !meta.is_file() || meta.len() > 1024 * 1024 {
            return Err("Invalid acceptance profile metadata".into());
        }
        serde_json::from_slice(&std::fs::read(path).map_err(|_| "Cannot read acceptance metadata")?)
            .map_err(|_| "Invalid acceptance metadata".into())
    };
    let marker = read(&parent.join(".connected-e2e.json"))?;
    if marker["fixture"] != "connected-e2e" || marker["schema"] != 1 {
        return Err("Not a marked connected-app acceptance profile".into());
    }
    let oauth = root.join("oauth");
    if oauth.exists() {
        if std::fs::symlink_metadata(&oauth)
            .map_err(|_| "Cannot inspect fixture OAuth directory")?
            .file_type()
            .is_symlink()
        {
            return Err("Fixture OAuth directory cannot be a symlink".into());
        }
        let clients = oauth.join("clients.json");
        if clients.exists() && read(&clients)? != serde_json::json!({}) {
            return Err("Refusing a profile with provider registrations".into());
        }
        let connections = oauth.join("connections.json");
        if connections.exists() {
            let entries = read(&connections)?;
            let entries = entries["entries"]
                .as_object()
                .ok_or("Invalid fixture account metadata")?;
            if entries.values().any(|c| {
                !c["app_id"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("org.octosense.samples."))
                    || !c["subject"].as_str().is_some_and(|s| {
                        s.starts_with("synthetic-") || s.ends_with("@example.test")
                    })
            }) {
                return Err("Refusing a non-synthetic connected account".into());
            }
        }
    }
    root.canonicalize()
        .map_err(|_| "Cannot resolve fixture root".into())
}

pub(crate) fn for_root(root: &Path) -> Option<Backend> {
    let root = root.canonicalize().ok()?;
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&root)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Refuse;
    impl CredentialStore for Refuse {
        fn put(&self, _: &str, _: &str) -> Result<(), String> {
            Err("fixture".into())
        }
        fn get(&self, _: &str) -> Result<String, String> {
            Err("fixture".into())
        }
        fn remove(&self, _: &str) -> Result<(), String> {
            Err("fixture".into())
        }
    }
    impl Transport for Refuse {
        fn send(&self, _: crate::transport::Request) -> Result<crate::transport::Response, String> {
            Err("fixture".into())
        }
    }
    #[test]
    fn native_fixture_is_exact_root_and_cannot_replace_a_live_registration() {
        let root = std::env::temp_dir().join(format!("connected-fixture-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join(".host/child")).unwrap();
        let backend = Backend {
            vault: Arc::new(Refuse),
            transport: Arc::new(Refuse),
        };
        let host = root.join(".host");
        assert!(install(&host, backend.clone()).is_err());
        std::fs::write(
            root.join(".connected-e2e.json"),
            r#"{"fixture":"connected-e2e","schema":1}"#,
        )
        .unwrap();
        install(&host, backend.clone()).unwrap();
        assert!(for_root(&host).is_some());
        assert!(for_root(&host.join("child")).is_none());
        assert!(for_root(&root).is_none());
        assert!(install(&host, backend).is_err());
        std::fs::create_dir_all(host.join("oauth")).unwrap();
        std::fs::write(
            host.join("oauth/clients.json"),
            r#"{"google":{"client_id":"fictional-live-registration"}}"#,
        )
        .unwrap();
        assert!(validate_root(&host).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
