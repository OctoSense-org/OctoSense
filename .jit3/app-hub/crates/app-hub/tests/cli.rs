mod common;

use common::Fixture;
use std::{fs, process::Command};

fn hub(fixture: &Fixture, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_hub"))
        .current_dir(&fixture.root)
        .args(args)
        .output()
        .expect("hub runs")
}

#[test]
fn help_is_read_only_for_commands_that_would_read_or_create_files() {
    let fixture = Fixture::new();
    let before = fs::read_dir(&fixture.root).unwrap().count();
    for args in [vec![], vec!["help"], vec!["--help"], vec!["check", "--help"],
                 vec!["keygen", "--help"], vec!["keygen", "not-created.key", "-h"]] {
        let result = hub(&fixture, &args);
        assert!(result.status.success(), "{args:?}: {}", String::from_utf8_lossy(&result.stderr));
        let text = String::from_utf8_lossy(&result.stdout);
        assert!(text.contains("OctoSense app hub"));
        assert!(text.contains("scan permits unsigned") && text.contains("--publisher-key"));
    }
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), before);
}

#[test]
fn keygen_creates_a_valid_key_and_never_replaces_an_existing_file() {
    let fixture = Fixture::new();
    let created = hub(&fixture, &["keygen", "new.key"]);
    assert!(created.status.success());
    let private = fs::read(fixture.root.join("new.key")).unwrap();
    assert_eq!(private.len(), 64);
    let public = hub(&fixture, &["pubkey", "new.key"]);
    assert!(public.status.success());
    assert_eq!(public.stdout, created.stdout);
    let refused = hub(&fixture, &["keygen", "new.key"]);
    assert!(!refused.status.success());
    assert!(refused.stdout.is_empty());
    assert_eq!(fs::read(fixture.root.join("new.key")).unwrap(), private);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(fixture.root.join("new.key")).unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[cfg(unix)]
#[test]
fn keygen_refuses_symlinks_without_changing_the_target() {
    let fixture = Fixture::new();
    let target = fixture.root.join("existing.key");
    fs::write(&target, "existing private material").unwrap();
    std::os::unix::fs::symlink(&target, fixture.root.join("alias.key")).unwrap();
    let result = hub(&fixture, &["keygen", "alias.key"]);
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert_eq!(fs::read_to_string(target).unwrap(), "existing private material");
}

#[test]
fn invalid_commands_and_missing_positionals_fail_without_writing() {
    let fixture = Fixture::new();
    let before = fs::read_dir(&fixture.root).unwrap().count();
    for args in [vec!["unknown"], vec!["keygen"], vec!["keygen", "--bad"], vec!["check"]] {
        let result = hub(&fixture, &args);
        assert!(!result.status.success(), "{args:?}");
        assert!(result.stdout.is_empty());
    }
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), before);
}

