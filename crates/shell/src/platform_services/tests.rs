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
    struct Host {
        closed: bool,
    }
    impl ServiceHost for Host {
        fn open_sheet(&mut self, _: String) {}
        fn close_sheet(&mut self) {
            self.closed = true;
        }
    }
    let root = Home::new();
    let app = format!("org.example.expired-{}", uuid::Uuid::new_v4());
    services::register_host_service(Box::new(DeviceService { family: "camera" }));
    let key = (app.clone(), root.0.clone(), "camera");
    let call = |from_sheet| ServiceCall {
        app_id: app.clone(),
        service: "camera.sheet.close".into(),
        args: json!({"ticket":"expired-ticket"}),
        from_sheet,
        may_prompt: true,
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
    state()
        .lock()
        .unwrap()
        .current
        .insert(key.clone(), ("new-ticket".into(), Instant::now()));
    host.closed = false;
    services::dispatch(call(true), 778401, 3, &mut host);
    assert!(host.closed);
    assert!(services::take_replies_for(&[778401])[0].2.is_ok());
    let current = state().lock().unwrap().current.remove(&key).unwrap();
    assert_eq!(current.0, "new-ticket");
}

#[test]
fn location_samples_validate_bounds_age_accuracy_and_platform_data() {
    use makepad_widgets::makepad_platform::event::LocationUpdateEvent;
    let options =
        location::Options::parse(&json!({"max_age_ms":1000,"max_accuracy_m":20})).unwrap();
    let mut fix = LocationUpdateEvent {
        lat: 37.0,
        lon: -122.0,
        accuracy_m: 10.0,
        time: 100.0,
        altitude_m: None,
        speed_mps: None,
        heading_deg: None,
    };
    assert_eq!(options.value(&fix, 100.5).unwrap()["age_ms"], 500);
    assert!(options.value(&fix, 101.001).is_none(), "stale fix");
    assert!(options.value(&fix, 98.0).is_none(), "future timestamp");
    fix.accuracy_m = 21.0;
    assert!(options.value(&fix, 100.0).is_none(), "accuracy threshold");
    fix.accuracy_m = 10.0;
    fix.lat = f64::NAN;
    assert!(
        options.value(&fix, 100.0).is_none(),
        "invalid platform coordinate"
    );
    for args in [
        json!(null),
        json!({"timeout_ms":0}),
        json!({"timeout_ms":30001}),
        json!({"max_age_ms":60001}),
        json!({"max_age_ms":1.5}),
        json!({"max_accuracy_m":0}),
        json!({"max_accuracy_m":100001}),
        json!({"app_id":"other"}),
    ] {
        assert!(location::Options::parse(&args).is_err(), "{args}");
    }
    let methods = DeviceService { family: "location" }.api_methods();
    let sample = methods.iter().find(|m| m.name == "location.sample");
    assert_eq!(sample.is_some(), permission_supported());
    if let Some(sample) = sample {
        assert_eq!(sample.agent_access, services::AgentAccess::ForegroundOnly);
        assert_eq!(sample.platforms, ["android", "macos"]);
        assert!(methods.iter().any(|m| m.name == "location.sample.cancel"));
        assert!(!methods.iter().any(|m| m.name.contains("watch")));
    }
}

