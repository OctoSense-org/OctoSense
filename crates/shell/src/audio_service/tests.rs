use super::*;
use makepad_widgets::*;

#[test]
fn declaration_does_not_replace_audio_admission_or_microphone_consent() {
    const CHILD: &str = "OCTOSENSE_AUDIO_DECLARATION_TEST";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "audio_service::tests::declaration_does_not_replace_audio_admission_or_microphone_consent", "--nocapture"])
            .env(CHILD, "1").output().unwrap();
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        return;
    }
    let root = std::env::temp_dir().join(format!("audio-declaration-{}", uuid::Uuid::new_v4()));
    octosense_appstore::set_data_root(root.clone());
    for (app, capabilities) in [
        ("os.audiodeclared", &["audio", "microphone", "storage"][..]),
        ("os.audioundeclared", &[][..]),
    ] {
        crate::host_tools::script_apps::tests::declaration_fixture(app, capabilities);
        assert!(admission(app, &root.join(".host")).is_ok());
        assert!(admission(app, &root.join("other-profile/.host")).is_err());
        assert!(crate::platform_services::microphone_consent(&root.join(".host"), app).is_err(), "a declaration never supplies consent");
    }
    assert!(admission("os.notinstalled", &root.join(".host")).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn paths_arguments_and_capture_duration_are_bounded_before_device_work() {
    assert!(parse(
        "microphone",
        "record_start",
        &json!({"path":"voice/reply.wav","max_duration_ms":30000})
    )
    .is_ok());
    for args in [
        json!({"path":"../outside"}),
        json!({"path":"/"}),
        json!({"path":"C:\\secret"}),
        json!({"path":"reply.wav","max_duration_ms":30001}),
        json!({"path":"reply.wav","max_duration_ms":0}),
        json!({"path":"reply.wav","max_duration_ms":1.5}),
        json!({"path":"reply.wav","device":"loopback"}),
        json!({"path":"reply.wav","background":true}),
    ] {
        assert!(
            parse("microphone", "record_start", &args).is_err(),
            "{args}"
        );
    }
    assert!(parse(
        "audio",
        "play",
        &json!({"url":"https://example.org/private.wav"})
    )
    .is_err());
    assert!(parse(
        "audio",
        "play",
        &json!({"path":"reply.wav","account":"other"})
    )
    .is_err());
    assert!(parse("audio", "status", &json!({"session":"guessed"})).is_err());
}

#[test]
fn native_sessions_follow_foreground_heap_identity_and_account_storage_without_a_draw() {
    let root = std::env::temp_dir().join(format!("audio-lifecycle-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut cx = Cx::new(Box::new(|_, _| {}));
    let mut card = cx.with_vm(|vm| {
        makepad_widgets::script_mod(vm);
        let value = vm.eval(script! {use mod.widgets.* Splash{}});
        Splash::script_from_value(vm, value)
    });
    card.set_policy(&mut cx, Some(vec![]), None);
    card.set_host_tag(&mut cx, Some("sample.audio".into()));
    card.set_sandbox_dir(&mut cx, Some(root.clone()));
    card.set_text(&mut cx, "View {}");
    let heap = card.isolate_heap_key(&mut cx).unwrap();
    let storage = splash_storage::storage_for_heap(heap, "sample.audio").unwrap();
    let (tx, rx) = mpsc::sync_channel(2);
    let mut session = Session {
        id: uuid::Uuid::new_v4().to_string(),
        app: "sample.audio".into(),
        heap,
        host: root.clone(),
        kind: Kind::Record,
        path: "voice.wav".into(),
        millis: 30000,
        storage,
        shared: Arc::new(Shared::new()),
        rx,
        start_reply: None,
        revision: None,
        permission: None,
        lease: None,
        pcm: None,
        state: "preparing",
        error: None,
        started: Instant::now(),
        ended: None,
        device: None,
        last_frames: 0,
        last_frame_at: Instant::now(),
    };
    assert!(session.owns("sample.audio", heap));
    assert!(!session.owns("other.audio", heap));
    assert!(!session.owns("sample.audio", heap + 1));
    assert!(live(&session, Some("sample.audio"), false));
    assert!(!live(&session, Some("other.audio"), false));
    assert!(!live(&session, Some("sample.audio"), true));
    card.set_host_prompts(&mut cx, false);
    assert!(!live(&session, Some("sample.audio"), false));
    card.set_host_prompts(&mut cx, true);
    assert!(live(&session, Some("sample.audio"), false));
    session.revision = Some(0);
    assert!(
        !live(&session, Some("sample.audio"), false),
        "an OS grant cannot replace app consent"
    );
    session.revision = None;
    let id = session.id.clone();
    let mut state = State {
        foreground: Some("sample.audio".into()),
        ..Default::default()
    };
    assert_eq!(
        state.window_unfocused,
        cfg!(target_os = "macos"),
        "macOS must wait for its first native focus event"
    );
    state.sessions.insert(id.clone(), session);
    maintain(&mut cx, &mut state, &Event::WindowLostFocus(WindowId(0, 0)));
    assert!(state.window_unfocused);
    assert_eq!(state.sessions[&id].state, "cancelled");
    assert!(state.sessions[&id].shared.stop.load(Ordering::Acquire));
    maintain(&mut cx, &mut state, &Event::Foreground);
    assert!(
        state.window_unfocused,
        "host resume cannot forge window focus"
    );
    maintain(&mut cx, &mut state, &Event::WindowGotFocus(WindowId(0, 0)));
    assert!(!state.window_unfocused);
    assert_eq!(
        state.sessions[&id].state, "cancelled",
        "focus gain never resumes capture"
    );

    // Queue an explicit Cancel before the UI sees a worker's ready file.
    // Native-only dispatch creates the normal pending reply; it does not
    // activate a device or grant an app any production capability.
    struct CancelFixture;
    impl HostService for CancelFixture {
        fn family(&self) -> &'static str {
            "audio_cancel_fixture"
        }
        fn call(&mut self, call: ServiceCall, reply: Replier, _: &mut dyn ServiceHost) {
            let id = call.args["session"].as_str().unwrap().to_owned();
            assert!(queue()
                .push(Work {
                    call,
                    reply,
                    operation: Operation::Cancel(id)
                })
                .is_ok());
        }
    }
    struct NoSheet;
    impl ServiceHost for NoSheet {
        fn open_sheet(&mut self, _: String) {
            panic!("audio cannot open a sheet");
        }
        fn close_sheet(&mut self) {
            panic!("audio cannot close another sheet");
        }
    }
    services::register_host_service(Box::new(CancelFixture));
    let session = state.sessions.get_mut(&id).unwrap();
    session.state = "stopping";
    session.ended = None;
    session.shared = Arc::new(Shared::new());
    let staged = session
        .storage
        .worker_snapshot()
        .prepare_import("voice.wav", b"synthetic PCM staging")
        .unwrap();
    assert!(tx.send(Update::Finished(Ok(Some(staged)))).is_ok());
    services::dispatch(
        ServiceCall {
            app_id: "sample.audio".into(),
            service: "audio_cancel_fixture.cancel".into(),
            args: json!({"session":id}),
            host_dir: root.clone(),
            from_sheet: false,
            may_prompt: true,
        },
        heap,
        88001,
        &mut NoSheet,
    );
    maintain(&mut cx, &mut state, &Event::Signal);
    assert_eq!(state.sessions[&id].state, "cancelled");
    assert!(
        !root.join("voice.wav").exists(),
        "queued Cancel must win over a ready file commit"
    );
    let replies = services::take_replies_for(&[heap]);
    assert_eq!(replies.len(), 1);
    let result: Value = serde_json::from_str(replies[0].2.as_ref().unwrap()).unwrap();
    assert_eq!(result["status"], "cancelled");
    let mut session = state.sessions.remove(&id).unwrap();
    card.set_sandbox_dir(&mut cx, Some(root.join("other-account")));
    assert!(!live(&session, Some("sample.audio"), false));
    session.finish(
        &mut cx,
        "cancelled",
        Some("cancelled: foreground changed".into()),
    );
    assert!(session.shared.cancel.load(Ordering::Acquire));
    assert!(session.shared.stop.load(Ordering::Acquire));
    let cancelled = session.snapshot();
    session.stop(&mut cx);
    session.cancel(&mut cx);
    assert_eq!(
        session.snapshot(),
        cancelled,
        "repeated stop/cancel preserves terminal truth"
    );
    session.state = "saved";
    session.error = None;
    let saved = session.snapshot();
    session.cancel(&mut cx);
    session.stop(&mut cx);
    assert_eq!(
        session.snapshot(),
        saved,
        "cancel cannot relabel an already saved file"
    );
    assert!(
        !root.join("voice.wav").exists(),
        "cancellation never stores captured bytes"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn microphone_ownership_is_exclusive_and_reusable_after_release() {
    let first = AudioInputLease::try_acquire().unwrap();
    assert!(AudioInputLease::try_acquire().is_none());
    drop(first);
    let next = AudioInputLease::try_acquire().unwrap();
    drop(next);
}

#[test]
fn bounded_capture_queue_reports_overflow_without_growing() {
    let shared = Shared::new();
    let chunk = Chunk {
        rate: 48000,
        len: CHUNK,
        samples: [0.0; CHUNK],
    };
    for _ in 0..32 {
        assert!(shared.queue.push(chunk).is_ok());
    }
    assert!(shared.queue.push(chunk).is_err());
    assert_eq!(shared.queue.len(), 32);
    shared.queue.pop();
    assert!(shared.queue.push(chunk).is_ok());
}

#[test]
fn discovery_never_advertises_background_capture_or_unsupported_platforms() {
    let mut methods = microphone_methods();
    methods.extend(AudioService.api_methods());
    assert_eq!(methods.len(), if supported() { 7 } else { 0 });
    for method in methods {
        method.validate().unwrap();
        assert_eq!(method.agent_access, AgentAccess::ForegroundOnly);
        assert_eq!(method.platforms, vec!["macos", "android"]);
    }
}
