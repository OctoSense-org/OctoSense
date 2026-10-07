//! Native Gmail, GitHub and Calendar approval. The script can request a review, never approve one.
use makepad_widgets::*;
use octosense_oauth_service::{host_api, host_inbox};

enum ReviewRequest {
    Mail(host_inbox::ReviewRequest),
    Save(host_api::ReviewRequest),
}
impl ReviewRequest {
    fn approve(&mut self, down: bool, up: bool) -> Result<(), String> {
        match self {
            Self::Mail(r) => r.approve(down, up),
            Self::Save(r) => r.approve(down, up),
        }
    }
    fn result(&self) -> Option<Result<serde_json::Value, String>> {
        match self {
            Self::Mail(r) => r.result(),
            Self::Save(r) => r.result(),
        }
    }
    fn cancel(&mut self) -> Result<(), String> {
        match self {
            Self::Mail(r) => r.cancel(),
            Self::Save(r) => r.cancel(),
        }
    }
    fn close_request(&self) -> (String, String) {
        match self {
            Self::Mail(_) => ("gmail.sheet.close".into(), String::new()),
            Self::Save(r) => r.close_request(),
        }
    }
}
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::Instant,
};

type Pending = HashMap<String, (Instant, ReviewRequest)>;
fn pending() -> &'static Mutex<Pending> {
    static PENDING: OnceLock<Mutex<Pending>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register() {
    widget_async::register_splash_isolate_mod(|vm| {
        script_mod(vm);
        script_eval!(vm, {mod.prelude.widgets.ConnectedReplyReview = mod.widgets.ConnectedReplyReview});
    });
}

