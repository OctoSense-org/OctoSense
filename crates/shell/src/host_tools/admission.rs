//! The agent path uses the same signed catalog and installed-release check as
//! the Card runner. A cached tool/peer or user consent cannot outlive withdrawal.
use std::path::{Path, PathBuf};

pub(crate) fn check(app: &str) -> Result<(), String> {
    if crate::native_apps::find(app).is_some()
        || [super::relay::SYSTEM, super::relay::TOOLBOX, super::relay::HOST_EXECUTOR, crate::agents::OWNER].contains(&app)
    {
        return Ok(());
    }
    crate::apps::check_script_app_id(app)?;
    let root = octosense_appstore::data_root_if_set().ok_or("App Hub has no apps root yet")?;
    if let Some(system) = octosense_appstore::system::system_app(app) {
        octosense_appstore::system::prepare(&root, &system)?;
        return Ok(());
    }
    installed_bundle(&root, app).map(|_| ())
}

pub(crate) fn installed_bundle(root: &Path, app: &str) -> Result<PathBuf, String> {
    let anchor = std::env::var("OCTOSENSE_HUB_ANCHOR")
        .unwrap_or_else(|_| octosense_appstore::DEFAULT_ANCHOR.to_string());
    installed_bundle_with_anchor(root, app, &anchor)
}

fn installed_bundle_with_anchor(root: &Path, app: &str, anchor: &str) -> Result<PathBuf, String> {
    let channel = octosense_appstore::source::CatalogChannel::from_environment(root)?;
    let mut store = channel.configure(octosense_app_hub::Store::new(
        anchor,
        root,
        octosense_app_contract::HostLimits::default(),
    )
    .with_host_api_versions(octosense_appstore::host_api::available_versions()));
    let catalog = channel.read_cache(root)
        .map_err(|_| "No verified App Hub catalog is available on this device".to_string())?;
    store.accept_catalog(&catalog).map_err(|e| format!("App Hub catalog refused: {e}"))?;
    octosense_app_hub_app::catalog::check_verified_catalog_floor(
        root, anchor, store.catalog().map(|catalog| catalog.sequence),
    )?;
    store.may_run(app).map_err(|e| format!("App agent is unavailable: {e}"))?;
    Ok(store.install_dir(app))
}

#[cfg(test)]
mod tests {
    use super::*;
    use octosense_app_hub::{Catalog, Entry, HubKey, Source, Status};
    use serde_json::json;

    #[test]
    fn cached_agent_admission_tracks_signed_withdrawal_and_bundle_integrity() {
        let root = std::env::temp_dir().join(format!("agent-withdrawal-{}", uuid::Uuid::new_v4()));
        let id = "org.example.fixture";
        let bundle = octosense_app_hub::installed_bundle_dir(&root, id);
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("main.splash"), "Label {text: \"Fixture\"}").unwrap();
        let mut manifest = octosense_app_contract::AppManifest::parse(&json!({
            "schema":1,"id":id,"version":"1.0.0","name":"Fixture",
            "integrity":{"bundle_blake3":octosense_app_contract::digest_dir(&bundle).unwrap()}
        }).to_string()).unwrap();
        let publisher = HubKey::generate();
        octosense_app_hub::sign_manifest(&publisher, &mut manifest, "fixture").unwrap();
        std::fs::write(bundle.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
        let anchor = HubKey::generate();
        let working = HubKey::generate();
        let certificate = anchor.certify(&working.public_hex()).unwrap();
        let entry = Entry {manifest, listing:None, tools:vec![], artifact:"fixture".into(),
            publisher:"fixture".into(),publisher_key:publisher.public_hex(),
            source:Source {repository:String::new(),commit:String::new()},
            status:Status::Offered,admitted:"2026-10-06".into()};
        let mut catalog = Catalog::new(1,"2026-10-06",vec![entry]);
        let write = |catalog: &mut Catalog| {
            working.sign_catalog(catalog, &certificate).unwrap();
            std::fs::write(root.join("catalog.json"),serde_json::to_vec(catalog).unwrap()).unwrap();
        };
        write(&mut catalog);
        let check = || installed_bundle_with_anchor(&root,id,&anchor.public_hex());
        assert_eq!(check().unwrap(), bundle);
        // A v2 cache selects v2 verification for agents as well as the UI.
        // Neither an invalid proof nor a legacy document renamed to v2 may
        // fall back to the still-valid, offered legacy release beside it.
        let v2 = root.join("catalog-v2.json");
        for bytes in ["{}".to_string(), std::fs::read_to_string(root.join("catalog.json")).unwrap()] {
            std::fs::write(&v2, bytes).unwrap();
            assert!(check().unwrap_err().contains("catalog refused"));
        }
        std::fs::remove_file(v2).unwrap();
        assert_eq!(check().unwrap(), bundle);
        catalog.sequence += 1;
        catalog.entries[0].status = Status::Withdrawn("unsafe release".into());
        write(&mut catalog);
        assert!(check().unwrap_err().contains("withdrawn"), "the same cached app id must stop after catalog refresh");
        // A new offered release does not resurrect the withdrawn installed one.
        let mut update = catalog.entries[0].clone();
        update.manifest.version = "2.0.0".into(); update.status = Status::Offered;
        catalog.entries.push(update); catalog.sequence += 1; write(&mut catalog);
        assert!(check().unwrap_err().contains("withdrawn"));
        catalog.entries[0].status = Status::Offered; catalog.sequence += 1; write(&mut catalog);
        assert!(check().is_ok(), "an explicit signed restoration can run without changing personal consent");
        std::fs::write(bundle.join("main.splash"),"changed").unwrap();
        assert!(check().is_err(), "a manifest's self-asserted digest is not admission");
        std::fs::write(root.join("catalog.json"),"{}").unwrap();
        assert!(check().is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
