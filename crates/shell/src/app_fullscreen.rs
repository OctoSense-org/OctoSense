//! Presentation full screen: an app's own window over the whole screen, at
//! the person's tap (`window.fullscreen`).
//!
//! A script app asks from the handler of a tap or click, and may say what it
//! does when full screen starts and ends:
//!
//! ```text
//! host.request("window.fullscreen", {on: true}, fn(r) {
//!     // r.is_ok: r.data is {applied, fullscreen, fills_screen}
//! })
//! fn on_app_fullscreen(on) { … }   // optional
//! ```
//!
//! On a desktop style the shell makes the app's window full screen, as ⌘F
//! and the green button do (`DesktopWindow::fullscreen`, tiled the
//! workspace's SUPER+F fullscreen; `WmLayout::set_fullscreen` names the
//! window): it covers the whole desk with no title bar, and the bar and the
//! dock hide. Unlike ⌘F's full screen, no edge brings them back while the
//! app presents (`WmState::presentation`: the top edge, the bottom edge and
//! every style's dock or taskbar stay away, as in a slideshow's play mode).
//! The shell's own window enters the host's full screen when it is not there
//! already ([`HostFullscreen`]), and the shell says "Press Esc to exit full
//! screen" for a few seconds (`OsdShow::fullscreen_hint`). Esc (the shell's
//! key: a script app gets no keys), the app's `{on: false}`, ⌘F, the window
//! closing or losing the focus, a style switch, or the person leaving the
//! host's full screen ends it and puts everything back as it was: the
//! window's rect and its place in the stack (never changed while it was full
//! screen), the bar and the dock, and the host window, which leaves full
//! screen only if this presentation took it there. The app hears both ends
//! through its optional `on_app_fullscreen(on)` hook, called in its own
//! isolate as `on_app_resize` is. On a phone every app already fills the
//! screen and the shell has no immersive mode: the request changes nothing
//! and says so.
//!
//! **Its own window only.** The shell finds the window whose Card runner
//! runs the isolate that asked, for the app that asked (`App::window_of`,
//! [`window_for`]), so an app reaches no other app's window, and a surface
//! that is not an app window (a glance card, a host sheet) is refused
//! `not_a_window`.
//!
//! **From a tap.** makepad's runtime cannot say whether a `host.request`
//! came from a person's tap. A request carries only `may_prompt`: whether
//! its surface is in the foreground (not an agent's tool call, a home tile
//! or a callback of one). Input provenance (`trusted_user_input`: macOS HID
//! pointers and Android touches only) holds only during the native event's
//! own handler, and a Splash `on_click` runs later, from the widget-to-script
//! queue, outside it. So the shell keeps a transient activation, as a browser
//! does: a press it routed into that window (`MpRunViewAction::Clicked`)
//! within [`ACTIVATION_SECONDS`] before the request, used up by a request
//! that applies ([`Activation`]). Any press counts, remote and synthetic ones
//! too: a script app cannot make input. Esc and the hint are the rest of the
//! protection.

use crate::layout::ClientId;
use makepad_widgets::{KeyCode, KeyModifiers};
use serde_json::{json, Value};
use std::collections::HashMap;

/// The host-service family: `window.fullscreen`.
pub const FAMILY: &str = "window";
/// How long after the person's press in a window that window may still go
/// full screen: a browser's transient activation lasts about as long.
pub const ACTIVATION_SECONDS: f64 = 5.0;
/// How long the host window gets to report entering the full screen asked of
/// it before the shell takes it that it did not.
pub const ENTER_GRACE_SECONDS: f64 = 3.0;
/// Requests waiting for the shell's next turn, from every app together.
pub const MAX_QUEUED: usize = 16;

pub const NOT_A_WINDOW: &str = "not_a_window: only the app's own window can go full screen";
pub const FOREGROUND_REQUIRED: &str = "foreground_required: the app's window must be in front and in use";
pub const GESTURE_REQUIRED: &str = "gesture_required: ask for full screen from the person's tap or click in the app's window";
pub const UNAVAILABLE: &str = "unavailable: this window cannot cover the desk in this layout";
pub const BUSY: &str = "busy: too many full-screen requests are waiting";

/// A `window.*` request, checked before it reaches the shell's turn: the
/// method, the surface and the arguments. The answer is `on`.
pub fn admit(service: &str, from_sheet: bool, may_prompt: bool, args: &Value) -> Result<bool, String> {
    if service != "window.fullscreen" {
        return Err(format!("method_unavailable: the window service has fullscreen {{on}} only, not {service:?}"));
    }
    if from_sheet {
        return Err(NOT_A_WINDOW.into());
    }
    if !may_prompt {
        return Err(FOREGROUND_REQUIRED.into());
    }
    parse_on(args)
}

