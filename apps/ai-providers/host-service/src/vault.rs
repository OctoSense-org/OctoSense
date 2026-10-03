//! Where provider keys live: wherever the embedded octos kernel reads them.
//!
//! octos (`octos-cli/src/auth/keychain.rs`) resolves a profile env var whose
//! value is a `keychain:` marker from its secret store: the generic password
//! with service `octos` and account = the env var name, through the
//! `security` tool on macOS; the 0600 file `$HOME/.octos/secrets/<account>`
//! on Linux. On Android and iOS octos has no secret store and reads the raw
//! value in `config.env_vars`, so there the key is written into the profile
//! itself, which lives in app-private storage with mode 0600. HarmonyOS
//! builds are `target_os = "linux"`, but their kernel is octos embedded in
//! the shell's own process: octos looks under the shell's `HOME`, never the
//! kernel's octos home, so a key in the secrets folder is never found and
//! the profile holds it there too. Sealing it with an Android Keystore key,
//! as Mail does its passwords, would leave the kernel unable to read it.
//!
//! - macOS: [`OctosKeychain`]. An item written through the Security framework
//!   directly (the `keyring` crate) would trust only the shell, and octos's
//!   `security find-generic-password` would raise an access prompt on every
//!   start of the kernel; the `security` tool makes the items readable by
//!   both without prompts. The key goes over `security -i`'s standard input,
//!   never on a command line.
//! - Linux: [`SecretsDir`] under the core dir (the kernel runs with its octos
//!   home there).
//! - Elsewhere (Android, iOS, HarmonyOS), and with
//!   `OCTOSENSE_LLM_VAULT=file`: [`InProfile`]. On HarmonyOS, keys an earlier
//!   build left in the secrets folder move into the profile first
//!   (`move_secrets_into_profile`).
//!
//! A vault that refuses a key (`put` fails) leaves it in the profile: the
//! kernel must be able to run on it either way.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The profile value that tells octos to read the key from its store.
pub const KEYCHAIN_MARKER: &str = "keychain:";
/// The keychain service octos reads.
pub const OCTOS_SERVICE: &str = "octos";

/// A secret store keyed by account (an env var name). Errors never carry the
/// secret.
pub trait Vault: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    /// Keep `secret`; an error means the profile must hold it instead.
    fn put(&self, account: &str, secret: &str) -> Result<(), String>;
    fn remove(&self, account: &str) -> Result<bool, String>;
    /// Where keys go, for the app to say: "keychain", "secrets folder" or
    /// "profile".
    fn kind(&self) -> &'static str;
}

/// The profile value's store account, if the value is a marker: `keychain:`
/// alone means the env var's own name, `keychain:ACCOUNT` a scoped account
/// (octos's `marker_account`).
pub fn marker_account<'a>(value: &'a str, env_var: &'a str) -> Option<&'a str> {
    let rest = value.strip_prefix(KEYCHAIN_MARKER)?;
    Some(if rest.is_empty() { env_var } else { rest })
}

/// Account names octos accepts (it also keeps them as file names).
fn valid_account(account: &str) -> bool {
    !account.is_empty()
        && account.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
        && account != "."
        && account != ".."
}

fn check(account: &str) -> Result<(), String> {
    if valid_account(account) { Ok(()) } else { Err(format!("invalid key name {account:?}")) }
}

/// The store for this platform, for the kernel whose octos home is
/// `core_dir`. `OCTOSENSE_LLM_VAULT=file` keeps keys in the owner-only
/// profile instead (development: nothing touches the login keychain).
pub fn platform(core_dir: &Path) -> Arc<dyn Vault> {
    if std::env::var("OCTOSENSE_LLM_VAULT").is_ok_and(|v| v == "file") {
        return Arc::new(InProfile);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = core_dir;
        return Arc::new(OctosKeychain::octos());
    }
    // A move that fails keeps the folder; the next start tries again.
    #[cfg(target_env = "ohos")]
    let _ = move_secrets_into_profile(core_dir);
    #[cfg(all(target_os = "linux", not(target_env = "ohos")))]
    return Arc::new(SecretsDir::new(core_dir.join("secrets")));
    #[allow(unreachable_code)]
    {
        let _ = core_dir;
        Arc::new(InProfile)
    }
}

