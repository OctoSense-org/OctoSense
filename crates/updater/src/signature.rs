//! GitHub's signature on a desktop release's `SHA256SUMS`.
//!
//! GitHub's asset digests and the checksum file only prove that the files are
//! the ones in the release: anyone who can publish a release can make them
//! agree. A desktop release is offered only when its `SHA256SUMS` also carries
//! GitHub's signature from the `sign-desktop-release` workflow on `main`, run
//! in the `desktop-release-signing` environment, whose reviewers approve each
//! run. This is the model App Hub uses for its catalog (`github_catalog.rs`):
//! the Sigstore library verifies the certificate, its timestamps and the
//! transparency-log proof offline against its embedded public-good trust root,
//! and this module checks GitHub's claims and the exact signed subject.
//!
//! Android Home needs no such signature: its installer accepts only an APK
//! signed by the pinned release signer (`HomeUpdatePolicy.java`).

use sha2::{Digest, Sha256};
use sigstore_trust_root::{SigstoreInstance, TrustedRoot};
use sigstore_types::{Bundle, MediaType, SignatureContent};
use sigstore_verify::{VerificationPolicy, Verifier};

/// The release asset holding the Sigstore bundle for `SHA256SUMS`.
pub(crate) const SIGNATURE_ASSET: &str = "SHA256SUMS.sigstore.json";
pub(crate) const MAX_SIGNATURE: u64 = 2 * 1024 * 1024;

const GITHUB_ISSUER: &str = "https://token.actions.githubusercontent.com";
const WORKFLOW_PATH: &str = ".github/workflows/sign-desktop-release.yml";
const WORKFLOW: &str = "https://github.com/OctoSense-org/OctoSense/.github/workflows/sign-desktop-release.yml@refs/heads/main";
const REPOSITORY: &str = "https://github.com/OctoSense-org/OctoSense";
const OWNER: &str = "https://github.com/OctoSense-org";
const REPOSITORY_ID: &str = "1361736338";
const OWNER_ID: &str = "328148893";
const ENVIRONMENT: &str = "desktop-release-signing";

/// What the workflow signs: the release's `SHA256SUMS`, named after its tag,
/// so a signature names the one release it was made for.
pub(crate) fn subject_name(tag: &str) -> String {
    format!("{tag}.SHA256SUMS")
}

/// Verify `proof`, a Sigstore bundle, over the exact bytes of `tag`'s
/// `SHA256SUMS`.
pub(crate) fn verify(tag: &str, sums: &[u8], proof: &[u8]) -> Result<(), &'static str> {
    if proof.len() as u64 > MAX_SIGNATURE {
        return Err("the release signature exceeds its size limit");
    }
    let bundle: Bundle = serde_json::from_slice(proof)
        .map_err(|_| "the release signature is not a Sigstore bundle")?;
    let root = TrustedRoot::from_embedded(SigstoreInstance::PublicGood)
        .map_err(|_| "the Sigstore trust root is invalid")?;
    let policy = VerificationPolicy::new(WORKFLOW, GITHUB_ISSUER);
    let result = verify_proof(sums, &bundle, &root, &policy)?;
    authorize_claims(
        &result
            .certificate()
            .ok_or("the release signature has no verified certificate")?
            .ci_claims,
    )?;
    verify_subject(
        &bundle,
        &subject_name(tag),
        &format!("{:x}", Sha256::digest(sums)),
    )
}

fn verify_proof(
    payload: &[u8],
    bundle: &Bundle,
    root: &TrustedRoot,
    policy: &VerificationPolicy,
) -> Result<sigstore_verify::VerificationResult, &'static str> {
    if bundle.media_type != MediaType::Bundle0_3 {
        return Err("the release signature must be a Sigstore v0.3 bundle");
    }
    let verifier = Verifier::new(root).map_err(|_| "the Sigstore trust root is invalid")?;
    let result = verifier
        .verify(payload, bundle, policy)
        .map_err(|_| "the release is not signed by OctoSense's release workflow")?;
    if !result.certificate_verified()
        || !result.sct_verified()
        || !result.tlog_verified()
        || !result.identity_policy_checked()
    {
        return Err("the release signature was not fully verified");
    }
    Ok(result)
}

