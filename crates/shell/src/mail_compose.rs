//! Explicit human request to turn an incoming informational card into a reply.
//! The host resolves identity; the app agent authors the draft and publication.
use crate::glance::GlanceCard;
use std::sync::{Arc, Mutex};

type ResultCell = Arc<Mutex<Option<Result<serde_json::Value, String>>>>;

#[derive(Default)]
pub(crate) struct ComposeReply {
    pending: Option<ResultCell>,
    pub requested: bool,
    pub error: Option<String>,
}

pub(crate) fn eligible(card: &GlanceCard) -> bool {
    card.app == "os.mail"
        && card.account.is_some()
        && card
            .card_id
            .strip_prefix("mail-")
            .is_some_and(|id| id.len() == 40 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        && card.l0.as_ref().is_some_and(|l| l.mail.is_none())
}

impl ComposeReply {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    pub fn start(&mut self, card: &GlanceCard) {
        if self.busy() {
            return;
        }
        self.error = None;
        if !eligible(card) || !card.account_valid() {
            self.error = Some("This Mail card is no longer available".into());
            return;
        }
        #[cfg(any(feature = "app-hub", native_mobile))]
        {
            let account = card.account.clone().unwrap();
            let card_id = card.card_id.clone();
            let result = Arc::new(Mutex::new(None));
            let output = result.clone();
            match std::thread::Builder::new()
                .name("mail-compose-source".into())
                .spawn(move || {
                    let answer = crate::mail_card::host_dir()
                        .and_then(|dir| {
                            octosense_mail_service::source_for_card(
                                &dir, "os.mail", &account, &card_id,
                            )
                        })
                        .map(|mut source| {
                            source["card_id"] = serde_json::json!(card_id);
                            source
                        });
                    *output.lock().unwrap_or_else(|e| e.into_inner()) = Some(answer);
                    makepad_widgets::makepad_platform::SignalToUI::set_ui_signal();
                }) {
                Ok(_) => self.pending = Some(result),
                Err(error) => self.error = Some(format!("Cannot prepare reply: {error}")),
            }
        }
        #[cfg(not(any(feature = "app-hub", native_mobile)))]
        {
            self.error = Some("Mail is unavailable in this build".into());
        }
    }

    pub fn take_ready(&mut self) -> Option<Result<serde_json::Value, String>> {
        let result = self
            .pending
            .as_ref()?
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if result.is_some() {
            self.pending = None;
        }
        result
    }
}