/// `{on: true}` or `{on: false}`, nothing else.
pub fn parse_on(args: &Value) -> Result<bool, String> {
    let wrong = || "invalid_arguments: expected {on: true} or {on: false}".to_string();
    let object = args.as_object().ok_or_else(wrong)?;
    if object.len() != 1 {
        return Err(wrong());
    }
    object.get("on").and_then(Value::as_bool).ok_or_else(wrong)
}

/// The answer: whether this request changed anything, whether the window now
/// covers the desk in full screen, and whether it fills all the room the
/// shell gives apps (while it covers the desk; always on a phone, where every
/// app does). It does not say whether the host window reached the OS's full
/// screen: that follows a moment later, or not at all in a hidden run.
pub fn answer(applied: bool, fullscreen: bool, fills_screen: bool) -> Value {
    json!({"applied": applied, "fullscreen": fullscreen, "fills_screen": fills_screen})
}

/// A window, as far as finding the one a request is for goes: its client,
/// the script app it runs (`ClientSlot::foreground_app`) and the isolate of
/// its Card runner, when it has one.
pub struct WindowIdentity {
    pub client: ClientId,
    pub app: Option<String>,
    pub isolate: Option<usize>,
}

/// The window of the app `app` whose Card runner runs `isolate`: the one
/// window a request from that isolate may change. None for any other
/// surface: another app's window, a glance card, a sheet.
pub fn window_for(app: &str, isolate: usize, windows: &[WindowIdentity]) -> Option<ClientId> {
    windows
        .iter()
        .find(|w| w.app.as_deref() == Some(app) && w.isolate == Some(isolate))
        .map(|w| w.client)
}

/// What the shell knows when a request arrives.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    /// A phone style is up: apps fill the screen already.
    pub mobile: bool,
    /// The requesting app's own window ([`window_for`]).
    pub window: Option<ClientId>,
    /// The window presenting now, if any.
    pub presenting: Option<ClientId>,
    /// The window is full screen already (⌘F, the green button,
    /// `SetFullscreen`).
    pub covering: bool,
    /// The window has the focus, shown, on the workspace in view, and its
    /// isolate is the app's foreground surface.
    pub in_front: bool,
    /// The person pressed in it within [`ACTIVATION_SECONDS`].
    pub pressed: bool,
}

/// What the shell does with a request.
#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    /// Answer at once, changing nothing.
    Answer(Value),
    Refuse(&'static str),
    /// Start presenting this window.
    Begin(ClientId),
    /// End this window's presentation.
    End(ClientId),
}

/// The decision for a request to turn full screen `on` or off.
pub fn decide(on: bool, facts: &Facts) -> Decision {
    if facts.mobile {
        return Decision::Answer(answer(false, false, true));
    }
    let Some(client) = facts.window else {
        return Decision::Refuse(NOT_A_WINDOW);
    };
    if !on {
        return if facts.presenting == Some(client) {
            Decision::End(client)
        } else {
            Decision::Answer(answer(false, facts.covering, facts.covering))
        };
    }
    if facts.presenting == Some(client) {
        return Decision::Answer(answer(false, true, true));
    }
    if !facts.in_front {
        return Decision::Refuse(FOREGROUND_REQUIRED);
    }
    if !facts.pressed {
        return Decision::Refuse(GESTURE_REQUIRED);
    }
    Decision::Begin(client)
}

/// The presses the shell routed into windows, for the transient activation
/// a request needs (see the module notes).
#[derive(Debug, Default)]
pub struct Activation {
    presses: HashMap<ClientId, f64>,
}

impl Activation {
    /// The person pressed in `client`'s window at `now` (seconds). Presses
    /// too old to count go, so closed windows leave nothing behind.
    pub fn press(&mut self, client: ClientId, now: f64) {
        self.presses.retain(|_, at| now >= *at && now - *at <= ACTIVATION_SECONDS);
        self.presses.insert(client, now);
    }
    /// Whether `client` was pressed within [`ACTIVATION_SECONDS`] before `now`.
    pub fn fresh(&self, client: ClientId, now: f64) -> bool {
        self.presses.get(&client).is_some_and(|at| now >= *at && now - *at <= ACTIVATION_SECONDS)
    }
    /// A request that applied used the press up: the next needs another.
    pub fn spend(&mut self, client: ClientId) {
        self.presses.remove(&client);
    }
}