fn authorize_claims(claims: &sigstore_verify::crypto::FulcioCiClaims) -> Result<(), &'static str> {
    for (actual, expected) in [
        (claims.build_signer_uri.as_deref(), WORKFLOW),
        (claims.source_repository_uri.as_deref(), REPOSITORY),
        (
            claims.source_repository_identifier.as_deref(),
            REPOSITORY_ID,
        ),
        (claims.source_repository_owner_uri.as_deref(), OWNER),
        (
            claims.source_repository_owner_identifier.as_deref(),
            OWNER_ID,
        ),
        (claims.source_repository_ref.as_deref(), "refs/heads/main"),
        (claims.build_config_uri.as_deref(), WORKFLOW),
        (claims.build_trigger.as_deref(), "workflow_dispatch"),
        (claims.runner_environment.as_deref(), "github-hosted"),
        (
            claims.source_repository_visibility_at_signing.as_deref(),
            "public",
        ),
        (claims.deployment_environment.as_deref(), ENVIRONMENT),
    ] {
        if actual != Some(expected) {
            return Err("the release was signed outside OctoSense's release workflow");
        }
    }
    Ok(())
}

fn verify_subject(bundle: &Bundle, name: &str, digest: &str) -> Result<(), &'static str> {
    let SignatureContent::DsseEnvelope(envelope) = &bundle.content else {
        return Err("the release signature is not an in-toto attestation");
    };
    let statement: serde_json::Value = serde_json::from_slice(envelope.payload.as_bytes())
        .map_err(|_| "the release signature's statement is invalid")?;
    if statement.get("_type").and_then(|v| v.as_str()) != Some("https://in-toto.io/Statement/v1")
        || statement.get("predicateType").and_then(|v| v.as_str())
            != Some("https://slsa.dev/provenance/v1")
    {
        return Err("the release signature has an unexpected statement type");
    }
    let subjects = statement
        .get("subject")
        .and_then(|v| v.as_array())
        .ok_or("the release signature names no subject")?;
    if subjects.len() != 1
        || subjects[0].get("name").and_then(|v| v.as_str()) != Some(name)
        || subjects[0]
            .pointer("/digest/sha256")
            .and_then(|v| v.as_str())
            != Some(digest)
    {
        return Err("the release signature does not name this release's SHA256SUMS");
    }
    // The maintained actions/attest SLSA workflow/v1 predicate: its builder
    // is the job's workflow, not merely a GitHub runner.
    for (pointer, expected) in [
        ("/predicate/runDetails/builder/id", WORKFLOW),
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
            WORKFLOW_PATH,
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
            return Err("the release signature names another workflow");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};

    // A public Sigstore bundle over synthetic text (see tests/fixtures).
    const PROOF: &str = include_str!("../tests/fixtures/cosign-v3-blob.sigstore.json");
    const PAYLOAD: &[u8] = include_bytes!("../tests/fixtures/cosign-v3-blob.txt");

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
    fn a_valid_public_signature_from_another_signer_is_refused() {
        let (bundle, root, policy) = public_fixture();
        // The cryptography holds for its own signer...
        let result = verify_proof(PAYLOAD, &bundle, &root, &policy).unwrap();
        assert!(result.certificate_verified() && result.sct_verified() && result.tlog_verified());
        // ...but it is not OctoSense's release workflow.
        assert!(authorize_claims(&result.certificate().unwrap().ci_claims).is_err());
        assert!(verify_proof(
            PAYLOAD,
            &bundle,
            &root,
            &VerificationPolicy::new(WORKFLOW, GITHUB_ISSUER)
        )
        .is_err());
        assert!(verify("desktop-v1.0.0", PAYLOAD, PROOF.as_bytes()).is_err());
    }

    #[test]
    fn tampered_checksums_signature_or_transparency_proof_are_refused() {
        let (bundle, root, policy) = public_fixture();
        assert!(verify_proof(b"modified checksums", &bundle, &root, &policy).is_err());
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
    fn other_workflows_branches_issuers_and_roots_are_refused() {
        let (bundle, root, policy) = public_fixture();
        for (identity, issuer) in [
            ("https://github.com/other/repo/.github/workflows/sign-desktop-release.yml@refs/heads/main", GITHUB_ISSUER),
            (WORKFLOW, "https://untrusted.example"),
            ("https://github.com/OctoSense-org/OctoSense/.github/workflows/release-desktop.yml@refs/heads/main", GITHUB_ISSUER),
            ("https://github.com/OctoSense-org/OctoSense/.github/workflows/sign-desktop-release.yml@refs/tags/desktop-v1.0.0", GITHUB_ISSUER),
        ] {
            assert!(verify_proof(PAYLOAD, &bundle, &root, &VerificationPolicy::new(identity, issuer)).is_err());
        }
        let staging = TrustedRoot::from_embedded(SigstoreInstance::Staging).unwrap();
        assert!(verify_proof(PAYLOAD, &bundle, &staging, &policy).is_err());
    }

    #[test]
    fn every_github_claim_of_the_signing_run_is_required() {
        let mut claims = sigstore_verify::crypto::FulcioCiClaims::default();
        claims.build_signer_uri = Some(WORKFLOW.into());
        claims.source_repository_uri = Some(REPOSITORY.into());
        claims.source_repository_identifier = Some(REPOSITORY_ID.into());
        claims.source_repository_owner_uri = Some(OWNER.into());
        claims.source_repository_owner_identifier = Some(OWNER_ID.into());
        claims.source_repository_ref = Some("refs/heads/main".into());
        claims.build_config_uri = Some(WORKFLOW.into());
        claims.build_trigger = Some("workflow_dispatch".into());
        claims.runner_environment = Some("github-hosted".into());
        claims.source_repository_visibility_at_signing = Some("public".into());
        claims.deployment_environment = Some(ENVIRONMENT.into());
        assert!(authorize_claims(&claims).is_ok());
        for i in 0..11 {
            for value in [Some("untrusted".to_string()), None] {
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
                *field = value;
                assert!(authorize_claims(&broken).is_err(), "claim {i}");
            }
        }
    }

    #[test]
    fn the_signature_must_name_this_tags_exact_checksums_and_the_workflow() {
        // The policy after cryptographic verification, on a synthetic statement.
        let tag = "desktop-v0.2.0-rc.1";
        let digest = format!("{:x}", Sha256::digest(b"checksums"));
        let statement = serde_json::json!({"_type":"https://in-toto.io/Statement/v1",
            "subject":[{"name":subject_name(tag),"digest":{"sha256":digest}}],
            "predicateType":"https://slsa.dev/provenance/v1","predicate":{
                "runDetails":{"builder":{"id":WORKFLOW}},
                "buildDefinition":{"buildType":"https://actions.github.io/buildtypes/workflow/v1",
                    "externalParameters":{"workflow":{"repository":REPOSITORY,"ref":"refs/heads/main","path":WORKFLOW_PATH}},
                    "internalParameters":{"github":{"event_name":"workflow_dispatch","repository_id":REPOSITORY_ID,"repository_owner_id":OWNER_ID,"runner_environment":"github-hosted"}}}}});
        let bundle_for = |statement: &serde_json::Value| {
            let mut value: serde_json::Value = serde_json::from_str(PROOF).unwrap();
            let signature = value["messageSignature"]["signature"].clone();
            value.as_object_mut().unwrap().remove("messageSignature");
            value["dsseEnvelope"] = serde_json::json!({"payloadType":"application/vnd.in-toto+json",
                "payload":STANDARD.encode(serde_json::to_vec(statement).unwrap()),"signatures":[{"sig":signature}]});
            serde_json::from_value::<Bundle>(value).unwrap()
        };
        assert!(verify_subject(&bundle_for(&statement), &subject_name(tag), &digest).is_ok());
        // Another release's checksums, or other bytes, are not this release's.
        assert!(verify_subject(
            &bundle_for(&statement),
            &subject_name("desktop-v0.1.0"),
            &digest
        )
        .is_err());
        assert!(
            verify_subject(&bundle_for(&statement), &subject_name(tag), &"a".repeat(64)).is_err()
        );
        for pointer in [
            "/subject/0/name",
            "/subject/0/digest/sha256",
            "/predicateType",
            "/_type",
            "/predicate/runDetails/builder/id",
            "/predicate/buildDefinition/buildType",
            "/predicate/buildDefinition/externalParameters/workflow/repository",
            "/predicate/buildDefinition/externalParameters/workflow/ref",
            "/predicate/buildDefinition/externalParameters/workflow/path",
            "/predicate/buildDefinition/internalParameters/github/event_name",
            "/predicate/buildDefinition/internalParameters/github/repository_id",
            "/predicate/buildDefinition/internalParameters/github/repository_owner_id",
            "/predicate/buildDefinition/internalParameters/github/runner_environment",
        ] {
            let mut bad = statement.clone();
            *bad.pointer_mut(pointer).unwrap() = "wrong".into();
            assert!(
                verify_subject(&bundle_for(&bad), &subject_name(tag), &digest).is_err(),
                "{pointer}"
            );
        }
        let mut two = statement.clone();
        two["subject"]
            .as_array_mut()
            .unwrap()
            .push(statement["subject"][0].clone());
        assert!(verify_subject(&bundle_for(&two), &subject_name(tag), &digest).is_err());
    }

    #[test]
    fn unsigned_oversized_and_malformed_signatures_are_refused() {
        for proof in [b"".as_slice(), b"{}", b"not json"] {
            assert!(verify("desktop-v1.0.0", b"sums", proof).is_err());
        }
        let oversized = vec![b' '; MAX_SIGNATURE as usize + 1];
        assert_eq!(
            verify("desktop-v1.0.0", b"sums", &oversized),
            Err("the release signature exceeds its size limit")
        );
    }
}
