use octosense_app_contract::{check_reserved_id, resolve, AppManifest, HostLimits};

fn manifest(id: &str) -> AppManifest {
    AppManifest::parse(
        &serde_json::json!({
            "schema": 1, "id": id, "version": "1.0.0", "name": "Example",
            "integrity": {"bundle_blake3": "00"}
        })
        .to_string(),
    )
    .unwrap()
}

#[test]
fn exact_host_catalog_file_ids_are_reserved_on_every_filesystem() {
    let limits = HostLimits::default().with_require_signature(false);
    for id in [
        "catalog.json",
        "catalog.lock",
        "catalog-v2.json",
        "catalog-v2.lock",
    ] {
        for variant in [
            id.to_string(),
            id.to_ascii_uppercase(),
            id.replacen('c', "C", 1),
        ] {
            let error = check_reserved_id(&variant).unwrap_err();
            assert!(
                error.contains("host catalog or cache lock file"),
                "{variant}: {error}"
            );
            assert!(resolve(&manifest(&variant), &limits).is_err(), "{variant}");
        }
    }
}

#[test]
fn host_file_reservation_does_not_claim_json_or_lock_namespaces() {
    let limits = HostLimits::default().with_require_signature(false);
    for id in [
        "json",
        "lock",
        "org.example.json",
        "org.example.lock",
        "org.example.catalog.json",
        "org.example.catalog-v2.lock",
        "catalog-v2.json.extra",
    ] {
        assert!(check_reserved_id(id).is_ok(), "{id}");
        assert!(resolve(&manifest(id), &limits).is_ok(), "{id}");
    }
}