/// What the shell asks of its own window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostOp {
    Enter,
    Leave,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum HostState {
    /// Nothing of a presentation's: the host window is as the person left it.
    #[default]
    Idle,
    /// Asked to enter full screen at `since`; it has not said it did. `leave`:
    /// the presentation ended meanwhile, so it leaves as soon as it is in.
    Entering { since: f64, leave: bool },
    /// In full screen because a presentation asked.
    Entered,
    /// Already in full screen when the presentation began: it stays so.
    AlreadyFull,
    /// Not asked: the windows are hidden (`launch_input::hidden_windows`).
    Skipped,
}

/// The host window's full screen around a presentation: entered only when it
/// was not full screen already, left only if the presentation entered it.
/// makepad's macOS backend has one toggle for both directions
/// (`toggleFullScreen:`), so the shell asks it to leave only while it reports
/// full screen, and one still entering leaves once it says it is in.
#[derive(Debug, Default)]
pub struct HostFullscreen {
    state: HostState,
}

impl HostFullscreen {
    /// A presentation begins, the host window `fullscreen` or not at `now`;
    /// `may_ask`: the shell may ask it to enter (its windows are shown).
    pub fn begin(&mut self, fullscreen: bool, may_ask: bool, now: f64) -> Option<HostOp> {
        if let HostState::Entering { since, .. } = self.state {
            if now - since < ENTER_GRACE_SECONDS {
                // The enter asked for a moment ago is on its way: it serves this one.
                self.state = HostState::Entering { since, leave: false };
                return None;
            }
        }
        self.state = if fullscreen {
            HostState::AlreadyFull
        } else if !may_ask {
            HostState::Skipped
        } else {
            HostState::Entering { since: now, leave: false }
        };
        matches!(self.state, HostState::Entering { .. }).then_some(HostOp::Enter)
    }

    /// The host window reports whether it is full screen. The op to send,
    /// and whether the person took it out of a full screen a presentation
    /// relies on (which ends the presentation).
    pub fn reported(&mut self, fullscreen: bool) -> (Option<HostOp>, bool) {
        match (self.state, fullscreen) {
            (HostState::Entering { leave: true, .. }, true) => {
                self.state = HostState::Idle;
                (Some(HostOp::Leave), false)
            }
            (HostState::Entering { leave: false, .. }, true) => {
                self.state = HostState::Entered;
                (None, false)
            }
            (HostState::Entered | HostState::AlreadyFull, false) => {
                self.state = HostState::Idle;
                (None, true)
            }
            _ => (None, false),
        }
    }

    /// A presentation ends, the host window `fullscreen` or not at `now`;
    /// `toggles`: entering and leaving are one toggle (macOS).
    pub fn end(&mut self, fullscreen: bool, toggles: bool, now: f64) -> Option<HostOp> {
        let op = match self.state {
            HostState::Entered if fullscreen || !toggles => Some(HostOp::Leave),
            HostState::Entering { since, .. } if toggles && now - since < ENTER_GRACE_SECONDS => {
                self.state = HostState::Entering { since, leave: true };
                return None;
            }
            HostState::Entering { .. } if !toggles => Some(HostOp::Leave),
            _ => None,
        };
        self.state = HostState::Idle;
        op
    }

    /// Whether a presentation asked the host window to enter full screen and
    /// has not given it back yet.
    pub fn entered_for_presentation(&self) -> bool {
        matches!(self.state, HostState::Entering { leave: false, .. } | HostState::Entered)
    }
}

/// One app window presenting.
#[derive(Clone, Debug)]
pub struct Presentation {
    pub client: ClientId,
    pub app: String,
    pub isolate: usize,
    /// It was full screen before (⌘F, the green button): it stays full
    /// screen afterwards.
    pub was_covering: bool,
}

/// Why a presentation ended, for the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    /// The person pressed Esc.
    Escape,
    /// The app asked (`{on: false}`).
    App,
    /// Its window closed, or its app stopped.
    Closed,
    /// It is no longer full screen: ⌘F took it out.
    Uncovered,
    /// Another window or workspace took the focus, or it was minimized.
    FocusLeft,
    /// The person took the host window out of full screen.
    HostLeft,
    /// A phone style came up.
    Style,
}

/// What ends a presentation that no longer holds, checked after every event:
/// its window is `alive`, a phone style is not up (`mobile`), it still
/// `covers` the desk and has the focus (`focused`).
pub fn ending_for(alive: bool, mobile: bool, covers: bool, focused: bool) -> Option<Ending> {
    if !alive {
        Some(Ending::Closed)
    } else if mobile {
        Some(Ending::Style)
    } else if !covers {
        Some(Ending::Uncovered)
    } else if !focused {
        Some(Ending::FocusLeft)
    } else {
        None
    }
}

