//! Host-owned Mail approval inside the expanded card. Generated L0 never draws
//! or activates this control. Platform provenance is checked on both ends of a
//! gesture; remote/instrument input may inspect the UI but cannot send mail.
use crate::shell::ui::{contains, rect, HAlign, ShellDraw};
use crate::shell::{CtrlState, ShellTokens};
use makepad_widgets::*;
use octosense_mail_service::drafts::{self, Review};
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq)]
enum Target {
    Approve,
    Cancel,
    Scroll,
}
struct Gesture {
    uid: u64,
    start: Vec2d,
    last: Vec2d,
    target: Target,
    trusted: bool,
    moved: bool,
}

struct Suggestion {
    binding: crate::mail_card::Binding,
    id: String,
    revision: u64,
}

#[derive(Clone)]
struct Retry {
    publisher: String,
    account: String,
    draft: String,
    operation: String,
    unknown: bool,
    acknowledged: bool,
}

impl Retry {
    fn from_receipt(publisher: &str, receipt: &Value) -> Option<(Self, Value)> {
        let attempt = receipt["attempts"].as_array()?.last()?;
        let status = attempt["status"].as_str()?;
        if !matches!(status, "failed_before_delivery" | "outcome_unknown")
            || receipt["status"] != status
        {
            return None;
        }
        let account = receipt["account"].as_str()?.to_owned();
        let draft = receipt["draft_id"].as_str()?.to_owned();
        let operation = attempt["operation_id"].as_str()?.to_owned();
        if publisher != "os.mail"
            || account.is_empty()
            || draft.is_empty()
            || operation.is_empty()
            || !attempt["payload"].is_object()
        {
            return None;
        }
        let retry = Self {
            publisher: publisher.into(),
            account,
            draft,
            operation,
            unknown: status == "outcome_unknown",
            acknowledged: false,
        };
        let snapshot = serde_json::json!({"publisher":publisher,"account":retry.account,"draft_id":retry.draft,
            "revision":attempt["revision"],"operation_id":retry.operation,"payload":attempt["payload"]});
        Some((retry, snapshot))
    }

    fn needs_acknowledgment(&self) -> bool {
        self.unknown && !self.acknowledged
    }
}

// Independent of the publication key: two cards can edit the same draft.
struct DraftWatch {
    publisher: String,
    account: String,
    draft: String,
    edit_generation: u64,
    checked_service_generation: Option<u64>,
}
impl DraftWatch {
    fn new(snapshot: &Value) -> Option<Self> {
        let publisher = snapshot["publisher"].as_str()?.to_owned();
        let account = snapshot["account"].as_str()?.to_owned();
        let draft = snapshot["draft_id"].as_str()?.to_owned();
        Some(Self {
            edit_generation: crate::mail_card::draft_review_generation(
                &publisher, &account, &draft,
            ),
            publisher,
            account,
            draft,
            checked_service_generation: None,
        })
    }
    fn locally_current(&self) -> bool {
        self.edit_generation
            == crate::mail_card::draft_review_generation(
                &self.publisher,
                &self.account,
                &self.draft,
            )
            && !crate::mail_card::draft_has_unsaved(&self.publisher, &self.account, &self.draft)
    }
}

// Payload equality guards exact review content, as well as the revision and
// current attempt identity. A cancelled or superseded attempt is never usable.
fn attempt_current(snapshot: &Value, draft: &Value, pending_review: bool) -> bool {
    let Some(attempt) = draft["attempts"].as_array().and_then(|a| a.last()) else {
        return false;
    };
    let valid_status = if pending_review {
        attempt["status"] == "awaiting_approval"
    } else {
        matches!(
            attempt["status"].as_str(),
            Some("failed_before_delivery" | "outcome_unknown")
        )
    };
    valid_status
        && draft["status"] == attempt["status"]
        && draft["account"] == snapshot["account"]
        && draft["draft_id"] == snapshot["draft_id"]
        && draft["revision"] == snapshot["revision"]
        && attempt["revision"] == snapshot["revision"]
        && attempt["operation_id"] == snapshot["operation_id"]
        && attempt["payload"] == snapshot["payload"]
}

