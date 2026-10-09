use super::*;

#[test]
fn authorization_rejects_revocation_replacement_and_closed_requests_at_each_boundary() {
    let original = json!({"capabilities":["files","storage"],"version":"1"});
    assert!(check_authorization(true, Some(&original), || Ok(original.clone())).is_ok());
    assert!(
        check_authorization(true, Some(&original), || Err("permission_denied".into())).is_err()
    );
    assert!(check_authorization(true, Some(&original), || Ok(
        json!({"capabilities":["files"],"version":"1"})
    ))
    .is_err());
    assert!(check_authorization(true, Some(&original), || Ok(
        json!({"capabilities":["files","storage"],"version":"2"})
    ))
    .is_err());
    assert!(check_authorization(false, Some(&original), || panic!(
        "closed requests must not perform admission IO"
    ))
    .is_err());
}

#[test]
fn import_and_export_accept_only_app_relative_file_arguments() {
    assert_eq!(
        parse_operation("import", &json!({"path":"/photos/a.png"})).unwrap(),
        Operation::Import {
            path: "/photos/a.png".into()
        }
    );
    assert_eq!(
        parse_operation("export", &json!({"path":"notes/a.txt"})).unwrap(),
        Operation::Export {
            path: "notes/a.txt".into(),
            name: "a.txt".into()
        }
    );
    assert_eq!(
        parse_operation("export", &json!({"path":"notes/a.txt","name":"Shared.txt"})).unwrap(),
        Operation::Export {
            path: "notes/a.txt".into(),
            name: "Shared.txt".into()
        }
    );
    for path in [
        "",
        "/",
        "..",
        "../../secret",
        "a/../../secret",
        "C:\\secret",
        "a:stream",
        "NUL",
        "CON.txt",
    ] {
        assert!(
            parse_operation("import", &json!({"path":path})).is_err(),
            "{path}"
        );
    }
    for name in [
        "../host.txt",
        "/host.txt",
        "a\\b",
        ".",
        "..",
        "NUL",
        "a:stream",
        "",
    ] {
        assert!(
            parse_operation("export", &json!({"path":"a.txt","name":name})).is_err(),
            "{name}"
        );
    }
    for args in [
        json!(null),
        json!([]),
        json!({"path":1}),
        json!({"path":"a","host_path":"/tmp/secret"}),
        json!({"path":"a","overwrite":true}),
    ] {
        assert!(parse_operation("import", &args).is_err());
    }
    assert!(parse_operation("export", &json!({"path":"a","name":null})).is_err());
    assert!(parse_operation("export", &json!({"path":"a","name":"x".repeat(129)})).is_err());
}

#[test]
fn one_transfer_reservation_is_released_on_cancellation() {
    let pending = Reservation::acquire().unwrap();
    assert!(Reservation::acquire().is_err());
    drop(pending);
    assert!(Reservation::acquire().is_ok());

    let pending = Arc::new(Reservation::acquire().unwrap());
    let worker = pending.clone();
    let guard = FileDialogAccessGuard::new(move || {
        let _keep_slot = &worker;
        false
    });
    drop(pending);
    assert!(
        Reservation::acquire().is_err(),
        "a cancelled provider read still owns its slot until it returns"
    );
    drop(guard);
    assert!(Reservation::acquire().is_ok());
}

#[test]
fn discovery_keeps_metadata_nonprompting_and_transfer_foreground_only() {
    let methods = FilesService.api_methods();
    assert_eq!(methods[0].name, "files.status");
    assert_eq!(methods[0].agent_access, AgentAccess::Allowed);
    assert_eq!(
        methods.len(),
        if file_dialogs::native_file_bytes_supported() {
            3
        } else {
            1
        }
    );
    for method in methods.into_iter().skip(1) {
        assert_eq!(method.capability, "files");
        assert_eq!(method.agent_access, AgentAccess::ForegroundOnly);
        assert!(!method
            .platforms
            .iter()
            .any(|p| ["ios", "web", "openharmony"].contains(&p.as_str())));
    }
}
