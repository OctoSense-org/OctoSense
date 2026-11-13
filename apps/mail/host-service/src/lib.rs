//! The `mail` host service: mail for contained apps, credentials kept by the
//! host.
//!
//! A script app granted `mail` calls, through `host.request`:
//!
//! | method | args | answer |
//! |---|---|---|
//! | `mail.accounts` | – | `[{id, address}]` this app may use |
//! | `mail.add_account` | `{account?}` | `{id, address}` once the person signs in on the host's sheet; with `account`, the sheet asks for that account's password again |
//! | `mail.remove_account` | `{account}` | `{}`; the account is deleted when no app uses it |
//! | `mail.folders` | `{account}` | `[{id, name, role}]`, the inbox first (`role`: inbox, sent, drafts, junk, trash, archive, all, flagged or "") |
//! | `mail.sync` | `{account, folder?}` | `{new, total}` after fetching new mail |
//! | `mail.list` | `{account, folder?, offset?, limit?}` | `{folder, total, messages: [{id, sender, address, to, subject, preview, time, unread}]}`; `to` is whom it was written to, clipped to 512 bytes (Sent lists it) |
//! | `mail.peek` | `{account, folder?, message, offset?}` | bounded plain-text page without marking read; agent account is host-bound |
//! | `mail.skip_event` | `{account, event_id, reason}` | durable no-card decision; host alone acknowledges |
//! | `mail.publish_card` | `{account, card_id, source, data?, title, summary?, notify?, priority?, draft_id?}` | model-authored L0; an optional draft receives a host-resolved binding |
//! | `mail.message` | `{account, folder?, message}` | `{id, sender, address, to, cc, reply_to, reply_all_cc, subject, body, html, attachments, date, time}`; `reply_to` and `reply_all_cc` are whom a reply and a Reply All address, plain addresses without the account's own |
//! | `mail.mark_read` | `{account, folder?, message}` | `{}` |
//! | `mail.mark` | `{account, folder?, message, unread?, flagged?}` | `{marked}`; Mail only, in the person's hands: this device's copy at once, then the server's `\Seen` and `\Flagged` over IMAP |
//! | `mail.move` | `{account, folder?, message, to?, target?}` | `{moved}`; Mail only, in the person's hands: `to` is `archive` or `trash`, and from the trash, or with no trash, it deletes for good; or `target`, a mailbox the server lists |
//! | `mail.attachment` | `{account, folder?, message, index}` | `{name, size}`; Mail only, in the person's hands: fetches attachment `index` and stages it beside Mail's storage |
//! | `mail.attachment_keep` | – | `{path, name}`; the staged attachment, now a file in Mail's storage for `files.export` |
//! | `mail.propose_reply` | `{account, folder?, message, body, reply_key?}` | bounded durable draft; optional key distinguishes an explicitly requested later reply |
//! | `mail.draft` | `{account, draft_id}` | bounded draft/revision/status snapshot |
//! | `mail.suggest_reply` | `{account, draft_id, expected_revision, body, edit_token?}` | proposal by default; native chat's single-use token saves the requested body at that revision; never sends |
//! | `mail.propose_send` | `{account, draft_id, expected_revision}` | immutable pending attempt; no approval or SMTP |
//! | `mail.review_send` | `{account, to, subject, body, compose_id?, expected_revision?, folder?, message?}` | foreground UI only: durable composer and host review, never approval |
//! | `mail.compose` | `{account, to, subject, body, compose_id?, expected_revision?, folder?, message?}` | app/account-bound saved composer; never approval or SMTP |
//! | `mail.compose_status` | `{account, compose_id}` | app/account-bound draft and attempt receipt |
//! | `mail.send` | composer fields | third-party foreground native review; os.mail retains its legacy refusal |
//! | `mail.notify` | `{title, body, card_id?, priority?}` | `{card_id, replaced, expires_at}` once the shell put Mail's notice card on the glance screen, with a notification ([`on_notify`]) |
//!
//! The app never sees a password or a socket. `mail.add_account` raises the
//! host's sign-in sheet, a separate isolate over the app; only calls from
//! that sheet (`mail.sheet.submit`, `mail.sheet.cancel`) can carry a
//! password, and the service tests the account before it keeps it. Each
//! account is granted to the apps that added it, and an app can reach only
//! those.
//!
//! `folder` defaults to the inbox. Accounts read over IMAP (folders, and the
//! read flag goes back to the server) or POP3 (the inbox only); both send
//! over SMTP. `html` is the message rebuilt from the few tags the app's
//! `Html` view draws, with nothing remote in it; `body` is its text.
//!
//! Agent `accounts`, `folders` and `list` are bounded cached reads: the shell
//! injects its trusted peer account and disables prompts. The UI retains its
//! existing arrays and network folder listing. UI/background sync share the
//! durable incoming queue in [`collect_inbox`]; the shell owns scheduling.
//! Reply draft edits and SMTP authorization belong to [`drafts`]' Rust API.
//! Only a trusted host control may consume its opaque, single-use `Review`;
//! script arguments and developer mode cannot mint it. SMTP errors after
//! submission starts remain uncertain, never an automatic retry.
//!
//! State lives under the host's own directory (`<host_dir>/mail`), outside
//! every app's jail: `accounts.json` (no passwords) and `box-<id>…json` (the
//! fetched mail). Passwords go to the platform's secret store ([`vault`]),
//! whose files are in the host's secrets folder ([`set_secrets_dir`]).
//!
//! The shell reads one more thing, host-side and only with the person's
//! consent: [`contacts::known_addresses`], their own addresses and the
//! people they sent mail to, for the approval rules' "recipients in my
//! contacts".
use octosense_appstore::makepad_widgets::splash_storage::{self, PreparedStorageImport};
use octosense_appstore::services::{close_sheet_later, HostService, Replier, ServiceCall, ServiceHost};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub mod contacts;
pub mod drafts;
pub mod vault;
mod html;
mod imap;
mod incoming;
pub mod public_review;
pub use incoming::{acknowledge_event, collect_inbox, event_resolved, pending_events, pending_events_try, resolve_and_ack_try, source_for_card, CollectReport, IncomingEvent};

use vault::Vault;

#[allow(dead_code)]
mod network;

pub const INBOX: &str = "INBOX";

/// An account an app gained or lost through this service (ADR 0004 §11):
/// the shell's account lifecycle opens the app's folder for it on
/// `Added` and deletes that folder (its agent suspended) on `Removed`.
/// `account` is the service's account id (`{id}` in `mail.accounts`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountEvent {
    Added { app_id: String, account: String },
    Removed { app_id: String, account: String },
}

/// Who hears [`AccountEvent`]s: the shell, once at startup.
pub type AccountListener = Arc<dyn Fn(AccountEvent) + Send + Sync>;

fn account_listener() -> &'static Mutex<Option<AccountListener>> {
    static LISTENER: std::sync::OnceLock<Mutex<Option<AccountListener>>> = std::sync::OnceLock::new();
    LISTENER.get_or_init(Default::default)
}

/// Install (or with `None` remove) the account listener.
pub fn on_account_event(listener: Option<AccountListener>) {
    *account_listener().lock().unwrap_or_else(|e| e.into_inner()) = listener;
}

fn account_event(event: AccountEvent) {
    let listener = account_listener().lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(listener) = listener {
        listener(event);
    }
}

/// Puts a notice on the glance screen as the calling app: `(app id,
/// mail.notify arguments)` → `{card_id, replaced, expires_at}`. The shell
/// installs it once at startup: it draws the notice card every system app
/// shares, and decides whether the app may publish (the `glance` grant).
pub type Notifier = Arc<dyn Fn(&str, &Value) -> Result<Value, String> + Send + Sync>;

fn notifier() -> &'static Mutex<Option<Notifier>> {
    static NOTIFIER: std::sync::OnceLock<Mutex<Option<Notifier>>> = std::sync::OnceLock::new();
    NOTIFIER.get_or_init(Default::default)
}

/// Install (or with `None` remove) what `mail.notify` goes to.
pub fn on_notify(notify: Option<Notifier>) {
    *notifier().lock().unwrap_or_else(|e| e.into_inner()) = notify;
}

/// `mail.notify`: the call, as `app`, to the shell's notifier.
fn notify(app: &str, args: &Value) -> Result<Value, String> {
    let notify = notifier().lock().unwrap_or_else(|e| e.into_inner()).clone();
    match notify {
        Some(notify) => notify(app, args),
        None => Err("This device shows no glance cards.".into()),
    }
}

/// Publishes model-authored L0 through the shell's own grant/level checker.
pub type CardPublisher = Arc<dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync>;
fn card_publisher() -> &'static Mutex<Option<CardPublisher>> {
    static PUBLISHER: std::sync::OnceLock<Mutex<Option<CardPublisher>>> = std::sync::OnceLock::new();
    PUBLISHER.get_or_init(Default::default)
}
pub fn on_publish_card(publisher: Option<CardPublisher>) {
    *card_publisher().lock().unwrap_or_else(|e| e.into_inner()) = publisher;
}
// Bound the encoded JSON payload, including escaped controls/quotes, while
// retaining a UTF-8 boundary for peek's next_offset.
fn clip(value: &str, bytes: usize) -> String {
    let (mut used, mut end) = (0, 0);
    for (offset, ch) in value.char_indices() {
        let cost = match ch {
            '\"' | '\\' | '\n' | '\r' | '\t' | '\u{08}' | '\u{0c}' => 2,
            c if c < '\u{20}' => 6,
            c => c.len_utf8(),
        };
        if used + cost > bytes { break; }
        used += cost;
        end = offset + ch.len_utf8();
    }
    value[..end].to_string()
}

/// Read-only agent API. The shell injects the peer's trusted account; the
/// service separately checks that the owning app is granted that account.
/// Cached folder/list/peek reads neither connect nor modify unread state.
fn agent_read(store: &Store, app: &str, method: &str, args: &Value) -> Result<Value, String> {
    let account = text(args, "account");
    let granted = store.granted(app, account)?;
    let folder = if text(args, "folder").is_empty() { INBOX } else { text(args, "folder") };
    if folder.len() > 256 { return Err("Folder is too long".into()); }
    let result = match method {
        "accounts" => json!({"accounts": [{"id": account, "address": clip(text(&granted, "address"), 160)}]}),
        "folders" => {
            let folders = store.folders(account).unwrap_or(json!(inbox_only()));
            let all = folders.as_array().cloned().unwrap_or_default();
            let offset = args["offset"].as_u64().unwrap_or(0).min(usize::MAX as u64) as usize;
            let rows: Vec<Value> = all.iter().skip(offset).take(8).map(|f| json!({"id": f["id"], "name": clip(text(f, "name"), 80), "role": clip(text(f, "role"), 24)})).collect();
            json!({"folders": rows, "cached": true, "total": all.len(), "next_offset": (offset.saturating_add(8) < all.len()).then_some(offset.saturating_add(8))})
        }
        "list" => {
            let box_ = store.mailbox(account, folder);
            let messages = box_["messages"].as_array().cloned().unwrap_or_default();
            let offset = args["offset"].as_u64().unwrap_or(0).min(usize::MAX as u64) as usize;
            let limit = args["limit"].as_u64().unwrap_or(5).clamp(1, 5) as usize;
            let rows: Vec<Value> = messages.iter().skip(offset).take(limit).map(|m| json!({
                "id": clip(text(m, "id"), 128), "sender": clip(text(m, "sender"), 64),
                "address": clip(text(m, "address"), 120), "subject": clip(text(m, "subject"), 160),
                "date": clip(text(m, "date"), 48), "unread": m["unread"]
            })).collect();
            json!({"folder": folder, "total": messages.len(), "messages": rows, "next_offset": (offset.saturating_add(limit) < messages.len()).then_some(offset.saturating_add(limit))})
        }
        "peek" => {
            let box_ = store.mailbox(account, folder);
            let message = box_["messages"].as_array().and_then(|rows| rows.iter().find(|m| text(m, "id") == text(args, "message"))).ok_or("There is no such message.")?;
            // Body pages are UTF-8 byte bounded; no raw HTML or attachment bytes.
            let body = text(message, "body");
            let offset = args["offset"].as_u64().unwrap_or(0).min(usize::MAX as u64) as usize;
            if offset > body.len() || !body.is_char_boundary(offset) { return Err("Body offset is not a UTF-8 boundary".into()); }
            let part = clip(&body[offset..], 2000);
            let end = offset + part.len();
            json!({"id": message["id"], "account": account, "folder": folder,
                "sender": clip(text(message, "sender"), 80), "address": clip(text(message, "address"), 160),
                "subject": clip(text(message, "subject"), 200), "date": clip(text(message, "date"), 48),
                "body": part, "unread": message["unread"], "next_offset": (end < body.len()).then_some(end)})
        }
        _ => return Err("Unknown Mail read method".into()),
    };
    if serde_json::to_vec(&result).map_err(|e| e.to_string())?.len() > 3800 { return Err("Mail result exceeds the agent response budget; use a smaller page".into()); }
    Ok(result)
}