#[derive(Default)]
pub struct MailReview {
    review: Option<Review>,
    suggestion: Option<Suggestion>,
    retry: Option<Retry>,
    snapshot: Value,
    draft_watch: Option<DraftWatch>,
    active: bool,
    scroll: f64,
    maximum: f64,
    viewport: Rect,
    approve: Rect,
    cancel: Rect,
    gesture: Option<Gesture>,
    worker: Option<Arc<Mutex<Option<Result<Value, String>>>>>,
    status: String,
}

impl MailReview {
    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn open(&mut self, review: Review) {
        self.replace();
        self.snapshot = review.snapshot().clone();
        self.draft_watch = DraftWatch::new(&self.snapshot);
        self.review = Some(review);
        self.active = true;
    }

    pub fn open_suggestion(&mut self, binding: &crate::mail_card::Binding) -> Result<(), String> {
        let preview = crate::mail_card::suggestion_preview(binding)?;
        let id = preview["suggestion_id"]
            .as_str()
            .ok_or("Missing suggestion identity")?
            .to_owned();
        let revision = preview["revision"]
            .as_u64()
            .ok_or("Missing suggestion revision")?;
        let body = preview["body"]
            .as_str()
            .ok_or("Missing suggestion text")?
            .to_owned();
        self.replace();
        self.snapshot = serde_json::json!({"account":binding.account, "revision":revision, "payload":{"body":body}});
        self.suggestion = Some(Suggestion {
            binding: binding.clone(),
            id,
            revision,
        });
        self.active = true;
        Ok(())
    }

    /// Reopen durable failure/uncertainty after the card or process was closed.
    /// This only displays a receipt or queues review; it never submits mail.
    pub fn open_receipt_or_review(
        &mut self,
        binding: &crate::mail_card::Binding,
    ) -> Result<(), String> {
        if !crate::mail_card::account_valid(&binding.account) {
            return Err("Account changed; reopen under its original account".into());
        }
        if crate::mail_card::card_status(binding).unsaved {
            return Err("Resolve unsaved edits before review".into());
        }
        let receipt = drafts::read(
            &crate::mail_card::host_dir()?,
            &binding.publisher,
            &binding.account,
            &binding.draft_id,
        )?;
        if let Some((retry, snapshot)) = Retry::from_receipt(&binding.publisher, &receipt) {
            self.replace();
            self.snapshot = snapshot;
            self.draft_watch = DraftWatch::new(&self.snapshot);
            self.retry = Some(retry);
            self.active = true;
            Ok(())
        } else {
            crate::mail_card::request_review(binding)
        }
    }

    fn has_action(&self) -> bool {
        self.review.is_some() || self.suggestion.is_some() || self.retry.is_some()
    }

    pub(crate) fn replace(&mut self) {
        if let Some(review) = self.review.take() {
            drafts::revoke_review(review);
        }
        *self = Self::default();
    }

    /// Cancelling a pending review invalidates its capability. After submission
    /// starts the durable service receipt remains authoritative; closing the UI
    /// does not promise to recall mail already handed to SMTP.
    pub fn close(&mut self) {
        if let Some(review) = self.review.take() {
            let _ = drafts::cancel_review(review);
        }
        *self = Self::default();
    }

    /// A publication or binding changed while its review was already visible.
    /// Remove private content and invalidate the capability, not merely its UI.
    pub fn invalidate(&mut self, reason: &str) {
        if let Some(review) = self.review.take() {
            drafts::revoke_review(review);
        }
        self.snapshot = Value::Null;
        self.draft_watch = None;
        self.suggestion = None;
        self.retry = None;
        self.gesture = None;
        self.status = reason.into();
    }

