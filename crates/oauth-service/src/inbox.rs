//! Gmail Inbox drafts. The host owns the durable revision and exact send ledger.
//! No JSON method authorizes sending. Native review must supply trusted pointer
//! provenance on both ends of the approval gesture, as required by ADR 0007.
use crate::{
    api::{Api, GMAIL_READ_SCOPE, GMAIL_SEND_SCOPE},
    transport::{json_ok, Body},
    Provider,
};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub id: String,
    pub thread_id: String,
    pub from: String,
    pub reply_to: String,
    pub subject: String,
    pub body: String,
    pub snippet: String,
    pub message_id: String,
    pub references: String,
    pub labels: Vec<String>,
    pub attachments_omitted: bool,
}

fn header(payload: &Value, wanted: &str) -> String {
    payload["headers"]
        .as_array()
        .and_then(|headers| {
            headers.iter().find(|h| {
                h["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(wanted))
            })
        })
        .and_then(|h| h["value"].as_str())
        .unwrap_or("")
        .to_string()
}
fn plain_part(payload: &Value, depth: usize, output: &mut String, attachments: &mut bool) {
    if depth > 12 || output.len() > 256 * 1024 {
        return;
    }
    if payload["body"]["attachmentId"].is_string()
        || payload["filename"].as_str().is_some_and(|s| !s.is_empty())
    {
        *attachments = true;
        return;
    }
    if payload["mimeType"] == "text/plain" {
        if let Some(encoded) = payload["body"]["data"].as_str() {
            if let Ok(bytes) = URL_SAFE_NO_PAD.decode(encoded.trim_end_matches('=')) {
                if let Ok(text) = String::from_utf8(bytes) {
                    output.push_str(&text);
                    output.push('\n');
                }
            }
        }
    }
    if let Some(parts) = payload["parts"].as_array() {
        for part in parts {
            plain_part(part, depth + 1, output, attachments);
        }
    }
}
impl Message {
    pub fn from_gmail(value: &Value) -> Result<Self, String> {
        let id = required(value, "id")?.to_string();
        let thread_id = required(value, "threadId")?.to_string();
        let payload = &value["payload"];
        let mut body = String::new();
        let mut attachments = false;
        plain_part(payload, 0, &mut body, &mut attachments);
        let snippet = value["snippet"].as_str().unwrap_or("").to_string();
        if body.is_empty() {
            body = format!("Plain-text body unavailable. Preview: {snippet}");
        }
        let from = header(payload, "From");
        let reply = header(payload, "Reply-To");
        Ok(Self {
            id,
            thread_id,
            reply_to: if reply.is_empty() {
                from.clone()
            } else {
                reply
            },
            from,
            subject: header(payload, "Subject"),
            body,
            snippet,
            message_id: header(payload, "Message-ID"),
            references: header(payload, "References"),
            labels: value["labelIds"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            attachments_omitted: attachments,
        })
    }
}

impl Api<'_> {
    pub fn gmail_labels(&mut self, caller: &str, connection: &str) -> Result<Value, String> {
        json_ok(self.request(
            caller,
            connection,
            Provider::Google,
            GMAIL_READ_SCOPE,
            "GET",
            Url::parse("https://gmail.googleapis.com/gmail/v1/users/me/labels").unwrap(),
            Body::Empty,
            None,
        )?)
    }
    pub fn gmail_inbox(
        &mut self,
        caller: &str,
        connection: &str,
        label: &str,
        page: Option<&str>,
    ) -> Result<Value, String> {
        if label.is_empty() || label.len() > 256 || label.chars().any(char::is_control) {
            return Err("Invalid mailbox label".into());
        }
        let mut url =
            Url::parse("https://gmail.googleapis.com/gmail/v1/users/me/messages").unwrap();
        url.query_pairs_mut()
            .append_pair("labelIds", label)
            .append_pair("maxResults", "30");
        if let Some(page) = page.filter(|s| !s.is_empty()) {
            if page.len() > 2048 {
                return Err("Invalid mailbox cursor".into());
            }
            url.query_pairs_mut().append_pair("pageToken", page);
        }
        json_ok(self.request(
            caller,
            connection,
            Provider::Google,
            GMAIL_READ_SCOPE,
            "GET",
            url,
            Body::Empty,
            None,
        )?)
    }
    pub fn inbox_message(
        &mut self,
        caller: &str,
        connection: &str,
        id: &str,
    ) -> Result<Message, String> {
        self.inbox_message_if_present(caller, connection, id)?
            .ok_or_else(|| "This Gmail message is no longer available".into())
    }
    /// A message may be deleted after its history entry was queued. Preserve
    /// the provider's 404 as absence, not a transient error retried forever.
    pub fn inbox_message_if_present(
        &mut self,
        caller: &str,
        connection: &str,
        id: &str,
    ) -> Result<Option<Message>, String> {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("Invalid Gmail message ID".into());
        }
        let mut url =
            Url::parse("https://gmail.googleapis.com/gmail/v1/users/me/messages/").unwrap();
        url.path_segments_mut().unwrap().pop_if_empty().push(id);
        url.query_pairs_mut().append_pair("format", "full");
        let response = self.request(
            caller,
            connection,
            Provider::Google,
            GMAIL_READ_SCOPE,
            "GET",
            url,
            Body::Empty,
            None,
        )?;
        if response.status == 404 {
            return Ok(None);
        }
        Message::from_gmail(&json_ok(response)?).map(Some)
    }
    pub fn gmail_sender(&self, caller: &str, connection: &str) -> Result<String, String> {
        let connected =
            self.connections
                .authorized(caller, connection, Provider::Google, GMAIL_SEND_SCOPE)?;
        mailbox(&connected.label)
    }
    // Called only after the durable claim in DraftStore::submit; never registered
    // as a service method or app tool.
    fn send_claimed(&mut self, draft: &Draft, attempt: &Attempt) -> SendOutcome {
        let raw = match raw_reply(&draft.reply, &attempt.message_id) {
            Ok(raw) => raw,
            Err(_) => return SendOutcome::Rejected("Invalid frozen reply".into()),
        };
        let response = self.request(
            &draft.app,
            &draft.connection,
            Provider::Google,
            GMAIL_SEND_SCOPE,
            "POST",
            Url::parse("https://gmail.googleapis.com/gmail/v1/users/me/messages/send").unwrap(),
            Body::Json(json!({"raw":raw,"threadId":draft.reply.thread_id})),
            None,
        );
        match response {
            Ok(r) if (200..300).contains(&r.status) => {
                match (r.body["id"].as_str(), r.body["threadId"].as_str()) {
                    (Some(id), Some(thread)) if !id.is_empty() && !thread.is_empty() => {
                        SendOutcome::Accepted(
                            json!({"message_id":id,"thread_id":thread,"accepted":true,"delivery_verified":false}),
                        )
                    }
                    _ => SendOutcome::Unknown,
                }
            }
            Ok(r) if matches!(r.status, 400 | 401 | 403 | 404 | 413 | 429) => {
                SendOutcome::Rejected(format!("Gmail rejected the submission (HTTP {})", r.status))
            }
            _ => SendOutcome::Unknown,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub thread_id: String,
    pub in_reply_to: String,
    pub references: String,
}
impl Reply {
    fn validate(&self) -> Result<(), String> {
        if mailbox(&self.from)? != self.from || mailbox(&self.to)? != self.to {
            return Err("Use one exact sender and recipient address".into());
        }
        if self.subject.len() > 512 || self.subject.chars().any(char::is_control) {
            return Err("Subject is too long or contains a line break".into());
        }
        if self.body.len() > 256 * 1024 || self.body.contains('\0') {
            return Err("Reply text exceeds its limit".into());
        }
        if self.thread_id.is_empty()
            || self.thread_id.len() > 256
            || !self
                .thread_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err("Invalid Gmail thread".into());
        }
        validate_message_ids(&self.in_reply_to)?;
        validate_message_ids(&self.references)?;
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Draft,
    AwaitingApproval,
    Sending,
    Accepted,
    FailedBeforeDelivery,
    OutcomeUnknown,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub id: String,
    pub message_id: String,
    pub revision: u64,
    pub status: Status,
    pub receipt: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub id: String,
    pub app: String,
    pub connection: String,
    pub source_message: String,
    pub revision: u64,
    pub reply: Reply,
    pub provenance: String,
    pub status: Status,
    pub attempts: Vec<Attempt>,
}
#[derive(Serialize, Deserialize, Default, Clone)]
struct Ledger {
    drafts: BTreeMap<String, Draft>,
}
pub struct DraftStore {
    path: PathBuf,
    app: String,
    connection: String,
    ledger: Ledger,
}
/// Non-clone, non-serializable capability owned by the native host review.
pub struct ReviewTicket {
    app: String,
    connection: String,
    draft: String,
    operation: String,
    revision: u64,
    snapshot: Reply,
    expires: Instant,
}
impl ReviewTicket {
    pub fn snapshot(&self) -> Value {
        json!({"app":self.app,"connection":self.connection,"draft":self.draft,"revision":self.revision,"operation":self.operation,"reply":self.snapshot})
    }
}
pub enum SendOutcome {
    Accepted(Value),
    Rejected(String),
    Unknown,
}

fn storage_path(root: &Path, app: &str, connection: &str) -> Result<PathBuf, String> {
    if app.is_empty() || app.len() > 256 || connection.is_empty() || connection.len() > 256 {
        return Err("Invalid Inbox identity".into());
    }
    let key = format!("{:x}", Sha256::digest(format!("{app}\0{connection}")));
    Ok(app_directory(root, "inbox", app).join(format!("{key}.json")))
}

/// The new service's private caches are partitioned by app as well as account,
/// so uninstall can erase historical connections that the person disconnected.
pub(crate) fn app_directory(root: &Path, folder: &str, app: &str) -> PathBuf {
    root.join("oauth")
        .join(folder)
        .join(format!("{:x}", Sha256::digest(app.as_bytes())))
}

/// Only uninstall calls this, after durable revocation. Ordinary disconnect
/// deliberately retains drafts.
pub(crate) fn purge_app(root: &Path, app: &str) -> Result<(), String> {
    purge_private_app(root, "inbox", app)
}

/// Remove only one app's flat, host-owned cache directory. Never recurse or
/// follow symlinks. A leaf symlink is unlinked without touching its target.
pub(crate) fn purge_private_app(root: &Path, folder: &str, app: &str) -> Result<(), String> {
    if app.is_empty() || app.len() > 256 {
        return Err("Invalid private-cache owner".into());
    }
    if private_directory(root, folder)?.is_none() {
        return Ok(());
    }
    let directory = app_directory(root, folder, app);
    let metadata = match std::fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("Cannot inspect app-private cache".into()),
    };
    if metadata.file_type().is_symlink() {
        return remove_private_file(&directory);
    }
    if !metadata.is_dir() {
        return Err("App-private cache is not a directory".into());
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&directory)
        .map_err(|_| "Cannot list app-private cache")?
        .take(4097)
    {
        let entry = entry.map_err(|_| "Cannot inspect app-private cache entry")?;
        let kind = entry
            .file_type()
            .map_err(|_| "Cannot inspect app-private cache entry")?;
        if !kind.is_file() && !kind.is_symlink() {
            return Err(
                "App-private cache contains a nonregular entry; refusing recursive removal".into(),
            );
        }
        files.push(entry.path());
    }
    if files.len() > 4096 {
        return Err("App-private cache entry limit exceeded".into());
    }
    for file in files {
        remove_private_file(&file)?;
    }
    std::fs::remove_dir(&directory)
        .map_err(|_| "Cannot erase app-private cache directory".to_string())?;
    #[cfg(unix)]
    std::fs::File::open(directory.parent().ok_or("Invalid cache directory")?)
        .and_then(|parent| parent.sync_all())
        .map_err(|_| "Cannot durably erase app-private cache directory".to_string())?;
    Ok(())
}

/// Reject symlinked host-private directories before reading or removing data.
/// The host serializes these operations with STORE_LOCK; root is host-supplied.
pub(crate) fn private_directory(root: &Path, folder: &str) -> Result<Option<PathBuf>, String> {
    for path in [
        root.to_path_buf(),
        root.join("oauth"),
        root.join("oauth").join(folder),
    ] {
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err("Host-private storage is not a regular directory".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Cannot inspect host-private storage".into()),
        }
    }
    Ok(Some(root.join("oauth").join(folder)))
}

pub(crate) fn remove_private_file(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() || metadata.file_type().is_symlink() => {}
        Ok(_) => return Err("Refusing to erase a nonregular host-private file".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("Cannot inspect host-private file".into()),
    }
    std::fs::remove_file(path).map_err(|_| "Cannot erase host-private file".to_string())?;
    #[cfg(unix)]
    std::fs::File::open(path.parent().ok_or("Invalid private file path")?)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "Cannot durably erase host-private file".to_string())?;
    Ok(())
}

