use octosense_app_contract::{AppManifest, HostLimits, RefuseAllSignatures};
use serde_json::{json, Value};

fn manifest() -> Value {
    json!({"schema":1,"id":"example.app","name":"Example","version":"1.2.3",
      "requires":["publisher-github-v1"],"integrity":{"bundle_blake3":"00","github":{
        "repository":"example/app","repository_id":"123","owner_id":"456",
        "workflow":".github/workflows/publish-app.yml","tag":"v1.2.3","commit":"a".repeat(40)}}})
}
#[test]
fn keyless_marker_and_identity_are_inseparable() {
    let good = manifest();
    AppManifest::parse(&good.to_string()).unwrap();
    let mut missing_marker = good.clone();
    missing_marker["requires"] = json!([]);
    assert!(AppManifest::parse(&missing_marker.to_string())
        .unwrap_err()
        .contains("declared together"));
    let mut missing_identity = good;
    missing_identity["integrity"]
        .as_object_mut()
        .unwrap()
        .remove("github");
    assert!(AppManifest::parse(&missing_identity.to_string()).is_err());
}
#[test]
fn identity_and_tag_are_canonical_and_version_bound() {
    for (field, bad) in [
        ("repository", "evil.example/o/r"),
        ("repository_id", "0123"),
        ("owner_id", "0"),
        ("workflow", ".github/workflows/../evil.yml"),
        ("tag", "v9.9.9"),
        ("commit", "main"),
    ] {
        let mut value = manifest();
        value["integrity"]["github"][field] = json!(bad);
        assert!(AppManifest::parse(&value.to_string()).is_err(), "{field}");
    }
}
#[test]
fn signing_bytes_exclude_only_proof_and_retain_identity() {
    let value = manifest();
    let first = AppManifest::parse(&value.to_string()).unwrap();
    let mut value = value;
    value["integrity"]["github"]["attestation"] = json!({"test":"not a valid proof"});
    let sealed = AppManifest::parse(&value.to_string()).unwrap();
    assert_eq!(
        first.signing_bytes().unwrap(),
        sealed.signing_bytes().unwrap()
    );
    value["integrity"]["github"]["repository_id"] = json!("999");
    assert_ne!(
        first.signing_bytes().unwrap(),
        AppManifest::parse(&value.to_string())
            .unwrap()
            .signing_bytes()
            .unwrap()
    );
}
#[test]
fn contract_without_a_real_verifier_refuses_even_in_unsigned_mode() {
    let mut value = manifest();
    let m = AppManifest::parse(&value.to_string()).unwrap();
    assert!(octosense_app_contract::resolve(&m, &HostLimits::default()).is_err());
    value["integrity"]["github"]["attestation"] = json!({"fake":true});
    let m = AppManifest::parse(&value.to_string()).unwrap();
    assert!(
        octosense_app_contract::verify::admit_digest(&m, "00", &RefuseAllSignatures)
            .unwrap_err()
            .contains("no GitHub publisher verifier")
    );
    assert!(octosense_app_contract::resolve(
        &m,
        &HostLimits::default().with_require_signature(false)
    )
    .is_ok());
    // Policy resolution never replaces mandatory integrity admission above.
}
#[test]
fn legacy_manifest_signing_bytes_have_no_new_field() {
    let m = AppManifest::parse(r#"{"schema":1,"id":"example","version":"1","name":"Example","integrity":{"bundle_blake3":"00"}}"#).unwrap();
    assert!(!String::from_utf8(m.signing_bytes().unwrap())
        .unwrap()
        .contains("github"));
}
