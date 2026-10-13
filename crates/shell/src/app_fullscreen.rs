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
//! closing or losing the focus, the app failing, a style switch, or the
//! person leaving the host's full screen ends it and puts everything back as
//! it was: the window's rect and its place in the stack (never changed while
//! it was full screen), the bar and the dock, and the host window, which
//! leaves full screen only if this presentation took it there, even when it
//! arrives there late ([`PENDING_LEAVE_SECONDS`]). The app hears both ends
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
//! came from a person's tap. Input provenance (`trusted_user_input`: macOS
//! HID pointers and Android touches only) holds only during the native
//! event's own handler, and a Splash `on_click` runs later, from the
//! widget-to-script queue, outside it. So the shell keeps a transient
//! activation, as a browser does: a press on the app's own content in that
//! window (`MpRunViewAction::Pressed`: the module's live root with no host
//! service's sheet up over it, never a group tab, a stopped app's face or a
//! process tile) within [`ACTIVATION_SECONDS`] before the request, used up
//! by a request that applies ([`Activation`]). A presentation that ends any
//! other way than the app's own `{on: false}` (Esc, ⌘F, the focus leaving)
//! drops the window's press as well ([`Presenter::finish`]): an app that
//! asks again from its `on_app_fullscreen(false)` hook needs a new press, so
//! a click that turned a slide cannot take it straight back. Any press
//! counts, remote and synthetic ones too: a script app cannot make input.
//! Esc and the hint are the rest of the protection.
//!
//! **Not from an agent.** Every request carries its surface's authority
//! (`ServiceCall::may_prompt`). App Hub's Card runner gives every app's
//! window that authority, whatever its manifest declares
//! (`IsolateSettings::host_prompts`; the `prompt` capability is a
//! declaration, `AppPolicy::may_prompt`, and nothing on a request's path
//! reads it), and takes it away while one of the app's agent tool calls
//! runs, and while the app is suspended, which only happens on a phone. A
//! callback keeps the authority of the request it answers. So a request
//! without it came from an agent's tool call or a callback of one, and
//! `{on: true}` is refused `agent_call`; a window not in front (the focus
//! elsewhere, another workspace, minimized) is refused `foreground_required`.
//! `{on: false}` needs neither: an app may always give the screen back. A
//! test (`no_capability_decides_a_requests_authority`) holds App Hub to
//! this, so a pin that tied the authority to a capability would fail here
//! first.

use crate::layout::ClientId;
use makepad_widgets::widget_async::SplashVmId;
use makepad_widgets::{KeyCode, KeyModifiers};
use serde_json::{json, Value};
use std::collections::HashMap;

/// The host-service family: `window.fullscreen`.
pub const FAMILY: &str = "window";
/// How long after the person's press in a window that window may still go
/// full screen: a browser's transient activation lasts about as long.
pub const ACTIVATION_SECONDS: f64 = 5.0;
/// Requests waiting for the shell's next turn, from every app together.
pub const MAX_QUEUED: usize = 16;
/// The log gets at most one line per app this often for requests that
/// changed nothing (refusals among them), and counts the rest.
pub const QUIET_LOG_SECONDS: f64 = 10.0;
/// How long a leave waits, once a presentation ended while the host window
/// was still entering full screen, for the window to say it is in. After
/// that the enter is taken never to have come (macOS ignores one asked for
/// while the window is still leaving the last full screen), and a full
/// screen the window reports is the person's.
pub const PENDING_LEAVE_SECONDS: f64 = 20.0;

pub const NOT_A_WINDOW: &str = "not_a_window: only the app's own window can go full screen";
pub const FOREGROUND_REQUIRED: &str =
    "foreground_required: the app's window must be in front: focused, on the workspace in view, not minimized";
pub const AGENT_CALL: &str =
    "agent_call: not from an agent's tool call, or from a callback of one: ask from the person's tap or click";
