use super::*;

struct Home(PathBuf);
impl Home {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("device-consent-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn consent_is_app_and_capability_scoped_persistent_and_private() {
    let root = Home::new();
    assert!(
        !consent::get(&root.0, "org.example.one", "camera")
            .unwrap()
            .allowed
    );
    let granted = consent::set(&root.0, "org.example.one", "camera", true, Some(0)).unwrap();
    assert!(granted.allowed);
    assert_eq!(
        consent::get(&root.0, "org.example.one", "camera").unwrap(),
        granted
    );
    assert!(
        !consent::get(&root.0, "org.example.two", "camera")
            .unwrap()
            .allowed
    );
    assert!(
        !consent::get(&root.0, "org.example.one", "microphone")
            .unwrap()
            .allowed
    );
    assert!(consent::cached(&root.0, "org.example.one", "camera"));
    let other = Home::new();
    assert!(!consent::cached(&other.0, "org.example.one", "camera"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(root.0.join("device-api-consent.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
#[test]
fn revocation_invalidates_pending_approval_and_cached_runtime_access() {
    let root = Home::new();
    let grant = consent::set(&root.0, "org.example.one", "location", true, Some(0)).unwrap();
    let revoked = consent::set(&root.0, "org.example.one", "location", false, None).unwrap();
    assert!(revoked.revision > grant.revision);
    assert!(consent::set(
        &root.0,
        "org.example.one",
        "location",
        true,
        Some(grant.revision)
    )
    .is_err());
    assert!(
        !consent::get(&root.0, "org.example.one", "location")
            .unwrap()
            .allowed
    );
    assert!(!consent::cached(&root.0, "org.example.one", "location"));
}
#[test]
fn corrupt_store_is_not_treated_as_consent() {
    let root = Home::new();
    std::fs::write(root.0.join("device-api-consent.json"), "not-json").unwrap();
    assert!(consent::get(&root.0, "org.example.one", "camera").is_err());
    assert!(consent::set(&root.0, "org.example.one", "camera", true, None).is_err());
}
#[test]
fn os_status_preserves_retry_and_settings_distinction() {
    assert_eq!(
        status_name(PermissionStatus::NotDetermined),
        "not_determined"
    );
    assert_eq!(status_name(PermissionStatus::DeniedCanRetry), "denied");
    assert_eq!(
        status_name(PermissionStatus::DeniedPermanent),
        "settings_required"
    );
}

#[test]
fn descriptors_do_not_claim_unimplemented_location_or_background_authorization() {
    for family in ["camera", "microphone", "location"] {
        let service = DeviceService { family };
        let methods = service.api_methods();
        if !permission_supported() {
            assert!(methods.is_empty());
            continue;
        }
        let request = methods
            .iter()
            .find(|m| m.name.ends_with("permission.request"))
            .unwrap();
        assert_eq!(request.agent_access, services::AgentAccess::ForegroundOnly);
        assert!(!request.platforms.contains(&"windows".into()));
        assert!(request.input_schema["additionalProperties"] == false);
        assert_eq!(
            methods.iter().any(|m| m.name == "location.get"),
            family == "location" && cfg!(target_os = "android")
        );
    }
}

#[test]
fn expired_sheets_can_close_without_cancelling_a_newer_review() {
    #[derive(Default)]
    struct Host { closed: bool }
    impl ServiceHost for Host {
        fn open_sheet(&mut self, _: String) {}
        fn close_sheet(&mut self) { self.closed = true; }
    }
    let root = Home::new();
    let app = format!("org.example.expired-{}", uuid::Uuid::new_v4());
    services::register_host_service(Box::new(DeviceService { family: "camera" }));
    let key = (app.clone(), root.0.clone(), "camera");
    let call = |from_sheet| ServiceCall {
        app_id: app.clone(), service: "camera.sheet.close".into(),
        args: json!({"ticket":"expired-ticket"}), from_sheet, may_prompt:true,
        host_dir: root.0.clone(),
    };
    let mut host = Host::default();
    // The service and broker must still refuse an app impersonating the sheet.
    services::dispatch(call(false), 778401, 1, &mut host);
    assert!(!host.closed);
    assert!(services::take_replies_for(&[778401])[0].2.is_err());
    // Its approval record has expired, while its native UI remains visible.
    services::dispatch(call(true), 778401, 2, &mut host);
    assert!(host.closed);
    assert!(services::take_replies_for(&[778401])[0].2.is_ok());
    // Another surface may now own a newer request for the same app/family.
    state().lock().unwrap().current.insert(key.clone(), ("new-ticket".into(), Instant::now()));
    host.closed = false;
    services::dispatch(call(true), 778401, 3, &mut host);
    assert!(host.closed);
    assert!(services::take_replies_for(&[778401])[0].2.is_ok());
    let current = state().lock().unwrap().current.remove(&key).unwrap();
    assert_eq!(current.0, "new-ticket");
}
