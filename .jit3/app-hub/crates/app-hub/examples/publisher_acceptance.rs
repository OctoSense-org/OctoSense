//! Exercise real publisher proofs without publishing or running app UI.
//!
//! cargo run --locked -p octosense-app-hub --example publisher_acceptance --
//!   first.bundle.pack.json [second.bundle.pack.json]
//!
//! Inputs must be real GitHub-attested releases of the same synthetic app.
//! The local test catalog uses ephemeral in-memory keys; it is NOT an
//! official v2 catalog proof or a catalog publication. No keys are saved.
use octosense_app_hub::{
    check_bundle, entry_for, github_publisher, publisher_tools, Catalog, CatalogPublishers, Entry,
    HubKey, PublisherKeys, Status, Store,
};
use octosense_app_policy::{AppManifest, HostLimits};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct OwnedDirectory(PathBuf);
impl Drop for OwnedDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn require(value: bool, message: &str) -> Result<(), String> {
    if value {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn read(bundle: &Path) -> Result<AppManifest, String> {
    AppManifest::parse(
        &fs::read_to_string(bundle.join("manifest.json")).map_err(|e| e.to_string())?,
    )
}
fn entry(bundle: &Path, previous: Option<&Catalog>) -> Result<Entry, String> {
    let m = read(bundle)?;
    let g = m
        .integrity
        .github
        .as_ref()
        .ok_or("missing real publisher proof")?;
    let report = check_bundle(
        bundle,
        &HostLimits::default(),
        &PublisherKeys::new(),
        previous,
    )?;
    if !report.passed() {
        return Err(report.render());
    }
    entry_for(
        bundle,
        &report,
        &format!("github:{}", g.repository_id),
        "",
        &g.repository_url(),
        &g.commit,
        &octosense_app_hub::today(),
    )
}
fn sign(catalog: &mut Catalog, anchor: &HubKey, working: &HubKey) -> Result<String, String> {
    working.sign_catalog(catalog, &anchor.certify(&working.public_hex())?)?;
    serde_json::to_string(catalog).map_err(|e| e.to_string())
}
fn run() -> Result<serde_json::Value, String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.is_empty() || args.len() > 2 {
        return Err("usage: publisher_acceptance <first pack> [second pack]".into());
    }
    let anchor = HubKey::generate();
    let working = HubKey::generate();
    let root = OwnedDirectory(std::env::temp_dir().join(format!(
        "octosense-publisher-acceptance-{}",
        &anchor.public_hex()[..24]
    )));
    fs::create_dir(&root.0).map_err(|e| e.to_string())?;
    let first = root.0.join("first");
    let second = root.0.join("second");
    publisher_tools::unpack(Path::new(&args[0]), None, &first)?;
    let one = entry(&first, None)?;
    let mut catalog = Catalog::new(1, &octosense_app_hub::today(), vec![one.clone()]);
    let first_json = sign(&mut catalog, &anchor, &working)?;
    let mut checks = vec!["real_publisher_proof"];
    let app_id = one.app_id();
    let mut store = Store::new(
        &anchor.public_hex(),
        &root.0.join("state"),
        HostLimits::default(),
    );
    store.accept_catalog(&first_json)?;
    store.install_staged(
        app_id,
        &first,
        &store.publisher_keys(),
        &octosense_app_hub::today(),
    )?;
    let prepared_one = store.prepare_launch(app_id)?;
    store.validate_prepared_launch(&prepared_one)?;
    require(
        serde_json::to_value(&prepared_one.manifest).map_err(|e| e.to_string())?
            == serde_json::to_value(&one.manifest).map_err(|e| e.to_string())?,
        "launch did not preserve the complete proof",
    )?;
    checks.push("install_and_verified_launch_preserve_proof");

    let installed = store.install_dir(app_id);
    let code = if installed.join("main.splash").exists() {
        installed.join("main.splash")
    } else {
        installed.join("page.card")
    };
    let original = fs::read(&code).map_err(|e| e.to_string())?;
    let mut altered = original.clone();
    altered.extend_from_slice(b"\n ");
    fs::write(&code, &altered).map_err(|e| e.to_string())?;
    require(
        store.may_run(app_id).is_err(),
        "changed installed content was runnable",
    )?;
    fs::write(code, original).map_err(|e| e.to_string())?;
    checks.push("installed_content_tamper_refused");

    let mut altered = one.manifest.clone();
    let signature = altered
        .integrity
        .github
        .as_mut()
        .and_then(|g| g.attestation.as_mut())
        .and_then(|a| a.pointer_mut("/dsseEnvelope/signatures/0/sig"))
        .ok_or("proof has no DSSE signature")?;
    let mut bytes = signature
        .as_str()
        .ok_or("signature is not text")?
        .as_bytes()
        .to_vec();
    require(!bytes.is_empty(), "signature is empty")?;
    bytes[0] = if bytes[0] == b'A' { b'B' } else { b'A' };
    *signature = serde_json::Value::String(String::from_utf8(bytes).map_err(|e| e.to_string())?);
    require(
        github_publisher::verify(
            altered.integrity.github.as_ref().unwrap(),
            &altered.signing_bytes()?,
        )
        .is_err(),
        "changed real proof was accepted",
    )?;
    let manifest_path = installed.join("manifest.json");
    let original = fs::read(&manifest_path).map_err(|e| e.to_string())?;
    fs::write(
        &manifest_path,
        serde_json::to_vec(&altered).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    require(
        store.prepare_launch(app_id).is_err(),
        "changed installed proof was launchable",
    )?;
    fs::write(&manifest_path, original).map_err(|e| e.to_string())?;
    checks.push("real_signature_and_installed_proof_tamper_refused");

    for field in [
        "repository",
        "repository_id",
        "owner_id",
        "workflow",
        "tag",
        "commit",
    ] {
        let mut m = one.manifest.clone();
        let g = m.integrity.github.as_mut().unwrap();
        match field {
            "repository" => g.repository = "other/fixture".into(),
            "repository_id" => g.repository_id = "999999999".into(),
            "owner_id" => g.owner_id = "999999999".into(),
            "workflow" => g.workflow = ".github/workflows/other.yml".into(),
            "tag" => {
                m.version = "999.0.0".into();
                g.tag = "v999.0.0".into();
            }
            _ => g.commit = "f".repeat(40),
        }
        require(
            github_publisher::verify(m.integrity.github.as_ref().unwrap(), &m.signing_bytes()?)
                .is_err(),
            "changed publisher identity accepted",
        )?;
    }
    checks.push("real_identity_substitution_refused");

    if args.len() == 1 {
        drop(prepared_one);
        let receipt = serde_json::json!({"schema":1,"passed":true,
            "publisher_proof":"real-github-tag-push",
            "catalog_authority":"ephemeral-local-test-catalog-not-official-v2",
            "app_id":app_id,"versions":[one.version()],"checks":checks,
            "update_tested":false,"production_publication":false,"owned_directory_cleanup":true});
        fs::remove_dir_all(&root.0).map_err(|_| "owned acceptance directory cleanup failed")?;
        return Ok(receipt);
    }
    publisher_tools::unpack(Path::new(&args[1]), None, &second)?;
    let two = entry(&second, Some(&catalog))?;
    require(
        one.app_id() == two.app_id(),
        "fixture packs must be the same app",
    )?;
    checks.push("second_real_publisher_proof_and_version_update_continuity");
    catalog.sequence = 2;
    catalog.entries.push(two.clone());
    let second_json = sign(&mut catalog, &anchor, &working)?;
    store.accept_catalog(&second_json)?;
    store.install_staged(
        app_id,
        &second,
        &store.publisher_keys(),
        &octosense_app_hub::today(),
    )?;
    let prepared_two = store.prepare_launch(app_id)?;
    store.validate_prepared_launch(&prepared_two)?;
    require(
        store.accept_catalog(&first_json).is_err(),
        "catalog rollback accepted",
    )?;
    require(
        !check_bundle(
            &first,
            &HostLimits::default(),
            &PublisherKeys::new(),
            Some(&catalog),
        )?
        .passed(),
        "old version republish accepted",
    )?;
    let mut reversed = catalog.clone();
    reversed.entries.reverse();
    require(
        CatalogPublishers::from_catalog(&reversed).is_err(),
        "publisher version rollback history accepted",
    )?;
    checks.push("update_installs_and_catalog_version_rollback_refused");

    let mut unsigned = two.manifest.clone();
    unsigned.version = "999.0.0".into();
    unsigned.integrity.github = None;
    unsigned.requires.retain(|f| f != "publisher-github-v1");
    let source_manifest = second.join("manifest.json");
    let original = fs::read(&source_manifest).map_err(|e| e.to_string())?;
    fs::write(
        &source_manifest,
        serde_json::to_vec(&unsigned).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    require(
        !check_bundle(
            &second,
            &HostLimits::default().with_require_signature(false),
            &PublisherKeys::new(),
            Some(&catalog),
        )?
        .passed(),
        "unsigned mode adopted keyless ownership",
    )?;
    fs::write(source_manifest, original).map_err(|e| e.to_string())?;
    checks.push("unsigned_update_escape_refused");

    catalog.sequence = 3;
    catalog.entries[0].status = Status::Withdrawn("synthetic acceptance withdrawal".into());
    store.accept_catalog(&sign(&mut catalog, &anchor, &working)?)?;
    require(
        store.validate_prepared_launch(&prepared_one).is_err(),
        "withdrawn prepared first release accepted",
    )?;
    store.validate_prepared_launch(&prepared_two)?;
    catalog.sequence = 4;
    catalog.entries[1].status = Status::Withdrawn("synthetic acceptance withdrawal".into());
    store.accept_catalog(&sign(&mut catalog, &anchor, &working)?)?;
    require(
        store.may_run(app_id).is_err() && store.validate_prepared_launch(&prepared_two).is_err(),
        "withdrawn installed or prepared release accepted",
    )?;
    checks.push("withdrawal_revokes_installed_and_prepared_releases");
    drop(prepared_one);
    drop(prepared_two);
    let receipt = serde_json::json!({"schema":1,"passed":true,"publisher_proof":"real-github-tag-push",
        "catalog_authority":"ephemeral-local-test-catalog-not-official-v2","app_id":app_id,
        "versions":[one.version(),two.version()],"checks":checks,"update_tested":true,"production_publication":false,"owned_directory_cleanup":true});
    fs::remove_dir_all(&root.0).map_err(|_| "owned acceptance directory cleanup failed")?;
    Ok(receipt)
}
fn main() {
    match run() {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("publisher acceptance: {error}");
            std::process::exit(1);
        }
    }
}