    fn account(&self) -> &str {
        self.snapshot["account"].as_str().unwrap_or("")
    }

    pub fn poll(&mut self) {
        if !self.active {
            return;
        }
        if self.snapshot.is_object() && !crate::mail_card::account_valid(self.account()) {
            self.invalidate(
                "Account changed. Reopen this card under its original account to review again.",
            );
        }

        if self.review.is_some() || self.retry.is_some() {
            let valid = (|| {
                let watch = self.draft_watch.as_mut()?;
                if !watch.locally_current() {
                    return None;
                }
                let generation = drafts::generation();
                // Check once even without a change: a queued capability may be
                // old by the time this sheet receives it. Unrelated changes do
                // not close a still-current review.
                if watch.checked_service_generation != Some(generation) {
                    let draft = drafts::read(
                        &crate::mail_card::host_dir().ok()?,
                        &watch.publisher,
                        &watch.account,
                        &watch.draft,
                    )
                    .ok()?;
                    if !attempt_current(&self.snapshot, &draft, self.review.is_some()) {
                        return None;
                    }
                    watch.checked_service_generation = Some(generation);
                }
                Some(())
            })()
            .is_some();
            if !valid {
                self.invalidate("This draft or send attempt changed. Reopen the card and review its current saved text.");
            }
        }

        let result = self
            .worker
            .as_ref()
            .and_then(|w| w.lock().unwrap_or_else(|e| e.into_inner()).take());
        if let Some(result) = result {
            self.worker = None;
            self.scroll = 0.;
            if self.snapshot.is_null() {
                return;
            } // A revoked view never resurfaces old account content.
            if let Ok(value) = &result {
                if value["account"] == self.snapshot["account"]
                    && value["draft_id"] == self.snapshot["draft_id"]
                {
                    if let Some((retry, snapshot)) = Retry::from_receipt("os.mail", value) {
                        self.retry = Some(retry);
                        self.snapshot = snapshot;
                        self.draft_watch = DraftWatch::new(&self.snapshot);
                        self.scroll = 0.;
                    }
                }
            }
            self.status = match result {
                Ok(value) => match value["status"].as_str() {
                    Some("accepted") => "SMTP accepted this message. Recipient delivery is not confirmed.".into(),
                    Some("outcome_unknown") => "Submission outcome is unknown. Check Sent before considering another send; a retry could create a duplicate.".into(),
                    Some("failed_before_delivery") => "Submission failed before acceptance. Nothing will be retried automatically.".into(),
                    _ => "The send record changed. Return to the card to inspect its current status.".into(),
                },
                Err(e) => format!("Send was not confirmed: {e}"),
            };
        }
    }

