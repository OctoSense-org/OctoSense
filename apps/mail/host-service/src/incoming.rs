//! Durable new-mail queue. Mailbox data, cursor and pending events commit in
//! one atomic file replacement; callers acknowledge only after their agent turn
//! succeeds. The shell owns scheduling, consent and retry policy.
use super::*;

/// Maximum outstanding events per inbox. Backpressure leaves the cursor intact.
const MAX_PENDING: usize = 128;
static SERIAL: Mutex<()> = Mutex::new(());
pub(crate) fn lock() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}
// UI status and completion paths may hold the dispatcher's settings lock.
// They must never queue behind collection's network wait.
fn try_lock() -> Option<std::sync::MutexGuard<'static, ()>> {
    match SERIAL.try_lock() {
        Ok(guard) => Some(guard),
        Err(std::sync::TryLockError::Poisoned(error)) => Some(error.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => None,
    }
}
type Backend = (Arc<dyn Transport>, Arc<dyn Vault>);
fn backend() -> &'static Mutex<Option<Backend>> {
    static BACKEND: std::sync::OnceLock<Mutex<Option<Backend>>> = std::sync::OnceLock::new();
    BACKEND.get_or_init(Default::default)
}
pub(crate) fn set_backend(transport: Arc<dyn Transport>, vault: Arc<dyn Vault>) {
    *backend().lock().unwrap_or_else(|e| e.into_inner()) = Some((transport, vault));
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncomingEvent {
    /// Stable ASCII id, also suitable as a glance card_id.
    pub id: String,
    pub account: String,
    pub folder: String,
    pub message: String,
    pub sender: String,
    pub subject: String,
}
impl IncomingEvent {
    pub fn to_json(&self) -> Value {
        json!({"id": self.id, "account": self.account, "folder": self.folder,
            "message": self.message, "sender": self.sender, "subject": self.subject})
    }
    fn from_json(v: &Value) -> Result<Self, String> {
        let field = |key| {
            v[key]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "Invalid pending mail event".to_string())
        };
        Ok(Self {
            id: field("id")?,
            account: field("account")?,
            folder: field("folder")?,
            message: field("message")?,
            sender: field("sender")?,
            subject: field("subject")?,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectReport {
    pub new: usize,
    pub total: usize,
    /// A first successful collection establishes a baseline, emitting nothing.
    pub baselined: bool,
    pub pending: usize,
}

/// Blocking host-only collection. Run off the UI thread. Uses the same
/// registered transport, vault and serialization as the UI's mail.sync.
pub fn collect_inbox(host_dir: &Path, app: &str, account: &str) -> Result<CollectReport, String> {
    let (transport, vault) = backend()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or("Mail service is not registered")?;
    collect(
        &Store::at(host_dir, vault),
        transport.as_ref(),
        app,
        account,
        INBOX,
    )
}

/// Strictly read a mailbox; corrupt state must not silently reset its baseline.
fn mailbox(store: &Store, account: &str, folder: &str) -> Result<Value, String> {
    match std::fs::read(store.mailbox_path(account, folder)) {
        Ok(bytes) => {
            let v: Value = serde_json::from_slice(&bytes)
                .map_err(|e| format!("Invalid stored mailbox: {e}"))?;
            if !v.is_object()
                || !v["messages"].is_array()
                || v.get("state").is_some_and(|s| !s.is_object())
                || v.get("pending_events").is_some_and(|s| !s.is_array())
                || v.get("skip_decisions").is_some_and(|s| !s.is_array())
                || v.get("publication_receipts").is_some_and(|s| !s.is_array())
            {
                return Err("Invalid stored mailbox".into());
            }
            Ok(v)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(json!({"messages": [], "state": {}}))
        }
        Err(e) => Err(format!("Cannot read mailbox: {e}")),
    }
}

pub(crate) fn collect(
    store: &Store,
    transport: &dyn Transport,
    app: &str,
    account: &str,
    folder: &str,
) -> Result<CollectReport, String> {
    let _guard = lock();
    let credentials = store.account_for(app, account)?;
    let mut box_ = mailbox(store, account, folder)?;
    if box_.get("state").is_none() {
        box_["state"] = json!({"seen": box_.get("seen").cloned().unwrap_or(json!([]))});
    }
    let mut fetch_state = box_["state"].clone();
    // POP3 has no increasing UID cursor: the first collection records its full
    // UIDL snapshot, including old messages outside the downloaded batch.
    fetch_state["_incoming_baseline"] = json!(box_["incoming_baselined"] != true);
    let fetched = transport.fetch(&credentials, folder, &fetch_state)?;
    let fetched_messages = fetched["messages"]
        .as_array()
        .ok_or("Invalid transport messages")?;
    let baseline = box_["incoming_baselined"] != true || fetched["reset"] == true;
    let mut messages = box_["messages"].as_array().cloned().unwrap_or_default();
    let mut known: HashSet<String> = messages.iter().map(|m| text(m, "id").to_string()).collect();
    let mut pending = box_
        .get("pending_events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut new = 0;
    // The transport returns newest first. Inserting in reverse keeps that order.
    for raw in fetched_messages.iter().rev() {
        let message = normalize(raw.clone());
        let id = text(&message, "id");
        if id.is_empty() {
            return Err("Fetched mail has no identity".into());
        }
        if !known.insert(id.to_string()) {
            continue;
        }
        if !baseline && folder == INBOX {
            if pending.len() >= MAX_PENDING {
                return Err("Incoming mail queue is full; acknowledge pending events before collecting again".into());
            }
            let event_id = format!(
                "mail-{}",
                &network::hash(&json!([account, folder, id]).to_string())[..40]
            );
            pending.push(
                IncomingEvent {
                    id: event_id,
                    account: account.into(),
                    folder: folder.into(),
                    message: id.into(),
                    sender: clip(text(&message, "address"), 160),
                    subject: clip(text(&message, "subject"), 200),
                }
                .to_json(),
            );
        }
        messages.insert(0, message);
        new += 1;
    }
    box_["messages"] = json!(messages);
    box_["state"] = fetched["state"].clone();
    box_["incoming_baselined"] = json!(true);
    box_["pending_events"] = json!(pending);
    box_.as_object_mut().unwrap().remove("seen");
    store.save_mailbox(account, folder, &box_)?;
    Ok(CollectReport {
        new,
        total: messages.len(),
        baselined: baseline,
        pending: pending.len(),
    })
}

/// Pending events survive retries and process restarts. Only granted accounts
/// are visible; removal invalidates these reads, even for a previously held id.
pub fn pending_events(
    host_dir: &Path,
    app: &str,
    account: &str,
) -> Result<Vec<IncomingEvent>, String> {
    let _guard = lock();
    let store = Store::at(host_dir, vault::platform());
    store.granted(app, account)?;
    let box_ = mailbox(&store, account, INBOX)?;
    read_pending(&box_)
}

fn read_pending(box_: &Value) -> Result<Vec<IncomingEvent>, String> {
    box_.get("pending_events")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(IncomingEvent::from_json)
        .collect()
}

/// Snapshot for UI status: None means collection/storage mutation is busy.
/// It never waits for the collection mutex; account checks still run whenever
/// a snapshot is returned. Disk read/JSON errors remain visible errors.
pub fn pending_events_try(
    host_dir: &Path,
    app: &str,
    account: &str,
) -> Result<Option<Vec<IncomingEvent>>, String> {
    let Some(_guard) = try_lock() else {
        return Ok(None);
    };
    let store = Store::at(host_dir, vault::platform());
    store.granted(app, account)?;
    read_pending(&mailbox(&store, account, INBOX)?).map(Some)
}

/// Verify a durable publication/skip decision and acknowledge under one lock.
/// None means busy (retry later); Some(false) means unresolved (retain/retry
/// the event); Some(true) means resolved and acknowledged. Repeating a saved
/// acknowledgement is successful while its resolution receipt is retained.
/// The caller must separately verify successful turn completion and its lease.
/// This method never waits for the collection mutex.
pub fn resolve_and_ack_try(
    host_dir: &Path,
    app: &str,
    account: &str,
    event_id: &str,
) -> Result<Option<bool>, String> {
    let Some(_guard) = try_lock() else {
        return Ok(None);
    };
    let store = Store::at(host_dir, vault::platform());
    store.granted(app, account)?;
    let mut box_ = mailbox(&store, account, INBOX)?;
    if !has_resolution(&box_, event_id) {
        return Ok(Some(false));
    }
    let mut pending = box_
        .get("pending_events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let before = pending.len();
    pending.retain(|event| text(event, "id") != event_id);
    if pending.len() != before {
        box_["pending_events"] = json!(pending);
        store.save_mailbox(account, INBOX, &box_)?;
    }
    Ok(Some(true))
}

/// Idempotent acknowledgement. Unknown/already-acknowledged ids return false.
/// Never acknowledge on dispatch alone: retry after failure with the same id.
pub fn acknowledge_event(
    host_dir: &Path,
    app: &str,
    account: &str,
    event_id: &str,
) -> Result<bool, String> {
    let _guard = lock();
    let store = Store::at(host_dir, vault::platform());
    store.granted(app, account)?;
    let mut box_ = mailbox(&store, account, INBOX)?;
    let mut pending = box_
        .get("pending_events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let before = pending.len();
    pending.retain(|event| text(event, "id") != event_id);
    if before == pending.len() {
        return Ok(false);
    }
    box_["pending_events"] = json!(pending);
    store.save_mailbox(account, INBOX, &box_)?;
    Ok(true)
}

/// Save a successful event notice once. Notice retries retain their existing
/// event-level behavior; generated-card repairs use `publish_revision` below.
pub(crate) fn publish_once(
    store: &Store,
    app: &str,
    account: &str,
    card_id: &str,
    publish: impl FnOnce() -> Result<Value, String>,
) -> Result<Value, String> {
    publish_versioned(store, app, account, card_id, None, None, publish)
}

/// Identical card payloads reuse the durable receipt; an intentional source/data
/// refresh publishes again under the same stable card identity. Only a successful
/// callback replaces the receipt. Old receipts lacking a fingerprint refresh once.
/// A crash between publication and receipt save remains at-least-once.
pub(crate) fn publish_revision(
    store: &Store,
    app: &str,
    account: &str,
    card_id: &str,
    fingerprint: &str,
    binding: Option<&Value>,
    publish: impl FnOnce() -> Result<Value, String>,
) -> Result<Value, String> {
    publish_versioned(
        store,
        app,
        account,
        card_id,
        Some(fingerprint),
        binding,
        publish,
    )
}

fn publish_versioned(
    store: &Store,
    app: &str,
    account: &str,
    card_id: &str,
    fingerprint: Option<&str>,
    binding: Option<&Value>,
    publish: impl FnOnce() -> Result<Value, String>,
) -> Result<Value, String> {
    let _guard = lock();
    store.granted(app, account)?;
    let mut box_ = mailbox(store, account, INBOX)?;
    let mut receipts = box_
        .get("publication_receipts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let previous = receipts.iter().position(|r| text(r, "id") == card_id);
    let binding = binding.map(|b| json!({"publisher":b["publisher"], "account":b["account"],
        "draft_id":b["draft_id"], "source_message":b["source_message"], "chat_thread":b["chat_thread"]}));
    if let Some(index) = previous {
        if fingerprint.is_some()
            && !receipts[index]["binding"].is_null()
            && binding.as_ref() != Some(&receipts[index]["binding"])
        {
            return Err("A published Mail card cannot change its account, email or draft".into());
        }
        if fingerprint.is_none() || receipts[index]["fingerprint"].as_str() == fingerprint {
            return Ok(receipts[index]["result"].clone());
        }
    }
    let pending = box_
        .get("pending_events")
        .and_then(Value::as_array)
        .is_some_and(|events| events.iter().any(|e| text(e, "id") == card_id));
    let result = publish()?;
    if fingerprint.is_some() || pending {
        let receipt =
            json!({"id":card_id, "fingerprint":fingerprint, "binding":binding, "result":result});
        if let Some(index) = previous {
            receipts[index] = receipt;
        } else {
            receipts.push(receipt);
        }
        if receipts.len() > 256 {
            receipts.remove(0);
        }
        box_["publication_receipts"] = json!(receipts);
        store.save_mailbox(account, INBOX, &box_)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct MemoryVault;
    impl Vault for MemoryVault {
        fn put(&self, _: &vault::Place, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn get(&self, _: &vault::Place, _: &str) -> Result<String, String> {
            Ok("secret-never-in-agent-results".into())
        }
        fn remove(&self, _: &vault::Place, _: &str) {}
    }
    #[derive(Default)]
    struct Fake {
        messages: Mutex<Vec<Value>>,
        fail: Mutex<bool>,
        marks: AtomicUsize,
    }
    impl Transport for Fake {
        fn test(&self, _: &Value) -> Result<(), String> {
            Ok(())
        }
        fn folders(&self, _: &Value) -> Result<Vec<Value>, String> {
            panic!("pure reads must not connect")
        }
        fn fetch(&self, _: &Value, _: &str, _: &Value) -> Result<Value, String> {
            if *self.fail.lock().unwrap() {
                return Err("offline".into());
            }
            let messages = self.messages.lock().unwrap().clone();
            // Deliberately redeliver known messages: the host must deduplicate.
            Ok(json!({"state": {"cursor": messages.len()}, "messages": messages, "reset": false}))
        }
        fn mark_seen(&self, _: &Value, _: &str, _: &Value) -> Result<(), String> {
            self.marks.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        fn send(&self, _: &Value, _: &Value) -> Result<Value, String> {
            panic!("must not send")
        }
    }
    struct Fixture {
        dir: PathBuf,
        store: Store,
        fake: Fake,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "mail-incoming-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let store = Store::at(&dir, Arc::new(MemoryVault));
            store
                .save_accounts(&[
                    json!({"id":"a1","address":"one@example.com","apps":["os.mail"]}),
                    json!({"id":"b2","address":"two@example.com","apps":["other.app"]}),
                ])
                .unwrap();
            Self {
                dir,
                store,
                fake: Fake::default(),
            }
        }
        fn add(&self, id: &str) {
            self.fake.messages.lock().unwrap().push(json!({"id":id,"address":"sender@example.com","sender":"Sender","subject":format!("Subject {id}"),"body":"测试 hello","date":"2026-10-03T12:00:00Z","unread":true}));
        }
        fn sync(&self) -> Result<CollectReport, String> {
            collect(&self.store, &self.fake, "os.mail", "a1", INBOX)
        }
        fn pending(&self) -> Vec<IncomingEvent> {
            pending_events(&self.dir, "os.mail", "a1").unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    // Other tests may momentarily own the process-wide lock. Wait only in
    // this test helper, never in either production try API.
    fn retry(operation: &dyn Fn() -> Result<Option<bool>, String>) -> Result<Option<bool>, String> {
        loop {
            match operation() {
                Ok(None) => std::thread::yield_now(),
                result => break result,
            }
        }
    }

    #[test]
    fn baseline_retry_restart_ack_and_deduplication() {
        let f = Fixture::new();
        f.add("old");
        assert!(f.sync().unwrap().baselined);
        assert!(f.pending().is_empty());
        f.add("new");
        assert_eq!(f.sync().unwrap().new, 1);
        let first = f.pending();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].message, "new");
        // Fresh Store/public API reconstructs the queue from disk, not memory.
        assert_eq!(f.pending(), first);
        assert_eq!(f.sync().unwrap().new, 0);
        assert_eq!(f.pending(), first);
        assert!(acknowledge_event(&f.dir, "os.mail", "a1", &first[0].id).unwrap());
        assert!(!acknowledge_event(&f.dir, "os.mail", "a1", &first[0].id).unwrap());
        assert!(f.pending().is_empty());
        f.sync().unwrap();
        assert!(f.pending().is_empty());
    }
    #[test]
    fn failed_fetch_and_failed_save_leave_cursor_and_queue_unchanged() {
        let f = Fixture::new();
        f.sync().unwrap();
        f.add("new");
        let path = f.store.mailbox_path("a1", INBOX);
        let before = std::fs::read(&path).unwrap();
        *f.fake.fail.lock().unwrap() = true;
        assert!(f.sync().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        *f.fake.fail.lock().unwrap() = false;
        std::fs::create_dir(path.with_extension("tmp")).unwrap();
        assert!(f.sync().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_dir(path.with_extension("tmp")).unwrap();
        f.sync().unwrap();
        assert_eq!(f.pending().len(), 1);
    }
    #[test]
    fn queue_backpressure_does_not_advance_and_corruption_does_not_rebaseline() {
        let f = Fixture::new();
        f.sync().unwrap();
        for n in 0..MAX_PENDING + 1 {
            f.add(&format!("m{n}"));
        }
        assert!(f.sync().unwrap_err().contains("full"));
        assert!(f.pending().is_empty());
        assert_eq!(f.store.mailbox("a1", INBOX)["state"]["cursor"], 0);
        std::fs::write(f.store.mailbox_path("a1", INBOX), b"not-json").unwrap();
        assert!(f.sync().unwrap_err().contains("Invalid"));
    }
    #[test]
    fn pure_peek_is_bounded_preserves_unread_and_accounts_are_isolated() {
        let f = Fixture::new();
        f.add("m1");
        f.sync().unwrap();
        let path = f.store.mailbox_path("a1", INBOX);
        let before = std::fs::read(&path).unwrap();
        let result = agent_read(
            &f.store,
            "os.mail",
            "peek",
            &json!({"account":"a1","message":"m1"}),
        )
        .unwrap();
        assert_eq!(result["body"], "测试 hello");
        assert_eq!(result["unread"], true);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(f.fake.marks.load(Ordering::Relaxed), 0);
        assert!(!result.to_string().contains("secret-never"));
        assert!(result.get("html").is_none());
        for method in ["accounts", "folders", "list", "peek"] {
            assert!(agent_read(
                &f.store,
                "os.mail",
                method,
                &json!({"account":"b2","message":"m1"})
            )
            .is_err());
            assert!(agent_read(
                &f.store,
                "other.app",
                method,
                &json!({"account":"a1","message":"m1"})
            )
            .is_err());
        }
        assert_eq!(
            agent_read(&f.store, "os.mail", "accounts", &json!({"account":"a1"})).unwrap()
                ["accounts"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let mut box_ = f.store.mailbox("a1", INBOX);
        box_["messages"][0]["body"] = json!("中文".repeat(1000));
        f.store.save_mailbox("a1", INBOX, &box_).unwrap();
        let page = agent_read(
            &f.store,
            "os.mail",
            "peek",
            &json!({"account":"a1","message":"m1"}),
        )
        .unwrap();
        assert!(page.to_string().len() < 3800);
        assert!(page["next_offset"].as_u64().is_some());
        box_["messages"][0]["body"] = json!("\u{01}\"\n".repeat(2000));
        f.store.save_mailbox("a1", INBOX, &box_).unwrap();
        let escaped = agent_read(
            &f.store,
            "os.mail",
            "peek",
            &json!({"account":"a1", "message":"m1"}),
        )
        .unwrap();
        assert!(escaped.to_string().len() < 3800);
        box_["messages"][0]["body"] = json!("中文".repeat(1000));
        f.store.save_mailbox("a1", INBOX, &box_).unwrap();
        assert!(agent_read(
            &f.store,
            "os.mail",
            "peek",
            &json!({"account":"a1","message":"m1","offset":1})
        )
        .is_err());
    }
    #[test]
    fn only_saved_publication_or_explicit_skip_resolves_an_event() {
        let f = Fixture::new();
        f.sync().unwrap();
        f.add("new");
        f.sync().unwrap();
        let event = f.pending().pop().unwrap();
        assert!(!event_resolved(&f.dir, "os.mail", "a1", &event.id).unwrap());
        assert!(publish_once(&f.store, "os.mail", "a1", &event.id, || Err(
            "invalid L0".into()
        ))
        .is_err());
        assert!(!event_resolved(&f.dir, "os.mail", "a1", &event.id).unwrap());
        assert!(skip_event(&f.store, "os.mail", "a1", "other-event", "no_action").is_err());
        assert!(skip_event(&f.store, "os.mail", "b2", &event.id, "no_action").is_err());
        assert!(skip_event(&f.store, "os.mail", "a1", &event.id, "done").is_err());
        skip_event(&f.store, "os.mail", "a1", &event.id, "outside_policy").unwrap();
        assert!(event_resolved(&f.dir, "os.mail", "a1", &event.id).unwrap());
        assert_eq!(
            f.pending().len(),
            1,
            "a skip never acknowledges on behalf of the host"
        );
    }

    #[test]
    fn selected_folder_sync_remains_account_scoped_without_inbox_events() {
        let f = Fixture::new();
        f.sync().unwrap();
        f.add("archive-message");
        let report = collect(&f.store, &f.fake, "os.mail", "a1", "Archive").unwrap();
        assert_eq!(report.new, 1);
        assert!(f.pending().is_empty());
        let page = agent_read(
            &f.store,
            "os.mail",
            "list",
            &json!({"account":"a1", "folder":"Archive"}),
        )
        .unwrap();
        assert_eq!(page["messages"][0]["id"], "archive-message");
        assert!(collect(&f.store, &f.fake, "other.app", "a1", "Archive").is_err());
        let folders: Vec<Value> = (0..29)
            .map(|n| json!({"id":format!("folder-{n}"),"name":format!("Folder {n}")}))
            .collect();
        write_atomic(
            &f.store.dir.join("folders-a1.json"),
            &serde_json::to_vec(&folders).unwrap(),
        )
        .unwrap();
        let page = agent_read(
            &f.store,
            "os.mail",
            "folders",
            &json!({"account":"a1", "offset":24}),
        )
        .unwrap();
        assert_eq!(page["folders"].as_array().unwrap().len(), 5);
        assert!(page["next_offset"].is_null());
        assert_eq!(page["total"], 29);
    }

    #[test]
    fn card_bridge_fixes_publisher_target_and_rejects_unauthorized_account() {
        let f = Fixture::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let writes = seen.clone();
        on_publish_card(Some(Arc::new(move |app, args| {
            writes.lock().unwrap().push((app.to_string(), args.clone()));
            Ok(json!({"card_id":args["card_id"]}))
        })));
        let mut args = json!({"account":"a1", "card_id":"event-1", "source":"host-checks-this", "title":"Delivery", "data":{}});
        assert!(publish_card(&f.store, "other.app", &args).is_err());
        args["account"] = json!("b2");
        assert!(publish_card(&f.store, "os.mail", &args).is_err());
        args["account"] = json!("a1");
        args["summary"] = json!("界".repeat(201));
        assert!(publish_card(&f.store, "os.mail", &args)
            .unwrap_err()
            .contains("200"));
        assert!(
            seen.lock().unwrap().is_empty(),
            "reject over-budget summaries before publishing"
        );
        args["summary"] = json!("界".repeat(200));
        args["open"] = json!({"app":"other"});
        publish_card(&f.store, "os.mail", &args).unwrap();
        let captured = seen.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].0, "os.mail");
        assert_eq!(captured[0].1["open"]["app"], "mail");
        assert_eq!(
            captured[0].1["summary"].as_str().unwrap().chars().count(),
            200
        );
        assert!(captured[0].1.get("account").is_none());
        drop(captured);
        publish_card(&f.store, "os.mail", &args).unwrap();
        assert_eq!(
            seen.lock().unwrap().len(),
            1,
            "identical retry does not notify again"
        );
        args["source"] = json!("repaired-source-host-checks-this");
        args["notify"] = json!(true);
        publish_card(&f.store, "os.mail", &args).unwrap();
        let captured = seen.lock().unwrap();
        assert_eq!(
            captured.len(),
            2,
            "changed source reaches the same card publisher"
        );
        assert_eq!(captured[1].1["card_id"], "event-1");
        assert_eq!(
            captured[1].1["notify"], true,
            "a deliberate refresh honors explicit notification"
        );
        drop(captured);
        on_publish_card(None);
    }

    #[test]
    fn status_and_completion_do_not_wait_behind_collection_lock() {
        let f = Fixture::new();
        f.sync().unwrap();
        f.add("new");
        f.sync().unwrap();
        let event = f.pending().pop().unwrap();
        let held = lock();
        let dir = f.dir.clone();
        let id = event.id.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let snapshot = pending_events_try(&dir, "os.mail", "a1");
            let completion = resolve_and_ack_try(&dir, "os.mail", "a1", &id);
            tx.send((snapshot, completion)).unwrap();
        });
        // Release before asserting/joining so a regression fails promptly
        // rather than leaving a blocked thread or poisoning the test suite.
        let observed = rx.recv_timeout(std::time::Duration::from_secs(1));
        drop(held);
        worker.join().unwrap();
        let (snapshot, completion) =
            observed.expect("status/completion waited for the collection lock");
        assert_eq!(snapshot.unwrap(), None);
        assert_eq!(completion.unwrap(), None);
        assert_eq!(f.pending().len(), 1);
    }

    #[test]
    fn resolution_and_ack_are_atomic_and_failed_save_leaves_event_pending() {
        let f = Fixture::new();
        f.sync().unwrap();
        f.add("new");
        f.sync().unwrap();
        let event = f.pending().pop().unwrap();
        let ack = || resolve_and_ack_try(&f.dir, "os.mail", "a1", &event.id);
        assert_eq!(retry(&ack).unwrap(), Some(false));
        assert_eq!(f.pending().len(), 1);
        skip_event(&f.store, "os.mail", "a1", &event.id, "no_action").unwrap();
        let path = f.store.mailbox_path("a1", INBOX);
        let before = std::fs::read(&path).unwrap();
        std::fs::create_dir(path.with_extension("tmp")).unwrap();
        assert!(retry(&ack).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(f.pending().len(), 1);
        std::fs::remove_dir(path.with_extension("tmp")).unwrap();
        assert_eq!(retry(&ack).unwrap(), Some(true));
        assert!(f.pending().is_empty());
        assert_eq!(retry(&ack).unwrap(), Some(true));
        let other_account = || resolve_and_ack_try(&f.dir, "os.mail", "b2", &event.id);
        assert!(retry(&other_account).is_err());
    }

    #[test]
    fn removing_an_account_revokes_pending_reads_ack_and_collection() {
        let f = Fixture::new();
        f.sync().unwrap();
        f.add("new");
        f.sync().unwrap();
        let event = f.pending().pop().unwrap();
        {
            let _guard = lock();
            f.store.forget("a1");
            f.store.save_accounts(&[]).unwrap();
        }
        assert!(pending_events(&f.dir, "os.mail", "a1").is_err());
        assert!(acknowledge_event(&f.dir, "os.mail", "a1", &event.id).is_err());
        assert!(f.sync().is_err());
        assert!(!f.store.mailbox_path("a1", INBOX).exists());
    }
    #[test]
    fn changed_card_repairs_after_ack_and_restart_but_identical_retries_do_not_publish() {
        let f = Fixture::new();
        f.sync().unwrap();
        f.add("new");
        f.sync().unwrap();
        let event = f.pending().pop().unwrap();
        let calls = AtomicUsize::new(0);
        let publish = || {
            let n = calls.fetch_add(1, Ordering::Relaxed) + 1;
            Ok(json!({"version":n}))
        };
        assert_eq!(
            publish_revision(&f.store, "os.mail", "a1", &event.id, "source-a", None, publish)
                .unwrap()["version"],
            1
        );
        assert!(
            retry(&|| resolve_and_ack_try(&f.dir, "os.mail", "a1", &event.id))
                .unwrap()
                .unwrap()
        );
        assert!(f.pending().is_empty());
        let reopened = Store::at(&f.dir, Arc::new(MemoryVault));
        assert_eq!(
            publish_revision(&reopened, "os.mail", "a1", &event.id, "source-a", None, publish)
                .unwrap()["version"],
            1
        );
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            publish_revision(&reopened, "os.mail", "a1", &event.id, "source-b", None, publish)
                .unwrap()["version"],
            2
        );
        assert_eq!(
            publish_revision(&reopened, "os.mail", "a1", &event.id, "source-b", None, publish)
                .unwrap()["version"],
            2
        );
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert!(publish_revision(
            &reopened,
            "os.mail",
            "a1",
            &event.id,
            "invalid",
            None,
            || Err("invalid L0".into())
        )
        .is_err());
        assert_eq!(
            publish_revision(&reopened, "os.mail", "a1", &event.id, "source-b", None, publish)
                .unwrap()["version"],
            2
        );
        assert!(
            publish_revision(&reopened, "os.mail", "b2", &event.id, "source-b", None, publish)
                .is_err()
        );
        assert!(publish_revision(
            &reopened,
            "other.app",
            "a1",
            &event.id,
            "source-b",
            None,
            publish
        )
        .is_err());
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert!(event_resolved(&f.dir, "os.mail", "a1", &event.id).unwrap());
        assert!(
            f.pending().is_empty(),
            "a repair never recreates an acknowledged event"
        );
    }
    #[test]
    fn publication_receipt_keeps_draft_binding_across_restart_and_repairs() {
        let f = Fixture::new();
        let binding = json!({"publisher":"os.mail","account":"a1","draft_id":"one","draft_revision":1,
            "source_message":{"message":"m1"},"chat_thread":"thread1"});
        publish_revision(
            &f.store,
            "os.mail",
            "a1",
            "card",
            "first",
            Some(&binding),
            || Ok(json!({"ok":true})),
        )
        .unwrap();
        let reopened = Store::at(&f.dir, Arc::new(MemoryVault));
        let mut edited = binding.clone();
        edited["draft_revision"] = json!(2);
        publish_revision(
            &reopened,
            "os.mail",
            "a1",
            "card",
            "edited",
            Some(&edited),
            || Ok(json!({"ok":true})),
        )
        .unwrap();
        edited["draft_id"] = json!("other");
        assert!(publish_revision(
            &reopened,
            "os.mail",
            "a1",
            "card",
            "retarget",
            Some(&edited),
            || panic!("retarget reached publisher")
        )
        .is_err());
        assert!(publish_revision(
            &reopened,
            "os.mail",
            "a1",
            "card",
            "unbind",
            None,
            || panic!("binding removal reached publisher")
        )
        .is_err());
    }
    #[test]
    fn old_event_receipts_allow_one_validated_card_refresh() {
        let f = Fixture::new();
        f.sync().unwrap();
        f.add("new");
        f.sync().unwrap();
        let event = f.pending().pop().unwrap();
        publish_once(&f.store, "os.mail", "a1", &event.id, || {
            Ok(json!({"old":true}))
        })
        .unwrap();
        let calls = AtomicUsize::new(0);
        for _ in 0..2 {
            let result =
                publish_revision(&f.store, "os.mail", "a1", &event.id, "new", None, || {
                    calls.fetch_add(1, Ordering::Relaxed);
                    Ok(json!({"new":true}))
                })
                .unwrap();
            assert_eq!(result["new"], true);
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn concurrent_syncs_emit_once_and_published_receipt_suppresses_retries() {
        let f = Fixture::new();
        f.sync().unwrap();
        f.add("new");
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| f.sync().unwrap());
            }
        });
        let pending = f.pending();
        assert_eq!(pending.len(), 1);
        let called = AtomicUsize::new(0);
        for _ in 0..2 {
            let result = publish_once(&f.store, "os.mail", "a1", &pending[0].id, || {
                called.fetch_add(1, Ordering::Relaxed);
                Ok(json!({"card_id": pending[0].id}))
            })
            .unwrap();
            assert_eq!(result["card_id"], pending[0].id);
        }
        assert_eq!(called.load(Ordering::Relaxed), 1);
        assert!(event_resolved(&f.dir, "os.mail", "a1", &pending[0].id).unwrap());
    }
}

/// A successful model turn must also make a durable decision: publish a card
/// (or fallback notice), or explicitly skip. A prose claim does not resolve it.
pub fn event_resolved(
    host_dir: &Path,
    app: &str,
    account: &str,
    event_id: &str,
) -> Result<bool, String> {
    let _guard = lock();
    let store = Store::at(host_dir, vault::platform());
    store.granted(app, account)?;
    let box_ = mailbox(&store, account, INBOX)?;
    Ok(has_resolution(&box_, event_id))
}

fn has_resolution(box_: &Value, event_id: &str) -> bool {
    ["publication_receipts", "skip_decisions"]
        .iter()
        .any(|key| {
            box_.get(*key)
                .and_then(Value::as_array)
                .is_some_and(|rows| rows.iter().any(|r| text(r, "id") == event_id))
        })
}

pub(crate) fn skip_event(
    store: &Store,
    app: &str,
    account: &str,
    event_id: &str,
    reason: &str,
) -> Result<Value, String> {
    if app != "os.mail" {
        return Err("Only Mail decides incoming Mail events".into());
    }
    if !matches!(reason, "no_action" | "duplicate" | "outside_policy") {
        return Err("Choose no_action, duplicate or outside_policy".into());
    }
    let _guard = lock();
    store.granted(app, account)?;
    let mut box_ = mailbox(store, account, INBOX)?;
    if !box_
        .get("pending_events")
        .and_then(Value::as_array)
        .is_some_and(|rows| rows.iter().any(|r| text(r, "id") == event_id))
    {
        return Err("There is no pending event of that id in this account".into());
    }
    let mut decisions = box_
        .get("skip_decisions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(decision) = decisions.iter().find(|r| text(r, "id") == event_id) {
        return Ok(json!({"event_id": event_id, "skipped": true, "reason": decision["reason"]}));
    }
    decisions.push(json!({"id": event_id, "reason": reason}));
    if decisions.len() > 256 {
        decisions.remove(0);
    }
    box_["skip_decisions"] = json!(decisions);
    store.save_mailbox(account, INBOX, &box_)?;
    Ok(json!({"event_id": event_id, "skipped": true, "reason": reason}))
}