/// Where the kernel cannot read the secrets folder (HarmonyOS), put each key
/// the profile marks `keychain:` back into the profile, then delete the
/// folder: nothing else reads it, and a removed provider's key stays there
/// otherwise. A profile that cannot be written keeps the folder, so no key
/// is lost. Returns how many keys moved.
#[cfg_attr(not(target_env = "ohos"), allow(dead_code))]
fn move_secrets_into_profile(core_dir: &Path) -> Result<usize, String> {
    use octosense_llm_config::profile;
    let dir = core_dir.join("secrets");
    if !dir.is_dir() {
        return Ok(0);
    }
    let folder = SecretsDir::new(dir.clone());
    let path = profile::profile_path(core_dir);
    let loaded = profile::load(&path).map_err(|_| "the profile could not be read".to_string())?;
    let mut keys = BTreeMap::new();
    for (name, value) in &loaded.env_vars {
        if let Some(account) = marker_account(value, name) {
            if let Some(secret) = folder.get(account)? {
                keys.insert(name.clone(), secret);
            }
        }
    }
    if !keys.is_empty() {
        profile::save_env(&path, &keys).map_err(|_| "the profile could not be written".to_string())?;
    }
    std::fs::remove_dir_all(&dir).map_err(|_| "the secrets folder could not be deleted".to_string())?;
    Ok(keys.len())
}

/// No store: every key stays in the profile (Android, iOS, HarmonyOS,
/// development).
pub struct InProfile;

impl Vault for InProfile {
    fn get(&self, _account: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn put(&self, _account: &str, _secret: &str) -> Result<(), String> {
        Err("keys are kept in the profile on this device".into())
    }
    fn remove(&self, _account: &str) -> Result<bool, String> {
        Ok(false)
    }
    fn kind(&self) -> &'static str {
        "profile"
    }
}

/// An in-memory store, for tests.
#[derive(Default)]
pub struct MemoryVault {
    entries: Mutex<BTreeMap<String, String>>,
}

impl Vault for MemoryVault {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        Ok(self.entries.lock().unwrap().get(account).cloned())
    }
    fn put(&self, account: &str, secret: &str) -> Result<(), String> {
        check(account)?;
        self.entries.lock().unwrap().insert(account.to_string(), secret.to_string());
        Ok(())
    }
    fn remove(&self, account: &str) -> Result<bool, String> {
        Ok(self.entries.lock().unwrap().remove(account).is_some())
    }
    fn kind(&self) -> &'static str {
        "keychain"
    }
}

/// octos's file store: one 0600 file per account in a 0700 directory.
pub struct SecretsDir {
    dir: PathBuf,
}

impl SecretsDir {
    pub fn new(dir: PathBuf) -> Self {
        SecretsDir { dir }
    }
}

impl Vault for SecretsDir {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        check(account)?;
        match std::fs::read_to_string(self.dir.join(account)) {
            Ok(text) => Ok((!text.is_empty()).then_some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(format!("could not read {account} from the secret store")),
        }
    }
    fn put(&self, account: &str, secret: &str) -> Result<(), String> {
        check(account)?;
        std::fs::create_dir_all(&self.dir).map_err(|_| "could not create the secret store".to_string())?;
        #[cfg(unix)]
        {
            use std::io::Write as _;
            use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
            let _ = std::fs::set_permissions(&self.dir, std::fs::Permissions::from_mode(0o700));
            let temp = self.dir.join(format!(".{account}.tmp"));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&temp)
                .map_err(|_| format!("could not store {account}"))?;
            file.write_all(secret.as_bytes()).map_err(|_| format!("could not store {account}"))?;
            std::fs::rename(&temp, self.dir.join(account)).map_err(|_| format!("could not store {account}"))
        }
        #[cfg(not(unix))]
        {
            let _ = secret;
            Err("no secret store on this platform".into())
        }
    }
    fn remove(&self, account: &str) -> Result<bool, String> {
        check(account)?;
        Ok(std::fs::remove_file(self.dir.join(account)).is_ok())
    }
    fn kind(&self) -> &'static str {
        "secrets folder"
    }
}