    pub fn draw(&mut self, cx: &mut Cx2d, d: &mut ShellDraw, area: Rect, tok: &ShellTokens) {
        self.poll();
        let ink = tok.notifications.surface.text;
        let width = (area.size.x - 16.).max(32.);
        d.label(
            cx,
            rect(area.pos.x + 8., area.pos.y, width, 28.),
            true,
            15.,
            ink,
            HAlign::Left,
            if self.suggestion.is_some() {
                "AI-written suggestion"
            } else if self.retry.is_some() {
                "Previous send · Mail"
            } else {
                "Review reply · Mail"
            },
        );
        self.viewport = rect(
            area.pos.x + 8.,
            area.pos.y + 32.,
            width,
            (area.size.y - 92.).max(20.),
        );
        let payload = &self.snapshot["payload"];
        let text = |name: &str| payload[name].as_str().unwrap_or("");
        let content = format!(
            "Account: {}\nFrom: {}\nTo: {}\nSubject: {}\n\nMessage\n{}\n\n{}",
            self.account(),
            text("from"),
            text("to"),
            text("subject"),
            text("body"),
            if self.status.is_empty() {
                "Only Approve & Send below authorizes this exact message."
            } else {
                ""
            }
        );
        let content = if self.suggestion.is_some() {
            format!(
                "Account: {}\nBased on saved revision {}\n\n{}\n\n{}",
                self.account(),
                self.snapshot["revision"],
                text("body"),
                if self.status.is_empty() {
                    "Use draft saves this suggestion as a new revision. It does not approve or send mail."
                } else {
                    ""
                }
            )
        } else {
            content
        };
        let content = if let Some(retry) = &self.retry {
            format!(
                "{}\nPrevious attempt: {}\n\n{}",
                if retry.unknown {
                    "OUTCOME UNKNOWN: this message may already have been accepted. Check your provider's Sent folder. Another submission could send a duplicate. I understand acknowledges this risk; it does not send mail."
                } else {
                    "The previous submission failed before acceptance. Review retry creates a new review; nothing is retried automatically."
                },
                retry.operation,
                content
            )
        } else if let Some(prior) = self.snapshot["prior_attempt"].as_str() {
            format!(
                "{}\nPrevious attempt: {}\n\n{}",
                if self.snapshot["prior_status"] == "outcome_unknown" {
                    "RETRY — POSSIBLE DUPLICATE: the previous message may already have been accepted. Approve & Send submits another message."
                } else {
                    "RETRY: this is a new submission linked to the previous attempt. Review its outcome and the exact message below before approving."
                },
                prior,
                content
            )
        } else {
            content
        };
        let content = if self.snapshot.is_null() {
            self.status.clone()
        } else if !self.status.is_empty() {
            // Outcomes and blocked-input notices must be visible even when
            // the exact message is long and the user has not scrolled down.
            format!("{}\n\n{}", self.status, content)
        } else {
            content
        };
        let lines = d.wrap_input(cx, 13., &content, width - 8.);
        let line_height = 21.;
        self.maximum = (lines.len() as f64 * line_height - self.viewport.size.y).max(0.);
        self.scroll = self.scroll.clamp(0., self.maximum);
        for (i, line) in lines.iter().enumerate() {
            let y = self.viewport.pos.y + i as f64 * line_height - self.scroll;
            // Whole lines only: host text must never draw over the approval bar.
            if y >= self.viewport.pos.y
                && y + line_height <= self.viewport.pos.y + self.viewport.size.y
            {
                d.label(
                    cx,
                    rect(self.viewport.pos.x, y, width - 8., line_height),
                    false,
                    13.,
                    ink,
                    HAlign::Left,
                    line,
                );
            }
        }
        if self.maximum > 0. {
            let track_h = self.viewport.size.y;
            let thumb = (track_h * track_h / (track_h + self.maximum)).max(16.);
            let y = self.viewport.pos.y + (track_h - thumb) * self.scroll / self.maximum;
            d.solid(
                cx,
                rect(self.viewport.pos.x + width - 3., y, 2., thumb),
                crate::shell::alpha(ink, 0.45),
            );
        }
        let y = area.pos.y + area.size.y - 48.;
        self.cancel = rect(area.pos.x + 8., y, 88., 44.);
        self.approve = rect(area.pos.x + 104., y, (width - 96.).max(80.), 44.);
        d.button(
            cx,
            self.cancel,
            tok,
            CtrlState::Normal,
            None,
            if self.has_action() { "Cancel" } else { "Back" },
            13.,
            ink,
            true,
        );
        if self.has_action() {
            d.button(
                cx,
                self.approve,
                tok,
                CtrlState::Selected,
                None,
                if self.suggestion.is_some() {
                    "Use draft"
                } else if let Some(retry) = &self.retry {
                    if retry.needs_acknowledgment() {
                        "I understand"
                    } else {
                        "Review retry"
                    }
                } else {
                    "Approve & Send"
                },
                13.,
                ink,
                true,
            );
        } else if self.worker.is_some() {
            d.label(
                cx,
                self.approve,
                false,
                13.,
                ink,
                HAlign::Center,
                "Submitting…",
            );
        }
    }