/// Whether a key ends the presentation: a plain Esc, the shell's own while
/// one is on. ⌃Esc stays the launcher's.
pub fn esc_ends(key: KeyCode, modifiers: &KeyModifiers, presenting: bool) -> bool {
    presenting && key == KeyCode::Escape && !(modifiers.shift || modifiers.control || modifiers.alt || modifiers.logo)
}

/// The shell's part: the presses, the presentation, the host window.
#[derive(Debug, Default)]
pub struct Presenter {
    pub activation: Activation,
    pub session: Option<Presentation>,
    pub host: HostFullscreen,
}

#[cfg(any(feature = "app-hub", native_mobile))]
pub use service::{register, take_requests, Request};

#[cfg(any(feature = "app-hub", native_mobile))]
mod service {
    use super::*;
    use makepad_widgets::makepad_platform::SignalToUI;
    use octosense_appstore::services::{self, HostService, Replier, ServiceCall, ServiceHost};
    use std::sync::Mutex;

    /// A request for the shell's next turn (`App::serve_window_requests`):
    /// the app and the isolate that asked, and where the answer goes.
    pub struct Request {
        pub app: String,
        pub isolate: usize,
        pub on: bool,
        pub reply: Replier,
    }

    static QUEUE: Mutex<Vec<Request>> = Mutex::new(Vec::new());

    /// Offer `window.fullscreen` to every app. It describes no method to
    /// App Hub's discovery yet: App Hub's contract knows no capability a
    /// window method could name (`KNOWN_CAPABILITIES`), and a description
    /// naming another would be refused.
    pub fn register() {
        services::register_host_service(Box::new(WindowService));
    }

    /// The requests waiting, taken off the queue.
    pub fn take_requests() -> Vec<Request> {
        std::mem::take(&mut *QUEUE.lock().unwrap_or_else(|e| e.into_inner()))
    }

    struct WindowService;

    impl HostService for WindowService {
        fn family(&self) -> &'static str {
            FAMILY
        }

        fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
            match admit(&call.service, call.from_sheet, call.may_prompt, &call.args) {
                Ok(on) => {
                    let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
                    if queue.len() >= MAX_QUEUED {
                        drop(queue);
                        reply.send(Err(BUSY.into()));
                        return;
                    }
                    queue.push(Request { app: call.app_id, isolate: reply.isolate_key(), on, reply });
                    drop(queue);
                    // The shell answers at the end of this event, or the next.
                    SignalToUI::set_ui_signal();
                }
                Err(refusal) => reply.send(Err(refusal)),
            }
        }
    }
}

use crate::*;

impl App {
    /// The person pressed in `client`'s window (`MpRunViewAction::Clicked`):
    /// for the next few seconds it may go full screen.
    pub(crate) fn note_window_press(&mut self, client: ClientId) {
        self.presenter.activation.press(client, host::now());
    }

    /// The `window.fullscreen` requests waiting, answered: after the event
    /// that pumped them, on the UI thread.
    #[cfg(any(feature = "app-hub", native_mobile))]
    pub(crate) fn serve_window_requests(&mut self, cx: &mut Cx) {
        for request in take_requests() {
            if !request.reply.is_pending() {
                continue;
            }
            let result = self.window_request(cx, &request);
            match &result {
                Ok(value) => log!("wm: window.fullscreen {{on: {}}} from {}: {}", request.on, request.app, value),
                Err(error) => log!("wm: window.fullscreen {{on: {}}} from {} refused: {}", request.on, request.app, error),
            }
            request.reply.send(result);
        }
    }

    #[cfg(any(feature = "app-hub", native_mobile))]
    fn window_request(&mut self, cx: &mut Cx, request: &Request) -> Result<Value, String> {
        let mobile = self.state.as_ref().is_none_or(|s| s.style.target.mobile());
        let window = if mobile { None } else { self.window_of(cx, &request.app, request.isolate) };
        let now = host::now();
        let facts = match (self.state.as_ref(), window) {
            (Some(state), Some(client)) => {
                let layout = &state.layout;
                Facts {
                    mobile,
                    window,
                    presenting: self.presenter.session.as_ref().map(|s| s.client),
                    covering: layout.is_fullscreen(client),
                    in_front: layout.focused_client() == Some(client)
                        && layout.workspace_of(client) == Some(layout.active)
                        && !layout.desktop.minimized(client)
                        && makepad_widgets::splash_host::surface_is_foreground(request.isolate, &request.app),
                    pressed: self.presenter.activation.fresh(client, now),
                }
            }
            _ => Facts { mobile, ..Facts::default() },
        };
        match decide(request.on, &facts) {
            Decision::Answer(value) => Ok(value),
            Decision::Refuse(refusal) => Err(refusal.into()),
            Decision::End(client) => {
                self.end_presentation(cx, Ending::App);
                // Still full screen when the person had made it so (⌘F) first.
                let full = self.state.as_ref().is_some_and(|s| s.layout.is_fullscreen(client));
                Ok(answer(true, full, full))
            }
            Decision::Begin(client) => {
                if self.begin_presentation(cx, client, &request.app, request.isolate) {
                    Ok(answer(true, true, true))
                } else {
                    Err(UNAVAILABLE.into())
                }
            }
        }
    }