/// The login keychain under `service` (`octos`; tests use their own so they
/// never touch real items), through the `security` tool as octos reads it.
pub struct OctosKeychain {
    #[allow(dead_code)]
    service: String,
}

impl OctosKeychain {
    pub fn octos() -> Self {
        Self::with_service(OCTOS_SERVICE)
    }
    pub fn with_service(service: &str) -> Self {
        OctosKeychain { service: service.to_string() }
    }
}

#[cfg(target_os = "macos")]
impl Vault for OctosKeychain {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        check(account)?;
        let out = std::process::Command::new("security")
            .args(["find-generic-password", "-s", &self.service, "-a", account, "-w"])
            .output()
            .map_err(|e| format!("could not run security: {e}"))?;
        if out.status.success() {
            let secret = String::from_utf8_lossy(&out.stdout).trim().to_string();
            return Ok((!secret.is_empty()).then_some(secret));
        }
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("could not be found") || err.contains("SecKeychainSearchCopyNext") {
            Ok(None)
        } else {
            Err(format!("keychain lookup failed for {account}"))
        }
    }

    fn put(&self, account: &str, secret: &str) -> Result<(), String> {
        use std::io::Write as _;
        check(account)?;
        // `security -i` reads one command per line and honours double quotes;
        // a key that cannot be quoted that way is refused (and so stays in
        // the profile).
        if secret.is_empty() || secret.chars().any(|c| c == '"' || c == '\\' || c.is_control()) {
            return Err("this key cannot be stored in the keychain".into());
        }
        let mut child = std::process::Command::new("security")
            .arg("-i")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("could not run security: {e}"))?;
        let line = format!("add-generic-password -U -s \"{}\" -a \"{}\" -w \"{}\"\n", self.service, account, secret);
        child
            .stdin
            .take()
            .ok_or("could not talk to security")?
            .write_all(line.as_bytes())
            .map_err(|e| format!("could not talk to security: {e}"))?;
        let out = child.wait_with_output().map_err(|e| format!("security failed: {e}"))?;
        // `security -i` exits 0 even when a command fails; confirm by reading.
        match self.get(account)? {
            Some(stored) if stored == secret && out.status.success() => Ok(()),
            _ => Err(format!("could not store {account} in the keychain")),
        }
    }

    fn remove(&self, account: &str) -> Result<bool, String> {
        check(account)?;
        let out = std::process::Command::new("security")
            .args(["delete-generic-password", "-s", &self.service, "-a", account])
            .output()
            .map_err(|e| format!("could not run security: {e}"))?;
        Ok(out.status.success())
    }

    fn kind(&self) -> &'static str {
        "keychain"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_accounts_follow_octos() {
        assert_eq!(marker_account("keychain:", "OPENAI_API_KEY"), Some("OPENAI_API_KEY"));
        assert_eq!(marker_account("keychain:OPENAI_API_KEY::alice", "OPENAI_API_KEY"), Some("OPENAI_API_KEY::alice"));
        assert_eq!(marker_account("sk-raw", "OPENAI_API_KEY"), None);
    }

    #[test]
    fn account_names_are_checked() {
        assert!(valid_account("OPENAI_API_KEY"));
        assert!(!valid_account("../x"));
        assert!(!valid_account("A B"));
        assert!(!valid_account(""));
    }

    #[test]
    fn a_secrets_dir_keeps_keys_owner_only() {
        let dir = std::env::temp_dir().join(format!("llm-vault-{}", std::process::id()));
        let vault = SecretsDir::new(dir.join("secrets"));
        vault.put("DEEPSEEK_API_KEY", "sk-test-dir-0001").unwrap();
        assert_eq!(vault.get("DEEPSEEK_API_KEY").unwrap().as_deref(), Some("sk-test-dir-0001"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("secrets/DEEPSEEK_API_KEY")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(vault.put("../escape", "x").is_err());
        assert!(vault.remove("DEEPSEEK_API_KEY").unwrap());
        assert_eq!(vault.get("DEEPSEEK_API_KEY").unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn core_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("llm-vault-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn write_profile(core: &Path, env: serde_json::Value) {
        let path = octosense_llm_config::profile::profile_path(core);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let profile = serde_json::json!({"id": "_main", "name": "Main", "config": {
            "llm": {"primary": {"family_id": "deepseek", "model_id": "deepseek-v4-flash", "context_window": 1048576}, "fallbacks": []},
            "env_vars": env}});
        std::fs::write(path, serde_json::to_vec_pretty(&profile).unwrap()).unwrap();
    }

    fn profile_json(core: &Path) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(octosense_llm_config::profile::profile_path(core)).unwrap()).unwrap()
    }

    /// HarmonyOS: a key an earlier build kept in the secrets folder, which
    /// the embedded kernel never reads, moves into the profile; the folder,
    /// with a removed provider's leftover key, is deleted.
    #[test]
    fn keys_in_the_secrets_folder_move_into_the_profile() {
        let core = core_dir("move");
        write_profile(&core, serde_json::json!({"DEEPSEEK_API_KEY": "keychain:", "GROQ_API_KEY": "keychain:GROQ_API_KEY::_main"}));
        let folder = SecretsDir::new(core.join("secrets"));
        folder.put("DEEPSEEK_API_KEY", "sk-test-move-0001").unwrap();
        folder.put("GROQ_API_KEY::_main", "gsk-test-move-0002").unwrap();
        folder.put("MOONSHOT_API_KEY", "sk-test-left-0003").unwrap();
        assert_eq!(move_secrets_into_profile(&core).unwrap(), 2);
        let profile = profile_json(&core);
        assert_eq!(profile["config"]["env_vars"]["DEEPSEEK_API_KEY"], "sk-test-move-0001");
        assert_eq!(profile["config"]["env_vars"]["GROQ_API_KEY"], "gsk-test-move-0002");
        assert_eq!(profile["config"]["llm"]["primary"]["context_window"], 1048576, "the selection is kept as it was");
        assert!(!core.join("secrets").exists(), "the folder and the leftover key are gone");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = octosense_llm_config::profile::profile_path(&core);
            assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert_eq!(move_secrets_into_profile(&core).unwrap(), 0, "nothing left to move");
        let _ = std::fs::remove_dir_all(&core);
    }

    #[test]
    fn a_profile_that_cannot_be_read_keeps_the_secrets_folder() {
        let core = core_dir("keep");
        let path = octosense_llm_config::profile::profile_path(&core);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not json").unwrap();
        SecretsDir::new(core.join("secrets")).put("DEEPSEEK_API_KEY", "sk-test-keep-0001").unwrap();
        assert!(move_secrets_into_profile(&core).is_err());
        assert_eq!(SecretsDir::new(core.join("secrets")).get("DEEPSEEK_API_KEY").unwrap().as_deref(), Some("sk-test-keep-0001"));
        let _ = std::fs::remove_dir_all(&core);
    }

    /// Touches the login keychain under a test-only service, so it runs only
    /// when asked: `cargo test -p octosense-llm-service -- --ignored keychain`.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn the_keychain_round_trips_under_a_test_service() {
        let vault = OctosKeychain::with_service("octosense-llm-service-test");
        let account = format!("TEST_KEY_{}", std::process::id());
        vault.put(&account, "sk-test-roundtrip-0000").unwrap();
        assert_eq!(vault.get(&account).unwrap().as_deref(), Some("sk-test-roundtrip-0000"));
        vault.put(&account, "sk-test-roundtrip-1111").unwrap();
        assert_eq!(vault.get(&account).unwrap().as_deref(), Some("sk-test-roundtrip-1111"));
        assert!(vault.remove(&account).unwrap());
        assert_eq!(vault.get(&account).unwrap(), None);
    }
}
