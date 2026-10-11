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
    assert!(matches!(parse_operation("share", &json!({"text":text})).unwrap(), Operation::Share { .. }));
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
        let _staged = STAGED.lock().unwrap();
        locked_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(2)).is_ok()
    });
    locked_rx.recv().unwrap();
    handle_event(&mut cx, &Event::Signal);
    let released_by_ui = release_tx.send(()).is_ok();
    assert!(
        worker.join().unwrap() && released_by_ui,
        "the UI must return before the worker releases its queue and staging locks"
    );
    assert!(QUEUED.try_lock().is_ok());
    assert!(STAGED.try_lock().is_ok());
}

#[test]
fn an_import_reports_only_a_clean_display_name() {
    if cfg!(target_os = "android") {
        assert_eq!(display_name("Report.pdf"), None, "Android's loader names every selection \"document\"");
        return;
    }
    assert_eq!(display_name("Q3 report.pdf").as_deref(), Some("Q3 report.pdf"));
    assert_eq!(display_name("/Users/someone/Documents/Q3.pdf").as_deref(), Some("Q3.pdf"), "never a folder");
    assert_eq!(display_name("C:\\Users\\someone\\Q3.pdf").as_deref(), Some("Q3.pdf"));
    assert_eq!(display_name("Invoice\u{202E}fdp.exe").as_deref(), Some("Invoicefdp.exe"), "no direction overrides");
    assert_eq!(display_name("line\nbreak\t.pdf").as_deref(), Some("linebreak.pdf"));
    for nothing in ["", "   ", "/", ".", "..", "dir/", "\u{200B}"] {
        assert_eq!(display_name(nothing), None, "{nothing:?}");
    }
    let long = format!("{}.pdf", "é".repeat(100));
    let shown = display_name(&long).unwrap();
    assert!(shown.len() <= 128 && long.starts_with(&shown), "cut on a character boundary: {shown}");
}

#[test]
fn imports_take_documents_above_the_script_write_limit_within_the_quota() {
    let import = Operation::Import { path: "a.pdf".into() };
    let photo = Operation::PickPhoto { path: "a.png".into() };
    assert!(MAX_IMPORT > MAX_FILE_BYTES && MAX_IMPORT <= MAX_IMPORT_BYTES);
    assert_eq!(MAX_IMPORT, if cfg!(target_os = "android") { 16 << 20 } else { 64 << 20 });
    assert_eq!(import.selection_limit(u64::MAX), MAX_IMPORT);
    assert_eq!(import.selection_limit(8 << 20), 8 << 20, "never more than the app's quota");
    assert_eq!(photo.selection_limit(u64::MAX), MAX_FILE_BYTES, "photo picks keep their 1 MiB bound");
    assert_eq!(photo.selection_limit(4096), 4096);
}