impl DraftStore {
    /// Caller supplies the trusted host root. Hashing keeps app/connection IDs
    /// out of paths and prevents an app from selecting another account's store.
    pub fn open(root: &Path, app: &str, connection: &str) -> Result<Self, String> {
        let path = storage_path(root, app, connection)?;
        let ledger: Ledger = match std::fs::read(&path) {
            Ok(v) if v.len() <= 16 * 1024 * 1024 => {
                serde_json::from_slice(&v).map_err(|_| "Invalid Inbox draft storage")?
            }
            Ok(_) => return Err("Inbox draft storage exceeds its limit".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ledger::default(),
            Err(_) => return Err("Cannot read Inbox drafts".into()),
        };
        if ledger
            .drafts
            .values()
            .any(|d| d.app != app || d.connection != connection)
        {
            return Err("Inbox draft ownership mismatch".into());
        }
        Ok(Self {
            path,
            app: app.into(),
            connection: connection.into(),
            ledger,
        })
    }
    fn save(&mut self, next: Ledger) -> Result<(), String> {
        persist(&self.path, &next)?;
        self.ledger = next;
        Ok(())
    }
    pub fn get(&self, id: &str) -> Result<Draft, String> {
        self.ledger
            .drafts
            .get(id)
            .cloned()
            .ok_or_else(|| "Draft is unavailable to this app and account".into())
    }
    pub fn reply(&mut self, message: &Message, sender: &str) -> Result<Draft, String> {
        // Opening a source again must restore the same authoritative draft.
        if let Some(draft) = self
            .ledger
            .drafts
            .values()
            .find(|d| d.source_message == message.id)
        {
            return Ok(draft.clone());
        }
        if self.ledger.drafts.len() >= 256 {
            return Err("Inbox draft limit reached".into());
        }
        let to = mailbox(&message.reply_to)?;
        let mut references = message.references.clone();
        if !references.is_empty() {
            references.push(' ')
        }
        references.push_str(&message.message_id);
        let reply = Reply {
            from: mailbox(sender)?,
            to,
            subject: if message.subject.to_ascii_lowercase().starts_with("re:") {
                message.subject.clone()
            } else {
                format!("Re: {}", message.subject)
            },
            body: String::new(),
            thread_id: message.thread_id.clone(),
            in_reply_to: message.message_id.clone(),
            references,
        };
        reply.validate()?;
        let draft = Draft {
            id: Uuid::new_v4().to_string(),
            app: self.app.clone(),
            connection: self.connection.clone(),
            source_message: message.id.clone(),
            revision: 1,
            reply,
            provenance: "manual".into(),
            status: Status::Draft,
            attempts: vec![],
        };
        let mut next = self.ledger.clone();
        next.drafts.insert(draft.id.clone(), draft.clone());
        self.save(next)?;
        Ok(draft)
    }
    /// Editing and model proposals use the same compare-and-swap revision. The
    /// bound sender, source, thread and headers cannot be changed by an app.
    pub fn edit(
        &mut self,
        id: &str,
        revision: u64,
        to: &str,
        subject: &str,
        body: &str,
        provenance: &str,
    ) -> Result<Draft, String> {
        let mut draft = self.get(id)?;
        if draft.revision != revision {
            return Err("Draft changed; reload before applying this edit".into());
        }
        if matches!(
            draft.status,
            Status::Sending | Status::Accepted | Status::OutcomeUnknown
        ) {
            return Err("This submitted draft cannot be edited or automatically retried".into());
        }
        draft.reply.to = mailbox(to)?;
        draft.reply.subject = subject.into();
        draft.reply.body = body.into();
        draft.reply.validate()?;
        draft.revision = draft
            .revision
            .checked_add(1)
            .ok_or("Draft revision exhausted")?;
        draft.provenance = if provenance == "model" {
            "model"
        } else {
            "manual"
        }
        .into();
        draft.status = Status::Draft;
        let mut next = self.ledger.clone();
        next.drafts.insert(id.into(), draft.clone());
        self.save(next)?;
        Ok(draft)
    }
    pub fn review(&mut self, id: &str, revision: u64) -> Result<ReviewTicket, String> {
        let mut draft = self.get(id)?;
        if draft.revision != revision || draft.reply.body.trim().is_empty() {
            return Err("Save a nonempty reply and review its current revision".into());
        }
        if matches!(
            draft.status,
            Status::Sending
                | Status::Accepted
                | Status::OutcomeUnknown
                | Status::FailedBeforeDelivery
        ) {
            return Err(
                "This draft already has a submission result; automatic retry is unavailable".into(),
            );
        }
        // Every replacement invalidates the previous capability; it cannot be
        // claimed merely because it names the same revision.
        let operation = Uuid::new_v4().to_string();
        draft
            .attempts
            .retain(|a| a.status != Status::AwaitingApproval);
        draft.attempts.push(Attempt {
            id: operation.clone(),
            message_id: format!("<{operation}@octosense.local>"),
            revision,
            status: Status::AwaitingApproval,
            receipt: Value::Null,
        });
        draft.status = Status::AwaitingApproval;
        let ticket = ReviewTicket {
            app: self.app.clone(),
            connection: self.connection.clone(),
            draft: id.into(),
            operation,
            revision,
            snapshot: draft.reply.clone(),
            expires: Instant::now() + Duration::from_secs(600),
        };
        let mut next = self.ledger.clone();
        next.drafts.insert(id.into(), draft);
        self.save(next)?;
        Ok(ticket)
    }
    pub fn cancel(&mut self, ticket: ReviewTicket) -> Result<Draft, String> {
        let mut draft = self.validate(&ticket)?;
        draft.status = Status::Draft;
        draft.attempts.retain(|a| a.id != ticket.operation);
        let mut next = self.ledger.clone();
        next.drafts.insert(draft.id.clone(), draft.clone());
        self.save(next)?;
        Ok(draft)
    }
    pub(crate) fn validate(&self, ticket: &ReviewTicket) -> Result<Draft, String> {
        if ticket.app != self.app
            || ticket.connection != self.connection
            || Instant::now() >= ticket.expires
        {
            return Err("Review expired or belongs to another app/account".into());
        }
        let draft = self.get(&ticket.draft)?;
        if draft.revision != ticket.revision
            || draft.reply != ticket.snapshot
            || draft.status != Status::AwaitingApproval
            || !draft
                .attempts
                .last()
                .is_some_and(|a| a.id == ticket.operation && a.status == Status::AwaitingApproval)
        {
            return Err("The reviewed reply is no longer current; review again".into());
        }
        Ok(draft)
    }
    /// The shell's native review is the only production caller. Both pointer
    /// endpoints must carry real platform provenance; sheet/click/tool/AI events
    /// are insufficient. The booleans are never accepted from a JSON method.
    pub fn submit(
        &mut self,
        ticket: ReviewTicket,
        down_trusted: bool,
        up_trusted: bool,
        api: &mut Api<'_>,
    ) -> Result<Draft, String> {
        if !down_trusted || !up_trusted {
            return Err(
                "Approve & Send requires a physical activation of the native host review".into(),
            );
        }
        self.submit_account_checked(ticket, api)
    }
    #[cfg(feature = "host")]
    pub(crate) fn submit_authenticated(
        &mut self,
        ticket: ReviewTicket,
        approval: crate::approval::ApprovedOperation,
        api: &mut Api<'_>,
    ) -> Result<Draft, String> {
        approval.validate(&self.app, &self.connection, &ticket.snapshot())?;
        self.submit_account_checked(ticket, api)
    }
    fn submit_account_checked(
        &mut self,
        ticket: ReviewTicket,
        api: &mut Api<'_>,
    ) -> Result<Draft, String> {
        if !api
            .connections
            .active(&self.app)
            .is_some_and(|account| account.handle == self.connection)
        {
            return Err(
                "The active account changed; select the original account and review again".into(),
            );
        }
        let sender = api.gmail_sender(&self.app, &self.connection)?;
        if sender != ticket.snapshot.from {
            return Err("Sending account changed; review again".into());
        }
        self.submit_claimed(ticket, |draft, attempt| api.send_claimed(draft, attempt))
    }
    #[cfg(test)]
    fn submit_with(
        &mut self,
        ticket: ReviewTicket,
        down_trusted: bool,
        up_trusted: bool,
        send: impl FnOnce(&Draft, &Attempt) -> SendOutcome,
    ) -> Result<Draft, String> {
        if !down_trusted || !up_trusted {
            return Err(
                "Approve & Send requires a physical activation of the native host review".into(),
            );
        }
        self.submit_claimed(ticket, send)
    }
    fn submit_claimed(
        &mut self,
        ticket: ReviewTicket,
        send: impl FnOnce(&Draft, &Attempt) -> SendOutcome,
    ) -> Result<Draft, String> {
        let mut draft = self.validate(&ticket)?;
        draft.status = Status::Sending;
        draft.attempts.last_mut().unwrap().status = Status::Sending;
        let mut next = self.ledger.clone();
        next.drafts.insert(draft.id.clone(), draft.clone());
        self.save(next)?;
        // An interrupted claimed record remains Sending on disk and is surfaced
        // as uncertain. No other call may claim it or retry it.
        let result = send(&draft, draft.attempts.last().unwrap());
        let (status, receipt) = match result {
            SendOutcome::Accepted(r) => (Status::Accepted, r),
            SendOutcome::Rejected(e) => (
                Status::FailedBeforeDelivery,
                json!({"error":e,"accepted":false}),
            ),
            SendOutcome::Unknown => (
                Status::OutcomeUnknown,
                json!({"accepted":null,"message":"Submission may have succeeded. Check Gmail Sent before taking further action; automatic retry is disabled."}),
            ),
        };
        draft.status = status.clone();
        let attempt = draft.attempts.last_mut().unwrap();
        attempt.status = status;
        attempt.receipt = receipt;
        let mut next = self.ledger.clone();
        next.drafts.insert(draft.id.clone(), draft.clone());
        self.save(next)?;
        Ok(draft)
    }
}
pub(crate) fn persist<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "Cannot serialize Inbox drafts")?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Inbox draft storage exceeds its limit".into());
    }
    let parent = path.parent().ok_or("Missing host Inbox directory")?;
    std::fs::create_dir_all(parent).map_err(|_| "Cannot create Inbox storage")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Cannot protect Inbox storage")?;
    }
    let temp = parent.join(format!(".{}.tmp", Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options
            .open(&temp)
            .map_err(|_| "Cannot create draft transaction")?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "Cannot save draft transaction")?;
        std::fs::rename(&temp, path).map_err(|_| "Cannot commit draft transaction")?;
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "Cannot durably commit draft transaction")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("Gmail returned no {key}"))
}
/// Deliberately one mailbox, not silently simplified Reply-All or a list.
fn mailbox(text: &str) -> Result<String, String> {
    if text.len() > 320 || text.chars().any(char::is_control) || text.contains([',', ';']) {
        return Err("Only one reviewed recipient is supported".into());
    }
    let trimmed = text.trim();
    let address = if let Some(start) = trimmed.rfind('<') {
        if !trimmed.ends_with('>') {
            return Err("Invalid email address".into());
        }
        &trimmed[start + 1..trimmed.len() - 1]
    } else {
        trimmed
    };
    if address.matches('@').count() != 1
        || address.starts_with('@')
        || address.ends_with('@')
        || !address.is_ascii()
        || address
            .bytes()
            .any(|b| b <= 32 || b >= 127 || b"<>\\\"(),;:".contains(&b))
    {
        return Err("Use one valid email address".into());
    }
    Ok(address.to_string())
}
fn validate_message_ids(value: &str) -> Result<(), String> {
    if value.len() > 4096
        || value.chars().any(char::is_control)
        || value.split_whitespace().any(|id| {
            !id.starts_with('<')
                || !id.ends_with('>')
                || id.matches('@').count() != 1
                || id[1..id.len() - 1].contains(['<', '>'])
        })
    {
        return Err("Unsupported reply message headers".into());
    }
    Ok(())
}
fn raw_reply(reply: &Reply, message_id: &str) -> Result<String, String> {
    reply.validate()?;
    validate_message_ids(message_id)?;
    // Split Unicode on scalar boundaries before RFC2047 encoding.
    let subject = if reply.subject.is_ascii() {
        reply.subject.clone()
    } else {
        let mut words = Vec::new();
        let mut chunk = String::new();
        for c in reply.subject.chars() {
            if chunk.len() + c.len_utf8() > 36 {
                words.push(format!("=?UTF-8?B?{}?=", STANDARD.encode(chunk.as_bytes())));
                chunk.clear();
            }
            chunk.push(c);
        }
        if !chunk.is_empty() {
            words.push(format!("=?UTF-8?B?{}?=", STANDARD.encode(chunk.as_bytes())))
        }
        words.join("\r\n ")
    };
    let encoded_body = STANDARD.encode(
        reply
            .body
            .replace("\r\n", "\n")
            .replace('\n', "\r\n")
            .as_bytes(),
    );
    let body = encoded_body
        .as_bytes()
        .chunks(76)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join("\r\n");
    let mut raw=format!("From: {}\r\nTo: {}\r\nSubject: {}\r\nMessage-ID: {}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n",reply.from,reply.to,subject,message_id);
    if !reply.in_reply_to.is_empty() {
        raw.push_str(&format!("In-Reply-To: {}\r\n", reply.in_reply_to))
    }
    if !reply.references.is_empty() {
        raw.push_str(&format!("References: {}\r\n", reply.references))
    }
    raw.push_str("\r\n");
    raw.push_str(&body);
    raw.push_str("\r\n");
    Ok(URL_SAFE_NO_PAD.encode(raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Message {
        Message {
            id: "m1".into(),
            thread_id: "thread1".into(),
            from: "Clinic <clinic@example.test>".into(),
            reply_to: "clinic@example.test".into(),
            subject: "Appointment".into(),
            body: "Please confirm Tuesday at 09:00 Pacific.".into(),
            snippet: "Confirm".into(),
            message_id: "<m1@example.test>".into(),
            references: "".into(),
            labels: vec!["INBOX".into()],
            attachments_omitted: false,
        }
    }
    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn store() -> (Temp, DraftStore, Draft) {
        let temp = Temp(std::env::temp_dir().join(format!("inbox-test-{}", Uuid::new_v4())));
        let mut store = DraftStore::open(&temp.0, "sample.inbox", "connection-1").unwrap();
        let d = store.reply(&fixture(), "person@example.test").unwrap();
        let d = store
            .edit(
                &d.id,
                d.revision,
                &d.reply.to,
                &d.reply.subject,
                "Tuesday works.",
                "manual",
            )
            .unwrap();
        (temp, store, d)
    }
    #[test]
    fn uninstall_purge_removes_disconnected_accounts_without_touching_other_apps() {
        let (root, mut current, draft) = store();
        let ticket = current.review(&draft.id, draft.revision).unwrap();
        DraftStore::open(&root.0, "sample.inbox", "disconnected-account")
            .unwrap()
            .reply(&fixture(), "person@example.test")
            .unwrap();
        let mut other = DraftStore::open(&root.0, "other.app", "connection-1").unwrap();
        let other_draft = other.reply(&fixture(), "person@example.test").unwrap();
        purge_app(&root.0, "sample.inbox").unwrap();
        assert!(!app_directory(&root.0, "inbox", "sample.inbox").exists());
        assert!(DraftStore::open(&root.0, "sample.inbox", "connection-1")
            .unwrap()
            .get(&draft.id)
            .is_err());
        // A late native review dismissal cannot recreate uninstalled data.
        assert!(DraftStore::open(&root.0, "sample.inbox", "connection-1")
            .unwrap()
            .cancel(ticket)
            .is_err());
        assert!(!app_directory(&root.0, "inbox", "sample.inbox").exists());
        assert_eq!(
            DraftStore::open(&root.0, "other.app", "connection-1")
                .unwrap()
                .get(&other_draft.id)
                .unwrap(),
            other_draft
        );
        purge_app(&root.0, "sample.inbox").unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn uninstall_unlinks_leaf_symlinks_and_refuses_symlinked_parent() {
        use std::os::unix::fs::symlink;
        let root = Temp(std::env::temp_dir().join(format!("inbox-purge-{}", Uuid::new_v4())));
        let outside = root.0.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let secret = outside.join("keep.json");
        std::fs::write(&secret, b"other owner's data").unwrap();
        let mine = app_directory(&root.0, "inbox", "sample.inbox");
        std::fs::create_dir_all(&mine).unwrap();
        symlink(&secret, mine.join("file.json")).unwrap();
        symlink(&outside, mine.join("folder-link")).unwrap();
        purge_app(&root.0, "sample.inbox").unwrap();
        assert_eq!(std::fs::read(&secret).unwrap(), b"other owner's data");
        symlink(&outside, &mine).unwrap();
        purge_app(&root.0, "sample.inbox").unwrap();
        assert!(std::fs::symlink_metadata(&mine).is_err());
        let collection = root.0.join("oauth/inbox");
        std::fs::remove_dir(&collection).unwrap();
        symlink(&outside, &collection).unwrap();
        assert!(purge_app(&root.0, "sample.inbox").is_err());
        assert_eq!(std::fs::read(&secret).unwrap(), b"other owner's data");
    }
    #[test]
    fn injected_and_mixed_provenance_never_send() {
        for (down, up) in [(false, false), (false, true), (true, false)] {
            let (_t, mut s, d) = store();
            let ticket = s.review(&d.id, d.revision).unwrap();
            let mut called = false;
            assert!(s
                .submit_with(ticket, down, up, |_, _| {
                    called = true;
                    SendOutcome::Unknown
                })
                .is_err());
            assert!(!called);
            assert_eq!(s.get(&d.id).unwrap().status, Status::AwaitingApproval);
        }
    }
    #[test]
    fn editing_invalidates_snapshot_and_stale_chat_patch() {
        let (_t, mut s, d) = store();
        let ticket = s.review(&d.id, d.revision).unwrap();
        let changed = s
            .edit(
                &d.id,
                d.revision,
                &d.reply.to,
                &d.reply.subject,
                "Wednesday at 11:00 instead.",
                "manual",
            )
            .unwrap();
        assert!(s
            .submit_with(ticket, true, true, |_, _| panic!("must not send"))
            .is_err());
        assert!(s
            .edit(
                &d.id,
                d.revision,
                &d.reply.to,
                &d.reply.subject,
                "Stale model text",
                "model"
            )
            .is_err());
        assert_eq!(s.get(&d.id).unwrap(), changed);
    }
    #[test]
    fn exact_review_persists_claim_before_transport_and_cannot_replay() {
        let (t, mut s, d) = store();
        let old = s.review(&d.id, d.revision).unwrap();
        let ticket = s.review(&d.id, d.revision).unwrap();
        assert!(s
            .submit_with(old, true, true, |_, _| panic!("superseded"))
            .is_err());
        let sent = s
            .submit_with(ticket, true, true, |draft, attempt| {
                assert_eq!(draft.reply.body, "Tuesday works.");
                assert_eq!(attempt.revision, d.revision);
                assert_eq!(
                    DraftStore::open(&t.0, "sample.inbox", "connection-1")
                        .unwrap()
                        .get(&d.id)
                        .unwrap()
                        .status,
                    Status::Sending
                );
                SendOutcome::Accepted(json!({"accepted":true}))
            })
            .unwrap();
        assert_eq!(sent.status, Status::Accepted);
        assert!(s.review(&d.id, d.revision).is_err());
        assert_eq!(
            DraftStore::open(&t.0, "sample.inbox", "connection-1")
                .unwrap()
                .get(&d.id)
                .unwrap()
                .status,
            Status::Accepted
        );
    }
    #[test]
    fn cancelled_cross_app_expired_and_unknown_cannot_send() {
        let (t, mut s, d) = store();
        let ticket = s.review(&d.id, d.revision).unwrap();
        let mut other = DraftStore::open(&t.0, "different.app", "connection-1").unwrap();
        assert!(other
            .submit_with(ticket, true, true, |_, _| panic!("wrong app"))
            .is_err());
        let ticket = s.review(&d.id, d.revision).unwrap();
        s.cancel(ticket).unwrap();
        assert_eq!(s.get(&d.id).unwrap().status, Status::Draft);
        let mut ticket = s.review(&d.id, d.revision).unwrap();
        ticket.expires = Instant::now();
        assert!(s
            .submit_with(ticket, true, true, |_, _| panic!("expired"))
            .is_err());
        let ticket = s.review(&d.id, d.revision).unwrap();
        let result = s
            .submit_with(ticket, true, true, |_, _| SendOutcome::Unknown)
            .unwrap();
        assert_eq!(result.status, Status::OutcomeUnknown);
        assert!(s.review(&d.id, d.revision).is_err());
        assert!(s
            .edit(
                &d.id,
                d.revision,
                &d.reply.to,
                &d.reply.subject,
                "retry",
                "manual"
            )
            .is_err());
    }
    #[test]
    fn restored_draft_is_shared_by_reopen_and_headers_cannot_inject() {
        let (t, mut s, d) = store();
        assert_eq!(s.reply(&fixture(), "person@example.test").unwrap(), d);
        assert_eq!(
            DraftStore::open(&t.0, "sample.inbox", "connection-1")
                .unwrap()
                .get(&d.id)
                .unwrap(),
            d
        );
        assert!(s
            .edit(
                &d.id,
                d.revision,
                "target@example.test\r\nBcc: hidden@example.test",
                "OK",
                "body",
                "manual"
            )
            .is_err());
        assert!(s
            .edit(
                &d.id,
                d.revision,
                "a@example.test,b@example.test",
                "OK",
                "body",
                "manual"
            )
            .is_err());
        let mut r = d.reply.clone();
        r.subject = "时间确认 🌅".into();
        let raw = String::from_utf8(
            URL_SAFE_NO_PAD
                .decode(raw_reply(&r, "<operation@example.test>").unwrap())
                .unwrap(),
        )
        .unwrap();
        assert!(raw.contains("In-Reply-To: <m1@example.test>\r\n"));
        assert!(raw.contains("Content-Transfer-Encoding: base64"));
        assert!(!raw.contains("Bcc:"));
    }
    #[test]
    fn gmail_submission_uses_threaded_mime_and_no_json_send_bypass() {
        use crate::{
            oauth::Tokens,
            transport::{Request, Response, Transport},
            Connections, CredentialStore,
        };
        use std::{
            collections::BTreeSet,
            sync::{Arc, Mutex},
        };
        #[derive(Default)]
        struct Vault(Mutex<BTreeMap<String, String>>);
        impl CredentialStore for Vault {
            fn put(&self, k: &str, v: &str) -> Result<(), String> {
                self.0.lock().unwrap().insert(k.into(), v.into());
                Ok(())
            }
            fn get(&self, k: &str) -> Result<String, String> {
                self.0
                    .lock()
                    .unwrap()
                    .get(k)
                    .cloned()
                    .ok_or("missing".into())
            }
            fn remove(&self, k: &str) -> Result<(), String> {
                self.0.lock().unwrap().remove(k);
                Ok(())
            }
        }
        struct Gmail {
            calls: Mutex<usize>,
        }
        impl Transport for Gmail {
            fn send(&self, r: Request) -> Result<Response, String> {
                *self.calls.lock().unwrap() += 1;
                assert_eq!(r.method, "POST");
                assert_eq!(
                    r.url.as_str(),
                    "https://gmail.googleapis.com/gmail/v1/users/me/messages/send"
                );
                assert_eq!(r.bearer.as_deref(), Some("fixture-access-only"));
                let Body::Json(body) = r.body else {
                    panic!("JSON required")
                };
                assert_eq!(body["threadId"], "thread1");
                let raw = String::from_utf8(
                    URL_SAFE_NO_PAD
                        .decode(body["raw"].as_str().unwrap())
                        .unwrap(),
                )
                .unwrap();
                assert!(raw.contains("To: clinic@example.test\r\n"));
                assert!(raw.contains("In-Reply-To: <m1@example.test>\r\n"));
                assert!(raw.contains("From: person@example.test\r\n"));
                assert!(raw.contains(&STANDARD.encode("Tuesday works.")));
                Ok(Response {
                    status: 200,
                    body: json!({"id":"gmail-receipt","threadId":"thread1"}),
                    etag: None,
                })
            }
        }
        let root = Temp(std::env::temp_dir().join(format!("gmail-wire-{}", Uuid::new_v4())));
        let mut connections =
            Connections::open(&root.0.join("oauth"), Arc::new(Vault::default())).unwrap();
        let c = connections
            .connect(
                "sample.inbox",
                Provider::Google,
                "subject-1",
                "person@example.test",
                Tokens {
                    access: "fixture-access-only".into(),
                    refresh: None,
                    expires_at: None,
                    scopes: BTreeSet::from([GMAIL_READ_SCOPE.into(), GMAIL_SEND_SCOPE.into()]),
                },
            )
            .unwrap();
        struct Reads;
        impl Transport for Reads {
            fn send(&self, request: Request) -> Result<Response, String> {
                assert_eq!(request.method, "GET");
                assert_eq!(request.bearer.as_deref(), Some("fixture-access-only"));
                assert_eq!(request.url.query(), Some("format=full"));
                let status = if request.url.path().ends_with("/deleted") {
                    404
                } else {
                    503
                };
                Ok(Response {
                    status,
                    body: json!({"error":"fixture"}),
                    etag: None,
                })
            }
        }
        {
            let mut reader = Api {
                connections: &mut connections,
                transport: &Reads,
                google_client: None,
                google_client_secret: None,
                now: 100,
            };
            assert!(reader
                .inbox_message_if_present("sample.inbox", &c.handle, "deleted")
                .unwrap()
                .is_none());
            assert!(reader
                .inbox_message_if_present("sample.inbox", &c.handle, "unavailable")
                .is_err());
            assert!(reader
                .inbox_message_if_present("sample.inbox", &c.handle, "../other")
                .is_err());
        }
        let gmail = Gmail {
            calls: Mutex::new(0),
        };
        let mut api = Api {
            connections: &mut connections,
            transport: &gmail,
            google_client: None,
            google_client_secret: None,
            now: 100,
        };
        let mut store = DraftStore::open(&root.0, "sample.inbox", &c.handle).unwrap();
        let d = store.reply(&fixture(), "person@example.test").unwrap();
        let d = store
            .edit(
                &d.id,
                d.revision,
                &d.reply.to,
                &d.reply.subject,
                "Tuesday works.",
                "manual",
            )
            .unwrap();
        let old_ticket = store.review(&d.id, d.revision).unwrap();
        api.connections
            .connect(
                "sample.inbox",
                Provider::Google,
                "subject-2",
                "other@example.test",
                Tokens {
                    access: "other-fixture".into(),
                    refresh: None,
                    expires_at: None,
                    scopes: BTreeSet::from([GMAIL_READ_SCOPE.into(), GMAIL_SEND_SCOPE.into()]),
                },
            )
            .unwrap();
        assert!(store.submit(old_ticket, true, true, &mut api).is_err());
        assert_eq!(*gmail.calls.lock().unwrap(), 0);
        api.connections.select("sample.inbox", &c.handle).unwrap();
        let ticket = store.review(&d.id, d.revision).unwrap();
        let sent = store.submit(ticket, true, true, &mut api).unwrap();
        assert_eq!(sent.status, Status::Accepted);
        assert_eq!(*gmail.calls.lock().unwrap(), 1);
        assert_eq!(
            sent.attempts.last().unwrap().receipt["delivery_verified"],
            false
        );
        assert!(!serde_json::to_string(&sent)
            .unwrap()
            .contains("fixture-access-only"));
        assert!(store.review(&d.id, d.revision).is_err());
        assert_eq!(*gmail.calls.lock().unwrap(), 1);
    }

    #[test]
    fn gmail_plaintext_parts_and_attachment_notice() {
        let value = json!({"id":"m","threadId":"t","snippet":"preview","payload":{"mimeType":"multipart/mixed","headers":[{"name":"From","value":"a@example.test"}],"parts":[{"mimeType":"text/plain","body":{"data":URL_SAFE_NO_PAD.encode("Hello 世界")}},{"filename":"private.pdf","body":{"attachmentId":"a"}}]}});
        let m = Message::from_gmail(&value).unwrap();
        assert_eq!(m.body, "Hello 世界\n");
        assert!(m.attachments_omitted);
        assert_eq!(m.reply_to, "a@example.test");
    }
}
