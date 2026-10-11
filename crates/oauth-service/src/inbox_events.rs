//! Durable, quiet Gmail event polling. Initial connection establishes a forward
//! baseline; it does not notify the entire historical mailbox. Acknowledgement
//! follows successful app-peer processing, never merely fetching an email.
use crate::{
    api::{Api, GMAIL_READ_SCOPE},
    transport::{json_ok, Body},
    Provider,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use url::Url;
use uuid::Uuid;

pub struct IncomingPage {
    pub ids: Vec<String>,
    pub next: Option<String>,
    pub history: String,
    pub expired: bool,
}
pub trait IncomingSource {
    fn baseline(&mut self) -> Result<String, String>;
    fn changes(&mut self, history: &str, page: Option<&str>) -> Result<IncomingPage, String>;
    fn recent(&mut self, since: u64, page: Option<&str>) -> Result<IncomingPage, String>;
}
pub struct GmailEvents<'a, 'b> {
    pub api: &'a mut Api<'b>,
    pub app: &'a str,
    pub connection: &'a str,
}
impl IncomingSource for GmailEvents<'_, '_> {
    fn baseline(&mut self) -> Result<String, String> {
        let value = json_ok(self.api.request(
            self.app,
            self.connection,
            Provider::Google,
            GMAIL_READ_SCOPE,
            "GET",
            Url::parse("https://gmail.googleapis.com/gmail/v1/users/me/profile").unwrap(),
            Body::Empty,
            None,
        )?)?;
        cursor(value["historyId"].as_str().unwrap_or(""))
    }
    fn changes(&mut self, history: &str, page: Option<&str>) -> Result<IncomingPage, String> {
        let mut url = Url::parse("https://gmail.googleapis.com/gmail/v1/users/me/history").unwrap();
        url.query_pairs_mut()
            .append_pair("startHistoryId", &cursor(history)?)
            .append_pair("labelId", "INBOX")
            .append_pair("historyTypes", "messageAdded")
            .append_pair("maxResults", "100");
        add_page(&mut url, page)?;
        let response = self.api.request(
            self.app,
            self.connection,
            Provider::Google,
            GMAIL_READ_SCOPE,
            "GET",
            url,
            Body::Empty,
            None,
        )?;
        if response.status == 404 {
            return Ok(IncomingPage {
                ids: vec![],
                next: None,
                history: String::new(),
                expired: true,
            });
        }
        let value = json_ok(response)?;
        let mut ids = Vec::new();
        if let Some(records) = value["history"].as_array() {
            for record in records {
                if let Some(added) = record["messagesAdded"].as_array() {
                    for added in added {
                        if let Some(id) = added["message"]["id"].as_str() {
                            ids.push(message_id(id)?);
                        }
                    }
                }
            }
        }
        Ok(IncomingPage {
            ids,
            next: value["nextPageToken"].as_str().map(str::to_owned),
            history: cursor(value["historyId"].as_str().unwrap_or(""))?,
            expired: false,
        })
    }
    fn recent(&mut self, since: u64, page: Option<&str>) -> Result<IncomingPage, String> {
        let mut url =
            Url::parse("https://gmail.googleapis.com/gmail/v1/users/me/messages").unwrap();
        url.query_pairs_mut()
            .append_pair("labelIds", "INBOX")
            .append_pair("q", &format!("after:{since}"))
            .append_pair("maxResults", "100");
        add_page(&mut url, page)?;
        let value = json_ok(self.api.request(
            self.app,
            self.connection,
            Provider::Google,
            GMAIL_READ_SCOPE,
            "GET",
            url,
            Body::Empty,
            None,
        )?)?;
        let mut ids = Vec::new();
        if let Some(messages) = value["messages"].as_array() {
            for message in messages {
                ids.push(message_id(message["id"].as_str().unwrap_or(""))?);
            }
        }
        Ok(IncomingPage {
            ids,
            next: value["nextPageToken"].as_str().map(str::to_owned),
            history: String::new(),
            expired: false,
        })
    }
}
fn cursor(value: &str) -> Result<String, String> {
    if value.is_empty() || value.len() > 128 || !value.bytes().all(|b| b.is_ascii_digit()) {
        Err("Invalid Gmail history cursor".into())
    } else {
        Ok(value.into())
    }
}
fn message_id(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        Err("Invalid Gmail message ID".into())
    } else {
        Ok(value.into())
    }
}
fn add_page(url: &mut Url, page: Option<&str>) -> Result<(), String> {
    if let Some(page) = page {
        if page.is_empty() || page.len() > 4096 || page.chars().any(char::is_control) {
            return Err("Invalid Gmail page cursor".into());
        }
        url.query_pairs_mut().append_pair("pageToken", page);
    }
    Ok(())
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct State {
    history: Option<String>,
    last_sync: u64,
    pending: BTreeMap<String, Pending>,
    completed: BTreeMap<String, u64>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    #[serde(default)]
    decision: Option<EventDecision>,
    lease: Option<String>,
    retry_after: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventDecision {
    pub kind: String,
    pub reason: String,
    pub at: u64,
}
pub struct EventStore {
    path: PathBuf,
    app: String,
    connection: String,
    state: State,
}
#[derive(Serialize)]
pub struct EventStatus {
    pub baseline_ready: bool,
    pub pending: usize,
    /// Last successful complete provider poll, not an attempted/failed request.
    pub last_poll_at: Option<u64>,
}
/// Cannot be constructed from JSON or retargeted to another account.
pub struct EventLease {
    app: String,
    connection: String,
    message: String,
    nonce: String,
}
impl EventLease {
    pub fn message_id(&self) -> &str {
        &self.message
    }
}
#[derive(Debug, Serialize)]
pub struct PollResult {
    pub initialized: bool,
    pub added: usize,
    pub pending: usize,
    pub recovered_history: bool,
}
fn storage_path(root: &Path, app: &str, connection: &str) -> Result<PathBuf, String> {
    if app.is_empty() || app.len() > 256 || connection.is_empty() || connection.len() > 256 {
        return Err("Missing or invalid event account identity".into());
    }
    let key = format!("{:x}", Sha256::digest(format!("{app}\0{connection}")));
    Ok(crate::inbox::app_directory(root, "inbox-events", app).join(format!("{key}.json")))
}
/// Uninstall-only cleanup includes previously disconnected accounts.
pub(crate) fn purge_app(root: &Path, app: &str) -> Result<(), String> {
    crate::inbox::purge_private_app(root, "inbox-events", app)
}
impl EventStore {
    pub fn open(root: &Path, app: &str, connection: &str) -> Result<Self, String> {
        let path = storage_path(root, app, connection)?;
        let state = match std::fs::read(&path) {
            // Match the atomic writer's cap. A full 4096-event queue with
            // bounded decision reasons can legitimately exceed 2 MiB.
            Ok(bytes) if bytes.len() <= 16 * 1024 * 1024 => {
                serde_json::from_slice(&bytes).map_err(|_| "Invalid Gmail event cursor")?
            }
            Ok(_) => return Err("Gmail event queue exceeds its limit".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(_) => return Err("Cannot read Gmail event queue".into()),
        };
        Ok(Self {
            path,
            app: app.into(),
            connection: connection.into(),
            state,
        })
    }
    fn save(&mut self, next: State) -> Result<(), String> {
        crate::inbox::persist(&self.path, &next)?;
        self.state = next;
        Ok(())
    }
    pub fn refresh(
        &mut self,
        source: &mut dyn IncomingSource,
        now: u64,
    ) -> Result<PollResult, String> {
        let Some(history) = self.state.history.clone() else {
            let next = State {
                history: Some(cursor(&source.baseline()?)?),
                last_sync: now,
                ..self.state.clone()
            };
            self.save(next)?;
            return Ok(PollResult {
                initialized: true,
                added: 0,
                pending: self.state.pending.len(),
                recovered_history: false,
            });
        };
        let mut ids = BTreeSet::new();
        let mut page = None;
        let mut pages = BTreeSet::new();
        let mut recovered = false;
        let mut recovery_cursor = None;
        let final_history = loop {
            if pages.len() >= 100 || ids.len() > 4096 {
                return Err("Too many pending Gmail updates; cursor was not advanced".into());
            }
            let batch = if recovered {
                source.recent(self.state.last_sync.saturating_sub(60), page.as_deref())?
            } else {
                source.changes(&history, page.as_deref())?
            };
            if batch.expired {
                if recovered {
                    return Err("Gmail recovery failed; cursor was not advanced".into());
                }
                // Record the provider's current cursor before rescan: messages
                // arriving during rescan are then covered by the next history call.
                recovery_cursor = Some(cursor(&source.baseline()?)?);
                recovered = true;
                page = None;
                pages.clear();
                ids.clear();
                continue;
            }
            for id in batch.ids {
                ids.insert(message_id(&id)?);
            }
            match batch.next {
                Some(next) if !next.is_empty() => {
                    if !pages.insert(next.clone()) {
                        return Err(
                            "Gmail repeated a page cursor; previous event state retained".into(),
                        );
                    }
                    page = Some(next)
                }
                _ => {
                    break if recovered {
                        recovery_cursor.unwrap()
                    } else {
                        cursor(&batch.history)?
                    }
                }
            }
        };
        let mut next = self.state.clone();
        let mut added = 0;
        for id in ids {
            if !next.completed.contains_key(&id) && !next.pending.contains_key(&id) {
                next.pending.insert(
                    id,
                    Pending {
                        decision: None,
                        lease: None,
                        retry_after: now,
                    },
                );
                added += 1;
            }
        }
        if next.pending.len() > 4096 {
            return Err("Gmail pending event limit reached; cursor was not advanced".into());
        }
        next.history = Some(final_history);
        next.last_sync = now;
        next.completed
            .retain(|_, at| *at >= now.saturating_sub(30 * 86400));
        self.save(next)?;
        Ok(PollResult {
            initialized: false,
            added,
            pending: self.state.pending.len(),
            recovered_history: recovered,
        })
    }
    /// Caller serializes this transaction. A crash loses no event: an unfinished
    /// lease becomes eligible after ten minutes and retains the same message ID.
    pub fn claim(&mut self, now: u64, limit: usize) -> Result<Vec<EventLease>, String> {
        let mut next = self.state.clone();
        let mut leases = Vec::new();
        for (message, pending) in &mut next.pending {
            if pending.retry_after <= now && leases.len() < limit.min(16) {
                let nonce = Uuid::new_v4().to_string();
                pending.lease = Some(nonce.clone());
                pending.retry_after = now.saturating_add(600);
                leases.push(EventLease {
                    app: self.app.clone(),
                    connection: self.connection.clone(),
                    message: message.clone(),
                    nonce,
                });
            }
        }
        if !leases.is_empty() {
            self.save(next)?;
        }
        Ok(leases)
    }
    /// `publication_verified` is supplied by trusted host lookup, never by
    /// a script/model argument. Published decisions require the same message's
    /// existing app/account-bound Glance card.
    pub fn decide(
        &mut self,
        message: &str,
        kind: &str,
        reason: &str,
        publication_verified: bool,
        now: u64,
    ) -> Result<EventDecision, String> {
        if !matches!(kind, "quiet" | "published") || reason.trim().is_empty() || reason.len() > 1000
        {
            return Err("Provide a quiet or published decision and a brief reason".into());
        }
        if kind == "published" && !publication_verified {
            return Err(
                "Publish the bound Glance card successfully before marking this event published"
                    .into(),
            );
        }
        let mut next = self.state.clone();
        let pending = next
            .pending
            .get_mut(message)
            .ok_or("This Gmail event is not pending")?;
        let decision = EventDecision {
            kind: kind.into(),
            reason: reason.into(),
            at: now,
        };
        pending.decision = Some(decision.clone());
        self.save(next)?;
        Ok(decision)
    }
    pub fn pending_count(&self) -> usize {
        self.state.pending.len()
    }
    pub fn status(&self) -> EventStatus {
        EventStatus {
            baseline_ready: self.state.history.is_some(),
            pending: self.pending_count(),
            last_poll_at: self.state.history.as_ref().map(|_| self.state.last_sync),
        }
    }
    pub fn decision(&self, message: &str) -> Option<&EventDecision> {
        self.state
            .pending
            .get(message)
            .and_then(|p| p.decision.as_ref())
    }
    pub fn complete(&mut self, lease: EventLease, success: bool, now: u64) -> Result<(), String> {
        if lease.app != self.app || lease.connection != self.connection {
            return Err("Event completion belongs to another app/account".into());
        }
        if !self
            .state
            .pending
            .get(&lease.message)
            .is_some_and(|p| p.lease.as_deref() == Some(&lease.nonce))
        {
            return Err("Event completion was replaced or already acknowledged".into());
        }
        if success
            && self
                .state
                .pending
                .get(&lease.message)
                .and_then(|p| p.decision.as_ref())
                .is_none()
        {
            return Err("Gmail event has no durable quiet or verified-publication decision".into());
        }
        let mut next = self.state.clone();
        if success {
            next.pending.remove(&lease.message);
            next.completed.insert(lease.message, now);
            if next.completed.len() > 4096 {
                let mut by_time: Vec<_> = next
                    .completed
                    .iter()
                    .map(|(id, at)| (*at, id.clone()))
                    .collect();
                by_time.sort();
                for (_, id) in by_time.into_iter().take(next.completed.len() - 4096) {
                    next.completed.remove(&id);
                }
            }
        } else {
            // Preserve an already durable decision across an interrupted turn.
            // The next peer can read its receipt and finish without reposting
            // a card (and notifying again) or reclassifying the same email.
            let pending = next.pending.get_mut(&lease.message).expect("lease checked");
            pending.lease = None;
            pending.retry_after = now.saturating_add(60);
        }
        self.save(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    struct Source {
        baseline: &'static str,
        answers: VecDeque<Result<IncomingPage, String>>,
        seen: Vec<String>,
    }
    impl IncomingSource for Source {
        fn baseline(&mut self) -> Result<String, String> {
            Ok(self.baseline.into())
        }
        fn changes(&mut self, h: &str, p: Option<&str>) -> Result<IncomingPage, String> {
            self.seen.push(format!("{h}:{p:?}"));
            self.answers.pop_front().unwrap()
        }
        fn recent(&mut self, s: u64, p: Option<&str>) -> Result<IncomingPage, String> {
            self.seen.push(format!("recent:{s}:{p:?}"));
            self.answers.pop_front().unwrap()
        }
    }
    fn page(ids: &[&str], next: Option<&str>, history: &str) -> Result<IncomingPage, String> {
        Ok(IncomingPage {
            ids: ids.iter().map(|s| s.to_string()).collect(),
            next: next.map(str::to_string),
            history: history.into(),
            expired: false,
        })
    }
    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn start() -> (Temp, EventStore, Source) {
        let t = Temp(std::env::temp_dir().join(format!("gmail-events-{}", Uuid::new_v4())));
        let mut store = EventStore::open(&t.0, "sample.inbox", "account1").unwrap();
        assert!(!store.status().baseline_ready);
        assert_eq!(store.status().last_poll_at, None);
        let mut source = Source {
            baseline: "100",
            answers: VecDeque::new(),
            seen: vec![],
        };
        let r = store.refresh(&mut source, 1000).unwrap();
        assert!(r.initialized);
        assert_eq!(r.added, 0);
        assert!(store.status().baseline_ready);
        assert_eq!(store.status().last_poll_at, Some(1000));
        (t, store, source)
    }
    #[test]
    fn uninstall_purge_erases_all_owner_cursors_but_keeps_other_apps() {
        let (root, mut store, mut source) = start();
        source.answers.push_back(page(&["new1"], None, "101"));
        store.refresh(&mut source, 1100).unwrap();
        EventStore::open(&root.0, "sample.inbox", "disconnected-account")
            .unwrap()
            .save(store.state.clone())
            .unwrap();
        EventStore::open(&root.0, "other.app", "account1")
            .unwrap()
            .save(store.state.clone())
            .unwrap();
        let lease = store.claim(1101, 1).unwrap().pop().unwrap();
        purge_app(&root.0, "sample.inbox").unwrap();
        assert!(!crate::inbox::app_directory(&root.0, "inbox-events", "sample.inbox").exists());
        assert!(
            !EventStore::open(&root.0, "sample.inbox", "account1")
                .unwrap()
                .status()
                .baseline_ready
        );
        // An in-flight peer's late completion cannot resurrect the queue.
        assert!(EventStore::open(&root.0, "sample.inbox", "account1")
            .unwrap()
            .complete(lease, false, 1102)
            .is_err());
        assert!(!crate::inbox::app_directory(&root.0, "inbox-events", "sample.inbox").exists());
        assert_eq!(
            EventStore::open(&root.0, "other.app", "account1")
                .unwrap()
                .pending_count(),
            1
        );
        purge_app(&root.0, "sample.inbox").unwrap();
    }
    #[test]
    fn full_bounded_queue_can_be_reopened_after_decisions() {
        let (t, mut s, _) = start();
        let mut next = s.state.clone();
        for n in 0..4096 {
            next.pending.insert(
                format!("message{n}"),
                Pending {
                    decision: Some(EventDecision {
                        kind: "quiet".into(),
                        reason: "x".repeat(1000),
                        at: 1001,
                    }),
                    lease: None,
                    retry_after: 1000,
                },
            );
        }
        s.save(next).unwrap();
        assert_eq!(
            EventStore::open(&t.0, "sample.inbox", "account1")
                .unwrap()
                .state
                .pending
                .len(),
            4096
        );
    }
    #[test]
    fn new_mail_pages_commit_together_and_only_once_after_ack() {
        let (t, mut s, mut f) = start();
        f.answers.extend([
            page(&["new1"], Some("page2"), "101"),
            page(&["new1", "new2"], None, "102"),
        ]);
        assert_eq!(s.refresh(&mut f, 1100).unwrap().added, 2);
        assert_eq!(f.seen, ["100:None", "100:Some(\"page2\")"]);
        let leases = s.claim(1100, 16).unwrap();
        assert_eq!(leases.len(), 2);
        assert!(s.claim(1100, 16).unwrap().is_empty());
        for lease in leases {
            s.decide(
                lease.message_id(),
                "quiet",
                "Fixture newsletter; no personal action",
                false,
                1101,
            )
            .unwrap();
            s.complete(lease, true, 1101).unwrap();
        }
        let mut s = EventStore::open(&t.0, "sample.inbox", "account1").unwrap();
        f.answers.push_back(page(&["new1", "new2"], None, "103"));
        assert_eq!(s.refresh(&mut f, 1200).unwrap().added, 0);
        assert!(s.claim(1200, 16).unwrap().is_empty());
    }
    #[test]
    fn page_failure_retains_cursor_and_queue() {
        let (_t, mut s, mut f) = start();
        f.answers
            .extend([page(&["new1"], Some("page2"), "101"), Err("offline".into())]);
        assert!(s.refresh(&mut f, 1100).is_err());
        assert_eq!(s.state.history.as_deref(), Some("100"));
        assert!(s.state.pending.is_empty());
        assert_eq!(s.status().last_poll_at, Some(1000));
    }
    #[test]
    fn expired_history_recovers_only_since_last_success() {
        let (_t, mut s, mut f) = start();
        f.baseline = "200";
        f.answers.extend([
            Ok(IncomingPage {
                ids: vec![],
                next: None,
                history: String::new(),
                expired: true,
            }),
            page(&["new1"], None, ""),
        ]);
        let r = s.refresh(&mut f, 1200).unwrap();
        assert!(r.recovered_history);
        assert_eq!(r.added, 1);
        assert_eq!(s.state.history.as_deref(), Some("200"));
        assert!(f.seen.contains(&"recent:940:None".into()));
    }
    #[test]
    fn failed_peer_retries_and_older_completion_cannot_ack_new_lease() {
        let (t, mut s, mut f) = start();
        f.answers.push_back(page(&["new1"], None, "101"));
        s.refresh(&mut f, 1100).unwrap();
        let lease = s.claim(1100, 1).unwrap().pop().unwrap();
        s.complete(lease, false, 1101).unwrap();
        assert!(s.claim(1150, 1).unwrap().is_empty());
        let old = s.claim(1161, 1).unwrap().pop().unwrap();
        let mut s = EventStore::open(&t.0, "sample.inbox", "account1").unwrap();
        let current = s.claim(1762, 1).unwrap().pop().unwrap();
        assert!(s.complete(old, true, 1763).is_err());
        s.decide(
            current.message_id(),
            "quiet",
            "Fixture processed",
            false,
            1763,
        )
        .unwrap();
        s.complete(current, true, 1763).unwrap();
        assert!(s.state.pending.is_empty());
    }
    #[test]
    fn cross_account_completion_and_cyclic_pages_fail() {
        let (t, mut s, mut f) = start();
        f.answers.push_back(page(&["new1"], None, "101"));
        s.refresh(&mut f, 1100).unwrap();
        let lease = s.claim(1100, 1).unwrap().pop().unwrap();
        let mut other = EventStore::open(&t.0, "sample.inbox", "account2").unwrap();
        assert!(other.complete(lease, true, 1101).is_err());
        f.answers.extend([
            page(&[], Some("repeat"), "102"),
            page(&[], Some("repeat"), "103"),
        ]);
        assert!(s.refresh(&mut f, 1200).is_err());
        assert_eq!(s.state.history.as_deref(), Some("101"));
    }
    #[test]
    fn interrupted_turn_keeps_publication_receipt_for_retry() {
        let (t, mut s, mut source) = start();
        source.answers.push_back(page(&["new1"], None, "101"));
        s.refresh(&mut source, 1100).unwrap();
        let lease = s.claim(1100, 1).unwrap().pop().unwrap();
        s.decide("new1", "published", "Verified publication", true, 1101)
            .unwrap();
        s.complete(lease, false, 1102).unwrap();
        let mut restored = EventStore::open(&t.0, "sample.inbox", "account1").unwrap();
        assert_eq!(restored.decision("new1").unwrap().kind, "published");
        let retry = restored.claim(1163, 1).unwrap().pop().unwrap();
        restored.complete(retry, true, 1164).unwrap();
        assert_eq!(restored.pending_count(), 0);
    }
    #[test]
    fn model_prose_and_unverified_publication_cannot_ack_event() {
        let (_t, mut s, mut f) = start();
        f.answers.push_back(page(&["new1"], None, "101"));
        s.refresh(&mut f, 1100).unwrap();
        let lease = s.claim(1100, 1).unwrap().pop().unwrap();
        assert!(s.complete(lease, true, 1101).is_err());
        assert!(s
            .decide("new1", "published", "Model claims success", false, 1101)
            .is_err());
        assert!(s.state.pending.contains_key("new1"));
        s.decide(
            "new1",
            "published",
            "Host verified this message's card",
            true,
            1101,
        )
        .unwrap();
        let lease = s.claim(1701, 1).unwrap().pop().unwrap();
        s.complete(lease, true, 1702).unwrap();
        assert!(s.state.pending.is_empty());
    }
}
