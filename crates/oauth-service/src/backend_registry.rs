//! Host-private registration observations prevent update/rollback from reviving
//! a previously revoked backend grant. No endpoint, identity or token is stored.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    schema: u32,
    app: String,
    binding: Option<String>,
}

/// The OAuth metadata lock must cover this operation and `revoke` together.
/// First-time introduction preserves a valid operator connection; first-time
/// refusal and every later change revoke before recording the new observation.
pub(super) fn observe(
    root: &Path,
    app: &str,
    binding: Option<&str>,
    mut revoke: impl FnMut() -> Result<(), String>,
) -> Result<(), String> {
    let directory = root.join("oauth/backend-registrations");
    std::fs::create_dir_all(&directory)
        .map_err(|_| "Cannot create backend registration history")?;
    let path = directory.join(format!("{:x}.json", Sha256::digest(app.as_bytes())));
    let previous = match std::fs::File::open(&path) {
        Ok(file) => {
            let mut bytes = vec![];
            file.take(4097)
                .read_to_end(&mut bytes)
                .map_err(|_| "Cannot read backend registration history")?;
            if bytes.len() > 4096 {
                return Err("Invalid backend registration history".into());
            }
            let saved: Observation = serde_json::from_slice(&bytes)
                .map_err(|_| "Invalid backend registration history")?;
            if saved.schema != 1 || saved.app != app {
                return Err("Invalid backend registration history".into());
            }
            Some(saved.binding)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err("Cannot read backend registration history".into()),
    };
    if previous
        .as_ref()
        .is_some_and(|old| old.as_deref() == binding)
    {
        return Ok(());
    }
    if previous.is_some() || binding.is_none() {
        revoke()?;
    }
    let bytes = serde_json::to_vec(&Observation {
        schema: 1,
        app: app.into(),
        binding: binding.map(str::to_owned),
    })
    .map_err(|_| "Cannot encode backend registration history")?;
    let temporary = directory.join(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Cannot protect backend registration history")?;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| "Cannot write backend registration history")?;
    let result = file
        .write_all(&bytes)
        .and_then(|_| file.sync_all())
        .and_then(|_| std::fs::rename(&temporary, &path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|_| "Cannot commit backend registration history".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removal_and_rollback_revoke_and_observations_survive_reopen() {
        let root =
            std::env::temp_dir().join(format!("backend-observation-{}", uuid::Uuid::new_v4()));
        let mut revoked = 0;
        observe(&root, "example.app", Some("first"), || {
            revoked += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(revoked, 0);
        observe(&root, "example.app", Some("first"), || {
            revoked += 1;
            Ok(())
        })
        .unwrap();
        observe(&root, "example.app", Some("second"), || {
            revoked += 1;
            Ok(())
        })
        .unwrap();
        observe(&root, "example.app", Some("first"), || {
            revoked += 1;
            Ok(())
        })
        .unwrap();
        observe(&root, "example.app", None, || {
            revoked += 1;
            Ok(())
        })
        .unwrap();
        observe(&root, "example.app", None, || {
            revoked += 1;
            Ok(())
        })
        .unwrap();
        observe(&root, "example.app", Some("first"), || {
            revoked += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(revoked, 4);
        assert!(observe(&root, "example.app", Some("third"), || Err(
            "vault unavailable".into()
        ))
        .is_err());
        observe(&root, "example.app", Some("third"), || {
            revoked += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            revoked, 5,
            "failed revocation must be retried before admitting the new registration"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
