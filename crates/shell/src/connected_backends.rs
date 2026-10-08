//! Installed app backend registrations come only from the signed, digest-checked
//! admitted bundle. Private registration history belongs to the OAuth service.
use octosense_oauth_service::backend::BackendDeclaration;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{Arc, Once},
    time::Duration,
};

pub fn register() {
    octosense_oauth_service::host::set_backend_resolver(Arc::new(resolve));
    static WATCHER: Once = Once::new();
    WATCHER.call_once(|| {
        let _ = std::thread::Builder::new()
            .name("backend-admission-watch".into())
            .spawn(|| {
                let mut previous = None;
                loop {
                    if let Some(root) = octosense_appstore::data_root_if_set() {
                        let stamp = (
                            root.clone(),
                            catalog_stamp(&root, octosense_appstore::source::CatalogChannel::from_environment(&root)),
                            file_stamp(&root.join(".host/oauth/connections.json")),
                            octosense_app_hub_app::icons::generation(),
                        );
                        if previous.as_ref() != Some(&stamp) {
                            previous = Some(stamp);
                            for app in backend_apps(&root.join(".host")) {
                                // This performs no provider HTTP. Refusal revokes locally;
                                // the app receives the actionable error on its next call.
                                let _ =
                                    octosense_oauth_service::host::revalidate_backend_registration(
                                        &root.join(".host"),
                                        &app,
                                    );
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_secs(5));
                }
            });
    });
}

fn file_stamp(path: &Path) -> Option<(u64, std::time::SystemTime)> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.len(), metadata.modified().ok()?))
}

// Include the selected channel even if its cache is missing. A transition to
// v2 (or a refused channel selection) must revalidate existing registrations;
// otherwise a quiet legacy cache could leave them active until the next call.
fn catalog_stamp(
    root: &Path,
    channel: Result<octosense_appstore::source::CatalogChannel, String>,
) -> Result<(&'static str, Option<(u64, std::time::SystemTime)>), String> {
    let channel = channel?;
    Ok((channel.filename(), file_stamp(&root.join(channel.filename()))))
}

fn valid_backend_app_id(app: &str) -> bool {
    !app.is_empty()
        && app.len() <= 64
        && !app.starts_with('.')
        && !app.contains("..")
        && app.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        })
        && crate::apps::check_script_app_id(app).is_ok()
}

fn resolve(root: &Path, app: &str) -> Result<Option<BackendDeclaration>, String> {
    let expected =
        octosense_appstore::data_root_if_set().ok_or("App Hub has no active apps root")?;
    resolve_admitted(root, &expected, app, || {
        crate::host_tools::script_apps::guidance(app)
            .map(|loaded| loaded.manifest)
            .map_err(|_| "This app's backend declaration is no longer admitted".into())
    })
}

fn resolve_admitted(
    root: &Path,
    apps_root: &Path,
    app: &str,
    load: impl FnOnce() -> Result<Value, String>,
) -> Result<Option<BackendDeclaration>, String> {
    if root != apps_root.join(".host") {
        return Err("Backend request belongs to another host profile".into());
    }
    match std::fs::symlink_metadata(root) {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err("Backend host directory is not a private directory".into())
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err("Backend host directory cannot be inspected".into())
        }
        _ => {}
    }
    if !valid_backend_app_id(app) {
        return Err("Invalid backend app identity".into());
    }
    let manifest = load()?;
    if manifest["id"].as_str() != Some(app) {
        return Err("Backend declaration belongs to another app".into());
    }
    let parsed = octosense_app_contract::AppManifest::parse(&manifest.to_string())?;
    if !parsed
        .capabilities
        .iter()
        .any(|capability| capability == "auth")
        || !parsed.storage.accounts
    {
        return Err("This app has no account authentication grant".into());
    }
    // Validate through the shared contract first; the OAuth crate's local
    // representation deliberately does not depend on the UI/store crate graph.
    parsed
        .backend
        .map(|declaration| {
            serde_json::to_value(declaration)
                .and_then(serde_json::from_value)
                .map_err(|_| "Invalid admitted backend registration".into())
        })
        .transpose()
}

fn backend_apps(host_root: &Path) -> BTreeSet<String> {
    let path = host_root.join("oauth/connections.json");
    if !std::fs::metadata(&path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= 1024 * 1024)
    {
        return BTreeSet::new();
    }
    let entries = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    entries
        .and_then(|value| value["entries"].as_object().cloned())
        .unwrap_or_default()
        .values()
        .filter(|entry| entry["provider"] == "backend")
        .filter_map(|entry| entry["app_id"].as_str())
        .filter(|app| valid_backend_app_id(app))
        .map(str::to_owned)
        .collect()
}