    /// The window of script app `app` whose Card runner runs `isolate`.
    fn window_of(&mut self, cx: &mut Cx, app: &str, isolate: usize) -> Option<ClientId> {
        let candidates: Vec<(ClientId, Option<String>)> = self
            .state
            .as_ref()?
            .clients
            .iter()
            .filter(|(client, slot)| self.module_host.is_module(**client) && slot.foreground_app().as_deref() == Some(app))
            .map(|(client, slot)| (*client, slot.foreground_app()))
            .collect();
        let windows: Vec<WindowIdentity> = candidates
            .into_iter()
            .map(|(client, app)| {
                // The Card runner's own surface, as `MpModuleView` finds it.
                let isolate = self
                    .module_host
                    .dispatch(cx, client, "its window identity", |cx, root| root.splash(cx, ids!(card)).isolate_heap_key(cx))
                    .flatten();
                WindowIdentity { client, app, isolate }
            })
            .collect();
        window_for(app, isolate, &windows)
    }

    /// `client` goes full screen with no edge reveal, and the host window
    /// goes full screen; false when it cannot in this layout.
    fn begin_presentation(&mut self, cx: &mut Cx, client: ClientId, app: &str, isolate: usize) -> bool {
        let Some(state) = self.state.as_mut() else {
            return false;
        };
        let was_covering = state.layout.is_fullscreen(client);
        if !was_covering && !state.layout.set_fullscreen(client, true) {
            return false;
        }
        // No edge brings the bar, the title bar or the dock out while it
        // presents; whatever was out goes back now.
        state.presentation = Some(client);
        self.top_edge.set(cx, false);
        self.presenter.activation.spend(client);
        self.presenter.session = Some(Presentation { client, app: app.to_string(), isolate, was_covering });
        let window = self.ui.window(cx, ids!(main_window));
        let fullscreen = window.is_fullscreen(cx);
        let may_ask = !crate::launch_input::hidden_windows();
        let op = self.presenter.host.begin(fullscreen, may_ask, host::now());
        if fullscreen {
            log!("wm: app full screen: {app} (client {client}); the host window is full screen already and stays so");
        } else if !may_ask {
            log!("wm: app full screen: {app} (client {client}); the host window is not asked to enter full screen: windows are hidden (MAKEPAD_HIDE_WINDOWS)");
        } else {
            log!("wm: app full screen: {app} (client {client}); the host window enters full screen");
        }
        self.host_window_op(cx, op);
        // The bar hides now, tiled or floating (the draw that follows sees a
        // floating window go full screen too).
        self.sync_bar_for_fullscreen(cx);
        self.show_osd(cx, shell::osd::OsdShow::fullscreen_hint());
        self.tell_app_fullscreen(cx, client, true);
        self.update_bar(cx);
        self.redraw_all(cx);
        true
    }

    /// End the presentation, if one is on, and put everything back as it was.
    pub(crate) fn end_presentation(&mut self, cx: &mut Cx, ending: Ending) {
        let Some(session) = self.presenter.session.take() else {
            return;
        };
        // The window leaves full screen, unless it was full screen before
        // (⌘F): its rect and its place in the stack never changed. In both
        // layouts, in case the style switched. The edges work again.
        if let Some(state) = self.state.as_mut() {
            state.presentation = None;
            if !session.was_covering {
                state.layout.clear_fullscreen(session.client);
            }
        }
        // The host window leaves full screen only if this presentation took it there.
        let window = self.ui.window(cx, ids!(main_window));
        let op = self.presenter.host.end(window.is_fullscreen(cx), cfg!(target_os = "macos"), host::now());
        self.host_window_op(cx, op);
        if ending != Ending::Closed {
            self.tell_app_fullscreen(cx, session.client, false);
        }
        log!("wm: app full screen ended: {} (client {}), {:?}", session.app, session.client, ending);
        // The bar and the dock come back with it.
        if self.state.is_some() {
            self.sync_bar_for_fullscreen(cx);
            self.update_bar(cx);
        }
        self.redraw_all(cx);
    }

    /// The host window reported its full screen (`WindowGeomChange`).
    pub(crate) fn host_window_reported(&mut self, cx: &mut Cx, fullscreen: bool) {
        let (op, left) = self.presenter.host.reported(fullscreen);
        self.host_window_op(cx, op);
        if left {
            self.end_presentation(cx, Ending::HostLeft);
        }
    }

