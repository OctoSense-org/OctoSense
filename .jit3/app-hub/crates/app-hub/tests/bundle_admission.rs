//! Structural admission: a bundle the gate passes has a runnable entry,
//! decodable artwork and well-formed text, within bounds checked before
//! anything is read or allocated.
mod common;
use common::*;
use std::fs;

#[test]
fn runnable_script_fixture_passes() {
    let f = Fixture::new();
    let report = f.report(None);
    assert!(report.passed(), "{}", report.render());
}

#[test]
fn signed_bundles_cannot_claim_host_catalog_or_cache_lock_files() {
    for id in ["catalog.json", "catalog.lock", "catalog-v2.json", "catalog-v2.lock", "Catalog-V2.Json"] {
        let mut f = Fixture::new();
        f.manifest.id = id.into();
        f.sign();
        let report = f.report(None);
        assert!(!report.passed(), "{id} must be refused");
        assert!(report.findings.iter().any(|finding|
            finding.check == "identity" && finding.detail.contains("host catalog or cache lock file")
        ), "{}", report.render());
    }
    for id in ["org.example.json", "org.example.lock"] {
        let mut f = Fixture::new();
        f.manifest.id = id.into();
        f.sign();
        assert!(f.report(None).passed(), "{id} remains a valid signed app");
    }
}

#[test]
fn runnable_card_fixture_passes() {
    let mut f = Fixture::new();
    f.card();
    let report = f.report(None);
    assert!(report.passed(), "{}", report.render());
}

#[test]
fn a_bundle_without_an_entry_is_refused() {
    let mut f = Fixture::new();
    fs::remove_file(f.bundle.join("main.splash")).unwrap();
    f.sign();
    let report = f.report(None);
    assert!(!report.passed(), "a listing without an app must not be admitted");
    assert!(report.findings.iter().any(|e| e.check == "entry"), "{}", report.render());
    let mut card = Fixture::new();
    card.card();
    fs::remove_file(card.bundle.join("page.card")).unwrap();
    card.sign();
    assert!(!card.report(None).passed());
}

#[test]
fn a_script_entry_must_be_utf8_text() {
    let mut f = Fixture::new();
    fs::write(f.bundle.join("main.splash"), [0xff, 0xfe]).unwrap();
    f.sign();
    assert!(!f.report(None).passed());
}

#[test]
fn missing_kit_is_refused() {
    let mut f = Fixture::new();
    f.card();
    fs::remove_dir_all(f.bundle.join("kit")).unwrap();
    f.sign();
    assert!(!f.report(None).passed(), "the runtime requires the kit closure");
}

#[test]
fn malformed_card_data_and_utf8_are_refused() {
    for (name, bytes) in [("page.card", b"not a card".as_slice()), ("page.data.json", b"{".as_slice()), ("page.card", &[0xff])] {
        let mut f = Fixture::new();
        f.card();
        fs::write(f.bundle.join(name), bytes).unwrap();
        f.sign();
        assert!(!f.report(None).passed(), "{name} must be valid input to the runtime");
    }
}

#[test]
fn undeclared_kit_component_is_refused() {
    let mut f = Fixture::new();
    f.card();
    let path = f.bundle.join("kit/native/light/kit.json");
    let mut pack: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    pack["components"].as_object_mut().unwrap().remove("title");
    fs::write(path, serde_json::to_vec(&pack).unwrap()).unwrap();
    f.sign();
    assert!(!f.report(None).passed());
}

#[test]
fn fake_or_truncated_png_is_refused() {
    for bytes in [b"not a screenshot".as_slice(), b"\x89PNG\r\n\x1a\n".as_slice()] {
        let mut f = Fixture::new();
        fs::write(f.bundle.join("screen.png"), bytes).unwrap();
        f.sign();
        assert!(!f.report(None).passed(), "an extension is not image validation");
    }
}

