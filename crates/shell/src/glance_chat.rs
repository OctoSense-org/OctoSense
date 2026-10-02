//! The shell's `sys.chat` (Octoscript profile §5.15): an L0 glance card's
//! in-card chat with its app's agent, through `octosense_l0_chat`.
//!
//! - **Whose.** The publishing app's only: a card naming another app is
//!   refused at publish (glance.rs), reads an `unavailable` transcript and
//!   writes nothing ([`seed`], [`perform`]).
//! - **Where.** One file per thread in the app's account folder (ADR 0004
//!   §11): `apps/<app>/accounts/<account>/chat/<thread>.json`, the account
//!   the app's agent acts for now (`device` for an app without accounts, or
//!   one with none yet), created owner-only like every app folder. That is
//!   the agent's own workspace, so it can read the transcripts it is part
//!   of. Without the host's storage (unit tests), threads live in memory.
//! - **Who answers.** [`HostResponder`]: the app's agent, a turn in the
//!   app's conversation (ADR 0004 §6: the person's lane on the host-owned
//!   app peer, `crate::agents::conversation`) once the person allowed it;
//!   otherwise a `host` notice saying why. Under `OCTOSENSE_GLANCE_DEMO=mail`
//!   Mail's chat answers with the canned demo reply ([`DEMO_ANSWER`]) by
//!   design, although Mail has an agent (its `tools.json`, `mail.notify`):
//!   the demo's cards and thread are fake data (glance.rs), so its chat
//!   never asks a model.
//! - **Stale.** Every change wakes the UI; a surface that shows the card
//!   (the card window, the glance panel) re-seeds and re-lowers it when
//!   [`generation`] moved (glance_card.rs `LiveCards`).
use octosense_l0_chat::{ChatStore, Done, Reply, Request, Responder};
use serde_json::Value;
use std::sync::{Arc, Mutex, OnceLock};

pub use octosense_l0_chat::{check_publisher, Entry, Role};

/// What the demo host answers a message sent from a card (no model call).
pub const DEMO_ANSWER: &str = "Demo answer: Mail's agent will reply here from the thread. (No model was called.)";

/// The shell's one chat store.
pub fn store() -> &'static Arc<ChatStore> {
    static STORE: OnceLock<Arc<ChatStore>> = OnceLock::new();
    STORE.get_or_init(|| {
        let store = if crate::app_storage::host().is_some() { ChatStore::with_folder(Box::new(folder)) } else { ChatStore::in_memory() };
        store.set_on_change(Box::new(makepad_widgets::makepad_platform::SignalToUI::set_ui_signal));
        Arc::new(store)
    })
}

/// The app's chat folder in its account folder, created owner-only.
fn folder(app: &str) -> Option<std::path::PathBuf> {
    folder_in(crate::app_storage::host()?, app, crate::ai_host::contained::account_of(app))
}

/// `app`'s chat folder in `storage`, for `account`: the account its agent
/// acts for, as `account_of` answers it (the shell's lookup is
/// `app_storage::lifecycle::contained_account_in`); the device folder for
/// [`crate::ai_host::contained::ACCOUNT`] or none.
fn folder_in(storage: &crate::app_storage::Storage, app: &str, account: Option<String>) -> Option<std::path::PathBuf> {
    let paths = storage.layout().app(app).ok()?;
    let account = account.filter(|a| a != crate::ai_host::contained::ACCOUNT);
    let dir = paths.account(account.as_deref()).join("chat");
    match crate::app_storage::ensure_private_dir(storage.layout().apps_root(), &dir) {
        Ok(()) => Some(dir),
        Err(e) => {
            makepad_widgets::log!("glance chat: {app}'s chat folder: {e}; kept in memory");
            None
        }
    }
}

/// Bumped on every change to any thread.
pub fn generation() -> u64 {
    store().generation()
}

/// `data` with the host's transcript under each `sys.chat` the card reads.
pub fn seed(publisher: &str, card: &str, data: &Value, state: &octoscript_ui_l0::InstanceStore) -> Value {
    octosense_l0_chat::seed(store(), publisher, card, data, state)
}

/// Whether the card reads a `sys.chat`.
pub fn reads_chat(card: &str) -> bool {
    !octosense_l0_chat::sources(card).is_empty()
}

/// Carry out the card's `sys.chat` write for `publisher`.
pub fn perform(
    publisher: &str,
    card: &str,
    state: &octoscript_ui_l0::InstanceStore,
    data: &Value,
    write: &octoscript_ui_l0::CollectionWrite,
    origin: Option<octoscript_ui_l0::ValueOrigin>,
) -> Result<Entry, String> {
    octosense_l0_chat::perform(store(), &HostResponder, publisher, card, state, data, write, origin, octosense_l0_chat::now_ms())
}

static DEMO_MAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Mail's chat answers with the canned demo reply (`OCTOSENSE_GLANCE_DEMO=mail`,
/// glance.rs, which publishes the fake Mail cards).
pub fn set_demo_mail(on: bool) {
    DEMO_MAIL.store(on, std::sync::atomic::Ordering::Relaxed);
}

fn demo_mail() -> bool {
    DEMO_MAIL.load(std::sync::atomic::Ordering::Relaxed)
}