    fn host_window_op(&mut self, cx: &mut Cx, op: Option<HostOp>) {
        let window = self.ui.window(cx, ids!(main_window));
        // TODO(makepad): `Window::fullscreen` and `normal` push
        // `FullscreenWindow` and `NormalizeWindow`, which makepad's macOS
        // backend ignores; `maximize` and `restore` both toggle
        // `toggleFullScreen:` there. Once makepad makes `fullscreen` and
        // `normal` real on macOS, enter and leave through them.
        match op {
            // The bar's own window control makes the same two calls.
            Some(HostOp::Enter) => window.maximize(cx),
            Some(HostOp::Leave) => {
                log!("wm: app full screen: the host window leaves the full screen it entered for it");
                window.restore(cx);
            }
            None => {}
        }
    }

    /// The app's optional `on_app_fullscreen(on)` hook, in its own isolate.
    fn tell_app_fullscreen(&mut self, cx: &mut Cx, client: ClientId, on: bool) {
        let told = self.module_host.dispatch(cx, client, "its full-screen hook", |cx, root| {
            root.splash(cx, ids!(card)).call_script_fn(cx, id!(on_app_fullscreen), &[on.into()])
        });
        if told == Some(true) {
            cx.with_vm_and_async(|_| {});
        }
    }

    /// After every event: a presentation that no longer holds ends (its
    /// window closed, a phone style came up, ⌘F took it out of full screen,
    /// or another window or workspace took the focus).
    pub(crate) fn sync_presentation(&mut self, cx: &mut Cx) {
        let (Some(state), Some(session)) = (self.state.as_ref(), self.presenter.session.as_ref()) else {
            return;
        };
        let client = session.client;
        let alive = state.clients.contains_key(&client) && self.module_host.is_module(client);
        let layout = &state.layout;
        if let Some(ending) = ending_for(alive, state.style.target.mobile(), layout.is_fullscreen(client), layout.focused_client() == Some(client)) {
            self.end_presentation(cx, ending);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(window: Option<ClientId>) -> Facts {
        Facts { window, in_front: true, pressed: true, ..Facts::default() }
    }

    #[test]
    fn only_on_true_or_false_is_a_request() {
        assert_eq!(parse_on(&json!({"on": true})), Ok(true));
        assert_eq!(parse_on(&json!({"on": false})), Ok(false));
        for bad in [json!(null), json!(true), json!({}), json!({"on": 1}), json!({"on": "yes"}), json!({"on": true, "window": 3}), json!([true])] {
            assert!(parse_on(&bad).unwrap_err().starts_with("invalid_arguments:"), "{bad}");
        }
    }

    /// A background surface (an agent's tool call, a home tile, a callback of
    /// one) and a host sheet are refused before the shell's turn; so is any
    /// other method of the family.
    #[test]
    fn only_a_foreground_app_surface_reaches_the_shell() {
        let on = json!({"on": true});
        assert_eq!(admit("window.fullscreen", false, true, &on), Ok(true));
        assert_eq!(admit("window.fullscreen", false, false, &on), Err(FOREGROUND_REQUIRED.into()));
        assert_eq!(admit("window.fullscreen", true, true, &on), Err(NOT_A_WINDOW.into()));
        assert!(admit("window.move", false, true, &on).unwrap_err().starts_with("method_unavailable:"));
        assert!(admit("window.fullscreen", false, true, &json!({})).unwrap_err().starts_with("invalid_arguments:"));
    }

    /// An app finds only its own window: the one whose Card runner runs the
    /// isolate that asked. Another app's window, a glance card's isolate and
    /// a window whose runner has no isolate are no one's to change.
    #[test]
    fn an_app_reaches_only_its_own_window() {
        let windows = [
            WindowIdentity { client: 1, app: Some("os.quickdeck".into()), isolate: Some(100) },
            WindowIdentity { client: 2, app: Some("os.pdftools".into()), isolate: Some(200) },
            WindowIdentity { client: 3, app: Some("os.pdftools".into()), isolate: None },
            WindowIdentity { client: 4, app: None, isolate: Some(400) },
        ];
        assert_eq!(window_for("os.quickdeck", 100, &windows), Some(1));
        assert_eq!(window_for("os.pdftools", 200, &windows), Some(2));
        assert_eq!(window_for("os.quickdeck", 200, &windows), None, "another app's isolate");
        assert_eq!(window_for("os.pdftools", 100, &windows), None, "another app's window");
        assert_eq!(window_for("os.quickdeck", 999, &windows), None, "a glance card's isolate");
        assert_eq!(window_for("os.native", 400, &windows), None, "not a script app's window");
        assert_eq!(decide(true, &facts(None)), Decision::Refuse(NOT_A_WINDOW));
        assert_eq!(decide(false, &facts(None)), Decision::Refuse(NOT_A_WINDOW));
    }

    /// On: the app's window in front, pressed a moment ago. Without the
    /// press, or out of front, it is refused, saying which.
    #[test]
    fn going_full_screen_needs_the_window_in_front_and_a_press() {
        assert_eq!(decide(true, &facts(Some(7))), Decision::Begin(7));
        let unpressed = Facts { pressed: false, ..facts(Some(7)) };
        assert_eq!(decide(true, &unpressed), Decision::Refuse(GESTURE_REQUIRED));
        let behind = Facts { in_front: false, ..facts(Some(7)) };
        assert_eq!(decide(true, &behind), Decision::Refuse(FOREGROUND_REQUIRED));
        let presenting = Facts { presenting: Some(7), covering: true, pressed: false, ..facts(Some(7)) };
        assert_eq!(decide(true, &presenting), Decision::Answer(answer(false, true, true)), "already: nothing to apply");
        // Another window presenting does not stand in the way: its focus left.
        let other = Facts { presenting: Some(8), ..facts(Some(7)) };
        assert_eq!(decide(true, &other), Decision::Begin(7));
    }

    /// Off: ends this window's presentation; otherwise there is nothing to
    /// apply, and the answer says whether the window still covers the desk
    /// (the person's ⌘F, which the app leaves alone). No press is needed.
    #[test]
    fn going_back_ends_only_the_apps_own_presentation() {
        let presenting = Facts { presenting: Some(7), covering: true, pressed: false, in_front: false, ..facts(Some(7)) };
        assert_eq!(decide(false, &presenting), Decision::End(7));
        assert_eq!(decide(false, &facts(Some(7))), Decision::Answer(answer(false, false, false)));
        let by_person = Facts { covering: true, ..facts(Some(7)) };
        assert_eq!(decide(false, &by_person), Decision::Answer(answer(false, true, true)));
        let other = Facts { presenting: Some(8), ..facts(Some(7)) };
        assert_eq!(decide(false, &other), Decision::Answer(answer(false, false, false)), "never another window's");
    }

    /// A phone's every app fills the screen and there is no immersive mode:
    /// nothing to apply, and the answer says the window fills the screen.
    #[test]
    fn on_a_phone_the_request_changes_nothing() {
        for on in [true, false] {
            let phone = Facts { mobile: true, ..Facts::default() };
            assert_eq!(decide(on, &phone), Decision::Answer(answer(false, false, true)));
        }
        assert_eq!(answer(true, true, true), json!({"applied": true, "fullscreen": true, "fills_screen": true}));
    }

    /// A press counts for a few seconds, only in its own window, and once.
    #[test]
    fn a_press_activates_its_window_briefly_and_once() {
        let mut a = Activation::default();
        assert!(!a.fresh(1, 10.0), "no press yet");
        a.press(1, 10.0);
        assert!(a.fresh(1, 10.2));
        assert!(a.fresh(1, 10.0 + ACTIVATION_SECONDS));
        assert!(!a.fresh(1, 10.1 + ACTIVATION_SECONDS), "too long ago");
        assert!(!a.fresh(1, 9.0), "a clock that went back proves nothing");
        assert!(!a.fresh(2, 10.2), "another window's press");
        a.spend(1);
        assert!(!a.fresh(1, 10.3), "used up");
        a.press(1, 11.0);
        assert!(a.fresh(1, 11.1));
        // A later press anywhere drops the ones too old to count.
        a.press(2, 30.0);
        assert_eq!(a.presses.len(), 1);
    }

    /// Already full screen: the host window is left as it is at both ends.
    #[test]
    fn a_host_window_already_full_screen_stays_so() {
        let mut host = HostFullscreen::default();
        assert_eq!(host.begin(true, true, 0.0), None);
        assert!(!host.entered_for_presentation());
        assert_eq!(host.end(true, true, 1.0), None);
        assert_eq!(host.end(true, false, 1.0), None);
    }

    /// Not full screen: the presentation enters it and leaves it again, on a
    /// toggling platform only while it reports full screen.
    #[test]
    fn a_presentation_leaves_only_the_full_screen_it_entered() {
        for toggles in [true, false] {
            let mut host = HostFullscreen::default();
            assert_eq!(host.begin(false, true, 0.0), Some(HostOp::Enter));
            assert!(host.entered_for_presentation());
            assert_eq!(host.reported(true), (None, false), "it went in");
            assert_eq!(host.reported(true), (None, false), "another geometry change");
            assert_eq!(host.end(true, toggles, 5.0), Some(HostOp::Leave), "toggles {toggles}");
            assert!(!host.entered_for_presentation());
            assert_eq!(host.reported(false), (None, false), "our own leave ends nothing");
            assert_eq!(host.end(false, toggles, 6.0), None, "nothing more to give back");
        }
        // The person left it already (macOS): a toggle now would enter it again.
        let mut host = HostFullscreen::default();
        host.begin(false, true, 0.0);
        host.reported(true);
        assert_eq!(host.end(false, true, 5.0), None);
    }

    /// The person leaving the host's full screen ends the presentation, which
    /// then has no full screen to give back.
    #[test]
    fn the_person_leaving_the_host_full_screen_ends_the_presentation() {
        for already in [false, true] {
            let mut host = HostFullscreen::default();
            host.begin(already, true, 0.0);
            host.reported(true);
            assert_eq!(host.reported(false), (None, true), "already {already}");
            assert_eq!(host.end(false, true, 2.0), None);
        }
    }

    /// Ended while the host window is still on its way in: on a toggling
    /// platform it leaves once it says it is in; elsewhere at once. One that
    /// never says so is taken not to have entered.
    #[test]
    fn a_presentation_ended_while_entering_leaves_once_in() {
        let mut host = HostFullscreen::default();
        host.begin(false, true, 0.0);
        assert_eq!(host.end(false, true, 0.5), None);
        assert_eq!(host.reported(false), (None, false), "a change from before the enter");
        assert_eq!(host.reported(true), (Some(HostOp::Leave), false));
        assert_eq!(host.reported(false), (None, false));

        let mut host = HostFullscreen::default();
        host.begin(false, true, 0.0);
        assert_eq!(host.end(false, false, 0.5), Some(HostOp::Leave), "maximize and restore do not toggle");

        let mut host = HostFullscreen::default();
        host.begin(false, true, 0.0);
        assert_eq!(host.end(false, true, ENTER_GRACE_SECONDS + 1.0), None, "it never went in");
        assert_eq!(host.reported(true), (None, false), "and if it does after all, it is the person's");
        assert_eq!(host.begin(false, true, 10.0), Some(HostOp::Enter));
    }

    /// A presentation beginning again while the enter asked for a moment ago
    /// is on its way does not toggle twice, and keeps that enter.
    #[test]
    fn a_new_presentation_keeps_an_enter_on_its_way() {
        let mut host = HostFullscreen::default();
        assert_eq!(host.begin(false, true, 0.0), Some(HostOp::Enter));
        assert_eq!(host.end(false, true, 0.2), None);
        assert_eq!(host.begin(false, true, 0.4), None, "the first enter serves it");
        assert_eq!(host.reported(true), (None, false), "in, for the new presentation");
        assert_eq!(host.end(true, true, 3.0), Some(HostOp::Leave));
    }

    /// Hidden windows: the host window is never asked, so nothing is given back.
    #[test]
    fn hidden_windows_are_never_asked_to_go_full_screen() {
        let mut host = HostFullscreen::default();
        assert_eq!(host.begin(false, false, 0.0), None);
        assert!(!host.entered_for_presentation());
        assert_eq!(host.reported(true), (None, false));
        assert_eq!(host.end(false, true, 1.0), None);
    }

    /// The presentation holds while its window is open, a desktop style is up,
    /// it covers the desk and has the focus; each of those ends it.
    #[test]
    fn a_presentation_ends_when_its_window_closes_or_lets_go() {
        assert_eq!(ending_for(true, false, true, true), None);
        assert_eq!(ending_for(false, false, true, true), Some(Ending::Closed), "the window closed");
        assert_eq!(ending_for(false, true, false, false), Some(Ending::Closed));
        assert_eq!(ending_for(true, true, true, true), Some(Ending::Style));
        assert_eq!(ending_for(true, false, false, true), Some(Ending::Uncovered), "⌘F");
        assert_eq!(ending_for(true, false, true, false), Some(Ending::FocusLeft));
    }

    /// Esc ends a presentation; ⌃Esc stays the launcher's, and with none on,
    /// Esc is the app's as before.
    #[test]
    fn esc_is_the_shells_while_a_window_presents() {
        let plain = KeyModifiers::default();
        assert!(esc_ends(KeyCode::Escape, &plain, true));
        assert!(!esc_ends(KeyCode::Escape, &plain, false));
        assert!(!esc_ends(KeyCode::Escape, &KeyModifiers { control: true, ..plain }, true));
        assert!(!esc_ends(KeyCode::Escape, &KeyModifiers { logo: true, ..plain }, true));
        assert!(!esc_ends(KeyCode::KeyF, &plain, true));
    }
}
