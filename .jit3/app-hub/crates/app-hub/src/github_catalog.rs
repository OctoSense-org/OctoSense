//! GitHub-admin catalog provenance, separate from the legacy Ed25519 channel.
//!
//! A device authorizes one compiled workflow identity. The Sigstore library
//! verifies certificates and transparency evidence; this module checks the
//! authenticated GitHub claims and the exact catalog subject. It neither
//! authenticates to GitHub nor invokes a command-line verifier.
use crate::{Catalog, CatalogPublishers};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sigstore_trust_root::{SigstoreInstance, TrustedRoot};
use sigstore_types::{Bundle, MediaType, SignatureContent};
use sigstore_verify::{VerificationPolicy, Verifier};
use std::path::Path;

pub const MAX_CATALOG_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PROOF_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_DOCUMENT_BYTES: usize = 14 * 1024 * 1024;
pub const CATALOG_SUBJECT: &str = "catalog-v2.payload.json";
pub const GITHUB_ISSUER: &str = "https://token.actions.githubusercontent.com";
pub const GITHUB_WORKFLOW: &str = "https://github.com/OctoSense-org/OctoSense-App-Hub/.github/workflows/publish-catalog.yml@refs/heads/main";
const REPOSITORY: &str = "https://github.com/OctoSense-org/OctoSense-App-Hub";
const OWNER: &str = "https://github.com/OctoSense-org";
const REPOSITORY_ID: &str = "1378439410";
const OWNER_ID: &str = "328148893";
const ENVIRONMENT: &str = "app-hub-catalog";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: u32,
    kind: String,
    /// Base64 preserves the exact attested file bytes, including whitespace.
    catalog: String,
    attestation: serde_json::Value,
}

/// A catalog whose bytes and approved GitHub workflow provenance both verified.
/// Fields are private so a caller cannot manufacture successful verification.
pub struct VerifiedCatalog {
    catalog: Catalog,
    digest: String,
}
impl VerifiedCatalog {
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }
    pub fn sha256(&self) -> &str {
        &self.digest
    }
    pub(crate) fn into_catalog(self) -> Catalog {
        self.catalog
    }
}

fn parse_catalog(bytes: &[u8]) -> Result<Catalog, String> {
    if bytes.len() > MAX_CATALOG_BYTES {
        return Err("catalog exceeds byte limit".into());
    }
    let catalog: Catalog =
        serde_json::from_slice(bytes).map_err(|_| "catalog payload is invalid JSON")?;
    if catalog.schema != crate::CATALOG_SCHEMA
        || catalog.key.is_some()
        || catalog.signature.is_some()
    {
        return Err(
            "GitHub catalog payload must use schema 1 without a legacy key or signature".into(),
        );
    }
    if catalog.entries.len() > 4096 {
        return Err("catalog exceeds entry limit".into());
    }
    CatalogPublishers::from_catalog(&catalog)?;
    let mut versions = std::collections::BTreeSet::new();
    for entry in &catalog.entries {
        if !versions.insert((entry.app_id(), entry.version())) {
            return Err("catalog repeats an app version".into());
        }
    }
    crate::components::check_catalog(&catalog)?;
    Ok(catalog)
}

fn require_claim(actual: Option<&str>, expected: &str, label: &str) -> Result<(), String> {
    if actual != Some(expected) {
        return Err(format!("GitHub catalog has unauthorized {label}"));
    }
    Ok(())
}

fn authorize_claims(claims: &sigstore_verify::crypto::FulcioCiClaims) -> Result<(), String> {
    for (actual, expected, name) in [
        (
            claims.build_signer_uri.as_deref(),
            GITHUB_WORKFLOW,
            "signer workflow",
        ),
        (
            claims.source_repository_uri.as_deref(),
            REPOSITORY,
            "repository",
        ),
        (
            claims.source_repository_identifier.as_deref(),
            REPOSITORY_ID,
            "repository id",
        ),
        (
            claims.source_repository_owner_uri.as_deref(),
            OWNER,
            "owner",
        ),
        (
            claims.source_repository_owner_identifier.as_deref(),
            OWNER_ID,
            "owner id",
        ),
        (
            claims.source_repository_ref.as_deref(),
            "refs/heads/main",
            "source ref",
        ),
        (
            claims.build_config_uri.as_deref(),
            GITHUB_WORKFLOW,
            "workflow",
        ),
        (
            claims.build_trigger.as_deref(),
            "workflow_dispatch",
            "trigger",
        ),
        (
            claims.runner_environment.as_deref(),
            "github-hosted",
            "runner",
        ),
        (
            claims.source_repository_visibility_at_signing.as_deref(),
            "public",
            "visibility",
        ),
        (
            claims.deployment_environment.as_deref(),
            ENVIRONMENT,
            "deployment environment",
        ),
    ] {
        require_claim(actual, expected, name)?;
    }
    Ok(())
}

