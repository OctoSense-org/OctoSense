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

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root { main_window := Window {
            window.inner_size: vec2(800, 620)
            body +: { flow: Down padding: 16 spacing: 12
                Label {text: "OctoSense embedded browser acceptance"}
                browser := View {width: Fill height: Fill}
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
    open: bool,
    #[rust]
    visible: bool,
}

fn browser_id() -> SystemBrowserId {
    SystemBrowserId(live_id!(octosense_browser_smoke))
}

impl App {
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
        match operation {
            "open" => {
                if let Some(url) = command["url"].as_str().filter(|url| url.len() <= 16_384) {
                    if self.open {
                        cx.system_browser(browser_id()).close();
                    }
                    if command["navigable"].as_bool().unwrap_or(false) {
                        cx.system_browser(browser_id()).spawn_navigable(url);
                    } else {
                        cx.system_browser(browser_id()).spawn(url);
                    }
                    self.open = true;
                    self.visible = true;
                } else {
                    accepted = false;
                }
            }
            "eval" => {
                if let Some(script) = command["script"]
                    .as_str()
                    .filter(|script| script.len() <= 32_768)
                {
                    cx.system_browser(browser_id()).eval_js(script);
                } else {
                    accepted = false;
                }
            }
            "hide" => {
                self.visible = false;
                cx.system_browser(browser_id()).detach();
            }
            "show" => self.visible = true,
            "close" => {
                cx.system_browser(browser_id()).detach();
                cx.system_browser(browser_id()).close();
                self.open = false;
            }
            "inspect" => {
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                cx.system_browser(browser_id()).inspect(
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
                cx.system_browser(browser_id()).close();
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
            self.timer = Some(cx.start_interval(0.05));
            self.record(json!({"kind":"started", "platform":std::env::consts::OS}));
        }
        if self
            .timer
            .is_some_and(|timer| timer.is_event(event).is_some())
        {
            self.commands(cx);
        }
        if let Event::Actions(actions) = event {
            for action in actions {
                if let Some(nav) =
                    action.downcast_ref::<makepad_platform::event::NativeSystemBrowserNavigation>()
                {
                    if nav.browser_id == browser_id().0 .0 {
                        self.record(json!({"kind":"navigation", "loading":nav.loading,"title":nav.title,"url":nav.url}));
                    }
                }
                if let Some(error) =
                    action.downcast_ref::<makepad_platform::event::NativeSystemBrowserPageError>()
                {
                    if error.browser_id == browser_id().0 .0 {
                        self.record(json!({"kind":"page_error", "code":error.code,"description":error.description}));
                    }
                }
            }
        }
        self.ui.handle_event(cx, event, &mut Scope::empty());
        if self.open && matches!(event, Event::Draw(_)) {
            let area = self.ui.widget(cx, ids!(browser)).area();
            cx.system_browser(browser_id()).update(area, self.visible);
        }
    }
}
