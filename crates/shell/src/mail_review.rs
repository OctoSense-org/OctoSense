//! Host-owned Mail approval inside the expanded card. Generated L0 never draws
//! or activates this control. Platform provenance is checked on both ends of a
//! gesture; remote/instrument input may inspect the UI but cannot send mail.
use crate::approvals::mac_kit::{self as kit, Face};
use crate::octosense::style::{AppIconDraw, DesktopStyle};
use crate::shell::ui::{contains, rect, HAlign, ShellDraw};
use crate::shell::{CtrlState, ShellTokens};
use makepad_widgets::*;
use octosense_mail_service::drafts::{self, Review};
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// A Mac's sheet: the room under the message for its row of buttons.
const MAC_BUTTONS: f64 = 44.;
/// A Mac's review is the alert macOS has drawn since 11, as the shell's own
/// alerts (`approvals/mac_view.rs`) and the apps' (`UiAlertPanel`) are: Mail's
/// icon on top, the title and what it means centred under it, the exact message
/// in a box of its own that scrolls, then the buttons, each as wide as the other:
/// two side by side, the default on the right; three one over another, the
/// default on top. `glance_sheet.rs` gives it its width and margins.
const MAC_ICON: f64 = 64.;
/// The message's box: its margins inside, and the least of it a short window keeps.
const MAC_BOX_X: f64 = 8.;
const MAC_BOX_Y: f64 = 6.;
const MAC_BOX_MIN: f64 = 52.;
/// Between two paragraphs of what the alert says.
const MAC_GAP: f64 = 6.;

thread_local! {
    /// Mail's icon on a Mac's review, as the Dock draws it: kept across
    /// reviews, so its art is loaded once.
    static MAC_ICON_DRAW: std::cell::RefCell<AppIconDraw> = std::cell::RefCell::new(AppIconDraw::default());
}

/// Words wrapped to `w`, paragraph by paragraph (an empty line between two),
/// never leaving one word alone on a paragraph's last line: it is wrapped a
/// little narrower until two words share that line.
fn mac_wrapped(d: &mut ShellDraw, cx: &mut Cx2d, bold: bool, px: f64, text: &str, w: f64) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        if para.trim().is_empty() {
            if out.last().is_some_and(|l: &String| !l.is_empty()) {
                out.push(String::new());
            }
            continue;
        }
        let mut lines = kit::wrapped(d, cx, bold, px, para, w);
        let mut narrower = w;
        while lines.len() > 1 && !lines.last().is_some_and(|l| l.trim().contains(' ')) && narrower > w * 0.7 {
            narrower -= 8.;
            lines = kit::wrapped(d, cx, bold, px, para, narrower);
        }
        out.extend(lines);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
enum Target {
    Approve,
    Cancel,
    /// A Mac's alert's third button, at the leading edge: on a send whose
    /// outcome is unknown, "I Understand" and then "Show Sent".
    Alternate,
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
            "revision":attempt["revision"],"operation_id":retry.operation,"source_message":receipt["source_message"],"payload":attempt["payload"]});
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
    alternate: Rect,
    gesture: Option<Gesture>,
    worker: Option<Arc<Mutex<Option<Result<Value, String>>>>>,
    status: String,
    /// A Mac's review knows the server took the message: Mail's own window says so.
    sent: bool,
    /// Frames asked for after a Mac's review is first drawn: Mail's icon draws
    /// its art from the frame after it is first asked for.
    icon_frames: u8,
    /// The Mac's form, decided by a test rather than the shell's style.
    #[cfg(test)]
    mac_form: Option<bool>,
}