pub(crate) fn verify_proof(
    payload: &[u8],
    bundle: &Bundle,
    root: &TrustedRoot,
    policy: &VerificationPolicy,
) -> Result<sigstore_verify::VerificationResult, String> {
    if bundle.media_type != MediaType::Bundle0_3 {
        return Err("catalog requires a Sigstore v0.3 bundle".into());
    }
    let verifier = Verifier::new(root).map_err(|_| "catalog trust root is invalid")?;
    let result = verifier
        .verify(payload, bundle, policy)
        .map_err(|_| "catalog attestation cryptographic verification failed")?;
    if !result.certificate_verified()
        || !result.sct_verified()
        || !result.tlog_verified()
        || !result.identity_policy_checked()
    {
        return Err("catalog attestation was not fully verified".into());
    }
    Ok(result)
}

fn verify_subject(bundle: &Bundle, digest: &str) -> Result<(), String> {
    let SignatureContent::DsseEnvelope(envelope) = &bundle.content else {
        return Err("catalog requires an in-toto attestation".into());
    };
    let statement: serde_json::Value = serde_json::from_slice(envelope.payload.as_bytes())
        .map_err(|_| "invalid signed statement")?;
    if statement.get("_type").and_then(|v| v.as_str()) != Some("https://in-toto.io/Statement/v1")
        || statement.get("predicateType").and_then(|v| v.as_str())
            != Some("https://slsa.dev/provenance/v1")
    {
        return Err("unexpected catalog attestation statement type".into());
    }
    let subjects = statement
        .get("subject")
        .and_then(|v| v.as_array())
        .ok_or("missing catalog subject")?;
    if subjects.len() != 1
        || subjects[0].get("name").and_then(|v| v.as_str()) != Some(CATALOG_SUBJECT)
        || subjects[0]
            .pointer("/digest/sha256")
            .and_then(|v| v.as_str())
            != Some(digest)
    {
        return Err("attestation subject does not name the exact catalog bytes".into());
    }
    // Match the maintained actions/attest SLSA workflow/v1 predicate. Its
    // builder is the job workflow URI, not merely the generic runner family.
    for (pointer, expected) in [
        ("/predicate/runDetails/builder/id", GITHUB_WORKFLOW),
        (
            "/predicate/buildDefinition/buildType",
            "https://actions.github.io/buildtypes/workflow/v1",
        ),
        (
            "/predicate/buildDefinition/externalParameters/workflow/repository",
            REPOSITORY,
        ),
        (
            "/predicate/buildDefinition/externalParameters/workflow/ref",
            "refs/heads/main",
        ),
        (
            "/predicate/buildDefinition/externalParameters/workflow/path",
            ".github/workflows/publish-catalog.yml",
        ),
        (
            "/predicate/buildDefinition/internalParameters/github/event_name",
            "workflow_dispatch",
        ),
        (
            "/predicate/buildDefinition/internalParameters/github/repository_id",
            REPOSITORY_ID,
        ),
        (
            "/predicate/buildDefinition/internalParameters/github/repository_owner_id",
            OWNER_ID,
        ),
        (
            "/predicate/buildDefinition/internalParameters/github/runner_environment",
            "github-hosted",
        ),
    ] {
        if statement.pointer(pointer).and_then(|v| v.as_str()) != Some(expected) {
            return Err("catalog attestation has an unauthorized workflow predicate".into());
        }
    }
    Ok(())
}

/// Verify exact prepared bytes and their complete public Sigstore proof offline.
pub fn verify_parts(payload: &[u8], proof: &[u8]) -> Result<VerifiedCatalog, String> {
    let catalog = parse_catalog(payload)?;
    if proof.len() > MAX_PROOF_BYTES {
        return Err("catalog proof exceeds byte limit".into());
    }
    let bundle: Bundle =
        serde_json::from_slice(proof).map_err(|_| "invalid Sigstore catalog bundle")?;
    let root = TrustedRoot::from_embedded(SigstoreInstance::PublicGood)
        .map_err(|_| "catalog trust snapshot is invalid")?;
    let policy = VerificationPolicy::new(GITHUB_WORKFLOW, GITHUB_ISSUER);
    let result = verify_proof(payload, &bundle, &root, &policy)?;
    authorize_claims(
        &result
            .certificate()
            .ok_or("missing verified signing certificate")?
            .ci_claims,
    )?;
    let digest = hex::encode(Sha256::digest(payload));
    verify_subject(&bundle, &digest)?;
    let verified = VerifiedCatalog { catalog, digest };
    CatalogPublishers::from_catalog(verified.catalog())?;
    Ok(verified)
}

