//! Publisher authoring tools. Outputs are files for review, never publication
//! or trusted runtime state; no command executes contributor code.
use crate::{
    admission, github_catalog::AuthenticatedCatalog, github_publisher, CatalogPublishers,
    GateReport, PublisherKeys,
};
use octosense_app_policy::{manifest::GithubPublisher, AppManifest, HostLimits, MANIFEST_FILE};
use sha2::{Digest, Sha256};
use std::path::Path;

fn manifest(bundle: &Path) -> Result<AppManifest, String> {
    AppManifest::parse(&admission::read_text(
        &bundle.join(MANIFEST_FILE),
        admission::MAX_MANIFEST_BYTES,
    )?)
}
fn write_manifest(bundle: &Path, m: &AppManifest) -> Result<(), String> {
    if !m.ignored_fields().is_empty() {
        return Err(
            "publisher tool must understand every manifest field before rewriting it".into(),
        );
    }
    let bytes = serde_json::to_vec_pretty(m).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > admission::MAX_MANIFEST_BYTES {
        return Err("sealed manifest exceeds 64 KiB".into());
    }
    crate::github_catalog::write_atomic(&bundle.join(MANIFEST_FILE), &bytes)
}
fn outside(bundle: &Path, output: &Path) -> Result<(), String> {
    let root = bundle.canonicalize().map_err(|e| e.to_string())?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent
        .canonicalize()
        .map_err(|_| "output parent must already exist")?;
    if parent.starts_with(root) {
        return Err("publisher output must be outside the bundle".into());
    }
    if std::fs::symlink_metadata(output).is_ok() {
        return Err("publisher output already exists; refusing to overwrite".into());
    }
    Ok(())
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .and_then(|mut f| f.write_all(bytes))
        .map_err(|e| format!("cannot create publisher output: {e}"))
}
pub fn prepare(
    bundle: &Path,
    mut identity: GithubPublisher,
    output: &Path,
) -> Result<serde_json::Value, String> {
    outside(bundle, output)?;
    if output.file_name().and_then(|v| v.to_str()) != Some(github_publisher::PUBLISHER_SUBJECT) {
        return Err("publisher subject filename must be octosense-app-manifest.json".into());
    }
    admission::inventory(bundle)?;
    let mut m = manifest(bundle)?;
    if m.integrity.signature.is_some()
        || m.integrity
            .github
            .as_ref()
            .is_some_and(|g| g.attestation.is_some())
    {
        return Err(
            "sealed release cannot be prepared or restamped; prepare an unsigned new version"
                .into(),
        );
    }
    identity.attestation = None;
    identity.validate(&m.version)?;
    m.integrity.github = Some(identity);
    if !m.requires.iter().any(|f| f == "publisher-github-v1") {
        m.requires.push("publisher-github-v1".into());
    }
    m.integrity.bundle_blake3 = octosense_app_policy::digest_dir(bundle)?;
    m.check_requires()?;
    let bytes = m.signing_bytes()?;
    if bytes.len() as u64 > admission::MAX_MANIFEST_BYTES {
        return Err("publisher subject exceeds byte limit".into());
    }
    write_new(output, &bytes)?;
    write_manifest(bundle, &m)?;
    Ok(
        serde_json::json!({"schema":1,"subject":github_publisher::PUBLISHER_SUBJECT,"sha256":hex::encode(Sha256::digest(&bytes)),
        "app_id":m.id,"version":m.version,"bundle_blake3":m.integrity.bundle_blake3,"status":"awaiting-github-attestation"}),
    )
}
pub fn attach(bundle: &Path, proof: &Path) -> Result<serde_json::Value, String> {
    let mut m = manifest(bundle)?;
    if octosense_app_policy::digest_dir(bundle)? != m.integrity.bundle_blake3 {
        return Err("bundle changed after publisher preparation".into());
    }
    let g = m
        .integrity
        .github
        .as_mut()
        .ok_or("run publisher-prepare before attaching a proof")?;
    if g.attestation.is_some() {
        return Err("publisher proof is already attached".into());
    }
    let bytes = admission::read_bounded(proof, github_publisher::MAX_PROOF_BYTES as u64)?;
    g.attestation =
        Some(serde_json::from_slice(&bytes).map_err(|_| "invalid publisher attestation JSON")?);
    m.check_requires()?;
    github_publisher::verify(m.integrity.github.as_ref().unwrap(), &m.signing_bytes()?)?;
    write_manifest(bundle, &m)?;
    Ok(
        serde_json::json!({"schema":1,"app_id":m.id,"version":m.version,"status":"publisher-proof-verified"}),
    )
}
pub fn verify(bundle: &Path, base: Option<&AuthenticatedCatalog>) -> Result<GateReport, String> {
    let m = manifest(bundle)?;
    if m.integrity.github.is_none() {
        return Err("publisher command requires publisher-github-v1".into());
    }
    let keys = base
        .map(|b| b.publishers().trusted_keys(PublisherKeys::new()))
        .unwrap_or_default();
    let report = match base {
        Some(base) => crate::gate::check_bundle_with_registry(
            bundle,
            &HostLimits::default(),
            &keys,
            base.catalog(),
            base.publishers(),
        )?,
        None => crate::check_bundle(bundle, &HostLimits::default(), &keys, None)?,
    };
    if !report.passed() {
        return Err(report.render());
    }
    Ok(report)
}
pub fn pack(
    bundle: &Path,
    base: Option<&AuthenticatedCatalog>,
    output: &Path,
) -> Result<serde_json::Value, String> {
    outside(bundle, output)?;
    let report = verify(bundle, base)?;
    let pack = crate::pack_dir(bundle)?;
    // Verify after the read too: a race must not produce an unverified pack.
    let staged = std::env::temp_dir().join(format!("octosense-publisher-pack-{}", random_name()?));
    std::fs::create_dir(&staged).map_err(|_| "cannot allocate new publisher staging directory")?;
    let result = (|| {
        crate::unpack(&pack, &staged)?;
        let again = verify(&staged, base)?;
        if again.digest != report.digest || again.admitted_manifest != report.admitted_manifest {
            return Err("bundle changed during packing".into());
        }
        let bytes = serde_json::to_vec(&pack).map_err(|e| e.to_string())?;
        write_new(output, &bytes)?;
        Ok(
            serde_json::json!({"schema":1,"app_id":report.app_id,"version":report.version,"bundle_blake3":report.digest,
            "pack_sha256":hex::encode(Sha256::digest(&bytes)),"status":"verified-pack-not-published"}),
        )
    })();
    let _ = std::fs::remove_dir_all(&staged);
    result
}
fn random_name() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    rand_core::TryRngCore::try_fill_bytes(&mut rand_core::OsRng, &mut bytes)
        .map_err(|_| "cannot allocate staging directory")?;
    Ok(hex::encode(bytes))
}
/// Build a review candidate without publishing it or changing installed ownership.
pub fn entry(bundle: &Path, base: &AuthenticatedCatalog, output: &Path) -> Result<(), String> {
    outside(bundle, output)?;
    let m = manifest(bundle)?;
    let g = m
        .integrity
        .github
        .as_ref()
        .ok_or("entry requires a GitHub publisher")?;
    let report = verify(bundle, Some(base))?;
    let publisher = format!("github:{}", g.repository_id);
    let entry = crate::entry_for(
        bundle,
        &report,
        &publisher,
        "",
        &g.repository_url(),
        &g.commit,
        &crate::today(),
    )?;
    let mut candidate = base.catalog().clone();
    candidate.key = None;
    candidate.signature = None;
    candidate.entries.push(entry.clone());
    CatalogPublishers::from_catalog(&candidate)?;
    write_new(
        output,
        &serde_json::to_vec_pretty(&entry).map_err(|e| e.to_string())?,
    )
}

/// Extract a downloaded release into a newly owned directory and admit its
/// exact final bytes. Failed verification removes only this new directory.
pub fn unpack(
    pack_path: &Path,
    base: Option<&AuthenticatedCatalog>,
    output: &Path,
) -> Result<GateReport, String> {
    let bytes =
        admission::read_bounded(pack_path, crate::github_catalog::MAX_DOCUMENT_BYTES as u64)?;
    let pack: crate::Pack =
        serde_json::from_slice(&bytes).map_err(|_| "invalid publisher pack JSON")?;
    std::fs::create_dir(output).map_err(|_| "publisher-unpack needs a new output directory")?;
    let result = (|| {
        crate::unpack(&pack, output)?;
        verify(output, base)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(output);
    }
    result
}