/// The import path after the native picker: a selection above the script
/// write limit is staged by a worker and linked into the admitted app's live
/// jail between script turns; one past the app's free storage is refused at
/// commit and leaves nothing behind. A child process owns the global
/// registries; no dialog or device is opened.
#[test]
fn a_selected_document_is_staged_off_the_ui_thread_and_linked_in_against_the_live_quota() {
    const CHILD: &str = "OCTOSENSE_FILES_IMPORT_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "files_service::tests::a_selected_document_is_staged_off_the_ui_thread_and_linked_in_against_the_live_quota",
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
    use makepad_widgets::*;
    const APP: &str = "os.filesimport";
    let root = std::env::temp_dir().join(format!("files-import-{}", uuid::Uuid::new_v4()));
    octosense_appstore::set_data_root(root.join("apps"));
    let dir = crate::host_tools::script_apps::tests::stamped_bundle("camera", APP, |dir, manifest| {
        manifest["id"] = json!(APP);
        manifest["capabilities"] = json!([]);
        manifest["requires"] = json!(["host-api-v1"]);
        manifest["storage"] = json!({"max_bytes": 8 << 20});
        manifest.as_object_mut().unwrap().remove("agent");
        for path in ["tools.json", "AGENT.md"] {
            let _ = std::fs::remove_file(dir.join(path));
        }
        std::fs::write(dir.join("main.splash"), "use mod.widgets.*\nApp { Label {text: \"Import test\"} }\n").unwrap();
    });
    let packed = octosense_app_hub::pack::pack_system_app(&dir).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    octosense_appstore::system::register_system_app(octosense_appstore::system::SystemApp {
        id: APP,
        name: "Import test",
        pack: Box::leak(packed.pack_json.into_boxed_str()),
        assets: &[],
    });
    let manifest = admission(APP, &root.join("apps/.host")).unwrap();

    let jail = root.join("jail");
    std::fs::create_dir_all(&jail).unwrap();
    let mut cx = Cx::new(Box::new(|_, _| {}));
    let mut card = cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        let value = vm.eval(script! {use mod.widgets.* Splash{}});
        Splash::script_from_value(vm, value)
    });
    card.set_policy(&mut cx, Some(vec![]), None);
    card.set_host_tag(&mut cx, Some(APP.into()));
    card.set_sandbox_dir(&mut cx, Some(jail.clone()));
    card.set_storage_quota(&mut cx, Some(8 << 20));
    card.set_text(&mut cx, "View {}");
    let heap = card.isolate_heap_key(&mut cx).unwrap();

    static CAPTURED: std::sync::Mutex<Option<(ServiceCall, Replier)>> = std::sync::Mutex::new(None);
    struct Capture;
    impl HostService for Capture {
        fn family(&self) -> &'static str {
            "files_import_fixture"
        }
        fn call(&mut self, call: ServiceCall, reply: Replier, _: &mut dyn ServiceHost) {
            *CAPTURED.lock().unwrap() = Some((call, reply));
        }
    }
    struct NoSheet;
    impl ServiceHost for NoSheet {
        fn open_sheet(&mut self, _: String) {
            panic!("an import opened a sheet");
        }
        fn close_sheet(&mut self) {}
    }
    services::register_host_service(Box::new(Capture));

    // A native-only dispatch makes the pending reply; the selection arrives
    // as the native loader delivers it.
    let mut import = |request: u64, path: &str, len: usize| -> Result<serde_json::Value, String> {
        services::dispatch(
            ServiceCall {
                app_id: APP.into(),
                service: "files_import_fixture.import".into(),
                args: json!({"path": path}),
                from_sheet: false,
                may_prompt: true,
                host_dir: root.join("apps/.host"),
            },
            heap,
            request,
            &mut NoSheet,
        );
        let (call, reply) = CAPTURED.lock().unwrap().take().unwrap();
        let id = LiveId::unique();
        let work = Work {
            call,
            reply,
            operation: Operation::Import { path: path.into() },
            manifest: Some(manifest.clone()),
            _reservation: std::sync::Arc::new(Reservation::acquire().unwrap()),
        };
        PENDING.with(|slot| *slot.borrow_mut() = Some(Pending { id, work, export: None }));
        let bytes: std::sync::Arc<[u8]> = vec![7u8; len].into();
        let file = VirtualFile { name: "document.pdf".into(), mime: "application/pdf".into(), bytes, size: len as u64 };
        handle_event(&mut cx, &Event::Actions(vec![Box::new(FileDialogAction::FileLoaded { id, files: vec![file] })]));
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            handle_event(&mut cx, &Event::Signal);
            if let Some((_, _, result)) = services::take_replies_for(&[heap]).pop() {
                return result.map(|text| serde_json::from_str(&text).unwrap());
            }
            assert!(std::time::Instant::now() < deadline, "the staged import was never committed");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let big = 3 * MAX_FILE_BYTES as usize;
    assert_eq!(
        import(1, "documents/report.pdf", big).unwrap(),
        json!({"cancelled": false, "path": "documents/report.pdf", "bytes": big, "name": "document.pdf"})
    );
    assert_eq!(std::fs::read(jail.join("documents/report.pdf")).unwrap(), vec![7u8; big]);
    let full = import(2, "documents/second.pdf", 6 * MAX_FILE_BYTES as usize).unwrap_err();
    assert!(full.contains("full"), "{full}");
    assert!(!jail.join("documents/second.pdf").exists());
    let staging: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(".native-import-"))
        .collect();
    assert!(staging.is_empty(), "abandoned staging files: {staging:?}");
    let _ = std::fs::remove_dir_all(root);
}

/// Without live storage, `files.status` says nothing of the storage's use
/// and does not measure a jail.
#[test]
fn status_without_storage_reports_no_use_or_quota() {
    let answer = status_answer(None, || panic!("no jail is measured without live storage"));
    assert_eq!(answer["storage_granted"], false);
    assert!(answer.get("used_bytes").is_none() && answer.get("quota_bytes").is_none(), "{answer}");
    assert_eq!(answer["max_import_bytes"], MAX_IMPORT);
    let schema = &FilesService.api_methods()[0].output_schema;
    for key in ["used_bytes", "quota_bytes"] {
        assert_eq!(schema["properties"][key]["type"], "integer", "discovery declares {key}");
    }
}