/// Read only the explicit v2 envelope. Invalid v2 never falls back to legacy.
pub fn verify_document(document: &[u8]) -> Result<VerifiedCatalog, String> {
    if document.len() > MAX_DOCUMENT_BYTES {
        return Err("catalog document exceeds byte limit".into());
    }
    let envelope: Envelope =
        serde_json::from_slice(document).map_err(|_| "invalid GitHub catalog envelope")?;
    if envelope.schema != 2 || envelope.kind != "github-attested-catalog" {
        return Err("GitHub catalog requires its v2 envelope".into());
    }
    if envelope.catalog.len() > MAX_CATALOG_BYTES.div_ceil(3) * 4 {
        return Err("catalog payload exceeds byte limit".into());
    }
    let payload = STANDARD
        .decode(&envelope.catalog)
        .map_err(|_| "invalid catalog byte encoding")?;
    let proof = serde_json::to_vec(&envelope.attestation).map_err(|_| "invalid catalog proof")?;
    verify_parts(&payload, &proof)
}

/// Prepare an envelope only after the same production verification as a device.
pub fn envelope(payload: &[u8], proof: &[u8]) -> Result<String, String> {
    verify_parts(payload, proof)?;
    let value = Envelope {
        schema: 2,
        kind: "github-attested-catalog".into(),
        catalog: STANDARD.encode(payload),
        attestation: serde_json::from_slice(proof).map_err(|_| "invalid catalog proof")?,
    };
    serde_json::to_string_pretty(&value).map_err(|_| "cannot encode catalog envelope".into())
}

/// Persist one complete document; a failed write never replaces the old one.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err("catalog cache exceeds byte limit".into());
    }
    let parent = path
        .parent()
        .ok_or("catalog cache needs a parent directory")?;
    let mut random = [0u8; 16];
    rand_core::TryRngCore::try_fill_bytes(&mut rand_core::OsRng, &mut random)
        .map_err(|_| "cannot allocate catalog cache name")?;
    let temporary = parent.join(format!(".catalog-{}.tmp", hex::encode(random)));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| "cannot create catalog cache transaction")?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "cannot persist catalog cache transaction")?;
        drop(file);
        std::fs::rename(&temporary, path).map_err(|_| "cannot commit catalog cache transaction")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Authenticate a publication base with its explicit scheme. No failed v2