    fn target(&self, p: Vec2d) -> Option<Target> {
        if contains(self.cancel, p) {
            Some(Target::Cancel)
        } else if self.has_action() && contains(self.approve, p) {
            Some(Target::Approve)
        } else if contains(self.viewport, p) {
            Some(Target::Scroll)
        } else {
            None
        }
    }

    fn begin(&mut self, uid: u64, p: Vec2d) {
        self.gesture = self.target(p).map(|target| Gesture {
            uid,
            start: p,
            last: p,
            target,
            trusted: crate::mail_card::trusted_user_gesture(),
            moved: false,
        });
    }

    fn move_to(&mut self, uid: u64, p: Vec2d) {
        if let Some(g) = self.gesture.as_mut().filter(|g| g.uid == uid) {
            if (p - g.start).length() > 10. {
                g.moved = true;
            }
            if g.target == Target::Scroll {
                self.scroll = (self.scroll + g.last.y - p.y).clamp(0., self.maximum);
            }
            g.last = p;
        }
    }

    fn release(&mut self, uid: u64, p: Vec2d) {
        let Some(g) = self.gesture.take().filter(|g| g.uid == uid) else {
            return;
        };
        if g.moved || self.target(p) != Some(g.target) {
            return;
        }
        match g.target {
            Target::Cancel => self.close(),
            Target::Approve => {
                if let Some(retry) = self.retry.as_mut() {
                    if retry.needs_acknowledgment() {
                        retry.acknowledged = true;
                        self.gesture = None;
                        self.scroll = 0.;
                        self.status = "Duplicate risk acknowledged. Review retry opens a fresh review; sending still requires a separate physical Approve & Send.".into();
                        return;
                    }
                    let retry = retry.clone();
                    let staged = (|| {
                        if !crate::mail_card::account_valid(&retry.account) {
                            return Err("Account changed; review again under the original account"
                                .to_string());
                        }
                        drafts::retry_review(
                            &crate::mail_card::host_dir()?,
                            &retry.publisher,
                            &retry.account,
                            &retry.draft,
                            &retry.operation,
                            retry.acknowledged,
                        )
                    })();
                    match staged {
                        Ok(review) => self.open(review),
                        Err(error) => self.status = error,
                    }
                    return; // Staging a retry cannot consume the new send capability.
                }
                if let Some(suggestion) = self.suggestion.take() {
                    match crate::mail_card::accept_suggestion(
                        &suggestion.binding,
                        &suggestion.id,
                        suggestion.revision,
                    ) {
                        Ok(()) => self.close(),
                        Err(error) => {
                            self.suggestion = Some(suggestion);
                            self.status = error;
                        }
                    }
                    return; // Accepting an AI draft never enters the send executor.
                }
                if !approval_allowed(
                    g.trusted,
                    crate::mail_card::trusted_user_gesture(),
                    crate::mail_card::account_valid(self.account()),
                ) {
                    self.status = "Sending requires a real activation of this host control under the original account. Automated input cannot approve.".into();
                    self.scroll = 0.;
                    return;
                }
                let Some(review) = self.review.take() else {
                    return;
                };
                let pending = Arc::new(Mutex::new(Some(review)));
                let result = Arc::new(Mutex::new(None));
                let (pending_worker, result_worker) = (pending.clone(), result.clone());
                // Provenance has already been checked in the UI callback. The
                // service rechecks capability expiry, account, revision and claim.
                let started = std::thread::Builder::new()
                    .name("mail-approved-send".into())
                    .spawn(move || {
                        if let Some(review) = pending_worker
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .take()
                        {
                            let result = drafts::approve_and_send(review);
                            *result_worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
                            makepad_platform::SignalToUI::set_ui_signal();
                        }
                    });
                if let Err(e) = started {
                    if let Some(review) = pending.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = drafts::cancel_review(review);
                    }
                    self.status = format!("Could not start submission: {e}");
                } else {
                    self.worker = Some(result);
                    self.status = "Submitting this exact reviewed message…".into();
                }
            }
            Target::Scroll => {}
        }
    }

    /// Called exclusively by the native sheet, never from a generated NAV route.
    pub fn handle_event(&mut self, event: &Event) {
        self.poll();
        match event {
            Event::Pause | Event::Background => self.close(),
            Event::TouchUpdate(e) => {
                use makepad_platform::event::TouchState;
                if self.gesture.as_ref().is_some_and(|g| {
                    !e.touches
                        .iter()
                        .any(|t| t.uid == g.uid && t.state != TouchState::Start)
                }) {
                    self.gesture = None;
                }
                for t in &e.touches {
                    match t.state {
                        TouchState::Start if self.gesture.is_none() => self.begin(t.uid, t.abs),
                        TouchState::Move => self.move_to(t.uid, t.abs),
                        TouchState::Stop => self.release(t.uid, t.abs),
                        _ => {}
                    }
                }
            }
            Event::MouseDown(e) if e.button == MouseButton::PRIMARY => self.begin(u64::MAX, e.abs),
            Event::MouseMove(e) => self.move_to(u64::MAX, e.abs),
            Event::MouseUp(e) if e.button == MouseButton::PRIMARY => self.release(u64::MAX, e.abs),
            Event::Scroll(e) if contains(self.viewport, e.abs) => {
                self.scroll = (self.scroll + e.scroll.y).clamp(0., self.maximum);
            }
            _ => {}
        }
    }
}