pub const GESTURE_REQUIRED: &str = "gesture_required: ask for full screen from the person's tap or click in the app's window";
pub const UNAVAILABLE: &str = "unavailable: this window cannot cover the desk in this layout";
pub const BUSY: &str = "busy: too many full-screen requests are waiting";

/// A `window.*` request, checked before it reaches the shell's turn: the
/// method, the surface and the arguments. The answer is `on`. Whether the
/// request may go full screen is the shell's turn to decide ([`decide`]),
/// with the window in view.
pub fn admit(service: &str, from_sheet: bool, args: &Value) -> Result<bool, String> {
    if service != "window.fullscreen" {
        return Err(format!("method_unavailable: the window service has fullscreen {{on}} only, not {service:?}"));
    }
    if from_sheet {
        return Err(NOT_A_WINDOW.into());
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
    /// The window has the focus, is not minimized, and is on the workspace
    /// in view.
    pub in_front: bool,
    /// The request came without its surface's authority
    /// (`ServiceCall::may_prompt`): from an agent's tool call, or from a
    /// callback of one (see the module notes).
    pub agent_call: bool,
    /// The person pressed the app's content in it within
    /// [`ACTIVATION_SECONDS`].
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
    if facts.agent_call {
        return Decision::Refuse(AGENT_CALL);
    }
    if !facts.pressed {
        return Decision::Refuse(GESTURE_REQUIRED);
    }
    Decision::Begin(client)
}

/// The presses on apps' content in their windows, for the transient
/// activation a request needs (see the module notes).
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
    /// `client`'s press counts no more: a request that applied used it up,
    /// or the window's presentation ended without the app asking.
    pub fn clear(&mut self, client: ClientId) {
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
    /// Asked to enter full screen; it has not said that it did. `leave`: the
    /// presentation ended meanwhile, at that time, so the window's next
    /// report decides: in, however late, and it leaves; not in, and there is
    /// nothing to give back (macOS reports a failed enter too). Past
    /// [`PENDING_LEAVE_SECONDS`] the leave is dropped.
    Entering { leave: Option<f64> },
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
    /// A presentation begins at `now`, the host window `fullscreen` or not;
    /// `may_ask`: the shell may ask it to enter (its windows are shown).
    pub fn begin(&mut self, fullscreen: bool, may_ask: bool, now: f64) -> Option<HostOp> {
        self.expire(now);
        if let HostState::Entering { .. } = self.state {
            // The enter asked for before is still on its way: it serves this
            // one, and a second toggle would take the window back out.
            self.state = HostState::Entering { leave: None };
            return None;
        }
        self.state = if fullscreen {
            HostState::AlreadyFull
        } else if !may_ask {
            HostState::Skipped
        } else {
            HostState::Entering { leave: None }
        };
        matches!(self.state, HostState::Entering { .. }).then_some(HostOp::Enter)
    }

    /// The host window reports at `now` whether it is full screen. The op
    /// to send, and whether the person took it out of a full screen a
    /// presentation relies on (which ends the presentation).
    pub fn reported(&mut self, fullscreen: bool, now: f64) -> (Option<HostOp>, bool) {
        self.expire(now);
        match (self.state, fullscreen) {
            // In after its presentation ended, however late: it goes back out.
            (HostState::Entering { leave: Some(_) }, true) => {
                self.state = HostState::Idle;
                (Some(HostOp::Leave), false)
            }
            // Not in after its presentation ended: the enter failed or never
            // came, and nothing is left to give back.
            (HostState::Entering { leave: Some(_) }, false) => {
                self.state = HostState::Idle;
                (None, false)
            }
            (HostState::Entering { leave: None }, true) => {
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

    /// A presentation ends at `now`, the host window `fullscreen` or not;
    /// `toggles`: entering and leaving are one toggle (macOS).
    pub fn end(&mut self, fullscreen: bool, toggles: bool, now: f64) -> Option<HostOp> {
        let op = match self.state {
            HostState::Entered if fullscreen || !toggles => Some(HostOp::Leave),
            // Still on its way in: a toggle now would only turn it round in
            // the middle, so the window's next report decides, for a while.
            HostState::Entering { .. } if toggles => {
                self.state = HostState::Entering { leave: Some(now) };
                return None;
            }
            HostState::Entering { .. } => Some(HostOp::Leave),
            _ => None,
        };
        self.state = HostState::Idle;
        op
    }

    /// A leave that waited longer than [`PENDING_LEAVE_SECONDS`] for an
    /// enter is dropped: whatever the window reports next is the person's.
    fn expire(&mut self, now: f64) {
        if let HostState::Entering { leave: Some(at) } = self.state {
            if !(now >= at && now - at <= PENDING_LEAVE_SECONDS) {
                self.state = HostState::Idle;
            }
        }
    }
}

/// One app window presenting.
#[derive(Clone, Debug)]
pub struct Presentation {
    pub client: ClientId,
    pub app: String,
    pub isolate: usize,
    /// The instance presenting: one that takes its place in the window does
    /// not inherit the presentation.
    pub instance: SplashVmId,
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
    /// Its window closed, or another instance took its place.
    Closed,
    /// Its app failed: the module panicked, and the window shows the closed
    /// face with Restart.
    Failed,
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
/// its window is `alive` (open, with the instance that began presenting),
/// its app has not `failed`, a phone style is not up (`mobile`), it still
/// `covers` the desk and has the focus (`focused`).
pub fn ending_for(alive: bool, failed: bool, mobile: bool, covers: bool, focused: bool) -> Option<Ending> {
    if !alive {
        Some(Ending::Closed)
    } else if failed {
        Some(Ending::Failed)
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

/// The log lines for requests that changed nothing, refusals among them: at
/// most one per app every [`QUIET_LOG_SECONDS`], so an app that asks in a
/// loop cannot flood the log.
#[derive(Debug, Default)]
pub struct QuietLog {
    /// app -> when its last line was written, and the requests since that
    /// were not.
    apps: HashMap<String, (f64, usize)>,
}

impl QuietLog {
    /// A request of `app`'s that changed nothing, at `now`: `Some(n)` to
    /// write its line, saying `n` more went unwritten since the last one;
    /// `None` to count it.
    pub fn note(&mut self, app: &str, now: f64) -> Option<usize> {
        if let Some((at, unwritten)) = self.apps.get_mut(app) {
            if now >= *at && now - *at < QUIET_LOG_SECONDS {
                *unwritten += 1;
                return None;
            }
        }
        Some(self.apps.insert(app.to_string(), (now, 0)).map_or(0, |(_, unwritten)| unwritten))
    }
}

/// The shell's part: the presses, the presentation, the host window.
#[derive(Debug, Default)]
pub struct Presenter {
    pub activation: Activation,
    pub session: Option<Presentation>,
    pub host: HostFullscreen,
    pub quiet: QuietLog,
}

impl Presenter {
    /// The person pressed the app's content in `client`'s window at `now`.
    pub fn press(&mut self, client: ClientId, now: f64) {
        self.activation.press(client, now);
    }

    /// Whether `client`'s window has a press to go full screen with at `now`.
    pub fn pressed(&self, client: ClientId, now: f64) -> bool {
        self.activation.fresh(client, now)
    }

    /// `presentation` begins, and uses its window's press up.
    pub fn start(&mut self, presentation: Presentation) {
        self.activation.clear(presentation.client);
        self.session = Some(presentation);
    }

    /// The presentation ends, for `ending`. Unless the app itself asked, its
    /// window's press goes too: a press made while it presented (a click
    /// that turned a slide) must not take it straight back to full screen
    /// when the app asks again, say from `on_app_fullscreen(false)`.
    pub fn finish(&mut self, ending: Ending) -> Option<Presentation> {
        let session = self.session.take()?;
        if ending != Ending::App {
            self.activation.clear(session.client);
        }
        Some(session)
    }
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
    /// the app and the isolate that asked, what it asked, whether it came
    /// without its surface's authority, and where the answer goes.
    pub struct Request {
        pub app: String,
        pub isolate: usize,
        pub on: bool,
        /// `!ServiceCall::may_prompt`: an agent's tool call, or a callback
        /// of one (the module notes).
        pub agent_call: bool,
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
            match admit(&call.service, call.from_sheet, &call.args) {
                Ok(on) => {
                    let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
                    if queue.len() >= MAX_QUEUED {
                        drop(queue);
                        reply.send(Err(BUSY.into()));
                        return;
                    }
                    queue.push(Request { app: call.app_id, isolate: reply.isolate_key(), on, agent_call: !call.may_prompt, reply });
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

/// Whether a host service's sheet is up over a card app: App Hub's Card
/// runner draws one (a sign-in, a wizard) as the Splash `sheet` over the
/// app, as `mobile_back::card_sheet_back` finds it. A press there is the
/// host's, not the app's. Run inside the module's isolate.
pub(crate) fn host_sheet_up(cx: &mut Cx, root: &WidgetRef) -> bool {
    root.splash(cx, ids!(sheet)).borrow().is_some_and(|sheet| sheet.view.visible)
}

impl App {
    /// The person pressed the app's own content in `client`'s window
    /// (`MpRunViewAction::Pressed`): for the next few seconds it may go full
    /// screen.
    pub(crate) fn note_window_press(&mut self, client: ClientId) {
        self.presenter.press(client, host::now());
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
            let applied = matches!(&result, Ok(value) if value["applied"] == true);
            // A request that changed something needed a press or ended a
            // presentation, so it cannot come in a flood; one that changed
            // nothing can.
            let unwritten = if applied { Some(0) } else { self.presenter.quiet.note(&request.app, host::now()) };
            if let Some(unwritten) = unwritten {
                let more = if unwritten > 0 { format!(" ({unwritten} more since the last such line)") } else { String::new() };
                match &result {
                    Ok(value) => log!("wm: window.fullscreen {{on: {}}} from {}: {}{}", request.on, request.app, value, more),
                    Err(error) => log!("wm: window.fullscreen {{on: {}}} from {} refused: {}{}", request.on, request.app, error, more),
                }
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
                        && !layout.desktop.minimized(client),
                    agent_call: request.agent_call,
                    pressed: self.presenter.pressed(client, now),
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

    /// The window of script app `app` whose Card runner runs `isolate`. A
    /// failed instance's window has no app in it to ask.
    fn window_of(&mut self, cx: &mut Cx, app: &str, isolate: usize) -> Option<ClientId> {
        let candidates: Vec<(ClientId, Option<String>)> = self
            .state
            .as_ref()?
            .clients
            .iter()
            .filter(|(client, slot)| {
                self.module_host.is_module(**client)
                    && !self.module_host.is_failed(**client)
                    && slot.foreground_app().as_deref() == Some(app)
            })
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
        // Another window's presentation this event has not ended yet (its
        // window lost the focus to this one) ends first.
        if self.presenter.session.as_ref().is_some_and(|s| s.client != client) {
            self.end_presentation(cx, Ending::FocusLeft);
        }
        let Some(instance) = self.module_host.get(client).map(|i| i.vm_id) else {
            return false;
        };
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
        self.presenter.start(Presentation { client, app: app.to_string(), isolate, instance, was_covering });
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
        let Some(session) = self.presenter.finish(ending) else {
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
        // A closed window, or a failed app, has no script left to tell.
        if !matches!(ending, Ending::Closed | Ending::Failed) {
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

    /// A window reported its geometry (`WindowGeomChange`). Only the shell's
    /// own window's full screen counts here, not an app's extra window's.
    pub(crate) fn host_window_reported(&mut self, cx: &mut Cx, window: WindowId, fullscreen: bool) {
        if self.ui.window(cx, ids!(main_window)).window_id() != Some(window) {
            return;
        }
        let (op, left) = self.presenter.host.reported(fullscreen, host::now());
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
    /// window closed or holds another instance, its app failed, a phone
    /// style came up, ⌘F took it out of full screen, or another window or
    /// workspace took the focus).
    pub(crate) fn sync_presentation(&mut self, cx: &mut Cx) {
        let (Some(state), Some(session)) = (self.state.as_ref(), self.presenter.session.as_ref()) else {
            return;
        };
        let client = session.client;
        let instance = self.module_host.get(client);
        let alive = state.clients.contains_key(&client) && instance.is_some_and(|i| i.vm_id == session.instance);
        let failed = instance.is_some_and(|i| i.failure().is_some());
        let layout = &state.layout;
        let ending = ending_for(
            alive,
            failed,
            state.style.target.mobile(),
            layout.is_fullscreen(client),
            layout.focused_client() == Some(client),
        );
        if let Some(ending) = ending {
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

    fn presentation(client: ClientId) -> Presentation {
        Presentation { client, app: "os.quickdeck".into(), isolate: 100, instance: SplashVmId(1), was_covering: false }
    }

    #[test]
    fn only_on_true_or_false_is_a_request() {
        assert_eq!(parse_on(&json!({"on": true})), Ok(true));
        assert_eq!(parse_on(&json!({"on": false})), Ok(false));
        for bad in [json!(null), json!(true), json!({}), json!({"on": 1}), json!({"on": "yes"}), json!({"on": true, "window": 3}), json!([true])] {
            assert!(parse_on(&bad).unwrap_err().starts_with("invalid_arguments:"), "{bad}");
        }
    }

    /// A host sheet is refused before the shell's turn, and so is any other
    /// method of the family. The request's authority is weighed in the
    /// shell's turn, with the window ([`decide`]).
    #[test]
    fn only_an_apps_own_surface_reaches_the_shell() {
        let on = json!({"on": true});
        assert_eq!(admit("window.fullscreen", false, &on), Ok(true));
        assert_eq!(admit("window.fullscreen", false, &json!({"on": false})), Ok(false));
        assert_eq!(admit("window.fullscreen", true, &on), Err(NOT_A_WINDOW.into()));
        assert!(admit("window.move", false, &on).unwrap_err().starts_with("method_unavailable:"));
        assert!(admit("window.fullscreen", false, &json!({})).unwrap_err().starts_with("invalid_arguments:"));
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

    /// On: the app's window in front, pressed a moment ago, asked by the app
    /// itself. Without the press, out of front, or from an agent's tool
    /// call, it is refused, saying which.
    #[test]
    fn going_full_screen_needs_the_window_in_front_and_a_press() {
        assert_eq!(decide(true, &facts(Some(7))), Decision::Begin(7));
        let unpressed = Facts { pressed: false, ..facts(Some(7)) };
        assert_eq!(decide(true, &unpressed), Decision::Refuse(GESTURE_REQUIRED));
        let behind = Facts { in_front: false, ..facts(Some(7)) };
        assert_eq!(decide(true, &behind), Decision::Refuse(FOREGROUND_REQUIRED));
        let agent = Facts { agent_call: true, ..facts(Some(7)) };
        assert_eq!(decide(true, &agent), Decision::Refuse(AGENT_CALL), "a press does not make an agent's call the person's");
        let agent_behind = Facts { agent_call: true, in_front: false, ..facts(Some(7)) };
        assert_eq!(decide(true, &agent_behind), Decision::Refuse(FOREGROUND_REQUIRED), "the window first");
        let presenting = Facts { presenting: Some(7), covering: true, pressed: false, ..facts(Some(7)) };
        assert_eq!(decide(true, &presenting), Decision::Answer(answer(false, true, true)), "already: nothing to apply");
        // Another window presenting does not stand in the way: its focus left.
        let other = Facts { presenting: Some(8), ..facts(Some(7)) };
        assert_eq!(decide(true, &other), Decision::Begin(7));
    }

    /// Off: ends this window's presentation; otherwise there is nothing to
    /// apply, and the answer says whether the window still covers the desk
    /// (the person's ⌘F, which the app leaves alone). No press is needed,
    /// nor the window in front, nor the surface's authority: an app may
    /// always give the screen back.
    #[test]
    fn going_back_ends_only_the_apps_own_presentation() {
        let presenting = Facts { presenting: Some(7), covering: true, pressed: false, in_front: false, ..facts(Some(7)) };
        assert_eq!(decide(false, &presenting), Decision::End(7));
        let from_agent = Facts { agent_call: true, ..presenting.clone() };
        assert_eq!(decide(false, &from_agent), Decision::End(7));
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

    /// The surface's authority is no capability: App Hub gives an app's
    /// window prompt authority whatever its manifest declares, and only a
    /// tool call or a suspended app takes it away. An App Hub that tied it to
    /// the `prompt` capability would have every app that does not declare
    /// `prompt` refused `agent_call` (rule 12): this fails first.
    #[cfg(feature = "app-hub")]
    #[test]
    fn no_capability_decides_a_requests_authority() {
        // None at all, PDF Tools' and Quick Deck's (neither declares
        // `prompt`), and `prompt` itself.
        let declared = [json!([]), json!(["storage", "files", "pdf"]), json!(["storage", "files", "deck", "model"]), json!(["prompt"])];
        for capabilities in declared {
            let manifest = octosense_app_contract::AppManifest::parse(
                &json!({
                    "schema": 1, "id": "org.example.presenter", "name": "Presenter", "version": "1.0.0",
                    "capabilities": capabilities, "integrity": {"bundle_blake3": ""}
                })
                .to_string(),
            )
            .unwrap();
            let limits = octosense_app_contract::HostLimits::default().with_require_signature(false);
            let policy = octosense_app_contract::resolve(&manifest, &limits).unwrap();
            let settings = octosense_app_policy::IsolateSettings::for_app(&policy, std::path::Path::new("/data"));
            assert!(settings.host_prompts, "declaring {capabilities} decided the window's authority");
        }
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
        a.clear(1);
        assert!(!a.fresh(1, 10.3), "used up");
        a.press(1, 11.0);
        assert!(a.fresh(1, 11.1));
        // A later press anywhere drops the ones too old to count.
        a.press(2, 30.0);
        assert_eq!(a.presses.len(), 1);
    }

    /// The presses through a presentation, as the shell wires them: the
    /// press that begins it is used up, and one made while it presents (a
    /// click that turned a slide) does not bring it back when the person
    /// ends it, by Esc or any other way, and the app asks again, say from
    /// `on_app_fullscreen(false)`. Only the app's own `{on: false}` leaves a
    /// press it was given, as a browser's exit does.
    #[test]
    fn a_press_made_while_presenting_does_not_bring_it_back() {
        let ask = |p: &Presenter, now: f64| decide(true, &Facts { pressed: p.pressed(7, now), ..facts(Some(7)) });
        for ending in [Ending::Escape, Ending::Uncovered, Ending::FocusLeft, Ending::HostLeft, Ending::Style, Ending::Failed, Ending::Closed] {
            let mut p = Presenter::default();
            p.press(7, 1.0);
            assert_eq!(ask(&p, 1.2), Decision::Begin(7));
            p.start(presentation(7));
            assert!(!p.pressed(7, 1.3), "the press that began it is used up");
            p.press(7, 2.0);
            assert_eq!(p.finish(ending).map(|s| s.client), Some(7));
            assert_eq!(ask(&p, 2.5), Decision::Refuse(GESTURE_REQUIRED), "{ending:?} then asked again");
            p.press(7, 3.0);
            assert_eq!(ask(&p, 3.1), Decision::Begin(7), "a new press after {ending:?} does");
        }
        let mut p = Presenter::default();
        p.press(7, 1.0);
        p.start(presentation(7));
        p.press(7, 2.0);
        assert_eq!(p.finish(Ending::App).map(|s| s.client), Some(7));
        assert_eq!(ask(&p, 2.5), Decision::Begin(7), "the app's own toggle keeps the press that toggled it");
        assert!(p.finish(Ending::Escape).is_none(), "nothing presents");
    }

    /// Already full screen: the host window is left as it is at both ends.
    #[test]
    fn a_host_window_already_full_screen_stays_so() {
        let mut host = HostFullscreen::default();
        assert_eq!(host.begin(true, true, 0.0), None);
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
            assert_eq!(host.reported(true, 0.5), (None, false), "it went in");
            assert_eq!(host.reported(true, 1.0), (None, false), "another geometry change");
            assert_eq!(host.end(true, toggles, 5.0), Some(HostOp::Leave), "toggles {toggles}");
            assert_eq!(host.reported(false, 5.5), (None, false), "our own leave ends nothing");
            assert_eq!(host.end(false, toggles, 6.0), None, "nothing more to give back");
        }
        // The person left it already (macOS): a toggle now would enter it again.
        let mut host = HostFullscreen::default();
        host.begin(false, true, 0.0);
        host.reported(true, 0.5);
        assert_eq!(host.end(false, true, 5.0), None);
    }

    /// The person leaving the host's full screen ends the presentation, which
    /// then has no full screen to give back.
    #[test]
    fn the_person_leaving_the_host_full_screen_ends_the_presentation() {
        for already in [false, true] {
            let mut host = HostFullscreen::default();
            host.begin(already, true, 0.0);
            host.reported(true, 0.5);
            assert_eq!(host.reported(false, 1.0), (None, true), "already {already}");
            assert_eq!(host.end(false, true, 2.0), None);
        }
    }

    /// Ended while the host window is still on its way in: on a toggling
    /// platform the window's next report decides, however late it comes
    /// within [`PENDING_LEAVE_SECONDS`]: in, and it leaves; not in (a failed
    /// enter), and nothing is given back. Elsewhere it leaves at once.
    #[test]
    fn a_presentation_ended_while_entering_leaves_once_in_however_late() {
        let mut host = HostFullscreen::default();
        host.begin(false, true, 0.0);
        assert_eq!(host.end(false, true, 0.5), None, "a toggle now would turn it round");
        let late = 0.5 + PENDING_LEAVE_SECONDS - 1.0;
        assert_eq!(host.reported(true, late), (Some(HostOp::Leave), false), "in, long after: it goes back out");
        assert_eq!(host.reported(false, late + 0.5), (None, false), "our own leave ends nothing");
        assert_eq!(host.reported(true, late + 9.0), (None, false), "and the next full screen is the person's");

        let mut host = HostFullscreen::default();
        host.begin(false, true, 0.0);
        host.end(false, true, 0.5);
        assert_eq!(host.reported(false, 1.0), (None, false), "the enter failed");
        assert_eq!(host.reported(true, 9.0), (None, false), "so a full screen after it is the person's");
        assert_eq!(host.begin(false, true, 10.0), Some(HostOp::Enter), "and a new presentation asks again");

        let mut host = HostFullscreen::default();
        host.begin(false, true, 0.0);
        assert_eq!(host.end(false, false, 0.5), Some(HostOp::Leave), "maximize and restore do not toggle");
    }

    /// A leave that waits has a limit. A presentation began while the window
    /// was still leaving the last full screen, so macOS ignored its enter,
    /// and the window's report of that leave came during the presentation.
    /// Once [`PENDING_LEAVE_SECONDS`] pass after the presentation ends, the
    /// person's own full screen is theirs, and a new presentation asks to
    /// enter again instead of waiting for an enter that never came.
    #[test]
    fn a_leave_waiting_for_an_enter_that_never_came_expires() {
        let mut host = HostFullscreen::default();
        assert_eq!(host.begin(false, true, 0.0), Some(HostOp::Enter), "macOS ignores it mid-transition");
        assert_eq!(host.reported(false, 0.8), (None, false), "the old leave's report, while it presents");
        assert_eq!(host.end(false, true, 30.0), None, "still waiting for its enter");
        let later = 30.0 + PENDING_LEAVE_SECONDS + 1.0;
        assert_eq!(host.reported(true, later), (None, false), "the person's own full screen stays");
        assert_eq!(host.end(true, true, later + 1.0), None, "and nothing of a presentation's is left");

        let mut host = HostFullscreen::default();
        host.begin(false, true, 0.0);
        host.end(false, true, 1.0);
        assert_eq!(host.begin(false, true, 1.0 + PENDING_LEAVE_SECONDS + 1.0), Some(HostOp::Enter), "it asks again");
        assert_eq!(host.reported(true, 30.0), (None, false), "in, for the new presentation");
        assert_eq!(host.end(true, true, 40.0), Some(HostOp::Leave));
    }

    /// A presentation beginning again while the enter asked for before is on
    /// its way does not toggle twice, and keeps that enter.
    #[test]
    fn a_new_presentation_keeps_an_enter_on_its_way() {
        let mut host = HostFullscreen::default();
        assert_eq!(host.begin(false, true, 0.0), Some(HostOp::Enter));
        assert_eq!(host.end(false, true, 0.2), None);
        assert_eq!(host.begin(false, true, 0.4), None, "the first enter serves it");
        assert_eq!(host.reported(true, 0.6), (None, false), "in, for the new presentation");
        assert_eq!(host.end(true, true, 3.0), Some(HostOp::Leave));
    }

    /// Hidden windows: the host window is never asked, so nothing is given back.
    #[test]
    fn hidden_windows_are_never_asked_to_go_full_screen() {
        let mut host = HostFullscreen::default();
        assert_eq!(host.begin(false, false, 0.0), None);
        assert_eq!(host.reported(true, 0.5), (None, false));
        assert_eq!(host.end(false, true, 1.0), None);
    }

    /// The presentation holds while its window is open with the instance
    /// that began it, its app runs, a desktop style is up, it covers the
    /// desk and has the focus; each of those ends it.
    #[test]
    fn a_presentation_ends_when_its_window_closes_fails_or_lets_go() {
        assert_eq!(ending_for(true, false, false, true, true), None);
        assert_eq!(ending_for(false, false, false, true, true), Some(Ending::Closed), "closed, or another instance");
        assert_eq!(ending_for(false, true, true, false, false), Some(Ending::Closed));
        assert_eq!(ending_for(true, true, false, true, true), Some(Ending::Failed), "its module panicked");
        assert_eq!(ending_for(true, false, true, true, true), Some(Ending::Style));
        assert_eq!(ending_for(true, false, false, false, true), Some(Ending::Uncovered), "⌘F");
        assert_eq!(ending_for(true, false, false, true, false), Some(Ending::FocusLeft));
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

    /// Requests that changed nothing write one line per app every few
    /// seconds, and the next line says how many went unwritten.
    #[test]
    fn a_flood_of_refusals_writes_a_line_now_and_then() {
        let mut log = QuietLog::default();
        assert_eq!(log.note("os.flood", 0.0), Some(0));
        for i in 1..=50 {
            assert_eq!(log.note("os.flood", i as f64 * 0.1), None);
        }
        assert_eq!(log.note("os.other", 1.0), Some(0), "every app has its own line");
        assert_eq!(log.note("os.flood", QUIET_LOG_SECONDS + 0.1), Some(50));
        assert_eq!(log.note("os.flood", QUIET_LOG_SECONDS + 0.2), None);
    }
}
