//! Prepare one reviewed candidate adding real GitHub-attested components and
//! their consuming apps together. Never signs, publishes, or runs their code.
//!
//! cargo run --locked -p octosense-app-hub --example shared_component_candidate -- \
//!   catalog-v2.json downloaded-release-files new-candidate-directory
//!
//! RELEASES contains *.component.json with the matching *.wasm and one or more
//! *.bundle.pack.json. The authenticated base remains unchanged. The output is
//! untrusted candidate data until the protected admin workflow attests it.
use octosense_app_hub::{
    admission, check_bundle, check_component, component_entry_for,
    components::{self, ComponentRelease},
    entry_for, github_catalog, publisher_tools, CatalogPublishers, PublisherKeys,
};
use octosense_app_policy::{AppManifest, HostLimits};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    fs::create_dir_all(path.parent().ok_or("missing output parent")?).map_err(|e| e.to_string())?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|e| e.to_string())
}

fn prepare(base: &Path, releases: &Path, output: &Path) -> Result<serde_json::Value, String> {
    let base_bytes = admission::read_bounded(base, github_catalog::MAX_DOCUMENT_BYTES as u64)?;
    // Explicit v2 proof, with no fallback to a legacy or unsigned catalog.
    github_catalog::verify_document(&base_bytes)?;
    let authenticated = github_catalog::authenticated_publication_base(
        &base_bytes,
        octosense_app_hub::DEFAULT_ANCHOR,
    )?;
    let mut candidate = authenticated.catalog().clone();
    candidate.sequence = candidate
        .sequence
        .checked_add(1)
        .ok_or("catalog sequence overflow")?;
    candidate.published = octosense_app_hub::today();
    candidate.key = None;
    candidate.signature = None;
    let mut components = Vec::new();
    let mut apps = Vec::new();
    for file in fs::read_dir(releases).map_err(|e| e.to_string())? {
        let file = file.map_err(|e| e.to_string())?;
        if !file.file_type().map_err(|e| e.to_string())?.is_file() {
            continue;
        }
        let name = file.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.ends_with(".component.json") {
            components.push(file.path());
        }
        if name.ends_with(".bundle.pack.json") {
            apps.push(file.path());
        }
    }
    components.sort();
    apps.sort();
    if components.is_empty() || apps.is_empty() {
        return Err(
            "release directory must contain both component releases and consumer packs".into(),
        );
    }
    // Ownership begins only after create_dir succeeds. Never clean up a path
    // that existed before this command, including a symlink or another run.
    fs::create_dir(output).map_err(|e| format!("use a new candidate directory: {e}"))?;
    let result = (|| {
        for path in components {
            let release = ComponentRelease::read(&path)?;
            let github = release
                .component
                .integrity
                .github
                .as_ref()
                .ok_or("component needs real GitHub publisher provenance")?;
            let wasm = releases.join(format!(
                "{}-{}.wasm",
                release.component.id, release.component.version
            ));
            let bytes = admission::read_bounded(&wasm, components::MAX_COMPONENT_BYTES)?;
            let report = check_component(&release, &bytes, true, Some(&candidate))?;
            if !report.passed() {
                return Err(report.render());
            }
            let entry = component_entry_for(
                &release,
                &bytes,
                &report,
                &format!("github:{}", github.repository_id),
                &github.repository_url(),
                &github.commit,
                &candidate.published,
            )?;
            write(&output.join(&entry.artifact), &bytes)?;
            write(
                &output.join(components::index_path(entry.id(), entry.version())),
                &serde_json::to_vec_pretty(&entry).map_err(|e| e.to_string())?,
            )?;
            candidate.components.push(entry);
        }
        let keys = authenticated
            .publishers()
            .trusted_keys(PublisherKeys::new());
        for (number, pack) in apps.iter().enumerate() {
            let staged = output.join(format!(".staging-{number}"));
            // Unpack verifies the real publisher proof, even before dependencies
            // are resolved against the complete candidate in the next step.
            publisher_tools::unpack(pack, None, &staged)?;
            let manifest = AppManifest::parse(&admission::read_text(
                &staged.join("manifest.json"),
                admission::MAX_MANIFEST_BYTES,
            )?)?;
            let github = manifest
                .integrity
                .github
                .as_ref()
                .ok_or("app needs real GitHub publisher provenance")?;
            let report = check_bundle(&staged, &HostLimits::default(), &keys, Some(&candidate))?;
            if !report.passed() {
                return Err(report.render());
            }
            let entry = entry_for(
                &staged,
                &report,
                &format!("github:{}", github.repository_id),
                "",
                &github.repository_url(),
                &github.commit,
                &candidate.published,
            )?;
            let destination = output.join(&entry.artifact);
            fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
            fs::rename(&staged, &destination).map_err(|e| e.to_string())?;
            write(
                &output.join(format!("{}.pack.json", entry.artifact)),
                &admission::read_bounded(pack, github_catalog::MAX_DOCUMENT_BYTES as u64)?,
            )?;
            write(
                &output.join(format!("index/{}-{}.json", entry.app_id(), entry.version())),
                &serde_json::to_vec_pretty(&entry).map_err(|e| e.to_string())?,
            )?;
            candidate.entries.push(entry);
            CatalogPublishers::from_catalog(&candidate)?;
        }
        let bytes = serde_json::to_vec_pretty(&candidate).map_err(|e| e.to_string())?;
        // Same final verification as hub catalog-prepare, including both real
        // proof classes, immutable history, all indexes and exact packed bytes.
        let payload = github_catalog::prepare_authenticated(&authenticated, &bytes, output)?;
        write(&output.join("catalog.json"), &bytes)?;
        write(&output.join(github_catalog::CATALOG_SUBJECT), &payload)?;
        let receipt = json!({"schema":1,"status":"candidate-only-awaiting-admin-attestation",
            "base_sequence":authenticated.catalog().sequence,"sequence":candidate.sequence,
            "base_sha256":hex::encode(Sha256::digest(&base_bytes)),
            "candidate_sha256":hex::encode(Sha256::digest(&bytes)),
            "payload_sha256":hex::encode(Sha256::digest(&payload)),
            "added_artifacts":github_catalog::added_artifacts(authenticated.catalog(),&candidate),
            "published":false,"executed_app_code":false});
        write(
            &output.join("prepare-receipt.json"),
            &serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
        )?;
        Ok(receipt)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(output);
    }
    result
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let answer = match args.as_slice() {
        [base, releases, output] => {
            prepare(Path::new(base), Path::new(releases), Path::new(output))
        }
        _ => Err(
            "usage: shared_component_candidate BASE-V2.json RELEASE-DIRECTORY NEW-OUTPUT-DIRECTORY"
                .into(),
        ),
    };
    match answer {
        Ok(receipt) => println!("{receipt}"),
        Err(error) => {
            eprintln!("shared component candidate: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fixture() -> Scratch {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = Scratch(std::env::temp_dir().join(format!(
            "shared-candidate-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fs::create_dir(&root.0).unwrap();
        // This is a genuine public admin proof, not a test verifier.
        fs::write(
            root.0.join("base.json"),
            include_bytes!("../../../catalog-v2.json"),
        )
        .unwrap();
        fs::create_dir(root.0.join("releases")).unwrap();
        root
    }
    #[test]
    fn unsigned_base_never_creates_a_candidate() {
        let root = fixture();
        fs::write(root.0.join("base.json"), "{}").unwrap();
        assert!(prepare(
            &root.0.join("base.json"),
            &root.0.join("releases"),
            &root.0.join("out")
        )
        .is_err());
        assert!(!root.0.join("out").exists());
    }
    #[test]
    fn unsigned_component_is_refused_and_its_owned_output_is_removed() {
        let root = fixture();
        let release=ComponentRelease::from_draft(serde_json::from_value(json!({
            "component":{"schema":1,"id":"org.example.candidaterehearsal","version":"1.0.0","name":"Fixture","license":"MIT",
                "publisher":{"name":"Example","support":"https://example.test/s","privacy_policy_url":"https://example.test/p"}},
            "listing":{"description":"Unsigned negative fixture"}
        })).unwrap(),include_bytes!("../tests/fixtures/notes.component.wasm")).unwrap();
        fs::write(
            root.0.join("releases/unsigned.component.json"),
            serde_json::to_vec(&release).unwrap(),
        )
        .unwrap();
        fs::write(root.0.join("releases/app.bundle.pack.json"), "{}").unwrap();
        let error = prepare(
            &root.0.join("base.json"),
            &root.0.join("releases"),
            &root.0.join("out"),
        )
        .unwrap_err();
        assert!(
            error.contains("real GitHub publisher provenance"),
            "{error}"
        );
        assert!(!root.0.join("out").exists());
    }
    #[test]
    fn never_replaces_or_removes_existing_output() {
        let root = fixture();
        fs::write(root.0.join("releases/unsigned.component.json"), "{}").unwrap();
        fs::write(root.0.join("releases/app.bundle.pack.json"), "{}").unwrap();
        fs::create_dir(root.0.join("out")).unwrap();
        fs::write(root.0.join("out/keep.txt"), "owned before this invocation").unwrap();
        let error = prepare(
            &root.0.join("base.json"),
            &root.0.join("releases"),
            &root.0.join("out"),
        )
        .unwrap_err();
        assert!(error.contains("new candidate directory"), "{error}");
        assert_eq!(
            fs::read_to_string(root.0.join("out/keep.txt")).unwrap(),
            "owned before this invocation"
        );
    }
}
