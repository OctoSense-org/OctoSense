//! A script app copies text at the person's click (`clipboard.write`), as a
//! web page may:
//!
//! ```text
//! host.request("clipboard.write", {text: link}, fn(r) { … })
//! ```
//!
//! It takes the same activation as `window.fullscreen` (app_fullscreen.rs): a
//! press on the app's own content in its window in the last few seconds, and
//! never an agent's tool call or a callback of one. A phone has one app in
//! front, so there only the second holds. No app can read the clipboard: that
//! stays a person's paste into a field.

use serde_json::{json, Value};

/// The host-service family: `clipboard.write`.
pub const FAMILY: &str = "clipboard";
/// More than anyone copies with a click, and small enough to hold.
pub const MAX_TEXT_BYTES: usize = 1 << 20;
/// Requests waiting for the shell's next turn, from every app together.
pub const MAX_QUEUED: usize = 16;

pub const NOT_A_WINDOW: &str = "not_a_window: only the app's own window can copy";
pub const AGENT_CALL: &str =
    "agent_call: not from an agent's tool call, or from a callback of one: copy from the person's tap or click";
pub const GESTURE_REQUIRED: &str = "gesture_required: copy from the person's tap or click in the app's window";
pub const BUSY: &str = "busy: too many clipboard requests are waiting";

/// A `clipboard.*` request, checked before it reaches the shell's turn. The
/// answer is the text.
pub fn admit(service: &str, from_sheet: bool, args: &Value) -> Result<String, String> {
    if service != "clipboard.write" {
        return Err(format!("method_unavailable: the clipboard service has write {{text}} only, not {service:?}"));
    }
    if from_sheet {
        return Err(NOT_A_WINDOW.into());
    }
    let wrong = || "invalid_arguments: expected {text: a string}".to_string();
    let object = args.as_object().ok_or_else(wrong)?;
    if object.len() != 1 {
        return Err(wrong());
    }
    let text = object.get("text").and_then(Value::as_str).ok_or_else(wrong)?;
    if text.len() > MAX_TEXT_BYTES {
        return Err(format!("invalid_arguments: text is over {MAX_TEXT_BYTES} bytes"));
    }
    Ok(text.to_string())
}

/// What the shell's turn knows about the request.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Facts {
    pub mobile: bool,
    pub agent_call: bool,
    /// The app's own window, pressed within the activation's few seconds.
    pub pressed: bool,
}

pub fn decide(facts: Facts) -> Result<Value, String> {
    if facts.agent_call {
        return Err(AGENT_CALL.into());
    }
    if !facts.mobile && !facts.pressed {
        return Err(GESTURE_REQUIRED.into());
    }
    Ok(json!({"copied": true}))
}

#[cfg(any(feature = "app-hub", native_mobile))]
pub use service::{register, take_requests};

#[cfg(any(feature = "app-hub", native_mobile))]
mod service {
    use super::*;
    use makepad_widgets::makepad_platform::SignalToUI;
    use octosense_appstore::services::{self, HostService, Replier, ServiceCall, ServiceHost};
    use std::sync::Mutex;

    /// A request for the shell's next turn (`App::serve_clipboard_requests`).
    pub struct Request {
        pub app: String,
        pub isolate: usize,
        pub text: String,
        /// `!ServiceCall::may_prompt`: an agent's tool call, or a callback of one.
        pub agent_call: bool,
        pub reply: Replier,
    }

    static QUEUE: Mutex<Vec<Request>> = Mutex::new(Vec::new());

    /// Offer `clipboard.write` to every app, as `window.fullscreen` is.
    pub fn register() {
        services::register_host_service(Box::new(ClipboardService));
    }

    pub fn take_requests() -> Vec<Request> {
        std::mem::take(&mut *QUEUE.lock().unwrap_or_else(|e| e.into_inner()))
    }

    struct ClipboardService;

    impl HostService for ClipboardService {
        fn family(&self) -> &'static str {
            FAMILY
        }

        fn call(&mut self, call: ServiceCall, reply: Replier, _host: &mut dyn ServiceHost) {
            match admit(&call.service, call.from_sheet, &call.args) {
                Ok(text) => {
                    let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
                    if queue.len() >= MAX_QUEUED {
                        drop(queue);
                        reply.send(Err(BUSY.into()));
                        return;
                    }
                    queue.push(Request {
                        app: call.app_id,
                        isolate: reply.isolate_key(),
                        text,
                        agent_call: !call.may_prompt,
                        reply,
                    });
                    drop(queue);
                    // The shell answers at the end of this event, or the next.
                    SignalToUI::set_ui_signal();
                }
                Err(refusal) => reply.send(Err(refusal)),
            }
        }
    }
}

#[cfg(any(feature = "app-hub", native_mobile))]
use crate::*;

#[cfg(any(feature = "app-hub", native_mobile))]
impl App {
    /// The `clipboard.write` requests waiting, answered: after the event that
    /// pumped them, on the UI thread.
    pub(crate) fn serve_clipboard_requests(&mut self, cx: &mut Cx) {
        for request in take_requests() {
            if !request.reply.is_pending() {
                continue;
            }
            let mobile = self.state.as_ref().is_none_or(|s| s.style.target.mobile());
            let window = if mobile { None } else { self.window_of(cx, &request.app, request.isolate) };
            let pressed = window.is_some_and(|client| self.presenter.pressed(client, host::now()));
            let result = decide(Facts { mobile, agent_call: request.agent_call, pressed });
            if result.is_ok() {
                cx.copy_to_clipboard(&request.text);
            }
            request.reply.send(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_short_text_from_the_apps_own_surface_is_admitted() {
        assert_eq!(admit("clipboard.write", false, &json!({"text": "a link"})), Ok("a link".to_string()));
        assert!(admit("clipboard.read", false, &json!({})).unwrap_err().starts_with("method_unavailable:"));
        assert_eq!(admit("clipboard.write", true, &json!({"text": "a link"})), Err(NOT_A_WINDOW.to_string()));
        assert!(admit("clipboard.write", false, &json!({})).unwrap_err().starts_with("invalid_arguments:"));
        assert!(admit("clipboard.write", false, &json!({"text": 3})).unwrap_err().starts_with("invalid_arguments:"));
        let long = "x".repeat(MAX_TEXT_BYTES + 1);
        assert!(admit("clipboard.write", false, &json!({"text": long})).unwrap_err().starts_with("invalid_arguments:"));
    }

    #[test]
    fn a_copy_takes_the_persons_press_and_never_an_agent() {
        let pressed = Facts { pressed: true, ..Facts::default() };
        assert!(decide(pressed).is_ok());
        assert_eq!(decide(Facts::default()), Err(GESTURE_REQUIRED.to_string()));
        assert_eq!(decide(Facts { agent_call: true, ..pressed }), Err(AGENT_CALL.to_string()));
        assert!(decide(Facts { mobile: true, ..Facts::default() }).is_ok());
        assert_eq!(decide(Facts { mobile: true, agent_call: true, ..Facts::default() }), Err(AGENT_CALL.to_string()));
    }
}
