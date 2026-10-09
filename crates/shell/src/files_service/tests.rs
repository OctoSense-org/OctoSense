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
        1 + if file_dialogs::native_file_bytes_supported() {
            3
        } else {
            0
        } + if cfg!(target_os = "android") { 1 } else { 0 }
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

#[test]
fn photo_picker_checks_content_not_filename_and_reuses_jailed_destination() {
    assert_eq!(
        parse_operation("pick_photo", &json!({"path":"/photos/new.png"})).unwrap(),
        Operation::PickPhoto {
            path: "/photos/new.png".into()
        }
    );
    assert!(parse_operation("pick_photo", &json!({"path":"../private/photo.png"})).is_err());
    assert!(parse_operation(
        "pick_photo",
        &json!({"path":"photo.png","allow_library":true})
    )
    .is_err());
    assert_eq!(image_mime(b"\x89PNG\r\n\x1a\n").unwrap(), "image/png");
    assert_eq!(image_mime(&[0xff, 0xd8, 0xff, 0xe0]).unwrap(), "image/jpeg");
    assert_eq!(image_mime(b"RIFF\0\0\0\0WEBPVP8X").unwrap(), "image/webp");
    assert!(image_mime(b"RIFF\0\0\0\0WAVEdata").is_err());
    for fake in [
        b"renamed text.png".as_slice(),
        b"\x89PNG",
        b"<svg></svg>",
        b"",
    ] {
        assert!(image_mime(fake).is_err());
    }
}
#[test]
fn sharing_has_bounded_text_no_file_authority_and_truthful_handoff() {
    let text = "你好 🌅 music 🎵";
    assert_eq!(
        parse_operation("share", &json!({"text":text})).unwrap(),
        Operation::Share { text: text.into() }
    );
    assert!(!parse_operation("share", &json!({"text":text}))
        .unwrap()
        .needs_storage());
    for args in [
        json!({"text":""}),
        json!({"text":"a\0b"}),
        json!({"text":"x".repeat(MAX_SHARE_TEXT+1)}),
        json!({"path":"photo.jpg"}),
        json!({"text":"hello","recipient":"someone"}),
    ] {
        assert!(parse_operation("share", &args).is_err());
    }
    assert_eq!(
        share_outcome(&json!({"outcome":"opened"})).unwrap(),
        json!({"handoff":"chooser_opened","delivery":"unknown"})
    );
    for value in [
        json!({}),
        json!({"outcome":"delivered"}),
        json!({"outcome":"error","reason":"foreground_required"}),
    ] {
        assert!(share_outcome(&value).is_err());
    }
}

#[test]
fn native_event_pump_never_waits_for_a_producer_queue_lock() {
    // A real worker holds the producer lock until the event pump returns.
    // The timeout keeps a regression bounded; success releases it explicitly.
    let mut cx = Cx::new(Box::new(|_, _| {}));
    let (locked_tx, locked_rx) = std::sync::mpsc::sync_channel(0);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
        let _queue = QUEUED.lock().unwrap();
        locked_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(2)).is_ok()
    });
    locked_rx.recv().unwrap();
    handle_event(&mut cx, &Event::Signal);
    let released_by_ui = release_tx.send(()).is_ok();
    assert!(
        worker.join().unwrap() && released_by_ui,
        "the UI must return before the worker releases its queue lock"
    );
    assert!(QUEUED.try_lock().is_ok());
}

#[test]
fn queued_foreground_authority_expires_before_launch_but_not_chooser_completion() {
    use makepad_widgets::WindowId;
    let app = "org.example.files";
    let queued_origin_may_prompt = true;
    let mut foreground = Foreground {
        app: Some(app.into()), background: false, window_unfocused: false,
    };
    assert!(foreground.allows_launch(app, queued_origin_may_prompt, true));
    for event in [Event::Pause, Event::Background, Event::WindowLostFocus(WindowId(0, 0))] {
        foreground.event(&event);
        assert!(!foreground.allows_launch(app, queued_origin_may_prompt, true),
            "a valid queued origin cannot outlive foreground loss");
        foreground.event(&Event::Resume);
        foreground.event(&Event::WindowGotFocus(WindowId(0, 0)));
    }
    foreground.app = Some("org.example.other".into());
    assert!(!foreground.allows_launch(app, queued_origin_may_prompt, true));
    foreground.app = Some(app.into());
    assert!(!foreground.allows_launch(app, queued_origin_may_prompt, false), "a closed/suspended surface cannot launch");
    assert!(!foreground.allows_launch(app, false, true), "returning to foreground cannot upgrade a background origin");
    assert!(foreground.allows_launch(app, queued_origin_may_prompt, true));
    if cfg!(target_os = "macos") {
        let hidden = Foreground { app: Some(app.into()), ..Foreground::default() };
        assert!(!hidden.allows_launch(app, queued_origin_may_prompt, true));
    }
    foreground.event(&Event::WindowLostFocus(WindowId(0, 0)));
    let manifest = json!({"capabilities":["files","storage"]});
    assert!(check_authorization(true, Some(&manifest), || Ok(manifest.clone())).is_ok(),
        "the already-open native chooser may retain focus while its authorized result completes");
}
