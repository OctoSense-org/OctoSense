//! Native-engine acceptance host. Its local control file exists only in this
//! explicit test binary; no script app or production shell exposes this route.
use makepad_widgets::*;
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};

#[cfg(test)]
mod policy_tests;

#[cfg(target_os = "windows")]
mod windows_message_probe;

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root { main_window := Window {
            window.inner_size: vec2(800, 620)
            body +: { flow: Down padding: 16 spacing: 12
                Label {text: "OctoSense embedded browser acceptance"}
                browser_surface := View {width: Fill height: Fill flow: Down
                    browser := WebReader {width: Fill height: Fill}
                }
            }
        }}
    }
}

#[derive(Script, ScriptHook)]
struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    root: PathBuf,
    #[rust]
    timer: Option<Timer>,
    #[rust]
    last_command: u64,
    #[rust]
    frozen_surface: Option<makepad_widgets::view::ViewTextureSnapshot>,
    #[cfg(target_os = "windows")]
    #[rust]
    message_probe: Option<windows_message_probe::WindowsMessageProbe>,
}

impl App {
    fn browser_id(&self, cx: &mut Cx) -> SystemBrowserId {
        SystemBrowserId(LiveId(self.ui.widget(cx, ids!(browser)).widget_uid().0))
    }

    fn record(&self, event: Value) {
        if self.root.as_os_str().is_empty() {
            return;
        }
        if let Ok(mut file) = OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.root.join("events.jsonl"))
        {
            let _ = writeln!(file, "{event}");
        }
    }

    fn commands(&mut self, cx: &mut Cx) {
        let Some(command) = fs::File::open(self.root.join("command.json"))
            .ok()
            .and_then(|file| {
                let mut bytes = Vec::new();
                file.take(65_537).read_to_end(&mut bytes).ok()?;
                (bytes.len() <= 65_536)
                    .then(|| serde_json::from_slice::<Value>(&bytes).ok())
                    .flatten()
            })
        else {
            return;
        };
        let Some(id) = command["id"].as_u64().filter(|id| *id > self.last_command) else {
            return;
        };
        self.last_command = id;
        let operation = command["op"].as_str().unwrap_or("");
        let mut accepted = true;
        let widget = self.ui.widget(cx, ids!(browser));
        let browser_id = self.browser_id(cx);
        match operation {
            "freeze" => {
                let surface = self.ui.view(cx, ids!(browser_surface));
                self.frozen_surface = surface
                    .borrow_mut()
                    .and_then(|mut view| view.take_texture_snapshot(cx));
                accepted = self.frozen_surface.is_some();
                if accepted {
                    surface.set_visible(cx, false);
                }
            }
            "restore" => {
                self.frozen_surface = None;
                self.ui
                    .view(cx, ids!(browser_surface))
                    .set_visible(cx, true);
            }
            "resize" => {
                if let Some((width, height)) = command["width"]
                    .as_f64()
                    .zip(command["height"].as_f64())
                    .filter(|(width, height)| {
                        (160.0..=4096.0).contains(width) && (160.0..=4096.0).contains(height)
                    })
                {
                    self.ui
                        .window(cx, ids!(main_window))
                        .resize(cx, dvec2(width, height));
                } else {
                    accepted = false;
                }
            }
            #[cfg(target_os = "windows")]
            "calibrate_messages" => {
                if self.message_probe.is_some() {
                    accepted = false;
                } else {
                    match windows_message_probe::WindowsMessageProbe::start() {
                        Ok(probe) => self.message_probe = Some(probe),
                        Err(error) => {
                            self.record(
                                json!({"kind":"message_control","passed":false,"error":error}),
                            );
                            accepted = false;
                        }
                    }
                }
            }
            "open" => {
                if let Some(url) = command["url"].as_str().filter(|url| url.len() <= 16_384) {
                    if let Some(mut reader) =
                        widget.borrow_mut::<makepad_widgets::web_reader::WebReader>()
                    {
                        reader.close(cx);
                        accepted = reader.open(cx, url);
                    } else {
                        accepted = false;
                    }
                    widget.set_visible(cx, true);
                } else {
                    accepted = false;
                }
            }
            "eval" => {
                if let Some(script) = command["script"]
                    .as_str()
                    .filter(|script| script.len() <= 32_768)
                {
                    cx.system_browser(browser_id).eval_js(script);
                } else {
                    accepted = false;
                }
            }
            "hide" => widget.set_visible(cx, false),
            "show" => widget.set_visible(cx, true),
            "close" => {
                if let Some(mut reader) =
                    widget.borrow_mut::<makepad_widgets::web_reader::WebReader>()
                {
                    reader.close(cx);
                }
            }
            "inspect" => {
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                cx.system_browser(browser_id).inspect(
                    self.root
                        .join(format!("inspect-{id}.json"))
                        .to_string_lossy()
                        .into_owned(),
                    Some(
                        self.root
                            .join(format!("snapshot-{id}.png"))
                            .to_string_lossy()
                            .into_owned(),
                    ),
                    None,
                );
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                {
                    accepted = false;
                }
            }
            "quit" => {
                cx.system_browser(browser_id).close();
                cx.quit();
            }
            _ => accepted = false,
        }
        self.record(json!({"kind":"command", "id":id, "operation":operation,"accepted":accepted}));
        self.ui.redraw(cx);
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::theme_mod(vm);
        script_eval!(vm, {mod.theme = mod.themes.light});
        makepad_widgets::widgets_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if matches!(event, Event::Startup) {
            self.root = std::env::args()
                .find_map(|arg| arg.strip_prefix("--control-root=").map(PathBuf::from))
                .filter(|path| path.is_absolute() && path.is_dir())
                .expect("Pass an existing, isolated absolute --control-root directory");
            if std::env::args().any(|arg| arg == "--texture-surface") {
                self.ui
                    .view(cx, ids!(browser_surface))
                    .borrow_mut()
                    .unwrap()
                    .set_optimize(cx, makepad_widgets::view::ViewOptimize::Texture);
            }
            self.timer = Some(cx.start_interval(0.05));
            self.record(json!({"kind":"started", "platform":std::env::consts::OS}));
        }
        if self
            .timer
            .is_some_and(|timer| timer.is_event(event).is_some())
        {
            self.commands(cx);
            #[cfg(target_os = "windows")]
            if let Some(outcome) = self.message_probe.as_mut().and_then(|probe| probe.poll()) {
                self.record(json!({"kind":"message_control","passed":outcome.passed,
                    "delivered_messages":outcome.delivered_messages,
                    "web_message_enabled":outcome.web_message_enabled,
                    "host_objects_allowed":outcome.host_objects_allowed,
                    "cleanup_complete":outcome.cleanup_complete,"error":outcome.error}));
                self.message_probe = None;
            }
        }
        if let Event::Actions(actions) = event {
            let browser_id = self.browser_id(cx);
            for action in actions {
                if let Some(nav) =
                    action.downcast_ref::<makepad_platform::event::NativeSystemBrowserNavigation>()
                {
                    if nav.browser_id == browser_id.0 .0 {
                        self.record(json!({"kind":"navigation", "loading":nav.loading,"title":nav.title,"url":nav.url}));
                    }
                }
                if let Some(blocked) = action
                    .downcast_ref::<makepad_platform::event::NativeSystemBrowserPolicyBlocked>(
                ) {
                    if blocked.browser_id == browser_id.0 .0 {
                        self.record(
                            json!({"kind":"policy_blocked","description":blocked.description}),
                        );
                    }
                }
                if let Some(error) =
                    action.downcast_ref::<makepad_platform::event::NativeSystemBrowserPageError>()
                {
                    if error.browser_id == browser_id.0 .0 {
                        self.record(json!({"kind":"page_error", "code":error.code,"description":error.description}));
                    }
                }
            }
        }
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}