/// Who answers a card's chat in the shell (module docs).
pub struct HostResponder;

impl Responder for HostResponder {
    fn respond(&self, request: Request, done: Done) {
        if demo_mail() && request.app == "os.mail" {
            return octosense_l0_chat::Canned(DEMO_ANSWER.into()).respond(request, done);
        }
        AgentResponder.respond(request, done)
    }
}

/// The app's own agent: a turn in its conversation (the person's lane on
/// the app's host-owned peer). Off the UI thread: the peer may be prepared
/// first.
pub struct AgentResponder;

/// The instance the card chat's conversation is opened under.
const INSTANCE: &str = "card-chat";

impl Responder for AgentResponder {
    fn respond(&self, request: Request, done: Done) {
        let Some(app) = crate::agents::all().into_iter().find(|a| a.id == request.app) else {
            return done(Reply::Notice(format!("{} has no agent to answer here yet.", request.app)));
        };
        let spawned = std::thread::Builder::new().name("card-chat".into()).spawn(move || {
            use crate::ai_host::app_peers::{ContextEvent, ContextOp, EventSink, TurnTrigger};
            let context = match crate::agents::conversation(&app, INSTANCE) {
                Ok(context) => context,
                Err(e) => return done(Reply::Notice(e)),
            };
            let done = Arc::new(Mutex::new(Some(done)));
            let finish = {
                let done = done.clone();
                let context = context.clone();
                move |reply: Reply| {
                    if let Some(done) = done.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        done(reply);
                    }
                    context.close();
                }
            };
            let sink: EventSink = {
                let finish = finish.clone();
                Arc::new(move |event| match event {
                    ContextEvent::Complete(Ok(value)) => finish(Reply::Model(value["text"].as_str().unwrap_or_default().to_string())),
                    ContextEvent::Complete(Err(e)) => finish(Reply::Notice(format!("The agent could not answer: {e}"))),
                    ContextEvent::Data(_) => {}
                })
            };
            // The person typed it in a card the host drew, but the card is
            // the app's: approval rules see the app's run, as for an app
            // that says the person asked (ADR 0004 §8).
            if let Err(e) = context.call(ContextOp::TurnFrom { text: request.text, trigger: TurnTrigger::AppSaysPerson }, sink) {
                finish(Reply::Notice(format!("The agent could not answer: {e}")));
            }
        });
        if let Err(e) = spawned {
            makepad_widgets::log!("glance chat: no thread for the agent: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An app with no agent gets a host notice, never a `model` entry.
    #[test]
    fn an_app_without_an_agent_gets_a_host_notice() {
        let got = Arc::new(Mutex::new(None));
        let sink = got.clone();
        AgentResponder.respond(
            Request { app: "com.example.noagent".into(), thread: "t".into(), text: "hi".into(), history: Vec::new() },
            Box::new(move |reply| *sink.lock().unwrap() = Some(reply)),
        );
        let reply = got.lock().unwrap().take();
        match reply {
            Some(Reply::Notice(text)) => assert!(text.contains("no agent"), "{text}"),
            other => panic!("{other:?}"),
        }
    }

    /// A card's chat thread is kept in the folder of the account the app's
    /// agent acts for: a signed-in Mail's account, also for a thread first
    /// touched at startup (the demo's seed), before anything opened Mail or
    /// prepared its agent in this run. The account comes from Mail's
    /// manifest then; before, the unrecorded block read as "no accounts"
    /// and the thread went to the device folder.
    #[cfg(any(feature = "app-hub", native_mobile))]
    #[test]
    fn a_cards_thread_is_kept_under_the_account_the_agent_acts_for() {
        use crate::app_storage::{lifecycle::contained_account_in, Layout, Storage};
        let home = crate::app_storage::tests::Scratch::new("glance-chat-account");
        let host = Storage::with_file_secrets(Layout::new(&home.0).unwrap());
        let apps = host.layout().apps_root().to_path_buf();
        let write = |path: std::path::PathBuf, value: Value| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, value.to_string()).unwrap();
        };
        write(apps.join(".system/os.mail/0001/manifest.json"), serde_json::json!({"id": "os.mail", "storage": {"accounts": true}}));
        write(apps.join(".host/mail/accounts.json"), serde_json::json!([{"id": "ana@example.org", "apps": ["os.mail"], "signed_in": 10}]));
        assert!(!host.has_spec("os.mail"), "nothing opened Mail in this run");
        let chat = {
            let host = host.clone();
            ChatStore::with_folder(Box::new(move |app| folder_in(&host, app, contained_account_in(&host, app))))
        };
        assert!(chat.seed_if_empty("os.mail", "ana-contract", &[(Role::Model, "Net 30.")], 0));
        let mail = host.layout().app("os.mail").unwrap();
        assert!(mail.account(Some("ana@example.org")).join("chat/ana-contract.json").is_file());
        assert!(!mail.account(None).join("chat").exists(), "nothing in the device folder");
        // An app without accounts keeps its threads in its device folder.
        assert!(chat.seed_if_empty("os.news", "main", &[(Role::Model, "Rates held.")], 0));
        assert!(host.layout().app("os.news").unwrap().account(None).join("chat/main.json").is_file());
    }
}