/// `files.status` says how much the calling app's storage holds and may
/// hold: `used_bytes`, as the host's app storage measures the app's jail
/// (the root App Hub gives its isolates), and `quota_bytes`, the live
/// storage scope's quota; a write shows on the next status. A child
/// process owns the global registries and the host's app storage, in a
/// home of its own.
#[test]
fn status_reports_the_storage_used_and_its_quota() {
    const CHILD: &str = "OCTOSENSE_FILES_STATUS_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let home = std::env::temp_dir().join(format!("files-status-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "files_service::tests::status_reports_the_storage_used_and_its_quota", "--nocapture"])
            .env(CHILD, &home)
            // Nothing outside this home: no legacy app homes to adopt.
            .env("OCTOSENSE_HOME", home.join("home"))
            .env_remove("OCTOSENSE_APP_DATA")
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&home);
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        return;
    }
    use makepad_widgets::*;
    const APP: &str = "os.filesstatus";
    let root = std::path::PathBuf::from(std::env::var_os(CHILD).unwrap());
    let storage = crate::app_storage::init(Some(root.clone())).expect("the host's app storage");
    let apps = storage.layout().apps_root().to_path_buf();
    octosense_appstore::set_data_root(apps.clone());
    let dir = crate::host_tools::script_apps::tests::stamped_bundle("camera", APP, |dir, manifest| {
        manifest["id"] = json!(APP);
        manifest["capabilities"] = json!([]);
        manifest["requires"] = json!(["host-api-v1"]);
        manifest["storage"] = json!({"max_bytes": 8 << 20});
        manifest.as_object_mut().unwrap().remove("agent");
        for path in ["tools.json", "AGENT.md"] {
            let _ = std::fs::remove_file(dir.join(path));
        }
        std::fs::write(dir.join("main.splash"), "use mod.widgets.*\nApp { Label {text: \"Status test\"} }\n").unwrap();
    });
    let packed = octosense_app_hub::pack::pack_system_app(&dir).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    octosense_appstore::system::register_system_app(octosense_appstore::system::SystemApp {
        id: APP,
        name: "Status test",
        pack: Box::leak(packed.pack_json.into_boxed_str()),
        assets: &[],
    });

    // The app's jail holds 3,002 bytes; its isolate is rooted there with an
    // 8 MiB quota, as App Hub seats it.
    let jail = storage.layout().app(APP).unwrap().jail;
    std::fs::create_dir_all(jail.join("library")).unwrap();
    std::fs::write(jail.join("library/a.pdf"), vec![1u8; 3_000]).unwrap();
    std::fs::write(jail.join("notes.json"), b"{}").unwrap();
    let mut cx = Cx::new(Box::new(|_, _| {}));
    let mut card = cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        let value = vm.eval(script! {use mod.widgets.* Splash{}});
        Splash::script_from_value(vm, value)
    });
    card.set_policy(&mut cx, Some(vec![]), None);
    card.set_host_tag(&mut cx, Some(APP.into()));
    card.set_sandbox_dir(&mut cx, Some(jail.clone()));
    card.set_storage_quota(&mut cx, Some(8 << 20));
    card.set_text(&mut cx, "View {}");
    let heap = card.isolate_heap_key(&mut cx).unwrap();

    struct NoSheet;
    impl ServiceHost for NoSheet {
        fn open_sheet(&mut self, _: String) {
            panic!("status opened a sheet");
        }
        fn close_sheet(&mut self) {}
    }
    register();
    let status = |request: u64| -> serde_json::Value {
        let call = ServiceCall { app_id: APP.into(), service: "files.status".into(), args: json!({}), from_sheet: false, may_prompt: false, host_dir: apps.join(".host") };
        services::dispatch(call, heap, request, &mut NoSheet);
        let (_, _, result) = services::take_replies_for(&[heap]).pop().expect("status answers at once");
        serde_json::from_str(&result.expect("status answers")).unwrap()
    };
    let first = status(1);
    assert_eq!((first["storage_granted"].clone(), first["used_bytes"].clone(), first["quota_bytes"].clone()), (json!(true), json!(3_002), json!(8 << 20)), "{first}");
    std::fs::write(jail.join("library/b.pdf"), vec![2u8; 1_000]).unwrap();
    assert_eq!(status(2)["used_bytes"], 4_002, "a write shows on the next status");
    // A live scope whose jail cannot be measured says neither.
    let live = splash_storage::storage_for_heap(heap, APP).expect("live storage");
    let unmeasured = status_answer(Some(&live), || None);
    assert_eq!(unmeasured["storage_granted"], true);
    assert!(unmeasured.get("used_bytes").is_none() && unmeasured.get("quota_bytes").is_none(), "{unmeasured}");
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