fn approval_allowed(start_trusted: bool, end_trusted: bool, account_valid: bool) -> bool {
    start_trusted && end_trusted && account_valid
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn neither_injected_endpoint_nor_a_changed_account_can_approve() {
        for start in [false, true] {
            for end in [false, true] {
                for account in [false, true] {
                    assert_eq!(
                        approval_allowed(start, end, account),
                        start && end && account
                    );
                }
            }
        }
    }
    #[test]
    fn invalidation_hides_private_snapshot_and_clears_an_armed_gesture() {
        let mut panel = MailReview::default();
        panel.active = true;
        panel.snapshot = serde_json::json!({"account":"old", "payload":{"body":"private body"}});
        panel.cancel = rect(0., 0., 88., 44.);
        panel.begin(1, dvec2(10., 10.));
        assert!(panel.gesture.is_some());
        panel.invalidate("Publication replaced; review again");
        assert!(panel.snapshot.is_null());
        assert!(panel.gesture.is_none());
        assert!(panel.review.is_none());
        assert!(
            panel.active,
            "the host explains why authorization was withdrawn"
        );
    }

    #[test]
    fn stale_revision_cancelled_attempt_and_alias_replacement_cannot_remain_reviewable() {
        let review = serde_json::json!({"account":"a","draft_id":"d","revision":4,
            "operation_id":"op","payload":{"body":"saved"}});
        let current = serde_json::json!({"account":"a","draft_id":"d","revision":4,
            "status":"awaiting_approval", "attempts":[{"revision":4,"operation_id":"op",
                "status":"awaiting_approval","payload":{"body":"saved"}}]});
        assert!(attempt_current(&review, &current, true));
        for changed in [
            serde_json::json!({"revision":5}),
            serde_json::json!({"operation_id":"new-alias-attempt"}),
            serde_json::json!({"status":"cancelled"}),
            serde_json::json!({"payload":{"body":"changed"}}),
        ] {
            let mut draft = current.clone();
            for (key, value) in changed.as_object().unwrap() {
                draft["attempts"][0][key] = value.clone();
            }
            assert!(!attempt_current(&review, &draft, true), "{changed}");
        }
        let mut edited = current.clone();
        edited["revision"] = serde_json::json!(5);
        assert!(!attempt_current(&review, &edited, true));
        let mut accepted = current.clone();
        accepted["status"] = serde_json::json!("accepted");
        assert!(!attempt_current(&review, &accepted, true));
    }

    fn receipt(status: &str) -> Value {
        serde_json::json!({"account":"test-original", "draft_id":"draft-1", "status":status,
            "attempts":[{"operation_id":"old", "status":"failed_before_delivery", "payload":{}},
                {"operation_id":"latest", "revision":7, "status":status, "payload":{"body":"exact previous body"}}]})
    }

    #[test]
    fn retry_uses_only_the_last_durable_failure_and_never_an_accepted_attempt() {
        let (retry, snapshot) =
            Retry::from_receipt("os.mail", &receipt("outcome_unknown")).unwrap();
        assert_eq!(retry.operation, "latest");
        assert_eq!(snapshot["payload"]["body"], "exact previous body");
        assert!(retry.needs_acknowledgment());
        let (failed, _) =
            Retry::from_receipt("os.mail", &receipt("failed_before_delivery")).unwrap();
        assert!(!failed.needs_acknowledgment());
        assert!(Retry::from_receipt("os.mail", &receipt("accepted")).is_none());
        let mut inconsistent = receipt("outcome_unknown");
        inconsistent["status"] = serde_json::json!("sending");
        assert!(Retry::from_receipt("os.mail", &inconsistent).is_none());
        assert!(Retry::from_receipt("another-app", &receipt("outcome_unknown")).is_none());
    }

    #[test]
    fn duplicate_acknowledgment_does_not_stage_or_send_and_requires_another_gesture() {
        let (retry, snapshot) =
            Retry::from_receipt("os.mail", &receipt("outcome_unknown")).unwrap();
        let mut panel = MailReview::default();
        panel.active = true;
        panel.snapshot = snapshot;
        panel.retry = Some(retry);
        panel.approve = rect(100., 100., 160., 44.);
        panel.begin(9, dvec2(110., 110.));
        panel.release(9, dvec2(110., 110.));
        assert!(panel.retry.as_ref().unwrap().acknowledged);
        assert!(panel.review.is_none());
        assert!(panel.worker.is_none());
        assert!(panel.gesture.is_none());
        panel.release(9, dvec2(110., 110.));
        assert!(
            panel.review.is_none(),
            "a repeated release cannot stage or approve anything"
        );
        panel.invalidate("Account changed");
        assert!(
            panel.retry.is_none(),
            "acknowledgment cannot cross an invalidated binding"
        );
    }

    #[test]
    fn a_released_or_dragged_gesture_cannot_activate_a_different_target() {
        let mut panel = MailReview::default();
        panel.active = true;
        panel.cancel = rect(0., 0., 88., 44.);
        panel.viewport = rect(0., 60., 200., 180.);
        panel.begin(1, dvec2(10., 10.));
        panel.move_to(1, dvec2(10., 100.));
        panel.release(1, dvec2(10., 10.));
        assert!(panel.active, "dragging does not Cancel");
        panel.begin(1, dvec2(10., 10.));
        panel.release(1, dvec2(10., 10.));
        assert!(!panel.active);
    }
}

