//! A script app that needs a model sends the person to AI Providers at their
//! click (`settings.open`), as a page may follow a link:
//!
//! ```text
//! host.request("settings.open", {pane: "ai-providers"}, fn(r) { … })
//! ```
//!
//! Or, refused the location, to the system's own place for it (`{pane: "location"}`).
//!
//! It takes the same activation as `clipboard.write` (app_clipboard.rs): a press
//! on the app's own content in its window in the last few seconds, and never an
//! agent's tool call or a callback of one. It opens those places: no app can
//! launch another by name.
//!
//! System Settings (`os.settings`, apps/settings/bundle) is the shell's own
//! settings in a window. It is the one app with these methods, and they never
//! answer an agent's call or a callback of one:
//!
//! | Method | Args | Answers |
//! | --- | --- | --- |
//! | `settings.state` | – | what its panes show ([`State`]), and the pane the shell asked for |
//! | `settings.set` | `{key, value}` | `{}`, at the person's click: `appearance`, `style`, `theme`, `picture` |
//! | `settings.open` | `{pane}` | also `app-hub`, `software-update`, `approvals`, `assistant` |
//! | `settings.commands` | `{on}` | on: `{on}` once the host's sheet closes; off: `{on: false}`, at the person's click |
//! | `settings.restart_assistant` | – | `{}`, at the person's click |
//!
//! Turning command execution on takes what the person types on the host's own
//! sheet (`settings.sheet.allow {typed}`, `settings.sheet.cancel`), which no app
//! can call (App Hub's services refuse any `*.sheet.*` from an app) or draw. The
//! shell's handler of the Settings row checks the words and makes the gesture
//! (system_chat/grants.rs), as it does for a row typed in the shell's menu.

use serde_json::{json, Value};

/// The host-service family: `settings.open`.
pub const FAMILY: &str = "settings";
/// The app that holds the one pane there is.
pub const AI_PROVIDERS: &str = "ai-providers";
/// The system's own place for location access: what an app that was refused can
/// point a person to. It grants nothing.
pub const LOCATION: &str = "location";
/// The one app that reads and changes the shell's settings.
pub const SYSTEM_SETTINGS: &str = "os.settings";
/// Its name among the shell's apps.
pub const SETTINGS_APP: &str = "settings";
/// Requests waiting for the shell's next turn, from every app together.
pub const MAX_QUEUED: usize = 16;

pub const NOT_A_WINDOW: &str = "not_a_window: only the app's own window can open settings";
pub const AGENT_CALL: &str =
    "agent_call: not from an agent's tool call, or from a callback of one: open settings from the person's tap or click";
pub const GESTURE_REQUIRED: &str = "gesture_required: open settings from the person's tap or click in the app's window";
pub const BUSY: &str = "busy: too many requests to open settings are waiting";

/// What System Settings may open besides AI Providers.
const SETTINGS_PANES: [&str; 4] = ["app-hub", "software-update", "approvals", "assistant"];
/// What `settings.set` may change.
const SET_KEYS: [&str; 4] = ["appearance", "style", "theme", "picture"];

/// A checked `settings.*` request.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// Open a place in the shell, at the person's click.
    Open(String),
    /// What System Settings' panes show.
    State,
    /// Change one thing, at the person's click.
    Set { key: String, value: String },
    /// Raise the host's sheet, which alone can turn command execution on.
    CommandsSheet,
    /// Turn command execution off, at the person's click.
    CommandsOff,
    /// Restart the assistant so a changed switch applies, at the person's click.
    RestartAssistant,
    /// The sheet's Allow: what the person typed.
    Allow(String),
    /// The sheet's Cancel.
    SheetCancel,
}

