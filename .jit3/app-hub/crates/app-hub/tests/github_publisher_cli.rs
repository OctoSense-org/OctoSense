mod common;
use common::Fixture;
use octosense_app_hub::{
    check_bundle, publisher_tools, Catalog, CatalogPublishers, HubKey, PublisherKeys, Store,
};
use octosense_app_policy::{AppManifest, HostLimits};
use serde_json::json;
use std::{fs, process::Command};
fn unsigned() -> Fixture {
    let mut f = Fixture::new();
    f.manifest.integrity.signature = None;
    f.write_manifest();
    f
}
fn prepare(f: &Fixture) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_hub"))
        .args([
            "publisher-prepare",
            f.bundle.to_str().unwrap(),
            "--repository",
            "example/app",
            "--repository-id",
            "123",
            "--owner-id",
            "456",
            "--workflow",
            ".github/workflows/publish-app.yml",
            "--tag",
            "v1.0.0",
            "--commit",
            &"a".repeat(40),
            "--out",
            f.root.join("octosense-app-manifest.json").to_str().unwrap(),
        ])
        .output()
        .unwrap()
}
fn read(f: &Fixture) -> AppManifest {
    AppManifest::parse(&fs::read_to_string(f.bundle.join("manifest.json")).unwrap()).unwrap()
}
#[test]
fn prepare_binds_final_bundle_without_any_developer_key() {
    let f = unsigned();
    let out = prepare(&f);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let m = read(&f);
    let subject = fs::read(f.root.join("octosense-app-manifest.json")).unwrap();
    assert_eq!(subject, m.signing_bytes().unwrap());
    assert_eq!(
        m.integrity.bundle_blake3,
        octosense_app_policy::digest_dir(&f.bundle).unwrap()
    );
    assert!(m.integrity.signature.is_none());
    assert!(m.integrity.github.unwrap().attestation.is_none());
    assert!(
        !prepare(&f).status.success(),
        "must not overwrite a prepared subject"
    );
}
#[test]
fn prepare_refuses_signed_input_and_output_inside_bundle() {
    let f = Fixture::new();
    let original = fs::read(f.bundle.join("manifest.json")).unwrap();
    assert!(!prepare(&f).status.success());
    assert_eq!(original, fs::read(f.bundle.join("manifest.json")).unwrap());
    let f = unsigned();
    let identity = serde_json::from_value(
        json!({"repository":"example/app","repository_id":"123","owner_id":"456",
        "workflow":".github/workflows/publish-app.yml","tag":"v1.0.0","commit":"a".repeat(40)}),
    )
    .unwrap();
    assert!(publisher_tools::prepare(
        &f.bundle,
        identity,
        &f.bundle.join("octosense-app-manifest.json")
    )
    .is_err());
}
#[test]
fn attach_rejects_unrelated_proof_and_post_prepare_edits_without_mutation() {
    let f = unsigned();
    assert!(prepare(&f).status.success());
    let path = f.root.join("proof.json");
    fs::write(
        &path,
        include_bytes!("fixtures/github-catalog/cosign-v3-blob.sigstore.json"),
    )
    .unwrap();
    let before = fs::read(f.bundle.join("manifest.json")).unwrap();
    let error = publisher_tools::attach(&f.bundle, &path).unwrap_err();
    assert!(error.contains("cryptographic"), "{error}");
    assert_eq!(before, fs::read(f.bundle.join("manifest.json")).unwrap());
    fs::write(f.bundle.join("main.splash"), "Label{text:\"tampered\"}").unwrap();
    assert!(publisher_tools::attach(&f.bundle, &path)
        .unwrap_err()
        .contains("changed after"));
}
#[test]
fn invalid_github_proof_is_not_unsigned_or_packable() {
    let f = unsigned();
    assert!(prepare(&f).status.success());
    let mut m = read(&f);
    m.integrity.github.as_mut().unwrap().attestation = Some(json!({"fake":true}));
    fs::write(
        f.bundle.join("manifest.json"),
        serde_json::to_vec(&m).unwrap(),
    )
    .unwrap();
    let report = check_bundle(
        &f.bundle,
        &HostLimits::default().with_require_signature(false),
        &PublisherKeys::new(),
        None,
    )
    .unwrap();
    assert!(!report.passed());
    assert!(report
        .findings
        .iter()
        .any(|f| f.check == "publisher-signature"));
    let output = f.root.join("release.pack.json");
    assert!(publisher_tools::pack(&f.bundle, None, &output).is_err());
    assert!(!output.exists());
    assert!(publisher_tools::verify(&f.bundle, None).is_err());
}
#[test]
fn local_catalog_signature_cannot_turn_an_invalid_publisher_proof_into_an_install() {
    let mut f = Fixture::new();
    let mut entry = f.entry();
    f.manifest.integrity.signature = None;
    f.write_manifest();
    assert!(prepare(&f).status.success());
    f.manifest = read(&f);
    f.manifest.integrity.github.as_mut().unwrap().attestation = Some(json!({"fake":true}));
    f.write_manifest();
    entry.manifest = f.manifest.clone();
    entry.publisher = "github:123".into();
    entry.publisher_key.clear();
    entry.source.repository = "https://github.com/example/app".into();
    entry.source.commit = "a".repeat(40);
    let mut catalog = Catalog::new(1, &octosense_app_hub::today(), vec![entry]);
    let anchor = HubKey::generate();
    let key = HubKey::generate();
    key.sign_catalog(&mut catalog, &anchor.certify(&key.public_hex()).unwrap())
        .unwrap();
    assert!(CatalogPublishers::from_catalog(&catalog).is_err());
    let mut store = Store::new(
        &anchor.public_hex(),
        &f.root.join("app-data"),
        HostLimits::default().with_require_signature(false),
    );
    assert!(store
        .accept_catalog(&serde_json::to_string(&catalog).unwrap())
        .is_err());
    assert!(store
        .install_staged(
            &f.manifest.id,
            &f.bundle,
            &PublisherKeys::new(),
            &octosense_app_hub::today()
        )
        .is_err());
}
#[test]
fn publisher_help_has_no_key_generation_or_migration_requirement() {
    let out = Command::new(env!("CARGO_BIN_EXE_hub"))
        .args(["publisher-prepare", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let help = String::from_utf8(out.stdout).unwrap();
    for command in [
        "publisher-prepare",
        "publisher-attach",
        "publisher-verify",
        "publisher-pack",
        "publisher-unpack",
        "publisher-entry",
    ] {
        assert!(help.contains(command));
    }
    assert!(!help.contains("publisher-migration"));
}

#[test]
fn invalid_release_unpack_cleans_only_its_owned_output() {
    let f = unsigned();
    assert!(prepare(&f).status.success());
    let pack = f.root.join("invalid.pack.json");
    fs::write(
        &pack,
        serde_json::to_vec(&octosense_app_hub::pack_dir(&f.bundle).unwrap()).unwrap(),
    )
    .unwrap();
    let output = f.root.join("unpacked");
    assert!(publisher_tools::unpack(&pack, None, &output).is_err());
    assert!(!output.exists());
    fs::create_dir(&output).unwrap();
    fs::write(output.join("keep.txt"), "owned by someone else").unwrap();
    assert!(publisher_tools::unpack(&pack, None, &output).is_err());
    assert!(output.join("keep.txt").exists());
}

#[test]
fn entry_builder_refuses_a_false_github_publisher_label_even_with_a_mock_gate() {
    struct GateOnlyVerifier;
    impl octosense_app_policy::SignatureVerifier for GateOnlyVerifier {
        fn verify(&self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
            Err("legacy refused".into())
        }
        fn verify_github(
            &self,
            _: &octosense_app_policy::manifest::GithubPublisher,
            _: &[u8],
        ) -> Result<(), String> {
            Ok(())
        }
    }
    let f = unsigned();
    assert!(prepare(&f).status.success());
    let mut m = read(&f);
    m.integrity.github.as_mut().unwrap().attestation = Some(json!({"mock":"gate-only"}));
    fs::write(
        f.bundle.join("manifest.json"),
        serde_json::to_vec(&m).unwrap(),
    )
    .unwrap();
    let report = check_bundle(&f.bundle, &HostLimits::default(), &GateOnlyVerifier, None).unwrap();
    assert!(report.passed());
    let error = octosense_app_hub::entry_for(
        &f.bundle,
        &report,
        "impostor",
        "",
        "https://github.com/example/app",
        &"a".repeat(40),
        "2026-10-08",
    )
    .unwrap_err();
    assert!(error.contains("publisher must be github:"), "{error}");
    // The entry boundary also rechecks real crypto: mock gate success cannot publish this proof.
    assert!(octosense_app_hub::entry_for(
        &f.bundle,
        &report,
        "github:123",
        "",
        "https://github.com/example/app",
        &"a".repeat(40),
        "2026-10-08"
    )
    .is_err());
}

#[test]
fn downloaded_pack_traversal_is_refused_before_writing_outside_output() {
    use base64::Engine;
    let f = unsigned();
    let path = f.root.join("bad.pack.json");
    fs::write(&path,serde_json::to_vec(&json!({"schema":1,"files":{"../escape.txt":base64::engine::general_purpose::STANDARD.encode("escape")}})).unwrap()).unwrap();
    let output = f.root.join("staged");
    assert!(publisher_tools::unpack(&path, None, &output).is_err());
    assert!(!output.exists());
    assert!(!f.root.join("escape.txt").exists());
}

#[test]
fn authoring_commands_preserve_prepared_and_attested_github_manifests() {
    let f = unsigned();
    assert!(prepare(&f).status.success());
    for attested in [false, true] {
        let mut manifest = read(&f);
        if attested {
            manifest.integrity.github.as_mut().unwrap().attestation =
                Some(json!({"fixture": true}));
            fs::write(
                f.bundle.join("manifest.json"),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .unwrap();
        }
        let before = fs::read(f.bundle.join("manifest.json")).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_hub"))
            .arg("stamp")
            .arg(&f.bundle)
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("cannot be restamped"));
        assert_eq!(fs::read(f.bundle.join("manifest.json")).unwrap(), before);
        let value = serde_json::to_value(&manifest).unwrap();
        assert!(
            octosense_app_hub::sign_manifest(&f.publisher, &mut manifest, "legacy-fixture")
                .is_err()
        );
        assert_eq!(serde_json::to_value(&manifest).unwrap(), value);
    }
}

#[test]
fn stamp_refuses_legacy_signatures_without_breaking_unsigned_null_signatures() {
    let f = Fixture::new();
    let before = fs::read(f.bundle.join("manifest.json")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_hub"))
        .arg("stamp")
        .arg(&f.bundle)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(fs::read(f.bundle.join("manifest.json")).unwrap(), before);
    let f = unsigned();
    assert!(serde_json::from_slice::<serde_json::Value>(
        &fs::read(f.bundle.join("manifest.json")).unwrap()
    )
    .unwrap()["integrity"]["signature"]
        .is_null());
    let out = Command::new(env!("CARGO_BIN_EXE_hub"))
        .arg("stamp")
        .arg(&f.bundle)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
