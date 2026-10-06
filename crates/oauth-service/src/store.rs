use crate::{oauth::Tokens, Provider};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;

/// Implemented by the host's platform credential vault, never by an app.
pub trait CredentialStore: Send + Sync {
    fn put(&self, key: &str, value: &str) -> Result<(), String>;
    fn get(&self, key: &str) -> Result<String, String>;
    fn remove(&self, key: &str) -> Result<(), String>;
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub handle: String,
    pub app_id: String,
    pub provider: Provider,
    pub subject: String,
    pub label: String,
    pub scopes: BTreeSet<String>,
    pub expires_at: Option<u64>,
}

pub struct Connections {
    root: PathBuf,
    entries: BTreeMap<String, Connection>,
    active: BTreeMap<String, String>,
    vault: Arc<dyn CredentialStore>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    entries: BTreeMap<String, Connection>,
    active: BTreeMap<String, String>,
}

impl Connections {
    /// The shell supplies a private host directory, outside all app jails.
    pub fn open(root: &Path, vault: Arc<dyn CredentialStore>) -> Result<Self, String> {
        let path = root.join("connections.json");
        let metadata: Metadata = match std::fs::read(&path) {
            Ok(bytes) if bytes.len() <= 1024 * 1024 => {
                serde_json::from_slice(&bytes).map_err(|_| "Invalid OAuth connection metadata")?
            }
            Ok(_) => return Err("OAuth connection metadata exceeds its limit".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Metadata::default(),
            Err(_) => return Err("Cannot read OAuth connection metadata".into()),
        };
        for (handle, connection) in &metadata.entries {
            if handle != &connection.handle || Uuid::parse_str(handle).is_err() {
                return Err("Invalid OAuth connection handle".into());
            }
        }
        for (app, handle) in &metadata.active {
            if !metadata
                .entries
                .get(handle)
                .is_some_and(|entry| &entry.app_id == app)
            {
                return Err("Invalid active OAuth connection".into());
            }
        }
        Ok(Self {
            root: root.into(),
            entries: metadata.entries,
            active: metadata.active,
            vault,
        })
    }
    pub fn active(&self, caller: &str) -> Option<Connection> {
        self.active
            .get(caller)
            .and_then(|id| self.entries.get(id))
            .cloned()
    }
    pub fn select(&mut self, caller: &str, handle: &str) -> Result<Connection, String> {
        let entry = self
            .entries
            .get(handle)
            .filter(|entry| entry.app_id == caller)
            .cloned()
            .ok_or("Connection is unavailable to this app")?;
        let previous = self.active.insert(caller.into(), handle.into());
        if let Err(error) = self.persist() {
            match previous {
                Some(id) => {
                    self.active.insert(caller.into(), id);
                }
                None => {
                    self.active.remove(caller);
                }
            }
            return Err(error);
        }
        Ok(entry)
    }
    pub fn list(&self, caller: &str) -> Vec<Connection> {
        self.entries
            .values()
            .filter(|c| c.app_id == caller)
            .cloned()
            .collect()
    }
    pub fn authorized(
        &self,
        caller: &str,
        handle: &str,
        provider: Provider,
        scope: &str,
    ) -> Result<&Connection, String> {
        let entry = self
            .entries
            .get(handle)
            .filter(|c| c.app_id == caller && c.provider == provider)
            .ok_or("Connection is unavailable to this app")?;
        if !entry.scopes.contains(scope) {
            return Err("This operation needs additional authorization".into());
        }
        Ok(entry)
    }
    /// Called only after the provider's verified identity endpoint succeeds.
    pub fn connect(
        &mut self,
        caller: &str,
        provider: Provider,
        subject: &str,
        label: &str,
        tokens: Tokens,
    ) -> Result<Connection, String> {
        if caller.is_empty()
            || subject.is_empty()
            || subject.len() > 512
            || label.len() > 512
            || self.entries.len() >= 128
        {
            return Err("Invalid account identity or connection limit reached".into());
        }
        let handle = Uuid::new_v4().to_string();
        let entry = Connection {
            handle: handle.clone(),
            app_id: caller.into(),
            provider,
            subject: subject.into(),
            label: label.into(),
            scopes: tokens.scopes.clone(),
            expires_at: tokens.expires_at,
        };
        self.write_tokens(&handle, &tokens)?;
        self.entries.insert(handle.clone(), entry.clone());
        let previous = self.active.insert(caller.into(), handle.clone());
        if let Err(error) = self.persist() {
            self.entries.remove(&handle);
            match previous {
                Some(id) => {
                    self.active.insert(caller.into(), id);
                }
                None => {
                    self.active.remove(caller);
                }
            }
            let _ = self.vault.remove(&handle);
            return Err(error);
        }
        Ok(entry)
    }
    pub fn disconnect(&mut self, caller: &str, handle: &str) -> Result<(), String> {
        let entry = self
            .entries
            .get(handle)
            .filter(|c| c.app_id == caller)
            .cloned()
            .ok_or("Connection is unavailable to this app")?;
        // Persist revocation before deleting credentials: an interrupted delete
        // must never leave a usable connection after restart.
        self.entries.remove(handle);
        let previous = if self
            .active
            .get(caller)
            .is_some_and(|active| active == handle)
        {
            self.active.remove(caller)
        } else {
            None
        };
        if let Err(error) = self.persist() {
            self.entries.insert(handle.into(), entry);
            if let Some(id) = previous {
                self.active.insert(caller.into(), id);
            }
            return Err(error);
        }
        self.vault.remove(handle)
    }
    pub(crate) fn tokens(
        &self,
        caller: &str,
        handle: &str,
        provider: Provider,
        scope: &str,
    ) -> Result<Tokens, String> {
        let entry = self.authorized(caller, handle, provider, scope)?;
        let encoded = self.vault.get(handle)?;
        let value: serde_json::Value =
            serde_json::from_str(&encoded).map_err(|_| "Stored OAuth credential is invalid")?;
        let access = value["access"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Sign in again")?
            .to_string();
        Ok(Tokens {
            access,
            refresh: value["refresh"].as_str().map(str::to_string),
            expires_at: value["expires_at"].as_u64(),
            scopes: entry.scopes.clone(),
        })
    }
    pub(crate) fn replace_tokens(
        &mut self,
        caller: &str,
        handle: &str,
        tokens: Tokens,
    ) -> Result<(), String> {
        let entry = self
            .entries
            .get(handle)
            .filter(|c| c.app_id == caller)
            .ok_or("Connection was revoked")?;
        if entry.scopes != tokens.scopes {
            return Err("Refresh cannot change connection permissions".into());
        }
        self.write_tokens(handle, &tokens)?;
        self.entries.get_mut(handle).unwrap().expires_at = tokens.expires_at;
        self.persist()
    }
    fn write_tokens(&self, handle: &str, tokens: &Tokens) -> Result<(), String> {
        self.vault.put(handle, &serde_json::json!({"access":tokens.access, "refresh":tokens.refresh, "expires_at":tokens.expires_at}).to_string())
    }
    fn persist(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.root).map_err(|_| "Cannot create OAuth host directory")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| "Cannot protect OAuth directory")?;
        }
        let bytes = serde_json::to_vec_pretty(&Metadata {
            entries: self.entries.clone(),
            active: self.active.clone(),
        })
        .map_err(|_| "Cannot encode OAuth metadata")?;
        let pending = self
            .root
            .join(format!("connections-{}.tmp", Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write;
        let mut file = options
            .open(&pending)
            .map_err(|_| "Cannot write OAuth metadata")?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "Cannot flush OAuth metadata")?;
        drop(file);
        std::fs::rename(&pending, self.root.join("connections.json"))
            .map_err(|_| "Cannot commit OAuth metadata".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    #[derive(Default)]
    struct MemoryVault(Mutex<BTreeMap<String, String>>);
    impl CredentialStore for MemoryVault {
        fn put(&self, key: &str, value: &str) -> Result<(), String> {
            self.0.lock().unwrap().insert(key.into(), value.into());
            Ok(())
        }
        fn get(&self, key: &str) -> Result<String, String> {
            self.0
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .ok_or("Missing credential".into())
        }
        fn remove(&self, key: &str) -> Result<(), String> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }
    #[test]
    fn credentials_survive_restart_without_entering_app_replies_or_metadata() {
        let dir = std::env::temp_dir().join(format!("oauth-store-{}", Uuid::new_v4()));
        let vault = Arc::new(MemoryVault::default());
        let mut store = Connections::open(&dir, vault.clone()).unwrap();
        let token = Tokens {
            access: "fixture-access-secret".into(),
            refresh: Some("fixture-refresh-secret".into()),
            expires_at: Some(1000),
            scopes: BTreeSet::from(["read:user".into()]),
        };
        let c = store
            .connect("app.one", Provider::Github, "123", "Test account", token)
            .unwrap();
        assert!(!serde_json::to_string(&c).unwrap().contains("secret"));
        assert!(!std::fs::read_to_string(dir.join("connections.json"))
            .unwrap()
            .contains("secret"));
        let mut store = Connections::open(&dir, vault).unwrap();
        assert_eq!(store.list("app.one"), [c.clone()]);
        assert!(store.list("app.two").is_empty());
        assert!(store
            .tokens("app.two", &c.handle, Provider::Github, "read:user")
            .is_err());
        assert!(store
            .tokens("app.one", &c.handle, Provider::Google, "read:user")
            .is_err());
        assert!(store
            .tokens("app.one", &c.handle, Provider::Github, "repo")
            .is_err());
        assert_eq!(
            store
                .tokens("app.one", &c.handle, Provider::Github, "read:user")
                .unwrap()
                .access,
            "fixture-access-secret"
        );
        assert!(store.disconnect("app.two", &c.handle).is_err());
        store.disconnect("app.one", &c.handle).unwrap();
        assert!(store
            .tokens("app.one", &c.handle, Provider::Github, "read:user")
            .is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn selected_account_survives_restart_and_never_crosses_app_ownership() {
        let dir = std::env::temp_dir().join(format!("oauth-active-{}", Uuid::new_v4()));
        let vault = Arc::new(MemoryVault::default());
        let mut store = Connections::open(&dir, vault.clone()).unwrap();
        let mut connect = |app, subject| {
            store
                .connect(
                    app,
                    Provider::Github,
                    subject,
                    "Fixture",
                    Tokens {
                        access: "fixture-only".into(),
                        refresh: None,
                        expires_at: None,
                        scopes: BTreeSet::from(["read:user".into()]),
                    },
                )
                .unwrap()
        };
        let a = connect("app.one", "1");
        let b = connect("app.one", "2");
        let foreign = connect("app.two", "3");
        assert_eq!(store.active("app.one"), Some(b.clone()));
        assert!(store.select("app.one", &foreign.handle).is_err());
        store.select("app.one", &a.handle).unwrap();
        let mut store = Connections::open(&dir, vault.clone()).unwrap();
        assert_eq!(store.active("app.one"), Some(a.clone()));
        store.disconnect("app.one", &b.handle).unwrap();
        assert_eq!(store.active("app.one"), Some(a.clone()));
        store.disconnect("app.one", &a.handle).unwrap();
        let store = Connections::open(&dir, vault).unwrap();
        assert!(store.active("app.one").is_none());
        assert_eq!(store.active("app.two"), Some(foreign));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
