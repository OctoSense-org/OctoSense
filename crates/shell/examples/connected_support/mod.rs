//! Development-only signed fixtures for the real installed-app launch path.
//! Private signing keys never leave memory. Provider credentials are neither
//! created nor copied; the ordinary host services keep their normal gates.
use octosense_app_hub::{
    check_bundle, entry_for, sign_manifest, Catalog, HubKey, PreparedLaunch, PublisherKeys, Store,
};
use octosense_app_policy::{digest_dir, AppManifest, HostLimits};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const PUBLISHER: &str = "connected-e2e-fixture";
const IDS: &[&str] = &[
    #[cfg(feature = "acceptance-fixtures")]
    "org.octosense.samples.backend",
    #[cfg(feature = "acceptance-fixtures")]
    "org.octosense.samples.backendother",
    "org.octosense.samples.githubnotes",
    "org.octosense.samples.inbox",
    "org.octosense.samples.googlecalendar",
];

fn copy_bundle(from: &Path, to: &Path) -> Result<(), String> {
    if !fs::symlink_metadata(from)
        .map_err(|e| e.to_string())?
        .is_dir()
    {
        return Err("Fixture source must be a directory, not a symlink".into());
    }
    fs::create_dir(to).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            copy_bundle(&entry.path(), &to.join(entry.file_name()))?;
        } else if kind.is_file() {
            fs::copy(entry.path(), to.join(entry.file_name())).map_err(|e| e.to_string())?;
        } else {
            return Err("Fixture refuses symlinks and special files".into());
        }
    }
    Ok(())
}

/// Populate a new isolated apps root. The caller owns cleanup after acceptance.
pub fn install(inputs: &[PathBuf], root: &Path) -> Result<Value, String> {
    if inputs.is_empty() {
        return Err("Pass at least one connected sample bundle".into());
    }
    if root.exists() {
        if fs::symlink_metadata(root)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
            || fs::read_dir(root)
                .map_err(|e| e.to_string())?
                .next()
                .is_some()
        {
            return Err(
                "Fixture installation requires a new or empty isolated app-data directory".into(),
            );
        }
    } else {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(root).map_err(|e| e.to_string())?;
    }
    let staging = root.join(".connected-staging");
    fs::create_dir(&staging).map_err(|e| e.to_string())?;
    let result = (|| {
        let publisher = HubKey::generate();
        let keys = PublisherKeys::new().with(PUBLISHER, &publisher.public_hex());
        let anchor = HubKey::generate();
        let working = HubKey::generate();
        let today = octosense_app_hub::today();
        let limits = HostLimits::default();
        if !limits.require_signature {
            return Err("Signed fixture admission is required".into());
        }
        let mut entries = Vec::new();
        let mut copies = Vec::new();
        let mut seen = BTreeSet::new();
        for source in inputs {
            let original =
                fs::read_to_string(source.join("manifest.json")).map_err(|e| e.to_string())?;
            let mut manifest = AppManifest::parse(&original)?;
            if !IDS.contains(&manifest.id.as_str()) || !seen.insert(manifest.id.clone()) {
                return Err("Expected distinct ordinary connected sample identities".into());
            }
            let digest = digest_dir(source)?;
            if manifest.integrity.bundle_blake3 != digest {
                return Err("Stamp the source bundle before fixture installation".into());
            }
            let copy = staging.join(&manifest.id);
            copy_bundle(source, &copy)?;
            sign_manifest(&publisher, &mut manifest, PUBLISHER)?;
            fs::write(
                copy.join("manifest.json"),
                serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let report = check_bundle(&copy, &limits, &keys, None)?;
            if !report.passed() {
                return Err(report.render());
            }
            entries.push(entry_for(
                &copy,
                &report,
                PUBLISHER,
                &publisher.public_hex(),
                "",
                "",
                &today,
            )?);
            copies.push((source.clone(), original, digest, manifest.id, copy));
        }
        let mut catalog = Catalog::new(1, &today, entries);
        working.sign_catalog(&mut catalog, &anchor.certify(&working.public_hex())?)?;
        let catalog_json = serde_json::to_string_pretty(&catalog).map_err(|e| e.to_string())?;
        let mut store = Store::new(&anchor.public_hex(), root, limits);
        store.accept_catalog(&catalog_json)?;
        let mut apps = Vec::new();
        for (source, original, digest, id, copy) in copies {
            let policy = store.install_staged(&id, &copy, &keys, &today)?;
            let prepared = store.prepare_launch(&id)?;
            store.validate_prepared_launch(&prepared)?;
            if prepared.policy != policy {
                return Err("Installed launch changed resolved grants".into());
            }
            if fs::read_to_string(source.join("manifest.json")).map_err(|e| e.to_string())?
                != original
                || digest_dir(&source)? != digest
            {
                return Err("Source bundle changed during installation".into());
            }
            apps.push(json!({"id":id,"bundle_digest":digest,"signed_install":true,"prepared_launch_verified":true}));
        }
        fs::write(root.join("catalog.json"), catalog_json).map_err(|e| e.to_string())?;
        let receipt = json!({"schema":1,"fixture":"connected-e2e","anchor":anchor.public_hex(),
            "apps":apps,"private_keys":"ephemeral memory only","provider_credentials":"not created or copied"});
        fs::write(
            root.join(".connected-e2e.json"),
            serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(receipt)
    })();
    fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    result
}

/// Reopen only a signed release from this explicitly owned fixture profile.
#[allow(dead_code)] // Shared with the install-only CLI, which does not launch UI.
pub fn open(root: &Path, id: &str) -> Result<PreparedLaunch, String> {
    let metadata: Value = serde_json::from_slice(
        &fs::read(root.join(".connected-e2e.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if metadata["schema"] != 1 || metadata["fixture"] != "connected-e2e" || !IDS.contains(&id) {
        return Err("Not a supported connected acceptance profile".into());
    }
    let anchor = metadata["anchor"]
        .as_str()
        .ok_or("Fixture public trust anchor is absent")?;
    let mut store = Store::new(anchor, root, HostLimits::default());
    store.accept_catalog(
        &fs::read_to_string(root.join("catalog.json")).map_err(|e| e.to_string())?,
    )?;
    let prepared = store.prepare_launch(id)?;
    store.validate_prepared_launch(&prepared)?;
    Ok(prepared)
}
