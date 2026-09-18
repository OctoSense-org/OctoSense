//! What the window manager tells a requester when an `Open` or `Launch`
//! named an app this catalog cannot launch. Any hosted app may parse it:
//! `{"wm_unavailable": {"app": "browser", "path": "https://..."}}`, as an
//! `Event::Custom` over a process's socket or into a module's isolate.
//!
//! The News crate keeps its own copy of this envelope
//! (`apps/news/src/model.rs`, `WmUnavailable`): a hosted app cannot depend
//! on the host, so a change to the shape here is a change there too.
use makepad_widgets::makepad_micro_serde::*;

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct WmUnavailable {
    pub app: String,
    pub path: String,
}

// The wire envelope, one concrete struct as `makepad_wm_api` keeps its own
// (the derives do not bound generics).
#[derive(SerJson, DeJson)]
struct Envelope {
    wm_unavailable: WmUnavailable,
}

impl WmUnavailable {
    pub fn to_json(&self) -> String {
        Envelope { wm_unavailable: self.clone() }.serialize_json()
    }

    /// The reply in a Custom message's json; None when it is something else.
    pub fn parse(json: &str) -> Option<Self> {
        if !json.contains("\"wm_unavailable\"") {
            return None;
        }
        Envelope::deserialize_json(json).ok().map(|e| e.wm_unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_envelope_is_one_object_and_round_trips_quotes_and_unicode() {
        let plain = WmUnavailable { app: "browser".into(), path: "https://x/a".into() };
        assert_eq!(plain.to_json(), r#"{"wm_unavailable":{"app":"browser","path":"https://x/a"}}"#);
        let odd = WmUnavailable { app: "browser".into(), path: "https://x/a?q=\"quoted\"&t=é✓".into() };
        assert_eq!(WmUnavailable::parse(&odd.to_json()), Some(odd));
    }

    #[test]
    fn other_json_is_not_the_envelope() {
        assert_eq!(WmUnavailable::parse(&makepad_wm_api::WmEvent::CloseRequested.to_json()), None);
        let named = makepad_wm_api::WmEvent::PreviewFile { path: "wm_unavailable".into() }.to_json();
        assert_eq!(WmUnavailable::parse(&named), None, "the key as a value is not the envelope");
        assert_eq!(WmUnavailable::parse("garbage"), None);
        assert_eq!(WmUnavailable::parse(r#"{"wm_unavailable":{"app":"browser"}}"#), None, "a missing path is not the envelope");
    }
}