impl MailReview {
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// On a Mac: the message went, and there's nothing left to press here.
    pub fn sent(&self) -> bool {
        self.sent
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

    fn mac(&self) -> bool {
        #[cfg(test)]
        if let Some(mac) = self.mac_form {
            return mac;
        }
        crate::shell::menu::MAC_KEYS.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// On a Mac, a send whose outcome is unknown offers Sent, where the
    /// person can see whether it went: the leading button, or the default
    /// until the risk is acknowledged.
    fn offers_sent(&self) -> bool {
        self.mac() && self.retry.as_ref().is_some_and(|r| r.unknown)
    }

    /// The person accepts that sending again may send a second copy. This
    /// stages nothing and sends nothing; the next step is a fresh review.
    fn acknowledge(&mut self) {
        let Some(retry) = self.retry.as_mut() else {
            return;
        };
        retry.acknowledged = true;
        self.gesture = None;
        self.scroll = 0.;
        self.status = if self.mac() {
            "Send Again opens a new review. Nothing goes until you approve it.".into()
        } else {
            "Review retry opens a new review. Nothing is sent until you approve it.".into()
        };
    }

    /// Show Sent: Mail's window opens its Sent mailbox, and this sheet goes.
    /// It sends nothing, and the attempt stays as uncertain as it was.
    fn show_sent(&mut self) {
        if let Some(retry) = &self.retry {
            drafts::request_show_sent(&retry.publisher, &retry.account, &retry.draft);
        }
        self.close();
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
                "The Mail account changed. Open this message under its original account to review it again.",
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
            let mac = crate::shell::menu::MAC_KEYS.load(std::sync::atomic::Ordering::Relaxed);
            self.status = match result {
                Ok(value) => match value["status"].as_str() {
                    Some("accepted") if mac => {
                        self.sent = true;
                        "The mail server accepted this message.".into()
                    }
                    // A Mac's sheet says these in its title and its first line.
                    Some("outcome_unknown" | "failed_before_delivery") if mac && self.retry.is_some() => String::new(),
                    Some("accepted") => "SMTP accepted this message. Recipient delivery is not confirmed.".into(),
                    Some("outcome_unknown") => "Submission outcome is unknown. Check Sent before considering another send; a retry could create a duplicate.".into(),
                    Some("failed_before_delivery") => "Submission failed before acceptance. Nothing will be retried automatically.".into(),
                    _ => "The send record changed. Return to the card to inspect its current status.".into(),
                },
                Err(_) => "Mail couldn't confirm whether this was sent. Check Sent before trying again; another send could reach them twice.".into(),
            };
        }
    }

    /// What's being reviewed, as a Mac's sheet titles it.
    pub fn mac_title(&self) -> &'static str {
        if self.suggestion.is_some() {
            "AI-Written Suggestion"
        } else if let Some(retry) = &self.retry {
            if retry.unknown { "Mail Can't Tell Whether This Was Sent" } else { "This Message Wasn't Sent" }
        } else if self.sent {
            "Message Sent"
        } else if self.is_reply() {
            "Review Your Reply"
        } else {
            "Review Your Message"
        }
    }

    fn is_reply(&self) -> bool {
        self.source_message_present()
            || self.snapshot["payload"]["in_reply_to"]
                .as_str()
                .is_some_and(|id| !id.trim().is_empty())
    }

