//! Small native acceptance host: real contained Splash -> Video::script_call
//! -> platform decoder. The only media is a bundled, silent synthetic clip.
use makepad_widgets::*;
use serde_json::{json, Value};
use std::{io::Write, path::PathBuf};

app_main!(App, font_set: International);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root { main_window := Window {
            window.inner_size: vec2(680, 680)
            body +: { app := Splash {width: Fill height: Fill} }
        }}
    }
}

#[derive(Script, ScriptHook)]
struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    card: SplashRef,
    #[rust]
    root: Option<PathBuf>,
    #[rust]
    heap: Option<usize>,
    #[rust]
    video_id: Option<LiveId>,
    #[rust]
    prepared: u64,
    #[rust]
    released: u64,
    #[rust]
    frames: u64,
    #[rust]
    native_position_ms: u128,
    #[rust]
    resume_positions_ms: Vec<u128>,
    #[rust]
    observing_resume: bool,
    #[rust]
    contained: bool,
    #[rust]
    sequence: u64,
    #[rust]
    script: Value,
    #[rust]
    error: Option<String>,
    #[rust]
    closed: bool,
    #[rust]
    closed_heap: bool,
    #[rust]
    close_released: bool,
    #[rust]
    frames_after_close_release: u64,
    #[rust]
    timer: Option<Timer>,
    #[rust]
    bridge_sequence: u64,
    #[rust]
    bridge_ready: bool,
    #[rust]
    bridge: Value,
}