#[test]
fn signed_scan_needs_the_documented_publisher_key() {
    let fixture = Fixture::new();
    assert!(!hub(&fixture, &["scan", "bundle"]).status.success());
    let key = format!("publisher-one={}", fixture.publisher.public_hex());
    let result = hub(&fixture, &["scan", "bundle", "--publisher-key", &key]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert!(String::from_utf8_lossy(&result.stdout).contains("questions for one"));
}

#[test]
fn system_development_checks_cannot_publish_or_relax_store_checks() {
    let mut fixture = Fixture::new();
    fixture.manifest.id = "os.fixture".into();
    fixture.manifest.capabilities.push("storage".into());
    fixture.sign();
    let key = format!("publisher-one={}", fixture.publisher.public_hex());
    let ordinary = hub(&fixture, &["check", "bundle", "--publisher-key", &key]);
    assert!(!ordinary.status.success());
    let checked = hub(&fixture, &["check", "bundle", "--system-app", "--publisher-key", &key]);
    assert!(checked.status.success(), "{}\n{}", String::from_utf8_lossy(&checked.stdout), String::from_utf8_lossy(&checked.stderr));
    assert!(String::from_utf8_lossy(&checked.stdout).contains("67108864"));
    let scanned = hub(&fixture, &["scan", "bundle", "--system-app", "--publisher-key", &key, "--packet", "review.json"]);
    assert!(scanned.status.success(), "{}", String::from_utf8_lossy(&scanned.stderr));
    let packet: serde_json::Value = serde_json::from_slice(&fs::read(fixture.root.join("review.json")).unwrap()).unwrap();
    assert_eq!(packet["app_id"], "os.fixture");

    let report = octosense_app_hub::gate::check_system_bundle(&fixture.bundle, &fixture.keys()).unwrap();
    assert!(report.passed());
    assert!(octosense_app_hub::entry_for(&fixture.bundle, &report, "publisher-one", &fixture.publisher.public_hex(), "", "", "2026-10-08").is_err());
    let store_report = octosense_app_hub::check_bundle(&fixture.bundle, &octosense_app_policy::HostLimits::system(), &fixture.keys(), None).unwrap();
    assert!(!store_report.passed(), "larger ceilings cannot authorize a system id in a store");
    for args in [vec!["publish", "bundle", "--system-app", "--catalog", "catalog.json"],
                 vec!["publish", "bundle", "--publisher-key", &key, "--catalog", "catalog.json"],
                 vec!["stamp", "bundle", "--system-app"]] {
        assert!(!hub(&fixture, &args).status.success(), "{args:?}");
        assert!(!fixture.root.join("catalog.json").exists());
        assert!(!fixture.root.join("artifacts").exists());
    }
    // System development remains a real integrity check, including signatures.
    assert!(!hub(&fixture, &["check", "bundle", "--system-app"]).status.success());
    fs::write(fixture.bundle.join("main.splash"), "Label{text: \"changed\"}").unwrap();
    assert!(!hub(&fixture, &["check", "bundle", "--system-app", "--publisher-key", &key]).status.success());
}

#[test]
fn system_mode_accepts_unsigned_system_bundles_but_refuses_store_ids() {
    let mut fixture = Fixture::new();
    let key = format!("publisher-one={}", fixture.publisher.public_hex());
    let refused = hub(&fixture, &["check", "bundle", "--system-app", "--publisher-key", &key]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stdout).contains("checks only shipped os.* apps"));
    fixture.manifest.id = "os.fixture".into();
    fixture.manifest.integrity.signature = None;
    fixture.write_manifest();
    let result = hub(&fixture, &["check", "bundle", "--system-app"]);
    assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
}

#[test]
fn component_info_prints_a_function_files_kind_imports_and_typed_exports() {
    use serde_json::{json, Value};
    let fixture = Fixture::new();
    fs::write(fixture.root.join("notes.wasm"), include_bytes!("fixtures/notes.component.wasm")).unwrap();
    let result = hub(&fixture, &["component-info", "notes.wasm"]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let info: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(info["kind"], "component");
    let imports: Vec<&str> = info["imports"].as_array().unwrap().iter().map(|i| i.as_str().unwrap()).collect();
    assert!(imports.iter().any(|i| i.starts_with("wasi:filesystem/")), "{imports:?}");
    assert!(imports.iter().all(|i| i.starts_with("wasi:")), "{imports:?}");
    let names: Vec<&str> = info["exports"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        ["analyze", "count", "echo-bytes", "grow", "now-ms", "random-u64", "read-file", "save-html", "spin", "to-html"]
    );
    assert_eq!(
        info["exports"][0],
        json!({
            "name": "analyze",
            "params": [["markdown", "string"]],
            "result": "record { words: u32, lines: u32, headings: list<string> }"
        })
    );

    // A core module: (module (import "octo" "log" (func (param i32 i32)))
    //   (func (export "add") (param i32 i32) (result i32)
    //     local.get 0 local.get 1 i32.add))
    let module: &[u8] = b"\0asm\x01\0\0\0\
        \x01\x0c\x02\x60\x02\x7f\x7f\x01\x7f\x60\x02\x7f\x7f\x00\
        \x02\x0c\x01\x04octo\x03log\x00\x01\
        \x03\x02\x01\x00\
        \x07\x07\x01\x03add\x00\x01\
        \x0a\x09\x01\x07\x00\x20\x00\x20\x01\x6a\x0b";
    fs::write(fixture.root.join("rank.wasm"), module).unwrap();
    let result = hub(&fixture, &["component-info", "rank.wasm"]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap(),
        json!({
            "kind": "module",
            "imports": ["octo.log"],
            "exports": [{"name": "add", "params": [["p0", "i32"], ["p1", "i32"]], "result": "i32"}]
        })
    );

    fs::write(fixture.root.join("script.wasm"), "#!/bin/sh\necho hi\n").unwrap();
    let result = hub(&fixture, &["component-info", "script.wasm"]);
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("not a WebAssembly core module or component"));
}