/// Host controls beside the model-authored card. These edit/review drafts;
/// only MailReview's separate trusted Approve & Send path submits mail.
#[derive(Default)]
pub struct MailToolbar {
    area: Rect,
    buttons: [Rect; 3],
    enabled: [bool; 3],
    pressed: Option<(u64, usize, Vec2d)>,
    error: String,
}

impl MailToolbar {
    pub const HEIGHT: f64 = 100.;

    pub fn draw(
        &mut self,
        cx: &mut Cx2d,
        d: &mut ShellDraw,
        area: Rect,
        tok: &ShellTokens,
        binding: &crate::mail_card::Binding,
    ) {
        self.area = area;
        let status = crate::mail_card::card_status(binding);
        let account_valid = crate::mail_card::account_valid(&binding.account);
        self.enabled = [
            status.suggestion && account_valid,
            status.unsaved && account_valid,
            !status.unsaved && account_valid,
        ];
        let ink = tok.notifications.surface.text;
        let text = if self.error.is_empty() {
            &status.text
        } else {
            &self.error
        };
        let lines = d.wrap_lines(cx, false, 12., text, area.size.x - 12., false);
        for (i, line) in lines.iter().take(2).enumerate() {
            d.label_elided(
                cx,
                rect(
                    area.pos.x + 6.,
                    area.pos.y + 4. + i as f64 * 20.,
                    area.size.x - 12.,
                    20.,
                ),
                false,
                12.,
                ink,
                HAlign::Left,
                line,
            );
        }
        let width = (area.size.x - 20.) / 3.;
        for (i, label) in ["Suggestion", "Restore saved", "Review reply"]
            .iter()
            .enumerate()
        {
            let r = rect(
                area.pos.x + 4. + i as f64 * (width + 6.),
                area.pos.y + 48.,
                width,
                44.,
            );
            self.buttons[i] = r;
            d.button(
                cx,
                r,
                tok,
                if self.enabled[i] {
                    CtrlState::Normal
                } else {
                    CtrlState::Disabled
                },
                None,
                label,
                11.,
                ink,
                true,
            );
        }
    }