/// document is retried as legacy, and the old anchor remains unchanged.
pub fn authenticated_base(document: &[u8]) -> Result<Catalog, String> {
    if document.len() > MAX_DOCUMENT_BYTES {
        return Err("base catalog exceeds byte limit".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(document).map_err(|_| "invalid base catalog")?;
    if value.get("schema").and_then(|v| v.as_u64()) == Some(2) {
        return verify_document(document).map(VerifiedCatalog::into_catalog);
    }
    let catalog: Catalog =
        serde_json::from_value(value).map_err(|_| "invalid legacy base catalog")?;
    crate::verify_catalog(&catalog, crate::DEFAULT_ANCHOR)?;
    Ok(catalog)
}

/// Authenticated publication history. Private fields prevent an unsigned
/// candidate from being presented as reviewed ownership history.
pub struct AuthenticatedCatalog {
    catalog: Catalog,
    publishers: CatalogPublishers,
}
impl AuthenticatedCatalog {
    pub fn catalog(&self) -> &Catalog { &self.catalog }
    pub fn publishers(&self) -> &CatalogPublishers { &self.publishers }
}

pub fn authenticated_publication_base(document: &[u8], legacy_anchor: &str) -> Result<AuthenticatedCatalog, String> {
    if document.len() > MAX_DOCUMENT_BYTES { return Err("base catalog exceeds byte limit".into()); }
    let value: serde_json::Value=serde_json::from_slice(document).map_err(|_| "invalid base catalog")?;
    if value.get("schema").and_then(|v|v.as_u64())==Some(2) {
        let verified=verify_document(document)?;
        let publishers=CatalogPublishers::from_catalog(verified.catalog())?;
        return Ok(AuthenticatedCatalog { catalog:verified.into_catalog(), publishers });
    }
    let catalog:Catalog=serde_json::from_value(value).map_err(|_|"invalid legacy base catalog")?;
    crate::verify_catalog(&catalog,legacy_anchor).map_err(|e|format!("could not authenticate base catalog: {e}"))?;
    let publishers=CatalogPublishers::from_catalog(&catalog)?;
    Ok(AuthenticatedCatalog {catalog,publishers})
}

/// Candidate construction never authenticates its output. Only a subsequent
/// protected workflow attestation can publish the proposed releases.
pub fn prepare_authenticated(base: &AuthenticatedCatalog, candidate_bytes: &[u8], artifact_root: &Path) -> Result<Vec<u8>, String> {
    prepare_with_registry(base.catalog(), base.publishers(), candidate_bytes, artifact_root, true)
}

/// Admit a reviewed candidate, without credentials or contributor execution.
/// The caller authenticates its immutable candidate commit separately.
pub fn prepare(
    base: &Catalog,
    candidate_bytes: &[u8],
    artifact_root: &Path,
) -> Result<Vec<u8>, String> {
    let registry=CatalogPublishers::from_catalog(base)?;
    prepare_with_registry(base, &registry, candidate_bytes, artifact_root, true)
}

/// What a prepared candidate adds to its base, for the publication receipt:
/// each new app version's bundle, pack and index, and each new component
/// version's file and index (App Hub ADR 0003).
pub fn added_artifacts(base: &Catalog, candidate: &Catalog) -> Vec<serde_json::Value> {
    let mut added: Vec<serde_json::Value> = candidate
        .entries
        .iter()
        .filter(|entry| !base.entries.iter().any(|old| old.app_id() == entry.app_id() && old.version() == entry.version()))
        .map(|entry| {
            serde_json::json!({"bundle":entry.artifact,"pack":format!("{}.pack.json",entry.artifact),
                "index":format!("index/{}-{}.json",entry.app_id(),entry.version())})
        })
        .collect();
    added.extend(
        candidate
            .components
            .iter()
            .filter(|entry| base.component(entry.id(), entry.version()).is_none())
            .map(|entry| serde_json::json!({"component":entry.artifact,"index":crate::components::index_path(entry.id(),entry.version())})),
    );
    added
}

/// The candidate's component history against its base (App Hub ADR 0003),
/// by the apps' rules: existing versions keep their order and bytes and may
/// only be withdrawn, with a reason; nothing is removed. Each new version is
/// offered, names an immutable public GitHub source, passes the component
/// gate (with GitHub provenance unless `provenance` is off, which only this
/// module's tests do) and matches its reviewed file and index exactly.
fn prepare_components(base: &Catalog, candidate: &Catalog, artifact_root: &Path, provenance: bool) -> Result<(), String> {
    let old: std::collections::BTreeMap<(&str, &str), &crate::ComponentEntry> =
        base.components.iter().map(|e| ((e.id(), e.version()), e)).collect();
    if old.len() != base.components.len() {
        return Err("base catalog has duplicate component versions".into());
    }
    let retained: Vec<_> = candidate.components.iter().filter(|e| old.contains_key(&(e.id(), e.version()))).map(|e| (e.id(), e.version())).collect();
    let previous_order: Vec<_> = base.components.iter().map(|e| (e.id(), e.version())).collect();
    if retained != previous_order {
        return Err("candidate reorders or drops component history; withdraw instead".into());
    }
    for entry in &candidate.components {
        if let Some(previous) = old.get(&(entry.id(), entry.version())) {
            let mut allowed = (*previous).clone();
            if matches!((&previous.status, &entry.status), (crate::Status::Offered, crate::Status::Withdrawn(reason)) if !reason.trim().is_empty() && reason.len() <= 4096) {
                allowed.status = entry.status.clone();
            }
            if &allowed != entry {
                return Err("candidate changes existing component history or re-offers a withdrawn version".into());
            }
            continue;
        }
        if !entry.status.is_offered() {
            return Err("new candidate components must be offered".into());
        }
        if entry.source.commit.len() != 40 || !entry.source.commit.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("new component source must name an immutable commit".into());
        }
        if !entry.source.repository.starts_with("https://github.com/")
            || entry.source.repository[19..].split('/').count() != 2
            || entry.source.repository.contains(['?', '#', '@', '\\'])
        {
            return Err("new component source must name a public GitHub repository URL".into());
        }
        if entry.artifact != crate::components::artifact_path(entry.id(), entry.version())
            || Path::new(&entry.artifact).components().any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("candidate component artifact path is not canonical".into());
        }
        let file = artifact_root.join(&entry.artifact);
        for ancestor in file.ancestors().take_while(|p| *p != artifact_root) {
            if std::fs::symlink_metadata(ancestor).map_err(|_| "candidate component artifact is missing")?.file_type().is_symlink() {
                return Err("candidate component artifact path contains a symlink".into());
            }
        }
        let wasm = crate::admission::read_bounded(&file, crate::components::MAX_COMPONENT_BYTES)?;
        let release = entry.release();
        let report = crate::gate::check_component(&release, &wasm, provenance, Some(base))?;
        if !report.passed() {
            return Err(format!("candidate component was refused by the gate:\n{}", report.render()));
        }
        let rebuilt = crate::gate::component_entry_for(
            &release,
            &wasm,
            &report,
            &entry.publisher,
            &entry.source.repository,
            &entry.source.commit,
            &entry.admitted,
        )?;
        if &rebuilt != entry {
            return Err("candidate component entry differs from the release admitted by the gate".into());
        }
        let index_path = artifact_root.join(crate::components::index_path(entry.id(), entry.version()));
        let index: serde_json::Value = serde_json::from_slice(&crate::admission::read_bounded(&index_path, 2 * 1024 * 1024)?)
            .map_err(|_| "candidate component index is invalid")?;
        if serde_json::to_value(entry).map_err(|_| "cannot encode candidate component")? != index {
            return Err("candidate component index differs from its admitted entry".into());
        }
    }
    Ok(())
}

