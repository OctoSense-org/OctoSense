//! Keyless publisher signatures. GitHub issues the short-lived identity; the
//! maintained Sigstore verifier checks its certificate and transparency proof.
//! The catalog separately authorizes that identity to update an existing app.
use octosense_app_policy::manifest::GithubPublisher;
use sha2::{Digest, Sha256};
use sigstore_trust_root::{SigstoreInstance, TrustedRoot};
use sigstore_types::{Bundle, SignatureContent};
use sigstore_verify::{crypto::FulcioCiClaims, VerificationPolicy};

pub const PUBLISHER_SUBJECT: &str = "octosense-app-manifest.json";
pub const MAX_PROOF_BYTES: usize = 48 * 1024;

pub fn verify(identity: &GithubPublisher, payload: &[u8]) -> Result<(), String> {
    if payload.len() > crate::admission::MAX_MANIFEST_BYTES as usize {
        return Err("publisher subject exceeds manifest limit".into());
    }
    let manifest = octosense_app_policy::AppManifest::parse(
        std::str::from_utf8(payload).map_err(|_| "publisher subject is not UTF-8")?,
    )?;
    let mut expected = identity.clone();
    expected.attestation = None;
    if manifest.integrity.github.as_ref() != Some(&expected)
        || manifest.signing_bytes()?.as_slice() != payload
    {
        return Err(
            "publisher subject must be the canonical manifest with this GitHub identity".into(),
        );
    }
    semver::Version::parse(&manifest.version)
        .map_err(|_| "GitHub publisher version must be semantic version major.minor.patch")?;
    verify_attestation(identity, payload, PUBLISHER_SUBJECT)
}

/// A shared component release's GitHub provenance (App Hub ADR 0003): the
/// same proof and identity rules as an app's, over the canonical release
/// ([`crate::components::ComponentRelease::subject_bytes`]) named
/// [`crate::components::COMPONENT_SUBJECT`]. The release carries the file's
/// digest, so the proof binds the `.wasm` file.
pub fn verify_component(identity: &GithubPublisher, payload: &[u8]) -> Result<(), String> {
    if payload.len() as u64 > crate::components::MAX_RELEASE_BYTES {
        return Err("component subject exceeds the release limit".into());
    }
    let release: crate::components::ComponentRelease =
        serde_json::from_slice(payload).map_err(|_| "component subject is not a component release")?;
    let mut expected = identity.clone();
    expected.attestation = None;
    if release.component.integrity.github.as_ref() != Some(&expected) || release.subject_bytes()?.as_slice() != payload {
        return Err("component subject must be the canonical release with this GitHub identity".into());
    }
    identity.validate(&release.component.version)?;
    semver::Version::parse(&release.component.version)
        .map_err(|_| "GitHub publisher version must be semantic version major.minor.patch")?;
    verify_attestation(identity, payload, crate::components::COMPONENT_SUBJECT)
}

/// The Sigstore proof in `identity.attestation` over exactly `payload`,
/// named `subject`, from the identity's public repository, tag and commit.
fn verify_attestation(identity: &GithubPublisher, payload: &[u8], subject: &str) -> Result<(), String> {
    let proof = serde_json::to_vec(
        identity
            .attestation
            .as_ref()
            .ok_or("publisher attestation is missing")?,
    )
    .map_err(|_| "invalid publisher proof")?;
    if proof.len() > MAX_PROOF_BYTES {
        return Err("publisher proof exceeds byte limit".into());
    }
    let bundle: Bundle =
        serde_json::from_slice(&proof).map_err(|_| "invalid Sigstore publisher bundle")?;
    let root = TrustedRoot::from_embedded(SigstoreInstance::PublicGood)
        .map_err(|_| "publisher trust snapshot is invalid")?;
    let policy = VerificationPolicy::new(
        identity.workflow_uri(),
        crate::github_catalog::GITHUB_ISSUER,
    );
    let verified = crate::github_catalog::verify_proof(payload, &bundle, &root, &policy)
        .map_err(|e| e.replace("catalog", "publisher"))?;
    authorize_claims(
        identity,
        &verified
            .certificate()
            .ok_or("missing publisher certificate")?
            .ci_claims,
    )?;
    verify_subject(identity, &bundle, subject, &hex::encode(Sha256::digest(payload)))
}