/// Exercise the real host broker and admitted app identity with synthetic native
/// events. A child process owns global registries; no OS/device is opened.
#[test]
fn location_sampling_broker_lifecycle() {
    if !permission_supported() {
        return;
    }
    const CHILD: &str = "OCTOSENSE_LOCATION_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "platform_services::tests::location_sampling_broker_lifecycle",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use makepad_widgets::makepad_platform::{
        event::LocationUpdateEvent, permission::PermissionResult,
    };
    use std::time::{SystemTime, UNIX_EPOCH};
    struct NoSheet;
    impl ServiceHost for NoSheet {
        fn open_sheet(&mut self, _: String) {
            panic!("sampling opened a sheet");
        }
        fn close_sheet(&mut self) {}
    }
    let root = Home::new();
    octosense_appstore::set_data_root(root.0.clone());
    for id in ["os.locationone", "os.locationtwo"] {
        let dir =
            crate::host_tools::script_apps::tests::stamped_bundle("camera", id, |dir, manifest| {
                manifest["id"] = json!(id);
                manifest["capabilities"] = json!(["location"]);
                manifest["requires"] = json!(["host-api-v1"]);
                manifest.as_object_mut().unwrap().remove("agent");
                for path in ["tools.json", "AGENT.md"] {
                    let _ = std::fs::remove_file(dir.join(path));
                }
                std::fs::write(
                    dir.join("main.splash"),
                    "use mod.widgets.*\nApp { Label {text: \"Location test\"} }\n",
                )
                .unwrap();
            });
        let packed = octosense_app_hub::pack::pack_system_app(&dir).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        octosense_appstore::system::register_system_app(octosense_appstore::system::SystemApp {
            id,
            name: "Location test",
            pack: Box::leak(packed.pack_json.into_boxed_str()),
            assets: &[],
        });
    }
    let host_dir = root.0.join(".host");
    for app in ["os.locationone", "os.locationtwo"] {
        consent::set(&host_dir, app, "location", true, None).unwrap();
        assert!(crate::host_tools::script_apps::grants(app, "location"));
    }
    services::register_host_service(Box::new(DeviceService { family: "location" }));
    let mut cx = Cx::new(Box::new(|_, _| {}));
    let call = |app: &str, heap, request, method: &str, args, foreground| {
        services::dispatch(
            ServiceCall {
                app_id: app.into(),
                service: format!("location.{method}"),
                args,
                from_sheet: false,
                may_prompt: foreground,
                host_dir: host_dir.clone(),
            },
            heap,
            request,
            &mut NoSheet,
        );
    };
    let checked = |_: &mut Cx| {
        let state = state().lock().unwrap();
        assert!(
            state.pending.values().all(|work| !work.requesting),
            "sampling must never escalate to an OS permission request"
        );
        state.pending.keys().copied().collect::<Vec<_>>()
    };
    let grant = |cx: &mut Cx, id, status| {
        handle_event(
            cx,
            &Event::PermissionResult(PermissionResult {
                permission: Permission::Location,
                request_id: id,
                status,
            }),
        )
    };
    let fix = || {
        Event::LocationUpdate(LocationUpdateEvent {
            lat: 37.0,
            lon: -122.0,
            accuracy_m: 4.0,
            time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64(),
            altitude_m: None,
            speed_mps: None,
            heading_deg: None,
        })
    };
    let replies = |heap| services::take_replies_for(&[heap]);
    // Foreground gate precedes native work; even a granted app cannot sample
    // from an agent/background surface.
    call("os.locationone", 80101, 1, "sample", json!({}), false);
    let refused = replies(80101);
    assert_eq!(refused.len(), 1);
    assert_eq!(
        refused[0].2.as_ref().unwrap_err(),
        "location.sample is unavailable to agents/background surfaces"
    );
    {
        let state = state().lock().unwrap();
        assert!(state.queued.is_empty());
        assert!(state.pending.is_empty());
        assert!(state.samples.is_empty());
        assert!(state.reviews.is_empty());
        assert!(!state.location_running);
    }
    // Two native permission checks bracket the actual fix delivery.
    call("os.locationone", 80101, 2, "sample", json!({}), true);
    handle_event(&mut cx, &Event::Signal);
    let ids = checked(&mut cx);
    assert_eq!(ids.len(), 1);
    grant(&mut cx, ids[0], PermissionStatus::Granted);
    assert!(state().lock().unwrap().location_running);
    assert!(replies(80101).is_empty());
    checked(&mut cx);
    let mut stale = fix();
    if let Event::LocationUpdate(ref mut value) = stale {
        value.time -= 120.0;
    }
    handle_event(&mut cx, &stale);
    assert!(checked(&mut cx).is_empty(), "stale fix must keep waiting");
    assert_eq!(state().lock().unwrap().samples.len(), 1);
    handle_event(&mut cx, &fix());
    assert!(
        replies(80101).is_empty(),
        "must recheck OS permission before returning a fix"
    );
    let ids = checked(&mut cx);
    assert_eq!(ids.len(), 1);
    grant(&mut cx, ids[0], PermissionStatus::Granted);
    let result: Value = serde_json::from_str(replies(80101)[0].2.as_ref().unwrap()).unwrap();
    assert_eq!(result["freshness"], "fresh");
    assert!(result["timestamp"].as_f64().unwrap() > 0.0);
    assert!(!state().lock().unwrap().location_running);
    // Cancellation is scoped to the app and answers every accepted request once.
    call("os.locationone", 80101, 3, "sample", json!({}), true);
    call("os.locationtwo", 80102, 1, "sample", json!({}), true);
    call(
        "os.locationone",
        80101,
        4,
        "sample.cancel",
        json!({}),
        false,
    );
    let out = replies(80101);
    assert_eq!(out.len(), 2);
    assert!(out
        .iter()
        .any(|v| v.1 == 3 && v.2.as_ref().unwrap_err().starts_with("cancelled:")));
    assert!(replies(80102).is_empty());
    handle_event(&mut cx, &Event::Signal);
    let ids = checked(&mut cx);
    assert_eq!(ids.len(), 1);
    grant(&mut cx, ids[0], PermissionStatus::Granted);
    services::cancel_heap(80102);
    handle_event(&mut cx, &Event::Signal);
    assert!(
        !state().lock().unwrap().location_running,
        "closing isolate releases device"
    );
    // OS denial must be an error, never a permission-status-shaped success.
    call("os.locationone", 80101, 5, "sample", json!({}), true);
    handle_event(&mut cx, &Event::Signal);
    let id = checked(&mut cx)[0];
    grant(&mut cx, id, PermissionStatus::DeniedPermanent);
    assert!(replies(80101)[0]
        .2
        .as_ref()
        .unwrap_err()
        .starts_with("authorization_required:"));
    // Revocation between the fix and its final permission result invalidates it.
    call("os.locationone", 80101, 6, "sample", json!({}), true);
    handle_event(&mut cx, &Event::Signal);
    let id = checked(&mut cx)[0];
    grant(&mut cx, id, PermissionStatus::Granted);
    checked(&mut cx);
    handle_event(&mut cx, &fix());
    let id = checked(&mut cx)[0];
    consent::set(&host_dir, "os.locationone", "location", false, None).unwrap();
    grant(&mut cx, id, PermissionStatus::Granted);
    assert!(replies(80101)[0]
        .2
        .as_ref()
        .unwrap_err()
        .starts_with("permission_denied:"));
    consent::set(&host_dir, "os.locationone", "location", true, None).unwrap();
    // Deadline expiry and backgrounding both stop the feed and cleanup timer.
    for (request, background) in [(7, false), (8, true)] {
        call("os.locationone", 80101, request, "sample", json!({}), true);
        handle_event(&mut cx, &Event::Signal);
        let id = checked(&mut cx)[0];
        grant(&mut cx, id, PermissionStatus::Granted);
        if background {
            handle_event(&mut cx, &Event::Background);
        } else {
            state().lock().unwrap().samples[0].deadline = Instant::now();
            handle_event(&mut cx, &Event::Signal);
        }
        assert!(replies(80101)[0].2.is_err());
        let state = state().lock().unwrap();
        assert!(!state.location_running);
        assert_eq!(state.sample_timer.0, 0);
    }
    handle_event(&mut cx, &Event::Foreground);
    call("os.locationone", 80101, 9, "sample", json!({}), true);
    handle_event(&mut cx, &Event::Signal);
    let id = checked(&mut cx)[0];
    grant(&mut cx, id, PermissionStatus::Granted);
    checked(&mut cx);
    handle_event(&mut cx, &fix());
    let id = checked(&mut cx)[0];
    grant(&mut cx, id, PermissionStatus::DeniedPermanent);
    assert!(
        replies(80101)[0]
            .2
            .as_ref()
            .unwrap_err()
            .starts_with("authorization_required:"),
        "OS revocation after acquisition must refuse the fix"
    );
    call("os.locationone", 80101, 10, "sample", json!({}), true);
    handle_event(&mut cx, &Event::Signal);
    let id = checked(&mut cx)[0];
    grant(&mut cx, id, PermissionStatus::Granted);
    handle_event(
        &mut cx,
        &Event::LocationError(
            makepad_widgets::makepad_platform::event::LocationErrorEvent::Unavailable(
                "provider disabled".into(),
            ),
        ),
    );
    assert!(replies(80101)[0]
        .2
        .as_ref()
        .unwrap_err()
        .starts_with("location_unavailable:"));
    assert!(!state().lock().unwrap().location_running);
    // Deterministically cross the deadline between sample maintenance and
    // the generic broker boundary. Both boundaries used to silently drop the
    // request here, leaving it waiting for the much longer App Hub timeout.
    for (request, native_pending) in [(11, false), (12, true)] {
        call("os.locationone", 80101, request, "sample", json!({}), true);
        if native_pending {
            handle_event(&mut cx, &Event::Signal);
        }
        {
            let mut state = state().lock().unwrap();
            location::maintain(&mut state, &mut cx, &Event::Signal);
            if native_pending {
                state.pending.values_mut().next().unwrap().deadline = Instant::now();
                state.pending.retain(|_, work| work.alive());
                assert!(state.pending.is_empty());
            } else {
                let mut work = state.queued.pop_front().unwrap();
                work.deadline = Instant::now();
                assert!(!work.alive());
            }
            location::sync(&mut state, &mut cx);
            assert_eq!(state.sample_timer.0, 0);
        }
        let out = replies(80101);
        assert_eq!(
            out.len(),
            1,
            "deadline must answer once before dropping work"
        );
        assert_eq!(out[0].2.as_ref().unwrap_err(), location::TIMEOUT_ERROR);
    }
}