#[test]
fn svg_external_resources_are_refused() {
    let mut f = Fixture::new();
    fs::write(f.bundle.join("icon.svg"), r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><image href="https://example.test/image.png"/></svg>"#).unwrap();
    f.sign();
    assert!(!f.report(None).passed());
}

#[test]
fn malformed_svg_is_refused() {
    let mut f = Fixture::new();
    fs::write(f.bundle.join("icon.svg"), "not svg").unwrap();
    f.sign();
    assert!(!f.report(None).passed());
}

#[test]
fn svg_mixed_local_and_external_css_urls_are_refused() {
    for value in ["fill:url(#local);filter:url(https://example.test/a)", "filter:URL (https://example.test/a)", r"filter:u\72l(https://example.test/a)"] {
        let mut f = Fixture::new();
        fs::write(f.bundle.join("icon.svg"), format!(r#"<svg width="64" height="64"><rect style="{value}"/></svg>"#)).unwrap();
        f.sign();
        assert!(!f.report(None).passed(), "{value}");
    }
}

#[test]
fn an_svg_style_element_is_allowed_unless_it_reaches_outside() {
    // Exported icons often carry a <style> block; only what it loads matters.
    let svg = |style: &str| format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><style>{style}</style><rect class="a" width="64" height="64"/></svg>"#);
    let mut f = Fixture::new();
    fs::write(f.bundle.join("icon.svg"), svg(".a{fill:#146}.b{fill:url(#g)}")).unwrap();
    f.sign();
    let report = f.report(None);
    assert!(report.passed(), "{}", report.render());
    for style in [
        "@import url(https://example.test/a.css);",
        ".a{fill:url(https://example.test/a)}",
        "@font-face{src:url(font.ttf)}",
        r".a{fill:u\72l(https://example.test/a)}",
        ".a{/* x */fill:#146}",
    ] {
        let mut f = Fixture::new();
        fs::write(f.bundle.join("icon.svg"), svg(style)).unwrap();
        f.sign();
        assert!(!f.report(None).passed(), "{style}");
    }
}

#[test]
fn kit_expansion_is_bounded_before_cloning_component_styles() {
    let mut f = Fixture::new();
    f.card();
    let path = f.bundle.join("kit/native/light/kit.json");
    let mut pack: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    // Each token is small, but the style resolves it repeatedly. Keeping this
    // fixture modest tests the budget without exhausting the test process.
    pack["tokens"]["large"] = serde_json::json!({"value": "x".repeat(200_000)});
    for index in 0..90 {
        pack["components"]["title"]["style"][format!("p{index}")] = serde_json::json!({"$token":"large"});
    }
    fs::write(path, serde_json::to_vec(&pack).unwrap()).unwrap();
    f.sign();
    assert!(!f.report(None).passed(), "small kit source must not allow unbounded expanded output");
}

#[test]
fn missing_local_resource_has_a_stable_code_and_path() {
    let mut f = Fixture::new();
    f.card();
    let path = f.bundle.join("page.data.json");
    let mut data: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    data["$kit"]["placements"]["panel"]["layout"]["src"] = "missing.png".into();
    fs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
    f.sign();
    let report = f.report(None);
    assert!(
        report.findings.iter().any(|e| e.check == "resource-invalid" && e.path.as_deref() == Some("page.data.json/$kit/placements/panel/layout/src")),
        "{}",
        report.render()
    );
}

#[test]
fn displayed_url_is_not_a_resource_reference_but_remains_conservatively_denied() {
    let mut f = Fixture::new();
    fs::write(f.bundle.join("extra.json"), r#"{"message":"Visit https://example.test"}"#).unwrap();
    f.sign();
    let report = f.report(None);
    assert!(report.resources.iter().any(|r| matches!(r.kind, octosense_app_hub::admission::ResourceKind::DisplayUrl)));
    assert!(!report.findings.iter().any(|r| r.check == "resource-invalid"));
    assert!(report.findings.iter().any(|r| r.check == "assets"), "network-policy conformance is still required before relaxing URL checks");
}

#[test]
fn font_attribution_links_are_documentation_not_network_grants() {
    let mut f = Fixture::new();
    fs::create_dir_all(f.bundle.join("assets/fonts")).unwrap();
    fs::write(f.bundle.join("assets/fonts/OFL.txt"), "Copyright Example (http://example.test/). Reserved Font Name 'Example'.").unwrap();
    fs::write(f.bundle.join("assets/fonts/README.md"), "Source: https://example.test/fonts\n").unwrap();
    f.sign();
    let report = f.report(None);
    assert!(report.passed(), "{}", report.render());
    let policy = report.policy.unwrap();
    assert!(policy.capabilities.is_empty());
    assert!(policy.hosts.is_empty());

    // Moving the same URL into executable/resource-bearing text still refuses.
    fs::write(f.bundle.join("extra.json"), r#"{"src":"https://example.test/fonts"}"#).unwrap();
    f.sign();
    assert!(!f.report(None).passed());
}

#[test]
fn image_dimensions_are_bounded_before_decode() {
    let mut f = Fixture::new();
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(4097, 1).write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    fs::write(f.bundle.join("screen.png"), bytes.into_inner()).unwrap();
    f.sign();
    assert!(!f.report(None).passed());
}

#[test]
fn launcher_icon_must_be_square() {
    let mut f = Fixture::new();
    let icon = f.bundle.join("icon.svg");
    fs::write(&icon, fs::read_to_string(&icon).unwrap().replace("height=\"64\"", "height=\"32\"")).unwrap();
    f.sign();
    assert!(!f.report(None).passed());
}

#[test]
fn metadata_limits_reject_large_files_many_entries_and_deep_directories() {
    use octosense_app_hub::admission::*;
    let f = Fixture::new();
    let huge = f.bundle.join("large.png");
    fs::File::create(&huge).unwrap().set_len(9 * 1024 * 1024).unwrap();
    assert!(inventory(&f.bundle).unwrap_err().contains("size limit"));
    fs::remove_file(huge).unwrap();
    let deep = f.bundle.join("deep");
    fs::create_dir_all((0..MAX_DEPTH + 1).fold(deep.clone(), |p, _| p.join("d"))).unwrap();
    assert!(inventory(&f.bundle).unwrap_err().contains("depth limit"));
    fs::remove_dir_all(deep).unwrap();
    let many = f.bundle.join("many");
    fs::create_dir(&many).unwrap();
    for i in 0..MAX_ENTRIES {
        fs::write(many.join(format!("{i}.txt")), "").unwrap();
    }
    assert!(inventory(&f.bundle).unwrap_err().contains("file count limit"));
}

#[test]
fn local_server_reports_match() {
    let mut f = Fixture::new();
    fs::write(f.bundle.join("screen.png"), "fake").unwrap();
    f.sign();
    let before = octosense_app_policy::digest_dir(&f.bundle).unwrap();
    let report = f.report(None);
    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_hub"))
        .args(["check", f.bundle.to_str().unwrap(), "--json", "--publisher-key", &format!("publisher-one={}", f.publisher.public_hex())])
        .output()
        .unwrap();
    assert!(!cli.status.success());
    let json: serde_json::Value = serde_json::from_slice(&cli.stdout).expect("machine-readable gate output");
    assert_eq!(json["findings"], serde_json::to_value(&report.findings).unwrap());
    assert_eq!(before, octosense_app_policy::digest_dir(&f.bundle).unwrap(), "checks must not write into the bundle");
}

#[test]
fn pack_limits_apply_before_writing_staged_files() {
    use base64::Engine;
    use octosense_app_hub::{pack_dir, unpack, Pack};
    let f = Fixture::new();
    let encoded = base64::engine::general_purpose::STANDARD.encode(vec![0; 9 * 1024 * 1024]);
    let pack = Pack { schema: 1, files: [("large.png".into(), encoded)].into() };
    let out = f.root.join("staged");
    assert!(unpack(&pack, &out).is_err());
    assert!(!out.join("large.png").exists());
    fs::File::create(f.bundle.join("large.png")).unwrap().set_len(9 * 1024 * 1024).unwrap();
    assert!(pack_dir(&f.bundle).is_err());
}

#[cfg(unix)]
#[test]
fn unpack_cannot_follow_an_existing_staging_symlink() {
    use base64::Engine;
    use octosense_app_hub::{unpack, Pack};
    let f = Fixture::new();
    let out = f.root.join("staged");
    fs::create_dir(&out).unwrap();
    std::os::unix::fs::symlink(&f.bundle, out.join("escape")).unwrap();
    let pack = Pack { schema: 1, files: [("escape/owned.txt".into(), base64::engine::general_purpose::STANDARD.encode(b"x"))].into() };
    assert!(unpack(&pack, &out).is_err());
    assert!(!f.bundle.join("owned.txt").exists());
}

#[test]
fn data_fields_and_kit_prop_names_are_not_resource_loads() {
    let mut f = Fixture::new();
    f.card();
    fs::write(f.bundle.join("extra.json"), r#"{"src":"database column","image":"description"}"#).unwrap();
    let path = f.bundle.join("kit/native/light/kit.json");
    let mut pack: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    pack["components"]["image-example"] = serde_json::json!({"props":{"src":"src"},"slot":false,"style":{"t":"image"}});
    fs::write(path, serde_json::to_vec(&pack).unwrap()).unwrap();
    f.sign();
    assert!(f.report(None).passed(), "schema property names and arbitrary data are not asset paths");
}

#[test]
fn native_theme_overlay_does_not_require_native_placements() {
    let mut f = Fixture::new();
    f.card();
    // Semantic Card shape used by the renderer's own l0_packs test. A native
    // theme overlay may coexist with legacy kit modules without placements.
    fs::write(f.bundle.join("page.card"), "theme light\nview root Surface { TextBody(text: \"Source type\") }").unwrap();
    fs::write(f.bundle.join("page.data.json"), "{}").unwrap();
    f.sign();
    assert!(f.report(None).passed(), "the native kit is checked; its placements are optional");
}

#[test]
fn a_nested_bundle_is_read_under_the_names_a_manifest_uses() {
    // The gate, the digest and a manifest all speak `/`. A path read back
    // through the host's separator carries a `\` on Windows, and then the
    // inventory refuses the bundle's own subdirectories while the checks
    // inside a kit never run. Regression for #75.
    let mut f = Fixture::new();
    f.card();
    let names: Vec<String> = octosense_app_hub::admission::inventory(&f.bundle)
        .expect("a bundle with subdirectories is inventoried")
        .iter()
        .map(|file| octosense_app_policy::portable_path(&file.path).expect("a walked path is a plain UTF-8 name"))
        .collect();
    assert!(
        names.iter().any(|name| name == "kit/native/light/kit.json"),
        "the kit is named the way a manifest names it: {names:?}"
    );
    assert!(
        names.iter().all(|name| !name.contains('\\')),
        "no name carries the host's separator: {names:?}"
    );
}

#[test]
fn kit_font_objects_and_tokens_cannot_bypass_asset_checks() {
    use serde_json::json;
    for (font, token, allowed) in [
        (json!({"url":"missing.ttf"}), None, false),
        (json!({"$token":"face"}), None, false),
        (json!({"$token":"face"}), Some(json!("missing.ttf")), false),
        (json!({"$token":"face"}), Some(json!({"$token":"nested"})), false),
        (json!({"$token":"face"}), Some(json!("makepad_widgets:resources/Inter.ttf")), true),
        (json!("makepad_widgets:resources/LXGWWenKaiRegular.ttf"), None, true),
        (json!({"$token":"face"}), Some(json!("makepad_widgets:resources/LXGWWenKaiBold.ttf")), true),
        (json!("makepad_widgets:resources/../private.ttf"), None, false),
        (json!("makepad_widgets:resources/Unshipped.ttf"), None, false),
        (json!("other_crate:resources/LXGWWenKaiRegular.ttf"), None, false),
        (json!({"$token":"face"}), Some(json!("assets/body.ttf")), true),
        (json!(["assets/body.ttf"]), None, false),
    ] {
        let mut f = Fixture::new(); f.card();
        fs::create_dir_all(f.bundle.join("assets")).unwrap();
        fs::write(f.bundle.join("assets/body.ttf"), b"asset-path fixture only").unwrap();
        let path = f.bundle.join("kit/native/light/kit.json");
        let mut pack: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        pack["components"]["title"]["style"]["font_src"] = font.clone();
        if let Some(token) = token { pack["tokens"]["face"] = json!({"value":token}); }
        fs::write(path, serde_json::to_vec(&pack).unwrap()).unwrap(); f.sign();
        let report = f.report(None);
        assert_eq!(report.passed(), allowed, "font {font}: {}", report.render());
        if !allowed { assert!(report.findings.iter().any(|r| r.check == "resource-invalid")); }
    }
}

#[test]
fn theme_font_token_is_checked_even_without_a_component_reference() {
    let mut f = Fixture::new(); f.card();
    let path = f.bundle.join("kit/native/light/kit.json");
    let mut pack: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    pack["tokens"]["typography.body.font_src"] = serde_json::json!({"value":"missing.ttf"});
    fs::write(path, serde_json::to_vec(&pack).unwrap()).unwrap(); f.sign();
    let report = f.report(None);
    assert!(!report.passed());
    assert!(report.findings.iter().any(|r| r.check == "resource-invalid" && r.path.as_deref() == Some("kit/native/light/kit.json/tokens/typography.body.font_src/value")));
}

// ---- the app's own functions (fns/*.wasm, the wasm capability) ----------

/// The smallest valid WebAssembly core module: magic and version, no sections.
const MODULE: &[u8] = b"\0asm\x01\0\0\0";

fn with_functions(capability: bool, files: &[(&str, &[u8])]) -> octosense_app_hub::GateReport {
    let mut f = Fixture::new();
    for (name, bytes) in files {
        let path = f.bundle.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    if capability {
        f.manifest.capabilities.push("wasm".into());
    }
    f.sign();
    f.report(None)
}

#[test]
fn an_apps_own_functions_pass_with_or_without_wasm_disclosure() {
    let report = with_functions(true, &[("fns/rank.wasm", MODULE)]);
    assert!(report.passed(), "{}", report.render());
    let report = with_functions(false, &[("fns/rank.wasm", MODULE)]);
    assert!(report.passed(), "{}", report.render());
    assert!(report.findings.iter().any(|f| f.check == "functions"), "{}", report.render());
}

#[test]
fn a_module_out_of_place_misnamed_or_not_wasm_is_refused() {
    for (name, bytes) in [
        ("rank.wasm", MODULE),
        ("lib/rank.wasm", MODULE),
        ("fns/deep/rank.wasm", MODULE),
        ("fns/Rank.wasm", MODULE),
        ("fns/.wasm", MODULE),
        ("fns/rank.wasm", b"\0asm\x02\0\0\0".as_slice()),
        ("fns/rank.wasm", b"\0asm".as_slice()),
        ("fns/rank.wasm", b"#!/bin/sh\necho hi\n".as_slice()),
    ] {
        let report = with_functions(true, &[(name, bytes)]);
        assert!(!report.passed(), "{name} {bytes:?} must be refused: {}", report.render());
    }
}

#[test]
fn a_bundle_carries_at_most_eight_modules_and_hears_about_none() {
    let max = octosense_app_hub::gate::MAX_FUNCTION_MODULES;
    let names: Vec<String> = (0..=max).map(|i| format!("fns/f{i}.wasm")).collect();
    let files: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), MODULE)).collect();
    let report = with_functions(true, &files[..max]);
    assert!(report.passed(), "{}", report.render());
    assert!(!with_functions(true, &files).passed());
    let report = with_functions(true, &[]);
    assert!(report.passed(), "{}", report.render());
    assert!(
        report.findings.iter().any(|f| f.check == "functions" && f.severity == octosense_app_hub::Severity::Warning),
        "{}",
        report.render()
    );
}

// ---- components (OctoSense ADR 0014, wasm-components-v1) ----------------

/// Components built with plain `cargo build --target wasm32-wasip2`, copied
/// from OctoSense's `crates/wasm-host/tests/fixtures` (its
/// `tests/component-guest/build.sh` builds them). `notes` imports the clock,
/// random numbers and the filesystem; `netprobe` imports `wasi:sockets`;
/// `fetch` imports `wasi:http` and `hostcall` `octosense:host` (OctoSense
/// ADR 0014 phase 3).
const NOTES: &[u8] = include_bytes!("fixtures/notes.component.wasm");
const HOSTCALL: &[u8] = include_bytes!("fixtures/hostcall.component.wasm");
const NETPROBE: &[u8] = include_bytes!("fixtures/netprobe.component.wasm");
const FETCH: &[u8] = include_bytes!("fixtures/fetch.component.wasm");

fn with_component(requires: bool, capabilities: &[&str], bytes: &[u8]) -> octosense_app_hub::GateReport {
    with_component_hosts(requires, capabilities, &[], bytes)
}

fn with_component_hosts(requires: bool, capabilities: &[&str], hosts: &[&str], bytes: &[u8]) -> octosense_app_hub::GateReport {
    let mut f = Fixture::new();
    fs::create_dir_all(f.bundle.join("fns")).unwrap();
    fs::write(f.bundle.join("fns/notes.wasm"), bytes).unwrap();
    f.manifest.capabilities.extend(capabilities.iter().map(|c| c.to_string()));
    f.manifest.network.hosts.extend(hosts.iter().map(|h| h.to_string()));
    if requires {
        f.manifest.requires.push("wasm-components-v1".into());
    }
    f.sign();
    f.report(None)
}

fn refusal<'a>(report: &'a octosense_app_hub::GateReport, check: &str) -> Option<&'a octosense_app_hub::Finding> {
    report.findings.iter().find(|f| f.check == check && f.severity == octosense_app_hub::Severity::Refusal)
}

#[test]
fn a_component_passes_under_its_feature_and_tells_reviewers_what_it_reaches() {
    let report = with_component(true, &["wasm", "storage"], NOTES);
    assert!(report.passed(), "{}", report.render());
    let reach = report
        .findings
        .iter()
        .find(|f| f.path.as_deref() == Some("fns/notes.wasm"))
        .unwrap_or_else(|| panic!("no reach line: {}", report.render()));
    assert_eq!(reach.severity, octosense_app_hub::Severity::Warning);
    assert_eq!(
        reach.detail,
        "fns/notes.wasm is a component that reaches the clock, random numbers and files in its app folder, but no network or other app"
    );
}

#[test]
fn a_component_is_refused_unless_the_manifest_requires_components() {
    let report = with_component(false, &["wasm", "storage"], NOTES);
    let finding = refusal(&report, "functions").unwrap_or_else(|| panic!("{}", report.render()));
    assert!(finding.detail.contains("must require wasm-components-v1"), "{}", finding.detail);
}

#[test]
fn component_files_work_without_usage_declarations_and_remain_disclosed() {
    let report = with_component(true, &[], NOTES);
    assert!(report.passed(), "{}", report.render());
    assert!(report.findings.iter().any(|f| f.detail.contains("disclose storage usage")));
}

#[test]
fn a_component_that_uses_http_declares_net_and_reviewers_see_it() {
    // network.hosts is a declaration, not a limit (OctoSense's ruling of
    // 8 October 2026): the component reaches any host, listed or not.
    for hosts in [&[][..], &["api.example.com"][..]] {
        let report = with_component_hosts(true, &["wasm", "net"], hosts, FETCH);
        assert!(report.passed(), "{}", report.render());
        let reach = report
            .findings
            .iter()
            .find(|f| f.path.as_deref() == Some("fns/notes.wasm"))
            .unwrap_or_else(|| panic!("no reach line: {}", report.render()));
        assert!(reach.detail.contains("the clock and the network, but no files or other app"), "{}", reach.detail);
    }
    let report = with_component(true, &[], FETCH);
    assert!(report.passed(), "{}", report.render());
    assert!(report.findings.iter().any(|f| f.detail.contains("disclose net usage")));
}

#[test]
fn a_component_may_call_its_apps_host_services_with_no_grant_of_its_own() {
    let report = with_component(true, &["wasm"], HOSTCALL);
    assert!(report.passed(), "{}", report.render());
    let reach = report
        .findings
        .iter()
        .find(|f| f.path.as_deref() == Some("fns/notes.wasm"))
        .unwrap_or_else(|| panic!("no reach line: {}", report.render()));
    assert!(reach.detail.contains("its app's host services"), "{}", reach.detail);
}

#[test]
fn socket_components_require_the_host_abi_but_not_network_disclosures() {
    let report = with_component(true, &["wasm", "storage"], NETPROBE);
    let finding = refusal(&report, "functions").unwrap_or_else(|| panic!("{}", report.render()));
    assert!(finding.detail.contains("requires host_api.required wasm.sockets@1"), "{}", finding.detail);
    assert!(refusal(&report, "contents-invalid").is_none(), "{}", report.render());

    let mut f = Fixture::new();
    fs::create_dir_all(f.bundle.join("fns")).unwrap();
    fs::write(f.bundle.join("fns/network.wasm"), NETPROBE).unwrap();
    f.manifest.requires.push("wasm-components-v1".into());
    f.socket_api();
    let report = f.report(None);
    assert!(report.passed(), "{}", report.render());
    assert!(report.findings.iter().any(|f| f.detail.contains("imports wasi:sockets; disclose net usage")));
    assert!(report.findings.iter().any(|f| f.detail.contains("is a component that reaches") && f.detail.contains("the network")));
    assert!(f.manifest.capabilities.is_empty(), "usage declarations are not grants");

    // Optional or another ABI version cannot disguise a required socket import.
    f.manifest.host_api.as_mut().unwrap().required.insert("wasm.sockets".into(), 2);
    f.sign();
    assert!(!f.report(None).passed());
    let api = f.manifest.host_api.as_mut().unwrap();
    api.required.clear();
    api.optional.insert("wasm.sockets".into(), 1);
    f.sign();
    assert!(!f.report(None).passed());
}

#[test]
fn a_component_that_does_not_validate_is_refused() {
    let truncated = &NOTES[..NOTES.len() / 2];
    let report = with_component(true, &["wasm", "storage"], truncated);
    let finding = refusal(&report, "contents-invalid").unwrap_or_else(|| panic!("{}", report.render()));
    assert!(finding.detail.contains("not a valid WebAssembly component"), "{}", finding.detail);
}

#[test]
fn a_module_needs_no_feature_and_the_feature_alone_is_only_a_warning() {
    let report = with_functions(true, &[("fns/rank.wasm", MODULE)]);
    assert!(report.passed(), "{}", report.render());
    assert!(report.findings.iter().all(|f| f.path.as_deref() != Some("fns/rank.wasm")), "{}", report.render());
    let report = with_component(true, &["wasm"], MODULE);
    assert!(report.passed(), "{}", report.render());
    assert!(
        report.findings.iter().any(|f| f.check == "functions" && f.detail.contains("holds no component")),
        "{}",
        report.render()
    );
}

// ---- the crates a component lists (octosense-crates) ---------------------

/// `bytes` with a custom section `name` holding `payload` appended, as App
/// Flow's `tools/octo wasm build` appends the crate list.
fn with_section(bytes: &[u8], name: &str, payload: &[u8]) -> Vec<u8> {
    fn leb(mut n: usize, out: &mut Vec<u8>) {
        loop {
            let byte = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }
    let mut body = Vec::new();
    leb(name.len(), &mut body);
    body.extend_from_slice(name.as_bytes());
    body.extend_from_slice(payload);
    let mut out = bytes.to_vec();
    out.push(0);
    leb(body.len(), &mut out);
    out.extend(body);
    out
}

const CRATES: &str = r#"{"crates":[{"checksum":"0000000000000000000000000000000000000000000000000000000000000000","name":"getrandom","source":"crates.io","version":"0.4.1"},{"name":"pulldown-cmark","source":"crates.io","version":"0.9.0"}],"schema":1}"#;

#[test]
fn reviewers_see_the_crates_a_component_lists() {
    let listed = with_section(NOTES, "octosense-crates", CRATES.as_bytes());
    let report = with_component(true, &["wasm", "storage"], &listed);
    assert!(report.passed(), "{}", report.render());
    assert!(
        report.findings.iter().any(|f| f.detail == "fns/notes.wasm is built from 2 crates: getrandom 0.4.1 and pulldown-cmark 0.9.0"),
        "{}",
        report.render()
    );
    let report = with_component(true, &["wasm", "storage"], NOTES);
    assert!(
        report.findings.iter().any(|f| f.detail.starts_with("fns/notes.wasm does not list the crates it is built from")),
        "{}",
        report.render()
    );
}

#[test]
fn two_crate_lists_or_an_unreadable_one_are_refused() {
    let twice = with_section(&with_section(NOTES, "octosense-crates", CRATES.as_bytes()), "octosense-crates", CRATES.as_bytes());
    let report = with_component(true, &["wasm", "storage"], &twice);
    let finding = refusal(&report, "contents-invalid").unwrap_or_else(|| panic!("{}", report.render()));
    assert!(finding.detail.contains("more than one octosense-crates section"), "{}", finding.detail);
    let garbled = with_section(NOTES, "octosense-crates", b"{not json");
    let report = with_component(true, &["wasm", "storage"], &garbled);
    let finding = refusal(&report, "contents-invalid").unwrap_or_else(|| panic!("{}", report.render()));
    assert!(finding.detail.contains("is not a crate list"), "{}", finding.detail);
}

#[test]
fn listed_crates_are_checked_against_a_rustsec_checkout() {
    let mut f = Fixture::new();
    fs::create_dir_all(f.bundle.join("fns")).unwrap();
    fs::write(f.bundle.join("fns/notes.wasm"), with_section(NOTES, "octosense-crates", CRATES.as_bytes())).unwrap();
    f.manifest.capabilities.extend(["wasm".to_string(), "storage".to_string()]);
    f.manifest.requires.push("wasm-components-v1".into());
    f.sign();
    let db = std::env::temp_dir().join(format!("advisory-db-gate-{}", std::process::id()));
    let _ = fs::remove_dir_all(&db);
    fs::create_dir_all(db.join("crates/pulldown-cmark")).unwrap();
    fs::write(
        db.join("crates/pulldown-cmark/RUSTSEC-2099-0001.md"),
        "```toml\n[advisory]\nid = \"RUSTSEC-2099-0001\"\npackage = \"pulldown-cmark\"\ndate = 2099-01-01\n\n[versions]\npatched = [\">= 0.9.3\"]\n```\n\n# A made-up flaw\n",
    )
    .unwrap();
    let findings = octosense_app_hub::advisories::findings(&f.bundle, &db).unwrap();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(
        findings[0].detail,
        "fns/notes.wasm includes pulldown-cmark 0.9.0, which RUSTSEC-2099-0001 reports as a vulnerability: A made-up flaw; fixed in >= 0.9.3"
    );
    assert!(octosense_app_hub::advisories::findings(&f.bundle, &f.bundle).is_err(), "not a database");
    let _ = fs::remove_dir_all(&db);
}