/// Installation notifications supplement per-call checks and the catalog watcher.
pub fn installed_changed(app: &str) {
    let Some(root) = octosense_appstore::data_root_if_set() else {
        return;
    };
    if !backend_apps(&root.join(".host")).contains(app) {
        return;
    }
    let app = app.to_owned();
    let _ = std::thread::Builder::new()
        .name("backend-install-check".into())
        .spawn(move || {
            // Invalidate the lifecycle event itself, even if a rapid reinstall
            // has already restored an identical declaration by this point.
            let _ = octosense_oauth_service::host::invalidate_backend_registration(
                &root.join(".host"),
                &app,
            );
            let _ = octosense_oauth_service::host::revalidate_backend_registration(
                &root.join(".host"),
                &app,
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn watcher_tracks_selected_catalog_updates_and_channel_refusal() {
        use octosense_appstore::source::CatalogChannel::{GitHub, Legacy};
        let root = std::env::temp_dir().join(format!("backend-catalog-watch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("catalog.json"), "unchanged legacy").unwrap();
        let legacy = catalog_stamp(&root, Ok(Legacy));
        let missing_v2 = catalog_stamp(&root, Ok(GitHub));
        assert_ne!(legacy, missing_v2, "changing channel must invalidate even before its cache arrives");
        std::fs::write(root.join("catalog-v2.json"), "first v2").unwrap();
        let first_v2 = catalog_stamp(&root, Ok(GitHub));
        assert_ne!(missing_v2, first_v2);
        std::fs::write(root.join("catalog-v2.json"), "updated v2 withdrawal").unwrap();
        assert_ne!(first_v2, catalog_stamp(&root, Ok(GitHub)));
        assert_eq!(legacy, catalog_stamp(&root, Ok(Legacy)), "v2 refresh is independent of legacy bytes");
        assert_ne!(legacy, catalog_stamp(&root, Err("downgrade refused".into())));
        std::fs::remove_file(root.join("catalog-v2.json")).unwrap();
        assert_eq!(missing_v2, catalog_stamp(&root, Ok(GitHub)), "cache removal must invalidate too");
        std::fs::remove_dir_all(root).unwrap();
    }
    fn manifest() -> Value {
        serde_json::json!({"schema":1,"id":"org.example.backend","version":"1.0.0","name":"Backend fixture",
            "integrity":{"bundle_blake3":"00".repeat(32)},"capabilities":["auth"],"storage":{"accounts":true},
            "requires":["backend-api-v1"],"backend":{"id":"fixture","client_id":"public-native",
            "authorization_url":"https://example.test/authorize","token_url":"https://example.test/token",
            "me_url":"https://example.test/me","logout_url":"https://example.test/logout","scopes":["app.session"],
            "operations":{"notes.list":{"method":"GET","path":"/api/notes"}}}})
    }
    #[test]
    fn resolver_only_accepts_matching_profile_identity_and_admitted_grants() {
        let root = std::env::temp_dir().join(format!("backend-resolver-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join(".host")).unwrap();
        let id = "org.example.backend";
        let declaration = resolve_admitted(&root.join(".host"), &root, id, || Ok(manifest()))
            .unwrap()
            .unwrap();
        assert!(declaration.operations.contains_key("notes.list"));
        assert!(resolve_admitted(&root.join("other"), &root, id, || panic!(
            "foreign root must not load a bundle"
        ))
        .is_err());
        assert!(
            resolve_admitted(&root.join(".host"), &root, "org.example.other", || Ok(
                manifest()
            ))
            .is_err()
        );
        assert!(resolve_admitted(&root.join(".host"), &root, id, || Err(
            "withdrawn or digest mismatch".into()
        ))
        .is_err());
        let mut removed = manifest();
        removed.as_object_mut().unwrap().remove("backend");
        assert!(
            resolve_admitted(&root.join(".host"), &root, id, || Ok(removed))
                .unwrap()
                .is_none()
        );
        let mut denied = manifest();
        denied["capabilities"] = serde_json::json!([]);
        assert!(resolve_admitted(&root.join(".host"), &root, id, || Ok(denied)).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn watcher_uses_only_backend_app_id_metadata() {
        let root = std::env::temp_dir().join(format!("backend-watch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("oauth")).unwrap();
        std::fs::write(
            root.join("oauth/connections.json"),
            serde_json::json!({"entries":{
            "a":{"provider":"backend","app_id":"org.example.backend"},
            "b":{"provider":"google","app_id":"org.example.calendar"},
            "c":{"provider":"backend","app_id":"../../bad"}}})
            .to_string(),
        )
        .unwrap();
        assert_eq!(
            backend_apps(&root),
            BTreeSet::from(["org.example.backend".into()])
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