fn prepare_with_registry(base: &Catalog, registry: &CatalogPublishers, candidate_bytes: &[u8], artifact_root: &Path, component_provenance: bool) -> Result<Vec<u8>, String> {
    let candidate = parse_catalog(candidate_bytes)?;
    if candidate.sequence
        != base
            .sequence
            .checked_add(1)
            .ok_or("catalog sequence exhausted")?
    {
        return Err("candidate must advance the reviewed base by exactly one sequence".into());
    }
    if candidate.published != crate::today() {
        return Err("candidate publication date must be today".into());
    }
    let old = base
        .entries
        .iter()
        .map(|e| ((e.app_id(), e.version()), e))
        .collect::<std::collections::BTreeMap<_, _>>();
    if old.len() != base.entries.len() {
        return Err("base catalog has duplicate versions".into());
    }
    // Existing relative order is part of the append-only ownership history.
    let retained: Vec<_> = candidate.entries.iter().filter(|e| old.contains_key(&(e.app_id(),e.version())))
        .map(|e|(e.app_id(),e.version())).collect();
    let previous_order:Vec<_>=base.entries.iter().map(|e|(e.app_id(),e.version())).collect();
    if retained!=previous_order { return Err("candidate reorders or drops publisher history".into()); }
    // Components first, so a new app may name a new component of the same
    // candidate: apps resolve their components against the candidate's.
    prepare_components(base, &candidate, artifact_root, component_provenance)?;
    let mut resolving = base.clone();
    resolving.components = candidate.components.clone();
    let mut present = std::collections::BTreeSet::new();
    for entry in &candidate.entries {
        let key = (entry.app_id(), entry.version());
        present.insert(key);
        if let Some(previous) = old.get(&key) {
            let mut allowed = (*previous).clone();
            if matches!((&previous.status, &entry.status), (crate::Status::Offered, crate::Status::Withdrawn(reason)) if !reason.trim().is_empty() && reason.len() <= 4096)
            {
                allowed.status = entry.status.clone();
            }
            if serde_json::to_value(&allowed).map_err(|_| "invalid base entry")?
                != serde_json::to_value(entry).map_err(|_| "invalid candidate entry")?
            {
                return Err(
                    "candidate changes existing publisher history or re-offers a withdrawn version"
                        .into(),
                );
            }
            continue;
        }
        if !entry.status.is_offered() {
            return Err("new candidate entries must be offered".into());
        }
        if entry.source.commit.len() != 40
            || !entry.source.commit.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("new entry source must name an immutable commit".into());
        }
        if !entry.source.repository.starts_with("https://github.com/")
            || entry.source.repository[19..].split('/').count() != 2
            || entry.source.repository.contains(['?', '#', '@', '\\'])
        {
            return Err("new entry source must name a public GitHub repository URL".into());
        }
        let expected = format!("artifacts/{}-{}.bundle", entry.app_id(), entry.version());
        if entry.artifact != expected
            || Path::new(&entry.artifact)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("candidate artifact path is not canonical".into());
        }
        crate::verify_continuity(&entry.manifest, registry)?;
        let bundle = artifact_root.join(&entry.artifact);
        for ancestor in bundle.ancestors().take_while(|p| *p != artifact_root) {
            if std::fs::symlink_metadata(ancestor)
                .map_err(|_| "candidate artifact is missing")?
                .file_type()
                .is_symlink()
            {
                return Err("candidate artifact path contains a symlink".into());
            }
        }
        let keys = registry
            .trusted_keys(crate::PublisherKeys::new().with(&entry.publisher, &entry.publisher_key));
        let report = crate::gate::check_bundle_with_registry(
            &bundle, &octosense_app_policy::HostLimits::default(), &keys, &resolving, registry,
        )?;
        if !report.passed() {
            return Err(format!("candidate bundle was refused by the gate:\n{}", report.render()));
        }
        let rebuilt = crate::entry_for(
            &bundle,
            &report,
            &entry.publisher,
            &entry.publisher_key,
            &entry.source.repository,
            &entry.source.commit,
            &entry.admitted,
        )?;
        if serde_json::to_value(&rebuilt).map_err(|_| "invalid admitted entry")?
            != serde_json::to_value(entry).map_err(|_| "invalid candidate entry")?
        {
            return Err("candidate entry differs from the bundle admitted by the gate".into());
        }
        let pack = crate::pack_dir(&bundle)?;
        let pack_path = artifact_root.join(format!("{}.pack.json", entry.artifact));
        let pack_bytes =
            crate::admission::read_bounded(&pack_path, crate::gate::MAX_BUNDLE_BYTES * 2)?;
        let actual: serde_json::Value =
            serde_json::from_slice(&pack_bytes).map_err(|_| "candidate pack is invalid")?;
        if serde_json::to_value(&pack).map_err(|_| "cannot encode admitted pack")? != actual {
            return Err("candidate pack differs from its admitted bundle".into());
        }
        let index_path =
            artifact_root.join(format!("index/{}-{}.json", entry.app_id(), entry.version()));
        let index: serde_json::Value = serde_json::from_slice(&crate::admission::read_bounded(
            &index_path,
            2 * 1024 * 1024,
        )?)
        .map_err(|_| "candidate index is invalid")?;
        if serde_json::to_value(entry).map_err(|_| "cannot encode candidate entry")? != index {
            return Err("candidate index differs from its admitted entry".into());
        }
    }
    if old.keys().any(|k| !present.contains(k)) {
        return Err("candidate removes published history; withdraw instead".into());
    }
    serde_json::to_vec(&candidate).map_err(|_| "cannot encode candidate catalog".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    const PROOF: &str =
        include_str!("../tests/fixtures/github-catalog/cosign-v3-blob.sigstore.json");
    const PAYLOAD: &[u8] = include_bytes!("../tests/fixtures/github-catalog/cosign-v3-blob.txt");

    fn public_fixture() -> (Bundle, TrustedRoot, VerificationPolicy) {
        let bundle = Bundle::from_json(PROOF).unwrap();
        let cert =
            sigstore_verify::crypto::parse_certificate_info(bundle.signing_certificate().unwrap())
                .unwrap();
        let policy = VerificationPolicy::new(cert.identity.unwrap().as_str(), cert.issuer.unwrap());
        (
            bundle,
            TrustedRoot::from_embedded(SigstoreInstance::PublicGood).unwrap(),
            policy,
        )
    }

    #[test]
    fn public_proof_verifies_offline_but_never_authorizes_the_app_hub() {
        let (bundle, root, policy) = public_fixture();
        let result = verify_proof(PAYLOAD, &bundle, &root, &policy).unwrap();
        assert!(result.certificate_verified() && result.sct_verified() && result.tlog_verified());
        assert!(authorize_claims(&result.certificate().unwrap().ci_claims).is_err());
        assert!(verify_proof(
            PAYLOAD,
            &bundle,
            &root,
            &VerificationPolicy::new(GITHUB_WORKFLOW, GITHUB_ISSUER)
        )
        .is_err());
    }

    #[test]
    fn tampered_payload_signature_and_transparency_evidence_are_refused() {
        let (bundle, root, policy) = public_fixture();
        assert!(verify_proof(b"modified bytes", &bundle, &root, &policy).is_err());
        for pointer in [
            "/messageSignature/signature",
            "/verificationMaterial/tlogEntries/0/inclusionPromise/signedEntryTimestamp",
            "/verificationMaterial/tlogEntries/0/inclusionProof/rootHash",
        ] {
            let mut value: serde_json::Value = serde_json::from_str(PROOF).unwrap();
            let field = value
                .pointer_mut(pointer)
                .expect("fixture must contain attacked field");
            let mut bytes = STANDARD.decode(field.as_str().unwrap()).unwrap();
            bytes[0] ^= 1;
            *field = serde_json::Value::String(STANDARD.encode(bytes));
            let broken: Bundle = serde_json::from_value(value).unwrap();
            assert!(
                verify_proof(PAYLOAD, &broken, &root, &policy).is_err(),
                "{pointer}"
            );
        }
        let mut no_log = bundle;
        no_log.verification_material.tlog_entries.clear();
        assert!(verify_proof(PAYLOAD, &no_log, &root, &policy).is_err());
    }

    #[test]
    fn wrong_signer_and_root_or_authority_expiry_are_refused() {
        let (bundle, root, policy) = public_fixture();
        for (identity, issuer) in [("https://github.com/other/repo/.github/workflows/publish-catalog.yml@refs/heads/main", GITHUB_ISSUER),
            (GITHUB_WORKFLOW, "https://untrusted.example"),
            ("https://github.com/OctoSense-org/OctoSense-App-Hub/.github/workflows/other.yml@refs/heads/main", GITHUB_ISSUER),
            ("https://github.com/OctoSense-org/OctoSense-App-Hub/.github/workflows/publish-catalog.yml@refs/heads/unreviewed", GITHUB_ISSUER)] {
            assert!(verify_proof(PAYLOAD, &bundle, &root, &VerificationPolicy::new(identity, issuer)).is_err());
        }
        let staging = TrustedRoot::from_embedded(SigstoreInstance::Staging).unwrap();
        assert!(verify_proof(PAYLOAD, &bundle, &staging, &policy).is_err());
        let mut expired = root;
        for ca in &mut expired.certificate_authorities {
            ca.valid_for = Some(sigstore_types::TimeRange::new(
                "2000-01-01T00:00:00Z".parse().unwrap(),
                Some("2001-01-01T00:00:00Z".parse().unwrap()),
            ));
        }
        assert!(verify_proof(PAYLOAD, &bundle, &expired, &policy).is_err());
    }

    #[test]
    fn every_authenticated_github_claim_is_required() {
        let mut claims = sigstore_verify::crypto::FulcioCiClaims::default();
        claims.build_signer_uri = Some(GITHUB_WORKFLOW.into());
        claims.source_repository_uri = Some(REPOSITORY.into());
        claims.source_repository_identifier = Some(REPOSITORY_ID.into());
        claims.source_repository_owner_uri = Some(OWNER.into());
        claims.source_repository_owner_identifier = Some(OWNER_ID.into());
        claims.source_repository_ref = Some("refs/heads/main".into());
        claims.build_config_uri = Some(GITHUB_WORKFLOW.into());
        claims.build_trigger = Some("workflow_dispatch".into());
        claims.runner_environment = Some("github-hosted".into());
        claims.source_repository_visibility_at_signing = Some("public".into());
        claims.deployment_environment = Some(ENVIRONMENT.into());
        assert!(authorize_claims(&claims).is_ok());
        for i in 0..11 {
            let mut broken = claims.clone();
            let field = match i {
                0 => &mut broken.build_signer_uri,
                1 => &mut broken.source_repository_uri,
                2 => &mut broken.source_repository_identifier,
                3 => &mut broken.source_repository_owner_uri,
                4 => &mut broken.source_repository_owner_identifier,
                5 => &mut broken.source_repository_ref,
                6 => &mut broken.build_config_uri,
                7 => &mut broken.build_trigger,
                8 => &mut broken.runner_environment,
                9 => &mut broken.source_repository_visibility_at_signing,
                _ => &mut broken.deployment_environment,
            };
            *field = Some("untrusted".into());
            assert!(authorize_claims(&broken).is_err());
        }
    }

    #[test]
    fn unsigned_unknown_oversize_and_legacy_documents_cannot_enter_v2() {
        for bytes in [
            b"{}".as_slice(),
            br#"{"schema":2,"kind":"github-attested-catalog","catalog":"e30=","attestation":{}}"#,
            br#"{"schema":1,"sequence":0,"published":"2026-10-08","entries":[]}"#,
        ] {
            assert!(verify_document(bytes).is_err());
        }
        assert!(verify_document(&vec![b' '; MAX_DOCUMENT_BYTES + 1]).is_err());
        assert!(verify_parts(
            &serde_json::to_vec(&Catalog::new(1, &crate::today(), vec![])).unwrap(),
            PROOF.as_bytes()
        )
        .is_err());
        assert!(authenticated_base(br#"{"schema":2,"signature":"legacy-looking"}"#).is_err());
    }

    /// A new component's whole admission path but its GitHub proof, which
    /// `components_must_carry_github_provenance` and the publisher tests
    /// cover: the file, the gate, the rebuilt entry, the index and the
    /// receipt. Production preparation never turns the proof off.
    #[test]
    fn a_new_component_is_admitted_from_its_reviewed_file_and_index() {
        const NOTES: &[u8] = include_bytes!("../tests/fixtures/notes.component.wasm");
        let root = std::env::temp_dir().join(format!("hub-component-prepare-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("artifacts")).unwrap();
        std::fs::create_dir_all(root.join("index/components")).unwrap();
        let draft: crate::components::ComponentDraft = serde_json::from_value(serde_json::json!({
            "component": {"schema": 1, "id": "org.example.markdown", "version": "1.0.0", "name": "Markdown",
                "publisher": {"name": "Example", "support": "https://example.test/s", "privacy_policy_url": "https://example.test/p"},
                "license": "MIT"},
            "listing": {"description": "Renders Markdown."}
        })).unwrap();
        let release = crate::components::ComponentRelease::from_draft(draft, NOTES).unwrap();
        let report = crate::gate::check_component(&release, NOTES, false, None).unwrap();
        let entry = crate::gate::component_entry_for(
            &release, NOTES, &report, "dev:example", "https://github.com/example/markdown", &"1".repeat(40), &crate::today(),
        )
        .unwrap();
        std::fs::write(root.join(&entry.artifact), NOTES).unwrap();
        std::fs::write(root.join(crate::components::index_path(entry.id(), entry.version())), serde_json::to_vec(&entry).unwrap()).unwrap();
        let base = Catalog::new(4, "2026-10-08", vec![]);
        let mut candidate = Catalog::new(5, &crate::today(), vec![]);
        candidate.components.push(entry.clone());
        let registry = CatalogPublishers::from_catalog(&base).unwrap();
        let bytes = serde_json::to_vec(&candidate).unwrap();
        let prepared = prepare_with_registry(&base, &registry, &bytes, &root, false).unwrap();
        assert_eq!(prepared, bytes);
        assert_eq!(
            added_artifacts(&base, &serde_json::from_slice(&prepared).unwrap()),
            [serde_json::json!({"component": "artifacts/org.example.markdown-1.0.0.wasm",
                "index": "index/components/org.example.markdown-1.0.0.json"})]
        );
        // The public path requires GitHub provenance.
        assert!(prepare(&base, &bytes, &root).unwrap_err().contains("App Hub accepts only GitHub-attested component releases"));
        // An entry edited after review no longer matches its reviewed index,
        // and a stale index or another file is refused.
        let mut edited = candidate.clone();
        edited.components[0].listing.description = "Something else.".into();
        let edited = serde_json::to_vec(&edited).unwrap();
        assert_eq!(
            prepare_with_registry(&base, &registry, &edited, &root, false).unwrap_err(),
            "candidate component index differs from its admitted entry"
        );
        std::fs::write(root.join(crate::components::index_path(entry.id(), entry.version())), b"{}").unwrap();
        assert_eq!(
            prepare_with_registry(&base, &registry, &bytes, &root, false).unwrap_err(),
            "candidate component index differs from its admitted entry"
        );
        std::fs::write(root.join(&entry.artifact), &NOTES[..1000]).unwrap();
        assert!(prepare_with_registry(&base, &registry, &bytes, &root, false).unwrap_err().contains("[refused] digest"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn attested_statement_must_name_one_exact_subject_and_workflow() {
        // This tests the application policy after cryptographic verification,
        // independently from the real signed upstream proof tests above.
        let digest = "a".repeat(64);
        let statement = serde_json::json!({"_type":"https://in-toto.io/Statement/v1",
            "subject":[{"name":CATALOG_SUBJECT,"digest":{"sha256":digest}}],
            "predicateType":"https://slsa.dev/provenance/v1","predicate":{
                "runDetails":{"builder":{"id":GITHUB_WORKFLOW}},
                "buildDefinition":{"buildType":"https://actions.github.io/buildtypes/workflow/v1",
                    "externalParameters":{"workflow":{"repository":REPOSITORY,"ref":"refs/heads/main","path":".github/workflows/publish-catalog.yml"}},
                    "internalParameters":{"github":{"event_name":"workflow_dispatch","repository_id":REPOSITORY_ID,"repository_owner_id":OWNER_ID,"runner_environment":"github-hosted"}}}}});
        let bundle_for = |statement: &serde_json::Value| {
            let mut value: serde_json::Value = serde_json::from_str(PROOF).unwrap();
            let signature = value["messageSignature"]["signature"].clone();
            value.as_object_mut().unwrap().remove("messageSignature");
            value["dsseEnvelope"] = serde_json::json!({"payloadType":"application/vnd.in-toto+json",
                "payload":STANDARD.encode(serde_json::to_vec(statement).unwrap()),"signatures":[{"sig":signature}]});
            serde_json::from_value::<Bundle>(value).unwrap()
        };
        assert!(verify_subject(&bundle_for(&statement), &digest).is_ok());
        for pointer in [
            "/subject/0/name",
            "/subject/0/digest/sha256",
            "/predicateType",
            "/_type",
            "/predicate/runDetails/builder/id",
            "/predicate/buildDefinition/externalParameters/workflow/repository",
            "/predicate/buildDefinition/externalParameters/workflow/ref",
            "/predicate/buildDefinition/internalParameters/github/repository_id",
        ] {
            let mut bad = statement.clone();
            *bad.pointer_mut(pointer).unwrap() = "wrong".into();
            assert!(
                verify_subject(&bundle_for(&bad), &digest).is_err(),
                "{pointer}"
            );
        }
        let mut multiple = statement.clone();
        multiple["subject"]
            .as_array_mut()
            .unwrap()
            .push(statement["subject"][0].clone());
        assert!(verify_subject(&bundle_for(&multiple), &digest).is_err());
    }
}