/// A `settings.*` request, checked before it reaches the shell's turn.
pub fn admit(app: &str, service: &str, from_sheet: bool, args: &Value) -> Result<Ask, String> {
    let settings = app == SYSTEM_SETTINGS;
    match service {
        "settings.sheet.allow" | "settings.sheet.cancel" if !from_sheet || !settings => Err(NOT_A_WINDOW.into()),
        "settings.sheet.cancel" => Ok(Ask::SheetCancel),
        "settings.sheet.allow" => match args.get("typed").and_then(Value::as_str) {
            Some(typed) if typed.len() <= 256 => Ok(Ask::Allow(typed.to_string())),
            _ => Err("invalid_arguments: expected {typed}".into()),
        },
        _ if from_sheet => Err(NOT_A_WINDOW.into()),
        "settings.open" => {
            let pane = args.get("pane").and_then(Value::as_str).filter(|_| args.as_object().is_some_and(|object| object.len() == 1));
            match pane {
                Some(AI_PROVIDERS) => Ok(Ask::Open(AI_PROVIDERS.into())),
                Some(LOCATION) => Ok(Ask::Open(LOCATION.into())),
                Some(pane) if settings && SETTINGS_PANES.contains(&pane) => Ok(Ask::Open(pane.into())),
                _ => Err(format!("invalid_arguments: expected {{pane: {AI_PROVIDERS:?}}}")),
            }
        }
        "settings.state" if settings => Ok(Ask::State),
        "settings.set" if settings => {
            let text = |name: &str| args.get(name).and_then(Value::as_str).filter(|text| text.len() <= 64);
            match (text("key"), text("value")) {
                (Some(key), Some(value)) if SET_KEYS.contains(&key) => Ok(Ask::Set { key: key.into(), value: value.into() }),
                _ => Err(format!("invalid_arguments: expected {{key, value}}, a key of {SET_KEYS:?}")),
            }
        }
        "settings.commands" if settings => match args.get("on").and_then(Value::as_bool) {
            Some(true) => Ok(Ask::CommandsSheet),
            Some(false) => Ok(Ask::CommandsOff),
            None => Err("invalid_arguments: expected {on}".into()),
        },
        "settings.restart_assistant" if settings => Ok(Ask::RestartAssistant),
        _ => Err(format!("method_unavailable: the settings service has open {{pane}} only, not {service:?}")),
    }
}

/// What the shell's turn knows about the request. A phone has one app in front,
/// so there only the last two hold.
pub fn decide(facts: crate::app_clipboard::Facts) -> Result<Value, String> {
    if facts.agent_call {
        return Err(AGENT_CALL.into());
    }
    if !facts.mobile && !facts.pressed {
        return Err(GESTURE_REQUIRED.into());
    }
    Ok(json!({"opened": true}))
}

