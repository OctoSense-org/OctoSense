//! `hub component-*` (App Hub ADR 0003): prepare a release from a component
//! file, pack and check it, build a candidate's index entry, and run a local
//! development hub that resolves an app's components.
mod common;
use common::*;
use octosense_app_hub::*;
use serde_json::Value;
use std::{fs, process::Command};

fn hub(fixture: &Fixture, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_hub")).current_dir(&fixture.root).args(args).output().expect("hub runs")
}

fn ok(output: &std::process::Output) -> String {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn failed(output: &std::process::Output) -> (String, String) {
    assert!(!output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
    (String::from_utf8_lossy(&output.stdout).into_owned(), String::from_utf8_lossy(&output.stderr).into_owned())
}

/// A publisher's draft and component file in the fixture's directory.
fn sources(f: &Fixture) {
    fs::write(f.root.join("markdown.wasm"), NOTES).unwrap();
    fs::write(f.root.join("component.json"), serde_json::to_vec_pretty(&draft_json("org.example.markdown", "1.0.0")).unwrap()).unwrap();
}

#[test]
fn prepare_describes_the_file_as_a_development_release_or_a_subject_to_attest() {
    let f = Fixture::new();
    sources(&f);
    let receipt: Value =
        serde_json::from_str(&ok(&hub(&f, &["component-prepare", "markdown.wasm", "--draft", "component.json", "--out", "release.json"]))).unwrap();
    assert_eq!(receipt["status"], "unsigned-development-release");
    assert_eq!(receipt["wasm_blake3"], blake3_hex(NOTES));
    assert_eq!(receipt["bytes"], NOTES.len());
    let release = ComponentRelease::read(&f.root.join("release.json")).unwrap();
    assert_eq!(release.component.wasm_blake3, blake3_hex(NOTES));
    assert!(release.component.integrity.github.is_none());
    // Never over an existing file.
    let (_, err) = failed(&hub(&f, &["component-prepare", "markdown.wasm", "--draft", "component.json", "--out", "release.json"]));
    assert!(err.contains("already exists"), "{err}");

    // With the GitHub identity: the canonical subject the workflow attests.
    let identity = [
        "--repository", "example/markdown", "--repository-id", "123", "--owner-id", "456",
        "--workflow", ".github/workflows/publish-component.yml", "--tag", "v1.0.0", "--commit",
    ];
    let commit = "a".repeat(40);
    let mut args = vec!["component-prepare", "markdown.wasm", "--draft", "component.json"];
    args.extend(identity);
    args.push(&commit);
    let mut named = args.clone();
    named.extend(["--out", "subject.json"]);
    let (_, err) = failed(&hub(&f, &named));
    assert!(err.contains("the attested subject is named octosense-component.json"), "{err}");
    fs::create_dir(f.root.join("build")).unwrap();
    args.extend(["--out", "build/octosense-component.json"]);
    let receipt: Value = serde_json::from_str(&ok(&hub(&f, &args))).unwrap();
    assert_eq!(receipt["status"], "awaiting-github-attestation");
    let subject = fs::read(f.root.join("build/octosense-component.json")).unwrap();
    use sha2::Digest;
    assert_eq!(receipt["sha256"], hex::encode(sha2::Sha256::digest(&subject)));
    let prepared: ComponentRelease = serde_json::from_slice(&subject).unwrap();
    assert_eq!(prepared.subject_bytes().unwrap(), subject, "the subject is canonical");
    assert_eq!(prepared.component.integrity.github.as_ref().unwrap().repository, "example/markdown");
    // A partial identity is refused, and so is a tag for another version.
    let (_, err) = failed(&hub(&f, &["component-prepare", "markdown.wasm", "--draft", "component.json", "--repository", "example/markdown", "--out", "x.json"]));
    assert!(err.contains("give all of --repository"), "{err}");
    // Packing the prepared subject without its proof is refused: the proof
    // must verify.
    let (_, err) = failed(&hub(&f, &["component-pack", "build/octosense-component.json", "--wasm", "markdown.wasm", "--out", "pack"]));
    assert!(err.contains("the component was refused"), "{err}");
    assert!(!f.root.join("pack").exists());
}

#[test]
fn check_and_pack_hold_a_release_to_its_file() {
    let f = Fixture::new();
    sources(&f);
    ok(&hub(&f, &["component-prepare", "markdown.wasm", "--draft", "component.json", "--out", "release.json"]));
    // App Hub refuses an unsigned release; a development check warns.
    let (stdout, _) = failed(&hub(&f, &["component-check", "release.json", "--wasm", "markdown.wasm"]));
    assert!(stdout.contains("[refused] publisher-signature: App Hub accepts only GitHub-attested component releases"), "{stdout}");
    let report = ok(&hub(&f, &["component-check", "release.json", "--wasm", "markdown.wasm", "--allow-unsigned"]));
    assert_eq!(
        report,
        format!(
            "org.example.markdown 1.0.0 (component {}) — PASSED\n  [warning] publisher-signature: unsigned: accountability rests on the hub alone\n  [warning] functions (org.example.markdown-1.0.0.wasm): org.example.markdown 1.0.0 is a component that reaches the clock, random numbers and files in its app folder, but no network or other app\n",
            blake3_hex(NOTES)
        )
    );
    let json: Value = serde_json::from_str(&ok(&hub(&f, &["component-check", "release.json", "--wasm", "markdown.wasm", "--allow-unsigned", "--json"]))).unwrap();
    assert_eq!((json["kind"].as_str(), json["passed"].as_bool()), (Some("component"), Some(true)));
    // Another file is not the release.
    fs::write(f.root.join("other.wasm"), FETCH).unwrap();
    let (stdout, _) = failed(&hub(&f, &["component-check", "release.json", "--wasm", "other.wasm", "--allow-unsigned"]));
    assert!(stdout.contains("[refused] digest: the file hashes to"), "{stdout}");

    // A development pack writes the release and its file, named for the hub.
    let receipt: Value =
        serde_json::from_str(&ok(&hub(&f, &["component-pack", "release.json", "--wasm", "markdown.wasm", "--allow-unsigned", "--out", "release"]))).unwrap();
    assert_eq!(receipt["status"], "unsigned-development-release");
    assert_eq!(fs::read(f.root.join("release/org.example.markdown-1.0.0.wasm")).unwrap(), NOTES);
    let packed = ComponentRelease::read(&f.root.join("release/org.example.markdown-1.0.0.component.json")).unwrap();
    assert_eq!(packed, ComponentRelease::read(&f.root.join("release.json")).unwrap());
    let (_, err) = failed(&hub(&f, &["component-pack", "release.json", "--wasm", "markdown.wasm", "--allow-unsigned", "--out", "release"]));
    assert!(err.contains("needs a new output directory"), "{err}");
}

#[test]
fn a_development_hub_publishes_a_component_and_resolves_an_app_against_it() {
    let mut f = Fixture::new();
    sources(&f);
    ok(&hub(&f, &["component-prepare", "markdown.wasm", "--draft", "component.json", "--out", "release.json"]));
    let anchor = HubKey::generate();
    let working = HubKey::generate();
    fs::write(f.root.join("working.key"), hex::encode(working.to_bytes())).unwrap();
    let certificate = anchor.certify(&working.public_hex()).unwrap();
    let published = ok(&hub(
        &f,
        &[
            "component-publish", "release.json", "--wasm", "markdown.wasm", "--catalog", "catalog.json", "--key", "working.key",
            "--anchor-cert", &certificate, "--publisher", "dev:example", "--out", "hub", "--allow-unsigned",
        ],
    ));
    assert!(published.ends_with("published component org.example.markdown 1.0.0 (catalog sequence 1)\n"), "{published}");
    assert_eq!(fs::read(f.root.join("hub/artifacts/org.example.markdown-1.0.0.wasm")).unwrap(), NOTES);
    let catalog: Catalog = serde_json::from_slice(&fs::read(f.root.join("catalog.json")).unwrap()).unwrap();
    verify_catalog(&catalog, &anchor.public_hex()).unwrap();
    let entry = catalog.component("org.example.markdown", "1.0.0").unwrap().clone();
    assert_eq!(entry.publisher, "dev:example");
    // The same version again is refused.
    let (stdout, _) = failed(&hub(
        &f,
        &[
            "component-publish", "release.json", "--wasm", "markdown.wasm", "--catalog", "catalog.json", "--key", "working.key",
            "--anchor-cert", &certificate, "--publisher", "dev:example", "--anchor", &anchor.public_hex(), "--allow-unsigned",
        ],
    ));
    assert!(stdout.contains("[refused] version: version 1.0.0 of component org.example.markdown is already published"), "{stdout}");

    // An app that pins it resolves against this catalog.
    f.pin(&[dependency("markdown", &entry)], &["storage"], &[]);
    let key = format!("publisher-one={}", f.publisher.public_hex());
    let report = ok(&hub(&f, &["check", "bundle", "--catalog", "catalog.json", "--anchor", &anchor.public_hex(), "--publisher-key", &key]));
    assert!(
        report.contains(
            "[warning] components: component markdown (org.example.markdown 1.0.0) reaches the clock, random numbers and files in its app folder, but no network or other app"
        ),
        "{report}"
    );
    // Withdrawn, it refuses the app.
    let withdrawn = ok(&hub(
        &f,
        &[
            "withdraw", "org.example.markdown", "--version", "1.0.0", "--reason", "Renders scripts it should escape",
            "--catalog", "catalog.json", "--key", "working.key", "--anchor-cert", &certificate,
        ],
    ));
    assert!(withdrawn.contains("withdrew org.example.markdown 1.0.0"), "{withdrawn}");
    let (stdout, _) = failed(&hub(&f, &["check", "bundle", "--catalog", "catalog.json", "--anchor", &anchor.public_hex(), "--publisher-key", &key]));
    assert!(
        stdout.contains("[refused] components: component markdown (org.example.markdown 1.0.0) was withdrawn: Renders scripts it should escape"),
        "{stdout}"
    );
}

#[test]
fn an_index_entry_needs_github_provenance() {
    let f = Fixture::new();
    sources(&f);
    ok(&hub(&f, &["component-prepare", "markdown.wasm", "--draft", "component.json", "--out", "release.json"]));
    let (_, err) = failed(&hub(&f, &["component-entry", "release.json", "--wasm", "markdown.wasm", "--out", "index.json"]));
    assert!(err.contains("--catalog <authenticated catalog> is required"), "{err}");
    fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/../../catalog.json"), f.root.join("base.json")).unwrap();
    let (stdout, err) = failed(&hub(&f, &["component-entry", "release.json", "--wasm", "markdown.wasm", "--catalog", "base.json", "--out", "index.json"]));
    assert!(stdout.contains("[refused] publisher-signature: App Hub accepts only GitHub-attested component releases"), "{stdout}");
    assert!(err.contains("the component was refused"), "{err}");
    assert!(!f.root.join("index.json").exists());
}
