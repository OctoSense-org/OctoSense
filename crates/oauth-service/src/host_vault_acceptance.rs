//! Explicit opt-in check of the real OS credential adapter. No provider call,
//! existing account, fixture backend or developer plaintext vault is involved.
use crate::{host, oauth::Tokens, Provider};
use serde_json::json;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

struct Profile(PathBuf);
impl Drop for Profile {
    fn drop(&mut self) {
        // A failed assertion must not leave the synthetic connection behind.
        if let Ok(mut store) = host::connections(&self.0) {
            for connection in store.list(APP) {
                let _ = store.disconnect(APP, &connection.handle);
            }
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
const APP: &str = "org.octosense.acceptance.platformvault";

fn contains_plaintext(root: &Path, needle: &[u8]) -> bool {
    std::fs::read_dir(root).unwrap().any(|entry| {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(
            !kind.is_symlink(),
            "Synthetic profile unexpectedly contains a symlink"
        );
        if kind.is_dir() {
            contains_plaintext(&entry.path(), needle)
        } else if kind.is_file() {
            std::fs::read(entry.path())
                .unwrap()
                .windows(needle.len())
                .any(|bytes| bytes == needle)
        } else {
            false
        }
    })
}

#[test]
#[ignore = "Uses the actual OS credential store; run explicitly in an unlocked test session"]
fn platform_vault_persists_across_reopen_without_plaintext_credentials() {
    assert!(
        !std::env::var("OCTOSENSE_MAIL_VAULT").is_ok_and(|value| value == "file"),
        "Disable the plaintext development override for OS vault acceptance"
    );
    let profile = Profile(std::env::temp_dir().join(format!(
        "octosense-vault-acceptance-{}",
        uuid::Uuid::new_v4()
    )));
    std::fs::create_dir(&profile.0).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&profile.0, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let access = format!("synthetic-not-a-provider-token-{}", uuid::Uuid::new_v4());
    let refresh = format!("synthetic-not-a-refresh-token-{}", uuid::Uuid::new_v4());
    let scopes: BTreeSet<String> = ["openid".into(), "email".into()].into_iter().collect();
    let tokens = Tokens::from_response(
        &json!({
            "access_token": access,
            "refresh_token": refresh,
            "token_type": "Bearer",
            "expires_in": 3600,
            "scope": "openid email"
        }),
        &scopes,
        1_791_321_600,
    )
    .unwrap();
    let mut store = host::connections(&profile.0).expect("Actual OS vault must be available");
    let connection = store
        .connect(
            APP,
            Provider::Google,
            "synthetic-subject",
            "Synthetic vault acceptance",
            tokens,
        )
        .expect("Actual OS vault must accept the synthetic credential");
    drop(store);

    let mut reopened = host::connections(&profile.0).unwrap();
    assert_eq!(reopened.active(APP).unwrap().handle, connection.handle);
    let restored = reopened
        .tokens(APP, &connection.handle, Provider::Google, "openid")
        .unwrap();
    assert!(
        restored.access == access,
        "Reopened vault returned different access data"
    );
    assert!(
        restored.refresh.as_deref() == Some(refresh.as_str()),
        "Reopened vault returned different refresh data"
    );
    assert!(!contains_plaintext(&profile.0, access.as_bytes()));
    assert!(!contains_plaintext(&profile.0, refresh.as_bytes()));
    reopened.disconnect(APP, &connection.handle).unwrap();
    drop(reopened);
    let revoked = host::connections(&profile.0).unwrap();
    assert!(revoked.active(APP).is_none());
    assert!(revoked.list(APP).is_empty());
    assert!(revoked
        .tokens(APP, &connection.handle, Provider::Google, "openid")
        .is_err());
    // Successful logical revocation is tested here. Actual Keychain / Credential
    // Manager item deletion failures are not observable through Mail's legacy
    // void-returning remove adapter and must not be claimed by this assertion.
}
