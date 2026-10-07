//! Host-owned native OAuth registrations. This is not a token or app API.
use serde::Deserialize;
use std::{io::Read, path::Path};

const MAX_CONFIG_BYTES: u64 = 16_384;
const INVALID_CONFIG: &str = "Sign-in is unavailable because this installation's provider configuration is invalid. Contact the OctoSense distributor.";
const UNREADABLE_CONFIG: &str = "Sign-in is unavailable because this installation's provider configuration could not be read. Contact the OctoSense distributor.";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Client {
    pub client_id: String,
    pub client_secret: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct Clients {
    pub github: Option<Client>,
    pub google: Option<Client>,
}

impl Clients {
    fn validate(self) -> Result<Self, String> {
        for client in [&self.github, &self.google].into_iter().flatten() {
            if client.client_id.is_empty()
                || client.client_id.len() > 256
                || client.client_id.chars().any(char::is_whitespace)
                || client.client_id.chars().any(char::is_control)
                || client
                    .client_secret
                    .as_ref()
                    .is_some_and(|value| value.len() > 4096 || value.chars().any(char::is_control))
            {
                return Err(INVALID_CONFIG.into());
            }
        }
        Ok(self)
    }
}

/// Values supplied by the host distributor at compilation, never by a bundle.
/// Native registration values are distributed with the executable and cannot
/// protect a confidential web-client secret. User tokens belong in the vault.
#[derive(Default)]
struct BuildRegistrations<'a> {
    github_client_id: Option<&'a str>,
    google_desktop_client_id: Option<&'a str>,
    google_desktop_registration_value: Option<&'a str>,
}

impl BuildRegistrations<'_> {
    fn clients(&self) -> Result<Clients, String> {
        let github = self.github_client_id.filter(|id| !id.is_empty());
        let google = self.google_desktop_client_id.filter(|id| !id.is_empty());
        let google_value = self
            .google_desktop_registration_value
            .filter(|value| !value.is_empty());
        if google.is_none() && google_value.is_some() {
            return Err(INVALID_CONFIG.into());
        }
        Clients {
            github: github.map(|id| Client {
                client_id: id.into(),
                client_secret: None,
            }),
            google: google.map(|id| Client {
                client_id: id.into(),
                client_secret: google_value.map(str::to_owned),
            }),
        }
        .validate()
    }
}

pub(crate) fn clients(root: &Path) -> Result<Clients, String> {
    resolve(
        root,
        &BuildRegistrations {
            github_client_id: option_env!("OCTOSENSE_GITHUB_CLIENT_ID"),
            google_desktop_client_id: option_env!("OCTOSENSE_GOOGLE_DESKTOP_CLIENT_ID"),
            google_desktop_registration_value: option_env!(
                "OCTOSENSE_GOOGLE_DESKTOP_REGISTRATION_VALUE"
            ),
        },
    )
}

