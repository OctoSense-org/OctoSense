//! Signed installation proof for the three ordinary connected samples.
//! All keys stay in memory; only temporary copies are signed or mutated.
//! Pass the GitHub Notes, Inbox and Google Calendar bundle directories in any order.
use octosense_app_hub::{
    check_bundle, entry_for, sign_manifest, Catalog, HubKey, PublisherKeys, Store,
};
use octosense_app_policy::{digest_dir, AgentBundle, AppManifest, AppPolicy, HostLimits};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
mod connected_support;

const IDS: [&str; 3] = [
    "org.octosense.samples.githubnotes",
    "org.octosense.samples.inbox",
    "org.octosense.samples.googlecalendar",
];
const PUBLISHER: &str = "connected-install-fixture";

struct PrivateRoot(PathBuf);
impl PrivateRoot {
    fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "octosense-connected-install-{}",
            uuid::Uuid::new_v4()
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
    fn cleanup(&self) -> Result<(), String> {
        fs::remove_dir_all(&self.0)
            .map_err(|_| "Temporary install fixture cleanup failed".to_string())?;
        ensure(!self.0.exists(), "Temporary install fixture remains")
    }
}
impl Drop for PrivateRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn copy_bundle(from: &Path, to: &Path) -> Result<(), String> {
    if !fs::symlink_metadata(from)
        .map_err(|e| e.to_string())?
        .is_dir()
    {
        return Err("Bundle input must be a directory, not a symlink".into());
    }
    fs::create_dir(to).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let destination = to.join(entry.file_name());
        if kind.is_dir() {
            copy_bundle(&entry.path(), &destination)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), destination).map_err(|e| e.to_string())?;
        } else {
            return Err("Bundle fixture refuses symlinks and special files".into());
        }
    }
    Ok(())
}
fn ensure(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn expected_capabilities(id: &str) -> BTreeSet<String> {
    let capabilities: &[&str] = match id {
        "org.octosense.samples.githubnotes" => &["storage", "auth", "github"],
        "org.octosense.samples.inbox" => &[
            "storage",
            "auth",
            "gmail",
            "model",
            "glance",
            "octos.session.open",
            "octos.turn.start",
        ],
        "org.octosense.samples.googlecalendar" => &[
            "storage",
            "auth",
            "gcalendar",
            "glance",
            "octos.session.open",
            "octos.turn.start",
        ],
        _ => &[],
    };
    capabilities.iter().map(|s| (*s).to_owned()).collect()
}
fn check_grants(policy: &AppPolicy, id: &str) -> Result<(), String> {
    ensure(policy.app_id == id, "Installed policy changed app identity")?;
    ensure(
        policy.capabilities == expected_capabilities(id),
        "Unexpected connected-app capability grant",
    )?;
    ensure(
        policy.hosts.is_empty(),
        "Connected samples must not receive direct network hosts",
    )?;
    ensure(
        policy.storage.accounts,
        "Connected samples require account-scoped agents",
    )?;
    if let Some(agent) = &policy.agent {
        ensure(
            agent.profile.as_kernel_mode() == "read-only",
            "Unexpected peer profile",
        )?;
        ensure(
            agent.background_requested == (id == IDS[1]),
            "Unexpected background request",
        )?;
    } else {
        ensure(
            id == IDS[0],
            "Calendar and Inbox require admitted agent declarations",
        )?;
    }
    Ok(())
}
fn run(inputs: Vec<PathBuf>) -> Result<(), String> {
    ensure(
        inputs.len() == 3,
        "Pass exactly three sample bundle directories",
    )?;
    let root = PrivateRoot::new()?;
    let limits = HostLimits::default();
    ensure(
        limits.require_signature,
        "This proof requires signed admission",
    )?;
    let publisher = HubKey::generate();
    let keys = PublisherKeys::new().with(PUBLISHER, &publisher.public_hex());
    let anchor = HubKey::generate();
    let working = HubKey::generate();
    let today = octosense_app_hub::today();
    let mut originals = Vec::new();
    let mut staged = BTreeMap::new();
    let mut entries = Vec::new();
    for original in inputs {
        let original_manifest =
            fs::read_to_string(original.join("manifest.json")).map_err(|e| e.to_string())?;
        let mut manifest = AppManifest::parse(&original_manifest)?;
        ensure(
            IDS.contains(&manifest.id.as_str()),
            "Only the three ordinary sample IDs are accepted",
        )?;
        ensure(!staged.contains_key(&manifest.id), "Duplicate sample ID")?;
        let original_digest = digest_dir(&original)?;
        let copy = root.0.join(&manifest.id);
        copy_bundle(&original, &copy)?;
        manifest.integrity.bundle_blake3 = digest_dir(&copy)?;
        sign_manifest(&publisher, &mut manifest, PUBLISHER)?;
        fs::write(
            copy.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let report = check_bundle(&copy, &limits, &keys, None)?;
        ensure(report.passed(), &report.render())?;
        let admitted = report
            .policy
            .as_ref()
            .ok_or("Signed gate returned no policy")?
            .clone();
        check_grants(&admitted, &manifest.id)?;
        entries.push(entry_for(
            &copy,
            &report,
            PUBLISHER,
            &publisher.public_hex(),
            "",
            "",
            &today,
        )?);
        staged.insert(manifest.id.clone(), (copy, manifest, admitted));
        originals.push((original, original_manifest, original_digest));
    }
    let mut catalog = Catalog::new(1, &today, entries);
    working.sign_catalog(&mut catalog, &anchor.certify(&working.public_hex())?)?;
    let catalog_json = serde_json::to_string(&catalog).map_err(|e| e.to_string())?;
    let device = root.0.join("fresh-profile");
    let mut store = Store::new(&anchor.public_hex(), &device, limits.clone());
    store.accept_catalog(&catalog_json)?;
    // These are real signature checks, not an unsigned development path.
    let mut wrong_anchor = Store::new(
        &HubKey::generate().public_hex(),
        &root.0.join("wrong-anchor"),
        limits.clone(),
    );
    ensure(
        wrong_anchor.accept_catalog(&catalog_json).is_err(),
        "Untrusted catalog anchor was accepted",
    )?;
    let mut unsigned = catalog.clone();
    unsigned.signature = None;
    let mut unsigned_store = Store::new(
        &anchor.public_hex(),
        &root.0.join("unsigned"),
        limits.clone(),
    );
    ensure(
        unsigned_store
            .accept_catalog(&serde_json::to_string(&unsigned).map_err(|e| e.to_string())?)
            .is_err(),
        "Unsigned catalog was accepted",
    )?;

    let mut jails = BTreeSet::new();
    let mut results = Vec::new();
    for id in IDS {
        let (bundle, manifest, admitted) = &staged[id];
        let installed = store.install_staged(id, bundle, &keys, &today)?;
        ensure(
            &installed == admitted,
            "Installation changed admitted grants",
        )?;
        ensure(
            store.may_run(id)? == installed,
            "Launch changed installed grants",
        )?;
        check_grants(&installed, id)?;
        let jail = installed.jail_root(&device);
        ensure(
            jails.insert(jail.clone()),
            "Two sample apps share a storage jail",
        )?;
        ensure(
            !store.install_dir(id).starts_with(&jail),
            "Executable bundle is inside writable app storage",
        )?;
        AgentBundle::load(&store.install_dir(id), manifest)?;
        let prepared = store.prepare_launch(id)?;
        store.validate_prepared_launch(&prepared)?;
        ensure(
            prepared.policy == installed,
            "Prepared launch changed installed grants",
        )?;
        // Reopen a new Store object against the same independently signed catalog.
        let mut reopened = Store::new(&anchor.public_hex(), &device, limits.clone());
        reopened.accept_catalog(&catalog_json)?;
        ensure(
            reopened.may_run(id)? == installed,
            "Fresh Store cannot reopen signed install",
        )?;
        let installed_source = store.install_dir(id).join("main.splash");
        let source = fs::read(&installed_source).map_err(|e| e.to_string())?;
        let mut tampered = source.clone();
        tampered.extend_from_slice(b"\n// deliberate fixture tamper\n");
        fs::write(&installed_source, tampered).map_err(|e| e.to_string())?;
        ensure(
            store.may_run(id).is_err(),
            "Modified installed source was accepted",
        )?;
        fs::write(&installed_source, &source).map_err(|e| e.to_string())?;
        ensure(
            store.may_run(id)? == installed,
            "Restored original source did not verify",
        )?;
        let other = IDS.iter().find(|other| **other != id).unwrap();
        ensure(
            store
                .install_staged(id, &staged[*other].0, &keys, &today)
                .is_err(),
            "Another app's signed bundle was accepted for this identity",
        )?;
        ensure(
            store.may_run(id)? == installed,
            "Refused cross-app install damaged current release",
        )?;
        results.push(
            json!({"app":id,"bundle_digest":manifest.integrity.bundle_blake3,
            "signed_install":true,"fresh_store_reopen":true,"capabilities":installed.capabilities,
            "accounts":installed.storage.accounts,"direct_network_hosts":installed.hosts,
            "modified_source_refused":true,"cross_app_staging_refused":true}),
        );
    }
    for (path, manifest, digest) in originals {
        ensure(
            fs::read_to_string(path.join("manifest.json")).map_err(|e| e.to_string())? == manifest,
            "Original input manifest changed",
        )?;
        ensure(
            digest_dir(&path)? == digest,
            "Original input bundle changed",
        )?;
    }
    root.cleanup()?;
    println!("{}", serde_json::to_string_pretty(&json!({
        "result":"pass", "proof":"private signed catalog installation", "apps":results,
        "untrusted_anchor_refused":true,"unsigned_catalog_refused":true,"separate_storage_jails":true,
        "input_bundles_unchanged":true,"production_catalog_modified":false,
        "keys":"ephemeral memory only", "cleanup":"temporary fixture removed and checked",
        "not_verified":["public catalog publication", "provider OAuth", "live provider reads/writes", "Android runtime"]
    })).map_err(|e|e.to_string())?);
    Ok(())
}
fn main() {
    let inputs: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if let Some(profile) = inputs.first().and_then(|arg| arg.to_str()).and_then(|arg| arg.strip_prefix("--keep-profile=")) {
        match connected_support::install(&inputs[1..], Path::new(profile)) {
            Ok(receipt) => println!("{}", serde_json::to_string_pretty(&receipt).unwrap()),
            Err(error) => { eprintln!("connected-install: {error}"); std::process::exit(1); }
        }
        return;
    }
    if let Err(error) = run(inputs) {
        eprintln!("connected-install: {error}");
        std::process::exit(1);
    }
}
