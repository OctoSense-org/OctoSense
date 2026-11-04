//! `hub` — the command the publisher and the hub both run.
//!
//! ```sh
//! hub keygen <path>                       # a signing key (anchor or working)
//! hub certify --anchor <key> --working <key>
//! hub stamp <bundle>                      # write the bundle digest into its manifest
//! hub sign-manifest <bundle> --key <key> --key-id <id>
//! hub check <bundle> [--catalog <file> [--anchor <hex>]] [--allow-unsigned] [--publisher-key id=hex] [--json]
//! hub publish <bundle> --catalog <file> --key <working> --anchor-cert <hex>
//!             --publisher <id> --repo <url> --commit <sha> [--out <dir>] [--anchor <hex>]
//! hub verify <catalog> --anchor <hex>
//! hub component-info <file.wasm>          # a function file's kind, imports and exports
//! hub component-prepare|pack|check|entry|publish   # shared components (ADR 0003)
//! ```
//!
//! `check` is the gate: a developer runs it before submitting and sees the
//! same report the hub's job produces. `publish` runs the gate again, copies
//! the bundle into the artifact store and signs the catalog.
//!
//! An existing `--catalog` is history only once it verifies against the
//! anchor (`--anchor`, default [`DEFAULT_ANCHOR`]): its entries decide which
//! key each known publisher signs with, and a `--publisher-key` cannot
//! replace one.
use octosense_app_hub::*;
use octosense_app_hub::scan::Route;
use octosense_app_policy::{AppManifest, HostLimits};
use std::path::{Path, PathBuf};