    fn press(&mut self, uid: u64, point: Vec2d) -> bool {
        self.pressed = self
            .buttons
            .iter()
            .enumerate()
            .find(|(i, r)| self.enabled[*i] && contains(**r, point))
            .map(|(i, _)| (uid, i, point));
        contains(self.area, point)
    }

    fn release(
        &mut self,
        uid: u64,
        point: Vec2d,
        binding: &crate::mail_card::Binding,
        review: &mut MailReview,
    ) -> bool {
        let Some((held, index, start)) = self.pressed.take() else {
            return contains(self.area, point);
        };
        if held != uid
            || (point - start).length() > 10.
            || !self.enabled[index]
            || !contains(self.buttons[index], point)
        {
            return true;
        }
        let result = match index {
            0 => review.open_suggestion(binding),
            1 => crate::mail_card::discard_unsaved(binding),
            _ => review.open_receipt_or_review(binding),
        };
        self.error = result.err().unwrap_or_default();
        true
    }

    pub fn handle_event(
        &mut self,
        event: &Event,
        binding: &crate::mail_card::Binding,
        review: &mut MailReview,
    ) -> bool {
        match event {
            Event::Pause | Event::Background => {
                self.pressed = None;
                false
            }
            Event::TouchUpdate(e) => {
                use makepad_platform::event::TouchState;
                if self.pressed.as_ref().is_some_and(|(uid, _, _)| {
                    !e.touches
                        .iter()
                        .any(|t| t.uid == *uid && t.state != TouchState::Start)
                }) {
                    self.pressed = None;
                }
                let mut consumed = self.pressed.is_some();
                for t in &e.touches {
                    match t.state {
                        TouchState::Start if self.pressed.is_none() => {
                            consumed |= self.press(t.uid, t.abs)
                        }
                        TouchState::Move => {
                            if self.pressed.as_ref().is_some_and(|(uid, _, p)| {
                                *uid == t.uid && (t.abs - *p).length() > 10.
                            }) {
                                self.pressed = None;
                            }
                        }
                        TouchState::Stop => consumed |= self.release(t.uid, t.abs, binding, review),
                        _ => {}
                    }
                }
                consumed
            }
            Event::MouseDown(e) if e.button == MouseButton::PRIMARY => self.press(u64::MAX, e.abs),
            Event::MouseUp(e) if e.button == MouseButton::PRIMARY => {
                self.release(u64::MAX, e.abs, binding, review)
            }
            Event::Scroll(e) => contains(self.area, e.abs),
            _ => false,
        }
    }
}