impl App {
    fn write_state(&self) {
        let Some(root) = &self.root else { return };
        let state = json!({
            "schema":1, "sequence":self.sequence, "script":self.script,
            "native":{"prepared":self.prepared,"released":self.released,
                "frames":self.frames,"position_ms":self.native_position_ms,
                "resume_positions_ms":self.resume_positions_ms,
                "error":self.error,"closed":self.closed,"heap_closed":self.closed_heap,
                "close_released":self.close_released,
                "frames_after_close_release":self.frames_after_close_release},
            "contained":self.contained, "synthetic_media":true, "personal_data":false,
            "bridge":self.bridge, "bridge_ready":self.bridge_ready,
            "bridge_sequence":self.bridge_sequence
        });
        let temporary = root.join("state.json.tmp");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(&state).unwrap())
            .unwrap();
        drop(file);
        std::fs::rename(temporary, root.join("state.json")).unwrap();
    }
    /// Android has no standalone Makepad remote server. This fixture-only lane
    /// accepts bounded test commands in its private directory, then invokes the
    /// normal contained Button callback. It never calls a Rust VideoRef, injects
    /// OS input, opens a listener, or supplies trusted approval provenance.
    #[cfg(target_os = "android")]
    fn handle_android_command(&mut self, cx: &mut Cx, event: &Event) {
        use std::io::Read;
        if !self
            .timer
            .is_some_and(|timer| timer.is_event(event).is_some())
        {
            return;
        }
        if !self.bridge_ready {
            self.bridge_ready = cx.widget_tree().snapshot(cx).into_iter().any(|row| {
                row.widget_type == "Button"
                    && row.text.as_deref() == Some("Probe")
                    && row.visible
                    && row.enabled
            });
            if !self.bridge_ready {
                return;
            }
            self.write_state();
        }
        let Some(root) = &self.root else { return };
        let path = root.join("command.json");
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) => {
                self.bridge = json!({"error":"command_read_failed", "physical_input":false});
                self.write_state();
                return;
            }
        };
        let mut bytes = Vec::new();
        let read_ok = file.take(513).read_to_end(&mut bytes).is_ok();
        // Removal consumes this envelope exactly once; the driver writes through
        // a temporary file and atomic rename, never modifies an open envelope.
        let remove_ok = std::fs::remove_file(path).is_ok();
        let result = (|| -> Result<(), &'static str> {
            if !read_ok || !remove_ok || bytes.len() > 512 {
                return Err("invalid_command_size");
            }
            let value: Value =
                serde_json::from_slice(&bytes).map_err(|_| "invalid_command_json")?;
            let object = value.as_object().ok_or("invalid_command_object")?;
            if object.len() != 2 {
                return Err("invalid_command_fields");
            }
            let sequence = value["sequence"].as_u64().ok_or("invalid_sequence")?;
            if sequence != self.bridge_sequence + 1 || sequence > 4096 {
                return Err("invalid_sequence");
            }
            let action = value["action"].as_str().ok_or("invalid_action")?;
            let label = match action {
                "prepare" => "Prepare",
                "play" => "Play",
                "pause" => "Pause",
                "resume" => "Resume",
                "seek" => "Seek 2s",
                "seek-start" => "Seek start",
                "stop" => "Stop",
                "probe" => "Probe",
                "args" => "Check args",
                "close" => "Close app",
                _ => return Err("unknown_action"),
            };
            if self.closed || self.heap.is_none() {
                return Err("app_closed");
            }
            // Snapshot visibility prevents a clipped/offscreen button from
            // being called a usable rendered control. Invocation is still a
            // callback-level test, not a physical pointer/touch acceptance.
            let visible = cx
                .widget_tree()
                .snapshot(cx)
                .into_iter()
                .filter(|row| {
                    row.widget_type == "Button"
                        && row.text.as_deref() == Some(label)
                        && row.visible
                        && row.enabled
                        && row.width > 0
                        && row.height > 0
                })
                .count();
            if visible != 1 {
                return Err("button_not_uniquely_visible");
            }
            let buttons: Vec<_> = cx
                .widget_tree()
                .flat_tree(cx)
                .into_iter()
                .filter_map(|row| {
                    let widget = cx.widget_tree().widget(WidgetUid(row.uid));
                    let matches = widget.borrow::<Button>().is_some() && widget.text() == label;
                    matches.then_some(widget)
                })
                .collect();
            if buttons.len() != 1 {
                return Err("button_not_unique");
            }
            let accepted = makepad_platform::with_untrusted_input(|| {
                cx.with_vm(|vm| {
                    matches!(buttons[0].script_call(vm, live_id!(on_click), NIL),
                    ScriptAsyncResult::Return(value) if value == TRUE)
                })
            });
            if !accepted {
                return Err("button_callback_refused");
            }
            self.bridge_sequence = sequence;
            self.bridge = json!({"sequence":sequence,"action":action,"callback_queued":true,
                "physical_input":false,"transport":"app_private_command_file","error":null});
            Ok(())
        })();
        if let Err(error) = result {
            self.bridge = json!({"error":error,"physical_input":false});
        }
        self.write_state();
    }

    fn observe_native(&mut self, event: &Event) {
        let mut changed = false;
        match event {
            Event::VideoPlaybackPrepared(e) => {
                assert!(self.video_id.is_none_or(|id| id == e.video_id));
                self.video_id = Some(e.video_id);
                self.prepared += 1;
                self.native_position_ms = 0;
                changed = true;
            }
            Event::VideoTextureUpdated(e) if self.video_id == Some(e.video_id) => {
                self.frames += 1;
                self.native_position_ms = e.current_position_ms;
                if self.observing_resume && self.resume_positions_ms.len() < 8 {
                    self.resume_positions_ms.push(e.current_position_ms);
                }
                if self.close_released {
                    self.frames_after_close_release += 1;
                    changed = true;
                }
            }
            Event::VideoPlaybackResourcesReleased(e) if self.video_id == Some(e.video_id) => {
                self.released += 1;
                self.close_released |= self.closed;
                changed = true;
            }
            Event::VideoDecodingError(e) if self.video_id.is_none_or(|id| id == e.video_id) => {
                self.error = Some(e.error.clone());
                changed = true;
            }
            _ => {}
        }
        // No per-frame disk IO: reports snapshot frame counters when requested.
        if changed {
            self.write_state();
        }
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        #[cfg(not(target_os = "android"))]
        let root = PathBuf::from(
            std::env::args()
                .find_map(|arg| arg.strip_prefix("--out=").map(str::to_owned))
                .expect("Pass a fresh --out directory for synthetic evidence"),
        );
        #[cfg(target_os = "android")]
        let root = PathBuf::from(cx.get_data_dir().expect("Android private directory"))
            .join("video-api-lab");
        std::fs::create_dir(&root).expect("Fixture output must be a fresh directory");
        let jail = root.join("app-storage");
        std::fs::create_dir(&jail).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(jail.join("playback.mp4"))
            .unwrap();
        file.write_all(include_bytes!("../resources/playback.mp4"))
            .unwrap();
        self.root = Some(root);
        self.script = json!({});
        self.bridge = json!({"enabled":cfg!(target_os = "android"),"sequence":0,
            "physical_input":false,"error":null});
        self.card = self.ui.splash(cx, ids!(app));
        self.card.set_policy(cx, Some(vec![]), None);
        self.card
            .set_host_caps(cx, vec!["storage".into(), "video_lab".into()]);
        self.card
            .set_host_tag(cx, Some("org.octosense.video-smoke".into()));
        self.card.set_host_prompts(cx, false);
        self.card.set_sandbox_dir(cx, Some(jail.clone()));
        self.card
            .set_text(cx, include_str!("../resources/main.splash"));
        self.heap = self.card.isolate_heap_key(cx);
        let heap = self.heap.expect("Fixture must own a live contained heap");
        self.contained = splash_policy::is_enforced(heap)
            && splash_policy::service_allowed(heap, "storage.read").is_ok()
            && splash_policy::service_allowed(heap, "video_lab.report").is_ok()
            && splash_policy::service_allowed(heap, "camera.preview").is_err()
            && splash_policy::local_path_for_heap(heap, "playback.mp4")
                .is_some_and(|path| PathBuf::from(path) == jail.join("playback.mp4"))
            && splash_policy::local_path_for_heap(heap, "../outside.mp4").is_none()
            && !splash_policy::url_allowed(heap, "https://video.invalid/fixture.mp4");
        assert!(self.contained, "Fixture must retain the app storage jail");
        self.timer = Some(cx.start_interval(0.05));
        self.write_state();
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::theme_mod(vm);
        script_eval!(vm, {mod.theme = mod.themes.light});
        makepad_widgets::widgets_mod(vm);
        widget_async::set_splash_theme(widget_async::SplashTheme::Light);
        self::script_mod(vm)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        #[cfg(target_os = "android")]
        self.handle_android_command(cx, event);
        self.observe_native(event);
        if let Some(heap) = self.heap {
            for request in splash_host::take_splash_host_requests_for(&[heap]) {
                assert_eq!(request.app_tag, "org.octosense.video-smoke");
                match request.service.as_str() {
                    "video_lab.report" => {
                        self.script = serde_json::from_str(&request.args_json).unwrap();
                        if self.script["action"] == "resume" {
                            self.resume_positions_ms.clear();
                            self.observing_resume = true;
                        } else if self.script["action"] != "probe" {
                            self.observing_resume = false;
                        }
                        self.sequence += 1;
                        self.write_state();
                    }
                    "video_lab.close" => {
                        self.closed = true;
                        // Actual app teardown, not a Rust VideoRef Stop call.
                        self.card.set_text(cx, "");
                        self.closed_heap = self.card.isolate_heap_key(cx).is_none();
                        self.heap = None;
                        self.write_state();
                    }
                    _ => panic!("Unexpected fixture host request"),
                }
            }
        }
    }
}