fn authorize_claims(identity: &GithubPublisher, claims: &FulcioCiClaims) -> Result<(), String> {
    let repository = identity.repository_url();
    let workflow = identity.workflow_uri();
    let owner = format!(
        "https://github.com/{}",
        identity.repository.split('/').next().unwrap_or_default()
    );
    let reference = format!("refs/tags/{}", identity.tag);
    for (actual, expected, name) in [
        (
            claims.build_signer_uri.as_deref(),
            workflow.as_str(),
            "signer workflow",
        ),
        (
            claims.build_config_uri.as_deref(),
            workflow.as_str(),
            "build workflow",
        ),
        (
            claims.source_repository_uri.as_deref(),
            repository.as_str(),
            "repository",
        ),
        (
            claims.source_repository_identifier.as_deref(),
            identity.repository_id.as_str(),
            "repository id",
        ),
        (
            claims.source_repository_owner_uri.as_deref(),
            owner.as_str(),
            "owner",
        ),
        (
            claims.source_repository_owner_identifier.as_deref(),
            identity.owner_id.as_str(),
            "owner id",
        ),
        (
            claims.source_repository_ref.as_deref(),
            reference.as_str(),
            "tag",
        ),
        (
            claims.source_repository_digest.as_deref(),
            identity.commit.as_str(),
            "source commit",
        ),
        (
            claims.build_signer_digest.as_deref(),
            identity.commit.as_str(),
            "signer commit",
        ),
        (
            claims.build_config_digest.as_deref(),
            identity.commit.as_str(),
            "workflow commit",
        ),
        (claims.build_trigger.as_deref(), "push", "trigger"),
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
    ] {
        if actual != Some(expected) {
            return Err(format!("publisher proof has unauthorized {name}"));
        }
    }
    Ok(())
}

fn verify_subject(identity: &GithubPublisher, bundle: &Bundle, subject: &str, digest: &str) -> Result<(), String> {
    let SignatureContent::DsseEnvelope(envelope) = &bundle.content else {
        return Err("publisher requires an in-toto attestation".into());
    };
    let statement: serde_json::Value = serde_json::from_slice(envelope.payload.as_bytes())
        .map_err(|_| "invalid publisher statement")?;
    authorize_statement(identity, &statement, subject, digest)
}

fn authorize_statement(
    identity: &GithubPublisher,
    statement: &serde_json::Value,
    subject: &str,
    digest: &str,
) -> Result<(), String> {
    if statement.get("_type").and_then(|v| v.as_str()) != Some("https://in-toto.io/Statement/v1")
        || statement.get("predicateType").and_then(|v| v.as_str())
            != Some("https://slsa.dev/provenance/v1")
    {
        return Err("unexpected publisher statement type".into());
    }
    let subjects = statement
        .get("subject")
        .and_then(|v| v.as_array())
        .ok_or("missing publisher subject")?;
    if subjects.len() != 1
        || subjects[0].get("name").and_then(|v| v.as_str()) != Some(subject)
        || subjects[0]
            .pointer("/digest/sha256")
            .and_then(|v| v.as_str())
            != Some(digest)
    {
        let what = if subject == PUBLISHER_SUBJECT { "manifest" } else { "component release" };
        return Err(format!("publisher proof does not name the exact canonical {what}"));
    }
    let repository = identity.repository_url();
    let workflow = identity.workflow_uri();
    let reference = format!("refs/tags/{}", identity.tag);
    for (pointer, expected) in [
        ("/predicate/runDetails/builder/id", workflow.as_str()),
        (
            "/predicate/buildDefinition/buildType",
            "https://actions.github.io/buildtypes/workflow/v1",
        ),
        (
            "/predicate/buildDefinition/externalParameters/workflow/repository",
            repository.as_str(),
        ),
        (
            "/predicate/buildDefinition/externalParameters/workflow/ref",
            reference.as_str(),
        ),
        (
            "/predicate/buildDefinition/externalParameters/workflow/path",
            identity.workflow.as_str(),
        ),
        (
            "/predicate/buildDefinition/internalParameters/github/event_name",
            "push",
        ),
        (
            "/predicate/buildDefinition/internalParameters/github/repository_id",
            identity.repository_id.as_str(),
        ),
        (
            "/predicate/buildDefinition/internalParameters/github/repository_owner_id",
            identity.owner_id.as_str(),
        ),
        (
            "/predicate/buildDefinition/internalParameters/github/runner_environment",
            "github-hosted",
        ),
    ] {
        if statement.pointer(pointer).and_then(|v| v.as_str()) != Some(expected) {
            return Err(format!(
                "publisher proof has unauthorized predicate {pointer}"
            ));
        }
    }
    // Fulcio binds all three commits above. The predicate must describe that
    // same source checkout, rather than a second mutable checkout in the job.
    let dependencies = statement
        .pointer("/predicate/buildDefinition/resolvedDependencies")
        .and_then(|v| v.as_array())
        .ok_or("publisher proof has no resolved source dependency")?;
    let source = format!("git+{repository}@{reference}");
    if !dependencies.iter().any(|v| {
        v.get("uri").and_then(|v| v.as_str()) == Some(&source)
            && v.pointer("/digest/gitCommit").and_then(|v| v.as_str()) == Some(&identity.commit)
    }) {
        return Err("publisher proof has a different resolved source commit".into());
    }
    Ok(())
}

