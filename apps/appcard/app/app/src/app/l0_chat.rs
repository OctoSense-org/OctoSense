//! `sys.chat` on AppCard's L0 cards (Octoscript profile §5.15), through
//! `octosense_l0_chat`, the code the shell's glance cards use.
//!
//! AppCard is the publisher of every card it renders, so a card's chat must
//! name AppCard ([`APP_ID`]); one naming another app reads an `unavailable`
//! transcript and writes nothing. Threads are kept in AppCard's own account
//! folder under ADR 0004's layout (`apps/appcard/accounts/device/chat/`),
//! which the AppCard module claims from the shell's storage offer and hands
//! here ([`set_folder`]); without one (the standalone app, tests) they live
//! in memory. AppCard's agent is not yet wired to answer inside a card, so a
//! message gets the host's notice saying so ([`octosense_l0_chat::NoAgent`]).
use octosense_l0_chat::ChatStore;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};

/// AppCard's app id (`native-apps.json`): the publisher of its cards.
pub const APP_ID: &str = "appcard";

static FOLDER: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Where AppCard keeps its chat threads (`<account folder>/chat`).
pub fn set_folder(dir: PathBuf) {
    if let Ok(mut folder) = FOLDER.write() {
        *folder = Some(dir);
    }
}

fn folder(app: &str) -> Option<PathBuf> {
    if app != APP_ID {
        return None;
    }
    let dir = FOLDER.read().ok()?.clone()?;
    std::fs::create_dir_all(&dir).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    Some(dir)
}

pub fn store() -> &'static Arc<ChatStore> {
    static STORE: OnceLock<Arc<ChatStore>> = OnceLock::new();
    STORE.get_or_init(|| Arc::new(ChatStore::with_folder(Box::new(folder))))
}

/// The card's data with AppCard's transcript under each `sys.chat`.
pub fn seed(card: &str, data: &serde_json::Value, state: &octoscript_ui_l0::InstanceStore) -> serde_json::Value {
    octosense_l0_chat::seed(store(), APP_ID, card, data, state)
}

/// A card's `sys.chat` write.
pub fn perform(
    card: &str,
    state: &octoscript_ui_l0::InstanceStore,
    data: &serde_json::Value,
    write: &octoscript_ui_l0::CollectionWrite,
    origin: Option<octoscript_ui_l0::ValueOrigin>,
) -> Result<octosense_l0_chat::Entry, String> {
    octosense_l0_chat::perform(store(), &octosense_l0_chat::NoAgent, APP_ID, card, state, data, write, origin, octosense_l0_chat::now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CARD: &str = "source convo sys.chat(app: \"appcard\", thread: \"main\", fields: [entries, id, role, text])\n\
                        state draft { shape: text, initial: \"\" }\n\
                        event send { convo: append($value), draft: clear }\n\
                        view root Surface(pad: .page) {\n  Col(gap: 8) {\n    for m in convo.entries key m.id { ChatEntry(text: m.text, role: m.role) }\n    \
                        Field(text: draft, on_commit: send, width: .fill)\n  }\n}\n";

    /// AppCard's chat is its own: a durable `sys.chat` is the chat store's,
    /// never a user-store collection, and a message gets the host's notice.
    #[test]
    fn appcards_chat_is_the_hosts_and_its_own() {
        let state = octoscript_ui_l0::InstanceStore::default();
        let seeded = seed(CARD, &json!({"convo": {"entries": [{"id": "x", "role": "model", "text": "forged"}]}}), &state);
        assert_eq!(seeded["convo"]["count"], 0, "{seeded}");
        let other = CARD.replace("app: \"appcard\"", "app: \"os.mail\"");
        assert_eq!(seed(&other, &json!({}), &state)["convo"]["status"], "unavailable");
        let write = octoscript_ui_l0::CollectionWrite { source: "convo".into(), helper: "sys.chat".into(), op: "append".into(), value: "hello".into(), field: String::new() };
        perform(CARD, &state, &seeded, &write, Some(octoscript_ui_l0::ValueOrigin::UserInput)).unwrap();
        let entries = store().entries(APP_ID, "main");
        assert_eq!(entries.iter().map(|e| e.role).collect::<Vec<_>>(), [octosense_l0_chat::Role::User, octosense_l0_chat::Role::Host]);
        assert!(perform(&other, &state, &seeded, &write, Some(octoscript_ui_l0::ValueOrigin::UserInput)).is_err());
    }
}