/// What this host runs on, as a Mac's About names it: the macOS version, and the chip
/// (an Intel Mac's processor), read from the system. Empty where it can't be read.
fn runs_on() -> Value {
    #[cfg(target_os = "macos")]
    {
        let cpu = sysctl_text("machdep.cpu.brand_string").unwrap_or_default();
        let apple = cpu.starts_with("Apple");
        json!({
            "macos": sysctl_text("kern.osproductversion").unwrap_or_default(),
            "chip": if apple { cpu.as_str() } else { "" },
            "processor": if apple { "" } else { cpu.as_str() },
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        json!({"macos": "", "chip": "", "processor": ""})
    }
}

/// A sysctl's text, without its trailing NUL; `None` when the system has none.
#[cfg(target_os = "macos")]
fn sysctl_text(name: &str) -> Option<String> {
    let name = std::ffi::CString::new(name).ok()?;
    let mut len = 0usize;
    // SAFETY: the first call only asks for the size; the second writes at most `len`
    // bytes into a buffer of that size and sets `len` to what it wrote.
    let mut buf = unsafe {
        if libc::sysctlbyname(name.as_ptr(), std::ptr::null_mut(), &mut len, std::ptr::null_mut(), 0) != 0 || len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len];
        if libc::sysctlbyname(name.as_ptr(), buf.as_mut_ptr().cast(), &mut len, std::ptr::null_mut(), 0) != 0 {
            return None;
        }
        buf.truncate(len);
        buf
    };
    while buf.last() == Some(&0) {
        buf.pop();
    }
    String::from_utf8(buf).ok().map(|text| text.trim().to_string()).filter(|text| !text.is_empty())
}

/// The sheet that turns command execution on. Its words are the host's and what
/// is typed in it reaches only `settings.sheet.allow`.
fn commands_sheet() -> String {
    use crate::system_chat::grants;
    const SHEET: &str = r##"let theme = mod.theme
let desk = try { theme.ui_pointer == true } catch { false }
let mac = desk && try { theme.ui_unified == true } catch { false }
let dark = dot(theme.color_text, #x36b61200) >= 0.5
fn tone(light, night){ if dark { night } else { light } }
let ink = tone(#x1d1d1f, #xf5f5f7)
let soft = tone(#x3a3a3c, #xd1d1d6)
let line = tone(#xc9c9ce, #x545458)
let face = tone(#xffffff, #x5a5a5e)
let danger = tone(#xd70015, #xff7b72)
let Words = Label{clip_y: false padding: 0 width: Fill height: Fit flow: Flow.Right{wrap: true} text: "" draw_text.color: soft draw_text.text_style: theme.font_regular{font_size: if desk { 9 } else { 13 } line_spacing: 1.3}}
let Push = ButtonFlat{height: if desk { 28 } else { 44 } margin: 0 padding: Inset{left: 14 right: 14}
    draw_bg +: {border_radius: 3.0 border_size: 1.0 border_color: line color: face color_hover: face color_down: face color_focus: face}
    draw_text +: {color: ink color_hover: ink color_down: ink color_focus: ink text_style: theme.font_regular{font_size: if desk { 9.75 } else { 14 }}}}
let Go = Push{draw_bg +: {border_size: 0.0 color: #x0071e3 color_hover: #x006edb color_down: #x0060c0 color_focus: #x0071e3}
    draw_text +: {color: #xffffff color_hover: #xffffff color_down: #xffffff color_focus: #xffffff}}
// A Mac greys a disabled push button's title as the apps' own disabled buttons do (AI Providers' `DeskOff`):
// the ink faded into the page. Elsewhere it keeps the sheet's soft ink.
let off_ink = if mac { mix(theme.color_text, theme.color_bg_app, 0.26) } else { soft }
let Off = Push{enabled: false draw_text +: {color: off_ink color_hover: off_ink color_down: off_ink color_focus: off_ink}}
fn cancel(){ host.request("settings.sheet.cancel", {}, nil) }
// The words as the host reads them: case and spacing aside.
let words = regex(@WORDS@, "i")
fn said(text){ text.match_str(words) != nil }
// A Mac's Allow, the sheet's default button, stays grey until the words are the
// ones shown, and Return does nothing before then.
fn typed(text){
    let some = if mac { said(text) } else { text.trim() != "" }
    ui.allow_on.set_visible(some)
    ui.allow_off.set_visible(!some)
}
fn allow(){
    if ui.phrase.text().trim() == "" { return }
    if mac && !said(ui.phrase.text()) { return }
    host.request("settings.sheet.allow", {typed: ui.phrase.text()}, fn(r){
        if !r.is_ok || r.data.accepted != true {
            ui.wrong.set_visible(true)
            ui.phrase.focus()
        }
    })
}
start_timeout(0.05, || ui.phrase.focus())
SolidView{width: Fill height: Fill flow: Down align: Align{x: 0.5} padding: Inset{left: 12 right: 12 top: 53} new_batch: true draw_bg.color: if desk { tone(#x00000024, #x00000052) } else { tone(#x00000026, #x00000026) }
    RoundedView{width: Fill max_width: 420 height: Fit flow: Down spacing: 8 padding: 18 show_bg: true draw_bg +: {color: tone(#xf6f6f8, #x2c2c2e) border_radius: 5.0 border_size: 1.0 border_color: tone(#xe3e3e8, #x48484a)}
        Label{clip_y: false padding: 0 width: Fill text: "Let the Assistant Run Commands?" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: if desk { 9.75 } else { 15 }}}
        Words{text: @RISK@}
        Words{margin: Inset{top: 4} text: @ASK@}
        phrase := TextInput{width: Fill height: if desk { 28 } else { 44 } empty_text: "" on_change: |text| typed(text) on_return: |text| allow()
            draw_bg +: {color: tone(#xffffff, #x3a3a3c) color_hover: tone(#xffffff, #x3a3a3c) color_focus: tone(#xffffff, #x3a3a3c) color_empty: tone(#xffffff, #x3a3a3c)
                border_color: line border_color_hover: line border_color_focus: #x0071e3 border_color_empty: line border_radius: 3.0}
            draw_text +: {color: ink color_hover: ink color_focus: ink color_empty: soft color_empty_hover: soft color_empty_focus: soft text_style: theme.font_regular{font_size: if desk { 9.75 } else { 14 }}}}
        wrong := Words{visible: false text: "Those aren’t the words. Type them as they’re shown." draw_text.color: danger}
        View{width: Fill height: Fit flow: Right spacing: 8 align: Align{x: 1.0 y: 0.5} margin: Inset{top: 6}
            Push{text: "Cancel" on_click: || cancel()}
            allow_off := Off{text: "Allow"}
            allow_on := Go{visible: false text: "Allow" on_click: || allow()}
        }
    }
}
"##;
    let quoted = |text: &str| serde_json::to_string(text).unwrap_or_default();
    let ask = format!("To allow it, type \u{201c}{}\u{201d}.", grants::CONFIRM_PHRASE);
    // The phrase's words, any spacing between and round them (`grants::is_confirm_phrase`).
    let words = format!("^\\s*{}\\s*$", grants::CONFIRM_PHRASE.split_whitespace().collect::<Vec<_>>().join("\\s+"));
    SHEET.replace("@RISK@", &quoted(grants::RISK)).replace("@ASK@", &quoted(&ask)).replace("@WORDS@", &quoted(&words))
}

#[cfg(any(feature = "app-hub", native_mobile))]
pub use service::{register, take_requests};

#[cfg(any(feature = "app-hub", native_mobile))]
mod service {
    use super::*;
    use makepad_widgets::makepad_platform::SignalToUI;
    use octosense_appstore::services::{self, HostService, Replier, ServiceCall, ServiceHost};
    use std::sync::Mutex;

    /// A request for the shell's next turn (`App::serve_settings_requests`).
    pub struct Request {
        pub app: String,
        pub isolate: usize,
        /// `!ServiceCall::may_prompt`: an agent's tool call, or a callback of one.
        pub agent_call: bool,
        pub ask: Ask,
        pub reply: Replier,
    }

    static QUEUE: Mutex<Vec<Request>> = Mutex::new(Vec::new());

    /// Offer `settings.open` to every app, as `clipboard.write` is.
    pub fn register() {
        services::register_host_service(Box::new(SettingsService { waiting: None }));
    }

    pub fn take_requests() -> Vec<Request> {
        std::mem::take(&mut *QUEUE.lock().unwrap_or_else(|e| e.into_inner()))
    }

    struct SettingsService {
        /// System Settings' `settings.commands`, while the sheet it raised is up.
        waiting: Option<Replier>,
    }

    impl SettingsService {
        fn answer_waiting(&mut self, on: bool) {
            if let Some(waiting) = self.waiting.take() {
                waiting.send(Ok(json!({"on": on})));
            }
        }
    }

    impl HostService for SettingsService {
        fn family(&self) -> &'static str {
            FAMILY
        }

        fn call(&mut self, call: ServiceCall, reply: Replier, host: &mut dyn ServiceHost) {
            let ask = match admit(&call.app_id, &call.service, call.from_sheet, &call.args) {
                Ok(ask) => ask,
                Err(refusal) => {
                    reply.send(Err(refusal));
                    return;
                }
            };
            match &ask {
                Ask::CommandsSheet => {
                    if !call.may_prompt {
                        reply.send(Err(AGENT_CALL.into()));
                        return;
                    }
                    self.answer_waiting(false);
                    host.open_sheet(commands_sheet());
                    self.waiting = Some(reply);
                    return;
                }
                Ask::SheetCancel => {
                    self.answer_waiting(false);
                    host.close_sheet();
                    reply.send(Ok(json!({"cancelled": true})));
                    return;
                }
                Ask::Allow(typed) => {
                    if !crate::system_chat::grants::is_confirm_phrase(typed) {
                        reply.send(Ok(json!({"accepted": false})));
                        return;
                    }
                    self.answer_waiting(true);
                    host.close_sheet();
                }
                _ => {}
            }
            let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
            if queue.len() >= MAX_QUEUED {
                drop(queue);
                reply.send(Err(BUSY.into()));
                return;
            }
            queue.push(Request { app: call.app_id, isolate: reply.isolate_key(), agent_call: !call.may_prompt, ask, reply });
            drop(queue);
            // The shell answers at the end of this event, or the next.
            SignalToUI::set_ui_signal();
        }
    }
}

#[cfg(any(feature = "app-hub", native_mobile))]
use crate::*;

#[cfg(any(feature = "app-hub", native_mobile))]
impl App {
    /// The `settings.*` requests waiting, answered: after the event that
    /// pumped them, on the UI thread.
    pub(crate) fn serve_settings_requests(&mut self, cx: &mut Cx) {
        for request in take_requests() {
            // The accepted sheet has closed by now, and what it accepted still happens.
            if !request.reply.is_pending() && !matches!(request.ask, Ask::Allow(_)) {
                continue;
            }
            let answer = match request.ask {
                // It came from the host's sheet, and the words were the right ones.
                Ask::Allow(typed) => {
                    self.assistant_commands_activate(cx, &format!("{}:{typed}", shell::menu::COMMANDS_ALLOW_ROW), true);
                    Ok(json!({"accepted": true}))
                }
                Ask::State if request.agent_call => Err(AGENT_CALL.to_string()),
                Ask::State => Ok(self.settings_state()),
                ask => {
                    let mobile = self.state.as_ref().is_none_or(|s| s.style.target.mobile());
                    let window = if mobile { None } else { self.window_of(cx, &request.app, request.isolate) };
                    let pressed = window.is_some_and(|client| self.presenter.pressed(client, host::now()));
                    decide(crate::app_clipboard::Facts { mobile, agent_call: request.agent_call, pressed }).and_then(|opened| self.settings_act(cx, ask, opened))
                }
            };
            request.reply.send(answer);
            self.redraw_all(cx);
        }
    }

    /// What a request the person's press allowed does.
    fn settings_act(&mut self, cx: &mut Cx, ask: Ask, opened: Value) -> Result<Value, String> {
        match ask {
            Ask::Open(pane) => {
                match pane.as_str() {
                    "app-hub" => self.launch_app(cx, "apphub"),
                    "software-update" => self.launch_app(cx, "updater"),
                    "approvals" => self.shell_menu_activate(cx, shell::menu::APPROVALS_ROW),
                    "assistant" => self.shell_menu_activate(cx, shell::menu::SYSTEM_CHAT_ROW),
                    LOCATION => crate::mac_menus::open_host_settings(LOCATION),
                    _ => self.launch_app(cx, AI_PROVIDERS),
                }
                Ok(opened)
            }
            Ask::Set { key, value } => {
                let target = self.settings_target(&key, &value)?;
                self.shell_menu_activate(cx, &target);
                self.update_bar(cx);
                Ok(json!({}))
            }
            Ask::CommandsOff => {
                self.assistant_commands_activate(cx, shell::menu::COMMANDS_OFF_ROW, true);
                Ok(json!({"on": false}))
            }
            Ask::RestartAssistant => {
                self.assistant_commands_activate(cx, shell::menu::ASSISTANT_RESTART_ROW, true);
                Ok(json!({}))
            }
            _ => Err("method_unavailable: not an action".into()),
        }
    }

    /// The row of the shell's own menus that does what `settings.set` asks.
    #[cfg(not(mobile_only))]
    fn settings_target(&mut self, key: &str, value: &str) -> Result<String, String> {
        let invalid = || format!("invalid_arguments: no {key} {value:?}");
        let style = self.state_mut().style.target;
        match key {
            "appearance" if !["auto", "light", "dark"].contains(&value) => Err(invalid()),
            "appearance" if style == desktop::DesktopStyle::Macos => Ok(format!("mac.appearance.{value}")),
            "appearance" if value == "auto" || !style.supports_dark() => Err(invalid()),
            "appearance" => Ok(format!("desktop.{}{}", style.id(), if value == "dark" { "-dark" } else { "" })),
            "style" if desktop::DesktopStyle::parse(value).is_some_and(|choice| choice.id() == value) => {
                // Another style keeps the appearance the person chose, where it has a dark one; a
                // Mac that follows the computer's setting goes on following it.
                let dark = self.state_mut().style.dark
                    && match desktop::DesktopStyle::parse(value) {
                        Some(desktop::DesktopStyle::Macos) => self.mac_appearance != crate::mac_appearance::Choice::Auto,
                        Some(next) => next.supports_dark(),
                        None => false,
                    };
                Ok(format!("desktop.{value}{}", if dark { "-dark" } else { "" }))
            }
            "theme" if theme::installed_themes().iter().any(|name| name == value) => Ok(format!("style.theme.{value}")),
            "picture" if style == desktop::DesktopStyle::Macos => crate::mac_appearance::Picture::parse(value)
                .map(|picture| format!("mac.picture.{}", picture.id()))
                .ok_or_else(invalid),
            "picture" => Ok("style.background".to_string()),
            _ => Err(invalid()),
        }
    }

    #[cfg(mobile_only)]
    fn settings_target(&mut self, key: &str, value: &str) -> Result<String, String> {
        Err(format!("method_unavailable: no {key} {value:?} on a phone"))
    }

    /// System Settings at a pane, or where it was; a build without it keeps the
    /// shell's own list.
    pub(crate) fn open_settings_pane(&mut self, cx: &mut Cx, pane: &str) {
        if crate::clients::find_app(SETTINGS_APP).is_none() {
            let path = if pane == "keyboard" { "learn.keybindings" } else { "setup" };
            self.open_shell_menu(cx, path, MenuSkin::Menu);
            return;
        }
        self.settings_pane = (!pane.is_empty()).then(|| pane.to_string());
        self.launch_app(cx, SETTINGS_APP);
        // A window already open goes there now; one still opening asks.
        let focused = self.state_mut().layout.focused_client();
        let front = focused.filter(|client| self.state_mut().clients.get(client).is_some_and(|slot| slot.app == SETTINGS_APP));
        if let (Some(client), Some(pane)) = (front, self.settings_pane.clone()) {
            let command = format!("pane:{pane}");
            let called = self.module_host.dispatch(cx, client, "its pane", |cx, root| root.splash(cx, ids!(card)).call_script_fn_with_strings(cx, id!(on_menu_command), &[command.as_str()]));
            if called == Some(true) {
                self.settings_pane = None;
                cx.with_vm_and_async(|_| {});
            }
        }
    }

    /// A Mac's Approvals page is one of System Settings' pages, a sheet on its
    /// window: just opened from anywhere else (the system menu, a chat's Open
    /// Settings), Settings comes forward at its Assistant pane under it.
    pub(crate) fn approvals_page_opened(&mut self, cx: &mut Cx) {
        let mac = self.state.as_ref().is_some_and(|s| s.style.target == crate::desktop::DesktopStyle::Macos);
        if !crate::approvals::take_settings_fresh() || !mac {
            return;
        }
        let focused = self.state_mut().layout.focused_client();
        let front = focused.is_some_and(|client| self.state_mut().clients.get(&client).is_some_and(|slot| slot.app == SETTINGS_APP));
        if !front && crate::clients::find_app(SETTINGS_APP).is_some() {
            self.open_settings_pane(cx, "assistant");
        }
    }

    /// What System Settings' panes show.
    fn settings_state(&mut self) -> Value {
        use crate::system_chat::grants;
        let pane = self.settings_pane.take();
        let available = |id: &str| crate::clients::find_app(id).is_some();
        // A Terminal whose launch ran unsandboxed counts as none; one not
        // launched yet may still be (its first launch decides).
        let terminal = crate::apps::terminal_runs_as_process() && crate::sandbox::launch_state(crate::apps::TERMINAL) != Some(false);
        let on = grants::command_execution();
        let note = match (on, terminal) {
            (_, false) => format!("{}, because it {}.", if on { "On, but it can’t run" } else { "Off" }, grants::NEEDS_PROCESS_TERMINAL),
            (true, true) => "On. Each command asks you first.".to_string(),
            (false, true) => "Off. The assistant runs no commands.".to_string(),
        };
        let applied = match grants::applied() {
            grants::Applied::NeedsRestart => "restart",
            grants::Applied::NextStart => "next-start",
            grants::Applied::Yes => "yes",
        };
        let keys: Vec<Value> = shell::menu::shortcut_rows().into_iter().map(|(title, keys)| json!({"title": title, "keys": keys})).collect();
        let mut state = json!({
            "pane": pane.unwrap_or_default(),
            "version": env!("CARGO_PKG_VERSION"),
            "runs_on": runs_on(),
            "has": {"update": available("updater"), "hub": available("apphub"), "ai": available(AI_PROVIDERS)},
            "commands": {"on": on, "can": terminal, "note": note, "risk": grants::RISK, "applied": applied},
            "keys": keys,
            "look": "",
            "looks": [],
            "style": "",
            "styles": [],
            "themed": false,
            "theme": "",
            "themes": [],
            "picture": "",
            "pictures": [],
        });
        #[cfg(not(mobile_only))]
        {
            use desktop::DesktopStyle;
            let style = self.state_mut().style.target;
            let dark = self.state_mut().style.dark;
            let (looks, look): (&[&str], &str) = if style == DesktopStyle::Macos {
                (&["auto", "light", "dark"], self.mac_appearance.word())
            } else if style.supports_dark() {
                (&["light", "dark"], if dark { "dark" } else { "light" })
            } else {
                (&[], "")
            };
            state["looks"] = json!(looks);
            state["look"] = json!(look);
            state["style"] = json!(style.id());
            state["styles"] = DesktopStyle::ALL.iter().filter(|choice| !choice.mobile()).map(|choice| json!({"id": choice.id(), "label": choice.label()})).collect();
            // Only Omarchy's desktop draws a theme's colours and its pictures.
            state["themed"] = json!(style == DesktopStyle::Omarchy);
            state["theme"] = json!(self.state_mut().theme_name.clone());
            let title = |name: &str| {
                name.split('-')
                    .map(|word| {
                        let mut chars = word.chars();
                        chars.next().map_or(String::new(), |first| first.to_uppercase().chain(chars).collect())
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            state["themes"] = theme::installed_themes().into_iter().map(|name| json!({"label": title(&name), "id": name})).collect();
            // A Mac's desktop picture; Omarchy's are its theme's.
            if style == DesktopStyle::Macos {
                use crate::mac_appearance::Picture;
                state["picture"] = json!(self.mac_picture.id());
                state["pictures"] = Picture::ALL.iter().map(|picture| json!({"id": picture.id(), "label": picture.label()})).collect();
            }
        }
        state
    }
}

/// A shell without App Hub runs no script apps, so its own list stands in.
#[cfg(not(any(feature = "app-hub", native_mobile)))]
impl crate::App {
    pub(crate) fn open_settings_pane(&mut self, cx: &mut makepad_widgets::Cx, pane: &str) {
        let path = if pane == "keyboard" { "learn.keybindings" } else { "setup" };
        self.open_shell_menu(cx, path, crate::shell::menu::MenuSkin::Menu);
    }

    /// No System Settings to hang the Approvals page from: it stands alone.
    pub(crate) fn approvals_page_opened(&mut self, _cx: &mut makepad_widgets::Cx) {
        crate::approvals::take_settings_fresh();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_clipboard::Facts;

    #[test]
    fn about_names_what_it_runs_on_from_the_system_only() {
        let facts = runs_on();
        let macos = facts["macos"].as_str().unwrap_or("x");
        if cfg!(target_os = "macos") {
            assert!(macos.chars().next().is_some_and(|c| c.is_ascii_digit()), "{facts}");
            assert!(facts["chip"].as_str().is_some_and(|chip| chip.is_empty() || chip.starts_with("Apple")), "{facts}");
        } else {
            assert_eq!(facts, json!({"macos": "", "chip": "", "processor": ""}));
        }
    }

    #[test]
    fn only_the_ai_providers_pane_from_the_apps_own_surface_is_admitted() {
        let pane = json!({"pane": "ai-providers"});
        assert_eq!(admit("os.news", "settings.open", false, &pane), Ok(Ask::Open(AI_PROVIDERS.into())));
        assert_eq!(admit("os.maps", "settings.open", false, &json!({"pane": "location"})), Ok(Ask::Open(LOCATION.into())));
        assert_eq!(admit("os.maps", "settings.open", true, &json!({"pane": "location"})), Err(NOT_A_WINDOW.to_string()));
        assert!(admit("os.news", "settings.read", false, &pane).unwrap_err().starts_with("method_unavailable:"));
        assert_eq!(admit("os.news", "settings.open", true, &pane), Err(NOT_A_WINDOW.to_string()));
        assert!(admit("os.news", "settings.open", false, &json!({})).unwrap_err().starts_with("invalid_arguments:"));
        assert!(admit("os.news", "settings.open", false, &json!({"pane": "mail"})).unwrap_err().starts_with("invalid_arguments:"));
        assert!(admit("os.news", "settings.open", false, &json!({"pane": "ai-providers", "then": 1})).unwrap_err().starts_with("invalid_arguments:"));
    }

    #[test]
    fn only_system_settings_reads_and_changes_the_shells_settings() {
        let set = json!({"key": "style", "value": "macos"});
        for (service, args) in [
            ("settings.state", json!({})),
            ("settings.set", set.clone()),
            ("settings.commands", json!({"on": true})),
            ("settings.commands", json!({"on": false})),
            ("settings.restart_assistant", json!({})),
        ] {
            assert!(admit("os.news", service, false, &args).unwrap_err().starts_with("method_unavailable:"), "{service}");
            assert!(admit(SYSTEM_SETTINGS, service, false, &args).is_ok(), "{service}");
            assert_eq!(admit(SYSTEM_SETTINGS, service, true, &args), Err(NOT_A_WINDOW.to_string()), "{service}");
        }
        assert_eq!(admit(SYSTEM_SETTINGS, "settings.commands", false, &json!({"on": true})), Ok(Ask::CommandsSheet));
        assert_eq!(admit(SYSTEM_SETTINGS, "settings.commands", false, &json!({"on": false})), Ok(Ask::CommandsOff));
        assert!(admit(SYSTEM_SETTINGS, "settings.set", false, &json!({"key": "home", "value": "x"})).unwrap_err().starts_with("invalid_arguments:"));
        assert!(admit("os.news", "settings.open", false, &json!({"pane": "approvals"})).unwrap_err().starts_with("invalid_arguments:"));
        assert_eq!(admit(SYSTEM_SETTINGS, "settings.open", false, &json!({"pane": "approvals"})), Ok(Ask::Open("approvals".into())));
    }

    #[test]
    fn what_turns_commands_on_is_typed_on_the_hosts_sheet() {
        let typed = json!({"typed": crate::system_chat::grants::CONFIRM_PHRASE});
        assert_eq!(admit(SYSTEM_SETTINGS, "settings.sheet.allow", false, &typed), Err(NOT_A_WINDOW.to_string()));
        assert_eq!(admit("os.news", "settings.sheet.allow", true, &typed), Err(NOT_A_WINDOW.to_string()));
        assert_eq!(admit(SYSTEM_SETTINGS, "settings.sheet.allow", true, &typed), Ok(Ask::Allow(crate::system_chat::grants::CONFIRM_PHRASE.into())));
        assert!(admit(SYSTEM_SETTINGS, "settings.sheet.allow", true, &json!({})).unwrap_err().starts_with("invalid_arguments:"));
        assert!(commands_sheet().contains(crate::system_chat::grants::CONFIRM_PHRASE));
        assert!(commands_sheet().contains(crate::system_chat::grants::RISK));
    }

    #[test]
    fn opening_takes_the_persons_press_and_never_an_agent() {
        let pressed = Facts { pressed: true, ..Facts::default() };
        assert!(decide(pressed).is_ok());
        assert_eq!(decide(Facts::default()), Err(GESTURE_REQUIRED.to_string()));
        assert_eq!(decide(Facts { agent_call: true, ..pressed }), Err(AGENT_CALL.to_string()));
        assert!(decide(Facts { mobile: true, ..Facts::default() }).is_ok());
        assert_eq!(decide(Facts { mobile: true, agent_call: true, ..Facts::default() }), Err(AGENT_CALL.to_string()));
    }
}