fn publish_card(store: &Store, app: &str, args: &Value) -> Result<Value, String> {
    if app != "os.mail" { return Err("Only Mail publishes Mail cards".into()); }
    store.granted(app, text(args, "account"))?;
    let source = text(args, "source");
    let card_id = text(args, "card_id");
    if source.is_empty() || source.len() > 16 * 1024 { return Err("Provide L0 source of at most 16 KiB".into()); }
    if card_id.is_empty() || card_id.len() > 64 || !card_id.bytes().all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c)) { return Err("Invalid card_id".into()); }
    let title = text(args, "title");
    if title.trim().is_empty() || title.chars().count() > 80 || text(args, "summary").chars().count() > 200 { return Err("Card title must be 1–80 characters and summary at most 200 characters".into()); }
    let data = args.get("data").cloned().unwrap_or(json!({}));
    if !data.is_object() || serde_json::to_vec(&data).map_err(|e| e.to_string())?.len() > 32 * 1024 { return Err("Card data must be an object of at most 32 KiB".into()); }
    let publisher = card_publisher().lock().unwrap_or_else(|e| e.into_inner()).clone().ok_or("This device shows no glance cards.")?;
    let mut payload = json!({"source": source, "data": data, "card_id": card_id, "title": title,
        "summary": text(args, "summary"), "priority": args["priority"].as_i64().unwrap_or(60).clamp(0, 100),
        "notify": args["notify"].as_bool().unwrap_or(true), "open": {"app": "mail"}});
    if let Some(id) = args.get("draft_id") {
        let id = id.as_str().ok_or("draft_id must be text")?;
        payload["mail_binding"] = drafts::publication_binding(store, app, text(args,"account"), id)?;
        // An incoming-event card cannot acquire a draft for a different email.
        if card_id.strip_prefix("mail-").is_some_and(|id| id.len() == 40 && id.bytes().all(|b| b.is_ascii_hexdigit())) {
            let source = &payload["mail_binding"]["source_message"];
            if card_id != incoming::event_id(text(args, "account"), text(source, "folder"), text(source, "message")) {
                return Err("The reply draft does not belong to this incoming email card".into());
            }
        }
    }
    // Fingerprint normalized host payload, including its authoritative binding.
    // A deliberate repair keeps the card id but must reach the renderer again.
    let fingerprint = network::hash(&serde_json::to_string(&payload).map_err(|e| e.to_string())?);
    incoming::publish_revision(store, app, text(args, "account"), card_id, &fingerprint, payload.get("mail_binding"), || publisher(app, payload.clone()))
}

/// How mail moves: IMAP or POP3 and SMTP in the shell, or a fake in tests.
pub trait Transport: Send + Sync {
    /// Sign in and out again: the account works.
    fn test(&self, account: &Value) -> Result<(), String>;
    fn folders(&self, account: &Value) -> Result<Vec<Value>, String>;
    /// Mail in `folder` that `state` has not seen, newest first: `{messages,
    /// state, reset}`. `state` is what the last fetch returned (or `{}`);
    /// `reset` says the folder must be fetched afresh.
    fn fetch(&self, account: &Value, folder: &str, state: &Value) -> Result<Value, String>;
    /// Tell the server a message was read, where it keeps that.
    fn mark_seen(&self, account: &Value, folder: &str, message: &Value) -> Result<(), String>;
    fn send(&self, account: &Value, draft: &Value) -> Result<Value, String>;
    /// An unclassified backend error is conservatively uncertain. Only a
    /// backend that knows SMTP submission has not begun may classify failure.
    fn send_checked(&self, account: &Value, draft: &Value) -> Result<Value, drafts::SendFailure> {
        self.send(account, draft).map_err(drafts::SendFailure::Unknown)
    }
    /// Move a message to the folder the server keeps for `role` (`archive`
    /// or `trash`). From the trash, or with no trash, delete it for good.
    fn move_message(&self, _account: &Value, _folder: &str, _message: &Value, _role: &str) -> Result<(), String> {
        Err("This account can't move messages.".into())
    }
    /// The name and bytes of a message's attachment number `index`.
    fn attachment(&self, _account: &Value, _folder: &str, _message: &Value, _index: usize) -> Result<(String, Vec<u8>), String> {
        Err("This attachment isn't available.".into())
    }
    /// Set or clear a message's flag (`seen` or `flagged`) where the server
    /// keeps one. A server that keeps none leaves it to this device.
    fn set_flag(&self, _account: &Value, _folder: &str, _message: &Value, _flag: &str, _on: bool) -> Result<(), String> {
        Ok(())
    }
    /// Move a message to the mailbox `target`, by its id.
    fn move_to(&self, _account: &Value, _folder: &str, _message: &Value, _target: &str) -> Result<(), String> {
        Err("This account can't move messages.".into())
    }
}

fn is_imap(account: &Value) -> bool {
    text(account, "protocol") == "imap"
}

fn inbox_only() -> Vec<Value> {
    vec![json!({"id": INBOX, "name": "Inbox", "role": "inbox"})]
}

/// IMAP or POP3 for reading, SMTP for sending: the native Mail app's code
/// and the service's IMAP client.
pub struct Network;

impl Transport for Network {
    fn test(&self, account: &Value) -> Result<(), String> {
        if is_imap(account) {
            imap::Imap::connect(account)?.logout();
            Ok(())
        } else {
            network::test(account).map(|_| ())
        }
    }
    fn folders(&self, account: &Value) -> Result<Vec<Value>, String> {
        if !is_imap(account) {
            return Ok(inbox_only());
        }
        let mut imap = imap::Imap::connect(account)?;
        let folders = imap.folders();
        imap.logout();
        folders
    }
    fn fetch(&self, account: &Value, folder: &str, state: &Value) -> Result<Value, String> {
        // A stalled resolver/server cannot hold the collection worker forever.
        // A timed-out fetch cannot write mailbox state, and no second fetch is
        // started until its network thread has actually unwound.
        use std::sync::atomic::{AtomicBool, Ordering};
        static ACTIVE: AtomicBool = AtomicBool::new(false);
        if ACTIVE.swap(true, Ordering::AcqRel) { return Err("A previous mail fetch is still finishing".into()); }
        let (account, folder, state) = (account.clone(), folder.to_string(), state.clone());
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let spawn = std::thread::Builder::new().name("mail-fetch".into()).spawn(move || {
            struct Release;
            impl Drop for Release { fn drop(&mut self) { ACTIVE.store(false, Ordering::Release); } }
            let _release = Release;
            let _ = tx.send(fetch_network(&account, &folder, &state));
        });
        if let Err(e) = spawn { ACTIVE.store(false, Ordering::Release); return Err(format!("Cannot start mail fetch: {e}")); }
        rx.recv_timeout(std::time::Duration::from_secs(90)).map_err(|_| "Mail collection timed out; no mailbox cursor was advanced".to_string())?
    }
    fn mark_seen(&self, account: &Value, folder: &str, message: &Value) -> Result<(), String> {
        let Some(uid) = message["imap_uid"].as_u64().filter(|_| is_imap(account)) else { return Ok(()) };
        let mut imap = imap::Imap::connect(account)?;
        let marked = imap.mark_seen(folder, uid);
        imap.logout();
        marked
    }
    fn send(&self, account: &Value, draft: &Value) -> Result<Value, String> {
        network::send(account, draft)
    }
    fn send_checked(&self, account: &Value, draft: &Value) -> Result<Value, drafts::SendFailure> {
        network::send_checked(account, draft)
    }
    fn move_message(&self, account: &Value, folder: &str, message: &Value, role: &str) -> Result<(), String> {
        if !is_imap(account) {
            // POP3 keeps one mailbox and no other: all it can do is delete.
            if role != "trash" {
                return Err("This account has no Archive mailbox.".into());
            }
            return network::pop_delete(account, text(message, "uid"));
        }
        let uid = message["imap_uid"].as_u64().ok_or("This message is no longer on the server.")?;
        let mut imap = imap::Imap::connect(account)?;
        let moved = imap.move_message(folder, uid, role);
        imap.logout();
        moved
    }
    fn set_flag(&self, account: &Value, folder: &str, message: &Value, flag: &str, on: bool) -> Result<(), String> {
        let Some(uid) = message["imap_uid"].as_u64().filter(|_| is_imap(account)) else { return Ok(()) };
        let mut imap = imap::Imap::connect(account)?;
        let set = imap.set_flag(folder, uid, flag, on);
        imap.logout();
        set
    }
    fn move_to(&self, account: &Value, folder: &str, message: &Value, target: &str) -> Result<(), String> {
        if !is_imap(account) {
            return Err("This account has only an inbox.".into());
        }
        let uid = message["imap_uid"].as_u64().ok_or("This message is no longer on the server.")?;
        let mut imap = imap::Imap::connect(account)?;
        let moved = imap.move_to(folder, uid, target);
        imap.logout();
        moved
    }
    fn attachment(&self, account: &Value, folder: &str, message: &Value, index: usize) -> Result<(String, Vec<u8>), String> {
        let raw = if is_imap(account) {
            let uid = message["imap_uid"].as_u64().ok_or("This message is no longer on the server.")?;
            let mut imap = imap::Imap::connect(account)?;
            let raw = imap.raw_message(folder, uid);
            imap.logout();
            raw?
        } else {
            network::pop_raw(account, text(message, "uid"))?
        };
        network::attachment_of(&raw, index)
    }
}

fn fetch_network(account: &Value, folder: &str, state: &Value) -> Result<Value, String> {
    if is_imap(account) {
        let mut imap = imap::Imap::connect(account)?;
        let fetched = imap.fetch(folder, state, 25);
        imap.logout();
        return fetched;
    }
    if folder != INBOX {
        return Err("This account reads the inbox only.".into());
    }
    let mut seen: Vec<Value> = state["seen"].as_array().cloned().unwrap_or_default();
    let known: HashSet<String> = seen.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
    let fetched = network::fetch(account, &known)?;
    let messages = fetched["messages"].as_array().cloned().unwrap_or_default();
    seen = pop_seen(state, &fetched);
    Ok(json!({"messages": messages, "state": {"seen": seen}, "reset": false}))
}

fn pop_seen(state: &Value, fetched: &Value) -> Vec<Value> {
    let mut seen = state["seen"].as_array().cloned().unwrap_or_default();
    seen.extend(fetched["messages"].as_array().into_iter().flatten().map(|m| m["uid"].clone()));
    seen.extend(fetched["skipped_uids"].as_array().cloned().unwrap_or_default());
    if state["_incoming_baseline"] == true {
        seen.extend(fetched["uid_snapshot"].as_array().cloned().unwrap_or_default());
    }
    let mut unique = HashSet::new();
    seen.retain(|uid| uid.as_str().is_some_and(|s| !s.is_empty() && unique.insert(s.to_string())));
    seen
}