    fn source_message_present(&self) -> bool {
        self.snapshot["source_message"]["message"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    }

    fn missing_reply_reference(&self) -> bool {
        self.source_message_present()
            && self.snapshot["payload"]["in_reply_to"]
                .as_str()
                .is_none_or(|id| id.trim().is_empty())
    }

    /// How tall a Mac's alert `width` wide (inside its margins) is for all it
    /// holds: Mail's icon, the title and what it says, the message's box and
    /// the buttons. A window too short for it keeps the box shorter, and the
    /// message scrolls there.
    pub fn mac_card_height(&self, cx: &mut Cx2d, d: &mut ShellDraw, width: f64) -> f64 {
        let (said, message) = self.mac_parts();
        let mut h = MAC_ICON + 10. + mac_wrapped(d, cx, true, kit::TEXT, self.mac_title(), width).len() as f64 * kit::TEXT_LINE;
        let said = mac_wrapped(d, cx, false, kit::NOTE, &said, width);
        if !said.is_empty() {
            h += 4. + said.iter().map(|l| if l.is_empty() { MAC_GAP } else { kit::NOTE_LINE }).sum::<f64>();
        }
        if !message.is_empty() {
            let lines = kit::wrapped_exact(d, cx, false, kit::TEXT, &message, width - MAC_BOX_X * 2. - 6.);
            h += 10. + lines.len() as f64 * kit::TEXT_LINE + MAC_BOX_Y * 2.;
        }
        let rows = self.mac_button_rows();
        h + 16. + rows * kit::CONTROL_H + (rows - 1.) * 8.
    }

    /// A Mac's alert stands its three buttons one over another; one or two
    /// share a row.
    fn mac_button_rows(&self) -> f64 {
        if self.has_action() && self.offers_sent() { 3. } else { 1. }
    }

    /// What a Mac's alert says under its title, how things stand and what a
    /// press does (the first), and the exact message its box shows (the
    /// second): every word `content()` gives the other forms.
    fn mac_parts(&self) -> (String, String) {
        let payload = &self.snapshot["payload"];
        let text = |name: &str| payload[name].as_str().unwrap_or("");
        let mut said: Vec<String> = Vec::new();
        if !self.status.is_empty() {
            said.push(self.status.clone());
        }
        if self.snapshot.is_null() {
            return (said.join("\n\n"), String::new());
        }
        if let Some(retry) = &self.retry {
            said.push(if retry.unknown {
                "The connection ended while it was sending, so it may already have gone. Check Sent first: sending it again could reach them twice."
            } else {
                "The mail server didn't take it, so nothing was sent. Nothing is tried again until you say so."
            }.into());
        } else if self.snapshot["prior_attempt"].is_string() && self.snapshot["prior_status"] == "outcome_unknown" {
            said.push("This message may already have gone. Approve & Send sends it again, and it could reach them twice.".into());
        }
        if self.suggestion.is_some() {
            if self.status.is_empty() {
                said.push("Use draft saves this suggestion as a new revision. It does not approve or send mail.".into());
            }
            let message = format!(
                "Account: {}\nBased on saved revision {}\n\n{}",
                self.account(),
                self.snapshot["revision"],
                text("body")
            );
            return (said.join("\n\n"), message);
        }
        if self.missing_reply_reference() {
            said.push("Conversation notice: Mail can't link this reply to the original message. It may appear as a new conversation.".into());
        }
        if self.status.is_empty() && self.retry.is_none() {
            said.push(if self.snapshot["prior_status"] == "outcome_unknown" {
                "Approve & Send sends this exact message."
            } else {
                "Nothing has been sent. Approve & Send sends this exact message."
            }.into());
        }
        let copied = text("cc");
        let copied = if copied.is_empty() { String::new() } else { format!("Cc: {copied}\n") };
        let message = format!(
            "From: {}\nTo: {}\n{}Subject: {}\n\n{}",
            text("from"),
            text("to"),
            copied,
            text("subject"),
            text("body")
        );
        (said.join("\n\n"), message)
    }

    /// A Mac's alert in `area`, inside its margins. The buttons' hit rects are
    /// the rects drawn here, and nothing here changes what a press does.
    fn draw_mac(&mut self, cx: &mut Cx2d, d: &mut ShellDraw, area: Rect, tok: &ShellTokens) {
        let ink = tok.notifications.surface.text;
        let k = kit::mac_ink(ink.x + ink.y + ink.z >= 1.5);
        let (x, w) = (area.pos.x, area.size.x);
        let icon = rect(x + ((w - MAC_ICON) * 0.5).floor(), area.pos.y, MAC_ICON, MAC_ICON);
        MAC_ICON_DRAW.with(|icons| icons.borrow_mut().draw(cx, "mail", DesktopStyle::Macos, icon, 1.0, k.ink));
        if self.icon_frames < 2 {
            self.icon_frames += 1;
            cx.new_next_frame();
        }
        let mut y = icon.pos.y + MAC_ICON + 10.;
        for line in mac_wrapped(d, cx, true, kit::TEXT, self.mac_title(), w) {
            d.label(cx, rect(x, y, w, kit::TEXT_LINE), true, kit::TEXT, k.ink, HAlign::Center, &line);
            y += kit::TEXT_LINE;
        }
        let (said, message) = self.mac_parts();
        let said = mac_wrapped(d, cx, false, kit::NOTE, &said, w);
        if !said.is_empty() {
            y += 4.;
            for line in &said {
                if line.is_empty() {
                    y += MAC_GAP;
                    continue;
                }
                d.label(cx, rect(x, y, w, kit::NOTE_LINE), false, kit::NOTE, k.ink, HAlign::Center, line);
                y += kit::NOTE_LINE;
            }
        }
        let rows = self.mac_button_rows();
        let top = area.pos.y + area.size.y - rows * kit::CONTROL_H - (rows - 1.) * 8.;
        if message.is_empty() {
            self.viewport = Rect::default();
            self.maximum = 0.;
            self.scroll = 0.;
        } else {
            // The exact message, every character of it: it scrolls in its box.
            y += 10.;
            let frame = rect(x, y, w, (top - 16. - y).max(MAC_BOX_MIN));
            kit::draw_group(d, cx, frame, &k);
            self.viewport = rect(frame.pos.x + MAC_BOX_X, frame.pos.y + MAC_BOX_Y, frame.size.x - MAC_BOX_X * 2., frame.size.y - MAC_BOX_Y * 2.);
            let lines = kit::wrapped_exact(d, cx, false, kit::TEXT, &message, self.viewport.size.x - 6.);
            self.maximum = (lines.len() as f64 * kit::TEXT_LINE - self.viewport.size.y).max(0.);
            self.scroll = self.scroll.clamp(0., self.maximum);
            for (i, line) in lines.iter().enumerate() {
                let ly = self.viewport.pos.y + i as f64 * kit::TEXT_LINE - self.scroll;
                // Whole lines only: host text must never draw over the buttons.
                if ly >= self.viewport.pos.y - 0.5 && ly + kit::TEXT_LINE <= self.viewport.pos.y + self.viewport.size.y + 0.5 {
                    d.label(cx, rect(self.viewport.pos.x, ly, self.viewport.size.x - 6., kit::TEXT_LINE), false, kit::TEXT, k.ink, HAlign::Left, line);
                }
            }
            if self.maximum > 0. {
                let track = self.viewport.size.y;
                let thumb = (track * track / (track + self.maximum)).max(16.);
                let ty = self.viewport.pos.y + (track - thumb) * self.scroll / self.maximum;
                d.rounded(cx, rect(frame.pos.x + frame.size.x - 6., ty, 3., thumb), 1.5, k.thumb);
            }
        }
        let held = self.gesture.as_ref().filter(|g| !g.moved).map(|g| (g.target, g.last));
        let down = |t: Target, r: Rect| held.is_some_and(|(target, at)| target == t && contains(r, at));
        let row = |i: f64| rect(x, top + i * (kit::CONTROL_H + 8.), w, kit::CONTROL_H);
        if self.has_action() {
            let action = if self.suggestion.is_some() {
                "Use Draft"
            } else if let Some(retry) = &self.retry {
                match (retry.needs_acknowledgment(), retry.unknown) {
                    (true, _) => "Show Sent",
                    (false, true) => "Send Again…",
                    (false, false) => "Try Again…",
                }
            } else {
                "Approve & Send"
            };
            if rows > 1. {
                // On a send that may have gone: the default on top, then I
                // Understand (or Show Sent, once that is understood), then Cancel.
                self.approve = row(0.);
                self.alternate = row(1.);
                self.cancel = row(2.);
                let alternate = if self.retry.as_ref().is_some_and(Retry::needs_acknowledgment) { "I Understand" } else { "Show Sent" };
                kit::push_button(d, cx, self.approve, &k, Face::Default, false, down(Target::Approve, self.approve), action);
                kit::push_button(d, cx, self.alternate, &k, Face::Plain, false, down(Target::Alternate, self.alternate), alternate);
                kit::push_button(d, cx, self.cancel, &k, Face::Plain, false, down(Target::Cancel, self.cancel), "Cancel");
            } else {
                let half = ((w - 8.) * 0.5).floor();
                self.alternate = Rect::default();
                self.cancel = rect(x, top, half, kit::CONTROL_H);
                self.approve = rect(x + w - half, top, half, kit::CONTROL_H);
                kit::push_button(d, cx, self.cancel, &k, Face::Plain, false, down(Target::Cancel, self.cancel), "Cancel");
                kit::push_button(d, cx, self.approve, &k, Face::Default, false, down(Target::Approve, self.approve), action);
            }
        } else if self.worker.is_some() {
            // While the message is going there's nothing a Cancel could call back.
            self.alternate = Rect::default();
            self.cancel = Rect::default();
            self.approve = row(0.);
            d.label(cx, kit::text_seat(self.approve), false, kit::TEXT, k.muted, HAlign::Center, "Sending…");
        } else {
            // Nothing left to decide: OK, the default, across the alert.
            self.alternate = Rect::default();
            self.approve = Rect::default();
            self.cancel = row(0.);
            kit::push_button(d, cx, self.cancel, &k, Face::Default, false, down(Target::Cancel, self.cancel), "OK");
        }
    }

    pub fn draw(&mut self, cx: &mut Cx2d, d: &mut ShellDraw, area: Rect, tok: &ShellTokens) {
        self.poll();
        let mac = crate::shell::menu::MAC_KEYS.load(std::sync::atomic::Ordering::Relaxed);
        // A Mac's review is an alert of its own (`draw_mac`); what follows is the
        // other styles' and the phone's.
        if mac {
            return self.draw_mac(cx, d, area, tok);
        }
        let ink = tok.notifications.surface.text;
        let width = (area.size.x - 16.).max(32.);
        // A Mac's sheet says what this is in its title.
        if !mac {
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
                } else if self.is_reply() {
                    "Review your reply"
                } else {
                    "Review your message"
                },
            );
        }
        self.viewport = if mac {
            rect(area.pos.x + 8., area.pos.y, width, (area.size.y - MAC_BUTTONS).max(20.))
        } else {
            rect(area.pos.x + 8., area.pos.y + 32., width, (area.size.y - 92.).max(20.))
        };
        let content = self.content();
        let lines = d.wrap_input(cx, 13., &content, width - 8.);
        let line_height = Self::LINE_HEIGHT;
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
        let mut actions = *tok;
        if mac {
            // A Mac's push buttons, at the trailing edge, the action last and in the accent.
            let (back_w, action_w, height) = (72., 132., 28.);
            let y = area.pos.y + area.size.y - height - 4.;
            let right = area.pos.x + area.size.x - 8.;
            self.approve = rect(right - action_w, y, action_w, height);
            self.alternate = if self.offers_sent() { rect(area.pos.x + 8., y, 104., height) } else { Rect::default() };
            // With nothing left to decide the one button is OK, and while the
            // message is going there's nothing a Cancel could call back.
            self.cancel = if self.has_action() {
                rect(right - action_w - 8. - back_w, y, back_w, height)
            } else if self.worker.is_some() {
                Rect::default()
            } else {
                rect(right - back_w, y, back_w, height)
            };
            actions.controls.selected_color = crate::shell::rgb(0, 113, 227);
        } else {
            let y = area.pos.y + area.size.y - 48.;
            self.cancel = rect(area.pos.x + 8., y, 88., 44.);
            self.approve = rect(area.pos.x + 104., y, (width - 96.).max(80.), 44.);
            actions.controls.selected_color = crate::shell::rgb(54, 104, 232);
        }
        actions.controls.selected_fill_alpha = 1.0;
        actions.controls.selected_border_width = 0.0;
        if let (true, Some(retry)) = (mac && self.offers_sent(), &self.retry) {
            d.button(
                cx,
                self.alternate,
                tok,
                CtrlState::Normal,
                None,
                if retry.needs_acknowledgment() { "I Understand" } else { "Show Sent" },
                13.,
                ink,
                true,
            );
        }
        if !mac || self.has_action() {
            d.button(
                cx,
                self.cancel,
                tok,
                CtrlState::Normal,
                None,
                if mac { "Cancel" } else { "Back" },
                13.,
                ink,
                true,
            );
        } else if self.worker.is_none() {
            d.button(
                cx,
                self.cancel,
                &actions,
                CtrlState::Selected,
                None,
                "OK",
                13.,
                crate::shell::rgb(255, 255, 255),
                true,
            );
        }
        if self.has_action() {
            d.button(
                cx,
                self.approve,
                &actions,
                CtrlState::Selected,
                None,
                if self.suggestion.is_some() {
                    if mac { "Use Draft" } else { "Use draft" }
                } else if let Some(retry) = &self.retry {
                    match (mac, retry.needs_acknowledgment(), retry.unknown) {
                        (true, true, _) => "Show Sent",
                        (true, false, true) => "Send Again…",
                        (true, false, false) => "Try Again…",
                        (false, true, _) => "I understand",
                        (false, false, _) => "Review retry",
                    }
                } else {
                    "Approve & Send"
                },
                13.,
                crate::shell::rgb(255, 255, 255),
                true,
            );
        } else if self.worker.is_some() {
            d.label(
                cx,
                self.approve,
                false,
                13.,
                ink,
                if mac { HAlign::Right } else { HAlign::Center },
                if mac { "Sending…" } else { "Submitting…" },
            );
        }
    }

    const LINE_HEIGHT: f64 = 21.;

    /// The words under review, whatever else must be said with them.
    fn content(&self) -> String {
        let mac = self.mac();
        let payload = &self.snapshot["payload"];
        let text = |name: &str| payload[name].as_str().unwrap_or("");
        let copied = text("cc");
        let copied = if copied.is_empty() { String::new() } else { format!("Cc: {copied}\n") };
        let link_notice = if self.missing_reply_reference() {
            "Conversation notice: Mail can't link this reply to the original message. It may appear as a new conversation.\n\n"
        } else {
            ""
        };
        let content = format!(
            "{}From: {}\nTo: {}\n{}Subject: {}\n\n{}\n\n{}",
            link_notice,
            text("from"),
            text("to"),
            copied,
            text("subject"),
            text("body"),
            if self.status.is_empty() && self.retry.is_none() {
                // The last try may have gone: this is all that's certain.
                if self.snapshot["prior_status"] == "outcome_unknown" {
                    "Approve & Send sends this exact message."
                } else {
                    "Nothing has been sent. Approve & Send sends this exact message."
                }
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
        let content = if let (true, Some(retry)) = (mac, &self.retry) {
            format!(
                "{}\n\n{}",
                if retry.unknown {
                    "The connection ended while it was sending, so it may already have gone. Check Sent first: sending it again could reach them twice."
                } else {
                    "The mail server didn't take it, so nothing was sent. Nothing is tried again until you say so."
                },
                content
            )
        } else if let Some(retry) = &self.retry {
            format!(
                "{}\n\n{}",
                if retry.unknown {
                    "This message may already have been sent. Check Sent before trying again; sending it again could reach the recipient twice. Choosing I understand doesn't send it."
                } else {
                    "This message wasn't sent. Nothing will be sent unless you review it again."
                },
                content
            )
        } else if mac && self.snapshot["prior_attempt"].is_string() {
            if self.snapshot["prior_status"] == "outcome_unknown" {
                format!("This message may already have gone. Approve & Send sends it again, and it could reach them twice.\n\n{content}")
            } else {
                content
            }
        } else if self.snapshot["prior_attempt"].is_string() {
            format!(
                "{}\n\n{}",
                if self.snapshot["prior_status"] == "outcome_unknown" {
                    "This message may already have been sent. Approving will send it again, which could reach the recipient twice."
                } else {
                    "This is a new review of a previous send attempt. Nothing has been sent for this attempt."
                },
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
        content
    }

    fn target(&self, p: Vec2d) -> Option<Target> {
        if contains(self.cancel, p) {
            Some(Target::Cancel)
        } else if self.offers_sent() && contains(self.alternate, p) {
            Some(Target::Alternate)
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
            // "I Understand" first, then "Show Sent": acknowledging stages nothing and sends nothing.
            Target::Alternate => {
                if !self.offers_sent() {
                    return;
                }
                if self.retry.as_ref().is_some_and(Retry::needs_acknowledgment) {
                    self.acknowledge();
                } else {
                    self.show_sent();
                }
            }
            Target::Approve => {
                // A Mac's default before the risk is acknowledged: look in Sent first.
                if self.offers_sent() && self.retry.as_ref().is_some_and(Retry::needs_acknowledgment) {
                    self.show_sent();
                    return;
                }
                if let Some(retry) = self.retry.as_ref() {
                    if retry.needs_acknowledgment() {
                        self.acknowledge();
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
                if started.is_err() {
                    if let Some(review) = pending.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = drafts::cancel_review(review);
                    }
                    self.status = "Mail couldn't start sending. Nothing was sent; try again.".into();
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
    fn a_reply_without_a_message_id_explains_that_the_thread_may_change() {
        let mut panel = MailReview::default();
        panel.snapshot = serde_json::json!({
            "source_message": {"folder":"INBOX", "message":"local-id"},
            "payload": {"subject":"Re: Example", "body":"Reply", "in_reply_to":""}
        });
        assert_eq!(panel.mac_title(), "Review Your Reply");
        let text = panel.content();
        assert!(text.contains("It may appear as a new conversation."));
        assert!(text.find("Conversation notice:").unwrap() < text.find("From:").unwrap(), "the notice is outside the message fields");

        panel.snapshot["payload"]["in_reply_to"] = serde_json::json!("<original@example.com>");
        assert_eq!(panel.mac_title(), "Review Your Reply");
        assert!(!panel.content().contains("It may appear as a new conversation."));

        panel.snapshot["source_message"] = serde_json::json!({"message":null});
        assert_eq!(panel.mac_title(), "Review Your Reply", "older snapshots can still carry the reply header");
        panel.snapshot["payload"]["in_reply_to"] = serde_json::json!("");
        assert_eq!(panel.mac_title(), "Review Your Message");
        assert!(!panel.content().contains("It may appear as a new conversation."));
    }

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
        let mut unlinked_reply = receipt("failed_before_delivery");
        unlinked_reply["source_message"] = serde_json::json!({"folder":"INBOX", "message":"local-id"});
        let (_, snapshot) = Retry::from_receipt("os.mail", &unlinked_reply).unwrap();
        let mut panel = MailReview::default();
        panel.snapshot = snapshot;
        assert!(panel.content().contains("It may appear as a new conversation."), "the retry keeps the source notice");
    }

    #[test]
    fn touch_retry_explains_duplicate_risk_without_exposing_attempt_ids() {
        let (retry, snapshot) = Retry::from_receipt("os.mail", &receipt("outcome_unknown")).unwrap();
        let mut panel = MailReview::default();
        panel.mac_form = Some(false);
        panel.snapshot = snapshot;
        panel.retry = Some(retry);
        let text = panel.content();
        assert!(text.contains("Check Sent") && text.contains("twice") && text.contains("doesn't send it"));
        assert!(!text.contains("latest"), "the internal attempt identity is not user-facing");
        panel.acknowledge();
        assert!(panel.status.contains("Nothing is sent until you approve it."));
        assert!(panel.review.is_none() && panel.worker.is_none(), "acknowledgment still sends nothing");

        panel.status.clear();
        panel.retry = None;
        panel.snapshot["prior_attempt"] = serde_json::json!("private-attempt-id");
        panel.snapshot["prior_status"] = serde_json::json!("outcome_unknown");
        let text = panel.content();
        assert!(text.contains("could reach the recipient twice"));
        assert!(!text.contains("private-attempt-id"));
    }

    #[test]
    fn a_send_result_error_is_explained_without_exposing_internal_details() {
        let mut panel = MailReview::default();
        panel.active = true;
        // A non-null, non-object snapshot avoids a real account or transport.
        panel.snapshot = serde_json::json!("synthetic-review");
        panel.worker = Some(Arc::new(Mutex::new(Some(Err("private-internal-detail".into())))));
        panel.poll();
        assert!(panel.status.contains("Check Sent") && panel.status.contains("twice"));
        assert!(!panel.status.contains("private-internal-detail"));
        assert!(panel.worker.is_none());
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
    fn a_macs_unknown_send_offers_sent_and_still_asks_i_understand_before_another_send() {
        let (retry, snapshot) =
            Retry::from_receipt("os.mail", &receipt("outcome_unknown")).unwrap();
        let sheet = |retry: &Retry| {
            let mut panel = MailReview::default();
            panel.mac_form = Some(true);
            panel.active = true;
            panel.snapshot = snapshot.clone();
            panel.retry = Some(retry.clone());
            panel.approve = rect(300., 100., 132., 28.);
            panel.alternate = rect(8., 100., 104., 28.);
            panel
        };
        // The default, Show Sent, closes the sheet: nothing is staged or sent.
        let mut panel = sheet(&retry);
        assert!(panel.offers_sent());
        panel.begin(5, dvec2(310., 110.));
        panel.release(5, dvec2(310., 110.));
        assert!(!panel.active && panel.review.is_none() && panel.worker.is_none());
        assert!(drafts::take_show_sent("os.mail", "test-original", "draft-1"));
        assert!(!drafts::take_show_sent("os.mail", "test-original", "draft-1"), "Mail hears it once");

        // I Understand, at the leading edge, acknowledges and stages nothing.
        let mut panel = sheet(&retry);
        panel.begin(3, dvec2(20., 110.));
        panel.release(3, dvec2(20., 110.));
        assert!(panel.retry.as_ref().unwrap().acknowledged);
        assert!(panel.active && panel.review.is_none() && panel.worker.is_none());
        assert!(panel.gesture.is_none());
        // Then the same place is Show Sent: it closes and sends nothing.
        panel.begin(4, dvec2(20., 110.));
        panel.release(4, dvec2(20., 110.));
        assert!(!panel.active && panel.review.is_none() && panel.worker.is_none());
        assert!(drafts::take_show_sent("os.mail", "test-original", "draft-1"));

        // A press that starts on one button and ends on the other does nothing.
        let mut panel = sheet(&retry);
        panel.begin(6, dvec2(20., 110.));
        panel.release(6, dvec2(310., 110.));
        assert!(panel.active && !panel.retry.as_ref().unwrap().acknowledged);
        // A send that failed before delivery has no third button.
        let (failed, _) =
            Retry::from_receipt("os.mail", &receipt("failed_before_delivery")).unwrap();
        let mut panel = sheet(&failed);
        assert!(!panel.offers_sent());
        panel.begin(7, dvec2(20., 110.));
        panel.release(7, dvec2(20., 110.));
        assert!(panel.active);
        assert!(!drafts::take_show_sent("os.mail", "test-original", "draft-1"));
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

    #[test]
    fn a_macs_alert_says_every_word_the_review_says() {
        // Each paragraph a Mac's form of `content()` holds is in the alert: under
        // its title or in the message's box.
        let message = serde_json::json!({"from":"me@example.com", "to":"noah@example.com", "cc":"sam@example.com",
            "subject":"Trail map", "body":"Do you still have the trail map?\n\nThanks."});
        let mut panels = Vec::new();
        for snapshot in [
            serde_json::json!({"payload": message.clone()}),
            serde_json::json!({"payload": message.clone(), "prior_attempt":"op", "prior_status":"outcome_unknown"}),
            serde_json::json!({"payload": message.clone(), "source_message":{"folder":"INBOX", "message":"local-id"}}),
        ] {
            let mut panel = MailReview::default();
            panel.mac_form = Some(true);
            panel.snapshot = snapshot;
            panels.push(panel);
        }
        for status in ["outcome_unknown", "failed_before_delivery"] {
            let (retry, snapshot) = Retry::from_receipt("os.mail", &receipt(status)).unwrap();
            let mut panel = MailReview::default();
            panel.mac_form = Some(true);
            panel.snapshot = snapshot;
            panel.retry = Some(retry);
            panels.push(panel);
        }
        let mut busy = MailReview::default();
        busy.mac_form = Some(true);
        busy.snapshot = serde_json::json!({"payload": message});
        busy.status = "Submitting this exact reviewed message…".into();
        panels.push(busy);
        for panel in panels {
            let (said, box_text) = panel.mac_parts();
            let alert = format!("{said}\n\n{box_text}");
            for para in panel.content().split("\n\n").filter(|p| !p.trim().is_empty()) {
                assert!(alert.contains(para), "{para:?} is missing from the alert: {alert:?}");
            }
        }
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
        let review = if status.reply { "Review reply" } else { "Review message" };
        for (i, label) in ["Suggestion", "Restore saved", review]
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
