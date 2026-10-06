//! Native Gmail approval. The script can request a review, never approve one.
use makepad_widgets::*;
use octosense_oauth_service::host_inbox::ReviewRequest;
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
pub fn sheet(request: ReviewRequest) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let mut requests = pending().lock().unwrap_or_else(|e| e.into_inner());
    requests.retain(|_, (when, _)| when.elapsed().as_secs() < 600);
    if requests.len() >= 32 {
        return Err("Too many pending reviews; close an earlier review".into());
    }
    requests.insert(id.clone(), (Instant::now(), request));
    Ok(format!(
        "ConnectedReplyReview {{width: Fill height: Fill ticket: {}}}",
        serde_json::json!(id)
    ))
}

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*
    mod.widgets.ConnectedReplyReviewBase = #(ConnectedReplyReview::register_widget(vm))
    mod.widgets.ConnectedReplyReview = set_type_default() do mod.widgets.ConnectedReplyReviewBase {
        width: Fill height: Fill flow: Down padding: 16 spacing: 12
        show_bg: true draw_bg.color: #fff
        Label {width: Fill height: Fit text: "Review reply" draw_text.color: #x172336 draw_text.text_style.font_size: 22}
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
}

impl ConnectedReplyReview {
    fn initialize(&mut self, cx: &mut Cx) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        self.request = pending()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.ticket)
            .filter(|(when, _)| when.elapsed().as_secs() < 600)
            .map(|(_, r)| r);
        if let Some(request) = &self.request {
            let snapshot = request.snapshot();
            let reply = &snapshot["reply"];
            let text = |value: &serde_json::Value| value.as_str().unwrap_or("").to_owned();
            self.view.label(cx, ids!(account)).set_text(
                cx,
                &format!("{}\nFrom: {}", text(&snapshot["app"]), text(&reply["from"])),
            );
            self.view
                .label(cx, ids!(recipient))
                .set_text(cx, &format!("To: {}", text(&reply["to"])));
            self.view
                .label(cx, ids!(subject))
                .set_text(cx, &text(&reply["subject"]));
            self.view
                .label(cx, ids!(message))
                .set_text(cx, &text(&reply["body"]));
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
                cancel.set_text(cx, "Return to Inbox");
            }
        }
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        let approve = self.view.button(cx, ids!(approve));
        if approve.pressed(&actions) {
            self.trusted_down = makepad_platform::trusted_user_input();
        }
        if self.view.button(cx, ids!(cancel)).clicked(&actions) {
            if let Some(mut request) = self.request.take() {
                let app = request.snapshot()["app"].as_str().unwrap_or("").to_owned();
                let _ = request.cancel();
                octosense_appstore::services::close_sheet_later(&app);
            }
        }
        if approve.clicked(&actions) {
            let down = std::mem::take(&mut self.trusted_down);
            if let Some(request) = &mut self.request {
                match request.approve(down, makepad_platform::trusted_user_input()) {
                    Ok(()) => {
                        self.view
                            .label(cx, ids!(status))
                            .set_text(cx, "Sending this exact reviewed reply…");
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