/// The owning service mounts this source in its host sheet, outside the app.
pub fn sheet(request: host_inbox::ReviewRequest) -> Result<String, String> {
    mount(ReviewRequest::Mail(request))
}
pub fn connector_sheet(request: host_api::ReviewRequest) -> Result<String, String> {
    mount(ReviewRequest::Save(request))
}
fn mount(request: ReviewRequest) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let mut requests = pending().lock().unwrap_or_else(|e| e.into_inner());
    requests.retain(|_, (when, _)| when.elapsed().as_secs() < 600);
    if requests.len() >= 32 {
        return Err("Too many pending reviews; close an earlier review".into());
    }
    let (close_service, close_ticket) = request.close_request();
    requests.insert(id.clone(), (Instant::now(), request));
    Ok(format!(
        "ConnectedReplyReview {{width: Fill height: Fill ticket: {} close_service: {} close_ticket: {}}}",
        serde_json::json!(id),
        serde_json::json!(close_service), serde_json::json!(close_ticket)
    ))
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*
    mod.widgets.ConnectedReplyReviewBase = #(ConnectedReplyReview::register_widget(vm))
    mod.widgets.ConnectedReplyReview = set_type_default() do mod.widgets.ConnectedReplyReviewBase {
        width: Fill height: Fill flow: Down padding: 16 spacing: 12
        // This native composite dereferences View, not SolidView. View's
        // default DrawQuad shader is transparent even when color is set.
        // The host review must cover the untrusted app beneath it.
        show_bg: true
        draw_bg +: {
            color: instance(#fff)
            pixel: fn() { return Pal.premul(self.color) }
        }
        heading := Label {width: Fill height: Fit text: "Review reply" draw_text.color: #x172336 draw_text.text_style.font_size: 22}
        ScrollYView {width: Fill height: Fill flow: Down spacing: 12
            account := Label {width: Fill height: Fit draw_text.color: #x526071 draw_text.wrap: Words}
            recipient := Label {width: Fill height: Fit draw_text.color: #x172336 draw_text.wrap: Words}
            subject := Label {width: Fill height: Fit draw_text.color: #x172336 draw_text.wrap: Words draw_text.text_style.font_size: 16}
            message := Label {width: Fill height: Fit draw_text.color: #x172336 draw_text.wrap: Words draw_text.text_style.font_size: 15}
        }
        status := Label {width: Fill height: Fit text: "Check the recipient and the complete reply before sending." draw_text.color: #x526071 draw_text.wrap: Words}
        View {width: Fill height: 48 spacing: 8
            cancel := ButtonFlat {width: Fill height: Fill text: "Back to editing"}
            approve := Button {width: Fill height: Fill text: "Approve & Send"}
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct ConnectedReplyReview {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,
    #[live]
    ticket: String,
    #[rust]
    request: Option<ReviewRequest>,
    #[rust]
    initialized: bool,
    #[rust]
    trusted_down: bool,
    #[rust]
    finished: bool,
    #[rust]
    is_save: bool,
    #[live]
    close_service: String,
    #[live]
    close_ticket: String,
}

impl ConnectedReplyReview {
    fn close_sheet(&self, cx: &mut Cx) {
        if makepad_widgets::splash_policy::is_enforced(self.source.heap_key()) {
            return;
        }
        // Close only this originating host sheet. The same app can also
        // be resident in its ordinary window or another Glance workspace.
        if !matches!(
            self.close_service.as_str(),
            "gmail.sheet.close" | "github.sheet.cancel" | "gcalendar.sheet.cancel"
        ) {
            return;
        }
        if let Some(owner) = cx.script_ref_vm_id(&self.source) {
            cx.with_script_vm_id(owner, |vm| {
                vm.eval(ScriptMod {
                    cargo_manifest_path: env!("CARGO_MANIFEST_DIR").into(),
                    module_path: "native_review_close".into(),
                    file: "native_review_close.splash".into(),
                    line: 0,
                    column: 0,
                    code: format!(
                        "mod.host.request({}, {{ticket: {}}}, nil)",
                        serde_json::json!(self.close_service),
                        serde_json::json!(self.close_ticket)
                    ),
                    values: vec![],
                });
            });
        }
    }
    fn initialize(&mut self, cx: &mut Cx) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        if makepad_widgets::splash_policy::is_enforced(self.source.heap_key()) {
            self.view.button(cx, ids!(approve)).set_enabled(cx, false);
            return;
        }
        self.request = pending()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.ticket)
            .filter(|(when, _)| when.elapsed().as_secs() < 600)
            .map(|(_, r)| r);
        if let Some(request) = &self.request {
            (self.close_service, self.close_ticket) = request.close_request();
            let text = |value: &serde_json::Value| value.as_str().unwrap_or("").to_owned();
            let (heading, account, details, subject, body) = match request {
                ReviewRequest::Mail(request) => {
                    let snapshot = request.snapshot();
                    let reply = &snapshot["reply"];
                    (
                        "Review reply".to_owned(),
                        format!("{}\nFrom: {}", text(&snapshot["app"]), text(&reply["from"])),
                        format!("To: {}", text(&reply["to"])),
                        text(&reply["subject"]),
                        text(&reply["body"]),
                    )
                }
                ReviewRequest::Save(request) => {
                    self.is_save = true;
                    let snapshot = request.snapshot();
                    self.view
                        .button(cx, ids!(approve))
                        .set_text(cx, "Approve & Save");
                    self.view.label(cx, ids!(status)).set_text(
                        cx,
                        "Review the account, destination and exact content before saving.",
                    );
                    (
                        text(&snapshot["title"]),
                        format!(
                            "{}\nAccount: {}",
                            text(&snapshot["app"]),
                            text(&snapshot["account"])
                        ),
                        text(&snapshot["details"]),
                        String::new(),
                        text(&snapshot["body"]),
                    )
                }
            };
            self.view.label(cx, ids!(heading)).set_text(cx, &heading);
            self.view.label(cx, ids!(account)).set_text(cx, &account);
            self.view.label(cx, ids!(recipient)).set_text(cx, &details);
            self.view.label(cx, ids!(subject)).set_text(cx, &subject);
            self.view
                .label(cx, ids!(subject))
                .set_visible(cx, !subject.is_empty());
            self.view.label(cx, ids!(message)).set_text(cx, &body);
        } else {
            self.view.label(cx, ids!(status)).set_text(
                cx,
                "This review expired. Return to the app and review your saved draft again.",
            );
            self.view.button(cx, ids!(approve)).set_enabled(cx, false);
        }
    }
}
impl Widget for ConnectedReplyReview {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.initialize(cx);
        self.view.draw_walk(cx, scope, walk)
    }
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.initialize(cx);
        if matches!(event, Event::Pause | Event::Background) {
            self.trusted_down = false;
            if let Some(mut request) = self.request.take() {
                let _ = request.cancel();
            }
            return;
        }
        if !self.finished {
            if let Some(result) = self.request.as_ref().and_then(ReviewRequest::result) {
                self.finished = true;
                let message = match result {
                    Ok(_) if self.is_save => {
                        "The reviewed change was saved. Return to the app.".to_owned()
                    }
                    Ok(value) if value["status"] == "accepted" => {
                        "Gmail accepted the reply. Return to your Inbox.".to_owned()
                    }
                    Ok(value) => format!(
                        "Submission status: {}. Check the saved receipt before trying again.",
                        value["status"].as_str().unwrap_or("unknown")
                    ),
                    Err(error) => error,
                };
                self.view.label(cx, ids!(status)).set_text(cx, &message);
                self.view.button(cx, ids!(approve)).set_enabled(cx, false);
                let cancel = self.view.button(cx, ids!(cancel));
                cancel.set_enabled(cx, true);
                cancel.set_text(
                    cx,
                    if self.is_save {
                        "Return to app"
                    } else {
                        "Return to Inbox"
                    },
                );
            }
        }
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        let approve = self.view.button(cx, ids!(approve));
        if approve.pressed(&actions) {
            self.trusted_down = makepad_platform::trusted_user_input();
        }
        if self.view.button(cx, ids!(cancel)).clicked(&actions) {
            if let Some(mut request) = self.request.take() {
                let _ = request.cancel();
            }
            self.close_sheet(cx);
        }
        if approve.clicked(&actions) {
            let down = std::mem::take(&mut self.trusted_down);
            if let Some(request) = &mut self.request {
                match request.approve(down, makepad_platform::trusted_user_input()) {
                    Ok(()) => {
                        self.view.label(cx, ids!(status)).set_text(
                            cx,
                            if self.is_save {
                                "Saving this exact reviewed version…"
                            } else {
                                "Sending this exact reviewed reply…"
                            },
                        );
                        approve.set_enabled(cx, false);
                        self.view.button(cx, ids!(cancel)).set_enabled(cx, false);
                    }
                    Err(error) => self.view.label(cx, ids!(status)).set_text(cx, &error),
                }
            }
        } else if approve.released(&actions) {
            self.trusted_down = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_review_close_is_scoped_and_contained_copies_cannot_use_it() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        register();
        let mut make = |contained| {
            let mut splash = cx.with_vm(|vm| {
                makepad_widgets::script_mod(vm);
                script_mod(vm);
                let value = vm.eval(script! { use mod.widgets.* Splash {} });
                Splash::script_from_value(vm, value)
            });
            if contained {
                splash.set_policy(&mut cx, Some(vec![]), None);
            }
            splash.set_text(
                &mut cx,
                r#"review := ConnectedReplyReview {
                ticket: "missing-native-capability"
                close_service: "github.sheet.cancel"
                close_ticket: "synthetic-ticket"
            }"#,
            );
            splash
        };
        let host = make(false);
        let app = make(true);
        for (splash, contained) in [(&host, false), (&app, true)] {
            let widget = splash
                .view
                .children
                .iter()
                .find(|(id, _)| *id == id!(review))
                .unwrap()
                .1
                .clone();
            let mut review = widget.borrow_mut::<ConnectedReplyReview>().unwrap();
            review.initialize(&mut cx);
            assert!(
                review.request.is_none(),
                "live fields cannot forge a native capability"
            );
            review.close_sheet(&mut cx);
            let requests = makepad_widgets::splash_host::take_splash_host_requests_for(&[review
                .source
                .heap_key()]);
            if contained {
                assert!(requests.is_empty());
            } else {
                assert_eq!(requests.len(), 1);
                assert_eq!(requests[0].service, "github.sheet.cancel");
            }
        }
    }
}