fn resolve(root: &Path, defaults: &BuildRegistrations<'_>) -> Result<Clients, String> {
    let path = root.join("oauth/clients.json");
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // A dangling operator symlink is an unreadable override, not an
            // absent one. Do not switch its OAuth identity to release defaults.
            return match std::fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => defaults.clients(),
                _ => Err(UNREADABLE_CONFIG.into()),
            };
        }
        Err(_) => return Err(UNREADABLE_CONFIG.into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| UNREADABLE_CONFIG)?;
    if bytes.len() > MAX_CONFIG_BYTES as usize {
        return Err(INVALID_CONFIG.into());
    }
    // A present operator file replaces the whole registration set, including
    // omitted providers. Never silently fall back to a different OAuth client.
    serde_json::from_slice::<Clients>(&bytes)
        .map_err(|_| INVALID_CONFIG)?
        .validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Profile(PathBuf);
    impl Profile {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("oauth-registration-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(root.join("oauth")).unwrap();
            Self(root)
        }
        fn write(&self, contents: &[u8]) {
            std::fs::write(self.0.join("oauth/clients.json"), contents).unwrap();
        }
    }
    impl Drop for Profile {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn defaults() -> BuildRegistrations<'static> {
        BuildRegistrations {
            github_client_id: Some("fixture-github-client"),
            google_desktop_client_id: Some("fixture-desktop.apps.googleusercontent.com"),
            google_desktop_registration_value: Some("fixture-native-registration-value"),
        }
    }

    #[test]
    fn fresh_profile_uses_release_registrations_without_creating_operator_file() {
        let profile = Profile::new();
        let clients = resolve(&profile.0, &defaults()).unwrap();
        assert_eq!(clients.github.unwrap().client_id, "fixture-github-client");
        let google = clients.google.unwrap();
        assert_eq!(
            google.client_id,
            "fixture-desktop.apps.googleusercontent.com"
        );
        assert_eq!(
            google.client_secret.as_deref(),
            Some("fixture-native-registration-value")
        );
        assert!(!profile.0.join("oauth/clients.json").exists());
    }

    #[test]
    fn operator_file_replaces_all_defaults_including_omitted_providers() {
        let profile = Profile::new();
        profile.write(br#"{"github":{"client_id":"fixture-operator-client"}}"#);
        let clients = resolve(&profile.0, &defaults()).unwrap();
        assert_eq!(clients.github.unwrap().client_id, "fixture-operator-client");
        assert!(clients.google.is_none());
        profile.write(b"{}");
        let clients = super::clients(&profile.0).unwrap();
        assert!(clients.github.is_none() && clients.google.is_none());
    }

    #[test]
    fn malformed_unknown_oversized_and_invalid_overrides_never_fall_back() {
        let profile = Profile::new();
        for contents in [
            b"{".as_slice(),
            br#"{"endpoint":"https://example.test"}"#,
            br#"{"google":{"client_id":"","client_secret":"fixture-private-value"}}"#,
        ] {
            profile.write(contents);
            assert!(
                matches!(resolve(&profile.0, &defaults()), Err(error) if error == INVALID_CONFIG)
            );
        }
        profile.write(&vec![b' '; MAX_CONFIG_BYTES as usize + 1]);
        assert!(matches!(resolve(&profile.0, &defaults()), Err(error) if error == INVALID_CONFIG));
    }

    #[test]
    fn unreadable_override_never_uses_release_identity() {
        let profile = Profile::new();
        std::fs::create_dir(profile.0.join("oauth/clients.json")).unwrap();
        assert!(
            matches!(resolve(&profile.0, &defaults()), Err(error) if error == UNREADABLE_CONFIG)
        );
    }

    #[cfg(unix)]
    #[test]
    fn dangling_operator_symlink_does_not_select_release_identity() {
        let profile = Profile::new();
        std::os::unix::fs::symlink(
            profile.0.join("missing-registration"),
            profile.0.join("oauth/clients.json"),
        )
        .unwrap();
        assert!(
            matches!(resolve(&profile.0, &defaults()), Err(error) if error == UNREADABLE_CONFIG)
        );
    }

    #[test]
    fn missing_release_registration_stays_unconfigured_and_invalid_values_are_redacted() {
        let profile = Profile::new();
        let clients = resolve(&profile.0, &BuildRegistrations::default()).unwrap();
        assert!(clients.github.is_none() && clients.google.is_none());
        let orphan = BuildRegistrations {
            google_desktop_registration_value: Some("fixture-private-value"),
            ..Default::default()
        };
        assert!(matches!(resolve(&profile.0, &orphan), Err(error) if error == INVALID_CONFIG));
        let invalid = BuildRegistrations {
            github_client_id: Some("fixture\ninvalid-client"),
            ..Default::default()
        };
        assert!(matches!(resolve(&profile.0, &invalid), Err(error) if error == INVALID_CONFIG));
        // A complete valid operator override does not consume invalid defaults.
        profile.write(b"{}");
        assert!(resolve(&profile.0, &invalid).is_ok());
    }
}