/// The accounts `app_id` was granted, in the order the person signed in to
/// them (`signed_in`, seconds; an account kept before that field counts as
/// the oldest), from the service's state under `host_dir` (the Card
/// runner's `<apps root>/.host`).
pub fn account_ids(host_dir: &Path, app_id: &str) -> Vec<String> {
    let accounts = std::fs::read(host_dir.join("mail").join("accounts.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<Value>>(&b).ok())
        .unwrap_or_default();
    let mut granted: Vec<(u64, String)> = accounts
        .iter()
        .filter(|a| a["apps"].as_array().is_some_and(|apps| apps.iter().any(|x| x == app_id)))
        .map(|a| (a["signed_in"].as_u64().unwrap_or(0), text(a, "id").to_owned()))
        .filter(|(_, id)| !id.is_empty())
        .collect();
    granted.sort_by_key(|(t, _)| *t);
    granted.into_iter().map(|(_, id)| id).collect()
}

/// The app's active account: the one the person signed in to last. The
/// shell binds the app's one agent to it (ADR 0004 §11).
pub fn active_account(host_dir: &Path, app_id: &str) -> Option<String> {
    account_ids(host_dir, app_id).pop()
}

fn secrets_slot() -> &'static Mutex<Option<PathBuf>> {
    static SLOT: std::sync::OnceLock<Mutex<Option<PathBuf>>> = std::sync::OnceLock::new();
    SLOT.get_or_init(Default::default)
}

/// The host's secrets folder for Mail's passwords (`<home>/secrets/os.mail/`,
/// ADR 0004 §11): the shell names it at startup, before any request. Without
/// one they stay in `<host dir>/mail/secrets/`.
pub fn set_secrets_dir(dir: Option<PathBuf>) {
    *secrets_slot().lock().unwrap_or_else(|e| e.into_inner()) = dir;
}

fn secrets_dir() -> Option<PathBuf> {
    secrets_slot().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Offer the service to the Card runner, over the network, with passwords in
/// the platform's secret store.
pub fn register() {
    register_with(Arc::new(Network));
}

/// Offer the service over a demo mailbox: any address, the password `demo`,
/// a few sample messages in its folders, and sends that go nowhere. For
/// developing and showing the Mail app without a real account. The demo
/// password is no secret, so it stays in a file rather than the keychain.
pub fn register_demo() {
    register_with_vault(Arc::new(DemoTransport::default()), Arc::new(vault::FileVault));
}

#[derive(Default)]
pub struct DemoTransport {
    sent: Mutex<usize>,
    /// What was archived or put in the trash, by the folder it went to.
    moved: Mutex<std::collections::HashMap<String, Vec<Value>>>,
    /// Moves to a named mailbox so far: each gives the message a new uid
    /// there, as a server does, so it can go back where it was seen before.
    renumbered: Mutex<usize>,
}

const DEMO_WELCOME: &str = r#"<html><head><style>.x{color:red}</style></head><body><table><tr><td>
<h1>Welcome to <b>Mail</b></h1>
<p>Mail never sees your password. OctoSense keeps it for you.</p>
<ul><li>Accounts over <b>IMAP</b> show every folder.</li><li>Your password stays on this device and goes only to your mail server.</li></ul>
<p>Read more at <a href="https://octosense.dev/mail">octosense.dev/mail</a>.<img src="https://tracker.example/p.gif"></p>
</td></tr></table></body></html>"#;

impl Transport for DemoTransport {
    fn test(&self, account: &Value) -> Result<(), String> {
        // A wrong password is turned away as a real server turns it away.
        if text(account, "password") == "demo" { Ok(()) } else { Err(imap::rejected()) }
    }
    fn folders(&self, account: &Value) -> Result<Vec<Value>, String> {
        self.test(account)?;
        let mut folders = inbox_only();
        folders.push(json!({"id": "Sent", "name": "Sent", "role": "sent"}));
        folders.push(json!({"id": "Archive", "name": "Archive", "role": "archive"}));
        folders.push(json!({"id": "Trash", "name": "Trash", "role": "trash"}));
        Ok(folders)
    }
    fn fetch(&self, account: &Value, folder: &str, state: &Value) -> Result<Value, String> {
        self.test(account)?;
        let samples: &[(&str, &str, &str, &str, &str, &str)] = if folder == INBOX {
            &[
                ("demo-1", "Rose Chen", "rose@example.com", "Dinner on Saturday?", "We are thinking of trying the new place on Market Street around seven. Are you in?", ""),
                ("demo-2", "OctoSense", "hello@octosense.dev", "Welcome to Mail", "", DEMO_WELCOME),
                ("demo-3", "Noah Park", "noah@example.com", "Photos from the hike", "I put the good ones in the shared album. The view from the ridge came out great.", ""),
            ]
        } else if folder == "Sent" {
            &[("demo-sent-1", "Me", "me@example.com", "Re: Photos from the hike", "They look great, thanks!", "")]
        } else {
            &[]
        };
        let mut seen: Vec<Value> = state["seen"].as_array().cloned().unwrap_or_default();
        let me = text(account, "address");
        let mut messages: Vec<Value> = samples
            .iter()
            .filter(|(uid, ..)| !seen.iter().any(|s| s == uid))
            .map(|(uid, sender, address, subject, body, html)| {
                let mut message = json!({"id": &network::hash(uid)[..24], "uid": uid, "sender": sender, "address": address, "subject": subject,
                    "body": if body.is_empty() { "(No readable message body)" } else { body }, "preview": body, "html": html,
                    "to": if folder == INBOX { me } else { "noah@example.com" }, "time": "Sep 25", "date": "2026-09-25T09:00:00Z", "unread": folder == INBOX});
                // Rose writes to two people and copies a third; Noah attaches a file.
                if *uid == "demo-1" {
                    message["to"] = json!(format!("{me}, Sam Ortiz <sam@example.com>"));
                    message["cc"] = json!("Maya Lin <maya@example.com>");
                }
                if *uid == "demo-3" {
                    message["attachment_items"] = json!([{"filename": "Ridge trail notes.txt", "mime": "text/plain", "size": 48}]);
                }
                message
            })
            .collect();
        let moved = self.moved.lock().unwrap().get(folder).cloned().unwrap_or_default();
        messages.extend(moved.into_iter().filter(|m| !seen.iter().any(|s| *s == m["uid"])));
        seen.extend(messages.iter().map(|m| m["uid"].clone()));
        Ok(json!({"messages": messages, "state": {"seen": seen}, "reset": false}))
    }
    fn mark_seen(&self, _account: &Value, _folder: &str, _message: &Value) -> Result<(), String> {
        Ok(())
    }
    fn send(&self, account: &Value, _draft: &Value) -> Result<Value, String> {
        self.test(account)?;
        *self.sent.lock().unwrap() += 1;
        Ok(json!({"accepted": true}))
    }
    fn move_message(&self, account: &Value, folder: &str, message: &Value, role: &str) -> Result<(), String> {
        self.test(account)?;
        let target = if role == "trash" { "Trash" } else { "Archive" };
        if folder != target {
            self.moved.lock().unwrap().entry(target.to_string()).or_default().push(message.clone());
        }
        Ok(())
    }
    fn move_to(&self, account: &Value, folder: &str, message: &Value, target: &str) -> Result<(), String> {
        if !self.folders(account)?.iter().any(|f| text(f, "id") == target) {
            return Err("That mailbox is no longer on the mail server.".into());
        }
        if folder != target {
            let mut count = self.renumbered.lock().unwrap();
            *count += 1;
            let mut moved = message.clone();
            moved["uid"] = json!(format!("{}~{}", text(message, "uid"), *count));
            self.moved.lock().unwrap().entry(target.to_string()).or_default().push(moved);
        }
        Ok(())
    }
    fn attachment(&self, account: &Value, _folder: &str, message: &Value, index: usize) -> Result<(String, Vec<u8>), String> {
        self.test(account)?;
        let name = text(&message["attachment_items"][index], "filename");
        if name.is_empty() {
            return Err("This attachment is no longer in the message.".into());
        }
        Ok((name.to_string(), format!("From the demo mailbox: {name}\n").into_bytes()))
    }
}

pub fn register_with(transport: Arc<dyn Transport>) {
    register_with_vault(transport, vault::platform());
}

pub fn register_with_vault(transport: Arc<dyn Transport>, vault: Arc<dyn Vault>) {
    incoming::set_backend(transport.clone(), vault.clone());
    drafts::set_backend(transport.clone(), vault.clone());
    octosense_appstore::services::register_host_service(Box::new(MailService { transport, vault, pending: Arc::default(), known: None, staged: Arc::default() }));
}

pub struct MailService {
    transport: Arc<dyn Transport>,
    vault: Arc<dyn Vault>,
    /// The app waiting on a sign-in, and where its answer goes. Shared with
    /// the worker that tests an account: it answers on success, and leaves
    /// the app waiting on a failure, so the person can fix the form.
    pending: Arc<Mutex<Option<(String, Replier)>>>,
    /// The account a sign-in is reconnecting: its settings, never a password.
    known: Option<Value>,
    /// An attachment fetched for saving, until the UI links it into the
    /// app's storage between script turns: `(path, name, staged file)`.
    staged: Arc<Mutex<Option<(String, String, PreparedStorageImport)>>>,
}

const ACCOUNT_FIELDS: [&str; 10] = ["address", "username", "protocol", "host", "port", "security", "smtp_host", "smtp_port", "smtp_security", "id"];

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

struct Store {
    dir: PathBuf,
    place: vault::Place,
    vault: Arc<dyn Vault>,
}

impl Store {
    fn at(host_dir: &Path, vault: Arc<dyn Vault>) -> Self {
        let dir = host_dir.join("mail");
        let place = vault::Place::resolve(&dir, secrets_dir().as_deref());
        Store { dir, place, vault }
    }

    fn accounts(&self) -> Vec<Value> {
        std::fs::read(self.dir.join("accounts.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<Value>>(&b).ok())
            .unwrap_or_default()
    }

    fn save_accounts(&self, accounts: &[Value]) -> Result<(), String> {
        write_atomic(&self.dir.join("accounts.json"), &serde_json::to_vec_pretty(accounts).unwrap())
    }

    /// An account this app was granted, with its password, ready for the
    /// transport; or why not.
    fn account_for(&self, app_id: &str, id: &str) -> Result<Value, String> {
        let account = self.granted(app_id, id)?;
        let mut full = account.clone();
        full["password"] = json!(self.vault.get(&self.place, id)?);
        Ok(full)
    }

    /// The account, without its password, if this app was granted it.
    fn granted(&self, app_id: &str, id: &str) -> Result<Value, String> {
        let account = self
            .accounts()
            .into_iter()
            .find(|a| text(a, "id") == id)
            .ok_or("There is no such account.")?;
        let granted = account["apps"].as_array().is_some_and(|apps| apps.iter().any(|a| a == app_id));
        if !granted {
            return Err("This app may not use that account.".into());
        }
        Ok(account)
    }

    fn mailbox_path(&self, id: &str, folder: &str) -> PathBuf {
        mailbox_file(&self.dir, id, folder)
    }

    fn mailbox(&self, id: &str, folder: &str) -> Value {
        let mut mailbox = std::fs::read(self.mailbox_path(id, folder))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .unwrap_or_else(|| json!({"messages": []}));
        if mailbox.get("state").is_none() {
            // Older builds kept POP3's seen list at the top.
            mailbox["state"] = json!({"seen": mailbox.get("seen").cloned().unwrap_or(json!([]))});
        }
        mailbox
    }

    fn save_mailbox(&self, id: &str, folder: &str, mailbox: &Value) -> Result<(), String> {
        write_atomic(&self.mailbox_path(id, folder), &serde_json::to_vec(mailbox).unwrap())
    }

    fn folders(&self, id: &str) -> Option<Value> {
        std::fs::read(self.dir.join(format!("folders-{id}.json"))).ok().and_then(|b| serde_json::from_slice(&b).ok())
    }

    fn forget(&self, id: &str) {
        drafts::forget(self, id);
        self.vault.remove(&self.place, id);
        contacts::forget(&self.dir, id);
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name == format!("box-{id}.json") || name.starts_with(&format!("box-{id}-")) || name == format!("folders-{id}.json") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// Where one folder's fetched mail is kept; the inbox keeps the name older
/// builds gave it.
fn mailbox_file(dir: &Path, id: &str, folder: &str) -> PathBuf {
    if folder == INBOX {
        dir.join(format!("box-{id}.json"))
    } else {
        dir.join(format!("box-{id}-{}.json", &network::hash(folder)[..12]))
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot store mail: {e}"))?;
    }
    let temp = path.with_extension("tmp");
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp).map_err(|e| format!("Cannot store mail: {e}"))?;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| format!("Cannot store mail: {e}"))?;
    std::fs::rename(&temp, path).map_err(|e| format!("Cannot store mail: {e}"))?;
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        std::fs::File::open(parent).and_then(|f| f.sync_all()).map_err(|e| format!("Cannot flush mail directory: {e}"))?;
    }
    Ok(())
}

/// A fetched message as it is kept: its HTML rebuilt from safe tags, its
/// text from that when the mail had no text part, and no attachment bytes
/// or inline images (nothing the app is shown needs them).
fn normalize(mut message: Value) -> Value {
    let source = text(&message, "html").to_string();
    if !source.trim().is_empty() {
        let rebuilt = html::rebuild(&source);
        let body = text(&message, "body").to_string();
        // The decoder falls back to every text node, style sheets included,
        // when a mail has no text part; the rebuilt text reads better.
        let fallback = scraper::Html::parse_fragment(&source).root_element().text().collect::<Vec<_>>().join(" ");
        if (body == fallback || body == "(No readable message body)" || body.trim().is_empty()) && !rebuilt.text.is_empty() {
            message["preview"] = json!(rebuilt.text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(180).collect::<String>());
            message["body"] = json!(rebuilt.text);
        }
        message["html"] = json!(rebuilt.html);
    }
    if let Some(items) = message.get("attachment_items").and_then(Value::as_array).cloned() {
        let listed: Vec<Value> = items.iter().map(|a| json!({"filename": a["filename"], "mime": a["mime"], "size": a["size"]})).collect();
        message["attachment_items"] = json!(listed);
    }
    if let Some(object) = message.as_object_mut() {
        object.remove("inline_images");
    }
    message
}

/// What an app sees of a message in a list.
fn header(message: &Value) -> Value {
    json!({
        "id": message["id"], "sender": message["sender"], "address": message["address"],
        // Whom it was written to: Mail's Sent lists the recipient where others list the sender.
        "to": clip(text(message, "to"), 512),
        "subject": message["subject"], "preview": message["preview"], "time": message["time"],
        "unread": message["unread"].as_bool().unwrap_or(true),
        "flagged": message["flagged"].as_bool().unwrap_or(false),
    })
}

/// The account a sign-in form describes, or why it cannot be one.
fn account_from_form(form: &Value) -> Result<Value, String> {
    let mut account = network::defaults();
    let imap = text(form, "protocol") != "pop3";
    account["protocol"] = json!(if imap { "imap" } else { "pop3" });
    if imap {
        account["host"] = json!("imap.gmail.com");
        account["port"] = json!("993");
    }
    for key in ["address", "username", "password", "host", "port", "security", "smtp_host", "smtp_port", "smtp_security"] {
        if let Some(value) = form[key].as_str().filter(|v| !v.trim().is_empty()) {
            // A password is taken as typed: spaces can be part of it.
            account[key] = json!(if key == "password" { value } else { value.trim() });
        } else if let Some(value) = form[key].as_f64() {
            account[key] = json!(value.to_string());
        }
    }
    if text(&account, "username").is_empty() {
        account["username"] = account["address"].clone();
    }
    network::validate(&account)?;
    Ok(account)
}

/// A worker thread for slow work, so the UI thread never waits on a server.
fn work(f: impl FnOnce() + Send + 'static) {
    std::thread::spawn(f);
}

impl HostService for MailService {
    fn api_methods(&self) -> Vec<octosense_appstore::services::HostApiMethod> {
        public_review::api_methods()
    }
    fn family(&self) -> &'static str {
        "mail"
    }

    fn call(&mut self, call: ServiceCall, reply: Replier, host: &mut dyn ServiceHost) {
        let store = Store::at(&call.host_dir, self.vault.clone());
        let account_arg = text(&call.args, "account").to_string();
        let folder = Some(text(&call.args, "folder")).filter(|f| !f.is_empty()).unwrap_or(INBOX).to_string();
        match call.method() {
            "accounts" => {
                if !account_arg.is_empty() {
                    return reply.send(agent_read(&store, &call.app_id, "accounts", &call.args));
                }
                let mine: Vec<Value> = store
                    .accounts()
                    .into_iter()
                    .filter(|a| a["apps"].as_array().is_some_and(|apps| apps.iter().any(|x| x == call.app_id.as_str())))
                    .map(|a| json!({"id": a["id"], "address": a["address"]}))
                    .collect();
                reply.send(Ok(json!(mine)));
            }
            "add_account" => {
                // A sign-in needs the person on Mail's sheet. From a surface
                // that cannot show one (a home-screen tile, an agent's tool
                // call), refuse before touching a sign-in already under way.
                if !call.may_prompt {
                    reply.send(Err("Signing in needs Mail open: open Mail to add an account.".into()));
                    return;
                }
                // With an account this app holds, the sheet asks only what it can't know.
                self.known = None;
                if !account_arg.is_empty() {
                    match store.granted(&call.app_id, &account_arg) {
                        Ok(account) => self.known = Some(account),
                        Err(e) => return reply.send(Err(e)),
                    }
                }
                if let Some((_, earlier)) = self.pending.lock().unwrap().take() {
                    earlier.send(Err("Another sign-in replaced this one.".into()));
                }
                *self.pending.lock().unwrap() = Some((call.app_id.clone(), reply));
                host.open_sheet(signin_sheet(call.app_id.as_str()));
            }
            "sheet.known" => reply.send(Ok(self.known.as_ref().map_or_else(|| json!({"address": ""}), |a| json!({
                "address": a["address"], "username": a["username"], "protocol": a["protocol"],
                "host": a["host"], "port": a["port"], "smtp_host": a["smtp_host"], "smtp_port": a["smtp_port"],
            })))),
            "sheet.cancel" => {
                host.close_sheet();
                if let Some((_, pending)) = self.pending.lock().unwrap().take() {
                    pending.send(Err("Sign-in cancelled.".into()));
                }
                reply.send(Ok(json!({})));
            }
            "sheet.submit" => {
                let Some(app_id) = self.pending.lock().unwrap().as_ref().map(|(app, _)| app.clone()) else {
                    host.close_sheet();
                    reply.send(Err("No app is waiting for this sign-in.".into()));
                    return;
                };
                let account = match account_from_form(&call.args) {
                    Ok(account) => account,
                    Err(e) => return reply.send(Err(e)),
                };
                let (transport, pending) = (self.transport.clone(), self.pending.clone());
                work(move || {
                    if let Err(e) = transport.test(&account) {
                        return reply.send(Err(e));
                    }
                    let _guard = incoming::lock();
                    let _draft_guard = drafts::lock();
                    drafts::invalidate_locked(&call.host_dir, None);
                    let id = network::identity(&account);
                    let mut accounts = store.accounts();
                    let signed_in = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                    let mut kept = json!({"apps": [app_id], "id": id, "signed_in": signed_in});
                    for key in ACCOUNT_FIELDS.iter().filter(|k| **k != "id") {
                        kept[*key] = account[*key].clone();
                    }
                    match accounts.iter_mut().find(|a| text(a, "id") == id) {
                        Some(existing) => {
                            // Signing in again updates the settings and
                            // grants the account to this app too.
                            let mut apps = existing["apps"].as_array().cloned().unwrap_or_default();
                            if !apps.iter().any(|a| a == app_id.as_str()) {
                                apps.push(json!(app_id));
                            }
                            kept["apps"] = json!(apps);
                            *existing = kept;
                        }
                        None => accounts.push(kept),
                    }
                    let saved = store.vault.put(&store.place, &id, text(&account, "password")).and_then(|_| store.save_accounts(&accounts));
                    if let Err(e) = saved {
                        return reply.send(Err(e));
                    }
                    close_sheet_later(&app_id);
                    drop(_draft_guard);
                    account_event(AccountEvent::Added { app_id: app_id.clone(), account: id.clone() });
                    if let Some((_, waiting)) = pending.lock().unwrap().take() {
                        waiting.send(Ok(json!({"id": id, "address": account["address"]})));
                    }
                    reply.send(Ok(json!({})));
                });
            }
            "remove_account" => {
                work(move || {
                    let _guard = incoming::lock();
                    // Linearize account removal with a send claim, never with
                    // its network wait. Late receipts cannot recreate deleted data.
                    let _draft_guard = drafts::lock();
                    let mut accounts = store.accounts();
                    let mut removed = false;
                    if let Some(account) = accounts.iter_mut().find(|a| text(a, "id") == account_arg) {
                        if let Some(apps) = account["apps"].as_array_mut() {
                            let before = apps.len();
                            apps.retain(|a| a != call.app_id.as_str());
                            removed = apps.len() < before;
                        }
                    }
                    let orphaned: Vec<String> = accounts
                        .iter()
                        .filter(|a| a["apps"].as_array().is_none_or(|apps| apps.is_empty()))
                        .map(|a| text(a, "id").to_string())
                        .collect();
                    accounts.retain(|a| !orphaned.iter().any(|id| id == text(a, "id")));
                    let saved = store.save_accounts(&accounts);
                    // Keep the old account's local mail and credential until
                    // the new account list has been stored successfully.
                    if saved.is_ok() {
                        if removed {
                            drafts::invalidate_app_locked(&call.host_dir, &account_arg, &call.app_id);
                        }
                        for id in &orphaned {
                            store.forget(id);
                        }
                    }
                    if removed && saved.is_ok() {
                        drafts::forget_composers(&store, &call.app_id, &account_arg);
                    }
                    drop(_draft_guard);
                    if removed && saved.is_ok() {
                        account_event(AccountEvent::Removed { app_id: call.app_id.clone(), account: account_arg.clone() });
                    }
                    reply.send(saved.map(|_| json!({})));
                });
            }
            "folders" => {
                if !call.may_prompt { return reply.send(agent_read(&store, &call.app_id, "folders", &call.args)); }
                let account = match store.account_for(&call.app_id, &account_arg) {
                    Ok(account) => account,
                    Err(e) => return reply.send(Err(e)),
                };
                let transport = self.transport.clone();
                work(move || {
                    let answer = match transport.folders(&account) {
                        Ok(folders) => {
                            let folders = json!(folders);
                            let _guard = incoming::lock();
                            if let Err(e) = store.granted(&call.app_id, &account_arg) { return reply.send(Err(e)); }
                            let _ = write_atomic(&store.dir.join(format!("folders-{account_arg}.json")), folders.to_string().as_bytes());
                            Ok(folders)
                        }
                        // Offline: the folders the last listing found.
                        Err(e) => store.folders(&account_arg).ok_or(e),
                    };
                    reply.send(answer);
                });
            }
            "sync" => {
                if folder.len() > 256 { return reply.send(Err("Folder is too long".into())); }
                let transport = self.transport.clone();
                work(move || {
                    let result = incoming::collect(&store, transport.as_ref(), &call.app_id, &account_arg, &folder)
                        .map(|r| json!({"new": r.new, "total": r.total}));
                    reply.send(result);
                });
            }
            // Agent reads are separate from the UI's network listing and
            // message-open operation. They never fetch or mark a message read.
            "peek" => {
                reply.send(agent_read(&store, &call.app_id, call.method(), &call.args));
            }
            "skip_event" => work(move || reply.send(incoming::skip_event(&store, &call.app_id, &account_arg, text(&call.args, "event_id"), text(&call.args, "reason")))),
            "publish_card" => work(move || reply.send(publish_card(&store, &call.app_id, &call.args))),
            "propose_reply" | "draft" | "suggest_reply" | "propose_send" => {
                work(move || reply.send(drafts::agent_call(&store, &call.app_id, call.method(), &call.args)));
            }
            "compose" | "compose_status" => {
                let rejected = reply.clone();
                if let Err(error) = public_review::work(move || {
                    if !reply.is_pending() {
                        return;
                    }
                    if !call.may_prompt
                        && active_account(&call.host_dir, &call.app_id).as_deref()
                            != Some(account_arg.as_str())
                    {
                        return reply
                            .send(Err("The app agent must use its active Mail account".into()));
                    }
                    // Several people or a copy are the person's own composing: an agent names one.
                    if !call.may_prompt && (call.args.get("cc").is_some() || text(&call.args, "to").contains([',', ';'])) {
                        return reply.send(Err("An agent's message goes to one person.".into()));
                    }
                    let result = if call.method() == "compose" {
                        drafts::compose_draft(&store, &call.app_id, &call.args)
                    } else {
                        // Mail's window hears once that the host's sheet asked it to show Sent.
                        drafts::composer_status(&store, &call.app_id, &call.args).map(|mut d| {
                            if call.may_prompt {
                                let asked = drafts::take_show_sent(&call.app_id, &account_arg, text(&d, "draft_id"));
                                d["show_sent"] = json!(asked);
                            }
                            d
                        })
                    };
                    reply.send(result.map(|d| drafts::composer_result(&d, call.may_prompt)));
                }) {
                    rejected.send(Err(error));
                }
            }
            "sheet.review_close" => {
                if !call.from_sheet {
                    return reply.send(Err("Only the host review can close its sheet".into()));
                }
                host.close_sheet();
                reply.send(Ok(Value::Null));
            }
            "review_send" if call.app_id != "os.mail" => {
                public_review::open(&store, &call, reply, host)
            }
            "review_send" => {
                if !call.may_prompt { return reply.send(Err("Open Mail to review a composed message".into())); }
                work(move || reply.send(drafts::review_composer(&store, &call.app_id, &call.args)));
            }
            "list" => {
                if !call.may_prompt { return reply.send(agent_read(&store, &call.app_id, "list", &call.args)); }
                if let Err(e) = store.granted(&call.app_id, &account_arg) {
                    return reply.send(Err(e));
                }
                let mailbox = store.mailbox(&account_arg, &folder);
                let messages = mailbox["messages"].as_array().cloned().unwrap_or_default();
                let offset = call.args["offset"].as_f64().unwrap_or(0.0).max(0.0) as usize;
                let limit = call.args["limit"].as_f64().unwrap_or(50.0).clamp(1.0, 200.0) as usize;
                let page: Vec<Value> = messages.iter().skip(offset).take(limit).map(header).collect();
                reply.send(Ok(json!({"folder": folder, "total": messages.len(), "messages": page})));
            }
            "message" | "mark_read" => {
                let transport = self.transport.clone();
                work(move || {
                    let _guard = incoming::lock();
                    if let Err(e) = store.granted(&call.app_id, &account_arg) {
                        return reply.send(Err(e));
                    }
                    let wanted = text(&call.args, "message").to_string();
                    let mut mailbox = store.mailbox(&account_arg, &folder);
                    let Some(message) = mailbox["messages"]
                        .as_array_mut()
                        .and_then(|m| m.iter_mut().find(|m| text(m, "id") == wanted))
                    else {
                        return reply.send(Err("There is no such message.".into()));
                    };
                    let was_unread = message["unread"].as_bool().unwrap_or(true);
                    message["unread"] = json!(false);
                    let answer = if call.method() == "message" {
                        let attachments: Vec<Value> = message["attachment_items"]
                            .as_array()
                            .map(|items| items.iter().map(|a| json!({"filename": a["filename"], "size": a["size"]})).collect())
                            .unwrap_or_default();
                        let me = store.accounts().iter().find(|a| text(a, "id") == account_arg).map(|a| text(a, "address").to_string()).unwrap_or_default();
                        let (reply_to, others) = reply_people(message, &me);
                        json!({
                            "id": message["id"], "sender": message["sender"], "address": message["address"],
                            "to": text(message, "to"), "cc": text(message, "cc"),
                            "reply_to": reply_to.join(", "), "reply_all_cc": others.join(", "),
                            "subject": message["subject"], "body": message["body"], "html": text(message, "html"),
                            "attachments": attachments, "date": message["date"], "time": message["time"],
                        })
                    } else {
                        json!({})
                    };
                    let seen = message.clone();
                    let _ = store.save_mailbox(&account_arg, &folder, &mailbox);
                    reply.send(Ok(answer));
                    // The server hears too, where it keeps a read flag; failing
                    // that is not worth an error: the next sync is unaffected.
                    if was_unread {
                        if let Ok(account) = store.account_for(&call.app_id, &account_arg) {
                            work(move || {
                                let _ = transport.mark_seen(&account, &folder, &seen);
                            });
                        }
                    }
                });
            }
            // Archive and delete, and saving a file, are the person's own
            // presses in Mail: never an agent's tool, never another app's.
            "move" | "mark" | "attachment" | "attachment_keep" if call.app_id != "os.mail" || !call.may_prompt => {
                reply.send(Err("Only Mail, in the person's hands, can do this.".into()));
            }
            "move" => {
                let role = text(&call.args, "to").to_string();
                // Or a mailbox by its id: Move to, from a row's menu.
                let target = text(&call.args, "target").to_string();
                if target.is_empty() && !matches!(role.as_str(), "archive" | "trash") {
                    return reply.send(Err("A message moves to the archive, the trash or another mailbox.".into()));
                }
                if !target.is_empty() && (target == folder || target.len() > 256) {
                    return reply.send(Err("Pick another mailbox to move this message to.".into()));
                }
                let transport = self.transport.clone();
                work(move || {
                    let wanted = text(&call.args, "message");
                    let (account, message) = match held_message(&store, &call.app_id, &account_arg, &folder, wanted) {
                        Ok(held) => held,
                        Err(e) => return reply.send(Err(e)),
                    };
                    // The server is waited for with nothing locked.
                    let moved = if target.is_empty() {
                        transport.move_message(&account, &folder, &message, &role)
                    } else {
                        transport.move_to(&account, &folder, &message, &target)
                    };
                    if let Err(e) = moved {
                        return reply.send(Err(e));
                    }
                    let _guard = incoming::lock();
                    let mut mailbox = store.mailbox(&account_arg, &folder);
                    if let Some(messages) = mailbox["messages"].as_array_mut() {
                        messages.retain(|m| text(m, "id") != wanted);
                    }
                    if let Some(events) = mailbox["pending_events"].as_array_mut() {
                        events.retain(|e| text(e, "message") != wanted);
                    }
                    reply.send(store.save_mailbox(&account_arg, &folder, &mailbox).map(|_| json!({"moved": true})));
                });
            }
            // Mark as Unread (or Read) and Flag, from a row's menu: this device's
            // copy at once, then the server where it keeps the flag, as a read
            // message is marked.
            "mark" => {
                let unread = call.args.get("unread").and_then(Value::as_bool);
                let flagged = call.args.get("flagged").and_then(Value::as_bool);
                if unread.is_none() && flagged.is_none() {
                    return reply.send(Err("Say what to mark: unread or flagged.".into()));
                }
                let transport = self.transport.clone();
                work(move || {
                    let wanted = text(&call.args, "message").to_string();
                    let marked = {
                        let _guard = incoming::lock();
                        if let Err(e) = store.granted(&call.app_id, &account_arg) {
                            return reply.send(Err(e));
                        }
                        let mut mailbox = store.mailbox(&account_arg, &folder);
                        let Some(message) = mailbox["messages"].as_array_mut().and_then(|m| m.iter_mut().find(|m| text(m, "id") == wanted)) else {
                            return reply.send(Err("There is no such message.".into()));
                        };
                        if let Some(on) = unread {
                            message["unread"] = json!(on);
                        }
                        if let Some(on) = flagged {
                            message["flagged"] = json!(on);
                        }
                        let marked = message.clone();
                        if let Err(e) = store.save_mailbox(&account_arg, &folder, &mailbox) {
                            return reply.send(Err(e));
                        }
                        marked
                    };
                    reply.send(Ok(json!({"marked": true})));
                    if let Ok(account) = store.account_for(&call.app_id, &account_arg) {
                        if let Some(on) = unread {
                            let _ = transport.set_flag(&account, &folder, &marked, "seen", !on);
                        }
                        if let Some(on) = flagged {
                            let _ = transport.set_flag(&account, &folder, &marked, "flagged", on);
                        }
                    }
                });
            }
            "attachment" => {
                let Some(storage) = splash_storage::storage_for_heap(reply.isolate_key(), &call.app_id) else {
                    return reply.send(Err("Mail's storage isn't available.".into()));
                };
                let Some(index) = call.args["index"].as_u64() else {
                    return reply.send(Err("index must be a number".into()));
                };
                let snapshot = storage.worker_snapshot();
                let (transport, staged) = (self.transport.clone(), self.staged.clone());
                work(move || {
                    let wanted = text(&call.args, "message");
                    let (account, message) = match held_message(&store, &call.app_id, &account_arg, &folder, wanted) {
                        Ok(held) => held,
                        Err(e) => return reply.send(Err(e)),
                    };
                    let (name, bytes) = match transport.attachment(&account, &folder, &message, index as usize) {
                        Ok(found) => found,
                        Err(e) => return reply.send(Err(e)),
                    };
                    let name = attachment_name(&name);
                    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
                    let path = format!("attachments/{stamp}");
                    reply.send(snapshot.prepare_import(&path, &bytes).map(|prepared| {
                        *staged.lock().unwrap() = Some((path, name.clone(), prepared));
                        json!({"name": name, "size": bytes.len()})
                    }));
                });
            }
            "attachment_keep" => {
                let Some((path, name, prepared)) = self.staged.lock().unwrap().take() else {
                    return reply.send(Err("No attachment is waiting to be saved.".into()));
                };
                let Some(storage) = splash_storage::storage_for_heap(reply.isolate_key(), &call.app_id) else {
                    return reply.send(Err("Mail's storage isn't available.".into()));
                };
                reply.send(storage.commit_import(prepared).map(|_| json!({"path": path, "name": name})));
            }
            "send" if call.app_id != "os.mail" => public_review::open(&store, &call, reply, host),
            "send" => {
                reply.send(Err("approval_required: use mail.review_send with Mail open, or open the reply card, then use the host's Approve & Send control. mail.send cannot authorize delivery.".into()));
            }
            "notify" => {
                if account_arg.is_empty() { reply.send(notify(&call.app_id, &call.args)); }
                else { work(move || reply.send(incoming::publish_once(&store, &call.app_id, &account_arg, text(&call.args, "card_id"), || notify(&call.app_id, &call.args)))); }
            }
            other => reply.send(Err(format!("mail has no method {other:?}"))),
        }
    }
}

/// The account, with its password, and a message this app may act on.
fn held_message(store: &Store, app_id: &str, account: &str, folder: &str, wanted: &str) -> Result<(Value, Value), String> {
    let _guard = incoming::lock();
    let held = store.account_for(app_id, account)?;
    let mailbox = store.mailbox(account, folder);
    let message = mailbox["messages"].as_array().and_then(|m| m.iter().find(|m| text(m, "id") == wanted)).cloned();
    Ok((held, message.ok_or("There is no such message.")?))
}

/// Whom a reply goes to, and whom else a Reply All copies: plain addresses,
/// nobody twice, and never the account itself.
fn reply_people(message: &Value, me: &str) -> (Vec<String>, Vec<String>) {
    fn plain(list: &str) -> Vec<String> {
        let Ok(parsed) = mailparse::addrparse(list) else { return Vec::new() };
        parsed
            .iter()
            .flat_map(|one| match one {
                mailparse::MailAddr::Single(single) => vec![single.addr.clone()],
                mailparse::MailAddr::Group(group) => group.addrs.iter().map(|single| single.addr.clone()).collect(),
            })
            .collect()
    }
    let reply_to = text(message, "reply_to");
    let mut to = plain(if reply_to.trim().is_empty() { text(message, "address") } else { reply_to });
    let (written_to, copied) = (plain(text(message, "to")), plain(text(message, "cc")));
    let mine = |one: &String| one.eq_ignore_ascii_case(me);
    // A reply to one's own message goes on to whom it went to.
    if to.iter().all(mine) {
        to = written_to.iter().filter(|one| !mine(one)).cloned().collect();
    }
    let mut seen: HashSet<String> = to.iter().map(|one| one.to_ascii_lowercase()).collect();
    seen.insert(me.to_ascii_lowercase());
    let others = written_to.into_iter().chain(copied).filter(|one| seen.insert(one.to_ascii_lowercase())).collect();
    (to, others)
}

/// A name a save panel may offer: the last part of what the sender wrote,
/// without what could disguise it, keeping its extension when it must be cut.
fn attachment_name(raw: &str) -> String {
    let leaf = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = leaf
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}'))
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.');
    if cleaned.is_empty() {
        return "attachment".into();
    }
    const MOST: usize = 128;
    if cleaned.len() <= MOST {
        return cleaned.to_string();
    }
    let extension = cleaned.rfind('.').map(|at| &cleaned[at..]).filter(|e| e.len() <= 16).unwrap_or("");
    let mut end = MOST - extension.len();
    while !cleaned.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{extension}", cleaned[..end].trim_end())
}

/// The host's sign-in sheet: a Splash program run in its own isolate, over
/// the app. What is typed here reaches the service, never the app.
fn signin_sheet(app_id: &str) -> String {
    // Who gets what: Mail is the person's own mail app; another app that asks only
    // gets the mail, never the password.
    let note = if app_id == "os.mail" {
        "Enter your email address and password. Your password stays on this device and goes only to your mail server."
    } else {
        "OctoSense keeps your password. The app that asked only gets your mail."
    };
    r##"// A phone's sheet keeps its fixed look; a desktop's follows the appearance at a Mac's metrics.
let desk = try { theme.ui_pointer == true } catch { false }
let desk_dark = desk && dot(mod.theme.color_text, #x36b61200) >= 0.5
fn by(touch, pointer){ if desk { pointer } else { touch } }
fn tone(touch, light, dark){ if !desk { touch } else if desk_dark { dark } else { light } }
let sh_card = tone(#xffffff, #xf6f6f8, #x2c2c2e)
let sh_ink = tone(#x1c1c1e, #x1d1d1f, #xf5f5f7)
let sh_ink2 = tone(#x3a3a3c, #x3a3a3c, #xd1d1d6)
let sh_ink3 = tone(#x6e6e73, #x636368, #xa1a1a6)
let sh_muted = tone(#x8e8e93, #x636368, #xa1a1a6)
let sh_faint = tone(#xaeaeb2, #x8e8e93, #x8e8e93)
let sh_accent = tone(#x007aff, #x0071e3, #x0071e3)
let sh_accent_hover = tone(#x0a84ff, #x006edb, #x006edb)
let sh_accent_down = tone(#x0062cc, #x0060c0, #x0060c0)
let sh_link = tone(#x007aff, #x0066cc, #x2997ff)
let sh_field = tone(#xf2f2f7, #xffffff, #x3a3a3c)
let sh_field_line = tone(#x00000000, #xc9c9ce, #x545458)
let sh_line = tone(#xe5e5ea, #xe3e3e8, #x48484a)
let sh_line2 = tone(#xd1d1d6, #xd1d1d6, #x545458)
let sh_danger = tone(#xff3b30, #xd70015, #xff7b72)
let sh_ok = tone(#x248a3d, #x1f7f36, #x30d158)
let sh_wash = tone(#x0000000a, #x0000000a, #xffffff14)
let sh_wash2 = tone(#x00000014, #x00000014, #xffffff24)
// A sheet's dim, the shell's for every app on a desktop: 14% black in the light, 32% in the dark.
let sh_dim = tone(#x000000aa, #x00000024, #x00000052)
fn sh_fs(n){
    if !desk { return n }
    if n <= 11 { return 8.25 }
    if n <= 12 { return 9 }
    if n <= 16 { return 9.75 }
    if n <= 17 { return 11.25 }
    if n <= 20 { return 12.75 }
    16.5
}
fn sh_h(n){
    if !desk { return n }
    if n >= 50 { return 32 }
    if n >= 40 { return 28 }
    if n >= 32 { return 26 }
    n
}
let protocol = "imap"
fn choose(p){
    protocol = p
    ui.imap_on.set_visible(p == "imap")
    ui.imap_off.set_visible(p != "imap")
    ui.pop_on.set_visible(p == "pop3")
    ui.pop_off.set_visible(p != "pop3")
    // Swap Gmail's servers for each other; anything typed stays.
    let h = ui.pop_host.text()
    if p == "imap" {
        ui.incoming.set_text("Incoming (IMAP, TLS)")
        if h == "pop.gmail.com" || h == "" { ui.pop_host.set_text("imap.gmail.com") ui.pop_port.set_text("993") }
    } else {
        ui.incoming.set_text("Incoming (POP3, TLS)")
        if h == "imap.gmail.com" || h == "" { ui.pop_host.set_text("pop.gmail.com") ui.pop_port.set_text("995") }
    }
}
fn submit(){
    ui.status.set_text("Checking the account…")
    host.request("mail.sheet.submit", {
        address: ui.address.text() username: ui.username.text() password: ui.password.text() protocol: protocol
        host: ui.pop_host.text() port: ui.pop_port.text() security: "tls"
        smtp_host: ui.smtp_host.text() smtp_port: ui.smtp_port.text() smtp_security: "tls"
    }, fn(r){ if r.is_ok { ui.status.set_text("Signed in") } else { ui.status.set_text(r.error) } })
}
fn cancel(){ host.request("mail.sheet.cancel", {}, fn(r){}) }
let Field = TextInput{width: Fill height: sh_h(40)
    draw_bg +: {color: sh_field color_hover: sh_field color_focus: sh_field color_empty: sh_field
        border_color: sh_field_line border_color_hover: sh_field_line border_color_focus: sh_accent border_color_empty: sh_field_line border_radius: by(10.0, 3.0)}
    draw_text +: {color: sh_ink color_hover: sh_ink color_focus: sh_ink color_empty: sh_muted color_empty_hover: sh_muted}
}
let Caption = Label{text: "" draw_text.color: sh_muted draw_text.text_style.font_size: sh_fs(11)}
let Choice = ButtonFlat{height: sh_h(32) width: Fill
    draw_bg +: {border_radius: by(8.0, 3.0) color: #x00000000 color_hover: sh_wash color_down: sh_wash2 border_size: 0.0}
    draw_text +: {color: sh_ink2 color_hover: sh_ink2 color_down: sh_ink2 text_style +: {font_size: sh_fs(13)}}}
let Chosen = ButtonFlat{height: sh_h(32) width: Fill
    draw_bg +: {border_radius: by(8.0, 3.0) color: sh_card color_hover: sh_card color_down: sh_card border_size: 0.0}
    draw_text +: {color: sh_ink color_hover: sh_ink color_down: sh_ink text_style +: {font_size: sh_fs(13)}}}
let SheetTouch = SolidView{width: Fill height: Fill flow: Down draw_bg.color: sh_dim new_batch: true
    // Scrolls: on a small or dense screen, or with the keyboard up, the
    // form is taller than the window. The actions sit at the top so they
    // are never scrolled out of reach.
    ScrollYView{width: Fill height: Fill flow: Down padding: Inset{left: 12 right: 12 top: 24 bottom: 24}
    RoundedView{width: Fill height: Fit flow: Down spacing: 8 padding: 16 new_batch: true show_bg: true draw_bg.color: sh_card draw_bg.border_radius: by(18.0, 5.0)
        View{width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
            ButtonFlat{text: "Cancel" height: sh_h(40) on_click: || cancel()
                draw_bg +: {color: #x00000000 color_hover: sh_wash color_down: sh_wash2 border_size: 0.0}
                draw_text +: {color: sh_link color_hover: sh_link color_down: sh_link text_style +: {font_size: sh_fs(15)}}}
            View{width: Fill height: 1}
            ButtonFlat{text: "Sign in" height: sh_h(40) padding: Inset{left: 20 right: 20} on_click: || submit()
                draw_bg +: {border_radius: by(20.0, 3.0) color: sh_accent color_hover: sh_accent_hover color_down: sh_accent_down border_size: 0.0}
                draw_text +: {color: #xffffff color_hover: #xffffff color_down: #xffffff text_style +: {font_size: sh_fs(15)}}}
        }
        Label{width: Fill text: "OctoSense · Add a mail account" draw_text.color: sh_ink draw_text.text_style: theme.font_bold{font_size: sh_fs(17)}}
        Label{width: Fill text: "Your password stays with OctoSense. The app that asked only gets your mail." draw_text.color: sh_ink2 draw_text.text_style.font_size: sh_fs(12)}
        status := Label{width: Fill text: "" draw_text.color: sh_danger draw_text.text_style.font_size: sh_fs(12)}
        Caption{text: "Email address"}
        address := Field{empty_text: "you@example.com"}
        Caption{text: "Login (if not the address)"}
        username := Field{empty_text: "optional"}
        Caption{text: "Password or app password"}
        password := Field{empty_text: "password" is_password: true}
        RoundedView{width: Fill height: Fit flow: Right padding: 2 show_bg: true draw_bg.color: sh_line draw_bg.border_radius: by(10.0, 3.0)
            imap_on := Chosen{text: "IMAP: all folders"}
            imap_off := Choice{visible: false text: "IMAP: all folders" on_click: || choose("imap")}
            pop_on := Chosen{visible: false text: "POP3: inbox only"}
            pop_off := Choice{text: "POP3: inbox only" on_click: || choose("pop3")}
        }
        View{width: Fill height: Fit flow: Right spacing: 8
            View{width: Fill height: Fit flow: Down spacing: 4 incoming := Caption{text: "Incoming (IMAP, TLS)"} pop_host := Field{text: "imap.gmail.com"}}
            View{width: 80 height: Fit flow: Down spacing: 4 Caption{text: "Port"} pop_port := Field{text: "993"}}
        }
        View{width: Fill height: Fit flow: Right spacing: 8
            View{width: Fill height: Fit flow: Down spacing: 4 Caption{text: "Outgoing (SMTP, TLS)"} smtp_host := Field{text: "smtp.gmail.com"}}
            View{width: 80 height: Fit flow: Down spacing: 4 Caption{text: "Port"} smtp_port := Field{text: "465"}}
        }
    }
    }
}
// A Mac's sheet: it hangs under the toolbar, its form has the label to the left of
// the field, and the servers wait behind a disclosure, guessed from the address.
let uni = desk && try { theme.ui_unified == true } catch { false }
let sh_empty = tone(#x8e8e93, #x6e6e73, #xaeaeb2)
let sh_panel_line = tone(#xe5e5ea, #xc9c9ce, #x545458)
let sh_shadow = tone(#x00000000, #x00000038, #x00000090)
let sh_push = tone(#xffffff, #xffffff, #x48484a)
let sh_push_hover = tone(#xffffff, #xf2f2f5, #x545458)
let sh_push_down = tone(#xffffff, #xe5e5ea, #x636366)
let edited = false
let revealed = false
let checking = false
fn desk_servers(){
    if edited { return }
    let found = regex("@([^\\s@]+)$").exec(ui.address.text().trim())
    let domain = ""
    if found != nil { domain = found.captures[1] }
    let imap = protocol == "imap"
    let incoming = ""
    let outgoing = ""
    let out_port = "465"
    if domain != "" {
        if regex("^googlemail\\.com$", "i").test(domain) { domain = "gmail.com" }
        if imap { incoming = "imap." + domain } else { incoming = "pop." + domain }
        outgoing = "smtp." + domain
        if regex("^(outlook|hotmail|live|msn)\\.", "i").test(domain) {
            incoming = "outlook.office365.com"
            outgoing = "smtp-mail.outlook.com"
            out_port = "587"
        }
        if regex("^(icloud|me|mac)\\.com$", "i").test(domain) {
            incoming = "imap.mail.me.com"
            outgoing = "smtp.mail.me.com"
            out_port = "587"
        }
        if regex("^yahoo\\.", "i").test(domain) {
            if imap { incoming = "imap.mail.yahoo.com" } else { incoming = "pop.mail.yahoo.com" }
            outgoing = "smtp.mail.yahoo.com"
        }
    }
    ui.pop_host.set_text(incoming)
    ui.smtp_host.set_text(outgoing)
    ui.smtp_port.set_text(out_port)
}
fn desk_ready(){
    let ready = !checking && ui.password.text() != "" && regex("^[^\\s@]+@[^\\s@]+$").test(ui.address.text().trim())
    ui.signin_on.set_visible(ready)
    ui.signin_off.set_visible(!ready)
    ready
}
fn desk_typed(){
    desk_servers()
    desk_ready()
}
fn desk_edit(){
    edited = true
}
fn desk_reveal(on){
    revealed = on
    ui.servers.set_visible(on)
    ui.caret_closed.set_visible(!on)
    ui.caret_open.set_visible(on)
}
fn desk_choose(p){
    protocol = p
    if p == "imap" {
        ui.account_type.set_text("IMAP")
        ui.pop_port.set_text("993")
    } else {
        ui.account_type.set_text("POP")
        ui.pop_port.set_text("995")
    }
    desk_servers()
}
fn desk_note(text, bad){
    ui.status.set_text(text)
    ui.progress.set_text(text)
    ui.status.set_visible(bad && text != "")
    ui.progress.set_visible(!bad && text != "")
}
// Reconnecting an account: everything but the password is known.
fn desk_known(known){
    if known.address == "" {
        ui.address.focus()
        return
    }
    desk_choose(known.protocol)
    edited = true
    ui.sheet_title.set_text("Reconnect " + known.address)
    ui.sheet_note.set_text("Mail can't sign in to this account. Enter its password again.")
    ui.address_line.set_visible(false)
    ui.address.set_text(known.address)
    ui.username.set_text(known.username)
    ui.pop_host.set_text(known.host)
    ui.pop_port.set_text("" + known.port)
    ui.smtp_host.set_text(known.smtp_host)
    ui.smtp_port.set_text("" + known.smtp_port)
    desk_ready()
    ui.password.focus()
}
if desk { host.request("mail.sheet.known", {}, fn(r){ if r.is_ok { desk_known(r.data) } }) }
fn desk_failure(e){
    let error = "" + e
    if regex("valid host", "i").test(error) { return "Enter the incoming server." }
    if regex("valid smtp_host", "i").test(error) { return "Enter the outgoing server." }
    if regex("valid username", "i").test(error) { return "Enter the user name." }
    if regex("TLS|certificate", "i").test(error) { return "Mail couldn't make a secure connection. Check the server names and this device's date." }
    error
}
fn desk_submit(){
    if !desk_ready() { return }
    checking = true
    desk_ready()
    desk_note("Checking the account…", false)
    let out_port = ui.smtp_port.text().trim()
    let out_security = "tls"
    if out_port == "587" { out_security = "starttls" }
    host.request("mail.sheet.submit", {
        address: ui.address.text().trim() username: ui.username.text() password: ui.password.text() protocol: protocol
        host: ui.pop_host.text() port: ui.pop_port.text() security: "tls"
        smtp_host: ui.smtp_host.text() smtp_port: out_port smtp_security: out_security
    }, fn(r){
        checking = false
        desk_ready()
        if r.is_ok { desk_note("", false) }
        else {
            desk_note(desk_failure(r.error), true)
            // The password's own answer needs no servers.
            if !regex("password", "i").test("" + r.error) { desk_reveal(true) }
        }
    })
}
// A label sits a point under a field's text with the same box: the margin lines their baselines up.
let DeskKey = Label{clip_y: false padding: 0 margin: Inset{bottom: 1} width: 118 text: "" align: Align{x: 1.0} draw_text +: {color: sh_ink text_style: theme.font_regular{font_size: 9.75}}}
let DeskLine = View{width: Fill height: Fit flow: Right align: Align{y: 0.5} spacing: 8}
let DeskField = TextInput{width: Fill height: 28 margin: 0 padding: Inset{left: 8 right: 8 top: 6 bottom: 6} empty_text: ""
    draw_bg +: {color: sh_field color_hover: sh_field color_focus: sh_field color_empty: sh_field
        border_color: sh_field_line border_color_hover: sh_field_line border_color_focus: sh_accent border_color_empty: sh_field_line border_radius: 3.0}
    draw_text +: {color: sh_ink color_hover: sh_ink color_focus: sh_ink color_empty: sh_empty color_empty_hover: sh_empty color_empty_focus: sh_empty text_style: theme.font_regular{font_size: 9.75}}
    draw_selection.color: #x557dcc55
}
let DeskPush = ButtonFlat{height: 28 margin: 0 padding: Inset{left: 14 right: 14}
    draw_bg +: {border_radius: 3.0 color: sh_push color_hover: sh_push_hover color_down: sh_push_down color_focus: sh_push border_size: 1.0 border_color: sh_field_line}
    draw_text +: {color: sh_ink color_hover: sh_ink color_down: sh_ink color_focus: sh_ink text_style: theme.font_regular{font_size: 9.75}}}
let DeskDefault = DeskPush{
    draw_bg +: {color: sh_accent color_hover: sh_accent_hover color_down: sh_accent_down color_focus: sh_accent border_size: 0.0}
    draw_text +: {color: #xffffff color_hover: #xffffff color_down: #xffffff color_focus: #xffffff}}
let sh_off_face = tone(#xe8e8ed, #xe8e8ed, #x48484a)
let sh_off_ink = tone(#x636368, #x636368, #xbcbcc0)
let DeskDefaultOff = DeskPush{enabled: false
    draw_bg +: {border_size: 0.0 color: sh_off_face color_hover: sh_off_face color_down: sh_off_face color_focus: sh_off_face color_disabled: sh_off_face}
    draw_text +: {color: sh_off_ink color_hover: sh_off_ink color_down: sh_off_ink color_focus: sh_off_ink color_disabled: sh_off_ink}}
let DeskCaret = Label{clip_y: false padding: 0 width: 10 text: "" draw_text +: {color: sh_muted text_style: theme.font_icons{font_size: 8.25}}}
let SheetDesk = View{width: Fill height: Fill flow: Overlay
    SolidView{width: Fill height: Fill new_batch: true draw_bg.color: sh_dim}
    ScrollYView{width: Fill height: Fill flow: Down align: Align{x: 0.5} padding: Inset{left: 16 right: 16 top: if uni { 53 } else { 41 } bottom: 24}
        RoundedShadowView{width: 480 height: Fit flow: Down spacing: 10 padding: Inset{left: 20 right: 20 top: 18 bottom: 16} new_batch: true cursor: MouseCursor.Default
            draw_bg +: {color: sh_card border_size: 1.0 border_color: sh_panel_line border_radius: 5.0 shadow_color: sh_shadow shadow_radius: 18.0 shadow_offset: vec2(0.0, 4.0)}
            View{width: Fill height: Fit flow: Down spacing: 4 margin: Inset{bottom: 4}
                sheet_title := Label{clip_y: false padding: 0 width: Fill text: "Add a Mail Account" draw_text +: {color: sh_ink text_style: theme.font_bold{font_size: 9.75}}}
                sheet_note := Label{clip_y: false padding: 0 width: Fill text: "@DESK_NOTE@" flow: Flow.Right{wrap: true} draw_text +: {color: sh_muted text_style: theme.font_regular{font_size: 8.25 line_spacing: 1.3}}}
            }
            address_line := DeskLine{
                DeskKey{text: "Email address:"}
                address := DeskField{empty_text: "name@example.com" on_change: |text| desk_typed() on_return: |text| desk_submit()}
            }
            DeskLine{
                DeskKey{text: "Password:"}
                password := DeskField{empty_text: "Required" is_password: true on_change: |text| desk_ready() on_return: |text| desk_submit()}
            }
            GestureView{width: Fit height: 24 flow: Right align: Align{y: 0.5} spacing: 3 cursor: MouseCursor.Default on_tap: |x, y| desk_reveal(!revealed)
                caret_closed := DeskCaret{text: "@CARET_RIGHT@"}
                caret_open := DeskCaret{visible: false text: "@CARET_DOWN@"}
                Label{clip_y: false padding: 0 text: "Server Settings" draw_text +: {color: sh_ink text_style: theme.font_regular{font_size: 9.75}}}
            }
            servers := View{visible: false width: Fill height: Fit flow: Down
                View{width: Fill height: Fit flow: Down spacing: 10
                    DeskLine{
                        DeskKey{margin: 0 text: "Account type:"}
                        type_pick := GestureView{width: 120 height: 28 flow: Overlay align: Align{x: 1.0 y: 0.5} on_menu: |title| desk_choose(if title == "POP" { "pop3" } else { "imap" })
                            account_type := DeskPush{width: Fill text: "IMAP" padding: Inset{left: 10 right: 24} align: Align{x: 0.0 y: 0.5} on_click: || ui.type_pick.open_menu(["IMAP" "POP"], if protocol == "pop3" { "POP" } else { "IMAP" })}
                            Label{clip_y: false padding: 0 margin: Inset{right: 9} text: "@UPDOWN@" draw_text +: {color: sh_muted text_style: theme.font_icons{font_size: 8.25}}}
                        }
                    }
                    DeskLine{
                        DeskKey{text: "User name:"}
                        username := DeskField{empty_text: "Same as the email address"}
                    }
                    DeskLine{
                        DeskKey{text: "Incoming server:"}
                        pop_host := DeskField{on_change: |text| desk_edit()}
                        Label{clip_y: false padding: 0 margin: Inset{bottom: 1} text: "Port:" draw_text +: {color: sh_ink text_style: theme.font_regular{font_size: 9.75}}}
                        pop_port := DeskField{width: 56 text: "993" on_change: |text| desk_edit()}
                    }
                    DeskLine{
                        DeskKey{text: "Outgoing server:"}
                        smtp_host := DeskField{on_change: |text| desk_edit()}
                        Label{clip_y: false padding: 0 margin: Inset{bottom: 1} text: "Port:" draw_text +: {color: sh_ink text_style: theme.font_regular{font_size: 9.75}}}
                        smtp_port := DeskField{width: 56 text: "465" on_change: |text| desk_edit()}
                    }
                }
            }
            status := Label{visible: false clip_y: false padding: 0 width: Fill text: "" flow: Flow.Right{wrap: true} draw_text +: {color: sh_danger text_style: theme.font_regular{font_size: 8.25 line_spacing: 1.3}}}
            progress := Label{visible: false clip_y: false padding: 0 width: Fill text: "" draw_text +: {color: sh_muted text_style: theme.font_regular{font_size: 8.25}}}
            View{width: Fill height: Fit flow: Right spacing: 8 align: Align{x: 1.0 y: 0.5} margin: Inset{top: 6}
                DeskPush{text: "Cancel" on_click: || cancel()}
                signin_off := DeskDefaultOff{text: "Sign In"}
                signin_on := DeskDefault{visible: false text: "Sign In" on_click: || desk_submit()}
            }
        }
    }
}
if desk { SheetDesk{} } else { SheetTouch{} }
"##
    .replace("@DESK_NOTE@", note)
    .replace("@CARET_RIGHT@", "\u{f0da}")
    .replace("@CARET_DOWN@", "\u{f0d7}")
    .replace("@UPDOWN@", "\u{f0dc}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn a_reply_all_copies_everyone_else_once_and_never_the_account() {
        let message = json!({"address": "rose@example.com", "reply_to": "", "to": "Me <ME@example.com>, Sam <sam@example.com>",
            "cc": "Maya Lin <maya@example.com>, sam@example.com, rose@example.com"});
        let (to, others) = reply_people(&message, "me@example.com");
        assert_eq!(to, ["rose@example.com"]);
        assert_eq!(others, ["sam@example.com", "maya@example.com"]);
        // Reply-To decides, and a group is read as its people.
        let list = json!({"address": "rose@example.com", "reply_to": "Team: a@example.com, b@example.com;", "to": "me@example.com"});
        assert_eq!(reply_people(&list, "me@example.com").0, ["a@example.com", "b@example.com"]);
        // One's own sent message: a reply goes on to whom it went to.
        let mine = json!({"address": "me@example.com", "to": "noah@example.com, me@example.com"});
        let (to, others) = reply_people(&mine, "me@example.com");
        assert_eq!(to, ["noah@example.com"]);
        assert!(others.is_empty());
    }

    #[test]
    fn an_attachment_is_saved_under_a_plain_name() {
        assert_eq!(attachment_name("Q3 board numbers.pdf"), "Q3 board numbers.pdf");
        assert_eq!(attachment_name("../../etc/passwd"), "passwd");
        assert_eq!(attachment_name("C:\\tmp\\a.txt"), "a.txt");
        assert_eq!(attachment_name(".hidden"), "hidden");
        assert_eq!(attachment_name("..."), "attachment");
        assert_eq!(attachment_name(""), "attachment");
        assert_eq!(attachment_name("in\u{202E}voice.pdf"), "invoice.pdf", "nothing that disguises an extension");
        let long = attachment_name(&format!("{}.pdf", "x".repeat(300)));
        assert!(long.len() <= 128 && long.ends_with(".pdf"), "{long}");
        let wide = attachment_name(&format!("{}.pdf", "文".repeat(100)));
        assert!(wide.len() <= 128 && wide.ends_with(".pdf"), "{wide}");
    }

    /// A mailbox in memory: `test` checks the password, `fetch` hands out
    /// what the seen set does not hold, `send` records the draft.
    struct Fake {
        password: String,
        inbox: Vec<Value>,
        sent: Mutex<Vec<Value>>,
        marked: Mutex<Vec<String>>,
        flags: Mutex<Vec<String>>,
        moves: Mutex<Vec<String>>,
    }
    impl Transport for Fake {
        fn test(&self, account: &Value) -> Result<(), String> {
            if text(account, "password") == self.password { Ok(()) } else { Err("Wrong password.".into()) }
        }
        fn folders(&self, account: &Value) -> Result<Vec<Value>, String> {
            self.test(account)?;
            Ok(vec![json!({"id": INBOX, "name": "Inbox", "role": "inbox"}), json!({"id": "Archive", "name": "Archive", "role": "archive"})])
        }
        fn fetch(&self, account: &Value, folder: &str, state: &Value) -> Result<Value, String> {
            self.test(account)?;
            let last = state["last"].as_u64().unwrap_or(0) as usize;
            let inbox: Vec<Value> = if folder == INBOX { self.inbox.clone() } else { vec![message("a1", "Archived")] };
            let new: Vec<Value> = inbox.iter().skip(last).cloned().collect();
            Ok(json!({"messages": new, "state": {"last": inbox.len()}, "reset": false}))
        }
        fn mark_seen(&self, _account: &Value, folder: &str, message: &Value) -> Result<(), String> {
            self.marked.lock().unwrap().push(format!("{folder}/{}", text(message, "uid")));
            Ok(())
        }
        fn send(&self, account: &Value, draft: &Value) -> Result<Value, String> {
            self.test(account)?;
            self.sent.lock().unwrap().push(draft.clone());
            Ok(json!({"accepted": true}))
        }
        fn set_flag(&self, _account: &Value, folder: &str, message: &Value, flag: &str, on: bool) -> Result<(), String> {
            self.flags.lock().unwrap().push(format!("{folder}/{} {flag} {on}", text(message, "uid")));
            Ok(())
        }
        fn move_to(&self, account: &Value, folder: &str, message: &Value, target: &str) -> Result<(), String> {
            self.test(account)?;
            self.moves.lock().unwrap().push(format!("{folder}/{} to {target}", text(message, "uid")));
            Ok(())
        }
    }

    #[derive(Default)]
    struct Host {
        sheet: Option<Option<String>>,
    }
    impl ServiceHost for Host {
        fn open_sheet(&mut self, body: String) {
            self.sheet = Some(Some(body));
        }
        fn close_sheet(&mut self) {
            self.sheet = Some(None);
        }
    }

    fn message(uid: &str, subject: &str) -> Value {
        json!({"id": &network::hash(uid)[..24], "uid": uid, "sender": "Alex", "address": "alex@example.com", "subject": subject,
            "preview": "hello", "body": "hello there", "time": "Sep 25", "date": "2026-09-25T00:00:00Z", "unread": true})
    }

    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(9_100);

    /// Send one request through the real dispatch path; each gets an
    /// isolate of its own, so answers cannot cross.
    fn send(dir: &Path, app: &str, service: &str, args: Value, from_sheet: bool, host: &mut Host) -> usize {
        let heap = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let call = ServiceCall { app_id: app.into(), service: service.into(), args, from_sheet, may_prompt: true, host_dir: dir.into() };
        octosense_appstore::services::dispatch(call, heap, 1, host);
        heap
    }

    fn wait(heap: usize) -> Result<Value, String> {
        for _ in 0..500 {
            if let Some((_, _, result)) = octosense_appstore::services::take_replies_for(&[heap]).pop() {
                return result.map(|s| serde_json::from_str(&s).unwrap());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("no answer on {heap}");
    }

    fn ask(dir: &Path, app: &str, service: &str, args: Value, from_sheet: bool, host: &mut Host) -> Result<Value, String> {
        let heap = send(dir, app, service, args, from_sheet, host);
        wait(heap)
    }

    #[test]
    fn pop_baseline_covers_undownloaded_old_mail_but_later_batches_do_not_skip_new_mail() {
        let fetched = json!({"messages":[{"uid":"last"}], "skipped_uids":["oversized"], "uid_snapshot":["old","last","oversized","other-new"]});
        let baseline = pop_seen(&json!({"_incoming_baseline":true}), &fetched);
        assert_eq!(baseline, json!(["last","oversized","old","other-new"]).as_array().unwrap().clone());
        let next = pop_seen(&json!({"seen":["old"]}), &fetched);
        assert_eq!(next, json!(["old","last","oversized"]).as_array().unwrap().clone());
        assert!(!next.contains(&json!("other-new")), "an unseen new message outside this batch must remain fetchable");
    }

    #[test]
    fn should_list_only_the_accounts_an_app_was_granted_when_the_shell_asks() {
        let dir = std::env::temp_dir().join(format!("mail-ids-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("mail")).unwrap();
        let accounts = json!([{"id": "a1", "apps": ["os.mail"]}, {"id": "b2", "apps": ["os.other"]}, {"id": "c3", "apps": ["os.mail", "os.other"]}]);
        std::fs::write(dir.join("mail/accounts.json"), accounts.to_string()).unwrap();
        assert_eq!(account_ids(&dir, "os.mail"), ["a1", "c3"]);
        assert!(account_ids(&dir.join("nowhere"), "os.mail").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The app's active account is the one the person signed in to last
    /// (its `signed_in` time), not the account file's order.
    #[test]
    fn should_choose_the_account_signed_in_last_when_the_shell_asks_for_the_active_one() {
        let dir = std::env::temp_dir().join(format!("mail-active-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("mail")).unwrap();
        let accounts = json!([
            {"id": "b2", "apps": ["os.mail"], "signed_in": 300},
            {"id": "a1", "apps": ["os.mail"], "signed_in": 100},
            {"id": "x9", "apps": ["os.other"], "signed_in": 900},
        ]);
        std::fs::write(dir.join("mail/accounts.json"), accounts.to_string()).unwrap();
        assert_eq!(active_account(&dir, "os.mail").as_deref(), Some("b2"));
        assert_eq!(active_account(&dir, "os.none"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `mail.notify` goes to the shell, which draws the notice card (the
    /// one every system app shares) and publishes it as the calling app:
    /// the service hands it the caller and the call as they came, and
    /// without the shell's notifier the call is refused.
    #[test]
    fn a_notice_goes_to_the_shell_as_the_calling_app() {
        assert_eq!(notify("os.mail", &json!({"title": "Hello", "body": "x"})).unwrap_err(), "This device shows no glance cards.");
        let heard: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
        let seen = heard.clone();
        on_notify(Some(Arc::new(move |app: &str, args: &Value| -> Result<Value, String> {
            seen.lock().unwrap().push((app.to_string(), args.clone()));
            Ok(json!({"card_id": "hello", "replaced": false}))
        })));
        let args = json!({"title": "Hello", "body": "From the system agent", "card_id": "hello"});
        assert_eq!(notify("os.mail", &args).unwrap()["card_id"], "hello");
        assert_eq!(*heard.lock().unwrap(), [("os.mail".to_string(), args)]);
        on_notify(None);
    }

    #[test]
    fn an_app_signs_in_reads_without_the_password_and_cannot_bypass_send_review() {
        let dir = std::env::temp_dir().join(format!("mail-service-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut html_mail = message("u2", "Second");
        html_mail["body"] = json!("(No readable message body)");
        html_mail["to"] = json!("Me <me@example.com>, Sam <sam@example.com>");
        html_mail["html"] = json!("<style>p{}</style><div><h1>Big news</h1><p>Read <a href='https://x.example'>this</a>.</p><img src='https://t.example/p'></div>");
        let fake = Arc::new(Fake { password: "s3cret".into(), inbox: vec![message("u1", "First"), html_mail], sent: Mutex::default(), marked: Mutex::default(),
            flags: Mutex::default(), moves: Mutex::default() });
        register_with_vault(fake.clone(), Arc::new(vault::FileVault));
        let mut host = Host::default();
        // The shell's account lifecycle hears who gained and lost an account.
        let events: Arc<Mutex<Vec<AccountEvent>>> = Arc::default();
        let heard = events.clone();
        on_account_event(Some(Arc::new(move |e| heard.lock().unwrap().push(e))));

        // The app cannot hand the service a password itself.
        assert!(ask(&dir, "os.mail", "mail.sheet.submit", json!({"address": "me@example.com", "password": "s3cret"}), false, &mut host)
            .unwrap_err()
            .contains("for the host's sheet"));

        // add_account raises the sheet and waits; a wrong password keeps it waiting.
        let add = send(&dir, "os.mail", "mail.add_account", Value::Null, false, &mut host);
        assert!(matches!(host.sheet, Some(Some(_))), "the host's sheet is up");

        // Mail's tile on the home screen cannot start a sign-in, and does
        // not take over the one the person is filling in.
        let tile = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut tile_host = Host::default();
        let background = ServiceCall { app_id: "os.mail".into(), service: "mail.add_account".into(), args: Value::Null,
            from_sheet: false, may_prompt: false, host_dir: dir.clone() };
        octosense_appstore::services::dispatch(background, tile, 1, &mut tile_host);
        assert!(wait(tile).unwrap_err().contains("open Mail"), "refused, with what to do");
        assert!(tile_host.sheet.is_none(), "no sheet over the tile");
        assert!(octosense_appstore::services::take_replies_for(&[add]).is_empty(), "the foreground sign-in is still waiting");
        let form = json!({"address": "me@example.com", "password": "wrong", "host": "pop.example.com", "port": "995", "security": "tls",
            "smtp_host": "smtp.example.com", "smtp_port": "465", "smtp_security": "tls"});
        assert!(ask(&dir, "os.mail", "mail.sheet.submit", form.clone(), true, &mut host).unwrap_err().contains("Wrong password"));
        assert!(octosense_appstore::services::take_replies_for(&[add]).is_empty(), "the app is still waiting");
        let mut good = form;
        good["password"] = json!("s3cret");
        ask(&dir, "os.mail", "mail.sheet.submit", good, true, &mut host).unwrap();
        let added = wait(add).unwrap();
        let id = text(&added, "id").to_string();
        assert_eq!(added["address"], "me@example.com");
        assert_eq!(*events.lock().unwrap(), [AccountEvent::Added { app_id: "os.mail".into(), account: id.clone() }]);
        assert!(!std::fs::read_to_string(dir.join("mail/accounts.json")).unwrap().contains("s3cret"), "no password in the account list");
        assert!(std::fs::read_to_string(dir.join("mail/accounts.json")).unwrap().contains("\"signed_in\""), "when the person signed in");

        // Another app cannot reach the account.
        assert!(ask(&dir, "os.other", "mail.list", json!({"account": id}), false, &mut host).unwrap_err().contains("may not use"));
        assert_eq!(ask(&dir, "os.other", "mail.accounts", Value::Null, false, &mut host).unwrap(), json!([]));
        assert_eq!(ask(&dir, "os.mail", "mail.accounts", Value::Null, false, &mut host).unwrap()[0]["id"], id.as_str());

        let synced = ask(&dir, "os.mail", "mail.sync", json!({"account": id}), false, &mut host).unwrap();
        assert_eq!(synced["new"], 2);
        let again = ask(&dir, "os.mail", "mail.sync", json!({"account": id}), false, &mut host).unwrap();
        assert_eq!(again["new"], 0, "seen mail is not fetched twice");
        let list = ask(&dir, "os.mail", "mail.list", json!({"account": id}), false, &mut host).unwrap();
        assert_eq!(list["total"], 2);
        assert!(list["messages"][0].get("body").is_none(), "a list carries headers, not bodies");
        let first = text(&list["messages"][0], "id").to_string();
        let read = ask(&dir, "os.mail", "mail.message", json!({"account": id, "message": first}), false, &mut host).unwrap();
        assert_eq!(read["body"], "hello there");
        let list = ask(&dir, "os.mail", "mail.list", json!({"account": id}), false, &mut host).unwrap();
        assert_eq!(list["messages"][0]["unread"], false);

        let html_id = text(&list["messages"][1], "id").to_string();
        assert_eq!(list["messages"][1]["preview"], "Big news Read this.", "a preview is the mail's text, not its style sheet");
        assert_eq!(list["messages"][1]["to"], "Me <me@example.com>, Sam <sam@example.com>", "a row says whom it was written to");
        assert_eq!(list["messages"][0]["to"], "", "and is empty where the mail names no one");
        let read = ask(&dir, "os.mail", "mail.message", json!({"account": id, "message": html_id}), false, &mut host).unwrap();
        assert_eq!(read["html"], "<h2>Big news</h2><p>Read <a href=\"https://x.example\">this</a>.</p>");
        assert_eq!(read["body"], "Big news\n\nRead this.");
        for _ in 0..200 {
            if fake.marked.lock().unwrap().len() == 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let mut marked = fake.marked.lock().unwrap().clone();
        marked.sort();
        assert_eq!(marked, ["INBOX/u1", "INBOX/u2"], "the server hears a message was read, once");
        ask(&dir, "os.mail", "mail.message", json!({"account": id, "message": html_id}), false, &mut host).unwrap();
        assert_eq!(fake.marked.lock().unwrap().len(), 2);

        // Folders: listed, then each synced and listed on its own.
        let folders = ask(&dir, "os.mail", "mail.folders", json!({"account": id}), false, &mut host).unwrap();
        assert_eq!(folders[1]["id"], "Archive");
        assert_eq!(ask(&dir, "os.mail", "mail.sync", json!({"account": id, "folder": "Archive"}), false, &mut host).unwrap()["new"], 1);
        let archive = ask(&dir, "os.mail", "mail.list", json!({"account": id, "folder": "Archive"}), false, &mut host).unwrap();
        assert_eq!(archive["messages"][0]["subject"], "Archived");
        assert_eq!(ask(&dir, "os.mail", "mail.list", json!({"account": id}), false, &mut host).unwrap()["total"], 2, "folders keep their own mail");

        let refused = ask(&dir, "os.mail", "mail.send", json!({"account": id, "to": "alex@example.com", "subject": "Hi", "body": "Hello"}), false, &mut host).unwrap_err();
        assert!(refused.contains("approval_required"));
        assert!(fake.sent.lock().unwrap().is_empty());
        // The approval rules' contacts: the account and whom it wrote to, no password.
        assert_eq!(contacts::known_addresses(&dir), ["me@example.com"]);
        assert!(!dir.join("mail").join(contacts::SENT_TO_FILE).exists());

        // Contestant compatibility path: a normal app's mail.send mounts a
        // native review. No app/synthetic event can approve it.
        #[cfg(any(target_os = "macos", target_os = "android"))]
        {
            let store = Store::at(&dir, Arc::new(vault::FileVault));
            let original = store.accounts();
            let mut accounts = original.clone();
            accounts[0]["apps"]
                .as_array_mut()
                .unwrap()
                .push(json!("sample.mail"));
            store.save_accounts(&accounts).unwrap();
            let pending: Arc<Mutex<Option<public_review::ReviewRequest>>> = Arc::default();
            let observed = pending.clone();
            public_review::on_review(Some(Arc::new(move |request| {
                *observed.lock().unwrap() = Some(request);
                Ok("native review fixture".into())
            })));
            let args = json!({"account":id,"to":"recipient@example.com","subject":"Exact review","body":"Reviewed body"});
            let staged = ask(
                &dir,
                "sample.mail",
                "mail.compose",
                args.clone(),
                false,
                &mut host,
            )
            .unwrap();
            assert_eq!(staged["status"], "draft");
            assert!(fake.sent.lock().unwrap().is_empty());
            let mut review_args = args.clone();
            review_args["compose_id"] = staged["compose_id"].clone();
            review_args["expected_revision"] = json!(1);
            // Holding the draft I/O lock must not block opening/polling the
            // native preparation surface on the dispatch/UI thread.
            let held_draft_io = drafts::lock();
            let (opened_tx, opened_rx) = std::sync::mpsc::sync_channel(1);
            let request_dir = dir.clone();
            let request_args = review_args.clone();
            let dispatch = std::thread::spawn(move || {
                let mut surface = Host::default();
                let heap = send(
                    &request_dir,
                    "sample.mail",
                    "mail.send",
                    request_args,
                    false,
                    &mut surface,
                );
                opened_tx.send((heap, surface)).unwrap();
            });
            let opened = opened_rx.recv_timeout(std::time::Duration::from_secs(2));
            if opened.is_ok() {
                let mut request = pending.lock().unwrap();
                let request = request.as_mut().unwrap();
                assert!(request.is_preparing());
                assert!(request.result().is_none());
                assert!(!request.can_approve());
            }
            drop(held_draft_io);
            dispatch.join().unwrap();
            let (heap, surface) = opened.expect("native review dispatch waited for draft I/O");
            host = surface;
            assert!(host
                .sheet
                .as_ref()
                .is_some_and(|s| s.as_deref() == Some("native review fixture")));
            assert!(
                octosense_appstore::services::take_replies_for(&[heap]).is_empty(),
                "staging is never reported as sent"
            );
            let mut review = pending.lock().unwrap().take().unwrap();
            assert!(
                !review.can_approve(),
                "preparation is never an approval-ready snapshot"
            );
            for _ in 0..1000 {
                let _ = review.result();
                if !review.is_preparing() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert!(review.can_approve());
            assert_eq!(review.snapshot()["publisher"], "sample.mail");
            assert_eq!(review.snapshot()["payload"]["body"], "Reviewed body");
            assert!(review
                .approve(false, true)
                .unwrap_err()
                .contains("physical"));
            assert!(review
                .approve(true, false)
                .unwrap_err()
                .contains("physical"));
            assert!(fake.sent.lock().unwrap().is_empty());
            review.approve(true, true).unwrap(); // Synthetic native test only, no real transport.
            let receipt = wait(heap).unwrap();
            assert_eq!(receipt["accepted"], true);
            assert_eq!(receipt["status"], "accepted");
            assert!(receipt["id"].as_str().is_some_and(|s| s.starts_with('<')));
            assert!(review.approve(true, true).is_err());
            assert_eq!(fake.sent.lock().unwrap().len(), 1);
            let saved = ask(
                &dir,
                "sample.mail",
                "mail.compose_status",
                json!({"account":id,"compose_id":staged["compose_id"]}),
                false,
                &mut host,
            )
            .unwrap();
            assert_eq!(saved["status"], "accepted");
            assert!(ask(
                &dir,
                "os.other",
                "mail.compose_status",
                json!({"account":id,"compose_id":staged["compose_id"]}),
                false,
                &mut host
            )
            .is_err());
            assert!(ask(
                &dir,
                "sample.mail",
                "mail.sheet.review_close",
                json!({}),
                false,
                &mut host
            )
            .is_err());
            let background = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            octosense_appstore::services::dispatch(
                ServiceCall {
                    app_id: "sample.mail".into(),
                    service: "mail.send".into(),
                    args: args.clone(),
                    from_sheet: false,
                    may_prompt: false,
                    host_dir: dir.clone(),
                },
                background,
                1,
                &mut host,
            );
            assert!(wait(background).unwrap_err().contains("agents/background"));
            assert!(pending.lock().unwrap().is_none());
            let cancelled = send(
                &dir,
                "sample.mail",
                "mail.send",
                args.clone(),
                false,
                &mut host,
            );
            pending.lock().unwrap().take().unwrap().cancel().unwrap();
            assert!(wait(cancelled).unwrap_err().contains("cancelled"));
            assert_eq!(fake.sent.lock().unwrap().len(), 1);
            let closed = send(&dir, "sample.mail", "mail.send", args, false, &mut host);
            let mut orphan = pending.lock().unwrap().take().unwrap();
            octosense_appstore::services::cancel_heap(closed);
            assert!(orphan.approve(true, true).is_err());
            orphan.cancel().unwrap();
            assert_eq!(fake.sent.lock().unwrap().len(), 1);
            public_review::on_review(None);
            store.save_accounts(&original).unwrap();
        }

        // A row's menu in Mail: Mark as Unread and Flag, here at once and on the server after.
        let marked = ask(&dir, "os.mail", "mail.mark", json!({"account": id, "message": first, "unread": true, "flagged": true}), false, &mut host).unwrap();
        assert_eq!(marked["marked"], true);
        let list = ask(&dir, "os.mail", "mail.list", json!({"account": id}), false, &mut host).unwrap();
        assert_eq!((list["messages"][0]["unread"].clone(), list["messages"][0]["flagged"].clone()), (json!(true), json!(true)));
        assert_eq!(list["messages"][1]["flagged"], false, "a list says whether each message is flagged");
        for _ in 0..200 {
            if fake.flags.lock().unwrap().len() == 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(*fake.flags.lock().unwrap(), ["INBOX/u1 seen false", "INBOX/u1 flagged true"]);
        assert!(ask(&dir, "os.mail", "mail.mark", json!({"account": id, "message": first}), false, &mut host).unwrap_err().contains("what to mark"));
        assert!(ask(&dir, "os.other", "mail.mark", json!({"account": id, "message": first, "flagged": false}), false, &mut host).unwrap_err().contains("Only Mail"));
        // Move to a mailbox by its id; never to the one it is in.
        assert!(ask(&dir, "os.mail", "mail.move", json!({"account": id, "message": first, "target": "INBOX"}), false, &mut host).unwrap_err().contains("another mailbox"));
        let moved = ask(&dir, "os.mail", "mail.move", json!({"account": id, "message": first, "target": "Archive"}), false, &mut host).unwrap();
        assert_eq!(moved["moved"], true);
        assert_eq!(*fake.moves.lock().unwrap(), ["INBOX/u1 to Archive"]);
        assert_eq!(ask(&dir, "os.mail", "mail.list", json!({"account": id}), false, &mut host).unwrap()["total"], 1, "it left the Inbox");

        // An app that never had the account removes nothing, including any
        // other app's still-open review capability.
        let review_store = Store::at(&dir, Arc::new(vault::FileVault));
        let review_args = json!({"account": id, "to": "recipient@example.com", "subject": "Local review", "body": "A local test"});
        let untouched_review = drafts::composer_review(&review_store, "os.mail", &review_args).unwrap();
        let untouched_draft = text(untouched_review.snapshot(), "draft_id").to_owned();
        ask(&dir, "os.other", "mail.remove_account", json!({"account": id}), false, &mut host).unwrap();
        assert_eq!(events.lock().unwrap().len(), 1);
        drafts::cancel_review(untouched_review).unwrap();
        assert_eq!(drafts::read(&dir, "os.mail", &id, &untouched_draft).unwrap()["status"], "draft", "another app cannot cancel Mail's review");
        // A refused account-list write must not delete the still-linked
        // account's local messages, credential, or pending review.
        let failed_review = drafts::composer_review(&review_store, "os.mail", &review_args).unwrap();
        let failed_draft = text(failed_review.snapshot(), "draft_id").to_owned();
        let blocked_temp = dir.join("mail/accounts.tmp");
        std::fs::create_dir(&blocked_temp).unwrap();
        assert!(ask(&dir, "os.mail", "mail.remove_account", json!({"account": id}), false, &mut host).is_err());
        assert_eq!(ask(&dir, "os.mail", "mail.accounts", Value::Null, false, &mut host).unwrap()[0]["id"], id.as_str());
        assert!(dir.join("mail/secrets").join(&id).exists(), "a failed removal keeps the saved credential");
        assert!(mailbox_file(&dir.join("mail"), &id, INBOX).exists(), "a failed removal keeps cached mail");
        assert_eq!(events.lock().unwrap().len(), 1, "a failed removal has no removal event");
        std::fs::remove_dir(blocked_temp).unwrap();
        drafts::cancel_review(failed_review).unwrap();
        assert_eq!(drafts::read(&dir, "os.mail", &id, &failed_draft).unwrap()["status"], "draft", "a failed removal keeps the review usable");
        // Unlinking Mail from a shared account must revoke Mail's review but
        // leave the other app's review and the shared account data usable.
        let mut shared = review_store.accounts();
        shared[0]["apps"].as_array_mut().unwrap().push(json!("other.mail"));
        review_store.save_accounts(&shared).unwrap();
        let mail_review = drafts::composer_review(&review_store, "os.mail", &review_args).unwrap();
        let other_review = drafts::composer_review(&review_store, "other.mail", &review_args).unwrap();
        let other_draft = text(other_review.snapshot(), "draft_id").to_owned();
        ask(&dir, "os.mail", "mail.remove_account", json!({"account": id}), false, &mut host).unwrap();
        assert_eq!(events.lock().unwrap()[1], AccountEvent::Removed { app_id: "os.mail".into(), account: id.clone() });
        assert!(dir.join("mail/secrets").join(&id).exists(), "a shared account keeps its credential");
        assert!(mailbox_file(&dir.join("mail"), &id, INBOX).exists(), "a shared account keeps cached mail");
        drafts::cancel_review(mail_review).unwrap();
        drafts::cancel_review(other_review).unwrap();
        assert_eq!(drafts::read(&dir, "other.mail", &id, &other_draft).unwrap()["status"], "draft", "the other app's review remains usable");
        ask(&dir, "other.mail", "mail.remove_account", json!({"account": id}), false, &mut host).unwrap();
        assert_eq!(events.lock().unwrap()[2], AccountEvent::Removed { app_id: "other.mail".into(), account: id.clone() });
        on_account_event(None);
        assert!(!dir.join("mail/secrets").join(&id).exists(), "the last app out takes the password with it");
        assert!(std::fs::read_dir(dir.join("mail")).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with("box-")), "and its mail");
        assert!(contacts::known_addresses(&dir).is_empty(), "and whom it wrote to");
    }
}