/// Stable update authority; the release-specific tag and commit may advance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GithubBinding {
    pub repository: String,
    pub repository_id: String,
    pub owner_id: String,
    pub workflow: String,
}
impl From<&GithubPublisher> for GithubBinding {
    fn from(g: &GithubPublisher) -> Self {
        Self {
            repository: g.repository.clone(),
            repository_id: g.repository_id.clone(),
            owner_id: g.owner_id.clone(),
            workflow: g.workflow.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn identity() -> GithubPublisher {
        serde_json::from_value(
            json!({"repository":"example/app","repository_id":"123","owner_id":"456",
            "workflow":".github/workflows/publish-app.yml","tag":"v1.2.3","commit":"a".repeat(40)}),
        )
        .unwrap()
    }
    fn claims(g: &GithubPublisher) -> FulcioCiClaims {
        let mut c = FulcioCiClaims::default();
        c.build_signer_uri = Some(g.workflow_uri());
        c.build_config_uri = Some(g.workflow_uri());
        c.source_repository_uri = Some(g.repository_url());
        c.source_repository_identifier = Some(g.repository_id.clone());
        c.source_repository_owner_uri = Some("https://github.com/example".into());
        c.source_repository_owner_identifier = Some(g.owner_id.clone());
        c.source_repository_ref = Some(format!("refs/tags/{}", g.tag));
        c.source_repository_digest = Some(g.commit.clone());
        c.build_signer_digest = Some(g.commit.clone());
        c.build_config_digest = Some(g.commit.clone());
        c.build_trigger = Some("push".into());
        c.runner_environment = Some("github-hosted".into());
        c.source_repository_visibility_at_signing = Some("public".into());
        c
    }
    #[test]
    fn authenticated_claim_policy_pins_ids_tag_all_commits_and_workflow() {
        let g = identity();
        let good = claims(&g);
        authorize_claims(&g, &good).unwrap();
        for field in 0..13 {
            let mut c = good.clone();
            let target = match field {
                0 => &mut c.build_signer_uri,
                1 => &mut c.build_config_uri,
                2 => &mut c.source_repository_uri,
                3 => &mut c.source_repository_identifier,
                4 => &mut c.source_repository_owner_uri,
                5 => &mut c.source_repository_owner_identifier,
                6 => &mut c.source_repository_ref,
                7 => &mut c.source_repository_digest,
                8 => &mut c.build_signer_digest,
                9 => &mut c.build_config_digest,
                10 => &mut c.build_trigger,
                11 => &mut c.runner_environment,
                _ => &mut c.source_repository_visibility_at_signing,
            };
            *target = Some("wrong".into());
            assert!(authorize_claims(&g, &c).is_err(), "field {field}");
        }
    }
    #[test]
    fn subject_policy_refuses_different_manifest_tag_commit_and_repo() {
        let g = identity();
        let digest = "0".repeat(64);
        let statement = json!({"_type":"https://in-toto.io/Statement/v1","predicateType":"https://slsa.dev/provenance/v1",
        "subject":[{"name":PUBLISHER_SUBJECT,"digest":{"sha256":digest}}],"predicate":{
            "runDetails":{"builder":{"id":g.workflow_uri()}}, "buildDefinition":{
                "buildType":"https://actions.github.io/buildtypes/workflow/v1",
                "externalParameters":{"workflow":{"repository":g.repository_url(),"ref":format!("refs/tags/{}",g.tag),"path":g.workflow}},
                "internalParameters":{"github":{"event_name":"push","repository_id":g.repository_id,"repository_owner_id":g.owner_id,"runner_environment":"github-hosted"}},
                "resolvedDependencies":[{"uri":format!("git+{}@refs/tags/{}",g.repository_url(),g.tag),"digest":{"gitCommit":g.commit}}]
            }}});
        authorize_statement(&g, &statement, PUBLISHER_SUBJECT, &digest).unwrap();
        // The same proof never stands for a component release, or the reverse.
        assert!(authorize_statement(&g, &statement, crate::components::COMPONENT_SUBJECT, &digest).is_err());
        for pointer in [
            "/subject/0/digest/sha256",
            "/subject/0/name",
            "/predicate/runDetails/builder/id",
            "/predicate/buildDefinition/externalParameters/workflow/ref",
            "/predicate/buildDefinition/internalParameters/github/repository_id",
            "/predicate/buildDefinition/resolvedDependencies/0/digest/gitCommit",
        ] {
            let mut changed = statement.clone();
            *changed.pointer_mut(pointer).unwrap() = json!("wrong");
            assert!(
                authorize_statement(&g, &changed, PUBLISHER_SUBJECT, &digest).is_err(),
                "{pointer}"
            );
        }
    }
    #[test]
    fn an_unrelated_or_missing_proof_cannot_authenticate_a_component_release() {
        const NOTES: &[u8] = include_bytes!("../tests/fixtures/notes.component.wasm");
        let draft: crate::components::ComponentDraft = serde_json::from_value(json!({
            "component": {"schema": 1, "id": "example.markdown", "version": "1.2.3", "name": "Markdown",
                "publisher": {"name": "Example", "support": "https://example.test/s", "privacy_policy_url": "https://example.test/p"},
                "license": "MIT"},
            "listing": {"description": "Renders Markdown."}
        }))
        .unwrap();
        let mut release = crate::components::ComponentRelease::from_draft(draft, NOTES).unwrap();
        let mut g = identity();
        release.component.integrity.github = Some(g.clone());
        let subject = release.subject_bytes().unwrap();
        // No proof attached.
        assert_eq!(verify_component(&g, &subject).unwrap_err(), "publisher attestation is missing");
        // A real public proof for other bytes.
        g.attestation = Some(serde_json::from_str(include_str!("../tests/fixtures/github-catalog/cosign-v3-blob.sigstore.json")).unwrap());
        assert!(verify_component(&g, &subject).is_err());
        // Not the canonical release, or another identity.
        assert_eq!(
            verify_component(&g, &serde_json::to_vec_pretty(&release).unwrap()).unwrap_err(),
            "component subject must be the canonical release with this GitHub identity"
        );
        let mut other = identity();
        other.repository_id = "999".into();
        assert!(verify_component(&other, &subject).unwrap_err().contains("canonical release with this GitHub identity"));
        // The tag must be v<version>.
        let mut retagged = release.clone();
        retagged.component.version = "1.2.4".into();
        let bytes = retagged.subject_bytes().unwrap();
        assert!(verify_component(&g, &bytes).unwrap_err().contains("publisher tag must be v followed by the exact manifest version"));
    }

    #[test]
    fn incomplete_or_unrelated_public_proof_cannot_authenticate_a_manifest() {
        let mut g = identity();
        assert!(verify(&g, b"{}").is_err());
        g.attestation = Some(json!({"mediaType":"application/vnd.dev.sigstore.bundle.v0.3+json"}));
        assert!(verify(&g, b"{}").is_err());
        g.attestation = Some(
            serde_json::from_str(include_str!(
                "../tests/fixtures/github-catalog/cosign-v3-blob.sigstore.json"
            ))
            .unwrap(),
        );
        assert!(verify(&g, b"{}").is_err());
    }
}