fn main() {
    if let Err(e) = run() {
        eprintln!("hub: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let argv: Vec<String> = std::env::args().collect();
    let command = argv.get(1).map(String::as_str).unwrap_or("help");
    // Help never treats a flag as a bundle or key path and never mutates files.
    if command == "help" || argv.iter().skip(1).any(|arg| matches!(arg.as_str(), "--help" | "-h")) {
        println!("{}", include_str!("hub-usage.txt"));
        return Ok(());
    }
    let flag = |name: &str| argv.windows(2).find(|w| w[0] == format!("--{name}")).map(|w| w[1].clone());
    let has = |name: &str| argv.iter().any(|a| a == &format!("--{name}"));
    let positional = argv.get(2).filter(|arg| !arg.starts_with('-')).cloned();
    if has("system-app") && !matches!(command, "check" | "scan") {
        return Err("--system-app is only supported by check and scan; system apps cannot be published".into());
    }

    match command {
        "publisher-prepare" => {
            let bundle=PathBuf::from(positional.ok_or("publisher-prepare <bundle>")?);
            let required=|name:&str|flag(name).ok_or_else(||format!("--{name} is required"));
            let identity=serde_json::from_value(serde_json::json!({
                "repository":required("repository")?,"repository_id":required("repository-id")?,"owner_id":required("owner-id")?,
                "workflow":required("workflow")?,"tag":required("tag")?,"commit":required("commit")?
            })).map_err(|e|e.to_string())?;
            let receipt=publisher_tools::prepare(&bundle,identity,Path::new(&required("out")?))?;
            println!("{receipt}"); Ok(())
        }
        "publisher-attach" => {
            let bundle=PathBuf::from(positional.ok_or("publisher-attach <bundle>")?);
            println!("{}",publisher_tools::attach(&bundle,Path::new(&flag("attestation").ok_or("--attestation <bundle.json>")?))?); Ok(())
        }
        "publisher-verify" | "publisher-pack" | "publisher-unpack" | "publisher-entry" => {
            let bundle=PathBuf::from(positional.ok_or("publisher command requires <bundle>")?);
            let base=flag("catalog").map(|path|{
                let bytes=admission::read_bounded(Path::new(&path),github_catalog::MAX_DOCUMENT_BYTES as u64)?;
                github_catalog::authenticated_publication_base(&bytes,&flag("anchor").unwrap_or_else(||DEFAULT_ANCHOR.into()))
            }).transpose()?;
            if command=="publisher-verify" {println!("{}",publisher_tools::verify(&bundle,base.as_ref())?.json());return Ok(());}
            let output=PathBuf::from(flag("out").ok_or("--out <outside-bundle file>")?);
            if command=="publisher-pack" {println!("{}",publisher_tools::pack(&bundle,base.as_ref(),&output)?);return Ok(());}
            if command=="publisher-unpack" {println!("{}",publisher_tools::unpack(&bundle,base.as_ref(),&output)?.json());return Ok(());}
            let base=base.as_ref().ok_or("--catalog <authenticated catalog> is required")?;
            publisher_tools::entry(&bundle,base,&output)?;
            println!("{}",serde_json::json!({"schema":1,"status":"candidate-only-awaiting-admin-catalog-approval"})); Ok(())
        }
        "catalog-prepare" => {
            use sha2::{Digest, Sha256};
            let read = |name: &str, max| -> Result<Vec<u8>, String> {
                admission::read_bounded(Path::new(&flag(name).ok_or_else(|| format!("--{name} <file>"))?), max)
            };
            let base_bytes = read("base", github_catalog::MAX_DOCUMENT_BYTES as u64)?;
            let authenticated = github_catalog::authenticated_publication_base(&base_bytes,DEFAULT_ANCHOR)?;
            let base=authenticated.catalog();
            let candidate = read("candidate", github_catalog::MAX_CATALOG_BYTES as u64)?;
            let root = PathBuf::from(flag("artifact-root").ok_or("--artifact-root <reviewed directory>")?);
            let payload = github_catalog::prepare_authenticated(&authenticated, &candidate, &root)?;
            let output = PathBuf::from(flag("out").ok_or("--out <catalog-v2.payload.json>")?);
            if output.file_name().and_then(|v| v.to_str()) != Some(github_catalog::CATALOG_SUBJECT) {
                return Err("prepared catalog filename must be catalog-v2.payload.json".into());
            }
            let catalog: Catalog = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
            let added = github_catalog::added_artifacts(base, &catalog);
            std::fs::write(&output, &payload).map_err(|e| e.to_string())?;
            println!("{}", serde_json::json!({"schema":1,"base_sequence":base.sequence,"sequence":catalog.sequence,
                "base_sha256":hex::encode(Sha256::digest(&base_bytes)),"candidate_sha256":hex::encode(Sha256::digest(&candidate)),
                "payload_sha256":hex::encode(Sha256::digest(&payload)),"subject":github_catalog::CATALOG_SUBJECT,"added_artifacts":added}));
            Ok(())
        }
        "catalog-envelope" => {
            let payload = admission::read_bounded(Path::new(&flag("catalog").ok_or("--catalog <catalog-v2.payload.json>")?), github_catalog::MAX_CATALOG_BYTES as u64)?;
            let proof = admission::read_bounded(Path::new(&flag("attestation").ok_or("--attestation <bundle.json>")?), github_catalog::MAX_PROOF_BYTES as u64)?;
            let envelope = github_catalog::envelope(&payload, &proof)?;
            let verified = github_catalog::verify_document(envelope.as_bytes())?;
            std::fs::write(flag("out").ok_or("--out <catalog-v2.json>")?, envelope.as_bytes()).map_err(|e| e.to_string())?;
            println!("{}", serde_json::json!({"schema":2,"sequence":verified.catalog().sequence,"payload_sha256":verified.sha256()}));
            Ok(())
        }
        "catalog-verify" => {
            let document = admission::read_bounded(Path::new(&positional.ok_or("usage: hub catalog-verify <catalog-v2.json>")?), github_catalog::MAX_DOCUMENT_BYTES as u64)?;
            let verified = github_catalog::verify_document(&document)?;
            println!("{}", serde_json::json!({"schema":2,"sequence":verified.catalog().sequence,"payload_sha256":verified.sha256()}));
            Ok(())
        }
        "keygen" => {
            let path = positional.ok_or("usage: hub keygen <path>")?;
            let key = HubKey::generate();
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&path).map_err(|e| format!("cannot create new signing key {path:?}: {e}"))?;
            use std::io::Write;
            file.write_all(hex::encode(key.to_bytes()).as_bytes()).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
            println!("{}", key.public_hex());
            Ok(())
        }
        "pubkey" => {
            let key = load_key(&positional.ok_or("usage: hub pubkey <key file>")?)?;
            println!("{}", key.public_hex());
            Ok(())
        }
        "component-info" => {
            // What a function file is and what it reaches (ADR 0014): kind,
            // imports and exports, each with its WIT parameters and result.
            let path = positional.ok_or("usage: hub component-info <file.wasm>")?;
            let bytes = admission::read_bounded(Path::new(&path), gate::MAX_BUNDLE_BYTES)?;
            let info = functions::describe(&bytes).map_err(|e| format!("{path}: {e}"))?;
            println!("{}", serde_json::to_string_pretty(&info).map_err(|e| e.to_string())?);
            Ok(())
        }
        // Shared components (App Hub ADR 0003): a release is the component's
        // manifest and listing (`<id>-<version>.component.json`) beside its
        // `.wasm` file. prepare describes the file, pack seals and verifies a
        // release, check is the gate, entry builds a candidate's index entry
        // and publish adds one to a legacy-signed (development) catalog.
        "component-prepare" => {
            let wasm_path = positional.ok_or("usage: hub component-prepare <file.wasm> --draft <component.json> --out <file>")?;
            let wasm = admission::read_bounded(Path::new(&wasm_path), components::MAX_COMPONENT_BYTES)?;
            let draft_path = flag("draft").ok_or("--draft <component.json>")?;
            let draft: components::ComponentDraft = serde_json::from_slice(&admission::read_bounded(Path::new(&draft_path), components::MAX_RELEASE_BYTES)?)
                .map_err(|e| format!("{draft_path}: {e}"))?;
            let mut release = components::ComponentRelease::from_draft(draft, &wasm).map_err(|e| format!("{wasm_path}: {e}"))?;
            let output = PathBuf::from(flag("out").ok_or("--out <file>")?);
            if std::fs::symlink_metadata(&output).is_ok() {
                return Err(format!("{} already exists; refusing to overwrite", output.display()));
            }
            let names = ["repository", "repository-id", "owner-id", "workflow", "tag", "commit"];
            let given: Vec<&str> = names.iter().copied().filter(|name| flag(name).is_some()).collect();
            let c = &release.component;
            let mut receipt = serde_json::json!({"schema":1,"id":c.id,"version":c.version,"wasm_blake3":c.wasm_blake3,"bytes":c.bytes});
            if given.is_empty() {
                write_new(&output, format!("{}\n", serde_json::to_string_pretty(&release).map_err(|e| e.to_string())?).as_bytes())?;
                receipt["status"] = "unsigned-development-release".into();
            } else if given.len() == names.len() {
                if output.file_name().and_then(|n| n.to_str()) != Some(components::COMPONENT_SUBJECT) {
                    return Err(format!("the attested subject is named {}", components::COMPONENT_SUBJECT));
                }
                let identity: octosense_app_policy::manifest::GithubPublisher = serde_json::from_value(serde_json::json!({
                    "repository":flag("repository"),"repository_id":flag("repository-id"),"owner_id":flag("owner-id"),
                    "workflow":flag("workflow"),"tag":flag("tag"),"commit":flag("commit")
                })).map_err(|e| e.to_string())?;
                identity.validate(&release.component.version)?;
                release.component.integrity.github = Some(identity);
                let bytes = release.subject_bytes()?;
                write_new(&output, &bytes)?;
                use sha2::{Digest, Sha256};
                receipt["subject"] = components::COMPONENT_SUBJECT.into();
                receipt["sha256"] = hex::encode(Sha256::digest(&bytes)).into();
                receipt["status"] = "awaiting-github-attestation".into();
            } else {
                return Err(format!("give all of --{}, or none for an unsigned development release", names.join(", --")));
            }
            println!("{receipt}");
            Ok(())
        }
        "component-pack" => {
            let release_path = positional.ok_or("usage: hub component-pack <release.json> --wasm <file.wasm> [--attestation <bundle.json>] --out <new directory>")?;
            let mut release = components::ComponentRelease::read(Path::new(&release_path))?;
            let wasm = admission::read_bounded(Path::new(&flag("wasm").ok_or("--wasm <file.wasm>")?), components::MAX_COMPONENT_BYTES)?;
            if let Some(proof) = flag("attestation") {
                let github = release.component.integrity.github.as_mut().ok_or("prepare the release with its GitHub identity before attaching a proof")?;
                if github.attestation.is_some() {
                    return Err("the release already carries a proof".into());
                }
                let bytes = admission::read_bounded(Path::new(&proof), github_publisher::MAX_PROOF_BYTES as u64)?;
                github.attestation = Some(serde_json::from_slice(&bytes).map_err(|_| "invalid publisher attestation JSON")?);
            }
            let report = check_component(&release, &wasm, !has("allow-unsigned"), None)?;
            if !report.passed() {
                print!("{}", report.render());
                return Err("the component was refused".into());
            }
            let output = PathBuf::from(flag("out").ok_or("--out <new directory>")?);
            std::fs::create_dir(&output).map_err(|e| format!("component-pack needs a new output directory: {e}"))?;
            let stem = format!("{}-{}", release.component.id, release.component.version);
            let release_file = output.join(format!("{stem}.component.json"));
            let wasm_file = output.join(format!("{stem}.wasm"));
            write_new(&release_file, format!("{}\n", serde_json::to_string_pretty(&release).map_err(|e| e.to_string())?).as_bytes())?;
            write_new(&wasm_file, &wasm)?;
            println!("{}", serde_json::json!({"schema":1,"id":report.id,"version":report.version,"wasm_blake3":report.digest,
                "release":release_file,"wasm":wasm_file,
                "status":if release.component.integrity.github.is_some() {"verified-release-not-published"} else {"unsigned-development-release"}}));
            Ok(())
        }
        "component-check" | "component-entry" => {
            let release_path = positional.ok_or_else(|| format!("usage: hub {command} <release.json> --wasm <file.wasm>"))?;
            let release = components::ComponentRelease::read(Path::new(&release_path))?;
            let wasm = admission::read_bounded(Path::new(&flag("wasm").ok_or("--wasm <file.wasm>")?), components::MAX_COMPONENT_BYTES)?;
            let base = flag("catalog").map(|path| {
                let bytes = admission::read_bounded(Path::new(&path), github_catalog::MAX_DOCUMENT_BYTES as u64)?;
                github_catalog::authenticated_publication_base(&bytes, &flag("anchor").unwrap_or_else(|| DEFAULT_ANCHOR.into()))
            }).transpose()?;
            if command == "component-check" {
                let report = check_component(&release, &wasm, !has("allow-unsigned"), base.as_ref().map(|b| b.catalog()))?;
                if has("json") { println!("{}", report.json()); } else { print!("{}", report.render()); }
                return if report.passed() { Ok(()) } else { Err("the component was refused".into()) };
            }
            let base = base.ok_or("--catalog <authenticated catalog> is required")?;
            let output = PathBuf::from(flag("out").ok_or("--out <index.json>")?);
            if std::fs::symlink_metadata(&output).is_ok() {
                return Err(format!("{} already exists; refusing to overwrite", output.display()));
            }
            let report = check_component(&release, &wasm, true, Some(base.catalog()))?;
            if !report.passed() {
                print!("{}", report.render());
                return Err("the component was refused".into());
            }
            let github = release.component.integrity.github.as_ref().ok_or("entry requires a GitHub publisher")?;
            let entry = component_entry_for(&release, &wasm, &report, &format!("github:{}", github.repository_id), &github.repository_url(), &github.commit, &today())?;
            write_new(&output, &serde_json::to_vec_pretty(&entry).map_err(|e| e.to_string())?)?;
            println!("{}", serde_json::json!({"schema":1,"artifact":entry.artifact,"index":components::index_path(entry.id(), entry.version()),
                "status":"candidate-only-awaiting-admin-catalog-approval"}));
            Ok(())
        }
        "component-publish" => {
            let release_path = positional.ok_or("usage: hub component-publish <release.json> --wasm <file.wasm> --catalog <f> --key <f> --anchor-cert <hex> --publisher <id>")?;
            let release = components::ComponentRelease::read(Path::new(&release_path))?;
            let wasm = admission::read_bounded(Path::new(&flag("wasm").ok_or("--wasm <file.wasm>")?), components::MAX_COMPONENT_BYTES)?;
            let catalog_path = PathBuf::from(flag("catalog").ok_or("--catalog <file>")?);
            let working = load_key(&flag("key").ok_or("--key <working key file>")?)?;
            let anchor_certificate = flag("anchor-cert").ok_or("--anchor-cert <hex>")?;
            let publisher = flag("publisher").ok_or("--publisher <id>")?;
            let out = PathBuf::from(flag("out").unwrap_or_else(|| ".".into()));
            let mut catalog = if catalog_path.exists() { trusted_catalog(&catalog_path, &argv)? } else { Catalog::new(0, &today(), Vec::new()) };
            let report = check_component(&release, &wasm, !has("allow-unsigned"), Some(&catalog))?;
            print!("{}", report.render());
            if !report.passed() {
                return Err("refusing to publish a component the gate refused".into());
            }
            let entry = component_entry_for(&release, &wasm, &report, &publisher, &flag("repo").unwrap_or_default(), &flag("commit").unwrap_or_default(), &today())?;
            let artifact = out.join(&entry.artifact);
            if let Some(parent) = artifact.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&artifact, &wasm).map_err(|e| format!("{}: {e}", artifact.display()))?;
            catalog.components.push(entry);
            catalog.sequence += 1;
            catalog.published = today();
            working.sign_catalog(&mut catalog, &anchor_certificate)?;
            write_json(&catalog_path, &serde_json::to_value(&catalog).map_err(|e| e.to_string())?)?;
            println!("published component {} {} (catalog sequence {})", report.id, report.version, catalog.sequence);
            Ok(())
        }
        "certify" => {
            let anchor = load_key(&flag("anchor").ok_or("--anchor <key file>")?)?;
            let working = load_key(&flag("working").ok_or("--working <key file>")?)?;
            println!("{}", anchor.certify(&working.public_hex())?);
            Ok(())
        }
        "stamp" => {
            let bundle = PathBuf::from(positional.ok_or("usage: hub stamp <bundle>")?);
            println!("{}", stamp_bundle(&bundle)?);
            Ok(())
        }
        "sign-manifest" => {
            let bundle = PathBuf::from(positional.ok_or("usage: hub sign-manifest <bundle> --key <f> --key-id <id>")?);
            let key = load_key(&flag("key").ok_or("--key <key file>")?)?;
            let key_id = flag("key-id").ok_or("--key-id <id>")?;
            let path = bundle.join(octosense_app_policy::MANIFEST_FILE);
            let mut manifest = AppManifest::parse(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?)?;
            sign_manifest(&key, &mut manifest, &key_id)?;
            write_json(&path, &serde_json::to_value(&manifest).map_err(|e| e.to_string())?)?;
            println!("signed {} {} with {key_id}", manifest.id, manifest.version);
            Ok(())
        }
        "check" => {
            let bundle = PathBuf::from(positional.ok_or("usage: hub check <bundle>")?);
            let mut report = match gate_for(&bundle, &argv, has("allow-unsigned"), flag("catalog")) {
                Ok(report) => report,
                Err(error) => {
                    // A bundle the gate cannot even read is a refusal too, in
                    // the same shape, for tools that read --json.
                    if has("json") {
                        println!("{}", serde_json::json!({"schema": 1, "stage": "structural", "passed": false,
                            "findings": [{"severity": "refusal", "check": "bundle-invalid", "detail": error}]}));
                    }
                    return Err(error);
                }
            };
            // The crates its components list, against a RustSec checkout.
            if let Some(db) = flag("advisory-db") {
                report.findings.extend(octosense_app_hub::advisories::findings(&bundle, Path::new(&db))?);
            }
            if has("json") {
                println!("{}", report.json());
            } else {
                print!("{}", report.render());
            }
            if report.passed() {
                Ok(())
            } else {
                Err("the bundle was refused".into())
            }
        }
        "publish" => {
            let bundle = PathBuf::from(positional.ok_or("usage: hub publish <bundle> --catalog <file> …")?);
            let catalog_path = PathBuf::from(flag("catalog").ok_or("--catalog <file>")?);
            let report = gate_for(&bundle, &argv, has("allow-unsigned"), Some(catalog_path.to_string_lossy().into()))?;
            print!("{}", report.render());
            if !report.passed() {
                return Err("refusing to publish a bundle the gate refused".into());
            }
            let working = load_key(&flag("key").ok_or("--key <working key file>")?)?;
            let anchor_certificate = flag("anchor-cert").ok_or("--anchor-cert <hex>")?;
            let publisher = flag("publisher").ok_or("--publisher <id>")?;
            let repository = flag("repo").unwrap_or_default();
            let commit = flag("commit").unwrap_or_default();
            let out = PathBuf::from(flag("out").unwrap_or_else(|| ".".into()));

            let mut catalog = if catalog_path.exists() {
                trusted_catalog(&catalog_path, &argv)?
            } else {
                Catalog::new(0, &today(), Vec::new())
            };
            // The publisher's public key travels in the signed catalog, so a
            // device can check their signature without asking the hub again.
            // A known publisher's is the recorded one; an unsigned release
            // records none.
            let signed = AppManifest::parse(&std::fs::read_to_string(bundle.join(octosense_app_policy::MANIFEST_FILE)).map_err(|e| e.to_string())?)?
                .integrity
                .signature
                .is_some();
            let recorded = CatalogPublishers::from_catalog(&catalog)?
                .binding(&publisher)
                .map(|binding| binding.public_key_hex.clone());
            let supplied = argv
                .windows(2)
                .filter(|w| w[0] == "--publisher-key")
                .filter_map(|w| w[1].split_once('=').map(|(id, key)| (id.to_string(), key.to_string())))
                .find(|(id, _)| id == &publisher)
                .map(|(_, key)| key);
            let publisher_key = if signed { recorded.or(supplied).unwrap_or_default() } else { String::new() };
            let entry = entry_for(&bundle, &report, &publisher, &publisher_key, &repository, &commit, &today())?;
            // The artifact store keeps its own copy: review binds to bytes.
            let artifact = out.join(&entry.artifact);
            if let Some(parent) = artifact.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            copy_tree(&bundle, &artifact)?;
            // The same bytes as one file, for devices that fetch over HTTP.
            let pack = pack_dir(&bundle)?;
            let pack_path = out.join(format!("{}.pack.json", entry.artifact));
            std::fs::write(&pack_path, serde_json::to_string(&pack).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            // Stage two, when a reviewer is configured: judgement on top of
            // the facts. A reject stops the publish; human-review stops it
            // too, until a person re-runs with --reviewed.
            if let Some(reviewer) = flag("reviewer") {
                let packet = packet(&bundle, &report)?;
                let verdict = scan(&packet, &reviewer);
                println!("  scan: {:?} — {}", verdict.route, verdict.reasons.join("; "));
                match verdict.route {
                    Route::Pass => {}
                    Route::Reject => return Err("the scan rejected this bundle".into()),
                    Route::HumanReview if has("reviewed") => println!("  scan: human review recorded by --reviewed"),
                    Route::HumanReview => return Err("the scan asks for human review; publish again with --reviewed once a person has looked".into()),
                }
            }
            catalog.entries.push(entry);
            catalog.sequence += 1;
            catalog.published = today();
            working.sign_catalog(&mut catalog, &anchor_certificate)?;
            write_json(&catalog_path, &serde_json::to_value(&catalog).map_err(|e| e.to_string())?)?;
            println!("published {} {} (catalog sequence {})", report.app_id, report.version, catalog.sequence);
            Ok(())
        }
        "withdraw" => {
            let app_id = positional.ok_or("usage: hub withdraw <app id> --version <v> --reason <text> --catalog <f> --key <f> --anchor-cert <hex>")?;
            let version = flag("version").ok_or("--version <v>")?;
            let reason = flag("reason").ok_or("--reason <text shown to the person>")?;
            let catalog_path = PathBuf::from(flag("catalog").ok_or("--catalog <file>")?);
            let working = load_key(&flag("key").ok_or("--key <working key file>")?)?;
            let anchor_certificate = flag("anchor-cert").ok_or("--anchor-cert <hex>")?;
            let mut catalog = read_catalog(&catalog_path)?;
            // An app version, or a shared component version (App Hub ADR
            // 0003): ids are never both.
            let status = match catalog.entries.iter_mut().find(|e| e.app_id() == app_id && e.version() == version) {
                Some(entry) => &mut entry.status,
                None => &mut catalog
                    .components
                    .iter_mut()
                    .find(|c| c.id() == app_id && c.version() == version)
                    .ok_or_else(|| format!("{app_id} {version} is not in the catalog"))?
                    .status,
            };
            *status = Status::Withdrawn(reason.clone());
            catalog.sequence += 1;
            catalog.published = today();
            working.sign_catalog(&mut catalog, &anchor_certificate)?;
            write_json(&catalog_path, &serde_json::to_value(&catalog).map_err(|e| e.to_string())?)?;
            println!("withdrew {app_id} {version}: {reason} (catalog sequence {})", catalog.sequence);
            Ok(())
        }
        // Withdrawal is the normal end of a version: the entry stays, marked,
        // so a device can say why it stopped running. Removal is for an entry
        // that should never have been offered (a test publish, a mistaken
        // id): it leaves the catalog and its copies leave the store, and the
        // version number becomes free again.
        "remove" => {
            let app_id = positional.ok_or("usage: hub remove <app id> [--version <v>] --catalog <f> --key <f> --anchor-cert <hex> [--out <dir>]")?;
            let version = flag("version");
            let catalog_path = PathBuf::from(flag("catalog").ok_or("--catalog <file>")?);
            let working = load_key(&flag("key").ok_or("--key <working key file>")?)?;
            let anchor_certificate = flag("anchor-cert").ok_or("--anchor-cert <hex>")?;
            let out = PathBuf::from(flag("out").unwrap_or_else(|| ".".into()));
            let mut catalog = read_catalog(&catalog_path)?;
            let before = catalog.entries.len();
            let (removed, kept): (Vec<_>, Vec<_>) = catalog
                .entries
                .drain(..)
                .partition(|e| e.app_id() == app_id && version.as_deref().is_none_or(|v| e.version() == v));
            catalog.entries = kept;
            if removed.is_empty() {
                return Err(format!("{app_id}{} is not in the catalog", version.map(|v| format!(" {v}")).unwrap_or_default()));
            }
            for entry in &removed {
                let artifact = out.join(&entry.artifact);
                let _ = std::fs::remove_dir_all(&artifact);
                let _ = std::fs::remove_file(out.join(format!("{}.pack.json", entry.artifact)));
                let _ = std::fs::remove_file(out.join("index").join(format!("{}-{}.json", entry.app_id(), entry.version())));
                println!("removed {} {}", entry.app_id(), entry.version());
            }
            catalog.sequence += 1;
            catalog.published = today();
            working.sign_catalog(&mut catalog, &anchor_certificate)?;
            write_json(&catalog_path, &serde_json::to_value(&catalog).map_err(|e| e.to_string())?)?;
            println!("{} of {} entries remain (catalog sequence {})", catalog.entries.len(), before, catalog.sequence);
            Ok(())
        }
        "scan" => {
            let bundle = PathBuf::from(positional.ok_or("usage: hub scan <bundle> [--reviewer <cmd>] [--packet <out.json>]")?);
            let report = gate_for(&bundle, &argv, true, flag("catalog"))?;
            if !report.passed() {
                print!("{}", report.render());
                return Err("the gate refused this bundle; a scan is not offered".into());
            }
            let packet = packet(&bundle, &report)?;
            if let Some(path) = flag("packet") {
                write_json(Path::new(&path), &serde_json::to_value(&packet).map_err(|e| e.to_string())?)?;
                println!("wrote the review packet to {path}");
            }
            match flag("reviewer") {
                Some(reviewer) => {
                    let verdict = scan(&packet, &reviewer);
                    println!("{}", serde_json::to_string_pretty(&verdict).map_err(|e| e.to_string())?);
                    if verdict.route == Route::Reject {
                        return Err("rejected".into());
                    }
                    Ok(())
                }
                None => {
                    println!("no --reviewer given; the packet holds {} questions for one", packet.questions.len());
                    Ok(())
                }
            }
        }
        "verify" => {
            let catalog = read_catalog(Path::new(&positional.ok_or("usage: hub verify <catalog> --anchor <hex>")?))?;
            let anchor = flag("anchor").ok_or("--anchor <hex public key>")?;
            verify_catalog(&catalog, &anchor)?;
            println!("catalog sequence {} verified, {} entries", catalog.sequence, catalog.entries.len());
            Ok(())
        }
        _ => Err(format!("unknown command {command:?}; run `hub help` for usage")),
    }
}

/// Write the bundle's digest into its manifest.
///
/// The manifest is parsed first, with the parser the gate uses, so one the
/// gate cannot read is refused at the earliest step of the flow instead of at
/// `check`, after signing. What the gate then *judges* about the bundle - a
/// capability, a host, a listing that is not there yet - is not judged here:
/// `stamp` runs before signing, and the gate needs a signature policy that
/// only `check` is given.
fn stamp_bundle(bundle: &Path) -> Result<String, String> {
    let path = bundle.join(octosense_app_policy::MANIFEST_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let manifest=AppManifest::parse(&text)?;
    if manifest.integrity.signature.is_some() || manifest.integrity.github.is_some()
        || manifest.requires.iter().any(|f| f == "publisher-github-v1") {
        return Err("publisher signing metadata cannot be restamped; prepare a new unsigned version".into());
    }
    let digest = octosense_app_policy::digest_dir(bundle)?;
    let mut value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    value["integrity"]["bundle_blake3"] = serde_json::Value::String(digest.clone());
    write_json(&path, &value)?;
    Ok(digest)
}

fn gate_for(bundle: &Path, argv: &[String], allow_unsigned: bool, catalog: Option<String>) -> Result<GateReport, String> {
    let mut keys = PublisherKeys::new();
    for pair in argv.windows(2).filter(|w| w[0] == "--publisher-key").map(|w| w[1].clone()) {
        let (id, public) = pair.split_once('=').ok_or("--publisher-key expects id=hexkey")?;
        keys = keys.with(id, public);
    }
    let limits = HostLimits::default().with_require_signature(!allow_unsigned);
    let previous = match catalog {
        Some(path) if !Path::new(&path).exists() && argv.get(1).is_some_and(|s|s=="publish") => None,
        Some(path) => Some(github_catalog::authenticated_publication_base(
            &admission::read_bounded(Path::new(&path),github_catalog::MAX_DOCUMENT_BYTES as u64)?,
            argv.windows(2).find(|w|w[0]=="--anchor").map(|w|w[1].as_str()).unwrap_or(DEFAULT_ANCHOR))?),
        None => None,
    };
    if let Some(base)=&previous { keys=base.publishers().trusted_keys(keys); }
    if argv.iter().any(|arg|arg=="--system-app") {octosense_app_hub::gate::check_system_bundle(bundle,&keys)}
    else if let Some(base)=&previous {octosense_app_hub::gate::check_authenticated_bundle(bundle,&limits,&keys,base)}
    else {check_bundle(bundle,&limits,&keys,None)}
}

/// The catalog at `path`, once it verifies against the trusted anchor.
fn trusted_catalog(path: &Path, argv: &[String]) -> Result<Catalog, String> {
    let catalog = read_catalog(path)?;
    let anchor = argv.windows(2).find(|w| w[0] == "--anchor").map(|w| w[1].as_str()).unwrap_or(DEFAULT_ANCHOR);
    verify_catalog(&catalog, anchor)
        .map_err(|e| format!("could not authenticate {} against the hub anchor {anchor}: {e}", path.display()))?;
    Ok(catalog)
}

fn load_key(path: &str) -> Result<HubKey, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let raw = hex::decode(text.trim()).map_err(|e| format!("{path}: not hex: {e}"))?;
    let bytes: [u8; 32] = raw.as_slice().try_into().map_err(|_| format!("{path}: not a 32-byte key"))?;
    Ok(HubKey::from_bytes(&bytes))
}

fn read_catalog(path: &Path) -> Result<Catalog, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Create a new file; never replace one.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|e| format!("cannot create {}: {e}", path.display()))
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, format!("{text}\n")).map_err(|e| format!("{}: {e}", path.display()))
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{"schema":1,"id":"stamp-check","version":"1.0.0","name":"Stamp check","integrity":{"bundle_blake3":""},"capabilities":[]}"#;

    fn scratch(name: &str, manifest: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hub-stamp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(octosense_app_policy::MANIFEST_FILE), manifest).unwrap();
        std::fs::write(dir.join("main.splash"), "Label{text: \"Example\"}").unwrap();
        dir
    }

    fn manifest_text(dir: &Path) -> String {
        std::fs::read_to_string(dir.join(octosense_app_policy::MANIFEST_FILE)).unwrap()
    }

    #[test]
    fn stamp_writes_the_digest_the_directory_hashes_to() {
        let dir = scratch("ok", MANIFEST);
        let digest = stamp_bundle(&dir).unwrap();
        let value: serde_json::Value = serde_json::from_str(&manifest_text(&dir)).unwrap();
        assert_eq!(value["integrity"]["bundle_blake3"], serde_json::Value::String(digest));
    }

    #[test]
    fn stamp_refuses_a_manifest_the_gate_cannot_read() {
        // An unknown field: `stamp` used to write the digest and exit 0, and
        // `check` refused afterwards, once the manifest had been signed.
        let bad = MANIFEST.replace(r#""capabilities":[]"#, r#""capabilities":[],"nonsense":1"#);
        let dir = scratch("unknown-field", &bad);
        let err = stamp_bundle(&dir).unwrap_err();
        assert!(err.contains("manifest is not valid") && err.contains("unknown field"), "{err}");
        // Nothing was written on the way out.
        assert!(manifest_text(&dir).contains(r#""bundle_blake3":"""#), "the digest stays empty");
    }

    #[test]
    fn stamp_still_works_before_signing_and_before_the_listing() {
        // The documented order is stamp, then sign, then check, so the bundle
        // stamp is handed has no signature and may have no listing yet. Stamp
        // refuses a manifest the gate cannot *read*, never one the gate would
        // later *judge*: that judgement needs the policy `check` is given.
        let dir = scratch("early", MANIFEST);
        assert!(stamp_bundle(&dir).is_ok(), "an unsigned bundle with no listing is still stamped");
    }
}
